//! App-owned worker for explicitly started, immutable transfer approvals.

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
    ApprovedTransferPlan, CopyControl, CopyExecution, CopyFailure, CopyPauseState, DirectoryTimes,
    EntryKind, ExecutionProgress, ExecutionState, SourceNodeId, TransferDraft, execute_copy_plan,
};

pub const TRANSFER_QUEUE_LIMIT: usize = 32;
const JOB_HISTORY_LIMIT: usize = 32;

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

#[derive(Debug)]
pub enum TransferJobResult {
    Finished(CopyExecution),
    Rejected(CopyFailure),
    CancelledBeforeStart,
    WorkerPanicked,
}

impl TransferJobResult {
    fn status(&self) -> TransferJobStatus {
        use TransferJobStatus::{Cancelled, Completed, Failed, Partial};
        let Self::Finished(report) = self else {
            return match self {
                Self::Rejected(CopyFailure::Cancelled) | Self::CancelledBeforeStart => Cancelled,
                _ => Failed,
            };
        };
        let mut retained = false;
        let mut incomplete = false;
        let mut cancelled = report.cancelled;
        let mut failed = false;
        for row in &report.rows {
            match &row.state {
                ExecutionState::Copied(_) => retained = true,
                ExecutionState::CreatedDirectory(directory) => {
                    retained = true;
                    incomplete |= !matches!(directory.times, DirectoryTimes::Preserved);
                    failed |= matches!(directory.times, DirectoryTimes::Failed(_));
                }
                ExecutionState::Failed(error) => {
                    retained |= error.artifact.is_some();
                    incomplete = true;
                    if matches!(error.failure, CopyFailure::Cancelled) {
                        cancelled = true;
                    } else {
                        failed = true;
                    }
                }
                ExecutionState::Cancelled => {
                    incomplete = true;
                    cancelled = true;
                }
                ExecutionState::BlockedByParent(_) => incomplete = true,
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
}

#[derive(Clone, Debug)]
pub struct TransferJobInfo {
    pub id: TransferJobId,
    pub activity: Option<ActivityKey>,
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
}

#[derive(Debug)]
pub enum TransferJobError {
    QueueFull,
    ApprovalAlreadyQueued,
    Closed,
    IdentityExhausted,
    WorkerUnavailable(std::io::Error),
}

impl fmt::Display for TransferJobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QueueFull => write!(f, "The transfer queue is full."),
            Self::ApprovalAlreadyQueued => write!(f, "This review has already been queued."),
            Self::Closed => write!(f, "The transfer service is closing."),
            Self::IdentityExhausted => write!(f, "Transfer job identities are exhausted."),
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
}

#[cfg(test)]
type TransferProgressHook = Arc<dyn Fn(TransferJobId, ExecutionProgress) + Send + Sync>;

#[derive(Default)]
struct Shared {
    state: Mutex<QueueState>,
    ready: Condvar,
    #[cfg(test)]
    progress_hook: Option<TransferProgressHook>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, QueueState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn cancel(&self, id: TransferJobId) -> bool {
        let mut state = self.lock();
        if let Some(job) = state.active.as_ref().filter(|job| job.info.id == id) {
            if let Some(activity) = &job.activity {
                activity.mark_cancelling();
            }
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

impl TransferJobs {
    pub fn new(activity: ActivityCenter) -> Self {
        Self {
            shared: Arc::new(Shared::default()),
            activity,
            worker: None,
        }
    }

    pub fn enqueue(&self, plan: ApprovedTransferPlan) -> Result<TransferJobId, TransferJobError> {
        let mut state = self.shared.lock();
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
        if !plan.claim_for_queue() {
            return Err(TransferJobError::ApprovalAlreadyQueued);
        }
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
            activity.set_cancel_handler(move || {
                if let Some(shared) = shared.upgrade() {
                    shared.cancel(id);
                }
            });
        }
        let info = TransferJobInfo {
            id,
            activity: activity.as_ref().map(ActivityTask::key),
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
        let mut state = self.shared.lock();
        let Some(from) = state.queued.iter().position(|job| job.info.id == id) else {
            return false;
        };
        let to = match before {
            Some(before) => {
                let Some(index) = state.queued.iter().position(|job| job.info.id == before) else {
                    return false;
                };
                index
            }
            None => state.queued.len(),
        };
        let to = to - usize::from(from < to);
        let armed = state.queued.iter().take_while(|job| job.info.armed).count();
        if (from < armed) != (to < armed) {
            return false;
        }
        if from != to
            && let Some(job) = state.queued.remove(from)
        {
            state.queued.insert(to, job);
        }
        true
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
        self.shared.cancel(id)
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
        }
    }

    pub fn shutdown(&mut self) {
        {
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
    loop {
        let (pending, control, preparing) = {
            let mut state = shared.lock();
            loop {
                if state.closing {
                    return;
                }
                if state.queued.front().is_some_and(|job| job.info.armed) {
                    break;
                }
                state = shared
                    .ready
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            let Some(pending) = state.queued.pop_front() else {
                continue;
            };
            let control = CopyControl::default();
            let started = Instant::now();
            let preparing = Arc::new(AtomicBool::new(true));
            if let Some(activity) = &pending.activity {
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
        let result = catch_unwind(AssertUnwindSafe(|| {
            execute_copy_plan(&pending.plan, &control, |event| {
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
            })
        }));
        drop(pending);
        drop(control);
        let result = match result {
            Ok(Ok(report)) => TransferJobResult::Finished(report),
            Ok(Err(error)) => TransferJobResult::Rejected(error),
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

fn retain_terminal(
    state: &mut QueueState,
    mut info: TransferJobInfo,
    activity: Option<ActivityTask>,
    result: TransferJobResult,
) {
    info.status = result.status();
    info.armed = false;
    info.progress.current = None;
    info.progress.staged_bytes = 0;
    info.current_path = None;
    info.result = Some(Arc::new(result));
    if let Some(task) = activity {
        let (outcome, diagnostic) = match info.status {
            TransferJobStatus::Completed => (ActivityOutcome::Completed, None),
            TransferJobStatus::Partial => (ActivityOutcome::Partial, None),
            TransferJobStatus::Cancelled => (ActivityOutcome::Cancelled, None),
            _ => (
                ActivityOutcome::Failed,
                Some(ActivityDiagnostic::WorkerFailed),
            ),
        };
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
            ExecutionState::Failed(super::super::CopyError {
                artifact: Some(super::super::CopyRecoveryArtifact::Owned(artifact)), ..
            }) if artifact.state == super::super::CopyArtifactState::Published
        ));
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
}
