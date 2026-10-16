//! Typed, versioned persistence for saved paste sequences.
//!
//! Clipboard history steps store a SHA-256 reference and a fixed, non-sensitive
//! label. The selected clipboard body is resolved and frozen only for an
//! invocation; it is never copied into a saved sequence. Literal text is
//! persisted only when an authored sequence is explicitly saved. The on-disk
//! format is `paste_sequences.json`, schema version 1, and all saved updates go
//! through the atomic sequence store.

use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const PASTE_SEQUENCES_FILE: &str = "paste_sequences.json";
pub const MAX_SEQUENCES: usize = 100;
pub const MAX_STEPS_PER_SEQUENCE: usize = 100;
pub const MAX_SEQUENCE_NAME_CHARS: usize = 80;
pub const MAX_SEQUENCE_DELAY_MS: u32 = 60_000;
pub const PASTE_SEQUENCE_SCHEMA_VERSION: u32 = 1;

const CLIPBOARD_REFERENCE_LABEL: &str = "Clipboard history item";

static SEQUENCE_FILE_TRANSACTION: Mutex<()> = Mutex::new(());

/// A stable identifier assigned when a draft is first added to a sequence
/// document. IDs are monotonically allocated and are never derived from row
/// positions or sequence names.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SequenceId(u64);

impl SequenceId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Debug for SequenceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("SequenceId").field(&self.0).finish()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackMode {
    #[default]
    Automatic,
    ManualAdvance,
}

/// A validated chord string in the application's existing hotkey syntax.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HotkeyChord(String);

impl HotkeyChord {
    pub fn new(chord: impl Into<String>) -> Result<Self, SequenceValidationError> {
        let chord = chord.into().trim().to_owned();
        if crate::hotkey::parse_hotkey(&chord).is_none() {
            return Err(SequenceValidationError::InvalidHotkey);
        }
        Ok(Self(chord))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(&self) -> Result<(), SequenceValidationError> {
        if crate::hotkey::parse_hotkey(&self.0).is_none() {
            return Err(SequenceValidationError::InvalidHotkey);
        }
        Ok(())
    }
}

impl fmt::Debug for HotkeyChord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HotkeyChord([redacted])")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardReferenceLabel {
    #[default]
    HistoryItem,
}

impl ClipboardReferenceLabel {
    pub const fn display_label(self) -> &'static str {
        CLIPBOARD_REFERENCE_LABEL
    }
}

/// Ordered source item in a sequence. Debug output intentionally omits aliases,
/// clipboard digests, and authored literal contents.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Step {
    SnippetRef {
        alias: String,
    },
    ClipboardSnapshotOrRef {
        content_sha256: String,
        #[serde(default)]
        label: ClipboardReferenceLabel,
    },
    LiteralText {
        text: String,
    },
}

impl Step {
    /// Create a reference to an exact clipboard-history payload without
    /// retaining that payload in the sequence model.
    pub fn clipboard_history_reference(text: &str) -> Self {
        Self::ClipboardSnapshotOrRef {
            content_sha256: hex::encode(Sha256::digest(text.as_bytes())),
            label: ClipboardReferenceLabel::HistoryItem,
        }
    }
}

impl fmt::Debug for Step {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SnippetRef { .. } => formatter.write_str("SnippetRef([alias redacted])"),
            Self::ClipboardSnapshotOrRef { .. } => {
                formatter.write_str("ClipboardSnapshotOrRef([reference redacted])")
            }
            Self::LiteralText { text } => formatter
                .debug_struct("LiteralText")
                .field("byte_len", &text.len())
                .finish(),
        }
    }
}

/// Persisted sequence. Debug output is intentionally metadata-only and never
/// includes a label, hotkey string, snippet alias, or step contents.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sequence {
    pub id: SequenceId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<HotkeyChord>,
    #[serde(default)]
    pub mode: PlaybackMode,
    #[serde(default)]
    pub delay_ms: u32,
    pub steps: Vec<Step>,
}

impl fmt::Debug for Sequence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Sequence")
            .field("id", &self.id)
            .field("step_count", &self.steps.len())
            .field("mode", &self.mode)
            .field("delay_ms", &self.delay_ms)
            .finish()
    }
}

