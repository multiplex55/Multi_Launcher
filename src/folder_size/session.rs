use crate::activity::{
    ActivityCenter, ActivityDiagnostic, ActivityIncarnation, ActivityOperation, ActivityOutcome,
    ActivityOwner, ActivityProgress, ActivityProgressUnit, ActivityRetirement, ActivitySlot,
    ActivityTask,
};
use crate::batch_rename::ExplicitDirectoryRoot;
use crate::file_operations::ScanCancellation;
use crate::thread_reaper::{self, ReapPermit};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, Weak};
use std::thread::{self, JoinHandle};

use super::model::{FolderSizeProgress, FolderSizeResult};
use super::scan::scan_folder_size;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderSizeSessionError {
    Closed,
    GenerationExhausted,
    WorkerCapacity(String),
    WorkerStart(String),
    WorkerPanicked,
    InvalidWorkerResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderSizeSessionStatus {
    Idle,
    Scanning,
    RefreshQueued,
    Cancelled,
    Complete,
    Partial,
    Failed,
    Closed,
}

impl std::fmt::Display for FolderSizeSessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => formatter.write_str("folder-size session is closed"),
            Self::GenerationExhausted => {
                formatter.write_str("folder-size scan generation space is exhausted")
            }
            Self::WorkerCapacity(message) | Self::WorkerStart(message) => {
                formatter.write_str(message)
            }
            Self::WorkerPanicked => formatter.write_str("folder-size worker panicked"),
            Self::InvalidWorkerResult => {
                formatter.write_str("folder-size worker returned a stale result")
            }
        }
    }
}

impl std::error::Error for FolderSizeSessionError {}

#[derive(Debug, Clone)]
struct ScanRequest {
    generation: u64,
    root: ExplicitDirectoryRoot,
    recursive: bool,
    cancellation: ScanCancellation,
}

struct ActiveScan {
    generation: u64,
    cancellation: ScanCancellation,
    activity: Option<ActivityTask>,
    completion: Option<ActivityCompletion>,
}

#[derive(Clone, Copy)]
struct ActivityCompletion {
    outcome: ActivityOutcome,
    diagnostic: Option<ActivityDiagnostic>,
}

struct ActivityRetirementTicket {
    activity: Option<ActivityTask>,
    fallback: Option<ActivityCompletion>,
}

impl ActivityRetirementTicket {
    fn new(activity: Option<ActivityTask>, fallback: Option<ActivityCompletion>) -> Self {
        Self { activity, fallback }
    }

    fn after_join(mut self, panicked: bool) {
        if let Some(activity) = self.activity.take() {
            if panicked {
                activity.finish(
                    ActivityOutcome::Failed,
                    Some(ActivityDiagnostic::WorkerFailed),
                    ActivityRetirement::Pending,
                );
            }
            activity.retirement_confirmed();
        }
    }
}

impl Drop for ActivityRetirementTicket {
    fn drop(&mut self) {
        if let Some(activity) = self.activity.take() {
            if let Some(completion) = self.fallback {
                activity.finish(
                    completion.outcome,
                    completion.diagnostic.or(Some(ActivityDiagnostic::Retiring)),
                    ActivityRetirement::Unavailable,
                );
                activity.retirement_unavailable();
            } else {
                activity.set_diagnostic(Some(ActivityDiagnostic::Retiring));
                activity.retirement_unavailable();
            }
        }
    }
}

#[derive(Default)]
struct SessionState {
    closed: bool,
    generation: u64,
    pending: Option<ScanRequest>,
    active: Option<ActiveScan>,
    accepted_result: Option<Arc<FolderSizeResult>>,
    progress: Option<FolderSizeProgress>,
    error: Option<FolderSizeSessionError>,
    cancelled: bool,
}

struct SessionShared {
    state: Mutex<SessionState>,
    wake: Arc<dyn Fn() + Send + Sync + 'static>,
    job: Arc<dyn FolderSizeScanJob>,
    activity_center: Option<ActivityCenter>,
}

struct WorkerOwner {
    join: JoinHandle<()>,
    reap_permit: ReapPermit,
    generation: u64,
    activity: Option<ActivityTask>,
}

trait FolderSizeScanJob: Send + Sync + 'static {
    fn run(
        &self,
        request: ScanRequest,
        progress: &mut dyn FnMut(FolderSizeProgress),
    ) -> FolderSizeResult;
}

struct NativeFolderSizeScanJob;

impl FolderSizeScanJob for NativeFolderSizeScanJob {
    fn run(
        &self,
        request: ScanRequest,
        progress: &mut dyn FnMut(FolderSizeProgress),
    ) -> FolderSizeResult {
        scan_folder_size(
            request.root,
            request.recursive,
            request.generation,
            &request.cancellation,
            progress,
        )
    }
}

pub struct FolderSizeSession {
    shared: Arc<SessionShared>,
    worker: Mutex<Option<WorkerOwner>>,
}

pub struct FolderSizeSessionView {
    generation: u64,
    result: Option<Arc<FolderSizeResult>>,
    progress: Option<FolderSizeProgress>,
    busy: bool,
    closed: bool,
    status: FolderSizeSessionStatus,
    error: Option<FolderSizeSessionError>,
}

impl FolderSizeSessionView {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn result(&self) -> Option<&Arc<FolderSizeResult>> {
        self.result.as_ref()
    }

