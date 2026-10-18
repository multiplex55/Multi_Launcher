use crate::batch_rename::ExplicitDirectoryRoot;
use crate::file_operations::ScanCancellation;
use crate::replace_executor::{
    ConfirmedReplacementRequest, REPLACE_RECOVERY_DIRECTORY, ReplaceExecutor,
    ReplaceExecutorWorker, ReplaceOperationResult, ReplaceProgress, ReplaceRecoveryReceipt,
    ReplaceTransactionStatus, ReplaceWorkerSnapshot,
};
use crate::search_replace::{
    MatchId, ReplacementCandidate, ReplacementCandidateReadiness, ReplacementMode,
    ReplacementRevision, ReplacementSpec, SearchFilterSpec, SearchGeneration, SearchLimits,
    SearchPattern, SearchPatternMode, SearchProgress, SearchProgressPhase, SearchRevision,
    SearchStatus, TextSearchRequest, TextSearchResult,
};
use eframe::egui::{self, Align, Layout};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const WINDOW_ID: &str = "search_replace_preview_dialog";
const FILE_ROW_HEIGHT: f32 = 30.0;
const MATCH_ROW_HEIGHT: f32 = 56.0;
const MAX_VISIBLE_ISSUES: usize = 80;
const MAX_INSPECTED_MATCH_CHARS: usize = 4096;

#[cfg(test)]
#[derive(Clone, Debug)]
struct DialogGeometry {
    window: egui::Rect,
    header: egui::Rect,
    body: egui::Rect,
    footer: egui::Rect,
    content: egui::Vec2,
    body_id: Option<egui::Id>,
    offset: f32,
    files: egui::Rect,
    approvals: egui::Rect,
    pattern_editor: egui::Rect,
    replacement_editor: egui::Rect,
    settings_open: bool,
    advanced_open: bool,
}

#[cfg(test)]
impl Default for DialogGeometry {
    fn default() -> Self {
        Self {
            window: egui::Rect::NOTHING,
            header: egui::Rect::NOTHING,
            body: egui::Rect::NOTHING,
            footer: egui::Rect::NOTHING,
            content: egui::Vec2::ZERO,
            body_id: None,
            offset: 0.0,
            files: egui::Rect::NOTHING,
            approvals: egui::Rect::NOTHING,
            pattern_editor: egui::Rect::NOTHING,
            replacement_editor: egui::Rect::NOTHING,
            settings_open: false,
            advanced_open: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SearchReplaceUiCommand {
    SelectRoot(PathBuf),
    Search,
    CancelSearch,
    ReviewApply,
    ConfirmApply,
    CancelApply,
    ReviewRestore(String),
    ReviewRecovery(String),
    ConfirmRestore,
    ConfirmRecovery,
    CancelRecoveryAction,
    CancelOperation,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileSort {
    Path,
    MatchCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryAction {
    Restore,
    Recover,
}

#[derive(Debug, Clone)]
struct ApplyReviewFile {
    path: PathBuf,
    selected_lines: Vec<usize>,
    selected_matches: usize,
    source_length: u64,
    source_sha256: [u8; 32],
    output_length: u64,
    output_sha256: [u8; 32],
}

#[derive(Debug, Clone)]
struct ApplyReview {
    roots: Vec<PathBuf>,
    files: Vec<ApplyReviewFile>,
    approved_matches: usize,
    estimated_changed_bytes: u64,
    search_generation: SearchGeneration,
    replacement_revision: ReplacementRevision,
}

#[derive(Debug, Clone)]
struct RecoveryReview {
    transaction_id: String,
    action: RecoveryAction,
}

/// Bounded, coalescing background worker for candidate construction and the
/// final C confirmation boundary. It never reads or writes files itself.
pub(crate) struct ReplacementPreviewWorker {
    shared: Arc<(Mutex<PreviewWorkerState>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}

struct PreviewWorkerState {
    pending: Option<PreviewJob>,
    result: Option<PreviewResult>,
    active: bool,
    shutdown: bool,
}

pub(crate) enum PreviewJob {
    Compute {
        revision: u64,
        search: Arc<TextSearchResult>,
        replacement: ReplacementSpec,
        selected: Vec<MatchId>,
    },
    Confirm {
        revision: u64,
        candidate: ReplacementCandidate,
    },
}

enum PreviewResult {
    Candidate {
        revision: u64,
        candidate: Result<CandidateView, String>,
    },
    Confirmed {
        revision: u64,
        request: Result<ConfirmedReplacementRequest, String>,
    },
}

struct CandidateView {
    candidate: ReplacementCandidate,
    unified_diffs: HashMap<PathBuf, Arc<str>>,
}

impl ReplacementPreviewWorker {
    pub(crate) fn new() -> std::io::Result<Self> {
        let shared = Arc::new((
            Mutex::new(PreviewWorkerState {
                pending: None,
                result: None,
                active: false,
                shutdown: false,
            }),
            Condvar::new(),
        ));
        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("replace-preview-worker".into())
            .spawn(move || replacement_preview_loop(worker_shared))?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    pub(crate) fn submit(&self, job: PreviewJob) -> Result<(), String> {
        let (lock, wake) = &*self.shared;
        let mut state = lock
            .lock()
            .map_err(|_| "replacement preview worker state is unavailable".to_owned())?;
        if state.shutdown {
            return Err("replacement preview worker is shutting down".into());
        }
        state.pending = Some(job);
        state.result = None;
        wake.notify_one();
        Ok(())
    }

    fn take_result(&self) -> Option<PreviewResult> {
        let (lock, _) = &*self.shared;
        lock.lock().ok()?.result.take()
    }

    fn is_busy(&self) -> bool {
        self.shared
            .0
            .lock()
            .map(|state| state.active || state.pending.is_some())
            .unwrap_or(true)
    }

    pub(crate) fn shutdown(mut self) -> std::io::Result<()> {
        let (lock, wake) = &*self.shared;
        if let Ok(mut state) = lock.lock() {
            state.shutdown = true;
            state.pending = None;
            wake.notify_all();
        }
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| std::io::Error::other("replacement preview worker panicked"))?;
        }
        Ok(())
    }
}

impl Drop for ReplacementPreviewWorker {
    fn drop(&mut self) {
        let (lock, wake) = &*self.shared;
        if let Ok(mut state) = lock.lock() {
            state.shutdown = true;
            state.pending = None;
            wake.notify_all();
        }
    }
}

fn replacement_preview_loop(shared: Arc<(Mutex<PreviewWorkerState>, Condvar)>) {
    loop {
        let job = {
            let (lock, wake) = &*shared;
            let Ok(state) = lock.lock() else {
                return;
            };
            let mut state =
                match wake.wait_while(state, |state| state.pending.is_none() && !state.shutdown) {
                    Ok(state) => state,
                    Err(_) => return,
                };
            if state.shutdown {
                return;
            }
            let Some(job) = state.pending.take() else {
                continue;
            };
            state.result = None;
            state.active = true;
            job
        };

        let result = match job {
            PreviewJob::Compute {
                revision,
                search,
                replacement,
                selected,
            } => {
                let result = replacement
                    .compile(search.pattern())
                    .map_err(|error| error.to_string())
                    .and_then(|compiled| {
                        search
                            .preview_replacement(&compiled, &selected)
                            .map_err(|error| error.to_string())
                    })
                    .map(|candidate| {
                        let unified_diffs = candidate
                            .files()
                            .iter()
                            .filter_map(|file| {
                                let context = file.diff_context()?;
                                let snapshot = file.original_snapshot();
                                let diff = crate::diff::text_export::TextExportSnapshot::text(
                                    snapshot.display_path(),
                                    "proposed",
                                    context.original_excerpt(),
                                    context.candidate_excerpt(),
                                )
                                .render(crate::diff::text_export::TextExportFormat::UnifiedDiff);
                                Some((snapshot.path().to_path_buf(), Arc::<str>::from(diff)))
                            })
                            .collect();
                        CandidateView {
                            candidate,
                            unified_diffs,
                        }
                    });
                PreviewResult::Candidate {
                    revision,
                    candidate: result,
                }
            }
            PreviewJob::Confirm {
                revision,
                candidate,
            } => PreviewResult::Confirmed {
                revision,
                request: ConfirmedReplacementRequest::confirm(candidate)
                    .map_err(|error| error.to_string()),
            },
        };

        let (lock, _) = &*shared;
        let Ok(mut state) = lock.lock() else {
            return;
        };
        state.active = false;
        if state.shutdown {
            return;
        }
        state.result = Some(result);
    }
}

/// Reads retained C receipts off the UI thread. This is a read-only view of
/// the executor's journal; Apply, Restore, and Recovery stay in its worker.
pub(crate) struct RecoveryReceiptLoader {
    receiver: mpsc::Receiver<Result<Vec<ReplaceRecoveryReceipt>, String>>,
    worker: Option<JoinHandle<()>>,
}

impl RecoveryReceiptLoader {
    pub(crate) fn new(directory: PathBuf) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("replace-receipt-loader".into())
            .spawn(move || {
                let result = ReplaceExecutor::new(directory)
                    .and_then(|executor| executor.receipts())
                    .map_err(|error| error.to_string());
                let _ = sender.send(result);
            })?;
        Ok(Self {
            receiver,
            worker: Some(worker),
        })
    }

    fn try_take(&self) -> Option<Result<Vec<ReplaceRecoveryReceipt>, String>> {
        match self.receiver.try_recv() {
            Ok(receipts) => Some(receipts),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                "the recovery receipt reader stopped unexpectedly".into(),
            )),
        }
    }

    pub(crate) fn shutdown(mut self) -> std::io::Result<()> {
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| std::io::Error::other("recovery receipt reader panicked"))?;
        }
        Ok(())
    }
}

/// Dialog drafts and display state. A snapshots own search evidence, B owns
/// immutable candidate computation, and C owns all writes and durable receipts.
pub(crate) struct SearchReplaceDialogState {
    pub open: bool,
    root: Option<PathBuf>,
    recursive: bool,
    include_extensions: String,
    include_globs: String,
    exclude_paths: String,
    exclude_globs: String,
    pattern: String,
    pattern_mode: SearchPatternMode,
    case_sensitive: bool,
    replacement: String,
    replacement_mode: ReplacementMode,
    search_revision: u64,
    next_search_generation: u64,
    current_generation: Option<SearchGeneration>,
    search_ticket: Option<crate::search_replace::SearchTicket>,
    search_progress: Option<SearchProgress>,
    search_result: Option<Arc<TextSearchResult>>,
    selected_matches: HashSet<MatchId>,
    selected_file: Option<PathBuf>,
    file_filter: String,
    file_sort: FileSort,
    visible_file_indices: Vec<usize>,
    preview_revision: u64,
    preview_dirty: bool,
    preview_pending: bool,
    candidate: Option<ReplacementCandidate>,
    candidate_unified_diffs: HashMap<PathBuf, Arc<str>>,
    candidate_error: Option<String>,
    inspected_match: Option<MatchId>,
    pending_confirmed_request: Option<ConfirmedReplacementRequest>,
    apply_confirmation_open: bool,
    apply_confirmation_pending: bool,
    apply_review: Option<ApplyReview>,
    recovery_review: Option<RecoveryReview>,
    receipts: Vec<ReplaceRecoveryReceipt>,
    recovery_directory: Option<PathBuf>,
    receipt_loader: Option<RecoveryReceiptLoader>,
    receipt_load_pending: bool,
    status_message: Option<String>,
    reveal_receipts: bool,
    body_viewport_height: f32,
    #[cfg(test)]
    geometry: DialogGeometry,
    #[cfg(test)]
    preview_result_publications: usize,
}

impl Default for SearchReplaceDialogState {
    fn default() -> Self {
        Self {
            open: false,
            root: None,
            recursive: false,
            include_extensions: "rs, txt, md, toml, json".into(),
            include_globs: String::new(),
            exclude_paths: String::new(),
            exclude_globs: String::new(),
            pattern: String::new(),
            pattern_mode: SearchPatternMode::Literal,
            case_sensitive: false,
            replacement: String::new(),
            replacement_mode: ReplacementMode::Literal,
            search_revision: 0,
            next_search_generation: 1,
            current_generation: None,
            search_ticket: None,
            search_progress: None,
            search_result: None,
            selected_matches: HashSet::new(),
            selected_file: None,
            file_filter: String::new(),
            file_sort: FileSort::Path,
            visible_file_indices: Vec::new(),
            preview_revision: 0,
            preview_dirty: false,
            preview_pending: false,
            candidate: None,
            candidate_unified_diffs: HashMap::new(),
            candidate_error: None,
            inspected_match: None,
            pending_confirmed_request: None,
            apply_confirmation_open: false,
            apply_confirmation_pending: false,
            apply_review: None,
            recovery_review: None,
            receipts: Vec::new(),
            recovery_directory: None,
            receipt_loader: None,
            receipt_load_pending: false,
            status_message: None,
            reveal_receipts: false,
            body_viewport_height: 220.0,
            #[cfg(test)]
            geometry: DialogGeometry::default(),
            #[cfg(test)]
            preview_result_publications: 0,
        }
    }
}

impl SearchReplaceDialogState {
    pub(crate) fn open(&mut self) {
        self.open = true;
    }

    pub(crate) fn explicit_root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub(crate) fn has_pending_search(&self) -> bool {
        self.search_ticket.is_some()
    }

    pub(crate) fn set_recovery_directory(&mut self, directory: PathBuf) {
        self.recovery_directory = Some(directory);
    }

