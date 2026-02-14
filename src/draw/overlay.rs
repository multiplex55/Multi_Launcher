use crate::draw::input::{
    bridge_key_event_to_runtime, bridge_left_down_to_runtime, bridge_left_up_to_runtime,
    bridge_mouse_move_to_runtime, DrawInputState, PointerModifiers,
};
use crate::draw::keyboard_hook::{map_key_event_to_command, KeyCommand, KeyEvent};
use crate::draw::messages::{ExitReason, MainToOverlay, OverlayToMain, SaveResult};
use crate::draw::model::{ObjectStyle, Tool};
use crate::draw::service::MonitorRect;
use anyhow::{anyhow, Result};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitDialogState {
    Hidden,
    PromptVisible,
    Saving,
    ErrorVisible,
}

impl ExitDialogState {
    pub fn blocks_drawing_input(self) -> bool {
        !matches!(self, Self::Hidden)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayPointerEvent {
    LeftDown { modifiers: PointerModifiers },
    Move,
    LeftUp,
}

pub struct OverlayHandles {
    pub overlay_thread_handle: JoinHandle<()>,
    pub main_to_overlay_tx: Sender<MainToOverlay>,
    pub overlay_to_main_rx: Receiver<OverlayToMain>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayPointerSample {
    pub global_point: (i32, i32),
    pub event: OverlayPointerEvent,
}

fn send_exit_after_cleanup<F>(
    cleanup: F,
    overlay_to_main_tx: &Sender<OverlayToMain>,
    reason: ExitReason,
    save_result: SaveResult,
) where
    F: FnOnce(),
{
    cleanup();
    let _ = overlay_to_main_tx.send(OverlayToMain::Exited {
        reason,
        save_result,
    });
}

pub fn spawn_overlay_for_monitor(monitor_rect: MonitorRect) -> Result<OverlayHandles> {
    let (main_to_overlay_tx, main_to_overlay_rx) = channel::<MainToOverlay>();
    let (overlay_to_main_tx, overlay_to_main_rx) = channel::<OverlayToMain>();

    let overlay_thread_handle = thread::Builder::new()
        .name("draw-overlay".to_string())
        .spawn(move || {
            let mut exit_reason: Option<ExitReason> = None;
            let mut did_start = false;
            let mut window = match OverlayWindow::create_for_monitor(monitor_rect) {
                Some(window) => window,
                None => {
                    let _ = overlay_to_main_tx.send(OverlayToMain::SaveError {
                        error: "unable to initialize draw overlay window".to_string(),
                    });
                    let _ = overlay_to_main_tx.send(OverlayToMain::Exited {
                        reason: ExitReason::StartFailure,
                        save_result: SaveResult::Skipped,
                    });
                    return;
                }
            };

            let mut draw_input = DrawInputState::new(Tool::Pen, ObjectStyle::default());
            loop {
                #[cfg(windows)]
                pump_overlay_messages();

                for pointer_event in window.drain_pointer_events() {
                    let _ = forward_pointer_event_and_request_paint(
                        &mut draw_input,
                        ExitDialogState::Hidden,
                        window.monitor_rect(),
                        pointer_event.global_point,
                        pointer_event.event,
                        &window,
                    );
                }

                match main_to_overlay_rx.recv_timeout(Duration::from_millis(16)) {
                    Ok(MainToOverlay::Start) => {
                        did_start = true;
                        window.show();
                    }
                    Ok(MainToOverlay::UpdateSettings) => {
                        window.request_paint();
                        let _ = overlay_to_main_tx.send(OverlayToMain::SaveProgress {
                            canvas: Default::default(),
                        });
                    }
                    Ok(MainToOverlay::RequestExit { reason }) => {
                        exit_reason = Some(reason);
                        break;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        break;
                    }
                }
            }

            if !did_start {
                let _ = overlay_to_main_tx.send(OverlayToMain::SaveError {
                    error: "overlay exited before start command".to_string(),
                });
            }
            send_exit_after_cleanup(
                || window.shutdown(),
                &overlay_to_main_tx,
                exit_reason.unwrap_or(ExitReason::OverlayFailure),
                SaveResult::Skipped,
            );
        })
        .map_err(|err| anyhow!("failed to spawn draw overlay thread: {err}"))?;

    Ok(OverlayHandles {
        overlay_thread_handle,
        main_to_overlay_tx,
        overlay_to_main_rx,
    })
}

#[cfg(windows)]
fn pump_overlay_messages() {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };

    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, HWND::default(), 0, 0, PM_REMOVE).into() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }
}

