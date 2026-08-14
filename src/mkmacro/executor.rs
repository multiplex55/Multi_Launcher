//! Platform-neutral plan executor and injectable effect boundaries.
use super::{
    Jump, MkAction, MkCompareOp, MkCondition, MkCoordinateTarget, MkExecutionPlan, MkImagePayload,
    MkKey, MkMouseButton, MkPoint, MkProcessPayload, MkTextPayload, MkUiPayload, MkValue,
    MkWaitOptions, MkWindowMatcher, MkWindowPayload, RuntimeVariables,
};
use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    TargetNotFound,
    AmbiguousTarget,
    Timeout,
    UnsupportedOperation,
    InputRejected,
    InvalidTarget,
    InvalidPlan,
    InvalidSelection,
    Backend,
    Cancelled,
    Panic,
    RuntimeUnavailable,
    TypeMismatch,
    InvalidRegex,
    IterationLimit,
    UnsupportedPattern,
    StaleElement,
    ComFailure,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionDiagnostic {
    pub kind: DiagnosticKind,
    pub message: String,
    pub context: BTreeMap<String, String>,
}
impl ExecutionDiagnostic {
    pub fn new(kind: DiagnosticKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            context: BTreeMap::new(),
        }
    }
    pub fn context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
}
impl fmt::Display for ExecutionDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for ExecutionDiagnostic {}
pub type ExecResult<T = ()> = Result<T, ExecutionDiagnostic>;

pub trait InputBackend: Send + Sync {
    fn key_down(&self, key: &MkKey) -> ExecResult;
    fn key_up(&self, key: &MkKey) -> ExecResult;
    fn button_down(&self, button: MkMouseButton) -> ExecResult;
    fn button_up(&self, button: MkMouseButton) -> ExecResult;
    fn move_mouse(&self, point: MkPoint) -> ExecResult;
    fn scroll(&self, delta: i32) -> ExecResult;
    fn text(&self, payload: &MkTextPayload) -> ExecResult;
}
pub trait WindowBackend: Send + Sync {
    fn exists(&self, m: &MkWindowMatcher) -> ExecResult<bool>;
    fn is_active(&self, m: &MkWindowMatcher) -> ExecResult<bool>;
    fn activate(&self, p: &MkWindowPayload) -> ExecResult;
    fn close(&self, m: &MkWindowMatcher) -> ExecResult;
}
pub trait ScreenBackend: Send + Sync {
    fn resolve(
        &self,
        target: &MkCoordinateTarget,
        variables: &RuntimeVariables,
    ) -> ExecResult<MkPoint>;
    fn image_found(&self, asset_id: u64, confidence: f32) -> ExecResult<Option<MkPoint>>;
    fn pixel_matches(
        &self,
        target: &MkCoordinateTarget,
        color: &str,
        tolerance: u8,
        variables: &RuntimeVariables,
    ) -> ExecResult<bool>;
}
pub trait UiAutomationBackend: Send + Sync {
    fn exists(&self, p: &MkUiPayload) -> ExecResult<bool>;
    fn invoke(&self, p: &MkUiPayload) -> ExecResult;
    fn set_value(&self, p: &MkUiPayload, value: &str) -> ExecResult;
    fn read_value(&self, _: &MkUiPayload) -> ExecResult<String> {
        unsupported()
    }
    fn toggle(&self, _: &MkUiPayload) -> ExecResult {
        unsupported()
    }
    fn select(&self, _: &MkUiPayload) -> ExecResult {
        unsupported()
    }
    fn focus(&self, _: &MkUiPayload) -> ExecResult {
        unsupported()
    }
}
pub trait UiAutomationInspector: Send + Sync {
    fn inspect_at(&self, point: MkPoint) -> ExecResult<super::UiElementInfo>;
}
pub trait LauncherBackend: Send + Sync {
    fn launch_process(&self, p: &MkProcessPayload) -> ExecResult;
    fn command(&self, command: &str, args: Option<&str>) -> ExecResult;
}

