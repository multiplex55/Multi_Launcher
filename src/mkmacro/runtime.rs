//! Worker-owned macro runtime.  The public methods only exchange messages and snapshots;
//! action execution and all waits live on the worker.
use super::{MkMacroStore, compile, executor::*};
use super::{
    NormalizationConfig, Operation, RecorderRuntime, RecorderSnapshot, RecordingResult,
    SharedOperationGuard, SystemRecorderClock, production_hook_service,
};
use anyhow::{Result, anyhow};
use once_cell::sync::Lazy;
use std::{
    collections::{BTreeMap, HashSet},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::SystemTime,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCommand {
    Run(u64),
    RunFrom(u64, u64),
    RunSelection(u64, Vec<u64>),
    Pause,
    Resume,
    Stop,
    Shutdown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeState {
    Idle,
    Running,
    Paused,
    Stopping,
    Completed,
    Stopped,
    Failed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Pending,
    Running,
    Success,
    Skipped,
    Failed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiagnosticKey {
    pub run_id: u64,
    pub step_id: u64,
}
#[derive(Debug, Clone)]
pub struct RuntimeSnapshot {
    pub state: RuntimeState,
    pub run_id: u64,
    pub macro_id: Option<u64>,
    pub step_id: Option<u64>,
    pub completed_steps: usize,
    pub total_steps: usize,
    pub started_at: Option<SystemTime>,
    pub finished_at: Option<SystemTime>,
    pub latest_failure: Option<ExecutionDiagnostic>,
    /// Transient results, deliberately held outside the persisted macro document.
    pub failures: Arc<BTreeMap<DiagnosticKey, ExecutionDiagnostic>>,
    pub steps: Arc<BTreeMap<u64, StepState>>,
    pub step_outcomes: Arc<BTreeMap<u64, StepOutcome>>,
    pub revision: u64,
}
impl Default for RuntimeSnapshot {
    fn default() -> Self {
        Self {
            state: RuntimeState::Idle,
            run_id: 0,
            macro_id: None,
            step_id: None,
            completed_steps: 0,
            total_steps: 0,
            started_at: None,
            finished_at: None,
            latest_failure: None,
            failures: Arc::new(BTreeMap::new()),
            steps: Arc::new(BTreeMap::new()),
            step_outcomes: Arc::new(BTreeMap::new()),
            revision: 0,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandResult {
    Accepted,
    AlreadyRunning { active_macro_id: u64 },
    Rejected(ExecutionDiagnostic),
}

struct Shared {
    snapshot: RwLock<Arc<RuntimeSnapshot>>,
    control: Arc<RunControl>,
    admission: Mutex<Option<u64>>,
    next_run_id: AtomicU64,
    operations: Arc<SharedOperationGuard>,
}
pub struct MacroRuntime {
    tx: mpsc::Sender<RuntimeCommand>,
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl MacroRuntime {
    pub fn new(store: Arc<MkMacroStore>, backends: Backends) -> Self {
        Self::with_guard(store, backends, Arc::new(SharedOperationGuard::default()))
    }
    fn with_guard(
        store: Arc<MkMacroStore>,
        backends: Backends,
        operations: Arc<SharedOperationGuard>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Shared {
            snapshot: RwLock::new(Arc::new(RuntimeSnapshot::default())),
            control: Arc::new(RunControl::default()),
            admission: Mutex::new(None),
            next_run_id: AtomicU64::new(1),
            operations,
        });
        let s = shared.clone();
        let worker = thread::Builder::new()
            .name("mkmacro-runtime".into())
            .spawn(move || worker_loop(store, backends, rx, s))
            .expect("spawn macro runtime");
        Self {
            tx,
            shared,
            worker: Mutex::new(Some(worker)),
        }
    }
    pub fn command(&self, c: RuntimeCommand) -> CommandResult {
        if matches!(&c, RuntimeCommand::RunSelection(_, ids) if ids.is_empty()) {
            return CommandResult::Rejected(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidSelection,
                "selection is empty",
            ));
        }
        let active = self.shared.admission.lock().unwrap().is_some();
        let state = self.snapshot().state;
        let wrong_state = match &c {
            RuntimeCommand::Pause => !active || state == RuntimeState::Paused,
            RuntimeCommand::Resume => !active || state != RuntimeState::Paused,
            RuntimeCommand::Stop => !active,
            _ => false,
        };
        if wrong_state {
            return CommandResult::Rejected(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                format!("command {c:?} is not valid while runtime is {state:?}"),
            ));
        }
        if let RuntimeCommand::Run(id)
        | RuntimeCommand::RunFrom(id, _)
        | RuntimeCommand::RunSelection(id, _) = &c
        {
            let mut active = self.shared.admission.lock().unwrap();
            if let Some(a) = *active {
                return CommandResult::AlreadyRunning { active_macro_id: a };
            }
            if !self.shared.operations.claim(Operation::Playback) {
                return CommandResult::Rejected(ExecutionDiagnostic::new(
                    DiagnosticKind::InvalidTarget,
                    "recording is active",
                ));
            }
            *active = Some(*id);
        }
        match c {
            RuntimeCommand::Pause => {
                self.shared.control.pause();
                if self.shared.admission.lock().unwrap().is_some() {
                    publish(&self.shared, |snapshot| {
                        snapshot.state = RuntimeState::Paused
                    });
                }
            }
            RuntimeCommand::Resume => {
                self.shared.control.resume();
                if self.shared.admission.lock().unwrap().is_some() {
                    publish(&self.shared, |snapshot| {
                        snapshot.state = RuntimeState::Running
                    });
                }
            }
            RuntimeCommand::Stop => {
                self.shared.control.stop();
                if self.shared.admission.lock().unwrap().is_some() {
                    publish(&self.shared, |snapshot| {
                        snapshot.state = RuntimeState::Stopping
                    });
                }
            }
            _ => {}
        }
        if self.tx.send(c).is_err() {
            *self.shared.admission.lock().unwrap() = None;
            self.shared.operations.release(Operation::Playback);
            return CommandResult::Rejected(ExecutionDiagnostic::new(
                DiagnosticKind::RuntimeUnavailable,
                "macro worker is shut down",
            ));
        }
        CommandResult::Accepted
    }
    pub fn snapshot(&self) -> Arc<RuntimeSnapshot> {
        self.shared.snapshot.read().unwrap().clone()
    }
    pub fn shutdown(&self) {
        self.shared.control.stop();
        let _ = self.tx.send(RuntimeCommand::Shutdown);
        if let Some(h) = self.worker.lock().unwrap().take() {
            let _ = h.join();
        }
    }
}
impl Drop for MacroRuntime {
    fn drop(&mut self) {
        self.shutdown()
    }
}

fn publish(shared: &Shared, f: impl FnOnce(&mut RuntimeSnapshot)) {
    let old = shared.snapshot.read().unwrap().as_ref().clone();
    let mut n = old;
    f(&mut n);
    n.revision += 1;
    *shared.snapshot.write().unwrap() = Arc::new(n)
}
fn worker_loop(
    store: Arc<MkMacroStore>,
    backends: Backends,
    rx: mpsc::Receiver<RuntimeCommand>,
    shared: Arc<Shared>,
) {
    while let Ok(cmd) = rx.recv() {
        match cmd {
            RuntimeCommand::Shutdown => break,
            RuntimeCommand::Pause => {
                if shared.control.is_active() {
                    publish(&shared, |s| s.state = RuntimeState::Paused)
                }
            }
            RuntimeCommand::Resume => {
                if shared.control.is_active() {
                    publish(&shared, |s| s.state = RuntimeState::Running)
                }
            }
            RuntimeCommand::Stop => {}
            RuntimeCommand::Run(mid) => run_one(&store, &backends, &shared, mid, None, None),
            RuntimeCommand::RunFrom(mid, sid) => {
                run_one(&store, &backends, &shared, mid, Some(sid), None)
            }
            RuntimeCommand::RunSelection(mid, ids) => {
                run_one(&store, &backends, &shared, mid, None, Some(ids))
            }
        }
    }
    shared.control.stop();
    *shared.admission.lock().unwrap() = None;
    shared.operations.release(Operation::Playback);
}
fn run_one(
    store: &MkMacroStore,
    backends: &Backends,
    shared: &Shared,
    mid: u64,
    from: Option<u64>,
    selection: Option<Vec<u64>>,
) {
    shared.control.reset();
    let run_id = shared.next_run_id.fetch_add(1, Ordering::Relaxed);
    let result = (|| {
        let doc = store.snapshot();
        let m = doc.macros.iter().find(|m| m.id == mid).ok_or_else(|| {
            ExecutionDiagnostic::new(
                DiagnosticKind::TargetNotFound,
                format!("macro {mid} was not found"),
            )
        })?;
        if !m.enabled {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                "macro is disabled",
            ));
        }
        let mut plan = compile(m).map_err(|d| {
            ExecutionDiagnostic::new(
                DiagnosticKind::InvalidPlan,
                format!(
                    "macro validation failed: {}",
                    d.first()
                        .map(|x| x.message.as_str())
                        .unwrap_or("invalid plan")
                ),
            )
        })?;
        if let Some(sid) = from {
            if plan
                .instructions
                .iter()
                .any(|instruction| instruction.step.action.is_structural())
            {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::InvalidSelection,
                    "run-from cannot enter a structured control-flow plan",
                ));
            }
            let start = *plan.step_to_instruction.get(&sid).ok_or_else(|| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::TargetNotFound,
                    format!("step {sid} was not found"),
                )
            })?;
            plan.instructions = plan.instructions[start..].to_vec().into();
            plan.step_to_instruction = plan
                .instructions
                .iter()
                .enumerate()
                .map(|(i, x)| (x.step.id, i))
                .collect();
        }
        if let Some(ids) = selection {
            let wanted: HashSet<_> = ids.into_iter().collect();
            if wanted
                .iter()
                .any(|id| !plan.step_to_instruction.contains_key(id))
            {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::TargetNotFound,
                    "selection contains an unknown step",
                ));
            }
            if plan
                .instructions
                .iter()
                .any(|x| x.step.action.is_structural() && !wanted.contains(&x.step.id))
                || wanted.iter().any(|id| {
                    plan.instructions[*plan.step_to_instruction.get(id).unwrap()]
                        .step
                        .action
                        .is_structural()
                })
            {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::InvalidSelection,
                    "structural selections must include a complete executable plan",
                ));
            }
            plan.instructions = plan
                .instructions
                .iter()
                .filter(|x| wanted.contains(&x.step.id))
                .cloned()
                .collect::<Vec<_>>()
                .into();
            plan.step_to_instruction = plan
                .instructions
                .iter()
                .enumerate()
                .map(|(i, x)| (x.step.id, i))
                .collect();
        }
        let states = plan
            .instructions
            .iter()
            .map(|x| {
                (
                    x.step.id,
                    if x.step.enabled {
                        StepState::Pending
                    } else {
                        StepState::Skipped
                    },
                )
            })
            .collect();
        publish(shared, |s| {
            s.state = RuntimeState::Running;
            s.run_id = run_id;
            s.macro_id = Some(mid);
            s.step_id = None;
            s.completed_steps = 0;
            s.total_steps = plan.instructions.iter().filter(|x| x.step.enabled).count();
            s.started_at = Some(SystemTime::now());
            s.finished_at = None;
            s.latest_failure = None;
            s.failures = Arc::new(BTreeMap::new());
            s.steps = Arc::new(states);
            s.step_outcomes = Arc::new(BTreeMap::new())
        });
        let observer = |ev: ExecutionEvent| {
            publish(shared, |s| match ev {
                ExecutionEvent::StepStarted(id) => {
                    s.step_id = Some(id);
                    Arc::make_mut(&mut s.steps).insert(id, StepState::Running);
                }
                ExecutionEvent::StepFinished(id) => {
                    s.completed_steps += 1;
                    Arc::make_mut(&mut s.steps).insert(id, StepState::Success);
                }
                ExecutionEvent::StepOutcome(id, outcome) => {
                    Arc::make_mut(&mut s.step_outcomes).insert(id, outcome);
                }
                ExecutionEvent::StepSkipped(id) => {
                    Arc::make_mut(&mut s.steps).insert(id, StepState::Skipped);
                }
                ExecutionEvent::StepFailed(id, d) => {
                    s.latest_failure = Some(d.clone());
                    Arc::make_mut(&mut s.failures).insert(
                        DiagnosticKey {
                            run_id,
                            step_id: id,
                        },
                        d,
                    );
                    Arc::make_mut(&mut s.steps).insert(id, StepState::Failed);
                }
                ExecutionEvent::Paused => s.state = RuntimeState::Paused,
                ExecutionEvent::Resumed => s.state = RuntimeState::Running,
            })
        };
        Executor::new(backends.clone(), shared.control.clone()).execute(&plan, &observer)
    })();
    match result {
        Ok(()) => publish(shared, |s| {
            s.state = RuntimeState::Completed;
            s.step_id = None;
            s.finished_at = Some(SystemTime::now())
        }),
        Err(d) => {
            let stopped = d.kind == DiagnosticKind::Cancelled;
            publish(shared, |s| {
                s.state = if stopped {
                    RuntimeState::Stopped
                } else {
                    RuntimeState::Failed
                };
                s.latest_failure = if stopped {
                    s.latest_failure.clone()
                } else {
                    Some(d)
                };
                s.step_id = None;
                s.finished_at = Some(SystemTime::now())
            })
        }
    };
    *shared.admission.lock().unwrap() = None;
    shared.operations.release(Operation::Playback);
}

