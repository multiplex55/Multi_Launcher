//! Typed, versioned persistence for saved paste sequences.
//!
//! Clipboard history steps store a SHA-256 reference and a fixed, non-sensitive
//! label. The selected clipboard body is resolved and frozen only for an
//! invocation; it is never copied into a saved sequence. Literal text is
//! persisted only when an authored sequence is explicitly saved. The on-disk
//! format is `paste_sequences.json`, schema version 1, and all saved updates go
//! through the atomic sequence store.

use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use crate::plugins::snippet_template::{
    SnippetInvocationContext, TemplateRenderError, render_for_copy_with_context,
};
use crate::plugins::snippets::{
    PreparedSnippetTemplate, SnippetEntry, SnippetRunMode, prepare_snippet_run,
};
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub mod admission;
mod playback;

#[cfg(test)]
pub(crate) use playback::TargetObservation;
pub(crate) use playback::{
    NativeSequencePlaybackPlatform, SequencePlaybackController, SequencePlaybackError,
    SequencePlaybackPlatform, SequencePlaybackStatus, SessionInputLease, StartedSequence,
};

/// Read-only acknowledgement from the main hotkey owner. This metadata never
/// participates in route selection or persistence and contains no step contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SequenceShortcutPresentation {
    Disabled,
    Active(String),
    Inactive { configured: String, reason: String },
}

