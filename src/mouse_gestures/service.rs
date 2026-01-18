use crate::gui::{send_event, MouseGestureEvent, WatchEvent};
use crate::mouse_gestures::mouse_gesture_overlay;
use crate::plugins::mouse_gestures::db::{
    select_binding, select_profile, ForegroundWindowInfo, MouseGestureDb,
};
use crate::plugins::mouse_gestures::engine::{
    direction_sequence, direction_similarity, parse_gesture, track_length, GestureDirection, Point,
};
use crate::plugins::mouse_gestures::settings::MouseGesturePluginSettings;
use once_cell::sync::OnceCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

const DEFAULT_DIRECTION_SEGMENT: f32 = 6.0;

#[derive(Clone, Debug)]
pub struct TrackOutcome {
    pub matched: bool,
    pub passthrough_click: bool,
}

impl TrackOutcome {
    fn passthrough() -> Self {
        Self {
            matched: false,
            passthrough_click: true,
        }
    }

    fn no_match() -> Self {
        Self {
            matched: false,
            passthrough_click: false,
        }
    }

    fn matched() -> Self {
        Self {
            matched: true,
            passthrough_click: false,
        }
    }
}

#[derive(Clone)]
struct MouseGestureSnapshots {
    settings: MouseGesturePluginSettings,
    db: MouseGestureDb,
}

impl Default for MouseGestureSnapshots {
    fn default() -> Self {
        Self {
            settings: MouseGesturePluginSettings::default(),
            db: MouseGestureDb::default(),
        }
    }
}

#[derive(Clone)]
pub struct MouseGestureRuntime {
    snapshots: Arc<RwLock<MouseGestureSnapshots>>,
    event_sink: Arc<dyn MouseGestureEventSink>,
}

impl MouseGestureRuntime {
    fn best_match(&self, points: &[Point]) -> Option<(String, f32)> {
        let snapshots = self
            .snapshots
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        if points.len() < 2 {
            return None;
        }
        let length = track_length(points);
        if length < snapshots.settings.min_track_len {
            return None;
        }
        if snapshots.settings.max_track_len > 0.0 && length > snapshots.settings.max_track_len {
            return None;
        }
        let track_dirs = direction_sequence(points, DEFAULT_DIRECTION_SEGMENT);
        if track_dirs.is_empty() {
            return None;
        }
        let gesture_templates = build_gesture_templates(&snapshots.db);
        let mut distances = HashMap::new();
        for (gesture_id, template) in &gesture_templates {
            let similarity = direction_similarity(&track_dirs, &template.directions);
            if similarity < snapshots.settings.match_threshold {
                continue;
            }
            distances.insert(gesture_id.clone(), 1.0 - similarity);
        }
        if distances.is_empty() {
            return None;
        }
        let window_info = current_foreground_window();
        let profile = select_profile(&snapshots.db, &window_info)?;
        let binding = select_binding(profile, &distances, 1.0)?;
        let similarity = 1.0 - binding.distance;
        let label = if binding.binding.label.trim().is_empty() {
            binding.binding.action.clone()
        } else {
            binding.binding.label.clone()
        };
        Some((label, similarity))
    }

    fn preview_text(&self, points: &[Point]) -> Option<String> {
        let snapshots = self
            .snapshots
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        if points.len() < 2 {
            return None;
        }
        let length = track_length(points);
        if length < snapshots.settings.min_track_len {
            return Some("Keep drawing".to_string());
        }
        if snapshots.settings.max_track_len > 0.0 && length > snapshots.settings.max_track_len {
            return Some("Too long".to_string());
        }
        let Some((label, similarity)) = self.best_match(points) else {
            return Some("No match".to_string());
        };
        Some(format!(
            "Will trigger: {label} ({:.0}%)",
            similarity * 100.0
        ))
    }