    pub(crate) fn start_receipt_load(&mut self) -> Result<(), String> {
        if self.receipt_loader.is_some() || !self.receipts.is_empty() || self.receipt_load_pending {
            return Ok(());
        }
        let Some(directory) = self.recovery_directory.clone() else {
            return Err("the application recovery directory is unavailable".into());
        };
        self.receipt_loader = Some(
            RecoveryReceiptLoader::new(directory)
                .map_err(|error| format!("could not read recovery receipts: {error}"))?,
        );
        self.receipt_load_pending = true;
        Ok(())
    }

    pub(crate) fn poll_background(
        &mut self,
        coordinator: Option<&crate::search_replace::SearchCoordinator>,
        preview_worker: Option<&ReplacementPreviewWorker>,
        replace_snapshot: Option<&ReplaceWorkerSnapshot>,
        replace_worker: Option<&ReplaceExecutorWorker>,
    ) -> bool {
        let mut changed = false;
        if let (Some(coordinator), Some(generation)) = (coordinator, self.current_generation)
            && let Some(progress) = coordinator.progress(generation)
        {
            self.search_progress = Some(progress);
        }
        if let Some(ticket) = self.search_ticket.as_ref() {
            match ticket.try_recv() {
                Ok(Some(result)) => {
                    self.search_ticket = None;
                    if self.current_generation == Some(result.generation()) {
                        self.accept_search_result(result);
                        changed = true;
                    }
                }
                Ok(None) => {}
                Err(_) => {
                    self.search_ticket = None;
                    self.status_message =
                        Some("The search worker stopped before returning a result.".into());
                    changed = true;
                }
            }
        }

        if self.receipt_load_pending
            && let Some(loader) = self.receipt_loader.as_ref()
            && let Some(result) = loader.try_take()
        {
            self.receipt_load_pending = false;
            match result {
                Ok(receipts) => self.merge_loaded_receipts(receipts),
                Err(error) => self.status_message = Some(error),
            }
            changed = true;
        }

        if let Some(preview_worker) = preview_worker {
            if let Some(result) = preview_worker.take_result() {
                self.accept_preview_result(result);
                changed = true;
            }
            if self.preview_dirty
                && let Some(job) = self.take_preview_job()
            {
                if let Err(error) = preview_worker.submit(job) {
                    self.preview_pending = false;
                    self.candidate_error = Some(error);
                }
                changed = true;
            }
            if preview_worker.is_busy() {
                changed = true;
            }
        }

        if let (Some(worker), Some(snapshot)) = (replace_worker, replace_snapshot) {
            if snapshot.result.is_some()
                && let Some(result) = worker.take_result()
            {
                self.accept_operation_result(result);
                changed = true;
            }
            if snapshot.active {
                changed = true;
            }
        }
        changed
    }

    pub(crate) fn has_background_work(
        &self,
        preview_worker: Option<&ReplacementPreviewWorker>,
        replace_operation_pending: bool,
    ) -> bool {
        self.search_ticket.is_some()
            || self.receipt_load_pending
            || self.preview_pending
            || preview_worker.is_some_and(ReplacementPreviewWorker::is_busy)
            || replace_operation_pending
    }

    pub(crate) fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    pub(crate) fn build_search_request(&mut self) -> Result<TextSearchRequest, String> {
        self.invalidate_search("Starting a new scoped search…".into());
        let root_path = self
            .root
            .as_ref()
            .ok_or_else(|| "Choose an explicit folder before searching.".to_owned())?;
        let root = ExplicitDirectoryRoot::new(root_path.clone())
            .map_err(|error| format!("The selected root is invalid: {error:?}"))?;
        let pattern =
            SearchPattern::compile(self.pattern.clone(), self.pattern_mode, self.case_sensitive)
                .map_err(|error| error.to_string())?;
        let filters = SearchFilterSpec {
            include_extensions: split_csv_or_lines(&self.include_extensions),
            include_path_globs: split_lines(&self.include_globs),
            exclude_paths: split_lines(&self.exclude_paths)
                .into_iter()
                .map(PathBuf::from)
                .collect(),
            exclude_path_globs: split_lines(&self.exclude_globs),
        };
        let revision = SearchRevision(self.search_revision.saturating_add(1));
        self.search_revision = revision.0;
        let generation = SearchGeneration(self.next_search_generation);
        self.next_search_generation = self.next_search_generation.saturating_add(1).max(1);
        self.current_generation = Some(generation);
        self.search_progress = None;
        self.status_message = Some("Searching only the selected root…".into());
        TextSearchRequest::new(
            vec![root],
            filters,
            pattern,
            revision,
            generation,
            SearchLimits::default(),
        )
        .map(|request| request.with_recursive(self.recursive))
        .map_err(|error| error.to_string())
    }

    pub(crate) fn record_search_ticket(&mut self, ticket: crate::search_replace::SearchTicket) {
        self.current_generation = Some(ticket.generation());
        self.search_ticket = Some(ticket);
        self.search_progress = None;
        self.status_message = Some("Search queued. No files have been changed.".into());
    }

    pub(crate) fn fail_search(&mut self, message: String) {
        self.search_ticket = None;
        self.search_result = None;
        self.candidate = None;
        self.candidate_unified_diffs.clear();
        self.inspected_match = None;
        self.preview_pending = false;
        self.preview_dirty = false;
        self.candidate_error = None;
        self.status_message = Some(message);
    }

    pub(crate) fn cancel_search(&mut self) {
        self.invalidate_search("Search canceled. No files were changed.".into());
    }

    pub(crate) fn set_root(&mut self, root: PathBuf) {
        self.invalidate_search("Root changed. Run Search to capture a new snapshot.".into());
        self.root = Some(root);
    }

    pub(crate) fn end_session(
        &mut self,
        coordinator: Option<&crate::search_replace::SearchCoordinator>,
        replace_worker: Option<&ReplaceExecutorWorker>,
    ) {
        self.open = false;
        self.reveal_receipts = false;
        self.cancel_current_search(coordinator);
        self.invalidate_search("The dialog was closed; Refresh before applying again.".into());
        if let Some(worker) = replace_worker {
            worker.cancel_active();
        }
        self.recovery_review = None;
        self.apply_confirmation_open = false;
        self.apply_confirmation_pending = false;
        self.apply_review = None;
        self.pending_confirmed_request = None;
    }

    pub(crate) fn cancel_current_search(
        &mut self,
        coordinator: Option<&crate::search_replace::SearchCoordinator>,
    ) {
        if let Some(generation) = self.current_generation
            && let Some(coordinator) = coordinator
        {
            coordinator.cancel_generation(generation);
        }
        if let Some(ticket) = self.search_ticket.take() {
            ticket.cancel();
        }
    }

    pub(crate) fn take_preview_job(&mut self) -> Option<PreviewJob> {
        if !self.preview_dirty {
            return None;
        }
        let search = self.search_result.as_ref()?.clone();
        self.preview_dirty = false;
        self.preview_pending = true;
        let selected = ordered_selected_ids(&search, &self.selected_matches);
        Some(PreviewJob::Compute {
            revision: self.preview_revision,
            search,
            replacement: ReplacementSpec::new(
                self.replacement.clone(),
                self.replacement_mode,
                ReplacementRevision(self.preview_revision),
            ),
            selected,
        })
    }

    pub(crate) fn take_confirm_job(&mut self) -> Option<PreviewJob> {
        if !self.apply_confirmation_open || self.apply_confirmation_pending {
            return None;
        }
        let candidate = self.candidate.take()?;
        self.apply_confirmation_pending = true;
        self.preview_pending = true;
        Some(PreviewJob::Confirm {
            revision: self.preview_revision,
            candidate,
        })
    }

    pub(crate) fn take_confirmed_request(&mut self) -> Option<ConfirmedReplacementRequest> {
        if !self.apply_confirmation_open || !self.apply_confirmation_pending {
            return None;
        }
        self.pending_confirmed_request.take()
    }

    pub(crate) fn confirm_request_submitted(&mut self) {
        self.apply_confirmation_open = false;
        self.apply_confirmation_pending = false;
        self.apply_review = None;
        self.search_result = None;
        self.selected_matches.clear();
        self.selected_file = None;
        self.candidate = None;
        self.candidate_unified_diffs.clear();
        self.inspected_match = None;
        self.preview_pending = false;
        self.preview_dirty = false;
        self.rebuild_visible_files();
        self.status_message = Some(
            "The reviewed batch was submitted. Progress and its recovery receipt will remain available if this dialog closes.".into(),
        );
    }

    pub(crate) fn confirm_request_failed(&mut self, message: String) {
        self.apply_confirmation_pending = false;
        self.apply_confirmation_open = false;
        self.apply_review = None;
        self.preview_pending = false;
        self.candidate_unified_diffs.clear();
        self.inspected_match = None;
        self.candidate_error = Some(message);
        self.preview_dirty = true;
        self.status_message = Some(
            "The reviewed candidate could not be frozen safely. Refresh and inspect the diagnostic before trying again.".into(),
        );
    }

    pub(crate) fn take_recovery_intent(&mut self) -> Option<(String, RecoveryAction)> {
        let review = self.recovery_review.take()?;
        Some((review.transaction_id, review.action))
    }

    pub(crate) fn review_recovery(&mut self, transaction_id: String, action: RecoveryAction) {
        let receipt = self
            .receipts
            .iter()
            .find(|receipt| receipt.id() == transaction_id);
        let status = receipt.map(ReplaceRecoveryReceipt::status);
        let allowed = match action {
            RecoveryAction::Restore => matches!(
                status,
                Some(ReplaceTransactionStatus::Applied | ReplaceTransactionStatus::Partial)
            ),
            RecoveryAction::Recover => status == Some(ReplaceTransactionStatus::RecoveryRequired),
        };
        if !allowed {
            self.status_message = Some(match action {
                RecoveryAction::Restore => {
                    "Restore is available only for an applied or partial receipt.".into()
                }
                RecoveryAction::Recover => {
                    "Recovery is available only for a recovery-required receipt.".into()
                }
            });
            return;
        }
        self.recovery_review = Some(RecoveryReview {
            transaction_id,
            action,
        });
    }

    pub(crate) fn take_receipt_loader(&mut self) -> Option<RecoveryReceiptLoader> {
        self.receipt_loader.take()
    }

    pub(crate) fn complete_recovery_submission(&mut self) {
        self.recovery_review = None;
        self.status_message = Some("The confirmed recovery operation was submitted.".into());
    }

    pub(crate) fn cancel_confirmation(&mut self) {
        self.apply_confirmation_open = false;
        self.apply_confirmation_pending = false;
        self.apply_review = None;
        self.pending_confirmed_request = None;
        if self.search_result.is_some() {
            self.queue_candidate_recompute();
        }
        self.recovery_review = None;
    }

    fn accept_search_result(&mut self, result: TextSearchResult) {
        if self.current_generation != Some(result.generation()) {
            return;
        }
        self.search_ticket = None;
        self.search_result = Some(Arc::new(result));
        self.selected_matches.clear();
        self.selected_file = None;
        self.inspected_match = None;
        self.status_message = None;
        self.rebuild_visible_files();
        self.queue_candidate_recompute();
    }

