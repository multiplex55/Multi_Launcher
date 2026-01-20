use crate::plugins::mouse_gestures::engine::Point;
use crate::plugins::mouse_gestures::settings::{
    MouseGestureOverlaySettings, MouseGesturePluginSettings,
};
use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Rendering surface for gesture overlays.
///
/// Implementations are expected to use transparent, always-on-top windows
/// so the stroke can be drawn over existing applications.
trait OverlayWindow: Send {
    fn update_settings(&mut self, settings: &MouseGestureOverlaySettings);
    fn begin_stroke(&mut self, settings: &MouseGestureOverlaySettings, start: Point);
    fn push_point(&mut self, settings: &MouseGestureOverlaySettings, point: Point);
    fn end_stroke(&mut self, settings: &MouseGestureOverlaySettings);
    fn update_preview(&mut self, text: Option<String>, point: Option<Point>);
    fn shutdown(&mut self);
}

#[derive(Default)]
struct NoopOverlayWindow {
    _settings: MouseGestureOverlaySettings,
}

impl OverlayWindow for NoopOverlayWindow {
    fn update_settings(&mut self, settings: &MouseGestureOverlaySettings) {
        self._settings = settings.clone();
    }

    fn begin_stroke(&mut self, settings: &MouseGestureOverlaySettings, _start: Point) {
        self._settings = settings.clone();
    }

    fn push_point(&mut self, settings: &MouseGestureOverlaySettings, _point: Point) {
        self._settings = settings.clone();
    }

    fn end_stroke(&mut self, settings: &MouseGestureOverlaySettings) {
        self._settings = settings.clone();
    }

    fn update_preview(&mut self, _text: Option<String>, _point: Option<Point>) {}

    fn shutdown(&mut self) {}
}

const INVALIDATE_CADENCE_MS: u32 = 16;
const INVALIDATE_CADENCE: Duration = Duration::from_millis(INVALIDATE_CADENCE_MS as u64);

#[derive(Debug, Clone, PartialEq)]
struct OverlaySnapshot {
    settings: MouseGestureOverlaySettings,
    points: Vec<Point>,
    visible: bool,
    preview: Option<(String, Point)>,
    fade_deadline: Option<Instant>,
}

impl OverlaySnapshot {
    fn new(settings: MouseGestureOverlaySettings) -> Self {
        Self {
            settings,
            points: Vec::new(),
            visible: false,
            preview: None,
            fade_deadline: None,
        }
    }
}

struct OverlaySnapshotBuffer {
    snapshot: Mutex<OverlaySnapshot>,
    version: AtomicU64,
}

impl OverlaySnapshotBuffer {
    fn new(settings: MouseGestureOverlaySettings) -> Self {
        Self {
            snapshot: Mutex::new(OverlaySnapshot::new(settings)),
            version: AtomicU64::new(0),
        }
    }

    fn update<F>(&self, f: F)
    where
        F: FnOnce(&mut OverlaySnapshot),
    {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            f(&mut snapshot);
            self.version.fetch_add(1, Ordering::Release);
        }
    }

    fn snapshot(&self) -> Option<OverlaySnapshot> {
        self.snapshot.lock().ok().map(|snapshot| snapshot.clone())
    }

    fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    fn apply_fade_if_needed(&self, now: Instant) -> bool {
        let mut changed = false;
        if let Ok(mut snapshot) = self.snapshot.lock() {
            if let Some(deadline) = snapshot.fade_deadline {
                if now >= deadline {
                    snapshot.visible = false;
                    snapshot.points.clear();
                    snapshot.preview = None;
                    snapshot.fade_deadline = None;
                    changed = true;
                }
            }
        }
        if changed {
            self.version.fetch_add(1, Ordering::Release);
        }
        changed
    }
}

fn clone_snapshot(buffer: &OverlaySnapshotBuffer) -> Option<OverlaySnapshot> {
    buffer.snapshot()
}

fn should_invalidate(deadline: Instant, now: Instant, dirty: bool) -> bool {
    dirty || now >= deadline
}

