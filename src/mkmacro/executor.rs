//! Platform-neutral plan executor and injectable effect boundaries.
use super::{
    Jump, MkAction, MkCompareOp, MkCondition, MkCoordinateTarget, MkExecutionPlan, MkImagePayload,
    MkKey, MkMacroStore, MkMouseButton, MkPlayback, MkPoint, MkProcessPayload,
    MkPromptInputPayload, MkTextPayload, MkUiPayload, MkValue, MkWaitOptions, MkWindowMatcher,
    MkWindowMoveResizePayload, MkWindowPayload, MkWindowState, PromptBackend, PromptRequest,
    PromptResponse, RuntimeVariables, interpolate,
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
    /// Reports the physical Escape key state so playback can be cancelled.
    fn escape_pressed(&self) -> bool {
        false
    }
    fn key_down(&self, key: &MkKey) -> ExecResult;
    fn key_up(&self, key: &MkKey) -> ExecResult;
    fn button_down(&self, button: MkMouseButton) -> ExecResult;
    fn button_up(&self, button: MkMouseButton) -> ExecResult;
    fn move_mouse(&self, point: MkPoint) -> ExecResult;
    fn cursor_position(&self) -> ExecResult<MkPoint>;
    fn scroll(&self, delta: i32) -> ExecResult;
    fn text(&self, payload: &MkTextPayload) -> ExecResult;
}
pub trait WindowBackend: Send + Sync {
    fn exists(&self, m: &MkWindowMatcher) -> ExecResult<bool>;
    fn is_active(&self, m: &MkWindowMatcher) -> ExecResult<bool>;
    fn activate(&self, p: &MkWindowPayload) -> ExecResult;
    fn close(&self, m: &MkWindowMatcher) -> ExecResult;
    fn move_resize(&self, p: &MkWindowMoveResizePayload) -> ExecResult;
    fn set_state(&self, m: &MkWindowMatcher, state: MkWindowState) -> ExecResult;
}
pub trait ScreenBackend: Send + Sync {
    fn resolve(
        &self,
        target: &MkCoordinateTarget,
        variables: &RuntimeVariables,
    ) -> ExecResult<MkPoint>;
    /// Applies platform desktop bounds to a fully resolved and randomized point.
    fn finalize_point(&self, point: MkPoint) -> ExecResult<MkPoint> {
        Ok(point)
    }
    fn find_image(&self, macro_id: u64, payload: &MkImagePayload) -> ExecResult<Option<MkPoint>>;
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
pub trait ClipboardBackend: Send + Sync {
    fn set_text(&self, text: &str) -> ExecResult;
}
#[derive(Clone)]
pub struct Backends {
    pub input: Arc<dyn InputBackend>,
    pub window: Arc<dyn WindowBackend>,
    pub screen: Arc<dyn ScreenBackend>,
    pub uia: Arc<dyn UiAutomationBackend>,
    pub launcher: Arc<dyn LauncherBackend>,
    pub prompt: Arc<dyn PromptBackend>,
    pub clipboard: Arc<dyn ClipboardBackend>,
}
impl Backends {
    pub fn unsupported() -> Self {
        let input = Arc::new(Unsupported { backend: "input" });
        let window = Arc::new(Unsupported { backend: "window" });
        let screen = Arc::new(Unsupported { backend: "screen" });
        let uia = Arc::new(Unsupported {
            backend: "UI Automation",
        });
        let launcher = Arc::new(Unsupported {
            backend: "launcher",
        });
        let prompt = Arc::new(Unsupported { backend: "prompt" });
        let clipboard = Arc::new(Unsupported {
            backend: "clipboard",
        });
        Self {
            input,
            window,
            screen,
            uia,
            launcher,
            prompt,
            clipboard,
        }
    }

