//! App-owned worker for explicitly started, immutable transfer approvals.

use super::receipts::store::{QueueAction, QueuePreview, ReceiptStore};
use rand::TryRng;
use std::collections::VecDeque;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::activity::{
    ActivityCenter, ActivityDiagnostic, ActivityIncarnation, ActivityKey, ActivityLifecycle,
    ActivityOperation, ActivityOutcome, ActivityOwner, ActivityProgress, ActivityProgressUnit,
    ActivityRetirement, ActivitySlot, ActivityTask,
};

use super::{
    ApprovedTransferPlan, CopyControl, CopyError, CopyExecution, CopyFailure, CopyPauseState,
    DirectorySourceOutcome, DirectoryTimes, EntryKind, ExecutionProgress, ExecutionState,
    SourceNodeId, TransferDraft, execute_copy_plan,
};

pub const TRANSFER_QUEUE_LIMIT: usize = 32;
const JOB_HISTORY_LIMIT: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TransferReceiptId(pub [u8; 16]);
impl fmt::Display for TransferReceiptId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TransferJobId(u64);

impl TransferJobId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferJobPhase {
    Preparing,
    Transferring,
    Verifying,
    Finalizing,
    RemovingSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferJobStatus {
    Queued,
    Preparing,
    Running,
    PauseRequested,
    Paused,
    Cancelling,
    Completed,
    Partial,
    Cancelled,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferJobProgress {
    pub phase: TransferJobPhase,
    pub current: Option<SourceNodeId>,
    pub staged_bytes: u64,
    pub completed_files: u64,
    pub completed_bytes: u64,
    pub total_files: u64,
    pub total_bytes: Option<u64>,
}

impl TransferJobProgress {
    fn new(plan: &ApprovedTransferPlan) -> Self {
        let (total_files, total_bytes) = plan
            .entries()
            .iter()
            .filter(|entry| entry.source().metadata().kind == EntryKind::File)
            .fold((0, Some(0_u64)), |(files, total), entry| {
                let len = entry.source().metadata().byte_len;
                (
                    files + 1,
                    total.zip(len).and_then(|(sum, len)| sum.checked_add(len)),
                )
            });
        Self {
            phase: TransferJobPhase::Preparing,
            current: None,
            staged_bytes: 0,
            completed_files: 0,
            completed_bytes: 0,
            total_files,
            total_bytes,
        }
    }

    fn observe(&mut self, event: ExecutionProgress) {
        match event {
            ExecutionProgress::Preparing(entry) => {
                self.phase = TransferJobPhase::Preparing;
                self.current = Some(entry);
                self.staged_bytes = 0;
            }
            ExecutionProgress::Copying {
                entry,
                staged_bytes,
            } => {
                self.phase = TransferJobPhase::Transferring;
                self.current = Some(entry);
                self.staged_bytes = staged_bytes;
            }
            ExecutionProgress::Verifying {
                entry,
                verification_requested,
            } => {
                self.phase = if verification_requested {
                    TransferJobPhase::Verifying
                } else {
                    TransferJobPhase::Finalizing
                };
                self.current = Some(entry);
                self.staged_bytes = 0;
            }
            ExecutionProgress::FileCompleted { entry, bytes } => {
                self.phase = TransferJobPhase::Finalizing;
                self.current = Some(entry);
                self.staged_bytes = 0;
                self.completed_files += 1;
                self.completed_bytes = self.completed_bytes.saturating_add(bytes);
            }
            ExecutionProgress::RemovingSource(entry) => {
                self.phase = TransferJobPhase::RemovingSource;
                self.current = Some(entry);
                self.staged_bytes = 0;
            }
            ExecutionProgress::DirectoryCreated(entry) | ExecutionProgress::Finalizing(entry) => {
                self.phase = TransferJobPhase::Finalizing;
                self.current = Some(entry);
            }
        }
    }

    fn activity(self) -> ActivityProgress {
        ActivityProgress::new(
            self.completed_files,
            Some(self.total_files),
            ActivityProgressUnit::Files,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferReceiptFailure {
    Unavailable,
    UnsafeNamespace,
    IncompleteScan,
    Capacity,
    Collision,
    InvalidRecord,
    Poisoned,
    QueueCommitUncertain,
    Io(std::io::ErrorKind),
}
impl From<super::receipts::store::ReceiptStoreError> for TransferReceiptFailure {
    fn from(error: super::receipts::store::ReceiptStoreError) -> Self {
        use super::receipts::store::ReceiptStoreError as E;
        match error {
            E::Unavailable => Self::Unavailable,
            E::UnsafeNamespace => Self::UnsafeNamespace,
            E::IncompleteScan => Self::IncompleteScan,
            E::Capacity => Self::Capacity,
            E::Collision => Self::Collision,
            E::InvalidRecord => Self::InvalidRecord,
            E::Poisoned => Self::Poisoned,
            E::QueueCommitUncertain => Self::QueueCommitUncertain,
            E::Io(kind) => Self::Io(kind),
            E::AfterCreate { reason, .. } => (*reason).into(),
        }
    }
}

#[derive(Debug)]
pub enum TransferJobResult {
    Finished(CopyExecution),
    Rejected(CopyFailure),
    CancelledBeforeStart,
    WorkerPanicked,
    ReceiptFailed {
        report: Option<CopyExecution>,
        receipt_id: Option<TransferReceiptId>,
        failure: TransferReceiptFailure,
    },
}

impl TransferJobResult {
    pub(super) fn status(&self) -> TransferJobStatus {
        use TransferJobStatus::{Cancelled, Completed, Failed, Partial};
        let Self::Finished(report) = self else {
            return match self {
                Self::Rejected(CopyFailure::Cancelled) | Self::CancelledBeforeStart => Cancelled,
                Self::ReceiptFailed {
                    report: Some(report),
                    ..
                } if report_has_effect(report) => Partial,
                _ => Failed,
            };
        };
        execution_status(report)
    }
}

fn execution_status(report: &CopyExecution) -> TransferJobStatus {
    use TransferJobStatus::{Cancelled, Completed, Failed, Partial};
    let mut retained = false;
    let mut incomplete = false;
    let mut cancelled = report.cancelled;
    let mut failed = false;
    for row in &report.rows {
        match &row.state {
            ExecutionState::Copied(_)
            | ExecutionState::Moved(_)
            | ExecutionState::MovedByCopy(_) => retained = true,
            ExecutionState::MovedDirectory(directory) => {
                retained |= directory.has_retained_effect();
                let kept_by_review = matches!(
                    &directory.source,
                    DirectorySourceOutcome::RetainedChildren { skipped, incomplete }
                        if !skipped.is_empty() && incomplete.is_empty()
                );
                let finished = directory.completed() || kept_by_review;
                let stopped = matches!(directory.source, DirectorySourceOutcome::Cancelled);
                incomplete |= !finished;
                cancelled |= stopped;
                failed |= !finished && !stopped;
            }
            ExecutionState::CreatedDirectory(directory) => {
                retained = true;
                incomplete |= !matches!(directory.times, DirectoryTimes::Preserved);
                failed |= matches!(directory.times, DirectoryTimes::Failed(_));
            }
            ExecutionState::Failed(error) => {
                retained |= error.has_retained_effect();
                incomplete = true;
                failed = true;
            }
            ExecutionState::Cancelled(error) => {
                retained |= error.as_ref().is_some_and(CopyError::has_retained_effect);
                incomplete = true;
                cancelled = true;
            }
            ExecutionState::BlockedByParent(_) | ExecutionState::BlockedByReceipt => {
                incomplete = true
            }
            ExecutionState::MergedDirectory(_) | ExecutionState::Skipped => {}
        }
    }
    if !incomplete {
        Completed
    } else if retained {
        Partial
    } else if cancelled && !failed {
        Cancelled
    } else {
        Failed
    }
}

fn report_has_effect(report: &CopyExecution) -> bool {
    report.rows.iter().any(|row| match &row.state {
        ExecutionState::Copied(_)
        | ExecutionState::Moved(_)
        | ExecutionState::MovedByCopy(_)
        | ExecutionState::CreatedDirectory(_) => true,
        ExecutionState::MovedDirectory(directory) => directory.has_retained_effect(),
        ExecutionState::Failed(error) => error.has_retained_effect(),
        ExecutionState::Cancelled(error) => {
            error.as_ref().is_some_and(CopyError::has_retained_effect)
        }
        _ => false,
    })
}

#[derive(Clone, Debug)]
pub struct TransferJobInfo {
    pub id: TransferJobId,
    pub activity: Option<ActivityKey>,
    pub receipt_id: Option<TransferReceiptId>,
    pub roots: Arc<[PathBuf]>,
    pub draft: TransferDraft,
    pub status: TransferJobStatus,
    pub armed: bool,
    pub progress: TransferJobProgress,
    pub current_path: Option<PathBuf>,
    pub elapsed: Option<Duration>,
    pub result: Option<Arc<TransferJobResult>>,
}

#[derive(Clone, Debug, Default)]
pub struct TransferQueueSnapshot {
    pub queued: Vec<TransferJobInfo>,
    pub active: Option<TransferJobInfo>,
    pub history: Vec<TransferJobInfo>,
    pub interrupted: Vec<InterruptedTransfer>,
    pub persistence_uncertain: bool,
    pub newer_snapshot_incomplete: bool,
}

#[derive(Clone, Debug)]
pub struct InterruptedTransfer {
    pub receipt_id: TransferReceiptId,
    pub action: InterruptedAction,
    pub source: String,
    pub destination: String,
    pub item_count: u64,
    pub verify_requested: bool,
    pub truncated: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterruptedAction {
    Copy,
    Move,
    Duplicate,
}
impl From<QueueAction> for InterruptedAction {
    fn from(value: QueueAction) -> Self {
        match value {
            QueueAction::Copy => Self::Copy,
            QueueAction::Move => Self::Move,
            QueueAction::Duplicate => Self::Duplicate,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TransferHistoryQuery {
    Interrupted,
    Summaries,
    Detail(TransferReceiptId),
    Retry(TransferReceiptId, u32),
    Undo(TransferReceiptId, u32),
    Discard(TransferReceiptId),
}
pub enum TransferHistoryValue {
    Interrupted(Vec<InterruptedTransfer>),
    Summaries(Vec<super::ReceiptSummary>),
    Detail(super::ReceiptDetail),
    Retry(super::PreparedReceiptRetry),
    Undo(super::PreparedReceiptUndo),
    Discarded(bool),
}
pub struct TransferHistoryResponse {
    pub request: u64,
    pub result: Result<TransferHistoryValue, TransferHistoryError>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferHistoryError {
    Busy,
    Unavailable,
    Refused,
}

#[derive(Debug)]
pub enum TransferJobError {
    QueueFull,
    ApprovalAlreadyQueued,
    Closed,
    IdentityExhausted,
    WorkerUnavailable(std::io::Error),
    Persistence,
}

impl fmt::Display for TransferJobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QueueFull => write!(f, "The transfer queue is full."),
            Self::ApprovalAlreadyQueued => write!(f, "This review has already been queued."),
            Self::Closed => write!(f, "The transfer service is closing."),
            Self::IdentityExhausted => write!(f, "Transfer job identities are exhausted."),
            Self::Persistence => write!(f, "Transfer queue persistence is unavailable."),
            Self::WorkerUnavailable(error) => {
                write!(f, "The transfer worker could not start ({error}).")
            }
        }
    }
}

impl std::error::Error for TransferJobError {}

struct PendingJob {
    info: TransferJobInfo,
    plan: ApprovedTransferPlan,
    activity: Option<ActivityTask>,
    preview: Option<QueuePreview>,
}

struct ActiveJob {
    info: TransferJobInfo,
    started: Instant,
    control: CopyControl,
    activity: Option<ActivityTask>,
}

#[derive(Default)]
struct QueueState {
    next_id: u64,
    closing: bool,
    queued: VecDeque<PendingJob>,
    active: Option<ActiveJob>,
    history: VecDeque<TransferJobInfo>,
    persisted: Vec<QueuePreview>,
    loaded: bool,
    persistence_uncertain: bool,
    newer_snapshot_incomplete: bool,
    next_query: u64,
    history_request: Option<(u64, TransferHistoryQuery)>,
    history_running: bool,
    history_response: Option<TransferHistoryResponse>,
}

#[cfg(test)]
type TransferProgressHook = Arc<dyn Fn(TransferJobId, ExecutionProgress) + Send + Sync>;

#[derive(Default)]
struct Shared {
    state: Mutex<QueueState>,
    ready: Condvar,
    receipt_store: Option<ReceiptStore>,
    queue_mutation: Mutex<()>,
    #[cfg(test)]
    progress_hook: Option<TransferProgressHook>,
    #[cfg(test)]
    fail_terminal: bool,
    #[cfg(test)]
    fail_handoff_uncertain: bool,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, QueueState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    // Caller holds queue_mutation; no QueueState lock spans disk I/O.
    fn load_persisted(&self) -> Result<(), TransferJobError> {
        if self.lock().loaded {
            return Ok(());
        }
        let Some(store) = &self.receipt_store else {
            self.lock().loaded = true;
            return Ok(());
        };
        let snapshot = store
            .load_queue_snapshot()
            .map_err(|_| TransferJobError::Persistence)?;
        let mut rows = Vec::new();
        for row in snapshot.rows {
            if !store
                .has_receipt_name(row.receipt_id)
                .map_err(|_| TransferJobError::Persistence)?
            {
                rows.push(row);
            }
        }
        let mut state = self.lock();
        state.persisted = rows;
        state.loaded = true;
        state.newer_snapshot_incomplete = snapshot.newer_incomplete;
        Ok(())
    }
    fn persist(&self, rows: &[QueuePreview]) -> Result<(), TransferJobError> {
        if self.lock().persistence_uncertain {
            return Err(TransferJobError::Persistence);
        }
        if let Some(store) = &self.receipt_store {
            if let Err(error) = store.save_queue_snapshot(rows) {
                if matches!(
                    error,
                    super::receipts::store::ReceiptStoreError::QueueCommitUncertain
                ) {
                    self.lock().persistence_uncertain = true;
                }
                return Err(TransferJobError::Persistence);
            }
        }
        Ok(())
    }

    fn cancel(&self, id: TransferJobId) -> bool {
        let state = self.lock();
        if let Some(job) = state.active.as_ref().filter(|job| job.info.id == id) {
            if let Some(activity) = &job.activity {
                activity.mark_cancelling();
            }
            job.control.cancel();
            return true;
        }
        drop(state);
        let _mutation = self
            .queue_mutation
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut state = self.lock();
        if let Some(job) = state.active.as_ref().filter(|job| job.info.id == id) {
            job.control.cancel();
            return true;
        }
        let Some(index) = state.queued.iter().position(|job| job.info.id == id) else {
            return false;
        };
        if let Some(job) = state.queued.remove(index) {
            retain_terminal(
                &mut state,
                job.info,
                job.activity,
                TransferJobResult::CancelledBeforeStart,
            );
        }
        true
    }
}

/// The queue belongs to the app, independent of any transfer view.
pub struct TransferJobs {
    shared: Arc<Shared>,
    activity: ActivityCenter,
    worker: Option<JoinHandle<()>>,
}

fn durable_preview(plan: &ApprovedTransferPlan) -> Result<QueuePreview, TransferJobError> {
    let mut rng = rand::rngs::SysRng;
    let mut id = [0u8; 16];
    id[..8].copy_from_slice(
        &rng.try_next_u64()
            .map_err(|_| TransferJobError::Persistence)?
            .to_le_bytes(),
    );
    id[8..].copy_from_slice(
        &rng.try_next_u64()
            .map_err(|_| TransferJobError::Persistence)?
            .to_le_bytes(),
    );
    if id == [0; 16] {
        return Err(TransferJobError::Persistence);
    }
    fn text(path: &std::path::Path) -> (String, bool) {
        let value = path.to_string_lossy();
        let mut end = value.len().min(2048);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        (value[..end].to_owned(), end != value.len())
    }
    let source = plan
        .source()
        .entries()
        .iter()
        .find(|entry| entry.parent().is_none())
        .map(|entry| entry.path())
        .unwrap_or_else(|| std::path::Path::new(""));
    let (source, source_truncated) = text(source);
    let (action, destination) = match &plan.draft().action {
        super::TransferAction::CopyTo(path) => (QueueAction::Copy, path.as_path()),
        super::TransferAction::MoveTo(path) => (QueueAction::Move, path.as_path()),
        super::TransferAction::Duplicate => (QueueAction::Duplicate, std::path::Path::new("")),
    };
    let (destination, destination_truncated) = text(destination);
    Ok(QueuePreview {
        receipt_id: TransferReceiptId(id),
        action,
        source,
        destination,
        item_count: plan.source().entries().len() as u64,
        verify_requested: plan.draft().verify,
        truncated: source_truncated
            || destination_truncated
            || plan
                .source()
                .entries()
                .iter()
                .filter(|entry| entry.parent().is_none())
                .count()
                > 1,
    })
}

impl TransferJobs {
    pub fn new(activity: ActivityCenter) -> Self {
        Self {
            shared: Arc::new(Shared::default()),
            activity,
            worker: None,
        }
    }

    pub fn with_data_root(
        activity: ActivityCenter,
        root: crate::platform::app_data::AppDataRoot,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                receipt_store: Some(ReceiptStore::new(&root)),
                ..Shared::default()
            }),
            activity,
            worker: None,
        }
    }

    #[cfg(test)]
    pub(super) fn fail_terminal_for_test(&mut self) {
        if let Some(shared) = Arc::get_mut(&mut self.shared) {
            shared.fail_terminal = true;
        }
    }

    pub fn enqueue(&self, plan: ApprovedTransferPlan) -> Result<TransferJobId, TransferJobError> {
        let _mutation = self
            .shared
            .queue_mutation
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        self.shared.load_persisted()?;
        let state = self.shared.lock();
        if state.closing {
            return Err(TransferJobError::Closed);
        }
        if state.queued.len() + usize::from(state.active.is_some()) >= TRANSFER_QUEUE_LIMIT {
            return Err(TransferJobError::QueueFull);
        }
        let value = state
            .next_id
            .checked_add(1)
            .ok_or(TransferJobError::IdentityExhausted)?;
        let preview = if self.shared.receipt_store.is_some() {
            Some(durable_preview(&plan)?)
        } else {
            None
        };
        if !plan.claim_for_queue() {
            return Err(TransferJobError::ApprovalAlreadyQueued);
        }
        let mut persisted = state.persisted.clone();
        if let Some(preview) = &preview {
            persisted.push(preview.clone());
        }
        drop(state);
        if let Err(error) = self.shared.persist(&persisted) {
            if !self.shared.lock().persistence_uncertain {
                plan.release_queue_claim();
            }
            return Err(error);
        }
        let mut state = self.shared.lock();
        state.persisted = persisted;
        state.next_id = value;
        let id = TransferJobId(value);
        let activity = self.activity.observe_queued(
            ActivityOwner::FileTransfer,
            ActivitySlot::Primary,
            ActivityIncarnation::new(value),
            ActivityOperation::TransferFiles,
        );
        let progress = TransferJobProgress::new(&plan);
        if let Some(activity) = &activity {
            let shared = Arc::downgrade(&self.shared);
            activity.set_progress(Some(progress.activity()));
            if self.shared.receipt_store.is_none() {
                activity.set_cancel_handler(move || {
                    if let Some(shared) = shared.upgrade() {
                        shared.cancel(id);
                    }
                });
            }
        }
        let info = TransferJobInfo {
            id,
            activity: activity.as_ref().map(ActivityTask::key),
            receipt_id: preview.as_ref().map(|row| row.receipt_id),
            roots: plan
                .source()
                .entries()
                .iter()
                .filter(|entry| entry.parent().is_none())
                .map(|entry| entry.path().to_path_buf())
                .collect(),
            draft: plan.draft().clone(),
            status: TransferJobStatus::Queued,
            armed: false,
            progress,
            current_path: None,
            elapsed: None,
            result: None,
        };
        state.queued.push_back(PendingJob {
            info,
            plan,
            activity,
            preview,
        });
        Ok(id)
    }

    /// Start the approvals currently in the queue. Later enqueue calls need
    /// their own explicit Start, even while an earlier batch is still running.
    pub fn start(&mut self) -> Result<usize, TransferJobError> {
        let mut state = self.shared.lock();
        if state.closing {
            return Err(TransferJobError::Closed);
        }
        if state.persistence_uncertain {
            return Err(TransferJobError::Persistence);
        }
        let count = state.queued.iter().filter(|job| !job.info.armed).count();
        if count == 0 {
            return Ok(0);
        }
        if self.worker.is_none() {
            let shared = Arc::clone(&self.shared);
            self.worker = Some(
                thread::Builder::new()
                    .name("file-transfer".into())
                    .spawn(move || worker_loop(&shared))
                    .map_err(TransferJobError::WorkerUnavailable)?,
            );
        }
        for job in &mut state.queued {
            job.info.armed = true;
        }
        self.shared.ready.notify_one();
        Ok(count)
    }

    /// Move an exact queued job without crossing the explicit Start boundary.
    pub fn reorder(&self, id: TransferJobId, before: Option<TransferJobId>) -> bool {
        self.try_reorder(id, before).unwrap_or(false)
    }
    pub fn try_reorder(
        &self,
        id: TransferJobId,
        before: Option<TransferJobId>,
    ) -> Result<bool, TransferJobError> {
        let _mutation = self
            .shared
            .queue_mutation
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        self.shared.load_persisted()?;
        let state = self.shared.lock();
        let Some(from) = state.queued.iter().position(|job| job.info.id == id) else {
            return Ok(false);
        };
        let to = match before {
            Some(before) => {
                let Some(index) = state.queued.iter().position(|job| job.info.id == before) else {
                    return Ok(false);
                };
                index
            }
            None => state.queued.len(),
        };
        let to = to - usize::from(from < to);
        let armed = state.queued.iter().take_while(|job| job.info.armed).count();
        if (from < armed) != (to < armed) {
            return Ok(false);
        }
        if from == to {
            return Ok(true);
        }
        let mut persisted = state.persisted.clone();
        if let Some(preview) = &state.queued[from].preview {
            let Some(index) = persisted
                .iter()
                .position(|p| p.receipt_id == preview.receipt_id)
            else {
                return Err(TransferJobError::Persistence);
            };
            let item = persisted.remove(index);
            let before_id = before
                .and_then(|id| state.queued.iter().find(|job| job.info.id == id))
                .and_then(|job| job.preview.as_ref())
                .map(|p| p.receipt_id);
            let index = before_id
                .and_then(|id| persisted.iter().position(|p| p.receipt_id == id))
                .unwrap_or(persisted.len());
            persisted.insert(index, item);
        }
        drop(state);
        self.shared.persist(&persisted)?;
        let mut state = self.shared.lock();
        state.persisted = persisted;
        if let Some(job) = state.queued.remove(from) {
            state.queued.insert(to, job);
        }
        Ok(true)
    }

    pub fn pause(&self, id: TransferJobId) -> bool {
        self.shared
            .lock()
            .active
            .as_ref()
            .filter(|job| job.info.id == id)
            .is_some_and(|job| job.control.request_pause())
    }

    pub fn resume(&self, id: TransferJobId) -> bool {
        self.shared
            .lock()
            .active
            .as_ref()
            .filter(|job| job.info.id == id)
            .is_some_and(|job| job.control.resume())
    }

    pub fn cancel(&self, id: TransferJobId) -> bool {
        self.try_cancel(id).unwrap_or(false)
    }
    pub fn try_cancel(&self, id: TransferJobId) -> Result<bool, TransferJobError> {
        {
            let state = self.shared.lock();
            if let Some(job) = state.active.as_ref().filter(|job| job.info.id == id) {
                if let Some(activity) = &job.activity {
                    activity.mark_cancelling();
                }
                job.control.cancel();
                return Ok(true);
            }
        }
        let _mutation = self
            .shared
            .queue_mutation
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        self.shared.load_persisted()?;
        let state = self.shared.lock();
        if let Some(job) = state.active.as_ref().filter(|job| job.info.id == id) {
            if let Some(activity) = &job.activity {
                activity.mark_cancelling();
            }
            job.control.cancel();
            return Ok(true);
        }
        let Some(index) = state.queued.iter().position(|job| job.info.id == id) else {
            return Ok(false);
        };
        let mut persisted = state.persisted.clone();
        if let Some(preview) = &state.queued[index].preview {
            persisted.retain(|row| row.receipt_id != preview.receipt_id);
        }
        drop(state);
        self.shared.persist(&persisted)?;
        let mut state = self.shared.lock();
        state.persisted = persisted;
        if let Some(job) = state.queued.remove(index) {
            retain_terminal(
                &mut state,
                job.info,
                job.activity,
                TransferJobResult::CancelledBeforeStart,
            );
        }
        Ok(true)
    }

    pub fn snapshot(&self) -> TransferQueueSnapshot {
        let state = self.shared.lock();
        TransferQueueSnapshot {
            queued: state.queued.iter().map(|job| job.info.clone()).collect(),
            active: state.active.as_ref().map(|job| {
                let mut info = job.info.clone();
                info.elapsed = Some(job.started.elapsed());
                info.status = match job.control.pause_state() {
                    CopyPauseState::Running
                        if info.progress.phase == TransferJobPhase::Preparing =>
                    {
                        TransferJobStatus::Preparing
                    }
                    CopyPauseState::Running => TransferJobStatus::Running,
                    CopyPauseState::PauseRequested => TransferJobStatus::PauseRequested,
                    CopyPauseState::Paused => TransferJobStatus::Paused,
                    CopyPauseState::Cancelled => TransferJobStatus::Cancelling,
                };
                info
            }),
            history: state.history.iter().cloned().collect(),
            interrupted: state
                .persisted
                .iter()
                .filter(|row| {
                    !state.queued.iter().any(|job| {
                        job.preview
                            .as_ref()
                            .is_some_and(|p| p.receipt_id == row.receipt_id)
                    }) && !state
                        .active
                        .as_ref()
                        .is_some_and(|job| job.info.receipt_id == Some(row.receipt_id))
                })
                .map(|row| InterruptedTransfer {
                    receipt_id: row.receipt_id,
                    action: row.action.into(),
                    source: row.source.clone(),
                    destination: row.destination.clone(),
                    item_count: row.item_count,
                    verify_requested: row.verify_requested,
                    truncated: row.truncated,
                })
                .collect(),
            persistence_uncertain: state.persistence_uncertain,
            newer_snapshot_incomplete: state.newer_snapshot_incomplete,
        }
    }

    /// Enqueue at most one bounded read-only history request for the existing worker.
    pub fn request_history(
        &mut self,
        query: TransferHistoryQuery,
    ) -> Result<u64, TransferHistoryError> {
        let mut state = self.shared.lock();
        if state.closing || self.shared.receipt_store.is_none() {
            return Err(TransferHistoryError::Unavailable);
        }
        if state.history_request.is_some()
            || state.history_running
            || state.history_response.is_some()
        {
            return Err(TransferHistoryError::Busy);
        }
        let id = state
            .next_query
            .checked_add(1)
            .ok_or(TransferHistoryError::Unavailable)?;
        if self.worker.is_none() {
            let shared = Arc::clone(&self.shared);
            self.worker = Some(
                thread::Builder::new()
                    .name("file-transfer".into())
                    .spawn(move || worker_loop(&shared))
                    .map_err(|_| TransferHistoryError::Unavailable)?,
            );
        }
        state.next_query = id;
        state.history_request = Some((id, query));
        self.shared.ready.notify_one();
        Ok(id)
    }
    pub fn take_history_response(&self) -> Option<TransferHistoryResponse> {
        self.shared.lock().history_response.take()
    }

    pub fn shutdown(&mut self) {
        {
            let _mutation = self
                .shared
                .queue_mutation
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let mut state = self.shared.lock();
            state.closing = true;
            while let Some(job) = state.queued.pop_front() {
                retain_terminal(
                    &mut state,
                    job.info,
                    job.activity,
                    TransferJobResult::CancelledBeforeStart,
                );
            }
            if let Some(job) = &state.active {
                if let Some(activity) = &job.activity {
                    activity.mark_cancelling();
                }
                job.control.cancel();
            }
            self.shared.ready.notify_one();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for TransferJobs {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn worker_loop(shared: &Shared) {
    let mut receipt_store = shared.receipt_store.clone();
    loop {
        {
            let mut state = shared.lock();
            while !state.closing
                && (state.persistence_uncertain
                    || !state.queued.front().is_some_and(|job| job.info.armed))
                && state.history_request.is_none()
            {
                state = shared
                    .ready
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            if state.closing {
                return;
            }
            if let Some((id, query)) = state.history_request.take() {
                state.history_running = true;
                drop(state);
                let result = run_history_query(shared, query);
                let mut state = shared.lock();
                state.history_running = false;
                state.history_response = Some(TransferHistoryResponse {
                    request: id,
                    result,
                });
                continue;
            }
        }
        let mutation = shared
            .queue_mutation
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let (pending, control, preparing) = {
            let mut state = shared.lock();
            if state.closing {
                return;
            }
            if state.persistence_uncertain
                || !state.queued.front().is_some_and(|job| job.info.armed)
            {
                continue;
            }
            let Some(pending) = state.queued.pop_front() else {
                continue;
            };
            let control = CopyControl::default();
            let started = Instant::now();
            let preparing = Arc::new(AtomicBool::new(true));
            if let Some(activity) = &pending.activity {
                if shared.receipt_store.is_some() {
                    let active_control = control.clone();
                    activity.set_cancel_handler(move || active_control.cancel());
                }
                let pause = control.clone();
                let preparing = preparing.clone();
                activity.mark_running_with_status(started, move || match pause.pause_state() {
                    CopyPauseState::PauseRequested => ActivityLifecycle::PauseRequested,
                    CopyPauseState::Paused => ActivityLifecycle::Paused,
                    CopyPauseState::Running if preparing.load(Ordering::Relaxed) => {
                        ActivityLifecycle::Preparing
                    }
                    _ => ActivityLifecycle::Running,
                });
            }
            state.active = Some(ActiveJob {
                info: pending.info.clone(),
                started,
                control: control.clone(),
                activity: pending.activity.clone(),
            });
            (pending, control, preparing)
        };
        drop(mutation);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut report_progress = |event| {
                #[cfg(test)]
                if let Some(hook) = &shared.progress_hook {
                    hook(pending.info.id, event);
                }
                if let Some(job) = &mut shared.lock().active {
                    let previous_files = job.info.progress.completed_files;
                    let previous_entry = job.info.progress.current;
                    job.info.progress.observe(event);
                    preparing.store(
                        job.info.progress.phase == TransferJobPhase::Preparing,
                        Ordering::Relaxed,
                    );
                    if job.info.progress.current != previous_entry {
                        job.info.current_path = job
                            .info
                            .progress
                            .current
                            .and_then(|id| pending.plan.source().get(id))
                            .map(|source| source.path().to_path_buf());
                    }
                    if job.info.progress.completed_files != previous_files
                        && let Some(activity) = &job.activity
                    {
                        activity.set_progress(Some(job.info.progress.activity()));
                    }
                }
            };
            if let Some(store) = receipt_store.as_mut() {
                let Some(preview) = pending.preview.as_ref() else {
                    return TransferJobResult::ReceiptFailed {
                        report: None,
                        receipt_id: None,
                        failure: TransferReceiptFailure::InvalidRecord,
                    };
                };
                let requested_id = preview.receipt_id;
                let mut receipt = match store.admit_with_id(&pending.plan, requested_id) {
                    Ok(receipt) => receipt,
                    Err(error) => {
                        if error.receipt_id().is_some() {
                            let _mutation = shared
                                .queue_mutation
                                .lock()
                                .unwrap_or_else(|e| e.into_inner());
                            shared
                                .lock()
                                .persisted
                                .retain(|row| row.receipt_id != requested_id);
                        }
                        let receipt_id = error
                            .receipt_id()
                            .map(TransferReceiptId)
                            .or(Some(requested_id));
                        if let Some(job) = &mut shared.lock().active {
                            job.info.receipt_id = receipt_id;
                        }
                        return TransferJobResult::ReceiptFailed {
                            report: None,
                            receipt_id,
                            failure: error.into(),
                        };
                    }
                };
                let receipt_id = TransferReceiptId(receipt.receipt_id());
                if let Some(job) = &mut shared.lock().active {
                    job.info.receipt_id = Some(receipt_id);
                }
                {
                    let _mutation = shared
                        .queue_mutation
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    let state = shared.lock();
                    if !state
                        .persisted
                        .iter()
                        .any(|row| row.receipt_id == receipt_id)
                    {
                        return TransferJobResult::ReceiptFailed {
                            report: None,
                            receipt_id: Some(receipt_id),
                            failure: TransferReceiptFailure::InvalidRecord,
                        };
                    }
                    let mut rows = state.persisted.clone();
                    rows.retain(|row| row.receipt_id != receipt_id);
                    drop(state);
                    #[cfg(test)]
                    let injected_uncertain = shared.fail_handoff_uncertain;
                    #[cfg(not(test))]
                    let injected_uncertain = false;
                    if injected_uncertain {
                        shared.lock().persistence_uncertain = true;
                    }
                    if injected_uncertain || shared.persist(&rows).is_err() {
                        // The receipt already exists: hide its old preview in memory,
                        // while leaving the last durable snapshot untouched for recovery.
                        shared.lock().persisted = rows;
                        return TransferJobResult::ReceiptFailed {
                            report: None,
                            receipt_id: Some(receipt_id),
                            failure: TransferReceiptFailure::Unavailable,
                        };
                    }
                    shared.lock().persisted = rows;
                }
                match super::execute::execute_with_journal(
                    &pending.plan,
                    &control,
                    &mut report_progress,
                    &mut receipt,
                ) {
                    Ok(report) => {
                        let status = execution_status(&report);
                        #[cfg(test)]
                        if shared.fail_terminal {
                            receipt.poison_terminal_for_test();
                        }
                        match finish_receipt(&mut receipt, &report, status) {
                            Ok(()) => TransferJobResult::Finished(report),
                            Err(error) => TransferJobResult::ReceiptFailed {
                                report: Some(report),
                                receipt_id: Some(receipt_id),
                                failure: error.into(),
                            },
                        }
                    }
                    Err(error) => TransferJobResult::Rejected(error),
                }
            } else {
                match execute_copy_plan(&pending.plan, &control, &mut report_progress) {
                    Ok(report) => TransferJobResult::Finished(report),
                    Err(error) => TransferJobResult::Rejected(error),
                }
            }
        }));
        drop(pending);
        drop(control);
        let result = match result {
            Ok(result) => result,
            Err(_) => TransferJobResult::WorkerPanicked,
        };
        let mut state = shared.lock();
        if let Some(mut job) = state.active.take() {
            job.info.elapsed = Some(job.started.elapsed());
            drop(job.control);
            retain_terminal(&mut state, job.info, job.activity, result);
        }
    }
}

pub(super) fn finish_receipt(
    receipt: &mut super::receipts::store::ReceiptSession<'_>,
    report: &CopyExecution,
    status: TransferJobStatus,
) -> Result<(), super::receipts::store::ReceiptStoreError> {
    use super::receipts::wire::{Disposition, EntryOutcome, JobOutcome};
    for row in &report.rows {
        let mut disposition = receipt
            .suggested_disposition(row.entry)
            .unwrap_or(Disposition::Uncertain);
        let outcome = match &row.state {
            ExecutionState::Copied(_)
            | ExecutionState::Moved(_)
            | ExecutionState::MovedByCopy(_) => EntryOutcome::Completed,
            ExecutionState::CreatedDirectory(directory) => {
                if matches!(directory.times, DirectoryTimes::Preserved) {
                    EntryOutcome::Completed
                } else if matches!(directory.times, DirectoryTimes::Cancelled) {
                    EntryOutcome::Cancelled
                } else {
                    EntryOutcome::Failed
                }
            }
            ExecutionState::MergedDirectory(_) => EntryOutcome::Completed,
            ExecutionState::MovedDirectory(directory) => match &directory.source {
                DirectorySourceOutcome::Removed { .. } => EntryOutcome::Completed,
                DirectorySourceOutcome::RetainedChildren {
                    skipped,
                    incomplete,
                } if !skipped.is_empty() && incomplete.is_empty() => {
                    if disposition != Disposition::Uncertain {
                        disposition = Disposition::Settled;
                    }
                    EntryOutcome::RetainedByReview
                }
                DirectorySourceOutcome::Cancelled => EntryOutcome::Cancelled,
                _ => EntryOutcome::Failed,
            },
            ExecutionState::Skipped => EntryOutcome::Skipped,
            ExecutionState::BlockedByParent(_) | ExecutionState::BlockedByReceipt => {
                EntryOutcome::Blocked
            }
            ExecutionState::Failed(error) => {
                if error.has_retained_effect() && disposition == Disposition::NoEffect {
                    disposition = Disposition::Uncertain;
                }
                EntryOutcome::Failed
            }
            ExecutionState::Cancelled(error) => {
                if error.as_ref().is_some_and(CopyError::has_retained_effect)
                    && disposition == Disposition::NoEffect
                {
                    disposition = Disposition::Uncertain;
                }
                EntryOutcome::Cancelled
            }
        };
        receipt.finish_entry(row.entry, outcome, disposition)?;
    }
    let outcome = match status {
        TransferJobStatus::Completed => JobOutcome::Completed,
        TransferJobStatus::Partial => JobOutcome::Partial,
        TransferJobStatus::Cancelled => JobOutcome::Cancelled,
        _ => JobOutcome::Failed,
    };
    receipt.finish_job(outcome)
}

fn run_history_query(
    shared: &Shared,
    query: TransferHistoryQuery,
) -> Result<TransferHistoryValue, TransferHistoryError> {
    let store = shared
        .receipt_store
        .as_ref()
        .ok_or(TransferHistoryError::Unavailable)?;
    match query {
        TransferHistoryQuery::Interrupted | TransferHistoryQuery::Discard(_) => {
            let _mutation = shared
                .queue_mutation
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            shared
                .load_persisted()
                .map_err(|_| TransferHistoryError::Unavailable)?;
            let state = shared.lock();
            let is_live = |id| {
                state
                    .queued
                    .iter()
                    .any(|job| job.info.receipt_id == Some(id))
                    || state
                        .active
                        .as_ref()
                        .is_some_and(|job| job.info.receipt_id == Some(id))
            };
            if let TransferHistoryQuery::Discard(id) = query {
                if is_live(id) {
                    return Err(TransferHistoryError::Refused);
                }
                if !state.persisted.iter().any(|row| row.receipt_id == id) {
                    return Ok(TransferHistoryValue::Discarded(false));
                }
                let mut rows = state.persisted.clone();
                rows.retain(|row| row.receipt_id != id);
                drop(state);
                if store
                    .has_receipt_name(id)
                    .map_err(|_| TransferHistoryError::Unavailable)?
                {
                    return Err(TransferHistoryError::Refused);
                }
                shared
                    .persist(&rows)
                    .map_err(|_| TransferHistoryError::Unavailable)?;
                shared.lock().persisted = rows;
                Ok(TransferHistoryValue::Discarded(true))
            } else {
                Ok(TransferHistoryValue::Interrupted(
                    state
                        .persisted
                        .iter()
                        .filter(|row| !is_live(row.receipt_id))
                        .map(|row| InterruptedTransfer {
                            receipt_id: row.receipt_id,
                            action: row.action.into(),
                            source: row.source.clone(),
                            destination: row.destination.clone(),
                            item_count: row.item_count,
                            verify_requested: row.verify_requested,
                            truncated: row.truncated,
                        })
                        .collect(),
                ))
            }
        }
        TransferHistoryQuery::Summaries => super::ReceiptHistory::from_store(store)
            .summaries()
            .map(TransferHistoryValue::Summaries)
            .map_err(|_| TransferHistoryError::Unavailable),
        TransferHistoryQuery::Detail(id) => super::ReceiptHistory::from_store(store)
            .reconcile(id, &crate::file_operations::ScanCancellation::new())
            .map(TransferHistoryValue::Detail)
            .map_err(|_| TransferHistoryError::Unavailable),
        TransferHistoryQuery::Retry(id, node) => super::ReceiptHistory::from_store(store)
            .prepare_retry(id, node, &crate::file_operations::ScanCancellation::new())
            .map(TransferHistoryValue::Retry)
            .map_err(|_| TransferHistoryError::Refused),
        TransferHistoryQuery::Undo(id, node) => super::ReceiptHistory::from_store(store)
            .prepare_undo(id, node, &crate::file_operations::ScanCancellation::new())
            .map(TransferHistoryValue::Undo)
            .map_err(|_| TransferHistoryError::Refused),
    }
}

fn retain_terminal(
    state: &mut QueueState,
    mut info: TransferJobInfo,
    activity: Option<ActivityTask>,
    result: TransferJobResult,
) {
    let receipt_failed = matches!(result, TransferJobResult::ReceiptFailed { .. });
    info.status = result.status();
    info.armed = false;
    info.progress.current = None;
    info.progress.staged_bytes = 0;
    info.current_path = None;
    info.result = Some(Arc::new(result));
    if let Some(task) = activity {
        let (outcome, mut diagnostic) = match info.status {
            TransferJobStatus::Completed => (ActivityOutcome::Completed, None),
            TransferJobStatus::Partial => (ActivityOutcome::Partial, None),
            TransferJobStatus::Cancelled => (ActivityOutcome::Cancelled, None),
            _ => (
                ActivityOutcome::Failed,
                Some(ActivityDiagnostic::WorkerFailed),
            ),
        };
        if receipt_failed {
            diagnostic = Some(ActivityDiagnostic::TransferReceiptUnavailable);
        }
        task.finish(outcome, diagnostic, ActivityRetirement::Confirmed);
    }
    state.history.push_front(info);
    state.history.truncate(JOB_HISTORY_LIMIT);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::ActivityCancelResult;
    use crate::file_operations::ScanCancellation;
    use crate::file_transfer::{
        TransferAction, TransferReview, inspect_source_tree, review_transfer,
    };
    use std::fs;
    use std::path::Path;

    fn review(source: &Path, destination: &Path) -> TransferReview {
        let cancel = ScanCancellation::new();
        let tree = inspect_source_tree(&[source.to_path_buf()], &cancel).unwrap();
        review_transfer(
            Arc::new(tree),
            TransferDraft {
                action: TransferAction::CopyTo(destination.to_path_buf()),
                verify: true,
            },
            &cancel,
        )
        .unwrap()
    }

    fn approve(review: &TransferReview) -> ApprovedTransferPlan {
        review
            .approve(review.generation(), &ScanCancellation::new())
            .unwrap()
    }

    fn wait_terminal(jobs: &TransferJobs, count: usize) -> TransferQueueSnapshot {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = jobs.snapshot();
            if snapshot.history.len() == count {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "jobs did not retire: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn transfer_jobs_start_reorder_and_late_enqueue_use_exact_approvals() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        let destination = temp.path().join("destination");
        for dir in [&a, &b, &destination] {
            fs::create_dir(dir).unwrap();
        }
        let src_a = a.join("same.txt");
        let src_b = b.join("same.txt");
        let late = a.join("late.txt");
        fs::write(&src_a, b"A").unwrap();
        fs::write(&src_b, b"BBB").unwrap();
        fs::write(&late, b"later").unwrap();
        let a = review(&src_a, &destination);
        let b = review(&src_b, &destination);
        let late = review(&late, &destination);

        let activity = ActivityCenter::new();
        let mut jobs = TransferJobs::new(activity.clone());
        let a_id = jobs.enqueue(approve(&a)).unwrap();
        let b_id = jobs.enqueue(approve(&b)).unwrap();
        assert!(matches!(
            jobs.enqueue(approve(&b)),
            Err(TransferJobError::ApprovalAlreadyQueued)
        ));
        assert!(jobs.reorder(b_id, Some(a_id)));
        let snapshot = jobs.snapshot();
        assert_eq!(
            snapshot.queued.iter().map(|job| job.id).collect::<Vec<_>>(),
            [b_id, a_id]
        );
        assert!(snapshot.active.is_none());
        assert!(fs::read_dir(&destination).unwrap().next().is_none());

        assert_eq!(jobs.start().unwrap(), 2);
        let late_id = jobs.enqueue(approve(&late)).unwrap();
        let snapshot = wait_terminal(&jobs, 2);
        let [a_job, b_job] = snapshot.history.as_slice() else {
            panic!("expected two retired approvals");
        };
        assert_eq!((a_job.id, a_job.status), (a_id, TransferJobStatus::Failed));
        assert_eq!(
            (b_job.id, b_job.status),
            (b_id, TransferJobStatus::Completed)
        );
        assert_eq!(b_job.progress.completed_files, 1);
        assert_eq!(b_job.progress.completed_bytes, 3);
        assert_eq!(fs::read(destination.join("same.txt")).unwrap(), b"BBB");
        assert_eq!(snapshot.queued[0].id, late_id);
        assert!(!snapshot.queued[0].armed);
        assert!(!destination.join("late.txt").exists());
        assert_eq!(jobs.start().unwrap(), 1);
        wait_terminal(&jobs, 3);
        assert_eq!(fs::read(destination.join("late.txt")).unwrap(), b"later");
        let activity = activity.snapshot();
        assert_eq!(activity.rows.len(), 3);
        assert!(activity.rows.iter().all(|row| {
            row.key.owner() == ActivityOwner::FileTransfer
                && row.lifecycle.is_terminal()
                && row.retirement == ActivityRetirement::Confirmed
        }));
        assert!(!jobs.cancel(b_id));
    }

    #[test]
    fn transfer_jobs_active_pause_and_activity_cancel_wait_for_native_retirement() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("destination");
        fs::create_dir(&destination).unwrap();
        let first = temp.path().join("first.txt");
        let second = temp.path().join("second.txt");
        let late = temp.path().join("late.txt");
        fs::write(&late, b"queued").unwrap();
        fs::write(&first, vec![71; 256 * 1024]).unwrap();
        fs::write(&second, b"safe").unwrap();

        let activity = ActivityCenter::new();
        let mut jobs = TransferJobs::new(activity.clone());
        let (staged_tx, staged_rx) = std::sync::mpsc::sync_channel(1);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
        let release_rx = Mutex::new(release_rx);
        Arc::get_mut(&mut jobs.shared).unwrap().progress_hook = Some(Arc::new(move |id, event| {
            if id.get() == 1 && matches!(event, ExecutionProgress::Copying { .. }) {
                staged_tx.send(()).unwrap();
                release_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
            }
        }));
        let first_id = jobs
            .enqueue(approve(&review(&first, &destination)))
            .unwrap();
        let second_id = jobs
            .enqueue(approve(&review(&second, &destination)))
            .unwrap();
        assert_eq!(jobs.start().unwrap(), 2);
        staged_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let late_id = jobs.enqueue(approve(&review(&late, &destination))).unwrap();
        assert!(!jobs.reorder(late_id, Some(second_id)));
        assert!(!jobs.reorder(second_id, None));
        let active = jobs.snapshot().active.unwrap();
        assert_eq!(active.id, first_id);
        assert!(jobs.pause(first_id));
        assert!(!jobs.pause(second_id));
        assert_eq!(
            jobs.snapshot().active.unwrap().status,
            TransferJobStatus::PauseRequested
        );
        let activity_key = active.activity.unwrap();
        let activity_row = || {
            activity
                .snapshot()
                .rows
                .into_iter()
                .find(|row| row.key == activity_key)
                .unwrap()
        };
        assert_eq!(activity_row().lifecycle, ActivityLifecycle::PauseRequested);
        assert!(activity_row().elapsed.is_some());
        release_tx.send(()).unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let snapshot = jobs.snapshot();
            assert_eq!(snapshot.queued[0].id, second_id);
            let active = snapshot.active.unwrap();
            if active.status == TransferJobStatus::Paused {
                assert_eq!(active.progress.completed_files, 0);
                assert!(active.progress.staged_bytes > 0);
                assert_eq!(activity_row().lifecycle, ActivityLifecycle::Paused);
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            activity.request_cancel(active.activity.unwrap()),
            ActivityCancelResult::Requested
        );
        let snapshot = wait_terminal(&jobs, 2);
        assert_eq!(snapshot.history[0].id, second_id);
        assert_eq!(snapshot.history[0].status, TransferJobStatus::Completed);
        assert_eq!(snapshot.history[1].id, first_id);
        assert_eq!(snapshot.history[1].status, TransferJobStatus::Cancelled);
        assert_eq!(snapshot.queued[0].id, late_id);
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 1);
        assert_eq!(fs::read(destination.join("second.txt")).unwrap(), b"safe");
        assert_eq!(fs::metadata(first).unwrap().len(), 256 * 1024);
    }

    #[test]
    fn transfer_jobs_retains_publication_when_cancelled_during_verification() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("destination");
        fs::create_dir(&destination).unwrap();
        let source = temp.path().join("source.txt");
        fs::write(&source, b"retained").unwrap();
        let target = destination.join("source.txt");

        let mut jobs = TransferJobs::new(ActivityCenter::new());
        let (verifying_tx, verifying_rx) = std::sync::mpsc::sync_channel(1);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
        let release_rx = Mutex::new(release_rx);
        Arc::get_mut(&mut jobs.shared).unwrap().progress_hook = Some(Arc::new(move |_, event| {
            if matches!(
                event,
                ExecutionProgress::Verifying {
                    verification_requested: true,
                    ..
                }
            ) {
                verifying_tx.send(()).unwrap();
                release_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
            }
        }));
        let id = jobs
            .enqueue(approve(&review(&source, &destination)))
            .unwrap();
        assert_eq!(jobs.start().unwrap(), 1);
        verifying_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(jobs.cancel(id));
        assert_eq!(
            jobs.snapshot().active.unwrap().status,
            TransferJobStatus::Cancelling
        );
        release_tx.send(()).unwrap();

        let snapshot = wait_terminal(&jobs, 1);
        let job = &snapshot.history[0];
        assert_eq!(job.status, TransferJobStatus::Partial);
        assert_eq!(job.progress.completed_files, 0);
        assert_eq!(fs::read(target).unwrap(), b"retained");
        assert_eq!(fs::read(source).unwrap(), b"retained");
        let Some(TransferJobResult::Finished(report)) = job.result.as_deref() else {
            panic!("expected the partial transfer report");
        };
        assert!(matches!(
            &report.rows[0].state,
            ExecutionState::Cancelled(Some(CopyError {
                artifact: Some(super::super::CopyRecoveryArtifact::Owned(artifact)), ..
            })) if artifact.state == super::super::CopyArtifactState::Published
        ));
    }

    #[test]
    fn transfer_jobs_reviewed_only_skip_completes_without_any_move() {
        use crate::file_transfer::{
            DirectoryMove, DirectoryMoveDestination, ExecutionRow, inspect_parent,
        };
        let fixture = tempfile::tempdir().unwrap();
        let source = fixture.path().join("source");
        let destination = fixture.path().join("existing");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(source.join("skipped.txt"), b"approved skip").unwrap();
        let cancel = ScanCancellation::new();
        let source_tree = inspect_source_tree(&[source], &cancel).unwrap();
        let [root, child] = source_tree.entries() else {
            panic!("expected one child");
        };
        let result = TransferJobResult::Finished(CopyExecution {
            rows: vec![
                ExecutionRow {
                    entry: root.id(),
                    state: ExecutionState::MovedDirectory(DirectoryMove {
                        destination: DirectoryMoveDestination::Merged(
                            inspect_parent(&destination, &cancel).unwrap(),
                        ),
                        source: DirectorySourceOutcome::RetainedChildren {
                            skipped: vec![child.id()],
                            incomplete: Vec::new(),
                        },
                    }),
                },
                ExecutionRow {
                    entry: child.id(),
                    state: ExecutionState::Skipped,
                },
            ],
            cancelled: false,
        });
        assert_eq!(result.status(), TransferJobStatus::Completed);
    }

    #[test]
    fn transfer_jobs_queue_bound_and_activity_cancel_are_exact() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("destination");
        fs::create_dir(&destination).unwrap();
        let source = temp.path().join("source.txt");
        fs::write(&source, b"source").unwrap();
        let activity = ActivityCenter::new();
        let mut jobs = TransferJobs::new(activity.clone());
        for _ in 0..TRANSFER_QUEUE_LIMIT {
            jobs.enqueue(approve(&review(&source, &destination)))
                .unwrap();
        }
        let extra = review(&source, &destination);
        assert!(matches!(
            jobs.enqueue(approve(&extra)),
            Err(TransferJobError::QueueFull)
        ));
        let snapshot = jobs.snapshot();
        let first = &snapshot.queued[0];
        let second = &snapshot.queued[1];
        let first_key = first.activity.unwrap();
        assert_eq!(
            activity.request_cancel(first_key),
            ActivityCancelResult::Requested
        );
        let snapshot = jobs.snapshot();
        assert_eq!(snapshot.queued[0].id, second.id);
        assert_eq!(snapshot.history[0].id, first.id);
        assert_eq!(snapshot.history[0].status, TransferJobStatus::Cancelled);
        assert_eq!(
            activity.request_cancel(first_key),
            ActivityCancelResult::Terminal
        );
        assert!(activity.snapshot().rows.iter().any(|row| {
            row.key == second.activity.unwrap() && row.lifecycle == ActivityLifecycle::Queued
        }));
        jobs.enqueue(approve(&extra)).unwrap();
        jobs.shutdown();
        assert!(matches!(
            jobs.enqueue(approve(&review(&source, &destination))),
            Err(TransferJobError::Closed)
        ));
        assert!(fs::read_dir(destination).unwrap().next().is_none());
    }
    fn history_response(jobs: &TransferJobs, request: u64) -> TransferHistoryResponse {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(value) = jobs.take_history_response() {
                assert_eq!(value.request, request);
                return value;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn transfer_jobs_durable_order_restart_and_explicit_discard() {
        let temp = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(temp.path());
        let out = temp.path().join("out");
        fs::create_dir(&out).unwrap();
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        fs::write(&a, b"a").unwrap();
        fs::write(&b, b"b").unwrap();
        let jobs = TransferJobs::with_data_root(ActivityCenter::new(), root.clone());
        let first = jobs.enqueue(approve(&review(&a, &out))).unwrap();
        let second = jobs.enqueue(approve(&review(&b, &out))).unwrap();
        let before = jobs.snapshot();
        let a_id = before.queued[0].receipt_id.unwrap();
        let b_id = before.queued[1].receipt_id.unwrap();
        assert!(jobs.try_reorder(second, Some(first)).unwrap());
        drop(jobs);
        let mut restored = TransferJobs::with_data_root(ActivityCenter::new(), root.clone());
        assert_eq!(restored.start().unwrap(), 0);
        let request = restored
            .request_history(TransferHistoryQuery::Interrupted)
            .unwrap();
        let TransferHistoryValue::Interrupted(rows) =
            history_response(&restored, request).result.unwrap()
        else {
            panic!("interrupted")
        };
        assert_eq!(
            rows.iter().map(|row| row.receipt_id).collect::<Vec<_>>(),
            [b_id, a_id]
        );
        let request = restored
            .request_history(TransferHistoryQuery::Discard(b_id))
            .unwrap();
        assert!(matches!(
            history_response(&restored, request).result,
            Ok(TransferHistoryValue::Discarded(true))
        ));
        drop(restored);
        let mut again = TransferJobs::with_data_root(ActivityCenter::new(), root);
        let request = again
            .request_history(TransferHistoryQuery::Interrupted)
            .unwrap();
        let TransferHistoryValue::Interrupted(rows) =
            history_response(&again, request).result.unwrap()
        else {
            panic!("interrupted")
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].receipt_id, a_id);
        assert!(!out.join("a").exists() && !out.join("b").exists());
    }
    #[test]
    fn transfer_jobs_durable_handoff_keeps_queued_receipt_identity() {
        let temp = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(temp.path());
        let source = temp.path().join("file");
        let out = temp.path().join("out");
        fs::write(&source, b"value").unwrap();
        fs::create_dir(&out).unwrap();
        let mut jobs = TransferJobs::with_data_root(ActivityCenter::new(), root.clone());
        jobs.enqueue(approve(&review(&source, &out))).unwrap();
        let id = jobs.snapshot().queued[0].receipt_id.unwrap();
        jobs.start().unwrap();
        let done = wait_terminal(&jobs, 1);
        assert_eq!(done.history[0].receipt_id, Some(id));
        assert_eq!(done.history[0].status, TransferJobStatus::Completed);
        assert!(
            temp.path()
                .join("transfers/receipts")
                .join(format!("{id}.receipt"))
                .exists()
        );
        let request = jobs
            .request_history(TransferHistoryQuery::Interrupted)
            .unwrap();
        let TransferHistoryValue::Interrupted(rows) =
            history_response(&jobs, request).result.unwrap()
        else {
            panic!("interrupted")
        };
        assert!(rows.is_empty());
    }
    #[test]
    fn transfer_jobs_durable_cancel_preserves_other_queued_preview() {
        let temp = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(temp.path());
        let out = temp.path().join("out");
        fs::create_dir(&out).unwrap();
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        fs::write(&a, b"a").unwrap();
        fs::write(&b, b"b").unwrap();
        let jobs = TransferJobs::with_data_root(ActivityCenter::new(), root.clone());
        let first = jobs.enqueue(approve(&review(&a, &out))).unwrap();
        jobs.enqueue(approve(&review(&b, &out))).unwrap();
        let remaining = jobs.snapshot().queued[1].receipt_id.unwrap();
        assert!(jobs.try_cancel(first).unwrap());
        drop(jobs);
        let mut restored = TransferJobs::with_data_root(ActivityCenter::new(), root);
        let request = restored
            .request_history(TransferHistoryQuery::Interrupted)
            .unwrap();
        let TransferHistoryValue::Interrupted(rows) =
            history_response(&restored, request).result.unwrap()
        else {
            panic!("interrupted")
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].receipt_id, remaining);
    }
    #[test]
    fn transfer_jobs_durable_queue_refuses_unsafe_store_before_publication() {
        let temp = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(temp.path());
        let out = temp.path().join("out");
        fs::create_dir(&out).unwrap();
        let source = temp.path().join("a");
        fs::write(&source, b"a").unwrap();
        fs::write(temp.path().join("transfers"), b"occupied").unwrap();
        let jobs = TransferJobs::with_data_root(ActivityCenter::new(), root);
        assert!(matches!(
            jobs.enqueue(approve(&review(&source, &out))),
            Err(TransferJobError::Persistence)
        ));
        assert!(jobs.snapshot().queued.is_empty());
        assert!(!out.join("a").exists());
    }
    #[test]
    fn transfer_jobs_durable_uncertain_handoff_stops_remaining_armed_jobs() {
        let temp = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(temp.path());
        let out = temp.path().join("out");
        fs::create_dir(&out).unwrap();
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        fs::write(&a, b"a").unwrap();
        fs::write(&b, b"b").unwrap();
        let mut jobs = TransferJobs::with_data_root(ActivityCenter::new(), root);
        Arc::get_mut(&mut jobs.shared)
            .unwrap()
            .fail_handoff_uncertain = true;
        jobs.enqueue(approve(&review(&a, &out))).unwrap();
        let second = jobs.enqueue(approve(&review(&b, &out))).unwrap();
        let snapshot = jobs.snapshot();
        let first_receipt = snapshot.queued[0].receipt_id.unwrap();
        let second_receipt = snapshot.queued[1].receipt_id.unwrap();
        assert_eq!(jobs.start().unwrap(), 2);
        let state = wait_terminal(&jobs, 1);
        assert!(state.persistence_uncertain);
        assert_eq!(state.queued.len(), 1);
        assert_eq!(state.queued[0].id, second);
        assert!(matches!(jobs.start(), Err(TransferJobError::Persistence)));
        assert!(!out.join("a").exists() && !out.join("b").exists());
        let receipts = temp.path().join("transfers/receipts");
        assert!(receipts.join(format!("{first_receipt}.receipt")).exists());
        assert!(!receipts.join(format!("{second_receipt}.receipt")).exists());
        let request = jobs
            .request_history(TransferHistoryQuery::Interrupted)
            .unwrap();
        let TransferHistoryValue::Interrupted(rows) =
            history_response(&jobs, request).result.unwrap()
        else {
            panic!("interrupted")
        };
        assert!(rows.is_empty()); // The other row is still a live queued job.
        let request = jobs
            .request_history(TransferHistoryQuery::Summaries)
            .unwrap();
        let TransferHistoryValue::Summaries(rows) =
            history_response(&jobs, request).result.unwrap()
        else {
            panic!("summaries")
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, Some(first_receipt));
    }

    #[test]
    fn transfer_jobs_durable_active_queue_edits_are_persisted() {
        let temp = tempfile::tempdir().unwrap();
        let root = crate::platform::app_data::AppDataRoot::from_path(temp.path());
        let out = temp.path().join("out");
        fs::create_dir(&out).unwrap();
        let paths: Vec<_> = ["a", "b", "c", "d"]
            .iter()
            .map(|name| temp.path().join(name))
            .collect();
        for path in &paths {
            fs::write(path, vec![42; 256 * 1024]).unwrap();
        }
        let mut jobs = TransferJobs::with_data_root(ActivityCenter::new(), root.clone());
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
        let release_rx = Mutex::new(release_rx);
        let once = std::sync::atomic::AtomicBool::new(false);
        Arc::get_mut(&mut jobs.shared).unwrap().progress_hook = Some(Arc::new(move |_, event| {
            if matches!(event, ExecutionProgress::Copying { .. })
                && !once.swap(true, Ordering::SeqCst)
            {
                entered_tx.send(()).unwrap();
                release_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap();
            }
        }));
        jobs.enqueue(approve(&review(&paths[0], &out))).unwrap();
        jobs.start().unwrap();
        entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let b = jobs.enqueue(approve(&review(&paths[1], &out))).unwrap();
        let c = jobs.enqueue(approve(&review(&paths[2], &out))).unwrap();
        let d = jobs.enqueue(approve(&review(&paths[3], &out))).unwrap();
        let ids = jobs
            .snapshot()
            .queued
            .iter()
            .map(|j| j.receipt_id.unwrap())
            .collect::<Vec<_>>();
        assert!(jobs.try_reorder(d, Some(b)).unwrap());
        assert!(jobs.try_cancel(c).unwrap());
        release_tx.send(()).unwrap();
        wait_terminal(&jobs, 2); // The cancelled queued job and the active job.
        drop(jobs);
        let mut restored = TransferJobs::with_data_root(ActivityCenter::new(), root);
        let request = restored
            .request_history(TransferHistoryQuery::Interrupted)
            .unwrap();
        let TransferHistoryValue::Interrupted(rows) =
            history_response(&restored, request).result.unwrap()
        else {
            panic!("interrupted")
        };
        assert_eq!(
            rows.iter().map(|r| r.receipt_id).collect::<Vec<_>>(),
            [ids[2], ids[0]]
        );
        assert!(out.join("a").exists());
        assert!(!out.join("b").exists() && !out.join("c").exists() && !out.join("d").exists());
    }
}