    pub fn progress(&self) -> Option<&FolderSizeProgress> {
        self.progress.as_ref()
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn status(&self) -> FolderSizeSessionStatus {
        self.status
    }

    pub fn result_is_current(&self) -> bool {
        self.result
            .as_ref()
            .is_some_and(|result| result.generation() == self.generation)
    }

    pub fn progress_is_current(&self) -> bool {
        self.progress
            .as_ref()
            .is_some_and(|progress| progress.generation() == self.generation)
    }

    pub fn error(&self) -> Option<&FolderSizeSessionError> {
        self.error.as_ref()
    }
}

impl FolderSizeSession {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self::with_job_wake_and_activity(Arc::new(NativeFolderSizeScanJob), Arc::new(wake), None)
    }

    pub fn new_with_activity_center(
        wake: impl Fn() + Send + Sync + 'static,
        activity_center: ActivityCenter,
    ) -> Self {
        Self::with_job_wake_and_activity(
            Arc::new(NativeFolderSizeScanJob),
            Arc::new(wake),
            Some(activity_center),
        )
    }

    fn with_job_and_wake(
        job: Arc<dyn FolderSizeScanJob>,
        wake: Arc<dyn Fn() + Send + Sync + 'static>,
    ) -> Self {
        Self::with_job_wake_and_activity(job, wake, None)
    }

    fn with_job_wake_and_activity(
        job: Arc<dyn FolderSizeScanJob>,
        wake: Arc<dyn Fn() + Send + Sync + 'static>,
        activity_center: Option<ActivityCenter>,
    ) -> Self {
        Self {
            shared: Arc::new(SessionShared {
                state: Mutex::new(SessionState::default()),
                wake,
                job,
                activity_center,
            }),
            worker: Mutex::new(None),
        }
    }

    /// Start a scan or replace the one queued behind a native call. The active
    /// call is cancelled cooperatively and retains the sole worker slot until
    /// it returns; refresh never starts overlapping filesystem work.
    pub fn refresh(
        &self,
        root: ExplicitDirectoryRoot,
        recursive: bool,
    ) -> Result<u64, FolderSizeSessionError> {
        let (generation, replaced_pending, superseded_activity) = {
            let mut state = self.shared.state.lock().unwrap();
            if state.closed {
                return Err(FolderSizeSessionError::Closed);
            }
            let generation = state
                .generation
                .checked_add(1)
                .ok_or(FolderSizeSessionError::GenerationExhausted)?;
            state.generation = generation;
            let superseded_activity = state.active.as_mut().and_then(|active| {
                let activity = active.activity.clone();
                if active.completion.is_none() {
                    active.cancellation.cancel();
                    active.completion = Some(ActivityCompletion {
                        outcome: ActivityOutcome::Superseded,
                        diagnostic: Some(ActivityDiagnostic::Superseded),
                    });
                    activity
                } else {
                    None
                }
            });
            // A result belongs to the generation that produced it. Keeping it
            // visible while a replacement scan is active invites callers to
            // treat a stale tree as the current selection and retains two
            // result path budgets at once.
            state.accepted_result = None;
            state.progress = None;
            state.error = None;
            state.cancelled = false;
            let replaced_pending = state.pending.replace(ScanRequest {
                generation,
                root,
                recursive,
                cancellation: ScanCancellation::new(),
            });
            (generation, replaced_pending, superseded_activity)
        };
        if let Some(activity) = superseded_activity {
            activity.mark_cancelling();
            activity.clear_cancel_handler();
        }
        if let Some(replaced) = replaced_pending {
            replaced.cancellation.cancel();
        }
        let result = self.ensure_worker();
        (self.shared.wake)();
        result.map(|()| generation)
    }

    /// Invalidate live publication immediately. The latest progress snapshot
    /// is frozen with its verified subtotal while a blocked native call settles.
    pub fn cancel(&self) -> Result<bool, FolderSizeSessionError> {
        let (cancelled, pending, activity) = {
            let mut state = self.shared.state.lock().unwrap();
            cancel_locked(&mut state, None)?
        };
        if !cancelled {
            return Ok(false);
        }
        if let Some(activity) = activity {
            activity.mark_cancelling();
            activity.clear_cancel_handler();
        }
        if let Some(pending) = pending {
            pending.cancellation.cancel();
        }
        (self.shared.wake)();
        Ok(cancelled)
    }

    /// Stop accepting publication and cancel pending/active work. A blocked
    /// native call remains owned until return and is joined asynchronously on
    /// drop rather than blocking the caller.
    pub fn close(&self) {
        let activity = {
            let mut state = self.shared.state.lock().unwrap();
            if state.closed {
                return;
            }
            state.closed = true;
            state.generation = state.generation.saturating_add(1);
            let activity = state.active.as_mut().and_then(|active| {
                let activity = active.activity.clone();
                if active.completion.is_none() {
                    active.cancellation.cancel();
                    active.completion = Some(ActivityCompletion {
                        outcome: ActivityOutcome::Cancelled,
                        diagnostic: None,
                    });
                    activity
                } else {
                    None
                }
            });
            if let Some(pending) = state.pending.take() {
                pending.cancellation.cancel();
            }
            state.accepted_result = None;
            state.progress = None;
            state.error = None;
            state.cancelled = false;
            activity
        };
        if let Some(activity) = activity {
            activity.mark_cancelling();
            activity.clear_cancel_handler();
        }
        (self.shared.wake)();
    }

    /// Join a completed slot and start the most recent queued refresh, if any.
    /// Call from the owner's event/update path after the session wake.
    pub fn poll(&self) -> Result<(), FolderSizeSessionError> {
        self.ensure_worker()
    }

