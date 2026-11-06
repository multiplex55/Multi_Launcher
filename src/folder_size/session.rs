use crate::batch_rename::ExplicitDirectoryRoot;
use crate::file_operations::ScanCancellation;
use crate::thread_reaper::{self, ReapPermit};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};
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
}

struct WorkerOwner {
    join: JoinHandle<()>,
    reap_permit: ReapPermit,
    generation: u64,
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
        Self::with_job_and_wake(Arc::new(NativeFolderSizeScanJob), Arc::new(wake))
    }

    fn with_job_and_wake(
        job: Arc<dyn FolderSizeScanJob>,
        wake: Arc<dyn Fn() + Send + Sync + 'static>,
    ) -> Self {
        Self {
            shared: Arc::new(SessionShared {
                state: Mutex::new(SessionState::default()),
                wake,
                job,
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
        let (generation, replaced_pending) = {
            let mut state = self.shared.state.lock().unwrap();
            if state.closed {
                return Err(FolderSizeSessionError::Closed);
            }
            let generation = state
                .generation
                .checked_add(1)
                .ok_or(FolderSizeSessionError::GenerationExhausted)?;
            state.generation = generation;
            if let Some(active) = state.active.as_ref() {
                active.cancellation.cancel();
            }
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
            (generation, replaced_pending)
        };
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
        let (cancelled, pending) = {
            let mut state = self.shared.state.lock().unwrap();
            if state.closed {
                return Err(FolderSizeSessionError::Closed);
            }
            if state.active.is_none() && state.pending.is_none() {
                return Ok(false);
            }
            let generation = state
                .generation
                .checked_add(1)
                .ok_or(FolderSizeSessionError::GenerationExhausted)?;
            state.generation = generation;
            let pending = state.pending.take();
            if let Some(active) = state.active.as_ref() {
                active.cancellation.cancel();
            }
            state.cancelled = true;
            state.error = None;
            (true, pending)
        };
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
        let mut state = self.shared.state.lock().unwrap();
        if state.closed {
            return;
        }
        state.closed = true;
        state.generation = state.generation.saturating_add(1);
        if let Some(active) = state.active.as_ref() {
            active.cancellation.cancel();
        }
        if let Some(pending) = state.pending.take() {
            pending.cancellation.cancel();
        }
        state.accepted_result = None;
        state.progress = None;
        state.error = None;
        state.cancelled = false;
        drop(state);
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
            let transfer = worker
                .reap_permit
                .reap_with_callback(worker.join, move |_| wake());
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
            });
            request
        };
        let shared = Arc::clone(&self.shared);
        let job = Arc::clone(&self.shared.job);
        let wake = Arc::clone(&self.shared.wake);
        let generation = request.generation;
        let join = thread::Builder::new()
            .name("folder-size-scan".into())
            .spawn(move || {
                let completion = completion;
                let run = catch_unwind(AssertUnwindSafe(|| {
                    let mut publish_progress = |progress: FolderSizeProgress| {
                        let mut state = shared.state.lock().unwrap();
                        if progress.generation() == generation
                            && !state.closed
                            && state.generation == generation
                            && state.active.as_ref().is_some_and(|active| {
                                active.generation == generation
                                    && !active.cancellation.is_cancelled()
                            })
                        {
                            state.progress = Some(progress);
                        }
                        drop(state);
                        (shared.wake)();
                    };
                    job.run(request, &mut publish_progress)
                }));
                let mut state = shared.state.lock().unwrap();
                let active_matches = state
                    .active
                    .as_ref()
                    .is_some_and(|active| active.generation == generation);
                if active_matches {
                    if state.generation == generation && !state.closed {
                        match run {
                            Ok(result) if result.generation() == generation => {
                                state.accepted_result = Some(Arc::new(result));
                                state.progress = None;
                                state.error = None;
                            }
                            Ok(_) => {
                                state.error = Some(FolderSizeSessionError::InvalidWorkerResult);
                            }
                            Err(_) => {
                                state.error = Some(FolderSizeSessionError::WorkerPanicked);
                            }
                        }
                    }
                    state.active = None;
                }
                drop(state);
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
                    {
                        if let Some(pending) = state.pending.take() {
                            pending.cancellation.cancel();
                        }
                    }
                    state.error = Some(failure.clone());
                }
                failure
            })?;
        *worker_slot = Some(WorkerOwner {
            join,
            reap_permit,
            generation,
        });
        Ok(())
    }
}

impl Drop for FolderSizeSession {
    fn drop(&mut self) {
        {
            let mut state = self.shared.state.lock().unwrap();
            if !state.closed {
                state.closed = true;
                state.generation = state.generation.saturating_add(1);
            }
            if let Some(active) = state.active.as_ref() {
                active.cancellation.cancel();
            }
            if let Some(pending) = state.pending.take() {
                pending.cancellation.cancel();
            }
            state.accepted_result = None;
            state.progress = None;
        }
        if let Some(worker) = self.worker.get_mut().unwrap().take()
            && let Err(error) = worker.reap_permit.reap(worker.join)
        {
            tracing::error!(%error, "failed to transfer folder-size worker to join reaper");
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