#[derive(Clone)]
pub struct Backends {
    pub input: Arc<dyn InputBackend>,
    pub window: Arc<dyn WindowBackend>,
    pub screen: Arc<dyn ScreenBackend>,
    pub uia: Arc<dyn UiAutomationBackend>,
    pub launcher: Arc<dyn LauncherBackend>,
}
impl Backends {
    pub fn unsupported() -> Self {
        let u = Arc::new(Unsupported);
        Self {
            input: u.clone(),
            window: u.clone(),
            screen: u.clone(),
            uia: u.clone(),
            launcher: u,
        }
    }
}
struct Unsupported;
fn unsupported<T>() -> ExecResult<T> {
    Err(ExecutionDiagnostic::new(
        DiagnosticKind::UnsupportedOperation,
        "automation backend is unavailable on this platform",
    ))
}
impl InputBackend for Unsupported {
    fn key_down(&self, _: &MkKey) -> ExecResult {
        unsupported()
    }
    fn key_up(&self, _: &MkKey) -> ExecResult {
        unsupported()
    }
    fn button_down(&self, _: MkMouseButton) -> ExecResult {
        unsupported()
    }
    fn button_up(&self, _: MkMouseButton) -> ExecResult {
        unsupported()
    }
    fn move_mouse(&self, _: MkPoint) -> ExecResult {
        unsupported()
    }
    fn scroll(&self, _: i32) -> ExecResult {
        unsupported()
    }
    fn text(&self, _: &MkTextPayload) -> ExecResult {
        unsupported()
    }
}
impl WindowBackend for Unsupported {
    fn exists(&self, _: &MkWindowMatcher) -> ExecResult<bool> {
        unsupported()
    }
    fn is_active(&self, _: &MkWindowMatcher) -> ExecResult<bool> {
        unsupported()
    }
    fn activate(&self, _: &MkWindowPayload) -> ExecResult {
        unsupported()
    }
    fn close(&self, _: &MkWindowMatcher) -> ExecResult {
        unsupported()
    }
}
impl ScreenBackend for Unsupported {
    fn resolve(&self, _: &MkCoordinateTarget, _: &RuntimeVariables) -> ExecResult<MkPoint> {
        unsupported()
    }
    fn image_found(&self, _: u64, _: f32) -> ExecResult<Option<MkPoint>> {
        unsupported()
    }
    fn pixel_matches(
        &self,
        _: &MkCoordinateTarget,
        _: &str,
        _: u8,
        _: &RuntimeVariables,
    ) -> ExecResult<bool> {
        unsupported()
    }
}
impl UiAutomationBackend for Unsupported {
    fn exists(&self, _: &MkUiPayload) -> ExecResult<bool> {
        unsupported()
    }
    fn invoke(&self, _: &MkUiPayload) -> ExecResult {
        unsupported()
    }
    fn set_value(&self, _: &MkUiPayload, _: &str) -> ExecResult {
        unsupported()
    }
    fn read_value(&self, _: &MkUiPayload) -> ExecResult<String> {
        unsupported()
    }
    fn toggle(&self, _: &MkUiPayload) -> ExecResult {
        unsupported()
    }
    fn select(&self, _: &MkUiPayload) -> ExecResult {
        unsupported()
    }
    fn focus(&self, _: &MkUiPayload) -> ExecResult {
        unsupported()
    }
}
impl LauncherBackend for Unsupported {
    fn launch_process(&self, _: &MkProcessPayload) -> ExecResult {
        unsupported()
    }
    fn command(&self, _: &str, _: Option<&str>) -> ExecResult {
        unsupported()
    }
}

#[derive(Default)]
struct ControlState {
    paused: bool,
    stopped: bool,
    active: bool,
}
#[derive(Default)]
pub struct RunControl {
    state: Mutex<ControlState>,
    wake: Condvar,
}
impl RunControl {
    pub fn reset(&self) {
        let mut s = self.state.lock().unwrap();
        *s = ControlState {
            active: true,
            ..Default::default()
        };
        self.wake.notify_all()
    }
    pub fn pause(&self) {
        self.state.lock().unwrap().paused = true;
        self.wake.notify_all()
    }
    pub fn resume(&self) {
        self.state.lock().unwrap().paused = false;
        self.wake.notify_all()
    }
    pub fn stop(&self) {
        self.state.lock().unwrap().stopped = true;
        self.wake.notify_all()
    }
    pub fn is_active(&self) -> bool {
        self.state.lock().unwrap().active
    }
    pub fn checkpoint(&self) -> ExecResult {
        let mut s = self.state.lock().unwrap();
        while s.paused && !s.stopped {
            s = self.wake.wait(s).unwrap()
        }
        if s.stopped {
            Err(ExecutionDiagnostic::new(
                DiagnosticKind::Cancelled,
                "automation stopped",
            ))
        } else {
            Ok(())
        }
    }
    pub fn wait(&self, duration: Duration) -> ExecResult {
        let mut remaining = duration;
        loop {
            self.checkpoint()?;
            if remaining.is_zero() {
                return Ok(());
            }
            let start = Instant::now();
            let s = self.state.lock().unwrap();
            let (s, timeout) = self.wake.wait_timeout(s, remaining).unwrap();
            let elapsed = start.elapsed();
            if !s.paused {
                remaining = remaining.saturating_sub(elapsed)
            }
            if s.stopped {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Cancelled,
                    "automation stopped",
                ));
            }
            if timeout.timed_out() && !s.paused {
                return Ok(());
            }
        }
    }
}
#[derive(Debug, Clone)]
pub enum ExecutionEvent {
    StepStarted(u64),
    StepFinished(u64),
    StepSkipped(u64),
    StepFailed(u64, ExecutionDiagnostic),
    Paused,
    Resumed,
}