    pub fn view(&self) -> FolderSizeSessionView {
        let _ = self.poll();
        let state = self.shared.state.lock().unwrap();
        let busy = state.active.is_some() || state.pending.is_some();
        let status = if state.closed {
            FolderSizeSessionStatus::Closed
        } else if state.cancelled {
            FolderSizeSessionStatus::Cancelled
        } else if state.pending.is_some() {
            FolderSizeSessionStatus::RefreshQueued
        } else if state.active.is_some() {
            FolderSizeSessionStatus::Scanning
        } else if state.error.is_some() {
            FolderSizeSessionStatus::Failed
        } else {
            state
                .accepted_result
                .as_ref()
                .map_or(FolderSizeSessionStatus::Idle, |result| {
                    match result.status() {
                        super::model::FolderSizeStatus::Complete => {
                            FolderSizeSessionStatus::Complete
                        }
                        super::model::FolderSizeStatus::Partial => FolderSizeSessionStatus::Partial,
                        super::model::FolderSizeStatus::Cancelled => {
                            FolderSizeSessionStatus::Cancelled
                        }
                        super::model::FolderSizeStatus::Failed => FolderSizeSessionStatus::Failed,
                    }
                })
        };
        FolderSizeSessionView {
            generation: state.generation,
            result: state.accepted_result.clone(),
            progress: state.progress.clone(),
            busy,
            closed: state.closed,
            status,
            error: state.error.clone(),
        }
    }

