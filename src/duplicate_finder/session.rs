use super::discovery::{
    DiscoveryJob, DiscoveryLimits, MAX_ROOT_SELECTION_PATH_BYTES, NativeDiscoveryJob,
};
use super::model::{
    ContentProgress, DiscoveryDiagnostic, DiscoveryProgress, DiscoverySnapshot, DiscoveryStage,
    DiscoveryStatus, DuplicateTotals, RootScan, RootScanStatus, ScanCounts,
};
use crate::activity::{
    ActivityCenter, ActivityDiagnostic, ActivityIncarnation, ActivityOperation, ActivityOutcome,
    ActivityOwner, ActivityProgress, ActivityProgressUnit, ActivityRetirement, ActivitySlot,
    ActivityTask,
};
use crate::file_operations::ScanCancellation;
use crate::thread_reaper::{self, ReapPermit};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DuplicateFinderSessionError {
    Closed,
    WorkerRetiring,
    GenerationExhausted,
    WorkerCapacity(String),
    WorkerStart(String),
    WorkerPanicked,
}

impl std::fmt::Display for DuplicateFinderSessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => write!(formatter, "duplicate discovery session is closed"),
            Self::WorkerRetiring => {
                write!(
                    formatter,
                    "the previous duplicate discovery worker is still retiring"
                )
            }
            Self::GenerationExhausted => {
                write!(
                    formatter,
                    "duplicate discovery generation space is exhausted"
                )
            }
            Self::WorkerCapacity(message) | Self::WorkerStart(message) => {
                formatter.write_str(message)
            }
            Self::WorkerPanicked => write!(formatter, "duplicate discovery worker panicked"),
        }
    }
}

impl std::error::Error for DuplicateFinderSessionError {}

/// A cheap read of one session publication. `snapshot` is immutable and shared;
/// high-frequency progress is separately coalesced so the worker never clones
/// the accumulated result while walking a tree.
pub(crate) struct DuplicateFinderSessionView {
    pub(crate) snapshot: Arc<DiscoverySnapshot>,
    pub(crate) progress: Option<DiscoveryProgress>,
    pub(crate) busy: bool,
    pub(crate) closed: bool,
}

struct ScanRequest {
    generation: u64,
    roots: Vec<PathBuf>,
    cancellation: ScanCancellation,
    activity: Option<ActivityTask>,
}

struct ActiveScan {
    generation: u64,
    cancellation: ScanCancellation,
    restart_blocked: bool,
    activity: Option<ActivityTask>,
}

struct SessionState {
    closed: bool,
    generation: u64,
    pending: Option<ScanRequest>,
    active: Option<ActiveScan>,
    snapshot: Arc<DiscoverySnapshot>,
    progress: Option<DiscoveryProgress>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            closed: false,
            generation: 0,
            pending: None,
            active: None,
            snapshot: Arc::new(DiscoverySnapshot::idle(0)),
            progress: None,
        }
    }
}

struct SessionShared {
    state: Mutex<SessionState>,
    changed: Condvar,
    wake: Arc<dyn Fn() + Send + Sync + 'static>,
}

struct WorkerOwner {
    join: JoinHandle<()>,
    reap_permit: ReapPermit,
}

enum WorkerSlot {
    Running(WorkerOwner),
    Retiring(Arc<AtomicU8>),
}

const WORKER_RETIREMENT_PENDING: u8 = 0;
const WORKER_RETIREMENT_JOINED: u8 = 1;
const WORKER_RETIREMENT_PANICKED: u8 = 2;
const WORKER_RETIREMENT_REAPER_FAILED: u8 = 3;

#[cfg(test)]
pub(super) type TestReapPermitReserver =
    Arc<dyn Fn() -> Result<ReapPermit, &'static str> + Send + Sync + 'static>;

/// One lazily started worker and one coalesced pending request. Close retains
/// the active join handle until it has actually finished; dropping the session
/// transfers that handle and its pre-reserved reaper slot without blocking.
pub(crate) struct DuplicateFinderSession {
    limits: DiscoveryLimits,
    job: Arc<dyn DiscoveryJob>,
    shared: Arc<SessionShared>,
    worker: Mutex<Option<WorkerSlot>>,
    activity_center: Option<ActivityCenter>,
    #[cfg(test)]
    test_reap_permit_reserver: Option<TestReapPermitReserver>,
}