static RUNTIME: Lazy<RwLock<Option<Arc<MacroRuntime>>>> = Lazy::new(|| RwLock::new(None));
static RECORDER: Lazy<RwLock<Option<Arc<RecorderRuntime>>>> = Lazy::new(|| RwLock::new(None));
static HOTKEYS: Lazy<RwLock<Option<Arc<super::hotkeys::MkMacroHotkeyService>>>> =
    Lazy::new(|| RwLock::new(None));
static RECORDER_HOTKEYS: Lazy<RwLock<Option<Arc<super::recorder_hotkeys::RecorderHotkeyService>>>> =
    Lazy::new(|| RwLock::new(None));
static RECORDING_TARGET: Lazy<RwLock<Option<u64>>> = Lazy::new(|| RwLock::new(None));
static RECORDING_OPTIONS: Lazy<RwLock<NormalizationConfig>> =
    Lazy::new(|| RwLock::new(NormalizationConfig::default()));
static RECORDING_STATUS: Lazy<RwLock<Option<String>>> = Lazy::new(|| RwLock::new(None));
static PENDING_RECORDINGS: Lazy<Mutex<Vec<RecordingResult>>> = Lazy::new(|| Mutex::new(Vec::new()));
pub fn set_shared_store(store: Arc<MkMacroStore>) {
    set_shared_store_with_backends_and_reserved(
        store.clone(),
        production_backends_with_store(store),
        &[],
    )
}
pub fn set_shared_store_with_reserved(store: Arc<MkMacroStore>, reserved: &[(&str, &str)]) {
    set_shared_store_with_backends_and_reserved(
        store.clone(),
        production_backends_with_store(store),
        reserved,
    )
}
/// Installs a shared runtime with injected effects (intended for tests).
pub fn set_shared_store_with_backends(store: Arc<MkMacroStore>, backends: Backends) {
    set_shared_store_with_backends_and_reserved(store, backends, &[])
}
/// Installs a shared runtime with injected effects and reserved launcher chords.
pub fn set_shared_store_with_backends_and_reserved(
    store: Arc<MkMacroStore>,
    backends: Backends,
    reserved: &[(&str, &str)],
) {
    // Stop the old poller before replacing the runtime it dispatches into.
    if let Some(old) = HOTKEYS.write().unwrap().take() {
        old.shutdown()
    }
    if let Some(old) = RECORDER_HOTKEYS.write().unwrap().take() {
        old.shutdown()
    }
    if let Some(old) = RUNTIME.write().unwrap().take() {
        old.shutdown()
    }
    if let Some(old) = RECORDER.write().unwrap().take() {
        old.shutdown()
    }
    let guard = Arc::new(SharedOperationGuard::default());
    *RUNTIME.write().unwrap() = Some(Arc::new(MacroRuntime::with_guard(
        store.clone(),
        backends,
        guard.clone(),
    )));
    *RECORDER.write().unwrap() = Some(Arc::new(RecorderRuntime::with_guard(
        store.clone(),
        production_hook_service(8192),
        Arc::new(SystemRecorderClock::default()),
        guard,
    )));
    *HOTKEYS.write().unwrap() = Some(Arc::new(
        super::hotkeys::MkMacroHotkeyService::new_with_reserved(store.clone(), reserved),
    ));
    *RECORDER_HOTKEYS.write().unwrap() = Some(Arc::new(
        super::recorder_hotkeys::RecorderHotkeyService::system(store),
    ));
}
fn global() -> Result<Arc<MacroRuntime>> {
    RUNTIME
        .read()
        .unwrap()
        .clone()
        .ok_or_else(|| anyhow!("macro runtime is not initialized"))
}
fn accepted(r: CommandResult) -> Result<()> {
    match r {
        CommandResult::Accepted => Ok(()),
        x => Err(anyhow!("{x:?}")),
    }
}
pub fn run(id: u64) -> Result<()> {
    accepted(global()?.command(RuntimeCommand::Run(id)))
}
pub fn run_from(macro_id: u64, step_id: u64) -> Result<()> {
    accepted(global()?.command(RuntimeCommand::RunFrom(macro_id, step_id)))
}
pub fn run_selection(macro_id: u64, ids: Vec<u64>) -> Result<()> {
    accepted(global()?.command(RuntimeCommand::RunSelection(macro_id, ids)))
}
pub fn pause() -> Result<()> {
    accepted(global()?.command(RuntimeCommand::Pause))
}
pub fn resume() -> Result<()> {
    accepted(global()?.command(RuntimeCommand::Resume))
}
pub fn stop() -> Result<()> {
    accepted(global()?.command(RuntimeCommand::Stop))
}
/// Returns transient execution state; it is never serialized by `MkMacroStore`.
pub fn snapshot() -> Option<Arc<RuntimeSnapshot>> {
    RUNTIME.read().unwrap().as_ref().map(|r| r.snapshot())
}
pub fn record(macro_id: u64, mut config: NormalizationConfig) -> Result<()> {
    if let Some(doc) = RECORDER
        .read()
        .unwrap()
        .as_ref()
        .map(|r| r.document_snapshot())
        .flatten()
    {
        if let Some(vk) =
            super::hotkeys::primary_virtual_key(&doc.settings.record_toggle_hotkey.key)
        {
            if !config.control_hotkeys.contains(&vk) {
                config.control_hotkeys.push(vk);
            }
        }
    }
    RECORDER
        .read()
        .unwrap()
        .clone()
        .ok_or_else(|| anyhow!("macro runtime is not initialized"))?
        .start(macro_id, config)
}
pub fn record_pause() -> Result<()> {
    RECORDER
        .read()
        .unwrap()
        .clone()
        .ok_or_else(|| anyhow!("macro runtime is not initialized"))?
        .pause()
}
pub fn record_resume() -> Result<()> {
    RECORDER
        .read()
        .unwrap()
        .clone()
        .ok_or_else(|| anyhow!("macro runtime is not initialized"))?
        .resume()
}
pub fn record_stop() -> Result<RecordingResult> {
    RECORDER
        .read()
        .unwrap()
        .clone()
        .ok_or_else(|| anyhow!("macro runtime is not initialized"))?
        .stop()
}
pub fn recorder_snapshot() -> Option<Arc<RecorderSnapshot>> {
    RECORDER.read().unwrap().as_ref().map(|r| r.snapshot())
}
pub fn set_recording_target(target: Option<u64>) {
    *RECORDING_TARGET.write().unwrap() = target;
    if target.is_some()
        && RECORDING_STATUS.read().unwrap().as_deref()
            == Some("Select a macro before starting recording")
    {
        *RECORDING_STATUS.write().unwrap() = None;
    }
}
pub fn set_recording_options(options: NormalizationConfig) {
    *RECORDING_OPTIONS.write().unwrap() = options;
}
pub fn recording_status() -> Option<String> {
    RECORDING_STATUS.read().unwrap().clone()
}
pub fn take_pending_recordings() -> Vec<RecordingResult> {
    std::mem::take(&mut *PENDING_RECORDINGS.lock().unwrap())
}