pub struct InputCleanupGuard {
    backend: Arc<dyn InputBackend>,
    keys: Vec<MkKey>,
    buttons: Vec<MkMouseButton>,
}
impl InputCleanupGuard {
    pub fn new(backend: Arc<dyn InputBackend>) -> Self {
        Self {
            backend,
            keys: vec![],
            buttons: vec![],
        }
    }
    fn down_key(&mut self, k: &MkKey) -> ExecResult {
        self.backend.key_down(k)?;
        self.keys.push(k.clone());
        Ok(())
    }
    fn up_key(&mut self, k: &MkKey) -> ExecResult {
        self.backend.key_up(k)?;
        if let Some(i) = self.keys.iter().rposition(|x| x == k) {
            self.keys.remove(i);
        }
        Ok(())
    }
    fn down_button(&mut self, b: MkMouseButton) -> ExecResult {
        self.backend.button_down(b.clone())?;
        self.buttons.push(b);
        Ok(())
    }
    fn up_button(&mut self, b: MkMouseButton) -> ExecResult {
        self.backend.button_up(b.clone())?;
        if let Some(i) = self.buttons.iter().rposition(|x| x == &b) {
            self.buttons.remove(i);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{fake::FakeBackend, *};
    use crate::mkmacro::{MkErrorPolicy, MkMacro, MkPlayback, MkStep, compile};

    fn step(id: u64, action: MkAction) -> MkStep {
        MkStep {
            id,
            enabled: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            action,
        }
    }
    fn plan(steps: Vec<MkStep>) -> MkExecutionPlan {
        compile(&MkMacro {
            id: 7,
            name: "test".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            playback: MkPlayback::default(),
            steps,
        })
        .unwrap()
    }
    #[test]
    fn sequential_order_and_owned_input_cleanup() {
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let p = plan(vec![
            step(1, MkAction::KeyDown(MkKey::Control)),
            step(
                2,
                MkAction::Text(MkTextPayload {
                    text: "hello".into(),
                    mode: crate::mkmacro::MkTextMode::Type,
                }),
            ),
        ]);
        Executor::new(fake.clone().backends(), control)
            .execute(&p, &|_| {})
            .unwrap();
        assert_eq!(
            fake.events(),
            vec!["key_down:Control", "text:hello", "key_up:Control"]
        );
    }
    #[test]
    fn never_releases_unowned_input() {
        let fake = Arc::new(FakeBackend::default());
        {
            let _guard = InputCleanupGuard::new(fake.clone());
        }
        assert!(fake.events().is_empty());
    }
    #[test]
    fn stop_wakes_a_long_wait() {
        let control = Arc::new(RunControl::default());
        control.reset();
        let c = control.clone();
        let worker = std::thread::spawn(move || c.wait(Duration::from_secs(60)));
        std::thread::sleep(Duration::from_millis(10));
        control.stop();
        assert_eq!(
            worker.join().unwrap().unwrap_err().kind,
            DiagnosticKind::Cancelled
        );
    }
}
impl Drop for InputCleanupGuard {
    fn drop(&mut self) {
        for b in self.buttons.drain(..).rev() {
            if let Err(e) = self.backend.button_up(b) {
                tracing::error!(error=%e,"failed to release owned mouse button")
            }
        }
        for k in self.keys.drain(..).rev() {
            if let Err(e) = self.backend.key_up(&k) {
                tracing::error!(error=%e,"failed to release owned key")
            }
        }
    }
}

pub struct Executor {
    backends: Backends,
    control: Arc<RunControl>,
}
impl Executor {
    const MAX_CONTROL_TRANSITIONS: u64 = 100_000;
    pub fn new(backends: Backends, control: Arc<RunControl>) -> Self {
        Self { backends, control }
    }
    pub fn execute(&self, plan: &MkExecutionPlan, observe: &dyn Fn(ExecutionEvent)) -> ExecResult {
        let mut guard = InputCleanupGuard::new(self.backends.input.clone());
        let mut vars = RuntimeVariables::new();
        let mut pc = 0;
        let mut loops: HashMap<usize, u32> = HashMap::new();
        let mut transitions = 0u64;
        vars.insert("macro.id".into(), MkValue::Number(plan.macro_id as f64));
        vars.insert("last_action_success".into(), MkValue::Boolean(true));
        while pc < plan.instructions.len() {
            self.control.checkpoint()?;
            transitions += 1;
            if transitions > Self::MAX_CONTROL_TRANSITIONS {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::IterationLimit,
                    "control-flow safety limit (100000 transitions) exceeded",
                )
                .context("limit", Self::MAX_CONTROL_TRANSITIONS.to_string()));
            }
            let ins = &plan.instructions[pc];
            let step = &ins.step;
            if !step.enabled {
                observe(ExecutionEvent::StepSkipped(step.id));
                pc += 1;
                continue;
            }
            observe(ExecutionEvent::StepStarted(step.id));
            vars.insert("step.id".into(), MkValue::Number(step.id as f64));
            let mut final_error = None;
            for repetition in 0..step.repeat {
                vars.insert("iteration".into(), MkValue::Number(repetition as f64));
                let attempts = match (&step.action.is_structural(), &step.on_error) {
                    (true, _) => 1,
                    (_, super::MkErrorPolicy::Retry(r)) => r.attempts.max(1),
                    _ => 1,
                };
                for attempt in 1..=attempts {
                    tracing::debug!(
                        macro_id = plan.macro_id,
                        step_id = step.id,
                        attempt,
                        "executing macro step"
                    );
                    match self.action(&step.action, &mut vars, &mut guard) {
                        Ok(()) => {
                            vars.insert("last_action_success".into(), MkValue::Boolean(true));
                            final_error = None;
                            break;
                        }
                        Err(e) => {
                            let e = e
                                .context("backend_operation", action_name(&step.action))
                                .context("attempt", attempt.to_string())
                                .context("attempts_exhausted", (attempt == attempts).to_string());
                            vars.insert("last_action_success".into(), MkValue::Boolean(false));
                            tracing::warn!(macro_id=plan.macro_id,step_id=step.id,attempt,error=%e,"macro step attempt failed");
                            final_error = Some(e);
                            if attempt < attempts {
                                if let super::MkErrorPolicy::Retry(r) = &step.on_error {
                                    self.control.wait(Duration::from_millis(r.delay_ms))?
                                }
                            }
                        }
                    }
                }
                if final_error.is_some() {
                    break;
                }
                if step.delay_after_ms > 0 {
                    self.control
                        .wait(Duration::from_millis(step.delay_after_ms))?
                }
            }
            if let Some(e) = final_error {
                observe(ExecutionEvent::StepFailed(step.id, e.clone()));
                if !matches!(step.on_error, super::MkErrorPolicy::Continue) {
                    return Err(e);
                }
            } else {
                observe(ExecutionEvent::StepFinished(step.id))
            }
            pc = match (&step.action, &ins.jump) {
                (MkAction::If(c) | MkAction::WhileStart { condition: c }, Jump::IfFalse(to)) => {
                    self.control.checkpoint()?;
                    if self.condition(c, &mut vars)? {
                        pc + 1
                    } else {
                        *to
                    }
                }
                (_, Jump::To(to) | Jump::Break(to) | Jump::Continue(to)) => *to,
                (MkAction::RepeatStart { count }, Jump::RepeatBegin { exit }) if *count == 0 => {
                    *exit
                }
                (MkAction::RepeatStart { count }, _) => {
                    loops.insert(pc, *count);
                    pc + 1
                }
                (_, Jump::RepeatEnd { start, exit }) => {
                    let entry = loops.entry(start.saturating_sub(1)).or_default();
                    if *entry > 1 {
                        *entry -= 1;
                        *start
                    } else {
                        loops.remove(&start.saturating_sub(1));
                        *exit
                    }
                }
                (_, Jump::WhileEnd { condition }) => *condition,
                _ => pc + 1,
            };
        }
        self.control.state.lock().unwrap().active = false;
        Ok(())
    }
    fn action(
        &self,
        a: &MkAction,
        v: &mut RuntimeVariables,
        g: &mut InputCleanupGuard,
    ) -> ExecResult {
        match a {
            MkAction::KeyDown(k) => g.down_key(k),
            MkAction::KeyUp(k) => g.up_key(k),
            MkAction::KeyPress(k) => {
                g.down_key(k)?;
                g.up_key(k)
            }
            MkAction::Hotkey(keys) => {
                for k in keys {
                    g.down_key(k)?
                }
                for k in keys.iter().rev() {
                    g.up_key(k)?
                }
                Ok(())
            }
            MkAction::Text(p) => self.backends.input.text(p),
            MkAction::MouseMove(t) => self.move_to(t, v),
            MkAction::MouseClick(p) => {
                let point = self.backends.screen.resolve(&p.target, v)?;
                set_point(v, "last_point", point);
                self.backends.input.move_mouse(point)?;
                set_point(v, "mouse", point);
                for _ in 0..p.clicks {
                    g.down_button(p.button.clone())?;
                    g.up_button(p.button.clone())?
                }
                Ok(())
            }
            MkAction::MouseDown(b) => g.down_button(b.clone()),
            MkAction::MouseUp(b) => g.up_button(b.clone()),
            MkAction::MouseScroll { i32_delta } => self.backends.input.scroll(*i32_delta),
            MkAction::Delay { milliseconds } => {
                self.control.wait(Duration::from_millis(*milliseconds))
            }
            MkAction::Process(p) => self.backends.launcher.launch_process(p),
            MkAction::LauncherCommand { command, args } => {
                self.backends.launcher.command(command, args.as_deref())
            }
            MkAction::WindowActivate(p) => self.backends.window.activate(p),
            MkAction::WindowClose(m) => self.backends.window.close(m),
            MkAction::WindowWait(p) => self.wait_condition(
                &MkCondition::WindowExists {
                    matcher: p.matcher.clone(),
                },
                p.wait.as_ref().unwrap_or(&MkWaitOptions {
                    timeout_ms: 0,
                    poll_interval_ms: 10,
                }),
                v,
            ),
            MkAction::WaitUntil { condition, wait } => self.wait_condition(condition, wait, v),
            MkAction::SetVariable { name, value } => {
                v.insert(name.clone(), value.clone());
                Ok(())
            }
            MkAction::UnsetVariable { name } => {
                v.remove(name);
                Ok(())
            }
            MkAction::ImageFind(p) => self.wait_image(p, v).map(|_| ()),
            MkAction::ImageClick(p) => {
                let pt = self.wait_image(p, v)?;
                self.backends.input.move_mouse(pt)?;
                set_point(v, "mouse", pt);
                set_point(v, "last_point", pt);
                g.down_button(MkMouseButton::Left)?;
                g.up_button(MkMouseButton::Left)
            }
            MkAction::PixelCheck {
                target,
                color,
                tolerance,
            } => {
                let matched = self
                    .backends
                    .screen
                    .pixel_matches(target, color, *tolerance, v)?;
                v.insert("last_pixel_result".into(), MkValue::Boolean(matched));
                v.insert("last_pixel_found".into(), MkValue::Boolean(matched));
                if matched {
                    Ok(())
                } else {
                    Err(ExecutionDiagnostic::new(
                        DiagnosticKind::TargetNotFound,
                        "pixel did not match",
                    ))
                }
            }
            MkAction::UiInvoke(p) => self.backends.uia.invoke(p),
            MkAction::UiSetValue { target, value } => self.backends.uia.set_value(target, value),
            MkAction::UiReadValue { target, variable } => {
                let value = self.backends.uia.read_value(target)?;
                v.insert(variable.clone(), MkValue::String(value));
                Ok(())
            }
            MkAction::UiToggle(p) => self.backends.uia.toggle(p),
            MkAction::UiSelect(p) => self.backends.uia.select(p),
            MkAction::UiFocus(p) => self.backends.uia.focus(p),
            MkAction::UiWait(p) => self.wait_until(
                p.wait.as_ref().unwrap_or(&MkWaitOptions {
                    timeout_ms: 0,
                    poll_interval_ms: 10,
                }),
                || self.backends.uia.exists(p),
            ),
            _ => Ok(()),
        }
    }
    fn move_to(&self, target: &MkCoordinateTarget, v: &mut RuntimeVariables) -> ExecResult {
        let point = self.backends.screen.resolve(target, v)?;
        self.backends.input.move_mouse(point)?;
        set_point(v, "mouse", point);
        set_point(v, "last_point", point);
        Ok(())
    }
    fn wait_image(&self, p: &MkImagePayload, v: &mut RuntimeVariables) -> ExecResult<MkPoint> {
        self.wait_condition(
            &MkCondition::ImageResult {
                asset_id: p.asset_id,
                found: true,
            },
            &p.wait,
            v,
        )?;
        let found = self.backends.screen.image_found(p.asset_id, p.confidence)?;
        v.insert(
            "last_image_result".into(),
            MkValue::Boolean(found.is_some()),
        );
        v.insert("last_image_found".into(), MkValue::Boolean(found.is_some()));
        if let Some(point) = found {
            set_point(v, "last_image", point);
            v.insert("last_image_x".into(), MkValue::Number(point.x.into()));
            v.insert("last_image_y".into(), MkValue::Number(point.y.into()));
        }
        found.ok_or_else(|| {
            ExecutionDiagnostic::new(DiagnosticKind::TargetNotFound, "image not found")
        })
    }
    fn wait_condition(
        &self,
        condition: &MkCondition,
        o: &MkWaitOptions,
        v: &mut RuntimeVariables,
    ) -> ExecResult {
        let started = Instant::now();
        let mut polls = 0u64;
        loop {
            self.control.checkpoint()?;
            polls += 1;
            if self.condition(condition, v)? {
                return Ok(());
            }
            let elapsed = started.elapsed();
            if elapsed >= Duration::from_millis(o.timeout_ms) {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Timeout,
                    format!("condition timed out after {} ms", o.timeout_ms),
                )
                .context("timeout_ms", o.timeout_ms.to_string())
                .context("poll_interval_ms", o.poll_interval_ms.to_string())
                .context("polls", polls.to_string()));
            }
            self.control.wait(
                Duration::from_millis(o.poll_interval_ms.max(1))
                    .min(Duration::from_millis(o.timeout_ms).saturating_sub(elapsed)),
            )?;
        }
    }
    fn wait_until(
        &self,
        o: &MkWaitOptions,
        mut poll: impl FnMut() -> ExecResult<bool>,
    ) -> ExecResult {
        let start = Instant::now();
        loop {
            self.control.checkpoint()?;
            if poll()? {
                return Ok(());
            }
            if start.elapsed() >= Duration::from_millis(o.timeout_ms) {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Timeout,
                    format!("condition timed out after {} ms", o.timeout_ms),
                ));
            }
            self.control
                .wait(Duration::from_millis(o.poll_interval_ms.max(1)))?
        }
    }
    fn condition(&self, c: &MkCondition, v: &mut RuntimeVariables) -> ExecResult<bool> {
        match c {
            MkCondition::Variable { name, op, value } => {
                compare(v.get(name).unwrap_or(&MkValue::Null), op, value)
            }
            MkCondition::WindowExists { matcher } => {
                let x = self
                    .backends
                    .window
                    .exists(matcher)
                    .map_err(|e| e.context("matcher", format!("{matcher:?}")))?;
                v.insert("last_window_result".into(), MkValue::Boolean(x));
                Ok(x)
            }
            MkCondition::WindowActive { matcher } => {
                let x = self
                    .backends
                    .window
                    .is_active(matcher)
                    .map_err(|e| e.context("matcher", format!("{matcher:?}")))?;
                v.insert("last_window_result".into(), MkValue::Boolean(x));
                Ok(x)
            }
            MkCondition::ImageResult { asset_id, found } => {
                let point = self.backends.screen.image_found(*asset_id, 0.0)?;
                v.insert(
                    "last_image_result".into(),
                    MkValue::Boolean(point.is_some()),
                );
                v.insert("last_image_found".into(), MkValue::Boolean(point.is_some()));
                if let Some(p) = point {
                    set_point(v, "last_image", p);
                    v.insert("last_image_x".into(), MkValue::Number(p.x.into()));
                    v.insert("last_image_y".into(), MkValue::Number(p.y.into()));
                };
                Ok(point.is_some() == *found)
            }
            MkCondition::PixelResult {
                target,
                color,
                tolerance,
            } => {
                let x = self
                    .backends
                    .screen
                    .pixel_matches(target, color, *tolerance, v)?;
                v.insert("last_pixel_result".into(), MkValue::Boolean(x));
                v.insert("last_pixel_found".into(), MkValue::Boolean(x));
                Ok(x)
            }
            MkCondition::All { conditions } => {
                for c in conditions {
                    if !self.condition(c, v)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            MkCondition::Any { conditions } => {
                for c in conditions {
                    if self.condition(c, v)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            MkCondition::Not { condition } => Ok(!self.condition(condition, v)?),
        }
    }
}
fn action_name(a: &MkAction) -> &'static str {
    match a {
        MkAction::KeyDown(_)
        | MkAction::KeyUp(_)
        | MkAction::KeyPress(_)
        | MkAction::Hotkey(_)
        | MkAction::Text(_) => "SendInput",
        MkAction::MouseMove(_)
        | MkAction::MouseClick(_)
        | MkAction::MouseDown(_)
        | MkAction::MouseUp(_)
        | MkAction::MouseScroll { .. } => "SendInput",
        MkAction::WindowActivate(_) | MkAction::WindowClose(_) | MkAction::WindowWait(_) => {
            "window"
        }
        MkAction::ImageFind(_) | MkAction::ImageClick(_) | MkAction::PixelCheck { .. } => "screen",
        MkAction::UiInvoke(_)
        | MkAction::UiSetValue { .. }
        | MkAction::UiReadValue { .. }
        | MkAction::UiToggle(_)
        | MkAction::UiSelect(_)
        | MkAction::UiFocus(_)
        | MkAction::UiWait(_) => "UIAutomation",
        MkAction::Process(_) | MkAction::LauncherCommand { .. } => "launcher",
        MkAction::WaitUntil { .. } => "condition_evaluator",
        _ => "runtime",
    }
}
fn set_point(v: &mut RuntimeVariables, prefix: &str, p: MkPoint) {
    v.insert(format!("{prefix}.x"), MkValue::Number(p.x as f64));
    v.insert(format!("{prefix}.y"), MkValue::Number(p.y as f64));
}
pub fn compare(a: &MkValue, op: &MkCompareOp, b: &MkValue) -> ExecResult<bool> {
    match op {
        MkCompareOp::Eq | MkCompareOp::NotEq => {
            if std::mem::discriminant(a) != std::mem::discriminant(b) {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::TypeMismatch,
                    "mixed-type comparison is not allowed",
                ));
            }
            Ok(if matches!(op, MkCompareOp::Eq) {
                a == b
            } else {
                a != b
            })
        }
        MkCompareOp::Less
        | MkCompareOp::LessOrEq
        | MkCompareOp::Greater
        | MkCompareOp::GreaterOrEq => {
            let (MkValue::Number(a), MkValue::Number(b)) = (a, b) else {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::TypeMismatch,
                    "ordering requires two numbers",
                ));
            };
            Ok(match op {
                MkCompareOp::Less => a < b,
                MkCompareOp::LessOrEq => a <= b,
                MkCompareOp::Greater => a > b,
                _ => a >= b,
            })
        }
        MkCompareOp::Contains
        | MkCompareOp::StartsWith
        | MkCompareOp::EndsWith
        | MkCompareOp::Regex => {
            let (MkValue::String(a), MkValue::String(b)) = (a, b) else {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::TypeMismatch,
                    "string operator requires two strings",
                ));
            };
            Ok(match op {
                MkCompareOp::Contains => a.contains(b),
                MkCompareOp::StartsWith => a.starts_with(b),
                MkCompareOp::EndsWith => a.ends_with(b),
                _ => regex::Regex::new(b)
                    .map_err(|e| {
                        ExecutionDiagnostic::new(
                            DiagnosticKind::InvalidRegex,
                            format!("invalid regular expression: {e}"),
                        )
                    })?
                    .is_match(a),
            })
        }
    }
}