    fn evaluate_track(&self, points: &[Point]) -> TrackOutcome {
        let snapshots = self
            .snapshots
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        let passthrough_on_no_match = snapshots.settings.passthrough_on_no_match;
        if points.len() < 2 {
            return TrackOutcome::passthrough();
        }
        let length = track_length(points);
        if length < snapshots.settings.min_track_len {
            return TrackOutcome::passthrough();
        }
        if snapshots.settings.max_track_len > 0.0 && length > snapshots.settings.max_track_len {
            return if passthrough_on_no_match {
                TrackOutcome::passthrough()
            } else {
                TrackOutcome::no_match()
            };
        }

        let track_dirs = direction_sequence(points, DEFAULT_DIRECTION_SEGMENT);
        if track_dirs.is_empty() {
            return if passthrough_on_no_match {
                TrackOutcome::passthrough()
            } else {
                TrackOutcome::no_match()
            };
        }

        let gesture_templates = build_gesture_templates(&snapshots.db);
        if gesture_templates.is_empty() {
            return if passthrough_on_no_match {
                TrackOutcome::passthrough()
            } else {
                TrackOutcome::no_match()
            };
        }

        let mut distances = HashMap::new();
        for (gesture_id, template) in &gesture_templates {
            let similarity = direction_similarity(&track_dirs, &template.directions);
            if similarity < snapshots.settings.match_threshold {
                continue;
            }
            let distance = 1.0 - similarity;
            distances.insert(gesture_id.clone(), distance);
        }
        if distances.is_empty() {
            return if passthrough_on_no_match {
                TrackOutcome::passthrough()
            } else {
                TrackOutcome::no_match()
            };
        }

        let window_info = current_foreground_window();
        let Some(profile) = select_profile(&snapshots.db, &window_info) else {
            return if passthrough_on_no_match {
                TrackOutcome::passthrough()
            } else {
                TrackOutcome::no_match()
            };
        };
        let Some(binding_match) = select_binding(profile, &distances, 1.0) else {
            return if passthrough_on_no_match {
                TrackOutcome::passthrough()
            } else {
                TrackOutcome::no_match()
            };
        };

        let template = gesture_templates
            .get(&binding_match.binding.gesture_id)
            .cloned();
        let event = MouseGestureEvent {
            gesture_id: binding_match.binding.gesture_id.clone(),
            gesture_name: template.and_then(|t| t.name),
            profile_id: profile.id.clone(),
            profile_label: profile.label.clone(),
            action_payload: binding_match.binding.action.clone(),
            action_args: binding_match.binding.args.clone(),
            distance: binding_match.distance,
        };
        self.event_sink.dispatch(event);
        TrackOutcome::matched()
    }
}

#[derive(Clone)]
struct GestureTemplate {
    name: Option<String>,
    directions: Vec<GestureDirection>,
}

fn build_gesture_templates(db: &MouseGestureDb) -> HashMap<String, GestureTemplate> {
    let mut templates = HashMap::new();
    for (gesture_id, serialized) in &db.bindings {
        let parsed = match parse_gesture(serialized) {
            Ok(def) => def,
            Err(_) => continue,
        };
        let directions = direction_sequence(&parsed.points, DEFAULT_DIRECTION_SEGMENT);
        if directions.is_empty() {
            continue;
        }
        templates.insert(
            gesture_id.clone(),
            GestureTemplate {
                name: parsed.name,
                directions,
            },
        );
    }
    templates
}

fn current_foreground_window() -> ForegroundWindowInfo {
    #[cfg(windows)]
    {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        use std::path::Path;
        use windows::core::PWSTR;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
            PROCESS_QUERY_LIMITED_INFORMATION,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            GetClassNameW, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
            GetWindowThreadProcessId,
        };

        fn window_title(hwnd: HWND) -> Option<String> {
            unsafe {
                let len = GetWindowTextLengthW(hwnd);
                if len <= 0 {
                    return None;
                }
                let mut buf = vec![0u16; len as usize + 1];
                let read = GetWindowTextW(hwnd, &mut buf);
                if read == 0 {
                    return None;
                }
                let title = String::from_utf16_lossy(&buf[..read as usize]);
                if title.trim().is_empty() {
                    None
                } else {
                    Some(title)
                }
            }
        }

        fn window_class(hwnd: HWND) -> Option<String> {
            unsafe {
                let mut buf = vec![0u16; 256];
                let len = GetClassNameW(hwnd, &mut buf) as usize;
                if len == 0 {
                    return None;
                }
                let class = String::from_utf16_lossy(&buf[..len]);
                if class.trim().is_empty() {
                    None
                } else {
                    Some(class)
                }
            }
        }

        fn window_exe(hwnd: HWND) -> Option<String> {
            unsafe {
                let mut pid = 0u32;
                let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
                if pid == 0 {
                    return None;
                }
                let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
                let mut buffer = vec![0u16; 1024];
                let mut size = buffer.len() as u32;
                let success = QueryFullProcessImageNameW(
                    handle,
                    PROCESS_NAME_FORMAT(0),
                    PWSTR(buffer.as_mut_ptr()),
                    &mut size,
                )
                .is_ok();
                let _ = windows::Win32::Foundation::CloseHandle(handle);
                if !success || size == 0 {
                    return None;
                }
                let path = OsString::from_wide(&buffer[..size as usize])
                    .to_string_lossy()
                    .to_string();
                Path::new(path.as_str())
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
            }
        }

        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return ForegroundWindowInfo {
                    exe: None,
                    class: None,
                    title: None,
                };
            }
            ForegroundWindowInfo {
                exe: window_exe(hwnd),
                class: window_class(hwnd),
                title: window_title(hwnd),
            }
        }
    }
    #[cfg(not(windows))]
    {
        ForegroundWindowInfo {
            exe: None,
            class: None,
            title: None,
        }
    }
}

