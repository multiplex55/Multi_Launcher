//! Worker-owned macro runtime.  The public methods only exchange messages and snapshots;
//! action execution and all waits live on the worker.
#[cfg(test)]
use super::compile_program;
pub use super::executor::DebugSnapshotReason;
use super::executor::{
    Backends, DiagnosticKind, ExecResult, ExecutionDiagnostic, ExecutionEvent,
    ExecutionFrameContext, ExecutionFrameSnapshot, ExecutionMode, ExecutionOptions, Executor,
    RunControl, StepOutcome, production_backends_with_store,
};
use super::{
    EventEnricher, KeyboardTranslator, MkInvocation, MkInvocationSubset, MkInvocationValues,
    MkMacroStore, MkValue, NormalizationConfig, Operation, RecorderObserverSession,
    RecorderRuntime, RecorderSnapshot, RecordingResult, RecordingTarget, RuntimeVariables,
    SharedOperationGuard, SystemRecorderClock, production_hook_service,
};
use anyhow::{Result, anyhow};
use once_cell::sync::Lazy;
#[cfg(test)]
use std::time::{Duration, Instant};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Condvar, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::SystemTime,
};

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeCommand {
    Invoke(MkInvocation),
    Run(u64),
    RunFrom(u64, u64),
    RunSelection(u64, Vec<u64>),
    DebugRun(u64),
    DebugRunFrom(u64, u64),
    DebugRunSelection(u64, Vec<u64>),
    /// Executes an already-compiled, process-local recording proposal. The
    /// ticket associates transient Review UI with this run without publishing
    /// the proposal through the store.
    RecordingPreview {
        macro_id: u64,
        ticket: u64,
    },
    Pause,
    Resume,
    Stop,
    Shutdown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeRunMode {
    Normal,
    Debug,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeOrigin {
    Stored,
    RecordingPreview { ticket: u64 },
}
#[derive(Debug, Clone, Copy)]
struct RunRequest<'a> {
    macro_id: u64,
    starting_step: Option<u64>,
    selection: Option<&'a [u64]>,
    mode: RuntimeRunMode,
    arguments: Option<&'a MkInvocationValues>,
}
impl RunRequest<'_> {
    fn to_invocation(self) -> MkInvocation {
        MkInvocation {
            macro_id: self.macro_id,
            arguments: self.arguments.cloned().unwrap_or_default(),
            mode: match self.mode {
                RuntimeRunMode::Normal => ExecutionMode::Normal,
                RuntimeRunMode::Debug => ExecutionMode::Debug,
            },
            subset: if let Some(id) = self.starting_step {
                MkInvocationSubset::From(id)
            } else if let Some(ids) = self.selection {
                MkInvocationSubset::Selected(ids.to_vec())
            } else {
                MkInvocationSubset::Whole
            },
        }
    }
}
fn run_request(command: &RuntimeCommand) -> Option<RunRequest<'_>> {
    if let RuntimeCommand::Invoke(invocation) = command {
        let (starting_step, selection) = match &invocation.subset {
            MkInvocationSubset::Whole => (None, None),
            MkInvocationSubset::From(id) => (Some(*id), None),
            MkInvocationSubset::Selected(ids) => (None, Some(ids.as_slice())),
        };
        return Some(RunRequest {
            macro_id: invocation.macro_id,
            starting_step,
            selection,
            mode: match invocation.mode {
                ExecutionMode::Normal => RuntimeRunMode::Normal,
                ExecutionMode::Debug => RuntimeRunMode::Debug,
            },
            arguments: Some(&invocation.arguments),
        });
    }
    let (macro_id, starting_step, selection, mode) = match command {
        RuntimeCommand::Run(id) => (*id, None, None, RuntimeRunMode::Normal),
        RuntimeCommand::RunFrom(id, step_id) => (*id, Some(*step_id), None, RuntimeRunMode::Normal),
        RuntimeCommand::RunSelection(id, selection) => (
            *id,
            None,
            Some(selection.as_slice()),
            RuntimeRunMode::Normal,
        ),
        RuntimeCommand::DebugRun(id) => (*id, None, None, RuntimeRunMode::Debug),
        RuntimeCommand::DebugRunFrom(id, step_id) => {
            (*id, Some(*step_id), None, RuntimeRunMode::Debug)
        }
        RuntimeCommand::DebugRunSelection(id, selection) => {
            (*id, None, Some(selection.as_slice()), RuntimeRunMode::Debug)
        }
        RuntimeCommand::RecordingPreview { macro_id, .. } => {
            (*macro_id, None, None, RuntimeRunMode::Normal)
        }
        _ => return None,
    };
    Some(RunRequest {
        macro_id,
        starting_step,
        selection,
        mode,
        arguments: None,
    })
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
pub enum RuntimePauseReason {
    User,
    Breakpoint {
        step_id: u64,
        frame: ExecutionFrameContext,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub struct DebugSnapshot {
    pub frame: ExecutionFrameContext,
    pub macro_name: Arc<str>,
    /// The last immutable variable map published by a Debug execution
    /// boundary. This is always owned by the snapshot and never aliases the
    /// executor's worker-local map.
    pub step_id: Option<u64>,
    pub variables: Arc<RuntimeVariables>,
    pub reason: DebugSnapshotReason,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MacroStepKey {
    pub macro_id: u64,
    pub step_id: u64,
}
impl MacroStepKey {
    pub const fn new(macro_id: u64, step_id: u64) -> Self {
        Self { macro_id, step_id }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MacroDiagnosticKey {
    pub run_id: u64,
    pub step: MacroStepKey,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BreakpointOccurrence {
    pub run_id: u64,
    pub occurrence: u64,
    pub frame_id: u64,
    pub step: MacroStepKey,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletedStepOutcome {
    Success(Option<StepOutcome>),
    Failure(ExecutionDiagnostic),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedStep {
    pub frame: ExecutionFrameContext,
    pub key: MacroStepKey,
    pub macro_name: Arc<str>,
    pub outcome: CompletedStepOutcome,
}

#[derive(Debug, Clone)]
pub struct RuntimeSnapshot {
    pub state: RuntimeState,
    pub run_mode: RuntimeRunMode,
    pub run_id: u64,
    pub origin: RuntimeOrigin,
    /// Root invocation identity, preserved for compatibility.
    pub macro_id: Option<u64>,
    pub root_macro_name: Option<Arc<str>>,
    /// Current root step; callees have their own authoritative stack identity.
    pub step_id: Option<u64>,
    pub call_stack: Arc<Vec<ExecutionFrameSnapshot>>,
    pub macro_steps: Arc<BTreeMap<MacroStepKey, StepState>>,
    pub macro_step_outcomes: Arc<BTreeMap<MacroStepKey, StepOutcome>>,
    pub macro_failures: Arc<BTreeMap<MacroDiagnosticKey, ExecutionDiagnostic>>,
    /// This result belongs to an invocation occurrence, independently of maps
    /// reset when the same callee is invoked again or its frame is popped.
    pub last_completed: Option<Arc<CompletedStep>>,
    pub pause_reason: Option<RuntimePauseReason>,
    /// Advances once per executor BreakpointHit, not per snapshot publication.
    pub breakpoint_sequence: u64,
    /// The most recently published Debug boundary for this run. `None` means
    /// that this run has not published debug data, which is the defined state
    /// for Normal playback, including a manually paused Normal run.
    pub debug_snapshot: Option<Arc<DebugSnapshot>>,
    /// The last immutable runtime-variable map published by a Debug event.
    /// This remains empty for Normal playback and is runtime-only.
    pub debug_variables: Arc<BTreeMap<String, MkValue>>,
    /// Step associated with `debug_variables`, if the Debug event had one.
    pub debug_variables_step_id: Option<u64>,
    /// Reason associated with `debug_variables`, if a Debug event was seen.
    pub debug_snapshot_reason: Option<DebugSnapshotReason>,
    /// Root-only projection of the last terminal step. A skipped step is not
    /// considered completed because it did not execute.
    pub last_completed_step_id: Option<u64>,
    pub completed_steps: usize,
    pub total_steps: usize,
    pub started_at: Option<SystemTime>,
    pub finished_at: Option<SystemTime>,
    pub latest_failure: Option<ExecutionDiagnostic>,
    /// Legacy root-only projections of the authoritative compound maps above.
    pub failures: Arc<BTreeMap<DiagnosticKey, ExecutionDiagnostic>>,
    pub steps: Arc<BTreeMap<u64, StepState>>,
    pub step_outcomes: Arc<BTreeMap<u64, StepOutcome>>,
    pub revision: u64,
}
impl Default for RuntimeSnapshot {
    fn default() -> Self {
        Self {
            state: RuntimeState::Idle,
            run_mode: RuntimeRunMode::Normal,
            run_id: 0,
            origin: RuntimeOrigin::Stored,
            macro_id: None,
            root_macro_name: None,
            step_id: None,
            call_stack: Arc::new(Vec::new()),
            macro_steps: Arc::new(BTreeMap::new()),
            macro_step_outcomes: Arc::new(BTreeMap::new()),
            macro_failures: Arc::new(BTreeMap::new()),
            last_completed: None,
            pause_reason: None,
            breakpoint_sequence: 0,
            debug_snapshot: None,
            debug_variables: Arc::new(BTreeMap::new()),
            debug_variables_step_id: None,
            debug_snapshot_reason: None,
            last_completed_step_id: None,
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
impl RuntimeSnapshot {
    pub fn active_frame(&self) -> Option<ExecutionFrameContext> {
        self.call_stack.last().map(|frame| frame.context)
    }
    pub fn active_macro_id(&self) -> Option<u64> {
        self.active_frame().map(|frame| frame.macro_id)
    }
    pub fn active_step_id(&self) -> Option<u64> {
        self.call_stack
            .last()
            .and_then(|frame| frame.active_step_id)
    }
    pub fn breakpoint_occurrence(&self) -> Option<BreakpointOccurrence> {
        match (self.state, self.pause_reason) {
            (RuntimeState::Paused, Some(RuntimePauseReason::Breakpoint { step_id, frame })) => {
                Some(BreakpointOccurrence {
                    run_id: self.run_id,
                    occurrence: self.breakpoint_sequence,
                    frame_id: frame.frame_id,
                    step: MacroStepKey::new(frame.macro_id, step_id),
                })
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandResult {
    Accepted,
    AlreadyRunning { active_macro_id: u64 },
    Rejected(ExecutionDiagnostic),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvocationDisposition {
    Submitted,
    AwaitingInput { request_id: u64 },
}

struct Shared {
    snapshot: RwLock<Arc<RuntimeSnapshot>>,
    // Retain the most recently published preview independently from the
    // process-wide presentation snapshot. A later stored run must not erase a
    // Review window's terminal outcome before its next frame observes it.
    last_preview_snapshot: RwLock<Option<Arc<RuntimeSnapshot>>>,
    control: Arc<RunControl>,
    admission: Mutex<Option<ActiveAdmission>>,
    next_run_id: AtomicU64,
    operations: Arc<SharedOperationGuard>,
    #[cfg(test)]
    test_events: Mutex<Vec<ExecutionEvent>>,
    #[cfg(test)]
    test_worker_barrier: Mutex<Option<Arc<TestWorkerBarrierInner>>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActiveAdmission {
    macro_id: u64,
    origin: RuntimeOrigin,
}
struct WorkerMessage {
    command: RuntimeCommand,
    program: Option<super::MkCompiledProgram>,
    origin: RuntimeOrigin,
}
static RUNTIME_GENERATIONS: AtomicU64 = AtomicU64::new(1);
static NEXT_PREVIEW_TICKET: AtomicU64 = AtomicU64::new(1);
pub struct MacroRuntime {
    tx: mpsc::Sender<WorkerMessage>,
    store: Arc<MkMacroStore>,
    generation: u64,
    available: AtomicBool,
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
    #[cfg(test)]
    test_commands: Mutex<Vec<RuntimeCommand>>,
}

#[cfg(test)]
struct TestWorkerBarrierInner {
    state: Mutex<(bool, bool)>,
    wake: Condvar,
}

#[cfg(test)]
pub(crate) struct TestWorkerBarrier(Arc<TestWorkerBarrierInner>);

#[cfg(test)]
impl TestWorkerBarrier {
    pub(crate) fn wait_until_blocked(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut state = self.0.state.lock().unwrap();
        while !state.0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "runtime worker did not reach test barrier"
            );
            let (next, timeout) = self.0.wake.wait_timeout(state, remaining).unwrap();
            state = next;
            assert!(
                !timeout.timed_out() || state.0,
                "runtime worker did not reach test barrier"
            );
        }
    }

    pub(crate) fn release(&self) {
        let mut state = self.0.state.lock().unwrap();
        state.1 = true;
        self.0.wake.notify_all();
    }
}

#[cfg(test)]
impl Drop for TestWorkerBarrier {
    fn drop(&mut self) {
        self.release();
    }
}
impl MacroRuntime {
    fn ensure_available(&self) -> ExecResult {
        let admission = self.shared.admission.lock().unwrap();
        if !self.available.load(Ordering::Acquire) {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::RuntimeUnavailable,
                "The macro runtime was replaced or closed",
            ));
        }
        if let Some(active) = *admission {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::RuntimeUnavailable,
                format!("Macro {} is already running", active.macro_id),
            ));
        }
        if self.shared.operations.active(Operation::Recording) {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                "recording is active",
            ));
        }
        Ok(())
    }
    /// Direct callers prepare on their own thread, without reserving a worker
    /// while the user considers input. Final submission retains atomic admission.
    pub fn prepare_invocation(
        self: &Arc<Self>,
        invocation: MkInvocation,
        broker: &Arc<super::invocation_prompt::InvocationPromptBroker>,
    ) -> ExecResult<InvocationDisposition> {
        self.prepare_direct_command(RuntimeCommand::Invoke(invocation), broker)
    }
    fn prepare_direct_command(
        self: &Arc<Self>,
        command: RuntimeCommand,
        broker: &Arc<super::invocation_prompt::InvocationPromptBroker>,
    ) -> ExecResult<InvocationDisposition> {
        self.ensure_available()?;
        broker.ensure_idle()?;
        let request = run_request(&command).ok_or_else(|| {
            ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                "Expected a direct invocation",
            )
        })?;
        let mut invocation = request.to_invocation();
        let document = self.store.snapshot();
        let program = super::invocation::compile_invocation_program(&document, &invocation)?;
        let target = document
            .macros
            .iter()
            .find(|m| m.id == invocation.macro_id)
            .ok_or_else(|| {
                ExecutionDiagnostic::new(DiagnosticKind::TargetNotFound, "Macro was not found")
            })?;
        let prepared = super::prepare_parameters(
            &target.signature.parameters,
            &target.signature.outputs,
            &invocation.arguments,
        )?;
        if !prepared.missing.is_empty() {
            let request_id = broker.enqueue(
                self,
                super::invocation_prompt::InvocationPromptRequest {
                    id: 0,
                    runtime_generation: self.generation,
                    invocation,
                    macro_name: target.name.clone(),
                    macro_description: target.description.clone(),
                    parameters: target.signature.parameters.clone(),
                    prepared_values: prepared.values,
                },
            )?;
            return Ok(InvocationDisposition::AwaitingInput { request_id });
        }
        invocation.arguments = prepared.values;
        // Keep the existing empty-argument command contract for compatibility.
        let command = if invocation.arguments.is_empty() {
            command
        } else {
            RuntimeCommand::Invoke(invocation)
        };
        command_result(self.submit_command(command, Some(program)))?;
        Ok(InvocationDisposition::Submitted)
    }
    pub(crate) fn confirm_invocation(
        &self,
        request: &super::invocation_prompt::InvocationPromptRequest,
        values: MkInvocationValues,
    ) -> ExecResult {
        self.ensure_available()?;
        if request.runtime_generation != self.generation {
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::RuntimeUnavailable,
                "The macro runtime changed while input was pending",
            ));
        }
        let mut invocation = request.invocation.clone();
        invocation.arguments = values;
        let document = self.store.snapshot();
        let program = super::invocation::compile_invocation_program(&document, &invocation)?;
        let plan = program.plan(invocation.macro_id).ok_or_else(|| {
            ExecutionDiagnostic::new(DiagnosticKind::InvalidPlan, "Program root is missing")
        })?;
        super::invocation::validate_parameter_assumptions(
            &request.parameters,
            plan.signature.parameters(),
        )?;
        let prepared = super::prepare_parameters(
            plan.signature.parameters(),
            plan.signature.outputs(),
            &invocation.arguments,
        )?;
        // The shared preparation boundary owns missing/type errors, including
        // callers outside egui that submit an incomplete response.
        prepared
            .clone()
            .into_variables(plan.signature.parameters())?;
        invocation.arguments = prepared.values;
        command_result(self.submit_command(RuntimeCommand::Invoke(invocation), Some(program)))
    }
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
            last_preview_snapshot: RwLock::new(None),
            control: Arc::new(RunControl::default()),
            admission: Mutex::new(None),
            next_run_id: AtomicU64::new(1),
            operations,
            #[cfg(test)]
            test_events: Mutex::new(Vec::new()),
            #[cfg(test)]
            test_worker_barrier: Mutex::new(None),
        });
        let s = shared.clone();
        let worker_store = store.clone();
        let worker = thread::Builder::new()
            .name("mkmacro-runtime".into())
            .spawn(move || worker_loop(worker_store, backends, rx, s))
            .expect("spawn macro runtime");
        Self {
            tx,
            store,
            generation: RUNTIME_GENERATIONS.fetch_add(1, Ordering::Relaxed),
            available: AtomicBool::new(true),
            shared,
            worker: Mutex::new(Some(worker)),
            #[cfg(test)]
            test_commands: Mutex::new(Vec::new()),
        }
    }
    pub fn command(&self, c: RuntimeCommand) -> CommandResult {
        self.submit_command(c, None)
    }
    pub fn preview(
        &self,
        document: &super::MkMacroDocument,
        macro_id: u64,
        ticket: u64,
    ) -> ExecResult {
        self.ensure_available()?;
        let program = super::compile_program(document, macro_id).map_err(|diagnostics| {
            let message = diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            ExecutionDiagnostic::new(DiagnosticKind::InvalidPlan, message)
        })?;
        command_result(self.submit_command(
            RuntimeCommand::RecordingPreview { macro_id, ticket },
            Some(program),
        ))
    }
    fn submit_command(
        &self,
        c: RuntimeCommand,
        program: Option<super::MkCompiledProgram>,
    ) -> CommandResult {
        #[cfg(test)]
        self.test_commands.lock().unwrap().push(c.clone());
        let request = run_request(&c);
        if matches!(&c, RuntimeCommand::RecordingPreview { .. }) && program.is_none() {
            return CommandResult::Rejected(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidPlan,
                "recording preview requires an ephemeral compiled program",
            ));
        }
        if request.is_some_and(|request| request.selection.is_some_and(<[u64]>::is_empty)) {
            return CommandResult::Rejected(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidSelection,
                "selection is empty",
            ));
        }
        // Admission serializes terminal publication with control commands and
        // new-run claims. Lock order is admission -> control/snapshot/operations.
        let mut admission = self.shared.admission.lock().unwrap();
        if !self.available.load(Ordering::Acquire) {
            return CommandResult::Rejected(ExecutionDiagnostic::new(
                DiagnosticKind::RuntimeUnavailable,
                "macro worker is shut down",
            ));
        }
        let active = admission.is_some();
        let state = self.snapshot().state;
        let stopped = self.shared.control.is_stopped();
        let wrong_state = match &c {
            RuntimeCommand::Pause => !active || stopped || state == RuntimeState::Paused,
            RuntimeCommand::Resume => !active || stopped || state != RuntimeState::Paused,
            RuntimeCommand::Stop => !active,
            _ => false,
        };
        if wrong_state {
            return CommandResult::Rejected(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                format!("command {c:?} is not valid while runtime is {state:?}"),
            ));
        }
        if let Some(request) = request {
            if let Some(active) = *admission {
                return CommandResult::AlreadyRunning {
                    active_macro_id: active.macro_id,
                };
            }
            if !self.shared.operations.claim(Operation::Playback) {
                return CommandResult::Rejected(ExecutionDiagnostic::new(
                    DiagnosticKind::InvalidTarget,
                    "recording is active",
                ));
            }
            self.shared.control.reset();
            let origin = match &c {
                RuntimeCommand::RecordingPreview { ticket, .. } => {
                    RuntimeOrigin::RecordingPreview { ticket: *ticket }
                }
                _ => RuntimeOrigin::Stored,
            };
            *admission = Some(ActiveAdmission {
                macro_id: request.macro_id,
                origin,
            });
        }
        match c {
            RuntimeCommand::Pause => {
                self.shared.control.pause();
                if admission.is_some() {
                    publish_manual_pause(&self.shared);
                }
            }
            RuntimeCommand::Resume => {
                self.shared.control.resume();
                if admission.is_some() {
                    publish(&self.shared, |snapshot| {
                        snapshot.state = RuntimeState::Running;
                        snapshot.pause_reason = None;
                    });
                }
            }
            RuntimeCommand::Stop => {
                self.shared.control.stop();
                if admission.is_some() {
                    publish(&self.shared, |snapshot| {
                        snapshot.state = RuntimeState::Stopping;
                        snapshot.pause_reason = None;
                    });
                }
            }
            _ => {}
        }
        if self
            .tx
            .send(WorkerMessage {
                origin: match &c {
                    RuntimeCommand::RecordingPreview { ticket, .. } => {
                        RuntimeOrigin::RecordingPreview { ticket: *ticket }
                    }
                    _ => RuntimeOrigin::Stored,
                },
                command: c,
                program,
            })
            .is_err()
        {
            self.shared.control.finish();
            self.shared.operations.release(Operation::Playback);
            *admission = None;
            return CommandResult::Rejected(ExecutionDiagnostic::new(
                DiagnosticKind::RuntimeUnavailable,
                "macro worker is shut down",
            ));
        }
        CommandResult::Accepted
    }
    /// Idempotently cancel one admitted recording preview. The comparison and
    /// Stop latch share the admission lock, so a stale Review can neither miss
    /// an accepted-but-unpublished preview nor stop a later stored run.
    pub fn stop_recording_preview(&self, ticket: u64) -> bool {
        let admission = self.shared.admission.lock().unwrap();
        if !admission
            .is_some_and(|active| active.origin == RuntimeOrigin::RecordingPreview { ticket })
        {
            return false;
        }
        self.shared.control.stop();
        let snapshot = self.snapshot();
        if snapshot.origin == (RuntimeOrigin::RecordingPreview { ticket })
            && matches!(snapshot.state, RuntimeState::Running | RuntimeState::Paused)
        {
            publish(&self.shared, |snapshot| {
                snapshot.state = RuntimeState::Stopping;
                snapshot.pause_reason = None;
            });
        }
        true
    }
    pub fn snapshot(&self) -> Arc<RuntimeSnapshot> {
        self.shared.snapshot.read().unwrap().clone()
    }
    pub fn recording_preview_status(&self, ticket: u64) -> (bool, Option<Arc<RuntimeSnapshot>>) {
        let active = self
            .shared
            .admission
            .lock()
            .unwrap()
            .is_some_and(|admission| {
                admission.origin == RuntimeOrigin::RecordingPreview { ticket }
            });
        let snapshot = self
            .shared
            .last_preview_snapshot
            .read()
            .unwrap()
            .as_ref()
            .filter(|snapshot| snapshot.origin == RuntimeOrigin::RecordingPreview { ticket })
            .cloned();
        (active, snapshot)
    }
    #[cfg(test)]
    pub(crate) fn take_test_commands(&self) -> Vec<RuntimeCommand> {
        std::mem::take(&mut *self.test_commands.lock().unwrap())
    }
    #[cfg(test)]
    pub(crate) fn take_test_events(&self) -> Vec<ExecutionEvent> {
        std::mem::take(&mut *self.shared.test_events.lock().unwrap())
    }
    #[cfg(test)]
    pub(crate) fn install_test_worker_barrier(&self) -> TestWorkerBarrier {
        let barrier = Arc::new(TestWorkerBarrierInner {
            state: Mutex::new((false, false)),
            wake: Condvar::new(),
        });
        let mut slot = self.shared.test_worker_barrier.lock().unwrap();
        assert!(
            slot.is_none(),
            "runtime test worker barrier is already installed"
        );
        *slot = Some(barrier.clone());
        TestWorkerBarrier(barrier)
    }
    pub fn shutdown(&self) {
        {
            let _admission = self.shared.admission.lock().unwrap();
            self.available.store(false, Ordering::Release);
            self.shared.control.stop();
            let _ = self.tx.send(WorkerMessage {
                command: RuntimeCommand::Shutdown,
                program: None,
                origin: RuntimeOrigin::Stored,
            });
        }
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
    // One owner serializes the transformation, not just the final replacement.
    // Callers must acquire admission/control before entering this boundary.
    let mut current = shared.snapshot.write().unwrap();
    let next = Arc::make_mut(&mut current);
    f(next);
    next.revision += 1;
    if matches!(next.origin, RuntimeOrigin::RecordingPreview { .. }) {
        *shared.last_preview_snapshot.write().unwrap() = Some(current.clone());
    }
}

fn controlled_state(control: &RunControl) -> RuntimeState {
    if control.is_stopped() {
        RuntimeState::Stopping
    } else if control.is_paused() {
        RuntimeState::Paused
    } else {
        RuntimeState::Running
    }
}

fn frame_name(snapshot: &RuntimeSnapshot, context: ExecutionFrameContext) -> Arc<str> {
    snapshot
        .call_stack
        .last()
        .filter(|frame| frame.context == context)
        .map(|frame| frame.macro_name.clone())
        .unwrap_or_else(|| Arc::from(format!("Macro #{}", context.macro_id)))
}

fn clear_debug_variables(snapshot: &mut RuntimeSnapshot) {
    if snapshot.debug_snapshot.is_none()
        && snapshot.debug_variables.is_empty()
        && snapshot.debug_variables_step_id.is_none()
        && snapshot.debug_snapshot_reason.is_none()
    {
        return;
    }
    snapshot.debug_snapshot = None;
    snapshot.debug_variables = Arc::new(BTreeMap::new());
    snapshot.debug_variables_step_id = None;
    snapshot.debug_snapshot_reason = None;
}

fn publish_debug_variables(
    snapshot: &mut RuntimeSnapshot,
    frame: ExecutionFrameContext,
    step_id: Option<u64>,
    variables: RuntimeVariables,
    reason: DebugSnapshotReason,
) {
    let variables = Arc::new(variables);
    snapshot.debug_snapshot = Some(Arc::new(DebugSnapshot {
        frame,
        macro_name: frame_name(snapshot, frame),
        step_id,
        variables: variables.clone(),
        reason,
    }));
    snapshot.debug_variables = variables;
    snapshot.debug_variables_step_id = step_id;
    snapshot.debug_snapshot_reason = Some(reason);
}

fn set_step_state(
    snapshot: &mut RuntimeSnapshot,
    frame: ExecutionFrameContext,
    step_id: u64,
    state: StepState,
) {
    Arc::make_mut(&mut snapshot.macro_steps)
        .insert(MacroStepKey::new(frame.macro_id, step_id), state);
    if frame.depth == 1 {
        Arc::make_mut(&mut snapshot.steps).insert(step_id, state);
    }
}

fn set_active_step(snapshot: &mut RuntimeSnapshot, context: ExecutionFrameContext, step_id: u64) {
    if let Some(frame) = snapshot.call_stack.last() {
        if frame.context == context && frame.active_step_id != Some(step_id) {
            Arc::make_mut(&mut snapshot.call_stack)
                .last_mut()
                .unwrap()
                .active_step_id = Some(step_id);
        }
    }
    if context.depth == 1 {
        snapshot.step_id = Some(step_id);
    }
}

fn complete_step(
    snapshot: &mut RuntimeSnapshot,
    context: ExecutionFrameContext,
    step_id: u64,
    outcome: CompletedStepOutcome,
) {
    snapshot.last_completed = Some(Arc::new(CompletedStep {
        frame: context,
        key: MacroStepKey::new(context.macro_id, step_id),
        macro_name: frame_name(snapshot, context),
        outcome,
    }));
    if context.depth == 1 {
        snapshot.last_completed_step_id = Some(step_id);
    }
}

fn apply_execution_event(
    snapshot: &mut RuntimeSnapshot,
    program: &super::MkCompiledProgram,
    context: ExecutionFrameContext,
    event: ExecutionEvent,
    control_state: RuntimeState,
) {
    snapshot.state = control_state;
    if control_state != RuntimeState::Paused {
        snapshot.pause_reason = None;
    } else if snapshot.pause_reason.is_none() {
        snapshot.pause_reason = Some(RuntimePauseReason::User);
    }
    match event {
        ExecutionEvent::FrameEntered(frame) => {
            if context.depth == 1 {
                snapshot.root_macro_name = Some(frame.macro_name.clone());
            }
            Arc::make_mut(&mut snapshot.call_stack).push(frame);
            clear_debug_variables(snapshot);
            // Only frame entry consults the immutable program. Repeated calls
            // reset current row state without altering the last completed record.
            if let Some(plan) = program.plan(context.macro_id) {
                Arc::make_mut(&mut snapshot.macro_steps)
                    .retain(|key, _| key.macro_id != context.macro_id);
                Arc::make_mut(&mut snapshot.macro_step_outcomes)
                    .retain(|key, _| key.macro_id != context.macro_id);
                Arc::make_mut(&mut snapshot.macro_failures)
                    .retain(|key, _| key.step.macro_id != context.macro_id);
                for instruction in plan.instructions.iter() {
                    set_step_state(
                        snapshot,
                        context,
                        instruction.step.id,
                        if instruction.step.enabled {
                            StepState::Pending
                        } else {
                            StepState::Skipped
                        },
                    );
                }
            }
        }
        ExecutionEvent::FrameExited { caller_boundary } => {
            if snapshot
                .call_stack
                .last()
                .is_some_and(|frame| frame.context == context)
            {
                Arc::make_mut(&mut snapshot.call_stack).pop();
            }
            if let Some(caller) = snapshot.active_frame() {
                if caller.depth == 1 {
                    snapshot.step_id = snapshot.active_step_id();
                }
                if let Some(boundary) = caller_boundary {
                    publish_debug_variables(
                        snapshot,
                        caller,
                        boundary.step_id,
                        boundary.variables,
                        DebugSnapshotReason::FrameRestored,
                    );
                } else {
                    clear_debug_variables(snapshot);
                }
            }
        }
        ExecutionEvent::BreakpointHit { step_id, variables } => {
            snapshot.breakpoint_sequence += 1;
            set_active_step(snapshot, context, step_id);
            set_step_state(snapshot, context, step_id, StepState::Pending);
            if control_state == RuntimeState::Paused {
                snapshot.pause_reason = Some(RuntimePauseReason::Breakpoint {
                    step_id,
                    frame: context,
                });
            }
            publish_debug_variables(
                snapshot,
                context,
                Some(step_id),
                variables,
                DebugSnapshotReason::Breakpoint,
            );
        }
        ExecutionEvent::DebugVariables {
            step_id,
            variables,
            reason,
        } => {
            publish_debug_variables(snapshot, context, step_id, variables, reason);
        }
        ExecutionEvent::StepStarted(step_id) => {
            set_active_step(snapshot, context, step_id);
            set_step_state(snapshot, context, step_id, StepState::Running);
        }
        ExecutionEvent::StepFinished(step_id) => {
            set_step_state(snapshot, context, step_id, StepState::Success);
            if context.depth == 1 {
                snapshot.completed_steps += 1;
            }
            let outcome = snapshot
                .macro_step_outcomes
                .get(&MacroStepKey::new(context.macro_id, step_id))
                .cloned();
            complete_step(
                snapshot,
                context,
                step_id,
                CompletedStepOutcome::Success(outcome),
            );
        }
        ExecutionEvent::StepOutcome(step_id, outcome) => {
            Arc::make_mut(&mut snapshot.macro_step_outcomes).insert(
                MacroStepKey::new(context.macro_id, step_id),
                outcome.clone(),
            );
            if context.depth == 1 {
                Arc::make_mut(&mut snapshot.step_outcomes).insert(step_id, outcome);
            }
        }
        ExecutionEvent::StepSkipped(step_id) => {
            set_step_state(snapshot, context, step_id, StepState::Skipped)
        }
        ExecutionEvent::StepFailed(step_id, diagnostic) => {
            set_step_state(snapshot, context, step_id, StepState::Failed);
            snapshot.latest_failure = Some(diagnostic.clone());
            Arc::make_mut(&mut snapshot.macro_failures).insert(
                MacroDiagnosticKey {
                    run_id: snapshot.run_id,
                    step: MacroStepKey::new(context.macro_id, step_id),
                },
                diagnostic.clone(),
            );
            if context.depth == 1 {
                Arc::make_mut(&mut snapshot.failures).insert(
                    DiagnosticKey {
                        run_id: snapshot.run_id,
                        step_id,
                    },
                    diagnostic.clone(),
                );
            }
            complete_step(
                snapshot,
                context,
                step_id,
                CompletedStepOutcome::Failure(diagnostic),
            );
        }
        ExecutionEvent::Paused | ExecutionEvent::Resumed => {
            // Shared control, read before publication, is authoritative.
        }
    }
}

fn publish_manual_pause(shared: &Shared) {
    publish(shared, set_manual_pause);
}

fn set_manual_pause(snapshot: &mut RuntimeSnapshot) {
    if snapshot.state == RuntimeState::Stopping {
        return;
    }
    snapshot.state = RuntimeState::Paused;
    // BreakpointHit is the authoritative pause event. A Pause command can
    // already be queued when that event arrives, so manual pause updates
    // must not downgrade the reason while still stopped at that breakpoint.
    let stopped_at_breakpoint = matches!(snapshot.pause_reason,
        Some(RuntimePauseReason::Breakpoint { step_id, frame })
            if snapshot.active_frame() == Some(frame) && snapshot.active_step_id() == Some(step_id));
    if !stopped_at_breakpoint {
        snapshot.pause_reason = Some(RuntimePauseReason::User);
    }
}

fn clear_pause_reason(shared: &Shared) {
    if shared.snapshot.read().unwrap().pause_reason.is_some() {
        publish(shared, |snapshot| snapshot.pause_reason = None);
    }
}

fn worker_loop(
    store: Arc<MkMacroStore>,
    backends: Backends,
    rx: mpsc::Receiver<WorkerMessage>,
    shared: Arc<Shared>,
) {
    while let Ok(WorkerMessage {
        command,
        program,
        origin,
    }) = rx.recv()
    {
        match command {
            RuntimeCommand::Shutdown => {
                clear_pause_reason(&shared);
                break;
            }
            // Control was already applied synchronously by command(). Replaying
            // it after a run ends could corrupt the next admitted run's state.
            RuntimeCommand::Pause | RuntimeCommand::Resume | RuntimeCommand::Stop => {}
            command => {
                let request = run_request(&command).expect("run commands are classified");
                #[cfg(test)]
                if let Some(barrier) = shared.test_worker_barrier.lock().unwrap().take() {
                    let mut state = barrier.state.lock().unwrap();
                    state.0 = true;
                    barrier.wake.notify_all();
                    while !state.1 {
                        state = barrier.wake.wait(state).unwrap();
                    }
                }
                run_one(&store, &backends, &shared, request, program, origin)
            }
        }
    }
    shared.control.stop();
    let mut admission = shared.admission.lock().unwrap();
    shared.operations.release(Operation::Playback);
    *admission = None;
}
/// Admission covers compilation and invocation preparation as well as effects.
/// The executor releases owned input before this root lifecycle releases activity.
struct RootRunGuard<'a> {
    shared: &'a Shared,
    completed: bool,
}
impl RootRunGuard<'_> {
    fn complete(mut self, publish_terminal: impl FnOnce()) {
        let mut admission = self.shared.admission.lock().unwrap();
        self.shared.control.finish();
        publish_terminal();
        self.shared.operations.release(Operation::Playback);
        *admission = None;
        self.completed = true;
    }
}
impl Drop for RootRunGuard<'_> {
    fn drop(&mut self) {
        if !self.completed {
            let mut admission = self.shared.admission.lock().unwrap();
            self.shared.control.finish();
            self.shared.operations.release(Operation::Playback);
            *admission = None;
        }
    }
}