impl fmt::Display for SequenceShortcutPresentation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => f.write_str("Disabled"),
            Self::Active(chord) => write!(f, "{chord} (active)"),
            Self::Inactive { configured, reason } => write!(f, "{configured} (inactive: {reason})"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SequenceRoutePresentation {
    /// The existing main-owned runtime generation, not a separate UI counter.
    pub generation: u64,
    pub physical_paste: admission::PhysicalPastePolicy,
    pub next: SequenceShortcutPresentation,
    pub cancel: SequenceShortcutPresentation,
    pub starts: Vec<(SequenceId, String)>,
    pub escape_cancel_enabled: bool,
    pub keep_dialog_visible_during_playback: bool,
}

impl SequenceRoutePresentation {
    pub fn summary(&self) -> String {
        let toggle = |enabled| if enabled { "On" } else { "Off" };
        let mut summary = format!(
            "Next: {}\nCancel: {}\nEscape cancellation: {}",
            self.next,
            self.cancel,
            toggle(self.escape_cancel_enabled)
        );
        summary.push_str(&format!(
            "\nPaste-to-Advance: {}",
            self.physical_paste
                .availability
                .reason()
                .unwrap_or_else(|| "Ctrl+V available".into())
        ));
        summary.push_str(&format!(
            "\nSaved Start shortcuts: {} active (edit in Paste Sequences)",
            self.starts.len()
        ));
        summary
    }
}

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

    /// Parse a stable ID carried by a payload-free launcher action.
    pub const fn from_raw(raw: u64) -> Option<Self> {
        if raw == 0 { None } else { Some(Self(raw)) }
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
    PasteToAdvance,
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

/// Payload-free routing data for the saved sequences' start hotkeys. This can
/// cross the hotkey preparation boundary without exposing step contents.
#[derive(Clone, PartialEq, Eq)]
pub struct SequenceHotkeyBinding {
    id: SequenceId,
    chord: HotkeyChord,
}

impl SequenceHotkeyBinding {
    pub const fn id(&self) -> SequenceId {
        self.id
    }

    pub fn chord(&self) -> &str {
        self.chord.as_str()
    }
}

impl fmt::Debug for SequenceHotkeyBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SequenceHotkeyBinding")
            .field("id", &self.id)
            .field("chord", &"[redacted]")
            .finish()
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

#[derive(Clone)]
struct SequenceCandidate {
    id: Option<SequenceId>,
    name: String,
    hotkey: Option<HotkeyChord>,
    mode: PlaybackMode,
    delay_ms: u32,
    steps: Vec<Step>,
}

impl From<Sequence> for SequenceCandidate {
    fn from(sequence: Sequence) -> Self {
        Self {
            id: Some(sequence.id),
            name: sequence.name,
            hotkey: sequence.hotkey,
            mode: sequence.mode,
            delay_ms: sequence.delay_ms,
            steps: sequence.steps,
        }
    }
}

impl From<SequenceDraft> for SequenceCandidate {
    fn from(draft: SequenceDraft) -> Self {
        Self {
            id: draft.id,
            name: draft.name,
            hotkey: draft.hotkey,
            mode: draft.mode,
            delay_ms: draft.delay_ms,
            steps: draft.steps,
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

    /// Returns only saved sequence identifiers and optional start hotkeys;
    /// step payloads never leave the store boundary during route preparation.
    pub fn hotkey_bindings(&self) -> Vec<SequenceHotkeyBinding> {
        self.document
            .sequences
            .iter()
            .filter_map(|sequence| {
                sequence.hotkey.clone().map(|chord| SequenceHotkeyBinding {
                    id: sequence.id,
                    chord,
                })
            })
            .collect()
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

    /// Restore a previously loaded snapshot only while the committed
    /// candidate is still the current document. A concurrent edit wins and is
    /// never overwritten by rollback. The identifier counter remains
    /// monotonic even when a newly allocated candidate is removed.
    pub fn rollback_if_revision(
        &self,
        expected_revision: u64,
        previous: &SequenceSnapshot,
    ) -> Result<(), SequenceStoreError> {
        let _transaction = SEQUENCE_FILE_TRANSACTION
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let current = load_document(&self.path)?;
        if current.revision != expected_revision {
            return Err(SequenceStoreError::StaleSnapshot {
                expected: expected_revision,
                actual: current.revision,
            });
        }

        let revision = current
            .revision
            .checked_add(1)
            .ok_or(SequenceStoreError::RevisionExhausted)?;
        let mut restored = previous.document.clone();
        restored.revision = revision;
        restored.next_id = restored.next_id.max(current.next_id);
        restored.validate()?;
        save_json_atomic(&self.path, &restored)?;
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

/// The external window a prepared sequence is authorized to target. Its title
/// is available to the explicit preflight UI, but omitted from Debug output.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SequenceTarget {
    window: crate::active_window::ExternalForegroundWindow,
}

impl SequenceTarget {
    pub(crate) fn from_external_window(
        window: crate::active_window::ExternalForegroundWindow,
        launcher_process_id: u32,
    ) -> Result<Self, SequencePreflightError> {
        if window.hwnd == 0 || window.process_id == 0 || window.thread_id == 0 {
            return Err(SequencePreflightError::InvalidTargetIdentity);
        }
        if window.process_id == launcher_process_id {
            return Err(SequencePreflightError::LauncherTarget);
        }
        Ok(Self { window })
    }

    pub(crate) fn hwnd(&self) -> isize {
        self.window.hwnd
    }

    pub(crate) fn process_id(&self) -> u32 {
        self.window.process_id
    }

    pub(crate) fn thread_id(&self) -> u32 {
        self.window.thread_id
    }

    pub(crate) fn title(&self) -> Option<&str> {
        self.window.title.as_deref()
    }
}

impl fmt::Debug for SequenceTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SequenceTarget")
            .field("hwnd_available", &(self.window.hwnd != 0))
            .field("process_id_available", &(self.window.process_id != 0))
            .field("thread_id_available", &(self.window.thread_id != 0))
            .field("title_available", &self.window.title.is_some())
            .finish()
    }
}

/// Immutable invocation inputs, captured before preflight takes focus. The
/// optional origin supplies template variables, never a playback destination.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SequenceInvocationContext {
    snippet_context: SnippetInvocationContext,
    origin: Option<SequenceTarget>,
}

impl SequenceInvocationContext {
    pub(crate) fn new(
        timestamp: DateTime<FixedOffset>,
        clipboard_text: Option<String>,
        origin: impl Into<Option<SequenceTarget>>,
    ) -> Self {
        let origin = origin.into();
        let snippet_context = SnippetInvocationContext::new(
            timestamp,
            clipboard_text,
            origin.as_ref().map(SequenceTarget::hwnd),
            origin
                .as_ref()
                .and_then(SequenceTarget::title)
                .map(str::to_owned),
        );
        Self {
            snippet_context,
            origin,
        }
    }

    pub(crate) fn snippet_context(&self) -> &SnippetInvocationContext {
        &self.snippet_context
    }

    pub(crate) fn origin(&self) -> Option<&SequenceTarget> {
        self.origin.as_ref()
    }
}

impl fmt::Debug for SequenceInvocationContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SequenceInvocationContext")
            .field("snippet_context", &self.snippet_context)
            .field("origin", &self.origin)
            .finish()
    }
}

/// Safe, payload-free preflight failures. User-authored aliases, templates,
/// clipboard data and rendered text are never included in these errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SequencePreflightError {
    SequenceAlreadyActive,
    InvalidSequence,
    InvalidTargetIdentity,
    LauncherTarget,
    MissingSnippet,
    AmbiguousSnippet,
    StaleSnippet,
    InvalidSnippet,
    RequiredValues,
    MissingHistoryItem,
    HistoryUnavailable,
    SnippetsUnavailable,
    NotReady,
}

