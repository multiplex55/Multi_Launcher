//! Versioned bounded byte codec for pure Diagram Studio session state.

use super::mermaid::{MAX_MERMAID_SOURCE_BYTES, parse_mermaid, uses_expanded_flowchart_syntax};
use super::model::DiagramDocument;
use super::session::{
    DiagramSession, EditHistory, HistoryEntry, SessionSnapshot, semantically_equivalent,
};
use super::visual::{
    MAX_VISUAL_HISTORY_ENTRIES, VisualAllocator, VisualDiagramDocument, VisualHistoryEntry,
    VisualScene,
};
use serde::de::{self, Error as _, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::VecDeque;
use std::fmt;

pub const DIAGRAM_SESSION_FORMAT_VERSION: u32 = 1;
pub const DIAGRAM_VISUAL_LEGACY_FORMAT_VERSION: u32 = 2;
pub const DIAGRAM_VISUAL_DOCUMENT_FORMAT_VERSION: u32 = 3;
pub const MAX_DIAGRAM_SESSION_BYTES: usize = 8 * 1024 * 1024;
pub use super::visual::MAX_VISUAL_DOCUMENT_TITLE_BYTES;

#[derive(Debug)]
pub(super) enum DiagramFileDocument {
    Mermaid(DiagramSession),
    Visual(VisualDiagramDocument),
}

pub(super) enum DiagramFileDocumentRef<'a> {
    Mermaid(&'a DiagramSession),
    Visual(&'a VisualDiagramDocument),
}

#[derive(Debug)]
pub enum DiagramCodecError {
    PayloadTooLarge { max_bytes: usize },
    Malformed(serde_json::Error),
    Serialization(serde_json::Error),
    UnsupportedVersion { found: u32, supported: u32 },
    InvalidState(String),
}

impl fmt::Display for DiagramCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PayloadTooLarge { max_bytes } => {
                write!(formatter, "diagram session exceeds {max_bytes} bytes")
            }
            Self::Malformed(error) => write!(formatter, "invalid diagram session data: {error}"),
            Self::Serialization(error) => {
                write!(formatter, "failed to encode diagram session: {error}")
            }
            Self::UnsupportedVersion { found, supported } => write!(
                formatter,
                "diagram session version {found} is unsupported (current version is {supported})"
            ),
            Self::InvalidState(reason) => {
                write!(formatter, "invalid diagram session state: {reason}")
            }
        }
    }
}