/// Callback used by the global recorder control. It exchanges only thread-safe runtime state;
/// GUI drafts are updated later when they drain `take_pending_recordings`.
pub(crate) fn toggle_recording() {
    let Some(recorder) = RECORDER.read().unwrap().clone() else {
        return;
    };
    let result: Result<()> = match recorder.snapshot().state {
        super::RecorderRuntimeState::Idle => {
            let target = *RECORDING_TARGET.read().unwrap();
            let Some(id) = target.filter(|id| recorder.store_contains(*id)) else {
                *RECORDING_STATUS.write().unwrap() =
                    Some("Select a macro before starting recording".into());
                return;
            };
            let mut config = RECORDING_OPTIONS.read().unwrap().clone();
            // Snapshot the persisted chord for the complete session.
            if let Some(store) = recorder.document_snapshot() {
                if let Some(vk) =
                    super::hotkeys::primary_virtual_key(&store.settings.record_toggle_hotkey.key)
                {
                    if !config.control_hotkeys.contains(&vk) {
                        config.control_hotkeys.push(vk);
                    }
                }
            }
            recorder.start(id, config)
        }
        super::RecorderRuntimeState::Recording | super::RecorderRuntimeState::Paused => {
            recorder.stop().map(|result| {
                PENDING_RECORDINGS.lock().unwrap().push(result);
            })
        }
        super::RecorderRuntimeState::Stopping => return,
    };
    *RECORDING_STATUS.write().unwrap() = result.err().map(|e| e.to_string());
}

