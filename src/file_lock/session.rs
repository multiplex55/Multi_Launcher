use super::windows::{FileLockInspection, FileLockInspectionFailure, inspect_file};
use crate::thread_reaper::{self, ReapPermit};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

const WORKER_JOINED: u8 = 1;
const WORKER_PANICKED: u8 = 2;
const WORKER_REAPER_FAILED: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileLockSessionError {
    Closed,
    GenerationExhausted,
    WorkerCapacity(String),
    WorkerStart(String),
    WorkerPanicked,
    InvalidWorkerResult,
}

impl std::fmt::Display for FileLockSessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => formatter.write_str("file-user inspection session is closed"),
            Self::GenerationExhausted => {
                formatter.write_str("file-user inspection generation space is exhausted")
            }
            Self::WorkerCapacity(message) | Self::WorkerStart(message) => {
                formatter.write_str(message)
            }
            Self::WorkerPanicked => formatter.write_str("file-user inspection worker panicked"),
            Self::InvalidWorkerResult => formatter
                .write_str("file-user inspection worker returned a result for a different file"),
        }
    }
}

impl std::error::Error for FileLockSessionError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileLockSessionStatus {
    Idle,
    Inspecting,
    RefreshQueued,
    Complete,
    Failed,
    Cancelled,
    Closed,
}

#[derive(Clone)]
struct InspectionRequest {
    generation: u64,
    path: PathBuf,
    cancelled: Arc<AtomicBool>,
}

struct ActiveInspection {
    generation: u64,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
struct SessionState {
    closed: bool,
    cancelled: bool,
    generation: u64,
    pending: Option<InspectionRequest>,
    active: Option<ActiveInspection>,
    inspection: Option<Arc<FileLockInspection>>,
    inspection_generation: Option<u64>,
    inspection_error: Option<FileLockInspectionFailure>,
    worker_error: Option<FileLockSessionError>,
}

struct SessionShared {
    state: Mutex<SessionState>,
    wake: Arc<dyn Fn() + Send + Sync + 'static>,
    job: Arc<dyn FileLockInspectionJob>,
}

struct WorkerOwner {
    join: JoinHandle<()>,
    reap_permit: ReapPermit,
    generation: u64,
}

enum WorkerSlot {
    Running(WorkerOwner),
    Retiring {
        receipt: Arc<AtomicU8>,
        generation: u64,
    },
}

struct ReaperReceipt {
    state: Arc<AtomicU8>,
    wake: Arc<dyn Fn() + Send + Sync + 'static>,
    completed: bool,
}

impl ReaperReceipt {
    fn after_join(mut self, panicked: bool) {
        self.state.store(
            if panicked {
                WORKER_PANICKED
            } else {
                WORKER_JOINED
            },
            Ordering::Release,
        );
        self.completed = true;
        (self.wake)();
    }
}

impl Drop for ReaperReceipt {
    fn drop(&mut self) {
        if !self.completed {
            self.state.store(WORKER_REAPER_FAILED, Ordering::Release);
            (self.wake)();
        }
    }
}

trait FileLockInspectionJob: Send + Sync + 'static {
    fn run(&self, path: PathBuf) -> Result<FileLockInspection, FileLockInspectionFailure>;
}

struct NativeInspectionJob;

impl FileLockInspectionJob for NativeInspectionJob {
    fn run(&self, path: PathBuf) -> Result<FileLockInspection, FileLockInspectionFailure> {
        inspect_file(&path)
    }
}

pub struct FileLockSession {
    shared: Arc<SessionShared>,
    worker: Mutex<Option<WorkerSlot>>,
}

pub struct FileLockSessionView {
    generation: u64,
    inspection: Option<Arc<FileLockInspection>>,
    inspection_generation: Option<u64>,
    inspection_error: Option<FileLockInspectionFailure>,
    worker_error: Option<FileLockSessionError>,
    status: FileLockSessionStatus,
    busy: bool,
    closed: bool,
}

impl FileLockSessionView {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn inspection(&self) -> Option<&Arc<FileLockInspection>> {
        self.inspection.as_ref()
    }

    pub fn inspection_is_current(&self) -> bool {
        self.inspection.is_some()
            && self.inspection_generation == Some(self.generation)
            && !self.closed
            && self.status == FileLockSessionStatus::Complete
    }

    pub fn inspection_error(&self) -> Option<&FileLockInspectionFailure> {
        self.inspection_error.as_ref()
    }