    fn accept_preview_result(&mut self, result: PreviewResult) {
        match result {
            PreviewResult::Candidate {
                revision,
                candidate,
            } if revision == self.preview_revision => {
                self.preview_pending = false;
                match candidate {
                    Ok(view)
                        if self.search_result.as_ref().is_some_and(|search| {
                            view.candidate.search_generation() == search.generation()
                                && view.candidate.search_revision() == search.revision()
                                && view.candidate.replacement_revision()
                                    == ReplacementRevision(self.preview_revision)
                        }) =>
                    {
                        self.candidate_error = None;
                        self.candidate_unified_diffs = view.unified_diffs;
                        self.candidate = Some(view.candidate);
                        #[cfg(test)]
                        {
                            self.preview_result_publications += 1;
                        }
                    }
                    Ok(_) => self.candidate_error = Some(
                        "A candidate completed for an old search or replacement revision and was discarded.".into(),
                    ),
                    Err(error) => self.candidate_error = Some(error),
                }
            }
            PreviewResult::Confirmed { revision, request }
                if revision == self.preview_revision
                    && self.apply_confirmation_open
                    && self.apply_confirmation_pending =>
            {
                self.preview_pending = false;
                match request {
                    Ok(request) => self.pending_confirmed_request = Some(request),
                    Err(error) => {
                        self.apply_confirmation_pending = false;
                        self.candidate_error = Some(error.clone());
                        self.status_message = Some(format!(
                            "The exact reviewed output could not be frozen: {error}"
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    fn accept_operation_result(&mut self, result: ReplaceOperationResult) {
        self.upsert_receipt(result.receipt);
        self.status_message = Some(result.message);
        self.apply_confirmation_open = false;
        self.apply_confirmation_pending = false;
        self.apply_review = None;
        self.recovery_review = None;
    }

    fn merge_loaded_receipts(&mut self, receipts: Vec<ReplaceRecoveryReceipt>) {
        for receipt in receipts {
            if !self
                .receipts
                .iter()
                .any(|existing| existing.id() == receipt.id())
            {
                self.receipts.push(receipt);
            }
        }
    }

    fn upsert_receipt(&mut self, receipt: ReplaceRecoveryReceipt) {
        if let Some(existing) = self
            .receipts
            .iter_mut()
            .find(|existing| existing.id() == receipt.id())
        {
            *existing = receipt;
        } else {
            self.receipts.push(receipt);
        }
    }

    fn invalidate_search(&mut self, message: String) {
        if let Some(ticket) = self.search_ticket.take() {
            ticket.cancel();
        }
        self.current_generation = None;
        self.search_progress = None;
        self.search_result = None;
        self.selected_matches.clear();
        self.selected_file = None;
        self.visible_file_indices.clear();
        self.candidate = None;
        self.candidate_unified_diffs.clear();
        self.inspected_match = None;
        self.candidate_error = None;
        self.preview_dirty = false;
        self.preview_pending = false;
        self.pending_confirmed_request = None;
        self.apply_confirmation_open = false;
        self.apply_confirmation_pending = false;
        self.apply_review = None;
        self.recovery_review = None;
        self.preview_revision = self.preview_revision.saturating_add(1);
        self.status_message = Some(message);
    }

    fn queue_candidate_recompute(&mut self) {
        self.preview_revision = self.preview_revision.saturating_add(1);
        self.candidate = None;
        self.candidate_unified_diffs.clear();
        self.candidate_error = None;
        self.preview_pending = self.search_result.is_some();
        self.preview_dirty = self.search_result.is_some();
        self.pending_confirmed_request = None;
        self.apply_confirmation_open = false;
        self.apply_confirmation_pending = false;
        self.apply_review = None;
    }

    fn set_match_selected(&mut self, id: &MatchId, selected: bool) {
        let changed = if selected {
            self.selected_matches.insert(id.clone())
        } else {
            self.selected_matches.remove(id)
        };
        if changed {
            self.queue_candidate_recompute();
        }
    }

    fn set_file_selected(&mut self, file_index: usize, selected: bool) {
        let Some(search) = self.search_result.as_ref() else {
            return;
        };
        let Some(file) = search.matched_files().get(file_index) else {
            return;
        };
        let ids = file
            .matches()
            .iter()
            .map(|span| span.id().clone())
            .collect::<Vec<_>>();
        let mut changed = false;
        for id in ids {
            changed |= if selected {
                self.selected_matches.insert(id)
            } else {
                self.selected_matches.remove(&id)
            };
        }
        if changed {
            self.queue_candidate_recompute();
        }
    }

    fn set_visible_files_selected(&mut self, selected: bool) {
        let indices = self.visible_file_indices.clone();
        let mut changed = false;
        for index in indices {
            let Some(file) = self
                .search_result
                .as_ref()
                .and_then(|search| search.matched_files().get(index))
            else {
                continue;
            };
            for span in file.matches() {
                changed |= if selected {
                    self.selected_matches.insert(span.id().clone())
                } else {
                    self.selected_matches.remove(span.id())
                };
            }
        }
        if changed {
            self.queue_candidate_recompute();
        }
    }

    fn rebuild_visible_files(&mut self) {
        let Some(search) = self.search_result.as_ref() else {
            self.visible_file_indices.clear();
            return;
        };
        let filter = self.file_filter.trim().to_lowercase();
        let mut indices = search
            .matched_files()
            .iter()
            .enumerate()
            .filter(|(_, file)| {
                filter.is_empty() || file.display_path().to_lowercase().contains(&filter)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        match self.file_sort {
            FileSort::Path => indices.sort_by(|left, right| {
                search.matched_files()[*left]
                    .path()
                    .cmp(search.matched_files()[*right].path())
            }),
            FileSort::MatchCount => indices.sort_by(|left, right| {
                search.matched_files()[*right]
                    .matches()
                    .len()
                    .cmp(&search.matched_files()[*left].matches().len())
                    .then_with(|| {
                        search.matched_files()[*left]
                            .path()
                            .cmp(search.matched_files()[*right].path())
                    })
            }),
        }
        self.visible_file_indices = indices;
    }

    fn build_apply_review(&self) -> Option<ApplyReview> {
        let candidate = self.candidate.as_ref()?;
        if candidate.readiness() != ReplacementCandidateReadiness::Ready {
            return None;
        }
        let search = self.search_result.as_ref()?;
        let mut roots = Vec::new();
        let mut files = Vec::new();
        for file in candidate.files() {
            if file.selected_match_ids().is_empty()
                || file.readiness() != crate::search_replace::ReplacementFileReadiness::Ready
            {
                continue;
            }
            let snapshot = file.original_snapshot();
            if !roots.iter().any(|root| root == snapshot.root()) {
                roots.push(snapshot.root().to_path_buf());
            }
            let selected_lines = snapshot
                .matches()
                .iter()
                .filter(|span| file.selected_match_ids().contains(span.id()))
                .map(|span| span.first_line())
                .collect::<Vec<_>>();
            files.push(ApplyReviewFile {
                path: snapshot.path().to_path_buf(),
                selected_lines,
                selected_matches: file.selected_match_ids().len(),
                source_length: snapshot.length(),
                source_sha256: snapshot.content_sha256(),
                output_length: file.output_bytes().len() as u64,
                output_sha256: file.output_sha256(),
            });
        }
        roots.sort();
        files.sort_by(|left, right| left.path.cmp(&right.path));
        Some(ApplyReview {
            roots,
            files,
            approved_matches: candidate.selected_match_ids().len(),
            estimated_changed_bytes: candidate.summary().estimated_changed_bytes(),
            search_generation: search.generation(),
            replacement_revision: candidate.replacement_revision(),
        })
    }

    fn draw_diff(&self, ui: &mut egui::Ui, file_index: usize) {
        let (Some(search), Some(candidate)) = (&self.search_result, &self.candidate) else {
            return;
        };
        let Some(snapshot) = search.matched_files().get(file_index) else {
            return;
        };
        let Some(file) = candidate
            .files()
            .iter()
            .find(|file| file.original_snapshot().path() == snapshot.path())
        else {
            return;
        };
        if let Some(diagnostic) = file.diagnostic() {
            ui.colored_label(ui.visuals().error_fg_color, diagnostic.message());
            return;
        }
        let Some(context) = file.diff_context() else {
            ui.weak("This file has no byte changes in the current candidate.");
            return;
        };
        if snapshot.encoding() == crate::search_replace::TextEncoding::Utf8Bom {
            ui.weak(
                "UTF-8 BOM is preserved as an original byte prefix; preview text starts after it.",
            );
        }
        ui.label(format!(
            "Bounded diff context begins near original byte {}. The full candidate remains exact.",
            context.first_change_byte_offset()
        ));
        if context.truncated() {
            ui.weak("This diff shows bounded context around the first change; other changes remain included in the exact candidate.");
        }
        ui.columns(2, |columns| {
            columns[0].label(egui::RichText::new("Original").strong());
            columns[1].label(egui::RichText::new("Proposed").strong());
            egui::ScrollArea::vertical()
                .id_salt(("replace_diff_original", snapshot.path()))
                .min_scrolled_height(self.panel_height(150.0))
                .max_height(self.panel_height(150.0))
                .show(&mut columns[0], |ui| {
                    ui.label(
                        egui::RichText::new(context.original_excerpt())
                            .monospace()
                            .color(ui.visuals().error_fg_color),
                    );
                });
            egui::ScrollArea::vertical()
                .id_salt(("replace_diff_candidate", snapshot.path()))
                .min_scrolled_height(self.panel_height(150.0))
                .max_height(self.panel_height(150.0))
                .show(&mut columns[1], |ui| {
                    ui.label(
                        egui::RichText::new(context.candidate_excerpt())
                            .monospace()
                            .color(ui.visuals().strong_text_color()),
                    );
                });
        });
        let Some(diff) = self.candidate_unified_diffs.get(snapshot.path()) else {
            ui.weak("The bounded unified diff is still being prepared on the preview worker.");
            return;
        };
        egui::ScrollArea::vertical()
            .id_salt(("replace_unified_diff", snapshot.path()))
            .min_scrolled_height(self.panel_height(125.0))
            .max_height(self.panel_height(125.0))
            .show(ui, |ui| {
                for line in diff.lines() {
                    let color = if line.starts_with('+') {
                        ui.visuals().strong_text_color()
                    } else if line.starts_with('-') {
                        ui.visuals().error_fg_color
                    } else {
                        ui.visuals().weak_text_color()
                    };
                    ui.colored_label(color, egui::RichText::new(line).monospace());
                }
            });
    }

    pub(crate) fn ui(
        &mut self,
        ctx: &egui::Context,
        allow_mutations: bool,
        runtime: &ReplaceWorkerSnapshot,
    ) -> Vec<SearchReplaceUiCommand> {
        let mut commands = Vec::new();
        if !self.open {
            return commands;
        }
        let process_interactions = allow_mutations
            && ctx.current_pass_index() == 0
            && !self.apply_confirmation_open
            && self.recovery_review.is_none();
        let mut window_open = self.open;
        let window = egui::Window::new("Search and Replace")
            .id(egui::Id::new(WINDOW_ID))
            .open(&mut window_open)
            .resizable(true)
            .default_size(egui::vec2(1120.0, 820.0))
            .min_size(egui::vec2(560.0, 360.0))
            .show(ctx, |ui| {
                let header_start = ui.cursor().min;
                self.draw_header(ui, &mut commands, process_interactions);
                ui.separator();
                let header = egui::Rect::from_min_max(header_start, ui.cursor().min + egui::vec2(ui.available_width(), 0.0));
                let row = ui.spacing().interact_size.y.max(ui.text_style_height(&egui::TextStyle::Body));
                // Include the ScrollArea's trailing item gap as well as the
                // separator and the two fixed rows; otherwise settled sizing
                // can grow the framed window below the short viewport.
                let footer_height = row * 2.0 + ui.spacing().item_spacing.y * 5.0;
                self.body_viewport_height = (ui.available_height() - footer_height).max(0.0);
                let body = egui::ScrollArea::vertical()
                    .id_salt("replace_dialog_body")
                    .min_scrolled_height(0.0)
                    .max_height(self.body_viewport_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        ui.weak("Choose a folder explicitly. Opening this dialog does not search; direct children are included by default.");
                        self.draw_search_fields(ui, &mut commands, process_interactions);
                        ui.separator();
                        self.draw_search_status(ui, &mut commands, process_interactions);
                        self.draw_results(ui, &mut commands, process_interactions);
                        ui.separator();
                        let receipts = ui.scope(|ui| self.draw_receipts(ui, &mut commands, process_interactions));
                        if self.reveal_receipts {
                            receipts.response.scroll_to_me_animation(
                                Some(Align::Min),
                                egui::style::ScrollAnimation::none(),
                            );
                            self.reveal_receipts = false;
                        }
                    });
                let footer_start = ui.cursor().min;
                ui.separator();
                self.draw_operation_status(ui, runtime, &mut commands, process_interactions);
                ui.horizontal(|ui| {
                    if ui.add_enabled(process_interactions, egui::Button::new("Close")).clicked() {
                        commands.push(SearchReplaceUiCommand::Closed);
                    }
                    if ui.add_enabled(process_interactions, egui::Button::new("Restore / Recovery")).clicked() {
                        self.reveal_receipts = true;
                        ctx.request_repaint();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let can_preview = process_interactions
                            && self.search_result.is_some()
                            && !self.preview_pending
                            && self.candidate.is_some();
                        if ui.add_enabled(can_preview, egui::Button::new("Review Apply\u{2026}")).clicked() {
                            if self.apply_review.is_none() { self.apply_review = self.build_apply_review(); }
                            if self.apply_review.is_some() {
                                self.apply_confirmation_open = true;
                                commands.push(SearchReplaceUiCommand::ReviewApply);
                            }
                        }
                    });
                });
                #[cfg(test)] {
                    self.geometry.header = header;
                    self.geometry.body = body.inner_rect;
                    self.geometry.footer = egui::Rect::from_min_max(footer_start, ui.min_rect().max);
                    self.geometry.content = body.content_size;
                    self.geometry.body_id = Some(body.id);
                    self.geometry.offset = body.state.offset.y;
                }
                #[cfg(not(test))] let _ = (header, body);
            });
        #[cfg(test)]
        if let Some(window) = window {
            self.geometry.window = window.response.rect;
        }
        #[cfg(not(test))]
        let _ = window;

        if allow_mutations
            && ctx.current_pass_index() == 0
            && (!window_open || commands.contains(&SearchReplaceUiCommand::Closed))
        {
            self.end_session(None, None);
            if !commands.contains(&SearchReplaceUiCommand::Closed) {
                commands.push(SearchReplaceUiCommand::Closed);
            }
            return commands;
        }

        self.draw_confirmations(ctx, runtime, &mut commands, allow_mutations);
        commands
    }

    fn draw_header(
        &mut self,
        ui: &mut egui::Ui,
        commands: &mut Vec<SearchReplaceUiCommand>,
        process_interactions: bool,
    ) {
        ui.horizontal(|ui| {
            let clicked = ui
                .add_enabled(
                    process_interactions,
                    egui::Button::new("Choose root folder\u{2026}"),
                )
                .clicked();
            if process_interactions && clicked {
                let mut choose = rfd::FileDialog::new();
                if let Some(root) = &self.root {
                    choose = choose.set_directory(root);
                }
                if let Some(root) = choose.pick_folder() {
                    commands.push(SearchReplaceUiCommand::SelectRoot(root));
                }
            }
            if ui
                .add_enabled(
                    process_interactions && self.root.is_some() && !self.pattern.is_empty(),
                    egui::Button::new("Search / Refresh"),
                )
                .clicked()
            {
                commands.push(SearchReplaceUiCommand::Search);
            }
            let root = self.root.as_ref().map_or_else(
                || "No root selected".into(),
                |root| root.display().to_string(),
            );
            ui.add(egui::Label::new(egui::RichText::new(&root).strong()).truncate())
                .on_hover_text(root);
        });
        let mut recursive = self.recursive;
        let response = ui.add_enabled(
            process_interactions,
            egui::Checkbox::new(&mut recursive, "Include subfolders"),
        );
        if process_interactions && response.changed() {
            self.recursive = recursive;
            self.invalidate_search("Search scope changed. Run Search to refresh.".into());
        }
    }

    fn draw_search_fields(
        &mut self,
        ui: &mut egui::Ui,
        _commands: &mut Vec<SearchReplaceUiCommand>,
        process_interactions: bool,
    ) {
        let mut search_changed = false;
        let mut replacement_changed = false;
        let active = [
            &self.include_extensions,
            &self.include_globs,
            &self.exclude_paths,
            &self.exclude_globs,
        ]
        .iter()
        .filter(|value| !value.trim().is_empty())
        .count();
        let settings = egui::CollapsingHeader::new(format!("Search and Replace settings ({active} filters active)"))
            .id_salt("replace_search_settings")
            .default_open(true)
            .show(ui, |ui| {
                ui.add_enabled_ui(process_interactions, |ui| {
                    ui.label("Search for");
                    let width = ui.available_width().min(520.0);
                    let pattern = ui.add(egui::TextEdit::singleline(&mut self.pattern)
                        .interactive(process_interactions)
                        .id(egui::Id::new("replace_search_pattern")).desired_width(width));
                    search_changed |= pattern.changed();
                    #[cfg(test)] { self.geometry.pattern_editor = pattern.rect; }
                    ui.horizontal_wrapped(|ui| {
                        egui::ComboBox::from_id_salt("replace_pattern_mode")
                            .selected_text(match self.pattern_mode { SearchPatternMode::Literal => "Literal", SearchPatternMode::RustRegex => "Rust regex" })
                            .show_ui(ui, |ui| {
                                search_changed |= ui.selectable_value(&mut self.pattern_mode, SearchPatternMode::Literal, "Literal").changed();
                                search_changed |= ui.selectable_value(&mut self.pattern_mode, SearchPatternMode::RustRegex, "Rust regex").changed();
                            });
                        search_changed |= ui.checkbox(&mut self.case_sensitive, "Case sensitive").changed();
                    });
                    ui.label("Replace with");
                    let replacement = ui.add(egui::TextEdit::singleline(&mut self.replacement)
                        .interactive(process_interactions)
                        .id(egui::Id::new("replace_replacement_text")).desired_width(width));
                    replacement_changed |= replacement.changed();
                    #[cfg(test)] { self.geometry.replacement_editor = replacement.rect; }
                    ui.horizontal_wrapped(|ui| {
                        egui::ComboBox::from_id_salt("replace_replacement_mode")
                            .selected_text(match self.replacement_mode { ReplacementMode::Literal => "Literal replacement", ReplacementMode::RustRegexCaptures => "Rust capture substitution" })
                            .show_ui(ui, |ui| {
                                replacement_changed |= ui.selectable_value(&mut self.replacement_mode, ReplacementMode::Literal, "Literal replacement").changed();
                                replacement_changed |= ui.selectable_value(&mut self.replacement_mode, ReplacementMode::RustRegexCaptures, "Rust capture substitution").changed();
                            });
                    });
                    ui.label("Capture substitutions use Rust regex syntax such as $1, ${name}, and $$; invalid references are shown before Apply.");
                });
                let advanced = egui::CollapsingHeader::new(format!("Advanced Filters ({active} active)"))
                    .id_salt("replace_advanced_filters")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.add_enabled_ui(process_interactions, |ui| {
                            for (label, id, value, hint) in [
                                ("Extensions", "replace_extensions", &mut self.include_extensions, "Comma separated; empty means all supported text extensions"),
                                ("Include globs", "replace_include_globs", &mut self.include_globs, "One root-relative path glob per line"),
                                ("Exclude paths", "replace_exclude_paths", &mut self.exclude_paths, "Root-relative file or folder paths, one per line"),
                                ("Exclude globs", "replace_exclude_globs", &mut self.exclude_globs, "One root-relative path glob per line"),
                            ] {
                                ui.label(label);
                                let width = ui.available_width().min(520.0);
                                search_changed |= ui.add(egui::TextEdit::singleline(value)
                                    .interactive(process_interactions)
                                    .id(egui::Id::new(id)).desired_width(width)).changed();
                                ui.weak(hint);
                            }
                        });
                    });
                #[cfg(test)] { self.geometry.advanced_open = advanced.body_response.is_some(); }
                #[cfg(not(test))] let _ = advanced;
            });
        #[cfg(test)]
        {
            self.geometry.settings_open = settings.body_response.is_some();
        }
        #[cfg(not(test))]
        let _ = settings;
        if search_changed {
            self.invalidate_search(
                "Search inputs changed. Run Search to capture a new result set.".into(),
            );
        } else if replacement_changed {
            self.queue_candidate_recompute();
        }
    }

    fn draw_search_status(
        &mut self,
        ui: &mut egui::Ui,
        _commands: &mut Vec<SearchReplaceUiCommand>,
        _process_interactions: bool,
    ) {
        ui.weak("Search uses the bounded A defaults for files, bytes, matches, and previews. Limit-triggered skips appear below, mark the result partial, and prevent Apply.");
        if let Some(message) = &self.status_message {
            ui.label(message);
        }
        if let Some(progress) = &self.search_progress {
            ui.horizontal(|ui| {
                if matches!(
                    progress.phase(),
                    SearchProgressPhase::Queued
                        | SearchProgressPhase::Discovering
                        | SearchProgressPhase::Reading
                        | SearchProgressPhase::Matching
                ) {
                    ui.spinner();
                }
                ui.label(format!(
                    "{:?} · roots {}/{} · files {} · matches {} · {} bytes",
                    progress.phase(),
                    progress.roots_completed(),
                    progress.roots_total(),
                    progress.files_examined(),
                    progress.matches_found(),
                    progress.bytes_read()
                ));
                if let Some(root) = progress.current_root() {
                    ui.weak(root.display().to_string());
                }
            });
        }
        if let Some(search) = &self.search_result {
            let summary = search.summary();
            let complete =
                summary.match_list_complete() && search.status() == SearchStatus::Complete;
            let label = format!(
                "{} · roots scanned {}/{} · files discovered {} · examined {} · searched {} · matched {} · matches {} · skipped {}",
                if complete {
                    "Complete result set"
                } else {
                    "Partial result set · Apply unavailable"
                },
                summary.roots_scanned(),
                summary.roots_scanned() + summary.roots_unfinished(),
                summary.files_discovered(),
                summary.files_examined(),
                summary.files_searched(),
                summary.files_matched(),
                summary.matches_found(),
                summary.files_skipped(),
            );
            ui.label(label);
            if !summary.issues().is_empty() || summary.issues_truncated() {
                ui.collapsing(
                    format!(
                        "Skipped files and scan issues · {} recorded{}",
                        summary.issue_count(),
                        if summary.issues_truncated() {
                            ", list truncated"
                        } else {
                            ""
                        }
                    ),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("replace_search_issues")
                            .max_height(110.0)
                            .show(ui, |ui| {
                                for issue in summary.issues().iter().take(MAX_VISIBLE_ISSUES) {
                                    ui.label(format!(
                                        "{:?} · {} · {}",
                                        issue.reason(),
                                        issue.path().map_or_else(
                                            || "(root)".to_owned(),
                                            |path| path.display().to_string()
                                        ),
                                        issue.message()
                                    ));
                                }
                                if summary.issues().len() > MAX_VISIBLE_ISSUES {
                                    ui.weak(format!(
                                        "{} additional listed issues are omitted from this panel.",
                                        summary.issues().len() - MAX_VISIBLE_ISSUES
                                    ));
                                }
                            });
                    },
                );
            }
        }
        if let Some(error) = &self.candidate_error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    }

    fn draw_results(
        &mut self,
        ui: &mut egui::Ui,
        _commands: &mut Vec<SearchReplaceUiCommand>,
        process_interactions: bool,
    ) {
        let Some(search) = self.search_result.as_ref().cloned() else {
            ui.weak("Search results appear here. A search only reads the chosen root and never edits files.");
            return;
        };
        let mut path_filter_changed = false;
        let mut sort_changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("Filter files");
            path_filter_changed |= ui
                .add_enabled(
                    process_interactions,
                    egui::TextEdit::singleline(&mut self.file_filter)
                        .id(egui::Id::new("replace_file_filter"))
                        .desired_width(250.0),
                )
                .changed();
            let mut sort = self.file_sort;
            ui.add_enabled_ui(process_interactions, |ui| {
                egui::ComboBox::from_id_salt("replace_file_sort")
                    .selected_text(match sort {
                        FileSort::Path => "Path order",
                        FileSort::MatchCount => "Match count",
                    })
                    .show_ui(ui, |ui| {
                        sort_changed |= ui
                            .selectable_value(&mut sort, FileSort::Path, "Path order")
                            .changed();
                        sort_changed |= ui
                            .selectable_value(&mut sort, FileSort::MatchCount, "Match count")
                            .changed();
                    });
            });
            if process_interactions && sort_changed {
                self.file_sort = sort;
            }
            let select = ui
                .add_enabled(
                    process_interactions,
                    egui::Button::new("Select visible matches"),
                )
                .clicked();
            let clear = ui
                .add_enabled(
                    process_interactions,
                    egui::Button::new("Clear visible matches"),
                )
                .clicked();
            if process_interactions && select {
                self.set_visible_files_selected(true);
            }
            if process_interactions && clear {
                self.set_visible_files_selected(false);
            }
        });
        if path_filter_changed || sort_changed {
            self.rebuild_visible_files();
        }

        let file_count = search.matched_files().len();
        let visible_count = self.visible_file_indices.len();
        ui.horizontal(|ui| {
            ui.label(format!(
                "Files with matches: {visible_count} shown of {file_count}"
            ));
            ui.label(format!(
                "Matches found: {}",
                search.summary().matches_found()
            ));
        });
        let files = egui::ScrollArea::vertical()
            .id_salt("replace_file_results")
            .auto_shrink([false, false])
            .min_scrolled_height(self.panel_height(220.0))
            .max_height(self.panel_height(220.0))
            .show_rows(ui, FILE_ROW_HEIGHT, visible_count, |ui, rows| {
                for visible_index in rows {
                    let Some(file_index) = self.visible_file_indices.get(visible_index).copied()
                    else {
                        continue;
                    };
                    let Some(file) = search.matched_files().get(file_index) else {
                        continue;
                    };
                    let total = file.matches().len();
                    let selected = file
                        .matches()
                        .iter()
                        .filter(|span| self.selected_matches.contains(span.id()))
                        .count();
                    let all_selected = total > 0 && selected == total;
                    let some_selected = selected > 0 && selected < total;
                    ui.horizontal(|ui| {
                        let mut checked = all_selected;
                        if ui
                            .add_enabled(
                                process_interactions,
                                egui::Checkbox::new(&mut checked, ""),
                            )
                            .clicked()
                        {
                            self.set_file_selected(file_index, !all_selected);
                        }
                        if some_selected {
                            ui.weak("some");
                        }
                        let active = self.selected_file.as_deref() == Some(file.path());
                        let label = format!(
                            "{}  ·  {} match{}  ·  {}/{} approved",
                            file.display_path(),
                            total,
                            if total == 1 { "" } else { "es" },
                            selected,
                            total
                        );
                        let label = bounded_single_line(&label, 220);
                        if ui
                            .add_enabled(
                                process_interactions,
                                egui::Button::selectable(active, label)
                                    .wrap_mode(egui::TextWrapMode::Truncate),
                            )
                            .clicked()
                        {
                            self.selected_file = Some(file.path().to_path_buf());
                            self.inspected_match = None;
                        }
                    });
                }
            });

        #[cfg(test)]
        {
            self.geometry.files = files.inner_rect;
        }
        #[cfg(not(test))]
        let _ = files;

        let selected_file_index = self.selected_file.as_ref().and_then(|path| {
            search
                .matched_files()
                .iter()
                .position(|file| file.path() == path)
        });
        let Some(file_index) = selected_file_index else {
            ui.weak("Select a file row to expand its stable per-match approvals and diff.");
            self.draw_candidate_summary(ui);
            return;
        };
        let file = &search.matched_files()[file_index];
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(file.display_path()).strong());
            ui.label(format!(
                "{} source match{} · stable IDs from generation {}",
                file.matches().len(),
                if file.matches().len() == 1 { "" } else { "es" },
                search.generation().0
            ));
        });
        ui.weak("File approval is derived from these stable match IDs. Filtering and sorting only change what is shown.");
        let approvals = egui::ScrollArea::vertical()
            .id_salt(("replace_match_rows", file.path(), search.generation().0))
            .auto_shrink([false, false])
            .min_scrolled_height(self.panel_height(190.0))
            .max_height(self.panel_height(190.0))
            .show_rows(ui, MATCH_ROW_HEIGHT, file.matches().len(), |ui, rows| {
                for match_index in rows {
                    let Some(span) = file.matches().get(match_index) else {
                        continue;
                    };
                    let id = span.id();
                    let mut checked = self.selected_matches.contains(id);
                    ui.push_id(id, |ui| {
                        ui.horizontal(|ui| {
                            let response = ui.add_enabled(
                                process_interactions,
                                egui::Checkbox::new(&mut checked, ""),
                            );
                            if response.clicked() {
                                self.set_match_selected(id, checked);
                            }
                            ui.vertical(|ui| {
                                ui.add(
                                    egui::Label::new(format!(
                                        "line {}{} · match {} · byte range {:?}",
                                        span.first_line(),
                                        if span.last_line() != span.first_line() {
                                            format!("–{}", span.last_line())
                                        } else {
                                            String::new()
                                        },
                                        id.ordinal() + 1,
                                        span.raw_byte_range()
                                    ))
                                    .truncate(),
                                );
                                let text = format!(
                                    "{}⟦{}⟧{}",
                                    span.before_context(),
                                    span.matched_preview(),
                                    span.after_context()
                                );
                                let text = bounded_single_line(&text, 320);
                                let response = ui.add_enabled(
                                    process_interactions,
                                    egui::Button::selectable(
                                        self.inspected_match.as_ref() == Some(id),
                                        egui::RichText::new(text).monospace(),
                                    )
                                    .wrap_mode(egui::TextWrapMode::Truncate),
                                );
                                if process_interactions && response.clicked() {
                                    self.inspected_match = Some(id.clone());
                                }
                            });
                        });
                    });
                }
            });
        ui.label("Select a match row to inspect its exact captured and proposed text. Match selection controls approval; previews use the captured B candidate.");
        #[cfg(test)]
        {
            self.geometry.approvals = approvals.inner_rect;
        }
        #[cfg(not(test))]
        let _ = approvals;
        self.draw_diff(ui, file_index);
        if let Some(span) = file
            .matches()
            .iter()
            .find(|span| self.inspected_match.as_ref() == Some(span.id()))
        {
            self.draw_match_inspector(ui, file.path(), span);
        } else {
            ui.weak("Select a match row to inspect its original and proposed text. Long values are bounded here and disclose the omitted character count.");
        }
        self.draw_candidate_summary(ui);
    }

    fn draw_match_inspector(
        &self,
        ui: &mut egui::Ui,
        path: &Path,
        span: &crate::search_replace::MatchSpan,
    ) {
        let Some(candidate) = self.candidate.as_ref().and_then(|candidate| {
            candidate
                .files()
                .iter()
                .find(|file| file.original_snapshot().path() == path)
        }) else {
            ui.weak("The current candidate is still being computed for this match.");
            return;
        };
        let Some(output) = candidate.match_output(span.id()) else {
            ui.weak("Select this match to compute its proposed text. The unselected match has no proposed output.");
            return;
        };
        ui.collapsing(
            format!("Inspect match {} · {}", span.id().ordinal() + 1, if output.changed() { "changes output" } else { "no-op" }),
            |ui| {
                ui.columns(2, |columns| {
                    let (original, original_truncated) =
                        bounded_text(output.original_text(), MAX_INSPECTED_MATCH_CHARS);
                    let (proposed, proposed_truncated) =
                        bounded_text(output.proposed_text(), MAX_INSPECTED_MATCH_CHARS);
                    columns[0].label(egui::RichText::new("Original").strong());
                    columns[1].label(egui::RichText::new("Proposed").strong());
                    columns[0].weak(format!("{} bytes", output.original_text().len()));
                    columns[1].weak(format!("{} bytes", output.proposed_text().len()));
                    egui::ScrollArea::vertical()
                        .id_salt(("replace_match_original", path, span.id()))
                        .min_scrolled_height(self.panel_height(130.0))
            .max_height(self.panel_height(130.0))
                        .show(&mut columns[0], |ui| {
                            ui.label(egui::RichText::new(original).monospace());
                            if original_truncated {
                                ui.weak(format!("Preview truncated after {MAX_INSPECTED_MATCH_CHARS} characters."));
                            }
                        });
                    egui::ScrollArea::vertical()
                        .id_salt(("replace_match_proposed", path, span.id()))
                        .min_scrolled_height(self.panel_height(130.0))
            .max_height(self.panel_height(130.0))
                        .show(&mut columns[1], |ui| {
                            ui.label(egui::RichText::new(proposed).monospace());
                            if proposed_truncated {
                                ui.weak(format!("Preview truncated after {MAX_INSPECTED_MATCH_CHARS} characters; the exact candidate remains retained for confirmation."));
                            }
                        });
                });
            },
        );
    }

    fn draw_candidate_summary(&self, ui: &mut egui::Ui) {
        if self.preview_pending {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Computing exact candidate and bounded diff on the preview worker…");
            });
        }
        let Some(candidate) = &self.candidate else {
            return;
        };
        let summary = candidate.summary();
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Candidate: {:?}", candidate.readiness()));
            ui.label(format!("affected files {}", summary.affected_files()));
            ui.label(format!("approved matches {}", summary.approved_matches()));
            ui.label(format!("changed matches {}", summary.changed_matches()));
            ui.label(format!(
                "selected no-op matches {}",
                summary.selected_noop_matches()
            ));
            ui.label(format!(
                "estimated changed bytes {}",
                summary.estimated_changed_bytes()
            ));
            if summary.failed_files() > 0 {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("{} file candidate(s) blocked", summary.failed_files()),
                );
            }
        });
        if !candidate.discovery_complete() {
            ui.colored_label(
                ui.visuals().error_fg_color,
                "Discovery was partial. This candidate cannot be applied as a complete result set.",
            );
        }
        if candidate.readiness() == ReplacementCandidateReadiness::NoOp {
            ui.weak("No selected match changes output bytes. Apply remains disabled.");
        }
    }

    fn panel_height(&self, preferred: f32) -> f32 {
        (self.body_viewport_height * 0.4).clamp(48.0, preferred)
    }

    fn draw_operation_status(
        &mut self,
        ui: &mut egui::Ui,
        runtime: &ReplaceWorkerSnapshot,
        commands: &mut Vec<SearchReplaceUiCommand>,
        process_interactions: bool,
    ) {
        ui.horizontal(|ui| {
            if runtime.active {
                if ui
                    .add_enabled(
                        process_interactions,
                        egui::Button::new("Cancel between files"),
                    )
                    .clicked()
                {
                    commands.push(SearchReplaceUiCommand::CancelOperation);
                }
                ui.spinner();
            }
            if self.search_ticket.is_some()
                && ui
                    .add_enabled(process_interactions, egui::Button::new("Cancel search"))
                    .clicked()
            {
                commands.push(SearchReplaceUiCommand::CancelSearch);
            }
            let message = if runtime.active {
                runtime.progress.as_ref().map_or_else(
                    || {
                        "Validating the frozen request and preparing guarded file operations…"
                            .into()
                    },
                    |progress| {
                        format!(
                            "{:?} · {} · {}/{} files",
                            progress.phase, progress.message, progress.completed, progress.total
                        )
                    },
                )
            } else if runtime.shutdown_requested {
                "The replace worker is shutting down; recovery data remains in application data."
                    .into()
            } else {
                self.status_message
                    .clone()
                    .unwrap_or_else(|| "No replacement operation is active.".into())
            };
            ui.add(egui::Label::new(&message).truncate())
                .on_hover_text(message);
        });
    }

    fn draw_receipts(
        &mut self,
        ui: &mut egui::Ui,
        commands: &mut Vec<SearchReplaceUiCommand>,
        process_interactions: bool,
    ) {
        if self.receipt_load_pending {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Loading durable replacement recovery receipts…");
            });
        }
        if self.receipts.is_empty() && !self.receipt_load_pending {
            ui.weak("No saved replacement receipts are available.");
            return;
        }
        egui::CollapsingHeader::new("Replacement receipts, Restore, and Recovery")
            .id_salt("replace_receipts")
            .open(self.reveal_receipts.then_some(true))
            .show(ui, |ui| {
                for receipt in self.receipts.iter().rev() {
                    ui.collapsing(
                        format!(
                            "{} · {:?} · {} file entries",
                            receipt.id(),
                            receipt.status(),
                            receipt.entries().len()
                        ),
                        |ui| {
                            ui.label(receipt.message());
                            let roots = receipt
                                .entries()
                                .iter()
                                .map(|entry| {
                                    entry
                                        .path()
                                        .parent()
                                        .unwrap_or(Path::new("."))
                                        .to_path_buf()
                                })
                                .collect::<HashSet<_>>();
                            ui.label(format!("Recorded parent folders: {}", roots.len()));
                            let can_restore = matches!(
                                receipt.status(),
                                ReplaceTransactionStatus::Applied
                                    | ReplaceTransactionStatus::Partial
                            );
                            let can_recover =
                                receipt.status() == ReplaceTransactionStatus::RecoveryRequired;
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(
                                        process_interactions && can_restore,
                                        egui::Button::new("Restore…"),
                                    )
                                    .clicked()
                                {
                                    commands.push(SearchReplaceUiCommand::ReviewRestore(
                                        receipt.id().to_owned(),
                                    ));
                                }
                                if ui
                                    .add_enabled(
                                        process_interactions && can_recover,
                                        egui::Button::new("Recover interrupted operation…"),
                                    )
                                    .clicked()
                                {
                                    commands.push(SearchReplaceUiCommand::ReviewRecovery(
                                        receipt.id().to_owned(),
                                    ));
                                }
                            });
                            egui::ScrollArea::vertical()
                                .id_salt(("replace_receipt_entries", receipt.id()))
                                .min_scrolled_height(self.panel_height(210.0))
                                .max_height(self.panel_height(210.0))
                                .show(ui, |ui| {
                                    for entry in receipt.entries() {
                                        ui.label(format!(
                                            "{} · {:?}",
                                            entry.path().display(),
                                            entry.outcome()
                                        ));
                                    }
                                });
                        },
                    );
                }
            });
        if let Some(directory) = &self.recovery_directory {
            ui.weak(format!(
                "Durable backups and the receipt journal are retained under {}.",
                directory.display()
            ));
        } else if self.receipt_load_pending {
            ui.weak(format!(
                "Recovery directory name: {REPLACE_RECOVERY_DIRECTORY}"
            ));
        }
    }

    fn draw_confirmations(
        &mut self,
        ctx: &egui::Context,
        runtime: &ReplaceWorkerSnapshot,
        commands: &mut Vec<SearchReplaceUiCommand>,
        allow_mutations: bool,
    ) {
        let process_interactions = allow_mutations && ctx.current_pass_index() == 0;
        if self.apply_confirmation_open {
            let Some(review) = self.apply_review.as_ref() else {
                if process_interactions {
                    self.apply_confirmation_open = false;
                }
                return;
            };
            let mut open = true;
            let mut confirm = false;
            let mut cancel = false;
            egui::Window::new("Confirm Search and Replace")
                .id(egui::Id::new("replace_apply_confirmation"))
                .collapsible(false)
                .resizable(true)
                .default_size(egui::vec2(760.0, 560.0))
                .min_size(egui::vec2(480.0, 320.0))
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label("This confirmation freezes the exact candidate bytes and approved match IDs. Search and preview never write files.");
                    for root in &review.roots {
                        ui.label(format!("Root: {}", root.display()));
                    }
                    ui.label(format!(
                        "Files: {} · approved matches: {} · estimated changed bytes: {} · search generation: {} · replacement revision: {}",
                        review.files.len(),
                        review.approved_matches,
                        review.estimated_changed_bytes,
                        review.search_generation.0,
                        review.replacement_revision.0
                    ));
                    ui.label("Apply rechecks each source against the captured identity and bytes. A stale file is skipped; its replacement is never recomputed automatically.");
                    ui.label("A recovery copy and durable receipt are recorded before each source changes. Cancellation and failures can leave a partial batch. Restore only proceeds while the written files still match their recorded post-write identities.");
                    if let Some(directory) = &self.recovery_directory {
                        ui.label(format!("Recovery storage: {}", directory.display()));
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("replace_apply_review_files")
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for file in &review.files {
                                ui.group(|ui| {
                                    ui.label(file.path.display().to_string());
                                    ui.label(format!(
                                        "{} approved match{} on line(s) {} · source {} bytes · proposed {} bytes",
                                        file.selected_matches,
                                        if file.selected_matches == 1 { "" } else { "es" },
                                        file.selected_lines
                                            .iter()
                                            .map(usize::to_string)
                                            .collect::<Vec<_>>()
                                            .join(", "),
                                        file.source_length,
                                        file.output_length
                                    ));
                                    ui.weak(format!(
                                        "Captured source SHA-256 {:02x?} · proposed SHA-256 {:02x?}",
                                        file.source_sha256,
                                        file.output_sha256
                                    ));
                                });
                            }
                        });
                    if self.apply_confirmation_pending {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Freezing the reviewed candidate on a worker before submitting it to the guarded executor…");
                            cancel = ui
                                .add_enabled(
                                    process_interactions,
                                    egui::Button::new("Cancel before submission"),
                                )
                                .clicked();
                        });
                    } else {
                        ui.horizontal(|ui| {
                            confirm = ui
                                .add_enabled(
                                    process_interactions
                                        && !runtime.active
                                        && self.candidate.is_some()
                                        && !self.preview_pending,
                                    egui::Button::new("Confirm and Apply"),
                                )
                                .clicked();
                            cancel = ui
                                .add_enabled(process_interactions, egui::Button::new("Cancel"))
                                .clicked();
                        });
                    }
                });
            if confirm {
                commands.push(SearchReplaceUiCommand::ConfirmApply);
            } else if process_interactions && (cancel || !open) {
                self.cancel_confirmation();
                commands.push(SearchReplaceUiCommand::CancelApply);
            }
        }
        if let Some(review) = self.recovery_review.as_ref() {
            let mut open = true;
            let mut confirm = false;
            let mut cancel = false;
            let title = match review.action {
                RecoveryAction::Restore => "Confirm replacement Restore",
                RecoveryAction::Recover => "Confirm replacement Recovery",
            };
            egui::Window::new(title)
                .id(egui::Id::new("replace_recovery_confirmation"))
                .collapsible(false)
                .resizable(true)
                .default_size(egui::vec2(620.0, 420.0))
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(format!("Receipt: {}", review.transaction_id));
                    match review.action {
                        RecoveryAction::Restore => {
                            ui.label("Restore checks the recorded post-write identities and restores only files that still match. Edited or replaced files are refused and left untouched.");
                        }
                        RecoveryAction::Recover => {
                            ui.label("Recovery reconciles only the operation's recorded staging names and identities. Conflicting occupants are retained and reported; the GUI never copies over or force-overwrites a file.");
                        }
                    }
                    ui.label("Per-file outcomes and any partial or recovery-required result will remain in this receipt.");
                    if ui
                        .add_enabled(process_interactions && !runtime.active, egui::Button::new(match review.action {
                            RecoveryAction::Restore => "Confirm Restore",
                            RecoveryAction::Recover => "Confirm Recovery",
                        }))
                        .clicked()
                    {
                        confirm = true;
                    }
                    cancel = ui
                        .add_enabled(process_interactions, egui::Button::new("Cancel"))
                        .clicked();
                });
            if confirm {
                commands.push(match review.action {
                    RecoveryAction::Restore => SearchReplaceUiCommand::ConfirmRestore,
                    RecoveryAction::Recover => SearchReplaceUiCommand::ConfirmRecovery,
                });
            } else if process_interactions && (cancel || !open) {
                self.recovery_review = None;
                commands.push(SearchReplaceUiCommand::CancelRecoveryAction);
            }
        }
    }

    fn queue_candidate_recompute_for_test(&mut self) {
        self.queue_candidate_recompute();
    }
}