/// An editable, memory-only sequence candidate. It has no serialization
/// implementation so Run Once drafts cannot be persisted accidentally.
#[derive(Clone, PartialEq, Eq)]
pub struct SequenceDraft {
    pub id: Option<SequenceId>,
    pub name: String,
    pub hotkey: Option<HotkeyChord>,
    pub mode: PlaybackMode,
    pub delay_ms: u32,
    pub steps: Vec<Step>,
}

impl SequenceDraft {
    pub fn new(name: impl Into<String>, steps: Vec<Step>) -> Self {
        Self {
            id: None,
            name: name.into(),
            hotkey: None,
            mode: PlaybackMode::Automatic,
            delay_ms: 0,
            steps,
        }
    }

    pub fn from_sequence(sequence: &Sequence) -> Self {
        Self {
            id: Some(sequence.id),
            name: sequence.name.clone(),
            hotkey: sequence.hotkey.clone(),
            mode: sequence.mode,
            delay_ms: sequence.delay_ms,
            steps: sequence.steps.clone(),
        }
    }
}

impl fmt::Debug for SequenceDraft {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SequenceDraft")
            .field("id", &self.id)
            .field("step_count", &self.steps.len())
            .field("mode", &self.mode)
            .field("delay_ms", &self.delay_ms)
            .finish()
    }
}

/// On-disk versioned document. The version field is checked by
/// [`SequenceStore::load`] before a document is exposed or modified.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SequenceDocument {
    version: u32,
    revision: u64,
    next_id: u64,
    sequences: Vec<Sequence>,
}

impl Default for SequenceDocument {
    fn default() -> Self {
        Self {
            version: PASTE_SEQUENCE_SCHEMA_VERSION,
            revision: 0,
            next_id: 1,
            sequences: Vec::new(),
        }
    }
}

impl SequenceDocument {
    pub const fn schema_version(&self) -> u32 {
        self.version
    }

    pub fn sequences(&self) -> &[Sequence] {
        &self.sequences
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn validate(&self) -> Result<(), SequenceValidationError> {
        if self.sequences.len() > MAX_SEQUENCES {
            return Err(SequenceValidationError::TooManySequences);
        }
        if self.next_id == 0 {
            return Err(SequenceValidationError::InvalidNextId);
        }

        let mut ids = HashSet::with_capacity(self.sequences.len());
        let mut labels = HashSet::with_capacity(self.sequences.len());
        let mut max_id = 0;
        for sequence in &self.sequences {
            if sequence.id.0 == 0 || !ids.insert(sequence.id) {
                return Err(SequenceValidationError::InvalidOrDuplicateId);
            }
            max_id = max_id.max(sequence.id.0);
            validate_sequence_fields(
                &sequence.name,
                sequence.hotkey.as_ref(),
                sequence.delay_ms,
                &sequence.steps,
            )?;
            if !labels.insert(normalize_sequence_name(&sequence.name).to_lowercase()) {
                return Err(SequenceValidationError::DuplicateName);
            }
        }
        if self.next_id <= max_id {
            return Err(SequenceValidationError::InvalidNextId);
        }
        Ok(())
    }
}

impl fmt::Debug for SequenceDocument {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SequenceDocument")
            .field("version", &self.version)
            .field("revision", &self.revision)
            .field("sequence_count", &self.sequences.len())
            .finish()
    }
}

/// A loaded document that can be edited in memory and committed atomically.
#[derive(Clone, PartialEq, Eq)]
pub struct SequenceSnapshot {
    document: SequenceDocument,
    dirty: bool,
}

impl SequenceSnapshot {
    pub fn sequences(&self) -> &[Sequence] {
        self.document.sequences()
    }

    pub fn sequence(&self, id: SequenceId) -> Option<&Sequence> {
        self.document
            .sequences
            .iter()
            .find(|sequence| sequence.id == id)
    }

    pub const fn revision(&self) -> u64 {
        self.document.revision
    }

    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Add a new draft or replace a draft's existing stable identifier.
    /// Validation happens before this snapshot changes.
    pub fn upsert(&mut self, mut draft: SequenceDraft) -> Result<SequenceId, SequenceStoreError> {
        draft.name = normalize_sequence_name(&draft.name);
        let mut candidate = self.document.clone();

        let id = if let Some(id) = draft.id {
            if let Some(sequence) = candidate
                .sequences
                .iter_mut()
                .find(|sequence| sequence.id == id)
            {
                *sequence = sequence_from_draft(id, draft);
            } else {
                return Err(SequenceStoreError::SequenceNotFound);
            }
            id
        } else {
            if candidate.sequences.len() >= MAX_SEQUENCES {
                return Err(SequenceValidationError::TooManySequences.into());
            }
            let id = SequenceId(candidate.next_id);
            candidate.next_id = candidate
                .next_id
                .checked_add(1)
                .ok_or(SequenceStoreError::IdentifierExhausted)?;
            candidate.sequences.push(sequence_from_draft(id, draft));
            id
        };

        candidate.validate()?;
        self.document = candidate;
        self.dirty = true;
        Ok(id)
    }