    fn ensure_worker(&self) -> Result<(), FolderSizeSessionError> {
        let mut worker_slot = self.worker.lock().unwrap();
        if let Some(worker) = worker_slot.as_ref()
            && !worker.join.is_finished()
        {
            let scan_settled = self.shared.state.lock().unwrap().active.is_none();
            if !scan_settled {
                return Ok(());
            }
            let Some(worker) = worker_slot.take() else {
                return Ok(());
            };
            let worker_generation = worker.generation;
            let wake = Arc::clone(&self.shared.wake);
            let ticket = ActivityRetirementTicket::new(worker.activity.clone(), None);
            let transfer = worker
                .reap_permit
                .reap_with_callback(worker.join, move |panicked| {
                    ticket.after_join(panicked);
                    wake();
                });
            if let Err(error) = transfer {
                let failure = FolderSizeSessionError::WorkerCapacity(format!(
                    "folder-size worker could not transfer its settled handle: {error}"
                ));
                let mut state = self.shared.state.lock().unwrap();
                if !state.closed && state.active.is_none() {
                    // The native scan has settled, but the bounded join
                    // supervisor could not take ownership of its epilogue.
                    // Fail the queued generation explicitly instead of
                    // leaving it busy with no future retirement wake.
                    if let Some(pending) = state.pending.take() {
                        pending.cancellation.cancel();
                        state.error = Some(failure.clone());
                    } else if state.generation == worker_generation {
                        state.error = Some(failure.clone());
                    }
                }
                drop(state);
                (self.shared.wake)();
                return Err(failure);
            }
        }
        if worker_slot
            .as_ref()
            .is_some_and(|worker| worker.join.is_finished())
        {
            let Some(worker) = worker_slot.take() else {
                return Ok(());
            };
            let worker_generation = worker.generation;
            let panicked = worker.join.join().is_err();
            drop(worker.reap_permit);
            if let Some(activity) = worker.activity {
                if panicked {
                    activity.finish(
                        ActivityOutcome::Failed,
                        Some(ActivityDiagnostic::WorkerFailed),
                        ActivityRetirement::Pending,
                    );
                }
                activity.retirement_confirmed();
            }
            if panicked {
                let mut state = self.shared.state.lock().unwrap();
                if state
                    .active
                    .as_ref()
                    .is_some_and(|active| active.generation == worker_generation)
                {
                    state.active = None;
                }
                if !state.closed && state.generation == worker_generation {
                    state.error = Some(FolderSizeSessionError::WorkerPanicked);
                }
            }
        }

        {
            let state = self.shared.state.lock().unwrap();
            if state.closed || state.pending.is_none() {
                return Ok(());
            }
        }

        let pending_generation = {
            let state = self.shared.state.lock().unwrap();
            if state.closed {
                return Ok(());
            }
            let Some(pending) = state.pending.as_ref() else {
                return Ok(());
            };
            pending.generation
        };
        let reap_permit = match thread_reaper::reserve() {
            Ok(permit) => permit,
            Err(error) => {
                let failure = FolderSizeSessionError::WorkerCapacity(format!(
                    "folder-size worker capacity is unavailable: {error}"
                ));
                let mut state = self.shared.state.lock().unwrap();
                if !state.closed
                    && state.generation == pending_generation
                    && state
                        .pending
                        .as_ref()
                        .is_some_and(|pending| pending.generation == pending_generation)
                {
                    if let Some(pending) = state.pending.take() {
                        pending.cancellation.cancel();
                    }
                    state.error = Some(failure.clone());
                }
                return Err(failure);
            }
        };
        let completion = reap_permit.completion_notifier();
        let request = {
            let mut state = self.shared.state.lock().unwrap();
            if state.closed {
                drop(reap_permit);
                return Err(FolderSizeSessionError::Closed);
            }
            let Some(request) = state.pending.take() else {
                drop(reap_permit);
                return Ok(());
            };
            state.active = Some(ActiveScan {
                generation: request.generation,
                cancellation: request.cancellation.clone(),
                activity: None,
                completion: None,
            });
            request
        };
        let generation = request.generation;
        let activity = self.shared.activity_center.as_ref().and_then(|center| {
            center.start(
                ActivityOwner::FolderSize,
                ActivitySlot::Primary,
                ActivityIncarnation::new(1),
                ActivityOperation::MeasureFolderSize,
            )
        });
        if let Some(activity) = activity.as_ref() {
            let weak = Arc::downgrade(&self.shared);
            activity.set_cancel_handler(move || {
                cancel_activity_generation(weak.clone(), generation);
            });
            activity.mark_running();
            let cancelled_before_spawn = {
                let mut state = self.shared.state.lock().unwrap();
                let current = state
                    .active
                    .as_mut()
                    .filter(|active| active.generation == generation);
                if let Some(active) = current {
                    active.activity = Some(activity.clone());
                    state.closed || state.cancelled || state.generation != generation
                } else {
                    true
                }
            };
            if cancelled_before_spawn {
                activity.mark_cancelling();
                activity.clear_cancel_handler();
            }
        }
        let shared = Arc::clone(&self.shared);
        let job = Arc::clone(&self.shared.job);
        let wake = Arc::clone(&self.shared.wake);
        let worker_activity = activity.clone();
        let join = thread::Builder::new()
            .name("folder-size-scan".into())
            .spawn(move || {
                let completion = completion;
                let run = catch_unwind(AssertUnwindSafe(|| {
                    let mut publish_progress = |progress: FolderSizeProgress| {
                        let progress_count =
                            u64::try_from(progress.entries_seen()).unwrap_or(u64::MAX);
                        let mut state = shared.state.lock().unwrap();
                        let accepted = if progress.generation() == generation
                            && !state.closed
                            && state.generation == generation
                            && state.active.as_ref().is_some_and(|active| {
                                active.generation == generation
                                    && !active.cancellation.is_cancelled()
                            }) {
                            state.progress = Some(progress);
                            true
                        } else {
                            false
                        };
                        drop(state);
                        if accepted && let Some(activity) = worker_activity.as_ref() {
                            activity.set_progress(Some(ActivityProgress::new(
                                progress_count,
                                None,
                                ActivityProgressUnit::Items,
                            )));
                        }
                        (shared.wake)();
                    };
                    job.run(request, &mut publish_progress)
                }));
                let mut state = shared.state.lock().unwrap();
                let active_matches = state
                    .active
                    .as_ref()
                    .is_some_and(|active| active.generation == generation);
                let active_retirement = state
                    .active
                    .as_ref()
                    .filter(|active| active.generation == generation)
                    .and_then(|active| active.completion);
                let mut activity_completion = if let Some(completion) = active_retirement {
                    completion
                } else if state.closed || state.cancelled {
                    ActivityCompletion {
                        outcome: ActivityOutcome::Cancelled,
                        diagnostic: None,
                    }
                } else if state.generation != generation {
                    ActivityCompletion {
                        outcome: ActivityOutcome::Superseded,
                        diagnostic: Some(ActivityDiagnostic::Superseded),
                    }
                } else {
                    ActivityCompletion {
                        outcome: ActivityOutcome::Failed,
                        diagnostic: Some(ActivityDiagnostic::WorkerFailed),
                    }
                };
                if active_matches {
                    if state.generation == generation && !state.closed && !state.cancelled {
                        match run {
                            Ok(result) if result.generation() == generation => {
                                activity_completion = match result.status() {
                                    super::model::FolderSizeStatus::Complete => {
                                        ActivityCompletion {
                                            outcome: ActivityOutcome::Completed,
                                            diagnostic: None,
                                        }
                                    }
                                    super::model::FolderSizeStatus::Partial => ActivityCompletion {
                                        outcome: ActivityOutcome::Partial,
                                        diagnostic: Some(ActivityDiagnostic::PartialResult),
                                    },
                                    super::model::FolderSizeStatus::Cancelled => {
                                        ActivityCompletion {
                                            outcome: ActivityOutcome::Cancelled,
                                            diagnostic: None,
                                        }
                                    }
                                    super::model::FolderSizeStatus::Failed => ActivityCompletion {
                                        outcome: ActivityOutcome::Failed,
                                        diagnostic: Some(ActivityDiagnostic::ResultUnavailable),
                                    },
                                };
                                state.accepted_result = Some(Arc::new(result));
                                state.progress = None;
                                state.error = None;
                                if let Some(active) = state.active.as_mut()
                                    && active.generation == generation
                                {
                                    active.completion = Some(activity_completion);
                                }
                            }
                            Ok(_) => {
                                state.error = Some(FolderSizeSessionError::InvalidWorkerResult);
                                activity_completion = ActivityCompletion {
                                    outcome: ActivityOutcome::Failed,
                                    diagnostic: Some(ActivityDiagnostic::ResultUnavailable),
                                };
                            }
                            Err(_) => {
                                state.error = Some(FolderSizeSessionError::WorkerPanicked);
                                activity_completion = ActivityCompletion {
                                    outcome: ActivityOutcome::Failed,
                                    diagnostic: Some(ActivityDiagnostic::WorkerFailed),
                                };
                            }
                        }
                    }
                    if let Some(active) = state.active.as_mut()
                        && active.generation == generation
                        && active.completion.is_none()
                    {
                        active.completion = Some(activity_completion);
                    }
                }
                drop(state);
                if let Some(activity) = worker_activity.as_ref() {
                    activity.finish(
                        activity_completion.outcome,
                        activity_completion.diagnostic,
                        ActivityRetirement::Pending,
                    );
                }
                {
                    let mut state = shared.state.lock().unwrap();
                    if state
                        .active
                        .as_ref()
                        .is_some_and(|active| active.generation == generation)
                    {
                        state.active = None;
                    }
                }
                drop(completion);
                (wake)();
            })
            .map_err(|error| {
                let mut state = self.shared.state.lock().unwrap();
                if state
                    .active
                    .as_ref()
                    .is_some_and(|active| active.generation == generation)
                {
                    if let Some(active) = state.active.as_ref() {
                        active.cancellation.cancel();
                    }
                    state.active = None;
                }
                let failure = FolderSizeSessionError::WorkerStart(error.to_string());
                if !state.closed && state.generation == generation {
                    if state
                        .pending
                        .as_ref()
                        .is_some_and(|pending| pending.generation == generation)
                        && let Some(pending) = state.pending.take()
                    {
                        pending.cancellation.cancel();
                    }
                    state.error = Some(failure.clone());
                }
                drop(state);
                if let Some(activity) = activity.as_ref() {
                    activity.finish(
                        ActivityOutcome::Failed,
                        Some(ActivityDiagnostic::WorkerUnavailable),
                        ActivityRetirement::Confirmed,
                    );
                }
                failure
            })?;
        *worker_slot = Some(WorkerOwner {
            join,
            reap_permit,
            generation,
            activity,
        });
        Ok(())
    }
}