fn decimate_points(points: &[Point], max_points: usize) -> Vec<Point> {
    if max_points == 0 || points.len() <= max_points {
        return points.to_vec();
    }
    let last_index = points.len().saturating_sub(1);
    if last_index == 0 || max_points == 1 {
        return vec![points[0]];
    }
    let step = last_index as f32 / (max_points - 1) as f32;
    let mut reduced = Vec::with_capacity(max_points);
    for i in 0..max_points {
        let index = (i as f32 * step).round() as usize;
        reduced.push(points[index.min(last_index)]);
    }
    reduced
}

struct OverlaySnapshotPublisher {
    snapshot: Arc<OverlaySnapshotBuffer>,
    raw_points: Vec<Point>,
}

impl OverlaySnapshotPublisher {
    fn new(snapshot: Arc<OverlaySnapshotBuffer>) -> Self {
        Self {
            snapshot,
            raw_points: Vec::new(),
        }
    }

    fn update_settings(&mut self, settings: &MouseGestureOverlaySettings) {
        let points = decimate_points(&self.raw_points, settings.max_render_points);
        self.snapshot.update(|snapshot| {
            snapshot.settings = settings.clone();
            snapshot.points = points;
        });
    }

    fn begin_stroke(&mut self, settings: &MouseGestureOverlaySettings, start: Point) {
        self.raw_points.clear();
        self.raw_points.push(start);
        let points = decimate_points(&self.raw_points, settings.max_render_points);
        self.snapshot.update(|snapshot| {
            snapshot.settings = settings.clone();
            snapshot.points = points;
            snapshot.visible = true;
            snapshot.preview = None;
            snapshot.fade_deadline = None;
        });
    }

    fn push_point(&mut self, settings: &MouseGestureOverlaySettings, point: Point) {
        self.raw_points.push(point);
        let points = decimate_points(&self.raw_points, settings.max_render_points);
        self.snapshot.update(|snapshot| {
            snapshot.settings = settings.clone();
            snapshot.points = points;
        });
    }

    fn end_stroke(&mut self, settings: &MouseGestureOverlaySettings) {
        let deadline = Instant::now() + Duration::from_millis(settings.fade);
        let points = decimate_points(&self.raw_points, settings.max_render_points);
        self.snapshot.update(|snapshot| {
            snapshot.settings = settings.clone();
            snapshot.points = points;
            snapshot.preview = None;
            snapshot.fade_deadline = Some(deadline);
        });
    }

    fn update_preview(&mut self, text: Option<String>, point: Option<Point>) {
        self.snapshot
            .update(|snapshot| snapshot.preview = text.zip(point));
    }
}

#[cfg(windows)]
struct OverlayThreadState {
    snapshot: Arc<OverlaySnapshotBuffer>,
    repaint_deadline: Mutex<Instant>,
    last_invalidate_version: AtomicU64,
}

#[cfg(windows)]
struct GdiOverlayWindow {
    publisher: OverlaySnapshotPublisher,
    thread_state: Arc<OverlayThreadState>,
    hwnd: Arc<Mutex<Option<isize>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(windows)]
impl GdiOverlayWindow {
    fn new(snapshot: Arc<OverlaySnapshotBuffer>) -> Self {
        let thread_state = Arc::new(OverlayThreadState {
            snapshot: Arc::clone(&snapshot),
            repaint_deadline: Mutex::new(Instant::now()),
            last_invalidate_version: AtomicU64::new(0),
        });
        Self {
            publisher: OverlaySnapshotPublisher::new(snapshot),
            thread_state,
            hwnd: Arc::new(Mutex::new(None)),
            thread: None,
        }
    }