impl Default for DuplicateFinderSession {
    fn default() -> Self {
        Self::new()
    }
}

impl DuplicateFinderSession {
    pub(crate) fn new() -> Self {
        Self::with_job_and_wake(
            DiscoveryLimits::default(),
            Arc::new(NativeDiscoveryJob::default()),
            Arc::new(|| {}),
        )
    }

    pub(crate) fn with_wake(wake: Arc<dyn Fn() + Send + Sync + 'static>) -> Self {
        Self::with_job_and_wake(
            DiscoveryLimits::default(),
            Arc::new(NativeDiscoveryJob::default()),
            wake,
        )
    }

    #[cfg(any(test, feature = "dev-smoke-binaries"))]
    pub(crate) fn with_wake_and_first_read_checkpoint(
        wake: Arc<dyn Fn() + Send + Sync + 'static>,
        checkpoint: Arc<super::content::FirstReadCheckpointGate>,
    ) -> Self {
        Self::with_job_and_wake(
            DiscoveryLimits::default(),
            Arc::new(NativeDiscoveryJob::with_first_read_checkpoint(checkpoint)),
            wake,
        )
    }

    fn with_job_and_wake(
        limits: DiscoveryLimits,
        job: Arc<dyn DiscoveryJob>,
        wake: Arc<dyn Fn() + Send + Sync + 'static>,
    ) -> Self {
        Self {
            limits,
            job,
            shared: Arc::new(SessionShared {
                state: Mutex::new(SessionState::default()),
                changed: Condvar::new(),
                wake,
            }),
            worker: Mutex::new(None),
            activity_center: None,
            #[cfg(test)]
            test_reap_permit_reserver: None,
        }
    }

    pub(crate) fn attach_activity_center(&mut self, center: ActivityCenter) {
        self.activity_center = Some(center);
    }

    /// Explicitly begin a new scan. Root-count and root-path byte limits are
    /// checked before the session retains a second copy; filesystem canonical
    /// identity and overlap admission remain on the worker.
    pub(crate) fn start_scan(
        &self,
        selected_roots: Vec<PathBuf>,
    ) -> Result<u64, DuplicateFinderSessionError> {
        let invalid_request = root_list_error(&selected_roots, self.limits);
        let (generation, replaced_pending) = {
            let mut state = self.shared.state.lock().unwrap();
            if state.closed {
                return Err(DuplicateFinderSessionError::Closed);
            }
            if state
                .active
                .as_ref()
                .is_some_and(|active| active.restart_blocked)
            {
                return Err(DuplicateFinderSessionError::WorkerRetiring);
            }
            let generation = state
                .generation
                .checked_add(1)
                .ok_or(DuplicateFinderSessionError::GenerationExhausted)?;
            state.generation = generation;
            if let Some(active) = state.active.as_ref() {
                active.cancellation.cancel();
                if let Some(activity) = active.activity.as_ref() {
                    activity.mark_cancelling();
                }
            }
            let replaced_pending = state.pending.take();
            let snapshot = if let Some(message) = invalid_request.as_ref() {
                Arc::new(failed_snapshot(generation, message.clone()))
            } else {
                let cancellation = ScanCancellation::new();
                let activity = self.activity_center.as_ref().and_then(|center| {
                    center.start(
                        ActivityOwner::DuplicateFinder,
                        ActivitySlot::Primary,
                        ActivityIncarnation::new(generation),
                        ActivityOperation::FindDuplicates,
                    )
                });
                if let Some(activity) = activity.as_ref() {
                    let cancel = cancellation.clone();
                    activity.set_cancel_handler(move || cancel.cancel());
                }
                state.pending = Some(ScanRequest {
                    generation,
                    roots: selected_roots.clone(),
                    cancellation,
                    activity,
                });
                Arc::new(DiscoverySnapshot::scanning(generation, selected_roots))
            };
            state.progress = snapshot.progress.clone();
            state.snapshot = snapshot;
            self.shared.changed.notify_one();
            (generation, replaced_pending)
        };

        if let Some(replaced) = replaced_pending {
            replaced.cancellation.cancel();
            if let Some(activity) = replaced.activity {
                activity.finish(
                    ActivityOutcome::Cancelled,
                    None,
                    ActivityRetirement::Confirmed,
                );
            }
        }

        if invalid_request.is_some() {
            (self.shared.wake)();
            return Ok(generation);
        }

        if let Err(error) = self.ensure_worker() {
            let mut state = self.shared.state.lock().unwrap();
            let mut failed_activity = None;
            if state.generation == generation && !state.closed {
                failed_activity = state.pending.take().and_then(|pending| pending.activity);
                state.snapshot = Arc::new(failed_snapshot(generation, error.to_string()));
                state.progress = None;
            }
            drop(state);
            if let Some(activity) = failed_activity {
                activity.finish(
                    ActivityOutcome::Failed,
                    Some(ActivityDiagnostic::WorkerUnavailable),
                    ActivityRetirement::Confirmed,
                );
            }
            (self.shared.wake)();
            return Err(error);
        }

        (self.shared.wake)();
        Ok(generation)
    }

