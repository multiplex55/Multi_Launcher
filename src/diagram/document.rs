//! Explicit native document identity and replacement/close transitions.

use super::layout::{DiagramLayoutError, layout_positions};
use super::persistence::{
    DiagramDocumentStore, DiagramPersistenceError, LoadedDiagramDocument, MermaidFile,
    MermaidImportError,
};
use super::session::{DiagramSession, DiagramSessionError, GraphOperation};
use super::templates::{DiagramTemplate, DiagramTemplateError};
use crate::common::atomic_file::save_atomic;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PendingDiagramTransition {
    New,
    Open(PathBuf),
    ImportMermaid(MermaidFile),
    CopySource(DiagramSourceCopy),
    ApplyTemplate(DiagramTemplate),
    Close,
}

/// A bounded exact Mermaid source copied into an untitled Diagram document.
/// It is an in-memory handoff; constructing or applying it performs no IO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramSourceCopy {
    source: String,
}

impl DiagramSourceCopy {
    pub fn new(source: impl Into<String>) -> Result<Self, DiagramDocumentError> {
        let source = source.into();
        if source.len() > super::mermaid::MAX_MERMAID_SOURCE_BYTES {
            return Err(DiagramDocumentError::Session(
                DiagramSessionError::SourceTooLarge {
                    max_bytes: super::mermaid::MAX_MERMAID_SOURCE_BYTES,
                },
            ));
        }
        Ok(Self { source })
    }

    pub fn source(&self) -> &str {
        &self.source
    }
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
pub enum DiagramDocumentError {
    NoActiveDocument,
    SaveAsRequired,
    PendingTransitionExists,
    NoPendingTransition,
    Session(DiagramSessionError),
    Persistence(DiagramPersistenceError),
    MermaidImport(MermaidImportError),
    Template(DiagramTemplateError),
    Layout(DiagramLayoutError),
}

impl std::fmt::Display for DiagramDocumentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoActiveDocument => formatter.write_str("there is no active diagram document"),
            Self::SaveAsRequired => formatter.write_str("choose a path with Save As first"),
            Self::PendingTransitionExists => {
                formatter.write_str("resolve the pending diagram transition first")
            }
            Self::NoPendingTransition => {
                formatter.write_str("there is no pending diagram transition")
            }
            Self::Session(error) => error.fmt(formatter),
            Self::Persistence(error) => error.fmt(formatter),
            Self::MermaidImport(error) => error.fmt(formatter),
            Self::Template(error) => error.fmt(formatter),
            Self::Layout(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for DiagramDocumentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Session(error) => Some(error),
            Self::Persistence(error) => Some(error),
            Self::MermaidImport(error) => Some(error),
            Self::Template(error) => Some(error),
            Self::Layout(error) => Some(error),
            _ => None,
        }
    }
}

impl From<DiagramSessionError> for DiagramDocumentError {
    fn from(error: DiagramSessionError) -> Self {
        Self::Session(error)
    }
}

impl From<DiagramPersistenceError> for DiagramDocumentError {
    fn from(error: DiagramPersistenceError) -> Self {
        Self::Persistence(error)
    }
}

impl From<MermaidImportError> for DiagramDocumentError {
    fn from(error: MermaidImportError) -> Self {
        Self::MermaidImport(error)
    }
}

impl From<DiagramTemplateError> for DiagramDocumentError {
    fn from(error: DiagramTemplateError) -> Self {
        Self::Template(error)
    }
}

impl From<DiagramLayoutError> for DiagramDocumentError {
    fn from(error: DiagramLayoutError) -> Self {
        Self::Layout(error)
    }
}

struct ActiveDocument {
    session: DiagramSession,
    path: Option<PathBuf>,
    disk_digest: Option<[u8; 32]>,
}

impl ActiveDocument {
    fn blank() -> Self {
        Self {
            session: DiagramSession::new(),
            path: None,
            disk_digest: None,
        }
    }

    fn loaded(loaded: LoadedDiagramDocument, path: PathBuf) -> Self {
        Self {
            session: loaded.session,
            path: Some(path),
            disk_digest: Some(loaded.disk_digest),
        }
    }

    fn untitled(session: DiagramSession) -> Self {
        Self {
            session,
            path: None,
            disk_digest: None,
        }
    }
}

/// Owns the active session plus its external path/digest and one pending
/// replacement or close decision. The `DiagramSession` remains the only owner
/// of source, graph, history, allocator, revision, and content baseline.
pub struct DiagramDocumentController {
    store: DiagramDocumentStore,
    active: Option<ActiveDocument>,
    pending: Option<PendingDiagramTransition>,
}

impl Default for DiagramDocumentController {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagramDocumentController {
    /// Creates an immediately usable blank document without filesystem IO.
    pub fn new() -> Self {
        Self {
            store: DiagramDocumentStore,
            active: Some(ActiveDocument::blank()),
            pending: None,
        }
    }

    pub fn session(&self) -> Option<&DiagramSession> {
        self.active.as_ref().map(|active| &active.session)
    }

    pub fn path(&self) -> Option<&Path> {
        self.active
            .as_ref()
            .and_then(|active| active.path.as_deref())
    }

    pub fn disk_digest(&self) -> Option<[u8; 32]> {
        self.active.as_ref().and_then(|active| active.disk_digest)
    }

    pub fn is_dirty(&self) -> bool {
        self.session().is_some_and(DiagramSession::is_dirty)
    }

    pub fn pending_transition(&self) -> Option<&PendingDiagramTransition> {
        self.pending.as_ref()
    }