    fn ensure_thread(&mut self) {
        if self.thread.is_some() {
            return;
        }
        let thread_state = Arc::clone(&self.thread_state);
        let hwnd_store = Arc::clone(&self.hwnd);
        let handle = std::thread::spawn(move || {
            use windows::core::{w, PCWSTR};
            use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
            use windows::Win32::Graphics::Gdi::{
                BeginPaint, CreatePen, DeleteObject, EndPaint, FillRect, GetStockObject, LineTo,
                MoveToEx, RedrawWindow, SelectObject, SetBkMode, SetDCPenColor, SetTextColor,
                TextOutW, BLACK_BRUSH, HBRUSH, PAINTSTRUCT, PS_SOLID, RDW_INVALIDATE, TRANSPARENT,
            };
            use windows::Win32::System::LibraryLoader::GetModuleHandleW;
            use windows::Win32::UI::WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetWindowLongPtrW,
                KillTimer, PostQuitMessage, RegisterClassW, SetLayeredWindowAttributes, SetTimer,
                SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, CS_HREDRAW,
                CS_VREDRAW, GWLP_USERDATA, HMENU, HWND_TOPMOST, LWA_COLORKEY, MSG, SWP_NOACTIVATE,
                SWP_NOMOVE, SWP_NOSIZE, SW_SHOW, WM_DESTROY, WM_PAINT, WM_TIMER, WNDCLASSW,
                WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
            };
            use windows::Win32::UI::WindowsAndMessaging::{
                GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN,
            };

            unsafe extern "system" fn wndproc(
                hwnd: HWND,
                msg: u32,
                wparam: WPARAM,
                lparam: LPARAM,
            ) -> LRESULT {
                let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if state_ptr != 0 {
                    let state = &*(state_ptr as *const OverlayThreadState);
                    if msg == WM_TIMER {
                        let now = Instant::now();
                        state.snapshot.apply_fade_if_needed(now);
                        let version = state.snapshot.version();
                        let last_version = state.last_invalidate_version.load(Ordering::Relaxed);
                        let dirty = version != last_version;
                        if let Ok(mut deadline) = state.repaint_deadline.lock() {
                            if should_invalidate(*deadline, now, dirty) {
                                *deadline = now + INVALIDATE_CADENCE;
                                state
                                    .last_invalidate_version
                                    .store(version, Ordering::Relaxed);
                                let _ = RedrawWindow(hwnd, None, None, RDW_INVALIDATE);
                            }
                        }
                        return LRESULT(0);
                    }
                    if msg == WM_PAINT {
                        let mut paint = PAINTSTRUCT::default();
                        let hdc = BeginPaint(hwnd, &mut paint);
                        let mut rect = RECT::default();
                        rect.right = paint.rcPaint.right;
                        rect.bottom = paint.rcPaint.bottom;
                        FillRect(hdc, &rect, HBRUSH(GetStockObject(BLACK_BRUSH).0));
                        let snapshot = clone_snapshot(&state.snapshot);
                        if let Some(snapshot) = snapshot {
                            if snapshot.visible && snapshot.points.len() >= 2 {
                                let color = parse_color(&snapshot.settings.color);
                                let pen =
                                    CreatePen(PS_SOLID, snapshot.settings.thickness as i32, color);
                                let old = SelectObject(hdc, pen);
                                SetBkMode(hdc, TRANSPARENT);
                                SetDCPenColor(hdc, color);
                                let first = snapshot.points[0];
                                let _ = MoveToEx(hdc, first.x as i32, first.y as i32, None);
                                for point in snapshot.points.iter().skip(1) {
                                    let _ = LineTo(hdc, point.x as i32, point.y as i32);
                                }
                                SelectObject(hdc, old);
                                let _ = DeleteObject(pen);
                            }
                            if let Some((text, point)) = &snapshot.preview {
                                let text_w = to_wide(text);
                                SetTextColor(hdc, windows::Win32::Foundation::COLORREF(0x00ffffff));
                                let _ = TextOutW(
                                    hdc,
                                    point.x as i32 + 12,
                                    point.y as i32 + 12,
                                    &text_w,
                                );
                            }
                        }
                        let _ = EndPaint(hwnd, &paint);
                        return LRESULT(0);
                    }
                }
                if msg == WM_DESTROY {
                    unsafe {
                        let _ = KillTimer(hwnd, 1);
                    }
                    PostQuitMessage(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }

            unsafe {
                let class_name = w!("MultiLauncherGestureOverlay");
                let hinstance = GetModuleHandleW(None).unwrap_or_default();
                let wc = WNDCLASSW {
                    style: CS_HREDRAW | CS_VREDRAW,
                    lpfnWndProc: Some(wndproc),
                    hInstance: hinstance.into(),
                    lpszClassName: class_name,
                    ..Default::default()
                };
                let _ = RegisterClassW(&wc);
                let width = GetSystemMetrics(SM_CXSCREEN);
                let height = GetSystemMetrics(SM_CYSCREEN);
                let hwnd = CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                    wc.lpszClassName,
                    PCWSTR::null(),
                    WS_POPUP,
                    0,
                    0,
                    width,
                    height,
                    None,
                    HMENU::default(),
                    hinstance,
                    None,
                )
                .ok();
                if let Some(hwnd) = hwnd {
                    if hwnd.0 != std::ptr::null_mut() {
                        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &*thread_state as *const _ as isize);
                        let _ = SetLayeredWindowAttributes(
                            hwnd,
                            windows::Win32::Foundation::COLORREF(0),
                            0,
                            LWA_COLORKEY,
                        );
                        let _ = ShowWindow(hwnd, SW_SHOW);
                        let _ = SetWindowPos(
                            hwnd,
                            HWND_TOPMOST,
                            0,
                            0,
                            0,
                            0,
                            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
                        );
                        let _ = SetTimer(hwnd, 1, INVALIDATE_CADENCE_MS, None);
                        if let Ok(mut store) = hwnd_store.lock() {
                            *store = Some(hwnd.0 as isize);
                        }
                        let _ = RedrawWindow(hwnd, None, None, RDW_INVALIDATE);
                    }
                }

                let mut msg = MSG::default();
                while GetMessageW(&mut msg, HWND(std::ptr::null_mut()), 0, 0).into() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        });
        self.thread = Some(handle);
    }
}