    /// Cancel only the currently published scan. Publication is invalidated
    /// synchronously; a native call already in flight keeps its single worker
    /// slot until the scanner reaches a cancellation checkpoint.
    pub(crate) fn cancel(&self) -> Result<bool, DuplicateFinderSessionError> {
        let (cancelled, pending) = {
            let mut state = self.shared.state.lock().unwrap();
            if state.closed {
                return Err(DuplicateFinderSessionError::Closed);
            }
            if state.pending.is_none() && state.active.is_none() {
                return Ok(false);
            }
            let Some(generation) = state.generation.checked_add(1) else {
                return Err(DuplicateFinderSessionError::GenerationExhausted);
            };
            state.generation = generation;
            let pending = state.pending.take();
            if let Some(active) = state.active.as_mut() {
                active.restart_blocked = true;
                active.cancellation.cancel();
                if let Some(activity) = active.activity.as_ref() {
                    activity.mark_cancelling();
                }
            }
            let mut snapshot = terminal_snapshot_from_current(
                &state.snapshot,
                generation,
                DiscoveryStatus::Cancelled,
            );
            if let Some(progress) = state.progress.as_mut() {
                progress.generation = generation;
                progress.current_root = None;
                snapshot.counts = progress.counts;
                snapshot.progress = Some(progress.clone());
            }
            state.snapshot = Arc::new(snapshot);
            if let Some(progress) = state.progress.as_mut() {
                progress.current_root = None;
            }
            self.shared.changed.notify_one();
            (true, pending)
        };
        if let Some(pending) = pending {
            pending.cancellation.cancel();
            if let Some(activity) = pending.activity {
                activity.finish(
                    ActivityOutcome::Cancelled,
                    None,
                    ActivityRetirement::Confirmed,
                );
            }
        }
        (self.shared.wake)();
        Ok(cancelled)
    }

