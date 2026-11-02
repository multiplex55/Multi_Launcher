//! Editable Calculation Pad session with bounded history and dirty handling.

use super::document::{
    CalculationDocument, DocumentDiagnostic, EditError, LineOutcome, MAX_DOCUMENT_BYTES,
    MAX_SOURCE_LINES,
};
use super::persistence::{
    DocumentId, LineId, LoadedDocument, PadDocumentStore, PadDocumentSummary, PadStoreError,
    StoredDocument,
};
use crate::platform::app_data::AppDataRoot;
use std::collections::VecDeque;
use std::path::Path;

const MAX_UNDO_ENTRIES: usize = 64;
const MAX_UNDO_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct SessionLineResult {
    pub id: LineId,
    pub line_index: usize,
    pub source_range: std::ops::Range<usize>,
    pub assignment_name: Option<String>,
    pub dependency_indices: Vec<usize>,
    pub outcome: LineOutcome,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionEvaluation {
    pub revision: u64,
    pub lines: Vec<SessionLineResult>,
    pub diagnostic: Option<DocumentDiagnostic>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingTransition {
    New,
    Open(DocumentId),
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionDecision {
    Save,
    Discard,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionOutcome {
    Applied,
    DecisionRequired,
    Cancelled,
}

#[derive(Debug)]
pub enum SessionError {
    Store(PadStoreError),
    Edit(EditError),
    NoActiveDocument,
    SaveAsRequired,
    PendingTransitionExists,
    NoPendingTransition,
    DocumentTooLarge { max_bytes: usize },
    TooManyLines { max_lines: usize },
    LineIdNotFound(LineId),
    LineIdStateMismatch,
    LineIdExhausted,
    RevisionExhausted,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => error.fmt(formatter),
            Self::Edit(error) => error.fmt(formatter),
            Self::NoActiveDocument => {
                formatter.write_str("there is no active Calculation Pad document")
            }
            Self::SaveAsRequired => {
                formatter.write_str("save this untitled document with a name first")
            }
            Self::PendingTransitionExists => {
                formatter.write_str("resolve the pending document transition first")
            }
            Self::NoPendingTransition => {
                formatter.write_str("there is no pending document transition")
            }
            Self::DocumentTooLarge { max_bytes } => {
                write!(formatter, "document exceeds the {max_bytes}-byte limit")
            }
            Self::TooManyLines { max_lines } => {
                write!(formatter, "document exceeds the {max_lines}-line limit")
            }
            Self::LineIdNotFound(id) => {
                write!(formatter, "line {} is not in the active document", id.get())
            }
            Self::LineIdStateMismatch => {
                formatter.write_str("line identity count does not match source lines")
            }
            Self::LineIdExhausted => formatter.write_str("no more stable line IDs are available"),
            Self::RevisionExhausted => formatter.write_str("document revision limit was reached"),
        }
    }
}

impl std::error::Error for SessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Store(error) => Some(error),
            Self::Edit(error) => Some(error),
            _ => None,
        }
    }
}

impl From<PadStoreError> for SessionError {
    fn from(error: PadStoreError) -> Self {
        Self::Store(error)
    }
}

