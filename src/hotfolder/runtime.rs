//! Bounded watcher ingress, event-driven file settling, and ready handoff.
//!
//! This runtime owns only reported-path observation and transfer of settled
//! candidates. It never invokes launcher actions or mutates watched files.

use super::capability::{
    ActionCapability, ActionEnablement, HotFolderCapabilityContext, evaluate_action_capability,
};
use super::model::{
    ConfigurationFingerprint, HotFolderEventKind, HotFolderRule, MAX_DUE_OBSERVATIONS_PER_TURN,
    MAX_EVENT_PATH_BYTES, MAX_EVENT_PATHS, MAX_PENDING_FILES, MAX_READY_CANDIDATES, MAX_RULES,
    MAX_RUNTIME_DIAGNOSTICS, MAX_WATCH_INGRESS_EVENTS, MAX_WATCHED_ROOTS,
    READY_LEASE_TIMEOUT_MS, RuleId, evaluate_rule_conditions,
};
use super::store::HotFolderRulesSnapshot;
use crate::file_operations::{
    NativeFileIdentity, NativeWatchErrorKind, NativeWatchFileObservation, NativeWatchRoot,
    admit_watch_root, observe_watch_file, refresh_watch_root,
};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, VecDeque};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

const MAX_SETTLE_ATTEMPTS: u8 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotFolderDiagnosticKind {
    InvalidRule,
    DuplicateRule,
    CapabilityUnavailable,
    QuarantinedRows,
    RootUnavailable,
    WatcherRegistrationFailed,
    NotifyError,
    LostCoverage,
    ReportedPathRejected,
    PendingCapacity,
    FileBusy,
    FileUnstable,
    ReadyCapacity,
    LeaseExpired,
}