impl SequencePreflightError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::SequenceAlreadyActive => "Another paste sequence is already active.",
            Self::InvalidSequence => "The sequence is empty or invalid.",
            Self::InvalidTargetIdentity => "The originating window could not be identified.",
            Self::LauncherTarget => "The launcher cannot be used as a sequence target.",
            Self::MissingSnippet => "A referenced snippet no longer exists.",
            Self::AmbiguousSnippet => "A referenced snippet alias is ambiguous.",
            Self::StaleSnippet => "A referenced snippet changed during preflight.",
            Self::InvalidSnippet => "A referenced snippet could not be prepared.",
            Self::RequiredValues => "Fill in the required snippet fields before continuing.",
            Self::MissingHistoryItem => "A referenced clipboard history item no longer exists.",
            Self::HistoryUnavailable => "Clipboard history could not be read.",
            Self::SnippetsUnavailable => "Saved snippets could not be read.",
            Self::NotReady => "Finish every prompted snippet before preparing the sequence.",
        }
    }
}

/// A single prompted step exposed to the existing snippet prompt form. Values
/// start from the snippet's configured defaults; the source entry and context
/// remain fixed for the whole sequence invocation.
pub(crate) struct SequencePrompt {
    pub(crate) step_index: usize,
    pub(crate) total_steps: usize,
    pub(crate) prompt_index: usize,
    pub(crate) total_prompts: usize,
    pub(crate) alias: String,
    pub(crate) prepared: PreparedSnippetTemplate,
    pub(crate) context: SnippetInvocationContext,
    pub(crate) values: HashMap<String, String>,
}

#[derive(Clone)]
struct PendingSequencePrompt {
    step_index: usize,
    alias: String,
    entry_snapshot: SnippetEntry,
    prepared: PreparedSnippetTemplate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreparedStepKind {
    Snippet,
    ClipboardHistory,
    LiteralText,
}

#[derive(Clone)]
struct ResolvedSequenceStep {
    text: String,
    kind: PreparedStepKind,
    hide_contents: bool,
}

/// In-memory, no-side-effect sequence preflight. It stores only clipboard
/// history digests until all prompts and snapshot checks have succeeded.
#[derive(Clone)]
pub(crate) struct SequencePreflight {
    sequence: SequenceCandidate,
    context: SequenceInvocationContext,
    snippets: Vec<(String, SnippetEntry)>,
    prompts: Vec<PendingSequencePrompt>,
    prompt_cursor: usize,
    steps: Vec<Option<ResolvedSequenceStep>>,
    history_references: Vec<(usize, String)>,
}

impl SequencePreflight {
    pub(crate) fn begin(
        sequence: Sequence,
        snippets: &[SnippetEntry],
        history: &[String],
        context: SequenceInvocationContext,
    ) -> Result<Self, SequencePreflightError> {
        Self::begin_inner(sequence.into(), snippets, history, context, false)
    }

    /// Prepare a memory-only draft for Run Once. ID zero is never valid in a
    /// saved document and cannot escape through the persistence API.
    pub(crate) fn begin_draft(
        draft: SequenceDraft,
        snippets: &[SnippetEntry],
        history: &[String],
        context: SequenceInvocationContext,
    ) -> Result<Self, SequencePreflightError> {
        Self::begin_inner(draft.into(), snippets, history, context, true)
    }

