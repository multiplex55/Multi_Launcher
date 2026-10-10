//! A single-worker coordinator for bounded, replaceable indexed-path scans.

#[cfg(test)]
use super::IndexCheckpoint;
use super::{IndexBatchIter, IndexBatchStep, IndexOptions};
use crate::actions::Action;
use crate::thread_reaper::{self, ReapPermit};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};

type NotifierCallback = Arc<dyn Fn() + Send + Sync + 'static>;
type Scanner =
    Arc<dyn Fn(&IndexConfig, Arc<AtomicBool>) -> ScanDisposition + Send + Sync + 'static>;
type WorkerTask = Box<dyn FnOnce() + Send + 'static>;
type WorkerSpawner = Box<dyn FnOnce(WorkerTask) -> std::io::Result<JoinHandle<()>>>;

fn invoke_callback(callback: NotifierCallback) {
    // Wakes are advisory; a faulty consumer callback must not kill the worker.
    let _ = catch_unwind(AssertUnwindSafe(|| callback()));
}

/// Immutable identity for one configured indexed-path traversal.
///
/// Root strings and ordering are preserved exactly. `None` and `Some(100_000)`
/// are distinct configurations even though they currently share a scan cap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexConfig {
    roots: Arc<[String]>,
    max_items: Option<usize>,
}

impl IndexConfig {
    #[must_use]
    pub fn new(roots: Vec<String>, max_items: Option<usize>) -> Self {
        Self {
            roots: roots.into(),
            max_items,
        }
    }

    #[must_use]
    pub fn roots(&self) -> &[String] {
        &self.roots
    }

    #[must_use]
    pub const fn max_items(&self) -> Option<usize> {
        self.max_items
    }
}

/// A complete traversal result. Failed scans never carry partial actions.
#[derive(Clone, Debug, PartialEq)]
pub struct IndexCompletion {
    generation: u64,
    config: IndexConfig,
    outcome: Result<Arc<Vec<Action>>, ScanFailure>,
}

impl IndexCompletion {
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn config(&self) -> &IndexConfig {
        &self.config
    }

    #[must_use]
    pub fn outcome(&self) -> &Result<Arc<Vec<Action>>, ScanFailure> {
        &self.outcome
    }
}

/// Public error information from a failed traversal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanFailure {
    message: String,
}

impl ScanFailure {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for ScanFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ScanFailure {}

/// Terminal state of the persistent coordinator worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerTermination {
    Panicked,
    ExitedUnexpectedly,
}

/// Errors from coordinator construction, request submission, and waiting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoordinatorError {
    ReaperCapacity(String),
    WorkerSpawn(String),
    Closed,
    WorkerTerminated(WorkerTermination),
    GenerationExhausted,
    NotifierIdExhausted,
    NotifierAlreadyAttached,
    Superseded {
        requested: u64,
        current: Option<u64>,
    },
    StartupResultNotAcknowledged(u64),
    ResultAlreadyAcknowledged(u64),
}

impl std::fmt::Display for CoordinatorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReaperCapacity(message) => write!(formatter, "{message}"),
            Self::WorkerSpawn(message) => {
                write!(formatter, "failed to spawn index worker: {message}")
            }
            Self::Closed => formatter.write_str("index coordinator is closed"),
            Self::WorkerTerminated(reason) => {
                write!(formatter, "index worker terminated: {reason:?}")
            }
            Self::GenerationExhausted => formatter.write_str("index request generation exhausted"),
            Self::NotifierIdExhausted => formatter.write_str("index notifier ID exhausted"),
            Self::NotifierAlreadyAttached => {
                formatter.write_str("an index notifier is already attached")
            }
            Self::Superseded { requested, current } => {
                write!(
                    formatter,
                    "index request {requested} was superseded by {current:?}"
                )
            }
            Self::StartupResultNotAcknowledged(generation) => write!(
                formatter,
                "startup index result {generation} has not been acknowledged"
            ),
            Self::ResultAlreadyAcknowledged(generation) => {
                write!(
                    formatter,
                    "index result {generation} was already acknowledged"
                )
            }
        }
    }
}

impl std::error::Error for CoordinatorError {}

/// Explicit shutdown can report either a worker panic or degraded reaping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShutdownError {
    ReaperSupervisorSpawn(String),
    WorkerPanicked,
    MissingReaperPermit,
}

impl std::fmt::Display for ShutdownError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReaperSupervisorSpawn(message) => {
                write!(
                    formatter,
                    "worker detached after join supervisor failure: {message}"
                )
            }
            Self::WorkerPanicked => formatter.write_str("index worker panicked"),
            Self::MissingReaperPermit => {
                formatter.write_str("index worker has no reaper reservation")
            }
        }
    }
}

impl std::error::Error for ShutdownError {}

/// Opaque identity used to revoke the currently attached completion wake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotifierToken(u64);

struct ScanRequest {
    generation: u64,
    config: IndexConfig,
    cancellation: Arc<AtomicBool>,
}

struct RequestIdentity {
    generation: u64,
    config: IndexConfig,
}

struct AttachedNotifier {
    token: NotifierToken,
    callback: NotifierCallback,
}

