use crate::batch_rename::ExplicitDirectoryRoot;
use crate::file_operations::{NativeFileIdentity, ScanCancellation};
use crate::replace_executor::{
    ConfirmedReplacementRequest, REPLACE_RECOVERY_DIRECTORY, ReplaceExecutor,
    ReplaceExecutorWorker, ReplaceOperationResult, ReplaceProgress, ReplaceRecoveryReceipt,
    ReplaceTransactionStatus, ReplaceWorkerSnapshot,
};
use crate::search_replace::{
    MatchId, ReplacementCandidate, ReplacementCandidateReadiness, ReplacementMode,
    ReplacementRevision, ReplacementSpec, SearchFileSnapshot, SearchFilterSpec, SearchGeneration,
    SearchLimits, SearchPattern, SearchPatternMode, SearchProgress, SearchProgressPhase,
    SearchRevision, SearchStatus, TextSearchRequest, TextSearchResult,
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
struct PaneGeometry {
    id: egui::Id,
    rect: egui::Rect,
    content: egui::Vec2,
    offset: egui::Vec2,
}

#[cfg(test)]
fn pane_geometry<T>(output: &egui::scroll_area::ScrollAreaOutput<T>) -> PaneGeometry {
    PaneGeometry {
        id: output.id,
        rect: output.inner_rect,
        content: output.content_size,
        offset: output.state.offset,
    }
}

#[cfg(test)]
#[derive(Clone, Debug, Default)]
struct PreviewGeometry {
    open: bool,
    original: Option<PaneGeometry>,
    proposed: Option<PaneGeometry>,
    unified: Option<PaneGeometry>,
    inspected_original: Option<PaneGeometry>,
    inspected_proposed: Option<PaneGeometry>,
}

#[cfg(test)]
#[derive(Clone, Debug)]
struct ConfirmationGeometry {
    window: egui::Rect,
    body: egui::Rect,
    footer: egui::Rect,
    content: egui::Vec2,
    body_id: egui::Id,
    offset: f32,
}

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
    files_open: bool,
    approvals_open: bool,
    file_rows_rendered: usize,
    match_rows_rendered: usize,
    first_file_row: usize,
    first_match_row: usize,
    file_scroll_id: Option<egui::Id>,
    match_scroll_id: Option<egui::Id>,
    preview: std::cell::RefCell<PreviewGeometry>,
    apply_modal: Option<ConfirmationGeometry>,
    recovery_modal: Option<ConfirmationGeometry>,
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
            files_open: false,
            approvals_open: false,
            file_rows_rendered: 0,
            match_rows_rendered: 0,
            first_file_row: 0,
            first_match_row: 0,
            file_scroll_id: None,
            match_scroll_id: None,
            preview: Default::default(),
            apply_modal: None,
            recovery_modal: None,
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

#[derive(Clone, Copy)]
struct SearchReplaceNativePopupAnchor {
    response_id: egui::Id,
    popup_id: egui::Id,
    layer_id: egui::LayerId,
}

struct SearchReplaceFormDraft {
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
    file_filter: String,
    file_sort: FileSort,
}

fn take_target_order(order: &mut u64) -> u64 {
    let current = *order;
    *order = order.saturating_add(1);
    current
}

fn register_search_replace_target_at_order(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    local_identity: impl AsRef<str>,
    label: impl Into<String>,
    kind: crate::gui::jump_mode::TargetKind,
    visual_order: u64,
) -> Option<crate::gui::jump_mode::ui::FocusDirective> {
    let mut next_order = visual_order;
    crate::gui::jump_mode::ui::register_and_apply_target(
        scope,
        ui,
        response,
        local_identity,
        label,
        kind,
        &mut next_order,
    )
}

fn register_search_replace_target(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    local_identity: impl AsRef<str>,
    label: impl Into<String>,
    kind: crate::gui::jump_mode::TargetKind,
    visual_order: &mut u64,
) -> Option<crate::gui::jump_mode::ui::FocusDirective> {
    let order = take_target_order(visual_order);
    register_search_replace_target_at_order(scope, ui, response, local_identity, label, kind, order)
}

fn opaque_file_identity(
    generation: SearchGeneration,
    root_identity: NativeFileIdentity,
    file_identity: NativeFileIdentity,
    relative_path: &Path,
) -> u64 {
    egui::Id::new((
        "search-replace-file",
        generation,
        root_identity,
        file_identity,
        relative_path,
    ))
    .value()
}

fn search_file_local_identity(file: &SearchFileSnapshot, generation: SearchGeneration) -> String {
    format!(
        "file/{:016x}",
        opaque_file_identity(
            generation,
            file.root_identity(),
            file.file_identity(),
            file.relative_path(),
        )
    )
}

fn search_match_local_identity(file_key: u64, id: &MatchId) -> String {
    format!(
        "match/{:016x}/{:016x}",
        file_key,
        egui::Id::new(("search-replace-match", id)).value()
    )
}

fn confirmation_action_widget_id(
    confirmation_instance: u64,
    subject_identity: u64,
    action: &'static str,
) -> egui::Id {
    egui::Id::new((
        "search-replace-confirmation-action",
        confirmation_instance,
        subject_identity,
        action,
    ))
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SearchReplaceConfirmationIdentity {
    pub(crate) confirmation_instance: u64,
    pub(crate) kind: crate::gui::jump_mode::ui::SearchReplaceConfirmationKind,
    pub(crate) subject: crate::gui::jump_mode::ui::SearchReplaceConfirmationSubject,
}

/// Bounded, coalescing background worker for candidate construction and the
/// final C confirmation boundary. It never reads or writes files itself.
pub(crate) struct ReplacementPreviewWorker {
    shared: Arc<(Mutex<PreviewWorkerState>, Condvar)>,
    worker: Option<JoinHandle<()>>,
    activity_center: Option<crate::activity::ActivityCenter>,
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
        activity: Option<crate::activity::ActivityTask>,
    },
    Confirm {
        revision: u64,
        candidate: ReplacementCandidate,
        activity: Option<crate::activity::ActivityTask>,
    },
}

impl PreviewJob {
    fn revision(&self) -> u64 {
        match self {
            Self::Compute { revision, .. } | Self::Confirm { revision, .. } => *revision,
        }
    }

    fn operation(&self) -> crate::activity::ActivityOperation {
        match self {
            Self::Compute { .. } => crate::activity::ActivityOperation::PreviewReplacement,
            Self::Confirm { .. } => crate::activity::ActivityOperation::ConfirmReplacement,
        }
    }

    fn activity(&self) -> Option<&crate::activity::ActivityTask> {
        match self {
            Self::Compute { activity, .. } | Self::Confirm { activity, .. } => activity.as_ref(),
        }
    }

    fn set_activity(&mut self, activity: Option<crate::activity::ActivityTask>) {
        match self {
            Self::Compute {
                activity: current, ..
            }
            | Self::Confirm {
                activity: current, ..
            } => *current = activity,
        }
    }

    fn cancel_before_start(self) {
        if let Some(activity) = self.activity() {
            activity.finish(
                crate::activity::ActivityOutcome::Cancelled,
                None,
                crate::activity::ActivityRetirement::Confirmed,
            );
        }
    }
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
            activity_center: None,
        })
    }

    pub(crate) fn attach_activity_center(&mut self, center: crate::activity::ActivityCenter) {
        self.activity_center = Some(center);
    }

    pub(crate) fn submit(&self, mut job: PreviewJob) -> Result<(), String> {
        let activity = self.activity_center.as_ref().and_then(|center| {
            center.start(
                crate::activity::ActivityOwner::SearchReplace,
                crate::activity::ActivitySlot::Preview,
                crate::activity::ActivityIncarnation::new(job.revision()),
                job.operation(),
            )
        });
        job.set_activity(activity);
        let (lock, wake) = &*self.shared;
        let mut state = match lock.lock() {
            Ok(state) => state,
            Err(_) => {
                job.cancel_before_start();
                return Err("replacement preview worker state is unavailable".to_owned());
            }
        };
        if state.shutdown {
            drop(state);
            job.cancel_before_start();
            return Err("replacement preview worker is shutting down".into());
        }
        let replaced = state.pending.replace(job);
        state.result = None;
        wake.notify_one();
        drop(state);
        if let Some(replaced) = replaced {
            replaced.cancel_before_start();
        }
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
        let pending = if let Ok(mut state) = lock.lock() {
            state.shutdown = true;
            let pending = state.pending.take();
            wake.notify_all();
            pending
        } else {
            None
        };
        if let Some(pending) = pending {
            pending.cancel_before_start();
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
        let pending = if let Ok(mut state) = lock.lock() {
            state.shutdown = true;
            let pending = state.pending.take();
            wake.notify_all();
            pending
        } else {
            None
        };
        if let Some(pending) = pending {
            pending.cancel_before_start();
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

        let activity = job.activity().cloned();
        if let Some(activity) = activity.as_ref() {
            activity.mark_running();
        }

        let result = match job {
            PreviewJob::Compute {
                revision,
                search,
                replacement,
                selected,
                activity: _,
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
                activity: _,
            } => PreviewResult::Confirmed {
                revision,
                request: ConfirmedReplacementRequest::confirm(candidate)
                    .map_err(|error| error.to_string()),
            },
        };

        if let Some(activity) = activity.as_ref() {
            let (outcome, diagnostic) = match &result {
                PreviewResult::Candidate { candidate, .. } => match candidate {
                    Ok(_) => (crate::activity::ActivityOutcome::Completed, None),
                    Err(_) => (
                        crate::activity::ActivityOutcome::Failed,
                        Some(crate::activity::ActivityDiagnostic::WorkerFailed),
                    ),
                },
                PreviewResult::Confirmed { request, .. } => match request {
                    Ok(_) => (crate::activity::ActivityOutcome::Completed, None),
                    Err(_) => (
                        crate::activity::ActivityOutcome::Failed,
                        Some(crate::activity::ActivityDiagnostic::WorkerFailed),
                    ),
                },
            };
            activity.finish(
                outcome,
                diagnostic,
                crate::activity::ActivityRetirement::Unavailable,
            );
        }

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
    file_view_revision: u64,
    native_popup_anchors: Vec<SearchReplaceNativePopupAnchor>,
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
    next_confirmation_instance: u64,
    apply_confirmation_instance: Option<u64>,
    recovery_review: Option<RecoveryReview>,
    recovery_confirmation_instance: Option<u64>,
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
            file_view_revision: 0,
            native_popup_anchors: Vec::new(),
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
            next_confirmation_instance: 1,
            apply_confirmation_instance: None,
            recovery_review: None,
            recovery_confirmation_instance: None,
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
        self.native_popup_anchors.clear();
        self.open = true;
    }

    pub(crate) fn jump_mode_parent_targets_allowed(&self, operation_active: bool) -> bool {
        self.open
            && !operation_active
            && !self.apply_confirmation_open
            && !self.apply_confirmation_pending
            && self.pending_confirmed_request.is_none()
            && self.recovery_review.is_none()
    }

    pub(crate) fn native_parent_ui_allowed(&self) -> bool {
        self.open
            && !self.apply_confirmation_open
            && !self.apply_confirmation_pending
            && self.pending_confirmed_request.is_none()
            && self.recovery_review.is_none()
    }

    pub(crate) fn native_confirmation_layer_id(&self) -> Option<egui::LayerId> {
        if self.apply_confirmation_open {
            Some(egui::LayerId::new(
                egui::Order::Middle,
                egui::Id::new("replace_apply_confirmation"),
            ))
        } else if self.recovery_review.is_some() {
            Some(egui::LayerId::new(
                egui::Order::Middle,
                egui::Id::new("replace_recovery_confirmation"),
            ))
        } else {
            None
        }
    }

    pub(crate) fn jump_mode_confirmation_identity(
        &self,
    ) -> Option<SearchReplaceConfirmationIdentity> {
        if self.apply_confirmation_open {
            let review = self.apply_review.as_ref()?;
            if self.current_generation != Some(review.search_generation)
                || self.preview_revision != review.replacement_revision.0
            {
                return None;
            }
            return Some(SearchReplaceConfirmationIdentity {
                confirmation_instance: self.apply_confirmation_instance?,
                kind: crate::gui::jump_mode::ui::SearchReplaceConfirmationKind::Apply,
                subject: crate::gui::jump_mode::ui::SearchReplaceConfirmationSubject::Apply {
                    search_generation: review.search_generation.0,
                    replacement_revision: review.replacement_revision.0,
                },
            });
        }

        let review = self.recovery_review.as_ref()?;
        let receipt = self
            .receipts
            .iter()
            .find(|receipt| receipt.id() == review.transaction_id)?;
        let kind = match review.action {
            RecoveryAction::Restore
                if matches!(
                    receipt.status(),
                    ReplaceTransactionStatus::Applied | ReplaceTransactionStatus::Partial
                ) =>
            {
                crate::gui::jump_mode::ui::SearchReplaceConfirmationKind::Restore
            }
            RecoveryAction::Recover
                if receipt.status() == ReplaceTransactionStatus::RecoveryRequired =>
            {
                crate::gui::jump_mode::ui::SearchReplaceConfirmationKind::Recover
            }
            _ => return None,
        };
        Some(SearchReplaceConfirmationIdentity {
            confirmation_instance: self.recovery_confirmation_instance?,
            kind,
            subject: crate::gui::jump_mode::ui::SearchReplaceConfirmationSubject::Receipt {
                receipt_identity: egui::Id::new((
                    "search-replace-receipt-subject",
                    receipt.id(),
                    kind,
                ))
                .value(),
            },
        })
    }

    fn allocate_confirmation_instance(&mut self) -> u64 {
        let instance = self.next_confirmation_instance;
        self.next_confirmation_instance = self.next_confirmation_instance.wrapping_add(1).max(1);
        instance
    }

    #[cfg(test)]
    pub(crate) fn test_pattern(&self) -> &str {
        &self.pattern
    }

    #[cfg(test)]
    pub(crate) fn test_search_ticket_active(&self) -> bool {
        self.search_ticket.is_some()
    }

    #[cfg(test)]
    pub(crate) fn test_result_counts(&self) -> Option<(SearchGeneration, usize, usize)> {
        self.search_result.as_ref().map(|search| {
            (
                search.generation(),
                search.matched_files().len(),
                search
                    .matched_files()
                    .iter()
                    .map(|file| file.matches().len())
                    .sum(),
            )
        })
    }

    #[cfg(test)]
    pub(crate) fn test_selected_file(&self) -> Option<&Path> {
        self.selected_file.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn test_approved_match_count(&self) -> usize {
        self.selected_matches.len()
    }

    #[cfg(test)]
    pub(crate) fn test_inspected_match(&self) -> Option<&MatchId> {
        self.inspected_match.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn test_push_receipt(&mut self, receipt: ReplaceRecoveryReceipt) {
        self.receipts.push(receipt);
    }

    #[cfg(test)]
    pub(crate) fn test_mark_apply_confirmation_pending(&mut self) {
        if self.apply_confirmation_open {
            self.apply_confirmation_pending = true;
        }
    }

    #[cfg(test)]
    pub(crate) fn test_apply_confirmation_open(&self) -> bool {
        self.apply_confirmation_open
    }

    #[cfg(test)]
    pub(crate) fn test_apply_confirmation_pending(&self) -> bool {
        self.apply_confirmation_pending
    }

    #[cfg(test)]
    pub(crate) fn test_reveal_receipts(&self) -> bool {
        self.reveal_receipts
    }

    pub(crate) fn current_owned_native_popup_layer(
        &self,
        ctx: &egui::Context,
    ) -> Option<egui::LayerId> {
        if !self.open || !egui::Popup::is_any_open(ctx) {
            return None;
        }

        self.native_popup_anchors.iter().find_map(|anchor| {
            let response = ctx.read_response(anchor.response_id)?;
            if !response.enabled()
                || response.layer_id != anchor.layer_id
                || !egui::Popup::is_id_open(ctx, anchor.popup_id)
            {
                return None;
            }
            let popup_rect = ctx.memory(|memory| memory.area_rect(anchor.popup_id))?;
            let popup_layer = egui::LayerId::new(egui::Order::Foreground, anchor.popup_id);
            let popup_is_interactive = ctx.layer_id_at(popup_rect.center()) == Some(popup_layer);
            let popup_is_sizing = egui::containers::AreaState::load(ctx, anchor.popup_id)
                .is_some_and(|state| !state.interactable);
            (popup_rect.is_positive() && (popup_is_interactive || popup_is_sizing))
                .then_some(popup_layer)
        })
    }

    fn record_native_popup_anchor(&mut self, ui: &egui::Ui, response: &egui::Response) {
        if !response.enabled() || !response.rect.intersect(ui.clip_rect()).is_positive() {
            return;
        }
        let popup_id = egui::Popup::default_response_id(response);
        let anchor = SearchReplaceNativePopupAnchor {
            response_id: response.id,
            popup_id,
            layer_id: response.layer_id,
        };
        if !self.native_popup_anchors.iter().any(|existing| {
            existing.response_id == anchor.response_id && existing.popup_id == anchor.popup_id
        }) {
            self.native_popup_anchors.push(anchor);
        }
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
        self.native_popup_anchors.clear();
        self.invalidate_search("Root changed. Run Search to capture a new snapshot.".into());
        self.root = Some(root);
    }

    pub(crate) fn end_session(
        &mut self,
        coordinator: Option<&crate::search_replace::SearchCoordinator>,
        replace_worker: Option<&ReplaceExecutorWorker>,
    ) {
        self.open = false;
        self.native_popup_anchors.clear();
        self.reveal_receipts = false;
        self.cancel_current_search(coordinator);
        self.invalidate_search("The dialog was closed; Refresh before applying again.".into());
        if let Some(worker) = replace_worker {
            worker.cancel_active();
        }
        self.recovery_review = None;
        self.recovery_confirmation_instance = None;
        self.apply_confirmation_open = false;
        self.apply_confirmation_pending = false;
        self.apply_confirmation_instance = None;
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
            activity: None,
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
            activity: None,
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
        self.apply_confirmation_instance = None;
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
        self.apply_confirmation_instance = None;
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
        self.recovery_confirmation_instance = None;
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
        self.recovery_confirmation_instance = Some(self.allocate_confirmation_instance());
    }

    pub(crate) fn take_receipt_loader(&mut self) -> Option<RecoveryReceiptLoader> {
        self.receipt_loader.take()
    }

    pub(crate) fn complete_recovery_submission(&mut self) {
        self.recovery_review = None;
        self.recovery_confirmation_instance = None;
        self.status_message = Some("The confirmed recovery operation was submitted.".into());
    }

    pub(crate) fn cancel_confirmation(&mut self) {
        self.apply_confirmation_open = false;
        self.apply_confirmation_pending = false;
        self.apply_confirmation_instance = None;
        self.apply_review = None;
        self.pending_confirmed_request = None;
        if self.search_result.is_some() {
            self.queue_candidate_recompute();
        }
        self.recovery_review = None;
        self.recovery_confirmation_instance = None;
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
        self.apply_confirmation_instance = None;
        self.apply_review = None;
        self.recovery_review = None;
        self.recovery_confirmation_instance = None;
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
        self.apply_confirmation_instance = None;
        self.apply_review = None;
        self.recovery_review = None;
        self.recovery_confirmation_instance = None;
        self.preview_revision = self.preview_revision.saturating_add(1);
        self.file_view_revision = self.file_view_revision.saturating_add(1);
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
        self.apply_confirmation_instance = None;
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
        self.file_view_revision = self.file_view_revision.saturating_add(1);
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
            let original = egui::ScrollArea::both()
                .id_salt(("replace_diff_original", snapshot.path()))
                .auto_shrink([false, false])
                .min_scrolled_height(self.panel_height(150.0))
                .max_height(self.panel_height(150.0))
                .show(&mut columns[0], |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(context.original_excerpt())
                                .monospace()
                                .color(ui.visuals().error_fg_color),
                        )
                        .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
            let proposed = egui::ScrollArea::both()
                .id_salt(("replace_diff_candidate", snapshot.path()))
                .auto_shrink([false, false])
                .min_scrolled_height(self.panel_height(150.0))
                .max_height(self.panel_height(150.0))
                .show(&mut columns[1], |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(context.candidate_excerpt())
                                .monospace()
                                .color(ui.visuals().strong_text_color()),
                        )
                        .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
            #[cfg(test)]
            {
                let mut geometry = self.geometry.preview.borrow_mut();
                geometry.original = Some(pane_geometry(&original));
                geometry.proposed = Some(pane_geometry(&proposed));
            }
            #[cfg(not(test))]
            let _ = (original, proposed);
        });
        let Some(diff) = self.candidate_unified_diffs.get(snapshot.path()) else {
            ui.weak("The bounded unified diff is still being prepared on the preview worker.");
            return;
        };
        let unified = egui::ScrollArea::both()
            .id_salt(("replace_unified_diff", snapshot.path()))
            .auto_shrink([false, false])
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
                    ui.add(
                        egui::Label::new(egui::RichText::new(line).monospace().color(color))
                            .wrap_mode(egui::TextWrapMode::Extend),
                    );
                }
            });
        #[cfg(test)]
        {
            self.geometry.preview.borrow_mut().unified = Some(pane_geometry(&unified));
        }
        #[cfg(not(test))]
        let _ = unified;
    }

    pub(crate) fn ui(
        &mut self,
        ctx: &egui::Context,
        allow_mutations: bool,
        runtime: &ReplaceWorkerSnapshot,
    ) -> Vec<SearchReplaceUiCommand> {
        self.ui_with_jump_mode(
            ctx,
            None,
            None,
            allow_mutations,
            allow_mutations,
            allow_mutations,
            0,
            runtime,
        )
    }

    pub(crate) fn ui_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        jump_mode_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        confirmation_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        native_ui_owner: bool,
        first_ui_pass: bool,
        native_confirmation_owner: bool,
        dialog_instance: u64,
        runtime: &ReplaceWorkerSnapshot,
    ) -> Vec<SearchReplaceUiCommand> {
        let mut commands = Vec::new();
        if !self.open {
            self.native_popup_anchors.clear();
            return commands;
        }
        self.native_popup_anchors.clear();
        let jump_mode_scope = jump_mode_scope.filter(|_| native_ui_owner);
        let confirmation_scope = confirmation_scope.filter(|_| native_confirmation_owner);
        let mut process_interactions = false;
        let mut discarded_form_draft = None;
        let mut visual_order = 0;
        let mut window_open = self.open;
        let window = egui::Window::new("Search and Replace")
            .id(egui::Id::new(WINDOW_ID))
            .open(&mut window_open)
            .resizable(true)
            .default_size(egui::vec2(1120.0, 820.0))
            .min_size(egui::vec2(560.0, 360.0))
            .show(ctx, |ui| {
                let native_visible_pass = native_ui_owner
                    && ui.is_enabled()
                    && ui.is_visible()
                    && !ui.is_sizing_pass();
                process_interactions = native_visible_pass
                    && first_ui_pass
                    && ctx.current_pass_index() == 0
                    && !self.apply_confirmation_open
                    && !self.apply_confirmation_pending
                    && self.recovery_review.is_none();
                if !process_interactions {
                    discarded_form_draft = Some(self.form_draft_snapshot());
                }
                let header_start = ui.cursor().min;
                self.draw_header(
                    ui,
                    &mut commands,
                    jump_mode_scope,
                    native_visible_pass,
                    process_interactions,
                    &mut visual_order,
                );
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
                        self.draw_search_fields(
                            ui,
                            &mut commands,
                            jump_mode_scope,
                            native_visible_pass,
                            process_interactions,
                            &mut visual_order,
                        );
                        ui.separator();
                        self.draw_search_status(
                            ui,
                            &mut commands,
                            jump_mode_scope,
                            &mut visual_order,
                        );
                        self.draw_results(
                            ui,
                            &mut commands,
                            jump_mode_scope,
                            native_visible_pass,
                            process_interactions,
                            dialog_instance,
                            &mut visual_order,
                        );
                        ui.separator();
                        let receipts = ui.scope(|ui| {
                            self.draw_receipts(
                                ui,
                                &mut commands,
                                jump_mode_scope,
                                native_visible_pass,
                                process_interactions,
                                dialog_instance,
                                &mut visual_order,
                            )
                        });
                    if process_interactions && self.reveal_receipts {
                            receipts.response.scroll_to_me_animation(
                                Some(Align::Min),
                                egui::style::ScrollAnimation::none(),
                            );
                            self.reveal_receipts = false;
                        }
                    });
                #[cfg(test)]
                let footer_start = ui.cursor().min;
                ui.separator();
                    self.draw_operation_status(
                    ui,
                    runtime,
                    &mut commands,
                    jump_mode_scope,
                    native_visible_pass,
                    process_interactions,
                    &mut visual_order,
                );
                ui.horizontal(|ui| {
                    let close = ui.add_enabled(native_visible_pass, egui::Button::new("Close"));
                    register_search_replace_target(
                        jump_mode_scope,
                        ui,
                        &close,
                        "footer/close",
                        "Close",
                        crate::gui::jump_mode::TargetKind::Button,
                        &mut visual_order,
                    );
                    if process_interactions && close.clicked() {
                        commands.push(SearchReplaceUiCommand::Closed);
                    }
                    let reveal_receipts = ui.add_enabled(
                        native_visible_pass,
                        egui::Button::new("Restore / Recovery"),
                    );
                    register_search_replace_target(
                        jump_mode_scope,
                        ui,
                        &reveal_receipts,
                        "footer/reveal-receipts",
                        "Reveal recovery receipts",
                        crate::gui::jump_mode::TargetKind::Button,
                        &mut visual_order,
                    );
                    if process_interactions && reveal_receipts.clicked() {
                        self.reveal_receipts = true;
                        ctx.request_repaint();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let can_preview = native_visible_pass
                            && self.search_result.is_some()
                            && !self.preview_pending
                            && self.candidate.is_some();
                        let apply = ui.add_enabled(can_preview, egui::Button::new("Review Apply\u{2026}"));
                        let apply_identity = self.search_result.as_ref().map(|search| {
                            format!(
                                "footer/review-apply/{}/{}",
                                search.generation().0,
                                self.preview_revision
                            )
                        });
                        if let Some(identity) = apply_identity {
                            register_search_replace_target(
                                jump_mode_scope,
                                ui,
                                &apply,
                                identity,
                                "Review Apply",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut visual_order,
                            );
                        }
                        if process_interactions && apply.clicked() {
                            if self.apply_review.is_none() { self.apply_review = self.build_apply_review(); }
                            if self.apply_review.is_some() {
                                self.apply_confirmation_open = true;
                                self.apply_confirmation_instance = Some(self.allocate_confirmation_instance());
                                if let Some(scope) = jump_mode_scope {
                                    scope.discard_registered_targets();
                                }
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

        if let Some(draft) = discarded_form_draft {
            self.restore_form_draft(draft);
        }

        if native_ui_owner
            && first_ui_pass
            && ctx.current_pass_index() == 0
            && (!window_open || commands.contains(&SearchReplaceUiCommand::Closed))
        {
            self.end_session(None, None);
            if !commands.contains(&SearchReplaceUiCommand::Closed) {
                commands.push(SearchReplaceUiCommand::Closed);
            }
            return commands;
        }

        self.draw_confirmations(
            ctx,
            runtime,
            &mut commands,
            confirmation_scope,
            native_confirmation_owner,
            first_ui_pass,
        );
        commands
    }

    fn form_draft_snapshot(&self) -> SearchReplaceFormDraft {
        SearchReplaceFormDraft {
            recursive: self.recursive,
            include_extensions: self.include_extensions.clone(),
            include_globs: self.include_globs.clone(),
            exclude_paths: self.exclude_paths.clone(),
            exclude_globs: self.exclude_globs.clone(),
            pattern: self.pattern.clone(),
            pattern_mode: self.pattern_mode,
            case_sensitive: self.case_sensitive,
            replacement: self.replacement.clone(),
            replacement_mode: self.replacement_mode,
            file_filter: self.file_filter.clone(),
            file_sort: self.file_sort,
        }
    }

    fn restore_form_draft(&mut self, draft: SearchReplaceFormDraft) {
        self.recursive = draft.recursive;
        self.include_extensions = draft.include_extensions;
        self.include_globs = draft.include_globs;
        self.exclude_paths = draft.exclude_paths;
        self.exclude_globs = draft.exclude_globs;
        self.pattern = draft.pattern;
        self.pattern_mode = draft.pattern_mode;
        self.case_sensitive = draft.case_sensitive;
        self.replacement = draft.replacement;
        self.replacement_mode = draft.replacement_mode;
        self.file_filter = draft.file_filter;
        self.file_sort = draft.file_sort;
    }

    fn draw_header(
        &mut self,
        ui: &mut egui::Ui,
        commands: &mut Vec<SearchReplaceUiCommand>,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        native_ui_owner: bool,
        process_interactions: bool,
        visual_order: &mut u64,
    ) {
        ui.horizontal(|ui| {
            let choose = ui.add_enabled(
                native_ui_owner,
                egui::Button::new("Choose root folder\u{2026}"),
            );
            register_search_replace_target(
                scope,
                ui,
                &choose,
                "form/choose-root",
                "Choose root folder",
                crate::gui::jump_mode::TargetKind::Button,
                visual_order,
            );
            if process_interactions && choose.clicked() {
                let mut choose = rfd::FileDialog::new();
                if let Some(root) = &self.root {
                    choose = choose.set_directory(root);
                }
                if let Some(root) = choose.pick_folder() {
                    commands.push(SearchReplaceUiCommand::SelectRoot(root));
                }
            }
            let search_enabled = self.root.is_some() && !self.pattern.is_empty();
            let search = ui.add_enabled(
                native_ui_owner && search_enabled,
                egui::Button::new("Search / Refresh"),
            );
            register_search_replace_target(
                scope,
                ui,
                &search,
                "form/search",
                "Search",
                crate::gui::jump_mode::TargetKind::Button,
                visual_order,
            );
            if process_interactions && search.clicked() {
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
            native_ui_owner,
            egui::Checkbox::new(&mut recursive, "Include subfolders"),
        );
        register_search_replace_target(
            scope,
            ui,
            &response,
            "form/recursive",
            "Include subfolders",
            crate::gui::jump_mode::TargetKind::Checkbox,
            visual_order,
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
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        native_ui_owner: bool,
        process_interactions: bool,
        visual_order: &mut u64,
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
        let settings_order = take_target_order(visual_order);
        let settings = egui::CollapsingHeader::new(format!("Search and Replace settings ({active} filters active)"))
            .id_salt("replace_search_settings")
            .default_open(true)
            .show(ui, |ui| {
                ui.add_enabled_ui(native_ui_owner, |ui| {
                    ui.label("Search for");
                    let width = ui.available_width().min(520.0);
                    let pattern = ui.add(egui::TextEdit::singleline(&mut self.pattern)
                        .interactive(native_ui_owner)
                        .id(egui::Id::new("replace_search_pattern")).desired_width(width));
                    register_search_replace_target(
                        scope,
                        ui,
                        &pattern,
                        "form/pattern",
                        "Search pattern",
                        crate::gui::jump_mode::TargetKind::TextEntry,
                        visual_order,
                    );
                    search_changed |= pattern.changed();
                    #[cfg(test)] { self.geometry.pattern_editor = pattern.rect; }
                    ui.horizontal_wrapped(|ui| {
                        let anchor = "form/pattern-mode";
                        let anchor_order = take_target_order(visual_order);
                        let options_scope = scope.map(|scope| scope.combo_popup_scope(anchor));
                        let combo = egui::ComboBox::from_id_salt("replace_pattern_mode")
                            .selected_text(match self.pattern_mode { SearchPatternMode::Literal => "Literal", SearchPatternMode::RustRegex => "Rust regex" })
                            .show_ui(ui, |ui| {
                                for (identity, mode, label) in [
                                    ("option/literal", SearchPatternMode::Literal, "Literal"),
                                    ("option/rust-regex", SearchPatternMode::RustRegex, "Rust regex"),
                                ] {
                                    let option = ui.selectable_value(&mut self.pattern_mode, mode, label);
                                    register_search_replace_target(
                                        options_scope.as_ref(),
                                        ui,
                                        &option,
                                        identity,
                                        label,
                                        crate::gui::jump_mode::TargetKind::Selectable,
                                        visual_order,
                                    );
                                    search_changed |= option.changed();
                                }
                            });
                        self.record_native_popup_anchor(ui, &combo.response);
                        register_search_replace_target_at_order(
                            scope,
                            ui,
                            &combo.response,
                            anchor,
                            "Search pattern mode",
                            crate::gui::jump_mode::TargetKind::ComboBox,
                            anchor_order,
                        );
                        let case_sensitive = ui.checkbox(&mut self.case_sensitive, "Case sensitive");
                        register_search_replace_target(
                            scope,
                            ui,
                            &case_sensitive,
                            "form/case-sensitive",
                            "Case sensitive",
                            crate::gui::jump_mode::TargetKind::Checkbox,
                            visual_order,
                        );
                        search_changed |= case_sensitive.changed();
                    });
                    ui.label("Replace with");
                    let replacement = ui.add(egui::TextEdit::singleline(&mut self.replacement)
                        .interactive(native_ui_owner)
                        .id(egui::Id::new("replace_replacement_text")).desired_width(width));
                    register_search_replace_target(
                        scope,
                        ui,
                        &replacement,
                        "form/replacement",
                        "Replacement text",
                        crate::gui::jump_mode::TargetKind::TextEntry,
                        visual_order,
                    );
                    replacement_changed |= replacement.changed();
                    #[cfg(test)] { self.geometry.replacement_editor = replacement.rect; }
                    ui.horizontal_wrapped(|ui| {
                        let anchor = "form/replacement-mode";
                        let anchor_order = take_target_order(visual_order);
                        let options_scope = scope.map(|scope| scope.combo_popup_scope(anchor));
                        let combo = egui::ComboBox::from_id_salt("replace_replacement_mode")
                            .selected_text(match self.replacement_mode { ReplacementMode::Literal => "Literal replacement", ReplacementMode::RustRegexCaptures => "Rust capture substitution" })
                            .show_ui(ui, |ui| {
                                for (identity, mode, label) in [
                                    ("option/literal", ReplacementMode::Literal, "Literal replacement"),
                                    ("option/rust-captures", ReplacementMode::RustRegexCaptures, "Rust capture substitution"),
                                ] {
                                    let option = ui.selectable_value(&mut self.replacement_mode, mode, label);
                                    register_search_replace_target(
                                        options_scope.as_ref(),
                                        ui,
                                        &option,
                                        identity,
                                        label,
                                        crate::gui::jump_mode::TargetKind::Selectable,
                                        visual_order,
                                    );
                                    replacement_changed |= option.changed();
                                }
                            });
                        self.record_native_popup_anchor(ui, &combo.response);
                        register_search_replace_target_at_order(
                            scope,
                            ui,
                            &combo.response,
                            anchor,
                            "Replacement mode",
                            crate::gui::jump_mode::TargetKind::ComboBox,
                            anchor_order,
                        );
                    });
                    ui.label("Capture substitutions use Rust regex syntax such as $1, ${name}, and $$; invalid references are shown before Apply.");
                });
                let advanced_order = take_target_order(visual_order);
                let advanced = egui::CollapsingHeader::new(format!("Advanced Filters ({active} active)"))
                    .id_salt("replace_advanced_filters")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.add_enabled_ui(native_ui_owner, |ui| {
                            for (label, id, value, hint) in [
                                ("Extensions", "replace_extensions", &mut self.include_extensions, "Comma separated; empty means all supported text extensions"),
                                ("Include globs", "replace_include_globs", &mut self.include_globs, "One root-relative path glob per line"),
                                ("Exclude paths", "replace_exclude_paths", &mut self.exclude_paths, "Root-relative file or folder paths, one per line"),
                                ("Exclude globs", "replace_exclude_globs", &mut self.exclude_globs, "One root-relative path glob per line"),
                            ] {
                                ui.label(label);
                                let width = ui.available_width().min(520.0);
                                let response = ui.add(egui::TextEdit::singleline(value)
                                    .interactive(native_ui_owner)
                                    .id(egui::Id::new(id)).desired_width(width));
                                register_search_replace_target(
                                    scope,
                                    ui,
                                    &response,
                                    format!("form/advanced/{id}"),
                                    label,
                                    crate::gui::jump_mode::TargetKind::TextEntry,
                                    visual_order,
                                );
                                search_changed |= response.changed();
                                ui.weak(hint);
                            }
                        });
                    });
                register_search_replace_target_at_order(
                    scope,
                    ui,
                    &advanced.header_response,
                    "form/advanced-filters",
                    "Advanced filters",
                    crate::gui::jump_mode::TargetKind::Button,
                    advanced_order,
                );
                #[cfg(test)] { self.geometry.advanced_open = advanced.body_response.is_some(); }
                #[cfg(not(test))] let _ = advanced;
            });
        register_search_replace_target_at_order(
            scope,
            ui,
            &settings.header_response,
            "form/settings",
            "Search and Replace settings",
            crate::gui::jump_mode::TargetKind::Button,
            settings_order,
        );
        #[cfg(test)]
        {
            self.geometry.settings_open = settings.body_response.is_some();
        }
        #[cfg(not(test))]
        let _ = settings;
        if process_interactions && search_changed {
            self.invalidate_search(
                "Search inputs changed. Run Search to capture a new result set.".into(),
            );
        } else if process_interactions && replacement_changed {
            self.queue_candidate_recompute();
        }
    }

    fn draw_search_status(
        &mut self,
        ui: &mut egui::Ui,
        _commands: &mut Vec<SearchReplaceUiCommand>,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        visual_order: &mut u64,
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
                let issue_order = take_target_order(visual_order);
                let issues = ui.collapsing(
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
                register_search_replace_target_at_order(
                    scope,
                    ui,
                    &issues.header_response,
                    "search/issues",
                    "Search issues",
                    crate::gui::jump_mode::TargetKind::Button,
                    issue_order,
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
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        native_ui_owner: bool,
        process_interactions: bool,
        dialog_instance: u64,
        visual_order: &mut u64,
    ) {
        #[cfg(test)]
        {
            self.geometry.files = egui::Rect::NOTHING;
            self.geometry.approvals = egui::Rect::NOTHING;
            self.geometry.files_open = false;
            self.geometry.approvals_open = false;
            self.geometry.file_rows_rendered = 0;
            self.geometry.match_rows_rendered = 0;
            self.geometry.first_file_row = 0;
            self.geometry.first_match_row = 0;
            self.geometry.file_scroll_id = None;
            self.geometry.match_scroll_id = None;
            *self.geometry.preview.get_mut() = PreviewGeometry::default();
        }
        let Some(search) = self.search_result.as_ref().cloned() else {
            ui.weak("Search results appear here. A search only reads the chosen root and never edits files.");
            return;
        };
        let file_count = search.matched_files().len();
        let current = self
            .selected_file
            .as_ref()
            .and_then(|path| path.file_name())
            .map_or_else(
                || "none".into(),
                |name| bounded_single_line(&name.to_string_lossy(), 48),
            );
        let title = format!(
            "Files with Matches ({} of {file_count} files, {} approved matches; current: {current})",
            self.visible_file_indices.len(),
            self.selected_matches.len()
        );
        let files_section_order = take_target_order(visual_order);
        let file_section = egui::CollapsingHeader::new(title)
            .id_salt("replace_files_section")
            .default_open(true)
            .show(ui, |ui| {
                let mut path_filter_changed = false;
                let mut sort_changed = false;
                ui.horizontal_wrapped(|ui| {
                    ui.label("Filter files");
                    let file_filter = ui.add_enabled(
                        native_ui_owner,
                        egui::TextEdit::singleline(&mut self.file_filter)
                            .interactive(native_ui_owner)
                            .id(egui::Id::new("replace_file_filter"))
                            .desired_width(250.0),
                    );
                    register_search_replace_target(
                        scope,
                        ui,
                        &file_filter,
                        "results/filter",
                        "Filter files",
                        crate::gui::jump_mode::TargetKind::TextEntry,
                        visual_order,
                    );
                    path_filter_changed |= file_filter.changed();
                    let mut sort = self.file_sort;
                    let sort_anchor = "results/sort";
                    let sort_anchor_order = take_target_order(visual_order);
                    let sort_options_scope =
                        scope.map(|scope| scope.combo_popup_scope(sort_anchor));
                    let combo = ui.add_enabled_ui(native_ui_owner, |ui| {
                        egui::ComboBox::from_id_salt("replace_file_sort")
                            .selected_text(match sort {
                                FileSort::Path => "Path order",
                                FileSort::MatchCount => "Match count",
                            })
                            .show_ui(ui, |ui| {
                                for (identity, value, label) in [
                                    ("option/path", FileSort::Path, "Path order"),
                                    ("option/match-count", FileSort::MatchCount, "Match count"),
                                ] {
                                    let option = ui.selectable_value(&mut sort, value, label);
                                    register_search_replace_target(
                                        sort_options_scope.as_ref(),
                                        ui,
                                        &option,
                                        identity,
                                        label,
                                        crate::gui::jump_mode::TargetKind::Selectable,
                                        visual_order,
                                    );
                                    sort_changed |= option.changed();
                                }
                            })
                    });
                    self.record_native_popup_anchor(ui, &combo.inner.response);
                    register_search_replace_target_at_order(
                        scope,
                        ui,
                        &combo.inner.response,
                        sort_anchor,
                        "Sort files",
                        crate::gui::jump_mode::TargetKind::ComboBox,
                        sort_anchor_order,
                    );
                    if process_interactions && sort_changed {
                        self.file_sort = sort;
                    }
                    let can_select_visible = !self.visible_file_indices.is_empty();
                    let can_clear_visible = self
                        .visible_file_indices
                        .iter()
                        .filter_map(|index| search.matched_files().get(*index))
                        .flat_map(|file| file.matches())
                        .any(|span| self.selected_matches.contains(span.id()));
                    let select = ui.add_enabled(
                        native_ui_owner && can_select_visible,
                        egui::Button::new("Select visible matches"),
                    );
                    let select_identity = format!(
                        "approvals/select-visible/{}/{}",
                        search.generation().0,
                        self.file_view_revision
                    );
                    register_search_replace_target(
                        scope,
                        ui,
                        &select,
                        select_identity,
                        "Select visible matches",
                        crate::gui::jump_mode::TargetKind::Button,
                        visual_order,
                    );
                    let clear = ui.add_enabled(
                        native_ui_owner && can_clear_visible,
                        egui::Button::new("Clear visible matches"),
                    );
                    let clear_identity = format!(
                        "approvals/clear-visible/{}/{}",
                        search.generation().0,
                        self.file_view_revision
                    );
                    register_search_replace_target(
                        scope,
                        ui,
                        &clear,
                        clear_identity,
                        "Clear visible matches",
                        crate::gui::jump_mode::TargetKind::Button,
                        visual_order,
                    );
                    if process_interactions && select.clicked() {
                        self.set_visible_files_selected(true);
                    }
                    if process_interactions && clear.clicked() {
                        self.set_visible_files_selected(false);
                    }
                });
                if process_interactions && (path_filter_changed || sort_changed) {
                    self.rebuild_visible_files();
                }

                let visible_count = self.visible_file_indices.len();
                let results_scope = scope.map(|scope| {
                    scope.child_namespace(format!(
                        "results/{}/{}/{}",
                        search.generation().0,
                        self.preview_revision,
                        self.file_view_revision,
                    ))
                });
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
                        #[cfg(test)]
                        {
                            self.geometry.file_rows_rendered = rows.len();
                            self.geometry.first_file_row = rows.start;
                        }
                        for visible_index in rows {
                            let Some(file_index) =
                                self.visible_file_indices.get(visible_index).copied()
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
                                let file_approval_identity =
                                    search_file_local_identity(file, search.generation());
                                let checkbox_scope = egui::Id::new((
                                    "replace-file-approval",
                                    dialog_instance,
                                    search.generation().0,
                                    file.root_identity(),
                                    file.file_identity(),
                                    file.relative_path(),
                                ));
                                let checkbox = ui
                                    .scope_builder(
                                        egui::UiBuilder::new().id(checkbox_scope),
                                        |checkbox_ui| {
                                            let checkbox = checkbox_ui.add_enabled(
                                                native_ui_owner,
                                                egui::Checkbox::new(&mut checked, ""),
                                            );
                                            register_search_replace_target(
                                                results_scope.as_ref(),
                                                checkbox_ui,
                                                &checkbox,
                                                format!("approval/file/{file_approval_identity}"),
                                                "Approve file matches",
                                                crate::gui::jump_mode::TargetKind::Checkbox,
                                                visual_order,
                                            );
                                            checkbox
                                        },
                                    )
                                    .inner;
                                if process_interactions && checkbox.clicked() {
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
                                let row_ui_id = egui::Id::new((
                                    "replace-file-result-row",
                                    dialog_instance,
                                    search.generation().0,
                                    file.root_identity(),
                                    file.file_identity(),
                                    file.relative_path(),
                                ));
                                let _ = ui.scope_builder(
                                    egui::UiBuilder::new().id(row_ui_id),
                                    |row_ui| {
                                        let response = row_ui.add_enabled(
                                            native_ui_owner,
                                            egui::Button::selectable(active, label)
                                                .wrap_mode(egui::TextWrapMode::Truncate),
                                        );
                                        let target_identity =
                                            search_file_local_identity(file, search.generation());
                                        let target_id = results_scope
                                            .as_ref()
                                            .map(|scope| scope.target_id(&target_identity));
                                        let directive = register_search_replace_target(
                                            results_scope.as_ref(),
                                            row_ui,
                                            &response,
                                            &target_identity,
                                            "Inspect file result",
                                            crate::gui::jump_mode::TargetKind::Action,
                                            visual_order,
                                        );
                                        let focused_by_hint = directive
                                            == Some(
                                                crate::gui::jump_mode::ui::FocusDirective::Select,
                                            );
                                        if process_interactions
                                            && (response.clicked() || focused_by_hint)
                                            && self.search_result.as_ref().is_some_and(|current| {
                                                current.generation() == search.generation()
                                                    && current
                                                        .matched_files()
                                                        .get(file_index)
                                                        .is_some_and(|current_file| {
                                                            current_file.file_identity()
                                                                == file.file_identity()
                                                                && current_file.path()
                                                                    == file.path()
                                                        })
                                                    && self
                                                        .visible_file_indices
                                                        .contains(&file_index)
                                            })
                                        {
                                            self.selected_file = Some(file.path().to_path_buf());
                                            self.inspected_match = None;
                                            if focused_by_hint {
                                                response.request_focus();
                                                if let (Some(scope), Some(target_id)) =
                                                    (results_scope.as_ref(), target_id.as_ref())
                                                {
                                                    scope.record_select_focus_owner(
                                                        target_id,
                                                        response.id,
                                                    );
                                                }
                                            }
                                        }
                                    },
                                );
                            });
                        }
                    });

                #[cfg(test)]
                {
                    self.geometry.files = files.inner_rect;
                    self.geometry.file_scroll_id = Some(files.id);
                }
                #[cfg(not(test))]
                let _ = files;
            });
        register_search_replace_target_at_order(
            scope,
            ui,
            &file_section.header_response,
            "results/files",
            "Files with matches",
            crate::gui::jump_mode::TargetKind::Button,
            files_section_order,
        );
        #[cfg(test)]
        {
            self.geometry.files_open = file_section.body_response.is_some();
        }
        #[cfg(not(test))]
        let _ = file_section;

        let results_scope = scope.map(|scope| {
            scope.child_namespace(format!(
                "results/{}/{}/{}",
                search.generation().0,
                self.preview_revision,
                self.file_view_revision,
            ))
        });
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
        let selected_file_key = opaque_file_identity(
            search.generation(),
            file.root_identity(),
            file.file_identity(),
            file.relative_path(),
        );
        let file_detail_scope = results_scope
            .map(|scope| scope.child_namespace(format!("file-detail/{selected_file_key:016x}")));
        let selected = file
            .matches()
            .iter()
            .filter(|span| self.selected_matches.contains(span.id()))
            .count();
        let approvals_order = take_target_order(visual_order);
        let approvals_section = egui::CollapsingHeader::new(format!("File Approvals ({selected} of {} approved)", file.matches().len()))
            .id_salt("replace_approvals_section")
            .default_open(true)
            .show(ui, |ui| {
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
                #[cfg(test)] { self.geometry.match_rows_rendered = rows.len();
                    self.geometry.first_match_row = rows.start; }
                for match_index in rows {
                    let Some(span) = file.matches().get(match_index) else {
                        continue;
                    };
                    let id = span.id();
                    let mut checked = self.selected_matches.contains(id);
                    let row_id = egui::Id::new((
                        "replace-match-result-row",
                        dialog_instance,
                        search.generation().0,
                        file.root_identity(),
                        file.file_identity(),
                        file.relative_path(),
                        id,
                    ));
                    let _ = ui.scope_builder(egui::UiBuilder::new().id(row_id), |ui| {
                        ui.horizontal(|ui| {
                            let checkbox = ui.add_enabled(
                                native_ui_owner,
                                egui::Checkbox::new(&mut checked, ""),
                            );
                            let approval_identity = search_match_local_identity(
                                selected_file_key,
                                id,
                            );
                            let approval_identity = format!(
                                "approval/{}/{}/{}",
                                search.generation().0,
                                self.preview_revision,
                                approval_identity
                            );
                            register_search_replace_target(
                                file_detail_scope.as_ref(),
                                ui,
                                &checkbox,
                                approval_identity,
                                "Approve match",
                                crate::gui::jump_mode::TargetKind::Checkbox,
                                visual_order,
                            );
                            if process_interactions && checkbox.clicked() {
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
                                    native_ui_owner,
                                    egui::Button::selectable(
                                        self.inspected_match.as_ref() == Some(id),
                                        egui::RichText::new(text).monospace(),
                                    )
                                    .wrap_mode(egui::TextWrapMode::Truncate),
                                );
                                let target_identity = search_match_local_identity(
                                    selected_file_key,
                                    id,
                                );
                                let target_id = file_detail_scope
                                    .as_ref()
                                    .map(|scope| scope.target_id(&target_identity));
                                let directive = register_search_replace_target(
                                    file_detail_scope.as_ref(),
                                    ui,
                                    &response,
                                    &target_identity,
                                    "Inspect match",
                                    crate::gui::jump_mode::TargetKind::Action,
                                    visual_order,
                                );
                                let focused_by_hint = directive
                                    == Some(crate::gui::jump_mode::ui::FocusDirective::Select);
                                let current_match = self.search_result.as_ref().is_some_and(|current| {
                                    current.generation() == search.generation()
                                        && current.matched_files().get(file_index).is_some_and(|current_file| {
                                            current_file.file_identity() == file.file_identity()
                                                && current_file.path() == file.path()
                                                && current_file.matches().iter().any(|candidate| candidate.id() == id)
                                        })
                                        && self.selected_file.as_deref() == Some(file.path())
                                });
                                if process_interactions && current_match && (response.clicked() || focused_by_hint) {
                                    self.inspected_match = Some(id.clone());
                                    if focused_by_hint {
                                        response.request_focus();
                                        if let (Some(scope), Some(target_id)) =
                                            (file_detail_scope.as_ref(), target_id.as_ref())
                                        {
                                            scope.record_select_focus_owner(target_id, response.id);
                                        }
                                    }
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
            self.geometry.match_scroll_id = Some(approvals.id);
        }
        #[cfg(not(test))]
        let _ = approvals;
            });
        register_search_replace_target_at_order(
            file_detail_scope.as_ref(),
            ui,
            &approvals_section.header_response,
            "results/file-approvals",
            "File approvals",
            crate::gui::jump_mode::TargetKind::Button,
            approvals_order,
        );
        #[cfg(test)]
        {
            self.geometry.approvals_open = approvals_section.body_response.is_some();
        }
        #[cfg(not(test))]
        let _ = approvals_section;
        let preview_order = take_target_order(visual_order);
        let preview = egui::CollapsingHeader::new("Original vs. Proposed Preview")
            .id_salt(("replace_preview_section", file.path()))
            .default_open(true)
            .show(ui, |ui| {
                self.draw_diff(ui, file_index);
                if let Some(span) = file.matches().iter().find(|span| self.inspected_match.as_ref() == Some(span.id())) {
                    self.draw_match_inspector(ui, file.path(), span);
                } else {
                    ui.weak("Select a match row to inspect its original and proposed text. Long values are bounded here and disclose the omitted character count.");
                }
            });
        #[cfg(test)]
        {
            self.geometry.preview.borrow_mut().open = preview.body_response.is_some();
        }
        #[cfg(not(test))]
        let _ = preview;
        register_search_replace_target_at_order(
            file_detail_scope.as_ref(),
            ui,
            &preview.header_response,
            "results/original-proposed-preview",
            "Original and proposed preview",
            crate::gui::jump_mode::TargetKind::Button,
            preview_order,
        );
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
                    let original = egui::ScrollArea::both()
                        .id_salt(("replace_match_original", path, span.id()))
                        .auto_shrink([false, false])
                        .min_scrolled_height(self.panel_height(130.0))
            .max_height(self.panel_height(130.0))
                        .show(&mut columns[0], |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(original).monospace())
                                .wrap_mode(egui::TextWrapMode::Extend));
                            if original_truncated {
                                ui.weak(format!("Preview truncated after {MAX_INSPECTED_MATCH_CHARS} characters."));
                            }
                        });
                    let proposed = egui::ScrollArea::both()
                        .id_salt(("replace_match_proposed", path, span.id()))
                        .auto_shrink([false, false])
                        .min_scrolled_height(self.panel_height(130.0))
            .max_height(self.panel_height(130.0))
                        .show(&mut columns[1], |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(proposed).monospace())
                                .wrap_mode(egui::TextWrapMode::Extend));
                            if proposed_truncated {
                                ui.weak(format!("Preview truncated after {MAX_INSPECTED_MATCH_CHARS} characters; the exact candidate remains retained for confirmation."));
                            }
                        });
                    #[cfg(test)] {
                        let mut geometry = self.geometry.preview.borrow_mut();
                        geometry.inspected_original = Some(pane_geometry(&original));
                        geometry.inspected_proposed = Some(pane_geometry(&proposed));
                    }
                    #[cfg(not(test))] let _ = (original, proposed);
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
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        native_ui_owner: bool,
        process_interactions: bool,
        visual_order: &mut u64,
    ) {
        ui.horizontal(|ui| {
            if runtime.active {
                if ui
                    .add_enabled(native_ui_owner, egui::Button::new("Cancel between files"))
                    .clicked()
                    && process_interactions
                {
                    commands.push(SearchReplaceUiCommand::CancelOperation);
                }
                ui.spinner();
            }
            if self.search_ticket.is_some() {
                let cancel = ui.add_enabled(native_ui_owner, egui::Button::new("Cancel search"));
                register_search_replace_target(
                    scope,
                    ui,
                    &cancel,
                    "footer/cancel-search",
                    "Cancel search",
                    crate::gui::jump_mode::TargetKind::Button,
                    visual_order,
                );
                if process_interactions && cancel.clicked() {
                    commands.push(SearchReplaceUiCommand::CancelSearch);
                }
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
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        native_ui_owner: bool,
        process_interactions: bool,
        dialog_instance: u64,
        visual_order: &mut u64,
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
        let section_order = take_target_order(visual_order);
        let section = egui::CollapsingHeader::new("Replacement receipts, Restore, and Recovery")
            .id_salt("replace_receipts")
            .open(self.reveal_receipts.then_some(true))
            .show(ui, |ui| {
                for receipt in self.receipts.iter().rev() {
                    let receipt_identity =
                        egui::Id::new(("search-replace-receipt", receipt.id())).value();
                    let receipt_id = receipt.id().to_owned();
                    let receipt_order = take_target_order(visual_order);
                    let native_id =
                        egui::Id::new(("replace-receipt-row", dialog_instance, receipt_identity));
                    let _ = ui.scope_builder(egui::UiBuilder::new().id(native_id), |row_ui| {
                        let row = row_ui.collapsing(
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
                                    let restore = ui.add_enabled(
                                        native_ui_owner && can_restore,
                                        egui::Button::new("Restore…"),
                                    );
                                    register_search_replace_target(
                                        scope,
                                        ui,
                                        &restore,
                                        format!("receipt/{receipt_identity:016x}/restore"),
                                        "Restore receipt",
                                        crate::gui::jump_mode::TargetKind::Button,
                                        visual_order,
                                    );
                                    if process_interactions && restore.clicked() {
                                        if let Some(scope) = scope {
                                            scope.discard_registered_targets();
                                        }
                                        commands.push(SearchReplaceUiCommand::ReviewRestore(
                                            receipt_id.clone(),
                                        ));
                                    }
                                    let recover = ui.add_enabled(
                                        native_ui_owner && can_recover,
                                        egui::Button::new("Recover interrupted operation…"),
                                    );
                                    register_search_replace_target(
                                        scope,
                                        ui,
                                        &recover,
                                        format!("receipt/{receipt_identity:016x}/recover"),
                                        "Recover receipt",
                                        crate::gui::jump_mode::TargetKind::Button,
                                        visual_order,
                                    );
                                    if process_interactions && recover.clicked() {
                                        if let Some(scope) = scope {
                                            scope.discard_registered_targets();
                                        }
                                        commands.push(SearchReplaceUiCommand::ReviewRecovery(
                                            receipt_id.clone(),
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
                        register_search_replace_target_at_order(
                            scope,
                            row_ui,
                            &row.header_response,
                            format!("receipt/{receipt_identity:016x}/details"),
                            "Receipt details",
                            crate::gui::jump_mode::TargetKind::Button,
                            receipt_order,
                        );
                    });
                }
            });
        register_search_replace_target_at_order(
            scope,
            ui,
            &section.header_response,
            "receipts/section",
            "Replacement receipts, Restore, and Recovery",
            crate::gui::jump_mode::TargetKind::Button,
            section_order,
        );
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
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        native_confirmation_owner: bool,
        first_ui_pass: bool,
    ) {
        #[cfg(test)]
        {
            self.geometry.apply_modal = None;
            self.geometry.recovery_modal = None;
        }
        if self.apply_confirmation_open {
            let Some(review) = self.apply_review.as_ref() else {
                if native_confirmation_owner && first_ui_pass && ctx.current_pass_index() == 0 {
                    self.apply_confirmation_open = false;
                    self.apply_confirmation_instance = None;
                }
                return;
            };
            let mut open = true;
            let mut confirm = false;
            let mut cancel = false;
            let mut visible_pass = false;
            let mut commit_pass = false;
            let mut visual_order = 0;
            let confirmation_instance = self.apply_confirmation_instance.unwrap_or_default();
            let subject_identity = egui::Id::new((
                "search-replace-apply-subject",
                review.search_generation.0,
                review.replacement_revision.0,
            ))
            .value();
            let window = egui::Window::new("Confirm Search and Replace")
                .id(egui::Id::new("replace_apply_confirmation"))
                .collapsible(false)
                .resizable(true)
                .default_size(egui::vec2(760.0, 560.0))
                .min_size(egui::vec2(480.0, 320.0))
                .open(&mut open)
                .show(ctx, |ui| {
                    visible_pass = native_confirmation_owner
                        && ui.is_enabled()
                        && ui.is_visible()
                        && !ui.is_sizing_pass();
                    commit_pass = visible_pass
                        && first_ui_pass
                        && ctx.current_pass_index() == 0;
                    let row = ui.spacing().interact_size.y.max(ui.text_style_height(&egui::TextStyle::Body));
                    let body_height = (ui.available_height() - row - ui.spacing().item_spacing.y * 4.0).max(0.0);
                    let body = egui::ScrollArea::vertical()
                        .id_salt("replace_apply_review_files")
                        .min_scrolled_height(0.0)
                        .max_height(body_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
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
                    let footer_start = ui.cursor().min;
                    ui.separator();
                    if self.apply_confirmation_pending {
                        ui.horizontal(|ui| {
                            let cancel_response = ui
                                .scope_builder(
                                    egui::UiBuilder::new().id(confirmation_action_widget_id(
                                        confirmation_instance,
                                        subject_identity,
                                        "cancel-before-submission",
                                    )),
                                    |button_ui| {
                                        button_ui.add_enabled(
                                            visible_pass,
                                            egui::Button::new("Cancel before submission"),
                                        )
                                    },
                                )
                                .inner;
                            register_search_replace_target(
                                scope,
                                ui,
                                &cancel_response,
                                "confirmation/pending-cancel",
                                "Cancel before submission",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut visual_order,
                            );
                            cancel = cancel_response.clicked();
                            ui.spinner();
                            let message = "Freezing the reviewed candidate on a worker before submitting it to the guarded executor…";
                            ui.add(egui::Label::new(message).truncate()).on_hover_text(message);
                        });
                    } else {
                        ui.horizontal(|ui| {
                            let confirm_response = ui
                                .scope_builder(
                                    egui::UiBuilder::new().id(confirmation_action_widget_id(
                                        confirmation_instance,
                                        subject_identity,
                                        "confirm-apply",
                                    )),
                                    |button_ui| {
                                        button_ui.add_enabled(
                                            visible_pass
                                                && !runtime.active
                                                && self.candidate.is_some()
                                                && !self.preview_pending,
                                            egui::Button::new("Confirm and Apply"),
                                        )
                                    },
                                )
                                .inner;
                            register_search_replace_target(
                                scope,
                                ui,
                                &confirm_response,
                                "confirmation/confirm",
                                "Confirm and Apply",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut visual_order,
                            );
                            confirm = confirm_response.clicked();
                            let cancel_response = ui
                                .scope_builder(
                                    egui::UiBuilder::new().id(confirmation_action_widget_id(
                                        confirmation_instance,
                                        subject_identity,
                                        "cancel-apply",
                                    )),
                                    |button_ui| {
                                        button_ui.add_enabled(
                                            visible_pass,
                                            egui::Button::new("Cancel"),
                                        )
                                    },
                                )
                                .inner;
                            register_search_replace_target(
                                scope,
                                ui,
                                &cancel_response,
                                "confirmation/cancel",
                                "Cancel",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut visual_order,
                            );
                            cancel = cancel_response.clicked();
                        });
                    }
                    #[cfg(test)] {
                        self.geometry.apply_modal = Some(ConfirmationGeometry {
                            window: egui::Rect::NOTHING, body: body.inner_rect,
                            footer: egui::Rect::from_min_max(footer_start, ui.min_rect().max),
                            content: body.content_size, body_id: body.id, offset: body.state.offset.y,
                        });
                    }
                    #[cfg(not(test))] let _ = (body, footer_start);
                });
            #[cfg(test)]
            if let Some(window) = window
                && let Some(geometry) = self.geometry.apply_modal.as_mut()
            {
                geometry.window = window.response.rect;
            }
            #[cfg(not(test))]
            let _ = window;
            let close_committed = native_confirmation_owner
                && first_ui_pass
                && ctx.current_pass_index() == 0
                && !open;
            if commit_pass && confirm {
                commands.push(SearchReplaceUiCommand::ConfirmApply);
            } else if (commit_pass && cancel) || close_committed {
                self.cancel_confirmation();
                if let Some(scope) = scope {
                    scope.discard_registered_targets();
                }
                commands.push(SearchReplaceUiCommand::CancelApply);
            }
        }
        if let Some(review) = self.recovery_review.as_ref() {
            let mut open = true;
            let mut confirm = false;
            let mut cancel = false;
            let mut visible_pass = false;
            let mut commit_pass = false;
            let mut visual_order = 0;
            let confirmation_instance = self.recovery_confirmation_instance.unwrap_or_default();
            let subject_identity = egui::Id::new((
                "search-replace-receipt-subject",
                review.transaction_id.as_str(),
                review.action as u8,
            ))
            .value();
            let title = match review.action {
                RecoveryAction::Restore => "Confirm replacement Restore",
                RecoveryAction::Recover => "Confirm replacement Recovery",
            };
            let window = egui::Window::new(title)
                .id(egui::Id::new("replace_recovery_confirmation"))
                .collapsible(false)
                .resizable(true)
                .default_size(egui::vec2(620.0, 420.0))
                .min_size(egui::vec2(440.0, 260.0))
                .open(&mut open)
                .show(ctx, |ui| {
                    visible_pass = native_confirmation_owner
                        && ui.is_enabled()
                        && ui.is_visible()
                        && !ui.is_sizing_pass();
                    commit_pass = visible_pass
                        && first_ui_pass
                        && ctx.current_pass_index() == 0;
                    let row = ui.spacing().interact_size.y.max(ui.text_style_height(&egui::TextStyle::Body));
                    let body_height = (ui.available_height() - row - ui.spacing().item_spacing.y * 4.0).max(0.0);
                    let body = egui::ScrollArea::vertical()
                        .id_salt("replace_recovery_review_body")
                        .min_scrolled_height(0.0)
                        .max_height(body_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
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

                        });
                    let footer_start = ui.cursor().min;
                    ui.separator();
                    ui.horizontal(|ui| {
                    let confirm_label = match review.action {
                        RecoveryAction::Restore => "Confirm Restore",
                        RecoveryAction::Recover => "Confirm Recovery",
                    };
                    let confirm_response = ui.scope_builder(
                        egui::UiBuilder::new().id(confirmation_action_widget_id(
                            confirmation_instance,
                            subject_identity,
                            "confirm-receipt",
                        )),
                        |button_ui| {
                            button_ui.add_enabled(
                                visible_pass && !runtime.active,
                                egui::Button::new(confirm_label),
                            )
                        },
                    ).inner;
                    register_search_replace_target(
                        scope,
                        ui,
                        &confirm_response,
                        "confirmation/confirm",
                        confirm_label,
                        crate::gui::jump_mode::TargetKind::Button,
                        &mut visual_order,
                    );
                    confirm = confirm_response.clicked();
                    let cancel_response = ui.scope_builder(
                        egui::UiBuilder::new().id(confirmation_action_widget_id(
                            confirmation_instance,
                            subject_identity,
                            "cancel-receipt",
                        )),
                        |button_ui| {
                            button_ui.add_enabled(visible_pass, egui::Button::new("Cancel"))
                        },
                    ).inner;
                    register_search_replace_target(
                        scope,
                        ui,
                        &cancel_response,
                        "confirmation/cancel",
                        "Cancel",
                        crate::gui::jump_mode::TargetKind::Button,
                        &mut visual_order,
                    );
                    cancel = cancel_response.clicked();
                    });
                    #[cfg(test)] {
                        self.geometry.recovery_modal = Some(ConfirmationGeometry {
                            window: egui::Rect::NOTHING, body: body.inner_rect,
                            footer: egui::Rect::from_min_max(footer_start, ui.min_rect().max),
                            content: body.content_size, body_id: body.id, offset: body.state.offset.y,
                        });
                    }
                    #[cfg(not(test))] let _ = (body, footer_start);
                });
            #[cfg(test)]
            if let Some(window) = window
                && let Some(geometry) = self.geometry.recovery_modal.as_mut()
            {
                geometry.window = window.response.rect;
            }
            #[cfg(not(test))]
            let _ = window;
            if commit_pass && confirm {
                if let Some(scope) = scope {
                    scope.discard_registered_targets();
                }
                commands.push(match review.action {
                    RecoveryAction::Restore => SearchReplaceUiCommand::ConfirmRestore,
                    RecoveryAction::Recover => SearchReplaceUiCommand::ConfirmRecovery,
                });
            } else if (commit_pass && cancel)
                || (native_confirmation_owner
                    && first_ui_pass
                    && ctx.current_pass_index() == 0
                    && !open)
            {
                self.recovery_review = None;
                self.recovery_confirmation_instance = None;
                if let Some(scope) = scope {
                    scope.discard_registered_targets();
                }
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
                    let rect = value.galley.rect.translate(value.pos.to_vec2());
                    // Keep the full galley bounds only when actually visible.
                    // Intersecting first makes a clipped sliver indistinguishable
                    // from a complete edge-aligned label to click helpers.
                    if rect.is_positive()
                        && rect.left() >= clip.left() - 0.5
                        && rect.right() <= clip.right() + 0.5
                        && rect.top() >= clip.top() - 0.5
                        && rect.bottom() <= clip.bottom() + 0.5
                    {
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

    fn click_body_command(
        state: &mut SearchReplaceDialogState,
        ctx: &egui::Context,
        size: egui::Vec2,
        label: &str,
    ) -> Vec<SearchReplaceUiCommand> {
        let runtime = ReplaceWorkerSnapshot::default();
        let id = state.geometry.body_id.unwrap();
        let mut scroll = egui::scroll_area::State::load(ctx, id).unwrap();
        scroll.offset.y = 0.0;
        scroll.store(ctx, id);
        let mut position = None;
        // Cover the actual current body range, including receipts below large
        // result lists, rather than assuming a fixed number of scan steps.
        let scroll_range = (state.geometry.content.y - state.geometry.body.height()).max(0.0);
        let scan_steps = (scroll_range / 30.0).ceil() as usize + 5;
        let mut last_visible = Vec::new();
        for _ in 0..scan_steps {
            let (commands, text) = layout_frame(state, ctx, size, &runtime, vec![], true);
            assert!(commands.is_empty());
            let fully_visible = |rect: egui::Rect, body: egui::Rect| {
                rect.left() >= body.left() - 0.5
                    && rect.right() <= body.right() + 0.5
                    && rect.top() >= body.top() - 0.5
                    && rect.bottom() <= body.bottom() + 0.5
            };
            if let Some((_, rect)) = text.iter().find(|(text, rect)| {
                text.starts_with(label) && fully_visible(*rect, state.geometry.body)
            }) {
                // Require full published bounds and a stable second frame.
                // A final header may legitimately align with the content edge.
                let prior = *rect;
                let (commands, settled) = layout_frame(state, ctx, size, &runtime, vec![], true);
                assert!(commands.is_empty());
                if let Some((_, rect)) = settled.iter().find(|(text, rect)| {
                    text.starts_with(label) && fully_visible(*rect, state.geometry.body)
                }) && (rect.top() - prior.top()).abs() < 1.0
                    && (rect.left() - prior.left()).abs() < 1.0
                {
                    position = Some(rect.center());
                    break;
                }
                continue;
            }
            last_visible = text;
            let mut scroll = egui::scroll_area::State::load(ctx, id).unwrap();
            scroll.offset.y += 30.0;
            scroll.store(ctx, id);
        }
        let pos = position.unwrap_or_else(|| {
            panic!(
                "missing body label {label}; body={:?}, content={:?}, offset={}, range={scroll_range}, steps={scan_steps}, last visible={last_visible:?}",
                state.geometry.body, state.geometry.content, state.geometry.offset
            )
        });
        let mut commands = Vec::new();
        for pressed in [true, false] {
            commands.extend(
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
                        },
                    ],
                    true,
                )
                .0,
            );
        }
        for _ in 0..3 {
            layout_frame(state, ctx, size, &runtime, vec![], true);
        }
        commands
    }

    fn click_body_label(
        state: &mut SearchReplaceDialogState,
        ctx: &egui::Context,
        size: egui::Vec2,
        label: &str,
    ) {
        assert!(click_body_command(state, ctx, size, label).is_empty());
    }

    fn click_fixed_label(
        state: &mut SearchReplaceDialogState,
        ctx: &egui::Context,
        size: egui::Vec2,
        runtime: &ReplaceWorkerSnapshot,
        label: &str,
        allow: bool,
    ) -> Vec<SearchReplaceUiCommand> {
        for _ in 0..2 {
            assert!(
                layout_frame(state, ctx, size, runtime, vec![], allow)
                    .0
                    .is_empty()
            );
        }
        let (_, text) = layout_frame(state, ctx, size, runtime, vec![], allow);
        let pos = text
            .iter()
            .find(|(text, _)| text == label)
            .unwrap_or_else(|| panic!("missing fixed {label}: {text:?}"))
            .1
            .center();
        let mut commands = Vec::new();
        for pressed in [true, false] {
            commands.extend(
                layout_frame(
                    state,
                    ctx,
                    size,
                    runtime,
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
        commands
    }

    #[test]
    fn replace_actions_and_confirmation_footers_stay_reachable_without_duplicate_submission() {
        let parent = PathBuf::from("target/refinement-evidence");
        std::fs::create_dir_all(&parent).unwrap();
        let fixture = tempfile::Builder::new()
            .prefix("replace_actions_")
            .tempdir_in(parent)
            .unwrap();
        let root = fixture.path().join("explicit-root");
        std::fs::create_dir_all(&root).unwrap();
        let mut originals = Vec::new();
        for index in 0..20 {
            let path = root.join(format!("owned-{index:02}.txt"));
            let text = "needle\r\nneedle\r\n";
            std::fs::write(&path, text).unwrap();
            originals.push((path, text));
        }
        let mut state = SearchReplaceDialogState::default();
        state.open();
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
        let search = state.search_result.clone().unwrap();
        assert_eq!(search.status(), SearchStatus::Complete);
        assert_eq!(search.matched_files().len(), 20);
        let ids: HashSet<_> = search
            .matched_files()
            .iter()
            .flat_map(|file| file.matches())
            .map(|span| span.id().clone())
            .collect();
        for id in &ids {
            state.set_match_selected(id, true);
        }
        wait_for_preview(&mut state, &coordinator, &worker, |state| {
            state.candidate.is_some() && !state.preview_pending
        });
        for (id, status) in [
            ("owned-restore", "applied"),
            ("owned-recovery", "recovery_required"),
        ] {
            state.receipts.push(serde_json::from_value(serde_json::json!({
                "id": id, "search_revision": 1, "search_generation": 1, "replacement_revision": 1,
                "status": status, "entries": [], "message": "Owned retained receipt"
            })).unwrap());
        }
        let receipts = state.receipts.clone();
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let size = egui::vec2(600.0, 420.0);
        let runtime = ReplaceWorkerSnapshot::default();
        for _ in 0..4 {
            layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
        }
        assert_eq!(
            click_fixed_label(
                &mut state,
                &ctx,
                size,
                &runtime,
                "Review Apply\u{2026}",
                true
            ),
            vec![SearchReplaceUiCommand::ReviewApply]
        );
        let assert_modal = |geometry: &ConfirmationGeometry, size: egui::Vec2| {
            assert!(
                geometry.window.width() <= size.x + 1.0 && geometry.window.height() <= size.y + 1.0,
                "{geometry:?}"
            );
            assert!(
                geometry.footer.bottom() <= size.y + 1.0
                    && geometry.footer.top() >= geometry.body.bottom(),
                "{geometry:?}"
            );
            assert!(
                geometry.body.height() >= 80.0 && geometry.body.width() > 200.0,
                "{geometry:?}"
            );
        };
        for viewport in [size, egui::vec2(1120.0, 820.0), size] {
            for _ in 0..4 {
                layout_frame(&mut state, &ctx, viewport, &runtime, vec![], true);
                assert_modal(state.geometry.apply_modal.as_ref().unwrap(), viewport);
            }
        }
        let modal = state.geometry.apply_modal.clone().unwrap();
        assert!(modal.content.y > modal.body.height());
        let outer_offset = state.geometry.offset;
        layout_frame(
            &mut state,
            &ctx,
            size,
            &runtime,
            vec![
                egui::Event::PointerMoved(modal.body.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -240.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            true,
        );
        for _ in 0..30 {
            layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
        }
        assert!(state.geometry.apply_modal.as_ref().unwrap().offset > 0.0);
        assert_eq!(
            state.geometry.offset, outer_offset,
            "modal wheel does not scroll disabled background"
        );
        let body_id = modal.body_id;
        let mut scroll = egui::scroll_area::State::load(&ctx, body_id).unwrap();
        scroll.offset.y = (modal.content.y - modal.body.height()).max(0.0);
        scroll.store(&ctx, body_id);
        assert!(
            click_fixed_label(&mut state, &ctx, size, &runtime, "Confirm and Apply", false)
                .is_empty()
        );
        let (_, text) = layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
        let pos = text
            .iter()
            .find(|(text, _)| text == "Confirm and Apply")
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
        let mut passes = 0;
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
                passes += 1;
                if ctx.current_pass_index() == 0 {
                    ctx.request_discard("replace confirmation single-command guard");
                }
            },
        );
        output.drop_without_applying_deltas();
        assert!(passes > 1);
        assert_eq!(commands, vec![SearchReplaceUiCommand::ConfirmApply]);
        let frozen_job = state
            .take_confirm_job()
            .expect("existing confirmation handoff");
        assert!(
            state.take_confirm_job().is_none(),
            "one freeze job per confirmation"
        );
        drop(frozen_job); // No writer/executor is submitted by this UI fixture.
        for _ in 0..3 {
            layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
        }
        assert_modal(state.geometry.apply_modal.as_ref().unwrap(), size);
        assert_eq!(
            click_fixed_label(
                &mut state,
                &ctx,
                size,
                &runtime,
                "Cancel before submission",
                true
            ),
            vec![SearchReplaceUiCommand::CancelApply]
        );
        assert!(!state.apply_confirmation_open && !state.apply_confirmation_pending);
        assert!(state.take_confirmed_request().is_none());
        assert_eq!(state.selected_matches, ids);
        assert_eq!(state.receipts, receipts);

        assert!(
            click_fixed_label(&mut state, &ctx, size, &runtime, "Restore / Recovery", true)
                .is_empty()
        );
        for _ in 0..3 {
            layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
        }
        for (id, action, entry, confirm, command) in [
            (
                "owned-restore",
                RecoveryAction::Restore,
                "Restore\u{2026}",
                "Confirm Restore",
                SearchReplaceUiCommand::ConfirmRestore,
            ),
            (
                "owned-recovery",
                RecoveryAction::Recover,
                "Recover interrupted operation\u{2026}",
                "Confirm Recovery",
                SearchReplaceUiCommand::ConfirmRecovery,
            ),
        ] {
            click_body_label(&mut state, &ctx, size, id);
            let expected = if action == RecoveryAction::Restore {
                SearchReplaceUiCommand::ReviewRestore(id.into())
            } else {
                SearchReplaceUiCommand::ReviewRecovery(id.into())
            };
            assert_eq!(
                click_body_command(&mut state, &ctx, size, entry),
                vec![expected]
            );
            state.review_recovery(id.into(), action); // Same existing owner called by the app command handler.
            assert_eq!(state.recovery_review.as_ref().unwrap().transaction_id, id);
            state.recovery_review.as_mut().unwrap().transaction_id =
                "owned-long-recovery-metadata/".repeat(70);
            for _ in 0..4 {
                layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
                assert_modal(state.geometry.recovery_modal.as_ref().unwrap(), size);
            }
            let recovery = state.geometry.recovery_modal.as_ref().unwrap();
            assert!(recovery.content.y > recovery.body.height());
            assert!(click_fixed_label(&mut state, &ctx, size, &runtime, confirm, false).is_empty());
            assert_eq!(
                click_fixed_label(&mut state, &ctx, size, &runtime, confirm, true),
                vec![command]
            );
            assert_eq!(
                click_fixed_label(&mut state, &ctx, size, &runtime, "Cancel", true),
                vec![SearchReplaceUiCommand::CancelRecoveryAction]
            );
            assert!(state.recovery_review.is_none());
        }

        click_body_label(&mut state, &ctx, size, "Search for");
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("replace_search_pattern")));
        let focused = ctx.memory(|memory| memory.focused());
        assert!(
            layout_frame(
                &mut state,
                &ctx,
                size,
                &runtime,
                vec![egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE
                }],
                true
            )
            .0
            .is_empty()
        );
        assert_ne!(ctx.memory(|memory| memory.focused()), focused);
        assert_eq!(state.pattern, "needle");
        assert_eq!(state.selected_matches, ids);
        let request = state.build_search_request().unwrap();
        state.record_search_ticket(coordinator.submit(request).unwrap());
        let active = ReplaceWorkerSnapshot {
            active: true,
            ..Default::default()
        };
        assert_eq!(
            click_fixed_label(&mut state, &ctx, size, &active, "Cancel search", true),
            vec![SearchReplaceUiCommand::CancelSearch]
        );
        assert_eq!(
            click_fixed_label(
                &mut state,
                &ctx,
                size,
                &active,
                "Cancel between files",
                true
            ),
            vec![SearchReplaceUiCommand::CancelOperation]
        );
        state.cancel_search();
        assert_eq!(
            click_fixed_label(&mut state, &ctx, size, &runtime, "Close", true),
            vec![SearchReplaceUiCommand::Closed]
        );
        assert!(!state.open);
        state.open();
        for viewport in [egui::vec2(1120.0, 820.0), size] {
            for _ in 0..4 {
                assert!(
                    layout_frame(&mut state, &ctx, viewport, &runtime, vec![], true)
                        .0
                        .is_empty()
                );
            }
            assert!(state.geometry.footer.bottom() <= viewport.y + 1.0);
        }
        assert!(
            state.search_ticket.is_none()
                && !state.apply_confirmation_open
                && state.recovery_review.is_none()
        );
        assert_eq!(state.pattern, "needle");
        assert_eq!(state.receipts, receipts);
        for (path, original) in originals {
            assert_eq!(std::fs::read_to_string(path).unwrap(), original);
        }
    }

    #[test]
    fn replace_preview_folding_keeps_exact_multiline_candidate_and_independent_long_line_scrolls() {
        let parent = PathBuf::from("target/refinement-evidence");
        std::fs::create_dir_all(&parent).unwrap();
        let fixture = tempfile::Builder::new()
            .prefix("replace_preview_")
            .tempdir_in(parent)
            .unwrap();
        let root = fixture.path().join("explicit-root");
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("owned.txt");
        let other_source = root.join("other.txt");
        let long = "漢".repeat(4_500);
        let filler = "unchanged\r\n".repeat(300);
        let original = format!("\u{feff}{long} needle\r\nfirst\r\nsecond\r\n{filler}{long} needle");
        std::fs::write(&source, original.as_bytes()).unwrap();
        std::fs::write(&other_source, "漢 needle").unwrap();
        let replacement = "置".repeat(5_000);
        let mut state = SearchReplaceDialogState::default();
        state.open = true;
        state.set_root(root);
        state.include_extensions = "txt".into();
        state.pattern = r"漢+ needle|first\r\nsecond".into();
        state.pattern_mode = SearchPatternMode::RustRegex;
        state.replacement = replacement.clone();
        let coordinator = crate::search_replace::SearchCoordinator::new().unwrap();
        let worker = ReplacementPreviewWorker::new().unwrap();
        let request = state.build_search_request().unwrap();
        state.record_search_ticket(coordinator.submit(request).unwrap());
        wait_for_preview(&mut state, &coordinator, &worker, |state| {
            state.candidate.is_some() && !state.preview_pending
        });
        let search = state.search_result.clone().unwrap();
        assert_eq!(search.status(), SearchStatus::Complete);
        let main = search
            .matched_files()
            .iter()
            .find(|file| file.path().file_name().unwrap() == "owned.txt")
            .unwrap();
        let other = search
            .matched_files()
            .iter()
            .find(|file| file.path().file_name().unwrap() == "other.txt")
            .unwrap();
        assert_eq!(main.matches().len(), 3);
        let ids: Vec<_> = main
            .matches()
            .iter()
            .map(|span| span.id().clone())
            .collect();
        for id in &ids {
            state.set_match_selected(id, true);
        }
        wait_for_preview(&mut state, &coordinator, &worker, |state| {
            state.candidate.is_some() && !state.preview_pending
        });
        state.selected_file = Some(main.path().to_path_buf());
        state.inspected_match = Some(ids[2].clone());
        let candidate_file = state
            .candidate
            .as_ref()
            .unwrap()
            .files()
            .iter()
            .find(|file| file.original_snapshot().path() == main.path())
            .unwrap();
        assert_eq!(
            candidate_file
                .match_output(&ids[1])
                .unwrap()
                .original_text(),
            "first\r\nsecond"
        );
        assert_eq!(
            candidate_file
                .match_output(&ids[1])
                .unwrap()
                .proposed_text(),
            replacement
        );
        assert_eq!(
            candidate_file
                .match_output(&ids[2])
                .unwrap()
                .original_text(),
            format!("{long} needle")
        );
        assert_eq!(
            candidate_file
                .match_output(&ids[2])
                .unwrap()
                .proposed_text(),
            replacement
        );
        let bytes = candidate_file.output_bytes().clone();
        assert_eq!(
            bytes.as_ref(),
            format!("\u{feff}{replacement}\r\n{replacement}\r\n{filler}{replacement}").as_bytes()
        );
        assert!(candidate_file.diff_context().unwrap().truncated());
        let cache = state
            .candidate_unified_diffs
            .get(main.path())
            .unwrap()
            .clone();
        let revisions = (
            state.preview_revision,
            state.preview_result_publications,
            state.current_generation,
        );
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let size = egui::vec2(600.0, 420.0);
        let runtime = ReplaceWorkerSnapshot::default();
        for _ in 0..4 {
            assert!(
                layout_frame(&mut state, &ctx, size, &runtime, vec![], true)
                    .0
                    .is_empty()
            );
            assert!(
                state.geometry.window.width() <= size.x + 1.0
                    && state.geometry.window.height() <= size.y + 1.0
            );
        }
        let panes = state.geometry.preview.borrow().clone();
        assert!(panes.open);
        let left = panes.original.unwrap();
        let right = panes.proposed.unwrap();
        let unified = panes.unified.unwrap();
        for pane in [&left, &right, &unified] {
            assert!(
                pane.rect.width() > 40.0
                    && pane.rect.height() > 40.0
                    && pane.rect.height() <= 151.0
            );
            assert!(
                pane.content.x > pane.rect.width() + 200.0,
                "long lines scroll instead of widening dialog: {pane:?}"
            );
        }
        assert_ne!(left.id, right.id);
        for (id, x) in [(left.id, 200.0), (right.id, 80.0), (unified.id, 120.0)] {
            let mut scroll = egui::scroll_area::State::load(&ctx, id).unwrap();
            scroll.offset.x = x;
            scroll.store(&ctx, id);
        }
        for _ in 0..3 {
            layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
        }
        let panes = state.geometry.preview.borrow().clone();
        assert!((panes.original.unwrap().offset.x - 200.0).abs() < 1.0);
        assert!((panes.proposed.unwrap().offset.x - 80.0).abs() < 1.0);
        assert!((panes.unified.unwrap().offset.x - 120.0).abs() < 1.0);

        click_body_label(&mut state, &ctx, size, "Inspect match 3");
        let panes = state.geometry.preview.borrow().clone();
        let inspected = panes.inspected_original.unwrap();
        let proposed = panes.inspected_proposed.unwrap();
        assert!(
            inspected.content.x > inspected.rect.width()
                && proposed.content.x > proposed.rect.width()
        );
        assert_ne!(inspected.id, proposed.id);
        click_body_label(&mut state, &ctx, size, "Preview truncated after");
        let (_, visible) = layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
        assert!(
            visible
                .iter()
                .any(|(text, _)| text.contains("Preview truncated after 4096 characters"))
        );
        click_body_label(&mut state, &ctx, size, "Original vs. Proposed Preview");
        let closed = state.geometry.preview.borrow().clone();
        assert!(!closed.open && closed.original.is_none() && closed.inspected_original.is_none());
        click_body_label(&mut state, &ctx, size, "Original vs. Proposed Preview");
        assert_eq!(
            state
                .geometry
                .preview
                .borrow()
                .original
                .as_ref()
                .unwrap()
                .id,
            left.id
        );
        assert_eq!(
            state
                .geometry
                .preview
                .borrow()
                .inspected_original
                .as_ref()
                .unwrap()
                .id,
            inspected.id
        );
        assert!(Arc::ptr_eq(&search, state.search_result.as_ref().unwrap()));
        assert!(Arc::ptr_eq(
            &cache,
            state.candidate_unified_diffs.get(main.path()).unwrap()
        ));
        let current = state
            .candidate
            .as_ref()
            .unwrap()
            .files()
            .iter()
            .find(|file| file.original_snapshot().path() == main.path())
            .unwrap();
        assert!(Arc::ptr_eq(&bytes, current.output_bytes()));
        assert_eq!(
            (
                state.preview_revision,
                state.preview_result_publications,
                state.current_generation
            ),
            revisions
        );
        assert_eq!(
            state.selected_matches,
            ids.iter().cloned().collect::<HashSet<_>>()
        );

        state.inspected_match = Some(ids[1].clone());
        click_body_label(&mut state, &ctx, size, "Inspect match 2");
        assert_ne!(
            state
                .geometry
                .preview
                .borrow()
                .inspected_original
                .as_ref()
                .unwrap()
                .id,
            inspected.id
        );
        state.selected_file = Some(other.path().to_path_buf());
        layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
        let other_geometry = state.geometry.preview.borrow().clone();
        assert!(other_geometry.original.is_none() && other_geometry.inspected_original.is_none());
        state.selected_file = None;
        layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
        assert!(!state.geometry.preview.borrow().open);
        state.selected_file = Some(main.path().to_path_buf());
        state.queue_candidate_recompute();
        state.candidate_error = Some("Owned stale preview failure".into());
        layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
        let stale = state.geometry.preview.borrow().clone();
        assert!(
            stale.original.is_none()
                && stale.proposed.is_none()
                && stale.unified.is_none()
                && stale.inspected_original.is_none()
        );
        assert!(state.candidate_unified_diffs.is_empty());
        state.invalidate_search("Owned source revision invalidation".into());
        layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
        assert!(!state.geometry.preview.borrow().open);
        assert_eq!(std::fs::read(source).unwrap(), original.as_bytes());
        assert_eq!(std::fs::read_to_string(other_source).unwrap(), "漢 needle");
    }

    #[test]
    fn replace_result_folds_filter_sort_and_virtual_scroll_preserve_exact_approvals() {
        let parent = PathBuf::from("target/refinement-evidence");
        std::fs::create_dir_all(&parent).unwrap();
        let fixture = tempfile::Builder::new()
            .prefix("replace_results_")
            .tempdir_in(parent)
            .unwrap();
        let root = fixture.path().join("explicit-root");
        std::fs::create_dir_all(&root).unwrap();
        let mut originals = Vec::new();
        for index in 0..80 {
            let path = root.join(format!("file{index:03}.txt"));
            let text = "needle\n".repeat(if index == 79 { 600 } else { 4 });
            std::fs::write(&path, &text).unwrap();
            originals.push((path, text));
        }
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
        let search = state.search_result.clone().unwrap();
        assert_eq!(search.status(), SearchStatus::Complete);
        assert!(search.summary().match_list_complete());
        assert_eq!(search.matched_files().len(), 80);
        let main_index = search
            .matched_files()
            .iter()
            .position(|file| file.path().file_name().unwrap() == "file079.txt")
            .unwrap();
        let other_index = search
            .matched_files()
            .iter()
            .position(|file| file.path().file_name().unwrap() == "file000.txt")
            .unwrap();
        let main = &search.matched_files()[main_index];
        let other = &search.matched_files()[other_index];
        assert_eq!(main.matches().len(), 600);
        let main_ids: HashSet<_> = [0, 350, 599]
            .into_iter()
            .map(|index| main.matches()[index].id().clone())
            .collect();
        let mut expected = main_ids.clone();
        expected.extend(other.matches().iter().take(2).map(|span| span.id().clone()));
        state.selected_matches = expected.clone();
        state.selected_file = Some(main.path().to_path_buf());
        state.inspected_match = Some(main.matches()[350].id().clone());
        let inspected = state.inspected_match.clone();
        let before = (
            state.search_revision,
            state.preview_revision,
            state.current_generation,
        );
        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let size = egui::vec2(600.0, 420.0);
        let runtime = ReplaceWorkerSnapshot::default();
        for _ in 0..4 {
            layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
        }
        assert!(state.geometry.files_open && state.geometry.approvals_open);
        assert!((1..80).contains(&state.geometry.file_rows_rendered));
        assert!((1..600).contains(&state.geometry.match_rows_rendered));
        let file_id = state.geometry.file_scroll_id.unwrap();
        let match_id = state.geometry.match_scroll_id.unwrap();
        for (id, offset) in [
            (file_id, FILE_ROW_HEIGHT * 70.0),
            (match_id, MATCH_ROW_HEIGHT * 550.0),
        ] {
            let mut scroll = egui::scroll_area::State::load(&ctx, id).unwrap();
            scroll.offset.y = offset;
            scroll.store(&ctx, id);
        }
        for _ in 0..3 {
            layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
        }
        assert!(state.geometry.first_file_row > 50 && state.geometry.first_match_row > 400);
        assert!((1..80).contains(&state.geometry.file_rows_rendered));
        assert!((1..600).contains(&state.geometry.match_rows_rendered));

        click_body_label(&mut state, &ctx, size, "Files with Matches");
        assert!(!state.geometry.files_open && state.geometry.approvals_open);
        assert_eq!(state.geometry.files, egui::Rect::NOTHING);
        assert_eq!(state.geometry.file_rows_rendered, 0);
        click_body_label(&mut state, &ctx, size, "Files with Matches");
        click_body_label(&mut state, &ctx, size, "File Approvals");
        assert!(state.geometry.files_open && !state.geometry.approvals_open);
        assert_eq!(state.geometry.approvals, egui::Rect::NOTHING);
        assert_eq!(state.geometry.match_rows_rendered, 0);
        click_body_label(&mut state, &ctx, size, "File Approvals");
        assert_eq!(state.geometry.file_scroll_id, Some(file_id));
        assert_eq!(state.geometry.match_scroll_id, Some(match_id));
        assert_eq!(state.selected_matches, expected);
        assert_eq!(state.selected_file.as_deref(), Some(main.path()));
        assert_eq!(state.inspected_match, inspected);

        click_body_label(&mut state, &ctx, size, "Path order");
        let (_, text) = layout_frame(&mut state, &ctx, size, &runtime, vec![], true);
        let pos = text
            .iter()
            .find(|(text, _)| text == "Match count")
            .expect("actual sort popup")
            .1
            .center();
        for pressed in [true, false] {
            assert!(
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
                            pressed,
                            modifiers: egui::Modifiers::NONE
                        }
                    ],
                    true
                )
                .0
                .is_empty()
            );
        }
        assert_eq!(state.file_sort, FileSort::MatchCount);
        assert_eq!(state.visible_file_indices[0], main_index);
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("replace_file_filter")));
        assert!(
            layout_frame(
                &mut state,
                &ctx,
                size,
                &runtime,
                vec![egui::Event::Text("file000".into())],
                true
            )
            .0
            .is_empty()
        );
        assert_eq!(state.visible_file_indices, vec![other_index]);
        assert_eq!(
            state.selected_file.as_deref(),
            Some(main.path()),
            "filtering does not rebind current file"
        );
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("replace_file_filter")));
        layout_frame(
            &mut state,
            &ctx,
            size,
            &runtime,
            vec![egui::Event::Text("ignored".into())],
            false,
        );
        assert_eq!(state.file_filter, "file000");
        assert_eq!(state.selected_matches, expected);
        assert!(Arc::ptr_eq(state.search_result.as_ref().unwrap(), &search));
        assert_eq!(
            (
                state.search_revision,
                state.preview_revision,
                state.current_generation
            ),
            before
        );

        click_body_label(&mut state, &ctx, size, "Clear visible matches");
        assert_eq!(
            state.selected_matches, main_ids,
            "only filtered file IDs clear"
        );
        click_body_label(&mut state, &ctx, size, "Select visible matches");
        expected = main_ids;
        expected.extend(other.matches().iter().map(|span| span.id().clone()));
        assert_eq!(
            state.selected_matches, expected,
            "only filtered file IDs approve"
        );
        assert!(state.search_ticket.is_none());
        for (path, original) in originals {
            assert_eq!(std::fs::read_to_string(path).unwrap(), original);
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