    pub fn can_undo(&self) -> bool {
        self.pending.is_none() && self.session().is_some_and(DiagramSession::can_undo)
    }

    pub fn can_redo(&self) -> bool {
        self.pending.is_none() && self.session().is_some_and(DiagramSession::can_redo)
    }

    pub fn replace_source(
        &mut self,
        source: impl AsRef<str>,
    ) -> Result<bool, DiagramDocumentError> {
        self.ensure_editable()?
            .replace_source(source)
            .map_err(Into::into)
    }

    pub fn apply_transaction(
        &mut self,
        operations: &[GraphOperation],
    ) -> Result<bool, DiagramDocumentError> {
        self.ensure_editable()?
            .apply_transaction(operations)
            .map_err(Into::into)
    }

    pub fn undo(&mut self) -> Result<bool, DiagramDocumentError> {
        self.ensure_editable()?.undo().map_err(Into::into)
    }

    pub fn redo(&mut self) -> Result<bool, DiagramDocumentError> {
        self.ensure_editable()?.redo().map_err(Into::into)
    }

    /// Applies deterministic positions as one guarded, undoable session edit.
    pub fn auto_layout(&mut self) -> Result<bool, DiagramDocumentError> {
        let session = self.ensure_editable()?;
        let graph = session
            .current_graph()
            .ok_or(DiagramDocumentError::Session(
                DiagramSessionError::VisualEditsBlocked,
            ))?;
        let positions = layout_positions(graph)?;
        session
            .apply_transaction(&[GraphOperation::MoveNodes { positions }])
            .map_err(Into::into)
    }

    /// Save always writes the complete native session, even when source/graph
    /// content is clean, so revision, allocator, and history state are retained.
    pub fn save(&mut self) -> Result<(), DiagramDocumentError> {
        self.save_with_writer(save_atomic)
    }

    /// Save As refuses occupied paths and publishes the new path/digest and
    /// saved-content acknowledgement only after the atomic write succeeds.
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> Result<(), DiagramDocumentError> {
        self.save_as_with_writer(path.as_ref(), save_atomic)
    }

    pub fn request_new(&mut self) -> Result<TransitionOutcome, DiagramDocumentError> {
        let requested = PendingDiagramTransition::New;
        if let Some(outcome) = self.repeat_or_reject_pending(&requested)? {
            return Ok(outcome);
        }
        if self.is_dirty() {
            self.pending = Some(requested);
            return Ok(TransitionOutcome::DecisionRequired);
        }
        self.active = Some(ActiveDocument::blank());
        Ok(TransitionOutcome::Applied)
    }