/// Configurable synchronized fake implementing every effect boundary.
pub mod fake {
    use super::*;
    #[derive(Default)]
    pub struct FakeBackend {
        pub events: Mutex<Vec<String>>,
        pub failures: Mutex<HashMap<String, ExecutionDiagnostic>>,
        pub conditions: Mutex<HashMap<String, bool>>,
    }
    impl FakeBackend {
        pub fn events(&self) -> Vec<String> {
            self.events.lock().unwrap().clone()
        }
        pub fn fail(&self, name: &str, d: ExecutionDiagnostic) {
            self.failures.lock().unwrap().insert(name.into(), d);
        }
        fn event(&self, e: String) -> ExecResult {
            self.events.lock().unwrap().push(e.clone());
            if let Some(d) = self.failures.lock().unwrap().get(&e).cloned() {
                Err(d)
            } else {
                Ok(())
            }
        }
        pub fn backends(self: Arc<Self>) -> Backends {
            Backends {
                input: self.clone(),
                window: self.clone(),
                screen: self.clone(),
                uia: self.clone(),
                launcher: self,
            }
        }
    }
    impl InputBackend for FakeBackend {
        fn key_down(&self, k: &MkKey) -> ExecResult {
            self.event(format!("key_down:{k:?}"))
        }
        fn key_up(&self, k: &MkKey) -> ExecResult {
            self.event(format!("key_up:{k:?}"))
        }
        fn button_down(&self, b: MkMouseButton) -> ExecResult {
            self.event(format!("button_down:{b:?}"))
        }
        fn button_up(&self, b: MkMouseButton) -> ExecResult {
            self.event(format!("button_up:{b:?}"))
        }
        fn move_mouse(&self, p: MkPoint) -> ExecResult {
            self.event(format!("move:{},{}", p.x, p.y))
        }
        fn scroll(&self, d: i32) -> ExecResult {
            self.event(format!("scroll:{d}"))
        }
        fn text(&self, p: &MkTextPayload) -> ExecResult {
            self.event(format!("text:{}", p.text))
        }
    }
    impl WindowBackend for FakeBackend {
        fn exists(&self, _: &MkWindowMatcher) -> ExecResult<bool> {
            Ok(*self
                .conditions
                .lock()
                .unwrap()
                .get("window_exists")
                .unwrap_or(&false))
        }
        fn is_active(&self, _: &MkWindowMatcher) -> ExecResult<bool> {
            Ok(*self
                .conditions
                .lock()
                .unwrap()
                .get("window_active")
                .unwrap_or(&false))
        }
        fn activate(&self, _: &MkWindowPayload) -> ExecResult {
            self.event("window_activate".into())
        }
        fn close(&self, _: &MkWindowMatcher) -> ExecResult {
            self.event("window_close".into())
        }
    }
    impl ScreenBackend for FakeBackend {
        fn resolve(&self, t: &MkCoordinateTarget, v: &RuntimeVariables) -> ExecResult<MkPoint> {
            match t {
                MkCoordinateTarget::Screen { point }
                | MkCoordinateTarget::ActiveWindow { point } => Ok(*point),
                MkCoordinateTarget::Variable { name } => match v.get(name) {
                    Some(MkValue::Point(p)) => Ok(*p),
                    _ => Err(ExecutionDiagnostic::new(
                        DiagnosticKind::InvalidTarget,
                        "point variable is missing",
                    )),
                },
                _ => Err(ExecutionDiagnostic::new(
                    DiagnosticKind::TargetNotFound,
                    "image target not found",
                )),
            }
        }
        fn image_found(&self, _: u64, _: f32) -> ExecResult<Option<MkPoint>> {
            Ok(self
                .conditions
                .lock()
                .unwrap()
                .get("image")
                .copied()
                .unwrap_or(false)
                .then_some(MkPoint { x: 1, y: 1 }))
        }
        fn pixel_matches(
            &self,
            _: &MkCoordinateTarget,
            _: &str,
            _: u8,
            _: &RuntimeVariables,
        ) -> ExecResult<bool> {
            Ok(*self
                .conditions
                .lock()
                .unwrap()
                .get("pixel")
                .unwrap_or(&false))
        }
    }
    impl UiAutomationBackend for FakeBackend {
        fn exists(&self, _: &MkUiPayload) -> ExecResult<bool> {
            Ok(*self.conditions.lock().unwrap().get("uia").unwrap_or(&false))
        }
        fn invoke(&self, _: &MkUiPayload) -> ExecResult {
            self.event("uia_invoke".into())
        }
        fn set_value(&self, _: &MkUiPayload, v: &str) -> ExecResult {
            self.event(format!("uia_value:{v}"))
        }
    }
    impl LauncherBackend for FakeBackend {
        fn launch_process(&self, p: &MkProcessPayload) -> ExecResult {
            self.event(format!("process:{}", p.program))
        }
        fn command(&self, c: &str, _: Option<&str>) -> ExecResult {
            self.event(format!("command:{c}"))
        }
    }
}