    fn begin_inner(
        sequence: SequenceCandidate,
        snippets: &[SnippetEntry],
        history: &[String],
        context: SequenceInvocationContext,
        allow_memory_only_id: bool,
    ) -> Result<Self, SequencePreflightError> {
        if (!allow_memory_only_id && sequence.id.is_none())
            || sequence.id.is_some_and(|id| id.0 == 0)
            || validate_sequence_fields(
                &sequence.name,
                sequence.hotkey.as_ref(),
                sequence.delay_ms,
                &sequence.steps,
            )
            .is_err()
        {
            return Err(SequencePreflightError::InvalidSequence);
        }

        let mut pending_snippets = Vec::new();
        let mut prompts = Vec::new();
        let mut steps = (0..sequence.steps.len())
            .map(|_| None)
            .collect::<Vec<Option<ResolvedSequenceStep>>>();
        let mut history_references = Vec::new();

        for (step_index, step) in sequence.steps.iter().enumerate() {
            match step {
                Step::SnippetRef { alias } => {
                    let entry = exact_snippet(snippets, alias)?.clone();
                    pending_snippets.push((alias.clone(), entry.clone()));
                    match prepare_snippet_run(&entry)
                        .map_err(|_| SequencePreflightError::InvalidSnippet)?
                    {
                        SnippetRunMode::Plain => {
                            steps[step_index] = Some(ResolvedSequenceStep {
                                text: entry.text,
                                kind: PreparedStepKind::Snippet,
                                hide_contents: entry.hide_contents,
                            });
                        }
                        SnippetRunMode::SystemOnly(prepared) => {
                            let text = render_for_copy_with_context(
                                &prepared.parsed,
                                &prepared.fields,
                                &HashMap::new(),
                                context.snippet_context(),
                            )
                            .map_err(map_template_error)?;
                            steps[step_index] = Some(ResolvedSequenceStep {
                                text,
                                kind: PreparedStepKind::Snippet,
                                hide_contents: entry.hide_contents,
                            });
                        }
                        SnippetRunMode::Prompted(prepared) => {
                            prompts.push(PendingSequencePrompt {
                                step_index,
                                alias: alias.clone(),
                                entry_snapshot: entry,
                                prepared,
                            });
                        }
                    }
                }
                Step::ClipboardSnapshotOrRef { content_sha256, .. } => {
                    if resolve_history_reference(history, content_sha256).is_none() {
                        return Err(SequencePreflightError::MissingHistoryItem);
                    }
                    history_references.push((step_index, content_sha256.clone()));
                }
                Step::LiteralText { text } => {
                    steps[step_index] = Some(ResolvedSequenceStep {
                        text: text.clone(),
                        kind: PreparedStepKind::LiteralText,
                        hide_contents: false,
                    });
                }
            }
        }

        Ok(Self {
            sequence,
            context,
            snippets: pending_snippets,
            prompts,
            prompt_cursor: 0,
            steps,
            history_references,
        })
    }

    pub(crate) fn current_prompt(&self) -> Option<SequencePrompt> {
        let prompt = self.prompts.get(self.prompt_cursor)?;
        let values = prompt
            .prepared
            .fields
            .iter()
            .map(|field| (field.name.clone(), field.default_value.clone()))
            .collect();
        Some(SequencePrompt {
            step_index: prompt.step_index,
            total_steps: self.sequence.steps.len(),
            prompt_index: self.prompt_cursor,
            total_prompts: self.prompts.len(),
            alias: prompt.alias.clone(),
            prepared: prompt.prepared.clone(),
            context: self.context.snippet_context().clone(),
            values,
        })
    }

    pub(crate) fn requires_history(&self) -> bool {
        !self.history_references.is_empty()
    }

    pub(crate) fn submit_current_prompt(
        &mut self,
        values: &HashMap<String, String>,
    ) -> Result<bool, SequencePreflightError> {
        let prompt = self
            .prompts
            .get(self.prompt_cursor)
            .ok_or(SequencePreflightError::NotReady)?;
        let text = render_for_copy_with_context(
            &prompt.prepared.parsed,
            &prompt.prepared.fields,
            values,
            self.context.snippet_context(),
        )
        .map_err(map_template_error)?;
        self.steps[prompt.step_index] = Some(ResolvedSequenceStep {
            text,
            kind: PreparedStepKind::Snippet,
            hide_contents: prompt.entry_snapshot.hide_contents,
        });
        self.prompt_cursor += 1;
        Ok(self.prompt_cursor < self.prompts.len())
    }

    pub(crate) fn finish(
        &self,
        current_snippets: &[SnippetEntry],
        current_history: &[String],
    ) -> Result<PreparedSequenceRun, SequencePreflightError> {
        if self.prompt_cursor != self.prompts.len() {
            return Err(SequencePreflightError::NotReady);
        }
        for (alias, snapshot) in &self.snippets {
            let current = exact_snippet(current_snippets, alias)?;
            if current != snapshot {
                return Err(SequencePreflightError::StaleSnippet);
            }
        }

        let mut steps = self
            .steps
            .iter()
            .map(|step| {
                step.as_ref().map(|step| ResolvedSequenceStep {
                    text: step.text.clone(),
                    kind: step.kind,
                    hide_contents: step.hide_contents,
                })
            })
            .collect::<Vec<_>>();
        for (step_index, digest) in &self.history_references {
            let text = resolve_history_reference(current_history, digest)
                .ok_or(SequencePreflightError::MissingHistoryItem)?;
            steps[*step_index] = Some(ResolvedSequenceStep {
                text,
                kind: PreparedStepKind::ClipboardHistory,
                hide_contents: false,
            });
        }

        let steps = steps
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or(SequencePreflightError::NotReady)?;
        Ok(PreparedSequenceRun {
            sequence_id: self.sequence.id,
            name: self.sequence.name.clone(),
            mode: self.sequence.mode,
            delay_ms: self.sequence.delay_ms,
            origin_title: self
                .context
                .origin()
                .and_then(SequenceTarget::title)
                .map(str::to_owned),
            steps,
        })
    }

