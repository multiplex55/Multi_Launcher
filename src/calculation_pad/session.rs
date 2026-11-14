//! Editable Calculation Pad session with bounded history and dirty handling.

use super::document::{
    CalculationDocument, CalculationValue, DocumentDiagnostic, EditError, FinalBinding, LineOutcome,
    MAX_DOCUMENT_BYTES, MAX_SOURCE_LINES,
};
use super::inspector::{
    DocumentIncarnation, InspectorInsertRequest, InspectorSourceOrigin, InspectorSourceSnapshot,
};
use super::persistence::{
    DocumentId, LineId, LoadedDocument, PadDocumentStore, PadDocumentSummary, PadStoreError,
    StoredDocument,
};
use super::{IntegerMode, IntegerWidth, TypedInteger};
use crate::platform::app_data::AppDataRoot;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;

const MAX_UNDO_ENTRIES: usize = 64;
const MAX_UNDO_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct SessionLineResult {
    pub id: LineId,
    pub line_index: usize,
    pub source_range: std::ops::Range<usize>,
    pub assignment_name: Option<String>,
    pub dependency_indices: Vec<usize>,
    pub loop_iteration: Option<Vec<usize>>,
    pub outcome: LineOutcome,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionEvaluation {
    pub revision: u64,
    pub lines: Vec<SessionLineResult>,
    /// Physical rows holding the final bindings in this evaluation.
    pub(crate) final_bindings: Arc<HashMap<String, FinalBinding>>,
    pub diagnostic: Option<DocumentDiagnostic>,
}

#[derive(Debug)]
pub enum InspectorInsertError {
    PendingTransition,
    StaleSource,
    InvalidTarget,
    Edit(SessionError),
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
    RecoveryScratchRequired,
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
            Self::RecoveryScratchRequired => {
                formatter.write_str("recovery close requires an unsaved scratch document")
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
    incarnation: DocumentIncarnation,
    name: Option<String>,
    source: String,
    line_ids: Vec<LineId>,
    next_line_id: u64,
    revision: u64,
    baseline: DocumentBaseline,
    cached_evaluation: Option<SessionEvaluation>,
    dependency_snapshot: Option<Arc<super::dependencies::DependencySnapshot>>,
    #[cfg(test)]
    dependency_build_count: usize,
    #[cfg(test)]
    evaluation_count: usize,
}

impl ActiveDocument {
    fn new_blank() -> Self {
        let id = DocumentId::fresh();
        let mut document = Self {
            id,
            incarnation: DocumentIncarnation(DocumentId::fresh()),
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
            dependency_snapshot: None,
            #[cfg(test)]
            dependency_build_count: 0,
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
            incarnation: DocumentIncarnation(DocumentId::fresh()),
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
            dependency_snapshot: None,
            #[cfg(test)]
            dependency_build_count: 0,
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

    /// Identity of this installed document, including same-file reopenings.
    pub fn document_incarnation(&self) -> Option<DocumentIncarnation> {
        self.current.as_ref().map(|document| document.incarnation)
    }

    /// Freeze the already-current cached physical rows without reevaluating the Pad.
    pub fn comparison_context(&self) -> Option<super::comparison::ComparisonContext> {
        let document = self.current.as_ref()?;
        let evaluation = document.cached_evaluation.as_ref()?;
        if evaluation.revision != document.revision || evaluation.diagnostic.is_some() {
            return None;
        }
        let rows = evaluation
            .lines
            .iter()
            .map(|line| super::comparison::ComparisonRow {
                id: line.id,
                assignment_name: line.assignment_name.clone(),
                outcome: line.outcome.clone(),
            })
            .collect::<Vec<_>>();
        Some(super::comparison::ComparisonContext {
            incarnation: document.incarnation,
            document_id: document.id,
            revision: document.revision,
            rows: rows.into(),
            final_bindings: Arc::clone(&evaluation.final_bindings),
        })
    }

    /// Read the current dependency graph without triggering Pad evaluation.
    pub fn dependency_snapshot(&mut self) -> Option<Arc<super::DependencySnapshot>> {
        let current = self.current.as_mut()?;
        let evaluation = current.cached_evaluation.as_ref()?;
        if evaluation.revision != current.revision {
            return None;
        }
        if let Some(snapshot) = current.dependency_snapshot.as_ref()
            && snapshot.document_id == current.id
            && snapshot.incarnation == current.incarnation
            && snapshot.revision == current.revision
        {
            return Some(Arc::clone(snapshot));
        }
        let snapshot = Arc::new(super::DependencySnapshot::from_evaluation(
            current.id,
            current.incarnation,
            &current.source,
            evaluation,
        )?);
        current.dependency_snapshot = Some(Arc::clone(&snapshot));
        #[cfg(test)]
        {
            current.dependency_build_count += 1;
        }
        Some(snapshot)
    }

    /// Resolve a stable row against the currently cached evaluation, never reevaluating.
    pub fn resolve_dependency_line(
        &self,
        document_id: DocumentId,
        incarnation: DocumentIncarnation,
        id: LineId,
    ) -> Option<super::DependencyLocation> {
        let current = self.current.as_ref()?;
        if current.id != document_id || current.incarnation != incarnation {
            return None;
        }
        let evaluation = current.cached_evaluation.as_ref()?;
        if evaluation.revision != current.revision {
            return None;
        }
        let row = evaluation.lines.iter().find(|row| row.id == id)?;
        current.source.get(row.source_range.clone())?;
        Some(super::DependencyLocation {
            line_index: row.line_index,
            source_range: row.source_range.clone(),
        })
    }

    /// Resolve only an already-current cached result; never evaluate as an observation.
    pub fn inspector_source(&self, line_id: LineId) -> Option<InspectorSourceSnapshot> {
        let current = self.current.as_ref()?;
        let evaluation = current.cached_evaluation.as_ref()?;
        if evaluation.revision != current.revision {
            return None;
        }
        let line = evaluation.lines.iter().find(|line| line.id == line_id)?;
        let (value, origin) = match &line.outcome {
            LineOutcome::Value(CalculationValue::Integer(value)) => {
                (*value, InspectorSourceOrigin::TypedInteger)
            }
            LineOutcome::Value(CalculationValue::ExactBase { value, .. }) => {
                let width = if *value <= u64::MAX as u128 {
                    IntegerWidth::Bits64
                } else {
                    IntegerWidth::Bits128
                };
                (
                    TypedInteger::from_unsigned(*value, IntegerMode::unsigned(width)).ok()?,
                    InspectorSourceOrigin::LegacyExactBase,
                )
            }
            _ => return None,
        };
        Some(InspectorSourceSnapshot {
            incarnation: current.incarnation,
            document_id: current.id,
            revision: current.revision,
            line_id,
            source_range: line.source_range.clone(),
            source_text: std::sync::Arc::from(current.source.get(line.source_range.clone())?),
            value,
            origin,
        })
    }

    /// Apply one canonical replacement only while its original source binding remains current.
    pub fn apply_inspector_insert(
        &mut self,
        request: InspectorInsertRequest,
    ) -> Result<bool, InspectorInsertError> {
        if self.pending.is_some() {
            return Err(InspectorInsertError::PendingTransition);
        }
        if self.inspector_source(request.source.line_id).as_ref() != Some(&request.source) {
            return Err(InspectorInsertError::StaleSource);
        }
        let current = self
            .current
            .as_ref()
            .ok_or(InspectorInsertError::StaleSource)?;
        if request.target.start > request.target.end
            || current.source.get(request.target.clone()).is_none()
        {
            return Err(InspectorInsertError::InvalidTarget);
        }
        let mut replacement = current.source.clone();
        replacement.replace_range(request.target, &request.expression);
        self.replace_source(replacement)
            .map_err(InspectorInsertError::Edit)
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

    /// Restores an unnamed recovery draft over the session's initial blank
    /// document. Recovery starts with fresh line identities, no undo history,
    /// and the blank source as its saved baseline so restored text stays dirty.
    pub(crate) fn restore_recovered_source(
        &mut self,
        source: String,
    ) -> Result<bool, SessionError> {
        self.ensure_editable()?;
        validate_source_size(&source)?;
        let current = self
            .current
            .as_ref()
            .ok_or(SessionError::NoActiveDocument)?;
        if current.name.is_some()
            || current.is_dirty()
            || !current.source.is_empty()
            || current.revision != 0
        {
            return Ok(false);
        }
        if source.is_empty() {
            self.history.clear();
            return Ok(true);
        }

        let (line_ids, next_line_id) = remap_line_ids(
            &current.source,
            &current.line_ids,
            &source,
            current.next_line_id,
        )?;
        let next_revision = current
            .revision
            .checked_add(1)
            .ok_or(SessionError::RevisionExhausted)?;
        let current = self
            .current
            .as_mut()
            .ok_or(SessionError::NoActiveDocument)?;
        current.source = source;
        current.line_ids = line_ids;
        current.next_line_id = next_line_id;
        current.revision = next_revision;
        current.cached_evaluation = None;
        self.history.clear();
        Ok(true)
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
                            loop_iteration: line.loop_iteration,
                            outcome: line.outcome,
                        })
                })
                .collect();
            current.cached_evaluation = Some(SessionEvaluation {
                revision: current.revision,
                lines,
                final_bindings: Arc::new(evaluation.final_bindings),
                diagnostic: evaluation.diagnostic,
            });
            #[cfg(test)]
            {
                current.evaluation_count += 1;
            }
        }
        current.cached_evaluation.as_ref()
    }

    /// Returns the source and its evaluation from the same current revision.
    pub(crate) fn evaluation_with_source(&mut self) -> Option<(&str, &SessionEvaluation)> {
        self.evaluation()?;
        let current = self.current.as_ref()?;
        Some((&current.source, current.cached_evaluation.as_ref()?))
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

    /// Retire an unbound scratch only after its recovery snapshot was saved.
    pub(crate) fn close_recovered_scratch(&mut self) -> Result<(), SessionError> {
        self.ensure_no_pending()?;
        if self.current.is_none() {
            return Err(SessionError::NoActiveDocument);
        }
        if self.is_saved() {
            return Err(SessionError::RecoveryScratchRequired);
        }
        self.current = None;
        self.history.clear();
        Ok(())
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
    pub(crate) fn evaluation_count(&self) -> usize {
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

    #[test]
    fn inspector_observes_exact_values_and_preserves_dirty_binding() {
        use crate::calculation_pad::{InspectorSourceOrigin, ProgrammerInspector};
        let (_dir, _root, mut session) = fixture();
        session
            .replace_source("123 >> 12\n9007199254740993 dec to hex\n340282366920938463463374607431768211455 dec to hex")
            .unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        assert!(session.inspector_source(ids[0]).is_none());
        session.evaluation();
        let mut inspector = ProgrammerInspector::default();
        inspector.observe(session.inspector_source(ids[0]));
        assert_eq!(inspector.value().formatted_decimal(), "0");
        inspector.toggle_bit(0).unwrap();
        inspector.observe(session.inspector_source(ids[1]));
        assert_eq!(inspector.adopted().unwrap().line_id, ids[0]);
        assert_eq!(
            inspector.candidate().unwrap().origin,
            InspectorSourceOrigin::LegacyExactBase
        );
        inspector.refresh().unwrap();
        assert_eq!(inspector.value().formatted_decimal(), "9007199254740993");
        inspector.observe(session.inspector_source(ids[2]));
        assert_eq!(inspector.value().formatted_decimal(), u128::MAX.to_string());
        assert_eq!(inspector.value().mode().type_name(), "u128");
        assert_eq!(session.evaluation_count(), 1);
    }

    #[test]
    fn inspector_insert_checks_binding_and_is_one_undo_step() {
        use crate::calculation_pad::{InspectorInsertError, ProgrammerInspector};
        let (_dir, _root, mut session) = fixture();
        session.replace_source("i8(-1)\nα").unwrap();
        session.save_as("inspector edit atomicity").unwrap();
        assert!(!session.is_dirty());
        let id = session.line_id(0).unwrap();
        session.evaluation();
        let mut inspector = ProgrammerInspector::default();
        inspector.observe(session.inspector_source(id));
        let fingerprint = |session: &CalculationPadSession| {
            (
                session.source().map(str::to_owned),
                session.line_ids().map(<[LineId]>::to_vec),
                session.revision(),
                session.is_dirty(),
                session.history.undo.len(),
                session.history.redo.len(),
                session.cached_evaluation_revision(),
                session.evaluation_count(),
                session
                    .current
                    .as_ref()
                    .and_then(|document| document.cached_evaluation.clone()),
            )
        };
        let unchanged = fingerprint(&session);
        let no_op = inspector.insert_request(0..6).unwrap();
        assert!(!session.apply_inspector_insert(no_op).unwrap());
        assert_eq!(fingerprint(&session), unchanged);
        inspector.toggle_bit(0).unwrap();
        let bad = inspector.insert_request(8..9).unwrap();
        let before = session.source().unwrap().to_owned();
        assert!(matches!(
            session.apply_inspector_insert(bad),
            Err(InspectorInsertError::InvalidTarget)
        ));
        assert_eq!(session.source(), Some(before.as_str()));
        assert_eq!(fingerprint(&session), unchanged);
        let undo_entries = session.history.undo.len();
        let stale = inspector.insert_request(0..6).unwrap();
        let request = inspector.insert_request(0..6).unwrap();
        assert!(session.apply_inspector_insert(request).unwrap());
        assert_eq!(session.history.undo.len(), undo_entries + 1);
        assert_eq!(session.source(), Some("i8(-2)\nα"));
        assert!(session.undo());
        assert_eq!(session.source(), Some(before.as_str()));
        assert!(session.redo());
        assert_eq!(session.source(), Some("i8(-2)\nα"));
        assert!(matches!(
            session.apply_inspector_insert(stale),
            Err(InspectorInsertError::StaleSource)
        ));
    }

    #[test]
    fn inspector_rejects_pending_and_reopened_document_identity() {
        use crate::calculation_pad::{InspectorInsertError, ProgrammerInspector};
        let (_dir, _root, mut session) = fixture();
        session.replace_source("u64(7)").unwrap();
        session.save_as("inspector reopen").unwrap();
        let id = session.current_id().unwrap();
        assert_eq!(
            session.request_open(id).unwrap(),
            TransitionOutcome::Applied
        );
        let line = session.line_id(0).unwrap();
        session.evaluation();
        let mut inspector = ProgrammerInspector::default();
        inspector.observe(session.inspector_source(line));
        let original = inspector.adopted().unwrap().clone();
        let request = inspector.insert_request(0..6).unwrap();
        session.replace_source("u64(8)").unwrap();
        assert_eq!(
            session.request_open(id).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert!(matches!(
            session.apply_inspector_insert(request.clone()),
            Err(InspectorInsertError::PendingTransition)
        ));
        assert_eq!(session.source(), Some("u64(8)"));
        session
            .resolve_pending(TransitionDecision::Discard)
            .unwrap();
        session.evaluation();
        let reopened = session.inspector_source(line).unwrap();
        assert_eq!(original.document_id, reopened.document_id);
        assert_eq!(original.revision, 0);
        assert_eq!(original.revision, reopened.revision);
        assert_ne!(original.incarnation, reopened.incarnation);
        let mut colliding_saved_binding = reopened.clone();
        colliding_saved_binding.incarnation = original.incarnation;
        assert_eq!(original, colliding_saved_binding);
        assert!(matches!(
            session.apply_inspector_insert(request),
            Err(InspectorInsertError::StaleSource)
        ));
    }

    #[test]
    fn inspector_refresh_and_canonical_roundtrip_preserve_pad_until_insert() {
        use crate::calculation_pad::{
            IntegerMode, IntegerWidth, ProgrammerInspector, TypedInteger,
        };
        let (_dir, _root, mut session) = fixture();
        session
            .replace_source("u64(9007199254740993)\nu8(1)")
            .unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        session.evaluation();
        let revision = session.revision();
        let history = session.can_undo();
        let mut inspector = ProgrammerInspector::default();
        inspector.observe(session.inspector_source(ids[0]));
        inspector.toggle_bit(0).unwrap();
        inspector.toggle_bit(0).unwrap();
        assert!(!inspector.numeric_dirty());
        inspector.set_annotation(0..1, "flag".into()).unwrap();
        inspector.observe(session.inspector_source(ids[1]));
        assert_eq!(inspector.adopted().unwrap().line_id, ids[0]);
        inspector.observe(None);
        assert!(inspector.refresh().is_err());
        assert_eq!(inspector.adopted().unwrap().line_id, ids[0]);
        inspector.observe(session.inspector_source(ids[1]));
        inspector.refresh().unwrap();
        assert_eq!(inspector.adopted().unwrap().line_id, ids[1]);
        assert_eq!(inspector.annotations()[0].label, "flag");
        assert_eq!(session.revision(), revision);
        assert_eq!(session.evaluation_count(), 1);
        assert_eq!(session.can_undo(), history);
        for value in [
            TypedInteger::from_signed(-1, IntegerMode::signed(IntegerWidth::Bits8)).unwrap(),
            TypedInteger::from_signed(i128::MIN, IntegerMode::signed(IntegerWidth::Bits128))
                .unwrap(),
            TypedInteger::from_unsigned(
                9_007_199_254_740_993,
                IntegerMode::unsigned(IntegerWidth::Bits64),
            )
            .unwrap(),
            TypedInteger::from_unsigned(u128::MAX, IntegerMode::unsigned(IntegerWidth::Bits128))
                .unwrap(),
        ] {
            inspector.set_value(value);
            let expression = inspector.canonical_expression();
            let document = crate::calculation_pad::CalculationDocument::new(expression);
            assert!(
                matches!(&document.evaluate().lines[0].outcome, LineOutcome::Value(CalculationValue::Integer(actual)) if *actual == value)
            );
        }
    }

    #[test]
    fn inspector_dirty_work_survives_changed_or_missing_candidate() {
        use crate::calculation_pad::ProgrammerInspector;
        let (_dir, _root, mut session) = fixture();
        session.replace_source("x = u8(1)\nx + 1").unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        session.evaluation();
        let mut inspector = ProgrammerInspector::default();
        inspector.observe(session.inspector_source(ids[1]));
        let adopted = inspector.adopted().unwrap().clone();
        inspector.toggle_bit(0).unwrap();
        let working = inspector.value();
        session.replace_line(ids[0], "x = u8(2)").unwrap();
        assert!(session.inspector_source(ids[1]).is_none());
        session.evaluation();
        inspector.observe(session.inspector_source(ids[1]));
        assert_eq!(inspector.adopted(), Some(&adopted));
        assert_eq!(inspector.value(), working);
        assert_ne!(inspector.candidate(), inspector.adopted());
        session.replace_line(ids[1], "1.5").unwrap();
        session.evaluation();
        inspector.observe(session.inspector_source(ids[1]));
        assert!(inspector.candidate().is_none());
        assert!(inspector.refresh().is_err());
        assert_eq!(inspector.value(), working);
        assert_eq!(inspector.adopted(), Some(&adopted));
        session.replace_source("u8(0)").unwrap();
        session.evaluation();
        inspector.observe(session.inspector_source(ids[1]));
        assert!(inspector.candidate().is_none());
        assert_eq!(inspector.adopted(), Some(&adopted));
    }

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

    fn integer(
        session: &mut CalculationPadSession,
        line_index: usize,
    ) -> crate::calculation_pad::TypedInteger {
        match &session.evaluation().unwrap().lines[line_index].outcome {
            LineOutcome::Value(CalculationValue::Integer(value)) => *value,
            other => panic!("expected an integer result on line {line_index}, got {other:?}"),
        }
    }

    fn formatted(session: &mut CalculationPadSession, line_index: usize) -> String {
        match &session.evaluation().unwrap().lines[line_index].outcome {
            LineOutcome::Value(value) => value
                .formatted_result()
                .expect("numeric result should have a display form"),
            other => panic!("expected a formatted result on line {line_index}, got {other:?}"),
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
    fn dependency_snapshot_uses_cached_physical_rows_and_reuses_one_build() {
        use std::sync::Arc;
        let (_directory, _root, mut session) = fixture();
        let source = "# comment\n\nα = 2\n{顧客 名} = α + 1\nr3 + {顧客 名}\nα * 2\n[1, 2]\nlater + 1\nlater = 2\nself = self + 1\na = b\nb = a\nx = 1\nx = 2\nz = x + 1";
        session.replace_source(source).unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        let id = session.current_id().unwrap();
        let incarnation = session.document_incarnation().unwrap();
        assert!(session.dependency_snapshot().is_none());
        assert!(
            session
                .resolve_dependency_line(id, incarnation, ids[2])
                .is_none()
        );
        assert_eq!(session.evaluation_count(), 0);
        let evaluation = session.evaluation().unwrap().clone();
        let before = (
            session.source().unwrap().to_owned(),
            session.revision(),
            session.line_ids().unwrap().to_vec(),
            session.is_dirty(),
            session.can_undo(),
            session.can_redo(),
            session.evaluation_count(),
        );
        let graph = session.dependency_snapshot().unwrap();
        assert!(Arc::ptr_eq(&graph, &session.dependency_snapshot().unwrap()));
        assert_eq!(session.current.as_ref().unwrap().dependency_build_count, 1);
        assert_eq!(graph.rows.len(), ids.len());
        assert_eq!(graph.rows[0].outcome, LineOutcome::Comment);
        assert_eq!(graph.rows[1].outcome, LineOutcome::Blank);
        assert_eq!(graph.rows[3].assignment_name.as_deref(), Some("顧客 名"));
        assert_eq!(graph.rows[3].source, "{顧客 名} = α + 1");
        assert_eq!(graph.rows[3].dependencies, vec![ids[2]]);
        assert_eq!(graph.rows[4].dependencies, vec![ids[2], ids[3]]);
        assert_eq!(graph.upstream(ids[4]), Some(vec![ids[2], ids[3]]));
        assert_eq!(graph.downstream(ids[2]), Some(vec![ids[3], ids[4], ids[5]]));
        assert!(matches!(
            graph.rows[6].outcome,
            LineOutcome::Value(CalculationValue::Vector(_))
        ));
        for (row, original) in graph.rows.iter().zip(&evaluation.lines) {
            assert_eq!(row.outcome, original.outcome);
            let mut expected = original
                .dependency_indices
                .iter()
                .map(|&index| ids[index])
                .collect::<Vec<_>>();
            expected.sort_by_key(|id| ids.iter().position(|candidate| candidate == id));
            expected.dedup();
            assert_eq!(row.dependencies, expected);
        }
        assert!(matches!(graph.rows[7].outcome, LineOutcome::Error(_)));
        assert!(matches!(graph.rows[9].outcome, LineOutcome::Error(_)));
        assert!(matches!(graph.rows[10].outcome, LineOutcome::Error(_)));
        assert_eq!(
            (
                session.source().unwrap().to_owned(),
                session.revision(),
                session.line_ids().unwrap().to_vec(),
                session.is_dirty(),
                session.can_undo(),
                session.can_redo(),
                session.evaluation_count()
            ),
            before
        );
    }

    #[test]
    fn dependency_location_tracks_live_line_and_rejects_stale_incarnations() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("α = 2\nα + 1").unwrap();
        session.save_as("dependencies").unwrap();
        let document = session.current_id().unwrap();
        let incarnation = session.document_incarnation().unwrap();
        let line = session.line_id(1).unwrap();
        session.evaluation();
        let first = session.dependency_snapshot().unwrap();
        let location = session
            .resolve_dependency_line(document, incarnation, line)
            .unwrap();
        assert_eq!(&session.source().unwrap()[location.source_range], "α + 1");
        session.replace_source("# inserted\nα = 2\nα + 1").unwrap();
        assert!(session.dependency_snapshot().is_none());
        assert!(
            session
                .resolve_dependency_line(document, incarnation, line)
                .is_none()
        );
        assert_eq!(session.evaluation_count(), 1);
        session.evaluation();
        let second = session.dependency_snapshot().unwrap();
        assert!(!std::sync::Arc::ptr_eq(&first, &second));
        assert_eq!(session.current.as_ref().unwrap().dependency_build_count, 2);
        let moved = session
            .resolve_dependency_line(document, incarnation, line)
            .unwrap();
        assert_eq!(moved.line_index, 2);
        assert_eq!(&session.source().unwrap()[moved.source_range], "α + 1");
        session.replace_source("# inserted\nα = 2").unwrap();
        session.evaluation();
        assert!(
            session
                .resolve_dependency_line(document, incarnation, line)
                .is_none()
        );
        assert_eq!(
            session.request_open(document).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(
            session
                .resolve_pending(TransitionDecision::Discard)
                .unwrap(),
            TransitionOutcome::Applied
        );
        session.evaluation();
        assert_eq!(session.current_id(), Some(document));
        assert_ne!(session.document_incarnation(), Some(incarnation));
        assert!(
            session
                .resolve_dependency_line(document, incarnation, line)
                .is_none()
        );
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
    fn exact_integer_modes_survive_session_cache_edit_undo_and_redo() {
        let (_directory, _root, mut session) = fixture();
        session.replace_source("mask = u8(40)\nmask + 1").unwrap();
        let ids = session.line_ids().unwrap().to_vec();

        let initial = integer(&mut session, 1);
        assert_eq!(initial.formatted_decimal(), "41");
        assert_eq!(initial.mode().type_name(), "u8");
        assert_eq!(session.evaluation_count(), 1);
        assert_eq!(integer(&mut session, 1), initial);
        assert_eq!(session.evaluation_count(), 1);

        session.replace_line(ids[0], "mask = u8(41)").unwrap();
        let edited = integer(&mut session, 1);
        assert_eq!(edited.formatted_decimal(), "42");
        assert_eq!(edited.mode().type_name(), "u8");
        assert_eq!(session.line_ids().unwrap().to_vec(), ids);
        assert_eq!(session.evaluation_count(), 2);

        assert!(session.undo());
        assert_eq!(integer(&mut session, 1), initial);
        assert_eq!(session.line_ids().unwrap().to_vec(), ids);
        assert_eq!(session.evaluation_count(), 3);

        assert!(session.redo());
        assert_eq!(integer(&mut session, 1), edited);
        assert_eq!(session.line_ids().unwrap().to_vec(), ids);
        assert_eq!(session.evaluation_count(), 4);
    }

    #[test]
    fn vector_values_follow_edits_history_save_reopen_and_inspector_rules() {
        use crate::calculation_pad::InspectorSourceOrigin;
        let (_directory, root, mut session) = fixture();
        session
            .replace_source("v = [1, 2, 3]\nr1 * 2\n9007199254740993 dec to hex\nu8(7)")
            .unwrap();
        let ids = session.line_ids().unwrap().to_vec();
        assert_eq!(formatted(&mut session, 1), "[2, 4, 6]");
        assert!(session.inspector_source(ids[0]).is_none());
        assert!(session.inspector_source(ids[1]).is_none());
        assert_eq!(
            session.inspector_source(ids[2]).unwrap().origin,
            InspectorSourceOrigin::LegacyExactBase
        );
        assert_eq!(
            session.inspector_source(ids[3]).unwrap().origin,
            InspectorSourceOrigin::TypedInteger
        );
        session.replace_line(ids[0], "v = [3, 4, 5]").unwrap();
        assert_eq!(formatted(&mut session, 1), "[6, 8, 10]");
        assert!(session.undo());
        assert_eq!(formatted(&mut session, 1), "[2, 4, 6]");
        assert!(session.redo());
        assert_eq!(formatted(&mut session, 1), "[6, 8, 10]");
        session.save_as("vector history").unwrap();
        let id = session.current_id().unwrap();
        let mut reopened = CalculationPadSession::new(&root);
        reopened.request_open(id).unwrap();
        assert_eq!(reopened.line_ids().unwrap(), ids);
        assert_eq!(formatted(&mut reopened, 1), "[6, 8, 10]");
    }

    #[test]
    fn dimensional_quantities_survive_session_cache_edit_undo_and_redo() {
        let (_directory, _root, mut session) = fixture();
        session
            .replace_source("area = (2 m)*(3 m)\narea to cm^2")
            .unwrap();
        let ids = session.line_ids().unwrap().to_vec();

        assert_eq!(formatted(&mut session, 0), "6 m²");
        assert_eq!(formatted(&mut session, 1), "60000 cm²");
        assert_eq!(session.evaluation_count(), 1);
        assert_eq!(formatted(&mut session, 1), "60000 cm²");
        assert_eq!(session.evaluation_count(), 1);

        session.replace_line(ids[0], "area = (3 m)*(3 m)").unwrap();
        assert_eq!(formatted(&mut session, 0), "9 m²");
        assert_eq!(formatted(&mut session, 1), "90000 cm²");
        assert_eq!(session.line_ids().unwrap(), ids);
        assert_eq!(session.evaluation_count(), 2);

        assert!(session.undo());
        assert_eq!(formatted(&mut session, 0), "6 m²");
        assert_eq!(formatted(&mut session, 1), "60000 cm²");
        assert_eq!(session.line_ids().unwrap(), ids);
        assert_eq!(session.evaluation_count(), 3);

        assert!(session.redo());
        assert_eq!(formatted(&mut session, 0), "9 m²");
        assert_eq!(formatted(&mut session, 1), "90000 cm²");
        assert_eq!(session.line_ids().unwrap(), ids);
        assert_eq!(session.evaluation_count(), 4);
    }

    #[test]
    fn recovered_source_is_an_exact_dirty_draft_with_fresh_line_ids() {
        let (_directory, _root, mut session) = fixture();
        let source = "α = 2\r\n{顧客 名} = α + 3\r\n";

        assert!(session.restore_recovered_source(source.to_owned()).unwrap());

        assert_eq!(session.source(), Some(source));
        assert!(session.is_dirty());
        assert!(!session.is_saved());
        assert!(!session.can_undo());
        let line_ids = session.line_ids().unwrap();
        assert_eq!(line_ids.len(), 3);
        let unique_ids = line_ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(unique_ids.len(), line_ids.len());
        assert_eq!(session.evaluation_count(), 0);
        session.evaluation().unwrap();
        assert_eq!(session.evaluation_count(), 1);
    }

    #[test]
    fn recovery_cannot_replace_a_live_draft_undone_to_blank() {
        let (_directory, _root, mut session) = fixture();
        let original_id = session.current_id();
        session.replace_source("temporary = 1").unwrap();
        assert!(session.undo());
        assert_eq!(session.source(), Some(""));
        assert!(session.can_redo());

        assert!(
            !session
                .restore_recovered_source("older draft = 9".to_owned())
                .unwrap()
        );

        assert_eq!(session.source(), Some(""));
        assert_eq!(session.current_id(), original_id);
        assert!(session.can_redo());
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
    fn recovery_scratch_close_rejects_saved_or_pending_documents() {
        let (_dir, _root, mut session) = fixture();
        session.replace_source("scratch").unwrap();
        session.request_new().unwrap();
        assert!(matches!(session.close_recovered_scratch(), Err(SessionError::PendingTransitionExists)));
        session.resolve_pending(TransitionDecision::Cancel).unwrap();
        session.save_as("bound").unwrap();
        assert!(matches!(session.close_recovered_scratch(), Err(SessionError::RecoveryScratchRequired)));
        assert_eq!(session.source(), Some("scratch"));
        session.request_new().unwrap();
        session.replace_source("recovered").unwrap();
        session.close_recovered_scratch().unwrap();
        assert_eq!(session.source(), None);
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