#[cfg(test)]
mod phase_d_tests {
    use super::{fake::FakeBackend, *};
    use crate::mkmacro::{
        MkErrorPolicy, MkMacro, MkPlayback, MkRetry, MkStep, MkTextMode, compile,
    };

    fn s(id: u64, action: MkAction) -> MkStep {
        MkStep {
            id,
            enabled: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            action,
        }
    }
    fn plan(steps: Vec<MkStep>) -> MkExecutionPlan {
        compile(&MkMacro {
            id: 9,
            name: "flow".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            playback: MkPlayback::default(),
            steps,
        })
        .unwrap()
    }
    fn text(value: &str) -> MkAction {
        MkAction::Text(MkTextPayload {
            text: value.into(),
            mode: MkTextMode::Type,
        })
    }
    fn run(steps: Vec<MkStep>, fake: Arc<FakeBackend>) -> ExecResult {
        let c = Arc::new(RunControl::default());
        c.reset();
        Executor::new(fake.backends(), c).execute(&plan(steps), &|_| {})
    }

    #[test]
    fn typed_comparisons_cover_every_operator_and_bad_regex() {
        use MkCompareOp::*;
        assert!(compare(&MkValue::Number(1.0), &Less, &MkValue::Number(2.0)).unwrap());
        assert!(compare(&MkValue::Number(2.0), &LessOrEq, &MkValue::Number(2.0)).unwrap());
        assert!(compare(&MkValue::Number(3.0), &Greater, &MkValue::Number(2.0)).unwrap());
        assert!(compare(&MkValue::Number(3.0), &GreaterOrEq, &MkValue::Number(3.0)).unwrap());
        assert!(
            compare(
                &MkValue::String("abcd".into()),
                &Contains,
                &MkValue::String("bc".into())
            )
            .unwrap()
        );
        assert!(
            compare(
                &MkValue::String("abcd".into()),
                &StartsWith,
                &MkValue::String("ab".into())
            )
            .unwrap()
        );
        assert!(
            compare(
                &MkValue::String("abcd".into()),
                &EndsWith,
                &MkValue::String("cd".into())
            )
            .unwrap()
        );
        assert!(
            compare(
                &MkValue::String("abcd".into()),
                &Regex,
                &MkValue::String("^a.*d$".into())
            )
            .unwrap()
        );
        assert!(compare(&MkValue::Boolean(true), &Eq, &MkValue::Boolean(true)).unwrap());
        assert!(compare(&MkValue::Boolean(true), &NotEq, &MkValue::Boolean(false)).unwrap());
        assert_eq!(
            compare(&MkValue::Number(1.0), &Eq, &MkValue::String("1".into()))
                .unwrap_err()
                .kind,
            DiagnosticKind::TypeMismatch
        );
        assert_eq!(
            compare(
                &MkValue::String("x".into()),
                &Regex,
                &MkValue::String("[".into())
            )
            .unwrap_err()
            .kind,
            DiagnosticKind::InvalidRegex
        );
    }

