//! Win32 color-key overlay renderer.  There is deliberately one popup per
//! physical monitor: a single virtual-desktop popup would cover monitor gaps.
use super::*;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};
use windows::{
    Win32::{
        Foundation::{
            COLORREF, ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, POINT, RECT,
            SetLastError, WIN32_ERROR, WPARAM,
        },
        Graphics::Gdi::{
            BeginPaint, CreatePen, CreateSolidBrush, DeleteObject, EndPaint, EnumDisplayMonitors,
            FillRect, GetMonitorInfoW, GetStockObject, HDC, HGDIOBJ, HMONITOR, InvalidateRect,
            MONITORINFO, NULL_BRUSH, PAINTSTRUCT, PS_SOLID, RDW_INVALIDATE, RDW_UPDATENOW,
            Rectangle, RedrawWindow, SelectObject, SetBkMode, SetTextColor, TRANSPARENT, TextOutW,
            UpdateWindow,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{GetDpiForWindow, GetThreadDpiAwarenessContext, GetWindowDpiAwarenessContext},
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, GetCapture, ReleaseCapture, SetCapture, VK_ESCAPE, VK_LBUTTON,
                VK_RETURN,
            },
            WindowsAndMessaging::{
                CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW,
                DestroyWindow, DispatchMessageW, GWL_EXSTYLE, GWLP_USERDATA, GetCursorPos,
                GetWindowLongPtrW, GetWindowRect, HTTRANSPARENT, HWND_TOPMOST, IsWindow,
                IsWindowVisible, LWA_ALPHA, LWA_COLORKEY, MA_NOACTIVATE, MSG, PM_REMOVE,
                PeekMessageW, RegisterClassW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE,
                SWP_SHOWWINDOW, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos,
                ShowWindow, TranslateMessage, WM_CAPTURECHANGED, WM_DISPLAYCHANGE, WM_ERASEBKGND,
                WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_NCCREATE,
                WM_NCHITTEST, WM_PAINT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
                WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
            },
        },
    },
    core::PCWSTR,
};

// COLORREF is 0x00bbggrr. Magenta is reserved exclusively for transparency.
const TRANSPARENT_KEY: COLORREF = COLORREF(0x00ff00ff);
const OUTLINE_COLOR: COLORREF = COLORREF(0x0000ffff); // bright yellow
const BADGE_COLOR: COLORREF = COLORREF(0x00400000); // dark blue
const LABEL_COLOR: COLORREF = COLORREF(0x00ffffff);
const OCR_LINE_COLOR: COLORREF = COLORREF(0x0000a5ff); // orange
const OCR_WORD_COLOR: COLORREF = COLORREF(0x00ffbf50); // light blue
const OCR_SELECTED_COLOR: COLORREF = COLORREF(0x0000ff00); // green

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowRole {
    VisibleOverlay,
    TransparentOverlay,
    Passive,
    Tooltip,
    InputShield,
}

fn role_is_hit_test_transparent(role: WindowRole) -> bool {
    matches!(
        role,
        WindowRole::TransparentOverlay | WindowRole::Passive | WindowRole::Tooltip
    )
}

fn full_surface_role(visual: &OverlayVisual) -> WindowRole {
    if overlay_is_mouse_transparent(visual) {
        WindowRole::TransparentOverlay
    } else {
        WindowRole::VisibleOverlay
    }
}

/// One input stream shared by every monitor shield. Native messages retain
/// complete rapid transactions even when both edges arrive between UI ticks.
#[derive(Default)]
struct RectangleNativeInput {
    ready: bool,
    startup_left_down: bool,
    left_down: bool,
    owned_left_down: bool,
    capture_lost: bool,
    topology_changed: bool,
    release_boundary: bool,
    events: Vec<OverlayInputKind>,
}
impl RectangleNativeInput {
    fn acknowledge(&mut self, left_down: bool) {
        self.events.clear();
        self.release_boundary = false;
        self.left_down = left_down;
        self.owned_left_down = false;
        self.startup_left_down = left_down;
        self.ready = true;
    }
    fn record(&mut self, msg: u32, point: MkPoint) {
        // A shield Down is fresh even if launch Up went to a foreign capture
        // owner before our first poll. The baseline was never our owned press.
        if msg == WM_LBUTTONDOWN {
            self.release_boundary = false;
            if self.ready && self.startup_left_down {
                self.events.push(OverlayInputKind::LeftReleased(point));
            }
            self.startup_left_down = false;
            if !self.owned_left_down {
                self.owned_left_down = true;
                self.left_down = true;
                if self.ready {
                    self.events.push(OverlayInputKind::LeftPressed(point));
                }
            }
        } else if msg == WM_LBUTTONUP {
            self.release_boundary = self.awaiting_release();
            self.owned_left_down = false;
            if self.left_down && self.ready {
                self.events.push(OverlayInputKind::LeftReleased(point));
            }
            self.left_down = false;
            self.startup_left_down = false;
        } else if msg == WM_MOUSEMOVE && self.ready {
            self.events.push(OverlayInputKind::PointerMoved(point));
        }
    }
    fn record_system_message(&mut self, msg: u32) {
        if msg == WM_CAPTURECHANGED && self.owned_left_down {
            self.capture_lost = true;
        }
        if msg == WM_DISPLAYCHANGE {
            self.topology_changed = true;
        }
    }
    fn wait_for_close_release(&mut self, physical_left: bool) -> bool {
        // Foreign launch Up is not ours; lost capture likewise cannot be
        // represented as a consumed native Up. Wait for physical-up after the
        // native queue is drained, then safely retire the interrupted picker.
        if !physical_left && (self.startup_left_down || self.capture_lost) {
            self.left_down = false;
            self.owned_left_down = false;
            self.startup_left_down = false;
        }
        self.events.clear();
        self.awaiting_release()
    }
    fn awaiting_release(&self) -> bool {
        self.left_down || self.owned_left_down
    }
    fn saw_release(&self) -> bool {
        self.release_boundary
    }
}

struct WindowPaintState {
    bounds: ScreenRect,
    frame: Vec<OverlayFramePrimitive>,
    hint: Option<String>,
    solid: Option<COLORREF>,
    operation_id: OperationId,
    description: &'static str,
    role: WindowRole,
    paint_count: AtomicUsize,
    rectangle_input: Option<Arc<Mutex<RectangleNativeInput>>>,
}
struct OverlayWindow {
    hwnd: HWND,
    // The pointee address stays stable while wndproc reads it through GWLP_USERDATA.
    state: Box<WindowPaintState>,
    created_at: Instant,
}

#[derive(Default)]
pub(super) struct NativeOverlayRenderer {
    // Full-monitor, layered picker surfaces only.
    windows: Vec<OverlayWindow>,
    // Non-transparent, non-activating input consumers used only by rectangle picking.
    input_shields: Vec<OverlayWindow>,
    rectangle_input: Option<Arc<Mutex<RectangleNativeInput>>>,
    // Small, opaque, non-activating passive resources.
    passive_edges: Vec<OverlayWindow>,
    passive_badges: Vec<OverlayWindow>,
    tooltip: Option<OverlayWindow>,
    operation_id: Option<OperationId>,
    visual: Option<OverlayVisual>,
    left_down: bool,
    escape_down: bool,
    return_down: bool,
    closing: bool,
    capture_recovery_attempted: bool,
}
unsafe impl Send for NativeOverlayRenderer {}

fn platform(message: impl Into<String>) -> VisualOverlayError {
    VisualOverlayError {
        kind: OverlayErrorKind::Platform,
        message: message.into(),
    }
}

fn key_pressed(current_down: bool, previous_down: bool) -> bool {
    current_down && !previous_down
}