fn cancel_activity_generation(shared: Weak<SessionShared>, generation: u64) {
    let Some(shared) = shared.upgrade() else {
        return;
    };
    let cancellation = {
        let mut state = shared.state.lock().unwrap();
        cancel_locked(&mut state, Some(generation)).ok()
    };
    if let Some((cancelled, pending, activity)) = cancellation
        && cancelled
    {
        if let Some(pending) = pending {
            pending.cancellation.cancel();
        }
        if let Some(activity) = activity {
            activity.mark_cancelling();
            activity.clear_cancel_handler();
        }
        (shared.wake)();
    }
}

fn cancel_locked(
    state: &mut SessionState,
    expected_active_generation: Option<u64>,
) -> Result<(bool, Option<ScanRequest>, Option<ActivityTask>), FolderSizeSessionError> {
    if state.closed {
        return Err(FolderSizeSessionError::Closed);
    }
    if let Some(expected_generation) = expected_active_generation
        && (state.generation != expected_generation
            || !state.active.as_ref().is_some_and(|active| {
                active.generation == expected_generation && active.completion.is_none()
            }))
    {
        return Ok((false, None, None));
    }
    let active_is_live = state
        .active
        .as_ref()
        .is_some_and(|active| active.completion.is_none());
    if !active_is_live && state.pending.is_none() {
        return Ok((false, None, None));
    }
    let generation = state
        .generation
        .checked_add(1)
        .ok_or(FolderSizeSessionError::GenerationExhausted)?;
    state.generation = generation;
    let pending = if expected_active_generation.is_none() {
        state.pending.take()
    } else {
        None
    };
    let activity = state.active.as_mut().and_then(|active| {
        if active.completion.is_none() {
            active.cancellation.cancel();
            active.completion = Some(ActivityCompletion {
                outcome: ActivityOutcome::Cancelled,
                diagnostic: None,
            });
            active.activity.clone()
        } else {
            None
        }
    });
    state.cancelled = true;
    state.error = None;
    Ok((true, pending, activity))
}