impl std::error::Error for DiagramCodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Malformed(error) | Self::Serialization(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct SessionWireRef<'a> {
    format_version: u32,
    current: &'a SessionSnapshot,
    revision: u64,
    next_edge_id: u64,
    undo_len: usize,
    history: Vec<&'a HistoryEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionWire {
    format_version: u32,
    current: SessionSnapshot,
    revision: u64,
    next_edge_id: u64,
    undo_len: usize,
    #[serde(deserialize_with = "deserialize_bounded_history")]
    history: Vec<HistoryEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyVisualDocumentWire {
    format_version: u32,
    authoring_kind: VisualAuthoringKind,
    revision: u64,
    #[serde(deserialize_with = "deserialize_visual_title")]
    title: String,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct VisualDocumentWireRef<'a> {
    format_version: u32,
    authoring_kind: VisualAuthoringKind,
    revision: u64,
    current: &'a VisualScene,
    allocator: VisualAllocator,
    undo_len: usize,
    history: Vec<&'a VisualHistoryEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VisualDocumentWire {
    format_version: u32,
    authoring_kind: VisualAuthoringKind,
    revision: u64,
    current: VisualScene,
    allocator: VisualAllocator,
    undo_len: usize,
    #[serde(deserialize_with = "deserialize_bounded_visual_history")]
    history: Vec<VisualHistoryEntry>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum VisualAuthoringKind {
    Visual,
}

#[derive(Deserialize)]
struct FormatVersionHeader {
    format_version: u32,
}

pub(super) fn encode_document(
    document: DiagramFileDocumentRef<'_>,
) -> Result<Vec<u8>, DiagramCodecError> {
    match document {
        DiagramFileDocumentRef::Mermaid(session) => encode_session(session),
        DiagramFileDocumentRef::Visual(visual) => {
            visual
                .validate()
                .map_err(|error| DiagramCodecError::InvalidState(error.to_string()))?;
            let history = visual
                .history
                .undo
                .iter()
                .chain(visual.history.redo.iter())
                .collect();
            let wire = VisualDocumentWireRef {
                format_version: DIAGRAM_VISUAL_DOCUMENT_FORMAT_VERSION,
                authoring_kind: VisualAuthoringKind::Visual,
                revision: visual.revision,
                current: &visual.current,
                allocator: visual.allocator,
                undo_len: visual.history.undo.len(),
                history,
            };
            let bytes = serde_json::to_vec(&wire).map_err(DiagramCodecError::Serialization)?;
            if bytes.len() > MAX_DIAGRAM_SESSION_BYTES {
                return Err(DiagramCodecError::PayloadTooLarge {
                    max_bytes: MAX_DIAGRAM_SESSION_BYTES,
                });
            }
            Ok(bytes)
        }
    }
}

pub(super) fn decode_document(bytes: &[u8]) -> Result<DiagramFileDocument, DiagramCodecError> {
    if bytes.len() > MAX_DIAGRAM_SESSION_BYTES {
        return Err(DiagramCodecError::PayloadTooLarge {
            max_bytes: MAX_DIAGRAM_SESSION_BYTES,
        });
    }
    let header: FormatVersionHeader =
        serde_json::from_slice(bytes).map_err(DiagramCodecError::Malformed)?;
    match header.format_version {
        DIAGRAM_SESSION_FORMAT_VERSION => decode_session(bytes).map(DiagramFileDocument::Mermaid),
        DIAGRAM_VISUAL_LEGACY_FORMAT_VERSION => {
            let wire: LegacyVisualDocumentWire =
                serde_json::from_slice(bytes).map_err(DiagramCodecError::Malformed)?;
            if wire.format_version != DIAGRAM_VISUAL_LEGACY_FORMAT_VERSION {
                return Err(DiagramCodecError::InvalidState(
                    "visual document version changed during decoding".into(),
                ));
            }
            let VisualAuthoringKind::Visual = wire.authoring_kind;
            VisualDiagramDocument::from_legacy_title(wire.title, wire.revision)
                .map(DiagramFileDocument::Visual)
                .map_err(|error| DiagramCodecError::InvalidState(error.to_string()))
        }
        DIAGRAM_VISUAL_DOCUMENT_FORMAT_VERSION => {
            let wire: VisualDocumentWire =
                serde_json::from_slice(bytes).map_err(DiagramCodecError::Malformed)?;
            if wire.format_version != DIAGRAM_VISUAL_DOCUMENT_FORMAT_VERSION {
                return Err(DiagramCodecError::InvalidState(
                    "visual document version changed during decoding".into(),
                ));
            }
            let VisualAuthoringKind::Visual = wire.authoring_kind;
            VisualDiagramDocument::from_wire(
                wire.current,
                wire.revision,
                wire.allocator,
                wire.history,
                wire.undo_len,
            )
            .map(DiagramFileDocument::Visual)
            .map_err(|error| DiagramCodecError::InvalidState(error.to_string()))
        }
        found => Err(DiagramCodecError::UnsupportedVersion {
            found,
            supported: DIAGRAM_VISUAL_DOCUMENT_FORMAT_VERSION,
        }),
    }
}

fn deserialize_visual_title<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let title = String::deserialize(deserializer)?;
    if title.len() > MAX_VISUAL_DOCUMENT_TITLE_BYTES {
        return Err(de::Error::custom(format!(
            "visual title exceeds the {MAX_VISUAL_DOCUMENT_TITLE_BYTES}-byte limit"
        )));
    }
    Ok(title)
}

fn deserialize_bounded_visual_history<'de, D>(
    deserializer: D,
) -> Result<Vec<VisualHistoryEntry>, D::Error>
where
    D: Deserializer<'de>,
{
    struct HistoryVisitor;
    impl<'de> serde::de::Visitor<'de> for HistoryVisitor {
        type Value = Vec<VisualHistoryEntry>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                formatter,
                "visual history with at most {MAX_VISUAL_HISTORY_ENTRIES} entries"
            )
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut entries = Vec::with_capacity(
                sequence
                    .size_hint()
                    .unwrap_or(0)
                    .min(MAX_VISUAL_HISTORY_ENTRIES),
            );
            while let Some(entry) = sequence.next_element()? {
                if entries.len() == MAX_VISUAL_HISTORY_ENTRIES {
                    return Err(de::Error::custom(format!(
                        "visual history exceeds the {MAX_VISUAL_HISTORY_ENTRIES}-entry limit"
                    )));
                }
                entries.push(entry);
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_seq(HistoryVisitor)
}

pub fn encode_session(session: &DiagramSession) -> Result<Vec<u8>, DiagramCodecError> {
    validate_session(session).map_err(DiagramCodecError::InvalidState)?;
    let history = session
        .history
        .undo
        .iter()
        .chain(session.history.redo.iter())
        .collect();
    let wire = SessionWireRef {
        format_version: DIAGRAM_SESSION_FORMAT_VERSION,
        current: &session.current,
        revision: session.revision,
        next_edge_id: session.next_edge_id,
        undo_len: session.history.undo.len(),
        history,
    };
    let bytes = serde_json::to_vec(&wire).map_err(DiagramCodecError::Serialization)?;
    if bytes.len() > MAX_DIAGRAM_SESSION_BYTES {
        return Err(DiagramCodecError::PayloadTooLarge {
            max_bytes: MAX_DIAGRAM_SESSION_BYTES,
        });
    }
    Ok(bytes)
}

pub fn decode_session(bytes: &[u8]) -> Result<DiagramSession, DiagramCodecError> {
    if bytes.len() > MAX_DIAGRAM_SESSION_BYTES {
        return Err(DiagramCodecError::PayloadTooLarge {
            max_bytes: MAX_DIAGRAM_SESSION_BYTES,
        });
    }
    let wire: SessionWire = serde_json::from_slice(bytes).map_err(DiagramCodecError::Malformed)?;
    if wire.format_version != DIAGRAM_SESSION_FORMAT_VERSION {
        return Err(DiagramCodecError::UnsupportedVersion {
            found: wire.format_version,
            supported: DIAGRAM_SESSION_FORMAT_VERSION,
        });
    }
    if wire.undo_len > wire.history.len() {
        return Err(DiagramCodecError::InvalidState(
            "undo split exceeds the bounded history length".into(),
        ));
    }

    let mut undo = VecDeque::with_capacity(wire.undo_len);
    let mut redo = VecDeque::with_capacity(wire.history.len() - wire.undo_len);
    let mut encoded_bytes = 0usize;
    for (index, entry) in wire.history.into_iter().enumerate() {
        encoded_bytes = encoded_bytes.saturating_add(entry.encoded_bytes);
        if index < wire.undo_len {
            undo.push_back(entry);
        } else {
            redo.push_back(entry);
        }
    }
    let current = wire.current;
    let session = DiagramSession {
        current: current.clone(),
        revision: wire.revision,
        next_edge_id: wire.next_edge_id,
        saved_baseline: current,
        history: EditHistory {
            undo,
            redo,
            encoded_bytes,
        },
    };
    validate_session(&session).map_err(DiagramCodecError::InvalidState)?;
    Ok(session)
}

fn deserialize_bounded_history<'de, D>(deserializer: D) -> Result<Vec<HistoryEntry>, D::Error>
where
    D: Deserializer<'de>,
{
    struct HistoryVisitor;

    impl<'de> Visitor<'de> for HistoryVisitor {
        type Value = Vec<HistoryEntry>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                formatter,
                "at most {} encoded diagram history entries",
                super::session::MAX_DIAGRAM_HISTORY_ENTRIES
            )
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let size_hint = sequence.size_hint();
            if size_hint.is_some_and(|size| size > super::session::MAX_DIAGRAM_HISTORY_ENTRIES) {
                return Err(A::Error::custom("diagram history has too many entries"));
            }
            let capacity = size_hint
                .unwrap_or(0)
                .min(super::session::MAX_DIAGRAM_HISTORY_ENTRIES);
            let mut entries = Vec::with_capacity(capacity);
            let mut total_bytes = 0usize;
            while let Some(mut entry) = sequence.next_element::<HistoryEntry>()? {
                if entries.len() == super::session::MAX_DIAGRAM_HISTORY_ENTRIES {
                    return Err(A::Error::custom("diagram history has too many entries"));
                }
                let encoded_bytes = serde_json::to_vec(&entry).map_err(A::Error::custom)?.len();
                total_bytes = total_bytes
                    .checked_add(encoded_bytes)
                    .ok_or_else(|| A::Error::custom("diagram history size overflow"))?;
                if total_bytes > super::session::MAX_DIAGRAM_HISTORY_BYTES {
                    return Err(A::Error::custom("diagram history exceeds its byte limit"));
                }
                entry.encoded_bytes = encoded_bytes;
                entries.push(entry);
            }
            Ok(entries)
        }
    }

    deserializer.deserialize_seq(HistoryVisitor)
}

