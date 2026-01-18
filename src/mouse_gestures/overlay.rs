use crate::plugins::mouse_gestures::engine::Point;
use crate::plugins::mouse_gestures::settings::{
    MouseGestureOverlaySettings, MouseGesturePluginSettings,
};
use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

#[derive(Default)]
struct OverlayState {
    settings: MouseGestureOverlaySettings,
    points: Vec<Point>,
    visible: bool,
    preview: Option<(String, Point)>,
}

#[cfg(windows)]
struct GdiOverlayWindow {
    state: Arc<Mutex<OverlayState>>,
    hwnd: Arc<Mutex<Option<isize>>>,
    thread: Option<std::thread::JoinHandle<()>>,
    stroke_id: Arc<AtomicU64>,
}

#[cfg(windows)]
impl GdiOverlayWindow {
    fn new(state: Arc<Mutex<OverlayState>>) -> Self {
        Self {
            state,
            hwnd: Arc::new(Mutex::new(None)),
            thread: None,
            stroke_id: Arc::new(AtomicU64::new(0)),
        }
    }

    fn ensure_thread(&mut self) {
        if self.thread.is_some() {
            return;
        }
        let state = Arc::clone(&self.state);
        let hwnd_store = Arc::clone(&self.hwnd);
        let handle = std::thread::spawn(move || {
            use windows::core::{w, PCWSTR};
            use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
            use windows::Win32::Graphics::Gdi::{
                BeginPaint, CreatePen, DeleteObject, EndPaint, FillRect, GetStockObject, LineTo,
                MoveToEx, RedrawWindow, SelectObject, SetBkMode, SetDCPenColor, SetTextColor,
                TextOutW, BLACK_BRUSH, HBRUSH, PAINTSTRUCT, PS_SOLID, RDW_INVALIDATE,
                RDW_UPDATENOW, TRANSPARENT,
            };
            use windows::Win32::System::LibraryLoader::GetModuleHandleW;
            use windows::Win32::UI::WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetWindowLongPtrW,
                PostQuitMessage, RegisterClassW, SetLayeredWindowAttributes, SetWindowLongPtrW,
                SetWindowPos, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA,
                HMENU, HWND_TOPMOST, LWA_COLORKEY, MSG, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
                SW_SHOW, WM_DESTROY, WM_PAINT, WNDCLASSW, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
                WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
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
                if msg == WM_PAINT {
                    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                    if state_ptr != 0 {
                        let state = &*(state_ptr as *const Mutex<OverlayState>);
                        let mut paint = PAINTSTRUCT::default();
                        let hdc = BeginPaint(hwnd, &mut paint);
                        let mut rect = RECT::default();
                        rect.right = paint.rcPaint.right;
                        rect.bottom = paint.rcPaint.bottom;
                        FillRect(hdc, &rect, HBRUSH(GetStockObject(BLACK_BRUSH).0));
                        if let Ok(state) = state.lock() {
                            if state.visible && state.points.len() >= 2 {
                                let color = parse_color(&state.settings.color);
                                let pen =
                                    CreatePen(PS_SOLID, state.settings.thickness as i32, color);
                                let old = SelectObject(hdc, pen);
                                SetBkMode(hdc, TRANSPARENT);
                                SetDCPenColor(hdc, color);
                                let first = state.points[0];
                                MoveToEx(hdc, first.x as i32, first.y as i32, None);
                                for point in state.points.iter().skip(1) {
                                    LineTo(hdc, point.x as i32, point.y as i32);
                                }
                                SelectObject(hdc, old);
                                DeleteObject(pen);
                            }
                            if let Some((text, point)) = &state.preview {
                                let text_w = to_wide(text);
                                SetTextColor(hdc, windows::Win32::Foundation::COLORREF(0x00ffffff));
                                TextOutW(
                                    hdc,
                                    point.x as i32 + 12,
                                    point.y as i32 + 12,
                                    PCWSTR(text_w.as_ptr()),
                                    (text_w.len().saturating_sub(1)) as i32,
                                );
                            }
                        }
                        EndPaint(hwnd, &paint);
                        return LRESULT(0);
                    }
                }
                if msg == WM_DESTROY {
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
                        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &*state as *const _ as isize);
                        let _ = SetLayeredWindowAttributes(
                            hwnd,
                            windows::Win32::Foundation::COLORREF(0),
                            0,
                            LWA_COLORKEY,
                        );
                        ShowWindow(hwnd, SW_SHOW);
                        let _ = SetWindowPos(
                            hwnd,
                            HWND_TOPMOST,
                            0,
                            0,
                            0,
                            0,
                            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
                        );
                        if let Ok(mut store) = hwnd_store.lock() {
                            *store = Some(hwnd.0 as isize);
                        }
                        RedrawWindow(hwnd, None, None, RDW_INVALIDATE | RDW_UPDATENOW);
                    }
                }

                let mut msg = MSG::default();
                while GetMessageW(&mut msg, HWND(std::ptr::null_mut()), 0, 0).into() {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        });
        self.thread = Some(handle);
    }

    fn invalidate(&self) {
        if let Ok(store) = self.hwnd.lock() {
            if let Some(hwnd) = *store {
                unsafe {
                    let hwnd = windows::Win32::Foundation::HWND(hwnd as *mut _);
                    windows::Win32::Graphics::Gdi::RedrawWindow(
                        hwnd,
                        None,
                        None,
                        windows::Win32::Graphics::Gdi::RDW_INVALIDATE
                            | windows::Win32::Graphics::Gdi::RDW_UPDATENOW,
                    );
                }
            }
        }
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
        if let Ok(mut state) = self.state.lock() {
            state.settings = settings.clone();
        }
        self.ensure_thread();
        self.invalidate();
    }

    fn begin_stroke(&mut self, settings: &MouseGestureOverlaySettings, start: Point) {
        self.ensure_thread();
        self.stroke_id.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut state) = self.state.lock() {
            state.settings = settings.clone();
            state.points.clear();
            state.points.push(start);
            state.visible = true;
            state.preview = None;
        }
        self.invalidate();
    }

    fn push_point(&mut self, settings: &MouseGestureOverlaySettings, point: Point) {
        if let Ok(mut state) = self.state.lock() {
            state.settings = settings.clone();
            state.points.push(point);
        }
        self.invalidate();
    }

    fn end_stroke(&mut self, settings: &MouseGestureOverlaySettings) {
        let fade = settings.fade;
        let state = Arc::clone(&self.state);
        let hwnd = Arc::clone(&self.hwnd);
        let expected = self.stroke_id.load(Ordering::SeqCst);
        let stroke_id = Arc::clone(&self.stroke_id);
        if let Ok(mut state) = self.state.lock() {
            state.settings = settings.clone();
            state.preview = None;
        }
        std::thread::spawn(move || {
            if fade > 0 {
                std::thread::sleep(Duration::from_millis(fade));
            }
            let current = stroke_id.load(Ordering::SeqCst);
            if let Ok(mut state) = state.lock() {
                if current == expected {
                    state.visible = false;
                    state.points.clear();
                    state.preview = None;
                }
            }
            if let Ok(store) = hwnd.lock() {
                if let Some(hwnd) = *store {
                    unsafe {
                        let hwnd = windows::Win32::Foundation::HWND(hwnd as *mut _);
                        windows::Win32::Graphics::Gdi::RedrawWindow(
                            hwnd,
                            None,
                            None,
                            windows::Win32::Graphics::Gdi::RDW_INVALIDATE
                                | windows::Win32::Graphics::Gdi::RDW_UPDATENOW,
                        );
                    }
                }
            }
        });
    }

    fn update_preview(&mut self, text: Option<String>, point: Option<Point>) {
        if let Ok(mut state) = self.state.lock() {
            state.preview = text.zip(point);
        }
        self.invalidate();
    }

    fn shutdown(&mut self) {
        if let Ok(store) = self.hwnd.lock() {
            if let Some(hwnd) = *store {
                unsafe {
                    windows::Win32::UI::WindowsAndMessaging::PostMessageW(
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
        let state = Arc::new(Mutex::new(OverlayState {
            settings: settings.clone(),
            points: Vec::new(),
            visible: false,
            preview: None,
        }));
        #[cfg(windows)]
        let window: Box<dyn OverlayWindow> = Box::new(GdiOverlayWindow::new(Arc::clone(&state)));
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
