use super::discovery::{
    DiscoveryJob, DiscoveryLimits, MAX_ROOT_SELECTION_PATH_BYTES, NativeDiscoveryJob,
};
use super::model::{
    ContentProgress, DiscoveryDiagnostic, DiscoveryProgress, DiscoverySnapshot, DiscoveryStage,
    DiscoveryStatus, DuplicateTotals, RootScan, RootScanStatus, ScanCounts,
};
use crate::file_operations::ScanCancellation;
use crate::thread_reaper::{self, ReapPermit};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
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
}

struct ActiveScan {
    generation: u64,
    cancellation: ScanCancellation,
    restart_blocked: bool,
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
    worker: Mutex<Option<WorkerOwner>>,
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
            Arc::new(NativeDiscoveryJob),
            Arc::new(|| {}),
        )
    }

    pub(crate) fn with_wake(wake: Arc<dyn Fn() + Send + Sync + 'static>) -> Self {
        Self::with_job_and_wake(
            DiscoveryLimits::default(),
            Arc::new(NativeDiscoveryJob),
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
            #[cfg(test)]
            test_reap_permit_reserver: None,
        }
    }

    /// Explicitly begin a new scan. Root-count and root-path byte limits are
    /// checked before the session retains a second copy; filesystem canonical
    /// identity and overlap admission remain on the worker.
    pub(crate) fn start_scan(
        &self,
        selected_roots: Vec<PathBuf>,
    ) -> Result<u64, DuplicateFinderSessionError> {
        let invalid_request = root_list_error(&selected_roots, self.limits);
        let generation = {
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
            }
            state.pending = None;
            let snapshot = if let Some(message) = invalid_request.as_ref() {
                Arc::new(failed_snapshot(generation, message.clone()))
            } else {
                state.pending = Some(ScanRequest {
                    generation,
                    roots: selected_roots.clone(),
                });
                Arc::new(DiscoverySnapshot::scanning(generation, selected_roots))
            };
            state.progress = snapshot.progress.clone();
            state.snapshot = snapshot;
            self.shared.changed.notify_one();
            generation
        };

        if invalid_request.is_some() {
            (self.shared.wake)();
            return Ok(generation);
        }

        if let Err(error) = self.ensure_worker() {
            let mut state = self.shared.state.lock().unwrap();
            if state.generation == generation && !state.closed {
                state.pending = None;
                state.snapshot = Arc::new(failed_snapshot(generation, error.to_string()));
                state.progress = None;
            }
            drop(state);
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
        let cancelled = {
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
            state.pending = None;
            if let Some(active) = state.active.as_mut() {
                active.restart_blocked = true;
                active.cancellation.cancel();
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
            true
        };
        (self.shared.wake)();
        Ok(cancelled)
    }

    /// Close invalidates all further publication but does not join the worker.
    /// `reopen` may reuse this owner only after the retained handle is finished.
    pub(crate) fn close(&self) {
        let _worker = self.worker.lock().unwrap();
        let mut state = self.shared.state.lock().unwrap();
        if !state.closed {
            state.closed = true;
            state.generation = state.generation.saturating_add(1);
            state.pending = None;
            if let Some(active) = state.active.as_ref() {
                active.cancellation.cancel();
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
        drop(_worker);
        (self.shared.wake)();
    }

    /// Reopen the same session after its old worker has actually terminated.
    /// A blocked filesystem call keeps the handle in this owner and prevents a
    /// second worker from receiving a fresh scan allowance.
    pub(crate) fn reopen(&self) -> Result<(), DuplicateFinderSessionError> {
        let mut worker_slot = self.worker.lock().unwrap();
        let mut state = self.shared.state.lock().unwrap();
        if !state.closed {
            return Ok(());
        }
        if worker_slot
            .as_ref()
            .is_some_and(|worker| !worker.join.is_finished())
        {
            return Err(DuplicateFinderSessionError::WorkerRetiring);
        }
        if let Some(worker) = worker_slot.take() {
            let WorkerOwner { join, reap_permit } = worker;
            if join.join().is_err() {
                drop(reap_permit);
                state.active = None;
                state.pending = None;
                return Err(DuplicateFinderSessionError::WorkerPanicked);
            }
            drop(reap_permit);
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
        if worker_slot
            .as_ref()
            .is_some_and(|worker| !worker.join.is_finished())
        {
            return Ok(());
        }
        if let Some(worker) = worker_slot.take() {
            let WorkerOwner { join, reap_permit } = worker;
            let panicked = join.join().is_err();
            drop(reap_permit);
            if panicked {
                let mut state = self.shared.state.lock().unwrap();
                state.active = None;
                if let Some(pending) = state.pending.take()
                    && !state.closed
                    && state.generation == pending.generation
                {
                    state.snapshot = Arc::new(failed_snapshot(
                        pending.generation,
                        "the prior duplicate discovery worker panicked before starting the queued scan".into(),
                    ));
                    state.progress = None;
                }
                return Err(DuplicateFinderSessionError::WorkerPanicked);
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
        let limits = self.limits;
        let join = thread::Builder::new()
            .name("duplicate-discovery-session".into())
            .spawn(move || {
                let completion = completion;
                worker_loop(shared, job, limits);
                drop(completion);
            })
            .map_err(|error| DuplicateFinderSessionError::WorkerStart(error.to_string()))?;
        *worker_slot = Some(WorkerOwner { join, reap_permit });
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
        self.worker
            .lock()
            .unwrap()
            .as_ref()
            .map(|worker| worker.reap_permit.reaped_probe())
    }
}

impl Drop for DuplicateFinderSession {
    fn drop(&mut self) {
        let worker_slot = self.worker.get_mut().unwrap();
        {
            let mut state = self.shared.state.lock().unwrap();
            state.closed = true;
            state.generation = state.generation.saturating_add(1);
            state.pending = None;
            if let Some(active) = state.active.as_ref() {
                active.cancellation.cancel();
            }
            self.shared.changed.notify_all();
        }
        if let Some(WorkerOwner { join, reap_permit }) = worker_slot.take() {
            if let Err(error) = reap_permit.reap(join) {
                tracing::error!(%error, "failed to transfer duplicate discovery worker to join reaper");
            }
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
                    return;
                }
                if let Some(request) = state.pending.take() {
                    let cancellation = ScanCancellation::new();
                    state.active = Some(ActiveScan {
                        generation: request.generation,
                        cancellation: cancellation.clone(),
                        restart_blocked: false,
                    });
                    break (request, cancellation);
                }
                state = shared.changed.wait(state).unwrap();
            }
        };

        let generation = request.generation;
        let progress_shared = Arc::clone(&shared);
        let mut publish = |progress: DiscoveryProgress| {
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
        let current = {
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
            current
        };
        if current {
            (shared.wake)();
        }
    }
}
