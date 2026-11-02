//! Versioned bounded byte codec for pure Diagram Studio session state.

use super::mermaid::{MAX_MERMAID_SOURCE_BYTES, parse_mermaid};
use super::model::DiagramDocument;
use super::session::{
    DiagramSession, EditHistory, HistoryEntry, SessionSnapshot, semantically_equivalent,
};
use serde::de::{self, Error as _, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::VecDeque;
use std::fmt;

pub const DIAGRAM_SESSION_FORMAT_VERSION: u32 = 1;
pub const MAX_DIAGRAM_SESSION_BYTES: usize = 8 * 1024 * 1024;

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
        DIAGRAM_SESSION_FORMAT_VERSION, DiagramCodecError, DiagramSession,
        MAX_DIAGRAM_SESSION_BYTES, decode_session,
    };
    use crate::diagram::{GraphOperation, NodeId, PreviewState};

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
    fn diagram_codec_roundtrips_current_history_invalid_draft_and_revisions() {
        let mut session = session_with_history();
        let previous_supported_source = session.draft().to_owned();
        session
            .replace_source("%% retain these bytes\r\nflowchart TD\r\nA --> B\r\n")
            .unwrap();
        let expected_revision = session.revision();
        let expected_allocator = session.next_edge_id();
        let bytes = session.encode().unwrap();
        let mut decoded = decode_session(&bytes).unwrap();
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
            "%% retain these bytes\r\nflowchart TD\r\nA --> B\r\n"
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
        no_preview["current"]["draft"] = serde_json::Value::String("%% unsupported".into());
        no_preview["current"]["last_good"] = serde_json::Value::Null;
        let bytes = serde_json::to_vec(&no_preview).unwrap();
        let decoded = decode_session(&bytes).unwrap();
        assert_eq!(decoded.draft(), "%% unsupported");
        assert_eq!(decoded.status().preview, PreviewState::NoPreview);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&decoded.encode().unwrap()).unwrap(),
            no_preview
        );
    }
}
