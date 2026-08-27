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
            MONITORINFO, NULL_BRUSH, PAINTSTRUCT, PS_SOLID, Rectangle, SelectObject, SetBkMode,
            SetTextColor, TRANSPARENT, TextOutW, UpdateWindow,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON},
            WindowsAndMessaging::{
                CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW,
                DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetCursorPos, GetWindowLongPtrW,
                GetWindowRect, HWND_TOPMOST, IsWindow, IsWindowVisible, LWA_COLORKEY, MSG,
                PM_REMOVE, PeekMessageW, RDW_INVALIDATE, RDW_UPDATENOW, RedrawWindow,
                RegisterClassW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE, SWP_SHOWWINDOW,
                SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow,
                TranslateMessage, WM_ERASEBKGND, WM_NCCREATE, WM_PAINT, WNDCLASSW, WS_EX_LAYERED,
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

struct WindowPaintState {
    bounds: ScreenRect,
    frame: Vec<OverlayFramePrimitive>,
    hint: Option<String>,
    solid: Option<COLORREF>,
    operation_id: OperationId,
    description: &'static str,
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

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = unsafe { &*(l.0 as *const CREATESTRUCTW) };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize) };
    }
    let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowPaintState };
    match msg {
        WM_ERASEBKGND => LRESULT(1), // WM_PAINT owns clearing the complete surface.
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
                PassiveWindowSpec::Edge { edge, target, rect } => (
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
                paint_count: AtomicUsize::new(0),
            });
            let width = match i32::try_from(bounds.width) { Ok(v) => v, Err(_) => return self.fail(format!("operation {operation_id} visual {visual:?} {description}: requested rectangle {bounds:?} width exceeds Win32 range")) };
            let height = match i32::try_from(bounds.height) { Ok(v) => v, Err(_) => return self.fail(format!("operation {operation_id} visual {visual:?} {description}: requested rectangle {bounds:?} height exceeds Win32 range")) };
            let hwnd = match unsafe {
                CreateWindowExW(
                    WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
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
        let monitor_bounds = match visual {
            // Preserve the descriptor topology: never create a virtual-desktop union window
            // spanning gaps between physical displays.
            OverlayVisual::Desktop(descriptors) => descriptors.iter().map(|d| d.bounds).collect(),
            _ => {
                let Some(target) = Self::target(visual) else {
                    return self
                        .fail("No physical display intersects the requested preview region");
                };
                let physical = match displays() {
                    Ok(v) => v,
                    Err(e) => return self.fail(e.message),
                };
                intersecting_monitor_bounds(&physical, target)
            }
        };
        if monitor_bounds.is_empty() {
            return self.fail("No physical display intersects the requested preview region");
        }
        let frame = overlay_frame(visual);
        for bounds in monitor_bounds {
            let mut state = Box::new(WindowPaintState {
                bounds,
                frame: frame.clone(),
                hint: None,
                solid: None,
                operation_id,
                description: "interactive",
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
                    bounds.width as i32,
                    bounds.height as i32,
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
                    bounds.width as i32,
                    bounds.height as i32,
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
        if let OverlayVisual::RectanglePicker { tooltip, .. } = visual {
            let all = displays()?;
            let monitor = monitor_nearest_pointer(&all, tooltip.pointer)
                .ok_or_else(|| platform("No display is available for the selection tooltip"))?;
            let size = (330, 76);
            let at =
                place_rectangle_tooltip(tooltip.pointer, size, monitor, RECTANGLE_TOOLTIP_OFFSET);
            let bounds = ScreenRect::new(at.x, at.y, size.0, size.1);
            let mut state = Box::new(WindowPaintState {
                bounds,
                frame: vec![],
                hint: Some(tooltip.text.clone()),
                solid: None,
                operation_id,
                description: "tooltip",
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
            .map_err(|e| platform(format!("tooltip window creation failed: {e}")))?;
            self.tooltip = Some(OverlayWindow {
                hwnd,
                state,
                created_at: Instant::now(),
            });
            unsafe { SetLayeredWindowAttributes(hwnd, TRANSPARENT_KEY, 0, LWA_COLORKEY) }
                .map_err(|e| platform(format!("tooltip layered configuration failed: {e}")))?;
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
            .map_err(|e| platform(format!("tooltip positioning failed: {e}")))?;
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
        if let (Some(window), OverlayVisual::RectanglePicker { tooltip, .. }) =
            (&mut self.tooltip, visual)
        {
            window.state.hint = Some(tooltip.text.clone());
            let all = displays()?;
            if let Some(monitor) = monitor_nearest_pointer(&all, tooltip.pointer) {
                let at = place_rectangle_tooltip(
                    tooltip.pointer,
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
            || !self.passive_edges.is_empty()
            || !self.passive_badges.is_empty()
        {
            tracing::debug!(operation_id = ?self.operation_id, visual = ?self.visual, "closing native visual overlay");
        }
        if let Some(window) = self.tooltip.take() {
            unsafe {
                SetWindowLongPtrW(window.hwnd, GWLP_USERDATA, 0);
                let _ = DestroyWindow(window.hwnd);
            }
        }
        for window in self.windows.drain(..) {
            unsafe {
                SetWindowLongPtrW(window.hwnd, GWLP_USERDATA, 0);
                let _ = DestroyWindow(window.hwnd);
            }
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
            unsafe {
                SetWindowLongPtrW(window.hwnd, GWLP_USERDATA, 0);
                let _ = DestroyWindow(window.hwnd);
            }
        }
        self.operation_id = None;
        self.visual = None;
        self.left_down = false;
        self.escape_down = false;
    }
}
impl Drop for NativeOverlayRenderer {
    fn drop(&mut self) {
        self.close();
    }
}