fn run_one(
    store: &MkMacroStore,
    backends: &Backends,
    shared: &Shared,
    request: RunRequest<'_>,
    prepared_program: Option<super::MkCompiledProgram>,
    origin: RuntimeOrigin,
) {
    let RunRequest {
        macro_id: mid,
        mode,
        arguments,
        ..
    } = request;
    let run_guard = RootRunGuard {
        shared,
        completed: false,
    };
    #[cfg(test)]
    shared.test_events.lock().unwrap().clear();
    let run_id = shared.next_run_id.fetch_add(1, Ordering::Relaxed);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let program = match prepared_program {
            Some(program) => program,
            None => {
                let invocation = request.to_invocation();
                super::invocation::compile_invocation_program(&store.snapshot(), &invocation)?
            }
        };
        let plan = program.plan(mid).ok_or_else(|| {
            ExecutionDiagnostic::new(
                DiagnosticKind::InvalidPlan,
                "Compiled program root is missing",
            )
        })?;
        let admission = shared.admission.lock().unwrap();
        let state = controlled_state(&shared.control);
        publish(shared, |s| {
            *s = RuntimeSnapshot {
                state,
                run_mode: mode,
                run_id,
                origin,
                macro_id: Some(mid),
                pause_reason: (state == RuntimeState::Paused).then_some(RuntimePauseReason::User),
                total_steps: plan.instructions.iter().filter(|x| x.step.enabled).count(),
                started_at: Some(SystemTime::now()),
                revision: s.revision,
                ..RuntimeSnapshot::default()
            };
        });
        drop(admission);
        let observer = |context: ExecutionFrameContext, event: ExecutionEvent| {
            #[cfg(test)]
            if context.depth == 1
                && !matches!(
                    event,
                    ExecutionEvent::FrameEntered(_) | ExecutionEvent::FrameExited { .. }
                )
            {
                shared.test_events.lock().unwrap().push(event.clone());
            }
            // Serialize state decisions with control commands. Read control
            // before snapshot publication to preserve the single lock order.
            let _admission = shared.admission.lock().unwrap();
            let state = controlled_state(&shared.control);
            publish(shared, |snapshot| {
                apply_execution_event(snapshot, &program, context, event, state)
            });
        };
        let executor = Executor::new(backends.clone(), shared.control.clone());
        let options = match mode {
            RuntimeRunMode::Normal => ExecutionOptions::normal(),
            RuntimeRunMode::Debug => ExecutionOptions::debug(),
        };
        executor.execute_program(
            &program,
            arguments.unwrap_or(&MkInvocationValues::new()),
            options,
            &observer,
        )
    }))
    .unwrap_or_else(|payload| {
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| {
                payload
                    .downcast_ref::<&str>()
                    .map(|text| (*text).to_owned())
            })
            .unwrap_or_else(|| "non-string panic payload".into());
        tracing::error!(macro_id = mid, %message, "macro execution panicked");
        Err(ExecutionDiagnostic::new(
            DiagnosticKind::Panic,
            format!("Macro execution panicked: {message}"),
        )
        .context("macro_id", mid.to_string()))
    });
    run_guard.complete(|| {
        let stopped = shared.control.is_stopped();
        publish(shared, |s| {
            // Failed preparation still belongs to this admitted run, rather
            // than retaining the identity/status maps of an earlier macro.
            if s.run_id != run_id {
                *s = RuntimeSnapshot {
                    run_id,
                    origin,
                    macro_id: Some(mid),
                    run_mode: mode,
                    revision: s.revision,
                    ..RuntimeSnapshot::default()
                };
            }
            match result {
                Ok(()) => {
                    s.state = if stopped {
                        RuntimeState::Stopped
                    } else {
                        RuntimeState::Completed
                    }
                }
                Err(d) if d.kind == DiagnosticKind::Cancelled => s.state = RuntimeState::Stopped,
                Err(d) => {
                    s.state = RuntimeState::Failed;
                    s.latest_failure = Some(d);
                }
            }
            // A panic can bypass frame-exit events. Terminal snapshots have no
            // active stack, while the last captured Debug identity stays owned.
            if !s.call_stack.is_empty() {
                s.call_stack = Arc::new(Vec::new());
            }
            let reason = match s.state {
                RuntimeState::Completed => DebugSnapshotReason::RunFinished,
                RuntimeState::Stopped => DebugSnapshotReason::RunCancelled,
                _ => DebugSnapshotReason::RunFailed,
            };
            if let Some(debug) = &mut s.debug_snapshot {
                Arc::make_mut(debug).reason = reason;
                s.debug_snapshot_reason = Some(reason);
            }
            s.step_id = None;
            s.pause_reason = None;
            s.finished_at = Some(SystemTime::now());
        });
    });
}