    pub fn worker_error(&self) -> Option<&FileLockSessionError> {
        self.worker_error.as_ref()
    }

    pub fn status(&self) -> FileLockSessionStatus {
        self.status
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }
}

impl FileLockSession {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self::with_job(Arc::new(NativeInspectionJob), Arc::new(wake))
    }

    fn with_job(
        job: Arc<dyn FileLockInspectionJob>,
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

    /// Queue a fresh inspection, replacing any not-yet-started request. An
    /// existing native call keeps the single worker slot until it returns.
    pub fn refresh(&self, path: PathBuf) -> Result<u64, FileLockSessionError> {
        let (generation, replaced) = {
            let mut state = self.shared.state.lock().unwrap();
            if state.closed {
                return Err(FileLockSessionError::Closed);
            }
            let generation = state
                .generation
                .checked_add(1)
                .ok_or(FileLockSessionError::GenerationExhausted)?;
            state.generation = generation;
            if let Some(active) = state.active.as_ref() {
                active.cancelled.store(true, Ordering::Release);
            }
            let replaced = state.pending.replace(InspectionRequest {
                generation,
                path,
                cancelled: Arc::new(AtomicBool::new(false)),
            });
            state.inspection = None;
            state.inspection_generation = None;
            state.inspection_error = None;
            state.worker_error = None;
            state.cancelled = false;
            (generation, replaced)
        };
        if let Some(request) = replaced {
            request.cancelled.store(true, Ordering::Release);
        }
        let start = self.ensure_worker();
        (self.shared.wake)();
        start.map(|()| generation)
    }

    /// Invalidate current publication without waiting for Restart Manager.
    pub fn cancel(&self) -> Result<bool, FileLockSessionError> {
        let (changed, pending) = {
            let mut state = self.shared.state.lock().unwrap();
            if state.closed {
                return Err(FileLockSessionError::Closed);
            }
            if state.pending.is_none() && state.active.is_none() {
                return Ok(false);
            }
            let generation = state
                .generation
                .checked_add(1)
                .ok_or(FileLockSessionError::GenerationExhausted)?;
            state.generation = generation;
            state.cancelled = true;
            state.inspection = None;
            state.inspection_generation = None;
            state.inspection_error = None;
            state.worker_error = None;
            if let Some(active) = state.active.as_ref() {
                active.cancelled.store(true, Ordering::Release);
            }
            (true, state.pending.take())
        };
        if let Some(request) = pending {
            request.cancelled.store(true, Ordering::Release);
        }
        (self.shared.wake)();
        Ok(changed)
    }

    /// Stop accepting publication and transfer any worker handle without
    /// waiting for the native call to return.
    pub fn close(&self) {
        let mut worker_slot = self.worker.lock().unwrap();
        {
            let mut state = self.shared.state.lock().unwrap();
            if state.closed {
                return;
            }
            state.closed = true;
            state.generation = state.generation.saturating_add(1);
            state.cancelled = false;
            state.inspection = None;
            state.inspection_generation = None;
            state.inspection_error = None;
            state.worker_error = None;
            if let Some(active) = state.active.as_ref() {
                active.cancelled.store(true, Ordering::Release);
            }
            if let Some(pending) = state.pending.take() {
                pending.cancelled.store(true, Ordering::Release);
            }
        }
        self.transfer_worker_to_reaper(&mut worker_slot);
        drop(worker_slot);
        (self.shared.wake)();
    }

    /// Join settled work and start only the latest queued request.
    pub fn poll(&self) -> Result<(), FileLockSessionError> {
        self.ensure_worker()
    }

    pub fn view(&self) -> FileLockSessionView {
        let _ = self.poll();
        let state = self.shared.state.lock().unwrap();
        let busy = state.active.is_some() || state.pending.is_some();
        let status = if state.closed {
            FileLockSessionStatus::Closed
        } else if state.cancelled {
            FileLockSessionStatus::Cancelled
        } else if state.pending.is_some() {
            FileLockSessionStatus::RefreshQueued
        } else if state.active.is_some() {
            FileLockSessionStatus::Inspecting
        } else if state.inspection_error.is_some() || state.worker_error.is_some() {
            FileLockSessionStatus::Failed
        } else if state.inspection.is_some() {
            FileLockSessionStatus::Complete
        } else {
            FileLockSessionStatus::Idle
        };
        FileLockSessionView {
            generation: state.generation,
            inspection: state.inspection.clone(),
            inspection_generation: state.inspection_generation,
            inspection_error: state.inspection_error.clone(),
            worker_error: state.worker_error.clone(),
            status,
            busy,
            closed: state.closed,
        }
    }

    fn ensure_worker(&self) -> Result<(), FileLockSessionError> {
        let mut worker_slot = self.worker.lock().unwrap();
        match worker_slot.as_ref() {
            Some(WorkerSlot::Running(worker)) if !worker.join.is_finished() => {
                let settled = self.shared.state.lock().unwrap().active.is_none();
                if !settled {
                    return Ok(());
                }
                self.transfer_worker_to_reaper(&mut worker_slot);
            }
            Some(WorkerSlot::Retiring {
                receipt,
                generation,
            }) if receipt.load(Ordering::Acquire) == WORKER_REAPER_FAILED => {
                let failure = FileLockSessionError::WorkerCapacity(
                    "the previous file-user inspection worker could not be joined safely".into(),
                );
                let mut state = self.shared.state.lock().unwrap();
                if !state.closed {
                    if let Some(pending) = state.pending.take() {
                        pending.cancelled.store(true, Ordering::Release);
                        state.worker_error = Some(failure.clone());
                    } else if state.generation == *generation && state.inspection.is_none() {
                        state.worker_error = Some(failure.clone());
                    }
                }
                return Err(failure);
            }
            Some(WorkerSlot::Retiring { receipt, .. }) if receipt.load(Ordering::Acquire) == 0 => {
                return Ok(());
            }
            _ => {}
        }

        if let Some(slot) = worker_slot.take() {
            match slot {
                WorkerSlot::Running(worker) => {
                    let panicked = worker.join.join().is_err();
                    drop(worker.reap_permit);
                    if panicked {
                        self.note_worker_panic(worker.generation);
                    }
                }
                WorkerSlot::Retiring {
                    receipt,
                    generation,
                } => {
                    let retired = receipt.load(Ordering::Acquire);
                    if retired == 0 {
                        *worker_slot = Some(WorkerSlot::Retiring {
                            receipt,
                            generation,
                        });
                        return Ok(());
                    }
                    if retired == WORKER_REAPER_FAILED {
                        *worker_slot = Some(WorkerSlot::Retiring {
                            receipt,
                            generation,
                        });
                        let failure = FileLockSessionError::WorkerCapacity(
                            "the previous file-user inspection worker could not be joined safely"
                                .into(),
                        );
                        let mut state = self.shared.state.lock().unwrap();
                        if !state.closed {
                            if let Some(pending) = state.pending.take() {
                                pending.cancelled.store(true, Ordering::Release);
                                state.worker_error = Some(failure.clone());
                            } else if state.generation == generation && state.inspection.is_none() {
                                state.worker_error = Some(failure.clone());
                            }
                        }
                        return Err(failure);
                    }
                    if retired == WORKER_PANICKED {
                        self.note_worker_panic(generation);
                    }
                }
            }
        }

        let pending_generation = {
            let state = self.shared.state.lock().unwrap();
            if state.closed || state.pending.is_none() {
                return Ok(());
            }
            state.pending.as_ref().map(|request| request.generation)
        };
        let Some(pending_generation) = pending_generation else {
            return Ok(());
        };

        let reap_permit = thread_reaper::reserve().map_err(|error| {
            let failure = FileLockSessionError::WorkerCapacity(format!(
                "file-user inspection worker capacity is unavailable: {error}"
            ));
            let mut state = self.shared.state.lock().unwrap();
            if state.generation == pending_generation && !state.closed {
                state.pending.take();
                state.worker_error = Some(failure.clone());
            }
            failure
        })?;
        let completion = reap_permit.completion_notifier();
        let request = {
            let mut state = self.shared.state.lock().unwrap();
            if state.closed {
                drop(reap_permit);
                return Err(FileLockSessionError::Closed);
            }
            let Some(request) = state.pending.take() else {
                drop(reap_permit);
                return Ok(());
            };
            if request.generation != pending_generation {
                state.pending = Some(request);
                drop(reap_permit);
                return Ok(());
            }
            state.active = Some(ActiveInspection {
                generation: request.generation,
                cancelled: Arc::clone(&request.cancelled),
            });
            request
        };
        let generation = request.generation;
        let shared = Arc::clone(&self.shared);
        let job = Arc::clone(&self.shared.job);
        let worker_path = request.path.clone();
        let cancelled = Arc::clone(&request.cancelled);
        let join = thread::Builder::new()
            .name("file-lock-inspection".into())
            .spawn(move || {
                let completion = completion;
                let outcome = if cancelled.load(Ordering::Acquire) {
                    None
                } else {
                    Some(catch_unwind(AssertUnwindSafe(|| job.run(worker_path))))
                };
                let mut state = shared.state.lock().unwrap();
                if state
                    .active
                    .as_ref()
                    .is_some_and(|active| active.generation == generation)
                {
                    state.active = None;
                }
                if !state.closed && !state.cancelled && state.generation == generation {
                    match outcome {
                        Some(Ok(Ok(inspection))) if inspection.path == request.path => {
                            state.inspection = Some(Arc::new(inspection));
                            state.inspection_generation = Some(generation);
                            state.inspection_error = None;
                            state.worker_error = None;
                        }
                        Some(Ok(Ok(_))) => {
                            state.worker_error = Some(FileLockSessionError::InvalidWorkerResult);
                            state.inspection_generation = None;
                        }
                        Some(Ok(Err(error))) => {
                            state.inspection_error = Some(error);
                            state.inspection_generation = None;
                            state.worker_error = None;
                        }
                        Some(Err(_)) => {
                            state.worker_error = Some(FileLockSessionError::WorkerPanicked);
                            state.inspection_generation = None;
                        }
                        None => {}
                    }
                }
                drop(state);
                drop(completion);
                (shared.wake)();
            })
            .map_err(|error| {
                let failure = FileLockSessionError::WorkerStart(error.to_string());
                let mut state = self.shared.state.lock().unwrap();
                if state
                    .active
                    .as_ref()
                    .is_some_and(|active| active.generation == generation)
                {
                    state.active = None;
                }
                if !state.closed && state.generation == generation {
                    state.worker_error = Some(failure.clone());
                }
                failure
            })?;
        *worker_slot = Some(WorkerSlot::Running(WorkerOwner {
            join,
            reap_permit,
            generation,
        }));
        Ok(())
    }

    fn transfer_worker_to_reaper(&self, worker_slot: &mut Option<WorkerSlot>) {
        let Some(WorkerSlot::Running(_)) = worker_slot.as_ref() else {
            return;
        };
        let Some(WorkerSlot::Running(worker)) = worker_slot.take() else {
            return;
        };
        let receipt = Arc::new(AtomicU8::new(0));
        let wake = Arc::clone(&self.shared.wake);
        let generation = worker.generation;
        let callback_receipt = ReaperReceipt {
            state: Arc::clone(&receipt),
            wake,
            completed: false,
        };
        let transfer = worker
            .reap_permit
            .reap_with_callback(worker.join, move |panicked| {
                callback_receipt.after_join(panicked);
            });
        if let Err(error) = transfer {
            receipt.store(WORKER_REAPER_FAILED, Ordering::Release);
            let mut state = self.shared.state.lock().unwrap();
            if !state.closed {
                if let Some(pending) = state.pending.take() {
                    pending.cancelled.store(true, Ordering::Release);
                    state.worker_error = Some(FileLockSessionError::WorkerCapacity(format!(
                        "file-user inspection worker could not transfer its settled handle: {error}"
                    )));
                } else if state.generation == generation && state.inspection.is_none() {
                    state.worker_error = Some(FileLockSessionError::WorkerCapacity(format!(
                        "file-user inspection worker could not transfer its settled handle: {error}"
                    )));
                }
            }
        }
        *worker_slot = Some(WorkerSlot::Retiring {
            receipt,
            generation,
        });
    }

    fn note_worker_panic(&self, generation: u64) {
        let mut state = self.shared.state.lock().unwrap();
        if !state.closed && state.generation == generation {
            state.active = None;
            state.pending = None;
            state.inspection = None;
            state.inspection_generation = None;
            state.inspection_error = None;
            state.worker_error = Some(FileLockSessionError::WorkerPanicked);
        }
    }
}