    /// Finalize the currently displayed prompt without mutating this flow.
    /// Keeping the flow intact lets the user correct inputs if fresh source
    /// snapshots or stale-reference checks fail during finalization.
    pub(crate) fn finish_with_current_prompt(
        &self,
        values: &HashMap<String, String>,
        current_snippets: &[SnippetEntry],
        current_history: &[String],
    ) -> Result<PreparedSequenceRun, SequencePreflightError> {
        let mut candidate = self.clone();
        candidate.submit_current_prompt(values)?;
        candidate.finish(current_snippets, current_history)
    }
}

fn exact_snippet<'a>(
    snippets: &'a [SnippetEntry],
    alias: &str,
) -> Result<&'a SnippetEntry, SequencePreflightError> {
    let mut matches = snippets.iter().filter(|entry| entry.alias == alias);
    let Some(entry) = matches.next() else {
        return Err(SequencePreflightError::MissingSnippet);
    };
    if matches.next().is_some() {
        return Err(SequencePreflightError::AmbiguousSnippet);
    }
    Ok(entry)
}

fn resolve_history_reference(history: &[String], digest: &str) -> Option<String> {
    let mut matched: Option<&str> = None;
    for text in history {
        if hex::encode(Sha256::digest(text.as_bytes())) != digest {
            continue;
        }
        if matched.is_some_and(|previous| previous != text) {
            return None;
        }
        matched = Some(text);
    }
    matched.map(str::to_owned)
}

fn map_template_error(error: TemplateRenderError) -> SequencePreflightError {
    if error.kind == crate::plugins::snippet_template::RenderErrorKind::RequiredValues {
        SequencePreflightError::RequiredValues
    } else {
        SequencePreflightError::InvalidSnippet
    }
}

/// Final immutable, ordered sequence text. It has no Debug implementation and
/// exposes payloads only to the playback owner after every preflight check.
pub(crate) struct PreparedSequenceRun {
    sequence_id: Option<SequenceId>,
    name: String,
    mode: PlaybackMode,
    delay_ms: u32,
    origin_title: Option<String>,
    steps: Vec<ResolvedSequenceStep>,
}

impl PreparedSequenceRun {
    pub(crate) fn sequence_id(&self) -> Option<SequenceId> {
        self.sequence_id
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn mode(&self) -> PlaybackMode {
        self.mode
    }

    pub(crate) fn delay_ms(&self) -> u32 {
        self.delay_ms
    }

    pub(crate) fn step_count(&self) -> usize {
        self.steps.len()
    }

    pub(crate) fn step_text(&self, index: usize) -> Option<&str> {
        self.steps.get(index).map(|step| step.text.as_str())
    }

    pub(crate) fn preview(&self) -> PreparedSequencePreview {
        let target_label = "Current eligible foreground application at each step".to_owned();
        PreparedSequencePreview {
            name: self.name.clone(),
            mode: self.mode,
            delay_ms: self.delay_ms,
            item_count: self.steps.len(),
            target_label,
            target_title: self.origin_title.clone(),
            ready: true,
            steps: self
                .steps
                .iter()
                .enumerate()
                .map(|(index, step)| PreparedSequencePreviewStep {
                    index,
                    kind: step.kind,
                    hidden: step.hide_contents,
                    text: (!step.hide_contents).then(|| step.text.clone()),
                })
                .collect(),
        }
    }
}

/// Presentation-ready metadata and per-step safe preview. Hidden snippets have
/// no text value; callers cannot accidentally include their payload in passive UI.
pub(crate) struct PreparedSequencePreview {
    pub(crate) name: String,
    pub(crate) mode: PlaybackMode,
    pub(crate) delay_ms: u32,
    pub(crate) item_count: usize,
    pub(crate) target_label: String,
    pub(crate) target_title: Option<String>,
    pub(crate) ready: bool,
    pub(crate) steps: Vec<PreparedSequencePreviewStep>,
}

pub(crate) struct PreparedSequencePreviewStep {
    pub(crate) index: usize,
    pub(crate) kind: PreparedStepKind,
    pub(crate) hidden: bool,
    pub(crate) text: Option<String>,
}

impl fmt::Debug for PreparedSequenceRun {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedSequenceRun")
            .field("sequence_id", &self.sequence_id)
            .field("step_count", &self.steps.len())
            .field("delay_ms", &self.delay_ms)
            .field("origin_title_available", &self.origin_title.is_some())
            .finish()
    }
}