static RUNTIME: Lazy<RwLock<Option<Arc<MacroRuntime>>>> = Lazy::new(|| RwLock::new(None));
static RECORDER: Lazy<RwLock<Option<Arc<RecorderRuntime>>>> = Lazy::new(|| RwLock::new(None));
static HOTKEYS: Lazy<RwLock<Option<Arc<super::hotkeys::MkMacroHotkeyService>>>> =
    Lazy::new(|| RwLock::new(None));
static RECORDER_HOTKEYS: Lazy<RwLock<Option<Arc<super::recorder_hotkeys::RecorderHotkeyService>>>> =
    Lazy::new(|| RwLock::new(None));
#[derive(Clone)]
struct RecordingArm {
    target: Option<RecordingTarget>,
    config: NormalizationConfig,
}
static RECORDING_ARM: Lazy<RwLock<RecordingArm>> = Lazy::new(|| {
    RwLock::new(RecordingArm {
        target: None,
        config: NormalizationConfig::default(),
    })
});
static RECORDING_STATUS: Lazy<RwLock<Option<String>>> = Lazy::new(|| RwLock::new(None));
static RECORDING_ANNOTATION_ACTIVE: AtomicBool = AtomicBool::new(false);
static PENDING_RECORDINGS: Lazy<Mutex<Vec<RecordingResult>>> = Lazy::new(|| Mutex::new(Vec::new()));
static RECORD_STOP_COORDINATOR: Lazy<(Mutex<bool>, Condvar)> =
    Lazy::new(|| (Mutex::new(false), Condvar::new()));
struct RecordStopJob {
    recorder: Arc<RecorderRuntime>,
    occurrence: Vec<u32>,
}
struct RecordStopCompletion;
impl Drop for RecordStopCompletion {
    fn drop(&mut self) {
        let mut pending = RECORD_STOP_COORDINATOR.0.lock().unwrap();
        *pending = false;
        RECORD_STOP_COORDINATOR.1.notify_all();
    }
}
static RECORD_STOP_WORKER: Lazy<mpsc::Sender<RecordStopJob>> = Lazy::new(|| {
    let (tx, rx) = mpsc::channel::<RecordStopJob>();
    thread::Builder::new()
        .name("mkmacro-record-stop".into())
        .spawn(move || {
            while let Ok(job) = rx.recv() {
                let _completion = RecordStopCompletion;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    job.recorder.stop_for_review_with_control(job.occurrence)
                }))
                .unwrap_or_else(|_| Err(anyhow!("recording finalization panicked")));
                match result {
                    Ok(result) => {
                        PENDING_RECORDINGS.lock().unwrap().push(result);
                        *RECORDING_STATUS.write().unwrap() = None;
                    }
                    Err(error) => {
                        *RECORDING_STATUS.write().unwrap() = Some(error.to_string());
                    }
                }
                if job.recorder.snapshot().state == super::RecorderRuntimeState::Idle {
                    job.recorder.complete_review_transfer();
                }
            }
        })
        .expect("spawn recorder stop worker");
    tx
});
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