    /// Close invalidates publication and transfers any retained handle to the
    /// bounded reaper. `reopen` remains busy until its post-join receipt arrives.
    pub(crate) fn close(&self) {
        let mut worker_slot = self.worker.lock().unwrap();
        let mut state = self.shared.state.lock().unwrap();
        let mut pending = None;
        if !state.closed {
            state.closed = true;
            state.generation = state.generation.saturating_add(1);
            pending = state.pending.take();
            if let Some(active) = state.active.as_ref() {
                active.cancellation.cancel();
                if let Some(activity) = active.activity.as_ref() {
                    activity.mark_cancelling();
                }
            }
            state.snapshot = Arc::new(DiscoverySnapshot {
                generation: state.generation,
                status: DiscoveryStatus::Closed,
                progress: None,
                roots: Vec::new(),
                size_groups: Vec::new(),
                verified_duplicate_groups: Vec::new(),
                identity_unverified_matches: Vec::new(),
                duplicate_totals: DuplicateTotals::default(),
                diagnostics: Vec::new(),
                diagnostics_truncated: false,
                counts: ScanCounts::default(),
            });
            state.progress = None;
            self.shared.changed.notify_all();
        }
        drop(state);
        if let Some(pending) = pending {
            pending.cancellation.cancel();
            if let Some(activity) = pending.activity {
                activity.finish(
                    ActivityOutcome::Cancelled,
                    None,
                    ActivityRetirement::Confirmed,
                );
            }
        }
        let running_worker = match worker_slot.take() {
            Some(WorkerSlot::Running(worker)) => Some(worker),
            Some(retiring) => {
                *worker_slot = Some(retiring);
                None
            }
            None => None,
        };
        if let Some(WorkerOwner { join, reap_permit }) = running_worker {
            let joined = Arc::new(AtomicU8::new(WORKER_RETIREMENT_PENDING));
            let retirement = Arc::clone(&joined);
            let wake = Arc::clone(&self.shared.wake);
            if let Err(error) = reap_permit.reap_with_callback(join, move |panicked| {
                retirement.store(
                    if panicked {
                        WORKER_RETIREMENT_PANICKED
                    } else {
                        WORKER_RETIREMENT_JOINED
                    },
                    Ordering::Release,
                );
                wake();
            }) {
                joined.store(WORKER_RETIREMENT_REAPER_FAILED, Ordering::Release);
                tracing::error!(%error, "failed to transfer closed duplicate discovery worker to join reaper");
            }
            *worker_slot = Some(WorkerSlot::Retiring(joined));
        }
        drop(worker_slot);
        (self.shared.wake)();
    }

    /// Reopen the same session after the bounded reaper has joined its old
    /// worker. A blocked filesystem call retains the retirement receipt and
    /// prevents a second worker from receiving a fresh scan allowance.
    pub(crate) fn reopen(&self) -> Result<(), DuplicateFinderSessionError> {
        let mut worker_slot = self.worker.lock().unwrap();
        let mut state = self.shared.state.lock().unwrap();
        if !state.closed {
            return Ok(());
        }
        let retirement_state = match worker_slot.as_ref() {
            Some(WorkerSlot::Retiring(joined)) => Some(joined.load(Ordering::Acquire)),
            _ => None,
        };
        if worker_slot.as_ref().is_some_and(
            |slot| matches!(slot, WorkerSlot::Running(worker) if !worker.join.is_finished()),
        ) {
            return Err(DuplicateFinderSessionError::WorkerRetiring);
        }
        if retirement_state == Some(WORKER_RETIREMENT_PENDING) {
            return Err(DuplicateFinderSessionError::WorkerRetiring);
        }
        if retirement_state == Some(WORKER_RETIREMENT_PANICKED) {
            worker_slot.take();
            state.active = None;
            state.pending = None;
            return Err(DuplicateFinderSessionError::WorkerPanicked);
        }
        if retirement_state == Some(WORKER_RETIREMENT_REAPER_FAILED) {
            return Err(DuplicateFinderSessionError::WorkerCapacity(
                "the prior duplicate discovery worker could not be safely transferred to the join reaper".into(),
            ));
        }
        if let Some(worker) = worker_slot.take() {
            match worker {
                WorkerSlot::Running(WorkerOwner { join, reap_permit }) => {
                    if join.join().is_err() {
                        drop(reap_permit);
                        state.active = None;
                        state.pending = None;
                        return Err(DuplicateFinderSessionError::WorkerPanicked);
                    }
                    drop(reap_permit);
                }
                WorkerSlot::Retiring(_) => {}
            }
        }

        if state.generation == u64::MAX {
            return Err(DuplicateFinderSessionError::GenerationExhausted);
        }
        state.closed = false;
        state.pending = None;
        state.active = None;
        state.snapshot = Arc::new(DiscoverySnapshot::idle(state.generation));
        state.progress = None;
        self.shared.changed.notify_all();
        drop(state);
        drop(worker_slot);
        (self.shared.wake)();
        Ok(())
    }

