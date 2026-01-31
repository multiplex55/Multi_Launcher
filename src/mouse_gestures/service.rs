use crate::mouse_gestures::db::{load_gestures, SharedGestureDb, GESTURES_FILE};
use crate::mouse_gestures::engine::{DirMode, GestureTracker};
use crate::mouse_gestures::overlay::{DefaultOverlayBackend, HintOverlay, OverlayBackend, TrailOverlay};
use anyhow::anyhow;
use once_cell::sync::OnceCell;
#[cfg(windows)]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
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
    pub cancel_behavior: CancelBehavior,
    pub no_match_behavior: NoMatchBehavior,
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
            cancel_behavior: CancelBehavior::DoNothing,
            no_match_behavior: NoMatchBehavior::DoNothing,
        }
    }
}

use crate::gui::{send_event, WatchEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelBehavior {
    DoNothing,
    PassThroughClick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoMatchBehavior {
    DoNothing,
    PassThroughClick,
    ShowNoMatchHint,
}

#[derive(Debug, Clone, Copy)]
pub enum HookEvent {
    RButtonDown,
    RButtonUp,
    WheelUp,
    WheelDown,
    Cancel,
}

pub trait HookBackend: Send {
    fn install(&mut self, sender: Sender<HookEvent>) -> anyhow::Result<()>;
    fn uninstall(&mut self) -> anyhow::Result<()>;
    fn is_installed(&self) -> bool;
}

pub trait OverlayFactory: Send + Sync {
    fn trail_backend(&self) -> Box<dyn OverlayBackend>;
    fn hint_backend(&self) -> Box<dyn OverlayBackend>;
}

#[derive(Debug)]
struct DefaultOverlayFactory;

impl OverlayFactory for DefaultOverlayFactory {
    fn trail_backend(&self) -> Box<dyn OverlayBackend> {
        Box::new(DefaultOverlayBackend::default())
    }

    fn hint_backend(&self) -> Box<dyn OverlayBackend> {
        Box::new(DefaultOverlayBackend::default())
    }
}

pub trait RightClickBackend: Send + Sync {
    fn send_right_click(&self);
}

#[derive(Debug)]
struct DefaultRightClickBackend;

impl RightClickBackend for DefaultRightClickBackend {
    fn send_right_click(&self) {
        send_right_click();
    }
}

pub trait CursorPositionProvider: Send + Sync {
    fn cursor_position(&self) -> Option<(f32, f32)>;
}

#[derive(Debug)]
struct DefaultCursorPositionProvider;

impl CursorPositionProvider for DefaultCursorPositionProvider {
    fn cursor_position(&self) -> Option<(f32, f32)> {
        get_cursor_position()
    }
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
    overlay_factory: Arc<dyn OverlayFactory>,
    right_click_backend: Arc<dyn RightClickBackend>,
    cursor_provider: Arc<dyn CursorPositionProvider>,
    worker: Option<WorkerHandle>,
}

impl Default for MouseGestureService {
    fn default() -> Self {
        Self::new_with_backend(Box::new(DefaultHookBackend::default()))
    }
}

impl MouseGestureService {
    pub fn new_with_backend(backend: Box<dyn HookBackend>) -> Self {
        Self::new_with_backend_and_overlays(
            backend,
            Arc::new(DefaultOverlayFactory),
            Arc::new(DefaultRightClickBackend),
            Arc::new(DefaultCursorPositionProvider),
        )
    }

    pub fn new_with_backend_and_overlays(
        backend: Box<dyn HookBackend>,
        overlay_factory: Arc<dyn OverlayFactory>,
        right_click_backend: Arc<dyn RightClickBackend>,
        cursor_provider: Arc<dyn CursorPositionProvider>,
    ) -> Self {
        let db = load_gestures(GESTURES_FILE)
            .map(|db| Arc::new(Mutex::new(db)))
            .ok();
        Self {
            config: MouseGestureConfig::default(),
            db,
            backend,
            overlay_factory,
            right_click_backend,
            cursor_provider,
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
        if self.config == config {
            return;
        }

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
        // If the worker is already running, it captured the old Option<db> by value.
        // Restart so the worker sees the new DB.
        if self.worker.is_some() {
            self.stop_running();
            if self.config.enabled {
                self.start_running();
            }
        }
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
        let overlay_factory = Arc::clone(&self.overlay_factory);
        let right_click_backend = Arc::clone(&self.right_click_backend);
        let cursor_provider = Arc::clone(&self.cursor_provider);
        let join = thread::spawn(move || {
            worker_loop(
                config,
                db,
                event_rx,
                stop_rx,
                overlay_factory,
                right_click_backend,
                cursor_provider,
            )
        });
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
    overlay_factory: Arc<dyn OverlayFactory>,
    right_click_backend: Arc<dyn RightClickBackend>,
    cursor_provider: Arc<dyn CursorPositionProvider>,
) {
    let mut tracker = GestureTracker::new(
        config.dir_mode,
        config.threshold_px,
        config.long_threshold_x,
        config.long_threshold_y,
        config.max_tokens,
    );
    let mut trail_overlay = TrailOverlay::new(
        overlay_factory.trail_backend(),
        config.show_trail,
        config.trail_color,
        config.trail_width,
        config.trail_start_move_px,
    );
    let mut hint_overlay = HintOverlay::new(
        overlay_factory.hint_backend(),
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
    let mut start_time = Instant::now();
    let mut selected_binding_idx: usize = 0;
    let mut cached_tokens = String::new();
    let mut cached_actions: Vec<crate::actions::Action> = Vec::new();
    let mut cached_gesture_label: Option<String> = None;

    loop {
        #[cfg(windows)]
        {
            use windows::Win32::UI::WindowsAndMessaging::{
                DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
            };

            let mut msg = MSG::default();
            while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() } {
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
        if stop_rx.try_recv().is_ok() {
            break;
        }
        match event_rx.recv_timeout(poll_interval) {
            Ok(event) => match event {
                HookEvent::RButtonDown => {
                    active = true;
                    exceeded_deadzone = false;
                    tracker.reset();
                    #[cfg(windows)]
                    hook_dispatch().set_tracking(false);
                    selected_binding_idx = 0;
                    cached_tokens.clear();
                    cached_actions.clear();
                    cached_gesture_label = None;
                    start_time = Instant::now();
                    let pos = cursor_provider.cursor_position().unwrap_or(start_pos);
                    start_pos = pos;
                    let ms = start_time.elapsed().as_millis() as u64;
                    tracker.feed_point(pos, ms);
                    trail_overlay.reset(pos);
                    hint_overlay.reset();
                    last_trail = Instant::now();
                    last_recognition = last_trail;
                    #[cfg(windows)]
                    hook_dispatch().set_active(true);
                }

                HookEvent::RButtonUp => {
                    if active {
                        // Sample cursor pos once on release so we tokenize the final motion.
                        let cursor_pos = cursor_provider.cursor_position().unwrap_or(start_pos);

                        // Always feed the final point so quick gestures still tokenize.
                        let ms = start_time.elapsed().as_millis() as u64;
                        let _ = tracker.feed_point(cursor_pos, ms);

                        let tokens = tracker.tokens_string();

                        println!(
                            "MG release tokens='{tokens}' mode={:?} db_present={}",
                            config.dir_mode,
                            db.is_some()
                        );
                        if let Some(db) = &db {
                            if let Ok(guard) = db.lock() {
                                for g in &guard.gestures {
                                    println!(
                                        "DB: label='{}' tokens='{}' mode={:?} enabled={} bindings={}",
                                        g.label, g.tokens, g.dir_mode, g.enabled, g.bindings.len()
                                        );
                                    for b in &g.bindings {
                                        println!("  - binding: label='{}' action='{}' args={:?} enabled={}",b.label, b.action, b.args, b.enabled);
                                    }
                                }
                            }
                        }

                        // If we produced any tokens, treat it as a gesture (swallow right click).
                        if !tokens.is_empty() {
                            // Execute the currently selected binding (wheel-cycled) if there are multiple.
                            if let Some((_gesture_label, actions)) =
                                match_binding_actions(&db, &tokens, config.dir_mode)
                            {
                                if !actions.is_empty() {
                                    let idx = selected_binding_idx % actions.len();
                                    if let Some(action) = actions.get(idx).cloned() {
                                        send_event(WatchEvent::ExecuteAction(action));
                                    }
                                }
                            } else {
                                match config.no_match_behavior {
                                    NoMatchBehavior::DoNothing => {}
                                    NoMatchBehavior::PassThroughClick => {
                                        right_click_backend.send_right_click();
                                    }
                                    NoMatchBehavior::ShowNoMatchHint => {
                                        hint_overlay.update("No match", None, cursor_pos);
                                    }
                                }
                            }
                        } else {
                            // No tokens => normal right click
                            right_click_backend.send_right_click();
                        }

                        // Always clear visuals on release
                        trail_overlay.clear();
                        hint_overlay.reset();

                        // Reset state
                        active = false;
                        exceeded_deadzone = false;
                        tracker.reset();
                        selected_binding_idx = 0;
                        cached_tokens.clear();
                        cached_actions.clear();
                        cached_gesture_label = None;
                        #[cfg(windows)]
                        hook_dispatch().set_active(false);
                    }
                }

                HookEvent::WheelUp | HookEvent::WheelDown => {
                    if active && exceeded_deadzone && cached_actions.len() > 1 {
                        let len = cached_actions.len();
                        match event {
                            HookEvent::WheelUp => {
                                selected_binding_idx = (selected_binding_idx + 1) % len;
                            }
                            HookEvent::WheelDown => {
                                selected_binding_idx = (selected_binding_idx + len - 1) % len;
                            }
                            _ => {}
                        }

                        if let Some(pos) = cursor_provider.cursor_position() {
                            let best_match = cached_gesture_label.as_deref().map(|label| {
                                format_selected_hint(label, &cached_actions, selected_binding_idx)
                            });
                            hint_overlay.update(&cached_tokens, best_match.as_deref(), pos);
                        }
                    }
                }

                HookEvent::Cancel => {
                    if active {
                        if config.cancel_behavior == CancelBehavior::PassThroughClick {
                            right_click_backend.send_right_click();
                        }
                        trail_overlay.clear();
                        hint_overlay.reset();
                        active = false;
                        exceeded_deadzone = false;
                        tracker.reset();
                        selected_binding_idx = 0;
                        cached_tokens.clear();
                        cached_actions.clear();
                        cached_gesture_label = None;
                        #[cfg(windows)]
                        {
                            hook_dispatch().set_tracking(false);
                            hook_dispatch().set_active(false);
                        }
                    }
                }
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if active && last_trail.elapsed() >= poll_interval {
            if let Some(pos) = cursor_provider.cursor_position() {
                let dx = pos.0 - start_pos.0;
                let dy = pos.1 - start_pos.1;
                let dist_sq = dx * dx + dy * dy;
                if !exceeded_deadzone && dist_sq >= config.deadzone_px * config.deadzone_px {
                    exceeded_deadzone = true;
                    #[cfg(windows)]
                    hook_dispatch().set_tracking(true);
                }

                trail_overlay.update_position(pos);

                if last_recognition.elapsed() >= recognition_interval {
                    let ms = start_time.elapsed().as_millis() as u64;
                    let _ = tracker.feed_point(pos, ms);
                    let tokens = tracker.tokens_string();
                    if tokens != cached_tokens {
                        cached_tokens = tokens.to_string();
                        selected_binding_idx = 0;
                        if let Some((gesture_label, actions)) =
                            match_binding_actions(&db, &tokens, config.dir_mode)
                        {
                            cached_gesture_label = Some(gesture_label);
                            cached_actions = actions;
                        } else {
                            if config.no_match_behavior == NoMatchBehavior::ShowNoMatchHint
                                && !tokens.is_empty()
                            {
                                cached_gesture_label = Some("No match".to_string());
                            } else {
                                cached_gesture_label = None;
                            }
                            cached_actions.clear();
                        }
                    }

                    let best_match = cached_gesture_label.as_deref().map(|label| {
                        format_selected_hint(label, &cached_actions, selected_binding_idx)
                    });

                    hint_overlay.update(&tokens, best_match.as_deref(), pos);
                    last_recognition = Instant::now();
                }
            }
            last_trail = Instant::now();
        }
    }
}

#[allow(dead_code)]
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

fn match_binding_actions(
    db: &Option<SharedGestureDb>,
    tokens: &str,
    dir_mode: DirMode,
) -> Option<(String, Vec<crate::actions::Action>)> {
    let db = db.as_ref()?;
    let guard = db.lock().ok()?;
    let (gesture_label, bindings) = guard.match_bindings_owned(tokens, dir_mode)?;
    let actions = bindings
        .iter()
        .map(|binding| binding.to_action(&gesture_label))
        .collect::<Vec<_>>();
    Some((gesture_label, actions))
}

fn format_selected_hint(
    gesture_label: &str,
    actions: &[crate::actions::Action],
    selected_idx: usize,
) -> String {
    if actions.is_empty() {
        return gesture_label.to_string();
    }
    let idx = selected_idx.min(actions.len().saturating_sub(1));
    if actions.len() == 1 {
        format!("{gesture_label}: {}", actions[idx].label)
    } else {
        format!(
            "{gesture_label}: {} [{}/{}]",
            actions[idx].label,
            idx + 1,
            actions.len()
        )
    }
}

#[allow(dead_code)]
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
const MG_INJECT_TAG: usize = 0x4D47_494E_4A; // "MG_INJ"

#[cfg(windows)]
fn send_right_click() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        MOUSEINPUT,
    };

    // Prevent the hook from consuming the injected click (and re-triggering itself)
    hook_dispatch().set_injecting(true);

    let down = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_RIGHTDOWN,
                time: 0,
                dwExtraInfo: MG_INJECT_TAG,
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
                dwExtraInfo: MG_INJECT_TAG,
            },
        },
    };

    let _ = unsafe { SendInput(&[down, up], std::mem::size_of::<INPUT>() as i32) };

    hook_dispatch().set_injecting(false);
}

#[cfg(not(windows))]
fn send_right_click() {}

#[cfg(windows)]
struct HookThread {
    thread_id: u32,
    join: std::thread::JoinHandle<()>,
}

#[cfg(windows)]
#[derive(Default)]
pub struct DefaultHookBackend {
    hook_thread: Option<HookThread>,
}

#[cfg(windows)]
unsafe impl Send for DefaultHookBackend {}

#[cfg(windows)]
impl HookBackend for DefaultHookBackend {
    fn install(&mut self, sender: Sender<HookEvent>) -> anyhow::Result<()> {
        if self.hook_thread.is_some() {
            return Ok(());
        }

        // Put the sender where the hook proc can see it.
        hook_dispatch().set_sender(Some(sender));
        hook_dispatch().set_tracking(false);
        hook_dispatch().set_active(false);
        hook_dispatch().set_enabled(true);

        use std::time::Duration;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::System::Threading::GetCurrentThreadId;
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, PeekMessageW, TranslateMessage, MSG,
            PM_NOREMOVE,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL,
        };

        // Handshake so install() only returns once the hook thread is actually ready.
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<anyhow::Result<u32>>(1);

        let join = std::thread::spawn(move || {
            // Ensure the thread has a message queue.
            let mut msg = MSG::default();
            unsafe {
                let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            }

            let thread_id = unsafe { GetCurrentThreadId() };

            let hmodule = match unsafe { GetModuleHandleW(None) } {
                Ok(h) => h,
                Err(e) => {
                    let _ = ready_tx.send(Err(anyhow!(e)));
                    return;
                }
            };

            let mouse_hook = match unsafe {
                SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), hmodule, 0)
            } {
                Ok(h) if !h.0.is_null() => h,
                Ok(_) => {
                    let _ = ready_tx.send(Err(anyhow!(windows::core::Error::from_win32())));
                    return;
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(anyhow!(e)));
                    return;
                }
            };

            let keyboard_hook = match unsafe {
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), hmodule, 0)
            } {
                Ok(h) if !h.0.is_null() => h,
                Ok(_) => {
                    let _ = ready_tx.send(Err(anyhow!(windows::core::Error::from_win32())));
                    unsafe {
                        let _ = UnhookWindowsHookEx(mouse_hook);
                    }
                    return;
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(anyhow!(e)));
                    unsafe {
                        let _ = UnhookWindowsHookEx(mouse_hook);
                    }
                    return;
                }
            };

            let _ = ready_tx.send(Ok(thread_id));

            // Message loop keeps WH_MOUSE_LL callbacks flowing.
            loop {
                let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
                if r.0 == 0 {
                    // WM_QUIT
                    break;
                }
                if r.0 == -1 {
                    break;
                }
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }

            unsafe {
                let _ = UnhookWindowsHookEx(mouse_hook);
                let _ = UnhookWindowsHookEx(keyboard_hook);
            }
        });

        let thread_id = ready_rx
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| anyhow!("hook thread did not signal readiness"))??;

        self.hook_thread = Some(HookThread { thread_id, join });
        Ok(())
    }

    fn uninstall(&mut self) -> anyhow::Result<()> {
        // Stop dispatch first to avoid any new work while shutting down.
        hook_dispatch().set_enabled(false);
        hook_dispatch().set_tracking(false);
        hook_dispatch().set_active(false);
        hook_dispatch().set_sender(None);

        if let Some(th) = self.hook_thread.take() {
            use windows::Win32::Foundation::{LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
            unsafe {
                let _ = PostThreadMessageW(th.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
            let _ = th.join.join();
        }

        Ok(())
    }

    fn is_installed(&self) -> bool {
        self.hook_thread.is_some()
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
    tracking: AtomicBool,
    injecting: AtomicBool,
    active: AtomicBool,
    sender: Mutex<Option<Sender<HookEvent>>>,
}

#[cfg(windows)]
impl HookDispatch {
    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
    }

    fn set_tracking(&self, tracking: bool) {
        self.tracking.store(tracking, Ordering::Release);
    }

    fn is_tracking(&self) -> bool {
        self.tracking.load(Ordering::Acquire)
    }

    fn set_injecting(&self, injecting: bool) {
        self.injecting.store(injecting, Ordering::Release);
    }

    fn is_injecting(&self) -> bool {
        self.injecting.load(Ordering::Acquire)
    }

    fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Release);
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
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
        tracking: AtomicBool::new(false),
        injecting: AtomicBool::new(false),
        active: AtomicBool::new(false),
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
        CallNextHookEx, HC_ACTION, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP,
    };

    use windows::Win32::UI::WindowsAndMessaging::MSLLHOOKSTRUCT;

    if n_code == HC_ACTION as i32 {
        let msg = w_param.0 as u32;

        if msg == WM_RBUTTONDOWN || msg == WM_RBUTTONUP || msg == WM_MOUSEWHEEL {
            let dispatch = hook_dispatch();

            if dispatch.enabled.load(Ordering::Acquire) {
                // If we're injecting (or the event is injected), do NOT consume it and do NOT forward to worker.
                let info = &*(l_param.0 as *const MSLLHOOKSTRUCT);

                // Flags: 0x1 = LLMHF_INJECTED, 0x2 = LLMHF_LOWER_IL_INJECTED
                let injected_flagged = (info.flags & 0x1) != 0 || (info.flags & 0x2) != 0;
                let injected_tagged = info.dwExtraInfo == MG_INJECT_TAG;

                if dispatch.is_injecting() || injected_flagged || injected_tagged {
                    return CallNextHookEx(
                        windows::Win32::UI::WindowsAndMessaging::HHOOK(std::ptr::null_mut()),
                        n_code,
                        w_param,
                        l_param,
                    );
                }

                // Only consume wheel events while a gesture is actively being tracked.
                if msg == WM_MOUSEWHEEL && !dispatch.is_tracking() {
                    return CallNextHookEx(
                        windows::Win32::UI::WindowsAndMessaging::HHOOK(std::ptr::null_mut()),
                        n_code,
                        w_param,
                        l_param,
                    );
                }

                if let Ok(guard) = dispatch.sender.try_lock() {
                    if let Some(sender) = guard.as_ref() {
                        if msg == WM_RBUTTONDOWN {
                            // Wheel-cycling is only enabled after worker exceeds deadzone.
                            dispatch.set_tracking(false);
                            let _ = sender.send(HookEvent::RButtonDown);
                        } else if msg == WM_RBUTTONUP {
                            dispatch.set_tracking(false);
                            let _ = sender.send(HookEvent::RButtonUp);
                        } else if msg == WM_MOUSEWHEEL {
                            // mouseData high word contains signed wheel delta (WHEEL_DELTA multiples).
                            let delta = ((info.mouseData >> 16) & 0xFFFF) as i16;
                            if delta > 0 {
                                let _ = sender.send(HookEvent::WheelUp);
                            } else if delta < 0 {
                                let _ = sender.send(HookEvent::WheelDown);
                            }
                        }
                    }
                }

                // Consume while MG is enabled (RMB always, wheel only when tracking).
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

#[cfg(windows)]
unsafe extern "system" fn keyboard_hook_proc(
    n_code: i32,
    w_param: windows::Win32::Foundation::WPARAM,
    l_param: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, HC_ACTION, KBDLLHOOKSTRUCT, KBDLLHOOKSTRUCT_FLAGS, WM_KEYDOWN,
        WM_SYSKEYDOWN,
    };

    if n_code == HC_ACTION as i32 {
        let msg = w_param.0 as u32;
        if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
            let info = &*(l_param.0 as *const KBDLLHOOKSTRUCT);
            let injected = (info.flags & KBDLLHOOKSTRUCT_FLAGS(0x10)) != KBDLLHOOKSTRUCT_FLAGS(0);
            if !injected && info.vkCode == VK_ESCAPE.0 as u32 {
                let dispatch = hook_dispatch();
                if dispatch.enabled.load(Ordering::Acquire) && dispatch.is_active() {
                    if let Ok(guard) = dispatch.sender.try_lock() {
                        if let Some(sender) = guard.as_ref() {
                            let _ = sender.send(HookEvent::Cancel);
                        }
                    }
                    return windows::Win32::Foundation::LRESULT(1);
                }
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
