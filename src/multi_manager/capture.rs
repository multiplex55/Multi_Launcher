//! Capture mode is a temporary, process-global input state used while the user
//! focuses the target application they want to capture. Because focus is outside
//! Multi Launcher during this mode, capture-control keys must be observed
//! globally rather than through egui focus. `Enter`, `Escape`, and `S` are the
//! capture-control keys; whenever this module handles one of them it must
//! suppress that key so the focused target application does not also receive it.

use crate::multi_manager::win::{self, CaptureKeyAction, CapturedWindow};
use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
#[cfg(all(windows, not(test)))]
use tracing::error;
#[cfg(windows)]
use tracing::warn;
use tracing::{debug, info};

const POLL_INTERVAL: Duration = Duration::from_millis(12);
const VK_ENTER: u32 = 0x0D;
const VK_ESCAPE: u32 = 0x1B;
const VK_S: u32 = 0x53;

#[derive(Debug, Clone)]
pub struct CaptureEvent {
    pub action: CaptureKeyAction,
    pub captured: Option<CapturedWindow>,
}

pub struct CaptureSession {
    pub rx: mpsc::Receiver<CaptureEvent>,
    cancel: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    #[cfg(all(windows, not(test)))]
    hook_thread_id: Option<Arc<std::sync::atomic::AtomicU32>>,
    #[cfg(test)]
    test_id: usize,
}

#[cfg(test)]
static NEXT_TEST_CAPTURE_SESSION_ID: AtomicUsize = AtomicUsize::new(1);