impl From<EditError> for SessionError {
    fn from(error: EditError) -> Self {
        Self::Edit(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EditableState {
    source: String,
    line_ids: Vec<LineId>,
}

impl EditableState {
    fn byte_size(&self) -> usize {
        self.source.len().saturating_add(
            self.line_ids
                .len()
                .saturating_mul(std::mem::size_of::<LineId>()),
        )
    }
}

#[derive(Clone, Debug)]
struct HistoryEntry {
    before: EditableState,
    after: EditableState,
}

impl HistoryEntry {
    fn byte_size(&self) -> usize {
        self.before
            .byte_size()
            .saturating_add(self.after.byte_size())
    }
}

#[derive(Default)]
struct EditHistory {
    undo: VecDeque<HistoryEntry>,
    redo: VecDeque<HistoryEntry>,
}

impl EditHistory {
    fn record(&mut self, before: EditableState, after: EditableState) {
        if before == after {
            return;
        }
        self.redo.clear();
        self.undo.push_back(HistoryEntry { before, after });
        self.trim();
    }

    fn trim(&mut self) {
        while self.undo.len().saturating_add(self.redo.len()) > MAX_UNDO_ENTRIES
            || self.total_bytes() > MAX_UNDO_BYTES
        {
            if self.undo.pop_front().is_none() {
                self.redo.pop_front();
            }
        }
    }

    fn total_bytes(&self) -> usize {
        self.undo
            .iter()
            .chain(self.redo.iter())
            .map(HistoryEntry::byte_size)
            .fold(0usize, usize::saturating_add)
    }

    fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[derive(Clone, Debug)]
struct DocumentBaseline {
    state: EditableState,
    id: DocumentId,
    name: Option<String>,
    disk_sha256: Option<[u8; 32]>,
    next_line_id: u64,
}

struct ActiveDocument {
    id: DocumentId,
    name: Option<String>,
    source: String,
    line_ids: Vec<LineId>,
    next_line_id: u64,
    revision: u64,
    baseline: DocumentBaseline,
    cached_evaluation: Option<SessionEvaluation>,
    #[cfg(test)]
    evaluation_count: usize,
}

impl ActiveDocument {
    fn new_blank() -> Self {
        let id = DocumentId::fresh();
        let mut document = Self {
            id,
            name: None,
            source: String::new(),
            line_ids: vec![LineId(1)],
            next_line_id: 2,
            revision: 0,
            baseline: DocumentBaseline {
                state: EditableState {
                    source: String::new(),
                    line_ids: vec![LineId(1)],
                },
                id,
                name: None,
                disk_sha256: None,
                next_line_id: 2,
            },
            cached_evaluation: None,
            #[cfg(test)]
            evaluation_count: 0,
        };
        document.baseline.state = document.editable_state();
        document
    }

    fn from_loaded(loaded: LoadedDocument) -> Self {
        let stored = loaded.stored;
        let saved_id = stored.id;
        let saved_name = stored.name.clone();
        let saved_next_line_id = stored.next_line_id;
        let mut document = Self {
            id: stored.id,
            name: Some(stored.name),
            source: stored.source,
            line_ids: stored.line_ids,
            next_line_id: stored.next_line_id,
            revision: 0,
            baseline: DocumentBaseline {
                state: EditableState {
                    source: String::new(),
                    line_ids: Vec::new(),
                },
                id: saved_id,
                name: Some(saved_name),
                disk_sha256: Some(loaded.disk_sha256),
                next_line_id: saved_next_line_id,
            },
            cached_evaluation: None,
            #[cfg(test)]
            evaluation_count: 0,
        };
        document.baseline.state = document.editable_state();
        document
    }

    fn editable_state(&self) -> EditableState {
        EditableState {
            source: self.source.clone(),
            line_ids: self.line_ids.clone(),
        }
    }

    fn is_dirty(&self) -> bool {
        self.source != self.baseline.state.source
            || self.line_ids != self.baseline.state.line_ids
            || self.id != self.baseline.id
            || self.name != self.baseline.name
            || self.next_line_id != self.baseline.next_line_id
    }

    fn stored_document(&self) -> Result<StoredDocument, SessionError> {
        let name = self.name.clone().ok_or(SessionError::SaveAsRequired)?;
        Ok(StoredDocument::new(
            self.id,
            name,
            self.source.clone(),
            self.line_ids.clone(),
            self.next_line_id,
        ))
    }
}

#[derive(Clone, Copy)]
enum PendingAction {
    New,
    Open(DocumentId),
    Close,
}

impl PendingAction {
    fn public_kind(&self) -> PendingTransition {
        match self {
            Self::New => PendingTransition::New,
            Self::Open(id) => PendingTransition::Open(*id),
            Self::Close => PendingTransition::Close,
        }
    }
}

/// Source editing, evaluation caching, bounded undo/redo, and explicit
/// save/discard/cancel transitions for one active document.
pub struct CalculationPadSession {
    store: PadDocumentStore,
    current: Option<ActiveDocument>,
    pending: Option<PendingAction>,
    history: EditHistory,
}

impl CalculationPadSession {
    /// Creates an immediately usable blank session without touching the filesystem.
    pub fn new(root: &AppDataRoot) -> Self {
        Self::with_store(PadDocumentStore::new(root))
    }

    pub fn with_store(store: PadDocumentStore) -> Self {
        Self {
            store,
            current: Some(ActiveDocument::new_blank()),
            pending: None,
            history: EditHistory::default(),
        }
    }

    pub fn current_id(&self) -> Option<DocumentId> {
        self.current.as_ref().map(|document| document.id)
    }

    pub fn current_name(&self) -> Option<&str> {
        self.current
            .as_ref()
            .and_then(|document| document.name.as_deref())
    }

    pub fn source(&self) -> Option<&str> {
        self.current
            .as_ref()
            .map(|document| document.source.as_str())
    }

    pub fn line_ids(&self) -> Option<&[LineId]> {
        self.current
            .as_ref()
            .map(|document| document.line_ids.as_slice())
    }

    pub fn line_id(&self, line_index: usize) -> Option<LineId> {
        self.current
            .as_ref()
            .and_then(|document| document.line_ids.get(line_index).copied())
    }

    pub fn revision(&self) -> Option<u64> {
        self.current.as_ref().map(|document| document.revision)
    }

    pub fn is_dirty(&self) -> bool {
        self.current.as_ref().is_some_and(ActiveDocument::is_dirty)
    }

    pub fn is_saved(&self) -> bool {
        self.current
            .as_ref()
            .is_some_and(|document| document.baseline.disk_sha256.is_some())
    }

    pub fn can_undo(&self) -> bool {
        self.pending.is_none() && !self.history.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        self.pending.is_none() && !self.history.redo.is_empty()
    }

    pub fn pending_transition(&self) -> Option<PendingTransition> {
        self.pending.as_ref().map(PendingAction::public_kind)
    }

    pub fn list_saved_documents(&self) -> Result<Vec<PadDocumentSummary>, SessionError> {
        self.store.list().map_err(Into::into)
    }

    pub fn replace_source(&mut self, source: impl Into<String>) -> Result<bool, SessionError> {
        self.ensure_editable()?;
        let source = source.into();
        validate_source_size(&source)?;
        let current = self
            .current
            .as_ref()
            .ok_or(SessionError::NoActiveDocument)?;
        if current.source == source {
            return Ok(false);
        }
        let (line_ids, next_line_id) = remap_line_ids(
            &current.source,
            &current.line_ids,
            &source,
            current.next_line_id,
        )?;
        self.commit_source_edit(source, line_ids, next_line_id)
    }

    fn commit_source_edit(
        &mut self,
        source: String,
        line_ids: Vec<LineId>,
        next_line_id: u64,
    ) -> Result<bool, SessionError> {
        let current = self
            .current
            .as_ref()
            .ok_or(SessionError::NoActiveDocument)?;
        if current.source == source {
            return Ok(false);
        }
        let before = current.editable_state();
        let mut after = before.clone();
        after.source = source.clone();
        after.line_ids = line_ids.clone();
        let next_revision = current
            .revision
            .checked_add(1)
            .ok_or(SessionError::RevisionExhausted)?;

        self.history.record(before, after);
        let current = self
            .current
            .as_mut()
            .ok_or(SessionError::NoActiveDocument)?;
        current.source = source;
        current.line_ids = line_ids;
        current.next_line_id = next_line_id;
        current.revision = next_revision;
        Ok(true)
    }

    pub fn replace_line(
        &mut self,
        line_id: LineId,
        replacement: &str,
    ) -> Result<bool, SessionError> {
        self.ensure_editable()?;
        let current = self
            .current
            .as_ref()
            .ok_or(SessionError::NoActiveDocument)?;
        let line_index = current
            .line_ids
            .iter()
            .position(|candidate| *candidate == line_id)
            .ok_or(SessionError::LineIdNotFound(line_id))?;
        let mut document = CalculationDocument::new(current.source.clone());
        document.replace_line(line_index, replacement)?;
        let source = document.source().to_owned();
        validate_source_size(&source)?;
        if current.source == source {
            return Ok(false);
        }
        let line_ids = current.line_ids.clone();
        let next_line_id = current.next_line_id;
        self.commit_source_edit(source, line_ids, next_line_id)
    }

    pub fn undo(&mut self) -> bool {
        if self.pending.is_some() || self.history.undo.is_empty() {
            return false;
        }
        let Some(current) = self.current.as_ref() else {
            return false;
        };
        let Some(next_revision) = current.revision.checked_add(1) else {
            return false;
        };
        let Some(entry) = self.history.undo.pop_back() else {
            return false;
        };
        if let Some(current) = self.current.as_mut() {
            apply_editable_state(current, &entry.before);
            current.revision = next_revision;
        }
        self.history.redo.push_back(entry);
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.pending.is_some() || self.history.redo.is_empty() {
            return false;
        }
        let Some(current) = self.current.as_ref() else {
            return false;
        };
        let Some(next_revision) = current.revision.checked_add(1) else {
            return false;
        };
        let Some(entry) = self.history.redo.pop_back() else {
            return false;
        };
        if let Some(current) = self.current.as_mut() {
            apply_editable_state(current, &entry.after);
            current.revision = next_revision;
        }
        self.history.undo.push_back(entry);
        true
    }

    /// Returns owned line outcomes, recomputing only after a source revision changes.
    pub fn evaluation(&mut self) -> Option<&SessionEvaluation> {
        let current = self.current.as_mut()?;
        if current
            .cached_evaluation
            .as_ref()
            .is_none_or(|evaluation| evaluation.revision != current.revision)
        {
            let document = CalculationDocument::new(current.source.clone());
            let evaluation = document.evaluate();
            let lines = evaluation
                .lines
                .into_iter()
                .filter_map(|line| {
                    current
                        .line_ids
                        .get(line.line_index)
                        .copied()
                        .map(|id| SessionLineResult {
                            id,
                            line_index: line.line_index,
                            source_range: line.source_range,
                            assignment_name: line.assignment_name,
                            dependency_indices: line.dependency_indices,
                            outcome: line.outcome,
                        })
                })
                .collect();
            current.cached_evaluation = Some(SessionEvaluation {
                revision: current.revision,
                lines,
                diagnostic: evaluation.diagnostic,
            });
            #[cfg(test)]
            {
                current.evaluation_count += 1;
            }
        }
        current.cached_evaluation.as_ref()
    }

    pub fn cached_evaluation_revision(&self) -> Option<u64> {
        self.current
            .as_ref()
            .and_then(|document| document.cached_evaluation.as_ref())
            .map(|evaluation| evaluation.revision)
    }

    pub fn save(&mut self) -> Result<(), SessionError> {
        self.save_with_writer(crate::common::atomic_file::save_atomic)
    }

    fn save_with_writer(
        &mut self,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<(), SessionError> {
        let current = self
            .current
            .as_ref()
            .ok_or(SessionError::NoActiveDocument)?;
        if current.name.is_none() {
            return Err(SessionError::SaveAsRequired);
        }
        if !current.is_dirty() {
            return Ok(());
        }
        let stored = current.stored_document()?;
        let expected_disk_sha256 = current
            .baseline
            .disk_sha256
            .ok_or(SessionError::SaveAsRequired)?;
        let disk_sha256 =
            self.store
                .save_existing_with_writer(&stored, expected_disk_sha256, writer)?;
        let current = self
            .current
            .as_mut()
            .ok_or(SessionError::NoActiveDocument)?;
        current.baseline = DocumentBaseline {
            state: current.editable_state(),
            id: current.id,
            name: current.name.clone(),
            disk_sha256: Some(disk_sha256),
            next_line_id: current.next_line_id,
        };
        Ok(())
    }

    pub fn save_as(&mut self, name: &str) -> Result<(), SessionError> {
        let current = self
            .current
            .as_ref()
            .ok_or(SessionError::NoActiveDocument)?;
        let (stored, disk_sha256) = self.store.save_as(
            name,
            current.source.clone(),
            current.line_ids.clone(),
            current.next_line_id,
        )?;
        let current = self
            .current
            .as_mut()
            .ok_or(SessionError::NoActiveDocument)?;
        current.id = stored.id;
        current.name = Some(stored.name);
        current.baseline = DocumentBaseline {
            state: current.editable_state(),
            id: current.id,
            name: current.name.clone(),
            disk_sha256: Some(disk_sha256),
            next_line_id: current.next_line_id,
        };
        Ok(())
    }

    pub fn request_new(&mut self) -> Result<TransitionOutcome, SessionError> {
        self.ensure_no_pending()?;
        if self.is_dirty() {
            self.pending = Some(PendingAction::New);
            return Ok(TransitionOutcome::DecisionRequired);
        }
        self.install_new();
        Ok(TransitionOutcome::Applied)
    }

    /// The target is read and validated before the active draft is touched.
    pub fn request_open(&mut self, id: DocumentId) -> Result<TransitionOutcome, SessionError> {
        self.ensure_no_pending()?;
        let loaded = self.store.open(id)?;
        if self.is_dirty() {
            self.pending = Some(PendingAction::Open(id));
            return Ok(TransitionOutcome::DecisionRequired);
        }
        self.current = Some(ActiveDocument::from_loaded(loaded));
        self.history.clear();
        Ok(TransitionOutcome::Applied)
    }

    pub fn request_close(&mut self) -> Result<TransitionOutcome, SessionError> {
        self.ensure_no_pending()?;
        if self.current.is_none() {
            return Ok(TransitionOutcome::Applied);
        }
        if self.is_dirty() {
            self.pending = Some(PendingAction::Close);
            return Ok(TransitionOutcome::DecisionRequired);
        }
        self.current = None;
        self.history.clear();
        Ok(TransitionOutcome::Applied)
    }

    pub fn resolve_pending(
        &mut self,
        decision: TransitionDecision,
    ) -> Result<TransitionOutcome, SessionError> {
        if self.pending.is_none() {
            return Err(SessionError::NoPendingTransition);
        }
        match decision {
            TransitionDecision::Cancel => {
                self.pending = None;
                Ok(TransitionOutcome::Cancelled)
            }
            TransitionDecision::Save => {
                if self.is_dirty() {
                    self.save()?;
                }
                self.apply_pending()?;
                Ok(TransitionOutcome::Applied)
            }
            TransitionDecision::Discard => {
                self.apply_pending()?;
                Ok(TransitionOutcome::Applied)
            }
        }
    }

    fn apply_pending(&mut self) -> Result<(), SessionError> {
        let pending = self.pending.ok_or(SessionError::NoPendingTransition)?;
        let current = match pending {
            PendingAction::New => Some(ActiveDocument::new_blank()),
            PendingAction::Open(id) => Some(ActiveDocument::from_loaded(self.store.open(id)?)),
            PendingAction::Close => None,
        };
        self.current = current;
        self.pending = None;
        self.history.clear();
        Ok(())
    }

    fn install_new(&mut self) {
        self.current = Some(ActiveDocument::new_blank());
        self.history.clear();
    }

    fn ensure_editable(&self) -> Result<(), SessionError> {
        self.ensure_no_pending()?;
        if self.current.is_none() {
            return Err(SessionError::NoActiveDocument);
        }
        Ok(())
    }

    fn ensure_no_pending(&self) -> Result<(), SessionError> {
        if self.pending.is_some() {
            Err(SessionError::PendingTransitionExists)
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    fn evaluation_count(&self) -> usize {
        self.current
            .as_ref()
            .map_or(0, |document| document.evaluation_count)
    }
}

fn validate_source_size(source: &str) -> Result<(), SessionError> {
    if source.len() > MAX_DOCUMENT_BYTES {
        return Err(SessionError::DocumentTooLarge {
            max_bytes: MAX_DOCUMENT_BYTES,
        });
    }
    let line_count = source.split('\n').count();
    if line_count > MAX_SOURCE_LINES {
        return Err(SessionError::TooManyLines {
            max_lines: MAX_SOURCE_LINES,
        });
    }
    Ok(())
}

fn apply_editable_state(document: &mut ActiveDocument, state: &EditableState) {
    document.source.clone_from(&state.source);
    document.line_ids.clone_from(&state.line_ids);
}

fn remap_line_ids(
    old_source: &str,
    old_ids: &[LineId],
    new_source: &str,
    next_line_id: u64,
) -> Result<(Vec<LineId>, u64), SessionError> {
    // Keep the diff's memory bounded even if this helper gains another caller
    // besides `replace_source` in the future.
    validate_source_size(new_source)?;
    let old_lines = physical_lines(old_source);
    let new_lines = physical_lines(new_source);
    if old_ids.len() != old_lines.len() {
        return Err(SessionError::LineIdStateMismatch);
    }

    let old_count = old_lines.len();
    let new_count = new_lines.len();
    if old_count == new_count {
        // A whole-editor snapshot with at most one changed physical row is a
        // single-line edit. Keep IDs by position so duplicate text cannot
        // make the LCS matcher assign a neighboring row's identity instead.
        let mut changed_lines = 0;
        for (old_line, new_line) in old_lines.iter().zip(&new_lines) {
            if old_line != new_line {
                changed_lines += 1;
                if changed_lines > 1 {
                    break;
                }
            }
        }
        if changed_lines <= 1 {
            return Ok((old_ids.to_vec(), next_line_id));
        }
    }
    let width = new_count + 1;
    let mut lcs = vec![0usize; (old_count + 1) * width];
    for old_index in (0..old_count).rev() {
        for new_index in (0..new_count).rev() {
            let value = if old_lines[old_index] == new_lines[new_index] {
                1 + lcs[(old_index + 1) * width + new_index + 1]
            } else {
                lcs[(old_index + 1) * width + new_index].max(lcs[old_index * width + new_index + 1])
            };
            lcs[old_index * width + new_index] = value;
        }
    }

    let mut anchors = Vec::new();
    let (mut old_index, mut new_index) = (0, 0);
    while old_index < old_count && new_index < new_count {
        if old_lines[old_index] == new_lines[new_index] {
            anchors.push((old_index, new_index));
            old_index += 1;
            new_index += 1;
        } else if lcs[(old_index + 1) * width + new_index] >= lcs[old_index * width + new_index + 1]
        {
            old_index += 1;
        } else {
            new_index += 1;
        }
    }

    let mut mapped = vec![None; new_count];
    let (mut old_start, mut new_start) = (0, 0);
    for (matched_old, matched_new) in anchors {
        map_unmatched_segment(
            old_start..matched_old,
            new_start..matched_new,
            old_ids,
            &mut mapped,
        );
        mapped[matched_new] = Some(old_ids[matched_old]);
        old_start = matched_old + 1;
        new_start = matched_new + 1;
    }
    map_unmatched_segment(
        old_start..old_count,
        new_start..new_count,
        old_ids,
        &mut mapped,
    );

    let mut next_id = next_line_id;
    let line_ids = mapped
        .into_iter()
        .map(|id| match id {
            Some(id) => Ok(id),
            None => {
                let fresh = LineId(next_id);
                next_id = next_id
                    .checked_add(1)
                    .ok_or(SessionError::LineIdExhausted)?;
                Ok(fresh)
            }
        })
        .collect::<Result<Vec<_>, SessionError>>()?;
    Ok((line_ids, next_id))
}

fn map_unmatched_segment(
    old_range: std::ops::Range<usize>,
    new_range: std::ops::Range<usize>,
    old_ids: &[LineId],
    mapped: &mut [Option<LineId>],
) {
    for offset in 0..old_range.len().min(new_range.len()) {
        mapped[new_range.start + offset] = Some(old_ids[old_range.start + offset]);
    }
}

fn physical_lines(source: &str) -> Vec<&str> {
    source
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        CalculationPadSession, MAX_UNDO_ENTRIES, PendingTransition, SessionError,
        TransitionDecision, TransitionOutcome,
    };
    use crate::calculation_pad::{
        CalculationValue, DiagnosticKind, DocumentId, LineId, LineOutcome, MAX_DOCUMENT_BYTES,
        MAX_SOURCE_LINES, PadStoreError,
    };
    use crate::platform::app_data::AppDataRoot;
    use std::path::PathBuf;

    fn fixture() -> (tempfile::TempDir, AppDataRoot, CalculationPadSession) {
        let directory = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(directory.path().join("app-data"));
        let session = CalculationPadSession::new(&root);
        (directory, root, session)
    }

    fn scalar(session: &mut CalculationPadSession, line_index: usize) -> f64 {
        match &session.evaluation().unwrap().lines[line_index].outcome {
            LineOutcome::Value(CalculationValue::Scalar(value)) => *value,
            other => panic!("expected scalar result on line {line_index}, got {other:?}"),
        }
    }

    fn saved_path(session: &CalculationPadSession, id: DocumentId) -> PathBuf {
        session
            .store
            .directory()
            .join(format!("{:032x}.json", id.as_u128()))
    }

    #[test]
    fn blank_session_is_usable_without_creating_or_scanning_persistent_state() {
        let (_directory, _root, mut session) = fixture();

        assert_eq!(session.source(), Some(""));
        assert_eq!(session.line_ids().unwrap().len(), 1);
        assert!(!session.is_dirty());
        assert!(session.list_saved_documents().unwrap().is_empty());
        assert!(!session.store.directory().exists());
        assert!(session.evaluation().is_some());
        assert!(!session.store.directory().exists());
    }

    #[test]
    fn edits_keep_line_ids_stable_and_leave_physical_row_references_unchanged() {
        let (_directory, _root, mut session) = fixture();
        let source = "α = 2\nr2 + α\n{顧客 名} = α + 3";
        session.replace_source(source).unwrap();
        let original_ids = session.line_ids().unwrap().to_vec();

        session
            .replace_source("α = 2\ninserted = 9\nr2 + α\n{顧客 名} = α + 3")
            .unwrap();
        let inserted_ids = session.line_ids().unwrap().to_vec();
        assert_eq!(inserted_ids[0], original_ids[0]);
        assert_eq!(inserted_ids[2], original_ids[1]);
        assert_eq!(inserted_ids[3], original_ids[2]);
        assert!(!original_ids.contains(&inserted_ids[1]));
        assert!(session.source().unwrap().contains("r2 + α"));

        session
            .replace_line(original_ids[2], "{顧客 名} = α + 4")
            .unwrap();
        assert_eq!(session.line_id(3), Some(original_ids[2]));
        session
            .replace_source("α = 2\nr2 + α\n{顧客 名} = α + 4")
            .unwrap();
        assert_eq!(
            session.line_ids().unwrap(),
            &[original_ids[0], original_ids[1], original_ids[2]]
        );
        assert!(session.source().unwrap().contains("r2 + α"));
    }

    #[test]
    fn whole_source_single_line_edit_keeps_identity_when_it_duplicates_a_neighbor() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("A\nB").unwrap();
        let original_ids = session.line_ids().unwrap().to_vec();

        session.replace_source("B\nB").unwrap();

        assert_eq!(session.source(), Some("B\nB"));
        assert_eq!(session.line_ids(), Some(original_ids.as_slice()));
    }

    #[test]
    fn targeted_line_edits_keep_identity_when_contents_duplicate() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("A\nB").unwrap();
        let ids = session.line_ids().unwrap().to_vec();

        session.replace_line(ids[0], "B").unwrap();

        assert_eq!(session.source(), Some("B\nB"));
        assert_eq!(session.line_ids().unwrap().to_vec(), ids);
    }

    #[test]
    fn rejects_oversized_and_overlong_edits_before_line_diffing() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("existing = 1").unwrap();
        let original = session.source().unwrap().to_owned();

        assert!(matches!(
            session.replace_source("x".repeat(MAX_DOCUMENT_BYTES + 1)),
            Err(SessionError::DocumentTooLarge { .. })
        ));
        let too_many_lines = vec![""; MAX_SOURCE_LINES + 1].join("\n");
        assert!(matches!(
            session.replace_source(too_many_lines),
            Err(SessionError::TooManyLines { .. })
        ));
        assert!(matches!(
            super::remap_line_ids("", &[LineId(1)], &"x".repeat(MAX_DOCUMENT_BYTES + 1), 2),
            Err(SessionError::DocumentTooLarge { .. })
        ));
        let too_many_lines = vec![""; MAX_SOURCE_LINES + 1].join("\n");
        assert!(matches!(
            super::remap_line_ids("", &[LineId(1)], &too_many_lines, 2),
            Err(SessionError::TooManyLines { .. })
        ));
        assert_eq!(session.source(), Some(original.as_str()));
        assert_eq!(session.line_ids().unwrap().len(), 1);
    }

    #[test]
    fn evaluation_is_cached_until_the_source_revision_changes() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("x = 2\nx + 1").unwrap();
        let revision = session.revision().unwrap();

        assert_eq!(scalar(&mut session, 1), 3.0);
        assert_eq!(session.evaluation_count(), 1);
        assert_eq!(scalar(&mut session, 1), 3.0);
        assert_eq!(session.evaluation_count(), 1);
        assert_eq!(session.cached_evaluation_revision(), Some(revision));

        let assignment_id = session.line_id(0).unwrap();
        session.replace_line(assignment_id, "x = 4").unwrap();
        assert_eq!(session.revision(), Some(revision + 1));
        assert_eq!(scalar(&mut session, 1), 5.0);
        assert_eq!(session.evaluation_count(), 2);
    }

    #[test]
    fn undo_and_redo_recompute_dependent_lines_and_restore_stable_ids() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("x = 2\nx + 1").unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        assert_eq!(scalar(&mut session, 1), 3.0);

        session.replace_line(ids[0], "x = 4").unwrap();
        assert_eq!(scalar(&mut session, 1), 5.0);
        assert_eq!(session.line_ids().unwrap().to_vec(), ids);
        assert!(session.undo());
        assert_eq!(session.source(), Some("x = 2\nx + 1"));
        assert_eq!(session.line_ids().unwrap().to_vec(), ids);
        assert_eq!(scalar(&mut session, 1), 3.0);
        assert!(session.redo());
        assert_eq!(session.source(), Some("x = 4\nx + 1"));
        assert_eq!(session.line_ids().unwrap().to_vec(), ids);
        assert_eq!(scalar(&mut session, 1), 5.0);
    }

    #[test]
    fn undo_history_is_bounded() {
        let (_directory, _root, mut session) = fixture();
        for value in 0..(MAX_UNDO_ENTRIES + 12) {
            session.replace_source(format!("value = {value}")).unwrap();
        }

        let mut undo_entries = 0;
        while session.undo() {
            undo_entries += 1;
        }
        assert_eq!(undo_entries, MAX_UNDO_ENTRIES);
        assert!(!session.can_undo());

        let (_directory, _root, mut large_edits) = fixture();
        for value in 0..24 {
            large_edits
                .replace_source(format!("{value} {}", "x".repeat(MAX_DOCUMENT_BYTES - 16)))
                .unwrap();
        }
        assert!(large_edits.history.total_bytes() <= super::MAX_UNDO_BYTES);
        assert!(large_edits.history.undo.len() < MAX_UNDO_ENTRIES);
    }

    #[test]
    fn save_and_reopen_round_trip_unicode_source_and_line_ids() {
        let (_directory, root, mut session) = fixture();
        let source = "π = 2\n{顧客 名} = π + 1\n";
        session.replace_source(source).unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        session.save_as("計算 draft").unwrap();
        let id = session.current_id().unwrap();
        assert!(!session.is_dirty());

        let mut reopened = CalculationPadSession::new(&root);
        assert_eq!(
            reopened.request_open(id).unwrap(),
            TransitionOutcome::Applied
        );
        assert_eq!(reopened.source(), Some(source));
        assert_eq!(reopened.line_ids().unwrap().to_vec(), ids);
        assert_eq!(reopened.current_name(), Some("計算 draft"));
    }

    #[test]
    fn saving_an_untitled_blank_document_requires_save_as() {
        let (_directory, _root, mut session) = fixture();
        assert_eq!(session.source(), Some(""));
        assert_eq!(session.current_name(), None);

        assert!(matches!(session.save(), Err(SessionError::SaveAsRequired)));
        assert!(session.list_saved_documents().unwrap().is_empty());
    }

    #[test]
    fn save_as_identity_is_not_reverted_by_undo_and_redo() {
        let (_directory, root, mut session) = fixture();
        session.replace_source("x = 1").unwrap();
        session.save_as("undo identity").unwrap();
        let id = session.current_id().unwrap();
        let line_id = session.line_id(0).unwrap();

        session.replace_line(line_id, "x = 2").unwrap();
        assert!(session.undo());
        assert_eq!(session.source(), Some("x = 1"));
        assert_eq!(session.current_id(), Some(id));
        assert_eq!(session.current_name(), Some("undo identity"));
        assert!(!session.is_dirty());
        assert!(session.redo());
        assert_eq!(session.current_id(), Some(id));
        assert_eq!(session.current_name(), Some("undo identity"));
        session.save().unwrap();

        let mut reopened = CalculationPadSession::new(&root);
        reopened.request_open(id).unwrap();
        assert_eq!(reopened.source(), Some("x = 2"));
    }

    #[test]
    fn persisted_line_id_allocator_does_not_reuse_removed_identities() {
        let (_directory, root, mut session) = fixture();
        session.save_as("line identity counter").unwrap();
        let saved_id = session.current_id().unwrap();

        session.replace_source("first\nsecond").unwrap();
        let removed_id = session.line_id(1).unwrap();
        session.replace_source("").unwrap();
        assert!(session.is_dirty());
        session.replace_source("first\nsecond").unwrap();
        let replacement_id = session.line_id(1).unwrap();
        assert_ne!(replacement_id, removed_id);
        assert!(replacement_id.get() > removed_id.get());
        session.save().unwrap();

        let mut reopened = CalculationPadSession::new(&root);
        reopened.request_open(saved_id).unwrap();
        reopened.replace_source("first\ninserted\nsecond").unwrap();
        assert_eq!(reopened.line_id(1).unwrap().get(), replacement_id.get() + 1);
    }

    #[test]
    fn failed_open_preserves_the_draft_and_open_transition_can_cancel_or_discard() {
        let (_directory, root, mut owner) = fixture();
        owner.replace_source("target = 7\ntarget * 2").unwrap();
        owner.save_as("target").unwrap();
        let target_id = owner.current_id().unwrap();
        let target_path = saved_path(&owner, target_id);
        let target_bytes = std::fs::read(&target_path).unwrap();

        let mut draft = CalculationPadSession::new(&root);
        draft.replace_source("local draft = 9").unwrap();
        let draft_source = draft.source().unwrap().to_owned();
        std::fs::write(&target_path, b"not valid JSON").unwrap();
        assert!(matches!(
            draft.request_open(target_id),
            Err(SessionError::Store(PadStoreError::MalformedJson { .. }))
        ));
        assert_eq!(draft.source(), Some(draft_source.as_str()));
        assert_eq!(draft.pending_transition(), None);

        std::fs::write(&target_path, target_bytes).unwrap();
        assert_eq!(
            draft.request_open(target_id).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(
            draft.pending_transition(),
            Some(PendingTransition::Open(target_id))
        );
        assert_eq!(
            draft.resolve_pending(TransitionDecision::Cancel).unwrap(),
            TransitionOutcome::Cancelled
        );
        assert_eq!(draft.source(), Some(draft_source.as_str()));

        assert_eq!(
            draft.request_open(target_id).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert!(matches!(
            draft.resolve_pending(TransitionDecision::Save),
            Err(SessionError::SaveAsRequired)
        ));
        assert_eq!(
            draft.pending_transition(),
            Some(PendingTransition::Open(target_id))
        );
        draft.save_as("saved draft").unwrap();
        assert_eq!(
            draft.resolve_pending(TransitionDecision::Save).unwrap(),
            TransitionOutcome::Applied
        );
        assert_eq!(draft.source(), Some("target = 7\ntarget * 2"));
        assert_eq!(draft.current_id(), Some(target_id));

        draft.replace_source("local target edit").unwrap();
        assert_eq!(
            draft.request_open(target_id).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(
            draft.resolve_pending(TransitionDecision::Discard).unwrap(),
            TransitionOutcome::Applied
        );
        assert_eq!(draft.source(), Some("target = 7\ntarget * 2"));
    }

    #[test]
    fn opening_the_same_saved_document_after_save_reloads_its_latest_source() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("answer = 1").unwrap();
        session.save_as("same document").unwrap();
        let id = session.current_id().unwrap();

        let answer_line = session.line_id(0).unwrap();
        session.replace_line(answer_line, "answer = 2").unwrap();
        assert_eq!(
            session.request_open(id).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(
            session.pending_transition(),
            Some(PendingTransition::Open(id))
        );
        assert_eq!(
            session.resolve_pending(TransitionDecision::Save).unwrap(),
            TransitionOutcome::Applied
        );

        assert_eq!(session.current_id(), Some(id));
        assert_eq!(session.source(), Some("answer = 2"));
        assert!(!session.is_dirty());
        let loaded = session.store.open(id).unwrap();
        assert_eq!(loaded.stored.source, "answer = 2");
        assert_eq!(
            session.current.as_ref().unwrap().baseline.disk_sha256,
            Some(loaded.disk_sha256)
        );
    }

    #[test]
    fn pending_open_reloads_after_save_and_preserves_session_on_target_error() {
        let (_directory, root, mut current) = fixture();
        current.replace_source("current = 1").unwrap();
        current.save_as("current").unwrap();
        let current_id = current.current_id().unwrap();

        let mut target = CalculationPadSession::new(&root);
        target.replace_source("target = 2").unwrap();
        target.save_as("target").unwrap();
        let target_id = target.current_id().unwrap();
        let target_path = saved_path(&current, target_id);
        let valid_target_bytes = std::fs::read(&target_path).unwrap();

        let current_line = current.line_id(0).unwrap();
        current.replace_line(current_line, "current = 3").unwrap();
        assert_eq!(
            current.request_open(target_id).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        std::fs::write(&target_path, b"invalid after the prompt").unwrap();

        assert!(matches!(
            current.resolve_pending(TransitionDecision::Save),
            Err(SessionError::Store(PadStoreError::MalformedJson { .. }))
        ));
        assert_eq!(current.current_id(), Some(current_id));
        assert_eq!(current.current_name(), Some("current"));
        assert_eq!(current.source(), Some("current = 3"));
        assert!(!current.is_dirty());
        assert_eq!(
            current.pending_transition(),
            Some(PendingTransition::Open(target_id))
        );
        assert_eq!(
            current.store.open(current_id).unwrap().stored.source,
            "current = 3"
        );

        std::fs::write(&target_path, valid_target_bytes).unwrap();
        assert_eq!(
            current
                .resolve_pending(TransitionDecision::Discard)
                .unwrap(),
            TransitionOutcome::Applied
        );
        assert_eq!(current.current_id(), Some(target_id));
        assert_eq!(current.source(), Some("target = 2"));
    }

    #[test]
    fn new_and_close_transitions_handle_save_discard_and_cancel() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("unsaved = 5").unwrap();
        assert_eq!(
            session.request_new().unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert!(matches!(
            session.resolve_pending(TransitionDecision::Save),
            Err(SessionError::SaveAsRequired)
        ));
        assert_eq!(session.pending_transition(), Some(PendingTransition::New));
        session.save_as("kept on Save").unwrap();
        assert_eq!(
            session.resolve_pending(TransitionDecision::Save).unwrap(),
            TransitionOutcome::Applied
        );
        assert_eq!(session.source(), Some(""));
        assert_eq!(
            session.list_saved_documents().unwrap()[0].name,
            "kept on Save"
        );

        session.replace_source("discard me").unwrap();
        assert_eq!(
            session.request_close().unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(
            session.resolve_pending(TransitionDecision::Cancel).unwrap(),
            TransitionOutcome::Cancelled
        );
        assert_eq!(session.source(), Some("discard me"));
        assert_eq!(
            session.request_close().unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(
            session
                .resolve_pending(TransitionDecision::Discard)
                .unwrap(),
            TransitionOutcome::Applied
        );
        assert_eq!(session.source(), None);

        session.request_new().unwrap();
        session.replace_source("save on close").unwrap();
        session.save_as("close-save").unwrap();
        session.replace_source("saved close").unwrap();
        let close_id = session.current_id().unwrap();
        assert_eq!(
            session.request_close().unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(
            session.resolve_pending(TransitionDecision::Save).unwrap(),
            TransitionOutcome::Applied
        );
        assert_eq!(session.source(), None);
        assert_eq!(
            session.store.open(close_id).unwrap().stored.source,
            "saved close"
        );
        assert_eq!(
            session
                .list_saved_documents()
                .unwrap()
                .iter()
                .find(|document| document.id == close_id)
                .unwrap()
                .source_bytes,
            "saved close".len()
        );
    }

    #[test]
    fn external_changes_and_deletion_keep_the_local_draft_dirty() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("baseline").unwrap();
        session.save_as("external-check").unwrap();
        session.replace_source("local draft").unwrap();
        let id = session.current_id().unwrap();
        let path = saved_path(&session, id);
        let baseline = session.current.as_ref().unwrap().baseline.disk_sha256;
        let name = session.current_name().unwrap().to_owned();

        let mut external = std::fs::read(&path).unwrap();
        external.push(b' ');
        std::fs::write(&path, external).unwrap();
        assert_eq!(
            session.request_close().unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert!(matches!(
            session.resolve_pending(TransitionDecision::Save),
            Err(SessionError::Store(PadStoreError::DiskConflict(found))) if found == id
        ));
        assert_eq!(session.pending_transition(), Some(PendingTransition::Close));
        assert_eq!(session.source(), Some("local draft"));
        assert_eq!(session.current_name(), Some(name.as_str()));
        assert_eq!(
            session.current.as_ref().unwrap().baseline.disk_sha256,
            baseline
        );
        assert!(session.is_dirty());

        std::fs::remove_file(path).unwrap();
        assert!(matches!(
            session.resolve_pending(TransitionDecision::Save),
            Err(SessionError::Store(PadStoreError::DiskConflict(found))) if found == id
        ));
        assert_eq!(session.pending_transition(), Some(PendingTransition::Close));
        assert_eq!(session.source(), Some("local draft"));
        assert!(session.is_dirty());
    }

    #[test]
    fn failed_atomic_write_keeps_file_baseline_and_dirty_draft_unchanged() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("before").unwrap();
        session.save_as("write-failure").unwrap();
        session.replace_source("after").unwrap();
        let id = session.current_id().unwrap();
        let path = saved_path(&session, id);
        let original_bytes = std::fs::read(&path).unwrap();
        let baseline = session.current.as_ref().unwrap().baseline.disk_sha256;
        let name = session.current_name().unwrap().to_owned();

        assert!(matches!(
            session.save_with_writer(|_, _| anyhow::bail!("injected write failure")),
            Err(SessionError::Store(PadStoreError::AtomicWrite { .. }))
        ));
        assert_eq!(std::fs::read(path).unwrap(), original_bytes);
        assert_eq!(session.current_id(), Some(id));
        assert_eq!(session.current_name(), Some(name.as_str()));
        assert_eq!(session.source(), Some("after"));
        assert_eq!(
            session.current.as_ref().unwrap().baseline.disk_sha256,
            baseline
        );
        assert!(session.is_dirty());
    }

    #[test]
    fn failed_or_invalid_rows_remain_owned_by_the_source_order() {
        let (_directory, _root, mut session) = fixture();
        session
            .replace_source("good = 2\nbad = missing\ngood + 1")
            .unwrap();
        let first_line_id = session.line_id(0).unwrap();
        let evaluation = session.evaluation().unwrap();
        assert_eq!(evaluation.lines.len(), 3);
        assert_eq!(
            evaluation
                .lines
                .iter()
                .map(|line| line.line_index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(evaluation.lines[0].id, first_line_id);
        assert!(matches!(
            &evaluation.lines[1].outcome,
            LineOutcome::Error(diagnostic) if matches!(&diagnostic.kind, DiagnosticKind::UnknownName(_))
        ));
    }
}