    pub fn remove(&mut self, id: SequenceId) -> bool {
        let Some(index) = self
            .document
            .sequences
            .iter()
            .position(|sequence| sequence.id == id)
        else {
            return false;
        };
        self.document.sequences.remove(index);
        self.dirty = true;
        true
    }
}

impl fmt::Debug for SequenceSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SequenceSnapshot")
            .field("revision", &self.document.revision)
            .field("sequence_count", &self.document.sequences.len())
            .field("dirty", &self.dirty)
            .finish()
    }
}

fn sequence_from_draft(id: SequenceId, draft: SequenceDraft) -> Sequence {
    Sequence {
        id,
        name: draft.name,
        hotkey: draft.hotkey,
        mode: draft.mode,
        delay_ms: draft.delay_ms,
        steps: draft.steps,
    }
}

fn validate_sequence_fields(
    name: &str,
    hotkey: Option<&HotkeyChord>,
    delay_ms: u32,
    steps: &[Step],
) -> Result<(), SequenceValidationError> {
    let normalized_name = normalize_sequence_name(name);
    if normalized_name.is_empty() {
        return Err(SequenceValidationError::EmptyName);
    }
    if normalized_name.chars().count() > MAX_SEQUENCE_NAME_CHARS {
        return Err(SequenceValidationError::NameTooLong);
    }
    if delay_ms > MAX_SEQUENCE_DELAY_MS {
        return Err(SequenceValidationError::DelayOutOfRange);
    }
    if steps.is_empty() {
        return Err(SequenceValidationError::EmptySequence);
    }
    if steps.len() > MAX_STEPS_PER_SEQUENCE {
        return Err(SequenceValidationError::TooManySteps);
    }
    if let Some(hotkey) = hotkey {
        hotkey.validate()?;
    }

    for step in steps {
        match step {
            Step::SnippetRef { alias } if alias.trim().is_empty() => {
                return Err(SequenceValidationError::EmptySnippetAlias);
            }
            Step::ClipboardSnapshotOrRef { content_sha256, .. }
                if content_sha256.len() != 64
                    || !content_sha256
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) =>
            {
                return Err(SequenceValidationError::InvalidClipboardReference);
            }
            Step::SnippetRef { .. }
            | Step::ClipboardSnapshotOrRef { .. }
            | Step::LiteralText { .. } => {}
        }
    }
    Ok(())
}

fn normalize_sequence_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SequenceValidationError {
    EmptyName,
    NameTooLong,
    TooManySequences,
    EmptySequence,
    TooManySteps,
    DelayOutOfRange,
    InvalidHotkey,
    InvalidOrDuplicateId,
    InvalidNextId,
    DuplicateName,
    EmptySnippetAlias,
    InvalidClipboardReference,
}

impl fmt::Display for SequenceValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptyName => "sequence name cannot be empty",
            Self::NameTooLong => "sequence name is too long",
            Self::TooManySequences => "too many saved sequences",
            Self::EmptySequence => "a sequence must contain at least one step",
            Self::TooManySteps => "sequence contains too many steps",
            Self::DelayOutOfRange => "sequence delay is out of range",
            Self::InvalidHotkey => "sequence hotkey is invalid",
            Self::InvalidOrDuplicateId => "sequence identifier is invalid or duplicated",
            Self::InvalidNextId => "sequence identifier counter is invalid",
            Self::DuplicateName => "sequence names must be unique",
            Self::EmptySnippetAlias => "snippet reference alias cannot be empty",
            Self::InvalidClipboardReference => "clipboard history reference is invalid",
        })
    }
}

impl std::error::Error for SequenceValidationError {}

#[derive(Debug)]
pub enum SequenceStoreError {
    Persistence(PersistenceError),
    UnsupportedVersion(u32),
    Validation(SequenceValidationError),
    StaleSnapshot { expected: u64, actual: u64 },
    SequenceNotFound,
    IdentifierExhausted,
    RevisionExhausted,
}