fn win32_dimensions(
    bounds: ScreenRect,
    description: &str,
) -> Result<(i32, i32), VisualOverlayError> {
    let width = i32::try_from(bounds.width).map_err(|_| {
        platform(format!(
            "{description} rectangle {bounds:?} width exceeds Win32 range"
        ))
    })?;
    let height = i32::try_from(bounds.height).map_err(|_| {
        platform(format!(
            "{description} rectangle {bounds:?} height exceeds Win32 range"
        ))
    })?;
    Ok((width, height))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShieldFailure {
    MissingShield,
    WindowRectQueryFailed,
    StyleQueryFailed,
    RectMismatch,
    WindowNotVisible,
    LayeredMissing,
    NoactivateMissing,
    TopmostMissing,
    UnexpectedTransparent,
}

impl ShieldFailure {
    fn description(self) -> &'static str {
        match self {
            Self::MissingShield => "missing shield",
            Self::WindowRectQueryFailed => "GetWindowRect failed",
            Self::StyleQueryFailed => "GetWindowLongPtrW failed",
            Self::RectMismatch => "window position/size mismatch",
            Self::WindowNotVisible => "window is not visible",
            Self::LayeredMissing => "layered style missing",
            Self::NoactivateMissing => "nonactivating style missing",
            Self::TopmostMissing => "topmost style missing",
            Self::UnexpectedTransparent => "unexpected click-through style",
        }
    }
}

#[derive(Debug)]
struct ShieldProbe {
    actual: Option<ScreenRect>,
    rect_error: Option<u32>,
    visible: bool,
    ex: u32,
    style_error: Option<u32>,
    thread_dpi_context: isize,
    window_dpi_context: isize,
    window_dpi: u32,
}
impl ShieldProbe {
    fn failures(&self, expected: ScreenRect) -> Vec<ShieldFailure> {
        let mut reasons = Vec::new();
        if self.actual.is_none() {
            reasons.push(ShieldFailure::WindowRectQueryFailed);
        }
        if self.style_error.is_some() {
            reasons.push(ShieldFailure::StyleQueryFailed);
        }
        if self
            .actual
            .is_some_and(|actual| actual != expected || expected.is_empty())
        {
            reasons.push(ShieldFailure::RectMismatch);
        }
        if !self.visible {
            reasons.push(ShieldFailure::WindowNotVisible);
        }
        for (flag, reason) in [
            (WS_EX_LAYERED.0, ShieldFailure::LayeredMissing),
            (WS_EX_NOACTIVATE.0, ShieldFailure::NoactivateMissing),
            (WS_EX_TOPMOST.0, ShieldFailure::TopmostMissing),
        ] {
            if self.ex & flag == 0 {
                reasons.push(reason);
            }
        }
        if self.ex & WS_EX_TRANSPARENT.0 != 0 {
            reasons.push(ShieldFailure::UnexpectedTransparent);
        }
        reasons
    }
}

fn probe_shield(hwnd: HWND) -> ShieldProbe {
    let mut rect = RECT::default();
    let result = unsafe { GetWindowRect(hwnd, &mut rect) };
    // Capture before another Win32 call can replace the error.
    let rect_error = result.err().map(|_| unsafe { GetLastError().0 });
    let actual = rect_error.is_none().then(|| {
        ScreenRect::new(
            rect.left,
            rect.top,
            (i64::from(rect.right) - i64::from(rect.left)).max(0) as u32,
            (i64::from(rect.bottom) - i64::from(rect.top)).max(0) as u32,
        )
    });
    unsafe {
        SetLastError(WIN32_ERROR(0));
    }
    let ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    let style_error = if ex == 0 {
        let error = unsafe { GetLastError().0 };
        (error != 0).then_some(error)
    } else {
        None
    };
    ShieldProbe {
        actual,
        rect_error,
        ex,
        style_error,
        visible: unsafe { IsWindowVisible(hwnd) }.as_bool(),
        thread_dpi_context: unsafe { GetThreadDpiAwarenessContext() }.0 as isize,
        window_dpi_context: unsafe { GetWindowDpiAwarenessContext(hwnd) }.0 as isize,
        window_dpi: unsafe { GetDpiForWindow(hwnd) },
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = unsafe { &*(l.0 as *const CREATESTRUCTW) };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize) };
    }
    let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowPaintState };
    match msg {
        WM_ERASEBKGND => LRESULT(1), // WM_PAINT owns clearing the complete surface.
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST
            if !state.is_null() && unsafe { role_is_hit_test_transparent((*state).role) } =>
        {
            LRESULT(HTTRANSPARENT as isize)
        }
        WM_PAINT if !state.is_null() => {
            let mut ps = PAINTSTRUCT::default();
            let dc = unsafe { BeginPaint(hwnd, &mut ps) };
            unsafe { paint_frame(dc, &*state) };
            let count = unsafe { &*state }
                .paint_count
                .fetch_add(1, Ordering::Relaxed)
                + 1;
            tracing::debug!(operation_id = unsafe { &*state }.operation_id, ?hwnd,
                edge = unsafe { &*state }.description, rect = ?unsafe { &*state }.bounds,
                paint_count = count, "painted overlay window");
            unsafe {
                let _ = EndPaint(hwnd, &ps);
            };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}

unsafe extern "system" fn input_shield_wndproc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = unsafe { &*(l.0 as *const CREATESTRUCTW) };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize) };
    }
    match msg {
        // These windows are intentionally hit-testable, but never activate and
        // never forward mouse messages to the application below them.
        WM_CAPTURECHANGED | WM_DISPLAYCHANGE => {
            let state =
                unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowPaintState };
            if !state.is_null()
                && let Some(input) = unsafe { &*state }.rectangle_input.as_ref()
                && let Ok(mut input) = input.lock()
            {
                input.record_system_message(msg);
            }
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_LBUTTONDOWN | WM_LBUTTONUP | WM_MOUSEMOVE => {
            let state =
                unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowPaintState };
            if !state.is_null() {
                let state = unsafe { &*state };
                if let Some(input) = &state.rectangle_input {
                    // Mouse LPARAM is signed client coordinates, including captured
                    // positions beyond this monitor. Preserve the virtual origin.
                    let point = MkPoint {
                        x: state.bounds.x.saturating_add(l.0 as u16 as i16 as i32),
                        y: state
                            .bounds
                            .y
                            .saturating_add((l.0 >> 16) as u16 as i16 as i32),
                    };
                    if let Ok(mut input) = input.lock() {
                        input.record(msg, point);
                    }
                }
            }
            if msg == WM_LBUTTONDOWN {
                unsafe {
                    SetCapture(hwnd);
                }
            }
            if msg == WM_LBUTTONUP && unsafe { GetCapture() } == hwnd {
                let _ = unsafe { ReleaseCapture() };
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let state =
                unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowPaintState };
            if state.is_null() {
                return unsafe { DefWindowProcW(hwnd, msg, w, l) };
            }
            let mut ps = PAINTSTRUCT::default();
            let dc = unsafe { BeginPaint(hwnd, &mut ps) };
            unsafe { paint_input_shield(dc, &*state) };
            unsafe {
                let _ = (&*state).paint_count.fetch_add(1, Ordering::Relaxed);
                let _ = EndPaint(hwnd, &ps);
            };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}

unsafe fn paint_input_shield(dc: HDC, state: &WindowPaintState) {
    let client = RECT {
        left: 0,
        top: 0,
        right: state.bounds.width as i32,
        bottom: state.bounds.height as i32,
    };
    // The one-alpha layered surface is deliberately black and visually
    // imperceptible. It is not color-keyed and never uses alpha zero.
    let surface = unsafe { CreateSolidBrush(COLORREF(0)) };
    unsafe { FillRect(dc, &client, surface) };
    unsafe {
        let _ = DeleteObject(HGDIOBJ(surface.0));
    };
}