    #[test]
    fn nested_flow_zero_repeat_break_and_continue_have_correct_destinations() {
        let f = Arc::new(FakeBackend::default());
        run(
            vec![
                s(1, MkAction::RepeatStart { count: 0 }),
                s(2, text("never")),
                s(3, MkAction::RepeatEnd),
                s(4, MkAction::RepeatStart { count: 3 }),
                s(5, text("once")),
                s(6, MkAction::Break),
                s(7, text("never2")),
                s(8, MkAction::RepeatEnd),
                s(9, MkAction::RepeatStart { count: 2 }),
                s(10, MkAction::Continue),
                s(11, text("never3")),
                s(12, MkAction::RepeatEnd),
                s(
                    16,
                    MkAction::SetVariable {
                        name: "x".into(),
                        value: MkValue::Boolean(false),
                    },
                ),
                s(
                    13,
                    MkAction::If(MkCondition::Not {
                        condition: Box::new(MkCondition::Variable {
                            name: "x".into(),
                            op: MkCompareOp::Eq,
                            value: MkValue::Boolean(true),
                        }),
                    }),
                ),
                s(14, text("nested")),
                s(15, MkAction::EndIf),
            ],
            f.clone(),
        )
        .unwrap();
        assert_eq!(f.events(), vec!["text:once", "text:nested"]);
    }