fn validate_session(session: &DiagramSession) -> Result<(), String> {
    validate_snapshot(&session.current)?;
    if session.history.undo.len() + session.history.redo.len()
        > super::session::MAX_DIAGRAM_HISTORY_ENTRIES
    {
        return Err("undo and redo exceed their combined entry limit".into());
    }
    if session.next_edge_id == 0 {
        return Err("edge allocator is zero".into());
    }

    let mut history_bytes = 0usize;
    let mut maximum_graph_allocator = 1u64;
    let mut greatest_edge_id = 0u64;
    for entry in session
        .history
        .undo
        .iter()
        .chain(session.history.redo.iter())
    {
        if entry.before == entry.after {
            return Err("history contains a no-op transition".into());
        }
        for snapshot in [&entry.before, &entry.after] {
            validate_snapshot(snapshot)?;
            if let Some(last_good) = &snapshot.last_good {
                maximum_graph_allocator = maximum_graph_allocator.max(last_good.graph.next_edge_id);
                greatest_edge_id = greatest_edge_id.max(max_edge_id(&last_good.graph));
            }
        }
        let encoded_bytes = serde_json::to_vec(entry)
            .map_err(|error| format!("cannot measure a history entry: {error}"))?
            .len();
        if encoded_bytes != entry.encoded_bytes {
            return Err("history entry byte accounting does not match its encoding".into());
        }
        history_bytes = history_bytes
            .checked_add(encoded_bytes)
            .ok_or_else(|| "history byte accounting overflow".to_owned())?;
    }
    if history_bytes != session.history.encoded_bytes
        || history_bytes > super::session::MAX_DIAGRAM_HISTORY_BYTES
    {
        return Err("combined history exceeds or disagrees with its encoded-byte budget".into());
    }

    for snapshot in std::iter::once(&session.current).chain(
        session
            .history
            .undo
            .iter()
            .flat_map(|entry| [&entry.before, &entry.after])
            .chain(
                session
                    .history
                    .redo
                    .iter()
                    .flat_map(|entry| [&entry.before, &entry.after]),
            ),
    ) {
        if let Some(last_good) = &snapshot.last_good {
            maximum_graph_allocator = maximum_graph_allocator.max(last_good.graph.next_edge_id);
            greatest_edge_id = greatest_edge_id.max(max_edge_id(&last_good.graph));
        }
    }
    if session.next_edge_id < maximum_graph_allocator || session.next_edge_id <= greatest_edge_id {
        return Err("session edge allocator is below current or historical IDs".into());
    }

    validate_history_chain(session)
}