unsafe fn paint_frame(dc: HDC, state: &WindowPaintState) {
    let client = RECT {
        left: 0,
        top: 0,
        right: state.bounds.width as i32,
        bottom: state.bounds.height as i32,
    };
    let clear = unsafe { CreateSolidBrush(state.solid.unwrap_or(TRANSPARENT_KEY)) };
    unsafe { FillRect(dc, &client, clear) };
    unsafe {
        let _ = DeleteObject(HGDIOBJ(clear.0));
    };
    if state.solid.is_some() {
        return;
    }

    if let Some(text) = &state.hint {
        let background = unsafe { CreateSolidBrush(COLORREF(0x00202020)) };
        unsafe { FillRect(dc, &client, background) };
        unsafe {
            let _ = DeleteObject(HGDIOBJ(background.0));
        }
        unsafe {
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, LABEL_COLOR);
        }
        for (line, value) in text.lines().enumerate() {
            let encoded: Vec<u16> = value.encode_utf16().collect();
            unsafe {
                let _ = TextOutW(dc, 10, 9 + line as i32 * 18, &encoded);
            }
        }
        return;
    }
    let pen = unsafe { CreatePen(PS_SOLID, RECTANGLE_OUTLINE_WIDTH, OUTLINE_COLOR) };
    let old_pen = unsafe { SelectObject(dc, HGDIOBJ(pen.0)) };
    let old_brush = unsafe { SelectObject(dc, GetStockObject(NULL_BRUSH)) };
    for primitive in &state.frame {
        match primitive {
            OverlayFramePrimitive::Clear => {}
            OverlayFramePrimitive::Outline(rect) => {
                let (left, top, right, bottom) = desktop_to_overlay(*rect, state.bounds);
                unsafe {
                    let _ = Rectangle(dc, left as i32, top as i32, right as i32, bottom as i32);
                };
            }
            OverlayFramePrimitive::MonitorLabel { bounds, index } => unsafe {
                draw_label(dc, state.bounds, *bounds, *index)
            },
            OverlayFramePrimitive::OcrOutline { bounds, style } => {
                let (color, width) = match style {
                    OcrDebugStyle::SearchRegion => (OUTLINE_COLOR, 3),
                    OcrDebugStyle::Line => (OCR_LINE_COLOR, 2),
                    OcrDebugStyle::Word => (OCR_WORD_COLOR, 1),
                    OcrDebugStyle::SelectedMatch => (OCR_SELECTED_COLOR, 4),
                };
                let styled_pen = unsafe { CreatePen(PS_SOLID, width, color) };
                let previous = unsafe { SelectObject(dc, HGDIOBJ(styled_pen.0)) };
                let (left, top, right, bottom) = desktop_to_overlay(*bounds, state.bounds);
                unsafe {
                    let _ = Rectangle(dc, left as i32, top as i32, right as i32, bottom as i32);
                    SelectObject(dc, previous);
                    let _ = DeleteObject(HGDIOBJ(styled_pen.0));
                }
            }
        }
    }
    unsafe { SelectObject(dc, old_brush) };
    unsafe { SelectObject(dc, old_pen) };
    unsafe {
        let _ = DeleteObject(HGDIOBJ(pen.0));
    };
}

unsafe fn draw_label(dc: HDC, origin: ScreenRect, bounds: ScreenRect, index: usize) {
    let (left, top, _, _) = desktop_to_overlay(bounds, origin);
    let badge = RECT {
        left: left as i32 + 24,
        top: top as i32 + 24,
        right: left as i32 + 86,
        bottom: top as i32 + 66,
    };
    let brush = unsafe { CreateSolidBrush(BADGE_COLOR) };
    unsafe { FillRect(dc, &badge, brush) };
    unsafe {
        let _ = DeleteObject(HGDIOBJ(brush.0));
    };
    let text: Vec<u16> = index.to_string().encode_utf16().collect();
    unsafe { SetBkMode(dc, TRANSPARENT) };
    unsafe { SetTextColor(dc, LABEL_COLOR) };
    unsafe {
        let _ = TextOutW(dc, badge.left + 18, badge.top + 12, &text);
    };
}

fn displays() -> Result<Vec<ScreenRect>, VisualOverlayError> {
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> windows::Win32::Foundation::BOOL {
        let out = unsafe { &mut *(data.0 as *mut Vec<ScreenRect>) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            let r = info.rcMonitor;
            if r.right > r.left && r.bottom > r.top {
                out.push(ScreenRect::new(
                    r.left,
                    r.top,
                    (r.right - r.left) as u32,
                    (r.bottom - r.top) as u32,
                ));
            }
        }
        true.into()
    }
    let mut result = vec![];
    let ok = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut result as *mut _ as isize),
        )
    };
    if !ok.as_bool() {
        Err(platform(format!(
            "Could not enumerate physical monitors: {}",
            unsafe { GetLastError().0 }
        )))
    } else {
        Ok(result)
    }
}

impl NativeOverlayRenderer {
    fn target(visual: &OverlayVisual) -> Option<ScreenRect> {
        match visual {
            OverlayVisual::PointPicker { .. } => None,
            OverlayVisual::RectanglePicker {
                virtual_desktop, ..
            } => Some(*virtual_desktop),
            OverlayVisual::RectanglePreview(r) | OverlayVisual::Window { rect: r, .. } => Some(*r),
            OverlayVisual::Monitor(d) => Some(d.bounds),
            OverlayVisual::Monitors(ds) => monitor_union(ds),
            OverlayVisual::Desktop(_) => None,
            OverlayVisual::OcrDebug(plan) => Some(plan.region),
        }
    }
    fn fail<T>(&mut self, message: impl Into<String>) -> Result<T, VisualOverlayError> {
        let error = platform(message);
        self.close();
        Err(error)
    }