pub fn monitor_contains_point(rect: MonitorRect, point: (i32, i32)) -> bool {
    point.0 >= rect.x
        && point.0 < rect.x + rect.width
        && point.1 >= rect.y
        && point.1 < rect.y + rect.height
}

pub fn resolve_monitor_from_cursor() -> Option<MonitorRect> {
    #[cfg(windows)]
    {
        let monitors = platform::enumerate_monitors();
        let cursor = platform::resolve_cursor_position()?;
        return select_monitor_for_point(&monitors, cursor).or_else(|| monitors.first().copied());
    }

    #[cfg(not(windows))]
    {
        None
    }
}

pub fn select_monitor_for_point(
    monitors: &[MonitorRect],
    point: (i32, i32),
) -> Option<MonitorRect> {
    monitors
        .iter()
        .copied()
        .find(|rect| monitor_contains_point(*rect, point))
}

pub fn global_to_local(point: (i32, i32), origin: (i32, i32)) -> (i32, i32) {
    (point.0 - origin.0, point.1 - origin.1)
}

pub fn monitor_local_point_for_global(
    monitors: &[MonitorRect],
    point: (i32, i32),
) -> Option<(MonitorRect, (i32, i32))> {
    let monitor =
        select_monitor_for_point(monitors, point).or_else(|| monitors.first().copied())?;
    Some((monitor, global_to_local(point, (monitor.x, monitor.y))))
}

pub fn forward_pointer_event_to_draw_input(
    draw_input: &mut DrawInputState,
    exit_dialog_state: ExitDialogState,
    tool_monitor_rect: MonitorRect,
    global_point: (i32, i32),
    event: OverlayPointerEvent,
) -> bool {
    if exit_dialog_state.blocks_drawing_input() {
        return false;
    }

    let (_, local_point) = monitor_local_point_for_global(&[tool_monitor_rect], global_point)
        .unwrap_or((
            tool_monitor_rect,
            global_to_local(global_point, (tool_monitor_rect.x, tool_monitor_rect.y)),
        ));
    match event {
        OverlayPointerEvent::LeftDown { modifiers } => {
            bridge_left_down_to_runtime(draw_input, local_point, modifiers);
        }
        OverlayPointerEvent::Move => bridge_mouse_move_to_runtime(draw_input, local_point),
        OverlayPointerEvent::LeftUp => bridge_left_up_to_runtime(draw_input, local_point),
    }
    true
}

pub fn forward_pointer_event_and_request_paint(
    draw_input: &mut DrawInputState,
    exit_dialog_state: ExitDialogState,
    tool_monitor_rect: MonitorRect,
    global_point: (i32, i32),
    event: OverlayPointerEvent,
    window: &OverlayWindow,
) -> bool {
    let handled = forward_pointer_event_to_draw_input(
        draw_input,
        exit_dialog_state,
        tool_monitor_rect,
        global_point,
        event,
    );
    if handled {
        window.request_paint();
    }
    handled
}
pub fn forward_key_event_to_draw_input(
    draw_input: &mut DrawInputState,
    exit_dialog_state: ExitDialogState,
    event: KeyEvent,
) -> bool {
    if exit_dialog_state.blocks_drawing_input() {
        return false;
    }

    bridge_key_event_to_runtime(draw_input, event);
    true
}

