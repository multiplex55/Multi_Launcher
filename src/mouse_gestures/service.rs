use crate::mouse_gestures::db::SharedGestureDb;
use crate::mouse_gestures::engine::{DirMode, GestureTracker};
use crate::mouse_gestures::overlay::{DefaultOverlayBackend, HintOverlay, TrailOverlay};
use anyhow::anyhow;
use once_cell::sync::OnceCell;
#[cfg(windows)]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct MouseGestureConfig {
    pub enabled: bool,
    pub trail_interval_ms: u64,
    pub recognition_interval_ms: u64,
    pub deadzone_px: f32,
    pub trail_start_move_px: f32,
    pub show_trail: bool,
    pub trail_color: [u8; 4],
    pub trail_width: f32,
    pub show_hint: bool,
    pub hint_offset: (f32, f32),
    pub dir_mode: DirMode,
    pub threshold_px: f32,
    pub long_threshold_x: f32,
    pub long_threshold_y: f32,
    pub max_tokens: usize,
}

impl Default for MouseGestureConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            trail_interval_ms: 16,
            recognition_interval_ms: 40,
            deadzone_px: 12.0,
            trail_start_move_px: 8.0,
            show_trail: true,
            trail_color: [0xff, 0x00, 0x00, 0xff],
            trail_width: 2.0,
            show_hint: true,
            hint_offset: (16.0, 16.0),
            dir_mode: DirMode::Four,
            threshold_px: 8.0,
            long_threshold_x: 30.0,
            long_threshold_y: 30.0,
            max_tokens: 10,
        }
    }
}

use crate::gui::{send_event, WatchEvent};

#[derive(Debug, Clone, Copy)]
pub enum HookEvent {
    RButtonDown,
    RButtonUp,
}

pub trait HookBackend: Send {
    fn install(&mut self, sender: Sender<HookEvent>) -> anyhow::Result<()>;
    fn uninstall(&mut self) -> anyhow::Result<()>;
    fn is_installed(&self) -> bool;
}

#[derive(Debug)]
struct WorkerHandle {
    stop_tx: Sender<()>,
    join: JoinHandle<()>,
}

pub struct MouseGestureService {
    config: MouseGestureConfig,
    db: Option<SharedGestureDb>,
    backend: Box<dyn HookBackend>,
    worker: Option<WorkerHandle>,
}

impl Default for MouseGestureService {
    fn default() -> Self {
        Self::new_with_backend(Box::new(DefaultHookBackend::default()))
    }
}

impl MouseGestureService {
    pub fn new_with_backend(backend: Box<dyn HookBackend>) -> Self {
        Self {
            config: MouseGestureConfig::default(),
            db: None,
            backend,
            worker: None,
        }
    }

    pub fn start(&mut self) {
        self.config.enabled = true;
        self.start_running();
    }

    pub fn stop(&mut self) {
        self.config.enabled = false;
        self.stop_running();
    }

    pub fn update_config(&mut self, config: MouseGestureConfig) {
        let enabled = config.enabled;
        let should_restart = self.worker.is_some();
        self.config = config;
        if enabled {
            if should_restart {
                self.stop_running();
            }
            self.start_running();
        } else {
            self.stop_running();
        }
    }

    pub fn update_db(&mut self, db: Option<SharedGestureDb>) {
        self.db = db;
    }

    pub fn is_running(&self) -> bool {
        self.worker.is_some()
    }