    fn destroy_windows_unchecked(&mut self) {
        if self.operation_id.is_some()
            || !self.windows.is_empty()
            || !self.input_shields.is_empty()
            || !self.passive_edges.is_empty()
            || !self.passive_badges.is_empty()
        {
            tracing::debug!(operation_id = ?self.operation_id, visual = ?self.visual, "closing native visual overlay");
        }
        for window in self.input_shields.drain(..) {
            let destroyed_at = Instant::now();
            tracing::debug!(
                operation_id=?self.operation_id,
                hwnd=?window.hwnd,
                created_at=?window.created_at,
                destruction_time=?destroyed_at,
                lifetime=?destroyed_at.duration_since(window.created_at),
                paint_count=window.state.paint_count.load(Ordering::Relaxed),
                "destroying input shield window"
            );
            destroy_overlay_window(window);
        }
        if let Some(window) = self.tooltip.take() {
            destroy_overlay_window(window);
        }
        for window in self.windows.drain(..) {
            destroy_overlay_window(window);
        }
        for window in self
            .passive_edges
            .drain(..)
            .chain(self.passive_badges.drain(..))
        {
            let destroyed_at = Instant::now();
            tracing::debug!(operation_id=?self.operation_id, hwnd=?window.hwnd,
                edge=window.state.description, created_at=?window.created_at,
                destruction_time=?destroyed_at, lifetime=?destroyed_at.duration_since(window.created_at),
                paint_count=window.state.paint_count.load(Ordering::Relaxed), "destroying passive overlay window");
            destroy_overlay_window(window);
        }
        self.closing = false;
        self.capture_recovery_attempted = false;
        self.rectangle_input = None;
        self.operation_id = None;
        self.visual = None;
        self.left_down = false;
        self.escape_down = false;
        self.return_down = false;
    }
    fn pump_messages(&mut self) {
        if self
            .rectangle_input
            .as_ref()
            .is_some_and(|input| input.lock().is_ok_and(|input| input.saw_release()))
        {
            return;
        }
        let mut msg = MSG::default();
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            // Return Up before dispatching a later press. Terminal transitions
            // tear down at this boundary, so later input cannot extend a drag.
            if self
                .rectangle_input
                .as_ref()
                .is_some_and(|input| input.lock().is_ok_and(|input| input.saw_release()))
            {
                break;
            }
        }
    }
    fn destroy_windows(&mut self) {
        for window in &self.input_shields {
            if unsafe { GetCapture() } == window.hwnd {
                let _ = unsafe { ReleaseCapture() };
            }
        }
        self.destroy_windows_unchecked();
    }
    fn refresh_shield_coverage(&mut self) -> Result<(), VisualOverlayError> {
        let physical = displays()?;
        if physical.is_empty() {
            return Err(platform(
                "No active monitor remains for rectangle input coverage",
            ));
        }
        let missing: Vec<_> = physical
            .iter()
            .copied()
            .filter(|bounds| {
                !self.input_shields.iter().any(|window| {
                    window.state.bounds == *bounds
                        && unsafe { IsWindowVisible(window.hwnd) }.as_bool()
                })
            })
            .collect();
        if !missing.is_empty() {
            let module = unsafe { GetModuleHandleW(None) }
                .map_err(|e| platform(format!("shield repair module lookup failed: {e}")))?;
            let operation_id = self
                .operation_id
                .or_else(|| self.input_shields.first().map(|w| w.state.operation_id))
                .unwrap_or(0);
            self.show_input_shields(
                module,
                windows::core::w!("MultiLauncherVisualOverlayInputShield"),
                operation_id,
                &missing,
            )?;
        }
        self.verify_shield_coverage(&physical, "display_change")?;
        // Retain old shields through release; never destroy the old capture
        // owner first while repairing coverage for a changed topology.
        Ok(())
    }
    fn verify_shield_coverage(
        &self,
        bounds: &[ScreenRect],
        stage: &'static str,
    ) -> Result<(), VisualOverlayError> {
        for (monitor, expected) in bounds.iter().enumerate() {
            let mut rejected = None;
            let covered = self
                .input_shields
                .iter()
                .filter(|window| window.state.bounds == *expected)
                .any(|window| {
                    let probe = probe_shield(window.hwnd);
                    let reasons = probe.failures(*expected);
                    if reasons.is_empty() {
                        true
                    } else {
                        rejected = Some((window, probe, reasons));
                        false
                    }
                });
            if covered {
                continue;
            }
            let Some((window, probe, reasons)) = rejected else {
                tracing::error!(operation_id=?self.operation_id.or_else(|| self.input_shields.first().map(|w| w.state.operation_id)), stage, monitor, ?expected,
                    reason=?ShieldFailure::MissingShield, "rectangle shield acknowledgement failed");
                return Err(platform(format!(
                    "Rectangle input shield monitor {}: {}",
                    monitor + 1,
                    ShieldFailure::MissingShield.description()
                )));
            };
            tracing::error!(operation_id=window.state.operation_id, stage, monitor, hwnd=?window.hwnd,
                    ?expected, actual=?probe.actual, get_window_rect_error=?probe.rect_error,
                    get_window_long_ptr_error=?probe.style_error, visible=probe.visible, ex=probe.ex,
                    thread_dpi_context=probe.thread_dpi_context, window_dpi_context=probe.window_dpi_context,
                    window_dpi=probe.window_dpi, ?reasons, "rectangle shield acknowledgement failed");
            return Err(platform(format!(
                "Rectangle input shield monitor {}: {}",
                monitor + 1,
                reasons[0].description()
            )));
        }
        if bounds.is_empty() {
            return Err(platform("Incomplete rectangle input shield coverage"));
        }
        Ok(())
    }

    fn show_input_shields(
        &mut self,
        module: windows::Win32::Foundation::HMODULE,
        class: PCWSTR,
        operation_id: OperationId,
        bounds_list: &[ScreenRect],
    ) -> Result<(), VisualOverlayError> {
        for bounds in bounds_list.iter().copied() {
            let (width, height) = win32_dimensions(bounds, "input shield")?;
            let mut state = Box::new(WindowPaintState {
                bounds,
                frame: vec![],
                hint: None,
                solid: Some(COLORREF(0)),
                operation_id,
                description: "input shield",
                role: WindowRole::InputShield,
                paint_count: AtomicUsize::new(0),
                rectangle_input: self.rectangle_input.clone(),
            });
            let hwnd = unsafe {
                CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    class,
                    PCWSTR::null(),
                    WS_POPUP,
                    bounds.x,
                    bounds.y,
                    width,
                    height,
                    None,
                    None,
                    module,
                    Some(state.as_mut() as *mut _ as *const _),
                )
            }
            .map_err(|e| {
                platform(format!(
                    "input shield window creation failed for {bounds:?}: {e}"
                ))
            })?;
            // Store the state before any subsequent Win32 call can fail so
            // fail()/close() can reclaim every partially-created shield.
            self.input_shields.push(OverlayWindow {
                hwnd,
                state,
                created_at: Instant::now(),
            });
            unsafe { SetLayeredWindowAttributes(hwnd, COLORREF(0), 1, LWA_ALPHA) }.map_err(
                |e| {
                    platform(format!(
                        "input shield layered configuration failed for {bounds:?}: {e}"
                    ))
                },
            )?;
            unsafe {
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    bounds.x,
                    bounds.y,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )
            }
            .map_err(|e| {
                platform(format!(
                    "input shield positioning failed for {bounds:?}: {e}"
                ))
            })?;
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            if !unsafe { InvalidateRect(hwnd, None, false) }.as_bool()
                || !unsafe { UpdateWindow(hwnd) }.as_bool()
            {
                return Err(platform(format!(
                    "input shield initial paint failed for {bounds:?}: {}",
                    unsafe { GetLastError().0 }
                )));
            }
        }
        Ok(())
    }

    fn show_passive(
        &mut self,
        module: windows::Win32::Foundation::HMODULE,
        class: PCWSTR,
        operation_id: OperationId,
        visual: &OverlayVisual,
    ) -> Result<(), VisualOverlayError> {
        let physical = match displays() {
            Ok(v) => v,
            Err(e) => return self.fail(e.message),
        };
        let Some(plan) = passive_overlay_plan(visual, &physical) else {
            return self.fail("Rectangle picker cannot use the passive preview path");
        };
        for spec in plan {
            let (bounds, target, hint, solid, description, badge) = match spec {
                PassiveWindowSpec::Edge {
                    edge,
                    target,
                    rect,
                    style: PassiveWindowStyle::BrightYellow,
                } => (
                    rect,
                    target,
                    None,
                    Some(OUTLINE_COLOR),
                    match edge {
                        OutlineEdge::Top => "top",
                        OutlineEdge::Bottom => "bottom",
                        OutlineEdge::Left => "left",
                        OutlineEdge::Right => "right",
                    },
                    false,
                ),
                PassiveWindowSpec::Badge { monitor, index } => {
                    let text = index.to_string();
                    let width = 32u32.saturating_add(text.len() as u32 * 12);
                    let bounds = ScreenRect::new(
                        monitor.x.saturating_add(24),
                        monitor.y.saturating_add(24),
                        width,
                        42,
                    );
                    (bounds, monitor, Some(text), None, "monitor badge", true)
                }
            };
            let mut state = Box::new(WindowPaintState {
                bounds,
                frame: vec![],
                hint,
                solid,
                operation_id,
                description,
                role: WindowRole::Passive,
                paint_count: AtomicUsize::new(0),
                rectangle_input: None,
            });
            let width = match i32::try_from(bounds.width) { Ok(v) => v, Err(_) => return self.fail(format!("operation {operation_id} visual {visual:?} {description}: requested rectangle {bounds:?} width exceeds Win32 range")) };
            let height = match i32::try_from(bounds.height) { Ok(v) => v, Err(_) => return self.fail(format!("operation {operation_id} visual {visual:?} {description}: requested rectangle {bounds:?} height exceeds Win32 range")) };
            let hwnd = match unsafe {
                CreateWindowExW(
                    WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                    class,
                    PCWSTR::null(),
                    WS_POPUP,
                    bounds.x,
                    bounds.y,
                    width,
                    height,
                    None,
                    None,
                    module,
                    Some(state.as_mut() as *mut _ as *const _),
                )
            } {
                Ok(hwnd) => hwnd,
                Err(e) => return self.fail(format!("operation {operation_id} visual {visual:?} target {target:?} {description} badge={badge} requested {bounds:?}: CreateWindowExW failed: {e}; GetLastError={}", unsafe { GetLastError().0 })),
            };
            let window = OverlayWindow {
                hwnd,
                state,
                created_at: Instant::now(),
            };
            tracing::debug!(operation_id, visual=?visual, target=?target, requested=?bounds, ?hwnd, edge=description, badge, "created passive overlay window");
            if badge {
                self.passive_badges.push(window);
            } else {
                self.passive_edges.push(window);
            }
            if let Err(e) = unsafe {
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    bounds.x,
                    bounds.y,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )
            } {
                let what = if badge {
                    "monitor-identification badge"
                } else {
                    "passive preview edge"
                };
                return self.fail(format!("operation {operation_id} visual {visual:?} target {target:?} {description} badge={badge} HWND={hwnd:?} requested={bounds:?}: SetWindowPos failed ({what}): {e}; GetLastError={}", unsafe { GetLastError().0 }));
            }
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            if !unsafe { InvalidateRect(hwnd, None, false) }.as_bool() {
                return self.fail(format!("operation {operation_id} visual {visual:?} {description} HWND={hwnd:?} requested={bounds:?}: InvalidateRect failed; GetLastError={}", unsafe { GetLastError().0 }));
            }
            if !unsafe { UpdateWindow(hwnd) }.as_bool()
                && !unsafe { RedrawWindow(hwnd, None, None, RDW_INVALIDATE | RDW_UPDATENOW) }
                    .as_bool()
            {
                return self.fail(format!("operation {operation_id} visual {visual:?} {description} HWND={hwnd:?} requested={bounds:?}: synchronous UpdateWindow/RedrawWindow failed; GetLastError={}", unsafe { GetLastError().0 }));
            }
            if !unsafe { IsWindow(hwnd) }.as_bool() {
                return self.fail(format!("operation {operation_id} visual {visual:?} {description} badge={badge} HWND={hwnd:?} requested={bounds:?}: IsWindow invariant failed; GetLastError={}", unsafe { GetLastError().0 }));
            }
            if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
                return self.fail(format!("operation {operation_id} visual {visual:?} {description} badge={badge} HWND={hwnd:?} requested={bounds:?}: IsWindowVisible invariant failed; GetLastError={}", unsafe { GetLastError().0 }));
            }
            let mut actual = RECT::default();
            if unsafe { GetWindowRect(hwnd, &mut actual) }.is_err() {
                return self.fail(format!("operation {operation_id} visual {visual:?} {description} badge={badge} HWND={hwnd:?} requested={bounds:?}: GetWindowRect failed; GetLastError={}", unsafe { GetLastError().0 }));
            }
            let requested = (
                i64::from(bounds.x),
                i64::from(bounds.y),
                i64::from(bounds.x) + i64::from(bounds.width),
                i64::from(bounds.y) + i64::from(bounds.height),
            );
            let got = (
                i64::from(actual.left),
                i64::from(actual.top),
                i64::from(actual.right),
                i64::from(actual.bottom),
            );
            if requested != got {
                return self.fail(format!("operation {operation_id} visual {visual:?} target={target:?} {description} badge={badge} HWND={hwnd:?}: GetWindowRect mismatch; requested={requested:?}, actual={got:?}"));
            }
            tracing::debug!(operation_id, visual=?visual, target=?target, requested=?bounds, actual=?got, ?hwnd, edge=description, badge, "verified passive overlay window visible");
        }
        Ok(())
    }
}