pub fn forward_key_event_and_request_paint(
    draw_input: &mut DrawInputState,
    exit_dialog_state: ExitDialogState,
    event: KeyEvent,
    window: &OverlayWindow,
) -> bool {
    let mapped = map_key_event_to_command(true, event);
    let handled = forward_key_event_to_draw_input(draw_input, exit_dialog_state, event);
    if handled && matches!(mapped, Some(KeyCommand::Undo | KeyCommand::Redo)) {
        window.request_paint();
    }
    handled
}
#[cfg(windows)]
mod platform {
    use super::{global_to_local, OverlayPointerEvent, OverlayPointerSample};
    use crate::draw::model::FIRST_PASS_TRANSPARENCY_COLORKEY;
    use crate::draw::service::MonitorRect;
    use once_cell::sync::Lazy;
    use std::collections::HashMap;
    use std::mem;
    use std::ptr;
    use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
    use std::sync::Mutex;
    use std::sync::Once;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{BOOL, COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, EndPaint,
        EnumDisplayMonitors, GetMonitorInfoW, InvalidateRect, SelectObject, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, MONITORINFOEXW,
        PAINTSTRUCT, SRCCOPY,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, GetWindowLongPtrW,
        RegisterClassW, ReleaseCapture, SetCapture, SetLayeredWindowAttributes, SetWindowLongPtrW,
        SetWindowPos, GWLP_USERDATA, HWND_TOPMOST, LWA_COLORKEY, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOSIZE, SWP_SHOWWINDOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_ACTIVATE, WM_ERASEBKGND,
        WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SHOWWINDOW, WM_WINDOWPOSCHANGED,
        WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    };

    static POINTER_SENDERS: Lazy<Mutex<HashMap<isize, Sender<OverlayPointerSample>>>> =
        Lazy::new(|| Mutex::new(HashMap::new()));
    static WINDOW_ORIGINS: Lazy<Mutex<HashMap<isize, (i32, i32)>>> =
        Lazy::new(|| Mutex::new(HashMap::new()));

    pub fn compose_overlay_window_ex_style() -> WINDOW_EX_STYLE {
        WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE
    }

    pub enum OverlayTransparencyMode {
        ColorKeyFirstPass,
    }

    pub fn first_pass_transparency_colorkey() -> COLORREF {
        COLORREF(
            (FIRST_PASS_TRANSPARENCY_COLORKEY.r as u32)
                | ((FIRST_PASS_TRANSPARENCY_COLORKEY.g as u32) << 8)
                | ((FIRST_PASS_TRANSPARENCY_COLORKEY.b as u32) << 16),
        )
    }

    pub fn configure_layered_window_transparency(
        hwnd: HWND,
        mode: OverlayTransparencyMode,
    ) -> windows::core::Result<()> {
        // Technical debt: this first pass uses colorkey transparency, which cannot
        // represent per-pixel alpha. Keep this seam so we can swap to
        // `UpdateLayeredWindow` without changing overlay window creation call sites.
        let (key, alpha, flags) = match mode {
            OverlayTransparencyMode::ColorKeyFirstPass => {
                (first_pass_transparency_colorkey(), 0, LWA_COLORKEY)
            }
        };
        unsafe { SetLayeredWindowAttributes(hwnd, key, alpha, flags) }
    }

    fn widestring(value: &str) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub(super) fn resolve_cursor_position() -> Option<(i32, i32)> {
        let mut point = POINT::default();
        unsafe {
            if GetCursorPos(&mut point).is_ok() {
                Some((point.x, point.y))
            } else {
                None
            }
        }
    }