impl Drop for FolderSizeSession {
    fn drop(&mut self) {
        let activity_completion = {
            let mut state = self.shared.state.lock().unwrap();
            if !state.closed {
                state.closed = true;
                state.generation = state.generation.saturating_add(1);
            }
            let activity = state.active.as_mut().and_then(|active| {
                if active.completion.is_none() {
                    active.cancellation.cancel();
                    active.completion = Some(ActivityCompletion {
                        outcome: ActivityOutcome::Cancelled,
                        diagnostic: None,
                    });
                }
                active.activity.clone().zip(active.completion)
            });
            if let Some(pending) = state.pending.take() {
                pending.cancellation.cancel();
            }
            state.accepted_result = None;
            state.progress = None;
            activity
        };
        let fallback = activity_completion
            .as_ref()
            .map(|(_, completion)| *completion);
        if let Some((activity, completion)) = activity_completion.as_ref() {
            if matches!(
                completion.outcome,
                ActivityOutcome::Cancelled | ActivityOutcome::Superseded
            ) {
                activity.mark_cancelling();
            }
            activity.clear_cancel_handler();
        }
        if let Some(worker) = self.worker.get_mut().unwrap().take() {
            let ticket = ActivityRetirementTicket::new(worker.activity.clone(), fallback);
            let transfer = worker
                .reap_permit
                .reap_with_callback(worker.join, move |panicked| ticket.after_join(panicked));
            if let Err(error) = transfer {
                tracing::error!(%error, "failed to transfer folder-size worker to join reaper");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::time::{Duration, Instant};

    struct ActiveJobCount(Arc<AtomicUsize>);

    impl Drop for ActiveJobCount {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::AcqRel);
        }
    }

    struct EarlyWakeJob {
        started: Sender<u64>,
        release_first: Mutex<Receiver<()>>,
        block_final_wake: Arc<AtomicBool>,
        active: Arc<AtomicUsize>,
        maximum_active: Arc<AtomicUsize>,
    }

    impl FolderSizeScanJob for EarlyWakeJob {
        fn run(
            &self,
            request: ScanRequest,
            progress: &mut dyn FnMut(FolderSizeProgress),
        ) -> FolderSizeResult {
            let active = self.active.fetch_add(1, Ordering::AcqRel) + 1;
            self.maximum_active.fetch_max(active, Ordering::AcqRel);
            let _active = ActiveJobCount(Arc::clone(&self.active));
            let generation = request.generation;
            let _ = self.started.send(generation);
            if generation == 1 {
                let _ = self.release_first.lock().unwrap().recv();
            }
            let result = scan_folder_size(
                request.root,
                request.recursive,
                generation,
                &request.cancellation,
                progress,
            );
            if generation == 1 {
                self.block_final_wake.store(true, Ordering::Release);
            }
            result
        }
    }

    struct HoldAfterProgressJob {
        hold_generation: u64,
        progress_generations: Sender<u64>,
        completed_runs: Sender<u64>,
        release: Mutex<Receiver<()>>,
    }

    impl FolderSizeScanJob for HoldAfterProgressJob {
        fn run(
            &self,
            request: ScanRequest,
            progress: &mut dyn FnMut(FolderSizeProgress),
        ) -> FolderSizeResult {
            let generation = request.generation;
            let mut relay_progress = |snapshot: FolderSizeProgress| {
                let snapshot_generation = snapshot.generation();
                progress(snapshot);
                let _ = self.progress_generations.send(snapshot_generation);
            };
            let result = scan_folder_size(
                request.root,
                request.recursive,
                generation,
                &request.cancellation,
                &mut relay_progress,
            );
            if generation == self.hold_generation {
                let _ = self.release.lock().unwrap().recv();
            }
            let _ = self.completed_runs.send(generation);
            result
        }
    }

    struct MismatchedGenerationJob;

    impl FolderSizeScanJob for MismatchedGenerationJob {
        fn run(
            &self,
            request: ScanRequest,
            progress: &mut dyn FnMut(FolderSizeProgress),
        ) -> FolderSizeResult {
            scan_folder_size(
                request.root,
                request.recursive,
                request.generation.saturating_add(1),
                &request.cancellation,
                progress,
            )
        }
    }

    fn wait_for_view(
        session: &FolderSizeSession,
        predicate: impl Fn(&FolderSizeSessionView) -> bool,
    ) -> FolderSizeSessionView {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let view = session.view();
            if predicate(&view) {
                return view;
            }
            assert!(
                Instant::now() < deadline,
                "folder-size session did not reach the expected state; status={:?}, busy={}, error={:?}, result={:?}",
                view.status(),
                view.is_busy(),
                view.error(),
                view.result()
                    .map(|result| (result.status(), result.issues()))
            );
            thread::yield_now();
        }
    }

    fn explicit_root(path: &std::path::Path) -> ExplicitDirectoryRoot {
        ExplicitDirectoryRoot::new(path.to_path_buf()).unwrap()
    }