    fn start_running(&mut self) {
        if self.worker.is_some() || !self.config.enabled {
            return;
        }

        let (event_tx, event_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();

        if let Err(err) = self.backend.install(event_tx) {
            tracing::error!(?err, "failed to install mouse hook");
            return;
        }

        let config = self.config.clone();
        let db = self.db.clone();
        let join = thread::spawn(move || worker_loop(config, db, event_rx, stop_rx));
        self.worker = Some(WorkerHandle { stop_tx, join });
    }

    fn stop_running(&mut self) {
        if self.worker.is_none() && !self.backend.is_installed() {
            return;
        }

        if let Err(err) = self.backend.uninstall() {
            tracing::error!(?err, "failed to uninstall mouse hook");
        }

        if let Some(worker) = self.worker.take() {
            let _ = worker.stop_tx.send(());
            let _ = worker.join.join();
        }
    }
}

static SERVICE: OnceCell<Mutex<MouseGestureService>> = OnceCell::new();

pub fn with_service<F>(f: F)
where
    F: FnOnce(&mut MouseGestureService),
{
    let service = SERVICE.get_or_init(|| Mutex::new(MouseGestureService::default()));
    match service.lock() {
        Ok(mut guard) => f(&mut guard),
        Err(err) => tracing::error!(?err, "failed to lock mouse gesture service"),
    }
}

fn worker_loop(
    config: MouseGestureConfig,
    db: Option<SharedGestureDb>,
    event_rx: Receiver<HookEvent>,
    stop_rx: Receiver<()>,
) {
    let mut tracker = GestureTracker::new(
        config.dir_mode,
        config.threshold_px,
        config.long_threshold_x,
        config.long_threshold_y,
        config.max_tokens,
    );
    let mut trail_overlay = TrailOverlay::new(
        DefaultOverlayBackend::default(),
        config.show_trail,
        config.trail_color,
        config.trail_width,
        config.trail_start_move_px,
    );
    let mut hint_overlay = HintOverlay::new(
        DefaultOverlayBackend::default(),
        config.show_hint,
        config.hint_offset,
    );
    let poll_interval = Duration::from_millis(config.trail_interval_ms.max(1));
    let recognition_interval = Duration::from_millis(config.recognition_interval_ms.max(1));
    let mut active = false;
    let mut exceeded_deadzone = false;
    let mut start_pos = (0.0_f32, 0.0_f32);
    let mut last_trail = Instant::now();
    let mut last_recognition = Instant::now();
    let start_time = Instant::now();

    loop {
        if stop_rx.try_recv().is_ok() {
            break;
        }

        match event_rx.recv_timeout(poll_interval) {
            Ok(event) => match event {
                HookEvent::RButtonDown => {
                    active = true;
                    exceeded_deadzone = false;
                    tracker.reset();
                    let pos = get_cursor_position().unwrap_or(start_pos);
                    start_pos = pos;
                    let ms = start_time.elapsed().as_millis() as u64;
                    tracker.feed_point(pos, ms);
                    trail_overlay.reset(pos);
                    hint_overlay.reset();
                    last_trail = Instant::now();
                    last_recognition = last_trail;
                }
                HookEvent::RButtonUp => {
                    if active {
                        if exceeded_deadzone {
                            let tokens = tracker.tokens_string();
                            if let Some(action) =
                                match_binding_action(&db, &tokens, config.dir_mode)
                            {
                                send_event(WatchEvent::ExecuteAction(action));
                            }
                        } else {
                            send_right_click();
                        }
                        active = false;
                        tracker.reset();
                        hint_overlay.reset();
                    }
                }
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if active && last_trail.elapsed() >= poll_interval {
            if let Some(pos) = get_cursor_position() {
                let dx = pos.0 - start_pos.0;
                let dy = pos.1 - start_pos.1;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq >= config.deadzone_px * config.deadzone_px {
                    exceeded_deadzone = true;
                }

                trail_overlay.update_position(pos);

                if last_recognition.elapsed() >= recognition_interval {
                    let ms = start_time.elapsed().as_millis() as u64;
                    let _ = tracker.feed_point(pos, ms);
                    let tokens = tracker.tokens_string();
                    let best_match = best_match_name(&db, &tokens, config.dir_mode);
                    hint_overlay.update(&tokens, best_match.as_deref(), pos);
                    last_recognition = Instant::now();
                }
            }
            last_trail = Instant::now();
        }
    }
}

fn match_binding_action(
    db: &Option<SharedGestureDb>,
    tokens: &str,
    dir_mode: DirMode,
) -> Option<crate::actions::Action> {
    let db = db.as_ref()?;
    let guard = db.lock().ok()?;
    guard
        .match_binding_owned(tokens, dir_mode)
        .map(|(label, binding)| binding.to_action(&label))
}

fn best_match_name(
    db: &Option<SharedGestureDb>,
    tokens: &str,
    dir_mode: DirMode,
) -> Option<String> {
    let db = db.as_ref()?;
    let guard = db.lock().ok()?;
    guard
        .match_binding_owned(tokens, dir_mode)
        .map(|(label, binding)| format!("{}: {}", label, binding.label))
}

#[cfg(windows)]
fn get_cursor_position() -> Option<(f32, f32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut point).is_ok() } {
        Some((point.x as f32, point.y as f32))
    } else {
        None
    }
}

#[cfg(not(windows))]
fn get_cursor_position() -> Option<(f32, f32)> {
    None
}

#[cfg(windows)]
fn send_right_click() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        MOUSEINPUT,
    };

    let down = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_RIGHTDOWN,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let up = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_RIGHTUP,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    let _ = unsafe { SendInput(&[down, up], std::mem::size_of::<INPUT>() as i32) };
}

#[cfg(not(windows))]
fn send_right_click() {}

#[cfg(windows)]
#[derive(Default)]
pub struct DefaultHookBackend {
    hook: Option<windows::Win32::UI::WindowsAndMessaging::HHOOK>,
}

#[cfg(windows)]
unsafe impl Send for DefaultHookBackend {}

#[cfg(windows)]
impl HookBackend for DefaultHookBackend {
    fn install(&mut self, sender: Sender<HookEvent>) -> anyhow::Result<()> {
        if self.hook.is_some() {
            return Ok(());
        }

        hook_dispatch().set_sender(Some(sender));
        hook_dispatch().set_enabled(true);

        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowsHookExW, WH_MOUSE_LL};