    pub(super) fn enumerate_monitors() -> Vec<MonitorRect> {
        unsafe extern "system" fn enum_proc(
            monitor: windows::Win32::Graphics::Gdi::HMONITOR,
            _hdc: HDC,
            _rect: *mut RECT,
            data: LPARAM,
        ) -> BOOL {
            let monitors = unsafe { &mut *(data.0 as *mut Vec<MonitorRect>) };
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = mem::size_of::<MONITORINFOEXW>() as u32;
            if unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo as *mut _ as *mut _) }
                .as_bool()
            {
                let rc = info.monitorInfo.rcMonitor;
                monitors.push(MonitorRect {
                    x: rc.left,
                    y: rc.top,
                    width: rc.right - rc.left,
                    height: rc.bottom - rc.top,
                });
            }
            BOOL(1)
        }

        let mut monitors = Vec::new();
        unsafe {
            let _ = EnumDisplayMonitors(
                HDC::default(),
                None,
                Some(enum_proc),
                LPARAM(&mut monitors as *mut Vec<MonitorRect> as isize),
            );
        }
        monitors
    }

    unsafe extern "system" fn overlay_wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
                if !hdc.0.is_null() {
                    let mem_dc = HDC(unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut _);
                    if !mem_dc.0.is_null() {
                        let width = ps.rcPaint.right - ps.rcPaint.left;
                        let height = ps.rcPaint.bottom - ps.rcPaint.top;
                        let _ = unsafe {
                            BitBlt(
                                hdc,
                                ps.rcPaint.left,
                                ps.rcPaint.top,
                                width,
                                height,
                                mem_dc,
                                ps.rcPaint.left,
                                ps.rcPaint.top,
                                SRCCOPY,
                            )
                        };
                    }
                }
                unsafe {
                    let _ = EndPaint(hwnd, &ps);
                }
                LRESULT(0)
            }
            WM_SHOWWINDOW | WM_ACTIVATE | WM_WINDOWPOSCHANGED => {
                let _ = unsafe {
                    SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    )
                };
                unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
            }
            WM_LBUTTONDOWN | WM_MOUSEMOVE | WM_LBUTTONUP => {
                if msg == WM_LBUTTONDOWN {
                    let _ = unsafe { SetCapture(hwnd) };
                } else if msg == WM_LBUTTONUP {
                    unsafe { ReleaseCapture() };
                }

                let local_x = (lparam.0 & 0xffff) as i16 as i32;
                let local_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
                if let (Ok(origins), Ok(senders)) = (WINDOW_ORIGINS.lock(), POINTER_SENDERS.lock())
                {
                    if let (Some(origin), Some(tx)) = (origins.get(&hwnd.0), senders.get(&hwnd.0)) {
                        let event = match msg {
                            WM_LBUTTONDOWN => OverlayPointerEvent::LeftDown {
                                modifiers: Default::default(),
                            },
                            WM_MOUSEMOVE => OverlayPointerEvent::Move,
                            WM_LBUTTONUP => OverlayPointerEvent::LeftUp,
                            _ => unreachable!(),
                        };
                        let _ = tx.send(OverlayPointerSample {
                            global_point: (origin.0 + local_x, origin.1 + local_y),
                            event,
                        });
                    }
                }
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    #[derive(Debug)]
    pub struct OverlayWindow {
        hwnd: HWND,
        mem_dc: HDC,
        dib: HBITMAP,
        old_bitmap: HGDIOBJ,
        pub bits: *mut u8,
        size_bytes: usize,
        monitor_rect: MonitorRect,
        origin: (i32, i32),
        pointer_rx: Receiver<OverlayPointerSample>,
    }

    unsafe impl Send for OverlayWindow {}

    impl OverlayWindow {
        pub fn create_for_cursor() -> Option<Self> {
            let cursor = resolve_cursor_position()?;
            let monitors = enumerate_monitors();
            let monitor_rect = super::select_monitor_for_point(&monitors, cursor)
                .or_else(|| monitors.first().copied())?;
            Self::create_for_monitor(monitor_rect)
        }

        pub fn create_for_monitor(monitor_rect: MonitorRect) -> Option<Self> {
            static REGISTER_CLASS: Once = Once::new();
            let class_name = widestring("MultiLauncherDrawOverlay");
            let hinstance = unsafe { GetModuleHandleW(PCWSTR::null()) }.ok()?;

            REGISTER_CLASS.call_once(|| unsafe {
                let wc = WNDCLASSW {
                    hInstance: hinstance.into(),
                    lpszClassName: PCWSTR(class_name.as_ptr()),
                    lpfnWndProc: Some(overlay_wndproc),
                    ..Default::default()
                };
                let _ = RegisterClassW(&wc);
            });

            let hwnd = unsafe {
                CreateWindowExW(
                    compose_overlay_window_ex_style(),
                    PCWSTR(class_name.as_ptr()),
                    PCWSTR::null(),
                    WINDOW_STYLE(WS_POPUP.0),
                    monitor_rect.x,
                    monitor_rect.y,
                    monitor_rect.width,
                    monitor_rect.height,
                    None,
                    None,
                    hinstance,
                    None,
                )
                .ok()?
            };

            if configure_layered_window_transparency(
                hwnd,
                OverlayTransparencyMode::ColorKeyFirstPass,
            )
            .is_err()
            {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return None;
            }

            let mem_dc = unsafe { CreateCompatibleDC(HDC::default()) };
            if mem_dc.0.is_null() {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return None;
            }

            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader = BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: monitor_rect.width,
                biHeight: -monitor_rect.height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            };

            let mut bits: *mut core::ffi::c_void = ptr::null_mut();
            let dib = unsafe {
                CreateDIBSection(
                    mem_dc,
                    &bmi,
                    DIB_RGB_COLORS,
                    &mut bits,
                    windows::Win32::Foundation::HANDLE::default(),
                    0,
                )
                .ok()?
            };
            if bits.is_null() {
                unsafe {
                    let _ = DeleteDC(mem_dc);
                    let _ = DestroyWindow(hwnd);
                }
                return None;
            }

            let old_bitmap = unsafe { SelectObject(mem_dc, dib) };
            unsafe {
                let _ = SetWindowLongPtrW(hwnd, GWLP_USERDATA, mem_dc.0 as isize);
            }

            let (pointer_tx, pointer_rx) = channel::<OverlayPointerSample>();
            if let Ok(mut senders) = POINTER_SENDERS.lock() {
                senders.insert(hwnd.0, pointer_tx);
            }
            if let Ok(mut origins) = WINDOW_ORIGINS.lock() {
                origins.insert(hwnd.0, (monitor_rect.x, monitor_rect.y));
            }

            let size_bytes = (monitor_rect.width as usize)
                .saturating_mul(monitor_rect.height as usize)
                .saturating_mul(4);

            Some(Self {
                hwnd,
                mem_dc,
                dib,
                old_bitmap,
                bits: bits as *mut u8,
                size_bytes,
                monitor_rect,
                origin: (monitor_rect.x, monitor_rect.y),
                pointer_rx,
            })
        }

        pub fn drain_pointer_events(&self) -> Vec<OverlayPointerSample> {
            let mut events = Vec::new();
            loop {
                match self.pointer_rx.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                }
            }
            events
        }

        pub fn monitor_rect(&self) -> MonitorRect {
            self.monitor_rect
        }

        pub fn global_to_local(&self, point: (i32, i32)) -> (i32, i32) {
            global_to_local(point, self.origin)
        }

        pub fn show(&self) {
            unsafe {
                let _ = SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }

        pub fn request_paint(&self) {
            unsafe {
                let _ = InvalidateRect(self.hwnd, None, false);
            }
        }

        pub fn with_bitmap_mut<F>(&mut self, mut f: F)
        where
            F: FnMut(&mut [u8], u32, u32),
        {
            if self.bits.is_null() || self.size_bytes == 0 {
                return;
            }

            let pixels = unsafe { std::slice::from_raw_parts_mut(self.bits, self.size_bytes) };
            f(
                pixels,
                self.monitor_rect.width as u32,
                self.monitor_rect.height as u32,
            );
        }

        pub fn shutdown(&mut self) {
            unsafe {
                if !self.mem_dc.0.is_null() {
                    let _ = SelectObject(self.mem_dc, self.old_bitmap);
                }
                if !self.dib.0.is_null() {
                    let _ = DeleteObject(self.dib);
                    self.dib = HBITMAP::default();
                }
                if !self.mem_dc.0.is_null() {
                    let _ = DeleteDC(self.mem_dc);
                    self.mem_dc = HDC::default();
                }
                if !self.hwnd.0.is_null() {
                    if let Ok(mut senders) = POINTER_SENDERS.lock() {
                        senders.remove(&self.hwnd.0);
                    }
                    if let Ok(mut origins) = WINDOW_ORIGINS.lock() {
                        origins.remove(&self.hwnd.0);
                    }
                    let _ = DestroyWindow(self.hwnd);
                    self.hwnd = HWND::default();
                }
                self.bits = ptr::null_mut();
                self.size_bytes = 0;
            }
        }
    }

    impl Drop for OverlayWindow {
        fn drop(&mut self) {
            self.shutdown();
        }
    }

    #[cfg(test)]
    mod windows_tests {
        use super::{
            compose_overlay_window_ex_style, configure_layered_window_transparency,
            first_pass_transparency_colorkey, OverlayTransparencyMode,
        };
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            LWA_COLORKEY, WS_EX_LAYERED, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
        };

        #[test]
        fn style_flags_include_topmost_layered_but_no_clickthrough() {
            let style = compose_overlay_window_ex_style();
            assert_ne!(style.0 & WS_EX_LAYERED.0, 0);
            assert_ne!(style.0 & WS_EX_TOPMOST.0, 0);
            assert_eq!(style.0 & WS_EX_TRANSPARENT.0, 0);
        }

        #[test]
        fn colorkey_mode_is_wired_for_first_pass_transparency() {
            assert_eq!(first_pass_transparency_colorkey().0, 0x00ff00ff);

            let result = configure_layered_window_transparency(
                HWND::default(),
                OverlayTransparencyMode::ColorKeyFirstPass,
            );
            assert!(result.is_err());
            assert_eq!(LWA_COLORKEY.0, 0x1);
        }
    }
}