pub trait MouseGestureEventSink: Send + Sync {
    fn dispatch(&self, event: MouseGestureEvent);
}

struct GuiMouseGestureEventSink;

impl MouseGestureEventSink for GuiMouseGestureEventSink {
    fn dispatch(&self, event: MouseGestureEvent) {
        send_event(WatchEvent::MouseGesture(event));
    }
}

pub trait MouseHookBackend: Send + Sync {
    fn start(&self, runtime: MouseGestureRuntime) -> anyhow::Result<()>;
    fn stop(&self);
    fn is_running(&self) -> bool;
}

pub struct MouseGestureService {
    snapshots: Arc<RwLock<MouseGestureSnapshots>>,
    backend: Arc<dyn MouseHookBackend>,
    event_sink: Arc<dyn MouseGestureEventSink>,
    running: AtomicBool,
}

impl MouseGestureService {
    pub fn new_with_backend_and_sink(
        backend: Arc<dyn MouseHookBackend>,
        event_sink: Arc<dyn MouseGestureEventSink>,
    ) -> Self {
        Self {
            snapshots: Arc::new(RwLock::new(MouseGestureSnapshots::default())),
            backend,
            event_sink,
            running: AtomicBool::new(false),
        }
    }

    pub fn new_with_backend(backend: Arc<dyn MouseHookBackend>) -> Self {
        Self::new_with_backend_and_sink(backend, Arc::new(GuiMouseGestureEventSink))
    }

    pub fn update_settings(&self, settings: MouseGesturePluginSettings) {
        if let Ok(mut guard) = self.snapshots.write() {
            guard.settings = settings.clone();
        }
        if let Ok(mut overlay) = mouse_gesture_overlay().lock() {
            overlay.update_settings(&settings);
            if !settings.enabled {
                overlay.end_stroke();
                overlay.update_preview(None, None);
            }
        }
        if settings.enabled {
            self.start();
        } else {
            self.stop();
        }
    }

    pub fn update_db(&self, db: MouseGestureDb) {
        if let Ok(mut guard) = self.snapshots.write() {
            guard.db = db;
        }
    }

    pub fn start(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let runtime = MouseGestureRuntime {
            snapshots: Arc::clone(&self.snapshots),
            event_sink: Arc::clone(&self.event_sink),
        };
        if let Err(err) = self.backend.start(runtime) {
            self.running.store(false, Ordering::SeqCst);
            tracing::error!(?err, "failed to start mouse gesture backend");
        }
    }

    pub fn stop(&self) {
        if !self.running.swap(false, Ordering::SeqCst) {
            return;
        }
        self.backend.stop();
    }
}

static MOUSE_GESTURE_SERVICE: OnceCell<Arc<MouseGestureService>> = OnceCell::new();

pub fn mouse_gesture_service() -> Arc<MouseGestureService> {
    MOUSE_GESTURE_SERVICE
        .get_or_init(|| {
            Arc::new(MouseGestureService::new_with_backend(Arc::new(
                WindowsMouseHookBackend::default(),
            )))
        })
        .clone()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TriggerButton {
    Left,
    Right,
    Middle,
}

impl TriggerButton {
    fn from_setting(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "middle" => Some(Self::Middle),
            _ => None,
        }
    }
}