impl fmt::Debug for PreparedSequencePreview {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedSequencePreview")
            .field("step_count", &self.steps.len())
            .field("delay_ms", &self.delay_ms)
            .field("target_available", &self.target_title.is_some())
            .field("ready", &self.ready)
            .finish()
    }
}

impl fmt::Debug for PreparedSequencePreviewStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedSequencePreviewStep")
            .field("index", &self.index)
            .field("hidden", &self.hidden)
            .field("text_available", &self.text.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ClipboardReferenceLabel, HotkeyChord, MAX_SEQUENCE_DELAY_MS, MAX_SEQUENCE_NAME_CHARS,
        PlaybackMode, PreparedStepKind, Sequence, SequenceDraft, SequenceId,
        SequenceInvocationContext, SequencePreflight, SequencePreflightError, SequenceStore,
        SequenceStoreError, SequenceTarget, SequenceValidationError, Step,
    };
    use crate::plugins::snippets::{SnippetEntry, SnippetFieldDefinition, SnippetInputKind};
    use chrono::{DateTime, FixedOffset};
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;

    fn clipboard_step(text: &str) -> Step {
        Step::clipboard_history_reference(text)
    }

    fn entry(alias: &str, text: &str, prompted: bool, hidden: bool) -> SnippetEntry {
        SnippetEntry {
            alias: alias.into(),
            text: text.into(),
            hide_contents: hidden,
            prompt_for_fields: prompted,
            fields: prompted
                .then(|| SnippetFieldDefinition {
                    name: "name".into(),
                    label: "Name".into(),
                    default_value: "default-name".into(),
                    required: true,
                    input_kind: SnippetInputKind::SingleLine,
                })
                .into_iter()
                .collect(),
            tags: Vec::new(),
        }
    }

    fn target(process_id: u32) -> SequenceTarget {
        SequenceTarget::from_external_window(
            crate::active_window::ExternalForegroundWindow {
                hwnd: 123,
                process_id,
                thread_id: 456,
                title: Some("origin editor".into()),
            },
            999,
        )
        .unwrap()
    }

    fn invocation_context() -> SequenceInvocationContext {
        let timestamp: DateTime<FixedOffset> =
            DateTime::parse_from_rfc3339("2024-01-02T03:04:05+00:00").unwrap();
        SequenceInvocationContext::new(timestamp, Some("captured clipboard".into()), target(42))
    }

    fn sequence(steps: Vec<Step>) -> Sequence {
        Sequence {
            id: SequenceId(1),
            name: "Preflight test".into(),
            hotkey: None,
            mode: PlaybackMode::Automatic,
            delay_ms: 250,
            steps,
        }
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

    #[test]
    fn preflight_resolves_mixed_steps_with_one_fixed_context_and_ordered_preview() {
        let snippets = vec![
            entry("legacy", "literal {{date}}", false, false),
            entry(
                "system",
                "{{sys.time:%H:%M}}/{{sys.clipboard}}/{{sys.window_title}}",
                false,
                false,
            ),
            entry(
                "prompt",
                "Hello {{name}} at {{sys.time:%H:%M}}",
                true,
                false,
            ),
        ];
        let history_item = "history snapshot".to_string();
        let history = vec![history_item.clone()];
        let mut preflight = SequencePreflight::begin(
            sequence(vec![
                Step::SnippetRef {
                    alias: "legacy".into(),
                },
                Step::SnippetRef {
                    alias: "system".into(),
                },
                Step::SnippetRef {
                    alias: "prompt".into(),
                },
                clipboard_step(&history_item),
                Step::LiteralText {
                    text: "authored text".into(),
                },
            ]),
            &snippets,
            &history,
            invocation_context(),
        )
        .unwrap();

        let prompt = preflight.current_prompt().unwrap();
        assert_eq!(prompt.step_index, 2);
        assert_eq!(prompt.total_steps, 5);
        assert_eq!(prompt.values.get("name").unwrap(), "default-name");
        preflight
            .submit_current_prompt(&HashMap::from([("name".into(), "Ada".into())]))
            .unwrap();
        let run = preflight.finish(&snippets, &history).unwrap();

        assert_eq!(run.step_count(), 5);
        assert_eq!(run.step_text(0), Some("literal {{date}}"));
        assert_eq!(
            run.step_text(1),
            Some("03:04/captured clipboard/origin editor")
        );
        assert_eq!(run.step_text(2), Some("Hello Ada at 03:04"));
        assert_eq!(run.step_text(3), Some("history snapshot"));
        assert_eq!(run.step_text(4), Some("authored text"));

        let preview = run.preview();
        assert!(preview.ready);
        assert_eq!(preview.item_count, 5);
        assert_eq!(preview.target_title.as_deref(), Some("origin editor"));
        assert!(preview.target_label.contains("Current eligible"));
        assert_eq!(preview.steps.len(), 5);
        assert_eq!(preview.steps[1].kind, PreparedStepKind::Snippet);
        assert_eq!(preview.steps[3].kind, PreparedStepKind::ClipboardHistory);
        assert_eq!(preview.steps[4].kind, PreparedStepKind::LiteralText);
        assert_eq!(
            preview.steps[1].text.as_deref(),
            Some("03:04/captured clipboard/origin editor")
        );
    }

    #[test]
    fn playback_modes_preserve_legacy_serde_and_round_trip_paste_to_advance() {
        assert_eq!(PlaybackMode::default(), PlaybackMode::Automatic);
        for (json, mode) in [
            ("\"automatic\"", PlaybackMode::Automatic),
            ("\"manual_advance\"", PlaybackMode::ManualAdvance),
            ("\"paste_to_advance\"", PlaybackMode::PasteToAdvance),
        ] {
            assert_eq!(serde_json::from_str::<PlaybackMode>(json).unwrap(), mode);
            assert_eq!(serde_json::to_string(&mode).unwrap(), json);
        }
        let directory = tempfile::tempdir().unwrap();
        let store = SequenceStore::new(directory.path().join(super::PASTE_SEQUENCES_FILE));
        let mut snapshot = store.load().unwrap();
        let mut draft = SequenceDraft::new(
            "Third mode",
            vec![Step::LiteralText {
                text: "exact \u{96ea}\ntext".into(),
            }],
        );
        draft.mode = PlaybackMode::PasteToAdvance;
        let id = snapshot.upsert(draft).unwrap();
        store.commit(&mut snapshot).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(
            loaded.sequence(id).unwrap().mode,
            PlaybackMode::PasteToAdvance
        );
    }

    #[test]
    fn preflight_without_origin_freezes_empty_title_and_preserves_hidden_preview() {
        let snippets = vec![entry(
            "context",
            "{{sys.window_title}}/{{sys.clipboard}}/{{sys.time:%H:%M}}",
            false,
            true,
        )];
        let context = SequenceInvocationContext::new(
            DateTime::parse_from_rfc3339("2024-01-02T03:04:05+00:00").unwrap(),
            Some("origin clipboard".into()),
            None,
        );
        let run = SequencePreflight::begin(
            sequence(vec![Step::SnippetRef {
                alias: "context".into(),
            }]),
            &snippets,
            &[],
            context,
        )
        .unwrap()
        .finish(&snippets, &[])
        .unwrap();
        assert_eq!(run.step_text(0), Some("/origin clipboard/03:04"));
        let preview = run.preview();
        assert!(preview.ready);
        assert!(preview.target_title.is_none());
        assert!(preview.steps[0].hidden);
        assert!(preview.steps[0].text.is_none());
        assert!(!format!("{run:?}").contains("origin clipboard"));
    }

    #[test]
    fn repeated_snippet_references_receive_independent_prompt_values() {
        let snippets = vec![entry("same", "{{name}}", true, false)];
        let history = Vec::new();
        let mut preflight = SequencePreflight::begin(
            sequence(vec![
                Step::SnippetRef {
                    alias: "same".into(),
                },
                Step::SnippetRef {
                    alias: "same".into(),
                },
            ]),
            &snippets,
            &history,
            invocation_context(),
        )
        .unwrap();
        assert_eq!(preflight.current_prompt().unwrap().step_index, 0);
        assert!(
            preflight
                .submit_current_prompt(&HashMap::from([("name".into(), "first".into())]))
                .unwrap()
        );
        assert_eq!(preflight.current_prompt().unwrap().step_index, 1);
        assert_eq!(
            preflight
                .current_prompt()
                .unwrap()
                .values
                .get("name")
                .unwrap(),
            "default-name"
        );
        assert!(
            !preflight
                .submit_current_prompt(&HashMap::from([("name".into(), "second".into())]))
                .unwrap()
        );
        let run = preflight.finish(&snippets, &history).unwrap();
        assert_eq!(run.step_text(0), Some("first"));
        assert_eq!(run.step_text(1), Some("second"));
    }

    #[test]
    fn preflight_rejects_missing_ambiguous_stale_and_invalid_sources_atomically() {
        let sequence_for = |alias: &str| {
            sequence(vec![Step::SnippetRef {
                alias: alias.into(),
            }])
        };
        let context = invocation_context();
        assert!(matches!(
            SequencePreflight::begin(sequence_for("absent"), &[], &[], context.clone()),
            Err(SequencePreflightError::MissingSnippet)
        ));
        let duplicate = entry("duplicate", "first", false, false);
        assert!(matches!(
            SequencePreflight::begin(
                sequence_for("duplicate"),
                &[duplicate.clone(), duplicate],
                &[],
                context.clone()
            ),
            Err(SequencePreflightError::AmbiguousSnippet)
        ));

        let saved = entry("saved", "before", false, false);
        let preflight = SequencePreflight::begin(
            sequence_for("saved"),
            std::slice::from_ref(&saved),
            &[],
            context.clone(),
        )
        .unwrap();
        let changed = entry("saved", "after", false, false);
        assert!(matches!(
            preflight.finish(&[changed], &[]),
            Err(SequencePreflightError::StaleSnippet)
        ));

        let missing_history = SequencePreflight::begin(
            sequence(vec![clipboard_step("vanished")]),
            &[],
            &[],
            context.clone(),
        );
        assert!(matches!(
            missing_history,
            Err(SequencePreflightError::MissingHistoryItem)
        ));

        let history_item = "rolls during prompt".to_string();
        let with_history = SequencePreflight::begin(
            sequence(vec![clipboard_step(&history_item)]),
            &[],
            std::slice::from_ref(&history_item),
            context,
        )
        .unwrap();
        let run_result = with_history.finish(&[], &[]);
        assert!(matches!(
            run_result,
            Err(SequencePreflightError::MissingHistoryItem)
        ));
    }

    #[test]
    fn target_identity_rejects_unknown_or_self_windows_and_hidden_preview_masks_text() {
        let invalid = crate::active_window::ExternalForegroundWindow {
            hwnd: 0,
            process_id: 42,
            thread_id: 456,
            title: Some("unusable".into()),
        };
        assert!(matches!(
            SequenceTarget::from_external_window(invalid, 999),
            Err(SequencePreflightError::InvalidTargetIdentity)
        ));
        assert!(matches!(
            SequenceTarget::from_external_window(
                crate::active_window::ExternalForegroundWindow {
                    hwnd: 123,
                    process_id: 999,
                    thread_id: 456,
                    title: None,
                },
                999
            ),
            Err(SequencePreflightError::LauncherTarget)
        ));

        let secret = "hidden snippet secret";
        let snippets = vec![entry("hidden", secret, false, true)];
        let history = Vec::new();
        let preflight = SequencePreflight::begin(
            sequence(vec![Step::SnippetRef {
                alias: "hidden".into(),
            }]),
            &snippets,
            &history,
            invocation_context(),
        )
        .unwrap();
        let run = preflight.finish(&snippets, &history).unwrap();
        let preview = run.preview();
        assert!(preview.steps[0].hidden);
        assert!(preview.steps[0].text.is_none());
        assert!(!format!("{run:?}").contains(secret));
        assert!(!format!("{preview:?}").contains(secret));
        assert_eq!(run.step_text(0), Some(secret));
    }

    #[test]
    fn required_values_reject_empty_submission_without_losing_defaults_or_context() {
        let snippets = vec![entry("required", "{{name}}/{{sys.clipboard}}", true, false)];
        let history = Vec::new();
        let mut preflight = SequencePreflight::begin(
            sequence(vec![Step::SnippetRef {
                alias: "required".into(),
            }]),
            &snippets,
            &history,
            invocation_context(),
        )
        .unwrap();
        assert!(matches!(
            preflight.submit_current_prompt(&HashMap::from([("name".into(), String::new())])),
            Err(SequencePreflightError::RequiredValues)
        ));
        assert_eq!(
            preflight
                .current_prompt()
                .unwrap()
                .values
                .get("name")
                .unwrap(),
            "default-name"
        );
        preflight
            .submit_current_prompt(&HashMap::from([("name".into(), "kept".into())]))
            .unwrap();
        let run = preflight.finish(&snippets, &history).unwrap();
        assert_eq!(run.step_text(0), Some("kept/captured clipboard"));
    }
}
