//! One app-owned Hot Folder lifecycle worker.
//!
//! The GUI submits bounded typed requests and reads a short-lock cached view.
//! All Store, B watcher and C executor calls are owned by the worker thread.

use crate::hotfolder::{
    ActionCapability, ActionEnablement, ConfigurationFingerprint, HotFolderCapabilityContext,
    HotFolderEnableRequest, HotFolderEventKind, HotFolderExecutionControl,
    HotFolderExecutionEngine, HotFolderExecutionProgress, HotFolderExecutionSnapshot,
    HotFolderRule, HotFolderRuntimeSnapshot, HotFolderStoreError, HotFolderWatchService,
    RootAvailability, RuleAction, RuleId, StrongActionConfirmation, evaluate_action_capability,
    evaluate_rule_conditions,
};
use crate::mkmacro::MkMacroDocument;
use crate::platform::app_data::AppDataRoot;
use crate::universal_actions::{ActionResolutionContext, ActionSurface, ResolvedActionTarget};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const DRIVER_QUEUE_CAPACITY: usize = 32;
const MAX_DRIVER_ACKNOWLEDGEMENTS: usize = 64;
const MAX_PENDING_ENABLE_PREVIEWS: usize = 16;
const DRIVER_IDLE_TICK: Duration = Duration::from_millis(80);

#[derive(Clone)]
pub(crate) struct HotFolderCapabilityInputs {
    pub(crate) launcher_targets: Vec<ResolvedActionTarget>,
    pub(crate) mkmacro_document: MkMacroDocument,
}

impl HotFolderCapabilityInputs {
    pub(crate) fn empty() -> Self {
        Self {
            launcher_targets: Vec::new(),
            mkmacro_document: MkMacroDocument::default(),
        }
    }

    fn with_context<T>(
        &self,
        rules: &crate::hotfolder::HotFolderRulesSnapshot,
        adapter: &dyn crate::hotfolder::HotFolderExternalActionAdapter,
        operation: impl FnOnce(HotFolderCapabilityContext<'_>) -> T,
    ) -> T {
        let mut availability = HashMap::new();
        for rule in &rules.rules {
            availability
                .entry(rule.watched_root.clone())
                .or_insert_with(|| inspect_directory(&rule.watched_root));
            match &rule.action {
                RuleAction::Copy { destination } | RuleAction::Move { destination } => {
                    availability
                        .entry(destination.clone())
                        .or_insert_with(|| inspect_directory(destination));
                }
                RuleAction::Rename { .. }
                | RuleAction::RegisteredLauncherAction { .. }
                | RuleAction::MkMacro { .. } => {}
            }
        }
        let query = String::new();
        let context = HotFolderCapabilityContext::new(
            &self.launcher_targets,
            ActionResolutionContext::new(ActionSurface::LauncherList, &query),
            &self.mkmacro_document,
            &availability,
        )
        .with_external_action_adapter(adapter);
        operation(context)
    }

    pub(crate) fn evaluate_rule(
        &self,
        rule: &HotFolderRule,
        adapter: &dyn crate::hotfolder::HotFolderExternalActionAdapter,
    ) -> ActionCapability {
        let rules = crate::hotfolder::HotFolderRulesSnapshot::for_preview(rule.clone());
        self.with_context(&rules, adapter, |context| {
            evaluate_action_capability(rule, &context)
        })
    }
}

#[derive(Clone)]
pub(crate) struct HotFolderEnablePreview {
    pub(crate) request_id: u64,
    pub(crate) rule: HotFolderRule,
    pub(crate) expected_revision: u64,
    pub(crate) resulting_revision: u64,
    pub(crate) fingerprint: ConfigurationFingerprint,
    pub(crate) capability: ActionCapability,
}

#[derive(Clone)]
pub(crate) struct HotFolderDriverAcknowledgement {
    pub(crate) request_id: u64,
    pub(crate) result: Result<(), String>,
}

#[derive(Clone, Default)]
pub(crate) struct HotFolderDriverSnapshot {
    pub(crate) plugin_enabled: bool,
    pub(crate) loaded: bool,
    pub(crate) running: bool,
    pub(crate) paused: bool,
    pub(crate) admission_blocked: bool,
    pub(crate) store: Option<crate::hotfolder::HotFolderRulesSnapshot>,
    pub(crate) runtime: Option<HotFolderRuntimeSnapshot>,
    pub(crate) execution: Option<HotFolderExecutionSnapshot>,
    pub(crate) execution_progress: HotFolderExecutionProgress,
    pub(crate) error: Option<String>,
    pub(crate) acknowledgements: VecDeque<HotFolderDriverAcknowledgement>,
    pub(crate) enable_previews: VecDeque<HotFolderEnablePreview>,
}

enum DriverOperation {
    EnsureLoaded,
    Start,
    Stop,
    SaveNew {
        expected_revision: u64,
        rule: HotFolderRule,
    },
    SaveEdit {
        expected_revision: u64,
        rule: HotFolderRule,
    },
    Delete {
        expected_revision: u64,
        rule_id: RuleId,
    },
    PrepareEnable {
        expected_revision: u64,
        rule_id: RuleId,
    },
    ConfirmEnable {
        preview_id: u64,
        confirmation: StrongActionConfirmation,
    },
    CancelEnable {
        preview_id: u64,
    },
    Disable {
        expected_revision: u64,
        rule_id: RuleId,
    },
    Pause,
    Resume,
    ClearHistory,
}

struct DriverRequest {
    request_id: u64,
    gate_generation: u64,
    restore_admission_on_error: bool,
    operation: DriverOperation,
    inputs: Option<HotFolderCapabilityInputs>,
}

/// A persistent bounded control surface. Closing the dialog does not stop or
/// recreate this driver.
pub(crate) struct HotFolderRuntimeDriver {
    sender: SyncSender<DriverRequest>,
    snapshot: Arc<RwLock<HotFolderDriverSnapshot>>,
    next_request_id: AtomicU64,
    execution_control: HotFolderExecutionControl,
    shutdown_requested: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    reaper: Option<JoinHandle<()>>,
    repair_store: crate::hotfolder::HotFolderRulesStore,
}

impl HotFolderRuntimeDriver {
    /// Construct the app-owned driver state without starting its worker.
    /// Requests fail through the same disconnected-channel path used when a
    /// live driver has shut down, while reads retain a valid empty snapshot.
    #[cfg(test)]
    pub(crate) fn dormant(data_root: AppDataRoot) -> Self {
        let repair_store = crate::hotfolder::HotFolderRulesStore::new(&data_root);
        let (sender, receiver) = mpsc::sync_channel(DRIVER_QUEUE_CAPACITY);
        drop(receiver);
        Self {
            sender,
            snapshot: Arc::new(RwLock::new(HotFolderDriverSnapshot::default())),
            next_request_id: AtomicU64::new(1),
            execution_control: HotFolderExecutionControl::initially_blocked(),
            shutdown_requested: Arc::new(AtomicBool::new(true)),
            worker: None,
            reaper: None,
            repair_store,
        }
    }

    pub(crate) fn new(
        data_root: AppDataRoot,
        adapter: Arc<dyn crate::hotfolder::HotFolderExternalActionAdapter>,
    ) -> Self {
        let repair_store = crate::hotfolder::HotFolderRulesStore::new(&data_root);
        let (sender, receiver) = mpsc::sync_channel(DRIVER_QUEUE_CAPACITY);
        let snapshot = Arc::new(RwLock::new(HotFolderDriverSnapshot::default()));
        let execution_control = HotFolderExecutionControl::initially_blocked();
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown_requested);
        let worker_snapshot = Arc::clone(&snapshot);
        let worker_control = execution_control.clone();
        let worker_repair_store = repair_store.clone();
        let worker = thread::Builder::new()
            .name("hotfolder-app-driver".into())
            .spawn(move || {
                driver_main(
                    receiver,
                    worker_repair_store,
                    adapter,
                    worker_control,
                    worker_shutdown,
                    worker_snapshot,
                )
            })
            .expect("Hot Folder app driver thread must start");
        Self {
            sender,
            snapshot,
            next_request_id: AtomicU64::new(1),
            execution_control,
            shutdown_requested,
            worker: Some(worker),
            reaper: None,
            repair_store,
        }
    }

    pub(crate) fn repair_store_handle(&self) -> crate::hotfolder::HotFolderRulesStore {
        self.repair_store.clone()
    }

    #[cfg(test)]
    pub(crate) fn has_workers_for_test(&self) -> bool {
        self.worker.is_some() || self.reaper.is_some()
    }

    pub(crate) fn snapshot(&self) -> HotFolderDriverSnapshot {
        let mut snapshot = self
            .snapshot
            .read()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_default();
        snapshot.admission_blocked = self.execution_control.admission_blocked();
        snapshot.execution_progress = self.execution_control.progress_snapshot();
        snapshot
    }

