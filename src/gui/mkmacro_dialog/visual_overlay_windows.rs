//! Win32 implementation of the overlay renderer.
//!
//! A separate popup is used for every physical display.  This avoids holes in
//! non-rectangular monitor arrangements and, because every conversion starts
//! with the popup's signed origin, also handles displays above/left of primary.
use super::*;
use windows::{
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{
            CreatePen, DeleteObject, EnumDisplayMonitors, GetDC, GetMonitorInfoW, HDC, HGDIOBJ,
            HMONITOR, MONITORINFO, PS_SOLID, Rectangle, ReleaseDC, SelectObject, SetBkMode,
            SetTextColor, TRANSPARENT, TextOutW,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON},
            WindowsAndMessaging::{
                CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
                DispatchMessageW, GetCursorPos, HWND_TOPMOST, MSG, PM_REMOVE, PeekMessageW,
                RegisterClassW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetWindowPos,
                ShowWindow, TranslateMessage, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
                WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
            },
        },
    },
    core::PCWSTR,
};

#[derive(Clone, Copy)]
struct OverlayWindow {
    hwnd: HWND,
    bounds: ScreenRect,
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
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
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

fn intersects(a: ScreenRect, b: ScreenRect) -> bool {
    i64::from(a.x) < b.right()
        && i64::from(b.x) < a.right()
        && i64::from(a.y) < b.bottom()
        && i64::from(b.y) < a.bottom()
}

impl NativeOverlayRenderer {
    fn paint(&self) {
        for window in &self.windows {
            let Some(visual) = &self.visual else { continue };
            unsafe {
                let dc = GetDC(window.hwnd);
                if dc.0.is_null() {
                    continue;
                }
                let pen = CreatePen(PS_SOLID, 5, COLORREF(0x0000ffff));
                let old = SelectObject(dc, HGDIOBJ(pen.0));
                let outline = |r: ScreenRect| {
                    let x = i64::from(r.x) - i64::from(window.bounds.x);
                    let y = i64::from(r.y) - i64::from(window.bounds.y);
                    let _ = Rectangle(
                        dc,
                        x as i32,
                        y as i32,
                        (x + i64::from(r.width)) as i32,
                        (y + i64::from(r.height)) as i32,
                    );
                };
                match visual {
                    OverlayVisual::RectanglePicker { selection, .. } => {
                        if let Some(r) = selection {
                            outline(*r);
                        }
                    }
                    OverlayVisual::RectanglePreview(r) | OverlayVisual::Window { rect: r, .. } => {
                        outline(*r)
                    }
                    OverlayVisual::Monitor(d) => {
                        outline(d.bounds);
                        draw_label(dc, window.bounds, d);
                    }
                    OverlayVisual::Monitors(ds) => {
                        for d in ds {
                            outline(d.bounds);
                            draw_label(dc, window.bounds, d);
                        }
                    }
                }
                SelectObject(dc, old);
                let _ = DeleteObject(HGDIOBJ(pen.0));
                ReleaseDC(window.hwnd, dc);
            }
        }
    }
}

unsafe fn draw_label(dc: HDC, origin: ScreenRect, descriptor: &MonitorDescriptor) {
    let text: Vec<u16> = descriptor.index.to_string().encode_utf16().collect();
    unsafe {
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, COLORREF(0x0000ffff));
    }
    let x = i64::from(descriptor.bounds.x) - i64::from(origin.x) + 30;
    let y = i64::from(descriptor.bounds.y) - i64::from(origin.y) + 30;
    let _ = unsafe { TextOutW(dc, x as i32, y as i32, &text) };
}

impl OverlayRenderer for NativeOverlayRenderer {
    fn show(
        &mut self,
        operation_id: OperationId,
        visual: &OverlayVisual,
        mouse_transparent: bool,
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
        unsafe {
            RegisterClassW(&wc);
        }
        let target = match visual {
            OverlayVisual::RectanglePicker {
                virtual_desktop, ..
            } => *virtual_desktop,
            OverlayVisual::RectanglePreview(r) | OverlayVisual::Window { rect: r, .. } => *r,
            OverlayVisual::Monitor(d) => d.bounds,
            OverlayVisual::Monitors(ds) => ds
                .iter()
                .map(|d| d.bounds)
                .reduce(|a, b| {
                    ScreenRect::new(
                        a.x.min(b.x),
                        a.y.min(b.y),
                        (a.right().max(b.right()) - i64::from(a.x.min(b.x))) as u32,
                        (a.bottom().max(b.bottom()) - i64::from(a.y.min(b.y))) as u32,
                    )
                })
                .unwrap_or(ScreenRect::new(0, 0, 0, 0)),
        };
        for bounds in displays().into_iter().filter(|b| intersects(*b, target)) {
            let mut ex = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
            if mouse_transparent {
                ex |= WS_EX_TRANSPARENT;
            }
            let hwnd = unsafe {
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
                    None,
                )
            }
            .map_err(|e| platform(format!("overlay window creation failed: {e}")))?;
            unsafe {
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    bounds.x,
                    bounds.y,
                    bounds.width as i32,
                    bounds.height as i32,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )
                .map_err(|e| platform(format!("overlay positioning failed: {e}")))?;
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            self.windows.push(OverlayWindow { hwnd, bounds });
        }
        if self.windows.is_empty() {
            return Err(platform("no physical display intersects the overlay"));
        }
        self.operation_id = Some(operation_id);
        self.visual = Some(visual.clone());
        self.paint();
        Ok(())
    }
    fn repaint(
        &mut self,
        operation_id: OperationId,
        visual: &OverlayVisual,
    ) -> Result<(), VisualOverlayError> {
        if self.operation_id == Some(operation_id) {
            self.visual = Some(visual.clone());
            self.paint();
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
        for w in self.windows.drain(..) {
            unsafe {
                let _ = DestroyWindow(w.hwnd);
            }
        }
        self.operation_id = None;
        self.visual = None;
        self.left_down = false;
    }
}
impl Drop for NativeOverlayRenderer {
    fn drop(&mut self) {
        self.close();
    }
}