struct CoordinatorState {
    closed: bool,
    terminal: Option<WorkerTermination>,
    latest: Option<RequestIdentity>,
    next_generation: Option<u64>,
    active_cancellation: Option<Arc<AtomicBool>>,
    pending: Option<ScanRequest>,
    result: Option<Arc<IndexCompletion>>,
    acknowledged_generation: Option<u64>,
    notifier: Option<AttachedNotifier>,
    next_notifier_id: Option<u64>,
    notification_outstanding: bool,
    #[cfg(test)]
    ack_hook: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Default for CoordinatorState {
    fn default() -> Self {
        Self {
            closed: false,
            terminal: None,
            latest: None,
            next_generation: Some(1),
            active_cancellation: None,
            pending: None,
            result: None,
            acknowledged_generation: None,
            notifier: None,
            next_notifier_id: Some(1),
            notification_outstanding: false,
            #[cfg(test)]
            ack_hook: None,
        }
    }
}

struct Shared {
    state: Mutex<CoordinatorState>,
    changed: Condvar,
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, CoordinatorState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// One persistent scanner with a replaceable pending request and result slot.
///
/// The coordinator does not own UI events. A caller can attach a small wake
/// callback; completed actions remain in the single result slot until taken.
pub struct IndexCoordinator {
    shared: Arc<Shared>,
    lifecycle: Mutex<WorkerLifecycle>,
    #[cfg(test)]
    shutdown_hook: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

struct WorkerLifecycle {
    worker: Option<JoinHandle<()>>,
    reap_permit: Option<ReapPermit>,
}

impl IndexCoordinator {
    /// Reserve bounded join capacity and start the persistent worker.
    pub fn new() -> Result<Self, CoordinatorError> {
        Self::build(
            Arc::new(scan_indexed_paths),
            Box::new(|task| {
                thread::Builder::new()
                    .name("multi-launcher-indexer".into())
                    .spawn(task)
            }),
        )
    }

    fn build(scanner: Scanner, spawn_worker: WorkerSpawner) -> Result<Self, CoordinatorError> {
        let reap_permit = thread_reaper::reserve()
            .map_err(|error| CoordinatorError::ReaperCapacity(error.to_owned()))?;
        let completion_notifier = reap_permit.completion_notifier();
        let shared = Arc::new(Shared {
            state: Mutex::new(CoordinatorState::default()),
            changed: Condvar::new(),
        });
        let worker_shared = Arc::clone(&shared);
        let worker_task: WorkerTask = Box::new(move || {
            // Declared before worker locals so it drops only after traversal,
            // lifecycle publication, and panic handling have all completed.
            let completion_notifier = completion_notifier;
            let worker_result = catch_unwind(AssertUnwindSafe(|| {
                worker_loop(Arc::clone(&worker_shared), scanner)
            }));
            match worker_result {
                Ok(()) => mark_unexpected_exit_if_open(&worker_shared),
                Err(_) => mark_worker_termination(&worker_shared, WorkerTermination::Panicked),
            }
            drop(completion_notifier);
        });
        let worker = spawn_worker(worker_task)
            .map_err(|error| CoordinatorError::WorkerSpawn(error.to_string()))?;
        Ok(Self {
            shared,
            lifecycle: Mutex::new(WorkerLifecycle {
                worker: Some(worker),
                reap_permit: Some(reap_permit),
            }),
            #[cfg(test)]
            shutdown_hook: Mutex::new(None),
        })
    }

    /// Queue the newest configuration and return its monotonic generation.
    ///
    /// Only the active scan and one pending request can exist. A new request
    /// cancels the active token, replaces any pending request, and invalidates
    /// a result not yet consumed.
    pub fn submit(&self, config: IndexConfig) -> Result<u64, CoordinatorError> {
        let generation = {
            let mut state = self.shared.state();
            if state.closed {
                return Err(CoordinatorError::Closed);
            }
            if let Some(termination) = state.terminal {
                return Err(CoordinatorError::WorkerTerminated(termination));
            }
            let generation = state
                .next_generation
                .ok_or(CoordinatorError::GenerationExhausted)?;
            state.next_generation = generation.checked_add(1);
            if let Some(active) = state.active_cancellation.as_ref() {
                active.store(true, Ordering::Release);
            }
            let cancellation = Arc::new(AtomicBool::new(false));
            state.latest = Some(RequestIdentity {
                generation,
                config: config.clone(),
            });
            state.pending = Some(ScanRequest {
                generation,
                config,
                cancellation,
            });
            state.result = None;
            generation
        };
        self.shared.changed.notify_all();
        Ok(generation)
    }

    /// Atomically take and acknowledge the newest retained completion.
    pub fn take_result(&self) -> Option<Arc<IndexCompletion>> {
        let result = {
            let mut state = self.shared.state();
            let result = state.result.take();
            if let Some(completion) = result.as_ref() {
                state.acknowledged_generation = Some(completion.generation);
            }
            // Also acknowledges a queued stale wake with no current result.
            state.notification_outstanding = false;
            #[cfg(test)]
            if let Some(hook) = state.ack_hook.clone() {
                hook();
            }
            result
        };
        self.shared.changed.notify_all();
        result
    }

    /// Validate that startup consumed the result for the coordinator's exact
    /// latest identity before ownership is transferred to the GUI.
    pub fn validate_acknowledged_result(
        &self,
        generation: u64,
        config: &IndexConfig,
    ) -> Result<(), CoordinatorError> {
        let state = self.shared.state();
        let matches_latest = state
            .latest
            .as_ref()
            .is_some_and(|latest| latest.generation == generation && latest.config == *config);
        if !matches_latest {
            return Err(CoordinatorError::Superseded {
                requested: generation,
                current: state.latest.as_ref().map(|latest| latest.generation),
            });
        }
        if state.acknowledged_generation != Some(generation) {
            return Err(CoordinatorError::StartupResultNotAcknowledged(generation));
        }
        Ok(())
    }

    /// Attach one wake callback. A result retained before attachment wakes it.
    pub fn attach_notifier(
        &self,
        callback: impl Fn() + Send + Sync + 'static,
    ) -> Result<NotifierToken, CoordinatorError> {
        let (token, callback_to_run) = {
            let mut state = self.shared.state();
            if state.closed {
                return Err(CoordinatorError::Closed);
            }
            if let Some(termination) = state.terminal {
                return Err(CoordinatorError::WorkerTerminated(termination));
            }
            if state.notifier.is_some() {
                return Err(CoordinatorError::NotifierAlreadyAttached);
            }
            let id = state
                .next_notifier_id
                .ok_or(CoordinatorError::NotifierIdExhausted)?;
            state.next_notifier_id = id.checked_add(1);
            let token = NotifierToken(id);
            let callback: NotifierCallback = Arc::new(callback);
            state.notifier = Some(AttachedNotifier {
                token,
                callback: Arc::clone(&callback),
            });
            let callback_to_run = schedule_notification(&mut state);
            (token, callback_to_run)
        };
        if let Some(callback) = callback_to_run {
            invoke_callback(callback);
        }
        Ok(token)
    }