    pub(crate) fn view(&self) -> DuplicateFinderSessionView {
        let state = self.shared.state.lock().unwrap();
        DuplicateFinderSessionView {
            snapshot: Arc::clone(&state.snapshot),
            progress: state.progress.clone(),
            busy: state.active.is_some() || state.pending.is_some(),
            closed: state.closed,
        }
    }

    pub(crate) fn is_busy(&self) -> bool {
        let state = self.shared.state.lock().unwrap();
        state.active.is_some() || state.pending.is_some()
    }

    fn ensure_worker(&self) -> Result<(), DuplicateFinderSessionError> {
        let mut worker_slot = self.worker.lock().unwrap();
        match worker_slot.as_ref() {
            Some(WorkerSlot::Running(worker)) if !worker.join.is_finished() => return Ok(()),
            Some(WorkerSlot::Retiring(joined))
                if joined.load(Ordering::Acquire) == WORKER_RETIREMENT_PENDING =>
            {
                return Err(DuplicateFinderSessionError::WorkerRetiring);
            }
            _ => {}
        }
        if let Some(worker) = worker_slot.take() {
            match worker {
                WorkerSlot::Running(WorkerOwner { join, reap_permit }) => {
                    let panicked = join.join().is_err();
                    drop(reap_permit);
                    if panicked {
                        let mut state = self.shared.state.lock().unwrap();
                        state.active = None;
                        let pending = state.pending.take();
                        let failed_activity = pending
                            .as_ref()
                            .filter(|pending| {
                                !state.closed && state.generation == pending.generation
                            })
                            .and_then(|pending| pending.activity.clone());
                        if let Some(pending) = pending.filter(|pending| {
                            !state.closed && state.generation == pending.generation
                        }) {
                            state.snapshot = Arc::new(failed_snapshot(
                                pending.generation,
                                "the prior duplicate discovery worker panicked before starting the queued scan".into(),
                            ));
                            state.progress = None;
                        }
                        drop(state);
                        if let Some(activity) = failed_activity {
                            activity.finish(
                                ActivityOutcome::Failed,
                                Some(ActivityDiagnostic::WorkerFailed),
                                ActivityRetirement::Confirmed,
                            );
                        }
                        return Err(DuplicateFinderSessionError::WorkerPanicked);
                    }
                }
                WorkerSlot::Retiring(_) => {}
            }
        }
        {
            let state = self.shared.state.lock().unwrap();
            if state.closed {
                return Err(DuplicateFinderSessionError::Closed);
            }
            if state.pending.is_none() {
                return Ok(());
            }
        }

        let reap_permit = {
            #[cfg(test)]
            if let Some(reserver) = self.test_reap_permit_reserver.as_ref() {
                reserver()
            } else {
                thread_reaper::reserve()
            }
            #[cfg(not(test))]
            {
                thread_reaper::reserve()
            }
        }
        .map_err(|error| {
            DuplicateFinderSessionError::WorkerCapacity(format!(
                "duplicate discovery worker capacity is unavailable: {error}"
            ))
        })?;
        let completion = reap_permit.completion_notifier();
        let shared = Arc::clone(&self.shared);
        let job = Arc::clone(&self.job);
        let exit_wake = Arc::clone(&shared.wake);
        let limits = self.limits;
        let join = thread::Builder::new()
            .name("duplicate-discovery-session".into())
            .spawn(move || {
                let completion = completion;
                worker_loop(shared, job, limits);
                drop(completion);
                (exit_wake)();
            })
            .map_err(|error| DuplicateFinderSessionError::WorkerStart(error.to_string()))?;
        *worker_slot = Some(WorkerSlot::Running(WorkerOwner { join, reap_permit }));
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn with_test_job(limits: DiscoveryLimits, job: Arc<dyn DiscoveryJob>) -> Self {
        Self::with_job_and_wake(limits, job, Arc::new(|| {}))
    }

    #[cfg(test)]
    pub(super) fn with_test_job_and_reap_permit_reserver(
        limits: DiscoveryLimits,
        job: Arc<dyn DiscoveryJob>,
        reserver: TestReapPermitReserver,
    ) -> Self {
        let mut session = Self::with_job_and_wake(limits, job, Arc::new(|| {}));
        session.test_reap_permit_reserver = Some(reserver);
        session
    }

    #[cfg(test)]
    pub(super) fn reaper_probe_for_test(
        &self,
    ) -> Option<crate::thread_reaper::ReapCompletionProbe> {
        match self.worker.lock().unwrap().as_ref() {
            Some(WorkerSlot::Running(worker)) => Some(worker.reap_permit.reaped_probe()),
            Some(WorkerSlot::Retiring(_)) | None => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn worker_reaped_for_test(&self) -> bool {
        matches!(
            self.worker.lock().unwrap().as_ref(),
            Some(WorkerSlot::Retiring(joined))
                if joined.load(Ordering::Acquire) != WORKER_RETIREMENT_PENDING
        )
    }
}

impl Drop for DuplicateFinderSession {
    fn drop(&mut self) {
        let worker_slot = self.worker.get_mut().unwrap();
        let pending = {
            let mut state = self.shared.state.lock().unwrap();
            state.closed = true;
            state.generation = state.generation.saturating_add(1);
            let pending = state.pending.take();
            if let Some(active) = state.active.as_ref() {
                active.cancellation.cancel();
                if let Some(activity) = active.activity.as_ref() {
                    activity.mark_cancelling();
                }
            }
            self.shared.changed.notify_all();
            pending
        };
        if let Some(pending) = pending {
            pending.cancellation.cancel();
            if let Some(activity) = pending.activity {
                activity.finish(
                    ActivityOutcome::Cancelled,
                    None,
                    ActivityRetirement::Confirmed,
                );
            }
        }
        if let Some(WorkerSlot::Running(WorkerOwner { join, reap_permit })) = worker_slot.take()
            && let Err(error) = reap_permit.reap(join)
        {
            tracing::error!(%error, "failed to transfer duplicate discovery worker to join reaper");
        }
    }
}

fn root_list_error(roots: &[PathBuf], limits: DiscoveryLimits) -> Option<String> {
    if roots.is_empty() {
        return Some("select at least one directory root before scanning".into());
    }
    if roots.len() > limits.max_roots {
        return Some(format!(
            "select no more than {} roots per scan",
            limits.max_roots
        ));
    }
    let path_bytes = roots.iter().fold(0usize, |total, path| {
        total.saturating_add(path_storage_bytes(path))
    });
    if path_bytes > MAX_ROOT_SELECTION_PATH_BYTES || path_bytes > limits.max_path_bytes {
        return Some("selected root paths exceed the bounded path-list size".into());
    }
    None
}

fn path_storage_bytes(path: &Path) -> usize {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().count().saturating_mul(2)
    }
    #[cfg(not(windows))]
    {
        path.as_os_str().as_encoded_bytes().len()
    }
}

fn failed_snapshot(generation: u64, message: String) -> DiscoverySnapshot {
    DiscoverySnapshot {
        generation,
        status: DiscoveryStatus::Failed,
        progress: None,
        roots: Vec::new(),
        size_groups: Vec::new(),
        verified_duplicate_groups: Vec::new(),
        identity_unverified_matches: Vec::new(),
        duplicate_totals: DuplicateTotals::default(),
        diagnostics: vec![DiscoveryDiagnostic {
            root_index: None,
            path: None,
            message: message.chars().take(512).collect(),
        }],
        diagnostics_truncated: false,
        counts: ScanCounts::default(),
    }
}

fn terminal_snapshot_from_current(
    current: &DiscoverySnapshot,
    generation: u64,
    status: DiscoveryStatus,
) -> DiscoverySnapshot {
    let mut roots = current
        .roots
        .iter()
        .map(|root| RootScan {
            selected_path: root.selected_path.clone(),
            canonical_root: None,
            identity: None,
            status: if matches!(
                root.status,
                RootScanStatus::Pending | RootScanStatus::Scanning
            ) {
                RootScanStatus::Cancelled
            } else {
                root.status
            },
            counts: root.counts,
        })
        .collect::<Vec<_>>();
    let counts = current
        .progress
        .as_ref()
        .map_or(ScanCounts::default(), |p| p.counts);
    if status == DiscoveryStatus::Cancelled {
        for root in &mut roots {
            if root.status == RootScanStatus::Scanning {
                root.status = RootScanStatus::Cancelled;
            }
        }
    }
    DiscoverySnapshot {
        generation,
        status,
        progress: Some(DiscoveryProgress {
            generation,
            stage: DiscoveryStage::Traversing,
            roots_total: roots.len(),
            roots_preflighted: roots
                .iter()
                .filter(|root| root.status != RootScanStatus::Pending)
                .count(),
            roots_completed: 0,
            current_root: None,
            counts,
            content: Some(ContentProgress::default()),
        }),
        roots,
        size_groups: Vec::new(),
        verified_duplicate_groups: Vec::new(),
        identity_unverified_matches: Vec::new(),
        duplicate_totals: DuplicateTotals::default(),
        diagnostics: Vec::new(),
        diagnostics_truncated: false,
        counts,
    }
}

fn worker_loop(shared: Arc<SessionShared>, job: Arc<dyn DiscoveryJob>, limits: DiscoveryLimits) {
    loop {
        let (request, cancellation) = {
            let mut state = shared.state.lock().unwrap();
            loop {
                if state.closed {
                    drop(state);
                    (shared.wake)();
                    return;
                }
                if let Some(request) = state.pending.take() {
                    let cancellation = request.cancellation.clone();
                    state.active = Some(ActiveScan {
                        generation: request.generation,
                        cancellation: cancellation.clone(),
                        restart_blocked: false,
                        activity: request.activity.clone(),
                    });
                    break (request, cancellation);
                }
                state = shared.changed.wait(state).unwrap();
            }
        };

        let generation = request.generation;
        let activity = request.activity.clone();
        if let Some(activity) = activity.as_ref() {
            activity.mark_running();
        }
        let progress_shared = Arc::clone(&shared);
        let mut publish = |progress: DiscoveryProgress| {
            if let Some(activity) = activity.as_ref() {
                let progress_value = progress.content.map_or(
                    ActivityProgress::new(
                        progress.counts.files_seen as u64,
                        None,
                        ActivityProgressUnit::Files,
                    ),
                    |content| {
                        ActivityProgress::new(
                            content.files_completed as u64,
                            Some(content.files_total as u64),
                            ActivityProgressUnit::Files,
                        )
                    },
                );
                activity.set_progress(Some(progress_value));
            }
            let should_wake = {
                let mut state = progress_shared.state.lock().unwrap();
                if state.closed || state.generation != generation {
                    false
                } else {
                    state.progress = Some(progress);
                    true
                }
            };
            if should_wake {
                (progress_shared.wake)();
            }
        };
        let result = job.scan(
            request.roots,
            generation,
            limits,
            &cancellation,
            &mut publish,
        );
        if let Some(activity) = activity.as_ref() {
            let (outcome, diagnostic) = match result.status {
                DiscoveryStatus::Cancelled => (ActivityOutcome::Cancelled, None),
                DiscoveryStatus::Failed => (
                    ActivityOutcome::Failed,
                    Some(ActivityDiagnostic::WorkerFailed),
                ),
                DiscoveryStatus::Idle
                | DiscoveryStatus::Scanning
                | DiscoveryStatus::Complete
                | DiscoveryStatus::Partial
                | DiscoveryStatus::Closed => (ActivityOutcome::Completed, None),
            };
            activity.finish(outcome, diagnostic, ActivityRetirement::Unavailable);
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
            let current = !state.closed && state.generation == generation;
            if current {
                state.progress = result.progress.clone();
                state.snapshot = Arc::new(result);
            }
            shared.changed.notify_all();
        }
        // The active scan slot can retire without publishing, for example
        // after cancel or close. The GUI still needs a repaint to observe that
        // the retained worker has drained and can be reused or joined.
        (shared.wake)();
    }
}