        let hmodule = unsafe { GetModuleHandleW(None) }?;
        let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), hmodule, 0) }?;
        if hook.0.is_null() {
            hook_dispatch().set_enabled(false);
            hook_dispatch().set_sender(None);
            return Err(anyhow!(windows::core::Error::from_win32()));
        }
        self.hook = Some(hook);
        Ok(())
    }

    fn uninstall(&mut self) -> anyhow::Result<()> {
        hook_dispatch().set_enabled(false);
        hook_dispatch().set_sender(None);

        use windows::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx;

        if let Some(hook) = self.hook.take() {
            unsafe {
                let _ = UnhookWindowsHookEx(hook);
            }
        }
        Ok(())
    }

    fn is_installed(&self) -> bool {
        self.hook.is_some()
    }
}

#[cfg(not(windows))]
#[derive(Default)]
pub struct DefaultHookBackend;

#[cfg(not(windows))]
impl HookBackend for DefaultHookBackend {
    fn install(&mut self, _sender: Sender<HookEvent>) -> anyhow::Result<()> {
        Err(anyhow!("mouse hooks are not supported on this platform"))
    }

    fn uninstall(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn is_installed(&self) -> bool {
        false
    }
}

#[derive(Clone)]
pub struct MockHookBackend {
    state: Arc<MockHookState>,
}

#[derive(Default)]
struct MockHookState {
    install_count: AtomicUsize,
    uninstall_count: AtomicUsize,
    sender: Mutex<Option<Sender<HookEvent>>>,
}

impl MockHookBackend {
    pub fn new() -> (Self, MockHookHandle) {
        let state = Arc::new(MockHookState::default());
        (
            Self {
                state: Arc::clone(&state),
            },
            MockHookHandle { state },
        )
    }
}

impl HookBackend for MockHookBackend {
    fn install(&mut self, sender: Sender<HookEvent>) -> anyhow::Result<()> {
        let mut guard = self.state.sender.lock().map_err(|_| anyhow!("lock"))?;
        if guard.is_none() {
            self.state.install_count.fetch_add(1, Ordering::SeqCst);
            *guard = Some(sender);
        }
        Ok(())
    }

    fn uninstall(&mut self) -> anyhow::Result<()> {
        let mut guard = self.state.sender.lock().map_err(|_| anyhow!("lock"))?;
        if guard.is_some() {
            self.state.uninstall_count.fetch_add(1, Ordering::SeqCst);
        }
        *guard = None;
        Ok(())
    }

    fn is_installed(&self) -> bool {
        match self.state.sender.lock() {
            Ok(guard) => guard.is_some(),
            Err(_) => false,
        }
    }
}

pub struct MockHookHandle {
    state: Arc<MockHookState>,
}

impl MockHookHandle {
    pub fn install_count(&self) -> usize {
        self.state.install_count.load(Ordering::SeqCst)
    }

    pub fn uninstall_count(&self) -> usize {
        self.state.uninstall_count.load(Ordering::SeqCst)
    }

    pub fn emit(&self, event: HookEvent) -> bool {
        match self.state.sender.lock() {
            Ok(guard) => guard
                .as_ref()
                .map(|sender| sender.send(event).is_ok())
                .unwrap_or(false),
            Err(_) => false,
        }
    }
}

#[cfg(windows)]
struct HookDispatch {
    enabled: AtomicBool,
    sender: Mutex<Option<Sender<HookEvent>>>,
}

#[cfg(windows)]
impl HookDispatch {
    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
    }

    fn set_sender(&self, sender: Option<Sender<HookEvent>>) {
        if let Ok(mut guard) = self.sender.lock() {
            *guard = sender;
        }
    }
}

#[cfg(windows)]
static HOOK_DISPATCH: OnceCell<HookDispatch> = OnceCell::new();

#[cfg(windows)]
fn hook_dispatch() -> &'static HookDispatch {
    HOOK_DISPATCH.get_or_init(|| HookDispatch {
        enabled: AtomicBool::new(false),
        sender: Mutex::new(None),
    })
}

#[cfg(windows)]
unsafe extern "system" fn mouse_hook_proc(
    n_code: i32,
    w_param: windows::Win32::Foundation::WPARAM,
    l_param: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, HC_ACTION, WM_RBUTTONDOWN, WM_RBUTTONUP,
    };

    if n_code == HC_ACTION as i32 {
        let msg = w_param.0 as u32;
        if msg == WM_RBUTTONDOWN || msg == WM_RBUTTONUP {
            let dispatch = hook_dispatch();
            if dispatch.enabled.load(Ordering::Acquire) {
                if let Ok(guard) = dispatch.sender.try_lock() {
                    if let Some(sender) = guard.as_ref() {
                        let event = if msg == WM_RBUTTONDOWN {
                            HookEvent::RButtonDown
                        } else {
                            HookEvent::RButtonUp
                        };
                        let _ = sender.send(event);
                    }
                }
                return windows::Win32::Foundation::LRESULT(1);
            }
        }
    }

    CallNextHookEx(
        windows::Win32::UI::WindowsAndMessaging::HHOOK(std::ptr::null_mut()),
        n_code,
        w_param,
        l_param,
    )
}