#[cfg(windows)]
fn parse_color(value: &str) -> windows::Win32::Foundation::COLORREF {
    let raw = value.trim().trim_start_matches('#');
    if raw.len() != 6 {
        return windows::Win32::Foundation::COLORREF(0x00ff66cc);
    }
    let r = u8::from_str_radix(&raw[0..2], 16).unwrap_or(0xff);
    let g = u8::from_str_radix(&raw[2..4], 16).unwrap_or(0x66);
    let b = u8::from_str_radix(&raw[4..6], 16).unwrap_or(0xcc);
    windows::Win32::Foundation::COLORREF((b as u32) | ((g as u32) << 8) | ((r as u32) << 16))
}

#[cfg(windows)]
fn to_wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
impl OverlayWindow for GdiOverlayWindow {
    fn update_settings(&mut self, settings: &MouseGestureOverlaySettings) {
        self.publisher.update_settings(settings);
        self.ensure_thread();
    }

    fn begin_stroke(&mut self, settings: &MouseGestureOverlaySettings, start: Point) {
        self.ensure_thread();
        self.publisher.begin_stroke(settings, start);
    }

    fn push_point(&mut self, settings: &MouseGestureOverlaySettings, point: Point) {
        self.ensure_thread();
        self.publisher.push_point(settings, point);
    }

    fn end_stroke(&mut self, settings: &MouseGestureOverlaySettings) {
        self.ensure_thread();
        self.publisher.end_stroke(settings);
    }

    fn update_preview(&mut self, text: Option<String>, point: Option<Point>) {
        self.ensure_thread();
        self.publisher.update_preview(text, point);
    }

    fn shutdown(&mut self) {
        if let Ok(store) = self.hwnd.lock() {
            if let Some(hwnd) = *store {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        windows::Win32::Foundation::HWND(hwnd as *mut _),
                        windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    );
                }
            }
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

pub struct StrokeOverlay {
    settings: MouseGestureOverlaySettings,
    window: Box<dyn OverlayWindow>,
}

impl StrokeOverlay {
    pub fn new() -> Self {
        let settings = MouseGestureOverlaySettings::default();
        #[cfg(windows)]
        let snapshot = Arc::new(OverlaySnapshotBuffer::new(settings.clone()));
        #[cfg(windows)]
        let window: Box<dyn OverlayWindow> = Box::new(GdiOverlayWindow::new(Arc::clone(&snapshot)));
        #[cfg(not(windows))]
        let window: Box<dyn OverlayWindow> = Box::new(NoopOverlayWindow::default());
        Self { settings, window }
    }