    fn wait_for_activity_row(
        center: &ActivityCenter,
        key: crate::activity::ActivityKey,
        predicate: impl Fn(&crate::activity::ActivityRow) -> bool,
    ) -> crate::activity::ActivityRow {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(row) = center
                .snapshot()
                .rows
                .into_iter()
                .find(|row| row.key == key)
                && predicate(&row)
            {
                return row;
            }
            assert!(
                Instant::now() < deadline,
                "Folder Size activity did not settle"
            );
            thread::yield_now();
        }
    }

    fn latest_folder_size_activity_key(center: &ActivityCenter) -> crate::activity::ActivityKey {
        center
            .snapshot()
            .rows
            .into_iter()
            .filter(|row| row.key.owner() == ActivityOwner::FolderSize)
            .max_by_key(|row| row.key.operation_id().get())
            .expect("a promoted Folder Size scan has an activity row")
            .key
    }

    #[test]
    fn folder_size_activity_starts_on_promotion_tracks_progress_and_supersedes() {
        let first_root = crate::folder_size::disposable_scan_fixture_root();
        let superseded_root = crate::folder_size::disposable_scan_fixture_root();
        std::fs::write(first_root.path().join("payload.bin"), b"x").unwrap();
        let (progress_tx, progress_rx) = mpsc::channel();
        let (completed_tx, _completed_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let job = Arc::new(HoldAfterProgressJob {
            hold_generation: 1,
            progress_generations: progress_tx,
            completed_runs: completed_tx,
            release: Mutex::new(release_rx),
        });
        let center = ActivityCenter::new();
        let session = FolderSizeSession::with_job_wake_and_activity(
            job,
            Arc::new(|| {}),
            Some(center.clone()),
        );

        let first_generation = session
            .refresh(explicit_root(first_root.path()), true)
            .unwrap();
        assert_eq!(
            progress_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            first_generation
        );
        let first_row =
            wait_for_activity_row(&center, latest_folder_size_activity_key(&center), |row| {
                row.lifecycle == crate::activity::ActivityLifecycle::Running
                    && row.progress.is_some_and(|progress| progress.completed > 0)
            });
        assert_eq!(first_row.operation, ActivityOperation::MeasureFolderSize);
        assert_eq!(
            first_row.lifecycle,
            crate::activity::ActivityLifecycle::Running
        );
        let progress = first_row.progress.expect("scan progress is attached");
        assert_eq!(progress.total, None);
        assert_eq!(progress.unit, ActivityProgressUnit::Items);
        assert!(progress.completed > 0);

        let superseded_generation = session
            .refresh(explicit_root(superseded_root.path()), true)
            .unwrap();
        assert!(superseded_generation > first_generation);
        let latest_root = crate::folder_size::disposable_scan_fixture_root();
        let latest_generation = session
            .refresh(explicit_root(latest_root.path()), true)
            .unwrap();
        assert!(latest_generation > first_generation);
        let rows_while_queued = center.snapshot().rows;
        assert_eq!(rows_while_queued.len(), 1, "queued refresh has no task yet");
        assert_eq!(
            rows_while_queued[0].lifecycle,
            crate::activity::ActivityLifecycle::Cancelling
        );
        assert_eq!(
            center.request_cancel(first_row.key),
            crate::activity::ActivityCancelResult::Unsupported,
            "superseded scan has relinquished its cancellation capability"
        );
        cancel_activity_generation(Arc::downgrade(&session.shared), first_generation);
        let after_stale_cancel = session.view();
        assert_eq!(after_stale_cancel.generation(), latest_generation);
        assert_eq!(
            session
                .shared
                .state
                .lock()
                .unwrap()
                .pending
                .as_ref()
                .map(|pending| pending.generation),
            Some(latest_generation),
            "a stale A cancellation cannot replace the queued C request"
        );

        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut saw_latest_progress = false;
        while !saw_latest_progress {
            let _ = session.view();
            saw_latest_progress = progress_rx
                .recv_timeout(Duration::from_millis(25))
                .is_ok_and(|generation| generation == latest_generation);
            assert!(Instant::now() < deadline, "latest scan was not promoted");
        }
        let latest = wait_for_view(&session, |view| {
            view.status() == FolderSizeSessionStatus::Complete && view.result_is_current()
        });
        assert_eq!(latest.result().unwrap().generation(), latest_generation);
        assert_eq!(latest.result().unwrap().root(), latest_root.path());
        let deadline = Instant::now() + Duration::from_secs(5);
        let (first_terminal, latest_row) = loop {
            let _ = session.poll();
            let rows = center.snapshot().rows;
            assert_eq!(rows.len(), 2, "only running A and promoted C have tickets");
            let first = rows.iter().find(|row| row.key == first_row.key);
            let latest = rows.iter().find(|row| row.key != first_row.key);
            if let (Some(first), Some(latest)) = (first, latest)
                && first.lifecycle
                    == crate::activity::ActivityLifecycle::Terminal(ActivityOutcome::Superseded)
                && first.retirement == ActivityRetirement::Confirmed
                && latest.lifecycle
                    == crate::activity::ActivityLifecycle::Terminal(ActivityOutcome::Completed)
                && latest.retirement == ActivityRetirement::Confirmed
            {
                break (first.clone(), latest.clone());
            }
            assert!(
                Instant::now() < deadline,
                "Folder Size activity tickets did not retire"
            );
            thread::yield_now();
        };
        assert_eq!(
            first_terminal.diagnostic,
            Some(ActivityDiagnostic::Superseded)
        );
        assert_eq!(latest_row.key.owner(), ActivityOwner::FolderSize);
        assert_ne!(latest_row.key.operation_id(), first_row.key.operation_id());
    }

    #[test]
    fn folder_size_activity_cancel_is_generation_fenced_and_drop_retires_worker() {
        let root = crate::folder_size::disposable_scan_fixture_root();
        let (progress_tx, progress_rx) = mpsc::channel();
        let (completed_tx, _completed_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let job = Arc::new(HoldAfterProgressJob {
            hold_generation: 1,
            progress_generations: progress_tx,
            completed_runs: completed_tx,
            release: Mutex::new(release_rx),
        });
        let center = ActivityCenter::new();
        let session = FolderSizeSession::with_job_wake_and_activity(
            job,
            Arc::new(|| {}),
            Some(center.clone()),
        );
        let generation = session.refresh(explicit_root(root.path()), true).unwrap();
        assert_eq!(
            progress_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            generation
        );
        let key = center
            .snapshot()
            .rows
            .into_iter()
            .find(|row| row.key.owner() == ActivityOwner::FolderSize)
            .unwrap()
            .key;

        assert_eq!(
            center.request_cancel(key),
            crate::activity::ActivityCancelResult::Requested
        );
        let cancelled_generation = session.view().generation();
        assert!(cancelled_generation > generation);
        cancel_activity_generation(Arc::downgrade(&session.shared), generation);
        assert_eq!(session.view().generation(), cancelled_generation);
        assert_eq!(
            center.request_cancel(key),
            crate::activity::ActivityCancelResult::Unsupported
        );

        drop(session);
        let pending_retirement = center.snapshot().rows[0].retirement;
        assert_eq!(pending_retirement, ActivityRetirement::Pending);
        release_tx.send(()).unwrap();
        let terminal = wait_for_activity_row(&center, key, |row| {
            row.lifecycle.is_terminal() && row.retirement == ActivityRetirement::Confirmed
        });
        assert_eq!(
            terminal.lifecycle,
            crate::activity::ActivityLifecycle::Terminal(ActivityOutcome::Cancelled)
        );
    }

    #[test]
    fn folder_size_session_settled_early_wake_starts_latest_refresh_without_overlap() {
        let first_root = crate::folder_size::disposable_scan_fixture_root();
        let latest_root = crate::folder_size::disposable_scan_fixture_root();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_first_tx, release_first_rx) = mpsc::channel();
        let (epilogue_entered_tx, epilogue_entered_rx) = mpsc::channel();
        let (release_epilogue_tx, release_epilogue_rx) = mpsc::channel();
        let release_epilogue_rx = Mutex::new(release_epilogue_rx);
        let block_final_wake = Arc::new(AtomicBool::new(false));
        let active = Arc::new(AtomicUsize::new(0));
        let maximum_active = Arc::new(AtomicUsize::new(0));
        let job = Arc::new(EarlyWakeJob {
            started: started_tx,
            release_first: Mutex::new(release_first_rx),
            block_final_wake: Arc::clone(&block_final_wake),
            active: Arc::clone(&active),
            maximum_active: Arc::clone(&maximum_active),
        });
        let wake: Arc<dyn Fn() + Send + Sync> = {
            let block_final_wake = Arc::clone(&block_final_wake);
            Arc::new(move || {
                if thread::current().name() == Some("folder-size-scan")
                    && block_final_wake.swap(false, Ordering::AcqRel)
                {
                    let _ = epilogue_entered_tx.send(());
                    let _ = release_epilogue_rx.lock().unwrap().recv();
                }
            })
        };
        let session = FolderSizeSession::with_job_and_wake(job, wake);

        let first_generation = session
            .refresh(explicit_root(first_root.path()), true)
            .unwrap();
        assert_eq!(
            started_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            first_generation
        );
        let latest_generation = session
            .refresh(explicit_root(latest_root.path()), true)
            .unwrap();
        assert!(latest_generation > first_generation);
        release_first_tx.send(()).unwrap();

        epilogue_entered_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        // The scan has returned and cleared `active`, but its thread is still
        // in the final wake callback. Polling transfers its handle to the
        // reaper and must start the queued scan instead of relying on a second
        // (nonexistent) worker wake.
        session.poll().unwrap();
        assert_eq!(
            started_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            latest_generation
        );
        release_epilogue_tx.send(()).unwrap();

        let latest = wait_for_view(&session, |view| {
            view.status() == FolderSizeSessionStatus::Complete && view.result_is_current()
        });
        assert_eq!(latest.result().unwrap().generation(), latest_generation);
        assert_eq!(latest.result().unwrap().root(), latest_root.path());
        assert_eq!(maximum_active.load(Ordering::Acquire), 1);
    }

    #[test]
    fn folder_size_session_refresh_clears_old_result_and_cancel_freezes_progress() {
        let first_root = crate::folder_size::disposable_scan_fixture_root();
        let second_root = crate::folder_size::disposable_scan_fixture_root();
        std::fs::write(second_root.path().join("payload.bin"), b"verified bytes").unwrap();
        let (progress_tx, progress_rx) = mpsc::channel();
        let (completed_tx, completed_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let job = Arc::new(HoldAfterProgressJob {
            hold_generation: 2,
            progress_generations: progress_tx,
            completed_runs: completed_tx,
            release: Mutex::new(release_rx),
        });
        let session = FolderSizeSession::with_job_and_wake(job, Arc::new(|| {}));

        let first_generation = session
            .refresh(explicit_root(first_root.path()), true)
            .unwrap();
        let first = wait_for_view(&session, |view| {
            view.status() == FolderSizeSessionStatus::Complete && view.result_is_current()
        });
        assert_eq!(first.result().unwrap().generation(), first_generation);
        assert_eq!(
            completed_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            first_generation
        );

        let second_generation = session
            .refresh(explicit_root(second_root.path()), true)
            .unwrap();
        let in_flight = session.view();
        assert!(in_flight.result().is_none());
        assert_eq!(in_flight.status(), FolderSizeSessionStatus::Scanning);
        loop {
            if progress_rx.recv_timeout(Duration::from_secs(5)).unwrap() == second_generation {
                break;
            }
        }
        let scanning = session.view();
        let progress = scanning.progress().unwrap();
        assert_eq!(progress.generation(), second_generation);
        assert!(scanning.progress_is_current());
        assert_eq!(progress.total_logical_bytes(), 14);

        assert!(session.cancel().unwrap());
        let cancelled = session.view();
        assert_eq!(cancelled.status(), FolderSizeSessionStatus::Cancelled);
        assert!(cancelled.is_busy());
        assert!(!cancelled.progress_is_current());
        assert_eq!(
            cancelled.progress().unwrap().generation(),
            second_generation
        );
        assert_eq!(
            cancelled.progress().unwrap().total_logical_bytes(),
            progress.total_logical_bytes()
        );

        release_tx.send(()).unwrap();
        assert_eq!(
            completed_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            second_generation
        );
        let settled = wait_for_view(&session, |view| {
            view.status() == FolderSizeSessionStatus::Cancelled && !view.is_busy()
        });
        assert!(settled.result().is_none());
        assert_eq!(settled.progress().unwrap().generation(), second_generation);
    }

    #[test]
    fn folder_size_session_rejects_mismatched_progress_and_result_generations() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("payload.bin"), b"bytes").unwrap();
        let session = FolderSizeSession::with_job_and_wake(
            Arc::new(MismatchedGenerationJob),
            Arc::new(|| {}),
        );

        let generation = session.refresh(explicit_root(root.path()), true).unwrap();
        let failed = wait_for_view(&session, |view| {
            view.status() == FolderSizeSessionStatus::Failed && !view.is_busy()
        });
        assert_eq!(failed.generation(), generation);
        assert!(failed.progress().is_none());
        assert!(!failed.progress_is_current());
        assert!(failed.result().is_none());
        assert_eq!(
            failed.error(),
            Some(&FolderSizeSessionError::InvalidWorkerResult)
        );
    }
}
