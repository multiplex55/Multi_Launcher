//! Markdown source, bounded undo, saved baseline, and document transitions.

use std::collections::VecDeque;
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::common::atomic_file::save_atomic;
use crate::notes_markdown::{
    AppliedMarkdownEdit, MAX_MARKDOWN_SOURCE_BYTES, MarkdownEditError, PreparedMarkdownEdit,
};

use super::persistence::{
    LoadedMarkdownDocument, MarkdownDiskConflictKind, MarkdownDocumentStore, MarkdownFile,
    MarkdownPersistenceError,
};

pub const MAX_MARKDOWN_UNDO_ENTRIES: usize = 64;
pub const MAX_MARKDOWN_UNDO_BYTES: usize = 2 * MAX_MARKDOWN_SOURCE_BYTES;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PendingMarkdownTransition {
    New,
    Open(MarkdownFile),
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownTransitionDecision {
    Save,
    Discard,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownTransitionOutcome {
    Applied,
    DecisionRequired,
    Cancelled,
    SaveAsRequired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownSaveOutcome {
    Saved,
    SaveAsRequired,
}

#[derive(Debug)]
pub enum MarkdownDocumentError {
    NoActiveDocument,
    PendingTransitionExists,
    NoPendingTransition,
    RevisionExhausted,
    UndoHistoryLimit { max_bytes: usize },
    UndoHistoryOutOfSync,
    Edit(MarkdownEditError),
    Persistence(MarkdownPersistenceError),
}

impl std::fmt::Display for MarkdownDocumentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoActiveDocument => formatter.write_str("there is no active Markdown document"),
            Self::PendingTransitionExists => {
                formatter.write_str("resolve the pending Markdown transition first")
            }
            Self::NoPendingTransition => {
                formatter.write_str("there is no pending Markdown transition")
            }
            Self::RevisionExhausted => formatter.write_str("Markdown revision limit exhausted"),
            Self::UndoHistoryLimit { max_bytes } => write!(
                formatter,
                "Markdown edit cannot fit in the {max_bytes}-byte undo history limit"
            ),
            Self::UndoHistoryOutOfSync => {
                formatter.write_str("Markdown undo history no longer matches the source")
            }
            Self::Edit(error) => error.fmt(formatter),
            Self::Persistence(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for MarkdownDocumentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Edit(error) => Some(error),
            Self::Persistence(error) => Some(error),
            _ => None,
        }
    }
}

impl From<MarkdownEditError> for MarkdownDocumentError {
    fn from(error: MarkdownEditError) -> Self {
        Self::Edit(error)
    }
}

impl From<MarkdownPersistenceError> for MarkdownDocumentError {
    fn from(error: MarkdownPersistenceError) -> Self {
        Self::Persistence(error)
    }
}

struct EditDelta {
    start: usize,
    removed: String,
    inserted: String,
}

impl EditDelta {
    fn byte_cost(&self) -> usize {
        self.removed.len().saturating_add(self.inserted.len())
    }
}

#[derive(Default)]
struct UndoHistory {
    undo: VecDeque<EditDelta>,
    redo: Vec<EditDelta>,
    bytes: usize,
}

impl UndoHistory {
    fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn push(&mut self, edit: EditDelta) -> Result<(), MarkdownDocumentError> {
        let cost = edit.byte_cost();
        if cost > MAX_MARKDOWN_UNDO_BYTES {
            return Err(MarkdownDocumentError::UndoHistoryLimit {
                max_bytes: MAX_MARKDOWN_UNDO_BYTES,
            });
        }
        for redo in self.redo.drain(..) {
            self.bytes = self.bytes.saturating_sub(redo.byte_cost());
        }
        while self.undo.len() >= MAX_MARKDOWN_UNDO_ENTRIES
            || self.bytes.saturating_add(cost) > MAX_MARKDOWN_UNDO_BYTES
        {
            let Some(oldest) = self.undo.pop_front() else {
                return Err(MarkdownDocumentError::UndoHistoryLimit {
                    max_bytes: MAX_MARKDOWN_UNDO_BYTES,
                });
            };
            self.bytes = self.bytes.saturating_sub(oldest.byte_cost());
        }
        self.bytes += cost;
        self.undo.push_back(edit);
        Ok(())
    }
}

struct ActiveMarkdownDocument {
    source: String,
    saved_source: String,
    path: Option<PathBuf>,
    disk_digest: Option<[u8; 32]>,
    revision: u64,
    history: UndoHistory,
}

impl ActiveMarkdownDocument {
    fn blank() -> Self {
        Self::from_source(String::new(), None, None)
    }

    fn loaded(loaded: LoadedMarkdownDocument, path: PathBuf) -> Self {
        Self::from_source(loaded.source, Some(path), Some(loaded.disk_digest))
    }

    fn from_source(source: String, path: Option<PathBuf>, disk_digest: Option<[u8; 32]>) -> Self {
        Self {
            saved_source: source.clone(),
            source,
            path,
            disk_digest,
            revision: 0,
            history: UndoHistory::default(),
        }
    }

    fn is_dirty(&self) -> bool {
        self.source != self.saved_source
    }
}

/// Owns one plain UTF-8 Markdown draft, its path/disk digest, saved-content
/// baseline, bounded undo history, and one pending replacement/close decision.
/// No filesystem work occurs on construction.
pub struct MarkdownDocument {
    store: MarkdownDocumentStore,
    active: Option<ActiveMarkdownDocument>,
    pending: Option<PendingMarkdownTransition>,
}

impl Default for MarkdownDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownDocument {
    pub fn new() -> Self {
        Self {
            store: MarkdownDocumentStore,
            active: Some(ActiveMarkdownDocument::blank()),
            pending: None,
        }
    }

    pub fn source(&self) -> Option<&str> {
        self.active.as_ref().map(|active| active.source.as_str())
    }

    pub fn saved_source(&self) -> Option<&str> {
        self.active
            .as_ref()
            .map(|active| active.saved_source.as_str())
    }

    pub fn path(&self) -> Option<&Path> {
        self.active
            .as_ref()
            .and_then(|active| active.path.as_deref())
    }

    pub fn disk_digest(&self) -> Option<[u8; 32]> {
        self.active.as_ref().and_then(|active| active.disk_digest)
    }

    pub fn revision(&self) -> Option<u64> {
        self.active.as_ref().map(|active| active.revision)
    }

    pub fn is_dirty(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(ActiveMarkdownDocument::is_dirty)
    }

    pub fn pending_transition(&self) -> Option<&PendingMarkdownTransition> {
        self.pending.as_ref()
    }

    pub fn can_undo(&self) -> bool {
        self.pending.is_none()
            && self
                .active
                .as_ref()
                .is_some_and(|active| active.history.can_undo())
    }

    pub fn can_redo(&self) -> bool {
        self.pending.is_none()
            && self
                .active
                .as_ref()
                .is_some_and(|active| active.history.can_redo())
    }

    pub fn undo_depth(&self) -> usize {
        self.active
            .as_ref()
            .map_or(0, |active| active.history.undo.len())
    }

    pub fn redo_depth(&self) -> usize {
        self.active
            .as_ref()
            .map_or(0, |active| active.history.redo.len())
    }

    /// Replace the full editor source as one history entry. The common changed
    /// range is UTF-8 aligned so edits to large notes do not duplicate the file
    /// in undo history when only a small region changed.
    pub fn replace_source(
        &mut self,
        source: impl AsRef<str>,
    ) -> Result<bool, MarkdownDocumentError> {
        let source = source.as_ref();
        if source.len() > MAX_MARKDOWN_SOURCE_BYTES {
            return Err(MarkdownEditError::SourceTooLarge {
                max_bytes: MAX_MARKDOWN_SOURCE_BYTES,
            }
            .into());
        }
        let active = self.ensure_editable()?;
        if source == active.source {
            return Ok(false);
        }
        let (old_range, new_range) = changed_ranges(&active.source, source);
        let edit = EditDelta {
            start: old_range.start,
            removed: active.source[old_range].to_owned(),
            inserted: source[new_range].to_owned(),
        };
        self.commit_source(source.to_owned(), edit)
    }

    /// Apply a prepared M6-A authoring/table/fence edit only to the exact source
    /// snapshot from which it was prepared.
    pub fn apply_prepared_edit(
        &mut self,
        edit: &PreparedMarkdownEdit,
    ) -> Result<AppliedMarkdownEdit, MarkdownDocumentError> {
        let active = self.ensure_editable()?;
        let applied = edit.apply(&active.source)?;
        if applied.source == active.source {
            return Ok(applied);
        }
        let range = edit.range();
        let history = EditDelta {
            start: range.start,
            removed: active.source[range].to_owned(),
            inserted: edit.replacement().to_owned(),
        };
        self.commit_source(applied.source.clone(), history)?;
        Ok(applied)
    }

    pub fn undo(&mut self) -> Result<bool, MarkdownDocumentError> {
        let active = self.ensure_editable()?;
        let Some(edit) = active.history.undo.back() else {
            return Ok(false);
        };
        let revision = next_revision(active.revision)?;
        let source = apply_delta(&active.source, edit.start, &edit.inserted, &edit.removed)
            .ok_or(MarkdownDocumentError::UndoHistoryOutOfSync)?;
        let edit = active.history.undo.pop_back().expect("checked undo entry");
        active.history.redo.push(edit);
        active.source = source;
        active.revision = revision;
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, MarkdownDocumentError> {
        let active = self.ensure_editable()?;
        let Some(edit) = active.history.redo.last() else {
            return Ok(false);
        };
        let revision = next_revision(active.revision)?;
        let source = apply_delta(&active.source, edit.start, &edit.removed, &edit.inserted)
            .ok_or(MarkdownDocumentError::UndoHistoryOutOfSync)?;
        let edit = active.history.redo.pop().expect("checked redo entry");
        active.history.undo.push_back(edit);
        active.source = source;
        active.revision = revision;
        Ok(true)
    }

    pub fn request_new(&mut self) -> Result<MarkdownTransitionOutcome, MarkdownDocumentError> {
        let requested = PendingMarkdownTransition::New;
        if let Some(outcome) = self.repeat_or_reject_pending(&requested)? {
            return Ok(outcome);
        }
        if self.is_dirty() {
            self.pending = Some(requested);
            return Ok(MarkdownTransitionOutcome::DecisionRequired);
        }
        self.active = Some(ActiveMarkdownDocument::blank());
        Ok(MarkdownTransitionOutcome::Applied)
    }

    /// Read and validate a candidate before asking about a dirty draft. When
    /// Save is chosen later, the path is reread rather than installing this
    /// potentially stale candidate.
    pub fn request_open(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<MarkdownTransitionOutcome, MarkdownDocumentError> {
        let file = MarkdownFile::new(path)?;
        let requested = PendingMarkdownTransition::Open(file.clone());
        if let Some(outcome) = self.repeat_or_reject_pending(&requested)? {
            return Ok(outcome);
        }
        let loaded = self.store.open(&file)?;
        if self.is_dirty() {
            self.pending = Some(requested);
            return Ok(MarkdownTransitionOutcome::DecisionRequired);
        }
        self.active = Some(ActiveMarkdownDocument::loaded(
            loaded,
            file.path().to_path_buf(),
        ));
        Ok(MarkdownTransitionOutcome::Applied)
    }

    pub fn request_close(&mut self) -> Result<MarkdownTransitionOutcome, MarkdownDocumentError> {
        let requested = PendingMarkdownTransition::Close;
        if let Some(outcome) = self.repeat_or_reject_pending(&requested)? {
            return Ok(outcome);
        }
        if self.active.is_none() {
            return Ok(MarkdownTransitionOutcome::Applied);
        }
        if self.is_dirty() {
            self.pending = Some(requested);
            return Ok(MarkdownTransitionOutcome::DecisionRequired);
        }
        self.active = None;
        Ok(MarkdownTransitionOutcome::Applied)
    }

    pub fn resolve_pending(
        &mut self,
        decision: MarkdownTransitionDecision,
    ) -> Result<MarkdownTransitionOutcome, MarkdownDocumentError> {
        self.resolve_pending_with_writer(decision, save_atomic)
    }

    /// Continue a pending Save decision after the caller chooses a new `.md`
    /// path. No write occurs unless a pending transition exists.
    pub fn resolve_pending_save_as(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<MarkdownTransitionOutcome, MarkdownDocumentError> {
        self.resolve_pending_save_as_with_writer(path.as_ref(), save_atomic)
    }

    pub fn save(&mut self) -> Result<MarkdownSaveOutcome, MarkdownDocumentError> {
        self.ensure_no_pending_transition()?;
        self.save_current_with_writer(save_atomic)
    }

    pub fn save_as(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<MarkdownSaveOutcome, MarkdownDocumentError> {
        self.ensure_no_pending_transition()?;
        self.save_as_current_with_writer(path.as_ref(), save_atomic)
    }

    fn resolve_pending_with_writer(
        &mut self,
        decision: MarkdownTransitionDecision,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<MarkdownTransitionOutcome, MarkdownDocumentError> {
        let pending = self
            .pending
            .clone()
            .ok_or(MarkdownDocumentError::NoPendingTransition)?;
        match decision {
            MarkdownTransitionDecision::Cancel => {
                self.pending = None;
                Ok(MarkdownTransitionOutcome::Cancelled)
            }
            MarkdownTransitionDecision::Save => {
                if self.save_current_with_writer(writer)? == MarkdownSaveOutcome::SaveAsRequired {
                    return Ok(MarkdownTransitionOutcome::SaveAsRequired);
                }
                self.apply_pending(pending)
            }
            MarkdownTransitionDecision::Discard => self.apply_pending(pending),
        }
    }

    fn resolve_pending_save_as_with_writer(
        &mut self,
        path: &Path,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<MarkdownTransitionOutcome, MarkdownDocumentError> {
        let pending = self
            .pending
            .clone()
            .ok_or(MarkdownDocumentError::NoPendingTransition)?;
        self.save_as_current_with_writer(path, writer)?;
        self.apply_pending(pending)
    }

    fn apply_pending(
        &mut self,
        pending: PendingMarkdownTransition,
    ) -> Result<MarkdownTransitionOutcome, MarkdownDocumentError> {
        match pending {
            PendingMarkdownTransition::New => {
                self.active = Some(ActiveMarkdownDocument::blank());
            }
            PendingMarkdownTransition::Close => self.active = None,
            PendingMarkdownTransition::Open(file) => {
                let loaded = self.store.open(&file)?;
                self.active = Some(ActiveMarkdownDocument::loaded(
                    loaded,
                    file.path().to_path_buf(),
                ));
            }
        }
        self.pending = None;
        Ok(MarkdownTransitionOutcome::Applied)
    }

    fn save_current_with_writer(
        &mut self,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<MarkdownSaveOutcome, MarkdownDocumentError> {
        let active = self
            .active
            .as_ref()
            .ok_or(MarkdownDocumentError::NoActiveDocument)?;
        let Some(path) = active.path.as_deref() else {
            return Ok(MarkdownSaveOutcome::SaveAsRequired);
        };
        let file = MarkdownFile::new(path)?;
        let expected_digest = active
            .disk_digest
            .ok_or(MarkdownDocumentError::NoActiveDocument)?;
        let saved_snapshot = active.source.clone();
        let digest = self.store.save_existing_with_writer(
            &file,
            expected_digest,
            &saved_snapshot,
            writer,
        )?;
        let active = self
            .active
            .as_mut()
            .ok_or(MarkdownDocumentError::NoActiveDocument)?;
        active.saved_source = saved_snapshot;
        active.disk_digest = Some(digest);
        Ok(MarkdownSaveOutcome::Saved)
    }

    fn save_as_current_with_writer(
        &mut self,
        path: &Path,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<MarkdownSaveOutcome, MarkdownDocumentError> {
        let file = MarkdownFile::new(path)?;
        let active = self
            .active
            .as_ref()
            .ok_or(MarkdownDocumentError::NoActiveDocument)?;
        let saved_snapshot = active.source.clone();
        let digest = self
            .store
            .save_as_with_writer(&file, &saved_snapshot, writer)?;
        let active = self
            .active
            .as_mut()
            .ok_or(MarkdownDocumentError::NoActiveDocument)?;
        active.saved_source = saved_snapshot;
        active.path = Some(path.to_path_buf());
        active.disk_digest = Some(digest);
        Ok(MarkdownSaveOutcome::Saved)
    }

    fn commit_source(
        &mut self,
        source: String,
        edit: EditDelta,
    ) -> Result<bool, MarkdownDocumentError> {
        let active = self
            .active
            .as_mut()
            .ok_or(MarkdownDocumentError::NoActiveDocument)?;
        if source == active.source {
            return Ok(false);
        }
        let revision = next_revision(active.revision)?;
        active.history.push(edit)?;
        active.source = source;
        active.revision = revision;
        Ok(true)
    }

    fn ensure_editable(&mut self) -> Result<&mut ActiveMarkdownDocument, MarkdownDocumentError> {
        self.ensure_no_pending_transition()?;
        self.active
            .as_mut()
            .ok_or(MarkdownDocumentError::NoActiveDocument)
    }

    fn ensure_no_pending_transition(&self) -> Result<(), MarkdownDocumentError> {
        if self.pending.is_some() {
            Err(MarkdownDocumentError::PendingTransitionExists)
        } else {
            Ok(())
        }
    }

    fn repeat_or_reject_pending(
        &self,
        requested: &PendingMarkdownTransition,
    ) -> Result<Option<MarkdownTransitionOutcome>, MarkdownDocumentError> {
        match self.pending.as_ref() {
            None => Ok(None),
            Some(pending) if pending == requested => {
                Ok(Some(MarkdownTransitionOutcome::DecisionRequired))
            }
            Some(_) => Err(MarkdownDocumentError::PendingTransitionExists),
        }
    }
}

fn next_revision(revision: u64) -> Result<u64, MarkdownDocumentError> {
    revision
        .checked_add(1)
        .ok_or(MarkdownDocumentError::RevisionExhausted)
}

fn changed_ranges(old: &str, new: &str) -> (Range<usize>, Range<usize>) {
    let mut prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    while prefix > 0 && (!old.is_char_boundary(prefix) || !new.is_char_boundary(prefix)) {
        prefix -= 1;
    }
    let mut suffix = old[prefix..]
        .bytes()
        .rev()
        .zip(new[prefix..].bytes().rev())
        .take_while(|(left, right)| left == right)
        .count();
    while suffix > 0
        && (!old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix))
    {
        suffix -= 1;
    }
    (prefix..old.len() - suffix, prefix..new.len() - suffix)
}

fn apply_delta(source: &str, start: usize, expected: &str, replacement: &str) -> Option<String> {
    let end = start.checked_add(expected.len())?;
    if source.get(start..end)? != expected {
        return None;
    }
    let len = source
        .len()
        .checked_sub(expected.len())?
        .checked_add(replacement.len())?;
    if len > MAX_MARKDOWN_SOURCE_BYTES {
        return None;
    }
    let mut result = String::with_capacity(len);
    result.push_str(&source[..start]);
    result.push_str(replacement);
    result.push_str(&source[end..]);
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown_studio::MAX_MARKDOWN_DOCUMENT_BYTES;
    use crate::notes_markdown::{
        MarkdownBlockSnippet, MarkdownInlineFormat, parse_markdown_tables, prepare_block_snippet,
        prepare_inline_format, prepare_insert_column,
    };
    use std::fs;

    #[test]
    fn markdown_document_saves_and_reopens_exact_unicode_bom_and_crlf_source() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notes.md");
        let exact = "\u{feff}# Café 🌱\r\n\r\nsecond line\r\n";
        let mut document = MarkdownDocument::new();
        assert!(document.replace_source(exact).unwrap());
        assert!(document.is_dirty());
        assert_eq!(document.save_as(&path).unwrap(), MarkdownSaveOutcome::Saved);
        assert_eq!(fs::read(&path).unwrap(), exact.as_bytes());
        assert_eq!(document.source(), Some(exact));
        assert_eq!(document.saved_source(), Some(exact));
        assert!(!document.is_dirty());

        let mut reopened = MarkdownDocument::new();
        assert_eq!(
            reopened.request_open(&path).unwrap(),
            MarkdownTransitionOutcome::Applied
        );
        assert_eq!(reopened.source(), Some(exact));
        assert_eq!(reopened.path(), Some(path.as_path()));
        assert_eq!(reopened.disk_digest(), document.disk_digest());
        assert!(!reopened.is_dirty());
    }

    #[test]
    fn markdown_document_dirty_transitions_cancel_discard_and_reread_open_after_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notes.md");
        fs::write(&path, "saved baseline\r\n").unwrap();
        let mut document = MarkdownDocument::new();
        assert_eq!(
            document.request_open(&path).unwrap(),
            MarkdownTransitionOutcome::Applied
        );
        assert!(document.replace_source("draft 🌿\r\n").unwrap());

        assert_eq!(
            document.request_new().unwrap(),
            MarkdownTransitionOutcome::DecisionRequired
        );
        assert!(matches!(
            document.replace_source("must stay blocked"),
            Err(MarkdownDocumentError::PendingTransitionExists)
        ));
        assert_eq!(
            document
                .resolve_pending(MarkdownTransitionDecision::Cancel)
                .unwrap(),
            MarkdownTransitionOutcome::Cancelled
        );
        assert_eq!(document.source(), Some("draft 🌿\r\n"));
        assert!(document.is_dirty());

        assert_eq!(
            document.request_open(&path).unwrap(),
            MarkdownTransitionOutcome::DecisionRequired
        );
        assert_eq!(
            document
                .resolve_pending(MarkdownTransitionDecision::Save)
                .unwrap(),
            MarkdownTransitionOutcome::Applied
        );
        assert_eq!(fs::read(&path).unwrap(), "draft 🌿\r\n".as_bytes());
        assert_eq!(document.source(), Some("draft 🌿\r\n"));
        assert_eq!(document.path(), Some(path.as_path()));
        assert!(!document.is_dirty());
        assert_eq!(document.undo_depth(), 0);

        document.replace_source("discard me").unwrap();
        assert_eq!(
            document.request_new().unwrap(),
            MarkdownTransitionOutcome::DecisionRequired
        );
        assert_eq!(
            document
                .resolve_pending(MarkdownTransitionDecision::Discard)
                .unwrap(),
            MarkdownTransitionOutcome::Applied
        );
        assert_eq!(document.source(), Some(""));
        assert_eq!(document.path(), None);
        assert!(!document.is_dirty());

        document.replace_source("keep on cancel").unwrap();
        assert_eq!(
            document.request_close().unwrap(),
            MarkdownTransitionOutcome::DecisionRequired
        );
        assert_eq!(
            document
                .resolve_pending(MarkdownTransitionDecision::Cancel)
                .unwrap(),
            MarkdownTransitionOutcome::Cancelled
        );
        assert_eq!(document.source(), Some("keep on cancel"));
        assert!(document.is_dirty());
        assert_eq!(
            document.request_close().unwrap(),
            MarkdownTransitionOutcome::DecisionRequired
        );
        assert_eq!(
            document
                .resolve_pending(MarkdownTransitionDecision::Discard)
                .unwrap(),
            MarkdownTransitionOutcome::Applied
        );
        assert_eq!(document.source(), None);
    }

    #[test]
    fn markdown_document_blank_save_as_continuation_and_failed_write_retain_pending_close() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("draft.md");
        let mut document = MarkdownDocument::new();
        assert_eq!(
            document.save().unwrap(),
            MarkdownSaveOutcome::SaveAsRequired
        );
        assert_eq!(document.source(), Some(""));
        assert!(!document.is_dirty());

        document.replace_source("unsaved draft\r\n").unwrap();
        assert_eq!(
            document.request_close().unwrap(),
            MarkdownTransitionOutcome::DecisionRequired
        );
        assert_eq!(
            document
                .resolve_pending(MarkdownTransitionDecision::Save)
                .unwrap(),
            MarkdownTransitionOutcome::SaveAsRequired
        );
        assert!(matches!(
            document.pending_transition(),
            Some(PendingMarkdownTransition::Close)
        ));
        let source_before = document.source().unwrap().to_owned();
        let saved_before = document.saved_source().unwrap().to_owned();
        let pending_before = document.pending_transition().cloned();
        let result = document.resolve_pending_save_as_with_writer(&path, |_path, _bytes| {
            anyhow::bail!("injected save failure")
        });
        assert!(matches!(
            result,
            Err(MarkdownDocumentError::Persistence(
                MarkdownPersistenceError::AtomicWrite { .. }
            ))
        ));
        assert_eq!(document.source(), Some(source_before.as_str()));
        assert_eq!(document.saved_source(), Some(saved_before.as_str()));
        assert_eq!(document.path(), None);
        assert!(document.is_dirty());
        assert_eq!(document.pending_transition(), pending_before.as_ref());
        assert!(!path.exists());

        assert_eq!(
            document.resolve_pending_save_as(&path).unwrap(),
            MarkdownTransitionOutcome::Applied
        );
        assert_eq!(fs::read(&path).unwrap(), b"unsaved draft\r\n");
        assert_eq!(document.source(), None);
        assert_eq!(document.pending_transition(), None);

        let other_path = directory.path().join("other.md");
        let mut no_pending = MarkdownDocument::new();
        let mut writes = 0;
        assert!(matches!(
            no_pending.resolve_pending_save_as_with_writer(&other_path, |_path, _bytes| {
                writes += 1;
                Ok(())
            }),
            Err(MarkdownDocumentError::NoPendingTransition)
        ));
        assert_eq!(writes, 0);
        assert!(!other_path.exists());
    }

    #[test]
    fn markdown_document_conflicts_and_failed_saves_preserve_active_state() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notes.md");
        let occupied = directory.path().join("occupied.md");
        fs::write(&occupied, b"do not replace").unwrap();
        let mut document = MarkdownDocument::new();
        document.replace_source("original").unwrap();
        assert!(matches!(
            document.save_as(&occupied),
            Err(MarkdownDocumentError::Persistence(
                MarkdownPersistenceError::OccupiedPath(_)
            ))
        ));
        assert_eq!(fs::read(&occupied).unwrap(), b"do not replace");
        assert_eq!(document.path(), None);
        assert_eq!(document.saved_source(), Some(""));

        document.save_as(&path).unwrap();
        let original_digest = document.disk_digest();
        let original_saved = document.saved_source().unwrap().to_owned();
        document.replace_source("updated").unwrap();
        let current_source = document.source().unwrap().to_owned();
        let current_revision = document.revision();
        assert!(matches!(
            document.save_current_with_writer(|_path, _bytes| {
                anyhow::bail!("injected atomic failure")
            }),
            Err(MarkdownDocumentError::Persistence(
                MarkdownPersistenceError::AtomicWrite { .. }
            ))
        ));
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(document.source(), Some(current_source.as_str()));
        assert_eq!(document.saved_source(), Some(original_saved.as_str()));
        assert_eq!(document.disk_digest(), original_digest);
        assert_eq!(document.revision(), current_revision);
        assert!(document.is_dirty());

        fs::write(&path, b"external").unwrap();
        assert!(matches!(
            document.save(),
            Err(MarkdownDocumentError::Persistence(
                MarkdownPersistenceError::DiskConflict {
                    kind: MarkdownDiskConflictKind::Changed,
                    ..
                }
            ))
        ));
        assert_eq!(document.source(), Some(current_source.as_str()));
        assert_eq!(document.saved_source(), Some(original_saved.as_str()));
        assert_eq!(document.disk_digest(), original_digest);
        fs::remove_file(&path).unwrap();
        assert!(matches!(
            document.save(),
            Err(MarkdownDocumentError::Persistence(
                MarkdownPersistenceError::DiskConflict {
                    kind: MarkdownDiskConflictKind::Deleted,
                    ..
                }
            ))
        ));
        assert_eq!(document.source(), Some(current_source.as_str()));
        assert!(document.is_dirty());
    }

    #[test]
    fn markdown_document_rejects_invalid_open_without_replacing_the_draft() {
        let directory = tempfile::tempdir().unwrap();
        let invalid_utf8 = directory.path().join("invalid.md");
        let oversized = directory.path().join("oversized.md");
        let directory_path = directory.path().join("folder.md");
        fs::write(&invalid_utf8, [0xff, 0xfe]).unwrap();
        fs::write(&oversized, vec![b'x'; MAX_MARKDOWN_DOCUMENT_BYTES + 1]).unwrap();
        fs::create_dir(&directory_path).unwrap();
        let mut document = MarkdownDocument::new();
        document.replace_source("active draft").unwrap();
        let original_revision = document.revision();

        assert!(matches!(
            document.request_open(&invalid_utf8),
            Err(MarkdownDocumentError::Persistence(
                MarkdownPersistenceError::InvalidUtf8(_)
            ))
        ));
        assert!(matches!(
            document.request_open(&oversized),
            Err(MarkdownDocumentError::Persistence(
                MarkdownPersistenceError::Oversized { .. }
            ))
        ));
        assert!(matches!(
            document.request_open(&directory_path),
            Err(MarkdownDocumentError::Persistence(
                MarkdownPersistenceError::UnsafePath(_)
            ))
        ));
        assert!(matches!(
            document.request_open(directory.path().join("wrong.txt")),
            Err(MarkdownDocumentError::Persistence(
                MarkdownPersistenceError::WrongExtension(_)
            ))
        ));
        assert_eq!(document.source(), Some("active draft"));
        assert_eq!(document.revision(), original_revision);
        assert!(document.is_dirty());
        assert_eq!(document.pending_transition(), None);
    }

    #[test]
    fn markdown_document_uses_one_bounded_undo_owner_for_prepared_edits() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notes.md");
        let source = "word\r\n| A | B |\r\n| --- | --- |\r\n| x | y |\r\n";
        let mut document = MarkdownDocument::new();
        document.replace_source(source).unwrap();
        document.save_as(&path).unwrap();
        let stale = prepare_inline_format(
            document.source().unwrap(),
            0..4,
            MarkdownInlineFormat::Strong,
        )
        .unwrap();

        let formatted = document.apply_prepared_edit(&stale).unwrap();
        assert_eq!(&formatted.source[formatted.selection], "**word**");
        assert_eq!(
            document.source(),
            Some("**word**\r\n| A | B |\r\n| --- | --- |\r\n| x | y |\r\n")
        );
        assert!(document.is_dirty());
        assert_eq!(document.undo_depth(), 2);
        assert!(document.undo().unwrap());
        assert_eq!(document.source(), Some(source));
        assert!(!document.is_dirty());
        assert!(!document.replace_source(source).unwrap());
        assert_eq!(document.redo_depth(), 1);
        assert!(document.redo().unwrap());
        assert!(document.is_dirty());
        let revision = document.revision();
        let undo_depth = document.undo_depth();
        assert!(matches!(
            document.apply_prepared_edit(&stale),
            Err(MarkdownDocumentError::Edit(
                MarkdownEditError::SourceChanged
            ))
        ));
        assert_eq!(document.revision(), revision);
        assert_eq!(document.undo_depth(), undo_depth);

        let table_source = document.source().unwrap();
        let table = parse_markdown_tables(table_source).unwrap().remove(0);
        let table_edit = prepare_insert_column(table_source, &table, 1, "Added").unwrap();
        let table_result = document.apply_prepared_edit(&table_edit).unwrap();
        assert!(table_result.source.contains("| A | Added | B |"));
        assert!(document.undo().unwrap());
        assert_eq!(
            document.source(),
            Some("**word**\r\n| A | B |\r\n| --- | --- |\r\n| x | y |\r\n")
        );
        assert!(document.redo().unwrap());

        let block_source = document.source().unwrap();
        let block_edit = prepare_block_snippet(
            block_source,
            0.."**word**".len(),
            MarkdownBlockSnippet::Quote,
        )
        .unwrap();
        let block_result = document.apply_prepared_edit(&block_edit).unwrap();
        assert!(block_result.source.starts_with("> **word**\r\n"));
        assert!(document.undo().unwrap());
        assert!(document.source().unwrap().starts_with("**word**\r\n"));
        assert!(document.redo().unwrap());
        assert!(document.source().unwrap().starts_with("> **word**\r\n"));
    }

    #[test]
    fn markdown_document_history_stays_within_entry_bound() {
        let mut document = MarkdownDocument::new();
        for index in 0..MAX_MARKDOWN_UNDO_ENTRIES + 8 {
            let current = document.source().unwrap();
            let updated = format!("{current}{index};");
            document.replace_source(updated).unwrap();
        }
        assert_eq!(document.undo_depth(), MAX_MARKDOWN_UNDO_ENTRIES);
        for _ in 0..MAX_MARKDOWN_UNDO_ENTRIES {
            assert!(document.undo().unwrap());
        }
        assert!(!document.can_undo());
        assert!(document.can_redo());
        assert!(document.undo_depth() <= MAX_MARKDOWN_UNDO_ENTRIES);
        assert!(document.redo_depth() <= MAX_MARKDOWN_UNDO_ENTRIES);

        let mut large_deltas = MarkdownDocument::new();
        large_deltas.replace_source("a".repeat(100_000)).unwrap();
        for index in 0..32 {
            let marker = if index % 2 == 0 { 'b' } else { 'a' };
            let mut updated = marker.to_string().repeat(40_000);
            updated.push_str(&"a".repeat(60_000));
            large_deltas.replace_source(updated).unwrap();
        }
        let history = &large_deltas.active.as_ref().unwrap().history;
        assert!(history.bytes <= MAX_MARKDOWN_UNDO_BYTES);
    }
}