impl OverlayRenderer for NativeOverlayRenderer {
    fn left_button_down(&mut self) -> Result<bool, VisualOverlayError> {
        Ok(unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0)
    }
    fn cursor_position(&mut self) -> Result<MkPoint, VisualOverlayError> {
        let mut point = POINT::default();
        unsafe { GetCursorPos(&mut point) }
            .map_err(|e| platform(format!("cursor query failed: {e}")))?;
        Ok(MkPoint {
            x: point.x,
            y: point.y,
        })
    }
    fn show(
        &mut self,
        operation_id: OperationId,
        visual: &OverlayVisual,
        _mouse_transparent: bool,
    ) -> Result<(), VisualOverlayError> {
        tracing::debug!(operation_id, visual = ?visual, "showing native visual overlay; replacing current operation");
        self.close();
        if self.closing {
            return Err(platform(
                "Previous rectangle transaction is still awaiting release",
            ));
        }
        if matches!(visual, OverlayVisual::RectanglePicker { .. }) {
            self.escape_down = unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) } < 0;
            self.return_down = unsafe { GetAsyncKeyState(VK_RETURN.0 as i32) } < 0;
        } else if matches!(visual, OverlayVisual::PointPicker { .. }) {
            // Treat the launch click as held until polling observes an actual
            // up state. This also synthesizes the arming release when command
            // dispatch happens just after the GUI button was released.
            self.left_down = true;
            self.escape_down = unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) } < 0;
            self.return_down = unsafe { GetAsyncKeyState(VK_RETURN.0 as i32) } < 0;
        }
        let module = unsafe { GetModuleHandleW(None) }
            .map_err(|e| platform(format!("overlay module lookup failed: {e}")))?;
        let class = windows::core::w!("MultiLauncherVisualOverlay");
        let wc = WNDCLASSW {
            hInstance: module.into(),
            lpszClassName: class,
            lpfnWndProc: Some(wndproc),
            style: CS_HREDRAW | CS_VREDRAW,
            ..Default::default()
        };
        if unsafe { RegisterClassW(&wc) } == 0
            && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS
        {
            return self.fail(format!("overlay class registration failed: {}", unsafe {
                GetLastError().0
            }));
        }
        if visual.passive() && !matches!(visual, OverlayVisual::OcrDebug(_)) {
            self.operation_id = Some(operation_id);
            self.visual = Some(visual.clone());
            self.show_passive(module, class, operation_id, visual)?;
            return Ok(());
        }
        let physical = match visual {
            OverlayVisual::PointPicker { .. } | OverlayVisual::Desktop(_) => vec![],
            _ => match displays() {
                Ok(displays) => displays,
                Err(error) => return self.fail(error.message),
            },
        };
        let monitor_bounds = match visual {
            OverlayVisual::PointPicker { .. } => vec![],
            // Preserve the descriptor topology: never create a virtual-desktop union window
            // spanning gaps between physical displays.
            OverlayVisual::Desktop(descriptors) => descriptors.iter().map(|d| d.bounds).collect(),
            OverlayVisual::OcrDebug(plan) => ocr_debug_surface_plan(plan, &physical),
            _ => {
                let Some(target) = Self::target(visual) else {
                    return self
                        .fail("No physical display intersects the requested preview region");
                };
                intersecting_monitor_bounds(&physical, target)
            }
        };
        if monitor_bounds.is_empty() && !matches!(visual, OverlayVisual::PointPicker { .. }) {
            return self.fail("No physical display intersects the requested preview region");
        }
        let input_shield_bounds = input_shield_plan(visual, &physical);
        if overlay_requires_input_shields(visual) {
            self.rectangle_input = Some(Arc::new(Mutex::new(RectangleNativeInput::default())));
            let shield_class = windows::core::w!("MultiLauncherVisualOverlayInputShield");
            let shield_wc = WNDCLASSW {
                hInstance: module.into(),
                lpszClassName: shield_class,
                lpfnWndProc: Some(input_shield_wndproc),
                style: CS_HREDRAW | CS_VREDRAW,
                ..Default::default()
            };
            if unsafe { RegisterClassW(&shield_wc) } == 0
                && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS
            {
                return self.fail(format!(
                    "input shield class registration failed: {}",
                    unsafe { GetLastError().0 }
                ));
            }
            if let Err(error) =
                self.show_input_shields(module, shield_class, operation_id, &input_shield_bounds)
            {
                return self.fail(error.message);
            }
        }

        let transparent = overlay_is_mouse_transparent(visual);
        let frame = overlay_frame(visual);
        for bounds in monitor_bounds {
            let (width, height) = match win32_dimensions(bounds, "interactive overlay") {
                Ok(size) => size,
                Err(error) => return self.fail(error.message),
            };
            let mut state = Box::new(WindowPaintState {
                bounds,
                frame: frame.clone(),
                hint: None,
                solid: None,
                operation_id,
                description: "interactive",
                role: full_surface_role(visual),
                paint_count: AtomicUsize::new(0),
                rectangle_input: None,
            });
            let mut ex = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
            if transparent {
                ex |= WS_EX_TRANSPARENT;
            }
            let created = unsafe {
                CreateWindowExW(
                    ex,
                    class,
                    PCWSTR::null(),
                    WS_POPUP,
                    bounds.x,
                    bounds.y,
                    width,
                    height,
                    None,
                    None,
                    module,
                    Some(state.as_mut() as *mut _ as *const _),
                )
            };
            let hwnd = match created {
                Ok(hwnd) => hwnd,
                Err(e) => return self.fail(format!("overlay window creation failed: {e}")),
            };
            self.windows.push(OverlayWindow {
                hwnd,
                state,
                created_at: Instant::now(),
            });
            if let Err(e) =
                unsafe { SetLayeredWindowAttributes(hwnd, TRANSPARENT_KEY, 0, LWA_COLORKEY) }
            {
                return self.fail(format!("overlay layered configuration failed: {e}"));
            }
            if let Err(e) = unsafe {
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    bounds.x,
                    bounds.y,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )
            } {
                return self.fail(format!("overlay positioning failed: {e}"));
            }
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            if !unsafe { InvalidateRect(hwnd, None, false) }.as_bool()
                || !unsafe { UpdateWindow(hwnd) }.as_bool()
            {
                return self.fail(format!("overlay initial paint failed: {}", unsafe {
                    GetLastError().0
                }));
            }
        }
        let tooltip_spec = match visual {
            OverlayVisual::RectanglePicker { tooltip, .. } => {
                Some((tooltip.pointer, tooltip.text.clone(), (330, 76)))
            }
            OverlayVisual::PointPicker { pointer } => {
                Some((*pointer, POINT_INSTRUCTION.to_owned(), (300, 42)))
            }
            _ => None,
        };
        if let Some((tooltip_pointer, tooltip_text, size)) = tooltip_spec {
            let all = match displays() {
                Ok(displays) => displays,
                Err(error) => return self.fail(error.message),
            };
            let monitor = match monitor_nearest_pointer(&all, tooltip_pointer) {
                Some(monitor) => monitor,
                None => return self.fail("No display is available for the selection tooltip"),
            };
            let at =
                place_rectangle_tooltip(tooltip_pointer, size, monitor, RECTANGLE_TOOLTIP_OFFSET);
            let bounds = ScreenRect::new(at.x, at.y, size.0, size.1);
            let mut state = Box::new(WindowPaintState {
                bounds,
                frame: vec![],
                hint: Some(tooltip_text),
                solid: None,
                operation_id,
                description: "tooltip",
                role: WindowRole::Tooltip,
                paint_count: AtomicUsize::new(0),
                rectangle_input: None,
            });
            let hwnd = unsafe {
                CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                    class,
                    PCWSTR::null(),
                    WS_POPUP,
                    at.x,
                    at.y,
                    size.0 as i32,
                    size.1 as i32,
                    None,
                    None,
                    module,
                    Some(state.as_mut() as *mut _ as *const _),
                )
            }
            .map_err(|e| platform(format!("tooltip window creation failed: {e}")))
            .inspect_err(|_| self.close())?;
            self.tooltip = Some(OverlayWindow {
                hwnd,
                state,
                created_at: Instant::now(),
            });
            unsafe { SetLayeredWindowAttributes(hwnd, TRANSPARENT_KEY, 0, LWA_COLORKEY) }
                .map_err(|e| platform(format!("tooltip layered configuration failed: {e}")))
                .inspect_err(|_| self.close())?;
            unsafe {
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    at.x,
                    at.y,
                    size.0 as i32,
                    size.1 as i32,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )
            }
            .map_err(|e| platform(format!("tooltip positioning failed: {e}")))
            .inspect_err(|_| self.close())?;
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                let _ = InvalidateRect(hwnd, None, false);
                let _ = UpdateWindow(hwnd);
            }
        }
        if let Some(input) = &self.rectangle_input {
            if input_shield_bounds.is_empty()
                || self.input_shields.len() != input_shield_bounds.len()
            {
                return self.fail("Incomplete rectangle input shield coverage");
            }
            if let Err(error) = self.verify_shield_coverage(&input_shield_bounds, "startup") {
                return self.fail(error.message);
            }
            for window in &self.input_shields {
                // Discard pre-acknowledgement mouse transactions. No selection is
                // armed until every monitor is covered and this baseline exists.
                let mut msg = MSG::default();
                while unsafe {
                    PeekMessageW(&mut msg, window.hwnd, WM_MOUSEMOVE, WM_LBUTTONUP, PM_REMOVE)
                }
                .as_bool()
                {
                    unsafe {
                        DispatchMessageW(&msg);
                    }
                }
            }
            let left = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0;
            input
                .lock()
                .map_err(|_| platform("Rectangle input state lock poisoned"))?
                .acknowledge(left);
        }
        self.operation_id = Some(operation_id);
        self.visual = Some(visual.clone());
        Ok(())
    }

    fn rectangle_startup_left_down(&mut self) -> Result<bool, VisualOverlayError> {
        let input = self
            .rectangle_input
            .as_ref()
            .ok_or_else(|| platform("Rectangle input coverage is not installed"))?;
        let input = input
            .lock()
            .map_err(|_| platform("Rectangle input state lock poisoned"))?;
        if !input.ready {
            return Err(platform("Rectangle input coverage is not acknowledged"));
        }
        Ok(input.startup_left_down)
    }

    fn repaint(
        &mut self,
        operation_id: OperationId,
        visual: &OverlayVisual,
    ) -> Result<(), VisualOverlayError> {
        if self.operation_id != Some(operation_id) {
            return Ok(());
        }
        let frame = overlay_frame(visual);
        self.visual = Some(visual.clone());
        for window in &mut self.windows {
            window.state.frame.clone_from(&frame);
            if !unsafe { InvalidateRect(window.hwnd, None, false) }.as_bool()
                || !unsafe { UpdateWindow(window.hwnd) }.as_bool()
            {
                return Err(platform(format!("overlay repaint failed: {}", unsafe {
                    GetLastError().0
                })));
            }
        }
        if let Some(window) = &mut self.tooltip {
            let (pointer, text) = match visual {
                OverlayVisual::RectanglePicker { tooltip, .. } => {
                    (tooltip.pointer, tooltip.text.clone())
                }
                OverlayVisual::PointPicker { pointer } => (*pointer, POINT_INSTRUCTION.to_owned()),
                _ => return Ok(()),
            };
            window.state.hint = Some(text);
            let all = displays()?;
            if let Some(monitor) = monitor_nearest_pointer(&all, pointer) {
                let at = place_rectangle_tooltip(
                    pointer,
                    (window.state.bounds.width, window.state.bounds.height),
                    monitor,
                    RECTANGLE_TOOLTIP_OFFSET,
                );
                window.state.bounds.x = at.x;
                window.state.bounds.y = at.y;
                unsafe {
                    SetWindowPos(
                        window.hwnd,
                        HWND_TOPMOST,
                        at.x,
                        at.y,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOSIZE,
                    )
                }
                .map_err(|e| platform(format!("tooltip move failed: {e}")))?;
                unsafe {
                    let _ = InvalidateRect(window.hwnd, None, false);
                    let _ = UpdateWindow(window.hwnd);
                }
            }
        }
        Ok(())
    }

    fn poll_input(&mut self) -> Result<Vec<OverlayInput>, VisualOverlayError> {
        self.pump_messages();
        let mut interruption = None;
        if let Some(input) = self.rectangle_input.clone() {
            let (capture_lost, topology_changed, owned_down) = {
                let input = input
                    .lock()
                    .map_err(|_| platform("Rectangle input state lock poisoned"))?;
                (
                    input.capture_lost,
                    input.topology_changed,
                    input.owned_left_down,
                )
            };
            if topology_changed {
                input
                    .lock()
                    .map_err(|_| platform("Rectangle input state lock poisoned"))?
                    .topology_changed = false;
                interruption = Some(self.refresh_shield_coverage().err().unwrap_or_else(|| {
                    platform("Rectangle selection canceled because monitor topology changed")
                }));
            }
            if capture_lost && owned_down && !self.capture_recovery_attempted {
                // One attempt to retain native capture; shields still own
                // hit-testing when Windows limits background capture.
                if !self.capture_recovery_attempted {
                    self.capture_recovery_attempted = true;
                    if let Some(window) = self.input_shields.first() {
                        unsafe {
                            SetCapture(window.hwnd);
                        }
                    }
                }
                interruption = Some(platform(
                    "Rectangle selection canceled because mouse capture was lost",
                ));
            }
        }
        if self.closing {
            let left = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0;
            let waiting = self.rectangle_input.as_ref().is_some_and(|input| {
                input
                    .lock()
                    .is_ok_and(|mut input| input.wait_for_close_release(left))
            });
            if !waiting {
                self.destroy_windows();
            }
            return interruption.map_or(Ok(vec![]), Err);
        }
        if let Some(error) = interruption {
            return Err(error);
        }
        // Passive windows have no input contract; in particular, do not let a
        // click intended for the underlying application affect controller state.
        if self.visual.as_ref().is_none_or(OverlayVisual::passive) {
            return Ok(vec![]);
        }
        let Some(id) = self.operation_id else {
            return Ok(vec![]);
        };
        let mut out = vec![];
        let left = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0;
        let esc = unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) } < 0;
        let enter = unsafe { GetAsyncKeyState(VK_RETURN.0 as i32) } < 0;
        let mut p = POINT::default();
        unsafe { GetCursorPos(&mut p) }
            .map_err(|e| platform(format!("cursor query failed: {e}")))?;
        if let Some(input) = &self.rectangle_input {
            let mut input = input
                .lock()
                .map_err(|_| platform("Rectangle input state lock poisoned"))?;
            // A launch press can belong to another HWND's pre-existing capture.
            // Its release only arms the picker; subsequent edges come exclusively
            // from shield messages, so a rapid Down+Up cannot disappear.
            if input.startup_left_down && input.left_down && !left {
                input.record(WM_LBUTTONUP, MkPoint { x: p.x, y: p.y });
            }
            if !input.left_down {
                input.startup_left_down = false;
            }
            input.release_boundary = false;
            out.extend(
                std::mem::take(&mut input.events)
                    .into_iter()
                    .map(|kind| OverlayInput {
                        operation_id: id,
                        kind,
                    }),
            );
            out.push(OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::PointerMoved(MkPoint { x: p.x, y: p.y }),
            });
        } else {
            if left && !self.left_down {
                out.push(OverlayInput {
                    operation_id: id,
                    kind: OverlayInputKind::LeftPressed(MkPoint { x: p.x, y: p.y }),
                });
            }
            out.push(OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::PointerMoved(MkPoint { x: p.x, y: p.y }),
            });
            if !left && self.left_down {
                out.push(OverlayInput {
                    operation_id: id,
                    kind: OverlayInputKind::LeftReleased(MkPoint { x: p.x, y: p.y }),
                });
            }
        }
        if esc && !self.escape_down {
            out.push(OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::Escape,
            });
        }
        if key_pressed(enter, self.return_down) {
            out.push(OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::Enter,
            });
        }
        self.left_down = left;
        self.escape_down = esc;
        self.return_down = enter;
        Ok(out)
    }

    fn close(&mut self) {
        if self.closing {
            return;
        }
        // Cancel may arrive before the next tick has witnessed a queued Down.
        self.pump_messages();
        let waiting = self
            .rectangle_input
            .as_ref()
            .is_some_and(|input| input.lock().is_ok_and(|input| input.awaiting_release()));
        if waiting {
            self.closing = true;
        } else {
            self.destroy_windows();
        }
    }
    fn close_pending(&self) -> bool {
        self.closing
    }
    fn close_after_release(&mut self) {
        self.destroy_windows();
    }
}
fn destroy_overlay_window(window: OverlayWindow) {
    let hwnd = window.hwnd;
    unsafe {
        if IsWindow(hwnd).as_bool() {
            // Clear the borrowed state pointer before DestroyWindow drops the
            // native object; invalid/already-destroyed handles are ignored.
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            let _ = DestroyWindow(hwnd);
        }
    }
}