fn validate_snapshot(snapshot: &SessionSnapshot) -> Result<(), String> {
    if snapshot.draft.len() > MAX_MERMAID_SOURCE_BYTES {
        return Err("draft exceeds the source byte limit".into());
    }
    let parsed_draft = parse_mermaid(&snapshot.draft);
    if let Some(last_good) = &snapshot.last_good {
        if last_good.source.len() > MAX_MERMAID_SOURCE_BYTES {
            return Err("last-good source exceeds the source byte limit".into());
        }
        last_good
            .graph
            .validate()
            .map_err(|error| format!("invalid last-good graph: {error}"))?;
        let parsed_last_good = parse_mermaid(&last_good.source)
            .map_err(|error| format!("last-good source does not parse: {error}"))?;
        if !semantically_equivalent(&parsed_last_good, &last_good.graph) {
            return Err("last-good graph does not match its source".into());
        }
    }

    if let Ok(parsed) = parsed_draft {
        let legacy_stale = snapshot.last_good.as_ref()
            .is_none_or(|last_good| last_good.source != snapshot.draft
                && !uses_expanded_flowchart_syntax(&last_good.source))
            && uses_expanded_flowchart_syntax(&snapshot.draft);
        if legacy_stale {
            return Ok(());
        }
        let Some(last_good) = &snapshot.last_good else {
            return Err("supported draft has no last-good graph".into());
        };
        if last_good.source != snapshot.draft || !semantically_equivalent(&parsed, &last_good.graph)
        {
            return Err("supported draft and current graph disagree".into());
        }
    }
    Ok(())
}

fn max_edge_id(graph: &DiagramDocument) -> u64 {
    graph
        .edges
        .iter()
        .map(|edge| edge.id.get())
        .max()
        .unwrap_or(0)
}