#[cfg(windows)]
#[derive(Default)]
pub struct WindowsMouseHookBackend {
    running: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
    thread_id: Arc<AtomicUsize>,
    handle: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}

#[cfg(windows)]
#[derive(Default)]
struct HookTrackingState {
    active_button: Option<TriggerButton>,
    points: Vec<Point>,
    last_point: Option<Point>,
}

#[cfg(windows)]
struct HookState {
    runtime: MouseGestureRuntime,
    tracking: Mutex<HookTrackingState>,
}

#[cfg(windows)]
static HOOK_STATE: OnceCell<Arc<HookState>> = OnceCell::new();

#[cfg(windows)]
impl MouseHookBackend for WindowsMouseHookBackend {
    fn start(&self, runtime: MouseGestureRuntime) -> anyhow::Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        self.stop_flag.store(false, Ordering::SeqCst);
        let runtime_state = HOOK_STATE.get_or_init(|| {
            Arc::new(HookState {
                runtime,
                tracking: Mutex::new(HookTrackingState::default()),
            })
        });
        if let Ok(mut tracking) = runtime_state.tracking.lock() {
            *tracking = HookTrackingState::default();
        }

        let stop_flag = Arc::clone(&self.stop_flag);
        let thread_id = Arc::clone(&self.thread_id);
        let handle = std::thread::spawn(move || {
            use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::{
                CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW,
                SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HC_ACTION, MSG,
                MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
                WM_MBUTTONUP, WM_MOUSEMOVE, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP,
            };

            unsafe extern "system" fn hook_proc(
                code: i32,
                wparam: WPARAM,
                lparam: LPARAM,
            ) -> LRESULT {
                if code != HC_ACTION as i32 {
                    return CallNextHookEx(None, code, wparam, lparam);
                }
                let Some(state) = HOOK_STATE.get() else {
                    return CallNextHookEx(None, code, wparam, lparam);
                };
                let event = wparam.0 as u32;
                let data = &*(lparam.0 as *const MSLLHOOKSTRUCT);
                let point = Point {
                    x: data.pt.x as f32,
                    y: data.pt.y as f32,
                };
                let trigger_button =
                    state.runtime.snapshots.read().ok().and_then(|snap| {
                        TriggerButton::from_setting(&snap.settings.trigger_button)
                    });

                let mut tracking = match state.tracking.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };

                let event_button = match event {
                    WM_LBUTTONDOWN | WM_LBUTTONUP => Some(TriggerButton::Left),
                    WM_RBUTTONDOWN | WM_RBUTTONUP => Some(TriggerButton::Right),
                    WM_MBUTTONDOWN | WM_MBUTTONUP => Some(TriggerButton::Middle),
                    _ => None,
                };

                if matches!(event, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN)
                    && event_button == trigger_button
                {
                    tracking.active_button = event_button;
                    tracking.points.clear();
                    tracking.last_point = Some(point);
                    tracking.points.push(point);
                    if let Ok(mut overlay) = mouse_gesture_overlay().lock() {
                        overlay.begin_stroke(point);
                        overlay.update_preview(None, None);
                    }
                    return LRESULT(1);
                }

                if event == WM_MOUSEMOVE && tracking.active_button.is_some() {
                    if tracking.last_point.map(|p| p != point).unwrap_or(true) {
                        tracking.points.push(point);
                        tracking.last_point = Some(point);
                        if let Ok(mut overlay) = mouse_gesture_overlay().lock() {
                            overlay.push_point(point);
                            let preview_enabled = state
                                .runtime
                                .snapshots
                                .read()
                                .ok()
                                .map(|snap| snap.settings.preview_enabled)
                                .unwrap_or(false);
                            if preview_enabled {
                                let text = state.runtime.preview_text(&tracking.points);
                                overlay.update_preview(text, Some(point));
                            } else {
                                overlay.update_preview(None, None);
                            }
                        }
                    }
                    return CallNextHookEx(None, code, wparam, lparam);
                }

                if matches!(event, WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP)
                    && tracking.active_button.is_some()
                {
                    tracking.points.push(point);
                    let points = std::mem::take(&mut tracking.points);
                    let button = tracking.active_button.take();
                    tracking.last_point = None;
                    drop(tracking);

                    if let Ok(mut overlay) = mouse_gesture_overlay().lock() {
                        overlay.end_stroke();
                        overlay.update_preview(None, None);
                    }

                    let outcome = state.runtime.evaluate_track(&points);
                    if outcome.passthrough_click {
                        if let Some(button) = button {
                            send_passthrough_click(button);
                        }
                    }
                    return LRESULT(1);
                }

                CallNextHookEx(None, code, wparam, lparam)
            }

            unsafe fn send_passthrough_click(button: TriggerButton) {
                use windows::Win32::UI::Input::KeyboardAndMouse::{
                    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN,
                    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
                    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT,
                };
                let (down, up) = match button {
                    TriggerButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
                    TriggerButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
                    TriggerButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
                };
                let down_input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: 0,
                            dy: 0,
                            mouseData: 0,
                            dwFlags: down,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                };
                let mut up_input = down_input;
                up_input.Anonymous.mi.dwFlags = up;
                let _ = SendInput(&[down_input, up_input], std::mem::size_of::<INPUT>() as i32);
            }