impl Drop for NativeOverlayRenderer {
    fn drop(&mut self) {
        // Normal service shutdown drains release first. Unwinding still must
        // reclaim HWNDs before dropping their borrowed paint-state boxes.
        self.destroy_windows();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_shield_native_events_preserve_rapid_transaction_and_signed_positions() {
        let mut input = RectangleNativeInput::default();
        let start = MkPoint { x: -1920, y: -300 };
        let end = MkPoint { x: 800, y: 600 };
        input.record(WM_LBUTTONDOWN, start);
        assert!(
            input.events.is_empty(),
            "pre-acknowledgement input never arms selection"
        );
        input.acknowledge(false);
        input.record(WM_LBUTTONDOWN, start);
        input.record(WM_MOUSEMOVE, end);
        input.record(WM_LBUTTONUP, end);
        assert_eq!(
            input.events,
            vec![
                OverlayInputKind::LeftPressed(start),
                OverlayInputKind::PointerMoved(end),
                OverlayInputKind::LeftReleased(end)
            ]
        );
        assert!(!input.left_down);
        input.record(WM_LBUTTONUP, end);
        assert_eq!(
            input.events.len(),
            3,
            "no duplicate release from another monitor"
        );
    }

    #[test]
    fn rectangle_shield_startup_held_release_only_arms_next_press() {
        let mut input = RectangleNativeInput::default();
        let point = MkPoint { x: -40, y: 20 };
        input.acknowledge(true);
        input.record(WM_LBUTTONUP, point);
        input.record(WM_LBUTTONDOWN, point);
        input.record(WM_LBUTTONUP, point);
        assert_eq!(
            input.events,
            vec![
                OverlayInputKind::LeftReleased(point),
                OverlayInputKind::LeftPressed(point),
                OverlayInputKind::LeftReleased(point)
            ]
        );
    }

    #[test]
    fn rectangle_shield_fresh_down_after_foreign_launch_release_is_not_lost() {
        let mut input = RectangleNativeInput::default();
        let point = MkPoint { x: -40, y: 20 };
        input.acknowledge(true);
        // The foreign owner received launch Up before this fresh shield Down.
        input.record(WM_LBUTTONDOWN, point);
        input.record(WM_LBUTTONUP, point);
        assert_eq!(
            input.events,
            vec![
                OverlayInputKind::LeftReleased(point),
                OverlayInputKind::LeftPressed(point),
                OverlayInputKind::LeftReleased(point)
            ]
        );
        assert!(!input.awaiting_release());
        assert!(!input.startup_left_down);
        assert!(!input.owned_left_down);
    }

    #[test]
    fn rectangle_shield_partial_install_release_is_a_boundary_without_semantic_events() {
        let mut input = RectangleNativeInput::default();
        let point = MkPoint { x: -20, y: 10 };
        input.record(WM_LBUTTONDOWN, point);
        assert!(input.awaiting_release());
        assert!(input.events.is_empty());
        input.record(WM_LBUTTONUP, point);
        assert!(
            input.saw_release(),
            "native pump must stop before a later unrelated Down"
        );
        assert!(
            input.events.is_empty(),
            "unacknowledged picker never publishes selection"
        );
        assert!(!input.wait_for_close_release(false));
    }

    #[test]
    fn rectangle_shield_interruption_retains_owned_transaction_until_release() {
        let point = MkPoint { x: 10, y: 20 };
        for signal in [WM_CAPTURECHANGED, WM_DISPLAYCHANGE] {
            let mut input = RectangleNativeInput::default();
            input.acknowledge(false);
            input.record(WM_LBUTTONDOWN, point);
            input.record_system_message(signal);
            assert!(input.wait_for_close_release(true));
            assert_eq!(input.capture_lost, signal == WM_CAPTURECHANGED);
            assert_eq!(input.topology_changed, signal == WM_DISPLAYCHANGE);
            if signal == WM_DISPLAYCHANGE {
                assert!(
                    input.wait_for_close_release(false),
                    "intact owned capture needs its native Up"
                );
                input.record(WM_LBUTTONUP, point);
            }
            assert!(!input.wait_for_close_release(false));
            input.record_system_message(WM_CAPTURECHANGED);
            assert!(!input.awaiting_release());
        }
    }

    #[test]
    fn rectangle_shield_acknowledgement_rejects_missing_invisible_or_transparent_coverage() {
        let bounds = ScreenRect::new(-1920, -200, 1920, 1080);
        let ex = (WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOPMOST).0;
        let mut probe = ShieldProbe {
            actual: Some(bounds),
            rect_error: None,
            visible: true,
            ex,
            style_error: None,
            thread_dpi_context: 0,
            window_dpi_context: 0,
            window_dpi: 96,
        };
        assert!(probe.failures(bounds).is_empty());
        probe.visible = false;
        assert_eq!(probe.failures(bounds), [ShieldFailure::WindowNotVisible]);
        probe.visible = true;
        probe.actual = Some(ScreenRect::new(0, 0, 1920, 1080));
        assert_eq!(probe.failures(bounds), [ShieldFailure::RectMismatch]);
        probe.actual = None;
        probe.rect_error = Some(5);
        assert_eq!(
            probe.failures(bounds),
            [ShieldFailure::WindowRectQueryFailed]
        );
        probe.actual = Some(bounds);
        probe.rect_error = None;
        probe.style_error = Some(5);
        assert_eq!(probe.failures(bounds), [ShieldFailure::StyleQueryFailed]);
        probe.style_error = None;
        for (flag, reason) in [
            (WS_EX_LAYERED.0, ShieldFailure::LayeredMissing),
            (WS_EX_NOACTIVATE.0, ShieldFailure::NoactivateMissing),
            (WS_EX_TOPMOST.0, ShieldFailure::TopmostMissing),
        ] {
            probe.ex = ex & !flag;
            assert_eq!(probe.failures(bounds), [reason]);
        }
        probe.ex = ex | WS_EX_TRANSPARENT.0;
        assert_eq!(
            probe.failures(bounds),
            [ShieldFailure::UnexpectedTransparent]
        );
        let renderer = NativeOverlayRenderer::default();
        assert!(
            renderer
                .verify_shield_coverage(&[bounds], "test")
                .unwrap_err()
                .message
                .contains("missing shield")
        );
        assert_eq!(
            full_surface_role(&OverlayVisual::RectanglePicker {
                virtual_desktop: bounds,
                selection: None,
                tooltip: RectangleTooltip {
                    text: String::new(),
                    pointer: MkPoint { x: 0, y: 0 }
                }
            }),
            WindowRole::TransparentOverlay
        );
    }

    #[test]
    fn ocr_full_surface_role_is_nonactivating_and_hit_test_transparent() {
        let visual = OverlayVisual::OcrDebug(OcrDebugOverlayPlan {
            region: ScreenRect::new(-100, -50, 200, 100),
            rects: vec![],
            truncated_rects: 0,
        });
        let role = full_surface_role(&visual);
        assert_eq!(role, WindowRole::TransparentOverlay);
        assert!(role_is_hit_test_transparent(role));
        assert!(!role_is_hit_test_transparent(WindowRole::VisibleOverlay));
    }

    #[test]
    fn return_key_held_down_emits_only_one_pressed_transition() {
        let states = [true, true, false, true];
        let mut previous = false;
        let transitions = states
            .into_iter()
            .filter(|&current| {
                let pressed = key_pressed(current, previous);
                previous = current;
                pressed
            })
            .count();
        assert_eq!(transitions, 2);
        assert!(key_pressed(true, false));
        assert!(!key_pressed(true, true));
    }
}

/// Manual diagnostic entry point. This deliberately uses the same renderer,
/// visual, message pump, and teardown path as production without constructing
/// any Action Editor state.
pub(super) fn run_passive_overlay_smoke_test() -> Result<(), VisualOverlayError> {
    const OPERATION_ID: OperationId = 1;
    let visual = OverlayVisual::RectanglePreview(ScreenRect::new(100, 100, 500, 300));
    let mut renderer = NativeOverlayRenderer::default();

    eprintln!("passive-overlay-smoke: launch: planning passive rectangle");
    let physical = displays().map_err(|error| {
        eprintln!("passive-overlay-smoke: planning failed: {error}");
        error
    })?;
    let planned = passive_overlay_plan(&visual, &physical).ok_or_else(|| {
        let error = platform("passive rectangle unexpectedly has no passive window plan");
        eprintln!("passive-overlay-smoke: planning failed: {error}");
        error
    })?;
    if planned.len() != 4 {
        let error = platform(format!(
            "expected four native edge windows, planned {}",
            planned.len()
        ));
        eprintln!("passive-overlay-smoke: planning failed: {error}");
        return Err(error);
    }
    eprintln!("passive-overlay-smoke: launch: plan ready; creating four native windows");
    renderer
        .show(OPERATION_ID, &visual, true)
        .map_err(|error| {
            eprintln!("passive-overlay-smoke: creation failed: {error}");
            error
        })?;
    eprintln!(
        "passive-overlay-smoke: active: four bright-yellow edges; pumping messages for ~2.5s"
    );

    let deadline = Instant::now() + PASSIVE_OVERLAY_DURATION;
    while Instant::now() < deadline {
        if let Err(error) = renderer.poll_input() {
            eprintln!("passive-overlay-smoke: message pump failed: {error}");
            renderer.close();
            eprintln!("passive-overlay-smoke: cleanup: overlay windows dismissed after failure");
            return Err(error);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    renderer.close();
    eprintln!("passive-overlay-smoke: cleanup: all overlay windows dismissed");
    Ok(())
}

#[path = "visual_overlay_rectangle_smoke.rs"]
mod rectangle_input_smoke;

pub(super) fn run_rectangle_input_isolation_smoke_test() -> Result<(), VisualOverlayError> {
    rectangle_input_smoke::run()
}

/// No cursor queries or input injection; exercise the production readiness gate.
pub(super) fn run_rectangle_shield_probe() -> Result<(), VisualOverlayError> {
    let bounds = displays()?;
    let desktop = bounds
        .first()
        .copied()
        .ok_or_else(|| platform("No physical monitor"))?;
    let visual = OverlayVisual::RectanglePicker {
        virtual_desktop: desktop,
        selection: None,
        tooltip: RectangleTooltip {
            text: String::new(),
            pointer: MkPoint {
                x: desktop.x,
                y: desktop.y,
            },
        },
    };
    let mut renderer = NativeOverlayRenderer::default();
    let result = renderer.show(1, &visual, false);
    for (monitor, window) in renderer.input_shields.iter().enumerate() {
        eprintln!(
            "shield-probe: monitor={} expected={:?} probe={:?}",
            monitor + 1,
            window.state.bounds,
            probe_shield(window.hwnd)
        );
    }
    renderer.close();
    result?;
    eprintln!("shield-probe: startup acknowledged; cleanup complete");
    Ok(())
}
