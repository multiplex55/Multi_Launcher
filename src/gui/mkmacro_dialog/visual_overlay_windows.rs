//! Win32 color-key overlay renderer.  There is deliberately one popup per
//! physical monitor: a single virtual-desktop popup would cover monitor gaps.
use super::*;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
use windows::{
    Win32::{
        Foundation::{
            COLORREF, ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, POINT, RECT,
            WPARAM,
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
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON},
            WindowsAndMessaging::{
                CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW,
                DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetCursorPos, GetWindowLongPtrW,
                GetWindowRect, HTTRANSPARENT, HWND_TOPMOST, IsWindow, IsWindowVisible, LWA_ALPHA,
                LWA_COLORKEY, MA_NOACTIVATE, MSG, PM_REMOVE, PeekMessageW, RegisterClassW,
                SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE, SWP_SHOWWINDOW,
                SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow,
                TranslateMessage, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE,
                WM_MOUSEMOVE, WM_NCCREATE, WM_NCHITTEST, WM_PAINT, WNDCLASSW, WS_EX_LAYERED,
                WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowRole {
    VisibleOverlay,
    Passive,
    Tooltip,
    InputShield,
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
}
struct OverlayWindow {
    hwnd: HWND,
    // The pointee address stays stable while wndproc reads it through GWLP_USERDATA.
    state: Box<WindowPaintState>,
    created_at: Instant,
}

pub(super) struct NativeOverlayRenderer {
    // Full-monitor, layered picker surfaces only.
    windows: Vec<OverlayWindow>,
    // Non-transparent, non-activating input consumers used only by rectangle picking.
    input_shields: Vec<OverlayWindow>,
    // Small, opaque, non-activating passive resources.
    passive_edges: Vec<OverlayWindow>,
    passive_badges: Vec<OverlayWindow>,
    tooltip: Option<OverlayWindow>,
    operation_id: Option<OperationId>,
    visual: Option<OverlayVisual>,
    left_down: bool,
    escape_down: bool,
}
unsafe impl Send for NativeOverlayRenderer {}
impl Default for NativeOverlayRenderer {
    fn default() -> Self {
        Self {
            windows: vec![],
            input_shields: vec![],
            passive_edges: vec![],
            passive_badges: vec![],
            tooltip: None,
            operation_id: None,
            visual: None,
            left_down: false,
            escape_down: false,
        }
    }
}

fn platform(message: impl Into<String>) -> VisualOverlayError {
    VisualOverlayError {
        kind: OverlayErrorKind::Platform,
        message: message.into(),
    }
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
            if !state.is_null()
                && unsafe {
                    matches!((*state).role, WindowRole::Passive | WindowRole::Tooltip)
                } =>
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
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_LBUTTONDOWN | WM_LBUTTONUP | WM_MOUSEMOVE => LRESULT(0),
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
        }
    }
    fn fail<T>(&mut self, message: impl Into<String>) -> Result<T, VisualOverlayError> {
        let error = platform(message);
        self.close();
        Err(error)
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
        if matches!(visual, OverlayVisual::RectanglePicker { .. }) {
            self.left_down = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0;
            self.escape_down = unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) } < 0;
        } else if matches!(visual, OverlayVisual::PointPicker { .. }) {
            // Treat the launch click as held until polling observes an actual
            // up state. This also synthesizes the arming release when command
            // dispatch happens just after the GUI button was released.
            self.left_down = true;
            self.escape_down = unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) } < 0;
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
        if visual.passive() {
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
                role: WindowRole::VisibleOverlay,
                paint_count: AtomicUsize::new(0),
            });
            let mut ex = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
            if overlay_is_mouse_transparent(visual) {
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
            .map_err(|error| {
                self.close();
                error
            })?;
            self.tooltip = Some(OverlayWindow {
                hwnd,
                state,
                created_at: Instant::now(),
            });
            unsafe { SetLayeredWindowAttributes(hwnd, TRANSPARENT_KEY, 0, LWA_COLORKEY) }
                .map_err(|e| platform(format!("tooltip layered configuration failed: {e}")))
                .map_err(|error| {
                    self.close();
                    error
                })?;
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
            .map_err(|error| {
                self.close();
                error
            })?;
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                let _ = InvalidateRect(hwnd, None, false);
                let _ = UpdateWindow(hwnd);
            }
        }
        self.operation_id = Some(operation_id);
        self.visual = Some(visual.clone());
        Ok(())
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
        let mut msg = MSG::default();
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
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
        let mut p = POINT::default();
        unsafe { GetCursorPos(&mut p) }
            .map_err(|e| platform(format!("cursor query failed: {e}")))?;
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
        if esc && !self.escape_down {
            out.push(OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::Escape,
            });
        }
        self.left_down = left;
        self.escape_down = esc;
        Ok(out)
    }

    fn close(&mut self) {
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
        self.operation_id = None;
        self.visual = None;
        self.left_down = false;
        self.escape_down = false;
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
        self.close();
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