    pub fn update_settings(&mut self, plugin_settings: &MouseGesturePluginSettings) {
        self.settings = plugin_settings.overlay.clone();
        self.window.update_settings(&self.settings);
    }

    pub fn begin_stroke(&mut self, start: Point) {
        self.window.begin_stroke(&self.settings, start);
    }

    pub fn push_point(&mut self, point: Point) {
        self.window.push_point(&self.settings, point);
    }

    pub fn end_stroke(&mut self) {
        self.window.end_stroke(&self.settings);
    }

    pub fn update_preview(&mut self, text: Option<String>, point: Option<Point>) {
        self.window.update_preview(text, point);
    }

    pub fn shutdown(&mut self) {
        self.window.shutdown();
    }
}

impl Default for StrokeOverlay {
    fn default() -> Self {
        Self::new()
    }
}

static OVERLAY: OnceCell<Arc<Mutex<StrokeOverlay>>> = OnceCell::new();

pub fn mouse_gesture_overlay() -> Arc<Mutex<StrokeOverlay>> {
    OVERLAY
        .get_or_init(|| Arc::new(Mutex::new(StrokeOverlay::new())))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_invalidate_boundaries() {
        let base = Instant::now();
        let deadline = base + Duration::from_millis(16);
        assert!(!should_invalidate(
            deadline,
            base + Duration::from_millis(15),
            false
        ));
        assert!(should_invalidate(
            deadline,
            base + Duration::from_millis(16),
            false
        ));
        assert!(should_invalidate(
            deadline,
            base + Duration::from_millis(1),
            true
        ));
    }

    #[test]
    fn snapshot_buffer_updates_preview_without_mutation() {
        let settings = MouseGestureOverlaySettings::default();
        let buffer = Arc::new(OverlaySnapshotBuffer::new(settings.clone()));
        let mut publisher = OverlaySnapshotPublisher::new(Arc::clone(&buffer));

        publisher.begin_stroke(&settings, Point { x: 1.0, y: 2.0 });
        publisher.push_point(&settings, Point { x: 3.0, y: 4.0 });
        let version_after_points = buffer.version();

        publisher.update_preview(Some("Preview".into()), Some(Point { x: 9.0, y: 8.0 }));
        assert!(buffer.version() > version_after_points);

        let snapshot = buffer.snapshot().expect("snapshot");
        assert_eq!(snapshot.points.len(), 2);
        assert_eq!(
            snapshot.preview,
            Some(("Preview".into(), Point { x: 9.0, y: 8.0 }))
        );
    }

    #[test]
    fn clone_snapshot_returns_independent_copy() {
        let settings = MouseGestureOverlaySettings::default();
        let buffer = OverlaySnapshotBuffer::new(settings.clone());
        buffer.update(|snapshot| {
            snapshot.settings = settings;
            snapshot.points = vec![Point { x: 1.0, y: 2.0 }];
            snapshot.visible = true;
        });

        let mut snapshot = clone_snapshot(&buffer).expect("snapshot");
        snapshot.points.push(Point { x: 3.0, y: 4.0 });

        let snapshot_again = clone_snapshot(&buffer).expect("snapshot");
        assert_eq!(snapshot_again.points.len(), 1);
    }

    #[test]
    fn fade_deadline_hides_snapshot_on_tick() {
        let settings = MouseGestureOverlaySettings::default();
        let buffer = OverlaySnapshotBuffer::new(settings.clone());
        let now = Instant::now();
        buffer.update(|snapshot| {
            snapshot.settings = settings;
            snapshot.points = vec![Point { x: 1.0, y: 2.0 }];
            snapshot.visible = true;
            snapshot.fade_deadline = Some(now);
        });

        assert!(buffer.apply_fade_if_needed(now));
        let snapshot = buffer.snapshot().expect("snapshot");
        assert!(!snapshot.visible);
        assert!(snapshot.points.is_empty());
        assert!(snapshot.preview.is_none());
        assert!(snapshot.fade_deadline.is_none());
    }
}