            let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), None, 0).ok() };
            let thread = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
            thread_id.store(thread as usize, Ordering::SeqCst);
            let mut msg = MSG::default();
            loop {
                let result = unsafe { GetMessageW(&mut msg, HWND(std::ptr::null_mut()), 0, 0) };
                if result.0 == -1 {
                    break;
                }
                if result.0 == 0 || msg.message == WM_QUIT {
                    break;
                }
                unsafe {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                if stop_flag.load(Ordering::SeqCst) {
                    unsafe {
                        PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0));
                    }
                }
            }
            if let Some(hook) = hook {
                unsafe {
                    let _ = UnhookWindowsHookEx(hook);
                }
            }
        });

        if let Ok(mut guard) = self.handle.lock() {
            *guard = Some(handle);
        }
        Ok(())
    }

    fn stop(&self) {
        if !self.running.swap(false, Ordering::SeqCst) {
            return;
        }
        self.stop_flag.store(true, Ordering::SeqCst);
        let thread_id = self.thread_id.load(Ordering::SeqCst) as u32;
        if thread_id != 0 {
            #[allow(clippy::cast_possible_wrap)]
            unsafe {
                use windows::Win32::Foundation::{LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
                let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        if let Ok(mut guard) = self.handle.lock() {
            if let Some(handle) = guard.take() {
                let _ = handle.join();
            }
        }
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

#[cfg(not(windows))]
#[derive(Default)]
pub struct WindowsMouseHookBackend;

#[cfg(not(windows))]
impl MouseHookBackend for WindowsMouseHookBackend {
    fn start(&self, _runtime: MouseGestureRuntime) -> anyhow::Result<()> {
        Ok(())
    }

    fn stop(&self) {}

    fn is_running(&self) -> bool {
        false
    }
}

#[derive(Default)]
pub struct MockMouseHookBackend {
    runtime: Mutex<Option<MouseGestureRuntime>>,
    start_count: AtomicUsize,
    stop_count: AtomicUsize,
    passthrough_clicks: AtomicUsize,
}

impl MockMouseHookBackend {
    pub fn start_count(&self) -> usize {
        self.start_count.load(Ordering::SeqCst)
    }

    pub fn stop_count(&self) -> usize {
        self.stop_count.load(Ordering::SeqCst)
    }

    pub fn passthrough_clicks(&self) -> usize {
        self.passthrough_clicks.load(Ordering::SeqCst)
    }

    pub fn simulate_track(&self, points: Vec<Point>) -> TrackOutcome {
        let runtime = self.runtime.lock().ok().and_then(|guard| guard.clone());
        let Some(runtime) = runtime else {
            return TrackOutcome::no_match();
        };
        let outcome = runtime.evaluate_track(&points);
        if outcome.passthrough_click {
            self.passthrough_clicks.fetch_add(1, Ordering::SeqCst);
        }
        outcome
    }
}

impl MouseHookBackend for MockMouseHookBackend {
    fn start(&self, runtime: MouseGestureRuntime) -> anyhow::Result<()> {
        self.start_count.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut guard) = self.runtime.lock() {
            *guard = Some(runtime);
        }
        Ok(())
    }

    fn stop(&self) {
        self.stop_count.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut guard) = self.runtime.lock() {
            *guard = None;
        }
    }

    fn is_running(&self) -> bool {
        self.runtime
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }
}
