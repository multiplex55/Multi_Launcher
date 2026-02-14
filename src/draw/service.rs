use crate::draw::messages::{ExitReason, MainToOverlay, OverlayToMain};
use crate::draw::overlay::spawn_overlay;
use crate::draw::save::ExitPromptState;
use crate::draw::settings::DrawSettings;
use crate::draw::state::{can_transition, DrawLifecycle};
use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartOutcome {
    Started,
    AlreadyActive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryContext {
    pub monitor_rect: MonitorRect,
    pub launcher_offscreen_context: Option<String>,
    pub mouse_gestures_prior_effective_state: bool,
    pub timeout_deadline: Option<Instant>,
}

impl Default for EntryContext {
    fn default() -> Self {
        Self {
            monitor_rect: MonitorRect::default(),
            launcher_offscreen_context: None,
            mouse_gestures_prior_effective_state: true,
            timeout_deadline: None,
        }
    }
}

struct DrawRuntimeState {
    lifecycle: DrawLifecycle,
    settings: DrawSettings,
    overlay_thread_handle: Option<JoinHandle<()>>,
    main_to_overlay_tx: Option<Sender<MainToOverlay>>,
    overlay_to_main_rx: Option<Receiver<OverlayToMain>>,
    entry_context: Option<EntryContext>,
    exit_prompt: Option<ExitPromptState>,
    dispatched_messages: Vec<MainToOverlay>,
}

impl Default for DrawRuntimeState {
    fn default() -> Self {
        Self {
            lifecycle: DrawLifecycle::Idle,
            settings: DrawSettings::default(),
            overlay_thread_handle: None,
            main_to_overlay_tx: None,
            overlay_to_main_rx: None,
            entry_context: None,
            exit_prompt: None,
            dispatched_messages: Vec::new(),
        }
    }
}

pub struct DrawRuntime {
    state: Mutex<DrawRuntimeState>,
}

impl Default for DrawRuntime {
    fn default() -> Self {
        Self {
            state: Mutex::new(DrawRuntimeState {
                lifecycle: DrawLifecycle::Idle,
                ..DrawRuntimeState::default()
            }),
        }
    }
}

static DRAW_RUNTIME: Lazy<DrawRuntime> = Lazy::new(DrawRuntime::default);

type Hook = Box<dyn Fn() -> Result<()> + Send + Sync>;
type SpawnHook = Box<dyn Fn() -> Result<OverlayStartupHandshake> + Send + Sync>;
static DRAW_SPAWN_HOOK: Lazy<Mutex<Option<SpawnHook>>> = Lazy::new(|| Mutex::new(None));
static DRAW_RESTORE_HOOK: Lazy<Mutex<Option<Hook>>> = Lazy::new(|| Mutex::new(None));
const OVERLAY_JOIN_TIMEOUT: Duration = Duration::from_secs(2);

pub struct OverlayStartupHandshake {
    pub overlay_thread_handle: JoinHandle<()>,
    pub main_to_overlay_tx: Sender<MainToOverlay>,
    pub overlay_to_main_rx: Receiver<OverlayToMain>,
}

pub fn runtime() -> &'static DrawRuntime {
    &DRAW_RUNTIME
}

pub fn set_runtime_spawn_hook(hook: Option<SpawnHook>) {
    if let Ok(mut guard) = DRAW_SPAWN_HOOK.lock() {
        *guard = hook;
    }
}

pub fn set_runtime_restore_hook(hook: Option<Hook>) {
    if let Ok(mut guard) = DRAW_RESTORE_HOOK.lock() {
        *guard = hook;
    }
}

impl DrawRuntime {
    pub fn start(&self) -> Result<StartOutcome> {
        self.start_with_context(EntryContext::default())
    }

    pub fn lifecycle(&self) -> DrawLifecycle {
        self.state
            .lock()
            .map(|s| s.lifecycle)
            .unwrap_or(DrawLifecycle::Idle)
    }

    pub fn is_active(&self) -> bool {
        self.lifecycle().is_active()
    }