impl CaptureSession {
    #[cfg(test)]
    pub(crate) fn test_empty() -> Self {
        let (_tx, rx) = mpsc::channel();
        Self {
            rx,
            cancel: Arc::new(AtomicBool::new(false)),
            join: None,
            #[cfg(all(windows, not(test)))]
            hook_thread_id: None,
            test_id: NEXT_TEST_CAPTURE_SESSION_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    #[cfg(test)]
    pub(crate) fn test_id(&self) -> usize {
        self.test_id
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        info!("capture session stop requested");
        self.cancel.store(true, Ordering::Relaxed);
        #[cfg(all(windows, not(test)))]
        if let Some(thread_id) = &self.hook_thread_id {
            let thread_id = thread_id.load(Ordering::Relaxed);
            if thread_id != 0 {
                windows_backend::post_stop_message(thread_id);
            } else {
                debug!("capture hook thread id unavailable during shutdown");
            }
        }
        if let Some(join) = self.join.take()
            && join.join().is_err()
        {
            #[cfg(windows)]
            warn!("capture session thread panicked during shutdown");
            #[cfg(not(windows))]
            debug!("capture session thread panicked during shutdown");
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CaptureKeySnapshot {
    pub enter: bool,
    pub escape: bool,
    pub s: bool,
}

impl CaptureKeySnapshot {
    fn any_down(self) -> bool {
        self.enter || self.escape || self.s
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct HookLifecycleState {
    installed: bool,
    callback_state_present: bool,
}

impl HookLifecycleState {
    fn reserve(&mut self) -> Result<(), &'static str> {
        if self.callback_state_present {
            return Err("another keyboard hook capture session is already active");
        }
        self.callback_state_present = true;
        Ok(())
    }

    fn mark_installed(&mut self) {
        self.installed = true;
    }

    fn clear(&mut self) -> bool {
        let was_stale = self.callback_state_present && !self.installed;
        self.installed = false;
        self.callback_state_present = false;
        was_stale
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureKeyEdgeDetector {
    armed: bool,
    previous: CaptureKeySnapshot,
}

impl CaptureKeyEdgeDetector {
    pub fn new(initial: CaptureKeySnapshot) -> Self {
        Self {
            armed: !initial.any_down(),
            previous: initial,
        }
    }

    pub fn update(&mut self, current: CaptureKeySnapshot) -> Option<CaptureKeyAction> {
        if !self.armed {
            self.previous = current;
            if !current.any_down() {
                self.armed = true;
            }
            return None;
        }

        let action = if current.enter && !self.previous.enter {
            Some(CaptureKeyAction::Confirm)
        } else if current.escape && !self.previous.escape {
            Some(CaptureKeyAction::Cancel)
        } else if current.s && !self.previous.s {
            Some(CaptureKeyAction::Skip)
        } else {
            None
        };
        self.previous = current;
        action
    }
}

#[cfg(all(windows, not(test)))]
pub fn start_capture_session(ctx: eframe::egui::Context) -> CaptureSession {
    info!("capture session start requested");
    info!(backend = "hook", "selected capture backend");
    match windows_backend::start_hook_capture_session(ctx.clone()) {
        Ok(session) => session,
        Err(err) => {
            warn!(error = %err, fallback_backend = "polling", "capture hook install failed; falling back to polling backend");
            start_polling_capture_session(ctx)
        }
    }
}

#[cfg(all(not(windows), not(test)))]
pub fn start_capture_session(ctx: eframe::egui::Context) -> CaptureSession {
    info!("capture session start requested");
    info!(backend = "polling", "selected capture backend");
    start_polling_capture_session(ctx)
}

#[cfg(test)]
pub fn start_capture_session(_ctx: eframe::egui::Context) -> CaptureSession {
    info!("capture session start requested");
    info!(backend = "test", "selected capture backend");
    CaptureSession::test_empty()
}

fn start_polling_capture_session(ctx: eframe::egui::Context) -> CaptureSession {
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let thread_cancel = Arc::clone(&cancel);
    let join = thread::spawn(move || {
        let mut detector = CaptureKeyEdgeDetector::new(current_snapshot());
        while !thread_cancel.load(Ordering::Relaxed) {
            thread::sleep(POLL_INTERVAL);
            if let Some(action) = detector.update(current_snapshot()) {
                info!(?action, "capture control key received by polling backend");
                let captured = capture_foreground_window(action);
                let _ = tx.send(CaptureEvent { action, captured });
                ctx.request_repaint();
                break;
            }
        }
        debug!("polling capture session stopped");
    });

    CaptureSession {
        rx,
        cancel,
        join: Some(join),
        #[cfg(all(windows, not(test)))]
        hook_thread_id: None,
        #[cfg(test)]
        test_id: NEXT_TEST_CAPTURE_SESSION_ID.fetch_add(1, Ordering::Relaxed),
    }
}

fn virtual_key_to_capture_action(vk_code: u32) -> Option<CaptureKeyAction> {
    match vk_code {
        VK_ENTER => Some(CaptureKeyAction::Confirm),
        VK_ESCAPE => Some(CaptureKeyAction::Cancel),
        VK_S => Some(CaptureKeyAction::Skip),
        _ => None,
    }
}

fn current_snapshot() -> CaptureKeySnapshot {
    CaptureKeySnapshot {
        enter: win::capture_key_is_down(VK_ENTER),
        escape: win::capture_key_is_down(VK_ESCAPE),
        s: win::capture_key_is_down(VK_S),
    }
}

fn capture_foreground_window(action: CaptureKeyAction) -> Option<CapturedWindow> {
    if action != CaptureKeyAction::Confirm {
        return None;
    }

    info!("foreground capture attempted");
    let captured = win::active_window();
    log_capture_metadata(&captured);
    captured
}

fn log_capture_metadata(captured: &Option<CapturedWindow>) {
    if let Some(window) = captured {
        info!(
            hwnd = window.hwnd,
            title = %window.title,
            executable = %window.executable,
            class_name = %window.class_name,
            process_path = %window.process_path,
            "foreground capture succeeded"
        );
    } else {
        info!("foreground capture returned none");
    }
}

#[cfg(all(windows, not(test)))]
mod windows_backend {
    use super::*;
    use std::sync::Mutex;
    use std::sync::OnceLock;
    use std::sync::atomic::AtomicU32;
    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG, PostThreadMessageW,
        SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
    };

    #[derive(Clone)]
    struct HookCallbackState {
        tx: mpsc::Sender<CaptureEvent>,
        ctx: eframe::egui::Context,
        cancel: Arc<AtomicBool>,
        thread_id: u32,
    }

    struct HookState {
        callback: HookCallbackState,
        lifecycle: HookLifecycleState,
    }

    static HOOK_STATE: OnceLock<Mutex<Option<HookState>>> = OnceLock::new();

    pub fn start_hook_capture_session(
        ctx: eframe::egui::Context,
    ) -> Result<CaptureSession, String> {
        let (tx, rx) = mpsc::channel();
        let (setup_tx, setup_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let thread_cancel = Arc::clone(&cancel);
        let hook_thread_id = Arc::new(AtomicU32::new(0));
        let hook_thread_id_for_thread = Arc::clone(&hook_thread_id);

        let join = thread::spawn(move || {
            let thread_id = unsafe { GetCurrentThreadId() };
            hook_thread_id_for_thread.store(thread_id, Ordering::Relaxed);
            let state_lock = HOOK_STATE.get_or_init(|| Mutex::new(None));
            match state_lock.lock() {
                Ok(mut state) if state.is_none() => {
                    let mut lifecycle = HookLifecycleState::default();
                    if let Err(err) = lifecycle.reserve() {
                        let _ = setup_tx.send(Err(err.to_string()));
                        return;
                    }
                    *state = Some(HookState {
                        callback: HookCallbackState {
                            tx,
                            ctx,
                            cancel: Arc::clone(&thread_cancel),
                            thread_id,
                        },
                        lifecycle,
                    });
                }
                Ok(mut state) => {
                    if let Some(stale) = state.as_mut().filter(|state| !state.lifecycle.installed) {
                        warn!("cleaning up stale Windows capture hook state before startup");
                        stale.lifecycle.clear();
                        *state = None;
                    }
                    let _ = setup_tx.send(Err(
                        "another keyboard hook capture session is already active".to_string(),
                    ));
                    return;
                }
                Err(_) => {
                    let _ = setup_tx.send(Err("keyboard hook state lock is poisoned".to_string()));
                    return;
                }
            }

            let hook = match unsafe {
                SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(low_level_keyboard_proc),
                    HINSTANCE::default(),
                    0,
                )
            } {
                Ok(hook) => {
                    mark_installed();
                    info!("capture hook install succeeded");
                    let _ = setup_tx.send(Ok(()));
                    hook
                }
                Err(err) => {
                    clear_state();
                    error!(error = %err, "capture hook install failed");
                    let _ = setup_tx.send(Err(err.to_string()));
                    return;
                }
            };

            let mut msg = MSG::default();
            while !thread_cancel.load(Ordering::Relaxed) {
                let result = unsafe { GetMessageW(&mut msg, None, 0, 0) };
                if result.0 <= 0 || msg.message == WM_QUIT {
                    break;
                }
            }

            info!("capture hook/message-loop cleanup started");
            match unsafe { UnhookWindowsHookEx(hook) } {
                Ok(()) => info!(result = "succeeded", "capture hook uninstall result"),
                Err(err) => warn!(result = "failed", error = %err, "capture hook uninstall result"),
            }
            clear_state();
        });

        match setup_rx.recv() {
            Ok(Ok(())) => Ok(CaptureSession {
                rx,
                cancel,
                join: Some(join),
                hook_thread_id: Some(hook_thread_id),
                #[cfg(test)]
                test_id: NEXT_TEST_CAPTURE_SESSION_ID.fetch_add(1, Ordering::Relaxed),
            }),
            Ok(Err(err)) => {
                let _ = join.join();
                Err(err)
            }
            Err(err) => {
                let _ = join.join();
                Err(err.to_string())
            }
        }
    }

    pub fn post_stop_message(thread_id: u32) {
        match unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) } {
            Ok(()) => debug!(thread_id, "posted capture hook message-loop wake"),
            Err(err) => {
                warn!(thread_id, error = %err, "failed to post capture hook message-loop wake")
            }
        }
    }

    unsafe extern "system" fn low_level_keyboard_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code >= 0 && (wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN) {
            let kb = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
            let action = virtual_key_to_capture_action(kb.vkCode);
            if let Some(action) = action {
                info!(
                    ?action,
                    "capture control key received by Windows hook backend"
                );
                let callback = hook_callback_state();
                if let Some(callback) = callback {
                    callback.cancel.store(true, Ordering::Relaxed);
                    let captured = capture_foreground_window(action);
                    let _ = callback.tx.send(CaptureEvent { action, captured });
                    callback.ctx.request_repaint();
                    post_stop_message(callback.thread_id);
                }
                return LRESULT(1);
            }
        }
        unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) }
    }

    fn hook_callback_state() -> Option<HookCallbackState> {
        HOOK_STATE
            .get()
            .and_then(|state_lock| state_lock.lock().ok())
            .and_then(|state| state.as_ref().map(|state| state.callback.clone()))
    }

    fn mark_installed() {
        if let Some(state_lock) = HOOK_STATE.get()
            && let Ok(mut state) = state_lock.lock()
            && let Some(state) = state.as_mut()
        {
            state.lifecycle.mark_installed();
        }
    }

    fn clear_state() {
        if let Some(state_lock) = HOOK_STATE.get()
            && let Ok(mut state) = state_lock.lock()
        {
            if let Some(state) = state.as_mut()
                && state.lifecycle.clear()
            {
                warn!("cleaning up stale Windows capture hook state");
            }
            *state = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(enter: bool, escape: bool, s: bool) -> CaptureKeySnapshot {
        CaptureKeySnapshot { enter, escape, s }
    }

    #[test]
    fn polling_fallback_helper_name_remains_available() {
        let _helper: fn(eframe::egui::Context) -> CaptureSession = start_polling_capture_session;
    }

    #[test]
    fn virtual_key_mapping_converts_capture_control_keys() {
        assert_eq!(
            virtual_key_to_capture_action(VK_ENTER),
            Some(CaptureKeyAction::Confirm)
        );
        assert_eq!(
            virtual_key_to_capture_action(VK_ESCAPE),
            Some(CaptureKeyAction::Cancel)
        );
        assert_eq!(
            virtual_key_to_capture_action(VK_S),
            Some(CaptureKeyAction::Skip)
        );
        assert_eq!(virtual_key_to_capture_action(0x41), None);
    }

    #[test]
    fn lifecycle_rejects_second_active_session() {
        let mut lifecycle = HookLifecycleState::default();
        assert_eq!(lifecycle.reserve(), Ok(()));
        assert_eq!(
            lifecycle.reserve(),
            Err("another keyboard hook capture session is already active")
        );
    }

    #[test]
    fn lifecycle_reports_stale_cleanup_before_install() {
        let mut lifecycle = HookLifecycleState::default();
        assert_eq!(lifecycle.reserve(), Ok(()));
        assert!(lifecycle.clear());
        assert_eq!(lifecycle, HookLifecycleState::default());
    }

    #[test]
    fn lifecycle_clear_after_install_is_not_stale() {
        let mut lifecycle = HookLifecycleState::default();
        assert_eq!(lifecycle.reserve(), Ok(()));
        lifecycle.mark_installed();
        assert!(!lifecycle.clear());
        assert_eq!(lifecycle, HookLifecycleState::default());
    }

    #[test]
    fn enter_down_once_produces_exactly_one_confirm_event() {
        let mut detector = CaptureKeyEdgeDetector::new(snap(false, false, false));
        assert_eq!(
            detector.update(snap(true, false, false)),
            Some(CaptureKeyAction::Confirm)
        );
        assert_eq!(detector.update(snap(true, false, false)), None);
        assert_eq!(detector.update(snap(false, false, false)), None);
    }

    #[test]
    fn holding_enter_does_not_repeat_confirm_events() {
        let mut detector = CaptureKeyEdgeDetector::new(snap(false, false, false));
        assert_eq!(
            detector.update(snap(true, false, false)),
            Some(CaptureKeyAction::Confirm)
        );
        for _ in 0..5 {
            assert_eq!(detector.update(snap(true, false, false)), None);
        }
    }

    #[test]
    fn escape_produces_cancel() {
        let mut detector = CaptureKeyEdgeDetector::new(snap(false, false, false));
        assert_eq!(
            detector.update(snap(false, true, false)),
            Some(CaptureKeyAction::Cancel)
        );
    }

    #[test]
    fn s_produces_skip() {
        let mut detector = CaptureKeyEdgeDetector::new(snap(false, false, false));
        assert_eq!(
            detector.update(snap(false, false, true)),
            Some(CaptureKeyAction::Skip)
        );
    }

    #[test]
    fn keys_held_at_session_start_are_ignored_until_released() {
        let mut detector = CaptureKeyEdgeDetector::new(snap(true, false, false));
        assert_eq!(detector.update(snap(true, false, false)), None);
        assert_eq!(detector.update(snap(false, false, false)), None);
        assert_eq!(
            detector.update(snap(true, false, false)),
            Some(CaptureKeyAction::Confirm)
        );
    }
}