    /// Revoke only the matching attachment; a stale token cannot detach a new one.
    pub fn revoke_notifier(&self, token: NotifierToken) -> bool {
        let mut state = self.shared.state();
        if state
            .notifier
            .as_ref()
            .is_some_and(|notifier| notifier.token == token)
        {
            state.notifier = None;
            state.notification_outstanding = false;
            true
        } else {
            false
        }
    }

    /// Wait outside the GUI thread for one generation's retained completion.
    ///
    /// The wait ends with an explicit error if a newer request supersedes this
    /// generation, the result is acknowledged first, or the worker terminates.
    pub fn wait_for_completion(
        &self,
        generation: u64,
    ) -> Result<Arc<IndexCompletion>, CoordinatorError> {
        let mut state = self.shared.state();
        loop {
            if let Some(result) = state
                .result
                .as_ref()
                .filter(|result| result.generation == generation)
            {
                return Ok(Arc::clone(result));
            }
            if state.closed {
                return Err(CoordinatorError::Closed);
            }
            if let Some(termination) = state.terminal {
                return Err(CoordinatorError::WorkerTerminated(termination));
            }
            let current = state.latest.as_ref().map(|identity| identity.generation);
            if current != Some(generation) {
                return Err(CoordinatorError::Superseded {
                    requested: generation,
                    current,
                });
            }
            if state.acknowledged_generation == Some(generation) {
                return Err(CoordinatorError::ResultAlreadyAcknowledged(generation));
            }
            state = self
                .shared
                .changed
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    /// Current terminal worker state, if it panicked or exited unexpectedly.
    #[must_use]
    pub fn worker_termination(&self) -> Option<WorkerTermination> {
        self.shared.state().terminal
    }

    /// Close request intake and hand any live worker to the bounded reaper.
    ///
    /// This method never waits for active filesystem traversal. A completed
    /// handle is joined immediately; a live handle is transferred to the
    /// supervisor after coordinator locks are released.
    pub fn shutdown(&self) -> Result<(), ShutdownError> {
        let panicked = {
            let mut state = self.shared.state();
            state.closed = true;
            if let Some(cancellation) = state.active_cancellation.as_ref() {
                cancellation.store(true, Ordering::Release);
            }
            state.pending = None;
            state.result = None;
            state.latest = None;
            state.notifier = None;
            state.notification_outstanding = false;
            state.terminal == Some(WorkerTermination::Panicked)
        };
        self.shared.changed.notify_all();

        let (worker, reap_permit) = {
            let mut lifecycle = self
                .lifecycle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (lifecycle.worker.take(), lifecycle.reap_permit.take())
        };
        #[cfg(test)]
        let shutdown_hook = self
            .shutdown_hook
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        #[cfg(test)]
        if let Some(hook) = shutdown_hook {
            hook();
        }
        let (worker, reap_permit) = match (worker, reap_permit) {
            (Some(worker), Some(reap_permit)) => (worker, reap_permit),
            (Some(worker), None) => {
                drop(worker);
                return Err(ShutdownError::MissingReaperPermit);
            }
            (None, Some(reap_permit)) => {
                drop(reap_permit);
                return if panicked {
                    Err(ShutdownError::WorkerPanicked)
                } else {
                    Ok(())
                };
            }
            (None, None) => {
                return if panicked {
                    Err(ShutdownError::WorkerPanicked)
                } else {
                    Ok(())
                };
            }
        };

        if worker.is_finished() {
            let join_result = worker.join();
            drop(reap_permit);
            if panicked || join_result.is_err() {
                Err(ShutdownError::WorkerPanicked)
            } else {
                Ok(())
            }
        } else {
            match reap_permit.reap(worker) {
                Err(thread_reaper::ReapError::SupervisorSpawn(message)) => {
                    Err(ShutdownError::ReaperSupervisorSpawn(message))
                }
                Ok(()) if panicked => Err(ShutdownError::WorkerPanicked),
                Ok(()) => Ok(()),
            }
        }
    }

    #[cfg(test)]
    fn with_scanner(
        scanner: impl Fn(&IndexConfig, Arc<AtomicBool>) -> ScanDisposition + Send + Sync + 'static,
    ) -> Result<Self, CoordinatorError> {
        Self::build(
            Arc::new(scanner),
            Box::new(|task| {
                thread::Builder::new()
                    .name("index-coordinator-test".into())
                    .spawn(task)
            }),
        )
    }

    /// Narrow bridge for owner tests outside this private implementation
    /// module. Production construction continues to use the real scanner.
    #[cfg(test)]
    pub(crate) fn with_test_scanner(
        scanner: impl Fn(&IndexConfig) -> Result<Vec<Action>, String> + Send + Sync + 'static,
    ) -> Result<Self, CoordinatorError> {
        Self::with_scanner(move |config, cancellation| {
            if cancellation.load(Ordering::Acquire) {
                return ScanDisposition::Cancelled;
            }
            match scanner(config) {
                Ok(_actions) if cancellation.load(Ordering::Acquire) => ScanDisposition::Cancelled,
                Ok(actions) => ScanDisposition::Complete(actions),
                Err(message) => ScanDisposition::Failed(ScanFailure::new(message)),
            }
        })
    }

    #[cfg(test)]
    fn with_scanner_and_spawner(
        scanner: impl Fn(&IndexConfig, Arc<AtomicBool>) -> ScanDisposition + Send + Sync + 'static,
        spawner: impl FnOnce(WorkerTask) -> std::io::Result<JoinHandle<()>> + 'static,
    ) -> Result<Self, CoordinatorError> {
        Self::build(Arc::new(scanner), Box::new(spawner))
    }

    #[cfg(test)]
    fn set_ack_hook(&self, hook: Option<Arc<dyn Fn() + Send + Sync>>) {
        self.shared.state().ack_hook = hook;
    }

    #[cfg(test)]
    fn set_shutdown_hook(&self, hook: Option<Arc<dyn Fn() + Send + Sync>>) {
        *self
            .shutdown_hook
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = hook;
    }

    #[cfg(test)]
    fn bounds_for_test(&self) -> (usize, usize, usize, usize) {
        let state = self.shared.state();
        (
            usize::from(state.active_cancellation.is_some()),
            usize::from(state.pending.is_some()),
            usize::from(state.result.is_some()),
            usize::from(state.notification_outstanding),
        )
    }
}

impl Drop for IndexCoordinator {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

enum ScanDisposition {
    Complete(Vec<Action>),
    Failed(ScanFailure),
    Cancelled,
}

fn scan_indexed_paths(config: &IndexConfig, cancellation: Arc<AtomicBool>) -> ScanDisposition {
    #[cfg(test)]
    {
        scan_indexed_paths_inner(config, cancellation, None)
    }
    #[cfg(not(test))]
    {
        scan_indexed_paths_inner(config, cancellation)
    }
}

fn scan_indexed_paths_inner(
    config: &IndexConfig,
    cancellation: Arc<AtomicBool>,
    #[cfg(test)] checkpoint_hook: Option<Arc<dyn Fn(IndexCheckpoint) + Send + Sync>>,
) -> ScanDisposition {
    let iterator = IndexBatchIter::new_cancellable(
        config.roots(),
        IndexOptions::with_max_items(config.max_items()),
        Some(cancellation),
    );
    #[cfg(test)]
    let mut iterator = match checkpoint_hook {
        Some(hook) => iterator.with_checkpoint_hook(hook),
        None => iterator,
    };
    #[cfg(not(test))]
    let mut iterator = iterator;
    let mut actions = Vec::new();
    loop {
        match iterator.next_step() {
            Ok(IndexBatchStep::Batch(batch)) => actions.extend(batch),
            Ok(IndexBatchStep::Complete) => return ScanDisposition::Complete(actions),
            Ok(IndexBatchStep::Cancelled) => return ScanDisposition::Cancelled,
            Err(error) => return ScanDisposition::Failed(ScanFailure::new(error.to_string())),
        }
    }
}

fn worker_loop(shared: Arc<Shared>, scanner: Scanner) {
    loop {
        let request = {
            let mut state = shared.state();
            while !state.closed && state.terminal.is_none() && state.pending.is_none() {
                state = shared
                    .changed
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            if state.closed || state.terminal.is_some() {
                return;
            }
            let request = state.pending.take().expect("pending request was observed");
            state.active_cancellation = Some(Arc::clone(&request.cancellation));
            request
        };

        let disposition = scanner(&request.config, Arc::clone(&request.cancellation));
        let callback = {
            let mut state = shared.state();
            if state
                .active_cancellation
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, &request.cancellation))
            {
                state.active_cancellation = None;
            }
            if state.closed || request.cancellation.load(Ordering::Acquire) {
                None
            } else {
                let current = state.latest.as_ref().is_some_and(|identity| {
                    identity.generation == request.generation && identity.config == request.config
                });
                if !current {
                    None
                } else {
                    let outcome = match disposition {
                        ScanDisposition::Complete(actions) => Some(Ok(Arc::new(actions))),
                        ScanDisposition::Failed(error) => Some(Err(error)),
                        ScanDisposition::Cancelled => None,
                    };
                    if let Some(outcome) = outcome {
                        state.result = Some(Arc::new(IndexCompletion {
                            generation: request.generation,
                            config: request.config,
                            outcome,
                        }));
                        schedule_notification(&mut state)
                    } else {
                        None
                    }
                }
            }
        };
        shared.changed.notify_all();
        if let Some(callback) = callback {
            invoke_callback(callback);
        }
    }
}

fn schedule_notification(state: &mut CoordinatorState) -> Option<NotifierCallback> {
    if state.notification_outstanding || (state.result.is_none() && state.terminal.is_none()) {
        return None;
    }
    let notifier = state.notifier.as_ref()?;
    state.notification_outstanding = true;
    Some(Arc::clone(&notifier.callback))
}

fn mark_unexpected_exit_if_open(shared: &Shared) {
    let should_mark = {
        let state = shared.state();
        !state.closed && state.terminal.is_none()
    };
    if should_mark {
        mark_worker_termination(shared, WorkerTermination::ExitedUnexpectedly);
    }
}

fn mark_worker_termination(shared: &Shared, termination: WorkerTermination) {
    let callback = {
        let mut state = shared.state();
        state.terminal = Some(termination);
        if let Some(active) = state.active_cancellation.as_ref() {
            active.store(true, Ordering::Release);
        }
        state.active_cancellation = None;
        state.pending = None;
        schedule_notification(&mut state)
    };
    shared.changed.notify_all();
    if let Some(callback) = callback {
        invoke_callback(callback);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::{IndexOptions, index_paths_batched};
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{self, Receiver, SyncSender};
    use std::time::{Duration, Instant};

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn receive<T>(receiver: &Receiver<T>) -> T {
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("coordinator test channel should make progress")
    }

    fn action(label: &str) -> Action {
        Action {
            label: label.into(),
            desc: format!("{label} description"),
            action: format!("{label}:action"),
            args: None,
        }
    }

    fn complete(config: &IndexConfig, _cancellation: Arc<AtomicBool>) -> ScanDisposition {
        ScanDisposition::Complete(vec![action(
            config.roots().first().map_or("empty", String::as_str),
        )])
    }

    struct ReleaseOnDrop(Option<SyncSender<()>>);

    impl Drop for ReleaseOnDrop {
        fn drop(&mut self) {
            if let Some(release) = self.0.take() {
                let _ = release.try_send(());
            }
        }
    }

    #[test]
    fn index_coordinator_replaces_pending_and_suppresses_stale_completion() {
        let _serial = test_lock();
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let _release_on_drop = ReleaseOnDrop(Some(release_tx.clone()));
        let release_rx = Arc::new(Mutex::new(release_rx));
        let calls = Arc::new(Mutex::new(Vec::<String>::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let scanner = {
            let calls = Arc::clone(&calls);
            let active = Arc::clone(&active);
            let max_active = Arc::clone(&max_active);
            move |config: &IndexConfig, _cancel: Arc<AtomicBool>| {
                let now = active.fetch_add(1, Ordering::AcqRel) + 1;
                max_active.fetch_max(now, Ordering::AcqRel);
                let name = config.roots()[0].clone();
                calls.lock().unwrap().push(name.clone());
                if name == "A" {
                    entered_tx.send(()).unwrap();
                    let _ = release_rx.lock().unwrap().recv();
                }
                active.fetch_sub(1, Ordering::AcqRel);
                ScanDisposition::Complete(vec![action(&name)])
            }
        };
        let coordinator = IndexCoordinator::with_scanner(scanner).unwrap();
        let (wake_tx, wake_rx) = mpsc::sync_channel(2);
        coordinator
            .attach_notifier(move || wake_tx.try_send(()).unwrap())
            .unwrap();

        let first = coordinator
            .submit(IndexConfig::new(vec!["A".into()], None))
            .unwrap();
        receive(&entered_rx);
        let second = coordinator
            .submit(IndexConfig::new(vec!["B".into()], None))
            .unwrap();
        let third_config = IndexConfig::new(vec!["C".into()], None);
        let third = coordinator.submit(third_config.clone()).unwrap();
        assert!(third > second && second > first);
        assert_eq!(coordinator.bounds_for_test(), (1, 1, 0, 0));

        release_tx.send(()).unwrap();

        let completion = receive(&wake_rx);
        let result = coordinator.wait_for_completion(third).unwrap();
        assert_eq!(result.generation(), third);
        assert_eq!(result.config(), &third_config);
        assert_eq!(result.outcome().as_ref().unwrap()[0], action("C"));
        assert_eq!(completion, ());
        assert_eq!(
            *calls.lock().unwrap(),
            vec!["A".to_string(), "C".to_string()]
        );
        assert_eq!(max_active.load(Ordering::Acquire), 1);
        assert_eq!(
            coordinator.wait_for_completion(first),
            Err(CoordinatorError::Superseded {
                requested: first,
                current: Some(third)
            })
        );
        coordinator.shutdown().unwrap();
    }

    #[test]
    fn index_coordinator_retains_results_for_attach_and_rearms_revoked_wakes() {
        let _serial = test_lock();
        let coordinator = IndexCoordinator::with_scanner(complete).unwrap();
        let first = coordinator
            .submit(IndexConfig::new(vec!["retained".into()], None))
            .unwrap();
        let retained = coordinator.wait_for_completion(first).unwrap();
        assert_eq!(coordinator.bounds_for_test(), (0, 0, 1, 0));

        let (wake_tx, wake_rx) = mpsc::sync_channel(3);
        let stale = coordinator
            .attach_notifier(move || wake_tx.try_send(()).unwrap())
            .unwrap();
        receive(&wake_rx);
        assert!(coordinator.revoke_notifier(stale));
        assert!(!coordinator.revoke_notifier(stale));

        let (wake_tx, wake_rx) = mpsc::sync_channel(2);
        let current = coordinator
            .attach_notifier(move || wake_tx.try_send(()).unwrap())
            .unwrap();
        receive(&wake_rx);
        assert!(Arc::ptr_eq(
            &retained,
            &coordinator.wait_for_completion(first).unwrap()
        ));

        let second = coordinator
            .submit(IndexConfig::new(vec!["next".into()], None))
            .unwrap();
        let replaced = coordinator.wait_for_completion(second).unwrap();
        assert_eq!(replaced.generation(), second);
        assert_eq!(coordinator.bounds_for_test(), (0, 0, 1, 1));
        assert!(
            wake_rx.try_recv().is_err(),
            "coalesced completion reuses the outstanding wake"
        );
        assert!(Arc::ptr_eq(&replaced, &coordinator.take_result().unwrap()));
        assert_eq!(
            coordinator.wait_for_completion(second),
            Err(CoordinatorError::ResultAlreadyAcknowledged(second))
        );
        let third = coordinator
            .submit(IndexConfig::new(vec!["rearmed".into()], None))
            .unwrap();
        receive(&wake_rx);
        assert_eq!(
            coordinator.wait_for_completion(third).unwrap().generation(),
            third
        );
        assert!(coordinator.revoke_notifier(current));
        coordinator.shutdown().unwrap();
    }

    #[test]
    fn index_coordinator_completion_racing_ack_rearms_without_lost_wake() {
        let _serial = test_lock();
        let (second_started_tx, second_started_rx) = mpsc::sync_channel(1);
        let (second_returned_tx, second_returned_rx) = mpsc::sync_channel(1);
        let (second_release_tx, second_release_rx) = mpsc::sync_channel(1);
        let second_release_rx = Arc::new(Mutex::new(second_release_rx));
        let scanner = {
            let second_release_rx = Arc::clone(&second_release_rx);
            move |config: &IndexConfig, _cancel: Arc<AtomicBool>| {
                if config.roots()[0] == "second" {
                    second_started_tx.send(()).unwrap();
                    let _ = second_release_rx.lock().unwrap().recv();
                    second_returned_tx.send(()).unwrap();
                }
                ScanDisposition::Complete(vec![action(&config.roots()[0])])
            }
        };
        let coordinator = Arc::new(IndexCoordinator::with_scanner(scanner).unwrap());
        let (wake_tx, wake_rx) = mpsc::sync_channel(3);
        coordinator
            .attach_notifier(move || wake_tx.try_send(()).unwrap())
            .unwrap();
        let first = coordinator
            .submit(IndexConfig::new(vec!["first".into()], None))
            .unwrap();
        receive(&wake_rx);
        coordinator.wait_for_completion(first).unwrap();
        let second = coordinator
            .submit(IndexConfig::new(vec!["second".into()], None))
            .unwrap();
        receive(&second_started_rx);

        let (ack_entered_tx, ack_entered_rx) = mpsc::sync_channel(1);
        let (ack_release_tx, ack_release_rx) = mpsc::sync_channel(1);
        let ack_release_rx = Arc::new(Mutex::new(ack_release_rx));
        let only_once = Arc::new(AtomicBool::new(false));
        coordinator.set_ack_hook(Some(Arc::new({
            let ack_release_rx = Arc::clone(&ack_release_rx);
            let only_once = Arc::clone(&only_once);
            move || {
                if !only_once.swap(true, Ordering::AcqRel) {
                    ack_entered_tx.send(()).unwrap();
                    let _ = ack_release_rx.lock().unwrap().recv();
                }
            }
        })));
        let ack_coordinator = Arc::clone(&coordinator);
        let (ack_done_tx, ack_done_rx) = mpsc::sync_channel(1);
        let ack_thread = thread::spawn(move || {
            let result = ack_coordinator.take_result();
            ack_done_tx
                .send(result.map(|result| result.generation()))
                .unwrap();
        });
        receive(&ack_entered_rx);

        second_release_tx.send(()).unwrap();
        // Scanner return occurs while take_result still owns the publication lock.
        receive(&second_returned_rx);
        assert!(wake_rx.try_recv().is_err());
        ack_release_tx.send(()).unwrap();
        assert_eq!(receive(&ack_done_rx), None);
        ack_thread.join().unwrap();
        receive(&wake_rx);
        assert_eq!(
            coordinator
                .wait_for_completion(second)
                .unwrap()
                .generation(),
            second
        );
        assert_eq!(coordinator.bounds_for_test().2, 1);
        coordinator.shutdown().unwrap();
    }

    #[test]
    fn index_coordinator_cancellation_checks_skip_and_canonicalize_paths() {
        let _serial = test_lock();
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("subdir")).unwrap();
        fs::write(temp.path().join("one.txt"), "1").unwrap();
        fs::write(temp.path().join("two.txt"), "2").unwrap();
        let root = temp.path().to_string_lossy().into_owned();

        for (checkpoint, occurrence) in [
            (IndexCheckpoint::SkippedEntry, 1),
            (IndexCheckpoint::BeforeCanonicalize, 2),
            (IndexCheckpoint::AfterCanonicalize, 2),
        ] {
            let (entered_tx, entered_rx) = mpsc::sync_channel(1);
            let (release_tx, release_rx) = mpsc::sync_channel(1);
            let _release_on_drop = ReleaseOnDrop(Some(release_tx));
            let release_rx = Arc::new(Mutex::new(release_rx));
            let seen = Arc::new(AtomicUsize::new(0));
            let fired = Arc::new(AtomicBool::new(false));
            let checkpoint_hook: Arc<dyn Fn(IndexCheckpoint) + Send + Sync> = Arc::new({
                let release_rx = Arc::clone(&release_rx);
                let seen = Arc::clone(&seen);
                let fired = Arc::clone(&fired);
                move |actual| {
                    if actual == checkpoint
                        && seen.fetch_add(1, Ordering::AcqRel) + 1 == occurrence
                        && !fired.swap(true, Ordering::AcqRel)
                    {
                        entered_tx.send(()).unwrap();
                        let _ = release_rx.lock().unwrap().recv();
                    }
                }
            });
            let scanner = {
                let checkpoint_hook = Arc::clone(&checkpoint_hook);
                move |config: &IndexConfig, cancellation: Arc<AtomicBool>| {
                    if config.roots().is_empty() {
                        scan_indexed_paths(config, cancellation)
                    } else {
                        scan_indexed_paths_inner(
                            config,
                            cancellation,
                            Some(Arc::clone(&checkpoint_hook)),
                        )
                    }
                }
            };
            let coordinator = IndexCoordinator::with_scanner(scanner).unwrap();
            let first = coordinator
                .submit(IndexConfig::new(vec![root.clone()], None))
                .unwrap();
            receive(&entered_rx);
            let latest = coordinator
                .submit(IndexConfig::new(Vec::new(), None))
                .unwrap();
            drop(_release_on_drop);

            let latest_result = coordinator.wait_for_completion(latest).unwrap();
            assert!(latest_result.outcome().as_ref().unwrap().is_empty());
            assert_eq!(
                coordinator.wait_for_completion(first),
                Err(CoordinatorError::Superseded {
                    requested: first,
                    current: Some(latest)
                })
            );
            coordinator.shutdown().unwrap();
        }
    }

    #[test]
    fn index_coordinator_cancel_after_capped_batch_is_not_completion() {
        let _serial = test_lock();
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("one.txt"), "1").unwrap();
        let root = temp.path().to_string_lossy().into_owned();
        let cancellation = Arc::new(AtomicBool::new(false));
        let mut cancellable = IndexBatchIter::new_cancellable(
            std::slice::from_ref(&root),
            IndexOptions {
                batch_size: 512,
                max_items: 1,
            },
            Some(Arc::clone(&cancellation)),
        );
        assert!(matches!(
            cancellable.next_step(),
            Ok(IndexBatchStep::Batch(_))
        ));
        assert!(!cancellable.metric_scan_finished);
        cancellation.store(true, Ordering::Release);
        assert!(matches!(
            cancellable.next_step(),
            Ok(IndexBatchStep::Cancelled)
        ));
        assert!(!cancellable.metric_scan_finished);
        drop(cancellable);

        let mut legacy = IndexBatchIter::new(
            std::slice::from_ref(&root),
            IndexOptions {
                batch_size: 512,
                max_items: 1,
            },
        );
        assert!(matches!(legacy.next(), Some(Ok(_))));
        assert!(legacy.metric_scan_finished);
    }

    #[test]
    fn index_coordinator_failure_discards_partial_scan_and_next_request_recovers() {
        let _serial = test_lock();
        let temp = tempfile::tempdir().unwrap();
        let valid_root = temp.path().join("valid");
        fs::create_dir(&valid_root).unwrap();
        fs::write(valid_root.join("found.txt"), "payload").unwrap();
        let valid = valid_root.to_string_lossy().into_owned();
        let missing = temp.path().join("missing").to_string_lossy().into_owned();
        let coordinator = IndexCoordinator::new().unwrap();

        let failed_generation = coordinator
            .submit(IndexConfig::new(vec![valid.clone(), missing], None))
            .unwrap();
        let failed = coordinator.wait_for_completion(failed_generation).unwrap();
        assert!(failed.outcome().is_err());

        let recovered_generation = coordinator
            .submit(IndexConfig::new(vec![valid], None))
            .unwrap();
        let recovered = coordinator
            .wait_for_completion(recovered_generation)
            .unwrap();
        assert_eq!(recovered.outcome().as_ref().unwrap().len(), 1);
        assert!(
            recovered.outcome().as_ref().unwrap()[0]
                .action
                .ends_with("found.txt")
        );
        coordinator.shutdown().unwrap();
    }

    #[test]
    fn index_coordinator_production_scan_preserves_order_cap_and_runs_off_caller() {
        let _serial = test_lock();
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("nested")).unwrap();
        fs::write(temp.path().join("a.txt"), "a").unwrap();
        fs::write(temp.path().join("nested").join("b.txt"), "b").unwrap();
        let root = temp.path().to_string_lossy().into_owned();
        let roots = vec![root.clone(), root.clone()];
        let caller = thread::current().id();
        let worker_thread = Arc::new(Mutex::new(None));
        let scanner = {
            let worker_thread = Arc::clone(&worker_thread);
            move |config: &IndexConfig, cancellation: Arc<AtomicBool>| {
                *worker_thread.lock().unwrap() = Some(thread::current().id());
                scan_indexed_paths(config, cancellation)
            }
        };
        let coordinator = IndexCoordinator::with_scanner(scanner).unwrap();

        let all_generation = coordinator
            .submit(IndexConfig::new(roots.clone(), None))
            .unwrap();
        let all = coordinator.wait_for_completion(all_generation).unwrap();
        let expected = index_paths_batched(&roots, IndexOptions::default())
            .flat_map(Result::unwrap)
            .collect::<Vec<_>>();
        assert_eq!(all.outcome().as_ref().unwrap().as_ref(), &expected);
        assert_eq!(expected.len(), 2, "duplicate roots are deduplicated");
        assert_ne!(worker_thread.lock().unwrap().unwrap(), caller);

        let zero_generation = coordinator
            .submit(IndexConfig::new(roots.clone(), Some(0)))
            .unwrap();
        let zero = coordinator.wait_for_completion(zero_generation).unwrap();
        let expected_zero = index_paths_batched(&roots, IndexOptions::with_max_items(Some(0)))
            .flat_map(Result::unwrap)
            .collect::<Vec<_>>();
        assert_eq!(
            expected_zero.len(),
            1,
            "zero cap clamps to one like the legacy API"
        );
        assert_eq!(zero.outcome().as_ref().unwrap().as_ref(), &expected_zero);
        coordinator.shutdown().unwrap();
    }

    #[test]
    fn index_coordinator_shutdown_is_nonblocking_and_wakes_waiters() {
        let _serial = test_lock();
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let _release_on_drop = ReleaseOnDrop(Some(release_tx));
        let release_rx = Arc::new(Mutex::new(release_rx));
        let scanner = {
            let release_rx = Arc::clone(&release_rx);
            move |_config: &IndexConfig, _cancel: Arc<AtomicBool>| {
                entered_tx.send(()).unwrap();
                let _ = release_rx.lock().unwrap().recv();
                ScanDisposition::Complete(Vec::new())
            }
        };
        let coordinator = Arc::new(IndexCoordinator::with_scanner(scanner).unwrap());
        let generation = coordinator
            .submit(IndexConfig::new(vec!["blocked".into()], None))
            .unwrap();
        receive(&entered_rx);

        let waiter = {
            let coordinator = Arc::clone(&coordinator);
            let (done_tx, done_rx) = mpsc::sync_channel(1);
            thread::spawn(move || {
                done_tx
                    .send(coordinator.wait_for_completion(generation))
                    .unwrap();
            });
            done_rx
        };
        let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(1);
        {
            let coordinator = Arc::clone(&coordinator);
            thread::spawn(move || shutdown_tx.send(coordinator.shutdown()).unwrap());
        }
        receive(&shutdown_rx).unwrap();
        assert_eq!(receive(&waiter), Err(CoordinatorError::Closed));
        assert_eq!(
            coordinator.submit(IndexConfig::new(Vec::new(), None)),
            Err(CoordinatorError::Closed)
        );
        drop(_release_on_drop);
        coordinator.shutdown().unwrap();
        drop(reserve_all_reaper_slots());
    }

    #[test]
    fn index_coordinator_concurrent_shutdown_keeps_worker_and_reaper_permit_paired() {
        let _serial = test_lock();
        let (scan_started_tx, scan_started_rx) = mpsc::sync_channel(1);
        let (scan_release_tx, scan_release_rx) = mpsc::sync_channel(1);
        let _release_on_drop = ReleaseOnDrop(Some(scan_release_tx));
        let scan_release_rx = Arc::new(Mutex::new(scan_release_rx));
        let (scan_exited_tx, scan_exited_rx) = mpsc::sync_channel(1);
        let scanner = {
            let scan_release_rx = Arc::clone(&scan_release_rx);
            move |_config: &IndexConfig, _cancel: Arc<AtomicBool>| {
                scan_started_tx.send(()).unwrap();
                let _ = scan_release_rx.lock().unwrap().recv();
                scan_exited_tx.send(()).unwrap();
                ScanDisposition::Complete(Vec::new())
            }
        };
        let coordinator = Arc::new(IndexCoordinator::with_scanner(scanner).unwrap());
        coordinator
            .submit(IndexConfig::new(vec!["blocked".into()], None))
            .unwrap();
        receive(&scan_started_rx);

        let (lifecycle_taken_tx, lifecycle_taken_rx) = mpsc::sync_channel(1);
        let (finish_first_shutdown_tx, finish_first_shutdown_rx) = mpsc::sync_channel(1);
        let finish_first_shutdown_rx = Arc::new(Mutex::new(finish_first_shutdown_rx));
        let only_first = Arc::new(AtomicBool::new(false));
        coordinator.set_shutdown_hook(Some(Arc::new({
            let finish_first_shutdown_rx = Arc::clone(&finish_first_shutdown_rx);
            let only_first = Arc::clone(&only_first);
            move || {
                if !only_first.swap(true, Ordering::AcqRel) {
                    lifecycle_taken_tx.send(()).unwrap();
                    let _ = finish_first_shutdown_rx.lock().unwrap().recv();
                }
            }
        })));

        let (first_done_tx, first_done_rx) = mpsc::sync_channel(1);
        let first_coordinator = Arc::clone(&coordinator);
        let first_shutdown = thread::spawn(move || {
            first_done_tx.send(first_coordinator.shutdown()).unwrap();
        });
        receive(&lifecycle_taken_rx);

        let (second_done_tx, second_done_rx) = mpsc::sync_channel(1);
        let second_coordinator = Arc::clone(&coordinator);
        let second_shutdown = thread::spawn(move || {
            second_done_tx.send(second_coordinator.shutdown()).unwrap();
        });
        receive(&second_done_rx).unwrap();

        finish_first_shutdown_tx.send(()).unwrap();
        receive(&first_done_rx).unwrap();
        first_shutdown.join().unwrap();
        second_shutdown.join().unwrap();
        drop(_release_on_drop);
        receive(&scan_exited_rx);
        coordinator.set_shutdown_hook(None);
    }

    #[test]
    fn index_coordinator_worker_panic_is_terminal_and_wakes_waiters() {
        let _serial = test_lock();
        let coordinator = IndexCoordinator::with_scanner(|_, _| -> ScanDisposition {
            panic!("injected scanner panic")
        })
        .unwrap();
        let (wake_entered_tx, wake_entered_rx) = mpsc::sync_channel(1);
        let (wake_release_tx, wake_release_rx) = mpsc::sync_channel(1);
        let _release_on_drop = ReleaseOnDrop(Some(wake_release_tx));
        let wake_release_rx = Arc::new(Mutex::new(wake_release_rx));
        coordinator
            .attach_notifier({
                let wake_release_rx = Arc::clone(&wake_release_rx);
                move || {
                    wake_entered_tx.send(()).unwrap();
                    let _ = wake_release_rx.lock().unwrap().recv();
                }
            })
            .unwrap();
        let generation = coordinator
            .submit(IndexConfig::new(vec!["panic".into()], None))
            .unwrap();
        receive(&wake_entered_rx);
        assert_eq!(
            coordinator.worker_termination(),
            Some(WorkerTermination::Panicked)
        );
        assert_eq!(
            coordinator.wait_for_completion(generation),
            Err(CoordinatorError::WorkerTerminated(
                WorkerTermination::Panicked
            ))
        );
        assert_eq!(coordinator.shutdown(), Err(ShutdownError::WorkerPanicked));
        drop(_release_on_drop);
    }

    fn reserve_all_reaper_slots() -> Vec<ReapPermit> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let mut permits = Vec::new();
            while permits.len() < 16 {
                match thread_reaper::reserve() {
                    Ok(permit) => permits.push(permit),
                    Err(_) => break,
                }
            }
            if permits.len() == 16 {
                return permits;
            }
            drop(permits);
            assert!(
                Instant::now() < deadline,
                "reaper slots should be reclaimed"
            );
            thread::yield_now();
        }
    }

    #[test]
    fn index_coordinator_reserves_before_spawn_and_releases_failed_spawn_slot() {
        let _serial = test_lock();
        let permits = reserve_all_reaper_slots();
        assert!(matches!(
            IndexCoordinator::new(),
            Err(CoordinatorError::ReaperCapacity(_))
        ));
        drop(permits);

        let permits = reserve_all_reaper_slots();
        let mut held: Vec<_> = permits.into_iter().take(15).collect();
        let reservation_was_held_at_spawn = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&reservation_was_held_at_spawn);
        let result = IndexCoordinator::with_scanner_and_spawner(complete, move |_task| {
            observed.store(thread_reaper::reserve().is_err(), Ordering::Release);
            Err(std::io::Error::other("injected worker spawn failure"))
        });
        assert!(matches!(result, Err(CoordinatorError::WorkerSpawn(_))));
        assert!(reservation_was_held_at_spawn.load(Ordering::Acquire));
        held.push(thread_reaper::reserve().expect("failed worker spawn releases its permit"));
        assert_eq!(held.len(), 16);
        drop(held);
    }
}