    pub(crate) fn ensure_loaded(&self) -> Result<u64, &'static str> {
        self.submit(DriverOperation::EnsureLoaded, None)
    }

    pub(crate) fn start(&self, inputs: HotFolderCapabilityInputs) -> Result<u64, &'static str> {
        self.submit(DriverOperation::Start, Some(inputs))
    }

    pub(crate) fn stop(&self) -> Result<u64, &'static str> {
        self.submit_blocking(DriverOperation::Stop, None, false)
    }

    pub(crate) fn save_new(
        &self,
        expected_revision: u64,
        rule: HotFolderRule,
        inputs: HotFolderCapabilityInputs,
    ) -> Result<u64, &'static str> {
        self.submit_blocking(
            DriverOperation::SaveNew {
                expected_revision,
                rule,
            },
            Some(inputs),
            true,
        )
    }

    pub(crate) fn save_edit(
        &self,
        expected_revision: u64,
        rule: HotFolderRule,
        inputs: HotFolderCapabilityInputs,
    ) -> Result<u64, &'static str> {
        self.submit_blocking(
            DriverOperation::SaveEdit {
                expected_revision,
                rule,
            },
            Some(inputs),
            true,
        )
    }

    pub(crate) fn delete(
        &self,
        expected_revision: u64,
        rule_id: RuleId,
        inputs: HotFolderCapabilityInputs,
    ) -> Result<u64, &'static str> {
        self.submit_blocking(
            DriverOperation::Delete {
                expected_revision,
                rule_id,
            },
            Some(inputs),
            true,
        )
    }

    pub(crate) fn prepare_enable(
        &self,
        expected_revision: u64,
        rule_id: RuleId,
        inputs: HotFolderCapabilityInputs,
    ) -> Result<u64, &'static str> {
        self.submit(
            DriverOperation::PrepareEnable {
                expected_revision,
                rule_id,
            },
            Some(inputs),
        )
    }

    pub(crate) fn confirm_enable(
        &self,
        preview_id: u64,
        confirmation: StrongActionConfirmation,
        inputs: HotFolderCapabilityInputs,
    ) -> Result<u64, &'static str> {
        self.submit(
            DriverOperation::ConfirmEnable {
                preview_id,
                confirmation,
            },
            Some(inputs),
        )
    }

    pub(crate) fn cancel_enable(&self, preview_id: u64) -> Result<u64, &'static str> {
        self.submit(DriverOperation::CancelEnable { preview_id }, None)
    }

    pub(crate) fn disable(
        &self,
        expected_revision: u64,
        rule_id: RuleId,
        inputs: HotFolderCapabilityInputs,
    ) -> Result<u64, &'static str> {
        self.submit_blocking(
            DriverOperation::Disable {
                expected_revision,
                rule_id,
            },
            Some(inputs),
            true,
        )
    }

    pub(crate) fn pause(&self) -> Result<u64, &'static str> {
        self.submit_blocking(DriverOperation::Pause, None, false)
    }

    pub(crate) fn resume(&self, inputs: HotFolderCapabilityInputs) -> Result<u64, &'static str> {
        self.submit(DriverOperation::Resume, Some(inputs))
    }

    pub(crate) fn clear_history(&self) -> Result<u64, &'static str> {
        self.submit(DriverOperation::ClearHistory, None)
    }

    fn submit(
        &self,
        operation: DriverOperation,
        inputs: Option<HotFolderCapabilityInputs>,
    ) -> Result<u64, &'static str> {
        self.submit_with_gate(
            operation,
            inputs,
            self.execution_control.gate_snapshot(),
            false,
        )
    }

    fn submit_blocking(
        &self,
        operation: DriverOperation,
        inputs: Option<HotFolderCapabilityInputs>,
        restore_on_failure: bool,
    ) -> Result<u64, &'static str> {
        let (gate_generation, was_open) = self.execution_control.block_admission_for_request();
        self.submit_with_gate(
            operation,
            inputs,
            gate_generation,
            restore_on_failure && was_open,
        )
    }

    fn submit_with_gate(
        &self,
        operation: DriverOperation,
        inputs: Option<HotFolderCapabilityInputs>,
        gate_generation: u64,
        restore_admission_on_error: bool,
    ) -> Result<u64, &'static str> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        if request_id == 0 {
            if restore_admission_on_error {
                self.execution_control
                    .allow_admission_if_unchanged(gate_generation);
            }
            return Err("Hot Folder request ID space is exhausted");
        }
        match self.sender.try_send(DriverRequest {
            request_id,
            gate_generation,
            restore_admission_on_error,
            operation,
            inputs,
        }) {
            Ok(()) => Ok(request_id),
            Err(TrySendError::Full(_)) => {
                if restore_admission_on_error {
                    self.execution_control
                        .allow_admission_if_unchanged(gate_generation);
                }
                Err("Hot Folder control queue is full")
            }
            Err(TrySendError::Disconnected(_)) => {
                if restore_admission_on_error {
                    self.execution_control
                        .allow_admission_if_unchanged(gate_generation);
                }
                Err("Hot Folder driver is unavailable")
            }
        }
    }

    pub(crate) fn shutdown_and_reap(&mut self) {
        self.execution_control.block_admission();
        self.shutdown_requested.store(true, Ordering::Release);
        if self.worker.is_none() {
            return;
        }
        if let Some(worker) = self.worker.take() {
            let worker_holder = Arc::new(Mutex::new(Some(worker)));
            let reaper_worker = Arc::clone(&worker_holder);
            match thread::Builder::new()
                .name("hotfolder-worker-reaper".into())
                .spawn(move || {
                    let worker = reaper_worker
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .take();
                    if let Some(worker) = worker {
                        let _ = worker.join();
                    }
                }) {
                Ok(reaper) => self.reaper = Some(reaper),
                Err(error) => {
                    tracing::error!(reason = %error, "Hot Folder reaper could not start; waiting at shutdown boundary");
                    // This method is called at final application shutdown. If
                    // the small reaper thread cannot be created, join here
                    // rather than detach a live watcher or execution worker.
                    // The normal render/dialog path never calls this fallback.
                    let worker = worker_holder
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .take();
                    if let Some(worker) = worker {
                        let _ = worker.join();
                    }
                }
            }
        }
    }

    pub(crate) fn join_reaper(&mut self) {
        if let Some(reaper) = self.reaper.take()
            && reaper.join().is_err()
        {
            tracing::warn!("Hot Folder worker reaper panicked");
        }
    }
}

impl Drop for HotFolderRuntimeDriver {
    fn drop(&mut self) {
        self.shutdown_and_reap();
        self.join_reaper();
    }
}

#[cfg(feature = "perf-harness")]
struct InertHotFolderActionAdapter {
    dispatches: AtomicU64,
}

#[cfg(feature = "perf-harness")]
impl crate::hotfolder::HotFolderExternalActionAdapter for InertHotFolderActionAdapter {
    fn supports(&self, _action: &crate::hotfolder::RuleAction) -> bool {
        false
    }

    fn dispatch(
        &self,
        request: crate::hotfolder::HotFolderExternalActionRequest,
        _cancellation: &crate::file_operations::ScanCancellation,
    ) -> crate::hotfolder::HotFolderExternalActionOutcome {
        self.dispatches.fetch_add(1, Ordering::SeqCst);
        crate::hotfolder::HotFolderExternalActionOutcome {
            operation_id: request.operation_id,
            macro_run_id: None,
            status: crate::hotfolder::HotFolderTerminalStatus::Failure,
            phase: crate::hotfolder::HotFolderExecutionPhase::Admission,
            reason: Some("the performance fixture rejects all external actions".into()),
        }
    }
}

/// Feature-gated owner for a real Hot Folder driver with a fresh empty store.
/// The fixture exposes only state validation and explicit worker shutdown.
#[cfg(feature = "perf-harness")]
pub struct HotFolderIdleFixture {
    driver: HotFolderRuntimeDriver,
    scenario: crate::perf_harness::Scenario,
    adapter: Arc<InertHotFolderActionAdapter>,
}

#[cfg(feature = "perf-harness")]
impl HotFolderIdleFixture {
    pub fn new(
        run_output_dir: &Path,
        scenario: crate::perf_harness::Scenario,
    ) -> Result<Self, crate::perf_harness::ProbeFailure> {
        use crate::perf_harness::{ProbeFailure, ProbeStatus, Scenario};

        if !matches!(
            scenario,
            Scenario::HotfolderDisabled | Scenario::HotfolderEnabledZeroRules
        ) {
            return Err(ProbeFailure::error(
                "Hot Folder fixture accepts only disabled or enabled-zero-rules scenarios",
            ));
        }
        if !run_output_dir.is_absolute() {
            return Err(ProbeFailure::error(
                "Hot Folder fixture output directory must be absolute",
            ));
        }
        let canonical_run_dir = run_output_dir
            .canonicalize()
            .map_err(|error| fixture_io_failure("canonicalize run output directory", error))?;
        let fixture_dir = canonical_run_dir.join("fixture");
        std::fs::create_dir(&fixture_dir).map_err(|error| {
            fixture_io_failure("create fresh isolated fixture directory", error)
        })?;
        let fixture_dir = fixture_dir.canonicalize().map_err(|error| {
            fixture_io_failure("canonicalize isolated fixture directory", error)
        })?;
        if !fixture_dir.starts_with(&canonical_run_dir) {
            return Err(ProbeFailure::new(
                ProbeStatus::Error,
                "canonical Hot Folder fixture directory escaped this run's output directory",
            ));
        }
        let settings_path = fixture_dir.join("settings.json");
        let data_root = AppDataRoot::from_settings_path(settings_path).map_err(|error| {
            let reason = format!("derive isolated Hot Folder data root: {error}");
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io_error| io_error.raw_os_error() == Some(5))
            {
                ProbeFailure::new(ProbeStatus::BlockedEnvironment, reason)
            } else {
                ProbeFailure::error(reason)
            }
        })?;

        let adapter = Arc::new(InertHotFolderActionAdapter {
            dispatches: AtomicU64::new(0),
        });
        let driver_adapter: Arc<dyn crate::hotfolder::HotFolderExternalActionAdapter> =
            adapter.clone();
        let driver = HotFolderRuntimeDriver::new(data_root, driver_adapter);
        let request_id = match scenario {
            Scenario::HotfolderDisabled => driver.ensure_loaded(),
            Scenario::HotfolderEnabledZeroRules => driver.start(HotFolderCapabilityInputs::empty()),
            _ => unreachable!("scenario was validated above"),
        }
        .map_err(|error| {
            ProbeFailure::error(format!("submit Hot Folder fixture request: {error}"))
        })?;
        let snapshot = wait_for_fixture_acknowledgement(&driver, request_id)?;
        validate_hotfolder_idle_snapshot(&snapshot, scenario)?;
        let fixture = Self {
            driver,
            scenario,
            adapter,
        };
        fixture.validate()?;
        Ok(fixture)
    }

    pub fn validate(&self) -> Result<(), crate::perf_harness::ProbeFailure> {
        use crate::perf_harness::ProbeFailure;

        validate_hotfolder_idle_snapshot(&self.driver.snapshot(), self.scenario)?;
        if self.adapter.dispatches.load(Ordering::SeqCst) != 0 {
            return Err(ProbeFailure::error(
                "inert Hot Folder fixture received an external action dispatch",
            ));
        }
        Ok(())
    }

    pub fn shutdown(&mut self) {
        self.driver.shutdown_and_reap();
        self.driver.join_reaper();
    }
}