#[cfg(windows)]
pub use platform::OverlayWindow;

#[cfg(not(windows))]
#[derive(Debug, Default)]
pub struct OverlayWindow;

#[cfg(not(windows))]
impl OverlayWindow {
    pub fn create_for_cursor() -> Option<Self> {
        Some(Self)
    }

    pub fn create_for_monitor(_monitor_rect: MonitorRect) -> Option<Self> {
        Some(Self)
    }

    pub fn monitor_rect(&self) -> MonitorRect {
        MonitorRect::default()
    }

    pub fn global_to_local(&self, point: (i32, i32)) -> (i32, i32) {
        point
    }

    pub fn show(&self) {}

    pub fn request_paint(&self) {}

    pub fn drain_pointer_events(&self) -> Vec<OverlayPointerSample> {
        Vec::new()
    }

    pub fn with_bitmap_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut [u8], u32, u32),
    {
        let mut pixels = [];
        f(&mut pixels, 0, 0);
    }

    pub fn shutdown(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::{
        forward_pointer_event_to_draw_input, global_to_local, monitor_contains_point,
        monitor_local_point_for_global, select_monitor_for_point, send_exit_after_cleanup,
        ExitDialogState, OverlayPointerEvent,
    };
    use crate::draw::messages::{ExitReason, OverlayToMain, SaveResult};
    use crate::draw::{
        input::DrawInputState,
        model::{ObjectStyle, Tool},
        service::MonitorRect,
    };

    fn draw_state(tool: Tool) -> DrawInputState {
        DrawInputState::new(tool, ObjectStyle::default())
    }

    #[test]
    fn monitor_local_resolution_uses_selected_monitor_origin() {
        let monitors = [
            MonitorRect {
                x: -1920,
                y: 0,
                width: 1920,
                height: 1080,
            },
            MonitorRect {
                x: 0,
                y: 0,
                width: 2560,
                height: 1440,
            },
        ];

        let (monitor, local) =
            monitor_local_point_for_global(&monitors, (100, 100)).expect("monitor and local point");
        assert_eq!(monitor, monitors[1]);
        assert_eq!(local, (100, 100));

        let (monitor2, local2) =
            monitor_local_point_for_global(&monitors, (-100, 30)).expect("monitor and local point");
        assert_eq!(monitor2, monitors[0]);
        assert_eq!(local2, (1820, 30));
    }

    #[test]
    fn pointer_events_route_to_draw_input_state_and_commit_stroke() {
        let mut input = draw_state(Tool::Line);
        let monitor = MonitorRect {
            x: 1920,
            y: 0,
            width: 2560,
            height: 1440,
        };

        assert!(forward_pointer_event_to_draw_input(
            &mut input,
            ExitDialogState::Hidden,
            monitor,
            (2000, 200),
            OverlayPointerEvent::LeftDown {
                modifiers: Default::default()
            }
        ));
        assert!(forward_pointer_event_to_draw_input(
            &mut input,
            ExitDialogState::Hidden,
            monitor,
            (2100, 260),
            OverlayPointerEvent::Move,
        ));
        assert!(forward_pointer_event_to_draw_input(
            &mut input,
            ExitDialogState::Hidden,
            monitor,
            (2200, 300),
            OverlayPointerEvent::LeftUp,
        ));

        assert_eq!(input.history().undo_len(), 1);
        assert_eq!(input.history().canvas().objects.len(), 1);
    }

    #[test]
    fn non_hidden_dialog_blocks_pointer_events_and_prevents_commits() {
        for state in [
            ExitDialogState::PromptVisible,
            ExitDialogState::Saving,
            ExitDialogState::ErrorVisible,
        ] {
            let mut input = draw_state(Tool::Rect);
            let monitor = MonitorRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            };

            assert!(!forward_pointer_event_to_draw_input(
                &mut input,
                state,
                monitor,
                (100, 100),
                OverlayPointerEvent::LeftDown {
                    modifiers: Default::default()
                }
            ));
            assert!(!forward_pointer_event_to_draw_input(
                &mut input,
                state,
                monitor,
                (120, 120),
                OverlayPointerEvent::Move,
            ));
            assert!(!forward_pointer_event_to_draw_input(
                &mut input,
                state,
                monitor,
                (140, 140),
                OverlayPointerEvent::LeftUp,
            ));

            assert_eq!(
                input.history().undo_len(),
                0,
                "unexpected commit with state {state:?}"
            );
        }
    }

    #[test]
    fn select_monitor_by_containment() {
        let monitors = [
            MonitorRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            MonitorRect {
                x: 1920,
                y: 0,
                width: 2560,
                height: 1440,
            },
        ];

        let selected = select_monitor_for_point(&monitors, (2000, 100)).expect("monitor exists");
        assert_eq!(selected, monitors[1]);
        assert!(monitor_contains_point(monitors[0], (1919, 1079)));
        assert!(!monitor_contains_point(monitors[0], (1920, 10)));
    }

    #[test]
    fn global_to_local_uses_overlay_monitor_rect_not_launcher_rect() {
        let launcher_rect = MonitorRect {
            x: 100,
            y: 100,
            width: 800,
            height: 600,
        };
        let overlay_monitor_rect = MonitorRect {
            x: 1920,
            y: 200,
            width: 2560,
            height: 1440,
        };
        let point = (2050, 310);

        let overlay_local =
            global_to_local(point, (overlay_monitor_rect.x, overlay_monitor_rect.y));
        let launcher_local = global_to_local(point, (launcher_rect.x, launcher_rect.y));
        assert_eq!(overlay_local, (130, 110));
        assert_ne!(overlay_local, launcher_local);
    }

    #[test]
    fn exit_notification_is_emitted_only_after_cleanup_runs() {
        let (tx, rx) = std::sync::mpsc::channel();
        let cleaned = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cleaned_flag = cleaned.clone();

        send_exit_after_cleanup(
            move || {
                cleaned_flag.store(true, std::sync::atomic::Ordering::SeqCst);
            },
            &tx,
            ExitReason::UserRequest,
            SaveResult::Skipped,
        );

        let msg = rx.recv().expect("exit message should be sent");
        assert!(cleaned.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(
            msg,
            OverlayToMain::Exited {
                reason: ExitReason::UserRequest,
                save_result: SaveResult::Skipped,
            }
        );
    }
}