impl Drop for FileLockSession {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_operations::NativeFileIdentity;
    use std::collections::VecDeque;
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::time::{Duration, SystemTime};

    struct GateJob {
        started: Sender<PathBuf>,
        releases: Mutex<VecDeque<Receiver<()>>>,
    }

    impl FileLockInspectionJob for GateJob {
        fn run(&self, path: PathBuf) -> Result<FileLockInspection, FileLockInspectionFailure> {
            let _ = self.started.send(path.clone());
            if let Some(release) = self.releases.lock().unwrap().pop_front() {
                let _ = release.recv_timeout(Duration::from_secs(5));
            }
            Ok(FileLockInspection {
                path,
                identity: NativeFileIdentity::from_parts(1, [2; 16]),
                inspected_at: SystemTime::now(),
                processes: Vec::new(),
            })
        }
    }

    fn gated_session() -> (FileLockSession, Receiver<PathBuf>, Vec<Sender<()>>) {
        let (started_tx, started_rx) = mpsc::channel();
        let mut releases = Vec::new();
        let mut receivers = VecDeque::new();
        for _ in 0..4 {
            let (release_tx, release_rx) = mpsc::channel();
            releases.push(release_tx);
            receivers.push_back(release_rx);
        }
        let job = GateJob {
            started: started_tx,
            releases: Mutex::new(receivers),
        };
        (
            FileLockSession::with_job(Arc::new(job), Arc::new(|| {})),
            started_rx,
            releases,
        )
    }