    /// Validates a candidate before touching the active document. A pending
    /// Open retains only its path and rereads that path when the decision is
    /// applied, so a Save to the same path cannot install a stale candidate.
    pub fn request_open(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<TransitionOutcome, DiagramDocumentError> {
        let path = path.as_ref().to_path_buf();
        let requested = PendingDiagramTransition::Open(path.clone());
        if let Some(outcome) = self.repeat_or_reject_pending(&requested)? {
            return Ok(outcome);
        }
        let loaded = self.store.open(&path)?;
        if self.is_dirty() {
            self.pending = Some(requested);
            return Ok(TransitionOutcome::DecisionRequired);
        }
        self.active = Some(ActiveDocument::loaded(loaded, path));
        Ok(TransitionOutcome::Applied)
    }

    /// Requests a bounded UTF-8 `.mmd` import. Pending imports retain the path
    /// and reread it when applied after a Save decision.
    pub fn request_mermaid_import(
        &mut self,
        file: MermaidFile,
    ) -> Result<TransitionOutcome, DiagramDocumentError> {
        let requested = PendingDiagramTransition::ImportMermaid(file.clone());
        if let Some(outcome) = self.repeat_or_reject_pending(&requested)? {
            return Ok(outcome);
        }

        let source = self.store.read_mermaid(&file)?;
        let candidate = DiagramSession::from_fresh_source(source)?;
        if self.is_dirty() {
            self.pending = Some(requested);
            return Ok(TransitionOutcome::DecisionRequired);
        }
        self.active = Some(ActiveDocument::untitled(candidate));
        Ok(TransitionOutcome::Applied)
    }

    /// Requests an exact in-memory Mermaid source copy as a new untitled
    /// document. Dirty drafts use the same Save/Discard/Cancel transition as
    /// every other document replacement; no temporary file is created.
    pub fn request_source_copy(
        &mut self,
        source: DiagramSourceCopy,
    ) -> Result<TransitionOutcome, DiagramDocumentError> {
        let requested = PendingDiagramTransition::CopySource(source.clone());
        if let Some(outcome) = self.repeat_or_reject_pending(&requested)? {
            return Ok(outcome);
        }
        let candidate = DiagramSession::from_fresh_source(&source.source)?;
        if self.is_dirty() {
            self.pending = Some(requested);
            return Ok(TransitionOutcome::DecisionRequired);
        }
        self.active = Some(ActiveDocument::untitled(candidate));
        Ok(TransitionOutcome::Applied)
    }

    /// Requests a built-in validated template as a new untitled session.
    pub fn request_template(
        &mut self,
        template: DiagramTemplate,
    ) -> Result<TransitionOutcome, DiagramDocumentError> {
        let requested = PendingDiagramTransition::ApplyTemplate(template);
        if let Some(outcome) = self.repeat_or_reject_pending(&requested)? {
            return Ok(outcome);
        }
        let candidate = template.create_session()?;
        if self.is_dirty() {
            self.pending = Some(requested);
            return Ok(TransitionOutcome::DecisionRequired);
        }
        self.active = Some(ActiveDocument::untitled(candidate));
        Ok(TransitionOutcome::Applied)
    }

    pub fn request_close(&mut self) -> Result<TransitionOutcome, DiagramDocumentError> {
        let requested = PendingDiagramTransition::Close;
        if let Some(outcome) = self.repeat_or_reject_pending(&requested)? {
            return Ok(outcome);
        }
        if self.active.is_none() {
            return Ok(TransitionOutcome::Applied);
        }
        if self.is_dirty() {
            self.pending = Some(requested);
            return Ok(TransitionOutcome::DecisionRequired);
        }
        self.active = None;
        Ok(TransitionOutcome::Applied)
    }

    pub fn resolve_pending(
        &mut self,
        decision: TransitionDecision,
    ) -> Result<TransitionOutcome, DiagramDocumentError> {
        self.resolve_pending_with_writer(decision, save_atomic)
    }

    /// Saves the active document to a newly selected path and applies the
    /// pending transition after exactly one successful publication.
    ///
    /// This is the Save As continuation for a pending Save decision. The
    /// pending action is checked before any file inspection or write.
    pub fn resolve_pending_save_as(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<TransitionOutcome, DiagramDocumentError> {
        self.resolve_pending_save_as_with_writer(path.as_ref(), save_atomic)
    }

    fn resolve_pending_save_as_with_writer(
        &mut self,
        path: &Path,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<TransitionOutcome, DiagramDocumentError> {
        let pending = self
            .pending
            .clone()
            .ok_or(DiagramDocumentError::NoPendingTransition)?;
        self.save_as_with_writer(path, writer)?;
        self.apply_pending(pending)
    }

    fn resolve_pending_with_writer(
        &mut self,
        decision: TransitionDecision,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<TransitionOutcome, DiagramDocumentError> {
        let pending = self
            .pending
            .clone()
            .ok_or(DiagramDocumentError::NoPendingTransition)?;
        match decision {
            TransitionDecision::Cancel => {
                self.pending = None;
                Ok(TransitionOutcome::Cancelled)
            }
            TransitionDecision::Save => {
                self.save_with_writer(writer)?;
                self.apply_pending(pending)
            }
            TransitionDecision::Discard => self.apply_pending(pending),
        }
    }

    fn apply_pending(
        &mut self,
        pending: PendingDiagramTransition,
    ) -> Result<TransitionOutcome, DiagramDocumentError> {
        match pending {
            PendingDiagramTransition::New => {
                self.active = Some(ActiveDocument::blank());
            }
            PendingDiagramTransition::Close => {
                self.active = None;
            }
            PendingDiagramTransition::Open(path) => {
                // Keep both the active draft and the pending decision if the
                // target changed, disappeared, or became invalid meanwhile.
                let loaded = self.store.open(&path)?;
                self.active = Some(ActiveDocument::loaded(loaded, path));
            }
            PendingDiagramTransition::ImportMermaid(file) => {
                let source = self.store.read_mermaid(&file)?;
                let session = DiagramSession::from_fresh_source(source)?;
                self.active = Some(ActiveDocument::untitled(session));
            }
            PendingDiagramTransition::CopySource(source) => {
                let session = DiagramSession::from_fresh_source(&source.source)?;
                self.active = Some(ActiveDocument::untitled(session));
            }
            PendingDiagramTransition::ApplyTemplate(template) => {
                self.active = Some(ActiveDocument::untitled(template.create_session()?));
            }
        }
        self.pending = None;
        Ok(TransitionOutcome::Applied)
    }

    fn save_with_writer(
        &mut self,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<(), DiagramDocumentError> {
        let active = self
            .active
            .as_ref()
            .ok_or(DiagramDocumentError::NoActiveDocument)?;
        let path = active
            .path
            .as_deref()
            .ok_or(DiagramDocumentError::SaveAsRequired)?;
        let expected_digest = active
            .disk_digest
            .ok_or(DiagramDocumentError::SaveAsRequired)?;
        let saved = active.session.capture_saved_content();
        let disk_digest =
            self.store
                .save_existing_with_writer(path, expected_digest, &active.session, writer)?;
        let active = self
            .active
            .as_mut()
            .ok_or(DiagramDocumentError::NoActiveDocument)?;
        active.session.acknowledge_saved_content(&saved);
        active.disk_digest = Some(disk_digest);
        Ok(())
    }

    fn save_as_with_writer(
        &mut self,
        path: &Path,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<(), DiagramDocumentError> {
        let active = self
            .active
            .as_ref()
            .ok_or(DiagramDocumentError::NoActiveDocument)?;
        let saved = active.session.capture_saved_content();
        let disk_digest = self
            .store
            .save_as_with_writer(path, &active.session, writer)?;
        let active = self
            .active
            .as_mut()
            .ok_or(DiagramDocumentError::NoActiveDocument)?;
        active.session.acknowledge_saved_content(&saved);
        active.path = Some(path.to_path_buf());
        active.disk_digest = Some(disk_digest);
        Ok(())
    }

    fn ensure_editable(&mut self) -> Result<&mut DiagramSession, DiagramDocumentError> {
        if self.pending.is_some() {
            return Err(DiagramDocumentError::PendingTransitionExists);
        }
        self.active
            .as_mut()
            .map(|active| &mut active.session)
            .ok_or(DiagramDocumentError::NoActiveDocument)
    }

    fn repeat_or_reject_pending(
        &self,
        requested: &PendingDiagramTransition,
    ) -> Result<Option<TransitionOutcome>, DiagramDocumentError> {
        match self.pending.as_ref() {
            None => Ok(None),
            Some(pending) if pending == requested => Ok(Some(TransitionOutcome::DecisionRequired)),
            Some(_) => Err(DiagramDocumentError::PendingTransitionExists),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DiagramDocumentController, DiagramDocumentError, DiagramSourceCopy,
        PendingDiagramTransition, TransitionDecision, TransitionOutcome,
    };
    use crate::diagram::{
        DiagramPersistenceError, DiagramSession, DiagramSessionError, DiagramTemplate,
        DiskConflictKind, FlowDirection, GraphOperation, MAX_DIAGRAM_DOCUMENT_BYTES, MermaidFile,
        MermaidImportError, NodeId, NodePosition, Position, PreviewState,
    };
    use std::fs;

    const SUPPORTED: &str = "flowchart LR\nA[\"Alpha\"]\nB[\"Beta\"]\nA --> B\n";

    #[test]
    fn diagram_document_reopen_preserves_unsupported_draft_last_good_layout_ids_and_history() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("diagram.native");
        let mut original = DiagramDocumentController::new();
        original.replace_source(SUPPORTED).unwrap();
        original
            .apply_transaction(&[
                GraphOperation::MoveNodes {
                    positions: vec![NodePosition {
                        id: NodeId::new("A").unwrap(),
                        position: Position::new(420.0, -75.0).unwrap(),
                    }],
                },
                GraphOperation::SetDirection(FlowDirection::TopBottom),
            ])
            .unwrap();
        let last_good_source = original
            .session()
            .unwrap()
            .last_good_source()
            .unwrap()
            .to_owned();
        let last_good_graph = original.session().unwrap().preview_graph().unwrap().clone();
        let unsupported = "%% keep exact 🧭\r\nflowchart LR\r\nA --> B\r\n";
        original.replace_source(unsupported).unwrap();
        let revision = original.session().unwrap().revision();
        let undo_depth = original.session().unwrap().undo_depth();
        let allocator = original.session().unwrap().next_edge_id();
        assert_eq!(
            original.session().unwrap().status().preview,
            PreviewState::Stale
        );
        original.save_as(&path).unwrap();

        let mut reopened = DiagramDocumentController::new();
        assert_eq!(
            reopened.request_open(&path).unwrap(),
            TransitionOutcome::Applied
        );
        let loaded = reopened.session().unwrap();
        assert_eq!(loaded.draft(), unsupported);
        assert_eq!(loaded.status().preview, PreviewState::Stale);
        assert_eq!(loaded.last_good_source(), Some(last_good_source.as_str()));
        assert_eq!(loaded.preview_graph(), Some(&last_good_graph));
        assert_eq!(loaded.revision(), revision);
        assert_eq!(loaded.undo_depth(), undo_depth);
        assert_eq!(loaded.next_edge_id(), allocator);
        assert!(!reopened.is_dirty());
        assert!(reopened.can_undo());
        assert!(reopened.undo().unwrap());
        let restored = reopened.session().unwrap();
        assert_eq!(restored.draft(), last_good_source);
        assert_eq!(restored.status().preview, PreviewState::Current);
        assert_eq!(restored.current_graph(), Some(&last_good_graph));
        assert!(reopened.is_dirty());
        assert!(reopened.redo().unwrap());
        assert_eq!(reopened.session().unwrap().draft(), unsupported);
        assert_eq!(
            reopened.session().unwrap().status().preview,
            PreviewState::Stale
        );
        assert!(!reopened.is_dirty());
    }

    #[test]
    fn diagram_document_pending_new_open_and_close_apply_save_discard_and_cancel_safely() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target.native");
        let mut target_doc = DiagramDocumentController::new();
        target_doc.replace_source("flowchart TD\nTarget\n").unwrap();
        target_doc.save_as(&target).unwrap();

        let mut controller = DiagramDocumentController::new();
        controller.replace_source("flowchart TD\nDraft\n").unwrap();
        assert_eq!(
            controller.request_new().unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(
            controller.request_new().unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert!(matches!(
            controller.replace_source("flowchart TD\nBlocked\n"),
            Err(DiagramDocumentError::PendingTransitionExists)
        ));
        assert_eq!(
            controller
                .resolve_pending(TransitionDecision::Cancel)
                .unwrap(),
            TransitionOutcome::Cancelled
        );
        assert_eq!(
            controller.session().unwrap().draft(),
            "flowchart TD\nDraft\n"
        );

        controller.request_new().unwrap();
        controller
            .resolve_pending(TransitionDecision::Discard)
            .unwrap();
        assert_eq!(controller.session().unwrap().draft(), "");
        controller
            .replace_source("flowchart TD\nReplacement\n")
            .unwrap();
        assert_eq!(
            controller.request_open(&target).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert!(matches!(
            controller.pending_transition(),
            Some(PendingDiagramTransition::Open(path)) if path == &target
        ));
        controller
            .resolve_pending(TransitionDecision::Discard)
            .unwrap();
        assert_eq!(
            controller.session().unwrap().draft(),
            "flowchart TD\nTarget\n"
        );

        controller
            .replace_source("flowchart TD\nUnsaved\n")
            .unwrap();
        assert_eq!(
            controller.request_close().unwrap(),
            TransitionOutcome::DecisionRequired
        );
        controller
            .resolve_pending(TransitionDecision::Discard)
            .unwrap();
        assert!(controller.session().is_none());
        assert_eq!(
            controller.request_close().unwrap(),
            TransitionOutcome::Applied
        );
        let reopened_empty = DiagramDocumentController::new();
        assert_eq!(reopened_empty.session().unwrap().draft(), "");

        let saved_on_close = directory.path().join("saved-on-close.native");
        let mut save_then_close = DiagramDocumentController::new();
        save_then_close
            .replace_source("flowchart TD\nSaved before close\n")
            .unwrap();
        save_then_close.request_close().unwrap();
        assert!(matches!(
            save_then_close.resolve_pending(TransitionDecision::Save),
            Err(DiagramDocumentError::SaveAsRequired)
        ));
        assert!(matches!(
            save_then_close.pending_transition(),
            Some(PendingDiagramTransition::Close)
        ));
        let mut save_count = 0;
        save_then_close
            .resolve_pending_save_as_with_writer(&saved_on_close, |path, bytes| {
                save_count += 1;
                crate::common::atomic_file::save_atomic(path, bytes)
            })
            .unwrap();
        assert_eq!(save_count, 1, "Save As plus Close must publish once");
        assert!(save_then_close.session().is_none());
        let mut saved_close_reopen = DiagramDocumentController::new();
        saved_close_reopen.request_open(&saved_on_close).unwrap();
        assert_eq!(
            saved_close_reopen.session().unwrap().draft(),
            "flowchart TD\nSaved before close\n"
        );
    }

    #[test]
    fn diagram_document_pending_saveas_failure_and_cancel_keep_active_state() {
        let directory = tempfile::tempdir().unwrap();
        let occupied = directory.path().join("occupied.native");
        fs::write(&occupied, b"keep me").unwrap();
        let intended = directory.path().join("new.native");

        let mut controller = DiagramDocumentController::new();
        assert!(matches!(
            controller.save(),
            Err(DiagramDocumentError::SaveAsRequired)
        ));
        let mut no_pending_writes = 0;
        assert!(matches!(
            controller.resolve_pending_save_as_with_writer(&intended, |_path, _bytes| {
                no_pending_writes += 1;
                Ok(())
            }),
            Err(DiagramDocumentError::NoPendingTransition)
        ));
        assert_eq!(no_pending_writes, 0);
        assert!(!intended.exists());
        controller.replace_source("flowchart TD\nDraft\n").unwrap();
        controller.request_close().unwrap();
        assert!(matches!(
            controller.resolve_pending(TransitionDecision::Save),
            Err(DiagramDocumentError::SaveAsRequired)
        ));
        let pending = controller.pending_transition().cloned();
        let draft = controller.session().unwrap().draft().to_owned();
        // Cancelling the Save As chooser invokes no controller mutation.
        assert_eq!(controller.path(), None);
        assert_eq!(controller.disk_digest(), None);
        assert_eq!(controller.pending_transition(), pending.as_ref());
        assert_eq!(controller.session().unwrap().draft(), draft);
        assert!(matches!(
            controller.save_as(&occupied),
            Err(DiagramDocumentError::Persistence(
                DiagramPersistenceError::OccupiedPath(_)
            ))
        ));
        assert_eq!(controller.path(), None);
        assert_eq!(controller.disk_digest(), None);
        assert_eq!(controller.pending_transition(), pending.as_ref());
        assert_eq!(controller.session().unwrap().draft(), draft);
        assert!(controller.is_dirty());

        let failure = controller.resolve_pending_save_as_with_writer(&intended, |_path, _bytes| {
            anyhow::bail!("injected publication failure")
        });
        assert!(matches!(
            failure,
            Err(DiagramDocumentError::Persistence(
                DiagramPersistenceError::AtomicWrite { .. }
            ))
        ));
        assert!(!intended.exists());
        assert_eq!(controller.path(), None);
        assert_eq!(controller.pending_transition(), pending.as_ref());
        assert_eq!(controller.session().unwrap().draft(), draft);

        controller
            .resolve_pending(TransitionDecision::Cancel)
            .unwrap();
        assert!(controller.pending_transition().is_none());
        assert_eq!(controller.session().unwrap().draft(), draft);
        assert!(controller.is_dirty());
    }

    #[test]
    fn diagram_document_save_writes_clean_content_history_and_open_current_rereads_after_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("same.native");
        let mut controller = DiagramDocumentController::new();
        controller.replace_source(SUPPORTED).unwrap();
        controller.save_as(&path).unwrap();
        controller
            .apply_transaction(&[GraphOperation::SetDirection(FlowDirection::TopBottom)])
            .unwrap();
        controller.undo().unwrap();
        assert!(!controller.is_dirty());
        assert!(controller.can_redo());
        let before_explicit_save = fs::read(&path).unwrap();
        controller.save().unwrap();
        let after_explicit_save = fs::read(&path).unwrap();
        assert_ne!(after_explicit_save, before_explicit_save);

        controller
            .replace_source("flowchart LR\nChanged after disk baseline\n")
            .unwrap();
        assert_eq!(
            controller.request_open(&path).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        controller
            .resolve_pending(TransitionDecision::Save)
            .unwrap();
        assert_eq!(
            controller.session().unwrap().draft(),
            "flowchart LR\nChanged after disk baseline\n"
        );
        assert!(!controller.is_dirty());
        assert!(controller.can_undo());

        let mut reopened = DiagramDocumentController::new();
        reopened.request_open(&path).unwrap();
        assert_eq!(
            reopened.session().unwrap().draft(),
            "flowchart LR\nChanged after disk baseline\n"
        );
        assert!(reopened.can_undo());
    }

    #[test]
    fn diagram_document_open_validation_and_failed_pending_open_preserve_active_state() {
        let directory = tempfile::tempdir().unwrap();
        let malformed = directory.path().join("malformed.native");
        let future = directory.path().join("future.native");
        let oversized = directory.path().join("oversized.native");
        fs::write(&malformed, b"not a diagram document").unwrap();
        fs::write(&oversized, vec![b' '; MAX_DIAGRAM_DOCUMENT_BYTES + 1]).unwrap();
        let future_bytes = DiagramSession::new().encode().unwrap();
        let mut future_json: serde_json::Value = serde_json::from_slice(&future_bytes).unwrap();
        future_json["format_version"] = serde_json::json!(99);
        fs::write(&future, serde_json::to_vec(&future_json).unwrap()).unwrap();

        let mut controller = DiagramDocumentController::new();
        controller.replace_source("flowchart TD\nKeep\n").unwrap();
        let active_draft = controller.session().unwrap().draft().to_owned();
        for path in [&malformed, &future, &oversized] {
            assert!(controller.request_open(path).is_err());
            assert_eq!(controller.session().unwrap().draft(), active_draft);
            assert!(controller.pending_transition().is_none());
        }

        let valid_target = directory.path().join("valid.native");
        let mut target = DiagramDocumentController::new();
        target.replace_source("flowchart TD\nTarget\n").unwrap();
        target.save_as(&valid_target).unwrap();
        assert_eq!(
            controller.request_open(&valid_target).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        fs::write(&valid_target, b"changed into malformed content").unwrap();
        let pending = controller.pending_transition().cloned();
        assert!(
            controller
                .resolve_pending(TransitionDecision::Discard)
                .is_err()
        );
        assert_eq!(controller.session().unwrap().draft(), active_draft);
        assert_eq!(controller.pending_transition(), pending.as_ref());

        let current_path = directory.path().join("current.native");
        let saved_as_current_path = directory.path().join("saved-as-current.native");
        let changed_target = directory.path().join("changed-target.native");
        let mut current = DiagramDocumentController::new();
        current
            .replace_source("flowchart TD\nInitial local\n")
            .unwrap();
        current.save_as(&current_path).unwrap();
        let initial_digest = current.disk_digest();
        current
            .replace_source("flowchart TD\nEdited local\n")
            .unwrap();
        let mut candidate = DiagramDocumentController::new();
        candidate
            .replace_source("flowchart TD\nCandidate\n")
            .unwrap();
        candidate.save_as(&changed_target).unwrap();
        current.request_open(&changed_target).unwrap();
        fs::write(&changed_target, b"invalidated after validation").unwrap();

        let mut save_count = 0;
        assert!(
            current
                .resolve_pending_save_as_with_writer(&saved_as_current_path, |path, bytes| {
                    save_count += 1;
                    crate::common::atomic_file::save_atomic(path, bytes)
                })
                .is_err()
        );
        assert_eq!(save_count, 1);
        assert_eq!(
            current.session().unwrap().draft(),
            "flowchart TD\nEdited local\n"
        );
        assert!(!current.is_dirty());
        assert_eq!(current.path(), Some(saved_as_current_path.as_path()));
        assert_ne!(current.disk_digest(), initial_digest);
        let mut saved_current = DiagramDocumentController::new();
        saved_current.request_open(&saved_as_current_path).unwrap();
        assert_eq!(
            saved_current.session().unwrap().draft(),
            "flowchart TD\nEdited local\n"
        );
        assert_eq!(
            current.pending_transition(),
            Some(&PendingDiagramTransition::Open(changed_target.clone()))
        );
        current.resolve_pending(TransitionDecision::Cancel).unwrap();
    }

    #[test]
    fn diagram_document_changed_and_deleted_existing_save_keep_baseline_and_pending_choice() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("document.native");
        let mut controller = DiagramDocumentController::new();
        controller
            .replace_source("flowchart TD\nOriginal\n")
            .unwrap();
        controller.save_as(&path).unwrap();
        let digest = controller.disk_digest();
        controller
            .replace_source("flowchart TD\nChanged\n")
            .unwrap();
        controller.request_close().unwrap();
        fs::write(&path, b"external change").unwrap();
        assert!(matches!(
            controller.resolve_pending(TransitionDecision::Save),
            Err(DiagramDocumentError::Persistence(
                DiagramPersistenceError::DiskConflict {
                    kind: DiskConflictKind::Changed,
                    ..
                }
            ))
        ));
        assert_eq!(controller.path(), Some(path.as_path()));
        assert_eq!(controller.disk_digest(), digest);
        assert_eq!(
            controller.session().unwrap().draft(),
            "flowchart TD\nChanged\n"
        );
        assert!(controller.is_dirty());
        assert_eq!(
            controller.pending_transition(),
            Some(&PendingDiagramTransition::Close)
        );
        fs::remove_file(&path).unwrap();
        assert!(matches!(
            controller.resolve_pending(TransitionDecision::Save),
            Err(DiagramDocumentError::Persistence(
                DiagramPersistenceError::DiskConflict {
                    kind: DiskConflictKind::Deleted,
                    ..
                }
            ))
        ));
        assert_eq!(
            controller.pending_transition(),
            Some(&PendingDiagramTransition::Close)
        );
        controller
            .resolve_pending(TransitionDecision::Cancel)
            .unwrap();
    }

    #[test]
    fn diagram_document_failed_pending_existing_save_keeps_bytes_baseline_and_decision() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("document.native");
        let mut controller = DiagramDocumentController::new();
        controller
            .replace_source("flowchart TD\nSaved state\n")
            .unwrap();
        controller.save_as(&path).unwrap();
        let original_bytes = fs::read(&path).unwrap();
        let original_digest = controller.disk_digest();
        controller
            .replace_source("flowchart TD\nUnsaved state\n")
            .unwrap();
        controller.request_close().unwrap();

        assert!(matches!(
            controller.resolve_pending_with_writer(TransitionDecision::Save, |_path, _bytes| {
                anyhow::bail!("injected atomic publication failure")
            }),
            Err(DiagramDocumentError::Persistence(
                DiagramPersistenceError::AtomicWrite { .. }
            ))
        ));
        assert_eq!(fs::read(&path).unwrap(), original_bytes);
        assert_eq!(controller.disk_digest(), original_digest);
        assert_eq!(controller.path(), Some(path.as_path()));
        assert_eq!(
            controller.session().unwrap().draft(),
            "flowchart TD\nUnsaved state\n"
        );
        assert!(controller.is_dirty());
        assert_eq!(
            controller.pending_transition(),
            Some(&PendingDiagramTransition::Close)
        );
        controller
            .resolve_pending(TransitionDecision::Cancel)
            .unwrap();
    }

    #[test]
    fn diagram_document_failed_pending_save_as_keeps_existing_identity_and_baseline() {
        let directory = tempfile::tempdir().unwrap();
        let original_path = directory.path().join("original.native");
        let save_as_path = directory.path().join("replacement.native");
        let mut controller = DiagramDocumentController::new();
        controller
            .replace_source("flowchart TD\nSaved version\n")
            .unwrap();
        controller.save_as(&original_path).unwrap();
        let original_bytes = fs::read(&original_path).unwrap();
        let original_digest = controller.disk_digest();
        controller
            .replace_source("flowchart TD\nUnsaved version\n")
            .unwrap();
        controller.request_close().unwrap();
        let pending = controller.pending_transition().cloned();

        assert!(matches!(
            controller.resolve_pending_save_as_with_writer(&save_as_path, |_path, _bytes| {
                anyhow::bail!("injected Save As failure")
            }),
            Err(DiagramDocumentError::Persistence(
                DiagramPersistenceError::AtomicWrite { .. }
            ))
        ));
        assert!(!save_as_path.exists());
        assert_eq!(fs::read(&original_path).unwrap(), original_bytes);
        assert_eq!(controller.path(), Some(original_path.as_path()));
        assert_eq!(controller.disk_digest(), original_digest);
        assert_eq!(
            controller.session().unwrap().draft(),
            "flowchart TD\nUnsaved version\n"
        );
        assert!(controller.is_dirty());
        assert_eq!(controller.pending_transition(), pending.as_ref());
    }

    #[test]
    fn diagram_document_mermaid_import_is_exact_untitled_and_reports_no_preview() {
        let directory = tempfile::tempdir().unwrap();
        let source = "\u{feff}%% retain BOM, CRLF and comment\r\nflowchart TD\r\nA --> B\r\n";
        let path = directory.path().join("unsupported.mmd");
        let native_path = directory.path().join("before-import.native");
        fs::write(&path, source.as_bytes()).unwrap();

        let mut controller = DiagramDocumentController::new();
        controller.save_as(&native_path).unwrap();
        assert_eq!(controller.path(), Some(native_path.as_path()));
        let invalid_utf8 = directory.path().join("invalid.mmd");
        fs::write(&invalid_utf8, [0xff_u8]).unwrap();
        assert!(matches!(
            controller.request_mermaid_import(MermaidFile::new(&invalid_utf8).unwrap()),
            Err(DiagramDocumentError::MermaidImport(
                MermaidImportError::InvalidUtf8(_)
            ))
        ));
        assert_eq!(controller.session().unwrap().draft(), "");
        assert_eq!(controller.path(), Some(native_path.as_path()));
        assert_eq!(controller.pending_transition(), None);

        assert_eq!(
            controller
                .request_mermaid_import(MermaidFile::new(&path).unwrap())
                .unwrap(),
            TransitionOutcome::Applied
        );
        let session = controller.session().unwrap();
        assert_eq!(session.draft(), source);
        assert_eq!(session.status().preview, PreviewState::NoPreview);
        assert_eq!(session.last_good_source(), None);
        assert_eq!(session.revision(), 1);
        assert_eq!(session.undo_depth(), 1);
        assert!(session.is_dirty());
        assert_eq!(controller.path(), None);
        assert_eq!(controller.disk_digest(), None);
    }

    #[test]
    fn diagram_document_source_copy_is_in_memory_bounded_and_uses_dirty_decisions() {
        let source = "\u{feff}%% exact copied draft\r\nflowchart TD\r\nA -->\r\n";
        let request = DiagramSourceCopy::new(source).unwrap();
        let mut controller = DiagramDocumentController::new();
        controller.replace_source("flowchart LR\nDirty\n").unwrap();
        let original = controller.session().unwrap().draft().to_owned();
        assert_eq!(
            controller.request_source_copy(request.clone()).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(controller.session().unwrap().draft(), original);
        assert!(matches!(
            controller.pending_transition(),
            Some(PendingDiagramTransition::CopySource(pending)) if pending == &request
        ));
        assert_eq!(
            controller
                .resolve_pending(TransitionDecision::Cancel)
                .unwrap(),
            TransitionOutcome::Cancelled
        );
        assert_eq!(controller.session().unwrap().draft(), original);
        assert_eq!(controller.pending_transition(), None);

        assert_eq!(
            controller.request_source_copy(request).unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(
            controller
                .resolve_pending(TransitionDecision::Discard)
                .unwrap(),
            TransitionOutcome::Applied
        );
        let copied = controller.session().unwrap();
        assert_eq!(copied.draft(), source);
        assert_eq!(copied.status().preview, PreviewState::NoPreview);
        assert_eq!(copied.revision(), 1);
        assert_eq!(copied.undo_depth(), 1);
        assert!(copied.is_dirty());
        assert_eq!(controller.path(), None);
        assert_eq!(controller.disk_digest(), None);
        assert!(matches!(
            DiagramSourceCopy::new("x".repeat(super::super::mermaid::MAX_MERMAID_SOURCE_BYTES + 1)),
            Err(DiagramDocumentError::Session(
                DiagramSessionError::SourceTooLarge { .. }
            ))
        ));
    }

    #[test]
    fn diagram_document_dirty_template_replacement_cancels_or_discards_once() {
        let directory = tempfile::tempdir().unwrap();
        let native_path = directory.path().join("before-template.native");
        let mut controller = DiagramDocumentController::new();
        controller.replace_source("flowchart TD\nOld\n").unwrap();
        controller.save_as(&native_path).unwrap();
        controller
            .replace_source("flowchart TD\nEdited old\n")
            .unwrap();
        let original = controller.session().unwrap().draft().to_owned();

        assert_eq!(
            controller
                .request_template(DiagramTemplate::Decision)
                .unwrap(),
            TransitionOutcome::DecisionRequired
        );
        assert_eq!(
            controller.pending_transition(),
            Some(&PendingDiagramTransition::ApplyTemplate(
                DiagramTemplate::Decision
            ))
        );
        assert_eq!(
            controller
                .resolve_pending(TransitionDecision::Cancel)
                .unwrap(),
            TransitionOutcome::Cancelled
        );
        assert_eq!(controller.session().unwrap().draft(), original);
        assert!(controller.is_dirty());
        assert_eq!(controller.path(), Some(native_path.as_path()));

        controller
            .request_template(DiagramTemplate::Decision)
            .unwrap();
        assert_eq!(
            controller
                .resolve_pending(TransitionDecision::Discard)
                .unwrap(),
            TransitionOutcome::Applied
        );
        let session = controller.session().unwrap();
        assert_eq!(session.draft(), DiagramTemplate::Decision.source());
        assert_eq!(session.status().preview, PreviewState::Current);
        assert_eq!(session.revision(), 1);
        assert_eq!(session.undo_depth(), 1);
        assert_eq!(controller.path(), None);
        assert_eq!(controller.disk_digest(), None);
    }

    #[test]
    fn diagram_document_pending_import_rereads_after_single_save_as_publication() {
        let directory = tempfile::tempdir().unwrap();
        let import_path = directory.path().join("target.mmd");
        let save_as_path = directory.path().join("before-import.native");
        fs::write(&import_path, b"flowchart LR\nInitial\n").unwrap();

        let mut controller = DiagramDocumentController::new();
        controller.replace_source(SUPPORTED).unwrap();
        assert_eq!(
            controller
                .request_mermaid_import(MermaidFile::new(&import_path).unwrap())
                .unwrap(),
            TransitionOutcome::DecisionRequired
        );
        let reread_source = "\u{feff}%% changed after request\r\nflowchart TD\r\nA --> B\r\n";
        fs::write(&import_path, reread_source.as_bytes()).unwrap();

        assert!(matches!(
            controller.resolve_pending_save_as_with_writer(&save_as_path, |_path, _bytes| {
                anyhow::bail!("injected Save As failure")
            }),
            Err(DiagramDocumentError::Persistence(
                DiagramPersistenceError::AtomicWrite { .. }
            ))
        ));
        assert!(!save_as_path.exists());
        assert_eq!(controller.session().unwrap().draft(), SUPPORTED);
        assert_eq!(
            controller.pending_transition(),
            Some(&PendingDiagramTransition::ImportMermaid(
                MermaidFile::new(&import_path).unwrap()
            ))
        );
        assert!(controller.is_dirty());

        let mut writes = 0;
        assert_eq!(
            controller
                .resolve_pending_save_as_with_writer(&save_as_path, |path, bytes| {
                    writes += 1;
                    crate::common::atomic_file::save_atomic(path, bytes)
                })
                .unwrap(),
            TransitionOutcome::Applied
        );
        assert_eq!(writes, 1);
        let saved_before_import =
            DiagramSession::decode(&fs::read(&save_as_path).unwrap()).unwrap();
        assert_eq!(saved_before_import.draft(), SUPPORTED);

        let imported = controller.session().unwrap();
        assert_eq!(imported.draft(), reread_source);
        assert_eq!(imported.status().preview, PreviewState::NoPreview);
        assert_eq!(imported.revision(), 1);
        assert_eq!(imported.undo_depth(), 1);
        assert_eq!(controller.path(), None);
        assert!(controller.is_dirty());
        assert_eq!(controller.pending_transition(), None);
    }

    #[test]
    fn diagram_document_pending_import_reread_failure_keeps_active_draft_and_decision() {
        let directory = tempfile::tempdir().unwrap();
        let import_path = directory.path().join("temporary.mmd");
        fs::write(&import_path, b"flowchart LR\nCandidate\n").unwrap();

        let mut controller = DiagramDocumentController::new();
        controller.replace_source(SUPPORTED).unwrap();
        controller
            .request_mermaid_import(MermaidFile::new(&import_path).unwrap())
            .unwrap();
        let pending = controller.pending_transition().cloned();
        fs::remove_file(&import_path).unwrap();

        assert!(matches!(
            controller.resolve_pending(TransitionDecision::Discard),
            Err(DiagramDocumentError::MermaidImport(MermaidImportError::NotFound(path)))
                if path == import_path
        ));
        assert_eq!(controller.session().unwrap().draft(), SUPPORTED);
        assert!(controller.is_dirty());
        assert_eq!(controller.pending_transition(), pending.as_ref());
    }
}
