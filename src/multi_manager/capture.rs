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
    pub identity: Option<crate::window_catalog::WindowNativeIdentity>,
}

pub struct CaptureSession {
    pub rx: mpsc::Receiver<CaptureEvent>,
    cancel: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    reservation: Option<CaptureReservation>,
    completed: bool,
    authority: Arc<dyn Fn() -> bool + Send + Sync>,
    #[cfg(all(windows, not(test)))]
    hook_thread_id: Option<Arc<std::sync::Mutex<Option<u32>>>>,
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
            reservation: None,
            completed: false,
            authority: Arc::new(|| true),
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

static CAPTURE_RESERVED: AtomicBool = AtomicBool::new(false);

struct CaptureReservation;
impl CaptureReservation {
    fn acquire() -> Result<Self, CaptureStartError> {
        CAPTURE_RESERVED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| CaptureStartError::Busy)
    }
}
impl Drop for CaptureReservation {
    fn drop(&mut self) {
        CAPTURE_RESERVED.store(false, Ordering::Release);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureStartError {
    Busy,
    Cancelled,
    NativeUnavailable,
}

#[derive(Clone, Copy)]
pub enum CaptureMode {
    HookRequired,
    HookOrPolling,
}

#[derive(Clone)]
pub struct CaptureCancel {
    cancel: Arc<AtomicBool>,
    authority: Arc<dyn Fn() -> bool + Send + Sync>,
    #[cfg(all(windows, not(test)))]
    hook_thread_id: Option<Arc<std::sync::Mutex<Option<u32>>>>,
}
impl CaptureCancel {
    pub fn new() -> Self {
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
            authority: Arc::new(|| true),
            #[cfg(all(windows, not(test)))]
            hook_thread_id: Some(Arc::new(std::sync::Mutex::new(None))),
        }
    }
    pub fn with_authority(authority: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        let mut control = Self::new();
        control.authority = authority;
        control
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
        #[cfg(all(windows, not(test)))]
        if let Some(thread_id) = &self.hook_thread_id {
            if let Ok(thread_id) = thread_id.lock() {
                if let Some(thread_id) = *thread_id {
                    windows_backend::post_stop_message(thread_id);
                }
            }
        }
    }
}

impl CaptureSession {
    /// Wait off the UI thread until the control key is released and the owner is reaped.
    pub fn wait(mut self) -> Option<CaptureEvent> {
        let event = loop {
            match self.rx.recv_timeout(Duration::from_millis(50)) {
                Ok(event) => break Some(event),
                Err(mpsc::RecvTimeoutError::Disconnected) => break None,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if !(self.authority)() {
                        self.cancel();
                        break None;
                    }
                }
            }
        };
        if let Some(join) = self.join.take() {
            while !join.is_finished() {
                if !(self.authority)() {
                    self.cancel();
                }
                thread::sleep(Duration::from_millis(12));
            }
            let _ = join.join();
        }
        let cancelled = self.cancel.load(Ordering::Acquire);
        self.completed = true;
        if cancelled { None } else { event }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
        #[cfg(all(windows, not(test)))]
        if let Some(thread_id) = &self.hook_thread_id {
            if let Ok(thread_id) = thread_id.lock() {
                if let Some(thread_id) = *thread_id {
                    windows_backend::post_stop_message(thread_id);
                }
            }
        }
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        info!("capture session stop requested");
        self.cancel.store(true, Ordering::Relaxed);
        #[cfg(all(windows, not(test)))]
        if let Some(thread_id) = &self.hook_thread_id {
            if let Ok(thread_id) = thread_id.lock() {
                if let Some(thread_id) = *thread_id {
                    windows_backend::post_stop_message(thread_id);
                }
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

/// Egui adapter over the single process-wide capture owner.
#[cfg(not(test))]
pub fn start_capture_session(
    ctx: eframe::egui::Context,
) -> Result<CaptureSession, CaptureStartError> {
    start_capture_session_with_wake(
        Arc::new(move || ctx.request_repaint()),
        CaptureCancel::new(),
        CaptureMode::HookOrPolling,
    )
}
#[cfg(test)]
pub fn start_capture_session(
    _: eframe::egui::Context,
) -> Result<CaptureSession, CaptureStartError> {
    Ok(CaptureSession::test_empty())
}

pub fn start_capture_session_with_wake(
    wake: Arc<dyn Fn() + Send + Sync>,
    control: CaptureCancel,
    mode: CaptureMode,
) -> Result<CaptureSession, CaptureStartError> {
    if control.cancel.load(Ordering::Acquire) || !(control.authority)() {
        return Err(CaptureStartError::Cancelled);
    }
    let reservation = CaptureReservation::acquire()?;
    if control.cancel.load(Ordering::Acquire) || !(control.authority)() {
        return Err(CaptureStartError::Cancelled);
    }
    #[cfg(all(windows, not(test)))]
    {
        match windows_backend::start_hook_capture_session(wake.clone(), control.clone(), mode) {
            Ok(mut session) => {
                session.reservation = Some(reservation);
                return Ok(session);
            }
            Err(err) => {
                if control.cancel.load(Ordering::Acquire) {
                    return Err(CaptureStartError::Cancelled);
                }
                if matches!(mode, CaptureMode::HookRequired) {
                    return Err(CaptureStartError::NativeUnavailable);
                }
                warn!(error = %err, "capture hook install failed; falling back to polling");
            }
        }
    }
    #[cfg(any(not(windows), test))]
    if matches!(mode, CaptureMode::HookRequired) {
        return Err(CaptureStartError::NativeUnavailable);
    }
    let mut session = start_polling_capture_session(wake, control);
    session.reservation = Some(reservation);
    Ok(session)
}

fn start_polling_capture_session(
    wake: Arc<dyn Fn() + Send + Sync>,
    control: CaptureCancel,
) -> CaptureSession {
    let (tx, rx) = mpsc::channel();
    let cancel = control.cancel;
    let thread_cancel = Arc::clone(&cancel);
    let join = thread::spawn(move || {
        let mut detector = CaptureKeyEdgeDetector::new(current_snapshot());
        while !thread_cancel.load(Ordering::Acquire) {
            thread::sleep(POLL_INTERVAL);
            if let Some(action) = detector.update(current_snapshot()) {
                let (captured, identity) = capture_foreground_window(action);
                if !thread_cancel.load(Ordering::Acquire) {
                    let _ = tx.send(CaptureEvent {
                        action,
                        captured,
                        identity,
                    });
                    wake();
                }
                while !thread_cancel.load(Ordering::Acquire) && current_snapshot().any_down() {
                    thread::sleep(POLL_INTERVAL);
                }
                break;
            }
        }
    });
    CaptureSession {
        rx,
        cancel,
        join: Some(join),
        reservation: None,
        completed: false,
        authority: control.authority,
        #[cfg(all(windows, not(test)))]
        hook_thread_id: control.hook_thread_id,
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

fn capture_foreground_window(
    action: CaptureKeyAction,
) -> (
    Option<CapturedWindow>,
    Option<crate::window_catalog::WindowNativeIdentity>,
) {
    if action != CaptureKeyAction::Confirm {
        return (None, None);
    }
    #[cfg(windows)]
    let identity = {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        crate::window_catalog::native_window_identity(unsafe { GetForegroundWindow() }.0 as usize)
    };
    #[cfg(not(windows))]
    let identity: Option<crate::window_catalog::WindowNativeIdentity> = None;
    let captured = win::active_window();
    #[cfg(windows)]
    let foreground = {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        unsafe { GetForegroundWindow() }.0 as usize
    };
    #[cfg(not(windows))]
    let foreground = 0;
    let verified = captured
        .as_ref()
        .zip(identity.as_ref())
        .is_some_and(|(window, identity)| {
            foreground == window.hwnd
                && identity.hwnd == window.hwnd
                && crate::window_catalog::native_window_identity(window.hwnd).as_ref()
                    == Some(identity)
        });
    log_capture_metadata(&captured);
    (captured, if verified { identity } else { None })
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

#[cfg(any(windows, test))]
struct HookKeys {
    held: u32,
    claimed: u32,
    fired: bool,
}
#[cfg(any(windows, test))]
impl HookKeys {
    fn new(held: u32) -> Self {
        Self {
            held,
            claimed: 0,
            fired: false,
        }
    }
    fn update(&mut self, bit: u32, down: bool) -> (bool, bool, bool) {
        if down {
            let previous = self.held;
            self.held |= bit;
            if !self.fired && previous == 0 {
                self.claimed = bit;
                self.fired = true;
                return (true, true, false);
            }
            (self.claimed == bit, false, false)
        } else {
            self.held &= !bit;
            let claimed = self.claimed == bit;
            if claimed {
                self.claimed = 0;
            }
            (claimed, false, claimed)
        }
    }
}

#[cfg(all(windows, not(test)))]
mod windows_backend {
    use super::*;
    use std::sync::Mutex;
    use std::sync::OnceLock;
    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG, PM_NOREMOVE, PeekMessageW,
        PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN,
        WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    #[derive(Clone)]
    struct HookCallbackState {
        tx: mpsc::Sender<CaptureEvent>,
        wake: Arc<dyn Fn() + Send + Sync>,
        cancel: Arc<AtomicBool>,
        authority: Arc<dyn Fn() -> bool + Send + Sync>,
        thread_id: u32,
        keys: Arc<Mutex<HookKeys>>,
        ready: Arc<AtomicBool>,
        allow_skip: bool,
    }

    struct HookState {
        callback: HookCallbackState,
        lifecycle: HookLifecycleState,
    }

    static HOOK_STATE: OnceLock<Mutex<Option<HookState>>> = OnceLock::new();

    pub fn start_hook_capture_session(
        wake: Arc<dyn Fn() + Send + Sync>,
        control: CaptureCancel,
        mode: CaptureMode,
    ) -> Result<CaptureSession, String> {
        let (tx, rx) = mpsc::channel();
        let (setup_tx, setup_rx) = mpsc::channel();
        let cancel = control.cancel;
        let authority = control.authority;
        let thread_authority = authority.clone();
        let thread_cancel = Arc::clone(&cancel);
        let hook_thread_id = control.hook_thread_id.expect("hook cancel state");
        let hook_thread_id_for_thread = Arc::clone(&hook_thread_id);

        let join = thread::spawn(move || {
            let thread_id = unsafe { GetCurrentThreadId() };
            let mut queue_message = MSG::default();
            unsafe {
                let _ = PeekMessageW(&mut queue_message, None, 0, 0, PM_NOREMOVE);
            }
            struct ThreadReceipt(Arc<Mutex<Option<u32>>>);
            impl Drop for ThreadReceipt {
                fn drop(&mut self) {
                    if let Ok(mut id) = self.0.lock() {
                        *id = None;
                    }
                }
            }
            let _receipt = ThreadReceipt(hook_thread_id_for_thread.clone());
            if let Ok(mut id) = hook_thread_id_for_thread.lock() {
                *id = Some(thread_id);
            }
            if thread_cancel.load(Ordering::Acquire) {
                let _ = setup_tx.send(Err("cancelled".into()));
                return;
            }
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
                            wake,
                            cancel: Arc::clone(&thread_cancel),
                            authority: thread_authority.clone(),
                            thread_id,
                            keys: Arc::new(Mutex::new(HookKeys::new(snapshot_bits(
                                current_snapshot(),
                            )))),
                            ready: Arc::new(AtomicBool::new(false)),
                            allow_skip: matches!(mode, CaptureMode::HookOrPolling),
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
                    if let Some(state) = hook_callback_state() {
                        if let Ok(mut keys) = state.keys.lock() {
                            *keys = HookKeys::new(
                                snapshot_bits(current_snapshot())
                                    & if state.allow_skip { 7 } else { 3 },
                            );
                        }
                    }
                    if let Some(state) = hook_callback_state() {
                        state.ready.store(true, Ordering::Release);
                    }
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
                reservation: None,
                completed: false,
                authority,
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

    fn snapshot_bits(snapshot: CaptureKeySnapshot) -> u32 {
        u32::from(snapshot.enter) | (u32::from(snapshot.escape) << 1) | (u32::from(snapshot.s) << 2)
    }

    unsafe extern "system" fn low_level_keyboard_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code >= 0 {
            let message = wparam.0 as u32;
            if matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP) {
                let kb = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
                if let Some(action) = virtual_key_to_capture_action(kb.vkCode) {
                    if let Some(callback) = hook_callback_state() {
                        if !callback.ready.load(Ordering::Acquire)
                            || (!callback.allow_skip && action == CaptureKeyAction::Skip)
                            || callback.cancel.load(Ordering::Acquire)
                            || !(callback.authority)()
                        {
                            return unsafe {
                                CallNextHookEx(HHOOK::default(), code, wparam, lparam)
                            };
                        }
                        let bit = match action {
                            CaptureKeyAction::Confirm => 1,
                            CaptureKeyAction::Cancel => 2,
                            CaptureKeyAction::Skip => 4,
                        };
                        let down = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
                        let decision = callback
                            .keys
                            .lock()
                            .ok()
                            .map(|mut keys| keys.update(bit, down));
                        if let Some((suppress, fire, finished)) = decision {
                            if fire {
                                let (captured, identity) = capture_foreground_window(action);
                                if !callback.cancel.load(Ordering::Acquire) {
                                    let _ = callback.tx.send(CaptureEvent {
                                        action,
                                        captured,
                                        identity,
                                    });
                                    (callback.wake)();
                                }
                            }
                            if finished {
                                post_stop_message(callback.thread_id);
                            }
                            if suppress {
                                return LRESULT(1);
                            }
                        }
                    }
                }
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
        let _helper: fn(Arc<dyn Fn() + Send + Sync>, CaptureCancel) -> CaptureSession =
            start_polling_capture_session;
    }

    #[test]
    fn hook_claims_only_fresh_control_and_its_repeats_until_release() {
        let mut keys = HookKeys::new(1);
        assert_eq!(keys.update(1, false), (false, false, false));
        assert_eq!(keys.update(1, true), (true, true, false));
        assert_eq!(keys.update(1, true), (true, false, false));
        assert_eq!(keys.update(2, true), (false, false, false));
        assert_eq!(keys.update(2, false), (false, false, false));
        assert_eq!(keys.update(1, false), (true, false, true));
    }

    #[test]
    fn reaping_session_does_not_cancel_reusable_control() {
        let session = CaptureSession::test_empty();
        let cancelled = session.cancel.clone();
        assert!(session.wait().is_none());
        assert!(!cancelled.load(Ordering::Acquire));
    }

    #[test]
    fn cancelled_setup_never_reserves_owner() {
        let control = CaptureCancel::new();
        control.cancel();
        assert!(matches!(
            start_capture_session_with_wake(Arc::new(|| {}), control, CaptureMode::HookRequired),
            Err(CaptureStartError::Cancelled)
        ));
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