#[cfg(feature = "perf-harness")]
impl Drop for HotFolderIdleFixture {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(feature = "perf-harness")]
fn fixture_io_failure(operation: &str, error: std::io::Error) -> crate::perf_harness::ProbeFailure {
    let status = if error.raw_os_error() == Some(5) {
        crate::perf_harness::ProbeStatus::BlockedEnvironment
    } else {
        crate::perf_harness::ProbeStatus::Error
    };
    crate::perf_harness::ProbeFailure::new(status, format!("{operation} failed: {error}"))
}

#[cfg(feature = "perf-harness")]
fn wait_for_fixture_acknowledgement(
    driver: &HotFolderRuntimeDriver,
    request_id: u64,
) -> Result<HotFolderDriverSnapshot, crate::perf_harness::ProbeFailure> {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let snapshot = driver.snapshot();
        if let Some(acknowledgement) = snapshot
            .acknowledgements
            .iter()
            .find(|acknowledgement| acknowledgement.request_id == request_id)
        {
            acknowledgement.result.as_ref().map_err(|error| {
                crate::perf_harness::ProbeFailure::error(format!(
                    "Hot Folder fixture request failed: {error}"
                ))
            })?;
            return Ok(snapshot);
        }
        if std::time::Instant::now() >= deadline {
            return Err(crate::perf_harness::ProbeFailure::timeout(
                "Hot Folder fixture request was not acknowledged within 3 seconds",
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(feature = "perf-harness")]
fn validate_hotfolder_idle_snapshot(
    snapshot: &HotFolderDriverSnapshot,
    scenario: crate::perf_harness::Scenario,
) -> Result<(), crate::perf_harness::ProbeFailure> {
    use crate::perf_harness::{ProbeFailure, Scenario};

    if let Some(error) = snapshot.error.as_deref() {
        return Err(ProbeFailure::error(format!(
            "Hot Folder driver reported an error: {error}"
        )));
    }
    if !snapshot.loaded {
        return Err(ProbeFailure::error("Hot Folder store is not loaded"));
    }
    let store = snapshot
        .store
        .as_ref()
        .ok_or_else(|| ProbeFailure::error("Hot Folder store snapshot is unavailable"))?;
    if !store.rules.is_empty() {
        return Err(ProbeFailure::error(
            "isolated Hot Folder fixture unexpectedly contains rules",
        ));
    }

    match scenario {
        Scenario::HotfolderDisabled => {
            if snapshot.plugin_enabled
                || snapshot.running
                || snapshot.runtime.is_some()
                || snapshot.execution.is_some()
            {
                return Err(ProbeFailure::error(
                    "disabled Hot Folder fixture unexpectedly owns runtime or execution workers",
                ));
            }
        }
        Scenario::HotfolderEnabledZeroRules => {
            if !snapshot.plugin_enabled || !snapshot.running || snapshot.paused {
                return Err(ProbeFailure::error(
                    "zero-rule Hot Folder fixture is not enabled and running",
                ));
            }
            let runtime = snapshot
                .runtime
                .as_ref()
                .ok_or_else(|| ProbeFailure::error("Hot Folder runtime snapshot is unavailable"))?;
            if runtime.active_roots != 0
                || runtime.active_rules != 0
                || runtime.pending_files != 0
                || runtime.ready_candidates != 0
                || runtime.lease_active
            {
                return Err(ProbeFailure::error(
                    "zero-rule Hot Folder runtime contains active roots, rules, files, candidates, or leases",
                ));
            }
            let execution = snapshot.execution.as_ref().ok_or_else(|| {
                ProbeFailure::error("Hot Folder execution snapshot is unavailable")
            })?;
            if execution.active
                || execution.completed_runs != 0
                || execution.recovery_required != 0
                || execution.journal_error.is_some()
            {
                return Err(ProbeFailure::error(
                    "zero-rule Hot Folder execution state is active, recovered, or unhealthy",
                ));
            }
        }
        _ => {
            return Err(ProbeFailure::error(
                "Hot Folder validator received an unsupported scenario",
            ));
        }
    }
    Ok(())
}

struct PreparedEnable {
    preview: HotFolderEnablePreview,
}

fn driver_main(
    receiver: mpsc::Receiver<DriverRequest>,
    store: crate::hotfolder::HotFolderRulesStore,
    adapter: Arc<dyn crate::hotfolder::HotFolderExternalActionAdapter>,
    control: HotFolderExecutionControl,
    shutdown_requested: Arc<AtomicBool>,
    published: Arc<RwLock<HotFolderDriverSnapshot>>,
) {
    let mut store_loaded = false;
    let mut inputs = HotFolderCapabilityInputs::empty();
    let mut watcher: Option<HotFolderWatchService> = None;
    let mut engine: Option<HotFolderExecutionEngine> = None;
    let mut plugin_enabled = false;
    let mut paused = false;
    let mut pending_enables = VecDeque::<PreparedEnable>::new();
    let mut acknowledgements = VecDeque::new();
    let mut load_diagnostic = None;
    let mut last_error = None;

    loop {
        if shutdown_requested.load(Ordering::Acquire) {
            break;
        }
        let wait = if plugin_enabled {
            DRIVER_IDLE_TICK
        } else {
            Duration::from_millis(500)
        };
        match receiver.recv_timeout(wait) {
            Ok(request) => {
                if let Some(next_inputs) = request.inputs {
                    inputs = next_inputs;
                }
                let gate_generation = request.gate_generation;
                let restore_admission_on_error = request.restore_admission_on_error;
                let revision_before = store.snapshot().revision;
                let result = process_operation(
                    request.request_id,
                    request.operation,
                    gate_generation,
                    &store,
                    &mut store_loaded,
                    &inputs,
                    &adapter,
                    &control,
                    &mut watcher,
                    &mut engine,
                    &mut plugin_enabled,
                    &mut paused,
                    &mut pending_enables,
                    &mut load_diagnostic,
                    &mut last_error,
                );
                if result.is_err()
                    && restore_admission_on_error
                    && store.snapshot().revision == revision_before
                {
                    control.allow_admission_if_unchanged(gate_generation);
                }
                match &result {
                    Ok(()) => last_error = None,
                    Err(message) => last_error = Some(message.clone()),
                }
                acknowledgements.push_back(HotFolderDriverAcknowledgement {
                    request_id: request.request_id,
                    result,
                });
                while acknowledgements.len() > MAX_DRIVER_ACKNOWLEDGEMENTS {
                    acknowledgements.pop_front();
                }
                publish_driver_snapshot(
                    &published,
                    plugin_enabled,
                    store_loaded,
                    paused,
                    &store,
                    &watcher,
                    &engine,
                    last_error.as_deref().or(load_diagnostic.as_deref()),
                    &acknowledgements,
                    &pending_enables,
                );
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                control.block_admission();
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }

        if plugin_enabled
            && !paused
            && let (Some(watcher), Some(engine)) = (watcher.as_ref(), engine.as_ref())
        {
            let rules = store.snapshot();
            let result = inputs.with_context(&rules, adapter.as_ref(), |context| {
                engine.run_next_ready(watcher, &store, &context)
            });
            if let Err(error) = result {
                last_error = Some(format!("Hot Folder execution failed: {error:?}"));
            }
        }
        publish_driver_snapshot(
            &published,
            plugin_enabled,
            store_loaded,
            paused,
            &store,
            &watcher,
            &engine,
            last_error.as_deref().or(load_diagnostic.as_deref()),
            &acknowledgements,
            &pending_enables,
        );
    }
    control.block_admission();
    stop_services(&mut watcher, &mut engine);
    publish_driver_snapshot(
        &published,
        false,
        store_loaded,
        paused,
        &store,
        &watcher,
        &engine,
        last_error.as_deref().or(load_diagnostic.as_deref()),
        &acknowledgements,
        &pending_enables,
    );
}

#[allow(clippy::too_many_arguments)]
fn process_operation(
    request_id: u64,
    operation: DriverOperation,
    control_gate: u64,
    store: &crate::hotfolder::HotFolderRulesStore,
    store_loaded: &mut bool,
    inputs: &HotFolderCapabilityInputs,
    adapter: &Arc<dyn crate::hotfolder::HotFolderExternalActionAdapter>,
    control: &HotFolderExecutionControl,
    watcher: &mut Option<HotFolderWatchService>,
    engine: &mut Option<HotFolderExecutionEngine>,
    plugin_enabled: &mut bool,
    paused: &mut bool,
    pending_enables: &mut VecDeque<PreparedEnable>,
    load_diagnostic: &mut Option<String>,
    last_error: &mut Option<String>,
) -> Result<(), String> {
    match operation {
        DriverOperation::EnsureLoaded => {
            ensure_store_loaded(store, store_loaded, load_diagnostic);
            Ok(())
        }
        DriverOperation::Start => {
            ensure_store_loaded(store, store_loaded, load_diagnostic);
            if *plugin_enabled {
                return Ok(());
            }
            let next_engine = HotFolderExecutionEngine::new_with_external_adapter_and_control(
                store,
                Arc::clone(adapter),
                control.clone(),
            )
            .map_err(|error| format!("Hot Folder journal is unavailable: {error:?}"))?;
            let mut next_watcher = HotFolderWatchService::new();
            let start_result = if *paused {
                next_engine.pause_after_admission_closed();
                Ok(())
            } else {
                let snapshot = store.snapshot();
                inputs
                    .with_context(&snapshot, adapter.as_ref(), |context| {
                        next_watcher.restore_enabled_rules(&snapshot, &context)
                    })
                    .map(|_| ())
                    .map_err(|error| format!("{error:?}"))
            };
            if let Err(error) = start_result {
                next_engine.shutdown_after_admission_closed();
                let _ = next_watcher.shutdown();
                return Err(error);
            }
            *watcher = Some(next_watcher);
            *engine = Some(next_engine);
            *plugin_enabled = true;
            if !*paused {
                control.allow_admission_if_unchanged(control_gate);
            }
            *last_error = None;
            Ok(())
        }
        DriverOperation::Stop => {
            *plugin_enabled = false;
            pending_enables.clear();
            if let Some(engine) = engine.as_ref() {
                engine.shutdown_after_admission_closed();
            }
            if let Some(watcher) = watcher.as_mut() {
                let _ = watcher.shutdown();
            }
            *watcher = None;
            *engine = None;
            *last_error = None;
            Ok(())
        }
        DriverOperation::SaveNew {
            expected_revision,
            mut rule,
        } => {
            rule.enabled = false;
            ensure_store_ready(store, store_loaded)?;
            let snapshot = store
                .add_rule(expected_revision, rule)
                .map_err(store_error)?;
            reconcile_after_edit(
                &snapshot,
                inputs,
                adapter.as_ref(),
                control,
                watcher,
                engine,
                *paused,
                control_gate,
            )?;
            Ok(())
        }
        DriverOperation::SaveEdit {
            expected_revision,
            mut rule,
        } => {
            rule.enabled = false;
            ensure_store_ready(store, store_loaded)?;
            let snapshot = store
                .replace_rule(expected_revision, rule)
                .map_err(store_error)?;
            reconcile_after_edit(
                &snapshot,
                inputs,
                adapter.as_ref(),
                control,
                watcher,
                engine,
                *paused,
                control_gate,
            )?;
            Ok(())
        }
        DriverOperation::Delete {
            expected_revision,
            rule_id,
        } => {
            ensure_store_ready(store, store_loaded)?;
            let snapshot = store
                .remove_rule(expected_revision, &rule_id)
                .map_err(store_error)?;
            reconcile_after_edit(
                &snapshot,
                inputs,
                adapter.as_ref(),
                control,
                watcher,
                engine,
                *paused,
                control_gate,
            )?;
            Ok(())
        }
        DriverOperation::PrepareEnable {
            expected_revision,
            rule_id,
        } => {
            ensure_store_ready(store, store_loaded)?;
            if !*plugin_enabled || *paused {
                return Err("Hot Folder is paused or disabled".into());
            }
            let snapshot = store.snapshot();
            if snapshot.revision != expected_revision {
                return Err("Hot Folder rules changed; review the current rule again".into());
            }
            let mut rule = snapshot
                .rules
                .iter()
                .find(|rule| rule.id == rule_id)
                .cloned()
                .ok_or_else(|| "Hot Folder rule no longer exists".to_owned())?;
            if rule.enabled {
                return Err("Hot Folder rule is already enabled".into());
            }
            rule.enabled = true;
            rule.validate().map_err(|reason| reason.to_owned())?;
            let capability = inputs.evaluate_rule(&rule, adapter.as_ref());
            let fingerprint = ConfigurationFingerprint::of_serializable(&rule)
                .map_err(|_| "Hot Folder rule could not be fingerprinted".to_owned())?;
            match capability {
                ActionCapability::Supported {
                    enablement: ActionEnablement::Standard,
                } => enable_prepared_rule(
                    &rule,
                    expected_revision,
                    None,
                    store,
                    inputs,
                    adapter.as_ref(),
                    control,
                    watcher,
                    engine,
                    control_gate,
                ),
                ActionCapability::Supported {
                    enablement: ActionEnablement::StrongConfirmationRequired,
                } => {
                    let preview = HotFolderEnablePreview {
                        request_id,
                        rule,
                        expected_revision,
                        resulting_revision: expected_revision
                            .checked_add(1)
                            .ok_or_else(|| "Hot Folder rules revision is exhausted".to_owned())?,
                        fingerprint,
                        capability,
                    };
                    if pending_enables.len() >= MAX_PENDING_ENABLE_PREVIEWS {
                        pending_enables.pop_front();
                    }
                    pending_enables.push_back(PreparedEnable { preview });
                    Ok(())
                }
                unsupported => Err(capability_message(&unsupported)),
            }
        }
        DriverOperation::ConfirmEnable {
            preview_id,
            confirmation,
        } => {
            let Some(index) = pending_enables
                .iter()
                .position(|prepared| prepared.preview.request_id == preview_id)
            else {
                return Err("Hot Folder confirmation expired; review the rule again".into());
            };
            let prepared = pending_enables.remove(index).unwrap();
            if !*plugin_enabled || *paused {
                return Err("Hot Folder is paused or disabled".into());
            }
            let snapshot = store.snapshot();
            if snapshot.revision != prepared.preview.expected_revision {
                return Err("Hot Folder rules changed; review the current rule again".into());
            }
            if ConfigurationFingerprint::of_serializable(&prepared.preview.rule)
                .map_err(|_| "Hot Folder rule could not be fingerprinted".to_owned())?
                != prepared.preview.fingerprint
            {
                return Err("Hot Folder rule changed; review the current rule again".into());
            }
            let capability = inputs.evaluate_rule(&prepared.preview.rule, adapter.as_ref());
            if capability != prepared.preview.capability {
                return Err(capability_message(&capability));
            }
            enable_prepared_rule(
                &prepared.preview.rule,
                prepared.preview.expected_revision,
                Some(confirmation),
                store,
                inputs,
                adapter.as_ref(),
                control,
                watcher,
                engine,
                control_gate,
            )
        }
        DriverOperation::CancelEnable { preview_id } => {
            pending_enables.retain(|prepared| prepared.preview.request_id != preview_id);
            Ok(())
        }
        DriverOperation::Disable {
            expected_revision,
            rule_id,
        } => {
            ensure_store_ready(store, store_loaded)?;
            let snapshot = store.snapshot();
            if snapshot.revision != expected_revision {
                return Err("Hot Folder rules changed; retry with the current revision".into());
            }
            let mut rule = snapshot
                .rules
                .iter()
                .find(|rule| rule.id == rule_id)
                .cloned()
                .ok_or_else(|| "Hot Folder rule no longer exists".to_owned())?;
            rule.enabled = false;
            let snapshot = store
                .replace_rule(expected_revision, rule)
                .map_err(store_error)?;
            pending_enables.retain(|prepared| prepared.preview.rule.id != rule_id);
            reconcile_after_edit(
                &snapshot,
                inputs,
                adapter.as_ref(),
                control,
                watcher,
                engine,
                *paused,
                control_gate,
            )?;
            Ok(())
        }
        DriverOperation::Pause => {
            *paused = true;
            if let Some(watcher) = watcher.as_mut() {
                watcher.pause().map_err(|error| format!("{error:?}"))?;
            }
            if let Some(engine) = engine.as_ref() {
                engine.pause_after_admission_closed();
            }
            pending_enables.clear();
            Ok(())
        }
        DriverOperation::Resume => {
            if !*plugin_enabled {
                return Err("Hot Folder is disabled".into());
            }
            ensure_store_ready(store, store_loaded)?;
            let snapshot = store.snapshot();
            if watcher.is_none() {
                *watcher = Some(HotFolderWatchService::new());
            }
            let watcher_ref = watcher.as_mut().expect("watcher was created above");
            inputs
                .with_context(&snapshot, adapter.as_ref(), |context| {
                    watcher_ref.resume_enabled_rules(&snapshot, &context)
                })
                .map_err(|error| format!("{error:?}"))?;
            let engine_ref = engine
                .as_ref()
                .ok_or_else(|| "Hot Folder execution engine is unavailable".to_owned())?;
            engine_ref.resume_with_control_gate(control_gate);
            *paused = false;
            Ok(())
        }
        DriverOperation::ClearHistory => engine
            .as_ref()
            .ok_or_else(|| "Hot Folder execution history is unavailable".to_owned())?
            .clear_history()
            .map_err(|error| format!("{error:?}")),
    }
}

fn ensure_store_loaded(
    store: &crate::hotfolder::HotFolderRulesStore,
    loaded: &mut bool,
    diagnostic: &mut Option<String>,
) {
    if *loaded {
        return;
    }
    let report = store.load();
    *diagnostic = report.diagnostic;
    *loaded = true;
}

fn ensure_store_ready(
    store: &crate::hotfolder::HotFolderRulesStore,
    loaded: &bool,
) -> Result<(), String> {
    if !loaded {
        return Err("Hot Folder rules have not been loaded".into());
    }
    let snapshot = store.snapshot();
    if snapshot.is_write_blocked() {
        return Err(snapshot
            .write_block_reason()
            .unwrap_or("Hot Folder rules are read-only")
            .to_owned());
    }
    Ok(())
}

fn enable_prepared_rule(
    rule: &HotFolderRule,
    expected_revision: u64,
    confirmation: Option<StrongActionConfirmation>,
    store: &crate::hotfolder::HotFolderRulesStore,
    inputs: &HotFolderCapabilityInputs,
    adapter: &dyn crate::hotfolder::HotFolderExternalActionAdapter,
    control: &HotFolderExecutionControl,
    watcher: &mut Option<HotFolderWatchService>,
    engine: &mut Option<HotFolderExecutionEngine>,
    control_gate: u64,
) -> Result<(), String> {
    ensure_store_ready(store, &true)?;
    if !rule.enabled {
        return Err("Hot Folder rule is no longer eligible for enable".into());
    }
    let capability = inputs.evaluate_rule(rule, adapter);
    if !matches!(capability, ActionCapability::Supported { .. }) {
        return Err(capability_message(&capability));
    }
    let snapshot = store
        .replace_rule(expected_revision, rule.clone())
        .map_err(store_error)?;
    let request = HotFolderEnableRequest::for_rule(&snapshot, &rule.id)
        .ok_or_else(|| "Hot Folder enable request became stale".to_owned())?;
    let request = match confirmation {
        Some(confirmation) => request.with_explicit_strong_confirmation(confirmation),
        None => request,
    };
    if watcher.is_none() {
        *watcher = Some(HotFolderWatchService::new());
    }
    let watcher_ref = watcher.as_mut().expect("watcher was created above");
    inputs
        .with_context(&snapshot, adapter, |context| {
            watcher_ref.enable_rule(&snapshot, request, &context)
        })
        .map_err(|error| {
            let message = format!("{error:?}");
            let mut disabled = rule.clone();
            disabled.enabled = false;
            if let Ok(rolled_back) = store.replace_rule(snapshot.revision, disabled) {
                let _ = reconcile_after_edit(
                    &rolled_back,
                    inputs,
                    adapter,
                    control,
                    watcher,
                    engine,
                    false,
                    control_gate,
                );
            }
            message
        })?;
    if let Some(engine) = engine.as_ref() {
        engine.reconcile_rules(&snapshot);
    }
    control.allow_admission_if_unchanged(control_gate);
    Ok(())
}

fn reconcile_after_edit(
    snapshot: &crate::hotfolder::HotFolderRulesSnapshot,
    inputs: &HotFolderCapabilityInputs,
    adapter: &dyn crate::hotfolder::HotFolderExternalActionAdapter,
    control: &HotFolderExecutionControl,
    watcher: &mut Option<HotFolderWatchService>,
    engine: &mut Option<HotFolderExecutionEngine>,
    paused: bool,
    control_gate: u64,
) -> Result<(), String> {
    if let (Some(watcher), Some(engine)) = (watcher.as_mut(), engine.as_ref()) {
        inputs
            .with_context(snapshot, adapter, |context| {
                watcher.reconcile_rules(snapshot, &context)
            })
            .map_err(|error| format!("{error:?}"))?;
        engine.reconcile_rules(snapshot);
        if !paused {
            control.allow_admission_if_unchanged(control_gate);
        }
    }
    Ok(())
}

fn stop_services(
    watcher: &mut Option<HotFolderWatchService>,
    engine: &mut Option<HotFolderExecutionEngine>,
) {
    if let Some(engine) = engine.as_ref() {
        engine.shutdown();
    }
    if let Some(watcher) = watcher.as_mut() {
        let _ = watcher.shutdown();
    }
    *watcher = None;
    *engine = None;
}

fn publish_driver_snapshot(
    published: &Arc<RwLock<HotFolderDriverSnapshot>>,
    plugin_enabled: bool,
    loaded: bool,
    paused: bool,
    store: &crate::hotfolder::HotFolderRulesStore,
    watcher: &Option<HotFolderWatchService>,
    engine: &Option<HotFolderExecutionEngine>,
    error: Option<&str>,
    acknowledgements: &VecDeque<HotFolderDriverAcknowledgement>,
    pending_enables: &VecDeque<PreparedEnable>,
) {
    let snapshot = HotFolderDriverSnapshot {
        plugin_enabled,
        loaded,
        running: watcher.is_some() && engine.is_some() && plugin_enabled,
        paused,
        store: loaded.then(|| store.snapshot()),
        runtime: watcher.as_ref().map(HotFolderWatchService::snapshot),
        execution: engine.as_ref().map(HotFolderExecutionEngine::snapshot),
        admission_blocked: false,
        execution_progress: HotFolderExecutionProgress::default(),
        error: error.map(str::to_owned),
        acknowledgements: acknowledgements.clone(),
        enable_previews: pending_enables
            .iter()
            .map(|prepared| prepared.preview.clone())
            .collect(),
    };
    if let Ok(mut current) = published.write() {
        *current = snapshot;
    }
}

fn store_error(error: HotFolderStoreError) -> String {
    error.to_string()
}

fn capability_message(capability: &ActionCapability) -> String {
    match capability {
        ActionCapability::Supported { .. } => "Hot Folder action is available".into(),
        ActionCapability::Unsupported { reason }
        | ActionCapability::Unavailable { reason }
        | ActionCapability::Invalid { reason } => (*reason).to_owned(),
    }
}

fn inspect_directory(path: &Path) -> RootAvailability {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => RootAvailability::ExistingDirectory,
        Ok(_) => RootAvailability::NotDirectory,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => RootAvailability::Missing,
        Err(_) => RootAvailability::Unreadable,
    }
}

/// A small pure sample evaluator shared by the dialog's isolated read-only
/// sample worker. It never receives the Store, watcher, executor, or adapter.
pub(crate) fn evaluate_sample_conditions(
    rule: &HotFolderRule,
    event: HotFolderEventKind,
    relative_path: &Path,
) -> Result<(), &'static str> {
    evaluate_rule_conditions(rule, event, relative_path)
}

impl super::LauncherApp {
    pub(crate) fn hotfolder_capability_inputs(&self) -> HotFolderCapabilityInputs {
        let catalog = self.universal_action_catalog_snapshot();
        HotFolderCapabilityInputs {
            launcher_targets: catalog.entries,
            mkmacro_document: (*self.plugins.internal_services().mkmacro_store.snapshot()).clone(),
        }
    }

    pub(crate) fn open_hotfolder_manager(&mut self) {
        self.hotfolder_dialog.open();
        self.egui_ctx.request_repaint();
    }

    pub(crate) fn render_hotfolder_manager(&mut self, ctx: &eframe::egui::Context, dispatch: bool) {
        let snapshot = self.hotfolder_runtime.snapshot();
        let root_visible = self.visible_flag.load(std::sync::atomic::Ordering::Relaxed);
        if !self
            .hotfolder_dialog
            .needs_runtime_poll(&snapshot, root_visible)
        {
            return;
        }
        if self.hotfolder_dialog.needs_picker_snapshot() {
            let inputs = self.hotfolder_capability_inputs();
            self.hotfolder_dialog
                .set_picker_snapshot(inputs.launcher_targets, inputs.mkmacro_document);
        }
        let actions = self.hotfolder_dialog.ui(ctx, &snapshot);
        if dispatch {
            self.dispatch_hotfolder_dialog_actions(
                actions,
                snapshot.store.as_ref().map(|s| s.revision),
            );
        }
    }

    fn dispatch_hotfolder_dialog_actions(
        &mut self,
        actions: Vec<super::hotfolder_dialog::HotFolderDialogAction>,
        current_revision: Option<u64>,
    ) {
        use super::hotfolder_dialog::{HotFolderDialogAction as Action, PendingKind};

        for action in actions {
            let (request, kind, undo_rule) = match action {
                Action::EnsureLoaded => (
                    self.hotfolder_runtime.ensure_loaded(),
                    PendingKind::Load,
                    None,
                ),
                Action::SaveNew {
                    expected_revision,
                    rule,
                } => (
                    self.hotfolder_runtime.save_new(
                        expected_revision,
                        rule,
                        self.hotfolder_capability_inputs(),
                    ),
                    PendingKind::Save,
                    None,
                ),
                Action::SaveEdit {
                    expected_revision,
                    rule,
                } => (
                    self.hotfolder_runtime.save_edit(
                        expected_revision,
                        rule,
                        self.hotfolder_capability_inputs(),
                    ),
                    PendingKind::Save,
                    None,
                ),
                Action::Delete {
                    expected_revision,
                    rule,
                } => (
                    self.hotfolder_runtime.delete(
                        expected_revision,
                        rule.id.clone(),
                        self.hotfolder_capability_inputs(),
                    ),
                    PendingKind::Delete,
                    Some(rule),
                ),
                Action::UndoDelete(rule) => match current_revision {
                    Some(revision) => (
                        self.hotfolder_runtime.save_new(
                            revision,
                            rule,
                            self.hotfolder_capability_inputs(),
                        ),
                        PendingKind::UndoDelete,
                        None,
                    ),
                    None => {
                        self.hotfolder_dialog
                            .submission_failed("Rules are not loaded yet");
                        continue;
                    }
                },
                Action::PrepareEnable {
                    expected_revision,
                    rule_id,
                } => (
                    self.hotfolder_runtime.prepare_enable(
                        expected_revision,
                        rule_id,
                        self.hotfolder_capability_inputs(),
                    ),
                    PendingKind::PrepareEnable,
                    None,
                ),
                Action::ConfirmEnable {
                    preview_id,
                    confirmation,
                } => (
                    self.hotfolder_runtime.confirm_enable(
                        preview_id,
                        confirmation,
                        self.hotfolder_capability_inputs(),
                    ),
                    PendingKind::ConfirmEnable,
                    None,
                ),
                Action::CancelEnable(preview_id) => (
                    self.hotfolder_runtime.cancel_enable(preview_id),
                    PendingKind::CancelEnable,
                    None,
                ),
                Action::Disable {
                    expected_revision,
                    rule_id,
                } => (
                    self.hotfolder_runtime.disable(
                        expected_revision,
                        rule_id,
                        self.hotfolder_capability_inputs(),
                    ),
                    PendingKind::Disable,
                    None,
                ),
                Action::Pause => (self.hotfolder_runtime.pause(), PendingKind::Pause, None),
                Action::Resume => (
                    self.hotfolder_runtime
                        .resume(self.hotfolder_capability_inputs()),
                    PendingKind::Resume,
                    None,
                ),
                Action::ClearHistory => (
                    self.hotfolder_runtime.clear_history(),
                    PendingKind::ClearHistory,
                    None,
                ),
                Action::TestRule {
                    editor_id,
                    rule,
                    sample,
                    event,
                    draft_revision,
                } => {
                    let inputs = self.hotfolder_capability_inputs();
                    let adapter = self.hotfolder_external_action_adapter();
                    let unhandled = self.hotfolder_dialog.start_sample_test_from_action(
                        Action::TestRule {
                            editor_id,
                            rule,
                            sample,
                            event,
                            draft_revision,
                        },
                        inputs,
                        adapter,
                    );
                    debug_assert!(
                        unhandled.is_none(),
                        "Test Rule is handled in its read-only worker"
                    );
                    continue;
                }
            };
            match request {
                Ok(request_id) => self
                    .hotfolder_dialog
                    .note_request(request_id, kind, undo_rule),
                Err(error) => self.hotfolder_dialog.submission_failed(error),
            }
        }
    }
}

#[cfg(test)]
mod m10e_driver_lifecycle_fixtures {
    use super::*;
    use crate::hotfolder::{
        ConfigurationFingerprint, HotFolderExecutionPhase, HotFolderExternalActionOutcome,
        HotFolderExternalActionRequest, HotFolderExternalEnableAuthorization,
        HotFolderExternalFileEvidence, HotFolderRule, HotFolderTerminalStatus, RuleAction,
    };
    #[cfg(windows)]
    use crate::hotfolder::{RuleId, SettlePolicy, StrongActionConfirmation};
    use crate::mkmacro::{MkMacro, MkMacroDocument};
    #[cfg(windows)]
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{Receiver, SyncSender};
    use std::time::Instant;

    #[cfg(feature = "perf-harness")]
    #[test]
    fn perf_harness_idle_fixtures_validate_and_reap_without_external_dispatch() {
        use crate::perf_harness::Scenario;

        let temporary = tempfile::tempdir().expect("temporary fixture root");
        for (index, scenario) in [
            Scenario::HotfolderDisabled,
            Scenario::HotfolderEnabledZeroRules,
        ]
        .into_iter()
        .enumerate()
        {
            let run_dir = temporary.path().join(format!("run-{index}"));
            std::fs::create_dir(&run_dir).expect("create owned run directory");
            let mut fixture = HotFolderIdleFixture::new(&run_dir, scenario)
                .expect("construct inert Hot Folder driver fixture");
            fixture.validate().expect("fixture state remains empty");
            assert_eq!(fixture.adapter.dispatches.load(Ordering::SeqCst), 0);
            fixture.shutdown();
            assert!(fixture.driver.worker.is_none());
            assert!(fixture.driver.reaper.is_none());
        }

        let preexisting_run = temporary.path().join("preexisting");
        std::fs::create_dir(&preexisting_run).expect("create run directory");
        let existing_fixture = preexisting_run.join("fixture");
        std::fs::create_dir(&existing_fixture).expect("create pre-existing fixture directory");
        let sentinel = existing_fixture.join("settings.json");
        std::fs::write(&sentinel, b"owned sentinel").expect("write sentinel");
        assert!(HotFolderIdleFixture::new(&preexisting_run, Scenario::HotfolderDisabled).is_err());
        assert_eq!(
            std::fs::read(sentinel).expect("sentinel remains untouched"),
            b"owned sentinel"
        );
    }

    #[cfg(windows)]
    struct NativeOnlyAdapter;

    #[cfg(windows)]
    impl crate::hotfolder::HotFolderExternalActionAdapter for NativeOnlyAdapter {
        fn supports(&self, _action: &RuleAction) -> bool {
            false
        }

        fn dispatch(
            &self,
            _request: HotFolderExternalActionRequest,
            _cancellation: &crate::file_operations::ScanCancellation,
        ) -> HotFolderExternalActionOutcome {
            panic!("built-in Hot Folder actions must stay on the native file-operation path")
        }
    }

    struct BlockingExternalAdapter {
        started: SyncSender<u128>,
        release: Mutex<Receiver<()>>,
        dispatches: AtomicUsize,
    }

    impl crate::hotfolder::HotFolderExternalActionAdapter for BlockingExternalAdapter {
        fn supports(&self, action: &RuleAction) -> bool {
            matches!(action, RuleAction::MkMacro { .. })
        }

        fn dispatch(
            &self,
            request: HotFolderExternalActionRequest,
            cancellation: &crate::file_operations::ScanCancellation,
        ) -> HotFolderExternalActionOutcome {
            let dispatch_id = self.dispatches.fetch_add(1, Ordering::SeqCst) + 1;
            self.started
                .send(request.operation_id)
                .expect("fixture receives dispatch start");
            let _ = self
                .release
                .lock()
                .expect("fixture release lock")
                .recv_timeout(Duration::from_secs(10));
            let cancelled = cancellation.is_cancelled();
            HotFolderExternalActionOutcome {
                operation_id: request.operation_id,
                macro_run_id: Some(dispatch_id as u64),
                status: if cancelled {
                    HotFolderTerminalStatus::Cancelled
                } else {
                    HotFolderTerminalStatus::Success
                },
                phase: HotFolderExecutionPhase::Complete,
                reason: cancelled.then(|| "fixture cancellation reached owned operation".into()),
            }
        }
    }

    fn wait_for_acknowledgement(
        driver: &HotFolderRuntimeDriver,
        request_id: u64,
    ) -> HotFolderDriverSnapshot {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let snapshot = driver.snapshot();
            if snapshot
                .acknowledgements
                .iter()
                .any(|ack| ack.request_id == request_id)
            {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "driver request was not acknowledged"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(windows)]
    fn wait_for_successful_acknowledgement(
        driver: &HotFolderRuntimeDriver,
        request_id: u64,
        operation: &str,
    ) -> HotFolderDriverSnapshot {
        let snapshot = wait_for_acknowledgement(driver, request_id);
        let acknowledgement = snapshot
            .acknowledgements
            .iter()
            .find(|ack| ack.request_id == request_id)
            .expect("submitted runtime operation is acknowledged");
        assert!(
            acknowledgement.result.is_ok(),
            "{operation} succeeds: {:?}",
            acknowledgement.result
        );
        snapshot
    }

    #[cfg(windows)]
    fn wait_for_completion_count(
        driver: &HotFolderRuntimeDriver,
        expected: u64,
    ) -> HotFolderDriverSnapshot {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = driver.snapshot();
            if snapshot
                .execution
                .as_ref()
                .is_some_and(|execution| execution.completed_runs >= expected)
            {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "native fixture action did not settle and complete"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(windows)]
    fn observe_no_new_completions(
        driver: &HotFolderRuntimeDriver,
        expected: u64,
        duration: Duration,
    ) -> HotFolderDriverSnapshot {
        let deadline = Instant::now() + duration;
        loop {
            let snapshot = driver.snapshot();
            if let Some(execution) = snapshot.execution.as_ref() {
                assert_eq!(
                    execution.completed_runs, expected,
                    "one file event must produce at most one terminal run"
                );
            }
            if Instant::now() >= deadline {
                return snapshot;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(windows)]
    fn observe_file_unmoved(source: &Path, destination: &Path, duration: Duration) {
        let deadline = Instant::now() + duration;
        loop {
            assert!(
                source.exists(),
                "unadmitted fixture input remains in the watched directory"
            );
            assert!(
                !destination.exists(),
                "unadmitted fixture input is not moved"
            );
            if Instant::now() >= deadline {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(windows)]
    fn assert_fixture_path(actual: &Path, expected: &Path) {
        assert_eq!(actual.file_name(), expected.file_name());
        let actual_parent = std::fs::canonicalize(actual.parent().expect("actual fixture parent"))
            .expect("canonical actual fixture parent");
        let expected_parent =
            std::fs::canonicalize(expected.parent().expect("expected fixture parent"))
                .expect("canonical expected fixture parent");
        assert_eq!(
            crate::batch_rename::compare_paths_windows_ordinal(&actual_parent, &expected_parent),
            std::cmp::Ordering::Equal,
            "native receipt path remains inside the selected fixture directory"
        );
    }

    #[cfg(windows)]
    fn assert_successful_move(
        snapshot: &HotFolderDriverSnapshot,
        rule_id: &RuleId,
        source: &Path,
        destination: &Path,
        expected_bytes: &[u8],
    ) {
        let execution = snapshot
            .execution
            .as_ref()
            .expect("running execution history");
        let matching = execution
            .receipts
            .iter()
            .filter(|receipt| receipt.source_path.file_name() == source.file_name())
            .collect::<Vec<_>>();
        assert_eq!(
            matching.len(),
            1,
            "one source receives one terminal receipt"
        );
        let receipt = matching[0];
        assert_fixture_path(&receipt.source_path, source);
        assert_eq!(receipt.rule_id, *rule_id);
        assert_eq!(
            receipt.action,
            crate::hotfolder::HotFolderExecutionAction::Move
        );
        assert_eq!(receipt.status, HotFolderTerminalStatus::Success);
        assert_eq!(receipt.phase, HotFolderExecutionPhase::Complete);
        assert!(receipt.durable, "the terminal receipt is persisted");
        assert_eq!(receipt.source_length, expected_bytes.len() as u64);
        assert_eq!(receipt.outputs.len(), 1);
        assert_fixture_path(&receipt.outputs[0].path, destination);
        assert_eq!(
            std::fs::read(destination).expect("native move published the fixture bytes"),
            expected_bytes
        );
        assert!(!source.exists(), "successful move removes the owned source");
    }

    #[test]
    fn m10e_driver_reuses_service_and_restores_enabled_intent_without_backfill() {
        let fixture_parent = std::env::current_dir()
            .expect("resolve repository root")
            .join("target/productivity_expansion");
        std::fs::create_dir_all(&fixture_parent).expect("create disposable fixture parent");
        let temporary = tempfile::Builder::new()
            .prefix("hotfolder_driver_")
            .tempdir_in(&fixture_parent)
            .expect("fixture root");
        let fixture_root = temporary.path().join("hotfolder_driver");
        let watched = fixture_root.join("incoming");
        std::fs::create_dir_all(&watched).expect("create watched directory");
        let existing = watched.join("already-present.txt");
        std::fs::write(&existing, b"must not be backfilled").expect("create preexisting sample");

        let macro_document = MkMacroDocument {
            macros: vec![MkMacro {
                id: 901,
                name: "fixture macro".into(),
                description: String::new(),
                enabled: true,
                hotkey: None,
                hotkey_scope: Default::default(),
                folder_id: None,
                playback: Default::default(),
                signature: Default::default(),
                steps: Vec::new(),
            }],
            ..Default::default()
        };
        let action = crate::hotfolder::bind_mkmacro_action(&macro_document, 901)
            .expect("fixture macro binds through the exact current definition");
        let (started_tx, started_rx) = mpsc::sync_channel(4);
        let (release_tx, release_rx) = mpsc::channel();
        let adapter = Arc::new(BlockingExternalAdapter {
            started: started_tx,
            release: Mutex::new(release_rx),
            dispatches: AtomicUsize::new(0),
        });

        let data_root = AppDataRoot::from_path(fixture_root.join("app_data"));
        let store = crate::hotfolder::HotFolderRulesStore::new(&data_root);
        let empty = store.load();
        let enabled_rule = HotFolderRule::new("restored macro", &watched, action);
        let disabled = store
            .add_rule(empty.snapshot.revision, enabled_rule)
            .expect("persist disabled macro rule");
        let mut prior_intent = disabled.rules[0].clone();
        prior_intent.enabled = true;
        let prior_snapshot = store
            .replace_rule(disabled.revision, prior_intent)
            .expect("persist prior enabled intent");

        let inputs = HotFolderCapabilityInputs {
            launcher_targets: Vec::new(),
            mkmacro_document: macro_document,
        };
        assert!(prior_snapshot.rules[0].enabled);
        assert!(
            matches!(
                inputs.evaluate_rule(&prior_snapshot.rules[0], adapter.as_ref()),
                ActionCapability::Supported { .. }
            ),
            "fixture macro must pass the same current capability validation used by restore: {:?}",
            inputs.evaluate_rule(&prior_snapshot.rules[0], adapter.as_ref())
        );
        let mut driver = HotFolderRuntimeDriver::new(data_root, adapter.clone());
        let first_start = driver.start(inputs.clone()).expect("submit first start");
        let first_snapshot = wait_for_acknowledgement(&driver, first_start);
        assert!(first_snapshot.running);
        assert_eq!(
            first_snapshot
                .runtime
                .as_ref()
                .map(|runtime| (runtime.active_roots, runtime.active_rules)),
            Some((1, 1)),
            "fresh capability validation restores the enabled rule"
        );

        let second_start = driver
            .start(inputs.clone())
            .expect("submit repeated manager open");
        let repeated_snapshot = wait_for_acknowledgement(&driver, second_start);
        assert_eq!(
            repeated_snapshot
                .runtime
                .as_ref()
                .map(|runtime| (runtime.active_roots, runtime.active_rules)),
            Some((1, 1)),
            "repeated open reuses the one app-owned watcher"
        );

        thread::sleep(Duration::from_millis(1_200));
        assert!(existing.exists());
        assert_eq!(
            adapter.dispatches.load(Ordering::SeqCst),
            0,
            "startup does not scan or backfill a file that predates the watcher"
        );

        let current = driver.snapshot();
        let revision = current.store.as_ref().expect("loaded store").revision;
        let stale_save = driver
            .save_edit(
                revision.saturating_sub(1),
                current.store.as_ref().unwrap().rules[0].clone(),
                inputs.clone(),
            )
            .expect("stale save is submitted asynchronously");
        let stale_snapshot = wait_for_acknowledgement(&driver, stale_save);
        let stale_ack = stale_snapshot
            .acknowledgements
            .iter()
            .find(|ack| ack.request_id == stale_save)
            .expect("stale save acknowledgement");
        assert!(
            stale_ack.result.is_err(),
            "stale revision is rejected by the store owner"
        );
        assert!(
            !stale_snapshot.admission_blocked,
            "failed mutation restores only its own still-current admission generation"
        );

        let first_new = watched.join("new-one.txt");
        std::fs::write(&first_new, b"first settled input").expect("create first reported file");
        started_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("first settled input reaches the single execution owner");
        let live = driver.snapshot();
        assert!(live.execution_progress.operation_id.is_some());

        let dialog_context = eframe::egui::Context::default();
        let mut dialog = crate::gui::hotfolder_dialog::HotFolderDialog::default();
        dialog.open();
        let frame_started = Instant::now();
        dialog_context
            .run_ui(eframe::egui::RawInput::default(), |ctx| {
                let _ = dialog.ui(ctx, &live);
            })
            .drop_without_applying_deltas();
        assert!(
            frame_started.elapsed() < Duration::from_millis(500),
            "an active synchronous C operation leaves an egui frame responsive"
        );

        let queued = (0..DRIVER_QUEUE_CAPACITY)
            .map(|_| driver.ensure_loaded().expect("fill bounded control queue"))
            .collect::<Vec<_>>();
        let full_queue = driver.save_edit(
            revision,
            live.store.as_ref().unwrap().rules[0].clone(),
            inputs.clone(),
        );
        assert_eq!(full_queue, Err("Hot Folder control queue is full"));
        let after_queue_rejection = driver.snapshot();
        assert!(
            !after_queue_rejection.admission_blocked,
            "queue rejection restores admission only while its generation remains current"
        );
        assert!(
            after_queue_rejection
                .execution_progress
                .cancellation_requested,
            "rejected mutation still truthfully reports cancellation of the owned active run"
        );
        release_tx.send(()).expect("release first owned operation");
        let drained = wait_for_acknowledgement(&driver, *queued.last().expect("queue is nonempty"));
        assert_eq!(
            drained.execution.as_ref().unwrap().receipts.len(),
            1,
            "first blocked operation publishes one terminal receipt after cancellation"
        );

        let second_new = watched.join("new-two.txt");
        std::fs::write(&second_new, b"second settled input").expect("create second reported file");
        started_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("second settled input reaches the owner after the first terminal receipt");
        let second_live = driver.snapshot();
        assert!(second_live.execution_progress.operation_id.is_some());

        let updated_rule = second_live.store.as_ref().unwrap().rules[0].clone();
        let mut edited_rule = updated_rule;
        edited_rule.name = "saved before newer pause".into();
        let submission_started = Instant::now();
        let save_request = driver
            .save_edit(
                second_live.store.as_ref().unwrap().revision,
                edited_rule,
                inputs.clone(),
            )
            .expect("queue Save while the execution worker is busy");
        let pause_request = driver
            .pause()
            .expect("newer Pause closes admission immediately");
        assert!(
            submission_started.elapsed() < Duration::from_millis(500),
            "Save and Pause admission do not wait for the blocked C action"
        );
        let cancellation_snapshot = driver.snapshot();
        assert!(
            cancellation_snapshot
                .execution_progress
                .cancellation_requested
        );
        let cancellation_frame_started = Instant::now();
        dialog_context
            .run_ui(eframe::egui::RawInput::default(), |ctx| {
                let _ = dialog.ui(ctx, &cancellation_snapshot);
            })
            .drop_without_applying_deltas();
        assert!(
            cancellation_frame_started.elapsed() < Duration::from_millis(500),
            "the GUI can render an in-flight cancellation request without taking the C mutex"
        );
        release_tx.send(()).expect("release second owned operation");
        let paused = wait_for_acknowledgement(&driver, pause_request);
        assert!(
            paused
                .acknowledgements
                .iter()
                .any(|ack| ack.request_id == save_request && ack.result.is_ok())
        );
        assert!(
            paused
                .acknowledgements
                .iter()
                .any(|ack| ack.request_id == pause_request && ack.result.is_ok())
        );
        assert!(paused.paused);
        assert!(
            paused.admission_blocked,
            "the older queued Save cannot reopen the newer Pause generation"
        );
        assert!(
            !paused.store.as_ref().unwrap().rules[0].enabled,
            "saving the changed definition leaves it disabled"
        );
        let terminal = paused.execution.as_ref().unwrap();
        assert_eq!(
            terminal.receipts.len(),
            2,
            "each owned operation has one terminal receipt"
        );
        assert!(
            terminal
                .receipts
                .iter()
                .all(|receipt| receipt.status == HotFolderTerminalStatus::Cancelled)
        );
        assert_eq!(
            adapter.dispatches.load(Ordering::SeqCst),
            2,
            "the queued Save cannot admit a new action before the newer Pause is acknowledged"
        );

        // Model the main-thread completion queue at app exit. It must be
        // retired before the owned worker is joined so an outstanding reply
        // cannot hold the lifecycle boundary open.
        let macro_store = Arc::new(crate::mkmacro::MkMacroStore::empty_for_test(
            fixture_root.join("mkmacro_app_data"),
        ));
        let (launcher_adapter, pump) =
            crate::gui::hotfolder_dispatch::LauncherHotFolderActionAdapter::new(
                eframe::egui::Context::default(),
                macro_store,
            );
        let receipt_rule = paused.store.as_ref().unwrap().rules[0].clone();
        let rule_fingerprint = ConfigurationFingerprint::of_serializable(&receipt_rule)
            .expect("current fixture rule fingerprints");
        let pending_reply = launcher_adapter
            .enqueue_launcher_request(HotFolderExternalActionRequest {
                operation_id: 8181,
                candidate_id: crate::hotfolder::HotFolderCandidateId::from_persisted(1),
                rule_id: receipt_rule.id.clone(),
                rule_fingerprint: rule_fingerprint.clone(),
                action: RuleAction::MkMacro {
                    macro_id: 901,
                    configuration_fingerprint: rule_fingerprint.clone(),
                },
                authorization: HotFolderExternalEnableAuthorization {
                    store_revision: paused.store.as_ref().unwrap().revision,
                    epoch: 1,
                    rule_fingerprint,
                    enablement: crate::hotfolder::ActionEnablement::StrongConfirmationRequired,
                },
                file: HotFolderExternalFileEvidence {
                    path: first_new,
                    root_identity: Default::default(),
                    parent_identity: Default::default(),
                    file_identity: Default::default(),
                    length: 0,
                    modified: std::time::UNIX_EPOCH,
                    sha256: None,
                },
            })
            .expect("main-thread reply request is pending");
        let reply_thread =
            thread::spawn(move || pending_reply.receive_timeout(Duration::from_secs(5)));
        launcher_adapter.set_active(false);
        pump.set_active(false);
        let retired_reply = reply_thread
            .join()
            .expect("reply waiter joined")
            .expect("retirement produces a terminal reply");
        assert_eq!(retired_reply.operation_id, 8181);
        assert_eq!(retired_reply.status, HotFolderTerminalStatus::Failure);

        driver.shutdown_and_reap();
        driver.join_reaper();
        assert!(driver.worker.is_none());
        assert!(
            driver.reaper.is_none(),
            "owned driver and reaper are joined at final shutdown"
        );
    }

    #[cfg(windows)]
    #[test]
    fn m10f_disabled_rule_requires_strong_enable_and_moves_only_new_settled_files_across_lifecycle()
    {
        let fixture_parent = std::env::current_dir()
            .expect("resolve repository root")
            .join("target/productivity_expansion");
        std::fs::create_dir_all(&fixture_parent).expect("create disposable fixture parent");
        let temporary = tempfile::Builder::new()
            .prefix("hotfolder_m10f_")
            .tempdir_in(&fixture_parent)
            .expect("fixture root under the repository-owned native-test directory");
        let fixture_root = temporary.path().join("hotfolder_m10f");
        let watched = fixture_root.join("incoming");
        let destination = fixture_root.join("moved");
        std::fs::create_dir_all(&watched).expect("create watched fixture directory");
        std::fs::create_dir_all(&destination).expect("create destination fixture directory");
        assert!(watched.starts_with(&fixture_parent));
        assert!(destination.starts_with(&fixture_parent));

        let preexisting = watched.join("before-enable.txt");
        let outside = fixture_root.join("outside-watched-root.txt");
        std::fs::write(&preexisting, b"already here").expect("create pre-enable input");
        std::fs::write(&outside, b"outside remains untouched").expect("create outside-root input");
        let data_root = AppDataRoot::from_path(fixture_root.join("app_data"));
        let store = crate::hotfolder::HotFolderRulesStore::new(&data_root);
        let missing = store.load();
        let mut rule = HotFolderRule::new(
            "M10-F fixture move",
            &watched,
            RuleAction::Move {
                destination: destination.clone(),
            },
        );
        rule.settle = SettlePolicy {
            sample_interval_ms: 50,
            stable_observations: 2,
            timeout_ms: 1_000,
        };
        let disabled = store
            .add_rule(missing.snapshot.revision, rule)
            .expect("persist a disabled move rule through the production store");
        let rule_id = disabled.rules[0].id.clone();
        assert!(
            !disabled.rules[0].enabled,
            "new rules are persisted disabled"
        );

        let inputs = HotFolderCapabilityInputs::empty();
        let adapter: Arc<dyn crate::hotfolder::HotFolderExternalActionAdapter> =
            Arc::new(NativeOnlyAdapter);
        let mut driver = HotFolderRuntimeDriver::new(data_root.clone(), Arc::clone(&adapter));
        let start_request = driver
            .start(inputs.clone())
            .expect("start the app-owned runtime");
        let started = wait_for_successful_acknowledgement(&driver, start_request, "runtime start");
        assert!(started.running);
        assert_eq!(
            started
                .runtime
                .as_ref()
                .map(|runtime| (runtime.active_roots, runtime.active_rules)),
            Some((0, 0)),
            "loading a disabled rule does not create a native subscription"
        );
        let created_while_disabled = watched.join("after-start-disabled.txt");
        let disabled_destination = destination.join("after-start-disabled.txt");
        std::fs::write(&created_while_disabled, b"created while disabled")
            .expect("create a new input after the disabled driver starts");
        observe_file_unmoved(
            &created_while_disabled,
            &disabled_destination,
            Duration::from_millis(350),
        );
        let disabled_idle = observe_no_new_completions(&driver, 0, Duration::from_millis(100));
        assert_eq!(
            disabled_idle
                .execution
                .as_ref()
                .map_or(0, |execution| execution.receipts.len()),
            0,
            "a new arrival while disabled produces no terminal receipt"
        );
        observe_file_unmoved(
            &preexisting,
            &destination.join("before-enable.txt"),
            Duration::from_millis(250),
        );

        let prepare_request = driver
            .prepare_enable(disabled.revision, rule_id.clone(), inputs.clone())
            .expect("submit the real validated strong-enable preflight");
        let prepared = wait_for_successful_acknowledgement(
            &driver,
            prepare_request,
            "strong-enable preflight",
        );
        let preview = prepared
            .enable_previews
            .iter()
            .find(|preview| preview.rule.id == rule_id)
            .expect("Move presents its current confirmation preview");
        assert!(matches!(
            &preview.capability,
            ActionCapability::Supported {
                enablement: ActionEnablement::StrongConfirmationRequired
            }
        ));
        assert_eq!(preview.expected_revision, disabled.revision);
        assert_eq!(preview.resulting_revision, disabled.revision + 1);
        let confirmation = StrongActionConfirmation::from_explicit_confirmation(
            preview.rule.id.clone(),
            preview.resulting_revision,
            preview.fingerprint.clone(),
        );
        let confirm_request = driver
            .confirm_enable(preview.request_id, confirmation, inputs.clone())
            .expect("submit the explicit revision-and-fingerprint-bound confirmation");
        let enabled = wait_for_successful_acknowledgement(
            &driver,
            confirm_request,
            "strong-enable confirmation",
        );
        assert!(enabled.store.as_ref().unwrap().rules[0].enabled);
        assert_eq!(
            enabled
                .runtime
                .as_ref()
                .map(|runtime| (runtime.active_roots, runtime.active_rules)),
            Some((1, 1)),
            "the validated confirmation activates exactly one native watched root and rule"
        );
        observe_file_unmoved(
            &preexisting,
            &destination.join("before-enable.txt"),
            Duration::from_millis(350),
        );
        observe_file_unmoved(
            &created_while_disabled,
            &disabled_destination,
            Duration::from_millis(350),
        );
        observe_no_new_completions(&driver, 0, Duration::from_millis(350));

        let first_name = "new-burst.txt";
        let first_source = watched.join(first_name);
        let first_destination = destination.join(first_name);
        let first_bytes = b"first-duplicate-one-duplicate-two-duplicate-three";
        std::fs::write(&first_source, b"first").expect("create one new event");
        let mut burst = std::fs::OpenOptions::new()
            .append(true)
            .open(&first_source)
            .expect("reopen the same input for a duplicate notification burst");
        for payload in [
            &b"-duplicate-one"[..],
            &b"-duplicate-two"[..],
            &b"-duplicate-three"[..],
        ] {
            burst.write_all(payload).expect("write burst payload");
        }
        burst.flush().expect("flush burst payload");
        drop(burst);
        let reached_first_completion = wait_for_completion_count(&driver, 1);
        assert_eq!(
            reached_first_completion
                .execution
                .as_ref()
                .unwrap()
                .completed_runs,
            1
        );
        let first_completed = observe_no_new_completions(&driver, 1, Duration::from_millis(300));
        assert_eq!(
            first_completed.execution.as_ref().unwrap().completed_runs,
            1
        );
        assert_successful_move(
            &first_completed,
            &rule_id,
            &first_source,
            &first_destination,
            first_bytes,
        );
        assert_eq!(
            first_completed.execution.as_ref().unwrap().receipts.len(),
            1
        );
        drop(first_completed);

        let pause_request = driver
            .pause()
            .expect("close admission and pause the watcher");
        let paused = wait_for_successful_acknowledgement(&driver, pause_request, "pause");
        assert!(paused.paused && paused.admission_blocked);
        let paused_source = watched.join("while-paused.txt");
        let paused_destination = destination.join("while-paused.txt");
        std::fs::write(&paused_source, b"paused input").expect("create input while paused");
        observe_file_unmoved(
            &paused_source,
            &paused_destination,
            Duration::from_millis(350),
        );
        observe_no_new_completions(&driver, 1, Duration::from_millis(100));

        let resume_request = driver
            .resume(inputs.clone())
            .expect("revalidate and resume in a fresh watcher epoch");
        let resumed = wait_for_successful_acknowledgement(&driver, resume_request, "resume");
        assert!(!resumed.paused);
        assert_eq!(
            resumed
                .runtime
                .as_ref()
                .map(|runtime| (runtime.active_roots, runtime.active_rules)),
            Some((1, 1))
        );
        observe_file_unmoved(
            &paused_source,
            &paused_destination,
            Duration::from_millis(350),
        );
        observe_no_new_completions(&driver, 1, Duration::from_millis(100));

        let second_name = "after-resume.txt";
        let second_source = watched.join(second_name);
        let second_destination = destination.join(second_name);
        let second_bytes = b"fresh resume epoch";
        std::fs::write(&second_source, second_bytes).expect("create a fresh post-resume input");
        let reached_second_completion = wait_for_completion_count(&driver, 2);
        assert_eq!(
            reached_second_completion
                .execution
                .as_ref()
                .unwrap()
                .completed_runs,
            2
        );
        let second_completed = observe_no_new_completions(&driver, 2, Duration::from_millis(300));
        assert_successful_move(
            &second_completed,
            &rule_id,
            &second_source,
            &second_destination,
            second_bytes,
        );
        assert_eq!(
            second_completed.execution.as_ref().unwrap().receipts.len(),
            2
        );
        drop(second_completed);

        let stop_request = driver.stop().expect("disable the plugin-owned watcher");
        let stopped = wait_for_successful_acknowledgement(&driver, stop_request, "plugin stop");
        assert!(!stopped.plugin_enabled && !stopped.running);
        let plugin_off_source = watched.join("while-plugin-off.txt");
        let plugin_off_destination = destination.join("while-plugin-off.txt");
        std::fs::write(&plugin_off_source, b"plugin-off input").expect("create plugin-off input");
        observe_file_unmoved(
            &plugin_off_source,
            &plugin_off_destination,
            Duration::from_millis(250),
        );

        driver.shutdown_and_reap();
        driver.join_reaper();
        assert!(driver.worker.is_none());
        assert!(
            driver.reaper.is_none(),
            "plugin-off driver worker and reaper are joined"
        );

        let shutdown_source = watched.join("while-shutdown.txt");
        let shutdown_destination = destination.join("while-shutdown.txt");
        std::fs::write(&shutdown_source, b"shutdown input").expect("create shutdown input");
        observe_file_unmoved(
            &shutdown_source,
            &shutdown_destination,
            Duration::from_millis(250),
        );

        let mut restarted = HotFolderRuntimeDriver::new(data_root, adapter);
        let restart_request = restarted
            .start(inputs)
            .expect("start a new app-owned driver on the same persisted state");
        let restored = wait_for_successful_acknowledgement(&restarted, restart_request, "restart");
        assert!(restored.plugin_enabled && restored.running);
        assert!(restored.store.as_ref().unwrap().rules[0].enabled);
        assert_eq!(
            restored
                .runtime
                .as_ref()
                .map(|runtime| (runtime.active_roots, runtime.active_rules)),
            Some((1, 1)),
            "only the previously confirmed, still-valid rule is restored"
        );
        assert_eq!(restored.execution.as_ref().unwrap().completed_runs, 2);
        for (source, target) in [
            (&preexisting, destination.join("before-enable.txt")),
            (&created_while_disabled, disabled_destination),
            (&paused_source, paused_destination),
            (&plugin_off_source, plugin_off_destination),
            (&shutdown_source, shutdown_destination),
        ] {
            observe_file_unmoved(source, &target, Duration::from_millis(100));
        }
        observe_no_new_completions(&restarted, 2, Duration::from_millis(250));

        let final_name = "after-restart.txt";
        let final_source = watched.join(final_name);
        let final_destination = destination.join(final_name);
        let final_bytes = b"new event after restart";
        std::fs::write(&final_source, final_bytes).expect("create a fresh post-restart input");
        let reached_final_completion = wait_for_completion_count(&restarted, 3);
        assert_eq!(
            reached_final_completion
                .execution
                .as_ref()
                .unwrap()
                .completed_runs,
            3
        );
        let final_completed = observe_no_new_completions(&restarted, 3, Duration::from_millis(300));
        assert_successful_move(
            &final_completed,
            &rule_id,
            &final_source,
            &final_destination,
            final_bytes,
        );
        assert_eq!(
            final_completed.execution.as_ref().unwrap().receipts.len(),
            3
        );
        assert_eq!(
            std::fs::read(&outside).expect("outside watched root remains readable"),
            b"outside remains untouched",
            "the selected action never mutates a source outside its watched root"
        );

        restarted.shutdown_and_reap();
        restarted.join_reaper();
        assert!(restarted.worker.is_none());
        assert!(
            restarted.reaper.is_none(),
            "restarted driver exits with no detached worker"
        );
    }
}