    #[test]
    fn all_any_short_circuit_and_while_false_exit() {
        let f = Arc::new(FakeBackend::default());
        let false_c = MkCondition::Variable {
            name: "x".into(),
            op: MkCompareOp::Eq,
            value: MkValue::Boolean(true),
        };
        run(
            vec![
                s(
                    1,
                    MkAction::SetVariable {
                        name: "x".into(),
                        value: MkValue::Boolean(false),
                    },
                ),
                s(
                    2,
                    MkAction::If(MkCondition::All {
                        conditions: vec![
                            false_c.clone(),
                            MkCondition::Variable {
                                name: "x".into(),
                                // Valid at compile time, but would be a runtime type error if
                                // All failed to short-circuit after the false first condition.
                                op: MkCompareOp::Less,
                                value: MkValue::Number(1.0),
                            },
                        ],
                    }),
                ),
                s(3, text("never")),
                s(4, MkAction::EndIf),
                s(
                    5,
                    MkAction::If(MkCondition::Any {
                        conditions: vec![
                            MkCondition::Not {
                                condition: Box::new(false_c.clone()),
                            },
                            MkCondition::Variable {
                                name: "x".into(),
                                // Likewise, Any must not evaluate this after its true branch.
                                op: MkCompareOp::Less,
                                value: MkValue::Number(1.0),
                            },
                        ],
                    }),
                ),
                s(6, text("yes")),
                s(7, MkAction::EndIf),
                s(8, MkAction::WhileStart { condition: false_c }),
                s(9, text("never2")),
                s(10, MkAction::WhileEnd),
            ],
            f.clone(),
        )
        .unwrap();
        assert_eq!(f.events(), vec!["text:yes"]);
    }