    pub fn start_with_context(&self, mut entry_context: EntryContext) -> Result<StartOutcome> {
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow!("draw runtime lock poisoned"))?;
            if state.lifecycle != DrawLifecycle::Idle {
                return Ok(StartOutcome::AlreadyActive);
            }

            entry_context.mouse_gestures_prior_effective_state =
                crate::plugins::mouse_gestures::draw_effective_enabled();
            crate::plugins::mouse_gestures::set_draw_mode_active(true);

            self.transition_locked(&mut state, DrawLifecycle::Starting)?;
            state.entry_context = Some(entry_context.clone());
            state.exit_prompt = None;
            state.main_to_overlay_tx = None;
            state.overlay_to_main_rx = None;
            state.overlay_thread_handle = None;
        }

        let startup = match self.spawn_overlay_runtime() {
            Ok(startup) => startup,
            Err(err) => {
                let _ = self.rollback_after_start_failure();
                return Err(err);
            }
        };

        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("draw runtime lock poisoned"))?;
        state.overlay_thread_handle = Some(startup.overlay_thread_handle);
        state.main_to_overlay_tx = Some(startup.main_to_overlay_tx);
        state.overlay_to_main_rx = Some(startup.overlay_to_main_rx);
        self.transition_locked(&mut state, DrawLifecycle::Active)?;
        Self::send_overlay_message_locked(&mut state, MainToOverlay::Start);
        Ok(StartOutcome::Started)
    }

    pub fn request_exit(&self, reason: ExitReason) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("draw runtime lock poisoned"))?;

        if state.lifecycle == DrawLifecycle::Idle {
            return Ok(());
        }

        if state.lifecycle == DrawLifecycle::Starting || state.lifecycle == DrawLifecycle::Active {
            self.transition_locked(&mut state, DrawLifecycle::Exiting)?;
            state.exit_prompt = Some(ExitPromptState::from_exit_reason(reason.clone()));
            Self::send_overlay_message_locked(&mut state, MainToOverlay::RequestExit { reason });
        }
        Ok(())
    }

    pub fn notify_overlay_exit(&self, reason: ExitReason) -> Result<()> {
        self.restore_pipeline(reason, "overlay exit notification")
    }

    pub fn run_overlay_thread_entrypoint<F>(&self, entrypoint: F) -> Result<()>
    where
        F: FnOnce() -> Result<()> + panic::UnwindSafe,
    {
        match panic::catch_unwind(AssertUnwindSafe(entrypoint)) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(err)) => {
                tracing::error!(?err, "draw overlay thread failed");
                self.restore_pipeline(ExitReason::OverlayFailure, "overlay thread failure")
            }
            Err(payload) => {
                let panic_message = if let Some(message) = payload.downcast_ref::<&str>() {
                    (*message).to_string()
                } else if let Some(message) = payload.downcast_ref::<String>() {
                    message.clone()
                } else {
                    "unknown panic payload".to_string()
                };
                tracing::error!(panic_message, "draw overlay thread panicked");
                self.restore_pipeline(ExitReason::OverlayFailure, "overlay thread panic")
            }
        }
    }

    pub fn apply_settings(&self, settings: DrawSettings) {
        if let Ok(mut state) = self.state.lock() {
            state.settings = settings;
            if state.lifecycle.is_active() {
                Self::send_overlay_message_locked(&mut state, MainToOverlay::UpdateSettings);
            }
        }
    }

    #[cfg(test)]
    pub fn settings_for_test(&self) -> Option<DrawSettings> {
        self.state.lock().ok().map(|s| s.settings.clone())
    }

    #[cfg(test)]
    pub fn take_dispatched_messages_for_test(&self) -> Vec<MainToOverlay> {
        if let Ok(mut state) = self.state.lock() {
            std::mem::take(&mut state.dispatched_messages)
        } else {
            Vec::new()
        }
    }

    #[cfg(test)]
    pub fn entry_context_for_test(&self) -> Option<EntryContext> {
        self.state.lock().ok().and_then(|s| s.entry_context.clone())
    }

    #[cfg(test)]
    pub fn startup_handles_present_for_test(&self) -> bool {
        self.state
            .lock()
            .ok()
            .map(|state| {
                state.overlay_thread_handle.is_some()
                    && state.main_to_overlay_tx.is_some()
                    && state.overlay_to_main_rx.is_some()
            })
            .unwrap_or(false)
    }

    pub fn exit_prompt_state(&self) -> Option<ExitPromptState> {
        self.state.lock().ok().and_then(|s| s.exit_prompt.clone())
    }

    pub fn set_exit_prompt_error(&self, error: impl Into<String>) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(prompt) = state.exit_prompt.as_mut() {
                prompt.last_error = Some(error.into());
                prompt.overlay_hidden_for_capture = false;
            }
        }
    }

    pub fn mark_overlay_hidden_for_capture(&self) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(prompt) = state.exit_prompt.as_mut() {
                prompt.overlay_hidden_for_capture = true;
            }
        }
    }

    pub fn tick(&self, now: Instant) -> Result<()> {
        let timed_out = {
            let state = self
                .state
                .lock()
                .map_err(|_| anyhow!("draw runtime lock poisoned"))?;
            if state.lifecycle != DrawLifecycle::Active {
                false
            } else {
                state
                    .entry_context
                    .as_ref()
                    .and_then(|ctx| ctx.timeout_deadline)
                    .is_some_and(|deadline| now >= deadline)
            }
        };

        if timed_out {
            self.restore_pipeline(ExitReason::Timeout, "overlay timeout failsafe")?;
        }

        self.process_overlay_notifications()?;
        Ok(())
    }

    pub fn force_lifecycle_for_test(&self, lifecycle: DrawLifecycle) {
        if let Ok(mut state) = self.state.lock() {
            state.lifecycle = lifecycle;
        }
    }

    pub fn reset_for_test(&self) {
        if let Ok(mut state) = self.state.lock() {
            *state = DrawRuntimeState {
                lifecycle: DrawLifecycle::Idle,
                ..DrawRuntimeState::default()
            };
        }
    }

    fn process_overlay_notifications(&self) -> Result<()> {
        let mut terminal_reason = None;
        let mut save_error = None;

        {
            let state = self
                .state
                .lock()
                .map_err(|_| anyhow!("draw runtime lock poisoned"))?;
            if !state.lifecycle.is_active() && state.lifecycle != DrawLifecycle::Exiting {
                return Ok(());
            }
            if let Some(rx) = &state.overlay_to_main_rx {
                loop {
                    match rx.try_recv() {
                        Ok(OverlayToMain::Exited { reason, .. }) => {
                            terminal_reason = Some(reason);
                        }
                        Ok(OverlayToMain::SaveProgress { canvas }) => {
                            tracing::debug!(
                                objects = canvas.objects.len(),
                                "draw overlay save progress update"
                            );
                        }
                        Ok(OverlayToMain::SaveError { error }) => {
                            tracing::error!(error = %error, "draw overlay save error");
                            save_error = Some(error);
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            terminal_reason = Some(ExitReason::OverlayFailure);
                            break;
                        }
                    }
                }
            }
        }

        if let Some(error) = save_error {
            self.set_exit_prompt_error(error);
        }
        if let Some(reason) = terminal_reason {
            self.restore_pipeline(reason, "overlay exited notification")?;
        }

        Ok(())
    }

    fn rollback_after_start_failure(&self) -> Result<()> {
        self.restore_pipeline(ExitReason::StartFailure, "draw start failure rollback")
    }

    fn restore_pipeline(&self, reason: ExitReason, source: &str) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("draw runtime lock poisoned"))?;

        if matches!(
            state.lifecycle,
            DrawLifecycle::Idle | DrawLifecycle::Restoring
        ) {
            return Ok(());
        }

        if can_transition(state.lifecycle, DrawLifecycle::Restoring) {
            state.lifecycle = DrawLifecycle::Restoring;
        } else {
            return Ok(());
        }
        drop(state);

        if let Err(err) = self.call_hook(&DRAW_RESTORE_HOOK) {
            tracing::error!(?err, "draw restore hook failed after {source}");
        }
        tracing::warn!(?reason, "draw runtime restore executed from {source}");

        let (overlay_thread_handle, prior_mouse_gesture_state) = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow!("draw runtime lock poisoned"))?;
            let prior_mouse_gesture_state = state
                .entry_context
                .as_ref()
                .map(|ctx| ctx.mouse_gestures_prior_effective_state)
                .unwrap_or_else(crate::plugins::mouse_gestures::draw_effective_enabled);

            let handle = state.overlay_thread_handle.take();
            state.main_to_overlay_tx = None;
            state.overlay_to_main_rx = None;
            state.entry_context = None;
            state.exit_prompt = None;
            state.lifecycle = DrawLifecycle::Idle;
            (handle, prior_mouse_gesture_state)
        };

        crate::plugins::mouse_gestures::restore_draw_prior_effective_state(
            prior_mouse_gesture_state,
        );

        self.join_overlay_thread_with_timeout(overlay_thread_handle, source);
        Ok(())
    }

    fn join_overlay_thread_with_timeout(&self, handle: Option<JoinHandle<()>>, source: &str) {
        let Some(handle) = handle else {
            return;
        };

        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let join_result = handle.join();
            let _ = done_tx.send(join_result);
        });

        match done_rx.recv_timeout(OVERLAY_JOIN_TIMEOUT) {
            Ok(Ok(())) => {}
            Ok(Err(_)) => {
                tracing::error!("draw overlay thread panicked while joining during {source}");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                tracing::error!("draw overlay thread join timed out during {source}");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                tracing::error!("draw overlay thread join channel disconnected during {source}");
            }
        }
    }

    fn call_hook(&self, hook_slot: &Lazy<Mutex<Option<Hook>>>) -> Result<()> {
        let guard = hook_slot
            .lock()
            .map_err(|_| anyhow!("draw hook lock poisoned"))?;
        if let Some(ref hook) = *guard {
            hook()?;
        }
        Ok(())
    }

    fn spawn_overlay_runtime(&self) -> Result<OverlayStartupHandshake> {
        if let Ok(guard) = DRAW_SPAWN_HOOK.lock() {
            if let Some(ref hook) = *guard {
                return hook();
            }
        }

        let startup = spawn_overlay()?;
        Ok(OverlayStartupHandshake {
            overlay_thread_handle: startup.overlay_thread_handle,
            main_to_overlay_tx: startup.main_to_overlay_tx,
            overlay_to_main_rx: startup.overlay_to_main_rx,
        })
    }

    fn transition_locked(&self, state: &mut DrawRuntimeState, next: DrawLifecycle) -> Result<()> {
        if !can_transition(state.lifecycle, next) {
            return Err(anyhow!(
                "invalid draw lifecycle transition: {:?} -> {:?}",
                state.lifecycle,
                next
            ));
        }
        state.lifecycle = next;
        Ok(())
    }

    fn send_overlay_message_locked(state: &mut DrawRuntimeState, message: MainToOverlay) {
        state.dispatched_messages.push(message.clone());
        if let Some(tx) = &state.main_to_overlay_tx {
            let _ = tx.send(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        runtime, set_runtime_restore_hook, set_runtime_spawn_hook, DrawRuntime, EntryContext,
        StartOutcome,
    };
    use crate::draw::messages::{ExitReason, MainToOverlay, OverlayToMain, SaveResult};
    use crate::draw::settings::DrawSettings;
    use crate::draw::state::{can_transition, DrawLifecycle};
    use crate::plugins::mouse_gestures::{
        apply_runtime_settings, draw_effective_enabled, restore_draw_prior_effective_state,
        set_draw_mode_active, sync_enabled_plugins, MouseGestureSettings,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    fn reset_runtime(rt: &DrawRuntime) {
        set_runtime_spawn_hook(None);
        set_runtime_restore_hook(None);
        rt.reset_for_test();
        restore_draw_prior_effective_state(true);
        sync_enabled_plugins(None);
        apply_runtime_settings(MouseGestureSettings {
            enabled: true,
            ..MouseGestureSettings::default()
        });
        set_draw_mode_active(false);
    }

    #[test]
    fn start_from_idle_succeeds_only_after_handles_are_populated() {
        let rt = runtime();
        reset_runtime(rt);
        let outcome = rt.start().expect("start should succeed");
        assert_eq!(outcome, StartOutcome::Started);
        assert_eq!(rt.lifecycle(), DrawLifecycle::Active);
        assert!(rt.startup_handles_present_for_test());
        rt.reset_for_test();
    }

    #[test]
    fn start_when_active_is_idempotent() {
        let rt = runtime();
        reset_runtime(rt);
        rt.force_lifecycle_for_test(DrawLifecycle::Active);
        let outcome = rt.start().expect("idempotent start should succeed");
        assert_eq!(outcome, StartOutcome::AlreadyActive);
        rt.reset_for_test();
    }

    #[test]
    fn request_exit_transitions_to_exiting() {
        let rt = runtime();
        reset_runtime(rt);
        rt.force_lifecycle_for_test(DrawLifecycle::Active);
        rt.request_exit(ExitReason::UserRequest)
            .expect("request_exit should succeed");
        assert_eq!(rt.lifecycle(), DrawLifecycle::Exiting);
        let prompt = rt
            .exit_prompt_state()
            .expect("prompt state should be present");
        assert!(prompt.frozen_input);
        assert_eq!(prompt.reason, ExitReason::UserRequest);
        assert_eq!(
            rt.take_dispatched_messages_for_test(),
            vec![MainToOverlay::RequestExit {
                reason: ExitReason::UserRequest
            }]
        );
        rt.reset_for_test();
    }

    #[test]
    fn overlay_spawn_failure_rolls_back_context_and_mouse_state() {
        let rt = runtime();
        reset_runtime(rt);
        let restored = Arc::new(AtomicBool::new(false));
        let restored_clone = Arc::clone(&restored);
        set_runtime_spawn_hook(Some(Box::new(|| anyhow::bail!("overlay spawn failed"))));
        set_runtime_restore_hook(Some(Box::new(move || {
            restored_clone.store(true, Ordering::SeqCst);
            Ok(())
        })));

        let prior_mouse_state = draw_effective_enabled();
        let res = rt.start();
        assert!(res.is_err());
        assert_eq!(rt.lifecycle(), DrawLifecycle::Idle);
        assert_eq!(draw_effective_enabled(), prior_mouse_state);
        assert!(restored.load(Ordering::SeqCst));
        assert!(!rt.startup_handles_present_for_test());
        reset_runtime(rt);
    }

    #[test]
    fn overlay_exited_notification_from_channel_restores_once() {
        let rt = runtime();
        reset_runtime(rt);

        let (tx, rx) = std::sync::mpsc::channel::<OverlayToMain>();
        set_runtime_spawn_hook(Some(Box::new(move || {
            let (main_tx, main_rx) = std::sync::mpsc::channel::<MainToOverlay>();
            let handle = std::thread::spawn(move || {
                while let Ok(message) = main_rx.recv() {
                    if matches!(message, MainToOverlay::RequestExit { .. }) {
                        break;
                    }
                }
            });
            Ok(super::OverlayStartupHandshake {
                overlay_thread_handle: handle,
                main_to_overlay_tx: main_tx,
                overlay_to_main_rx: rx,
            })
        })));

        rt.start().expect("start should succeed");
        tx.send(OverlayToMain::Exited {
            reason: ExitReason::UserRequest,
            save_result: SaveResult::Skipped,
        })
        .expect("exit event should send");
        tx.send(OverlayToMain::Exited {
            reason: ExitReason::OverlayFailure,
            save_result: SaveResult::Skipped,
        })
        .expect("duplicate exit event should send");

        rt.tick(Instant::now()).expect("tick should process exits");
        assert_eq!(rt.lifecycle(), DrawLifecycle::Idle);
        assert!(!rt.startup_handles_present_for_test());
        reset_runtime(rt);
    }

    #[test]
    fn timeout_forces_restore_pipeline() {
        let rt = runtime();
        reset_runtime(rt);
        let restored = Arc::new(AtomicBool::new(false));
        let restored_clone = Arc::clone(&restored);
        set_runtime_restore_hook(Some(Box::new(move || {
            restored_clone.store(true, Ordering::SeqCst);
            Ok(())
        })));
        let timeout_ctx = EntryContext {
            timeout_deadline: Some(Instant::now() - Duration::from_secs(1)),
            ..EntryContext::default()
        };

        let outcome = rt
            .start_with_context(timeout_ctx)
            .expect("start with context should be idempotent/started");
        assert_eq!(outcome, StartOutcome::Started);
        rt.tick(Instant::now()).expect("tick should succeed");
        assert_eq!(rt.lifecycle(), DrawLifecycle::Idle);
        assert!(restored.load(Ordering::SeqCst));
        reset_runtime(rt);
    }

    #[test]
    fn overlay_panic_triggers_restore_callback() {
        let rt = runtime();
        reset_runtime(rt);
        let restored = Arc::new(AtomicBool::new(false));
        let restored_clone = Arc::clone(&restored);
        set_runtime_restore_hook(Some(Box::new(move || {
            restored_clone.store(true, Ordering::SeqCst);
            Ok(())
        })));
        rt.force_lifecycle_for_test(DrawLifecycle::Active);

        let result = rt.run_overlay_thread_entrypoint(|| -> anyhow::Result<()> {
            panic!("simulated panic");
        });

        assert!(result.is_ok());
        assert!(restored.load(Ordering::SeqCst));
        assert_eq!(rt.lifecycle(), DrawLifecycle::Idle);
        reset_runtime(rt);
    }

    #[test]
    fn duplicate_terminal_notifications_restore_once() {
        let rt = runtime();
        reset_runtime(rt);
        let restore_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let restore_count_clone = Arc::clone(&restore_count);
        set_runtime_restore_hook(Some(Box::new(move || {
            restore_count_clone.fetch_add(1, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(10));
            Ok(())
        })));
        rt.force_lifecycle_for_test(DrawLifecycle::Exiting);

        thread::scope(|scope| {
            scope.spawn(|| {
                let _ = rt.notify_overlay_exit(ExitReason::UserRequest);
            });
            scope.spawn(|| {
                let _ = rt.notify_overlay_exit(ExitReason::OverlayFailure);
            });
        });

        assert_eq!(restore_count.load(Ordering::SeqCst), 1);
        assert_eq!(rt.lifecycle(), DrawLifecycle::Idle);
        assert!(!rt.startup_handles_present_for_test());
        reset_runtime(rt);
    }

    #[test]
    fn timeout_path_uses_same_restore_pipeline_as_manual_exit() {
        let rt = runtime();
        reset_runtime(rt);
        let restore_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let restore_count_clone = Arc::clone(&restore_count);
        set_runtime_restore_hook(Some(Box::new(move || {
            restore_count_clone.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })));

        rt.force_lifecycle_for_test(DrawLifecycle::Exiting);
        rt.notify_overlay_exit(ExitReason::UserRequest)
            .expect("manual exit restore should succeed");
        assert_eq!(restore_count.load(Ordering::SeqCst), 1);

        let timeout_ctx = EntryContext {
            timeout_deadline: Some(Instant::now() - Duration::from_secs(1)),
            ..EntryContext::default()
        };
        rt.start_with_context(timeout_ctx)
            .expect("start with timeout should succeed");
        rt.tick(Instant::now()).expect("tick should restore");

        assert_eq!(restore_count.load(Ordering::SeqCst), 2);
        assert_eq!(rt.lifecycle(), DrawLifecycle::Idle);
        reset_runtime(rt);
    }

    #[test]
    fn apply_settings_while_active_emits_update_message() {
        let rt = runtime();
        reset_runtime(rt);
        rt.force_lifecycle_for_test(DrawLifecycle::Active);

        let mut updated = DrawSettings::default();
        updated.exit_timeout_seconds = 7;
        rt.apply_settings(updated.clone());

        assert_eq!(rt.settings_for_test(), Some(updated));
        assert_eq!(
            rt.take_dispatched_messages_for_test(),
            vec![MainToOverlay::UpdateSettings]
        );
        reset_runtime(rt);
    }

    #[test]
    fn draw_start_disables_mouse_gestures_when_previously_enabled() {
        let rt = runtime();
        reset_runtime(rt);

        assert!(draw_effective_enabled());
        let outcome = rt.start().expect("start should succeed");

        assert_eq!(outcome, StartOutcome::Started);
        assert!(!draw_effective_enabled());
        assert_eq!(
            rt.entry_context_for_test()
                .expect("entry context should be stored")
                .mouse_gestures_prior_effective_state,
            true
        );
        reset_runtime(rt);
    }

    #[test]
    fn draw_exit_restores_previous_mouse_gesture_state() {
        let rt = runtime();
        reset_runtime(rt);
        apply_runtime_settings(MouseGestureSettings {
            enabled: false,
            ..MouseGestureSettings::default()
        });

        assert!(!draw_effective_enabled());
        rt.start().expect("start should succeed");
        rt.notify_overlay_exit(ExitReason::UserRequest)
            .expect("exit should restore state");

        assert!(!draw_effective_enabled());
        reset_runtime(rt);
    }

    #[test]
    fn start_failure_and_timeout_restore_previous_mouse_gesture_state() {
        let rt = runtime();
        reset_runtime(rt);

        set_runtime_spawn_hook(Some(Box::new(|| anyhow::bail!("overlay spawn failed"))));
        let start_res = rt.start();
        assert!(start_res.is_err());
        assert!(draw_effective_enabled());

        set_runtime_spawn_hook(None);
        let timeout_ctx = EntryContext {
            timeout_deadline: Some(Instant::now() - Duration::from_secs(1)),
            ..EntryContext::default()
        };
        rt.start_with_context(timeout_ctx)
            .expect("start with timeout should succeed");
        assert!(!draw_effective_enabled());

        rt.tick(Instant::now()).expect("tick should restore state");
        assert!(draw_effective_enabled());
        reset_runtime(rt);
    }

    #[test]
    fn already_active_start_does_not_clobber_prior_state_snapshot() {
        let rt = runtime();
        reset_runtime(rt);

        rt.start().expect("first start should succeed");
        let initial_ctx = rt
            .entry_context_for_test()
            .expect("entry context should be present");

        apply_runtime_settings(MouseGestureSettings {
            enabled: false,
            ..MouseGestureSettings::default()
        });
        assert_eq!(
            rt.start().expect("already active should not fail"),
            StartOutcome::AlreadyActive
        );

        let after_ctx = rt
            .entry_context_for_test()
            .expect("entry context should remain present");
        assert_eq!(after_ctx, initial_ctx);
        reset_runtime(rt);
    }

    #[test]
    fn state_machine_rejects_invalid_transitions() {
        let cases = [
            (DrawLifecycle::Idle, DrawLifecycle::Active),
            (DrawLifecycle::Idle, DrawLifecycle::Exiting),
            (DrawLifecycle::Active, DrawLifecycle::Starting),
            (DrawLifecycle::Exiting, DrawLifecycle::Active),
            (DrawLifecycle::Restoring, DrawLifecycle::Active),
        ];

        for (from, to) in cases {
            assert!(
                !can_transition(from, to),
                "unexpected transition {from:?} -> {to:?}"
            );
        }
    }
}