    /// Backends used by the installed application. Live input remains an explicit
    /// production-only capability; tests should use [`MacroRuntime::new`] instead.
    pub fn production() -> Self {
        production_backends()
    }
}
struct Unsupported {
    backend: &'static str,
}
fn unsupported_context<T>(backend: &'static str, action: &'static str) -> ExecResult<T> {
    Err(ExecutionDiagnostic::new(
        DiagnosticKind::UnsupportedOperation,
        "This action is not available yet",
    )
    .context("backend", backend)
    .context("action", action))
}
fn unsupported<T>() -> ExecResult<T> {
    unsupported_context("automation", "unknown")
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
    fn cursor_position(&self) -> ExecResult<MkPoint> {
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
    fn move_resize(&self, _: &MkWindowMoveResizePayload) -> ExecResult {
        unsupported()
    }
    fn set_state(&self, _: &MkWindowMatcher, _: MkWindowState) -> ExecResult {
        unsupported()
    }
}
impl ScreenBackend for Unsupported {
    fn resolve(&self, _: &MkCoordinateTarget, _: &RuntimeVariables) -> ExecResult<MkPoint> {
        unsupported_context(self.backend, "resolve coordinates")
    }
    fn find_image(&self, _: u64, _: &MkImagePayload) -> ExecResult<Option<MkPoint>> {
        unsupported_context(self.backend, "find image")
    }
    fn pixel_matches(
        &self,
        _: &MkCoordinateTarget,
        _: &str,
        _: u8,
        _: &RuntimeVariables,
    ) -> ExecResult<bool> {
        unsupported_context(self.backend, "match pixel")
    }
}
impl UiAutomationBackend for Unsupported {
    fn exists(&self, _: &MkUiPayload) -> ExecResult<bool> {
        uia_unavailable("exists")
    }
    fn invoke(&self, _: &MkUiPayload) -> ExecResult {
        uia_unavailable("invoke")
    }
    fn set_value(&self, _: &MkUiPayload, _: &str) -> ExecResult {
        uia_unavailable("set value")
    }
    fn read_value(&self, _: &MkUiPayload) -> ExecResult<String> {
        uia_unavailable("read value")
    }
    fn toggle(&self, _: &MkUiPayload) -> ExecResult {
        uia_unavailable("toggle")
    }
    fn select(&self, _: &MkUiPayload) -> ExecResult {
        uia_unavailable("select")
    }
    fn focus(&self, _: &MkUiPayload) -> ExecResult {
        uia_unavailable("focus")
    }
}
fn uia_unavailable<T>(action: &'static str) -> ExecResult<T> {
    Err(ExecutionDiagnostic::new(
        DiagnosticKind::UnsupportedOperation,
        "UI Automation backend is not available yet",
    )
    .context("backend", "UI Automation")
    .context("action", action))
}

#[cfg(windows)]
struct ProductionLauncher;
#[cfg(windows)]
impl LauncherBackend for ProductionLauncher {
    fn launch_process(&self, p: &MkProcessPayload) -> ExecResult {
        let mut command = std::process::Command::new(&p.program);
        command.args(&p.arguments);
        if let Some(directory) = p
            .working_directory
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            command.current_dir(directory);
        }
        let result = if p.wait {
            command.status().map(|_| ())
        } else {
            command.spawn().map(|_| ())
        };
        result.map_err(|e| {
            ExecutionDiagnostic::new(DiagnosticKind::Backend, e.to_string())
                .context("backend", "launcher")
                .context("action", p.program.clone())
        })
    }
    fn command(&self, command: &str, args: Option<&str>) -> ExecResult {
        let action = crate::actions::Action {
            label: command.to_owned(),
            desc: "Macro".into(),
            action: command.to_owned(),
            args: args.map(str::to_owned),
        };
        crate::gui::execute_action(&action).map_err(|e| {
            ExecutionDiagnostic::new(DiagnosticKind::Backend, e.to_string())
                .context("backend", "launcher")
                .context("action", command)
        })
    }
}

/// Constructs the application's effect boundaries without exposing an accidental
/// live-input default.
pub fn production_backends() -> Backends {
    let unsupported = Backends::unsupported();
    #[cfg(windows)]
    {
        let input: Arc<dyn InputBackend> = Arc::new(super::input::Win32InputBackend::system(
            super::input::LiveInputOptIn::production(),
        ));
        Backends {
            input,
            window: Arc::new(super::windows::Win32WindowBackend),
            screen: Arc::new(super::screen::WindowsScreenBackend::system()),
            uia: unsupported.uia,
            launcher: Arc::new(ProductionLauncher),
            prompt: super::prompt::production_prompt_broker(),
            clipboard: Arc::new(ProductionClipboard),
        }
    }
    #[cfg(not(windows))]
    {
        Backends {
            prompt: super::prompt::production_prompt_broker(),
            clipboard: Arc::new(ProductionClipboard),
            ..unsupported
        }
    }
}

/// Store-aware production wiring used by the macro runtime. Keeping this
/// separate preserves callers that only need non-visual production boundaries.
pub fn production_backends_with_store(store: Arc<MkMacroStore>) -> Backends {
    let mut backends = production_backends();
    #[cfg(windows)]
    {
        backends.screen = Arc::new(super::screen::WindowsScreenBackend::production(store));
    }
    #[cfg(not(windows))]
    let _ = store;
    backends
}
impl LauncherBackend for Unsupported {
    fn launch_process(&self, _: &MkProcessPayload) -> ExecResult {
        unsupported()
    }
    fn command(&self, _: &str, _: Option<&str>) -> ExecResult {
        unsupported()
    }
}
impl PromptBackend for Unsupported {
    fn prompt(&self, _: PromptRequest, _: &RunControl) -> ExecResult<PromptResponse> {
        unsupported_context(self.backend, "prompt")
    }
}
impl ClipboardBackend for Unsupported {
    fn set_text(&self, _: &str) -> ExecResult {
        unsupported_context(self.backend, "set text")
    }
}
struct ProductionClipboard;
impl ClipboardBackend for ProductionClipboard {
    fn set_text(&self, text: &str) -> ExecResult {
        arboard::Clipboard::new()
            .and_then(|mut c| c.set_text(text.to_owned()))
            .map_err(|e| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    format!("failed to copy prompt result: {e}"),
                )
                .context("backend", "clipboard")
            })
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
    pub fn is_stopped(&self) -> bool {
        self.state.lock().unwrap().stopped
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

/// Clears runtime activity on every executor exit path, including cancellation
/// and backend failures. Without this guard, queued Pause/Resume commands can
/// mistake a finished run for an active one and overwrite its terminal state.
struct RunActivityGuard<'a>(&'a RunControl);
impl Drop for RunActivityGuard<'_> {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap();
        state.active = false;
        self.0.wake.notify_all();
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
    fn hotkey(&mut self, keys: &[MkKey]) -> ExecResult {
        let mut pressed = 0;
        let mut result = Ok(());
        for key in keys {
            match self.down_key(key) {
                Ok(()) => pressed += 1,
                Err(error) => {
                    result = Err(error);
                    break;
                }
            }
        }
        for key in keys[..pressed].iter().rev() {
            if let Err(error) = self.up_key(key)
                && result.is_ok()
            {
                result = Err(error);
            }
        }
        result
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
    use crate::mkmacro::{
        AlphaPolicy, MkErrorPolicy, MkMacro, MkPlayback, MkStep, ReturnPoint, SearchRegion, compile,
    };

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
    fn playback_duration_scaling_is_ceiling_and_saturating() {
        assert_eq!(scale_playback_duration(0, 50), 0);
        assert_eq!(scale_playback_duration(5, 50), 10);
        assert_eq!(scale_playback_duration(5, 100), 5);
        assert_eq!(scale_playback_duration(5, 200), 3);
        assert_eq!(scale_playback_duration(1, 1000), 1);
        assert_eq!(scale_playback_duration(u64::MAX, 50), u64::MAX);
    }

    #[test]
    fn sampled_delay_addition_is_deterministic_and_saturating() {
        assert_eq!(add_sampled_random_delay(10, 0), 10);
        assert_eq!(add_sampled_random_delay(10, 7), 17);
        assert_eq!(add_sampled_random_delay(u64::MAX, 1), u64::MAX);
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
    fn hotkey_cleanup_handles_every_down_failure_and_continues_after_up_failure() {
        let keys = [MkKey::Meta, MkKey::Control, MkKey::Left];
        for (position, failed_event) in ["key_down:Meta", "key_down:Control", "key_down:Left"]
            .into_iter()
            .enumerate()
        {
            let fake = Arc::new(FakeBackend::default());
            fake.fail(
                failed_event,
                ExecutionDiagnostic::new(DiagnosticKind::InputRejected, "injected"),
            );
            let mut guard = InputCleanupGuard::new(fake.clone());
            assert!(guard.hotkey(&keys).is_err());
            let expected_releases: Vec<_> = keys[..position]
                .iter()
                .rev()
                .map(|key| format!("key_up:{key:?}"))
                .collect();
            assert!(fake.events().ends_with(&expected_releases));
        }

        let fake = Arc::new(FakeBackend::default());
        fake.fail(
            "key_up:Left",
            ExecutionDiagnostic::new(DiagnosticKind::InputRejected, "injected"),
        );
        let mut guard = InputCleanupGuard::new(fake.clone());
        assert!(guard.hotkey(&keys).is_err());
        assert!(fake.events().ends_with(&[
            "key_up:Left".into(),
            "key_up:Control".into(),
            "key_up:Meta".into(),
        ]));
    }

    #[cfg(not(windows))]
    #[test]
    fn executor_rejects_virtual_desktop_without_injecting_input() {
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let error = Executor::new(fake.clone().backends(), control)
            .execute(
                &plan(vec![step(
                    1,
                    MkAction::VirtualDesktop(super::super::MkVirtualDesktopAction::Create),
                )]),
                &|_| {},
            )
            .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::UnsupportedOperation);
        assert!(fake.events().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn executor_virtual_desktop_uses_guarded_shortcut() {
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        Executor::new(fake.clone().backends(), control)
            .execute(
                &plan(vec![step(
                    1,
                    MkAction::VirtualDesktop(super::super::MkVirtualDesktopAction::Create),
                )]),
                &|_| {},
            )
            .unwrap();
        assert_eq!(
            fake.events(),
            [
                "key_down:Meta",
                "key_down:Control",
                "key_down:Character(\"D\")",
                "key_up:Character(\"D\")",
                "key_up:Control",
                "key_up:Meta",
            ]
        );
    }

    #[test]
    fn timed_recording_actions_use_smooth_move_and_drag_cleanup() {
        let fake = Arc::new(FakeBackend::default());
        let control = Arc::new(RunControl::default());
        control.reset();
        let target = |x, y| MkCoordinateTarget::Screen {
            point: MkPoint { x, y },
        };
        let p = plan(vec![
            step(
                1,
                MkAction::MouseMove(super::super::MkMouseMovePayload {
                    target: target(5, 6),
                    duration_ms: 1,
                }),
            ),
            step(
                2,
                MkAction::MouseDrag(super::super::MkMouseDragPayload {
                    from: target(5, 6),
                    to: target(9, 10),
                    button: MkMouseButton::Right,
                    duration_ms: 1,
                }),
            ),
        ]);
        Executor::new(fake.clone().backends(), control)
            .execute(&p, &|_| {})
            .unwrap();
        let events = fake.events();
        assert_eq!(events.iter().filter(|e| *e == "cursor_position").count(), 1);
        let down = events
            .iter()
            .position(|e| e == "button_down:Right")
            .unwrap();
        let up = events.iter().position(|e| e == "button_up:Right").unwrap();
        assert_eq!(
            events.iter().filter(|e| *e == "button_down:Right").count(),
            1
        );
        assert_eq!(events.iter().filter(|e| *e == "button_up:Right").count(), 1);
        assert!(down < up);

        let cancelled = Arc::new(RunControl::default());
        cancelled.reset();
        cancelled.stop();
        let error = super::super::input::drag(
            &*fake,
            &cancelled,
            MkMouseButton::Left,
            MkPoint { x: 0, y: 0 },
            MkPoint { x: 100, y: 100 },
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Cancelled);
        assert!(fake.events().ends_with(&[
            "move:0,0".into(),
            "button_down:Left".into(),
            "button_up:Left".into(),
        ]));
    }
    #[test]
    fn wait_image_records_legacy_and_asset_scoped_results_and_clears_stale_data() {
        let fake = Arc::new(FakeBackend::default());
        fake.conditions.lock().unwrap().insert("image".into(), true);
        let control = Arc::new(RunControl::default());
        control.reset();
        let executor = Executor::new(fake.clone().backends(), control);
        let payload = |asset_id| MkImagePayload {
            asset_id,
            wait: MkWaitOptions {
                timeout_ms: 0,
                poll_interval_ms: 1,
            },
            region: SearchRegion::Desktop,
            tolerance: 0,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
        };
        let mut vars = RuntimeVariables::new();
        assert_eq!(
            executor.wait_image(1, &payload(7), &mut vars).unwrap(),
            MkPoint { x: 1, y: 1 }
        );
        assert_eq!(
            executor.wait_image(1, &payload(8), &mut vars).unwrap(),
            MkPoint { x: 1, y: 1 }
        );
        for key in [
            "last_image",
            "last_image_x",
            "last_image_y",
            "last_image_result",
            "last_image_found",
            "__image.7",
            "__image.8",
        ] {
            assert!(
                vars.contains_key(key),
                "missing compatibility/runtime key {key}"
            );
        }
        assert_eq!(
            vars.get("last_image"),
            Some(&MkValue::Point(MkPoint { x: 1, y: 1 }))
        );

        fake.conditions
            .lock()
            .unwrap()
            .insert("image".into(), false);
        let error = executor.wait_image(1, &payload(7), &mut vars).unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Timeout);
        assert!(!vars.contains_key("__image.7"));
        assert!(vars.contains_key("__image.8"));
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
    #[test]
    fn cancelled_executor_clears_runtime_activity() {
        let control = Arc::new(RunControl::default());
        control.reset();
        let worker_control = control.clone();
        let worker = std::thread::spawn(move || {
            Executor::new(Arc::new(FakeBackend::default()).backends(), worker_control).execute(
                &plan(vec![step(
                    1,
                    MkAction::Delay {
                        milliseconds: 60_000,
                    },
                )]),
                &|_| {},
            )
        });
        let deadline = Instant::now() + Duration::from_secs(1);
        while !control.is_active() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        control.stop();
        assert_eq!(
            worker.join().unwrap().unwrap_err().kind,
            DiagnosticKind::Cancelled
        );
        assert!(!control.is_active());
    }

    #[test]
    fn pixel_action_updates_outputs_for_match_mismatch_and_backend_error() {
        let fake = Arc::new(FakeBackend::default());
        let executor = Executor::new(fake.clone().backends(), Arc::new(RunControl::default()));
        let action = MkAction::PixelCheck {
            target: MkCoordinateTarget::Screen {
                point: MkPoint { x: -1, y: 2 },
            },
            color: "#ABCDEF".into(),
            tolerance: 3,
        };
        let mut vars = RuntimeVariables::new();
        let mut guard = InputCleanupGuard::new(fake.clone());
        fake.conditions.lock().unwrap().insert("pixel".into(), true);
        executor
            .action(7, &action, &MkPlayback::default(), &mut vars, &mut guard)
            .unwrap();
        assert_eq!(vars.get("last_pixel_result"), Some(&MkValue::Boolean(true)));
        assert_eq!(vars.get("last_pixel_found"), Some(&MkValue::Boolean(true)));

        fake.conditions
            .lock()
            .unwrap()
            .insert("pixel".into(), false);
        assert_eq!(
            executor
                .action(7, &action, &MkPlayback::default(), &mut vars, &mut guard)
                .unwrap_err()
                .kind,
            DiagnosticKind::TargetNotFound
        );
        assert_eq!(
            vars.get("last_pixel_result"),
            Some(&MkValue::Boolean(false))
        );
        assert_eq!(vars.get("last_pixel_found"), Some(&MkValue::Boolean(false)));

        fake.conditions
            .lock()
            .unwrap()
            .insert("pixel_error".into(), true);
        assert_eq!(
            executor
                .action(7, &action, &MkPlayback::default(), &mut vars, &mut guard)
                .unwrap_err()
                .kind,
            DiagnosticKind::Backend
        );
        assert!(!vars.contains_key("last_pixel_result"));
        assert!(!vars.contains_key("last_pixel_found"));
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

/// Scales playback pacing using ceiling division. Thus a non-zero duration remains
/// non-zero at every valid speed, and widened arithmetic cannot overflow.
pub fn scale_playback_duration(milliseconds: u64, speed_percent: u32) -> u64 {
    if milliseconds == 0 {
        return 0;
    }
    debug_assert!(
        speed_percent > 0,
        "compiler validation rejects zero playback speed"
    );
    if speed_percent == 0 {
        return u64::MAX;
    }
    let numerator = u128::from(milliseconds) * 100;
    let scaled = (numerator + u128::from(speed_percent) - 1) / u128::from(speed_percent);
    scaled.min(u128::from(u64::MAX)) as u64
}

fn offset_point(point: MkPoint, x: i64, y: i64) -> MkPoint {
    let add = |value: i32, delta: i64| {
        (i64::from(value).saturating_add(delta)).clamp(i64::from(i32::MIN), i64::from(i32::MAX))
            as i32
    };
    MkPoint {
        x: add(point.x, x),
        y: add(point.y, y),
    }
}

/// Combines an already-scaled normal delay with a deterministic sampled value.
pub fn add_sampled_random_delay(normal: u64, sampled: u64) -> u64 {
    normal.saturating_add(sampled)
}

fn sample_delay(max: u64) -> u64 {
    if max == 0 {
        0
    } else {
        rand::random_range(0..=max)
    }
}
fn sample_offset(max: u32) -> i64 {
    if max == 0 {
        0
    } else {
        rand::random_range(-i64::from(max)..=i64::from(max))
    }
}
impl Executor {
    const MAX_CONTROL_TRANSITIONS: u64 = 100_000;
    pub fn new(backends: Backends, control: Arc<RunControl>) -> Self {
        Self { backends, control }
    }
    fn prompt_input(
        &self,
        p: &MkPromptInputPayload,
        variables: &mut RuntimeVariables,
    ) -> ExecResult {
        let expand = |field: &'static str, text: &str| {
            interpolate(text, variables).map_err(|error| error.context("field", field))
        };
        // Expand every prompt field before crossing the prompt effect boundary.
        let title = expand("prompt.title", &p.title)?;
        let prompt = expand("prompt.prompt", &p.prompt)?;
        let default_value = expand("prompt.default_value", &p.default_value)?;
        let response = self.backends.prompt.prompt(
            PromptRequest {
                id: super::prompt::next_request_id(),
                title,
                prompt,
                default_value,
            },
            &self.control,
        )?;
        match response {
            PromptResponse::Cancelled => Err(ExecutionDiagnostic::new(
                DiagnosticKind::Cancelled,
                "input prompt was cancelled",
            )),
            PromptResponse::Submitted(text) => {
                variables.insert(p.variable.clone(), MkValue::String(text.clone()));
                if p.copy_to_clipboard {
                    self.backends.clipboard.set_text(&text)?;
                }
                Ok(())
            }
        }
    }
    fn wait(&self, duration: Duration) -> ExecResult {
        let mut remaining = duration;
        while !remaining.is_zero() {
            if self.backends.input.escape_pressed() {
                self.control.stop();
            }
            let slice = remaining.min(Duration::from_millis(10));
            self.control.wait(slice)?;
            remaining = remaining.saturating_sub(slice);
        }
        Ok(())
    }
    pub fn execute(&self, plan: &MkExecutionPlan, observe: &dyn Fn(ExecutionEvent)) -> ExecResult {
        let _activity = RunActivityGuard(&self.control);
        let mut guard = InputCleanupGuard::new(self.backends.input.clone());
        let mut vars = RuntimeVariables::new();
        let mut pc = 0;
        let mut loops: HashMap<usize, u32> = HashMap::new();
        let mut transitions = 0u64;
        vars.insert("macro.id".into(), MkValue::Number(plan.macro_id as f64));
        vars.insert("last_action_success".into(), MkValue::Boolean(true));
        while pc < plan.instructions.len() {
            if self.backends.input.escape_pressed() {
                self.control.stop();
            }
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
                    match self.action(
                        plan.macro_id,
                        &step.action,
                        &plan.playback,
                        &mut vars,
                        &mut guard,
                    ) {
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
                            if attempt < attempts
                                && let super::MkErrorPolicy::Retry(r) = &step.on_error
                            {
                                self.wait(Duration::from_millis(r.delay_ms))?
                            }
                        }
                    }
                }
                if final_error.is_some() {
                    break;
                }
                // Retry delay is error-policy backoff, not playback pacing, and is intentionally unscaled.
                let normal =
                    scale_playback_duration(step.delay_after_ms, plan.playback.speed_percent);
                let delay = if step.action.is_structural() {
                    normal
                } else {
                    add_sampled_random_delay(normal, sample_delay(plan.playback.random_delay_ms))
                };
                if delay > 0 {
                    self.wait(Duration::from_millis(delay))?
                }
            }
            if let Some(e) = final_error {
                observe(ExecutionEvent::StepFailed(step.id, e.clone()));
                if e.kind == DiagnosticKind::Cancelled
                    || !matches!(step.on_error, super::MkErrorPolicy::Continue)
                {
                    return Err(e);
                }
            } else {
                observe(ExecutionEvent::StepFinished(step.id))
            }
            pc = match (&step.action, &ins.jump) {
                (MkAction::If(c) | MkAction::WhileStart { condition: c }, Jump::IfFalse(to)) => {
                    self.control.checkpoint()?;
                    if self.condition(plan.macro_id, c, &mut vars)? {
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
        Ok(())
    }
    fn action(
        &self,
        macro_id: u64,
        a: &MkAction,
        playback: &MkPlayback,
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
            MkAction::Hotkey(keys) => g.hotkey(keys),
            MkAction::Text(p) => {
                let mut expanded = p.clone();
                expanded.text =
                    interpolate(&p.text, v).map_err(|error| error.context("field", "text.text"))?;
                self.backends.input.text(&expanded)
            }
            MkAction::MouseMove(p) => self.move_to(p, playback, v),
            MkAction::MouseDrag(p) => {
                let from = self.finalize_target(&p.from, playback, v, "Mouse Drag")?;
                let to = self.finalize_target(&p.to, playback, v, "Mouse Drag")?;
                super::input::drag(
                    &*self.backends.input,
                    &self.control,
                    p.button.clone(),
                    from,
                    to,
                    Duration::from_millis(scale_playback_duration(
                        p.duration_ms,
                        playback.speed_percent,
                    )),
                )?;
                set_point(v, "mouse", to);
                set_point(v, "last_point", to);
                Ok(())
            }
            MkAction::MouseClick(p) => {
                let point = self.finalize_target(&p.target, playback, v, "Mouse Click")?;
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
            MkAction::Delay { milliseconds } => self.wait(Duration::from_millis(
                scale_playback_duration(*milliseconds, playback.speed_percent),
            )),
            MkAction::Process(p) => {
                let mut expanded = p.clone();
                expanded.arguments = p
                    .arguments
                    .iter()
                    .enumerate()
                    .map(|(index, argument)| {
                        interpolate(argument, v).map_err(|error| {
                            error.context("field", format!("process.arguments[{index}]"))
                        })
                    })
                    .collect::<ExecResult<_>>()?;
                expanded.working_directory = p
                    .working_directory
                    .as_deref()
                    .filter(|directory| !directory.is_empty())
                    .map(|directory| {
                        interpolate(directory, v)
                            .map_err(|error| error.context("field", "process.working_directory"))
                    })
                    .transpose()?;
                self.backends.launcher.launch_process(&expanded)
            }
            MkAction::LauncherCommand { command, args } => {
                let expanded_args = args
                    .as_deref()
                    .map(|args| {
                        interpolate(args, v)
                            .map_err(|error| error.context("field", "launcher_command.args"))
                    })
                    .transpose()?;
                self.backends
                    .launcher
                    .command(command, expanded_args.as_deref())
            }
            MkAction::WindowActivate(p) => self.backends.window.activate(p),
            MkAction::WindowClose(m) => self.backends.window.close(m),
            MkAction::WindowMoveResize(p) => self.backends.window.move_resize(p),
            MkAction::WindowState { matcher, state } => {
                self.backends.window.set_state(matcher, *state)
            }
            MkAction::VirtualDesktop(action) => {
                #[cfg(windows)]
                {
                    g.hotkey(&super::virtual_desktops::shortcut(*action))
                }
                #[cfg(not(windows))]
                {
                    Err(ExecutionDiagnostic::new(
                        DiagnosticKind::UnsupportedOperation,
                        "Virtual desktop automation is available only on Windows",
                    )
                    .context("backend", "virtual desktop")
                    .context("action", format!("{action:?}")))
                }
            }
            MkAction::WindowWait(p) => self.wait_condition(
                macro_id,
                &MkCondition::WindowExists {
                    matcher: p.matcher.clone(),
                },
                p.wait.as_ref().unwrap_or(&MkWaitOptions {
                    timeout_ms: 0,
                    poll_interval_ms: 10,
                }),
                v,
            ),
            MkAction::WaitUntil { condition, wait } => {
                self.wait_condition(macro_id, condition, wait, v)
            }
            MkAction::SetVariable { name, value } => {
                v.insert(name.clone(), value.clone());
                Ok(())
            }
            MkAction::UnsetVariable { name } => {
                v.remove(name);
                Ok(())
            }
            MkAction::PromptInput(p) => self.prompt_input(p, v),
            MkAction::ImageFind(p) => self.wait_image(macro_id, p, v).map(|_| ()),
            MkAction::ImageClick(p) => {
                let pt = self.wait_image(macro_id, p, v)?;
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
                // Never expose a result left behind by an earlier check if
                // coordinate resolution, capture, or color parsing fails.
                v.remove("last_pixel_result");
                v.remove("last_pixel_found");
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
            // Structural actions are deliberately executed by `run_program`; reaching
            // this payload dispatcher for one is therefore an intentional no-op.
            MkAction::If(_)
            | MkAction::Else
            | MkAction::EndIf
            | MkAction::RepeatStart { .. }
            | MkAction::RepeatEnd
            | MkAction::WhileStart { .. }
            | MkAction::WhileEnd
            | MkAction::Break
            | MkAction::Continue => Ok(()),
        }
    }
    fn finalize_target(
        &self,
        target: &MkCoordinateTarget,
        playback: &MkPlayback,
        v: &RuntimeVariables,
        action: &'static str,
    ) -> ExecResult<MkPoint> {
        let configured = self
            .backends
            .screen
            .resolve(target, v)
            .map_err(|mut error| {
                if error.kind == DiagnosticKind::TypeMismatch {
                    if let (Some(variable), Some(actual)) =
                        (error.context.get("variable"), error.context.get("actual"))
                    {
                        error.message = format!(
                            "Variable '{variable}' contains {actual}; {action} requires Point"
                        );
                    }
                    error.context.insert("action".into(), action.into());
                }
                error
            })?;
        let randomized = offset_point(
            configured,
            sample_offset(playback.random_offset_px),
            sample_offset(playback.random_offset_px),
        );
        self.backends.screen.finalize_point(randomized)
    }
    fn move_to(
        &self,
        payload: &super::MkMouseMovePayload,
        playback: &MkPlayback,
        v: &mut RuntimeVariables,
    ) -> ExecResult {
        let point = self.finalize_target(&payload.target, playback, v, "Mouse Move")?;
        if payload.duration_ms == 0 {
            self.backends.input.move_mouse(point)?;
        } else {
            let from = self.backends.input.cursor_position()?;
            super::input::smooth_move(
                &*self.backends.input,
                &self.control,
                from,
                point,
                Duration::from_millis(scale_playback_duration(
                    payload.duration_ms,
                    playback.speed_percent,
                )),
            )?;
        }
        set_point(v, "mouse", point);
        set_point(v, "last_point", point);
        Ok(())
    }
    fn wait_image(
        &self,
        macro_id: u64,
        p: &MkImagePayload,
        v: &mut RuntimeVariables,
    ) -> ExecResult<MkPoint> {
        let image_variable = super::screen::image_result_variable(p.asset_id);
        v.remove(&image_variable);
        for key in ["last_image", "last_image_x", "last_image_y"] {
            v.remove(key);
        }
        let mut found = None;
        let result = self.wait_until(&p.wait, || {
            found = self.backends.screen.find_image(macro_id, p)?;
            Ok(found.is_some())
        });
        v.insert(
            "last_image_result".into(),
            MkValue::Boolean(found.is_some()),
        );
        v.insert("last_image_found".into(), MkValue::Boolean(found.is_some()));
        if let Some(point) = found {
            v.insert(image_variable, MkValue::Point(point));
            v.insert("last_image".into(), MkValue::Point(point));
            set_point(v, "last_image", point);
            v.insert("last_image_x".into(), MkValue::Number(point.x.into()));
            v.insert("last_image_y".into(), MkValue::Number(point.y.into()));
        }
        match (result, found) {
            (_, Some(point)) => Ok(point),
            (Err(mut error), None) => {
                if error.kind == DiagnosticKind::Timeout {
                    error.message =
                        format!("Target image was not found within {} ms", p.wait.timeout_ms);
                    error
                        .context
                        .insert("timeout_ms".into(), p.wait.timeout_ms.to_string());
                }
                Err(error
                    .context("macro_id", macro_id.to_string())
                    .context("asset_id", p.asset_id.to_string())
                    .context("region", format!("{:?}", p.region)))
            }
            (Ok(()), None) => Err(ExecutionDiagnostic::new(
                DiagnosticKind::TargetNotFound,
                "Target image was not found",
            )
            .context("macro_id", macro_id.to_string())
            .context("asset_id", p.asset_id.to_string())
            .context("region", format!("{:?}", p.region))),
        }
    }
    fn wait_condition(
        &self,
        macro_id: u64,
        condition: &MkCondition,
        o: &MkWaitOptions,
        v: &mut RuntimeVariables,
    ) -> ExecResult {
        let started = Instant::now();
        let mut polls = 0u64;
        loop {
            self.control.checkpoint()?;
            polls += 1;
            if self.condition(macro_id, condition, v)? {
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
            self.wait(
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
            self.wait(Duration::from_millis(o.poll_interval_ms.max(1)))?
        }
    }
    fn condition(
        &self,
        macro_id: u64,
        c: &MkCondition,
        v: &mut RuntimeVariables,
    ) -> ExecResult<bool> {
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
                let point = self.backends.screen.find_image(
                    macro_id,
                    &MkImagePayload {
                        asset_id: *asset_id,
                        wait: MkWaitOptions {
                            timeout_ms: 0,
                            poll_interval_ms: 1,
                        },
                        region: super::SearchRegion::Desktop,
                        tolerance: 0,
                        alpha: super::AlphaPolicy::Compare,
                        return_point: super::ReturnPoint::Center,
                    },
                )?;
                v.insert(
                    "last_image_result".into(),
                    MkValue::Boolean(point.is_some()),
                );
                v.insert("last_image_found".into(), MkValue::Boolean(point.is_some()));
                if let Some(p) = point {
                    v.insert("last_image".into(), MkValue::Point(p));
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
                v.remove("last_pixel_result");
                v.remove("last_pixel_found");
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
                    if !self.condition(macro_id, c, v)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            MkCondition::Any { conditions } => {
                for c in conditions {
                    if self.condition(macro_id, c, v)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            MkCondition::Not { condition } => Ok(!self.condition(macro_id, condition, v)?),
        }
    }
}
/// Testable declaration that every action has deliberate executor handling.
pub fn has_runtime_support(action: &MkAction) -> bool {
    // This is production capability metadata, not merely a mirror of the
    // executor match. Mouse support includes the wired WindowsScreenBackend
    // coordinate resolver and SendInput paths (including drag).
    match action {
        MkAction::UiInvoke(_)
        | MkAction::UiSetValue { .. }
        | MkAction::UiReadValue { .. }
        | MkAction::UiToggle(_)
        | MkAction::UiSelect(_)
        | MkAction::UiFocus(_)
        | MkAction::UiWait(_) => false,
        MkAction::KeyDown(_)
        | MkAction::KeyUp(_)
        | MkAction::KeyPress(_)
        | MkAction::Hotkey(_)
        | MkAction::Text(_)
        | MkAction::MouseMove(_)
        | MkAction::MouseDrag(_)
        | MkAction::MouseClick(_)
        | MkAction::MouseDown(_)
        | MkAction::MouseUp(_)
        | MkAction::MouseScroll { .. }
        | MkAction::Delay { .. }
        | MkAction::Process(_)
        | MkAction::LauncherCommand { .. }
        | MkAction::WindowActivate(_)
        | MkAction::WindowClose(_)
        | MkAction::WindowWait(_)
        | MkAction::WindowMoveResize(_)
        | MkAction::WindowState { .. }
        | MkAction::WaitUntil { .. }
        | MkAction::SetVariable { .. }
        | MkAction::UnsetVariable { .. }
        | MkAction::PromptInput(_)
        | MkAction::If(_)
        | MkAction::Else
        | MkAction::EndIf
        | MkAction::RepeatStart { .. }
        | MkAction::RepeatEnd
        | MkAction::WhileStart { .. }
        | MkAction::WhileEnd
        | MkAction::Break
        | MkAction::Continue
        | MkAction::PixelCheck { .. } => true,
        MkAction::VirtualDesktop(_) => cfg!(windows),
        // Production installs `ProductionVisualSearch`, backed by the same
        // screen capture and matcher used by all visual-search execution.
        MkAction::ImageFind(_) | MkAction::ImageClick(_) => true,
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
        | MkAction::MouseDrag(_)
        | MkAction::MouseClick(_)
        | MkAction::MouseDown(_)
        | MkAction::MouseUp(_)
        | MkAction::MouseScroll { .. } => "SendInput",
        MkAction::WindowActivate(_)
        | MkAction::WindowClose(_)
        | MkAction::WindowWait(_)
        | MkAction::WindowMoveResize(_)
        | MkAction::WindowState { .. } => "window",
        MkAction::VirtualDesktop(_) => "virtual desktop",
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
        MkAction::PromptInput(_) => "prompt",
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
    pub struct FakeBackend {
        pub events: Mutex<Vec<String>>,
        pub failures: Mutex<HashMap<String, ExecutionDiagnostic>>,
        pub conditions: Mutex<HashMap<String, bool>>,
        pub cursor: Mutex<MkPoint>,
        pub prompt_responses: Mutex<Vec<PromptResponse>>,
        pub processes: Mutex<Vec<MkProcessPayload>>,
        pub commands: Mutex<Vec<(String, Option<String>)>>,
        pub prompts: Mutex<Vec<PromptRequest>>,
    }
    impl Default for FakeBackend {
        fn default() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
                failures: Mutex::new(HashMap::new()),
                conditions: Mutex::new(HashMap::new()),
                cursor: Mutex::new(MkPoint { x: 0, y: 0 }),
                prompt_responses: Mutex::new(Vec::new()),
                processes: Mutex::new(Vec::new()),
                commands: Mutex::new(Vec::new()),
                prompts: Mutex::new(Vec::new()),
            }
        }
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
                launcher: self.clone(),
                prompt: self.clone(),
                clipboard: self.clone(),
            }
        }
        pub fn script_prompt(&self, response: PromptResponse) {
            self.prompt_responses.lock().unwrap().push(response);
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
        fn cursor_position(&self) -> ExecResult<MkPoint> {
            self.event("cursor_position".into())?;
            Ok(*self.cursor.lock().unwrap())
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
        fn move_resize(&self, _: &MkWindowMoveResizePayload) -> ExecResult {
            self.event("window_move_resize".into())
        }
        fn set_state(&self, _: &MkWindowMatcher, state: MkWindowState) -> ExecResult {
            self.event(format!("window_state:{state:?}"))
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
        fn find_image(&self, _: u64, _: &MkImagePayload) -> ExecResult<Option<MkPoint>> {
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
            if self
                .conditions
                .lock()
                .unwrap()
                .get("pixel_error")
                .copied()
                .unwrap_or(false)
            {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    "injected pixel capture failure",
                ));
            }
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
            self.processes.lock().unwrap().push(p.clone());
            self.event(format!("process:{}", p.program))
        }
        fn command(&self, c: &str, args: Option<&str>) -> ExecResult {
            self.commands
                .lock()
                .unwrap()
                .push((c.into(), args.map(str::to_owned)));
            self.event(format!("command:{c}"))
        }
    }
    impl PromptBackend for FakeBackend {
        fn prompt(&self, request: PromptRequest, _: &RunControl) -> ExecResult<PromptResponse> {
            self.prompts.lock().unwrap().push(request.clone());
            self.event(format!("prompt:{}", request.id))?;
            if self.prompt_responses.lock().unwrap().is_empty() {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    "no scripted prompt response",
                )
                .context("backend", "prompt"));
            }
            Ok(self.prompt_responses.lock().unwrap().remove(0))
        }
    }
    impl ClipboardBackend for FakeBackend {
        fn set_text(&self, text: &str) -> ExecResult {
            self.event(format!("clipboard:{text}"))
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

    #[test]
    fn interpolation_uses_current_variables_and_prompt_answers() {
        let f = Arc::new(FakeBackend::default());
        f.script_prompt(PromptResponse::Submitted("my_project".into()));
        run(
            vec![
                s(
                    1,
                    MkAction::SetVariable {
                        name: "project_name".into(),
                        value: MkValue::String("initial".into()),
                    },
                ),
                s(2, text("Creating project ${project_name}")),
                s(
                    3,
                    MkAction::PromptInput(MkPromptInputPayload {
                        title: "Rename ${project_name}".into(),
                        prompt: "Current: ${project_name}".into(),
                        default_value: "${project_name}".into(),
                        variable: "project_name".into(),
                        copy_to_clipboard: false,
                    }),
                ),
                s(4, text("Creating project ${project_name}")),
            ],
            f.clone(),
        )
        .unwrap();
        assert_eq!(
            f.events()
                .into_iter()
                .filter(|event| event.starts_with("text:"))
                .collect::<Vec<_>>(),
            [
                "text:Creating project initial",
                "text:Creating project my_project"
            ]
        );
        let prompts = f.prompts.lock().unwrap();
        assert_eq!(prompts[0].title, "Rename initial");
        assert_eq!(prompts[0].prompt, "Current: initial");
        assert_eq!(prompts[0].default_value, "initial");
    }

    #[test]
    fn process_and_launcher_fields_expand_without_changing_boundaries() {
        let f = Arc::new(FakeBackend::default());
        let original_process = MkProcessPayload {
            program: "tool-${value}".into(),
            arguments: vec!["--name".into(), "${value}".into()],
            working_directory: Some("/work/${folder}".into()),
            wait: true,
        };
        let original_action = MkAction::Process(original_process.clone());
        run(
            vec![
                s(
                    1,
                    MkAction::SetVariable {
                        name: "value".into(),
                        value: MkValue::String("two words".into()),
                    },
                ),
                s(
                    2,
                    MkAction::SetVariable {
                        name: "folder".into(),
                        value: MkValue::String("project".into()),
                    },
                ),
                s(3, original_action.clone()),
                s(
                    4,
                    MkAction::LauncherCommand {
                        command: "canonical-${value}".into(),
                        args: Some("open ${value}".into()),
                    },
                ),
            ],
            f.clone(),
        )
        .unwrap();
        let processes = f.processes.lock().unwrap();
        assert_eq!(processes[0].program, "tool-${value}");
        assert_eq!(processes[0].arguments, ["--name", "two words"]);
        assert_eq!(
            processes[0].working_directory.as_deref(),
            Some("/work/project")
        );
        assert_eq!(original_action, MkAction::Process(original_process));
        assert_eq!(
            f.commands.lock().unwrap()[0],
            ("canonical-${value}".into(), Some("open two words".into()))
        );
    }

    #[test]
    fn failed_interpolation_has_no_effect_and_stops_later_steps() {
        let f = Arc::new(FakeBackend::default());
        let error = run(
            vec![s(1, text("secret ${missing}")), s(2, text("later"))],
            f.clone(),
        )
        .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::InvalidTarget);
        assert_eq!(
            error.context.get("variable").map(String::as_str),
            Some("missing")
        );
        assert!(f.events().is_empty());
    }

    #[test]
    fn repeated_steps_interpolate_each_iteration_at_execution_time() {
        let f = Arc::new(FakeBackend::default());
        let mut repeated = s(1, text("iteration ${iteration}"));
        repeated.repeat = 2;
        run(vec![repeated], f.clone()).unwrap();
        assert_eq!(f.events(), ["text:iteration 0", "text:iteration 1"]);
    }
}