fn validate_history_chain(session: &DiagramSession) -> Result<(), String> {
    for entry in session
        .history
        .undo
        .iter()
        .zip(session.history.undo.iter().skip(1))
    {
        if entry.0.after != entry.1.before {
            return Err("undo history is not a contiguous state chain".into());
        }
    }
    if session
        .history
        .undo
        .back()
        .is_some_and(|entry| entry.after != session.current)
    {
        return Err("current state does not follow the undo chain".into());
    }

    for (earlier, later) in session
        .history
        .redo
        .iter()
        .zip(session.history.redo.iter().skip(1))
    {
        if earlier.before != later.after {
            return Err("redo history is not a contiguous reverse state chain".into());
        }
    }
    if session
        .history
        .redo
        .back()
        .is_some_and(|entry| entry.before != session.current)
    {
        return Err("current state does not precede the redo chain".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DIAGRAM_SESSION_FORMAT_VERSION, DIAGRAM_VISUAL_DOCUMENT_FORMAT_VERSION,
        DIAGRAM_VISUAL_LEGACY_FORMAT_VERSION, DiagramCodecError, DiagramFileDocument,
        DiagramFileDocumentRef, DiagramSession, MAX_DIAGRAM_SESSION_BYTES,
        MAX_VISUAL_DOCUMENT_TITLE_BYTES, decode_document, decode_session, encode_document,
    };
    use crate::diagram::{GraphOperation, NodeId, PreviewState, VisualDiagramDocument};

    #[test]
    fn diagram_codec_rejects_duplicate_membership_child_keys_in_current_and_history() {
        use crate::diagram::{
            DEFAULT_LAYER_ID, VisualObjectDraft, VisualObjectKind, VisualOperation,
            VisualPrimitiveShape, VisualStyle, VisualTransform,
        };
        let mut document = VisualDiagramDocument::new();
        let draft = |kind| VisualObjectDraft {
            layer_id: DEFAULT_LAYER_ID,
            transform: VisualTransform::default(),
            style: VisualStyle::default(),
            kind,
        };
        let frame = document
            .apply_transaction(&[VisualOperation::AddObject(draft(
                VisualObjectKind::Container {
                    label: "Frame".into(),
                },
            ))])
            .unwrap()
            .created_objects[0];
        let child = document
            .apply_transaction(&[VisualOperation::AddObject(draft(
                VisualObjectKind::Primitive {
                    shape: VisualPrimitiveShape::Rectangle,
                },
            ))])
            .unwrap()
            .created_objects[0];
        document
            .apply_transaction(&[VisualOperation::SetParent {
                child,
                parent: Some(frame),
            }])
            .unwrap();
        document.replace_label("changed").unwrap();
        let bytes = encode_document(DiagramFileDocumentRef::Visual(&document)).unwrap();
        assert!(decode_document(&bytes).is_ok());
        let encoded = String::from_utf8(bytes).unwrap();
        let original = format!(
            "\"parent_by_child\":{{\"{}\":{}}}",
            child.get(),
            frame.get()
        );
        let duplicate = format!(
            "\"parent_by_child\":{{\"{}\":{},\"{}\":{}}}",
            child.get(),
            frame.get(),
            child.get(),
            frame.get()
        );
        let offsets = encoded
            .match_indices(&original)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert!(
            offsets.len() >= 2,
            "expected membership in current scene and history"
        );
        for offset in [offsets[0], *offsets.last().unwrap()] {
            let mut malformed = encoded.clone();
            malformed.replace_range(offset..offset + original.len(), &duplicate);
            let error = match decode_document(malformed.as_bytes()) {
                Err(error) => error.to_string(),
                Ok(_) => panic!("duplicate membership was accepted"),
            };
            assert!(
                error.contains("duplicate visual membership child"),
                "{error}"
            );
        }
    }

    #[test]
    fn diagram_codec_visual_container_v3_without_membership_field_keeps_legacy_scene_wire() {
        let visual = VisualDiagramDocument::new();
        let bytes = encode_document(DiagramFileDocumentRef::Visual(&visual)).unwrap();
        let mut wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(wire["current"].get("parent_by_child").is_none());
        wire["current"]
            .as_object_mut()
            .unwrap()
            .remove("parent_by_child");

        let decoded = decode_document(&serde_json::to_vec(&wire).unwrap()).unwrap();
        let DiagramFileDocument::Visual(decoded) = decoded else {
            panic!("visual payload decoded as Mermaid");
        };
        assert!(decoded.scene().parent_by_child.is_empty());
        assert_eq!(
            encode_document(DiagramFileDocumentRef::Visual(&decoded)).unwrap(),
            bytes
        );
    }

    #[test]
    fn styled_session_history_survives_save_decode_and_undo() {
        let mut session = session_with_history();
        session.replace_source("%%{init: {\"theme\":\"forest\"}}%%\nflowchart TD\nA[\"Styled\"] --> B\nstyle A fill:#123456,stroke:#abcdef,color:#fedcba\n").unwrap();
        let bytes = session.encode().unwrap();
        let mut reopened = decode_session(&bytes).unwrap();
        assert_eq!(reopened.draft(), session.draft());
        assert_eq!(reopened.preview_graph(), session.preview_graph());
        assert_eq!(reopened.encode().unwrap(), bytes);
        assert!(reopened.undo().unwrap());
        assert!(!reopened.draft().contains("style A"));
        assert!(reopened.redo().unwrap());
        assert!(reopened.draft().contains("style A"));
    }

    #[test]
    fn frozen_unstyled_session_history_remains_compatible() {
        let bytes = include_bytes!("../../tests/fixtures/diagram_session_v1_unstyled_history.json");
        let mut decoded = decode_session(bytes).unwrap();
        let expected = session_with_history();
        assert_eq!(decoded.draft(), expected.draft());
        assert_eq!(decoded.preview_graph(), expected.preview_graph());
        assert_eq!(decoded.encode().unwrap(), bytes);
        let latest = decoded.draft().to_owned();
        assert!(decoded.undo().unwrap());
        assert!(decoded.draft().contains("Alpha"));
        assert!(decoded.redo().unwrap());
        assert_eq!(decoded.draft(), latest);
    }

    fn session_with_history() -> DiagramSession {
        let mut session = DiagramSession::new();
        session
            .replace_source("flowchart TD\nA[\"Alpha\"]\nB\nA --> B\n")
            .unwrap();
        session
            .apply_transaction(&[GraphOperation::SetNodeLabel {
                id: NodeId::new("A").unwrap(),
                label: "Changed".into(),
            }])
            .unwrap();
        session
    }

    fn encode_as_value(session: &DiagramSession) -> serde_json::Value {
        serde_json::from_slice(&session.encode().unwrap()).unwrap()
    }

    #[test]
    fn diagram_codec_roundtrips_bounded_visual_documents_with_explicit_kind() {
        let visual = VisualDiagramDocument::from_legacy_title("交付計画 — Phase 1", 12).unwrap();
        let bytes = encode_document(DiagramFileDocumentRef::Visual(&visual)).unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            wire["format_version"],
            DIAGRAM_VISUAL_DOCUMENT_FORMAT_VERSION
        );
        assert_eq!(wire["authoring_kind"], "visual");
        assert_eq!(wire["revision"], 12);
        assert_eq!(wire["current"]["objects"][0]["id"], 1);
        assert_eq!(
            wire["current"]["objects"][0]["kind"]["text"],
            "交付計画 — Phase 1"
        );
        match decode_document(&bytes).unwrap() {
            DiagramFileDocument::Visual(visual) => {
                assert_eq!(visual.label(), Some("交付計画 — Phase 1"));
                assert_eq!(visual.revision(), 12);
            }
            DiagramFileDocument::Mermaid(_) => panic!("visual payload decoded as Mermaid"),
        }

        let mut oversized = VisualDiagramDocument::new();
        if let super::super::visual::VisualObjectKind::Text { text } =
            &mut oversized.current.objects[0].kind
        {
            *text = "x".repeat(MAX_VISUAL_DOCUMENT_TITLE_BYTES + 1);
        }
        assert!(matches!(
            encode_document(DiagramFileDocumentRef::Visual(&oversized)),
            Err(DiagramCodecError::InvalidState(_))
        ));
    }

    #[test]
    fn diagram_codec_rejects_unknown_visual_kinds_malformed_payloads_and_future_versions() {
        let mut visual = serde_json::json!({
            "format_version": DIAGRAM_VISUAL_LEGACY_FORMAT_VERSION,
            "authoring_kind": "visual",
            "revision": 0,
            "title": "Canvas"
        });
        visual["authoring_kind"] = serde_json::json!("mermaid");
        assert!(matches!(
            decode_document(&serde_json::to_vec(&visual).unwrap()),
            Err(DiagramCodecError::Malformed(_))
        ));

        visual["authoring_kind"] = serde_json::json!("visual");
        visual["future_field"] = serde_json::json!(true);
        assert!(matches!(
            decode_document(&serde_json::to_vec(&visual).unwrap()),
            Err(DiagramCodecError::Malformed(_))
        ));

        visual.as_object_mut().unwrap().remove("future_field");
        visual["format_version"] = serde_json::json!(99);
        assert!(matches!(
            decode_document(&serde_json::to_vec(&visual).unwrap()),
            Err(DiagramCodecError::UnsupportedVersion { found: 99, .. })
        ));

        let mut v3: serde_json::Value = serde_json::from_slice(
            &encode_document(DiagramFileDocumentRef::Visual(&VisualDiagramDocument::new()))
                .unwrap(),
        )
        .unwrap();
        v3["current"]["objects"][0]["kind"]["text"] =
            serde_json::json!("x".repeat(MAX_VISUAL_DOCUMENT_TITLE_BYTES + 1));
        assert!(matches!(
            decode_document(&serde_json::to_vec(&v3).unwrap()),
            Err(DiagramCodecError::InvalidState(_))
        ));

        assert!(matches!(
            decode_document(&vec![b' '; MAX_DIAGRAM_SESSION_BYTES + 1]),
            Err(DiagramCodecError::PayloadTooLarge { .. })
        ));
    }

    #[test]
    fn diagram_codec_migrates_frozen_visual_v2_title_to_one_deterministic_text_object() {
        let frozen = r#"{"format_version":2,"authoring_kind":"visual","revision":12,"title":"Legacy canvas — 保存"}"#;
        match decode_document(frozen.as_bytes()).unwrap() {
            DiagramFileDocument::Visual(visual) => {
                assert_eq!(visual.revision(), 12);
                assert_eq!(visual.label(), Some("Legacy canvas — 保存"));
                assert_eq!(visual.scene().objects.len(), 1);
                assert_eq!(visual.scene().objects[0].id.get(), 1);
            }
            DiagramFileDocument::Mermaid(_) => panic!("visual payload decoded as Mermaid"),
        }
    }

    #[test]
    fn diagram_codec_visual_connector_roundtrips_and_preserves_cascade_history() {
        use super::super::visual::{
            DEFAULT_LAYER_ID, VisualAnchor, VisualConnector, VisualConnectorDirection,
            VisualConnectorEndpoint, VisualConnectorRoute, VisualObjectDraft, VisualObjectKind,
            VisualOperation, VisualPrimitiveShape, VisualStyle, VisualTransform,
        };

        let mut visual = VisualDiagramDocument::new();
        let endpoints = visual
            .apply_transaction(&[
                VisualOperation::AddObject(VisualObjectDraft {
                    layer_id: DEFAULT_LAYER_ID,
                    transform: VisualTransform {
                        x: -80.0,
                        ..VisualTransform::default()
                    },
                    style: VisualStyle::default(),
                    kind: VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Diamond,
                    },
                }),
                VisualOperation::AddObject(VisualObjectDraft {
                    layer_id: DEFAULT_LAYER_ID,
                    transform: VisualTransform {
                        x: 80.0,
                        ..VisualTransform::default()
                    },
                    style: VisualStyle::default(),
                    kind: VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Cylinder,
                    },
                }),
            ])
            .unwrap()
            .created_objects;
        let connector_id = visual
            .apply_transaction(&[VisualOperation::AddObject(VisualObjectDraft {
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform::default(),
                style: VisualStyle::default(),
                kind: VisualObjectKind::Connector {
                    connector: VisualConnector {
                        manual_route: None,
                        from: VisualConnectorEndpoint {
                            object_id: endpoints[0],
                            anchor: VisualAnchor::NorthEast,
                        },
                        to: VisualConnectorEndpoint {
                            object_id: endpoints[1],
                            anchor: VisualAnchor::SouthWest,
                        },
                        route: VisualConnectorRoute::Curved,
                        direction: VisualConnectorDirection::Both,
                        label: "reviewed\nwith care".into(),
                    },
                },
            })])
            .unwrap()
            .created_objects[0];
        visual
            .apply_transaction(&[VisualOperation::RemoveObject(endpoints[0])])
            .unwrap();
        assert!(
            visual
                .scene()
                .objects
                .iter()
                .all(|object| object.id != connector_id)
        );
        visual.undo().unwrap();

        let bytes = encode_document(DiagramFileDocumentRef::Visual(&visual)).unwrap();
        let decoded = match decode_document(&bytes).unwrap() {
            DiagramFileDocument::Visual(decoded) => decoded,
            DiagramFileDocument::Mermaid(_) => panic!("visual payload decoded as Mermaid"),
        };
        assert_eq!(decoded.scene(), visual.scene());
        assert!(matches!(
            &decoded
                .scene()
                .objects
                .iter()
                .find(|object| object.id == connector_id)
                .unwrap()
                .kind,
            VisualObjectKind::Connector { connector }
                if connector.route == VisualConnectorRoute::Curved
                    && connector.direction == VisualConnectorDirection::Both
                    && connector.label == "reviewed\nwith care"
        ));
        assert!(decoded.can_redo());
        decoded.validate().unwrap();
    }

    #[test]
    fn diagram_codec_rejects_noop_and_reused_id_visual_history() {
        let mut base = VisualDiagramDocument::new();
        let base_value: serde_json::Value = serde_json::from_slice(
            &encode_document(DiagramFileDocumentRef::Visual(&base)).unwrap(),
        )
        .unwrap();
        base.apply_transaction(&[super::super::visual::VisualOperation::AddObject(
            super::super::visual::VisualObjectDraft {
                layer_id: super::super::visual::DEFAULT_LAYER_ID,
                transform: super::super::visual::VisualTransform::default(),
                style: super::super::visual::VisualStyle::default(),
                kind: super::super::visual::VisualObjectKind::Primitive {
                    shape: super::super::visual::VisualPrimitiveShape::Rectangle,
                },
            },
        )])
        .unwrap();
        let with_id_value: serde_json::Value = serde_json::from_slice(
            &encode_document(DiagramFileDocumentRef::Visual(&base)).unwrap(),
        )
        .unwrap();

        let mut no_op = with_id_value.clone();
        no_op["current"] = base_value["current"].clone();
        no_op["history"][0]["after"] = no_op["history"][0]["before"].clone();
        assert!(matches!(
            decode_document(&serde_json::to_vec(&no_op).unwrap()),
            Err(DiagramCodecError::InvalidState(_))
        ));

        let mut reused = with_id_value.clone();
        reused["revision"] = serde_json::json!(3);
        reused["undo_len"] = serde_json::json!(3);
        reused["history"] = serde_json::json!([
            {
                "before": base_value["current"],
                "after": with_id_value["current"]
            },
            {
                "before": with_id_value["current"],
                "after": base_value["current"]
            },
            {
                "before": base_value["current"],
                "after": with_id_value["current"]
            }
        ]);
        assert!(matches!(
            decode_document(&serde_json::to_vec(&reused).unwrap()),
            Err(DiagramCodecError::InvalidState(_))
        ));
    }

    #[test]
    fn diagram_codec_roundtrips_current_history_invalid_draft_and_revisions() {
        let mut session = session_with_history();
        let previous_supported_source = session.draft().to_owned();
        session
            .replace_source("%%{unsupported}%%\r\nflowchart TD\r\nA --> B\r\n")
            .unwrap();
        let expected_revision = session.revision();
        let expected_allocator = session.next_edge_id();
        let bytes = session.encode().unwrap();
        let mut decoded = decode_session(&bytes).unwrap();
        assert!(matches!(
            decode_document(&bytes).unwrap(),
            DiagramFileDocument::Mermaid(_)
        ));
        assert_eq!(decoded.encode().unwrap(), bytes);
        assert_eq!(decoded.draft(), session.draft());
        assert_eq!(decoded.last_good_source(), session.last_good_source());
        assert_eq!(decoded.revision(), expected_revision);
        assert_eq!(decoded.next_edge_id(), expected_allocator);
        assert_eq!(decoded.status().preview, PreviewState::Stale);
        assert!(!decoded.is_dirty());
        assert!(decoded.undo().unwrap());
        assert_eq!(decoded.draft(), previous_supported_source);
        assert!(decoded.redo().unwrap());
        assert_eq!(
            decoded.draft(),
            "%%{unsupported}%%\r\nflowchart TD\r\nA --> B\r\n"
        );
        assert!(decoded.revision() > expected_revision);
    }

    #[test]
    fn diagram_codec_rejects_historical_source_graph_mismatch_and_broken_chain() {
        let mut source_mismatch = encode_as_value(&session_with_history());
        source_mismatch["history"][0]["after"]["last_good"]["source"] =
            serde_json::Value::String("flowchart TD\nOther\n".into());
        let bytes = serde_json::to_vec(&source_mismatch).unwrap();
        assert!(matches!(
            decode_session(&bytes),
            Err(DiagramCodecError::InvalidState(_))
        ));

        let mut current_mismatch = encode_as_value(&session_with_history());
        current_mismatch["current"]["draft"] =
            serde_json::Value::String("flowchart TD\nOther\n".into());
        let bytes = serde_json::to_vec(&current_mismatch).unwrap();
        assert!(matches!(
            decode_session(&bytes),
            Err(DiagramCodecError::InvalidState(_))
        ));

        let mut session = session_with_history();
        session.replace_source("flowchart TD\nFinal\n").unwrap();
        let mut broken_chain = encode_as_value(&session);
        let nonadjacent_before = broken_chain["history"][0]["before"].clone();
        broken_chain["history"][1]["before"] = nonadjacent_before;
        let bytes = serde_json::to_vec(&broken_chain).unwrap();
        assert!(matches!(
            decode_session(&bytes),
            Err(DiagramCodecError::InvalidState(_))
        ));
    }

    #[test]
    fn diagram_codec_rejects_lowered_allocator_and_unknown_version() {
        let mut session = DiagramSession::new();
        session
            .replace_source("flowchart TD\nA\nB\nA --> B\n")
            .unwrap();
        let mut lowered_allocator = encode_as_value(&session);
        lowered_allocator["next_edge_id"] = serde_json::json!(1);
        assert!(matches!(
            decode_session(&serde_json::to_vec(&lowered_allocator).unwrap()),
            Err(DiagramCodecError::InvalidState(_))
        ));

        let mut unknown_version = encode_as_value(&session);
        unknown_version["format_version"] = serde_json::json!(DIAGRAM_SESSION_FORMAT_VERSION + 1);
        assert!(matches!(
            decode_session(&serde_json::to_vec(&unknown_version).unwrap()),
            Err(DiagramCodecError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn diagram_codec_rejects_oversized_collections_before_building_them() {
        let session = DiagramSession::new();
        let mut too_many_history = encode_as_value(&session);
        let history_template = encode_as_value(&session_with_history())["history"][0].clone();
        too_many_history["history"] = serde_json::Value::Array(
            (0..=crate::diagram::MAX_DIAGRAM_HISTORY_ENTRIES)
                .map(|_| history_template.clone())
                .collect(),
        );
        let history_error = decode_session(&serde_json::to_vec(&too_many_history).unwrap())
            .expect_err("the history collection exceeds its bound");
        assert!(matches!(
            history_error,
            DiagramCodecError::Malformed(error)
                if error.to_string().contains("diagram history has too many entries")
        ));

        let mut too_many_nodes = encode_as_value(&session);
        let node_template =
            encode_as_value(&session_with_history())["current"]["last_good"]["graph"]["nodes"][0]
                .clone();
        too_many_nodes["current"]["last_good"]["graph"]["nodes"] = serde_json::Value::Array(
            (0..=crate::diagram::MAX_DIAGRAM_NODES)
                .map(|index| {
                    let mut node = node_template.clone();
                    node["id"] = serde_json::Value::String(format!("Node{index}"));
                    node
                })
                .collect(),
        );
        let node_error = decode_session(&serde_json::to_vec(&too_many_nodes).unwrap())
            .expect_err("the node collection exceeds its bound");
        assert!(matches!(
            node_error,
            DiagramCodecError::Malformed(error)
                if error.to_string().contains("sequence exceeds the 1024-item limit")
        ));
    }

    #[test]
    fn diagram_codec_rejects_payload_above_documented_limit() {
        let oversized = vec![b' '; MAX_DIAGRAM_SESSION_BYTES + 1];
        assert!(matches!(
            decode_session(&oversized),
            Err(DiagramCodecError::PayloadTooLarge { .. })
        ));
    }

    #[test]
    fn diagram_codec_preserves_invalid_draft_without_a_last_good_preview() {
        let mut no_preview = encode_as_value(&DiagramSession::new());
        no_preview["current"]["draft"] = serde_json::Value::String("%%{unsupported}%%".into());
        no_preview["current"]["last_good"] = serde_json::Value::Null;
        let bytes = serde_json::to_vec(&no_preview).unwrap();
        let decoded = decode_session(&bytes).unwrap();
        assert_eq!(decoded.draft(), "%%{unsupported}%%");
        assert_eq!(decoded.status().preview, PreviewState::NoPreview);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&decoded.encode().unwrap()).unwrap(),
            no_preview
        );
    }

    #[test]
    fn legacy_newly_supported_draft_remains_readable_without_overwriting_history() {
        let mut wire = encode_as_value(&DiagramSession::new());
        wire["current"]["draft"] = serde_json::Value::String("%% old draft\ngraph TD; A --> B --> C".into());
        wire["current"]["last_good"] = serde_json::Value::Null;
        let bytes = serde_json::to_vec(&wire).unwrap();
        let mut decoded = decode_session(&bytes).unwrap();
        assert_eq!(decoded.status().preview, PreviewState::NoPreview);
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&decoded.encode().unwrap()).unwrap(), wire);
        assert!(decoded.replace_source("%% old draft\ngraph TD; A --> B --> C").unwrap());
        assert_eq!(decoded.status().preview, PreviewState::Current);
        assert!(decoded.undo().unwrap());
        assert_eq!(decoded.status().preview, PreviewState::NoPreview);
        decoded.replace_source("%% old draft\ngraph TD; A --> B --> C\n").unwrap();
        assert_eq!(decoded.status().preview, PreviewState::Current);
        assert!(decoded.undo().unwrap());
        assert_eq!(decoded.status().preview, PreviewState::NoPreview);

        let mut forged_session = DiagramSession::new();
        forged_session.replace_source("graph TD; A;").unwrap();
        let mut forged = encode_as_value(&forged_session);
        forged["current"]["draft"] = serde_json::Value::String("graph TD; B;".into());
        forged["history"] = serde_json::json!([]);
        forged["undo_len"] = serde_json::json!(0);
        let result = decode_session(&serde_json::to_vec(&forged).unwrap());
        assert!(matches!(
            result,
            Err(DiagramCodecError::InvalidState(message))
                if message == "supported draft and current graph disagree"
        ));
    }
}
