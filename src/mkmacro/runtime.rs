//! Worker-owned macro runtime.  The public methods only exchange messages and snapshots;
//! action execution and all waits live on the worker.
use super::{MkMacroStore, compile, executor::*};
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
}
pub struct MacroRuntime {
    tx: mpsc::Sender<RuntimeCommand>,
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl MacroRuntime {
    pub fn new(store: Arc<MkMacroStore>, backends: Backends) -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Shared {
            snapshot: RwLock::new(Arc::new(RuntimeSnapshot::default())),
            control: Arc::new(RunControl::default()),
            admission: Mutex::new(None),
            next_run_id: AtomicU64::new(1),
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
        if let RuntimeCommand::Run(id)
        | RuntimeCommand::RunFrom(id, _)
        | RuntimeCommand::RunSelection(id, _) = &c
        {
            let mut active = self.shared.admission.lock().unwrap();
            if let Some(a) = *active {
                return CommandResult::AlreadyRunning { active_macro_id: a };
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
            RuntimeCommand::Run(mid) | RuntimeCommand::RunFrom(mid, 0) => {
                run_one(&store, &backends, &shared, mid, None, None)
            }
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
            s.steps = Arc::new(states)
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
}

static RUNTIME: Lazy<RwLock<Option<Arc<MacroRuntime>>>> = Lazy::new(|| RwLock::new(None));
pub fn set_shared_store(store: Arc<MkMacroStore>) {
    if let Some(old) = RUNTIME.write().unwrap().take() {
        old.shutdown()
    }
    *RUNTIME.write().unwrap() = Some(Arc::new(MacroRuntime::new(store, Backends::unsupported())))
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
pub fn record() -> Result<()> {
    Err(anyhow!("macro recording is not implemented"))
}
pub fn record_stop() -> Result<()> {
    Err(anyhow!("macro recording is not implemented"))
}