/// A bounded, redacted-by-default runtime diagnostic. The path is retained as
/// an `OsString`-backed path when one was reported; it is never stringified by
/// the service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotFolderDiagnostic {
    pub kind: HotFolderDiagnosticKind,
    pub rule_id: Option<RuleId>,
    pub path: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotFolderLeaseDisposition {
    /// Transfer this evidence snapshot to the next runtime owner.
    TransferToConsumer,
    /// The consumer found that this candidate is stale.
    RejectStale,
    /// The consumer intentionally suppresses this candidate.
    Suppress,
    /// Return the same snapshot to the bounded ready queue for later handling.
    Defer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HotFolderCandidateId(u64);

impl HotFolderCandidateId {
    pub fn get(self) -> u64 {
        self.0
    }

    pub(crate) fn from_persisted(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotFolderFileEvidence {
    pub root_identity: NativeFileIdentity,
    pub parent_identity: NativeFileIdentity,
    pub file_identity: NativeFileIdentity,
    pub length: u64,
    pub modified: SystemTime,
}

#[derive(Clone, Debug)]
pub struct HotFolderReadyCandidate {
    pub correlation: HotFolderCandidateId,
    pub rule: HotFolderRule,
    pub rule_fingerprint: ConfigurationFingerprint,
    pub store_revision: u64,
    pub epoch: u64,
    pub path: PathBuf,
    pub evidence: HotFolderFileEvidence,
    root_id: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HotFolderRuntimeSnapshot {
    pub paused: bool,
    pub active_roots: usize,
    pub active_rules: usize,
    pub pending_files: usize,
    pub ready_candidates: usize,
    pub lease_active: bool,
    pub diagnostics: Vec<HotFolderDiagnostic>,
    pub dropped_diagnostics: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HotFolderReconcileReport {
    pub active_roots: usize,
    pub active_rules: usize,
    pub skipped_rules: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotFolderRuntimeError {
    WorkerStopped,
    StaleEnableRequest,
    ConfirmationRequired,
    WrongServiceLease,
    StaleLease,
    ReadyQueueFull,
}

#[derive(Clone, Debug)]
pub struct StrongActionConfirmation {
    rule_id: RuleId,
    store_revision: u64,
    rule_fingerprint: ConfigurationFingerprint,
}

impl StrongActionConfirmation {
    /// UI code may create this only after its explicit strong-action prompt.
    pub(crate) fn from_explicit_confirmation(
        rule_id: RuleId,
        store_revision: u64,
        rule_fingerprint: ConfigurationFingerprint,
    ) -> Self {
        Self {
            rule_id,
            store_revision,
            rule_fingerprint,
        }
    }
}

#[derive(Clone, Debug)]
pub struct HotFolderEnableRequest {
    rule_id: RuleId,
    store_revision: u64,
    rule_fingerprint: ConfigurationFingerprint,
    strong_confirmation: Option<StrongActionConfirmation>,
}

impl HotFolderEnableRequest {
    pub fn for_rule(snapshot: &HotFolderRulesSnapshot, rule_id: &RuleId) -> Option<Self> {
        let rule = snapshot.rules.iter().find(|rule| &rule.id == rule_id)?;
        let rule_fingerprint = ConfigurationFingerprint::of_serializable(rule).ok()?;
        Some(Self {
            rule_id: rule_id.clone(),
            store_revision: snapshot.revision,
            rule_fingerprint,
            strong_confirmation: None,
        })
    }

    pub(crate) fn with_explicit_strong_confirmation(
        mut self,
        confirmation: StrongActionConfirmation,
    ) -> Self {
        self.strong_confirmation = Some(confirmation);
        self
    }
}

/// A single outstanding ready lease. Dropping the lease returns it to the
/// front of the bounded queue. Finishing with `TransferToConsumer` consumes
/// the B-owned snapshot; no disposition represents execution success.
pub struct HotFolderReadyLease {
    lease_id: u64,
    candidate: HotFolderReadyCandidate,
    output: Weak<Mutex<OutputState>>,
    finished: bool,
}

impl HotFolderReadyLease {
    pub fn candidate(&self) -> &HotFolderReadyCandidate {
        &self.candidate
    }
}

impl Drop for HotFolderReadyLease {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let Some(output) = self.output.upgrade() else {
            return;
        };
        if let Ok(mut output) = output.lock()
            && output
                .leased
                .as_ref()
                .is_some_and(|leased| leased.lease_id == self.lease_id)
        {
            let Some(leased) = output.leased.take() else {
                return;
            };
            if output.ready.len() < MAX_READY_CANDIDATES {
                output.ready.push_front(leased.candidate);
            } else {
                push_diagnostic(
                    &mut output,
                    HotFolderDiagnostic {
                        kind: HotFolderDiagnosticKind::ReadyCapacity,
                        rule_id: Some(leased.candidate.rule.id),
                        path: Some(leased.candidate.path),
                    },
                );
            }
        }
    }
}

trait WatchRegistration: Send {}

struct NativeWatchRegistration(RecommendedWatcher);
impl WatchRegistration for NativeWatchRegistration {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WatchEventKind {
    Created,
    Modified,
    Removed,
    RenamedFrom,
    RenamedTo,
    RenamePair,
    Overflow,
    Error,
    Ignored,
}

#[derive(Clone, Debug)]
struct WatcherNotification {
    kind: WatchEventKind,
    paths: Vec<PathBuf>,
}

type WatcherCallback = Arc<dyn Fn(WatcherNotification) + Send + Sync>;

trait WatcherFactory: Send + Sync {
    fn watch(
        &self,
        root: &Path,
        recursive: bool,
        callback: WatcherCallback,
    ) -> Result<Box<dyn WatchRegistration>, ()>;
}

struct NativeWatcherFactory;

impl WatcherFactory for NativeWatcherFactory {
    fn watch(
        &self,
        root: &Path,
        recursive: bool,
        callback: WatcherCallback,
    ) -> Result<Box<dyn WatchRegistration>, ()> {
        let mut watcher = RecommendedWatcher::new(
            move |result: notify::Result<Event>| callback(notification_from_notify(result)),
            Config::default(),
        )
        .map_err(|_| ())?;
        watcher
            .watch(
                root,
                if recursive {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                },
            )
            .map_err(|_| ())?;
        Ok(Box::new(NativeWatchRegistration(watcher)))
    }
}

trait WatchBoundary: Send + Sync {
    fn admit_root(&self, root: &Path) -> Result<NativeWatchRoot, NativeWatchErrorKind>;
    fn refresh_root(&self, root: &NativeWatchRoot)
    -> Result<NativeWatchRoot, NativeWatchErrorKind>;
    fn observe_file(
        &self,
        root: &NativeWatchRoot,
        path: &Path,
        require_read_lock: bool,
    ) -> Result<NativeWatchFileObservation, NativeWatchErrorKind>;
}

struct NativeWatchBoundary;

impl WatchBoundary for NativeWatchBoundary {
    fn admit_root(&self, root: &Path) -> Result<NativeWatchRoot, NativeWatchErrorKind> {
        admit_watch_root(root).map_err(|error| error.kind())
    }

    fn refresh_root(
        &self,
        root: &NativeWatchRoot,
    ) -> Result<NativeWatchRoot, NativeWatchErrorKind> {
        refresh_watch_root(root).map_err(|error| error.kind())
    }

    fn observe_file(
        &self,
        root: &NativeWatchRoot,
        path: &Path,
        require_read_lock: bool,
    ) -> Result<NativeWatchFileObservation, NativeWatchErrorKind> {
        observe_watch_file(root, path, require_read_lock).map_err(|error| error.kind())
    }
}

struct WorkerHandle {
    sender: SyncSender<WorkerMessage>,
    thread: JoinHandle<()>,
}

/// Lazy managed Hot Folder watcher service. Construction does not create a
/// worker or watcher; `restore_enabled_rules` or `resume_enabled_rules` is the
/// explicit activation boundary.
pub struct HotFolderWatchService {
    worker: Option<WorkerHandle>,
    output: Arc<Mutex<OutputState>>,
    boundary: Arc<dyn WatchBoundary>,
    watcher_factory: Arc<dyn WatcherFactory>,
    clock: Arc<dyn RuntimeClock>,
    shutdown: bool,
    explicitly_paused: bool,
}

impl Default for HotFolderWatchService {
    fn default() -> Self {
        Self::new()
    }
}

impl HotFolderWatchService {
    pub fn new() -> Self {
        Self {
            worker: None,
            output: Arc::new(Mutex::new(OutputState::default())),
            boundary: Arc::new(NativeWatchBoundary),
            watcher_factory: Arc::new(NativeWatcherFactory),
            clock: Arc::new(SystemRuntimeClock),
            shutdown: false,
            explicitly_paused: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_test_ready_candidate(candidate: HotFolderReadyCandidate) -> Self {
        let service = Self::new();
        if let Ok(mut output) = service.output.lock() {
            output.ready.push_back(candidate);
        }
        service
    }

    #[cfg(test)]
    pub(crate) fn push_test_ready_candidate(&self, candidate: HotFolderReadyCandidate) {
        if let Ok(mut output) = self.output.lock()
            && output.ready.len() < MAX_READY_CANDIDATES
        {
            output.ready.push_back(candidate);
        }
    }

    pub fn restore_enabled_rules(
        &mut self,
        snapshot: &HotFolderRulesSnapshot,
        context: &HotFolderCapabilityContext<'_>,
    ) -> Result<HotFolderReconcileReport, HotFolderRuntimeError> {
        self.explicitly_paused = false;
        self.reconcile_with_mode(snapshot, context, ActivationPolicy::AllEnabled)
    }

    /// Reconcile existing active rules after a disable, edit or delete.
    /// Newly enabled or changed rows need an explicit activation request, and
    /// a paused service remains paused until `resume`.
    pub fn reconcile_rules(
        &mut self,
        snapshot: &HotFolderRulesSnapshot,
        context: &HotFolderCapabilityContext<'_>,
    ) -> Result<HotFolderReconcileReport, HotFolderRuntimeError> {
        self.reconcile_with_mode(snapshot, context, ActivationPolicy::ExistingOnly)
    }

    pub fn resume_enabled_rules(
        &mut self,
        snapshot: &HotFolderRulesSnapshot,
        context: &HotFolderCapabilityContext<'_>,
    ) -> Result<HotFolderReconcileReport, HotFolderRuntimeError> {
        self.explicitly_paused = false;
        self.reconcile_with_mode(snapshot, context, ActivationPolicy::AllEnabled)
    }

    /// Activate one just-enabled persisted rule. Potentially destructive rules
    /// require a typed confirmation tied to this exact row and store revision.
    pub fn enable_rule(
        &mut self,
        snapshot: &HotFolderRulesSnapshot,
        request: HotFolderEnableRequest,
        context: &HotFolderCapabilityContext<'_>,
    ) -> Result<HotFolderReconcileReport, HotFolderRuntimeError> {
        if request.store_revision != snapshot.revision {
            return Err(HotFolderRuntimeError::StaleEnableRequest);
        }
        let rule = snapshot
            .rules
            .iter()
            .find(|rule| rule.id == request.rule_id && rule.enabled)
            .ok_or(HotFolderRuntimeError::StaleEnableRequest)?;
        let fingerprint = ConfigurationFingerprint::of_serializable(rule)
            .map_err(|_| HotFolderRuntimeError::StaleEnableRequest)?;
        if fingerprint != request.rule_fingerprint {
            return Err(HotFolderRuntimeError::StaleEnableRequest);
        }
        if matches!(
            evaluate_action_capability(rule, context),
            ActionCapability::Supported {
                enablement: ActionEnablement::StrongConfirmationRequired
            }
        ) && !request
            .strong_confirmation
            .as_ref()
            .is_some_and(|confirmation| {
                confirmation.rule_id == request.rule_id
                    && confirmation.store_revision == request.store_revision
                    && confirmation.rule_fingerprint == request.rule_fingerprint
            })
        {
            return Err(HotFolderRuntimeError::ConfirmationRequired);
        }
        self.reconcile_with_mode(
            snapshot,
            context,
            ActivationPolicy::ExplicitRule {
                rule_id: request.rule_id,
                store_revision: request.store_revision,
                rule_fingerprint: request.rule_fingerprint,
                resume_if_idle: false,
            },
        )
    }

    pub fn pause(&mut self) -> Result<(), HotFolderRuntimeError> {
        self.explicitly_paused = true;
        let Some(worker) = self.worker.as_ref() else {
            return Ok(());
        };
        let (reply_tx, reply_rx) = mpsc::channel();
        worker
            .sender
            .send(WorkerMessage::Pause { reply: reply_tx })
            .map_err(|_| HotFolderRuntimeError::WorkerStopped)?;
        reply_rx
            .recv()
            .map_err(|_| HotFolderRuntimeError::WorkerStopped)
    }

    pub fn try_lease_ready(&self) -> Option<HotFolderReadyLease> {
        let lease = {
            let mut output = self.output.lock().ok()?;
            if output.leased.is_some() {
                return None;
            }
            let candidate = output.ready.pop_front()?;
            let lease_id = output.next_lease_id;
            output.next_lease_id = output.next_lease_id.checked_add(1).unwrap_or(1);
            let deadline = self.clock.now() + Duration::from_millis(READY_LEASE_TIMEOUT_MS);
            output.leased = Some(ActiveLease {
                lease_id,
                candidate: candidate.clone(),
                deadline,
            });
            HotFolderReadyLease {
                lease_id,
                candidate,
                output: Arc::downgrade(&self.output),
                finished: false,
            }
        };
        if let Some(worker) = &self.worker {
            let _ = worker.sender.try_send(WorkerMessage::Wake);
        }
        Some(lease)
    }

    pub fn finish_lease(
        &self,
        mut lease: HotFolderReadyLease,
        disposition: HotFolderLeaseDisposition,
    ) -> Result<Option<HotFolderReadyCandidate>, HotFolderRuntimeError> {
        if !lease.output.ptr_eq(&Arc::downgrade(&self.output)) {
            return Err(HotFolderRuntimeError::WrongServiceLease);
        }
        if let Some(worker) = &self.worker {
            let (reply_tx, reply_rx) = mpsc::channel();
            worker
                .sender
                .send(WorkerMessage::FinishLease {
                    lease_id: lease.lease_id,
                    disposition,
                    reply: reply_tx,
                })
                .map_err(|_| HotFolderRuntimeError::WorkerStopped)?;
            let result = reply_rx
                .recv()
                .map_err(|_| HotFolderRuntimeError::WorkerStopped)?;
            if !matches!(result, Err(HotFolderRuntimeError::ReadyQueueFull)) {
                lease.finished = true;
            }
            return result;
        }
        let result = finish_lease_in_output(
            lease.lease_id,
            disposition,
            &self.output,
            self.clock.now(),
            None,
        )?;
        lease.finished = true;
        Ok(result)
    }

    pub fn snapshot(&self) -> HotFolderRuntimeSnapshot {
        let mut snapshot = HotFolderRuntimeSnapshot {
            paused: true,
            ..HotFolderRuntimeSnapshot::default()
        };
        if let Some(worker) = self.worker.as_ref() {
            let (reply_tx, reply_rx) = mpsc::channel();
            if worker
                .sender
                .send(WorkerMessage::Snapshot { reply: reply_tx })
                .is_ok()
                && let Ok(worker_snapshot) = reply_rx.recv()
            {
                snapshot = worker_snapshot;
            }
        }
        if let Ok(output) = self.output.lock() {
            snapshot.ready_candidates = output.ready.len();
            snapshot.lease_active = output.leased.is_some();
            snapshot.diagnostics = output.diagnostics.iter().cloned().collect();
            snapshot.dropped_diagnostics = output.dropped_diagnostics;
        }
        snapshot
    }

    pub fn shutdown(&mut self) -> Result<(), HotFolderRuntimeError> {
        self.shutdown = true;
        if let Some(worker) = self.worker.take() {
            let _ = worker.sender.send(WorkerMessage::Shutdown);
            worker
                .thread
                .join()
                .map_err(|_| HotFolderRuntimeError::WorkerStopped)?;
        }
        Ok(())
    }

    fn reconcile_with_mode(
        &mut self,
        snapshot: &HotFolderRulesSnapshot,
        context: &HotFolderCapabilityContext<'_>,
        mut activation: ActivationPolicy,
    ) -> Result<HotFolderReconcileReport, HotFolderRuntimeError> {
        let (eligible, skipped) = prepare_eligible_rules(snapshot, context, &self.output);
        if let ActivationPolicy::ExplicitRule {
            ref mut resume_if_idle,
            ..
        } = activation
        {
            *resume_if_idle = self.worker.is_none() && !self.explicitly_paused;
        }
        self.ensure_worker()?;
        let worker = self
            .worker
            .as_ref()
            .ok_or(HotFolderRuntimeError::WorkerStopped)?;
        let (reply_tx, reply_rx) = mpsc::channel();
        worker
            .sender
            .send(WorkerMessage::Reconcile {
                revision: snapshot.revision,
                eligible,
                skipped,
                activation,
                reply: reply_tx,
            })
            .map_err(|_| HotFolderRuntimeError::WorkerStopped)?;
        reply_rx
            .recv()
            .map_err(|_| HotFolderRuntimeError::WorkerStopped)
    }

    fn ensure_worker(&mut self) -> Result<(), HotFolderRuntimeError> {
        if self.shutdown {
            return Err(HotFolderRuntimeError::WorkerStopped);
        }
        if self.worker.is_some() {
            return Ok(());
        }
        let (sender, receiver) = mpsc::sync_channel(MAX_WATCH_INGRESS_EVENTS);
        let ingress_sender = sender.clone();
        let output = Arc::clone(&self.output);
        let boundary = Arc::clone(&self.boundary);
        let watcher_factory = Arc::clone(&self.watcher_factory);
        let clock = Arc::clone(&self.clock);
        let thread = thread::Builder::new()
            .name("hot-folder-watch".to_owned())
            .spawn(move || {
                worker_main(
                    receiver,
                    ingress_sender,
                    output,
                    boundary,
                    watcher_factory,
                    clock,
                )
            })
            .map_err(|_| HotFolderRuntimeError::WorkerStopped)?;
        self.worker = Some(WorkerHandle { sender, thread });
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn test_ready_candidate(
    rule: HotFolderRule,
    path: PathBuf,
    evidence: HotFolderFileEvidence,
    epoch: u64,
    store_revision: u64,
) -> HotFolderReadyCandidate {
    let rule_fingerprint =
        ConfigurationFingerprint::of_serializable(&rule).expect("test rule fingerprint");
    HotFolderReadyCandidate {
        correlation: HotFolderCandidateId(1),
        rule,
        rule_fingerprint,
        store_revision,
        epoch,
        path,
        evidence,
        root_id: 1,
    }
}

trait RuntimeClock: Send + Sync {
    fn now(&self) -> Instant;
}

struct SystemRuntimeClock;

impl RuntimeClock for SystemRuntimeClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

impl Drop for HotFolderWatchService {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

struct OutputState {
    ready: VecDeque<HotFolderReadyCandidate>,
    leased: Option<ActiveLease>,
    next_lease_id: u64,
    diagnostics: VecDeque<HotFolderDiagnostic>,
    dropped_diagnostics: u64,
}

impl Default for OutputState {
    fn default() -> Self {
        Self {
            ready: VecDeque::new(),
            leased: None,
            next_lease_id: 1,
            diagnostics: VecDeque::new(),
            dropped_diagnostics: 0,
        }
    }
}

struct ActiveLease {
    lease_id: u64,
    candidate: HotFolderReadyCandidate,
    deadline: Instant,
}

#[derive(Clone)]
struct EligibleRule {
    rule: HotFolderRule,
    fingerprint: ConfigurationFingerprint,
}

fn prepare_eligible_rules(
    snapshot: &HotFolderRulesSnapshot,
    context: &HotFolderCapabilityContext<'_>,
    output: &Arc<Mutex<OutputState>>,
) -> (Vec<EligibleRule>, usize) {
    let mut eligible = Vec::with_capacity(snapshot.rules.len().min(MAX_RULES));
    let mut seen = std::collections::HashSet::with_capacity(snapshot.rules.len());
    let mut skipped = snapshot.rules.len().saturating_sub(MAX_RULES);
    for (index, rule) in snapshot.rules.iter().take(MAX_RULES).enumerate() {
        if !seen.insert(rule.id.clone()) {
            skipped += 1;
            record_diagnostic(
                output,
                HotFolderDiagnostic {
                    kind: HotFolderDiagnosticKind::DuplicateRule,
                    rule_id: Some(rule.id.clone()),
                    path: None,
                },
            );
            continue;
        }
        if !rule.enabled {
            continue;
        }
        if rule.validate().is_err() {
            skipped += 1;
            record_diagnostic(
                output,
                HotFolderDiagnostic {
                    kind: HotFolderDiagnosticKind::InvalidRule,
                    rule_id: Some(rule.id.clone()),
                    path: Some(rule.watched_root.clone()),
                },
            );
            continue;
        }
        if !matches!(
            evaluate_action_capability(rule, context),
            ActionCapability::Supported { .. }
        ) {
            skipped += 1;
            record_diagnostic(
                output,
                HotFolderDiagnostic {
                    kind: HotFolderDiagnosticKind::CapabilityUnavailable,
                    rule_id: Some(rule.id.clone()),
                    path: Some(rule.watched_root.clone()),
                },
            );
            continue;
        }
        let fingerprint = match ConfigurationFingerprint::of_serializable(rule) {
            Ok(fingerprint) => fingerprint,
            Err(_) => {
                skipped += 1;
                record_diagnostic(
                    output,
                    HotFolderDiagnostic {
                        kind: HotFolderDiagnosticKind::InvalidRule,
                        rule_id: Some(rule.id.clone()),
                        path: Some(rule.watched_root.clone()),
                    },
                );
                continue;
            }
        };
        eligible.push(EligibleRule {
            rule: rule.clone(),
            fingerprint,
        });
    }
    if !snapshot.quarantined_rows.is_empty() {
        record_diagnostic(
            output,
            HotFolderDiagnostic {
                kind: HotFolderDiagnosticKind::QuarantinedRows,
                rule_id: None,
                path: None,
            },
        );
        skipped = skipped.saturating_add(snapshot.quarantined_rows.len());
    }
    (eligible, skipped)
}

fn record_diagnostic(output: &Arc<Mutex<OutputState>>, diagnostic: HotFolderDiagnostic) {
    if let Ok(mut output) = output.lock() {
        push_diagnostic(&mut output, diagnostic);
    }
}

fn push_diagnostic(output: &mut OutputState, diagnostic: HotFolderDiagnostic) {
    if output.diagnostics.len() == MAX_RUNTIME_DIAGNOSTICS {
        output.diagnostics.pop_front();
        output.dropped_diagnostics = output.dropped_diagnostics.saturating_add(1);
    }
    output.diagnostics.push_back(diagnostic);
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
struct PendingKey {
    root_id: u64,
    epoch: u64,
    rule_id: RuleId,
    fingerprint: String,
    path: PathBuf,
}

#[derive(Clone)]
struct PendingFile {
    quiet_deadline: Instant,
    settle_deadline: Instant,
    sample_interval: Duration,
    stable_observations: u8,
    stable_count: u8,
    last_observation: Option<HotFolderFileEvidence>,
    saw_busy: bool,
    attempts: u8,
}

#[derive(Clone)]
struct ActiveRule {
    rule: HotFolderRule,
    fingerprint: ConfigurationFingerprint,
}

struct ActiveRoot {
    id: u64,
    epoch: u64,
    root: NativeWatchRoot,
    recursive: bool,
    store_revision: u64,
    rules: HashMap<RuleId, ActiveRule>,
    _watcher: Box<dyn WatchRegistration>,
    coverage_lost: Arc<AtomicBool>,
}

struct RuntimeCore {
    paused: bool,
    roots: HashMap<NativeFileIdentity, ActiveRoot>,
    blocked_roots: std::collections::HashSet<PathBuf>,
    pending: std::collections::BTreeMap<PendingKey, PendingFile>,
    next_root_id: u64,
    next_epoch: u64,
    next_candidate_id: u64,
}

impl Default for RuntimeCore {
    fn default() -> Self {
        Self {
            paused: true,
            roots: HashMap::new(),
            blocked_roots: std::collections::HashSet::new(),
            pending: std::collections::BTreeMap::new(),
            next_root_id: 1,
            next_epoch: 1,
            next_candidate_id: 1,
        }
    }
}

enum WorkerMessage {
    Reconcile {
        revision: u64,
        eligible: Vec<EligibleRule>,
        skipped: usize,
        activation: ActivationPolicy,
        reply: mpsc::Sender<HotFolderReconcileReport>,
    },
    Pause {
        reply: mpsc::Sender<()>,
    },
    Snapshot {
        reply: mpsc::Sender<HotFolderRuntimeSnapshot>,
    },
    FinishLease {
        lease_id: u64,
        disposition: HotFolderLeaseDisposition,
        reply: mpsc::Sender<Result<Option<HotFolderReadyCandidate>, HotFolderRuntimeError>>,
    },
    Notify {
        root_id: u64,
        epoch: u64,
        notification: WatcherNotification,
    },
    Wake,
    Shutdown,
}

#[derive(Clone)]
enum ActivationPolicy {
    /// Restore persisted enabled intent or explicitly resume the service.
    AllEnabled,
    /// Preserve only rules that are already active with the same config/root.
    ExistingOnly,
    /// Admit one exact rule/config after an explicit enable request.
    ExplicitRule {
        rule_id: RuleId,
        store_revision: u64,
        rule_fingerprint: ConfigurationFingerprint,
        resume_if_idle: bool,
    },
}

fn worker_main(
    receiver: Receiver<WorkerMessage>,
    ingress_sender: SyncSender<WorkerMessage>,
    output: Arc<Mutex<OutputState>>,
    boundary: Arc<dyn WatchBoundary>,
    watcher_factory: Arc<dyn WatcherFactory>,
    clock: Arc<dyn RuntimeClock>,
) {
    let mut core = RuntimeCore::default();
    loop {
        let now = clock.now();
        let deadline = next_deadline(&core, &output);
        let received = match deadline {
            Some(deadline) => receiver.recv_timeout(deadline.saturating_duration_since(now)),
            None => match receiver.recv() {
                Ok(message) => Ok(message),
                Err(_) => return,
            },
        };
        match received {
            Ok(WorkerMessage::Shutdown) => {
                pause_core(&mut core, &output, false);
                return;
            }
            Ok(message) => handle_message(
                message,
                &mut core,
                &ingress_sender,
                &output,
                &boundary,
                &watcher_factory,
                &clock,
            ),
            Err(RecvTimeoutError::Disconnected) => {
                pause_core(&mut core, &output, false);
                return;
            }
            Err(RecvTimeoutError::Timeout) => {
                while let Ok(message) = receiver.try_recv() {
                    if matches!(message, WorkerMessage::Shutdown) {
                        pause_core(&mut core, &output, false);
                        return;
                    }
                    handle_message(
                        message,
                        &mut core,
                        &ingress_sender,
                        &output,
                        &boundary,
                        &watcher_factory,
                        &clock,
                    );
                }
                process_lost_roots(&mut core, &output);
                expire_lease(&output, clock.now());
                process_due(&mut core, &output, &*boundary, clock.now());
            }
        }
    }
}

fn next_deadline(core: &RuntimeCore, output: &Arc<Mutex<OutputState>>) -> Option<Instant> {
    let pending_deadline = core
        .pending
        .values()
        .map(|pending| pending.quiet_deadline.min(pending.settle_deadline))
        .min();
    let lease_deadline = output
        .lock()
        .ok()
        .and_then(|output| output.leased.as_ref().map(|lease| lease.deadline));
    pending_deadline.into_iter().chain(lease_deadline).min()
}

fn handle_message(
    message: WorkerMessage,
    core: &mut RuntimeCore,
    ingress_sender: &SyncSender<WorkerMessage>,
    output: &Arc<Mutex<OutputState>>,
    boundary: &Arc<dyn WatchBoundary>,
    watcher_factory: &Arc<dyn WatcherFactory>,
    clock: &Arc<dyn RuntimeClock>,
) {
    match message {
        WorkerMessage::Reconcile {
            revision,
            eligible,
            skipped,
            activation,
            reply,
        } => {
            let resume_if_idle = matches!(
                &activation,
                ActivationPolicy::ExplicitRule {
                    resume_if_idle: true,
                    ..
                }
            );
            if matches!(&activation, ActivationPolicy::AllEnabled) || resume_if_idle {
                core.paused = false;
            }
            if matches!(&activation, ActivationPolicy::AllEnabled) {
                core.blocked_roots.clear();
            }
            let report = reconcile_core(
                core,
                revision,
                eligible,
                skipped,
                activation,
                ingress_sender,
                output,
                boundary,
                watcher_factory,
            );
            let _ = reply.send(report);
        }
        WorkerMessage::Pause { reply } => {
            pause_core(core, output, true);
            let _ = reply.send(());
        }
        WorkerMessage::Snapshot { reply } => {
            let mut snapshot = HotFolderRuntimeSnapshot {
                paused: core.paused,
                active_roots: core.roots.len(),
                active_rules: core.roots.values().map(|root| root.rules.len()).sum(),
                pending_files: core.pending.len(),
                ..HotFolderRuntimeSnapshot::default()
            };
            if let Ok(output) = output.lock() {
                snapshot.ready_candidates = output.ready.len();
                snapshot.lease_active = output.leased.is_some();
                snapshot.diagnostics = output.diagnostics.iter().cloned().collect();
                snapshot.dropped_diagnostics = output.dropped_diagnostics;
            }
            let _ = reply.send(snapshot);
        }
        WorkerMessage::FinishLease {
            lease_id,
            disposition,
            reply,
        } => {
            let result =
                finish_lease_in_output(lease_id, disposition, output, clock.now(), Some(core));
            let _ = reply.send(result);
        }
        WorkerMessage::Notify {
            root_id,
            epoch,
            notification,
        } => handle_notification(root_id, epoch, notification, core, output, clock.now()),
        WorkerMessage::Wake => {}
        WorkerMessage::Shutdown => {}
    }
    process_lost_roots(core, output);
    expire_lease(output, clock.now());
    process_due(core, output, &**boundary, clock.now());
}

fn reconcile_core(
    core: &mut RuntimeCore,
    revision: u64,
    eligible: Vec<EligibleRule>,
    skipped: usize,
    activation: ActivationPolicy,
    ingress_sender: &SyncSender<WorkerMessage>,
    output: &Arc<Mutex<OutputState>>,
    boundary: &Arc<dyn WatchBoundary>,
    watcher_factory: &Arc<dyn WatcherFactory>,
) -> HotFolderReconcileReport {
    if core.paused {
        core.roots.clear();
        core.pending.clear();
        return HotFolderReconcileReport {
            skipped_rules: skipped,
            ..HotFolderReconcileReport::default()
        };
    }
    let configured_roots = eligible
        .iter()
        .map(|rule| rule.rule.watched_root.clone())
        .collect::<std::collections::HashSet<_>>();
    core.blocked_roots
        .retain(|blocked| configured_roots.contains(blocked));
    let mut groups: HashMap<NativeFileIdentity, (NativeWatchRoot, Vec<EligibleRule>)> =
        HashMap::new();
    let mut skipped = skipped;
    for eligible in eligible {
        let path = eligible.rule.watched_root.clone();
        if core.blocked_roots.contains(&path) {
            skipped = skipped.saturating_add(1);
            continue;
        }
        let root = match boundary.admit_root(&path) {
            Ok(root) => root,
            Err(_) => {
                skipped = skipped.saturating_add(1);
                record_diagnostic(
                    output,
                    HotFolderDiagnostic {
                        kind: HotFolderDiagnosticKind::RootUnavailable,
                        rule_id: Some(eligible.rule.id.clone()),
                        path: Some(path),
                    },
                );
                continue;
            }
        };
        groups
            .entry(root.identity())
            .or_insert_with(|| (root.clone(), Vec::new()))
            .1
            .push(eligible);
    }
    let mut old_roots = std::mem::take(&mut core.roots);
    let mut next_roots = HashMap::with_capacity(groups.len().min(MAX_WATCHED_ROOTS));
    let mut report = HotFolderReconcileReport {
        skipped_rules: skipped,
        ..HotFolderReconcileReport::default()
    };
    let mut group_ids = groups.keys().copied().collect::<Vec<_>>();
    group_ids.sort_by(|left, right| {
        let left_root = &groups[left].0;
        let right_root = &groups[right].0;
        left_root
            .canonical_root()
            .cmp(right_root.canonical_root())
            .then_with(|| {
                left.volume_serial_number()
                    .cmp(&right.volume_serial_number())
            })
            .then_with(|| left.file_id().cmp(&right.file_id()))
    });
    for identity in group_ids {
        let Some((root, mut rules)) = groups.remove(&identity) else {
            continue;
        };
        rules.sort_by(|left, right| left.rule.id.cmp(&right.rule.id));
        if next_roots.len() >= MAX_WATCHED_ROOTS {
            report.skipped_rules = report.skipped_rules.saturating_add(rules.len());
            push_diagnostic(
                &mut output
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
                HotFolderDiagnostic {
                    kind: HotFolderDiagnosticKind::RootUnavailable,
                    rule_id: None,
                    path: Some(root.canonical_root().to_path_buf()),
                },
            );
            continue;
        }
        let same_root_identity = old_roots.contains_key(&identity);
        let requested_count = rules.len();
        rules.retain(|rule| match &activation {
            ActivationPolicy::AllEnabled => true,
            ActivationPolicy::ExistingOnly => {
                same_root_identity
                    && old_roots.get(&identity).is_some_and(|old| {
                        old.rules
                            .get(&rule.rule.id)
                            .is_some_and(|active| active.fingerprint == rule.fingerprint)
                    })
            }
            ActivationPolicy::ExplicitRule {
                rule_id,
                store_revision,
                rule_fingerprint,
                ..
            } => {
                let explicitly_enabled = rule.rule.id == *rule_id
                    && rule.fingerprint == *rule_fingerprint
                    && *store_revision == revision;
                explicitly_enabled
                    || (same_root_identity
                        && old_roots.get(&identity).is_some_and(|old| {
                            old.rules
                                .get(&rule.rule.id)
                                .is_some_and(|active| active.fingerprint == rule.fingerprint)
                        }))
            }
        });
        report.skipped_rules = report
            .skipped_rules
            .saturating_add(requested_count.saturating_sub(rules.len()));
        if rules.is_empty() {
            old_roots.remove(&identity);
            continue;
        }
        let recursive = rules.iter().any(|rule| rule.rule.include_subfolders);
        let mut active_rules = HashMap::with_capacity(rules.len());
        for rule in rules {
            active_rules.insert(
                rule.rule.id.clone(),
                ActiveRule {
                    rule: rule.rule,
                    fingerprint: rule.fingerprint,
                },
            );
        }
        if let Some(mut old) = old_roots.remove(&identity)
            && active_root_matches(&old, &root, recursive, &active_rules)
        {
            old.root = root;
            old.store_revision = revision;
            old.rules = active_rules;
            report.active_rules += old.rules.len();
            report.active_roots += 1;
            next_roots.insert(identity, old);
            continue;
        }
        let Some((root_id, epoch)) = allocate_epoch(core) else {
            report.skipped_rules = report.skipped_rules.saturating_add(active_rules.len());
            push_diagnostic(
                &mut output
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
                HotFolderDiagnostic {
                    kind: HotFolderDiagnosticKind::LostCoverage,
                    rule_id: None,
                    path: Some(root.canonical_root().to_path_buf()),
                },
            );
            continue;
        };
        let coverage_lost = Arc::new(AtomicBool::new(false));
        let callback = make_watcher_callback(
            ingress_sender.clone(),
            root_id,
            epoch,
            Arc::clone(&coverage_lost),
        );
        let watcher = match watcher_factory.watch(root.canonical_root(), recursive, callback) {
            Ok(watcher) => watcher,
            Err(()) => {
                report.skipped_rules = report.skipped_rules.saturating_add(active_rules.len());
                push_diagnostic(
                    &mut output
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner),
                    HotFolderDiagnostic {
                        kind: HotFolderDiagnosticKind::WatcherRegistrationFailed,
                        rule_id: None,
                        path: Some(root.canonical_root().to_path_buf()),
                    },
                );
                continue;
            }
        };
        report.active_rules += active_rules.len();
        report.active_roots += 1;
        next_roots.insert(
            identity,
            ActiveRoot {
                id: root_id,
                epoch,
                root,
                recursive,
                store_revision: revision,
                rules: active_rules,
                _watcher: watcher,
                coverage_lost,
            },
        );
    }
    drop(old_roots);
    core.roots = next_roots;
    let current_rules = core
        .roots
        .values()
        .flat_map(|root| {
            root.rules.values().map(move |rule| {
                (
                    root.id,
                    root.epoch,
                    rule.rule.id.clone(),
                    rule.fingerprint.as_str().to_owned(),
                )
            })
        })
        .collect::<std::collections::HashSet<_>>();
    core.pending.retain(|key, _| {
        current_rules.contains(&(
            key.root_id,
            key.epoch,
            key.rule_id.clone(),
            key.fingerprint.clone(),
        ))
    });
    prune_ready_candidates(core, output);
    report
}

fn allocate_epoch(core: &mut RuntimeCore) -> Option<(u64, u64)> {
    let root_id = core.next_root_id;
    let epoch = core.next_epoch;
    core.next_root_id = root_id.checked_add(1)?;
    core.next_epoch = epoch.checked_add(1)?;
    Some((root_id, epoch))
}

fn active_root_matches(
    active: &ActiveRoot,
    root: &NativeWatchRoot,
    recursive: bool,
    rules: &HashMap<RuleId, ActiveRule>,
) -> bool {
    active.root.identity() == root.identity()
        && paths_equivalent(active.root.canonical_root(), root.canonical_root())
        && active.recursive == recursive
        && active.rules.len() == rules.len()
        && active.rules.iter().all(|(id, active_rule)| {
            rules
                .get(id)
                .is_some_and(|rule| rule.fingerprint == active_rule.fingerprint)
        })
}

fn prune_ready_candidates(core: &RuntimeCore, output: &Arc<Mutex<OutputState>>) {
    let Ok(mut output) = output.lock() else {
        return;
    };
    output
        .ready
        .retain(|candidate| candidate_is_current(core, candidate));
    if output
        .leased
        .as_ref()
        .is_some_and(|lease| !candidate_is_current(core, &lease.candidate))
    {
        if let Some(lease) = output.leased.take() {
            push_diagnostic(
                &mut output,
                HotFolderDiagnostic {
                    kind: HotFolderDiagnosticKind::LeaseExpired,
                    rule_id: Some(lease.candidate.rule.id),
                    path: Some(lease.candidate.path),
                },
            );
        }
    }
}

fn candidate_is_current(core: &RuntimeCore, candidate: &HotFolderReadyCandidate) -> bool {
    core.roots.values().any(|root| {
        root.id == candidate.root_id
            && root.epoch == candidate.epoch
            && root
                .rules
                .get(&candidate.rule.id)
                .is_some_and(|rule| rule.fingerprint == candidate.rule_fingerprint)
    })
}

fn finish_lease_in_output(
    lease_id: u64,
    disposition: HotFolderLeaseDisposition,
    output: &Arc<Mutex<OutputState>>,
    now: Instant,
    core: Option<&RuntimeCore>,
) -> Result<Option<HotFolderReadyCandidate>, HotFolderRuntimeError> {
    let mut output = output
        .lock()
        .map_err(|_| HotFolderRuntimeError::WorkerStopped)?;
    expire_lease_locked(&mut output, now);
    let Some(active) = output.leased.as_ref() else {
        return Err(HotFolderRuntimeError::StaleLease);
    };
    if active.lease_id != lease_id {
        return Err(HotFolderRuntimeError::StaleLease);
    }
    if core.is_some_and(|core| !candidate_is_current(core, &active.candidate)) {
        if let Some(stale) = output.leased.take() {
            push_diagnostic(
                &mut output,
                HotFolderDiagnostic {
                    kind: HotFolderDiagnosticKind::LeaseExpired,
                    rule_id: Some(stale.candidate.rule.id),
                    path: Some(stale.candidate.path),
                },
            );
        }
        return Err(HotFolderRuntimeError::StaleLease);
    }
    if disposition == HotFolderLeaseDisposition::Defer && output.ready.len() >= MAX_READY_CANDIDATES
    {
        return Err(HotFolderRuntimeError::ReadyQueueFull);
    }
    let Some(active) = output.leased.take() else {
        return Err(HotFolderRuntimeError::StaleLease);
    };
    match disposition {
        HotFolderLeaseDisposition::TransferToConsumer => Ok(Some(active.candidate)),
        HotFolderLeaseDisposition::RejectStale | HotFolderLeaseDisposition::Suppress => Ok(None),
        HotFolderLeaseDisposition::Defer => {
            output.ready.push_front(active.candidate);
            Ok(None)
        }
    }
}

fn pause_core(core: &mut RuntimeCore, output: &Arc<Mutex<OutputState>>, record: bool) {
    core.paused = true;
    core.roots.clear();
    core.blocked_roots.clear();
    core.pending.clear();
    if let Ok(mut output) = output.lock() {
        output.ready.clear();
        output.leased = None;
        if record {
            push_diagnostic(
                &mut output,
                HotFolderDiagnostic {
                    kind: HotFolderDiagnosticKind::LostCoverage,
                    rule_id: None,
                    path: None,
                },
            );
        }
    }
}

fn make_watcher_callback(
    sender: SyncSender<WorkerMessage>,
    root_id: u64,
    epoch: u64,
    coverage_lost: Arc<AtomicBool>,
) -> WatcherCallback {
    Arc::new(move |notification| {
        let message = WorkerMessage::Notify {
            root_id,
            epoch,
            notification,
        };
        match sender.try_send(message) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                coverage_lost.store(true, Ordering::Release);
            }
        }
    })
}

fn notification_from_notify(result: notify::Result<Event>) -> WatcherNotification {
    let event = match result {
        Ok(event) => event,
        Err(_) => {
            return WatcherNotification {
                kind: WatchEventKind::Error,
                paths: Vec::new(),
            };
        }
    };
    if event.need_rescan() {
        return WatcherNotification {
            kind: WatchEventKind::Overflow,
            paths: Vec::new(),
        };
    }
    use notify::event::{CreateKind, DataChange, ModifyKind, RenameMode};
    let kind = match event.kind {
        EventKind::Create(CreateKind::Any | CreateKind::File) => WatchEventKind::Created,
        EventKind::Create(_) => WatchEventKind::Created,
        EventKind::Modify(ModifyKind::Data(
            DataChange::Any | DataChange::Content | DataChange::Size,
        ))
        | EventKind::Modify(ModifyKind::Any) => WatchEventKind::Modified,
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => WatchEventKind::RenamedFrom,
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => WatchEventKind::RenamedTo,
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => WatchEventKind::RenamePair,
        EventKind::Modify(ModifyKind::Name(_)) => WatchEventKind::Overflow,
        EventKind::Remove(_) => WatchEventKind::Removed,
        _ => {
            return WatcherNotification {
                kind: WatchEventKind::Ignored,
                paths: Vec::new(),
            };
        }
    };
    if kind == WatchEventKind::Overflow {
        return WatcherNotification {
            kind,
            paths: Vec::new(),
        };
    }
    let required_paths = if kind == WatchEventKind::RenamePair {
        2
    } else {
        1
    };
    if event.paths.len() < required_paths || event.paths.len() > MAX_EVENT_PATHS {
        return WatcherNotification {
            kind: WatchEventKind::Overflow,
            paths: Vec::new(),
        };
    }
    let mut paths = Vec::with_capacity(event.paths.len().min(MAX_EVENT_PATHS));
    for path in event.paths.iter().take(MAX_EVENT_PATHS) {
        if path.as_os_str().len() > MAX_EVENT_PATH_BYTES {
            return WatcherNotification {
                kind: WatchEventKind::Overflow,
                paths: Vec::new(),
            };
        }
        paths.push(path.clone());
    }
    if kind == WatchEventKind::RenamePair && paths.len() != 2 {
        return WatcherNotification {
            kind: WatchEventKind::Overflow,
            paths: Vec::new(),
        };
    }
    WatcherNotification { kind, paths }
}

fn handle_notification(
    root_id: u64,
    epoch: u64,
    notification: WatcherNotification,
    core: &mut RuntimeCore,
    output: &Arc<Mutex<OutputState>>,
    now: Instant,
) {
    if core.paused {
        return;
    }
    let Some(root_key) = core
        .roots
        .iter()
        .find_map(|(key, root)| (root.id == root_id && root.epoch == epoch).then_some(*key))
    else {
        return;
    };
    if notification.kind == WatchEventKind::Overflow {
        pause_root(
            core,
            root_key,
            output,
            HotFolderDiagnosticKind::LostCoverage,
            None,
        );
        return;
    }
    if notification.kind == WatchEventKind::Error {
        pause_root(
            core,
            root_key,
            output,
            HotFolderDiagnosticKind::NotifyError,
            None,
        );
        return;
    }
    let Some(active_root) = core.roots.get(&root_key) else {
        return;
    };
    let mut root_paths = vec![
        active_root.root.canonical_root().to_path_buf(),
        active_root.root.selected_root().to_path_buf(),
    ];
    root_paths.extend(
        active_root
            .rules
            .values()
            .map(|rule| rule.rule.watched_root.clone()),
    );
    let root_path_reported = |path: &Path| {
        root_paths
            .iter()
            .any(|root_path| paths_equivalent(path, root_path))
    };
    let root_evidence = active_root.root.clone();
    let active_rules = active_root.rules.values().cloned().collect::<Vec<_>>();
    match notification.kind {
        WatchEventKind::Created | WatchEventKind::Modified | WatchEventKind::RenamedTo => {
            for path in notification.paths {
                handle_arrival(
                    root_id,
                    epoch,
                    root_key,
                    &root_evidence,
                    &active_rules,
                    path,
                    notification.kind,
                    core,
                    output,
                    now,
                );
            }
        }
        WatchEventKind::Removed | WatchEventKind::RenamedFrom => {
            if let Some(path) = notification
                .paths
                .iter()
                .find(|path| root_path_reported(path))
            {
                pause_root(
                    core,
                    root_key,
                    output,
                    HotFolderDiagnosticKind::RootUnavailable,
                    Some(path.clone()),
                );
                return;
            }
            for path in notification.paths {
                retire_path_pending(root_id, epoch, &active_rules, &path, core);
            }
        }
        WatchEventKind::RenamePair => {
            if notification.paths.len() != 2 {
                pause_root(
                    core,
                    root_key,
                    output,
                    HotFolderDiagnosticKind::LostCoverage,
                    None,
                );
                return;
            }
            if let Some(path) = notification
                .paths
                .iter()
                .find(|path| root_path_reported(path))
            {
                pause_root(
                    core,
                    root_key,
                    output,
                    HotFolderDiagnosticKind::RootUnavailable,
                    Some(path.clone()),
                );
                return;
            }
            retire_path_pending(root_id, epoch, &active_rules, &notification.paths[0], core);
            handle_arrival(
                root_id,
                epoch,
                root_key,
                &root_evidence,
                &active_rules,
                notification.paths[1].clone(),
                WatchEventKind::RenamedTo,
                core,
                output,
                now,
            );
        }
        WatchEventKind::Overflow | WatchEventKind::Error | WatchEventKind::Ignored => {}
    }
}

fn handle_arrival(
    root_id: u64,
    epoch: u64,
    root_key: NativeFileIdentity,
    root: &NativeWatchRoot,
    rules: &[ActiveRule],
    path: PathBuf,
    event_kind: WatchEventKind,
    core: &mut RuntimeCore,
    output: &Arc<Mutex<OutputState>>,
    now: Instant,
) {
    if path.as_os_str().len() > MAX_EVENT_PATH_BYTES {
        pause_root(
            core,
            root_key,
            output,
            HotFolderDiagnosticKind::LostCoverage,
            Some(path),
        );
        return;
    }
    let Some(relative) = relative_path_beneath(root.canonical_root(), &path) else {
        record_diagnostic(
            output,
            HotFolderDiagnostic {
                kind: HotFolderDiagnosticKind::ReportedPathRejected,
                rule_id: None,
                path: Some(path),
            },
        );
        return;
    };
    let Some(event) = (match event_kind {
        WatchEventKind::Created => Some(HotFolderEventKind::Created),
        WatchEventKind::Modified => Some(HotFolderEventKind::Modified),
        WatchEventKind::RenamedTo => Some(HotFolderEventKind::RenamedTo),
        WatchEventKind::RenamedFrom => Some(HotFolderEventKind::RenamedFrom),
        WatchEventKind::Removed => Some(HotFolderEventKind::Removed),
        WatchEventKind::RenamePair
        | WatchEventKind::Overflow
        | WatchEventKind::Error
        | WatchEventKind::Ignored => None,
    }) else {
        return;
    };
    for active_rule in rules {
        let rule = &active_rule.rule;
        if evaluate_rule_conditions(rule, event, &relative).is_err() {
            continue;
        }
        let candidate_key = PendingKey {
            root_id,
            epoch,
            rule_id: rule.id.clone(),
            fingerprint: active_rule.fingerprint.as_str().to_owned(),
            path: path.clone(),
        };
        let existing_key = core
            .pending
            .keys()
            .find(|key| {
                key.root_id == candidate_key.root_id
                    && key.epoch == candidate_key.epoch
                    && key.rule_id == candidate_key.rule_id
                    && key.fingerprint == candidate_key.fingerprint
                    && paths_equivalent(&key.path, &candidate_key.path)
            })
            .cloned();
        let sample_interval = Duration::from_millis(rule.settle.sample_interval_ms);
        let timeout = Duration::from_millis(rule.settle.timeout_ms);
        if let Some(key) = existing_key
            && let Some(pending) = core.pending.get_mut(&key)
        {
            pending.quiet_deadline = now + sample_interval;
            pending.stable_count = 0;
            pending.last_observation = None;
            pending.attempts = 0;
            continue;
        }
        if core.pending.len() >= MAX_PENDING_FILES {
            pause_root(
                core,
                root_key,
                output,
                HotFolderDiagnosticKind::PendingCapacity,
                Some(path),
            );
            return;
        }
        core.pending.insert(
            candidate_key,
            PendingFile {
                quiet_deadline: now + sample_interval,
                settle_deadline: now + timeout,
                sample_interval,
                stable_observations: rule.settle.stable_observations,
                stable_count: 0,
                last_observation: None,
                saw_busy: false,
                attempts: 0,
            },
        );
    }
}

fn retire_path_pending(
    root_id: u64,
    epoch: u64,
    rules: &[ActiveRule],
    path: &Path,
    core: &mut RuntimeCore,
) {
    for rule in rules {
        let fingerprint = rule.fingerprint.as_str();
        core.pending.retain(|key, _| {
            !(key.root_id == root_id
                && key.epoch == epoch
                && key.rule_id == rule.rule.id
                && key.fingerprint == fingerprint
                && paths_equivalent(&key.path, path))
        });
    }
}

fn process_lost_roots(core: &mut RuntimeCore, output: &Arc<Mutex<OutputState>>) {
    let lost = core
        .roots
        .iter()
        .filter_map(|(key, root)| {
            root.coverage_lost
                .swap(false, Ordering::AcqRel)
                .then_some(*key)
        })
        .collect::<Vec<_>>();
    for root_key in lost {
        pause_root(
            core,
            root_key,
            output,
            HotFolderDiagnosticKind::LostCoverage,
            None,
        );
    }
}

fn pause_root(
    core: &mut RuntimeCore,
    root_key: NativeFileIdentity,
    output: &Arc<Mutex<OutputState>>,
    diagnostic_kind: HotFolderDiagnosticKind,
    path: Option<PathBuf>,
) {
    let Some(root) = core.roots.remove(&root_key) else {
        return;
    };
    let root_id = root.id;
    core.blocked_roots.extend(
        root.rules
            .values()
            .map(|rule| rule.rule.watched_root.clone()),
    );
    core.pending
        .retain(|key, _| key.root_id != root_id || key.epoch != root.epoch);
    let diagnostic_path = path.or_else(|| Some(root.root.canonical_root().to_path_buf()));
    drop(root);
    if let Ok(mut output) = output.lock() {
        output
            .ready
            .retain(|candidate| candidate.root_id != root_id);
        if output
            .leased
            .as_ref()
            .is_some_and(|lease| lease.candidate.root_id == root_id)
        {
            output.leased = None;
        }
        push_diagnostic(
            &mut output,
            HotFolderDiagnostic {
                kind: diagnostic_kind,
                rule_id: None,
                path: diagnostic_path,
            },
        );
    }
}

fn expire_lease(output: &Arc<Mutex<OutputState>>, now: Instant) {
    let Ok(mut output) = output.lock() else {
        return;
    };
    expire_lease_locked(&mut output, now);
}

fn expire_lease_locked(output: &mut OutputState, now: Instant) {
    if !output
        .leased
        .as_ref()
        .is_some_and(|lease| lease.deadline <= now)
    {
        return;
    }
    let Some(lease) = output.leased.take() else {
        return;
    };
    let diagnostic = HotFolderDiagnostic {
        kind: HotFolderDiagnosticKind::LeaseExpired,
        rule_id: Some(lease.candidate.rule.id.clone()),
        path: Some(lease.candidate.path.clone()),
    };
    if output.ready.len() < MAX_READY_CANDIDATES {
        output.ready.push_front(lease.candidate);
    } else {
        push_diagnostic(
            output,
            HotFolderDiagnostic {
                kind: HotFolderDiagnosticKind::ReadyCapacity,
                ..diagnostic.clone()
            },
        );
    }
    push_diagnostic(output, diagnostic);
}

fn process_due(
    core: &mut RuntimeCore,
    output: &Arc<Mutex<OutputState>>,
    boundary: &dyn WatchBoundary,
    now: Instant,
) {
    if core.paused {
        return;
    }
    let due = core
        .pending
        .iter()
        .filter(|(_, pending)| pending.quiet_deadline <= now || pending.settle_deadline <= now)
        .take(MAX_DUE_OBSERVATIONS_PER_TURN)
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    for key in due {
        let Some(mut pending) = core.pending.remove(&key) else {
            continue;
        };
        let current_root = core.roots.iter().find_map(|(root_key, root)| {
            (root.id == key.root_id && root.epoch == key.epoch).then(|| {
                (
                    *root_key,
                    root.root.clone(),
                    root.store_revision,
                    root.rules.get(&key.rule_id).cloned(),
                )
            })
        });
        let Some((root_key, root, store_revision, Some(active_rule))) = current_root else {
            continue;
        };
        if active_rule.fingerprint.as_str() != key.fingerprint {
            continue;
        }
        if (now >= pending.settle_deadline && pending.quiet_deadline > now)
            || pending.attempts >= MAX_SETTLE_ATTEMPTS
        {
            record_diagnostic(
                output,
                HotFolderDiagnostic {
                    kind: if pending.saw_busy {
                        HotFolderDiagnosticKind::FileBusy
                    } else {
                        HotFolderDiagnosticKind::FileUnstable
                    },
                    rule_id: Some(key.rule_id),
                    path: Some(key.path),
                },
            );
            continue;
        }
        pending.attempts = pending.attempts.saturating_add(1);
        match boundary.observe_file(&root, &key.path, true) {
            Ok(observation) => {
                if observation.root_identity() != root.identity() {
                    pause_root(
                        core,
                        root_key,
                        output,
                        HotFolderDiagnosticKind::LostCoverage,
                        Some(key.path),
                    );
                    continue;
                }
                let observed_path = observation.canonical_path().to_path_buf();
                if relative_path_beneath(root.canonical_root(), &observed_path).is_none()
                    || !paths_equivalent(&observed_path, &key.path)
                {
                    record_diagnostic(
                        output,
                        HotFolderDiagnostic {
                            kind: HotFolderDiagnosticKind::ReportedPathRejected,
                            rule_id: Some(key.rule_id),
                            path: Some(key.path),
                        },
                    );
                    continue;
                }
                let evidence = HotFolderFileEvidence {
                    root_identity: observation.root_identity(),
                    parent_identity: observation.parent_identity(),
                    file_identity: observation.file_identity(),
                    length: observation.length(),
                    modified: observation.modified(),
                };
                if pending.last_observation == Some(evidence) {
                    pending.stable_count = pending.stable_count.saturating_add(1);
                } else {
                    pending.stable_count = 1;
                }
                pending.last_observation = Some(evidence);
                if pending.stable_count >= pending.stable_observations {
                    let Some(correlation) = allocate_candidate_id(core) else {
                        pause_root(
                            core,
                            root_key,
                            output,
                            HotFolderDiagnosticKind::LostCoverage,
                            Some(observed_path),
                        );
                        continue;
                    };
                    let candidate = HotFolderReadyCandidate {
                        correlation,
                        rule: active_rule.rule,
                        rule_fingerprint: active_rule.fingerprint,
                        store_revision,
                        epoch: key.epoch,
                        path: key.path.clone(),
                        evidence,
                        root_id: key.root_id,
                    };
                    let enqueued = if let Ok(mut output) = output.lock() {
                        let outstanding = output.ready.len() + usize::from(output.leased.is_some());
                        if outstanding < MAX_READY_CANDIDATES {
                            output.ready.push_back(candidate);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    if !enqueued {
                        pause_root(
                            core,
                            root_key,
                            output,
                            HotFolderDiagnosticKind::ReadyCapacity,
                            Some(key.path),
                        );
                    }
                    continue;
                }
                pending.quiet_deadline = now + pending.sample_interval;
                core.pending.insert(key, pending);
            }
            Err(error) => match error {
                NativeWatchErrorKind::RootChanged | NativeWatchErrorKind::InvalidRoot => {
                    pause_root(
                        core,
                        root_key,
                        output,
                        HotFolderDiagnosticKind::LostCoverage,
                        Some(key.path),
                    );
                }
                NativeWatchErrorKind::OutsideRoot
                | NativeWatchErrorKind::InvalidPath
                | NativeWatchErrorKind::ParentChanged
                | NativeWatchErrorKind::NotRegularFile
                | NativeWatchErrorKind::ReparsePoint
                | NativeWatchErrorKind::UnsupportedIdentity
                | NativeWatchErrorKind::Unsupported
                | NativeWatchErrorKind::Missing => {
                    record_diagnostic(
                        output,
                        HotFolderDiagnostic {
                            kind: HotFolderDiagnosticKind::ReportedPathRejected,
                            rule_id: Some(key.rule_id),
                            path: Some(key.path),
                        },
                    );
                }
                NativeWatchErrorKind::Busy | NativeWatchErrorKind::Inaccessible => {
                    pending.saw_busy |= error == NativeWatchErrorKind::Busy;
                    if now + pending.sample_interval > pending.settle_deadline
                        || pending.attempts >= MAX_SETTLE_ATTEMPTS
                    {
                        record_diagnostic(
                            output,
                            HotFolderDiagnostic {
                                kind: if pending.saw_busy {
                                    HotFolderDiagnosticKind::FileBusy
                                } else {
                                    HotFolderDiagnosticKind::FileUnstable
                                },
                                rule_id: Some(key.rule_id),
                                path: Some(key.path),
                            },
                        );
                    } else {
                        pending.quiet_deadline = now + pending.sample_interval;
                        core.pending.insert(key, pending);
                    }
                }
            },
        }
    }
}

fn allocate_candidate_id(core: &mut RuntimeCore) -> Option<HotFolderCandidateId> {
    let id = core.next_candidate_id;
    core.next_candidate_id = id.checked_add(1)?;
    Some(HotFolderCandidateId(id))
}

pub(crate) fn relative_path_beneath(root: &Path, path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let root_components = root.components().collect::<Vec<_>>();
    let path_components = path.components().collect::<Vec<_>>();
    if path_components.len() <= root_components.len()
        || !root_components
            .iter()
            .zip(path_components.iter())
            .all(|(root, path)| {
                crate::batch_rename::compare_paths_windows_ordinal(
                    Path::new(root.as_os_str()),
                    Path::new(path.as_os_str()),
                ) == std::cmp::Ordering::Equal
            })
    {
        return None;
    }
    let mut relative = PathBuf::new();
    for component in path_components.iter().skip(root_components.len()) {
        match component {
            Component::Normal(name) => relative.push(name),
            _ => return None,
        }
    }
    Some(relative)
}

fn paths_equivalent(left: &Path, right: &Path) -> bool {
    crate::batch_rename::compare_paths_windows_ordinal(left, right) == std::cmp::Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_operations::{
        NativeFileIdentity, test_native_file_identity, test_native_watch_file_observation,
        test_native_watch_root,
    };
    use crate::hotfolder::model::{RuleAction, SettlePolicy};
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::sync::atomic::{AtomicU64, AtomicUsize};

    struct FakeClock {
        base: Instant,
        elapsed_ms: AtomicU64,
    }

    impl FakeClock {
        fn new() -> Self {
            Self {
                base: Instant::now(),
                elapsed_ms: AtomicU64::new(0),
            }
        }

        fn set_ms(&self, elapsed_ms: u64) {
            self.elapsed_ms.store(elapsed_ms, Ordering::Release);
        }
    }

    impl RuntimeClock for FakeClock {
        fn now(&self) -> Instant {
            self.base + Duration::from_millis(self.elapsed_ms.load(Ordering::Acquire))
        }
    }

    struct FakeWatchBoundary {
        roots: HashMap<PathBuf, NativeWatchRoot>,
        observations: Mutex<VecDeque<Result<NativeWatchFileObservation, NativeWatchErrorKind>>>,
        observation_count: AtomicUsize,
    }

    impl FakeWatchBoundary {
        fn new(
            roots: impl IntoIterator<Item = PathBuf>,
            observations: Vec<Result<NativeWatchFileObservation, NativeWatchErrorKind>>,
        ) -> Self {
            let roots = roots
                .into_iter()
                .map(|path| {
                    let identity = fake_root_identity(&path);
                    (
                        path.clone(),
                        test_native_watch_root(
                            &path,
                            identity.volume_serial_number(),
                            identity.file_id(),
                        ),
                    )
                })
                .collect();
            Self {
                roots,
                observations: Mutex::new(observations.into()),
                observation_count: AtomicUsize::new(0),
            }
        }

        fn root(&self, path: &Path) -> NativeWatchRoot {
            self.roots.get(path).expect("test root exists").clone()
        }

        fn observation_count(&self) -> usize {
            self.observation_count.load(Ordering::Acquire)
        }
    }

    impl WatchBoundary for FakeWatchBoundary {
        fn admit_root(&self, root: &Path) -> Result<NativeWatchRoot, NativeWatchErrorKind> {
            self.roots
                .get(root)
                .cloned()
                .ok_or(NativeWatchErrorKind::InvalidRoot)
        }

        fn refresh_root(
            &self,
            root: &NativeWatchRoot,
        ) -> Result<NativeWatchRoot, NativeWatchErrorKind> {
            self.roots
                .get(root.selected_root())
                .filter(|fresh| fresh.identity() == root.identity())
                .cloned()
                .ok_or(NativeWatchErrorKind::RootChanged)
        }

        fn observe_file(
            &self,
            root: &NativeWatchRoot,
            _path: &Path,
            require_read_lock: bool,
        ) -> Result<NativeWatchFileObservation, NativeWatchErrorKind> {
            assert!(require_read_lock);
            assert!(self.roots.contains_key(root.selected_root()));
            self.observation_count.fetch_add(1, Ordering::AcqRel);
            self.observations
                .lock()
                .expect("test observations lock")
                .pop_front()
                .unwrap_or(Err(NativeWatchErrorKind::Inaccessible))
        }
    }

    #[derive(Default)]
    struct FakeWatcherFactory {
        callbacks: Mutex<Vec<(PathBuf, WatcherCallback)>>,
    }

    impl FakeWatcherFactory {
        fn callback_for(&self, root: &Path) -> WatcherCallback {
            self.callbacks
                .lock()
                .expect("test callbacks lock")
                .iter()
                .find(|(registered_root, _)| registered_root == root)
                .expect("watcher callback registered")
                .1
                .clone()
        }

        fn registration_count(&self) -> usize {
            self.callbacks.lock().expect("test callbacks lock").len()
        }
    }

    struct FakeWatchRegistration;
    impl WatchRegistration for FakeWatchRegistration {}

    impl WatcherFactory for FakeWatcherFactory {
        fn watch(
            &self,
            root: &Path,
            _recursive: bool,
            callback: WatcherCallback,
        ) -> Result<Box<dyn WatchRegistration>, ()> {
            self.callbacks
                .lock()
                .map_err(|_| ())?
                .push((root.to_path_buf(), callback));
            Ok(Box::new(FakeWatchRegistration))
        }
    }

    struct TestRuntime {
        core: RuntimeCore,
        output: Arc<Mutex<OutputState>>,
        boundary: Arc<FakeWatchBoundary>,
        boundary_port: Arc<dyn WatchBoundary>,
        watcher_factory: Arc<FakeWatcherFactory>,
        watcher_port: Arc<dyn WatcherFactory>,
        ingress: Receiver<WorkerMessage>,
        clock: Arc<FakeClock>,
    }

    impl TestRuntime {
        fn new(
            rules: &[HotFolderRule],
            observations: Vec<Result<NativeWatchFileObservation, NativeWatchErrorKind>>,
        ) -> Self {
            Self::new_with_aliases(rules, observations, &[])
        }

        fn new_with_aliases(
            rules: &[HotFolderRule],
            observations: Vec<Result<NativeWatchFileObservation, NativeWatchErrorKind>>,
            aliases: &[(PathBuf, PathBuf)],
        ) -> Self {
            let mut boundary = FakeWatchBoundary::new(
                rules.iter().map(|rule| rule.watched_root.clone()),
                observations,
            );
            for (alias, target) in aliases {
                let target_root = boundary
                    .roots
                    .get(target)
                    .expect("alias target root exists")
                    .clone();
                // The alias resolves to the same admitted native root proof.
                boundary.roots.insert(alias.clone(), target_root);
            }
            let boundary = Arc::new(boundary);
            let boundary_port: Arc<dyn WatchBoundary> = boundary.clone();
            let watcher_factory = Arc::new(FakeWatcherFactory::default());
            let watcher_port: Arc<dyn WatcherFactory> = watcher_factory.clone();
            let output = Arc::new(Mutex::new(OutputState::default()));
            let (ingress_sender, ingress) = mpsc::sync_channel(MAX_WATCH_INGRESS_EVENTS);
            let eligible = rules
                .iter()
                .cloned()
                .map(|rule| EligibleRule {
                    fingerprint: ConfigurationFingerprint::of_serializable(&rule)
                        .expect("test rule fingerprints"),
                    rule,
                })
                .collect();
            let mut core = RuntimeCore::default();
            core.paused = false;
            let report = reconcile_core(
                &mut core,
                17,
                eligible,
                0,
                ActivationPolicy::AllEnabled,
                &ingress_sender,
                &output,
                &boundary_port,
                &watcher_port,
            );
            let unique_roots = boundary
                .roots
                .values()
                .map(NativeWatchRoot::identity)
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(report.active_roots, unique_roots.len());
            Self {
                core,
                output,
                boundary,
                boundary_port,
                watcher_factory,
                watcher_port,
                ingress,
                clock: Arc::new(FakeClock::new()),
            }
        }

        fn service(&self) -> HotFolderWatchService {
            HotFolderWatchService {
                worker: None,
                output: Arc::clone(&self.output),
                boundary: Arc::clone(&self.boundary_port),
                watcher_factory: Arc::clone(&self.watcher_port),
                clock: self.clock.clone(),
                shutdown: false,
                explicitly_paused: false,
            }
        }

        fn emit(&mut self, root: &Path, notification: WatcherNotification, now: Instant) {
            (self.watcher_factory.callback_for(root))(notification);
            let message = self
                .ingress
                .try_recv()
                .expect("bounded ingress accepted event");
            let WorkerMessage::Notify {
                root_id,
                epoch,
                notification,
            } = message
            else {
                panic!("watcher callback sends only notifications")
            };
            handle_notification(
                root_id,
                epoch,
                notification,
                &mut self.core,
                &self.output,
                now,
            );
        }
    }

    fn fake_root_identity(path: &Path) -> NativeFileIdentity {
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        let bytes = hasher.finish().to_le_bytes();
        let mut file_id = [0_u8; 16];
        file_id[..8].copy_from_slice(&bytes);
        test_native_file_identity(1, file_id)
    }

    fn test_root(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("productivity_expansion")
            .join("hotfolder")
            .join(name)
    }

    fn active_root_identity_for_path(
        core: &RuntimeCore,
        path: &Path,
    ) -> Option<NativeFileIdentity> {
        core.roots.iter().find_map(|(identity, root)| {
            root.rules
                .values()
                .any(|rule| paths_equivalent(&rule.rule.watched_root, path))
                .then_some(*identity)
        })
    }

    fn test_rule(name: &str, root: PathBuf) -> HotFolderRule {
        let mut rule = HotFolderRule::new(
            name,
            root,
            RuleAction::Rename {
                prefix: "ready_".to_owned(),
                suffix: String::new(),
            },
        );
        rule.enabled = true;
        rule.settle = SettlePolicy {
            sample_interval_ms: 50,
            stable_observations: 2,
            timeout_ms: 250,
        };
        rule
    }

    fn fake_observation(root: &Path, path: &Path, file_id: u8) -> NativeWatchFileObservation {
        let mut file = [0_u8; 16];
        file[0] = file_id;
        test_native_watch_file_observation(
            path,
            fake_root_identity(root),
            test_native_file_identity(1, [2; 16]),
            test_native_file_identity(1, file),
            12,
            SystemTime::UNIX_EPOCH + Duration::from_secs(123),
        )
    }

    fn notification(kind: WatchEventKind, path: PathBuf) -> WatcherNotification {
        WatcherNotification {
            kind,
            paths: vec![path],
        }
    }

    fn drain_notifications(runtime: &mut TestRuntime, now: Instant) {
        while let Ok(message) = runtime.ingress.try_recv() {
            if let WorkerMessage::Notify {
                root_id,
                epoch,
                notification,
            } = message
            {
                handle_notification(
                    root_id,
                    epoch,
                    notification,
                    &mut runtime.core,
                    &runtime.output,
                    now,
                );
            }
        }
    }

    #[test]
    fn event_and_lifecycle_coalescing_are_bounded_and_epoch_scoped() {
        let inactive = HotFolderWatchService::new();
        assert!(inactive.snapshot().paused);
        assert!(inactive.worker.is_none());

        let root = test_root("coalescing");
        let alias = PathBuf::from(root.to_string_lossy().to_uppercase());
        let other_root = test_root("independent");
        let rule = test_rule("one", root.clone());
        let mut alias_rule = test_rule("alias", alias.clone());
        alias_rule.filter.extensions = vec![".other".to_owned()];
        let other_rule = test_rule("two", other_root.clone());
        let mut runtime = TestRuntime::new_with_aliases(
            &[rule.clone(), alias_rule.clone(), other_rule.clone()],
            Vec::new(),
            &[(alias.clone(), root.clone())],
        );
        assert_eq!(runtime.watcher_factory.registration_count(), 2);
        let root_identity =
            active_root_identity_for_path(&runtime.core, &root).expect("root alias");
        assert_eq!(
            runtime.core.roots[&root_identity].rules.len(),
            2,
            "case and path aliases share one native-root watcher while retaining both rules"
        );
        assert_eq!(
            runtime.boundary.observation_count(),
            0,
            "activation never scans or reads files"
        );

        let base = runtime.clock.now();
        let part_path = root.join("download.zip.part");
        runtime.emit(
            &root,
            notification(WatchEventKind::Created, part_path),
            base,
        );
        assert!(
            runtime.core.pending.is_empty(),
            "temporary downloads are skipped by default"
        );

        let nested_path = root.join("nested").join("file.zip");
        runtime.emit(
            &root,
            notification(WatchEventKind::Created, nested_path),
            base,
        );
        assert!(
            runtime.core.pending.is_empty(),
            "direct-child rules ignore subfolders"
        );

        let path = root.join("arrival.zip");
        runtime.emit(
            &root,
            notification(WatchEventKind::Created, path.clone()),
            base,
        );
        assert_eq!(runtime.core.pending.len(), 1);
        runtime.clock.set_ms(10);
        let coalesced_at = runtime.clock.now();
        runtime.emit(
            &root,
            notification(WatchEventKind::Created, path.clone()),
            coalesced_at,
        );
        assert_eq!(runtime.core.pending.len(), 1);
        let pending = runtime
            .core
            .pending
            .values()
            .next()
            .expect("pending arrival");
        assert_eq!(pending.quiet_deadline, base + Duration::from_millis(60));
        assert_eq!(runtime.boundary.observation_count(), 0);

        runtime.clock.set_ms(20);
        runtime.emit(
            &root,
            notification(WatchEventKind::Removed, path.clone()),
            runtime.clock.now(),
        );
        assert!(
            runtime.core.pending.is_empty(),
            "remove retires only its pending arrival"
        );
        runtime.clock.set_ms(30);
        runtime.emit(
            &root,
            notification(WatchEventKind::Created, path.clone()),
            runtime.clock.now(),
        );
        assert_eq!(runtime.core.pending.len(), 1);

        let old_root_identity =
            active_root_identity_for_path(&runtime.core, &root).expect("active root");
        let old_root = runtime
            .core
            .roots
            .get(&old_root_identity)
            .expect("active root");
        let stale_id = old_root.id;
        let stale_epoch = old_root.epoch;
        handle_notification(
            stale_id,
            stale_epoch,
            notification(WatchEventKind::Removed, root.clone()),
            &mut runtime.core,
            &runtime.output,
            runtime.clock.now(),
        );
        assert!(!runtime.core.roots.contains_key(&old_root_identity));
        assert!(runtime.core.blocked_roots.contains(&alias));
        assert!(
            runtime
                .output
                .lock()
                .expect("output lock")
                .diagnostics
                .iter()
                .any(|diagnostic| {
                    diagnostic.kind == HotFolderDiagnosticKind::RootUnavailable
                        && diagnostic
                            .path
                            .as_deref()
                            .is_some_and(|path| paths_equivalent(path, &root))
                }),
            "root removal exposes lost coverage"
        );
        assert!(
            active_root_identity_for_path(&runtime.core, &other_root).is_some(),
            "loss is isolated to one root"
        );
        assert!(runtime.core.pending.is_empty());

        runtime.emit(
            &root,
            notification(WatchEventKind::Created, path),
            runtime.clock.now(),
        );
        let now = runtime.clock.now();
        drain_notifications(&mut runtime, now);
        assert!(
            runtime.core.pending.is_empty(),
            "late callbacks from a revoked epoch are inert"
        );

        let eligible = [rule, alias_rule, other_rule]
            .into_iter()
            .map(|rule| EligibleRule {
                fingerprint: ConfigurationFingerprint::of_serializable(&rule)
                    .expect("test fingerprint"),
                rule,
            })
            .collect();
        let (sender, _receiver) = mpsc::sync_channel(MAX_WATCH_INGRESS_EVENTS);
        let report = reconcile_core(
            &mut runtime.core,
            18,
            eligible,
            0,
            ActivationPolicy::ExistingOnly,
            &sender,
            &runtime.output,
            &runtime.boundary_port,
            &runtime.watcher_port,
        );
        assert_eq!(report.active_roots, 1);
        assert_eq!(
            runtime.watcher_factory.registration_count(),
            2,
            "ordinary reconcile does not resurrect lost coverage"
        );
    }

    #[test]
    fn settled_observation_leases_transfer_without_success_and_recheck_evidence() {
        let root = test_root("settled-lease");
        let rule = test_rule("settle", root.clone());
        let path = root.join("ready.zip");
        let samples = [3, 4, 4, 4, 4, 4, 4]
            .into_iter()
            .map(|file_id| Ok(fake_observation(&root, &path, file_id)))
            .collect();
        let mut runtime = TestRuntime::new(&[rule.clone()], samples);
        let mut service = runtime.service();
        let (wake_sender, wake_receiver) = mpsc::sync_channel(8);
        service.worker = Some(WorkerHandle {
            sender: wake_sender,
            thread: thread::spawn(|| {}),
        });
        let base = runtime.clock.now();

        runtime.emit(
            &root,
            notification(WatchEventKind::Created, path.clone()),
            base,
        );
        runtime.clock.set_ms(50);
        process_due(
            &mut runtime.core,
            &runtime.output,
            &*runtime.boundary_port,
            runtime.clock.now(),
        );
        runtime.clock.set_ms(100);
        process_due(
            &mut runtime.core,
            &runtime.output,
            &*runtime.boundary_port,
            runtime.clock.now(),
        );
        assert!(runtime.output.lock().expect("output lock").ready.is_empty());
        runtime.clock.set_ms(150);
        process_due(
            &mut runtime.core,
            &runtime.output,
            &*runtime.boundary_port,
            runtime.clock.now(),
        );
        let first_lease = service.try_lease_ready().expect("settled candidate lease");
        assert!(matches!(wake_receiver.try_recv(), Ok(WorkerMessage::Wake)));
        let first_id = first_lease.candidate().correlation;
        assert_eq!(
            first_lease.candidate().rule_fingerprint.as_str(),
            ConfigurationFingerprint::of_serializable(&rule)
                .expect("test fingerprint")
                .as_str()
        );
        assert_eq!(first_lease.candidate().store_revision, 17);
        assert_eq!(
            first_lease.candidate().evidence.file_identity.file_id()[0],
            4,
            "a file replacement resets the stable sample count"
        );

        runtime.clock.set_ms(160);
        runtime.emit(
            &root,
            notification(WatchEventKind::Created, path.clone()),
            runtime.clock.now(),
        );
        runtime.clock.set_ms(210);
        process_due(
            &mut runtime.core,
            &runtime.output,
            &*runtime.boundary_port,
            runtime.clock.now(),
        );
        runtime.clock.set_ms(260);
        process_due(
            &mut runtime.core,
            &runtime.output,
            &*runtime.boundary_port,
            runtime.clock.now(),
        );
        let queued = runtime
            .output
            .lock()
            .expect("output lock")
            .ready
            .front()
            .cloned()
            .unwrap();
        assert_ne!(
            queued.correlation, first_id,
            "a newer event can settle while an older lease is outstanding"
        );
        assert!(runtime.output.lock().expect("output lock").leased.is_some());
        let transferred = service
            .finish_lease(first_lease, HotFolderLeaseDisposition::TransferToConsumer)
            .expect("lease transfer");
        assert_eq!(
            transferred.expect("transferred evidence").correlation,
            first_id
        );

        let deferred = service.try_lease_ready().expect("newer candidate lease");
        assert!(matches!(
            service.finish_lease(deferred, HotFolderLeaseDisposition::Defer),
            Ok(None)
        ));
        let expired = service.try_lease_ready().expect("deferred candidate lease");
        let expiry_ms = runtime
            .output
            .lock()
            .expect("output lock")
            .leased
            .as_ref()
            .expect("active deferred lease")
            .deadline
            .duration_since(runtime.clock.base)
            .as_millis() as u64;
        runtime.clock.set_ms(expiry_ms);
        assert!(matches!(
            service.finish_lease(expired, HotFolderLeaseDisposition::TransferToConsumer),
            Err(HotFolderRuntimeError::StaleLease)
        ));
        assert_eq!(
            runtime.output.lock().expect("output lock").ready.len(),
            1,
            "finishing after the deadline expires and requeues before worker wakeup"
        );
        assert!(
            runtime
                .output
                .lock()
                .expect("output lock")
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind == HotFolderDiagnosticKind::LeaseExpired)
        );
        let rejected = service.try_lease_ready().expect("requeued candidate lease");
        assert!(matches!(
            service.finish_lease(rejected, HotFolderLeaseDisposition::RejectStale),
            Ok(None)
        ));
        assert!(runtime.output.lock().expect("output lock").ready.is_empty());

        runtime.clock.set_ms(expiry_ms + 50);
        runtime.emit(
            &root,
            notification(WatchEventKind::Created, path.clone()),
            runtime.clock.now(),
        );
        runtime.clock.set_ms(expiry_ms + 100);
        process_due(
            &mut runtime.core,
            &runtime.output,
            &*runtime.boundary_port,
            runtime.clock.now(),
        );
        runtime.clock.set_ms(expiry_ms + 150);
        process_due(
            &mut runtime.core,
            &runtime.output,
            &*runtime.boundary_port,
            runtime.clock.now(),
        );
        let stale = service
            .try_lease_ready()
            .expect("candidate before root revocation");
        let root_identity =
            active_root_identity_for_path(&runtime.core, &root).expect("active root");
        let root_epoch = runtime
            .core
            .roots
            .get(&root_identity)
            .expect("active root")
            .epoch;
        pause_root(
            &mut runtime.core,
            root_identity,
            &runtime.output,
            HotFolderDiagnosticKind::LostCoverage,
            None,
        );
        assert!(matches!(
            service.finish_lease(stale, HotFolderLeaseDisposition::TransferToConsumer),
            Err(HotFolderRuntimeError::StaleLease)
        ));
        assert!(
            !runtime
                .core
                .roots
                .values()
                .any(|active| active.epoch == root_epoch)
        );
        assert_eq!(runtime.boundary.observation_count(), 7);
    }
}