#[cfg(test)]
mod folder_tests {
    use super::*;
    use crate::mkmacro::{executor::fake::FakeBackend, model::*};
    use std::time::{Duration, Instant};

    fn run_document(document: MkMacroDocument, id: u64) -> (Arc<RuntimeSnapshot>, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        store.save(document).unwrap();
        let effects = Arc::new(FakeBackend::default());
        let runtime = MacroRuntime::new(Arc::new(store), effects.clone().backends());
        assert_eq!(
            runtime.command(RuntimeCommand::Run(id)),
            CommandResult::Accepted
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = runtime.snapshot();
            if matches!(
                snapshot.state,
                RuntimeState::Completed | RuntimeState::Failed
            ) {
                return (snapshot, effects.events());
            }
            assert!(
                Instant::now() < deadline,
                "runtime did not finish: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn folder_metadata_does_not_change_runtime_lookup_or_manual_run_eligibility() {
        let target = MkMacro {
            id: 91,
            name: "Target".into(),
            description: String::new(),
            enabled: true,
            hotkey: Some(MkHotkey {
                key: MkKey::Function(8),
                modifiers: vec![],
            }),
            hotkey_scope: MkHotkeyScope::ActiveWindow(MkWindowMatcher {
                process: Some("editor.exe".into()),
                ..Default::default()
            }),
            folder_id: None,
            playback: Default::default(),
            steps: vec![MkStep {
                id: 11,
                enabled: true,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::Text(MkTextPayload {
                    text: "target executed".into(),
                    mode: MkTextMode::Type,
                }),
            }],
            image_assets: vec![],
        };
        let mut decoy = target.clone();
        decoy.id = 7;
        decoy.name = "Decoy".into();
        decoy.hotkey = None;
        decoy.steps.clear();
        let mut document = MkMacroDocument {
            macros: vec![decoy, target],
            folders: vec![
                MkMacroFolder {
                    id: 42,
                    name: "Utilities".into(),
                },
                MkMacroFolder {
                    id: 43,
                    name: "Work".into(),
                },
            ],
            ..Default::default()
        };
        // A folder ID must never become a runtime target; disabled macros stay disabled.
        for (enabled, requested_id, expected_state) in [
            (true, 91, RuntimeState::Completed),
            (false, 91, RuntimeState::Failed),
            (true, 42, RuntimeState::Failed),
        ] {
            document.macros[1].enabled = enabled;
            document.macros[1].folder_id = None;
            document.folders[0].name = "Utilities".into();
            let (expected, expected_events) = run_document(document.clone(), requested_id);
            assert_eq!(expected.state, expected_state);
            if expected_state == RuntimeState::Completed {
                assert_eq!(expected.macro_id, Some(91));
                assert_eq!(expected.steps[&11], StepState::Success);
                assert_eq!(expected_events, ["text:target executed"]);
            } else {
                let kind = if enabled {
                    DiagnosticKind::TargetNotFound
                } else {
                    DiagnosticKind::InvalidTarget
                };
                assert_eq!(expected.latest_failure.as_ref().unwrap().kind, kind);
                assert!(expected_events.is_empty());
            }
            for (folder_id, name) in [
                (None, "Utilities"),
                (Some(42), "Utilities"),
                (Some(42), "Renamed folder"),
                (Some(43), "Utilities"),
            ] {
                document.macros[1].folder_id = folder_id;
                document.folders[0].name = name.into();
                let (actual, events) = run_document(document.clone(), requested_id);
                assert_eq!(actual.state, expected.state);
                assert_eq!(actual.macro_id, expected.macro_id);
                assert_eq!(actual.run_id, expected.run_id);
                assert_eq!(actual.steps, expected.steps);
                assert_eq!(actual.completed_steps, expected.completed_steps);
                assert_eq!(actual.total_steps, expected.total_steps);
                assert_eq!(actual.latest_failure, expected.latest_failure);
                assert_eq!(actual.failures, expected.failures);
                assert_eq!(events, expected_events);
            }
        }
    }
}

#[cfg(test)]
mod recording_controller_tests {
    use super::*;
    use crate::mkmacro::{
        MkMacro, MkMacroDocument, MkPlayback, SCHEMA_VERSION, executor::fake::FakeBackend,
    };

    #[test]
    fn toggle_requires_a_target_and_assigns_the_stopped_session_only_to_it() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        store
            .save(MkMacroDocument {
                schema_version: SCHEMA_VERSION,
                folders: vec![],
                settings: Default::default(),
                macros: [1, 2]
                    .into_iter()
                    .map(|id| MkMacro {
                        id,
                        name: format!("macro {id}"),
                        description: String::new(),
                        enabled: true,
                        hotkey: None,
                        hotkey_scope: Default::default(),
                        folder_id: None,
                        playback: MkPlayback::default(),
                        steps: vec![],
                        image_assets: vec![],
                    })
                    .collect(),
            })
            .unwrap();
        let store = Arc::new(store);
        let fake = Arc::new(FakeBackend::default());
        set_shared_store_with_backends(store, fake.backends());
        take_pending_recordings();

        set_recording_target(None);
        toggle_recording();
        assert_eq!(
            recorder_snapshot().unwrap().state,
            super::super::RecorderRuntimeState::Idle
        );
        assert_eq!(
            recording_status().as_deref(),
            Some("Select a macro before starting recording")
        );

        set_recording_target(Some(2));
        assert_eq!(recording_status(), None);
        toggle_recording();
        assert_eq!(recorder_snapshot().unwrap().macro_id, Some(2));
        toggle_recording();
        let results = take_pending_recordings();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].macro_id, 2);
        assert!(results[0].generated_steps.is_empty());
    }
}

#[cfg(test)]
mod step_outcome_tests {
    use super::*;

    #[test]
    fn image_match_and_continued_miss_are_distinct_success_details() {
        let matched = StepOutcome {
            last_image_found: Some(true),
        };
        let missed = StepOutcome {
            last_image_found: Some(false),
        };
        assert_ne!(matched.detail(), missed.detail());
        assert_eq!(matched.last_image_found, Some(true));
        assert_eq!(missed.last_image_found, Some(false));
        assert_eq!(
            missed.detail(),
            Some("Success — image not found; continued.")
        );
        // Outcome metadata augments, rather than changes, the successful state.
        assert_eq!(StepState::Success, StepState::Success);
    }

    #[test]
    fn unrelated_step_has_no_inherited_image_status() {
        let unrelated = StepOutcome::default();
        assert_eq!(unrelated.last_image_found, None);
        assert_eq!(unrelated.detail(), None);
    }
}
