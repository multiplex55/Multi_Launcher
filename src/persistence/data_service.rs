use super::{
    BackupEngine, BackupPolicy, PendingRecoveryDescriptor, PersistenceCatalog, PersistentStoreId,
    RecoveryManager, SnapshotRecord, SnapshotResult, StagedRecoveryAction, StoreCriticality,
    StoreHealth, StoreKind, StoreOwnership, StorePrivacy,
};
use crate::platform::app_data::AppDataRoot;
use crate::settings::Settings;
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

const RESULT_CAPACITY: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DataRequestId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataServiceRequest {
    ScanHealth,
    CreateSnapshot,
    ListSnapshots,
    StageRecovery(StagedRecoveryAction),
}

impl DataServiceRequest {
    fn index(&self) -> usize {
        match self {
            Self::ScanHealth => 0,
            Self::CreateSnapshot => 1,
            Self::ListSnapshots => 2,
            Self::StageRecovery(_) => 3,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreHealthReport {
    pub id: PersistentStoreId,
    pub label: &'static str,
    pub path: PathBuf,
    pub kind: StoreKind,
    pub parent_exists: bool,
    pub restore_eligible: bool,
    pub reset_eligible: bool,
    pub criticality: StoreCriticality,
    pub ownership: StoreOwnership,
    pub backup_policy: BackupPolicy,
    pub privacy: StorePrivacy,
    pub externally_configured: bool,
    pub health: StoreHealth,
}

#[derive(Clone, Debug)]
pub enum DataServiceResult {
    Health(Vec<StoreHealthReport>),
    Snapshot(SnapshotResult),
    Snapshots(Vec<SnapshotRecord>),
    RecoveryStaged(PendingRecoveryDescriptor),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataServiceFailure {
    Operation(String),
    Panicked,
}

#[derive(Clone, Debug)]
pub struct DataServiceCompletion {
    pub request_id: DataRequestId,
    pub request: DataServiceRequest,
    pub result: Result<DataServiceResult, DataServiceFailure>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmissionDisposition {
    Queued,
    CoalescedWithActive,
    ReplacedPending { request_id: DataRequestId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataServiceSubmission {
    pub request_id: DataRequestId,
    pub disposition: SubmissionDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataServiceActivity {
    pub active: Option<(DataServiceRequest, DataRequestId)>,
    pub pending: Option<(DataServiceRequest, DataRequestId)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataServiceSubmitError {
    ShuttingDown,
    WorkerStopped,
    RecoveryBusy,
}

#[derive(Clone)]
struct PendingRequest {
    request: DataServiceRequest,
    request_id: DataRequestId,
}

struct ActiveRequest {
    request: DataServiceRequest,
    cancel: Arc<AtomicBool>,
}

struct ServiceState {
    next_request_id: u64,
    latest_by_kind: [DataRequestId; 4],
    pending: Option<PendingRequest>,
    active: Option<ActiveRequest>,
    results: VecDeque<DataServiceCompletion>,
    shutting_down: bool,
    worker_stopped: bool,
}

impl Default for ServiceState {
    fn default() -> Self {
        Self {
            next_request_id: 0,
            latest_by_kind: [DataRequestId(0); 4],
            pending: None,
            active: None,
            results: VecDeque::with_capacity(RESULT_CAPACITY),
            shutting_down: false,
            worker_stopped: false,
        }
    }
}

trait DataServiceBackend: Send + 'static {
    fn execute(
        &mut self,
        request: DataServiceRequest,
        cancel: &AtomicBool,
    ) -> Result<DataServiceResult, String>;
}

struct ProductionBackend {
    root: AppDataRoot,
    catalog: Option<PersistenceCatalog>,
    catalog_factory: Option<Box<dyn CatalogFactory>>,
    #[cfg(test)]
    recovery_started: Option<Box<dyn Fn(&AtomicBool) + Send>>,
}

trait CatalogFactory: Send + 'static {
    fn build(&mut self) -> PersistenceCatalog;
}

impl<F> CatalogFactory for F
where
    F: FnMut() -> PersistenceCatalog + Send + 'static,
{
    fn build(&mut self) -> PersistenceCatalog {
        self()
    }
}

impl ProductionBackend {
    fn catalog(&mut self) -> &PersistenceCatalog {
        if self.catalog.is_none() {
            self.catalog = Some(
                self.catalog_factory
                    .as_mut()
                    .expect("lazy production backend has a catalog factory")
                    .build(),
            );
        }
        self.catalog.as_ref().expect("catalog initialized")
    }
}

impl DataServiceBackend for ProductionBackend {
    fn execute(
        &mut self,
        request: DataServiceRequest,
        cancel: &AtomicBool,
    ) -> Result<DataServiceResult, String> {
        if cancel.load(Ordering::Acquire) {
            return Err("operation cancelled".into());
        }
        #[cfg(test)]
        if matches!(request, DataServiceRequest::StageRecovery(_)) {
            if let Some(hook) = &self.recovery_started {
                hook(cancel);
            }
        }
        let root = self.root.clone();
        let catalog = self.catalog();
        match request {
            DataServiceRequest::ScanHealth => {
                let mut reports = Vec::with_capacity(catalog.stores().len());
                for store in catalog.stores() {
                    if cancel.load(Ordering::Acquire) {
                        return Err("operation cancelled".into());
                    }
                    reports.push(StoreHealthReport {
                        id: store.id,
                        label: store.label,
                        path: store.path.clone(),
                        kind: store.kind,
                        parent_exists: store.path.parent().is_some_and(|parent| parent.is_dir()),
                        restore_eligible: store.restore_eligible,
                        reset_eligible: store.reset_eligible,
                        criticality: store.criticality,
                        ownership: store.ownership,
                        backup_policy: store.backup_policy,
                        privacy: store.privacy,
                        externally_configured: store.externally_configured,
                        health: store.probe(),
                    });
                }
                Ok(DataServiceResult::Health(reports))
            }
            DataServiceRequest::CreateSnapshot => BackupEngine::new(&root, catalog)
                .create_snapshot_cancellable(&|| cancel.load(Ordering::Acquire))
                .map(DataServiceResult::Snapshot)
                .map_err(|error| error.to_string()),
            DataServiceRequest::ListSnapshots => BackupEngine::new(&root, catalog)
                .list_snapshots_cancellable(&|| cancel.load(Ordering::Acquire))
                .map(DataServiceResult::Snapshots)
                .map_err(|error| error.to_string()),
            DataServiceRequest::StageRecovery(action) => {
                let manager = RecoveryManager::new(&root, catalog);
                match action {
                    StagedRecoveryAction::Restore {
                        store_id,
                        snapshot_id,
                    } => manager.stage_restore_cancellable(store_id, &snapshot_id, &|| {
                        cancel.load(Ordering::Acquire)
                    }),
                    StagedRecoveryAction::Reset { store_id } => manager
                        .stage_reset_cancellable(store_id, &|| cancel.load(Ordering::Acquire)),
                }
                .map(DataServiceResult::RecoveryStaged)
                .map_err(|error| error.to_string())
            }
        }
    }
}

/// One owned, event-driven persistence worker. Submission only updates a
/// capacity-one pending slot and never performs filesystem work on the caller.
pub struct DataService {
    state: Arc<Mutex<ServiceState>>,
    wake: Option<SyncSender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl DataService {
    pub fn start(
        root: AppDataRoot,
        catalog: PersistenceCatalog,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        Self::start_with_backend(
            ProductionBackend {
                root,
                catalog: Some(catalog),
                catalog_factory: None,
                #[cfg(test)]
                recovery_started: None,
            },
            repaint,
        )
    }

    pub fn start_lazy(
        root: AppDataRoot,
        settings: Settings,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        let factory_root = root.clone();
        Self::start_with_backend(
            ProductionBackend {
                root,
                catalog: None,
                catalog_factory: Some(Box::new(move || {
                    PersistenceCatalog::new(&factory_root, &settings)
                })),
                #[cfg(test)]
                recovery_started: None,
            },
            repaint,
        )
    }

    fn start_with_backend(
        backend: impl DataServiceBackend,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        let state = Arc::new(Mutex::new(ServiceState::default()));
        let (wake, receiver) = sync_channel(1);
        let worker_state = Arc::clone(&state);
        let repaint = Arc::new(repaint);
        let worker = thread::Builder::new()
            .name("persistence-data-service".into())
            .spawn(move || run_worker(receiver, worker_state, backend, repaint))?;
        Ok(Self {
            state,
            wake: Some(wake),
            worker: Some(worker),
        })
    }

    pub fn submit(
        &self,
        request: DataServiceRequest,
    ) -> Result<DataServiceSubmission, DataServiceSubmitError> {
        let disposition;
        let request_id;
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| DataServiceSubmitError::WorkerStopped)?;
            if state.shutting_down {
                return Err(DataServiceSubmitError::ShuttingDown);
            }
            if state.worker_stopped {
                return Err(DataServiceSubmitError::WorkerStopped);
            }
            if state.active.as_ref().is_some_and(|active| {
                matches!(active.request, DataServiceRequest::StageRecovery(_))
                    && active.request != request
            }) {
                return Err(DataServiceSubmitError::RecoveryBusy);
            }
            state.next_request_id = state.next_request_id.saturating_add(1);
            request_id = DataRequestId(state.next_request_id);
            state.latest_by_kind[request.index()] = request_id;
            // A newly requested generation makes any completed-but-undrained
            // result of the same kind stale. Suppress it here so callers can
            // never observe an older generation after submit returns.
            state
                .results
                .retain(|completion| completion.request != request);
            if state
                .active
                .as_ref()
                .is_some_and(|active| active.request == request)
            {
                disposition = SubmissionDisposition::CoalescedWithActive;
            } else {
                disposition = state
                    .pending
                    .replace(PendingRequest {
                        request: request.clone(),
                        request_id,
                    })
                    .map_or(SubmissionDisposition::Queued, |replaced| {
                        SubmissionDisposition::ReplacedPending {
                            request_id: replaced.request_id,
                        }
                    });
            }
        }

        let Some(wake) = self.wake.as_ref() else {
            return Err(DataServiceSubmitError::ShuttingDown);
        };
        match wake.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) => Ok(DataServiceSubmission {
                request_id,
                disposition,
            }),
            Err(TrySendError::Disconnected(())) => {
                if let Ok(mut state) = self.state.lock() {
                    state.worker_stopped = true;
                    state.pending = None;
                }
                Err(DataServiceSubmitError::WorkerStopped)
            }
        }
    }

    pub fn drain_results(&self) -> Vec<DataServiceCompletion> {
        self.state
            .lock()
            .map(|mut state| state.results.drain(..).collect())
            .unwrap_or_default()
    }

    pub fn activity(&self) -> DataServiceActivity {
        self.state
            .lock()
            .map(|state| DataServiceActivity {
                active: state.active.as_ref().map(|active| {
                    (
                        active.request.clone(),
                        state.latest_by_kind[active.request.index()],
                    )
                }),
                pending: state
                    .pending
                    .as_ref()
                    .map(|pending| (pending.request.clone(), pending.request_id)),
            })
            .unwrap_or(DataServiceActivity {
                active: None,
                pending: None,
            })
    }

    /// Cancel a request only if it is still the newest generation for its
    /// operation kind. Stale request IDs cannot cancel newer joined work.
    pub fn cancel(&self, request_id: DataRequestId) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        if state
            .pending
            .as_ref()
            .is_some_and(|pending| pending.request_id == request_id)
        {
            state.pending = None;
            return true;
        }
        let Some(active) = state.active.as_ref() else {
            return false;
        };
        if state.latest_by_kind[active.request.index()] != request_id {
            return false;
        }
        active.cancel.store(true, Ordering::Release);
        true
    }

    pub fn shutdown(&mut self) {
        if self.worker.is_none() {
            return;
        }
        if let Ok(mut state) = self.state.lock() {
            state.shutting_down = true;
            state.pending = None;
            if let Some(active) = &state.active {
                active.cancel.store(true, Ordering::Release);
            }
        }
        if let Some(wake) = self.wake.take() {
            let _ = wake.try_send(());
            drop(wake);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for DataService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run_worker(
    receiver: std::sync::mpsc::Receiver<()>,
    state: Arc<Mutex<ServiceState>>,
    mut backend: impl DataServiceBackend,
    repaint: Arc<dyn Fn() + Send + Sync>,
) {
    while receiver.recv().is_ok() {
        let (pending, cancel) = {
            let Ok(mut state) = state.lock() else { break };
            if state.shutting_down {
                break;
            }
            let Some(pending) = state.pending.take() else {
                continue;
            };
            let cancel = Arc::new(AtomicBool::new(false));
            state.active = Some(ActiveRequest {
                request: pending.request.clone(),
                cancel: Arc::clone(&cancel),
            });
            (pending, cancel)
        };

        let executed = catch_unwind(AssertUnwindSafe(|| {
            backend.execute(pending.request.clone(), &cancel)
        }));
        let result = match executed {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(error)) => Err(DataServiceFailure::Operation(error)),
            Err(_) => Err(DataServiceFailure::Panicked),
        };

        let published = {
            let Ok(mut state) = state.lock() else { break };
            state.active = None;
            if state.shutting_down || cancel.load(Ordering::Acquire) {
                false
            } else {
                // Repeated requests of the active kind join that work. Publishing
                // under the newest ID makes every older generation stale by
                // construction, without repeating an expensive snapshot.
                let request_id = state.latest_by_kind[pending.request.index()];
                if state.results.len() == RESULT_CAPACITY {
                    state.results.pop_front();
                }
                state.results.push_back(DataServiceCompletion {
                    request_id,
                    request: pending.request,
                    result,
                });
                true
            }
        };
        if published {
            repaint();
        }
    }
    if let Ok(mut state) = state.lock() {
        state.active = None;
        state.pending = None;
        state.worker_stopped = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::time::{Duration, Instant};

    enum Behavior {
        Immediate,
        Controlled {
            started: Sender<()>,
            release: Receiver<()>,
        },
        Failure,
        Panic,
    }

    struct TestBackend {
        behavior: Behavior,
        calls: Arc<Mutex<Vec<DataServiceRequest>>>,
    }

    impl DataServiceBackend for TestBackend {
        fn execute(
            &mut self,
            request: DataServiceRequest,
            cancel: &AtomicBool,
        ) -> Result<DataServiceResult, String> {
            self.calls.lock().unwrap().push(request);
            match &self.behavior {
                Behavior::Immediate => {}
                Behavior::Controlled { started, release } => {
                    started.send(()).unwrap();
                    loop {
                        if cancel.load(Ordering::Acquire) {
                            return Err("controlled cancellation".into());
                        }
                        match release.recv_timeout(Duration::from_millis(10)) {
                            Ok(()) => break,
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                                return Err("controlled release disconnected".into());
                            }
                        }
                    }
                }
                Behavior::Failure => return Err("controlled failure".into()),
                Behavior::Panic => panic!("controlled backend panic"),
            }
            if cancel.load(Ordering::Acquire) {
                return Err("controlled cancellation".into());
            }
            Ok(DataServiceResult::Health(Vec::new()))
        }
    }

    fn wait_for_results(service: &DataService, count: usize) -> Vec<DataServiceCompletion> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut collected = Vec::new();
        loop {
            collected.extend(service.drain_results());
            if collected.len() >= count {
                return collected;
            }
            assert!(Instant::now() < deadline, "timed out waiting for result");
            thread::yield_now();
        }
    }

    #[test]
    fn lazy_catalog_factory_runs_once_on_named_worker_after_first_request() {
        let directory = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(directory.path());
        let factory_root = root.clone();
        let factory_calls = Arc::new(AtomicUsize::new(0));
        let factory_threads = Arc::new(Mutex::new(Vec::new()));
        let calls = Arc::clone(&factory_calls);
        let threads = Arc::clone(&factory_threads);
        let mut service = DataService::start_with_backend(
            ProductionBackend {
                root,
                catalog: None,
                catalog_factory: Some(Box::new(move || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    threads
                        .lock()
                        .unwrap()
                        .push(thread::current().name().unwrap_or("unnamed").to_owned());
                    PersistenceCatalog::new(&factory_root, &Settings::default())
                })),
                recovery_started: None,
            },
            || {},
        )
        .unwrap();

        assert_eq!(factory_calls.load(Ordering::SeqCst), 0);
        service.submit(DataServiceRequest::ScanHealth).unwrap();
        let _ = wait_for_results(&service, 1);
        assert_eq!(factory_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            factory_threads.lock().unwrap().as_slice(),
            &["persistence-data-service"]
        );

        service.submit(DataServiceRequest::ListSnapshots).unwrap();
        let _ = wait_for_results(&service, 1);
        assert_eq!(
            factory_calls.load(Ordering::SeqCst),
            1,
            "the worker must reuse its canonical catalog"
        );
        service.shutdown();
    }

    #[test]
    fn slow_request_is_nonblocking_and_repeated_clicks_join_active_work() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let repaints = Arc::new(AtomicUsize::new(0));
        let callback_count = Arc::clone(&repaints);
        let mut service = DataService::start_with_backend(
            TestBackend {
                behavior: Behavior::Controlled {
                    started: started_tx,
                    release: release_rx,
                },
                calls: Arc::clone(&calls),
            },
            move || {
                callback_count.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();
        let first = service.submit(DataServiceRequest::CreateSnapshot).unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let before = Instant::now();
        let second = service.submit(DataServiceRequest::CreateSnapshot).unwrap();
        assert!(before.elapsed() < Duration::from_millis(100));
        assert_eq!(
            second.disposition,
            SubmissionDisposition::CoalescedWithActive
        );
        release_tx.send(()).unwrap();
        let results = wait_for_results(&service, 1);
        assert_eq!(results[0].request_id, second.request_id);
        assert_ne!(results[0].request_id, first.request_id);
        assert_eq!(calls.lock().unwrap().len(), 1);
        assert_eq!(repaints.load(Ordering::SeqCst), 1);
        service.shutdown();
    }

    #[test]
    fn pending_slot_is_capacity_one_and_reports_replacement() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut service = DataService::start_with_backend(
            TestBackend {
                behavior: Behavior::Controlled {
                    started: started_tx,
                    release: release_rx,
                },
                calls: Arc::clone(&calls),
            },
            || {},
        )
        .unwrap();
        service.submit(DataServiceRequest::ScanHealth).unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let displaced = service.submit(DataServiceRequest::CreateSnapshot).unwrap();
        let replacement = service.submit(DataServiceRequest::ListSnapshots).unwrap();
        assert_eq!(
            replacement.disposition,
            SubmissionDisposition::ReplacedPending {
                request_id: displaced.request_id
            }
        );
        release_tx.send(()).unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        release_tx.send(()).unwrap();
        let results = wait_for_results(&service, 2);
        assert_eq!(results[0].request, DataServiceRequest::ScanHealth);
        assert_eq!(results[1].request, DataServiceRequest::ListSnapshots);
        assert_eq!(calls.lock().unwrap().len(), 2);
        service.shutdown();
    }

    #[test]
    fn newer_submission_suppresses_completed_but_undrained_same_kind_result() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut service = DataService::start_with_backend(
            TestBackend {
                behavior: Behavior::Controlled {
                    started: started_tx,
                    release: release_rx,
                },
                calls,
            },
            || {},
        )
        .unwrap();

        let unrelated = service.submit(DataServiceRequest::ListSnapshots).unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while service.state.lock().unwrap().results.len() < 1 {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }

        let first = service.submit(DataServiceRequest::ScanHealth).unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while service.state.lock().unwrap().results.len() < 2 {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }

        let second = service.submit(DataServiceRequest::ScanHealth).unwrap();
        assert_ne!(first.request_id, second.request_id);
        let retained = service.drain_results();
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].request_id, unrelated.request_id);
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        release_tx.send(()).unwrap();
        let results = wait_for_results(&service, 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].request_id, second.request_id);
        service.shutdown();
    }

    #[test]
    fn panic_is_contained_and_repaints_once_for_each_meaningful_result() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let repaints = Arc::new(AtomicUsize::new(0));
        let callback_count = Arc::clone(&repaints);
        let mut service = DataService::start_with_backend(
            TestBackend {
                behavior: Behavior::Panic,
                calls,
            },
            move || {
                callback_count.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();
        service.submit(DataServiceRequest::ScanHealth).unwrap();
        let results = wait_for_results(&service, 1);
        assert!(matches!(
            results[0].result,
            Err(DataServiceFailure::Panicked)
        ));
        assert_eq!(repaints.load(Ordering::SeqCst), 1);
        service.submit(DataServiceRequest::ListSnapshots).unwrap();
        let results = wait_for_results(&service, 1);
        assert!(matches!(
            results[0].result,
            Err(DataServiceFailure::Panicked)
        ));
        assert_eq!(repaints.load(Ordering::SeqCst), 2);
        service.shutdown();
    }

    #[test]
    fn operation_failure_is_delivered_without_stopping_worker() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut service = DataService::start_with_backend(
            TestBackend {
                behavior: Behavior::Failure,
                calls: Arc::clone(&calls),
            },
            || {},
        )
        .unwrap();
        service.submit(DataServiceRequest::ScanHealth).unwrap();
        let results = wait_for_results(&service, 1);
        assert!(matches!(
            &results[0].result,
            Err(DataServiceFailure::Operation(message)) if message == "controlled failure"
        ));
        service.submit(DataServiceRequest::ListSnapshots).unwrap();
        let results = wait_for_results(&service, 1);
        assert!(matches!(
            results[0].result,
            Err(DataServiceFailure::Operation(_))
        ));
        assert_eq!(calls.lock().unwrap().len(), 2);
        service.shutdown();
    }

    #[test]
    fn shutdown_cancels_pending_work_and_joins_worker() {
        let (started_tx, started_rx) = channel();
        let (_release_tx, release_rx) = channel();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut service = DataService::start_with_backend(
            TestBackend {
                behavior: Behavior::Controlled {
                    started: started_tx,
                    release: release_rx,
                },
                calls: Arc::clone(&calls),
            },
            || {},
        )
        .unwrap();
        service.submit(DataServiceRequest::ScanHealth).unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        service.submit(DataServiceRequest::CreateSnapshot).unwrap();
        service.shutdown();
        assert!(service.worker.is_none());
        assert_eq!(
            calls.lock().unwrap().as_slice(),
            &[DataServiceRequest::ScanHealth]
        );
        assert_eq!(
            service.submit(DataServiceRequest::ScanHealth),
            Err(DataServiceSubmitError::ShuttingDown)
        );
        assert!(service.drain_results().is_empty());
    }

    #[test]
    fn shutdown_promptly_cancels_real_backend_recovery_staging() {
        let directory = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(directory.path());
        let catalog = PersistenceCatalog::new(&root, &Settings::default());
        let (started_tx, started_rx) = channel();
        let backend = ProductionBackend {
            root: root.clone(),
            catalog: Some(catalog),
            catalog_factory: None,
            recovery_started: Some(Box::new(move |cancel| {
                started_tx.send(()).unwrap();
                while !cancel.load(Ordering::Acquire) {
                    thread::yield_now();
                }
            })),
        };
        let mut service = DataService::start_with_backend(backend, || {}).unwrap();
        service
            .submit(DataServiceRequest::StageRecovery(
                StagedRecoveryAction::Reset {
                    store_id: PersistentStoreId::Settings,
                },
            ))
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        let before = Instant::now();
        service.shutdown();

        assert!(before.elapsed() < Duration::from_secs(1));
        assert!(service.worker.is_none());
        assert!(!root.path().join("recovery/pending.json").exists());
        assert!(service.drain_results().is_empty());
    }

    #[test]
    fn explicit_cancellation_suppresses_result_and_stale_ids_cannot_cancel_joined_work() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let repaints = Arc::new(AtomicUsize::new(0));
        let callback_count = Arc::clone(&repaints);
        let mut service = DataService::start_with_backend(
            TestBackend {
                behavior: Behavior::Controlled {
                    started: started_tx,
                    release: release_rx,
                },
                calls,
            },
            move || {
                callback_count.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();
        let first = service.submit(DataServiceRequest::CreateSnapshot).unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let joined = service.submit(DataServiceRequest::CreateSnapshot).unwrap();
        assert!(!service.cancel(first.request_id));
        assert!(service.cancel(joined.request_id));
        release_tx.send(()).unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while service.activity().active.is_some() {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        assert!(service.drain_results().is_empty());
        assert_eq!(repaints.load(Ordering::SeqCst), 0);
        service.shutdown();
    }

    #[test]
    fn result_buffer_is_bounded() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut service = DataService::start_with_backend(
            TestBackend {
                behavior: Behavior::Immediate,
                calls,
            },
            || {},
        )
        .unwrap();
        for request in [
            DataServiceRequest::ScanHealth,
            DataServiceRequest::CreateSnapshot,
            DataServiceRequest::ListSnapshots,
            DataServiceRequest::ScanHealth,
        ] {
            service.submit(request.clone()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            while service
                .state
                .lock()
                .unwrap()
                .active
                .as_ref()
                .is_some_and(|active| active.request == request)
                || service.state.lock().unwrap().pending.is_some()
            {
                assert!(Instant::now() < deadline);
                thread::yield_now();
            }
        }
        assert_eq!(service.state.lock().unwrap().results.len(), RESULT_CAPACITY);
        service.shutdown();
    }

    #[test]
    fn typed_recovery_request_is_staged_by_the_owned_worker() {
        let directory = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(directory.path());
        let catalog = PersistenceCatalog::new(&root, &crate::settings::Settings::default());
        let repaints = Arc::new(AtomicUsize::new(0));
        let callback_count = Arc::clone(&repaints);
        let mut service = DataService::start(root.clone(), catalog, move || {
            callback_count.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();

        let submitted = service
            .submit(DataServiceRequest::StageRecovery(
                StagedRecoveryAction::Reset {
                    store_id: PersistentStoreId::Settings,
                },
            ))
            .unwrap();
        let results = wait_for_results(&service, 1);
        assert_eq!(results[0].request_id, submitted.request_id);
        assert!(matches!(
            &results[0].result,
            Ok(DataServiceResult::RecoveryStaged(
                PendingRecoveryDescriptor {
                    action: StagedRecoveryAction::Reset {
                        store_id: PersistentStoreId::Settings
                    },
                    ..
                }
            ))
        ));
        assert!(root.path().join("recovery/pending.json").exists());
        assert_eq!(repaints.load(Ordering::SeqCst), 1);
        service.shutdown();
    }

    #[test]
    fn distinct_recovery_cannot_retarget_an_active_request_id() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let mut service = DataService::start_with_backend(
            TestBackend {
                behavior: Behavior::Controlled {
                    started: started_tx,
                    release: release_rx,
                },
                calls: Arc::new(Mutex::new(Vec::new())),
            },
            || {},
        )
        .unwrap();
        let first_request = DataServiceRequest::StageRecovery(StagedRecoveryAction::Reset {
            store_id: PersistentStoreId::Settings,
        });
        let first = service.submit(first_request.clone()).unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            service.submit(DataServiceRequest::StageRecovery(
                StagedRecoveryAction::Reset {
                    store_id: PersistentStoreId::Actions,
                },
            )),
            Err(DataServiceSubmitError::RecoveryBusy)
        );
        release_tx.send(()).unwrap();
        let result = wait_for_results(&service, 1);
        assert_eq!(result[0].request_id, first.request_id);
        assert_eq!(result[0].request, first_request);
        service.shutdown();
    }
}