    #[test]
    fn wait_until_immediate_timeout_and_retry_context() {
        let f = Arc::new(FakeBackend::default());
        f.conditions
            .lock()
            .unwrap()
            .insert("window_exists".into(), true);
        let w = MkWaitOptions {
            timeout_ms: 5,
            poll_interval_ms: 1,
        };
        run(
            vec![s(
                1,
                MkAction::WaitUntil {
                    condition: MkCondition::WindowExists {
                        matcher: MkWindowMatcher {
                            title: Some("x".into()),
                            title_regex: None,
                            process: None,
                            class: None,
                        },
                    },
                    wait: w.clone(),
                },
            )],
            f.clone(),
        )
        .unwrap();
        f.conditions
            .lock()
            .unwrap()
            .insert("window_exists".into(), false);
        let mut step = s(
            2,
            MkAction::WaitUntil {
                condition: MkCondition::WindowExists {
                    matcher: MkWindowMatcher {
                        title: Some("x".into()),
                        title_regex: None,
                        process: None,
                        class: None,
                    },
                },
                wait: w,
            },
        );
        step.on_error = MkErrorPolicy::Retry(MkRetry {
            attempts: 2,
            delay_ms: 0,
        });
        let e = run(vec![step], f).unwrap_err();
        assert_eq!(e.kind, DiagnosticKind::Timeout);
        assert_eq!(
            e.context.get("attempts_exhausted").map(String::as_str),
            Some("true")
        );
        assert!(e.context.contains_key("poll_interval_ms"));
    }
}