impl fmt::Display for SequenceStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Persistence(error) => fmt::Display::fmt(error, formatter),
            Self::UnsupportedVersion(version) => {
                write!(
                    formatter,
                    "unsupported paste sequence document version {version}"
                )
            }
            Self::Validation(error) => fmt::Display::fmt(error, formatter),
            Self::StaleSnapshot { expected, actual } => write!(
                formatter,
                "paste sequence snapshot is stale (expected revision {expected}, found {actual})"
            ),
            Self::SequenceNotFound => formatter.write_str("sequence to update was not found"),
            Self::IdentifierExhausted => formatter.write_str("sequence identifiers are exhausted"),
            Self::RevisionExhausted => formatter.write_str("sequence revisions are exhausted"),
        }
    }
}

impl std::error::Error for SequenceStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Persistence(error) => Some(error),
            Self::Validation(error) => Some(error),
            _ => None,
        }
    }
}

impl From<PersistenceError> for SequenceStoreError {
    fn from(value: PersistenceError) -> Self {
        Self::Persistence(value)
    }
}

impl From<SequenceValidationError> for SequenceStoreError {
    fn from(value: SequenceValidationError) -> Self {
        Self::Validation(value)
    }
}

/// Atomic file-backed storage. Loading and editing are read-only; only an
/// explicit [`SequenceStore::commit`] writes the versioned document.
pub struct SequenceStore {
    path: PathBuf,
}

impl SequenceStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<SequenceSnapshot, SequenceStoreError> {
        let document = load_document(&self.path)?;
        Ok(SequenceSnapshot {
            document,
            dirty: false,
        })
    }

    /// Commit an explicitly edited snapshot after checking that its source
    /// revision is still current. Writes use the shared atomic JSON boundary.
    pub fn commit(&self, snapshot: &mut SequenceSnapshot) -> Result<(), SequenceStoreError> {
        if !snapshot.dirty {
            return Ok(());
        }

        let _transaction = SEQUENCE_FILE_TRANSACTION
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let current = load_document(&self.path)?;
        if current.revision != snapshot.document.revision {
            return Err(SequenceStoreError::StaleSnapshot {
                expected: snapshot.document.revision,
                actual: current.revision,
            });
        }

        snapshot.document.validate()?;
        let revision = current
            .revision
            .checked_add(1)
            .ok_or(SequenceStoreError::RevisionExhausted)?;
        let mut committed = snapshot.document.clone();
        committed.revision = revision;
        save_json_atomic(&self.path, &committed)?;
        snapshot.document = committed;
        snapshot.dirty = false;
        Ok(())
    }
}