/// Replaces only launcher-owned hotkey reservations without rebuilding the
/// macro runtime, recorder, or store during a settings reload.
pub fn refresh_shared_hotkey_reservations(reserved: &[(&str, &str)]) -> Result<(), String> {
    let service = HOTKEYS
        .read()
        .map_err(|_| "shared macro hotkey service lock is poisoned".to_string())?
        .clone()
        .ok_or_else(|| "shared macro hotkey service is not initialized".to_string())?;
    service.replace_reserved(reserved)
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
    set_shared_store_components(
        store,
        backends,
        reserved,
        production_hook_service(8192),
        true,
        None,
    )
}
fn set_shared_store_components(
    store: Arc<MkMacroStore>,
    backends: Backends,
    reserved: &[(&str, &str)],
    recorder_hooks: super::HookService,
    start_hotkey_services: bool,
    recorder_components: Option<(
        Box<dyn KeyboardTranslator>,
        Arc<dyn Fn() -> RecorderObserverSession + Send + Sync>,
        Box<dyn EventEnricher>,
    )>,
) {
    // Stop pollers before taking stop coordination; a poller callback may be
    // in the process of enqueuing the current recording's Stop occurrence.
    if let Some(old) = HOTKEYS.write().unwrap().take() {
        old.shutdown()
    }
    if let Some(old) = RECORDER_HOTKEYS.write().unwrap().take() {
        old.shutdown()
    }
    // Runtime replacement cannot discard an accepted stop/review transfer.
    // Serialize enqueue against replacement and wait for the exact captured
    // recorder instance to finish before shutting it down.
    let mut stop_coordination = RECORD_STOP_COORDINATOR.0.lock().unwrap();
    while *stop_coordination {
        stop_coordination = RECORD_STOP_COORDINATOR.1.wait(stop_coordination).unwrap();
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
    let recorder = match recorder_components {
        Some((translator, factory, enricher)) => RecorderRuntime::with_guard_and_components(
            store.clone(),
            recorder_hooks,
            Arc::new(SystemRecorderClock::default()),
            guard,
            translator,
            factory,
            enricher,
        ),
        None => RecorderRuntime::with_guard(
            store.clone(),
            recorder_hooks,
            Arc::new(SystemRecorderClock::default()),
            guard,
        ),
    };
    *RECORDER.write().unwrap() = Some(Arc::new(recorder));
    if start_hotkey_services {
        *HOTKEYS.write().unwrap() = Some(Arc::new(
            super::hotkeys::MkMacroHotkeyService::new_with_reserved(store.clone(), reserved),
        ));
        *RECORDER_HOTKEYS.write().unwrap() = Some(Arc::new(
            super::recorder_hotkeys::RecorderHotkeyService::system(store),
        ));
    }
}
fn global() -> Result<Arc<MacroRuntime>> {
    RUNTIME
        .read()
        .unwrap()
        .clone()
        .ok_or_else(|| anyhow!("macro runtime is not initialized"))
}
fn command_result(result: CommandResult) -> ExecResult {
    match result {
        CommandResult::Accepted => Ok(()),
        CommandResult::Rejected(diagnostic) => Err(diagnostic),
        CommandResult::AlreadyRunning { active_macro_id } => Err(ExecutionDiagnostic::new(
            DiagnosticKind::RuntimeUnavailable,
            format!("Macro {active_macro_id} is already running"),
        )),
    }
}
/// Success includes a queued parameter request; no execution is admitted until
/// complete typed values have been confirmed.
fn prepare_direct(command: RuntimeCommand) -> Result<()> {
    global()?.prepare_direct_command(
        command,
        &super::invocation_prompt::production_invocation_prompt_broker(),
    )?;
    Ok(())
}
fn accepted(r: CommandResult) -> Result<()> {
    match r {
        CommandResult::Accepted => Ok(()),
        x => Err(anyhow!("{x:?}")),
    }
}
pub fn invoke(invocation: MkInvocation) -> Result<()> {
    prepare_direct(RuntimeCommand::Invoke(invocation))
}
pub fn run(id: u64) -> Result<()> {
    prepare_direct(RuntimeCommand::Run(id))
}
pub fn run_from(macro_id: u64, step_id: u64) -> Result<()> {
    prepare_direct(RuntimeCommand::RunFrom(macro_id, step_id))
}
pub fn run_selection(macro_id: u64, ids: Vec<u64>) -> Result<()> {
    prepare_direct(RuntimeCommand::RunSelection(macro_id, ids))
}
pub fn debug_run(macro_id: u64) -> Result<()> {
    prepare_direct(RuntimeCommand::DebugRun(macro_id))
}
pub fn debug_run_from(macro_id: u64, step_id: u64) -> Result<()> {
    prepare_direct(RuntimeCommand::DebugRunFrom(macro_id, step_id))
}
pub fn debug_run_selection(macro_id: u64, ids: Vec<u64>) -> Result<()> {
    prepare_direct(RuntimeCommand::DebugRunSelection(macro_id, ids))
}
/// Compile and submit an ephemeral document snapshot without publishing it to
/// the store. It shares the normal worker, admission, controls, diagnostics,
/// and executor cleanup path.
pub fn preview_document(document: &super::MkMacroDocument, macro_id: u64) -> Result<u64> {
    let ticket = NEXT_PREVIEW_TICKET.fetch_add(1, Ordering::Relaxed);
    global()?.preview(document, macro_id, ticket)?;
    Ok(ticket)
}
pub fn stop_recording_preview(ticket: u64) -> bool {
    RUNTIME
        .read()
        .unwrap()
        .as_ref()
        .is_some_and(|runtime| runtime.stop_recording_preview(ticket))
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
/// Returns admission and the last retained snapshot for one preview ticket.
/// The retained snapshot survives later stored playback snapshots.
pub fn recording_preview_status(ticket: u64) -> (bool, Option<Arc<RuntimeSnapshot>>) {
    RUNTIME
        .read()
        .unwrap()
        .as_ref()
        .map_or((false, None), |runtime| {
            runtime.recording_preview_status(ticket)
        })
}
#[cfg(test)]
pub(crate) fn test_runtime() -> Option<Arc<MacroRuntime>> {
    RUNTIME.read().unwrap().clone()
}
pub fn record_target(target: RecordingTarget, config: NormalizationConfig) -> Result<()> {
    if record_stop_pending() {
        return Err(anyhow!("recording is stopping for review"));
    }
    RECORDER
        .read()
        .unwrap()
        .clone()
        .ok_or_else(|| anyhow!("macro runtime is not initialized"))?
        .start_target(target, config, Vec::new())
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
/// Stops capture and transfers ownership to the GUI's pending Review queue.
/// Command/headless callers must use this path because they cannot directly
/// present or safely discard the returned transient recording.
pub fn record_stop_for_review() -> Result<()> {
    request_record_stop_for_review().map(|_| ())
}
/// Requests capture finalization on one owned worker so egui can render the
/// Stopping state while observation/UIA lanes flush. Duplicate clicks are
/// idempotent and every successful result reaches the same Review queue.
pub fn request_record_stop_for_review() -> Result<bool> {
    let mut pending = RECORD_STOP_COORDINATOR.0.lock().unwrap();
    let recorder = RECORDER
        .read()
        .unwrap()
        .clone()
        .ok_or_else(|| anyhow!("macro runtime is not initialized"))?;
    enqueue_record_stop_for_review(&mut pending, recorder, Vec::new())
}
pub fn record_stop_pending() -> bool {
    *RECORD_STOP_COORDINATOR.0.lock().unwrap()
}

fn request_record_stop_for_review_on(
    recorder: Arc<RecorderRuntime>,
    occurrence: Vec<u32>,
) -> Result<bool> {
    let mut pending = RECORD_STOP_COORDINATOR.0.lock().unwrap();
    enqueue_record_stop_for_review(&mut pending, recorder, occurrence)
}

fn enqueue_record_stop_for_review(
    pending: &mut bool,
    recorder: Arc<RecorderRuntime>,
    occurrence: Vec<u32>,
) -> Result<bool> {
    if *pending {
        return Ok(false);
    }
    match recorder.snapshot().state {
        super::RecorderRuntimeState::Recording | super::RecorderRuntimeState::Paused => {}
        super::RecorderRuntimeState::Stopping => return Ok(false),
        super::RecorderRuntimeState::Idle => return Err(anyhow!("recorder is not active")),
    }
    *pending = true;
    RECORDING_ANNOTATION_ACTIVE.store(false, Ordering::Release);
    if RECORD_STOP_WORKER
        .send(RecordStopJob {
            recorder,
            occurrence,
        })
        .is_err()
    {
        *pending = false;
        return Err(anyhow!("recording stop worker is unavailable"));
    }
    Ok(true)
}
pub fn recorder_snapshot() -> Option<Arc<RecorderSnapshot>> {
    RECORDER.read().unwrap().as_ref().map(|r| r.snapshot())
}
pub fn record_marker() -> Result<()> {
    RECORDER
        .read()
        .unwrap()
        .clone()
        .ok_or_else(|| anyhow!("macro runtime is not initialized"))?
        .marker()
}
pub fn record_annotation(text: String) -> Result<()> {
    RECORDER
        .read()
        .unwrap()
        .clone()
        .ok_or_else(|| anyhow!("macro runtime is not initialized"))?
        .annotation(text)
}
pub fn set_recording_annotation_active(active: bool) {
    RECORDING_ANNOTATION_ACTIVE.store(active, Ordering::Release);
}
pub fn arm_recording(target: Option<RecordingTarget>, options: NormalizationConfig) {
    *RECORDING_ARM.write().unwrap() = RecordingArm {
        target,
        config: options,
    };
    if target.is_some()
        && RECORDING_STATUS.read().unwrap().as_deref()
            == Some("Select a macro before starting recording")
    {
        *RECORDING_STATUS.write().unwrap() = None;
    }
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
    recorder_control(super::recorder_hotkeys::RecorderControlAction::Toggle)
}

fn hotkey_keys(hotkey: &super::MkHotkey) -> Vec<u32> {
    hotkey
        .modifiers
        .iter()
        .chain(std::iter::once(&hotkey.key))
        .filter_map(|key| super::virtual_key(key).map(u32::from))
        .collect()
}
fn control_hotkey(
    doc: &super::MkMacroDocument,
    action: super::recorder_hotkeys::RecorderControlAction,
) -> Option<&super::MkHotkey> {
    match action {
        super::recorder_hotkeys::RecorderControlAction::Toggle => {
            Some(&doc.settings.record_toggle_hotkey)
        }
        super::recorder_hotkeys::RecorderControlAction::PauseResume => {
            doc.settings.recorder.pause_resume_hotkey.as_ref()
        }
        super::recorder_hotkeys::RecorderControlAction::Marker => {
            doc.settings.recorder.marker_hotkey.as_ref()
        }
    }
}
pub(crate) fn recorder_control(action: super::recorder_hotkeys::RecorderControlAction) {
    if RECORDING_ANNOTATION_ACTIVE.load(Ordering::Acquire)
        && action != super::recorder_hotkeys::RecorderControlAction::Toggle
    {
        return;
    }
    let Some(recorder) = RECORDER.read().unwrap().clone() else {
        return;
    };
    let document = recorder.document_snapshot();
    let occurrence = document
        .as_deref()
        .and_then(|doc| control_hotkey(doc, action))
        .map(hotkey_keys)
        .unwrap_or_default();
    let result: Result<()> = match (action, recorder.snapshot().state) {
        (
            super::recorder_hotkeys::RecorderControlAction::Toggle,
            super::RecorderRuntimeState::Idle,
        ) => {
            if record_stop_pending() {
                return;
            }
            let arm = RECORDING_ARM.read().unwrap().clone();
            let target = arm.target;
            let Some(target) = target else {
                *RECORDING_STATUS.write().unwrap() =
                    Some("Select a macro before starting recording".into());
                return;
            };
            recorder.start_target(target, arm.config, occurrence)
        }
        (
            super::recorder_hotkeys::RecorderControlAction::Toggle,
            super::RecorderRuntimeState::Recording | super::RecorderRuntimeState::Paused,
        ) => request_record_stop_for_review_on(recorder.clone(), occurrence).map(|_| ()),
        (
            super::recorder_hotkeys::RecorderControlAction::PauseResume,
            super::RecorderRuntimeState::Recording,
        ) => recorder.pause_with_control(occurrence),
        (
            super::recorder_hotkeys::RecorderControlAction::PauseResume,
            super::RecorderRuntimeState::Paused,
        ) => recorder.resume_with_held(occurrence),
        (
            super::recorder_hotkeys::RecorderControlAction::Marker,
            super::RecorderRuntimeState::Recording,
        ) => recorder
            .suppress_control(occurrence)
            .and_then(|()| recorder.marker()),
        (
            super::recorder_hotkeys::RecorderControlAction::Marker,
            super::RecorderRuntimeState::Paused,
        ) => recorder.marker(),
        (_, super::RecorderRuntimeState::Stopping | super::RecorderRuntimeState::Idle) => return,
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
            signature: Default::default(),
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
                metadata: Default::default(),
                id: 11,
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                action: MkAction::Text(MkTextPayload {
                    text: "target executed".into(),
                    mode: MkTextMode::Type,
                }),
            }],
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
mod run_mode_tests {
    use super::*;
    use crate::mkmacro::prompt::{PromptBackend, PromptRequest, PromptResponse};
    use crate::mkmacro::{
        MKMACROS_FILE, MkAction, MkCondition, MkCoordinateTarget, MkDelayPayload, MkKey, MkMacro,
        MkMacroDocument, MkMouseButton, MkMouseMovePayload, MkMousePayload, MkPoint,
        MkPromptInputPayload, MkStep, MkTextMode, MkTextPayload, MkValue,
        executor::fake::FakeBackend,
    };
    use std::fs;
    use std::sync::{Condvar, Mutex};
    use std::time::{Duration, Instant};

    struct AckHookLoop;
    impl crate::mkmacro::HookLoopAdapter for AckHookLoop {
        fn run(
            self,
            commands: std::sync::mpsc::Receiver<crate::mkmacro::HookCommandRequest>,
            _callback: crate::mkmacro::CallbackSender,
        ) {
            while let Ok(request) = commands.recv() {
                let shutdown = request.command == crate::mkmacro::HookCommand::Shutdown;
                request.acknowledge(true);
                if shutdown {
                    break;
                }
            }
        }
    }

    struct NoTextTranslator;
    impl crate::mkmacro::KeyboardTranslator for NoTextTranslator {
        fn translate(
            &mut self,
            _request: &crate::mkmacro::KeyboardTranslationRequest,
        ) -> crate::mkmacro::KeyTranslation {
            crate::mkmacro::KeyTranslation::None
        }
    }

    struct NoContextEnricher;
    impl crate::mkmacro::EventEnricher for NoContextEnricher {
        fn enrich(
            &mut self,
            _event: &crate::mkmacro::HookEvent,
        ) -> Option<crate::mkmacro::EventContext> {
            None
        }
    }

    struct FixedRecorderClock;
    impl crate::mkmacro::RecorderClock for FixedRecorderClock {
        fn now_us(&self) -> u64 {
            1
        }
    }

    fn empty_recorder_observer() -> crate::mkmacro::RecorderObserverSession {
        crate::mkmacro::RecorderObserverSession::with_parts(
            crate::mkmacro::ObservationBaseline::default(),
            crate::mkmacro::AuxiliaryObservationWorker::spawn(None, None),
        )
    }

    fn step(id: u64, action: MkAction) -> MkStep {
        MkStep {
            metadata: Default::default(),
            id,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action,
        }
    }

    fn test_macro(id: u64, enabled: bool, steps: Vec<MkStep>) -> MkMacro {
        MkMacro {
            signature: Default::default(),
            id,
            name: format!("macro {id}"),
            description: String::new(),
            enabled,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            steps,
        }
    }

    fn runtime_with(
        macros: Vec<MkMacro>,
    ) -> (tempfile::TempDir, MacroRuntime, Arc<SharedOperationGuard>) {
        let (dir, runtime, guard, _fake) = runtime_with_effects(macros);
        (dir, runtime, guard)
    }

    fn runtime_with_effects(
        macros: Vec<MkMacro>,
    ) -> (
        tempfile::TempDir,
        MacroRuntime,
        Arc<SharedOperationGuard>,
        Arc<FakeBackend>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        store
            .save(MkMacroDocument {
                macros,
                ..Default::default()
            })
            .unwrap();
        let guard = Arc::new(SharedOperationGuard::default());
        let fake = Arc::new(FakeBackend::default());
        let runtime =
            MacroRuntime::with_guard(Arc::new(store), fake.clone().backends(), guard.clone());
        (dir, runtime, guard, fake)
    }

    fn runtime_with_backends(
        macros: Vec<MkMacro>,
        backends: Backends,
    ) -> (tempfile::TempDir, MacroRuntime, Arc<SharedOperationGuard>) {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        store
            .save(MkMacroDocument {
                macros,
                ..Default::default()
            })
            .unwrap();
        let guard = Arc::new(SharedOperationGuard::default());
        let runtime = MacroRuntime::with_guard(Arc::new(store), backends, guard.clone());
        (dir, runtime, guard)
    }

    fn wait_for_terminal(runtime: &MacroRuntime) -> Arc<RuntimeSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = runtime.snapshot();
            if matches!(
                snapshot.state,
                RuntimeState::Completed | RuntimeState::Failed | RuntimeState::Stopped
            ) {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "runtime did not finish: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn wait_for_terminal_after(
        runtime: &MacroRuntime,
        previous_run_id: u64,
    ) -> Arc<RuntimeSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = runtime.snapshot();
            if snapshot.run_id > previous_run_id
                && matches!(
                    snapshot.state,
                    RuntimeState::Completed | RuntimeState::Failed | RuntimeState::Stopped
                )
            {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "runtime did not finish: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn wait_for_state(runtime: &MacroRuntime, wanted: RuntimeState) -> Arc<RuntimeSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = runtime.snapshot();
            if snapshot.state == wanted {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "runtime did not reach {wanted:?}: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn wait_for_debug_boundary(
        runtime: &MacroRuntime,
        step_id: Option<u64>,
        reason: DebugSnapshotReason,
    ) -> Arc<RuntimeSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = runtime.snapshot();
            if snapshot
                .debug_snapshot
                .as_ref()
                .is_some_and(|debug| debug.step_id == step_id && debug.reason == reason)
            {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "runtime did not publish debug boundary {step_id:?}/{reason:?}: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn frame_debugger_nested_same_ids_resume_stop_and_normal_mode() {
        use super::super::{
            MkCallMacroPayload, MkKey, MkMacroParameter, MkSignatureId, MkValueType,
        };
        for (mode, stop_at_breakpoint) in [
            (ExecutionMode::Debug, false),
            (ExecutionMode::Debug, true),
            (ExecutionMode::Normal, false),
        ] {
            let text = |value: &str| {
                MkAction::Text(MkTextPayload {
                    text: value.into(),
                    mode: MkTextMode::Type,
                })
            };
            let set = |value: &str| MkAction::SetVariable {
                name: "which".into(),
                value: MkValue::String(value.into()),
            };
            let call = |id| {
                MkAction::CallMacro(MkCallMacroPayload {
                    macro_id: id,
                    ..Default::default()
                })
            };
            let root = test_macro(
                1,
                true,
                vec![
                    step(1, set("root")),
                    step(4, MkAction::KeyDown(MkKey::Control)),
                    step(2, call(2)),
                    step(3, text("after:${which}")),
                ],
            );
            let child = test_macro(
                2,
                true,
                vec![
                    step(1, set("child")),
                    step(2, call(3)),
                    step(3, text("after:${which}")),
                ],
            );
            let mut leaf_step = step(1, text("${which}:${macro.name}"));
            leaf_step.breakpoint = true;
            let mut leaf = test_macro(3, true, vec![leaf_step]);
            leaf.signature.parameters.push(MkMacroParameter {
                id: MkSignatureId(1),
                name: "which".into(),
                value_type: MkValueType::String,
                description: String::new(),
                default_value: Some(MkValue::String("leaf".into())),
            });
            let (_dir, runtime, _guard, fake) = runtime_with_effects(vec![root, child, leaf]);
            let mut invocation = MkInvocation::new(1);
            invocation.mode = mode;
            assert_eq!(
                runtime.command(RuntimeCommand::Invoke(invocation)),
                CommandResult::Accepted
            );
            if mode == ExecutionMode::Debug {
                let paused = wait_for_state(&runtime, RuntimeState::Paused);
                assert_eq!(paused.macro_id, Some(1));
                assert_eq!(paused.step_id, Some(2));
                assert_eq!(paused.active_macro_id(), Some(3));
                assert_eq!(paused.active_step_id(), Some(1));
                assert_eq!(
                    paused
                        .call_stack
                        .iter()
                        .map(|frame| (
                            frame.context.macro_id,
                            frame.context.frame_id,
                            frame.context.depth,
                            frame.context.caller_step_id
                        ))
                        .collect::<Vec<_>>(),
                    [(1, 1, 1, None), (2, 2, 2, Some(2)), (3, 3, 3, Some(2))]
                );
                assert_eq!(
                    paused
                        .call_stack
                        .iter()
                        .map(|frame| frame.macro_name.as_ref())
                        .collect::<Vec<_>>(),
                    ["macro 1", "macro 2", "macro 3"]
                );
                assert_eq!(paused.call_stack[0].active_step_id, Some(2));
                assert_eq!(
                    paused.macro_steps[&MacroStepKey::new(1, 1)],
                    StepState::Success
                );
                assert_eq!(
                    paused.macro_steps[&MacroStepKey::new(3, 1)],
                    StepState::Pending
                );
                let debug = paused.debug_snapshot.as_ref().unwrap();
                assert_eq!(debug.frame.frame_id, 3);
                assert_eq!(debug.frame.macro_id, 3);
                assert_eq!(debug.variables["which"], MkValue::String("leaf".into()));
                assert_eq!(
                    debug.variables["macro.name"],
                    MkValue::String("macro 3".into())
                );
                assert_eq!(
                    paused.breakpoint_occurrence().unwrap().step,
                    MacroStepKey::new(3, 1)
                );
                assert_eq!(fake.events(), ["key_down:Control"]);
                assert_eq!(
                    runtime.command(if stop_at_breakpoint {
                        RuntimeCommand::Stop
                    } else {
                        RuntimeCommand::Resume
                    }),
                    CommandResult::Accepted
                );
            }
            let done = wait_for_terminal(&runtime);
            assert_eq!(
                done.state,
                if stop_at_breakpoint {
                    RuntimeState::Stopped
                } else {
                    RuntimeState::Completed
                }
            );
            assert!(done.call_stack.is_empty());
            assert_eq!(done.active_frame(), None);
            if stop_at_breakpoint {
                assert_eq!(fake.events(), ["key_down:Control", "key_up:Control"]);
            } else {
                assert_eq!(
                    fake.events(),
                    [
                        "key_down:Control",
                        "text:leaf:macro 3",
                        "text:after:child",
                        "text:after:root",
                        "key_up:Control"
                    ]
                );
            }
            if mode == ExecutionMode::Normal {
                assert!(done.debug_snapshot.is_none());
                assert!(done.debug_variables.is_empty());
            } else {
                assert_eq!(done.debug_snapshot.as_ref().unwrap().frame.macro_id, 1);
            }
        }
    }

    #[test]
    fn frame_debugger_snapshot_publication_is_atomic_and_monotonic() {
        let (_dir, runtime, _guard) = runtime_with(vec![]);
        let before = runtime.snapshot();
        std::thread::scope(|scope| {
            for _ in 0..6 {
                let shared = &runtime.shared;
                scope.spawn(move || {
                    for _ in 0..100 {
                        publish(shared, |snapshot| {
                            snapshot.completed_steps += 1;
                            snapshot.total_steps += 1;
                        });
                    }
                });
            }
        });
        let after = runtime.snapshot();
        assert_eq!(after.revision, before.revision + 600);
        assert_eq!((after.completed_steps, after.total_steps), (600, 600));
        assert_eq!(
            (before.completed_steps, before.total_steps, before.revision),
            (0, 0, 0)
        );
    }

    #[test]
    fn frame_debugger_stop_precedes_pause_resume_while_action_is_pending() {
        let gate = Arc::new(ControllablePromptBackend::default());
        let mut backends = Arc::new(FakeBackend::default()).backends();
        backends.prompt = gate.clone();
        let target = test_macro(
            1,
            true,
            vec![step(
                1,
                MkAction::PromptInput(MkPromptInputPayload {
                    variable: "answer".into(),
                    ..Default::default()
                }),
            )],
        );
        let (_dir, runtime, _guard) = runtime_with_backends(vec![target], backends);
        assert_eq!(
            runtime.command(RuntimeCommand::Run(1)),
            CommandResult::Accepted
        );
        gate.wait_until_entered();
        assert_eq!(
            runtime.command(RuntimeCommand::Stop),
            CommandResult::Accepted
        );
        assert_eq!(runtime.snapshot().state, RuntimeState::Stopping);
        for command in [RuntimeCommand::Pause, RuntimeCommand::Resume] {
            assert!(matches!(
                runtime.command(command),
                CommandResult::Rejected(_)
            ));
            assert_eq!(runtime.snapshot().state, RuntimeState::Stopping);
        }
        assert!(runtime.shared.control.is_stopped());
        gate.release();
        assert_eq!(wait_for_terminal(&runtime).state, RuntimeState::Stopped);
    }

    #[test]
    fn frame_debugger_restores_caller_and_preserves_previous_invocation_outcome() {
        use super::super::{FrameVariableBoundary, MkCallMacroPayload};
        let root = test_macro(
            1,
            true,
            vec![step(
                1,
                MkAction::CallMacro(MkCallMacroPayload {
                    macro_id: 2,
                    ..Default::default()
                }),
            )],
        );
        let child = test_macro(
            2,
            true,
            vec![step(
                1,
                MkAction::Text(MkTextPayload {
                    text: "child".into(),
                    mode: MkTextMode::Type,
                }),
            )],
        );
        let program = compile_program(
            &MkMacroDocument {
                macros: vec![root, child],
                ..Default::default()
            },
            1,
        )
        .unwrap();
        let root_context = ExecutionFrameContext::root(1);
        let child_context = ExecutionFrameContext {
            frame_id: 2,
            macro_id: 2,
            caller_step_id: Some(1),
            depth: 2,
        };
        let metadata = |context: ExecutionFrameContext| ExecutionFrameSnapshot {
            context,
            macro_name: Arc::from(program.name(context.macro_id).unwrap()),
            active_step_id: None,
        };
        let mut snapshot = RuntimeSnapshot {
            macro_id: Some(1),
            run_id: 9,
            run_mode: RuntimeRunMode::Debug,
            ..Default::default()
        };
        let apply = |snapshot: &mut RuntimeSnapshot, context, event| {
            apply_execution_event(snapshot, &program, context, event, RuntimeState::Running)
        };
        apply(
            &mut snapshot,
            root_context,
            ExecutionEvent::FrameEntered(metadata(root_context)),
        );
        apply(&mut snapshot, root_context, ExecutionEvent::StepStarted(1));
        apply(
            &mut snapshot,
            child_context,
            ExecutionEvent::FrameEntered(metadata(child_context)),
        );
        let failure = ExecutionDiagnostic::new(DiagnosticKind::Backend, "first invocation failed");
        apply(
            &mut snapshot,
            child_context,
            ExecutionEvent::StepFailed(1, failure.clone()),
        );
        let completed = snapshot.last_completed.clone().unwrap();
        let prior_snapshot = snapshot.clone();
        apply(
            &mut snapshot,
            child_context,
            ExecutionEvent::FrameExited {
                caller_boundary: Some(FrameVariableBoundary {
                    step_id: None,
                    variables: [("caller".into(), MkValue::String("safe parent".into()))]
                        .into_iter()
                        .collect(),
                }),
            },
        );
        assert_eq!(snapshot.active_frame(), Some(root_context));
        assert_eq!(snapshot.active_step_id(), Some(1));
        let debug = snapshot.debug_snapshot.as_ref().unwrap();
        assert_eq!(debug.frame, root_context);
        assert_eq!(debug.reason, DebugSnapshotReason::FrameRestored);
        assert_eq!(
            debug.variables["caller"],
            MkValue::String("safe parent".into())
        );
        let next_child = ExecutionFrameContext {
            frame_id: 3,
            ..child_context
        };
        apply(
            &mut snapshot,
            next_child,
            ExecutionEvent::FrameEntered(metadata(next_child)),
        );
        assert!(snapshot.debug_snapshot.is_none());
        assert!(snapshot.debug_variables.is_empty());
        assert_eq!(
            snapshot.macro_steps[&MacroStepKey::new(2, 1)],
            StepState::Pending
        );
        assert!(!snapshot.macro_failures.contains_key(&MacroDiagnosticKey {
            run_id: 9,
            step: MacroStepKey::new(2, 1)
        }));
        assert_eq!(snapshot.last_completed.as_ref().unwrap().frame.frame_id, 2);
        assert_eq!(completed.outcome, CompletedStepOutcome::Failure(failure));
        assert_eq!(prior_snapshot.active_frame(), Some(child_context));
        assert_eq!(
            prior_snapshot.macro_steps[&MacroStepKey::new(2, 1)],
            StepState::Failed
        );
        apply(&mut snapshot, next_child, ExecutionEvent::StepFinished(1));
        assert_eq!(snapshot.last_completed.as_ref().unwrap().frame.frame_id, 3);
        assert!(matches!(
            completed.outcome,
            CompletedStepOutcome::Failure(_)
        ));
    }

    #[test]
    fn frame_debugger_stale_breakpoint_publication_cannot_reverse_stop_or_resume() {
        let program = compile_program(
            &MkMacroDocument {
                macros: vec![test_macro(
                    1,
                    true,
                    vec![step(1, MkAction::Delay(MkDelayPayload::default()))],
                )],
                ..Default::default()
            },
            1,
        )
        .unwrap();
        let context = ExecutionFrameContext::root(1);
        for state in [
            RuntimeState::Stopping,
            RuntimeState::Running,
            RuntimeState::Paused,
        ] {
            let mut snapshot = RuntimeSnapshot {
                macro_id: Some(1),
                run_id: 1,
                ..Default::default()
            };
            apply_execution_event(
                &mut snapshot,
                &program,
                context,
                ExecutionEvent::FrameEntered(ExecutionFrameSnapshot {
                    context,
                    macro_name: Arc::from("root"),
                    active_step_id: None,
                }),
                state,
            );
            apply_execution_event(
                &mut snapshot,
                &program,
                context,
                ExecutionEvent::BreakpointHit {
                    step_id: 1,
                    variables: RuntimeVariables::new(),
                },
                state,
            );
            assert_eq!(snapshot.state, state);
            assert_eq!(
                snapshot.breakpoint_occurrence().is_some(),
                state == RuntimeState::Paused
            );
            if state != RuntimeState::Paused {
                assert_eq!(snapshot.pause_reason, None);
            }
        }
    }

    struct PanickingSound;
    impl super::super::executor::SoundBackend for PanickingSound {
        fn play(&self, _: &str) -> ExecResult {
            panic!("injected sound panic")
        }
    }

    #[test]
    fn backend_panic_releases_root_resources_and_worker_accepts_subsequent_run() {
        let fake = Arc::new(FakeBackend::default());
        let mut backends = fake.clone().backends();
        backends.sound = Arc::new(PanickingSound);
        let panic_macro = test_macro(
            1,
            true,
            vec![
                step(1, MkAction::KeyDown(super::super::MkKey::Control)),
                step(2, MkAction::MouseDown(MkMouseButton::Left)),
                step(
                    3,
                    MkAction::PlaySound(super::super::MkPlaySoundPayload::default()),
                ),
            ],
        );
        let harmless = test_macro(
            2,
            true,
            vec![step(
                1,
                MkAction::Text(MkTextPayload {
                    text: "after panic".into(),
                    mode: MkTextMode::Type,
                }),
            )],
        );
        let (_dir, runtime, guard) = runtime_with_backends(vec![panic_macro, harmless], backends);
        assert_eq!(
            runtime.command(RuntimeCommand::Run(1)),
            CommandResult::Accepted
        );
        let failed = wait_for_terminal(&runtime);
        assert_eq!(failed.state, RuntimeState::Failed);
        assert!(failed.call_stack.is_empty());
        assert_eq!(failed.active_frame(), None);
        assert_eq!(
            failed.latest_failure.as_ref().unwrap().kind,
            DiagnosticKind::Panic
        );
        assert!(
            failed
                .latest_failure
                .as_ref()
                .unwrap()
                .message
                .contains("injected sound panic")
        );
        assert_eq!(
            fake.events(),
            [
                "key_down:Control",
                "button_down:Left",
                "button_up:Left",
                "key_up:Control"
            ]
        );
        // command() shares terminal/admission serialization, so observing terminal
        // state never leaves a window where Pause can replace it or Run is busy.
        assert!(matches!(
            runtime.command(RuntimeCommand::Pause),
            CommandResult::Rejected(_)
        ));
        assert_eq!(runtime.snapshot().state, RuntimeState::Failed);
        assert!(!runtime.shared.control.is_active());
        assert!(guard.claim(Operation::Playback));
        guard.release(Operation::Playback);
        assert_eq!(
            runtime.command(RuntimeCommand::Run(2)),
            CommandResult::Accepted
        );
        assert_eq!(
            wait_for_terminal_after(&runtime, failed.run_id).state,
            RuntimeState::Completed
        );
        assert_eq!(fake.events().last().unwrap(), "text:after panic");
    }

    #[test]
    fn early_admission_failure_clears_activity_and_records_current_run_identity() {
        let (_dir, runtime, guard) = runtime_with(vec![test_macro(1, true, vec![])]);
        assert_eq!(
            runtime.command(RuntimeCommand::Run(999)),
            CommandResult::Accepted
        );
        let failed = wait_for_terminal(&runtime);
        assert_eq!(failed.state, RuntimeState::Failed);
        assert_eq!(failed.macro_id, Some(999));
        assert!(failed.run_id > 0);
        assert!(matches!(
            runtime.command(RuntimeCommand::Resume),
            CommandResult::Rejected(_)
        ));
        assert!(matches!(
            runtime.command(RuntimeCommand::Pause),
            CommandResult::Rejected(_)
        ));
        assert!(!runtime.shared.control.is_active());
        assert!(guard.claim(Operation::Playback));
        guard.release(Operation::Playback);
        assert_eq!(
            runtime.command(RuntimeCommand::Run(1)),
            CommandResult::Accepted
        );
        assert_eq!(
            wait_for_terminal_after(&runtime, failed.run_id).state,
            RuntimeState::Completed
        );
    }

    #[test]
    fn typed_root_invocation_applies_arguments_defaults_and_subset_only_to_root() {
        use super::super::{MkCallMacroPayload, MkMacroParameter, MkSignatureId, MkValueType};
        let mut root = test_macro(
            1,
            true,
            vec![
                step(
                    1,
                    MkAction::Text(MkTextPayload {
                        text: "skip root row".into(),
                        mode: MkTextMode::Type,
                    }),
                ),
                step(
                    2,
                    MkAction::CallMacro(MkCallMacroPayload {
                        macro_id: 2,
                        ..Default::default()
                    }),
                ),
                step(
                    3,
                    MkAction::Text(MkTextPayload {
                        text: "${parameter}:${defaulted}".into(),
                        mode: MkTextMode::Type,
                    }),
                ),
            ],
        );
        root.signature.parameters = vec![
            MkMacroParameter {
                id: MkSignatureId(1),
                name: "parameter".into(),
                value_type: MkValueType::String,
                description: String::new(),
                default_value: None,
            },
            MkMacroParameter {
                id: MkSignatureId(2),
                name: "defaulted".into(),
                value_type: MkValueType::String,
                description: String::new(),
                default_value: Some(MkValue::String("literal ${missing}".into())),
            },
        ];
        let child = test_macro(
            2,
            true,
            vec![step(
                1,
                MkAction::Text(MkTextPayload {
                    text: "full child".into(),
                    mode: MkTextMode::Type,
                }),
            )],
        );
        let (_dir, runtime, _guard, fake) = runtime_with_effects(vec![root, child]);
        let invocation = MkInvocation {
            macro_id: 1,
            arguments: [(MkSignatureId(1), MkValue::String("supplied".into()))]
                .into_iter()
                .collect(),
            mode: ExecutionMode::Normal,
            subset: MkInvocationSubset::From(2),
        };
        assert_eq!(
            runtime.command(RuntimeCommand::Invoke(invocation)),
            CommandResult::Accepted
        );
        let done = wait_for_terminal(&runtime);
        assert_eq!(done.state, RuntimeState::Completed);
        assert_eq!(
            fake.events(),
            ["text:full child", "text:supplied:literal ${missing}"]
        );
        assert_eq!(done.completed_steps, 2);
        assert!(!done.steps.contains_key(&1));
        assert_eq!(
            runtime.command(RuntimeCommand::DebugRunSelection(1, vec![3])),
            CommandResult::Accepted
        );
        let missing = wait_for_terminal_after(&runtime, done.run_id);
        assert_eq!(missing.state, RuntimeState::Failed);
        assert!(
            missing
                .latest_failure
                .as_ref()
                .unwrap()
                .message
                .contains("Required macro parameters")
        );
        assert_eq!(fake.events().len(), 2);
    }

    #[derive(Default)]
    struct PromptGateState {
        entered: bool,
        released: bool,
    }

    /// A worker-facing fake prompt that makes an action stay inside the
    /// executor until the test explicitly releases it.
    #[derive(Default)]
    struct ControllablePromptBackend {
        state: Mutex<PromptGateState>,
        wake: Condvar,
    }

    impl ControllablePromptBackend {
        fn wait_until_entered(&self) {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if self.state.lock().unwrap().entered {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "controllable prompt action was not entered"
                );
                thread::sleep(Duration::from_millis(2));
            }
        }

        fn release(&self) {
            self.state.lock().unwrap().released = true;
            self.wake.notify_all();
        }
    }

    impl PromptBackend for ControllablePromptBackend {
        fn prompt(&self, _: PromptRequest, _: &RunControl) -> ExecResult<PromptResponse> {
            let mut state = self.state.lock().unwrap();
            state.entered = true;
            self.wake.notify_all();
            while !state.released {
                state = self.wake.wait(state).unwrap();
            }
            Ok(PromptResponse::Submitted("during-action".into()))
        }
    }

    #[test]
    fn active_run_uses_immutable_callee_plan_after_document_publication_changes() {
        let gate = Arc::new(ControllablePromptBackend::default());
        let fake = Arc::new(FakeBackend::default());
        let mut backends = fake.clone().backends();
        backends.prompt = gate.clone();
        let root = test_macro(
            1,
            true,
            vec![
                step(
                    1,
                    MkAction::PromptInput(MkPromptInputPayload {
                        title: "compile boundary".into(),
                        prompt: "wait".into(),
                        default_value: String::new(),
                        variable: "answer".into(),
                        copy_to_clipboard: false,
                    }),
                ),
                step(
                    2,
                    MkAction::CallMacro(super::super::MkCallMacroPayload {
                        macro_id: 2,
                        ..Default::default()
                    }),
                ),
            ],
        );
        let child = test_macro(
            2,
            true,
            vec![step(
                1,
                MkAction::Text(MkTextPayload {
                    text: "captured child".into(),
                    mode: MkTextMode::Type,
                }),
            )],
        );
        let (_directory, runtime, _guard) = runtime_with_backends(vec![root, child], backends);

        assert_eq!(
            runtime.command(RuntimeCommand::Run(1)),
            CommandResult::Accepted
        );
        gate.wait_until_entered();
        let mut changed = (*runtime.store.snapshot()).clone();
        let MkAction::Text(text) = &mut changed.macros[1].steps[0].action else {
            unreachable!()
        };
        text.text = "newly published child".into();
        runtime.store.save(changed).unwrap();
        gate.release();

        assert_eq!(wait_for_terminal(&runtime).state, RuntimeState::Completed);
        assert_eq!(fake.events(), ["text:captured child"]);
        let published = runtime.store.snapshot();
        let MkAction::Text(text) = &published.macros[1].steps[0].action else {
            unreachable!()
        };
        assert_eq!(text.text, "newly published child");
    }

    #[test]
    fn all_six_runtime_subsets_keep_complete_callee_dependencies() {
        use super::super::MkCallMacroPayload;
        for mode in [ExecutionMode::Normal, ExecutionMode::Debug] {
            for subset in [
                MkInvocationSubset::Whole,
                MkInvocationSubset::From(2),
                MkInvocationSubset::Selected(vec![2]),
            ] {
                let text = |value: &str| {
                    MkAction::Text(MkTextPayload {
                        text: value.into(),
                        mode: MkTextMode::Type,
                    })
                };
                let root = test_macro(
                    1,
                    true,
                    vec![
                        step(1, text("root before")),
                        step(
                            2,
                            MkAction::CallMacro(MkCallMacroPayload {
                                macro_id: 2,
                                ..Default::default()
                            }),
                        ),
                        step(3, text("root after")),
                    ],
                );
                let child = test_macro(
                    2,
                    true,
                    vec![step(1, text("child first")), step(2, text("child second"))],
                );
                let (_directory, runtime, _guard, fake) = runtime_with_effects(vec![root, child]);
                let invocation = MkInvocation {
                    macro_id: 1,
                    arguments: MkInvocationValues::new(),
                    mode,
                    subset: subset.clone(),
                };

                assert_eq!(
                    runtime.command(RuntimeCommand::Invoke(invocation)),
                    CommandResult::Accepted
                );
                assert_eq!(wait_for_terminal(&runtime).state, RuntimeState::Completed);
                let expected = match subset {
                    MkInvocationSubset::Whole => vec![
                        "text:root before",
                        "text:child first",
                        "text:child second",
                        "text:root after",
                    ],
                    MkInvocationSubset::From(_) => {
                        vec!["text:child first", "text:child second", "text:root after"]
                    }
                    MkInvocationSubset::Selected(_) => {
                        vec!["text:child first", "text:child second"]
                    }
                };
                assert_eq!(fake.events(), expected);
            }
        }
    }

    fn failure_for(command: RuntimeCommand, macros: Vec<MkMacro>) -> ExecutionDiagnostic {
        let (_dir, runtime, _guard) = runtime_with(macros);
        assert_eq!(runtime.command(command), CommandResult::Accepted);
        wait_for_terminal(&runtime)
            .latest_failure
            .clone()
            .expect("failed run publishes a diagnostic")
    }

    fn wait_for_admission_release(runtime: &MacroRuntime) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while runtime.shared.admission.lock().unwrap().is_some() {
            assert!(Instant::now() < deadline, "admission lock was not released");
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn command_pairs(macro_id: u64) -> Vec<(RuntimeCommand, RuntimeCommand)> {
        vec![
            (
                RuntimeCommand::Run(macro_id),
                RuntimeCommand::DebugRun(macro_id),
            ),
            (
                RuntimeCommand::RunFrom(macro_id, 1),
                RuntimeCommand::DebugRunFrom(macro_id, 1),
            ),
            (
                RuntimeCommand::RunSelection(macro_id, vec![1]),
                RuntimeCommand::DebugRunSelection(macro_id, vec![1]),
            ),
        ]
    }

    #[test]
    fn normal_and_debug_runs_have_equal_target_and_enabled_validation() {
        let normal_missing = failure_for(RuntimeCommand::Run(404), vec![]);
        let debug_missing = failure_for(RuntimeCommand::DebugRun(404), vec![]);
        assert_eq!(normal_missing, debug_missing);
        assert_eq!(normal_missing.kind, DiagnosticKind::TargetNotFound);

        let disabled = test_macro(1, false, vec![]);
        let normal_disabled = failure_for(RuntimeCommand::Run(1), vec![disabled.clone()]);
        let debug_disabled = failure_for(RuntimeCommand::DebugRun(1), vec![disabled]);
        assert_eq!(normal_disabled, debug_disabled);
        assert_eq!(normal_disabled.kind, DiagnosticKind::InvalidTarget);
    }

    #[test]
    fn normal_and_debug_selections_have_equal_empty_and_invalid_validation() {
        let (_dir, runtime, _guard) = runtime_with(vec![]);
        let normal_empty = runtime.command(RuntimeCommand::RunSelection(1, vec![]));
        let debug_empty = runtime.command(RuntimeCommand::DebugRunSelection(1, vec![]));
        assert_eq!(normal_empty, debug_empty);
        assert!(matches!(
            normal_empty,
            CommandResult::Rejected(ExecutionDiagnostic {
                kind: DiagnosticKind::InvalidSelection,
                ..
            })
        ));

        let target = test_macro(1, true, vec![step(1, MkAction::Delay(Default::default()))]);
        let normal_invalid = failure_for(
            RuntimeCommand::RunSelection(1, vec![999]),
            vec![target.clone()],
        );
        let debug_invalid = failure_for(
            RuntimeCommand::DebugRunSelection(1, vec![999]),
            vec![target],
        );
        assert_eq!(normal_invalid, debug_invalid);
        assert_eq!(normal_invalid.kind, DiagnosticKind::TargetNotFound);
    }

    #[test]
    fn normal_and_debug_run_from_reject_structured_plans_equally() {
        let structured_plans = vec![
            vec![
                step(1, MkAction::If(MkCondition::All { conditions: vec![] })),
                step(2, MkAction::Delay(Default::default())),
                step(3, MkAction::EndIf),
            ],
            vec![
                step(1, MkAction::If(MkCondition::All { conditions: vec![] })),
                step(2, MkAction::Delay(Default::default())),
                step(3, MkAction::Else),
                step(4, MkAction::Delay(Default::default())),
                step(5, MkAction::EndIf),
            ],
            vec![
                step(1, MkAction::RepeatStart { count: 1 }),
                step(2, MkAction::Delay(Default::default())),
                step(3, MkAction::RepeatEnd),
            ],
            vec![
                step(
                    1,
                    MkAction::WhileStart {
                        condition: MkCondition::All { conditions: vec![] },
                    },
                ),
                step(2, MkAction::Delay(Default::default())),
                step(3, MkAction::WhileEnd),
            ],
            vec![
                step(1, MkAction::RepeatStart { count: 1 }),
                step(2, MkAction::Break),
                step(3, MkAction::RepeatEnd),
            ],
            vec![
                step(1, MkAction::RepeatStart { count: 1 }),
                step(2, MkAction::Continue),
                step(3, MkAction::RepeatEnd),
            ],
        ];

        for steps in structured_plans {
            let structured = test_macro(1, true, steps);
            assert!(super::super::compile(&structured).is_ok());
            let normal = failure_for(RuntimeCommand::RunFrom(1, 2), vec![structured.clone()]);
            let debug = failure_for(RuntimeCommand::DebugRunFrom(1, 2), vec![structured]);
            assert_eq!(normal, debug);
            assert_eq!(normal.kind, DiagnosticKind::InvalidSelection);
            assert_eq!(
                normal.message,
                "run-from cannot enter a structured control-flow plan"
            );
        }
    }

    #[test]
    fn debug_selected_uses_the_normal_filtered_plan_and_only_selected_breakpoints() {
        let mut selected_two = step(
            2,
            MkAction::Text(MkTextPayload {
                text: "selected two".into(),
                mode: MkTextMode::Type,
            }),
        );
        selected_two.breakpoint = true;
        let mut excluded_three = step(
            3,
            MkAction::Text(MkTextPayload {
                text: "excluded three".into(),
                mode: MkTextMode::Type,
            }),
        );
        excluded_three.breakpoint = true;
        let mut selected_four = step(
            4,
            MkAction::Text(MkTextPayload {
                text: "selected four".into(),
                mode: MkTextMode::Type,
            }),
        );
        selected_four.breakpoint = true;
        let target = test_macro(
            1,
            true,
            vec![
                step(
                    1,
                    MkAction::Text(MkTextPayload {
                        text: "excluded one".into(),
                        mode: MkTextMode::Type,
                    }),
                ),
                selected_two,
                excluded_three,
                selected_four,
            ],
        );
        let selected = vec![4, 2];
        let lifecycle = |events: &[ExecutionEvent]| {
            events
                .iter()
                .filter_map(|event| match event {
                    ExecutionEvent::StepStarted(id) => Some(("started", *id)),
                    ExecutionEvent::StepFinished(id) => Some(("finished", *id)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };

        let (_normal_dir, normal, _normal_guard, normal_fake) =
            runtime_with_effects(vec![target.clone()]);
        assert_eq!(
            normal.command(RuntimeCommand::RunSelection(1, selected.clone())),
            CommandResult::Accepted
        );
        let normal_completed = wait_for_terminal(&normal);
        let normal_events = normal.take_test_events();
        assert_eq!(normal_completed.run_mode, RuntimeRunMode::Normal);
        assert_eq!(
            normal_completed.steps.keys().copied().collect::<Vec<_>>(),
            [2, 4]
        );
        assert_eq!(normal_completed.total_steps, 2);
        assert_eq!(normal_completed.completed_steps, 2);
        assert_eq!(
            normal_fake.events(),
            ["text:selected two", "text:selected four"]
        );
        assert!(!normal_events.iter().any(|event| {
            matches!(
                event,
                ExecutionEvent::BreakpointHit { .. } | ExecutionEvent::Paused
            )
        }));

        let (_debug_dir, debug, _debug_guard, debug_fake) = runtime_with_effects(vec![target]);
        assert_eq!(
            debug.command(RuntimeCommand::DebugRunSelection(1, selected)),
            CommandResult::Accepted
        );
        let first_pause = wait_for_state(&debug, RuntimeState::Paused);
        assert_eq!(first_pause.run_mode, RuntimeRunMode::Debug);
        assert_eq!(
            first_pause.pause_reason,
            Some(RuntimePauseReason::Breakpoint {
                step_id: 2,
                frame: ExecutionFrameContext::root(1)
            })
        );
        assert_eq!(first_pause.step_id, Some(2));
        assert_eq!(
            first_pause.steps.keys().copied().collect::<Vec<_>>(),
            [2, 4]
        );
        assert_eq!(first_pause.steps[&2], StepState::Pending);
        assert_eq!(first_pause.steps[&4], StepState::Pending);
        assert_eq!(first_pause.total_steps, 2);
        assert_eq!(first_pause.completed_steps, 0);
        assert!(debug_fake.events().is_empty());

        assert_eq!(
            debug.command(RuntimeCommand::Resume),
            CommandResult::Accepted
        );
        let second_pause = wait_for_state(&debug, RuntimeState::Paused);
        assert_eq!(
            second_pause.pause_reason,
            Some(RuntimePauseReason::Breakpoint {
                step_id: 4,
                frame: ExecutionFrameContext::root(1)
            })
        );
        assert_eq!(second_pause.step_id, Some(4));
        assert_eq!(second_pause.steps[&2], StepState::Success);
        assert_eq!(second_pause.steps[&4], StepState::Pending);
        assert_eq!(debug_fake.events(), ["text:selected two"]);

        assert_eq!(
            debug.command(RuntimeCommand::Resume),
            CommandResult::Accepted
        );
        let debug_completed = wait_for_terminal(&debug);
        let debug_events = debug.take_test_events();
        assert_eq!(debug_completed.state, RuntimeState::Completed);
        assert_eq!(debug_completed.run_mode, RuntimeRunMode::Debug);
        assert_eq!(debug_completed.steps, normal_completed.steps);
        assert_eq!(debug_completed.total_steps, normal_completed.total_steps);
        assert_eq!(
            debug_completed.completed_steps,
            normal_completed.completed_steps
        );
        assert_eq!(
            debug_fake.events(),
            ["text:selected two", "text:selected four"]
        );
        assert_eq!(lifecycle(&debug_events), lifecycle(&normal_events));
        assert_eq!(
            debug_events
                .iter()
                .filter_map(|event| match event {
                    ExecutionEvent::BreakpointHit { step_id, .. } => Some(*step_id),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [2, 4]
        );
        assert!(
            !debug_events
                .iter()
                .any(|event| { matches!(event, ExecutionEvent::BreakpointHit { step_id: 3, .. }) })
        );
        assert!(!debug_completed.steps.contains_key(&1));
        assert!(!debug_completed.steps.contains_key(&3));
        assert!(
            !debug_fake
                .events()
                .iter()
                .any(|event| { event == "text:excluded one" || event == "text:excluded three" })
        );
    }

    #[test]
    fn all_normal_and_debug_pairs_share_recording_and_playback_admission() {
        let target = test_macro(
            1,
            true,
            vec![step(
                1,
                MkAction::Delay(MkDelayPayload {
                    fixed_ms: 60_000,
                    ..Default::default()
                }),
            )],
        );

        for (normal, debug) in command_pairs(1) {
            let (_dir, runtime, guard) = runtime_with(vec![target.clone()]);
            assert!(guard.claim(Operation::Recording));
            let normal_result = runtime.command(normal);
            let debug_result = runtime.command(debug);
            assert_eq!(normal_result, debug_result);
            assert!(matches!(
                normal_result,
                CommandResult::Rejected(ExecutionDiagnostic {
                    kind: DiagnosticKind::InvalidTarget,
                    ..
                })
            ));
            guard.release(Operation::Recording);
        }

        for (normal, debug) in command_pairs(1) {
            let (_dir, normal_runtime, _guard) = runtime_with(vec![target.clone()]);
            assert_eq!(
                normal_runtime.command(RuntimeCommand::Run(1)),
                CommandResult::Accepted
            );
            wait_for_state(&normal_runtime, RuntimeState::Running);
            let normal_result = normal_runtime.command(normal);

            let (_dir, debug_runtime, _guard) = runtime_with(vec![target.clone()]);
            assert_eq!(
                debug_runtime.command(RuntimeCommand::Run(1)),
                CommandResult::Accepted
            );
            wait_for_state(&debug_runtime, RuntimeState::Running);
            let debug_result = debug_runtime.command(debug);

            assert_eq!(normal_result, debug_result);
            assert_eq!(
                normal_result,
                CommandResult::AlreadyRunning { active_macro_id: 1 }
            );
            assert_eq!(
                normal_runtime.command(RuntimeCommand::Stop),
                CommandResult::Accepted
            );
            assert_eq!(
                debug_runtime.command(RuntimeCommand::Stop),
                CommandResult::Accepted
            );
            assert_eq!(
                wait_for_terminal(&normal_runtime).state,
                RuntimeState::Stopped
            );
            assert_eq!(
                wait_for_terminal(&debug_runtime).state,
                RuntimeState::Stopped
            );
        }
    }

    #[test]
    fn debug_controls_match_normal_playback_controls() {
        let target = test_macro(
            1,
            true,
            vec![step(
                1,
                MkAction::Delay(MkDelayPayload {
                    fixed_ms: 60_000,
                    ..Default::default()
                }),
            )],
        );
        let (_dir, runtime, _guard) = runtime_with(vec![target]);
        assert_eq!(
            runtime.command(RuntimeCommand::DebugRun(1)),
            CommandResult::Accepted
        );
        wait_for_state(&runtime, RuntimeState::Running);
        assert_eq!(
            runtime.command(RuntimeCommand::Pause),
            CommandResult::Accepted
        );
        assert_eq!(
            runtime.command(RuntimeCommand::Resume),
            CommandResult::Accepted
        );
        assert_eq!(
            runtime.command(RuntimeCommand::Stop),
            CommandResult::Accepted
        );
        assert_eq!(wait_for_terminal(&runtime).state, RuntimeState::Stopped);
    }

    #[test]
    fn manual_pause_and_resume_publish_reason_in_the_state_transition() {
        let target = test_macro(
            1,
            true,
            vec![step(
                1,
                MkAction::Delay(MkDelayPayload {
                    fixed_ms: 60_000,
                    ..Default::default()
                }),
            )],
        );
        let (_dir, runtime, _guard) = runtime_with(vec![target]);

        assert_eq!(
            runtime.command(RuntimeCommand::Run(1)),
            CommandResult::Accepted
        );
        wait_for_state(&runtime, RuntimeState::Running);
        assert_eq!(
            runtime.command(RuntimeCommand::Pause),
            CommandResult::Accepted
        );
        let paused = runtime.snapshot();
        assert_eq!(paused.state, RuntimeState::Paused);
        assert_eq!(paused.pause_reason, Some(RuntimePauseReason::User));

        assert_eq!(
            runtime.command(RuntimeCommand::Resume),
            CommandResult::Accepted
        );
        let resumed = runtime.snapshot();
        assert_eq!(resumed.state, RuntimeState::Running);
        assert_eq!(resumed.pause_reason, None);

        assert_eq!(
            runtime.command(RuntimeCommand::Stop),
            CommandResult::Accepted
        );
        assert_eq!(wait_for_terminal(&runtime).pause_reason, None);
    }

    #[test]
    fn breakpoint_resume_clears_reason_in_the_running_transition() {
        let mut breakpoint = step(
            7,
            MkAction::Delay(MkDelayPayload {
                fixed_ms: 60_000,
                ..Default::default()
            }),
        );
        breakpoint.breakpoint = true;
        let target = test_macro(1, true, vec![breakpoint]);
        let (_dir, runtime, _guard) = runtime_with(vec![target]);

        assert_eq!(
            runtime.command(RuntimeCommand::DebugRun(1)),
            CommandResult::Accepted
        );
        let paused = wait_for_state(&runtime, RuntimeState::Paused);
        assert_eq!(
            paused.pause_reason,
            Some(RuntimePauseReason::Breakpoint {
                step_id: 7,
                frame: ExecutionFrameContext::root(1)
            })
        );

        assert_eq!(
            runtime.command(RuntimeCommand::Resume),
            CommandResult::Accepted
        );
        let resumed = runtime.snapshot();
        assert_eq!(resumed.state, RuntimeState::Running);
        assert_eq!(resumed.pause_reason, None);

        assert_eq!(
            runtime.command(RuntimeCommand::Stop),
            CommandResult::Accepted
        );
        assert_eq!(wait_for_terminal(&runtime).pause_reason, None);
    }

    #[test]
    fn normal_execution_completely_ignores_persisted_breakpoint_metadata() {
        let mut set_variable = step(
            1,
            MkAction::SetVariable {
                name: "ready".into(),
                value: MkValue::Boolean(true),
            },
        );
        set_variable.breakpoint = true;
        let mut mouse_move = step(
            2,
            MkAction::MouseMove(MkMouseMovePayload {
                target: MkCoordinateTarget::Screen {
                    point: MkPoint { x: 37, y: 91 },
                },
                duration_ms: 0,
            }),
        );
        mouse_move.breakpoint = true;
        let (_dir, runtime, _guard, fake) =
            runtime_with_effects(vec![test_macro(1, true, vec![set_variable, mouse_move])]);

        assert_eq!(
            runtime.command(RuntimeCommand::Run(1)),
            CommandResult::Accepted
        );
        let completed = wait_for_terminal(&runtime);
        let events = runtime.take_test_events();

        assert_eq!(completed.state, RuntimeState::Completed);
        assert_ne!(completed.state, RuntimeState::Paused);
        assert_eq!(completed.pause_reason, None);
        assert_eq!(completed.steps[&1], StepState::Success);
        assert_eq!(completed.steps[&2], StepState::Success);
        assert_eq!(completed.completed_steps, 2);
        assert_eq!(completed.total_steps, 2);
        assert_eq!(fake.events(), ["move:37,91"]);
        assert!(events.iter().all(|event| {
            !matches!(
                event,
                ExecutionEvent::BreakpointHit { .. } | ExecutionEvent::Paused
            )
        }));
        assert!(matches!(
            events.as_slice(),
            [
                ExecutionEvent::StepStarted(1),
                ExecutionEvent::StepFinished(1),
                ExecutionEvent::StepStarted(2),
                ExecutionEvent::StepFinished(2),
            ]
        ));
    }

    #[test]
    fn breakpoint_snapshot_resume_executes_once_and_worker_accepts_next_run() {
        let mut breakpoint = step(
            1,
            MkAction::MouseClick(MkMousePayload {
                target: MkCoordinateTarget::Screen {
                    point: MkPoint { x: 10, y: 20 },
                },
                button: MkMouseButton::Left,
                clicks: 1,
            }),
        );
        breakpoint.breakpoint = true;
        let target = test_macro(1, true, vec![breakpoint]);
        let (_dir, runtime, _guard, fake) = runtime_with_effects(vec![target]);

        assert_eq!(
            runtime.command(RuntimeCommand::DebugRun(1)),
            CommandResult::Accepted
        );
        let paused = wait_for_state(&runtime, RuntimeState::Paused);
        assert_eq!(
            paused.revision, 4,
            "run initialization, FrameEntered, RunStarted, and BreakpointHit must each publish one revision"
        );
        assert_eq!(paused.step_id, Some(1));
        assert_eq!(paused.steps[&1], StepState::Pending);
        assert_eq!(
            paused.pause_reason,
            Some(RuntimePauseReason::Breakpoint {
                step_id: 1,
                frame: ExecutionFrameContext::root(1)
            })
        );
        let debug = paused.debug_snapshot.as_ref().unwrap();
        assert_eq!(debug.step_id, Some(1));
        assert_eq!(debug.reason, DebugSnapshotReason::Breakpoint);
        assert_eq!(debug.variables.get("macro.id"), Some(&MkValue::Number(1.0)));
        assert_eq!(
            debug.variables.get("last_action_success"),
            Some(&MkValue::Boolean(true))
        );
        assert_eq!(debug.variables.get("step.id"), Some(&MkValue::Number(1.0)));
        assert!(fake.events().is_empty());
        assert_eq!(
            runtime.command(RuntimeCommand::Pause),
            CommandResult::Rejected(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                "command Pause is not valid while runtime is Paused",
            ))
        );
        assert_eq!(
            runtime.command(RuntimeCommand::Pause),
            CommandResult::Rejected(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                "command Pause is not valid while runtime is Paused",
            ))
        );
        assert_eq!(
            runtime.command(RuntimeCommand::Resume),
            CommandResult::Accepted
        );
        let completed = wait_for_terminal(&runtime);
        let first_run_events = runtime.take_test_events();
        assert_eq!(completed.state, RuntimeState::Completed);
        assert_eq!(completed.pause_reason, None);
        assert_eq!(completed.steps[&1], StepState::Success);
        assert_eq!(
            first_run_events
                .iter()
                .filter(|event| matches!(event, ExecutionEvent::BreakpointHit { .. }))
                .count(),
            1
        );
        assert_eq!(
            fake.events()
                .iter()
                .filter(|event| *event == "button_down:Left")
                .count(),
            1
        );
        assert_eq!(
            fake.events()
                .iter()
                .filter(|event| *event == "button_up:Left")
                .count(),
            1
        );

        wait_for_admission_release(&runtime);
        assert_eq!(
            runtime.command(RuntimeCommand::Run(1)),
            CommandResult::Accepted
        );
        assert_eq!(
            wait_for_terminal_after(&runtime, completed.run_id).state,
            RuntimeState::Completed
        );
        assert_eq!(
            fake.events()
                .iter()
                .filter(|event| *event == "button_down:Left")
                .count(),
            2,
        );
        assert_eq!(
            fake.events()
                .iter()
                .filter(|event| *event == "button_up:Left")
                .count(),
            2,
        );
        assert_eq!(
            fake.events()
                .iter()
                .filter(|event| *event == "move:10,20")
                .count(),
            2,
            "the same runtime worker must remain usable without a debugger worker"
        );
    }

    #[test]
    fn breakpoint_snapshot_stop_cancels_before_dispatch_and_worker_is_reusable() {
        let held_button = step(1, MkAction::MouseDown(MkMouseButton::Left));
        let mut breakpoint = step(
            2,
            MkAction::MouseClick(MkMousePayload {
                target: MkCoordinateTarget::Screen {
                    point: MkPoint { x: 10, y: 20 },
                },
                button: MkMouseButton::Left,
                clicks: 1,
            }),
        );
        breakpoint.breakpoint = true;
        let target = test_macro(1, true, vec![held_button, breakpoint]);
        let harmless = test_macro(
            2,
            true,
            vec![step(
                1,
                MkAction::SetVariable {
                    name: "harmless".into(),
                    value: MkValue::Boolean(true),
                },
            )],
        );
        let (_dir, runtime, _guard, fake) = runtime_with_effects(vec![target, harmless]);

        assert_eq!(
            runtime.command(RuntimeCommand::DebugRun(1)),
            CommandResult::Accepted
        );
        let paused = wait_for_state(&runtime, RuntimeState::Paused);
        assert_eq!(
            paused.pause_reason,
            Some(RuntimePauseReason::Breakpoint {
                step_id: 2,
                frame: ExecutionFrameContext::root(1)
            })
        );
        assert_eq!(paused.step_id, Some(2));
        assert_eq!(paused.steps[&1], StepState::Success);
        assert_eq!(paused.steps[&2], StepState::Pending);
        assert_eq!(fake.events(), ["button_down:Left"]);
        assert_eq!(
            runtime.command(RuntimeCommand::Stop),
            CommandResult::Accepted
        );
        let stopping = runtime.snapshot();
        assert!(matches!(
            stopping.state,
            RuntimeState::Stopping | RuntimeState::Stopped
        ));
        assert_eq!(stopping.pause_reason, None);
        let stopped = wait_for_terminal(&runtime);
        assert_eq!(stopped.state, RuntimeState::Stopped);
        assert_eq!(stopped.pause_reason, None);
        assert_eq!(stopped.latest_failure, None);
        assert_eq!(stopped.steps[&2], StepState::Pending);
        assert_eq!(
            fake.events(),
            ["button_down:Left", "button_up:Left"],
            "stopping at the breakpoint must clean up input owned by an earlier step"
        );
        assert!(!fake.events().iter().any(|event| event.starts_with("move:")));
        let stopped_events = runtime.take_test_events();
        assert!(
            stopped_events
                .iter()
                .any(|event| matches!(event, ExecutionEvent::BreakpointHit { step_id: 2, .. }))
        );
        assert!(
            !stopped_events
                .iter()
                .any(|event| matches!(event, ExecutionEvent::StepFinished(2)))
        );
        assert!(
            !stopped_events
                .iter()
                .any(|event| matches!(event, ExecutionEvent::StepFailed(2, _)))
        );
        assert!(stopped_events.iter().any(|event| {
            matches!(
                event,
                ExecutionEvent::DebugVariables {
                    reason: DebugSnapshotReason::RunCancelled,
                    ..
                }
            )
        }));

        wait_for_admission_release(&runtime);
        assert_eq!(
            runtime.command(RuntimeCommand::Run(2)),
            CommandResult::Accepted
        );
        assert_eq!(
            wait_for_terminal_after(&runtime, stopped.run_id).state,
            RuntimeState::Completed
        );
        assert_eq!(
            fake.events(),
            ["button_down:Left", "button_up:Left"],
            "a harmless follow-up run proves the stopped worker released the breakpoint wait"
        );
    }

    #[test]
    fn queued_manual_pause_cannot_replace_a_breakpoint_reason() {
        let mut breakpoint = step(
            19,
            MkAction::Delay(MkDelayPayload {
                fixed_ms: 60_000,
                ..Default::default()
            }),
        );
        breakpoint.breakpoint = true;
        let target = test_macro(1, true, vec![breakpoint]);
        let (_dir, runtime, _guard) = runtime_with(vec![target]);

        assert_eq!(
            runtime.command(RuntimeCommand::DebugRun(1)),
            CommandResult::Accepted
        );
        wait_for_state(&runtime, RuntimeState::Paused);

        // Models a manual Paused event that was queued before BreakpointHit but
        // delivered afterward by either the command or executor event path.
        publish_manual_pause(&runtime.shared);
        let paused = runtime.snapshot();
        assert_eq!(paused.state, RuntimeState::Paused);
        assert_eq!(
            paused.pause_reason,
            Some(RuntimePauseReason::Breakpoint {
                step_id: 19,
                frame: ExecutionFrameContext::root(1)
            })
        );

        assert_eq!(
            runtime.command(RuntimeCommand::Stop),
            CommandResult::Accepted
        );
        assert_eq!(wait_for_terminal(&runtime).pause_reason, None);
    }

    #[test]
    fn manual_pause_retains_last_safe_debug_snapshot_until_next_boundary() {
        let target = test_macro(
            1,
            true,
            vec![
                step(
                    1,
                    MkAction::SetVariable {
                        name: "safe".into(),
                        value: MkValue::String("before action".into()),
                    },
                ),
                step(
                    2,
                    MkAction::PromptInput(MkPromptInputPayload {
                        title: "Controlled action".into(),
                        prompt: "wait".into(),
                        default_value: String::new(),
                        variable: "partial".into(),
                        copy_to_clipboard: false,
                    }),
                ),
                step(
                    3,
                    MkAction::Delay(MkDelayPayload {
                        fixed_ms: 60_000,
                        ..Default::default()
                    }),
                ),
            ],
        );
        let fake = Arc::new(FakeBackend::default());
        let prompt = Arc::new(ControllablePromptBackend::default());
        let mut backends = fake.backends();
        backends.prompt = prompt.clone();
        let (_dir, runtime, _guard) = runtime_with_backends(vec![target], backends);

        assert_eq!(
            runtime.command(RuntimeCommand::DebugRun(1)),
            CommandResult::Accepted
        );
        let safe_boundary =
            wait_for_debug_boundary(&runtime, Some(1), DebugSnapshotReason::StepBoundary);
        prompt.wait_until_entered();

        assert_eq!(
            runtime.command(RuntimeCommand::Pause),
            CommandResult::Accepted
        );
        let paused = runtime.snapshot();
        assert_eq!(paused.state, RuntimeState::Paused);
        assert_eq!(paused.pause_reason, Some(RuntimePauseReason::User));
        assert_eq!(paused.step_id, Some(2));
        let paused_debug = paused
            .debug_snapshot
            .as_ref()
            .expect("Debug playback retains the last safe boundary while paused");
        assert!(
            Arc::ptr_eq(paused_debug, safe_boundary.debug_snapshot.as_ref().unwrap()),
            "manual Pause must not replace the immutable debug snapshot"
        );
        assert_eq!(paused_debug.step_id, Some(1));
        assert_eq!(paused_debug.reason, DebugSnapshotReason::StepBoundary);
        assert_eq!(
            paused_debug.variables.get("safe"),
            Some(&MkValue::String("before action".into()))
        );
        assert!(
            !paused_debug.variables.contains_key("partial"),
            "variables written only after the action returns must not be exposed"
        );

        assert_eq!(
            runtime.command(RuntimeCommand::Resume),
            CommandResult::Accepted
        );
        prompt.release();
        let next_boundary =
            wait_for_debug_boundary(&runtime, Some(2), DebugSnapshotReason::StepBoundary);
        let next_debug = next_boundary.debug_snapshot.as_ref().unwrap();
        assert!(!Arc::ptr_eq(next_debug, paused_debug));
        assert_eq!(
            next_debug.variables.get("partial"),
            Some(&MkValue::String("during-action".into()))
        );
        assert_eq!(next_debug.reason, DebugSnapshotReason::StepBoundary);

        assert_eq!(
            runtime.command(RuntimeCommand::Stop),
            CommandResult::Accepted
        );
        assert_eq!(wait_for_terminal(&runtime).state, RuntimeState::Stopped);
    }

    #[test]
    fn normal_manual_pause_does_not_reuse_debug_data_from_an_earlier_run() {
        let debug_target = test_macro(
            1,
            true,
            vec![step(
                1,
                MkAction::SetVariable {
                    name: "debug_only".into(),
                    value: MkValue::String("from Debug run".into()),
                },
            )],
        );
        let normal_target = test_macro(
            2,
            true,
            vec![step(
                1,
                MkAction::PromptInput(MkPromptInputPayload {
                    title: "Controlled action".into(),
                    prompt: "wait".into(),
                    default_value: String::new(),
                    variable: "normal_value".into(),
                    copy_to_clipboard: false,
                }),
            )],
        );
        let fake = Arc::new(FakeBackend::default());
        let prompt = Arc::new(ControllablePromptBackend::default());
        let mut backends = fake.backends();
        backends.prompt = prompt.clone();
        let (_dir, runtime, _guard) =
            runtime_with_backends(vec![debug_target, normal_target], backends);

        assert_eq!(
            runtime.command(RuntimeCommand::DebugRun(1)),
            CommandResult::Accepted
        );
        let previous = wait_for_terminal(&runtime);
        let previous_debug = previous
            .debug_snapshot
            .as_ref()
            .expect("Debug run should publish a terminal snapshot");
        assert_eq!(previous_debug.reason, DebugSnapshotReason::RunFinished);
        assert_eq!(
            previous_debug.variables.get("debug_only"),
            Some(&MkValue::String("from Debug run".into()))
        );
        wait_for_admission_release(&runtime);

        assert_eq!(
            runtime.command(RuntimeCommand::Run(2)),
            CommandResult::Accepted
        );
        prompt.wait_until_entered();
        assert_eq!(
            runtime.command(RuntimeCommand::Pause),
            CommandResult::Accepted
        );
        let paused = runtime.snapshot();
        assert_eq!(paused.run_mode, RuntimeRunMode::Normal);
        assert_eq!(paused.pause_reason, Some(RuntimePauseReason::User));
        assert!(
            paused.debug_snapshot.is_none(),
            "Normal playback has no debug map or snapshot reason, even after a prior Debug run"
        );

        assert_eq!(
            runtime.command(RuntimeCommand::Resume),
            CommandResult::Accepted
        );
        prompt.release();
        assert_eq!(wait_for_terminal(&runtime).state, RuntimeState::Completed);
    }

    #[test]
    fn breakpoint_terminal_failure_and_completion_clear_reason() {
        for fail in [false, true] {
            let mut breakpoint = step(
                1,
                MkAction::Text(MkTextPayload {
                    text: "terminal transition".into(),
                    mode: MkTextMode::Type,
                }),
            );
            breakpoint.breakpoint = true;
            let target = test_macro(1, true, vec![breakpoint]);
            let (_dir, runtime, _guard, fake) = runtime_with_effects(vec![target]);
            if fail {
                fake.fail(
                    "text:terminal transition",
                    ExecutionDiagnostic::new(DiagnosticKind::Backend, "injected failure"),
                );
            }

            assert_eq!(
                runtime.command(RuntimeCommand::DebugRun(1)),
                CommandResult::Accepted
            );
            let paused = wait_for_state(&runtime, RuntimeState::Paused);
            assert_eq!(
                paused.pause_reason,
                Some(RuntimePauseReason::Breakpoint {
                    step_id: 1,
                    frame: ExecutionFrameContext::root(1)
                })
            );
            assert_eq!(
                runtime.command(RuntimeCommand::Resume),
                CommandResult::Accepted
            );
            let terminal = wait_for_terminal(&runtime);
            assert_eq!(
                terminal.state,
                if fail {
                    RuntimeState::Failed
                } else {
                    RuntimeState::Completed
                }
            );
            assert_eq!(terminal.pause_reason, None);
        }
    }

    #[test]
    fn run_after_breakpoint_starts_without_a_stale_reason() {
        let mut breakpoint = step(
            23,
            MkAction::Delay(MkDelayPayload {
                fixed_ms: 60_000,
                ..Default::default()
            }),
        );
        breakpoint.breakpoint = true;
        let target = test_macro(1, true, vec![breakpoint]);
        let (_dir, runtime, _guard) = runtime_with(vec![target]);

        assert_eq!(
            runtime.command(RuntimeCommand::DebugRun(1)),
            CommandResult::Accepted
        );
        wait_for_state(&runtime, RuntimeState::Paused);
        assert_eq!(
            runtime.command(RuntimeCommand::Stop),
            CommandResult::Accepted
        );
        let stopped = wait_for_terminal(&runtime);
        assert_eq!(stopped.pause_reason, None);
        wait_for_admission_release(&runtime);

        assert_eq!(
            runtime.command(RuntimeCommand::Run(1)),
            CommandResult::Accepted
        );
        let running = wait_for_state(&runtime, RuntimeState::Running);
        assert!(running.run_id > stopped.run_id);
        assert_eq!(running.pause_reason, None);

        assert_eq!(
            runtime.command(RuntimeCommand::Stop),
            CommandResult::Accepted
        );
        assert_eq!(wait_for_terminal(&runtime).pause_reason, None);
    }

    #[test]
    fn snapshots_publish_each_mode_without_leaking_between_runs() {
        assert_eq!(RuntimeSnapshot::default().run_mode, RuntimeRunMode::Normal);
        let target = test_macro(1, true, vec![step(1, MkAction::Delay(Default::default()))]);
        let (_dir, runtime, _guard) = runtime_with(vec![target]);

        assert_eq!(
            runtime.command(RuntimeCommand::DebugRun(1)),
            CommandResult::Accepted
        );
        let debug_snapshot = wait_for_terminal(&runtime);
        assert_eq!(debug_snapshot.run_mode, RuntimeRunMode::Debug);
        wait_for_admission_release(&runtime);

        assert_eq!(
            runtime.command(RuntimeCommand::Run(1)),
            CommandResult::Accepted
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = runtime.snapshot();
            if snapshot.run_id > debug_snapshot.run_id && snapshot.state == RuntimeState::Completed
            {
                assert_eq!(snapshot.run_mode, RuntimeRunMode::Normal);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "normal run did not replace debug snapshot: {snapshot:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn preview_terminal_snapshot_survives_a_later_stored_run() {
        let target = test_macro(1, true, vec![step(1, MkAction::Delay(Default::default()))]);
        let document = MkMacroDocument {
            macros: vec![target.clone()],
            ..Default::default()
        };
        let (_dir, runtime, _guard) = runtime_with(vec![target]);

        runtime.preview(&document, 1, 77).unwrap();
        let preview = wait_for_terminal(&runtime);
        assert_eq!(
            preview.origin,
            RuntimeOrigin::RecordingPreview { ticket: 77 }
        );
        wait_for_admission_release(&runtime);

        assert_eq!(
            runtime.command(RuntimeCommand::Run(1)),
            CommandResult::Accepted
        );
        let stored = wait_for_terminal_after(&runtime, preview.run_id);
        assert_eq!(stored.origin, RuntimeOrigin::Stored);
        let (active, retained) = runtime.recording_preview_status(77);
        assert!(!active);
        assert_eq!(retained.unwrap().run_id, preview.run_id);
    }

    #[test]
    fn preview_requires_a_prepared_program_and_stops_only_its_exact_ticket() {
        let target = test_macro(
            1,
            true,
            vec![step(
                1,
                MkAction::Delay(MkDelayPayload {
                    fixed_ms: 1_000,
                    ..Default::default()
                }),
            )],
        );
        let document = MkMacroDocument {
            macros: vec![target.clone()],
            ..Default::default()
        };
        let (_dir, runtime, _guard) = runtime_with(vec![target]);
        assert!(matches!(
            runtime.command(RuntimeCommand::RecordingPreview {
                macro_id: 1,
                ticket: 41,
            }),
            CommandResult::Rejected(ExecutionDiagnostic {
                kind: DiagnosticKind::InvalidPlan,
                ..
            })
        ));

        runtime.preview(&document, 1, 42).unwrap();
        assert!(!runtime.stop_recording_preview(41));
        assert!(runtime.stop_recording_preview(42));
        let stopped = wait_for_terminal(&runtime);
        assert_eq!(
            stopped.origin,
            RuntimeOrigin::RecordingPreview { ticket: 42 }
        );
        assert_eq!(stopped.state, RuntimeState::Stopped);
    }

    #[test]
    fn preview_executes_the_ephemeral_program_without_publishing_it() {
        let stored = test_macro(
            1,
            true,
            vec![step(
                1,
                MkAction::Text(MkTextPayload {
                    text: "stored".into(),
                    mode: MkTextMode::Type,
                }),
            )],
        );
        let preview = test_macro(
            1,
            true,
            vec![
                step(
                    10,
                    MkAction::Text(MkTextPayload {
                        text: "preview A".into(),
                        mode: MkTextMode::Type,
                    }),
                ),
                step(
                    20,
                    MkAction::Text(MkTextPayload {
                        text: "preview B".into(),
                        mode: MkTextMode::Type,
                    }),
                ),
            ],
        );
        let document = MkMacroDocument {
            macros: vec![preview],
            ..Default::default()
        };
        let (dir, runtime, _guard, fake) = runtime_with_effects(vec![stored.clone()]);
        let persisted_before = runtime.store.snapshot();
        let macro_file = dir.path().join(MKMACROS_FILE);
        let bytes_before = fs::read(&macro_file).unwrap();

        runtime.preview(&document, 1, 81).unwrap();
        let completed = wait_for_terminal(&runtime);

        assert_eq!(completed.state, RuntimeState::Completed);
        assert_eq!(
            completed.origin,
            RuntimeOrigin::RecordingPreview { ticket: 81 }
        );
        assert_eq!(fake.events(), ["text:preview A", "text:preview B"]);
        assert_eq!(runtime.store.snapshot(), persisted_before);
        assert_eq!(runtime.store.snapshot().macros[0], stored);
        assert_eq!(fs::read(macro_file).unwrap(), bytes_before);
    }

    #[test]
    fn stopping_preview_releases_owned_keyboard_and_mouse_input() {
        let target = test_macro(
            1,
            true,
            vec![
                step(1, MkAction::KeyDown(MkKey::Control)),
                step(2, MkAction::MouseDown(MkMouseButton::Left)),
                step(
                    3,
                    MkAction::Delay(MkDelayPayload {
                        fixed_ms: 60_000,
                        ..Default::default()
                    }),
                ),
            ],
        );
        let document = MkMacroDocument {
            macros: vec![target.clone()],
            ..Default::default()
        };
        let (_dir, runtime, _guard, fake) = runtime_with_effects(vec![target]);
        runtime.preview(&document, 1, 82).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while fake.events().len() < 2 {
            assert!(Instant::now() < deadline, "preview did not acquire input");
            thread::sleep(Duration::from_millis(2));
        }

        assert!(runtime.stop_recording_preview(82));
        let stopped = wait_for_terminal(&runtime);

        assert_eq!(stopped.state, RuntimeState::Stopped);
        assert_eq!(
            fake.events(),
            [
                "key_down:Control",
                "button_down:Left",
                "button_up:Left",
                "key_up:Control"
            ]
        );
    }

    #[test]
    fn preview_admission_conflicts_with_recording_and_stored_playback_both_ways() {
        let target = test_macro(
            1,
            true,
            vec![step(
                1,
                MkAction::Delay(MkDelayPayload {
                    fixed_ms: 60_000,
                    ..Default::default()
                }),
            )],
        );
        let document = MkMacroDocument {
            macros: vec![target.clone()],
            ..Default::default()
        };
        let (_dir, runtime, guard) = runtime_with(vec![target]);

        assert!(guard.claim(Operation::Recording));
        assert!(matches!(
            runtime.preview(&document, 1, 90),
            Err(ExecutionDiagnostic {
                kind: DiagnosticKind::InvalidTarget,
                ..
            })
        ));
        guard.release(Operation::Recording);

        runtime.preview(&document, 1, 91).unwrap();
        assert!(matches!(
            runtime.command(RuntimeCommand::Run(1)),
            CommandResult::AlreadyRunning { active_macro_id: 1 }
        ));
        assert!(runtime.stop_recording_preview(91));
        let preview = wait_for_terminal(&runtime);
        wait_for_admission_release(&runtime);

        assert_eq!(
            runtime.command(RuntimeCommand::Run(1)),
            CommandResult::Accepted
        );
        let running = wait_for_state(&runtime, RuntimeState::Running);
        assert!(running.run_id > preview.run_id);
        assert!(matches!(
            runtime.preview(&document, 1, 92),
            Err(ExecutionDiagnostic {
                kind: DiagnosticKind::RuntimeUnavailable,
                ..
            })
        ));
        assert!(!runtime.stop_recording_preview(91));
        assert_eq!(runtime.snapshot().state, RuntimeState::Running);
        assert_eq!(
            runtime.command(RuntimeCommand::Stop),
            CommandResult::Accepted
        );
        assert_eq!(wait_for_terminal(&runtime).state, RuntimeState::Stopped);
    }

    #[test]
    fn admitted_preview_blocks_real_recorder_until_exact_ticket_stop_releases_guard() {
        let target = test_macro(
            1,
            true,
            vec![step(
                1,
                MkAction::Delay(MkDelayPayload {
                    fixed_ms: 60_000,
                    ..Default::default()
                }),
            )],
        );
        let document = MkMacroDocument {
            macros: vec![target.clone()],
            ..Default::default()
        };
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        store
            .save(MkMacroDocument {
                macros: vec![target],
                ..Default::default()
            })
            .unwrap();
        let store = Arc::new(store);
        let guard = Arc::new(SharedOperationGuard::default());
        let runtime = MacroRuntime::with_guard(
            store.clone(),
            Arc::new(FakeBackend::default()).backends(),
            guard.clone(),
        );
        let recorder = RecorderRuntime::with_guard_and_components(
            store,
            crate::mkmacro::HookService::with_adapter(AckHookLoop, 8),
            Arc::new(FixedRecorderClock),
            guard,
            Box::new(NoTextTranslator),
            Arc::new(empty_recorder_observer),
            Box::new(NoContextEnricher),
        );
        let barrier = runtime.install_test_worker_barrier();

        runtime.preview(&document, 1, 94).unwrap();
        barrier.wait_until_blocked();
        let error = recorder
            .start_target(
                RecordingTarget {
                    macro_id: 1,
                    insertion_anchor_step_id: None,
                    insertion_anchor_generation: None,
                },
                NormalizationConfig::default(),
                Vec::new(),
            )
            .unwrap_err();
        assert!(error.to_string().contains("playback is active"));

        assert!(runtime.stop_recording_preview(94));
        barrier.release();
        assert_eq!(wait_for_terminal(&runtime).state, RuntimeState::Stopped);
        wait_for_admission_release(&runtime);
        recorder
            .start_target(
                RecordingTarget {
                    macro_id: 1,
                    insertion_anchor_step_id: None,
                    insertion_anchor_generation: None,
                },
                NormalizationConfig::default(),
                Vec::new(),
            )
            .unwrap();
        assert_eq!(recorder.stop().unwrap().target.macro_id, 1);
    }

    #[test]
    fn failed_preview_retains_diagnostic_for_its_exact_ticket() {
        let target = test_macro(
            1,
            true,
            vec![step(
                1,
                MkAction::Text(MkTextPayload {
                    text: "fail preview".into(),
                    mode: MkTextMode::Type,
                }),
            )],
        );
        let document = MkMacroDocument {
            macros: vec![target.clone()],
            ..Default::default()
        };
        let (_dir, runtime, _guard, fake) = runtime_with_effects(vec![target]);
        let expected = ExecutionDiagnostic::new(DiagnosticKind::Backend, "preview failed")
            .context("operation", "recording preview")
            .context("ticket", "93");
        fake.fail("text:fail preview", expected.clone());

        runtime.preview(&document, 1, 93).unwrap();
        let failed = wait_for_terminal(&runtime);
        let (active, retained) = runtime.recording_preview_status(93);

        assert_eq!(failed.state, RuntimeState::Failed);
        assert_eq!(
            failed.origin,
            RuntimeOrigin::RecordingPreview { ticket: 93 }
        );
        let failure = failed.latest_failure.as_ref().unwrap();
        assert_eq!(failure.kind, DiagnosticKind::Backend);
        assert_eq!(failure.message, "preview failed");
        for (key, value) in [
            ("attempt", "1"),
            ("attempts_exhausted", "true"),
            ("backend_operation", "SendInput"),
            ("operation", "recording preview"),
            ("origin_macro_id", "1"),
            ("origin_step_id", "1"),
            ("step", "1"),
            ("step_id", "1"),
            ("ticket", "93"),
        ] {
            assert_eq!(failure.context.get(key).map(String::as_str), Some(value));
        }
        assert!(!active);
        let retained = retained.unwrap();
        assert_eq!(retained.run_id, failed.run_id);
        assert_eq!(retained.latest_failure, failed.latest_failure);
        let (wrong_active, wrong_snapshot) = runtime.recording_preview_status(92);
        assert!(!wrong_active);
        assert!(wrong_snapshot.is_none());
    }
}

#[cfg(test)]
mod facade_tests {
    use super::*;
    use crate::mkmacro::{
        MkAction, MkMacro, MkMacroDocument, MkStep, MkTextMode, MkTextPayload, SCHEMA_VERSION,
        executor::fake::FakeBackend,
    };
    use serial_test::serial;
    use std::time::{Duration, Instant};

    fn step(id: u64, text: &str) -> MkStep {
        MkStep {
            metadata: Default::default(),
            id,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action: MkAction::Text(MkTextPayload {
                text: text.into(),
                mode: MkTextMode::Type,
            }),
        }
    }

    fn wait_for_terminal(runtime: &MacroRuntime) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = runtime.snapshot();
            if snapshot.state == RuntimeState::Completed {
                return;
            }
            assert!(
                !matches!(snapshot.state, RuntimeState::Failed | RuntimeState::Stopped),
                "debug facade run failed: {snapshot:?}"
            );
            assert!(Instant::now() < deadline, "debug facade run did not finish");
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn wait_for_admission_release(runtime: &MacroRuntime) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while runtime.shared.admission.lock().unwrap().is_some() {
            assert!(
                Instant::now() < deadline,
                "runtime admission was not released"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    #[serial]
    fn debug_facades_submit_the_matching_commands_without_rewriting_arguments() {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        store
            .save(MkMacroDocument {
                schema_version: SCHEMA_VERSION,
                macros: vec![MkMacro {
                    signature: Default::default(),
                    id: 7301,
                    name: "debug facade target".into(),
                    description: String::new(),
                    enabled: true,
                    hotkey: None,
                    hotkey_scope: Default::default(),
                    folder_id: None,
                    playback: Default::default(),
                    steps: vec![step(10, "first"), step(20, "second"), step(30, "third")],
                }],
                ..Default::default()
            })
            .unwrap();
        let fake = Arc::new(FakeBackend::default());
        set_shared_store_with_backends(Arc::new(store), fake.backends());
        let runtime = RUNTIME.read().unwrap().as_ref().unwrap().clone();

        debug_run(7301).unwrap();
        assert_eq!(
            runtime.take_test_commands(),
            vec![RuntimeCommand::DebugRun(7301)]
        );
        wait_for_terminal(&runtime);
        assert_eq!(runtime.snapshot().macro_id, Some(7301));
        assert_eq!(runtime.snapshot().run_mode, RuntimeRunMode::Debug);
        wait_for_admission_release(&runtime);

        debug_run_from(7301, 20).unwrap();
        assert_eq!(
            runtime.take_test_commands(),
            vec![RuntimeCommand::DebugRunFrom(7301, 20)]
        );
        wait_for_terminal(&runtime);
        assert_eq!(runtime.snapshot().macro_id, Some(7301));
        assert_eq!(runtime.snapshot().run_mode, RuntimeRunMode::Debug);
        wait_for_admission_release(&runtime);

        let selection = vec![30, 10, 20];
        debug_run_selection(7301, selection.clone()).unwrap();
        assert_eq!(
            runtime.take_test_commands(),
            vec![RuntimeCommand::DebugRunSelection(7301, selection)]
        );
        wait_for_terminal(&runtime);
        assert_eq!(runtime.snapshot().macro_id, Some(7301));
        assert_eq!(runtime.snapshot().run_mode, RuntimeRunMode::Debug);
    }
}

#[cfg(test)]
mod recording_controller_tests {
    use super::*;
    use crate::mkmacro::{
        AuxiliaryObservationWorker, CallbackSender, HookCommand, HookCommandRequest, HookEvent,
        HookLoopAdapter, HookService, KeyTranslation, KeyboardTranslationRequest, MkMacro,
        MkMacroDocument, MkPlayback, ObservationBaseline, RawWindowObservation,
        RecorderObserverSession, SCHEMA_VERSION, WindowEventSource, executor::fake::FakeBackend,
    };

    struct FakeHookLoop;
    impl HookLoopAdapter for FakeHookLoop {
        fn run(self, commands: mpsc::Receiver<HookCommandRequest>, _callback: CallbackSender) {
            while let Ok(request) = commands.recv() {
                let shutdown = request.command == HookCommand::Shutdown;
                request.acknowledge(true);
                if shutdown {
                    break;
                }
            }
        }
    }
    struct SyntheticTranslator;
    impl KeyboardTranslator for SyntheticTranslator {
        fn initial_key_state(&mut self) -> [u8; 256] {
            [0; 256]
        }
        fn translate(&mut self, _: &KeyboardTranslationRequest) -> KeyTranslation {
            KeyTranslation::None
        }
    }
    struct SyntheticEnricher;
    impl EventEnricher for SyntheticEnricher {
        fn enrich(&mut self, _: &HookEvent) -> Option<crate::mkmacro::EventContext> {
            None
        }
    }

    struct BlockingObservationSource {
        gate: Arc<(Mutex<(bool, bool)>, Condvar)>,
    }
    impl WindowEventSource for BlockingObservationSource {
        fn drain(&mut self) -> Vec<RawWindowObservation> {
            Vec::new()
        }
        fn shutdown_and_drain(&mut self) -> Vec<RawWindowObservation> {
            let (lock, wake) = &*self.gate;
            let mut state = lock.lock().unwrap();
            state.0 = true;
            wake.notify_all();
            while !state.1 {
                state = wake.wait(state).unwrap();
            }
            Vec::new()
        }
    }

    #[test]
    #[serial_test::serial]
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
                        signature: Default::default(),
                        id,
                        name: format!("macro {id}"),
                        description: String::new(),
                        enabled: true,
                        hotkey: None,
                        hotkey_scope: Default::default(),
                        folder_id: None,
                        playback: MkPlayback::default(),
                        steps: vec![],
                    })
                    .collect(),
            })
            .unwrap();
        let store = Arc::new(store);
        let fake = Arc::new(FakeBackend::default());
        let observation_gate = Arc::new((Mutex::new((false, false)), Condvar::new()));
        let observer_factory = {
            let gate = observation_gate.clone();
            Arc::new(move || {
                RecorderObserverSession::with_sources(
                    ObservationBaseline::default(),
                    AuxiliaryObservationWorker::spawn(None, None),
                    Some(Box::new(BlockingObservationSource { gate: gate.clone() })),
                )
            })
        };
        set_shared_store_components(
            store.clone(),
            fake.backends(),
            &[],
            HookService::with_adapter(FakeHookLoop, 16),
            false,
            Some((
                Box::new(SyntheticTranslator),
                observer_factory,
                Box::new(SyntheticEnricher),
            )),
        );
        take_pending_recordings();

        arm_recording(None, NormalizationConfig::default());
        toggle_recording();
        assert_eq!(
            recorder_snapshot().unwrap().state,
            super::super::RecorderRuntimeState::Idle
        );
        assert_eq!(
            recording_status().as_deref(),
            Some("Select a macro before starting recording")
        );

        arm_recording(
            Some(RecordingTarget {
                macro_id: 2,
                insertion_anchor_step_id: None,
                insertion_anchor_generation: None,
            }),
            NormalizationConfig::default(),
        );
        assert_eq!(recording_status(), None);
        toggle_recording();
        assert_eq!(recorder_snapshot().unwrap().macro_id, Some(2));
        let unchanged = store.snapshot();
        assert!(request_record_stop_for_review().unwrap());
        {
            let (lock, wake) = &*observation_gate;
            let state = lock.lock().unwrap();
            let (mut state, timeout) = wake
                .wait_timeout_while(state, std::time::Duration::from_secs(2), |state| !state.0)
                .unwrap();
            assert!(
                !timeout.timed_out(),
                "recording finalization did not reach its barrier"
            );
            assert!(record_stop_pending());
            assert_eq!(
                recorder_snapshot().unwrap().state,
                super::super::RecorderRuntimeState::Stopping
            );
            assert!(!request_record_stop_for_review().unwrap());
            assert!(
                run(1)
                    .unwrap_err()
                    .to_string()
                    .contains("recording is active")
            );
            assert_eq!(*unchanged, *store.snapshot());
            state.1 = true;
            wake.notify_all();
        }
        let (pending_lock, pending_wake) = &*RECORD_STOP_COORDINATOR;
        let pending = pending_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let (pending, timeout) = pending_wake
            .wait_timeout_while(pending, std::time::Duration::from_secs(2), |pending| {
                *pending
            })
            .unwrap_or_else(|error| error.into_inner());
        assert!(!timeout.timed_out(), "recording stop did not complete");
        assert!(!*pending);
        drop(pending);
        assert_eq!(
            recorder_snapshot().unwrap().state,
            super::super::RecorderRuntimeState::Idle
        );
        let results = take_pending_recordings();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].target.macro_id, 2);
        assert!(results[0].plan.steps.is_empty());
        assert_eq!(*unchanged, *store.snapshot());
        assert!(request_record_stop_for_review().is_err());
        assert!(take_pending_recordings().is_empty());
    }
}

#[cfg(test)]
mod step_outcome_tests {
    use super::*;

    #[test]
    fn image_match_and_continued_miss_are_distinct_success_details() {
        let matched = StepOutcome {
            last_image_found: Some(true),
            ..StepOutcome::default()
        };
        let missed = StepOutcome {
            last_image_found: Some(false),
            ..StepOutcome::default()
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
        assert_eq!(unrelated.last_ocr_found, None);
        assert_eq!(unrelated.detail(), None);
    }

    #[test]
    fn ocr_match_and_continued_miss_are_distinct_success_details() {
        let matched = StepOutcome {
            last_ocr_found: Some(true),
            ..StepOutcome::default()
        };
        let missed = StepOutcome {
            last_ocr_found: Some(false),
            ..StepOutcome::default()
        };
        assert_eq!(matched.detail(), Some("Success — OCR text found."));
        assert_eq!(
            missed.detail(),
            Some("Success — OCR text not found; continued.")
        );
    }
}

#[cfg(test)]
mod runtime_snapshot_tests {
    use super::*;
    use crate::mkmacro::{
        MkAction, MkCoordinateTarget, MkDelayPayload, MkImageNotFoundPolicy, MkImageOutputs,
        MkImagePayload, MkImageRef, MkMacro, MkMacroDocument, MkMouseMovePayload, MkOcrFindPayload,
        MkOcrSearchSpec, MkPoint, MkStep, MkTextMode, MkTextPayload, MkValue, MkWaitOptions,
        OcrDocument, OcrLine, OcrWord, ScreenCaptureBackend, ScreenRect, SearchRegion,
        executor::fake::FakeBackend,
    };
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn step(id: u64, action: MkAction) -> MkStep {
        MkStep {
            metadata: Default::default(),
            id,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            action,
        }
    }

    fn test_macro(steps: Vec<MkStep>) -> MkMacro {
        MkMacro {
            signature: Default::default(),
            id: 1,
            name: "runtime snapshot test".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            steps,
        }
    }

    fn image_payload(policy: MkImageNotFoundPolicy) -> MkImagePayload {
        MkImagePayload {
            image: MkImageRef::from_filename("10.png"),
            wait: MkWaitOptions {
                timeout_ms: 0,
                poll_interval_ms: 1,
            },
            region: Default::default(),
            tolerance: 0,
            alpha: Default::default(),
            return_point: Default::default(),
            not_found_policy: policy,
            outputs: MkImageOutputs {
                point: Some("target_point".into()),
                ..Default::default()
            },
        }
    }

    fn variable_move() -> MkAction {
        MkAction::MouseMove(MkMouseMovePayload {
            target: MkCoordinateTarget::Variable {
                name: "target_point".into(),
            },
            duration_ms: 0,
        })
    }

    fn document_for(target: MkMacro) -> MkMacroDocument {
        MkMacroDocument {
            macros: vec![target],
            ..Default::default()
        }
    }

    fn runtime_with(target: MkMacro) -> (tempfile::TempDir, MacroRuntime, Arc<FakeBackend>) {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        store
            .save(MkMacroDocument {
                macros: vec![target],
                ..Default::default()
            })
            .unwrap();
        let fake = Arc::new(FakeBackend::default());
        let runtime = MacroRuntime::new(Arc::new(store), fake.clone().backends());
        (directory, runtime, fake)
    }

    struct OcrCapture;

    impl ScreenCaptureBackend for OcrCapture {
        fn virtual_desktop(&self) -> crate::mkmacro::ExecResult<ScreenRect> {
            Ok(ScreenRect::new(0, 0, 200, 100))
        }

        fn region_bounds(&self, _: &SearchRegion) -> crate::mkmacro::ExecResult<ScreenRect> {
            Ok(ScreenRect::new(10, 20, 100, 40))
        }

        fn capture_rect(
            &self,
            rect: ScreenRect,
            _: &dyn Fn() -> bool,
        ) -> crate::mkmacro::ExecResult<image::RgbaImage> {
            Ok(image::RgbaImage::new(rect.width, rect.height))
        }
    }

    fn ocr_document(text: &str) -> OcrDocument {
        OcrDocument {
            image_width: 100,
            image_height: 40,
            lines: vec![OcrLine {
                text: text.into(),
                words: vec![OcrWord {
                    text: text.into(),
                    bounds: ScreenRect::new(2, 3, 30, 10),
                }],
            }],
            ..OcrDocument::default()
        }
    }

    fn runtime_with_ocr(
        target: MkMacro,
        recognized: &str,
    ) -> (tempfile::TempDir, MacroRuntime, Arc<FakeBackend>) {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        store.save(document_for(target)).unwrap();
        let fake = Arc::new(FakeBackend::default());
        fake.script_ocr(Ok(ocr_document(recognized)));
        let mut backends = fake.clone().backends();
        backends.screenshot_capture = Arc::new(OcrCapture);
        let runtime = MacroRuntime::new(Arc::new(store), backends);
        (directory, runtime, fake)
    }

    #[test]
    fn runtime_observes_ocr_match_and_continued_miss_step_outcomes() {
        for (recognized, expected_found) in [("target", true), ("other", false)] {
            let action = MkAction::OcrFindText(MkOcrFindPayload {
                search: MkOcrSearchSpec {
                    text: "target".into(),
                    ..Default::default()
                },
                wait: MkWaitOptions {
                    timeout_ms: 100,
                    poll_interval_ms: 100,
                },
                not_found_policy: MkImageNotFoundPolicy::Continue,
                ..Default::default()
            });
            let (_directory, runtime, _fake) =
                runtime_with_ocr(test_macro(vec![step(1, action)]), recognized);
            assert_eq!(
                runtime.command(RuntimeCommand::Run(1)),
                CommandResult::Accepted
            );
            let completed = wait_for_terminal(&runtime);
            assert_eq!(completed.state, RuntimeState::Completed, "{completed:?}");
            let expected = StepOutcome {
                last_ocr_found: Some(expected_found),
                ..StepOutcome::default()
            };
            assert_eq!(completed.step_outcomes.get(&1), Some(&expected));
            assert_eq!(
                completed.last_completed.as_ref().map(|step| &step.outcome),
                Some(&CompletedStepOutcome::Success(Some(expected.clone())))
            );
            assert!(runtime.take_test_events().iter().any(|event| {
                matches!(event, ExecutionEvent::StepOutcome(1, outcome) if outcome == &expected)
            }));
        }
    }

    fn wait_for_terminal(runtime: &MacroRuntime) -> Arc<RuntimeSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = runtime.snapshot();
            if matches!(
                snapshot.state,
                RuntimeState::Completed | RuntimeState::Failed | RuntimeState::Stopped
            ) {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "runtime did not finish: {snapshot:?}"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn wait_for_state(runtime: &MacroRuntime, state: RuntimeState) -> Arc<RuntimeSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = runtime.snapshot();
            if snapshot.state == state {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "runtime did not reach {state:?}: {snapshot:?}"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn wait_for_debug_boundary(
        runtime: &MacroRuntime,
        step_id: Option<u64>,
        reason: DebugSnapshotReason,
    ) -> Arc<RuntimeSnapshot> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = runtime.snapshot();
            if snapshot
                .debug_snapshot
                .as_ref()
                .is_some_and(|debug| debug.step_id == step_id && debug.reason == reason)
            {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "runtime did not publish debug boundary {step_id:?}/{reason:?}: {snapshot:?}"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn assert_debug_fields(
        snapshot: &RuntimeSnapshot,
        step_id: Option<u64>,
        reason: DebugSnapshotReason,
    ) {
        assert_eq!(snapshot.debug_variables_step_id, step_id);
        assert_eq!(snapshot.debug_snapshot_reason, Some(reason));
        let debug = snapshot
            .debug_snapshot
            .as_ref()
            .expect("debug event also publishes the compatibility snapshot");
        assert_eq!(debug.step_id, step_id);
        assert_eq!(debug.reason, reason);
        assert_eq!(&*snapshot.debug_variables, &*debug.variables);
    }

    #[test]
    fn debug_run_from_starts_at_set_b_and_resumes_into_mouse_move() {
        let target = test_macro(vec![
            step(
                1,
                MkAction::SetVariable {
                    name: "A".into(),
                    value: MkValue::String("Set A".into()),
                },
            ),
            {
                let mut breakpoint = step(
                    2,
                    MkAction::SetVariable {
                        name: "B".into(),
                        value: MkValue::String("Set B".into()),
                    },
                );
                breakpoint.breakpoint = true;
                breakpoint
            },
            step(
                3,
                MkAction::MouseMove(MkMouseMovePayload {
                    target: MkCoordinateTarget::Screen {
                        point: MkPoint { x: 19, y: 27 },
                    },
                    duration_ms: 0,
                }),
            ),
        ]);
        let (_directory, runtime, fake) = runtime_with(target);

        assert_eq!(
            runtime.command(RuntimeCommand::DebugRunFrom(1, 2)),
            CommandResult::Accepted
        );
        let paused = wait_for_debug_boundary(&runtime, Some(2), DebugSnapshotReason::Breakpoint);
        assert_eq!(paused.state, RuntimeState::Paused);
        assert_eq!(paused.step_id, Some(2));
        assert_eq!(
            paused.pause_reason,
            Some(RuntimePauseReason::Breakpoint {
                step_id: 2,
                frame: ExecutionFrameContext::root(1)
            })
        );
        assert!(!paused.steps.contains_key(&1));
        assert_eq!(paused.steps[&2], StepState::Pending);
        assert_eq!(paused.steps[&3], StepState::Pending);
        assert!(!paused.debug_variables.contains_key("A"));
        assert!(!paused.debug_variables.contains_key("B"));
        assert!(fake.events().is_empty());

        assert_eq!(
            runtime.command(RuntimeCommand::Resume),
            CommandResult::Accepted
        );
        let completed = wait_for_terminal(&runtime);
        let events = runtime.take_test_events();
        assert_eq!(completed.state, RuntimeState::Completed);
        assert_eq!(completed.run_mode, RuntimeRunMode::Debug);
        assert_eq!(completed.steps[&2], StepState::Success);
        assert_eq!(completed.steps[&3], StepState::Success);
        assert_eq!(completed.total_steps, 2);
        assert_eq!(completed.completed_steps, 2);
        assert_eq!(
            completed.debug_variables.get("B"),
            Some(&MkValue::String("Set B".into()))
        );
        assert_eq!(fake.events(), ["move:19,27"]);
        assert_eq!(
            events
                .iter()
                .filter_map(|event| match event {
                    ExecutionEvent::StepStarted(id) | ExecutionEvent::StepFinished(id) => {
                        Some(*id)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [2, 2, 3, 3]
        );
    }

    #[test]
    fn debug_breakpoint_snapshot_exposes_image_point_before_mouse_move() {
        let point = MkPoint { x: 823, y: 441 };
        let mut mouse_move = step(2, variable_move());
        mouse_move.breakpoint = true;
        let target = test_macro(vec![
            step(
                1,
                MkAction::ImageFind(image_payload(MkImageNotFoundPolicy::Fail)),
            ),
            mouse_move,
        ]);
        let (_directory, runtime, fake) = runtime_with(target);
        fake.script_image(MkImageRef::from_filename("10.png"), Ok(Some(point)));

        assert_eq!(
            runtime.command(RuntimeCommand::DebugRun(1)),
            CommandResult::Accepted
        );
        let paused = wait_for_debug_boundary(&runtime, Some(2), DebugSnapshotReason::Breakpoint);
        assert_eq!(paused.state, RuntimeState::Paused);
        assert_eq!(
            paused.pause_reason,
            Some(RuntimePauseReason::Breakpoint {
                step_id: 2,
                frame: ExecutionFrameContext::root(1)
            })
        );
        assert_eq!(
            paused.debug_variables.get("target_point"),
            Some(&MkValue::Point(point))
        );
        assert_debug_fields(&paused, Some(2), DebugSnapshotReason::Breakpoint);
        assert_eq!(paused.steps[&2], StepState::Pending);
        assert!(
            !fake.events().iter().any(|event| event.starts_with("move:")),
            "Mouse Move must not execute before its breakpoint is resumed"
        );

        assert_eq!(
            runtime.command(RuntimeCommand::Resume),
            CommandResult::Accepted
        );
        let completed = wait_for_terminal(&runtime);
        assert_eq!(completed.state, RuntimeState::Completed);
        assert_eq!(completed.steps[&2], StepState::Success);
        assert_eq!(
            fake.events()
                .iter()
                .filter(|event| event.as_str() == "move:823,441")
                .count(),
            1
        );
    }

    #[test]
    fn debug_breakpoint_snapshot_preserves_image_miss_as_null_for_inspection() {
        let mut mouse_move = step(2, variable_move());
        mouse_move.breakpoint = true;
        let target = test_macro(vec![
            step(
                1,
                MkAction::ImageFind(image_payload(MkImageNotFoundPolicy::Continue)),
            ),
            mouse_move,
        ]);
        let document = document_for(target.clone());
        let (_directory, runtime, fake) = runtime_with(target);
        fake.script_image(MkImageRef::from_filename("10.png"), Ok(None));

        assert_eq!(
            runtime.command(RuntimeCommand::DebugRun(1)),
            CommandResult::Accepted
        );
        let paused = wait_for_debug_boundary(&runtime, Some(2), DebugSnapshotReason::Breakpoint);
        assert_eq!(
            paused.debug_variables.get("target_point"),
            Some(&MkValue::Null)
        );
        assert_debug_fields(&paused, Some(2), DebugSnapshotReason::Breakpoint);
        assert_eq!(paused.steps[&2], StepState::Pending);
        assert!(
            !fake.events().iter().any(|event| event.starts_with("move:")),
            "Mouse Move must not execute before its breakpoint is resumed"
        );

        let view =
            crate::gui::mkmacro_dialog::runtime_inspector::RuntimeInspectorViewModel::from_snapshot(
                &paused, &document,
            );
        let row = view
            .variables
            .user
            .iter()
            .find(|entry| entry.name == "target_point")
            .expect("target_point is visible as a user variable");
        assert_eq!(row.name, "target_point");
        assert_eq!(row.value.type_name, "Null");
        assert_eq!(row.value.table_text, "null");

        assert_eq!(
            runtime.command(RuntimeCommand::Resume),
            CommandResult::Accepted
        );
        let failed = wait_for_terminal(&runtime);
        assert_eq!(failed.state, RuntimeState::Failed);
        assert_eq!(failed.steps[&2], StepState::Failed);
        let diagnostic = failed
            .latest_failure
            .as_ref()
            .expect("Mouse Move publishes its type diagnostic");
        assert_eq!(diagnostic.kind, DiagnosticKind::TypeMismatch);
        assert_eq!(
            diagnostic.message,
            "Variable 'target_point' contains Null; coordinate target requires Point"
        );
        assert_eq!(
            diagnostic.context.get("expected").map(String::as_str),
            Some("Point")
        );
        assert!(
            !fake.events().iter().any(|event| event.starts_with("move:")),
            "a Null coordinate target must not move the pointer"
        );
    }

    #[test]
    fn default_snapshot_and_new_run_reset_all_debugger_fields() {
        let default = RuntimeSnapshot::default();
        assert_eq!(default.run_mode, RuntimeRunMode::Normal);
        assert_eq!(default.pause_reason, None);
        assert!(default.debug_variables.is_empty());
        assert_eq!(default.debug_variables_step_id, None);
        assert_eq!(default.debug_snapshot_reason, None);
        assert_eq!(default.last_completed_step_id, None);

        let mut breakpoint = step(
            1,
            MkAction::Delay(MkDelayPayload {
                fixed_ms: 60_000,
                ..Default::default()
            }),
        );
        breakpoint.breakpoint = true;
        let (_directory, runtime, _fake) = runtime_with(test_macro(vec![breakpoint]));

        assert_eq!(
            runtime.command(RuntimeCommand::DebugRun(1)),
            CommandResult::Accepted
        );
        wait_for_state(&runtime, RuntimeState::Paused);
        assert_eq!(
            runtime.command(RuntimeCommand::Stop),
            CommandResult::Accepted
        );
        let stopped = wait_for_terminal(&runtime);
        assert_eq!(
            stopped.debug_snapshot_reason,
            Some(DebugSnapshotReason::RunCancelled)
        );
        assert!(!stopped.debug_variables.is_empty());

        assert_eq!(
            runtime.command(RuntimeCommand::Run(1)),
            CommandResult::Accepted
        );
        let running = wait_for_state(&runtime, RuntimeState::Running);
        assert_eq!(running.run_mode, RuntimeRunMode::Normal);
        assert_eq!(running.pause_reason, None);
        assert!(running.debug_variables.is_empty());
        assert_eq!(running.debug_variables_step_id, None);
        assert_eq!(running.debug_snapshot_reason, None);
        assert_eq!(running.last_completed_step_id, None);
        assert!(running.debug_snapshot.is_none());
        assert_eq!(
            runtime.command(RuntimeCommand::Stop),
            CommandResult::Accepted
        );
        assert_eq!(wait_for_terminal(&runtime).pause_reason, None);
    }

    #[test]
    fn debugger_fields_update_together_at_breakpoints_and_step_boundaries() {
        let mut breakpoint = step(
            2,
            MkAction::SetVariable {
                name: "after_breakpoint".into(),
                value: MkValue::Boolean(true),
            },
        );
        breakpoint.breakpoint = true;
        let mut hold = step(
            3,
            MkAction::Delay(MkDelayPayload {
                fixed_ms: 60_000,
                ..Default::default()
            }),
        );
        hold.breakpoint = false;
        let (_directory, runtime, _fake) = runtime_with(test_macro(vec![
            step(
                1,
                MkAction::SetVariable {
                    name: "before_breakpoint".into(),
                    value: MkValue::String("ready".into()),
                },
            ),
            breakpoint,
            hold,
        ]));

        assert_eq!(
            runtime.command(RuntimeCommand::DebugRun(1)),
            CommandResult::Accepted
        );
        let paused = wait_for_state(&runtime, RuntimeState::Paused);
        assert_debug_fields(&paused, Some(2), DebugSnapshotReason::Breakpoint);
        assert_eq!(
            paused.pause_reason,
            Some(RuntimePauseReason::Breakpoint {
                step_id: 2,
                frame: ExecutionFrameContext::root(1)
            })
        );
        assert!(!paused.debug_variables.contains_key("after_breakpoint"));

        assert_eq!(
            runtime.command(RuntimeCommand::Resume),
            CommandResult::Accepted
        );
        let boundary_deadline = Instant::now() + Duration::from_secs(10);
        let boundary = loop {
            let snapshot = runtime.snapshot();
            if snapshot.debug_snapshot_reason == Some(DebugSnapshotReason::StepBoundary)
                && snapshot.debug_variables_step_id == Some(2)
            {
                break snapshot;
            }
            assert!(
                Instant::now() < boundary_deadline,
                "step boundary was not published: {snapshot:?}"
            );
            std::thread::sleep(Duration::from_millis(2));
        };
        assert_debug_fields(&boundary, Some(2), DebugSnapshotReason::StepBoundary);
        assert_eq!(
            boundary.debug_variables.get("after_breakpoint"),
            Some(&MkValue::Boolean(true))
        );
        assert_eq!(boundary.last_completed_step_id, Some(2));

        assert_eq!(
            runtime.command(RuntimeCommand::Stop),
            CommandResult::Accepted
        );
        assert_eq!(wait_for_terminal(&runtime).pause_reason, None);
    }
    #[test]
    fn completion_failure_and_skip_policy_preserve_runtime_correlation() {
        let (_directory, success_runtime, _fake) = runtime_with(test_macro(vec![step(
            10,
            MkAction::SetVariable {
                name: "completed".into(),
                value: MkValue::Boolean(true),
            },
        )]));
        assert_eq!(
            success_runtime.command(RuntimeCommand::DebugRun(1)),
            CommandResult::Accepted
        );
        let success = wait_for_terminal(&success_runtime);
        assert_eq!(success.state, RuntimeState::Completed);
        assert_eq!(success.pause_reason, None);
        assert_eq!(success.last_completed_step_id, Some(10));
        assert_eq!(success.steps[&10], StepState::Success);
        assert_debug_fields(&success, Some(10), DebugSnapshotReason::RunFinished);

        let (_directory, failure_runtime, fake) = runtime_with(test_macro(vec![
            step(
                20,
                MkAction::SetVariable {
                    name: "safe".into(),
                    value: MkValue::String("before failure".into()),
                },
            ),
            step(
                21,
                MkAction::Text(MkTextPayload {
                    text: "failure".into(),
                    mode: MkTextMode::Type,
                }),
            ),
        ]));
        fake.fail(
            "text:failure",
            ExecutionDiagnostic::new(DiagnosticKind::Backend, "injected failure"),
        );
        assert_eq!(
            failure_runtime.command(RuntimeCommand::DebugRun(1)),
            CommandResult::Accepted
        );
        let failure = wait_for_terminal(&failure_runtime);
        assert_eq!(failure.state, RuntimeState::Failed);
        assert_eq!(failure.pause_reason, None);
        assert_eq!(failure.last_completed_step_id, Some(21));
        assert_eq!(failure.steps[&21], StepState::Failed);
        assert!(failure.failures.contains_key(&DiagnosticKey {
            run_id: failure.run_id,
            step_id: 21,
        }));
        assert_debug_fields(&failure, Some(20), DebugSnapshotReason::RunFailed);
        assert_eq!(
            failure.debug_variables.get("safe"),
            Some(&MkValue::String("before failure".into()))
        );

        let mut skipped = step(30, MkAction::Delay(Default::default()));
        skipped.enabled = false;
        let (_directory, skipped_runtime, _fake) = runtime_with(test_macro(vec![skipped]));
        assert_eq!(
            skipped_runtime.command(RuntimeCommand::Run(1)),
            CommandResult::Accepted
        );
        let skipped_snapshot = wait_for_terminal(&skipped_runtime);
        assert_eq!(skipped_snapshot.state, RuntimeState::Completed);
        assert_eq!(skipped_snapshot.steps[&30], StepState::Skipped);
        assert_eq!(skipped_snapshot.completed_steps, 0);
        // Skipped steps do not count as completed: they never produced an
        // execution outcome and therefore leave this correlation unset.
        assert_eq!(skipped_snapshot.last_completed_step_id, None);
    }
}