    fn drive_until(
        session: &FileLockSession,
        mut predicate: impl FnMut(&FileLockSessionView) -> bool,
    ) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            let _ = session.poll();
            let view = session.view();
            if predicate(&view) {
                return;
            }
            thread::yield_now();
        }
        panic!(
            "file lock session did not reach the expected state: {:?}",
            session.view().status()
        );
    }

    fn wait_for_started(
        session: &FileLockSession,
        started: &Receiver<PathBuf>,
        expected: &PathBuf,
    ) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            let _ = session.poll();
            match started.try_recv() {
                Ok(path) if &path == expected => return,
                Ok(path) => panic!("unexpected inspection started for {path:?}"),
                Err(mpsc::TryRecvError::Disconnected) => panic!("inspection start channel closed"),
                Err(mpsc::TryRecvError::Empty) => thread::yield_now(),
            }
        }
        panic!("inspection for {expected:?} did not start");
    }

    #[test]
    fn file_lock_session_refresh_runs_latest_only_and_rejects_stale_result() {
        let (session, started, releases) = gated_session();
        let path_a = PathBuf::from("A");
        let path_b = PathBuf::from("B");
        let path_c = PathBuf::from("C");

        session.refresh(path_a.clone()).expect("start A");
        assert_eq!(
            started.recv_timeout(Duration::from_secs(2)).unwrap(),
            path_a
        );
        session.refresh(path_b).expect("queue B");
        session.refresh(path_c.clone()).expect("replace with C");
        releases[0].send(()).unwrap();
        wait_for_started(&session, &started, &path_c);
        releases[1].send(()).unwrap();
        drive_until(&session, |view| {
            view.status() == FileLockSessionStatus::Complete && view.inspection_is_current()
        });

        let view = session.view();
        assert_eq!(view.inspection().unwrap().path, path_c);
        assert_eq!(view.generation(), 3);
        assert_eq!(started.try_recv(), Err(mpsc::TryRecvError::Empty));
    }

    #[test]
    fn file_lock_session_cancel_invalidates_publication_without_waiting() {
        let (session, started, releases) = gated_session();
        let path = PathBuf::from("blocked");

        session.refresh(path.clone()).expect("start");
        assert_eq!(started.recv_timeout(Duration::from_secs(2)).unwrap(), path);
        assert!(session.cancel().expect("cancel"));
        let view = session.view();
        assert_eq!(view.status(), FileLockSessionStatus::Cancelled);
        assert!(view.inspection().is_none());
        releases[0].send(()).unwrap();
        drive_until(&session, |view| {
            view.status() == FileLockSessionStatus::Cancelled && !view.is_busy()
        });
    }

    #[test]
    fn file_lock_session_close_does_not_join_a_blocked_inspection() {
        let (session, started, releases) = gated_session();
        let path = PathBuf::from("blocked-close");

        session.refresh(path.clone()).expect("start");
        assert_eq!(started.recv_timeout(Duration::from_secs(2)).unwrap(), path);
        let start = std::time::Instant::now();
        session.close();
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_eq!(session.view().status(), FileLockSessionStatus::Closed);
        releases[0].send(()).unwrap();
    }
}