fn load_document(path: &Path) -> Result<SequenceDocument, SequenceStoreError> {
    let document = match load_json::<SequenceDocument>(path)? {
        LoadState::Missing | LoadState::Empty => SequenceDocument::default(),
        LoadState::Loaded(document) => document,
    };
    if document.version != PASTE_SEQUENCE_SCHEMA_VERSION {
        return Err(SequenceStoreError::UnsupportedVersion(document.version));
    }
    document.validate()?;
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::{
        ClipboardReferenceLabel, HotkeyChord, MAX_SEQUENCE_DELAY_MS, MAX_SEQUENCE_NAME_CHARS,
        PlaybackMode, SequenceDraft, SequenceId, SequenceStore, SequenceStoreError,
        SequenceValidationError, Step,
    };
    use sha2::{Digest, Sha256};

    fn clipboard_step(text: &str) -> Step {
        Step::clipboard_history_reference(text)
    }

    #[test]
    fn drafts_are_memory_only_and_explicit_save_round_trips_order_and_stable_ids() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(super::PASTE_SEQUENCES_FILE);
        let store = SequenceStore::new(&path);
        let mut snapshot = store.load().unwrap();
        assert!(!path.exists());

        let clipboard_body = "temporary clipboard secret 雪";
        let authored_literal = "explicitly authored sequence text";
        let mut draft = SequenceDraft::new(
            "  Example    sequence  ",
            vec![
                Step::SnippetRef {
                    alias: "release".into(),
                },
                clipboard_step(clipboard_body),
                Step::LiteralText {
                    text: authored_literal.into(),
                },
            ],
        );
        draft.hotkey = Some(HotkeyChord::new("Ctrl+Shift+P").unwrap());
        draft.mode = PlaybackMode::ManualAdvance;
        draft.delay_ms = 250;

        // Creating a Run Once/edit draft performs no filesystem operation.
        assert!(!path.exists());
        assert!(!snapshot.is_dirty());

        let id = snapshot.upsert(draft).unwrap();
        assert_eq!(id, SequenceId(1));
        assert!(snapshot.is_dirty());
        assert!(!path.exists());
        assert!(!format!("{snapshot:?}").contains(authored_literal));
        store.commit(&mut snapshot).unwrap();

        let serialized = std::fs::read_to_string(&path).unwrap();
        assert!(serialized.contains(authored_literal));
        assert!(!serialized.contains(clipboard_body));
        assert!(serialized.contains(&hex::encode(Sha256::digest(clipboard_body.as_bytes()))));
        assert!(serialized.contains("history_item"));

        let loaded = store.load().unwrap();
        assert_eq!(loaded.sequences().len(), 1);
        let saved = &loaded.sequences()[0];
        assert_eq!(saved.id, id);
        assert_eq!(saved.name, "Example sequence");
        assert_eq!(saved.mode, PlaybackMode::ManualAdvance);
        assert_eq!(saved.delay_ms, 250);
        assert_eq!(saved.hotkey.as_ref().unwrap().as_str(), "Ctrl+Shift+P");
        assert_eq!(
            saved.steps[0],
            Step::SnippetRef {
                alias: "release".into()
            }
        );
        assert_eq!(saved.steps[1], clipboard_step(clipboard_body));
        assert_eq!(
            saved.steps[2],
            Step::LiteralText {
                text: authored_literal.into()
            }
        );
        assert_eq!(loaded.revision(), 1);

        let mut next = store.load().unwrap();
        let next_id = next
            .upsert(SequenceDraft::new(
                "Second",
                vec![Step::LiteralText { text: "x".into() }],
            ))
            .unwrap();
        assert_eq!(next_id, SequenceId(2));
        store.commit(&mut next).unwrap();
        assert_eq!(store.load().unwrap().sequences()[0].id, id);
        assert_eq!(store.load().unwrap().sequences()[1].id, next_id);
    }

    #[test]
    fn missing_and_empty_stores_load_as_defaults_without_rewriting() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("paste_sequences.json");
        let store = SequenceStore::new(&path);

        let missing = store.load().unwrap();
        assert!(missing.sequences().is_empty());
        assert_eq!(missing.revision(), 0);
        assert!(!path.exists());

        std::fs::write(&path, b" \r\n").unwrap();
        let empty = store.load().unwrap();
        assert!(empty.sequences().is_empty());
        assert_eq!(empty.revision(), 0);
        assert_eq!(std::fs::read(&path).unwrap(), b" \r\n");
    }

    #[test]
    fn clipboard_reference_uses_exact_utf8_sha256_and_redacted_debug() {
        let body = "clipboard雪\nvalue";
        let step = clipboard_step(body);
        let expected = hex::encode(Sha256::digest(body.as_bytes()));
        let Step::ClipboardSnapshotOrRef {
            content_sha256,
            label,
        } = &step
        else {
            panic!("clipboard constructor should create a reference step");
        };
        assert_eq!(content_sha256, &expected);
        assert_eq!(label, &ClipboardReferenceLabel::HistoryItem);
        assert_eq!(label.display_label(), "Clipboard history item");
        assert!(!format!("{step:?}").contains(body));
        assert!(!format!("{step:?}").contains(content_sha256));
    }

    #[test]
    fn unknown_version_and_malformed_document_are_retained_without_rewrite() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("paste_sequences.json");
        let store = SequenceStore::new(&path);

        let unknown = br#"{"version":99,"revision":0,"next_id":1,"sequences":[]}"#;
        std::fs::write(&path, unknown).unwrap();
        assert!(matches!(
            store.load(),
            Err(SequenceStoreError::UnsupportedVersion(99))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), unknown);

        let malformed = b"{not valid json";
        std::fs::write(&path, malformed).unwrap();
        assert!(store.load().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), malformed);
    }

    #[test]
    fn validation_rejects_empty_duplicate_oversized_and_malformed_sequences() {
        let directory = tempfile::tempdir().unwrap();
        let store = SequenceStore::new(directory.path().join("paste_sequences.json"));
        let mut snapshot = store.load().unwrap();

        assert!(matches!(
            snapshot.upsert(SequenceDraft::new("Empty", Vec::new())),
            Err(SequenceStoreError::Validation(
                SequenceValidationError::EmptySequence
            ))
        ));
        assert!(matches!(
            snapshot.upsert(SequenceDraft::new(
                " \t ",
                vec![Step::LiteralText { text: "x".into() }],
            )),
            Err(SequenceStoreError::Validation(
                SequenceValidationError::EmptyName
            ))
        ));
        assert!(matches!(
            snapshot.upsert(SequenceDraft::new(
                "n".repeat(MAX_SEQUENCE_NAME_CHARS + 1),
                vec![Step::LiteralText { text: "x".into() }],
            )),
            Err(SequenceStoreError::Validation(
                SequenceValidationError::NameTooLong
            ))
        ));
        assert!(matches!(
            HotkeyChord::new("not a supported chord"),
            Err(SequenceValidationError::InvalidHotkey)
        ));

        snapshot
            .upsert(SequenceDraft::new(
                "Same name",
                vec![Step::LiteralText { text: "ok".into() }],
            ))
            .unwrap();
        assert!(matches!(
            snapshot.upsert(SequenceDraft::new(
                " same   NAME ",
                vec![Step::LiteralText {
                    text: "other".into()
                }],
            )),
            Err(SequenceStoreError::Validation(
                SequenceValidationError::DuplicateName
            ))
        ));

        let mut invalid_reference = SequenceDraft::new(
            "Invalid clipboard ref",
            vec![Step::ClipboardSnapshotOrRef {
                content_sha256: "not-a-digest".into(),
                label: ClipboardReferenceLabel::HistoryItem,
            }],
        );
        assert!(matches!(
            snapshot.upsert(invalid_reference.clone()),
            Err(SequenceStoreError::Validation(
                SequenceValidationError::InvalidClipboardReference
            ))
        ));
        invalid_reference.steps = vec![Step::SnippetRef { alias: "  ".into() }];
        assert!(matches!(
            snapshot.upsert(invalid_reference),
            Err(SequenceStoreError::Validation(
                SequenceValidationError::EmptySnippetAlias
            ))
        ));

        let mut too_many_steps = SequenceDraft::new(
            "Too many steps",
            vec![Step::LiteralText { text: "x".into() }; super::MAX_STEPS_PER_SEQUENCE + 1],
        );
        assert!(matches!(
            snapshot.upsert(too_many_steps.clone()),
            Err(SequenceStoreError::Validation(
                SequenceValidationError::TooManySteps
            ))
        ));
        too_many_steps.delay_ms = MAX_SEQUENCE_DELAY_MS + 1;
        too_many_steps.steps.truncate(1);
        assert!(matches!(
            snapshot.upsert(too_many_steps),
            Err(SequenceStoreError::Validation(
                SequenceValidationError::DelayOutOfRange
            ))
        ));

        let mut too_many_sequences = store.load().unwrap();
        for index in 0..super::MAX_SEQUENCES {
            too_many_sequences
                .upsert(SequenceDraft::new(
                    format!("Sequence {index}"),
                    vec![Step::LiteralText { text: "x".into() }],
                ))
                .unwrap();
        }
        assert!(matches!(
            too_many_sequences.upsert(SequenceDraft::new(
                "Over limit",
                vec![Step::LiteralText { text: "x".into() }],
            )),
            Err(SequenceStoreError::Validation(
                SequenceValidationError::TooManySequences
            ))
        ));
    }

    #[test]
    fn commit_rejects_a_stale_snapshot_without_losing_the_first_update() {
        let directory = tempfile::tempdir().unwrap();
        let store = SequenceStore::new(directory.path().join("paste_sequences.json"));
        let mut first = store.load().unwrap();
        let mut stale = store.load().unwrap();

        first
            .upsert(SequenceDraft::new(
                "First",
                vec![Step::LiteralText {
                    text: "first".into(),
                }],
            ))
            .unwrap();
        store.commit(&mut first).unwrap();

        stale
            .upsert(SequenceDraft::new(
                "Stale",
                vec![Step::LiteralText {
                    text: "stale".into(),
                }],
            ))
            .unwrap();
        assert!(matches!(
            store.commit(&mut stale),
            Err(SequenceStoreError::StaleSnapshot {
                expected: 0,
                actual: 1
            })
        ));
        let reloaded = store.load().unwrap();
        assert_eq!(reloaded.sequences().len(), 1);
        assert_eq!(reloaded.sequences()[0].name, "First");
    }
}