fn ordered_selected_ids(search: &TextSearchResult, selected: &HashSet<MatchId>) -> Vec<MatchId> {
    search
        .matched_files()
        .iter()
        .flat_map(|file| file.matches())
        .filter(|span| selected.contains(span.id()))
        .map(|span| span.id().clone())
        .collect()
}

fn split_lines(value: &str) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn bounded_single_line(value: &str, max_chars: usize) -> String {
    let mut output = String::new();
    let mut truncated = false;
    for (index, character) in value.chars().enumerate() {
        if index == max_chars {
            truncated = true;
            break;
        }
        output.extend(character.escape_debug());
    }
    if truncated {
        output.push('…');
    }
    output
}

fn bounded_text(value: &str, max_chars: usize) -> (String, bool) {
    let mut characters = value.chars();
    let mut output = characters.by_ref().take(max_chars).collect::<String>();
    let truncated = characters.next().is_some();
    if truncated {
        output.push('…');
    }
    (output, truncated)
}

fn split_csv_or_lines(value: &str) -> Vec<String> {
    value
        .split([',', '\n'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn layout_frame(
        state: &mut SearchReplaceDialogState,
        ctx: &egui::Context,
        size: egui::Vec2,
        runtime: &ReplaceWorkerSnapshot,
        events: Vec<egui::Event>,
        allow: bool,
    ) -> (Vec<SearchReplaceUiCommand>, Vec<(String, egui::Rect)>) {
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        });
        let commands = state.ui(ctx, allow, runtime);
        let output = ctx.end_pass();
        fn collect(
            shape: &egui::epaint::Shape,
            clip: egui::Rect,
            text: &mut Vec<(String, egui::Rect)>,
        ) {
            match shape {
                egui::epaint::Shape::Text(value) => {
                    let rect = value
                        .galley
                        .rect
                        .translate(value.pos.to_vec2())
                        .intersect(clip);
                    if rect.is_positive() {
                        text.push((value.galley.text().into(), rect));
                    }
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, clip, text);
                    }
                }
                _ => {}
            }
        }
        let mut text = Vec::new();
        for shape in &output.shapes {
            collect(&shape.shape, shape.clip_rect, &mut text);
        }
        output.drop_without_applying_deltas();
        (commands, text)
    }

    fn click_body_label(
        state: &mut SearchReplaceDialogState,
        ctx: &egui::Context,
        size: egui::Vec2,
        label: &str,
    ) {
        let runtime = ReplaceWorkerSnapshot::default();
        let id = state.geometry.body_id.unwrap();
        let mut scroll = egui::scroll_area::State::load(ctx, id).unwrap();
        scroll.offset.y = 0.0;
        scroll.store(ctx, id);
        let mut position = None;
        for _ in 0..35 {
            let (commands, text) = layout_frame(state, ctx, size, &runtime, vec![], true);
            assert!(commands.is_empty());
            if let Some((_, rect)) = text.iter().find(|(text, _)| text.starts_with(label)) {
                position = Some(rect.center());
                break;
            }
            let mut scroll = egui::scroll_area::State::load(ctx, id).unwrap();
            scroll.offset.y += 30.0;
            scroll.store(ctx, id);
        }
        let pos = position.unwrap_or_else(|| panic!("missing body label {label}"));
        for pressed in [true, false] {
            assert!(
                layout_frame(
                    state,
                    ctx,
                    size,
                    &runtime,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        }
                    ],
                    true
                )
                .0
                .is_empty()
            );
        }
        for _ in 0..3 {
            layout_frame(state, ctx, size, &runtime, vec![], true);
        }
    }

    #[test]
    fn replace_settings_folding_preserves_snapshot_and_edits_keep_invalidation_boundaries() {
        let parent = PathBuf::from("target/refinement-evidence");
        std::fs::create_dir_all(&parent).unwrap();
        let fixture = tempfile::Builder::new()
            .prefix("replace_filters_")
            .tempdir_in(parent)
            .unwrap();
        let root = fixture.path().join("explicit-root");
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("owned.txt");
        std::fs::write(&source, "needle\nneedle\n").unwrap();
        let mut state = SearchReplaceDialogState::default();
        state.open = true;
        state.set_root(root);
        state.pattern = "needle".into();
        state.replacement = "replacement".into();
        state.include_extensions = "txt".into();
        let coordinator = crate::search_replace::SearchCoordinator::new().unwrap();
        let worker = ReplacementPreviewWorker::new().unwrap();
        let request = state.build_search_request().unwrap();
        state.record_search_ticket(coordinator.submit(request).unwrap());
        wait_for_preview(&mut state, &coordinator, &worker, |state| {
            state.candidate.is_some() && !state.preview_pending
        });
        let search = state.search_result.clone().unwrap();
        assert_eq!(search.status(), SearchStatus::Complete);
        assert_eq!(search.matched_files().len(), 1);
        let id = search.matched_files()[0].matches()[0].id().clone();
        state.set_match_selected(&id, true);
        wait_for_preview(&mut state, &coordinator, &worker, |state| {
            state.candidate.is_some() && !state.preview_pending
        });
        state.apply_review = state.build_apply_review();
        assert!(state.apply_review.is_some());
        state.receipts.push(serde_json::from_value(serde_json::json!({
            "id": "owned-receipt", "search_revision": 1, "search_generation": 1,
            "replacement_revision": 1, "status": "applied", "entries": [], "message": "owned retained receipt"
        })).unwrap());
        let receipts = state.receipts.clone();
        let selected = state.selected_matches.clone();
        let before = (
            state.search_revision,
            state.preview_revision,
            state.preview_result_publications,
        );
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let size = egui::vec2(600.0, 420.0);
        let runtime = ReplaceWorkerSnapshot::default();
        for _ in 0..4 {
            layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
        }
        assert!(state.geometry.settings_open && !state.geometry.advanced_open);
        for rect in [
            state.geometry.pattern_editor,
            state.geometry.replacement_editor,
        ] {
            assert!(
                rect.width() >= 160.0 && rect.right() <= state.geometry.window.right(),
                "responsive field: {rect:?}"
            );
        }
        let drafts = (
            state.pattern.clone(),
            state.replacement.clone(),
            state.include_extensions.clone(),
        );
        click_body_label(&mut state, &ctx, size, "Advanced Filters");
        assert!(state.geometry.advanced_open);
        click_body_label(&mut state, &ctx, size, "Advanced Filters");
        assert!(!state.geometry.advanced_open);
        click_body_label(&mut state, &ctx, size, "Search and Replace settings");
        assert!(!state.geometry.settings_open);
        click_body_label(&mut state, &ctx, size, "Search and Replace settings");
        assert!(state.geometry.settings_open);
        assert!(Arc::ptr_eq(state.search_result.as_ref().unwrap(), &search));
        assert_eq!(state.selected_matches, selected);
        assert_eq!(state.receipts, receipts);
        assert!(state.apply_review.is_some());
        assert_eq!(
            (
                state.search_revision,
                state.preview_revision,
                state.preview_result_publications
            ),
            before
        );
        assert_eq!(
            (
                state.pattern.clone(),
                state.replacement.clone(),
                state.include_extensions.clone()
            ),
            drafts
        );

        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("replace_replacement_text")));
        assert!(
            layout_frame(
                &mut state,
                &ctx,
                size,
                &runtime,
                vec![egui::Event::Text("ignored".into())],
                false
            )
            .0
            .is_empty()
        );
        assert_eq!(state.replacement, drafts.1);
        assert_eq!(state.preview_revision, before.1);
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("replace_replacement_text")));
        assert!(
            layout_frame(
                &mut state,
                &ctx,
                size,
                &runtime,
                vec![egui::Event::Text("!".into())],
                true
            )
            .0
            .is_empty()
        );
        assert_ne!(state.replacement, drafts.1);
        assert!(Arc::ptr_eq(state.search_result.as_ref().unwrap(), &search));
        assert_eq!(state.selected_matches, selected);
        assert_eq!(state.preview_revision, before.1 + 1);
        assert!(state.preview_dirty && state.candidate.is_none());
        assert!(
            state.search_ticket.is_none(),
            "editing does not launch a worker"
        );

        click_body_label(&mut state, &ctx, size, "Advanced Filters");
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("replace_exclude_globs")));
        assert!(
            layout_frame(
                &mut state,
                &ctx,
                size,
                &runtime,
                vec![egui::Event::Text("**/skip/**".into())],
                true
            )
            .0
            .is_empty()
        );
        assert_eq!(state.exclude_globs, "**/skip/**");
        assert!(state.search_result.is_none() && state.current_generation.is_none());
        assert_eq!(state.preview_revision, before.1 + 2);
        assert!(state.search_ticket.is_none());
        click_body_label(&mut state, &ctx, size, "Advanced Filters");
        assert!(!state.geometry.advanced_open);
        let (_, text) = layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
        assert!(
            text.iter()
                .any(|(text, _)| text == "Advanced Filters (2 active)")
        );
        click_body_label(&mut state, &ctx, size, "Advanced Filters");
        assert!(state.geometry.advanced_open);
        assert_eq!(state.exclude_globs, "**/skip/**");
        assert_eq!(state.receipts, receipts);
        assert_eq!(std::fs::read_to_string(source).unwrap(), "needle\nneedle\n");
    }

    #[test]
    fn replace_dialog_bounded_body_keeps_actions_and_nested_viewports_reachable() {
        let fixture_parent = PathBuf::from("target/refinement-evidence");
        std::fs::create_dir_all(&fixture_parent).unwrap();
        let fixture = tempfile::Builder::new()
            .prefix("replace_layout_")
            .tempdir_in(&fixture_parent)
            .unwrap();
        let root = fixture.path().join("explicit-root");
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("owned.txt");
        let original = "needle\n".repeat(20);
        std::fs::write(&source, &original).unwrap();
        let mut state = SearchReplaceDialogState::default();
        state.open = true;
        state.set_root(root);
        state.include_extensions = "txt".into();
        state.pattern = "needle".into();
        state.replacement = "changed".into();
        let coordinator = crate::search_replace::SearchCoordinator::new().unwrap();
        let worker = ReplacementPreviewWorker::new().unwrap();
        let request = state.build_search_request().unwrap();
        state.record_search_ticket(coordinator.submit(request).unwrap());
        wait_for_preview(&mut state, &coordinator, &worker, |state| {
            state.candidate.is_some() && !state.preview_pending
        });
        let search = state.search_result.as_ref().unwrap();
        assert_eq!(search.status(), SearchStatus::Complete);
        assert!(search.summary().match_list_complete());
        assert_eq!(
            search.matched_files().len(),
            1,
            "owned explicit root must contain one matched file"
        );
        let file = &search.matched_files()[0];
        assert_eq!(file.path().file_name(), source.file_name());
        assert_eq!(file.matches().len(), 20);
        state.selected_file = Some(file.path().to_path_buf());
        let ids: HashSet<_> = file
            .matches()
            .iter()
            .take(2)
            .map(|span| span.id().clone())
            .collect();
        state.selected_matches = ids.clone();
        state.root = Some(PathBuf::from(format!(
            "owned/{}",
            "long root segment/".repeat(70)
        )));
        let revisions = (
            state.search_revision,
            state.preview_revision,
            state.current_generation,
        );
        let runtime = ReplaceWorkerSnapshot {
            active: true,
            progress: Some(ReplaceProgress {
                transaction_id: "owned".into(),
                phase: ReplaceTransactionStatus::Applied,
                completed: 1,
                total: 2,
                message: "long operation status ".repeat(50),
            }),
            ..Default::default()
        };
        for size in [
            egui::vec2(600.0, 420.0),
            egui::vec2(1120.0, 820.0),
            egui::vec2(1600.0, 1000.0),
        ] {
            let ctx = egui::Context::default();
            for frame in 0..4 {
                let (commands, text) = layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
                assert!(commands.is_empty());
                let g = &state.geometry;
                assert!(
                    g.window.width() <= size.x + 1.0 && g.window.height() <= size.y + 1.0,
                    "{size:?}: {g:?}"
                );
                assert!(
                    g.footer.bottom() <= size.y + 1.0 && g.footer.top() >= g.body.bottom(),
                    "{g:?}"
                );
                assert!(
                    g.header.bottom() <= g.body.top() && g.body.height() >= 80.0,
                    "{g:?}"
                );
                assert!(
                    (48.0..=221.0).contains(&g.files.height())
                        && (48.0..=191.0).contains(&g.approvals.height()),
                    "{g:?}"
                );
                assert!(g.content.y > g.body.height(), "{g:?}");
                // A newly created egui Area measures in an invisible sizing
                // pass. Geometry is checked above even then; painted labels
                // and real clicks are checked after settled frames, as in M3.
                if frame >= 2 {
                    for label in [
                        "Search / Refresh",
                        "Close",
                        "Review Apply\u{2026}",
                        "Cancel between files",
                        "Restore / Recovery",
                    ] {
                        assert!(
                            text.iter().any(|(text, _)| text == label),
                            "missing {label}: {text:?}"
                        );
                    }
                }
            }
            let click = |state: &mut SearchReplaceDialogState, label: &str, allow| {
                let (_, text) = layout_frame(state, &ctx, size, &runtime, vec![], allow);
                let pos = text
                    .iter()
                    .find(|(text, _)| text == label)
                    .unwrap()
                    .1
                    .center();
                let mut all = Vec::new();
                for pressed in [true, false] {
                    all.extend(
                        layout_frame(
                            state,
                            &ctx,
                            size,
                            &runtime,
                            vec![
                                egui::Event::PointerMoved(pos),
                                egui::Event::PointerButton {
                                    pos,
                                    button: egui::PointerButton::Primary,
                                    pressed,
                                    modifiers: egui::Modifiers::NONE,
                                },
                            ],
                            allow,
                        )
                        .0,
                    );
                }
                all
            };
            assert_eq!(
                click(&mut state, "Search / Refresh", true),
                vec![SearchReplaceUiCommand::Search]
            );
            assert_eq!(
                click(&mut state, "Cancel between files", true),
                vec![SearchReplaceUiCommand::CancelOperation]
            );
            assert!(click(&mut state, "Search / Refresh", false).is_empty());
            let (_, text) = layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
            let pos = text
                .iter()
                .find(|(text, _)| text == "Search / Refresh")
                .unwrap()
                .1
                .center();
            layout_frame(
                &mut state,
                &ctx,
                size,
                &runtime,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                true,
            );
            let mut commands = Vec::new();
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events: vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ctx| {
                    commands.extend(state.ui(ctx, true, &runtime));
                    if ctx.current_pass_index() == 0 {
                        ctx.request_discard("verify replace action pass guard");
                    }
                },
            );
            output.drop_without_applying_deltas();
            assert_eq!(
                commands,
                vec![SearchReplaceUiCommand::Search],
                "discarded pass cannot duplicate commands"
            );
            assert!(click(&mut state, "Restore / Recovery", true).is_empty());
            for _ in 0..4 {
                layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
            }
            let g = &state.geometry;
            assert!(
                g.offset > 0.0 && g.offset + g.body.height() >= g.content.y - 2.0,
                "recovery entry reveals bottom: {g:?}"
            );
            assert_eq!(state.selected_matches, ids);
            assert_eq!(
                (
                    state.search_revision,
                    state.preview_revision,
                    state.current_generation
                ),
                revisions
            );
        }
        assert_eq!(std::fs::read_to_string(source).unwrap(), original);
    }

    fn wait_for_preview(
        state: &mut SearchReplaceDialogState,
        coordinator: &crate::search_replace::SearchCoordinator,
        worker: &ReplacementPreviewWorker,
        until: impl Fn(&SearchReplaceDialogState) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !until(state) && Instant::now() < deadline {
            state.poll_background(Some(coordinator), Some(worker), None, None);
            thread::sleep(Duration::from_millis(2));
        }
        assert!(until(state), "background search/preview did not finish");
    }

    fn wait_for_app_replace<F>(app: &mut crate::gui::LauncherApp, ctx: &egui::Context, until: F)
    where
        F: Fn(&crate::gui::LauncherApp) -> bool,
    {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !until(app) && Instant::now() < deadline {
            app.poll_search_replace_runtime(ctx);
            app.submit_ready_search_replace_confirmation();
            thread::sleep(Duration::from_millis(2));
        }
        assert!(until(app), "launcher replacement work did not finish");
    }

    #[test]
    fn m9d_selected_match_freezes_bounded_candidate_without_writing_source() {
        let fixture_parent = PathBuf::from("target/productivity_expansion");
        std::fs::create_dir_all(&fixture_parent).unwrap();
        let fixture = tempfile::Builder::new()
            .prefix("replace_dialog_")
            .tempdir_in(&fixture_parent)
            .unwrap();
        let root = fixture.path().join("explicit-root");
        std::fs::create_dir_all(&root).unwrap();
        let source_path = root.join("fixture.txt");
        let long_context = "漢".repeat(500);
        let original = format!(
            "\u{feff}{long_context} needle\r\nfirst\r\nsecond\r\n{long_context} needle last-without-newline"
        );
        std::fs::write(&source_path, original.as_bytes()).unwrap();

        let mut state = SearchReplaceDialogState::default();
        state.set_root(root.clone());
        state.include_extensions = "txt".into();
        state.pattern = r"needle|first\r\nsecond".into();
        state.pattern_mode = SearchPatternMode::RustRegex;
        state.replacement = "updated".into();
        let coordinator = crate::search_replace::SearchCoordinator::new().unwrap();
        let preview_worker = ReplacementPreviewWorker::new().unwrap();
        let request = state.build_search_request().unwrap();
        state.record_search_ticket(coordinator.submit(request).unwrap());
        wait_for_preview(&mut state, &coordinator, &preview_worker, |state| {
            state.search_result.is_some() && state.candidate.is_some() && !state.preview_pending
        });

        let search = state.search_result.as_ref().unwrap();
        let file = search.matched_files().first().unwrap();
        assert_eq!(file.matches().len(), 3);
        let match_ids = file
            .matches()
            .iter()
            .map(|span| span.id().clone())
            .collect::<Vec<_>>();
        let selected_path = file.path().to_path_buf();
        assert_eq!(
            state.candidate.as_ref().unwrap().readiness(),
            ReplacementCandidateReadiness::NoOp,
            "no matches are approved until the user selects them"
        );
        state.selected_file = Some(selected_path);
        for id in &match_ids {
            state.set_match_selected(id, true);
        }
        state.file_filter = "not-visible.txt".into();
        state.rebuild_visible_files();
        assert!(state.visible_file_indices.is_empty());
        assert_eq!(state.selected_matches.len(), match_ids.len());
        state.file_filter.clear();
        state.rebuild_visible_files();

        wait_for_preview(&mut state, &coordinator, &preview_worker, |state| {
            state.candidate.as_ref().is_some_and(|candidate| {
                !state.preview_pending
                    && candidate.readiness() == ReplacementCandidateReadiness::Ready
            })
        });
        let candidate = state.candidate.as_ref().unwrap();
        assert_eq!(candidate.summary().approved_matches(), 3);
        assert_eq!(candidate.summary().affected_files(), 1);
        let file_candidate = candidate.files().first().unwrap();
        assert_eq!(
            file_candidate.output_bytes().as_ref(),
            format!(
                "\u{feff}{long_context} updated\r\nupdated\r\n{long_context} updated last-without-newline"
            )
            .as_bytes()
        );
        let multiline_output = file_candidate.match_output(&match_ids[1]).unwrap();
        assert_eq!(multiline_output.original_text(), "first\r\nsecond");
        assert_eq!(multiline_output.proposed_text(), "updated");
        assert!(multiline_output.changed());
        let distant_output = file_candidate
            .match_output(match_ids.last().unwrap())
            .unwrap();
        assert_eq!(distant_output.original_text(), "needle");
        assert_eq!(distant_output.proposed_text(), "updated");
        let diff = file_candidate.diff_context().unwrap();
        assert!(diff.candidate_excerpt().contains("updated"));
        assert!(
            diff.truncated(),
            "the UI only publishes bounded diff context"
        );
        let cached_diff = state
            .candidate_unified_diffs
            .get(file_candidate.original_snapshot().path())
            .unwrap()
            .clone();
        assert!(cached_diff.contains("updated"));
        let ctx = egui::Context::default();
        for _ in 0..3 {
            let output = ctx.run_ui(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| state.draw_diff(ui, 0));
            });
            output.drop_without_applying_deltas();
        }
        assert!(Arc::ptr_eq(
            &cached_diff,
            state
                .candidate_unified_diffs
                .get(file_candidate.original_snapshot().path())
                .unwrap()
        ));
        assert_eq!(
            bounded_single_line("first\r\nsecond", 32),
            "first\\r\\nsecond"
        );
        let (bounded, truncated) = bounded_text(
            &"x".repeat(MAX_INSPECTED_MATCH_CHARS + 1),
            MAX_INSPECTED_MATCH_CHARS,
        );
        assert!(truncated);
        assert!(bounded.ends_with('…'));

        state.apply_review = state.build_apply_review();
        assert!(state.apply_review.is_some());
        state.apply_confirmation_open = true;
        let confirm = state.take_confirm_job().unwrap();
        preview_worker.submit(confirm).unwrap();
        wait_for_preview(&mut state, &coordinator, &preview_worker, |state| {
            state.pending_confirmed_request.is_some()
        });
        let frozen = state.take_confirmed_request().unwrap();
        drop(frozen);
        assert_eq!(std::fs::read(&source_path).unwrap(), original.as_bytes());

        let publications_before_stale_completion = state.preview_result_publications;
        state.replacement = "late stale replacement".into();
        state.queue_candidate_recompute();
        preview_worker
            .submit(state.take_preview_job().unwrap())
            .unwrap();
        state.set_root(root.join("different-root"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while preview_worker.is_busy() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        assert!(
            !preview_worker.is_busy(),
            "stale candidate worker did not finish"
        );
        state.poll_background(Some(&coordinator), Some(&preview_worker), None, None);
        assert!(state.search_result.is_none());
        assert!(state.selected_matches.is_empty());
        assert!(state.candidate.is_none());
        assert_eq!(
            state.preview_result_publications, publications_before_stale_completion,
            "candidate completion from an invalidated revision must be discarded"
        );
        assert!(state.take_confirmed_request().is_none());
        assert_eq!(std::fs::read(&source_path).unwrap(), original.as_bytes());

        state.cancel_current_search(Some(&coordinator));
        preview_worker.shutdown().unwrap();
        coordinator.shutdown().unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn m9e_launcher_replace_applies_only_fresh_selected_matches_and_routes_recovery_restore() {
        use crate::commands::{ActivationSource, CommandBus, SearchReplaceCommand, parse_command};
        use crate::plugin::Plugin;
        use crate::plugins::file_search::FileSearchPlugin;
        use crate::plugins::search_replace::SearchReplacePlugin;
        use crate::settings::Settings;
        use std::collections::HashSet;
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;

        struct NotesDirectoryOverride(Option<std::ffi::OsString>);
        impl NotesDirectoryOverride {
            fn use_fixture(path: &Path) -> Self {
                let previous = std::env::var_os("ML_NOTES_DIR");
                unsafe { std::env::set_var("ML_NOTES_DIR", path) };
                Self(previous)
            }
        }
        impl Drop for NotesDirectoryOverride {
            fn drop(&mut self) {
                if let Some(previous) = self.0.take() {
                    unsafe { std::env::set_var("ML_NOTES_DIR", previous) };
                } else {
                    unsafe { std::env::remove_var("ML_NOTES_DIR") };
                }
            }
        }

        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let fixture_parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("productivity_expansion");
        std::fs::create_dir_all(&fixture_parent).unwrap();
        let fixture = tempfile::Builder::new()
            .prefix("replace_m9e_app_")
            .tempdir_in(&fixture_parent)
            .unwrap();
        let root = fixture.path().join("project");
        let nested = root.join("nested");
        let excluded = root.join("excluded");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(&excluded).unwrap();

        let main_path = root.join("main.rs");
        let stale_path = root.join("stale.rs");
        let unchecked_path = root.join("unchecked.rs");
        let nested_path = nested.join("nested.rs");
        let excluded_path = excluded.join("ignored.rs");
        let binary_path = root.join("binary.rs");
        let text_path = root.join("notes.txt");
        let outside_path = fixture.path().join("outside.rs");
        let main_original = "\u{feff}// café Ω needle\r\nlet keep = \"needle\";"
            .as_bytes()
            .to_vec();
        let stale_original = b"// stale needle\r\n".to_vec();
        let unchecked_original = b"// leave this needle untouched\n".to_vec();
        let nested_original = b"// nested needle\n".to_vec();
        let excluded_original = b"// excluded needle\n".to_vec();
        let binary_original = b"needle\0binary".to_vec();
        let text_original = b"needle in an unsupported extension for this filter\n".to_vec();
        let outside_original = b"needle outside the selected root\n".to_vec();
        std::fs::write(&main_path, &main_original).unwrap();
        std::fs::write(&stale_path, &stale_original).unwrap();
        std::fs::write(&unchecked_path, &unchecked_original).unwrap();
        std::fs::write(&nested_path, &nested_original).unwrap();
        std::fs::write(&excluded_path, &excluded_original).unwrap();
        std::fs::write(&binary_path, &binary_original).unwrap();
        std::fs::write(&text_path, &text_original).unwrap();
        std::fs::write(&outside_path, &outside_original).unwrap();
        let stale_external = b"// outside edit kept needle\r\n".to_vec();

        // The existing fs command continues to parse as its normal content-search action.
        let fs_action = FileSearchPlugin::default()
            .search("fs content needle")
            .into_iter()
            .next()
            .expect("fs content search remains registered");
        assert!(
            fs_action.action.starts_with("file_search:start:"),
            "the normal fs command retains its file-search route"
        );

        let app_directory = fixture.path().join("app-data");
        let notes_directory = fixture.path().join("notes");
        std::fs::create_dir_all(&app_directory).unwrap();
        std::fs::create_dir_all(&notes_directory).unwrap();
        let _notes_directory = NotesDirectoryOverride::use_fixture(&notes_directory);
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.enabled_plugins = Some(HashSet::new());
        settings.dashboard.enabled = false;
        settings.dashboard.config_path = Some(
            app_directory
                .join("dashboard.json")
                .to_string_lossy()
                .into_owned(),
        );
        let mut app = crate::gui::LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new_inert_for_test_at(&app_directory),
            app_directory
                .join("actions.json")
                .to_string_lossy()
                .into_owned(),
            app_directory
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            settings,
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        app.watchers.clear();
        app.clipboard_modify_watcher = None;

        let replace_action = SearchReplacePlugin
            .search("replace")
            .into_iter()
            .next()
            .expect("replace command is registered");
        let invocation = parse_command(replace_action, None, ActivationSource::Dashboard).unwrap();
        assert_eq!(
            invocation.command,
            crate::commands::Command::SearchReplace(SearchReplaceCommand::Open)
        );
        CommandBus.dispatch(&invocation, &mut app).unwrap();
        assert!(app.search_replace_dialog.open);
        assert!(app.search_replace_dialog.root.is_none());
        assert!(app.search_replace_dialog.search_result.is_none());
        assert_eq!(std::fs::read(&main_path).unwrap(), main_original);

        app.search_replace_dialog.include_extensions = "rs, txt".into();
        app.search_replace_dialog.include_globs = "**/*.rs".into();
        app.search_replace_dialog.exclude_globs = "excluded/**".into();
        app.search_replace_dialog.pattern = "needle".into();
        app.search_replace_dialog.pattern_mode = SearchPatternMode::Literal;
        app.search_replace_dialog.case_sensitive = false;
        app.search_replace_dialog.replacement = "changed".into();
        app.search_replace_dialog.replacement_mode = ReplacementMode::Literal;
        app.search_replace_dialog.recursive = false;
        app.handle_search_replace_ui_command(SearchReplaceUiCommand::SelectRoot(root.clone()));
        app.handle_search_replace_ui_command(SearchReplaceUiCommand::Search);
        wait_for_app_replace(&mut app, &ctx, |app| {
            app.search_replace_dialog.search_result.is_some()
                && app.search_replace_dialog.candidate.is_some()
                && !app.search_replace_dialog.preview_pending
        });
        let direct = app.search_replace_dialog.search_result.as_ref().unwrap();
        let direct_paths = direct
            .matched_files()
            .iter()
            .map(|file| file.relative_path().to_path_buf())
            .collect::<HashSet<_>>();
        assert_eq!(
            direct_paths,
            HashSet::from([
                PathBuf::from("main.rs"),
                PathBuf::from("stale.rs"),
                PathBuf::from("unchecked.rs"),
            ])
        );
        assert!(
            direct
                .summary()
                .skipped_by_reason()
                .contains_key(&crate::search_replace::SkippedFileReason::Binary)
        );
        assert_eq!(std::fs::read(&main_path).unwrap(), main_original);

        // Explicit recursion includes nested files while extension/glob exclusions still apply.
        app.search_replace_dialog.set_root(root.clone());
        app.search_replace_dialog.recursive = true;
        app.handle_search_replace_ui_command(SearchReplaceUiCommand::Search);
        wait_for_app_replace(&mut app, &ctx, |app| {
            app.search_replace_dialog.search_result.is_some()
                && app.search_replace_dialog.candidate.is_some()
                && !app.search_replace_dialog.preview_pending
        });
        let search = app.search_replace_dialog.search_result.as_ref().unwrap();
        let relative_paths = search
            .matched_files()
            .iter()
            .map(|file| file.relative_path().to_path_buf())
            .collect::<HashSet<_>>();
        assert_eq!(
            relative_paths,
            HashSet::from([
                PathBuf::from("main.rs"),
                PathBuf::from("stale.rs"),
                PathBuf::from("unchecked.rs"),
                PathBuf::from("nested/nested.rs"),
            ])
        );
        let main_file = search
            .matched_files()
            .iter()
            .find(|file| file.relative_path() == Path::new("main.rs"))
            .unwrap();
        assert_eq!(main_file.matches().len(), 2);
        let main_match = main_file.matches()[0].id().clone();
        let stale_match = search
            .matched_files()
            .iter()
            .find(|file| file.relative_path() == Path::new("stale.rs"))
            .unwrap()
            .matches()[0]
            .id()
            .clone();
        app.search_replace_dialog
            .set_match_selected(&main_match, true);
        app.search_replace_dialog
            .set_match_selected(&stale_match, true);
        wait_for_app_replace(&mut app, &ctx, |app| {
            app.search_replace_dialog
                .candidate
                .as_ref()
                .is_some_and(|candidate| {
                    candidate.readiness() == ReplacementCandidateReadiness::Ready
                        && !app.search_replace_dialog.preview_pending
                })
        });
        let candidate = app.search_replace_dialog.candidate.as_ref().unwrap();
        assert_eq!(candidate.summary().approved_matches(), 2);
        assert_eq!(candidate.summary().affected_files(), 2);
        let main_candidate = candidate
            .files()
            .iter()
            .find(|file| file.original_snapshot().relative_path() == Path::new("main.rs"))
            .unwrap();
        assert_eq!(
            main_candidate.output_bytes().as_ref(),
            "\u{feff}// café Ω changed\r\nlet keep = \"needle\";".as_bytes()
        );
        let stale_candidate = candidate
            .files()
            .iter()
            .find(|file| file.original_snapshot().relative_path() == Path::new("stale.rs"))
            .unwrap();
        assert_eq!(
            stale_candidate.output_bytes().as_ref(),
            b"// stale changed\r\n"
        );
        assert_eq!(std::fs::read(&main_path).unwrap(), main_original);
        assert_eq!(std::fs::read(&stale_path).unwrap(), stale_original);

        // This edit is deliberately made after preview; C must skip it and retain the external bytes.
        std::fs::write(&stale_path, &stale_external).unwrap();
        app.search_replace_dialog.apply_review = app.search_replace_dialog.build_apply_review();
        assert_eq!(
            app.search_replace_dialog
                .apply_review
                .as_ref()
                .unwrap()
                .approved_matches,
            2
        );
        app.search_replace_dialog.apply_confirmation_open = true;
        app.handle_search_replace_ui_command(SearchReplaceUiCommand::ConfirmApply);
        wait_for_app_replace(&mut app, &ctx, |app| {
            app.search_replace_dialog
                .receipts
                .iter()
                .any(|receipt| receipt.status() == ReplaceTransactionStatus::Partial)
        });
        let receipt = app
            .search_replace_dialog
            .receipts
            .iter()
            .find(|receipt| receipt.status() == ReplaceTransactionStatus::Partial)
            .unwrap()
            .clone();
        assert_eq!(receipt.entries().len(), 2);
        assert!(
            receipt
                .entries()
                .iter()
                .any(|entry| entry.path() == main_path
                    && entry.outcome() == crate::replace_executor::ReplaceEntryOutcome::Applied)
        );
        assert!(
            receipt
                .entries()
                .iter()
                .any(|entry| entry.path() == stale_path
                    && entry.outcome()
                        == crate::replace_executor::ReplaceEntryOutcome::StaleSkipped)
        );
        let main_applied = "\u{feff}// café Ω changed\r\nlet keep = \"needle\";".as_bytes();
        assert_eq!(std::fs::read(&main_path).unwrap(), main_applied);
        assert_eq!(std::fs::read(&stale_path).unwrap(), stale_external);
        assert_eq!(std::fs::read(&unchecked_path).unwrap(), unchecked_original);
        assert_eq!(std::fs::read(&nested_path).unwrap(), nested_original);
        assert_eq!(std::fs::read(&excluded_path).unwrap(), excluded_original);
        assert_eq!(std::fs::read(&binary_path).unwrap(), binary_original);
        assert_eq!(std::fs::read(&text_path).unwrap(), text_original);
        assert_eq!(std::fs::read(&outside_path).unwrap(), outside_original);

        let recovery_directory =
            app_directory.join(crate::replace_executor::REPLACE_RECOVERY_DIRECTORY);
        let backup_files = std::fs::read_dir(&recovery_directory)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().contains("-backup.stage"))
            })
            .collect::<Vec<_>>();
        assert_eq!(backup_files.len(), 1);
        assert_eq!(std::fs::read(&backup_files[0]).unwrap(), main_original);
        assert!(
            recovery_directory
                .join(crate::replace_executor::REPLACE_RECOVERY_JOURNAL_FILE)
                .is_file()
        );

        // Force the actual C receipt into its recovery-required presentation state. Its
        // applied/stale entry evidence remains intact; Recovery must reconcile the journal
        // without rewriting the files, while Restore must remain a distinct later action.
        let journal_path =
            recovery_directory.join(crate::replace_executor::REPLACE_RECOVERY_JOURNAL_FILE);
        let mut journal: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&journal_path).unwrap()).unwrap();
        let receipt_value = journal["receipts"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|value| value["id"] == receipt.id())
            .unwrap();
        receipt_value["status"] = serde_json::Value::String("recovery_required".into());
        let recovery_receipt: ReplaceRecoveryReceipt =
            serde_json::from_value(receipt_value.clone()).unwrap();
        std::fs::write(&journal_path, serde_json::to_vec_pretty(&journal).unwrap()).unwrap();
        app.search_replace_dialog.upsert_receipt(recovery_receipt);
        app.handle_search_replace_ui_command(SearchReplaceUiCommand::ReviewRecovery(
            receipt.id().to_owned(),
        ));
        assert_eq!(
            app.search_replace_dialog
                .recovery_review
                .as_ref()
                .map(|review| review.action),
            Some(RecoveryAction::Recover)
        );

        // Root changes, an explicit refresh, and closing the dialog retain the durable receipt.
        let alternate_root = fixture.path().join("alternate");
        std::fs::create_dir_all(&alternate_root).unwrap();
        std::fs::write(
            alternate_root.join("alternate.rs"),
            b"needle in another chosen root\n",
        )
        .unwrap();
        app.handle_search_replace_ui_command(SearchReplaceUiCommand::SelectRoot(
            alternate_root.clone(),
        ));
        assert!(
            app.search_replace_dialog
                .receipts
                .iter()
                .any(|existing| existing.id() == receipt.id())
        );
        app.handle_search_replace_ui_command(SearchReplaceUiCommand::Search);
        wait_for_app_replace(&mut app, &ctx, |app| {
            app.search_replace_dialog.search_result.is_some()
                && app.search_replace_dialog.candidate.is_some()
                && !app.search_replace_dialog.preview_pending
        });
        app.handle_search_replace_ui_command(SearchReplaceUiCommand::SelectRoot(root.clone()));
        app.handle_search_replace_ui_command(SearchReplaceUiCommand::Search);
        wait_for_app_replace(&mut app, &ctx, |app| {
            app.search_replace_dialog.search_result.is_some()
                && app.search_replace_dialog.candidate.is_some()
                && !app.search_replace_dialog.preview_pending
        });
        assert_eq!(
            app.search_replace_dialog
                .search_result
                .as_ref()
                .unwrap()
                .matched_files()
                .iter()
                .find(|file| file.path() == stale_path)
                .unwrap()
                .raw_bytes()
                .as_ref(),
            stale_external
        );
        app.handle_search_replace_ui_command(SearchReplaceUiCommand::Closed);
        assert!(!app.search_replace_dialog.open);
        assert!(
            app.search_replace_dialog
                .receipts
                .iter()
                .any(|existing| existing.id() == receipt.id())
        );
        app.ensure_open(crate::gui::Panel::SearchReplaceDialog);
        assert!(app.search_replace_dialog.open);
        assert!(
            app.search_replace_dialog
                .receipts
                .iter()
                .any(|existing| existing.id() == receipt.id())
        );

        app.handle_search_replace_ui_command(SearchReplaceUiCommand::ReviewRecovery(
            receipt.id().to_owned(),
        ));
        app.handle_search_replace_ui_command(SearchReplaceUiCommand::ConfirmRecovery);
        wait_for_app_replace(&mut app, &ctx, |app| {
            app.search_replace_dialog.receipts.iter().any(|existing| {
                existing.id() == receipt.id()
                    && existing.status() == ReplaceTransactionStatus::Partial
                    && existing
                        .message()
                        .contains("interrupted apply phases were reconciled")
            })
        });
        assert_eq!(std::fs::read(&main_path).unwrap(), main_applied);
        assert_eq!(std::fs::read(&stale_path).unwrap(), stale_external);

        app.handle_search_replace_ui_command(SearchReplaceUiCommand::ReviewRestore(
            receipt.id().to_owned(),
        ));
        assert_eq!(
            app.search_replace_dialog
                .recovery_review
                .as_ref()
                .map(|review| review.action),
            Some(RecoveryAction::Restore)
        );
        app.handle_search_replace_ui_command(SearchReplaceUiCommand::ConfirmRestore);
        wait_for_app_replace(&mut app, &ctx, |app| {
            app.search_replace_dialog.receipts.iter().any(|existing| {
                existing.id() == receipt.id()
                    && existing.status() == ReplaceTransactionStatus::Restored
            })
        });
        assert_eq!(std::fs::read(&main_path).unwrap(), main_original);
        assert_eq!(std::fs::read(&stale_path).unwrap(), stale_external);
        assert_eq!(std::fs::read(&unchecked_path).unwrap(), unchecked_original);
        assert_eq!(std::fs::read(&outside_path).unwrap(), outside_original);
        app.shutdown_search_replace_runtime();
    }
}
