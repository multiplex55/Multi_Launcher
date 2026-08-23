//! Win32 color-key overlay renderer.  There is deliberately one popup per
//! physical monitor: a single virtual-desktop popup would cover monitor gaps.
use super::*;
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{
            GetLastError, COLORREF, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, RECT,
            WPARAM,
        },
        Graphics::Gdi::{
            BeginPaint, CreatePen, CreateSolidBrush, DeleteObject, EndPaint, EnumDisplayMonitors,
            FillRect, GetMonitorInfoW, GetStockObject, InvalidateRect, Rectangle, SelectObject,
            SetBkMode, SetTextColor, TextOutW, UpdateWindow, HDC, HGDIOBJ, HMONITOR, MONITORINFO,
            NULL_BRUSH, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos,
                GetWindowLongPtrW, PeekMessageW, RegisterClassW, SetLayeredWindowAttributes,
                SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, CREATESTRUCTW,
                CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, HWND_TOPMOST, LWA_COLORKEY, MSG, PM_REMOVE,
                SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_SHOWNOACTIVATE, WM_ERASEBKGND, WM_NCCREATE,
                WM_PAINT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
                WS_EX_TRANSPARENT, WS_POPUP,
            },
        },
    },
};

// COLORREF is 0x00bbggrr. Magenta is reserved exclusively for transparency.
const TRANSPARENT_KEY: COLORREF = COLORREF(0x00ff00ff);
const OUTLINE_COLOR: COLORREF = COLORREF(0x0000ffff); // bright yellow
const BADGE_COLOR: COLORREF = COLORREF(0x00400000); // dark blue
const LABEL_COLOR: COLORREF = COLORREF(0x00ffffff);

struct WindowPaintState {
    bounds: ScreenRect,
    frame: Vec<OverlayFramePrimitive>,
}
struct OverlayWindow {
    hwnd: HWND,
    // The pointee address stays stable while wndproc reads it through GWLP_USERDATA.
    state: Box<WindowPaintState>,
}

pub(super) struct NativeOverlayRenderer {
    windows: Vec<OverlayWindow>,
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
    let clear = unsafe { CreateSolidBrush(TRANSPARENT_KEY) };
    unsafe { FillRect(dc, &client, clear) };
    unsafe {
        let _ = DeleteObject(HGDIOBJ(clear.0));
    };

    let pen = unsafe { CreatePen(PS_SOLID, 5, OUTLINE_COLOR) };
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

fn displays() -> Vec<ScreenRect> {
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
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut result as *mut _ as isize),
        );
    }
    result
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
        }
    }
    fn fail<T>(&mut self, message: impl Into<String>) -> Result<T, VisualOverlayError> {
        let error = platform(message);
        self.close();
        Err(error)
    }
}

impl OverlayRenderer for NativeOverlayRenderer {
    fn show(
        &mut self,
        operation_id: OperationId,
        visual: &OverlayVisual,
        _mouse_transparent: bool,
    ) -> Result<(), VisualOverlayError> {
        self.close();
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
        let Some(target) = Self::target(visual) else {
            return self.fail("No physical display intersects the requested preview region");
        };
        let monitor_bounds = intersecting_monitor_bounds(&displays(), target);
        if monitor_bounds.is_empty() {
            return self.fail("No physical display intersects the requested preview region");
        }
        let frame = overlay_frame(visual);
        for bounds in monitor_bounds {
            let mut state = Box::new(WindowPaintState {
                bounds,
                frame: frame.clone(),
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
            self.windows.push(OverlayWindow { hwnd, state });
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
        if left {
            out.push(OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::PointerMoved(MkPoint { x: p.x, y: p.y }),
            });
        }
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
        for window in self.windows.drain(..) {
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
