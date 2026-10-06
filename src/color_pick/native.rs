use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};

use super::ColorPickOutcome;
use crate::mkmacro::screen::CapturedRegion;

pub(crate) trait NativePickerFactory: Send + Sync {
    fn spawn(
        &self,
        snapshot: Arc<CapturedRegion>,
        repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<NativePickerHandle, String>;
}

pub(crate) struct SystemNativePickerFactory;
impl NativePickerFactory for SystemNativePickerFactory {
    fn spawn(
        &self,
        snapshot: Arc<CapturedRegion>,
        repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<NativePickerHandle, String> {
        NativePickerHandle::spawn_worker(repaint, move |cancel, ready| {
            #[cfg(windows)]
            {
                windows_picker::run(snapshot, cancel, ready)
            }
            #[cfg(not(windows))]
            {
                let _ = (snapshot, cancel, ready);
                ColorPickOutcome::Failed("Screen color picking is available only on Windows".into())
            }
        })
    }
}

pub(crate) struct NativePickerHandle {
    receiver: mpsc::Receiver<ColorPickOutcome>,
    thread: Option<JoinHandle<()>>,
    cancel: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
}
impl NativePickerHandle {
    pub(crate) fn spawn_worker(
        repaint: Arc<dyn Fn() + Send + Sync>,
        worker: impl FnOnce(Arc<AtomicBool>, Arc<AtomicBool>) -> ColorPickOutcome + Send + 'static,
    ) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let ready = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let worker_ready = Arc::clone(&ready);
        let thread = thread::Builder::new()
            .name("color-picker-native".into())
            .spawn(move || {
                // The native function's RAII resources unwind before a terminal result
                // is sent; poll additionally waits for complete thread termination.
                let outcome =
                    catch_unwind(AssertUnwindSafe(|| worker(worker_cancel, worker_ready)))
                        .unwrap_or_else(|_| {
                            ColorPickOutcome::Failed("Native color picker worker panicked".into())
                        });
                let _ = sender.send(outcome);
                repaint();
            })
            .map_err(|error| format!("Could not start native color picker: {error}"))?;
        Ok(Self {
            receiver,
            thread: Some(thread),
            cancel,
            ready,
        })
    }
    #[cfg(test)]
    pub(crate) fn closed_fixture() -> Self {
        let (sender, receiver) = mpsc::channel();
        drop(sender);
        Self {
            receiver,
            thread: Some(thread::spawn(|| {})),
            cancel: Arc::new(AtomicBool::new(false)),
            ready: Arc::new(AtomicBool::new(true)),
        }
    }
    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub(crate) fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
    pub(crate) fn poll(&mut self) -> Option<ColorPickOutcome> {
        if !self.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            return None;
        }
        let joined = self.thread.take().map(|thread| thread.join());
        if matches!(joined, Some(Err(_))) {
            return Some(ColorPickOutcome::Failed(
                "Native color picker thread terminated unexpectedly".into(),
            ));
        }
        Some(self.receiver.try_recv().unwrap_or_else(|_| {
            ColorPickOutcome::Failed("Native color picker closed without a result".into())
        }))
    }
}
impl Drop for NativePickerHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(windows)]
mod windows_picker {
    use super::*;
    use crate::color_pick::{FrozenPicker, MAGNIFIER_SIDE};
    use crate::mkmacro::screen::ScreenRect;
    use std::{ffi::c_void, mem, ptr};
    use windows::Win32::Foundation::{
        COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
    };
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, BitBlt, CreateCompatibleDC,
        CreateDIBSection, CreateSolidBrush, DIB_RGB_COLORS, DeleteDC, DeleteObject, EndPaint,
        FillRect, FrameRect, HBITMAP, HDC, HGDIOBJ, InvalidateRect, PAINTSTRUCT, SRCCOPY,
        SelectObject,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        SetThreadDpiAwarenessContext,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        GWLP_USERDATA, GetCursorPos, GetMessageW, GetSystemMetrics, GetWindowLongPtrW,
        HWND_TOPMOST, IDC_CROSS, IsWindow, KillTimer, LoadCursorW, MSG, RegisterClassW,
        SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_HIDE,
        SW_SHOW, SWP_SHOWWINDOW, SetForegroundWindow, SetTimer, SetWindowLongPtrW, SetWindowPos,
        ShowWindow, TranslateMessage, WM_CLOSE, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_KEYDOWN,
        WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_TIMER, WNDCLASSW,
        WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    };
    use windows::core::{PCWSTR, w};

    struct ThreadDpi(DPI_AWARENESS_CONTEXT);
    impl Drop for ThreadDpi {
        fn drop(&mut self) {
            unsafe {
                SetThreadDpiAwarenessContext(self.0);
            }
        }
    }
    struct BackingDib {
        dc: HDC,
        bitmap: HBITMAP,
        old: HGDIOBJ,
    }
    impl BackingDib {
        fn new(snapshot: &CapturedRegion) -> Result<Self, String> {
            let rect = snapshot.rect();
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: i32::try_from(rect.width).map_err(|_| "Picker width is too large")?,
                    biHeight: -i32::try_from(rect.height)
                        .map_err(|_| "Picker height is too large")?,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                bmiColors: [Default::default()],
            };
            let dc = unsafe { CreateCompatibleDC(None) };
            if dc.0.is_null() {
                return Err("Could not allocate picker backing DC".into());
            }
            let mut bits = ptr::null_mut();
            let bitmap =
                match unsafe { CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, None, 0) } {
                    Ok(bitmap) if !bits.is_null() => bitmap,
                    Ok(bitmap) => {
                        unsafe {
                            let _ = DeleteObject(bitmap);
                            let _ = DeleteDC(dc);
                        }
                        return Err("Picker DIB has no pixel storage".into());
                    }
                    Err(error) => {
                        unsafe {
                            let _ = DeleteDC(dc);
                        }
                        return Err(format!("Could not allocate picker DIB: {error}"));
                    }
                };
            let old = unsafe { SelectObject(dc, bitmap) };
            if old.0.is_null() || old.0 as isize == -1 {
                unsafe {
                    let _ = DeleteObject(bitmap);
                    let _ = DeleteDC(dc);
                }
                return Err("Could not select picker DIB".into());
            }
            let backing = Self { dc, bitmap, old };
            // DIB dimensions and 32-bit stride exactly match the validated RGBA image.
            let destination = unsafe {
                std::slice::from_raw_parts_mut(bits.cast::<u8>(), snapshot.image.as_raw().len())
            };
            for (source, destination) in snapshot
                .image
                .as_raw()
                .chunks_exact(4)
                .zip(destination.chunks_exact_mut(4))
            {
                destination.copy_from_slice(&[source[2], source[1], source[0], 255]);
            }
            Ok(backing)
        }
    }
    impl Drop for BackingDib {
        fn drop(&mut self) {
            unsafe {
                let _ = SelectObject(self.dc, self.old);
                let _ = DeleteObject(self.bitmap);
                let _ = DeleteDC(self.dc);
            }
        }
    }
    struct Paint {
        hwnd: HWND,
        dc: HDC,
        info: PAINTSTRUCT,
    }
    impl Paint {
        fn begin(hwnd: HWND) -> Self {
            let mut info = PAINTSTRUCT::default();
            let dc = unsafe { BeginPaint(hwnd, &mut info) };
            Self { hwnd, dc, info }
        }
    }
    impl Drop for Paint {
        fn drop(&mut self) {
            unsafe {
                let _ = EndPaint(self.hwnd, &self.info);
            }
        }
    }
    struct Brush(windows::Win32::Graphics::Gdi::HBRUSH);
    impl Brush {
        fn new(channels: [u8; 3]) -> Self {
            let [r, g, b] = channels;
            Self(unsafe {
                CreateSolidBrush(COLORREF(
                    u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16,
                ))
            })
        }
    }
    impl Drop for Brush {
        fn drop(&mut self) {
            unsafe {
                let _ = DeleteObject(self.0);
            }
        }
    }

    struct WindowState {
        picker: FrozenPicker,
        backing: BackingDib,
        cancel: Arc<AtomicBool>,
        outcome: Option<ColorPickOutcome>,
        input_enabled: bool,
    }
    impl WindowState {
        fn pointer(&mut self) -> Result<(i32, i32), String> {
            let mut point = POINT::default();
            unsafe { GetCursorPos(&mut point) }
                .map_err(|error| format!("Could not read physical pointer: {error}"))?;
            Ok((point.x, point.y))
        }
        fn geometry_unchanged(&self) -> bool {
            let bounds = unsafe {
                ScreenRect::new(
                    GetSystemMetrics(SM_XVIRTUALSCREEN),
                    GetSystemMetrics(SM_YVIRTUALSCREEN),
                    GetSystemMetrics(SM_CXVIRTUALSCREEN) as u32,
                    GetSystemMetrics(SM_CYVIRTUALSCREEN) as u32,
                )
            };
            bounds == self.picker.snapshot().rect()
        }
        fn finish(&mut self, outcome: ColorPickOutcome) {
            if self.outcome.is_none() {
                self.outcome = Some(outcome);
            }
            self.input_enabled = false;
        }
        fn paint(&self, hwnd: HWND) -> Result<(), String> {
            let paint = Paint::begin(hwnd);
            if paint.dc.0.is_null() {
                return Err("Could not begin picker paint".into());
            }
            let bounds = self.picker.snapshot().rect();
            unsafe {
                BitBlt(
                    paint.dc,
                    0,
                    0,
                    bounds.width as i32,
                    bounds.height as i32,
                    self.backing.dc,
                    0,
                    0,
                    SRCCOPY,
                )
            }
            .map_err(|error| format!("Could not paint frozen desktop: {error}"))?;
            if let (Some((x, y)), Some(grid)) = (self.picker.hovered(), self.picker.magnifier()) {
                const CELL: i32 = 12;
                let side = MAGNIFIER_SIDE as i32 * CELL;
                let left = (i64::from(x) + 24)
                    .clamp(0, (i64::from(bounds.width) - i64::from(side) - 2).max(0))
                    as i32;
                let top = (i64::from(y) + 24)
                    .clamp(0, (i64::from(bounds.height) - i64::from(side) - 2).max(0))
                    as i32;
                for (row, pixels) in grid.iter().enumerate() {
                    for (column, pixel) in pixels.iter().enumerate() {
                        let brush = Brush::new(pixel.channels());
                        if brush.0.0.is_null() {
                            return Err("Could not allocate magnifier brush".into());
                        }
                        let rect = RECT {
                            left: left + column as i32 * CELL,
                            top: top + row as i32 * CELL,
                            right: left + (column as i32 + 1) * CELL,
                            bottom: top + (row as i32 + 1) * CELL,
                        };
                        if unsafe { FillRect(paint.dc, &rect, brush.0) } == 0 {
                            return Err("Could not paint magnifier".into());
                        }
                    }
                }
                let white = Brush::new([255; 3]);
                let black = Brush::new([0; 3]);
                for (rect, brush) in [
                    (
                        RECT {
                            left: left - 1,
                            top: top - 1,
                            right: left + side + 1,
                            bottom: top + side + 1,
                        },
                        black.0,
                    ),
                    (
                        RECT {
                            left: left + 4 * CELL,
                            top: top + 4 * CELL,
                            right: left + 5 * CELL,
                            bottom: top + 5 * CELL,
                        },
                        black.0,
                    ),
                    (
                        RECT {
                            left: left + 4 * CELL + 1,
                            top: top + 4 * CELL + 1,
                            right: left + 5 * CELL - 1,
                            bottom: top + 5 * CELL - 1,
                        },
                        white.0,
                    ),
                    (
                        RECT {
                            left: x as i32 - 3,
                            top: y as i32 - 3,
                            right: (x as i32).saturating_add(4),
                            bottom: (y as i32).saturating_add(4),
                        },
                        black.0,
                    ),
                    (
                        RECT {
                            left: x as i32 - 2,
                            top: y as i32 - 2,
                            right: (x as i32).saturating_add(3),
                            bottom: (y as i32).saturating_add(3),
                        },
                        white.0,
                    ),
                ] {
                    if unsafe { FrameRect(paint.dc, &rect, brush) } == 0 {
                        return Err("Could not paint center-pixel marker".into());
                    }
                }
            }
            Ok(())
        }
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_NCCREATE {
            let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            }
            return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
        }
        let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut WindowState;
        if message == WM_NCDESTROY {
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
        }
        if pointer.is_null() {
            return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
        }
        // Default processing can dispatch nested messages (for example,
        // BeginPaint sends WM_ERASEBKGND). Avoid borrowing state for those
        // messages so a reentrant default callback cannot alias its owner.
        if !matches!(
            message,
            WM_PAINT
                | WM_MOUSEMOVE
                | WM_LBUTTONDOWN
                | WM_KEYDOWN
                | WM_CLOSE
                | WM_DISPLAYCHANGE
                | WM_DPICHANGED
                | WM_TIMER
        ) {
            return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
        }
        // Box<WindowState> belongs to the native thread and outlives its HWND.
        // No callback destroys the window; the worker does so after dispatch.
        let state = unsafe { &mut *pointer };
        let handled = catch_unwind(AssertUnwindSafe(|| -> Result<bool, String> {
            match message {
                WM_PAINT => {
                    state.paint(hwnd)?;
                    Ok(true)
                }
                WM_MOUSEMOVE if state.input_enabled => {
                    let point = state.pointer()?;
                    state.picker.hover(point);
                    unsafe {
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                    Ok(true)
                }
                WM_LBUTTONDOWN if state.input_enabled => {
                    let point = state.pointer()?;
                    if let Some(outcome) = state.picker.accept(point) {
                        state.finish(outcome);
                    }
                    Ok(true)
                }
                WM_KEYDOWN if wparam.0 == 0x1b => {
                    state.finish(ColorPickOutcome::Cancelled);
                    Ok(true)
                }
                WM_CLOSE => {
                    state.finish(ColorPickOutcome::Cancelled);
                    Ok(true)
                }
                WM_DISPLAYCHANGE => {
                    state.finish(ColorPickOutcome::Failed(
                        "Display geometry changed; reopen the color picker".into(),
                    ));
                    Ok(true)
                }
                WM_DPICHANGED => {
                    state.finish(ColorPickOutcome::Failed(
                        "Display scaling changed; reopen the color picker".into(),
                    ));
                    Ok(true)
                }
                WM_TIMER => {
                    if state.cancel.load(Ordering::Acquire) {
                        state.finish(ColorPickOutcome::Cancelled);
                    } else if !state.geometry_unchanged() {
                        state.finish(ColorPickOutcome::Failed(
                            "Display geometry changed; reopen the color picker".into(),
                        ));
                    }
                    Ok(true)
                }
                _ => Ok(false),
            }
        }));
        match handled {
            Ok(Ok(true)) => LRESULT(0),
            Ok(Ok(false)) => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
            Ok(Err(error)) => {
                state.finish(ColorPickOutcome::Failed(error));
                LRESULT(0)
            }
            Err(_) => {
                state.finish(ColorPickOutcome::Failed(
                    "Native color picker callback panicked".into(),
                ));
                LRESULT(0)
            }
        }
    }

    struct Window(HWND);
    impl Window {
        fn close(&mut self) -> Result<(), String> {
            if self.0.0.is_null() {
                return Ok(());
            }
            unsafe {
                let _ = KillTimer(self.0, 1);
                let _ = ShowWindow(self.0, SW_HIDE);
                // Disarm callbacks before destroying the surface or releasing
                // its Box/DIB, including exceptional worker unwinding.
                SetWindowLongPtrW(self.0, GWLP_USERDATA, 0);
                if IsWindow(self.0).as_bool() {
                    DestroyWindow(self.0).map_err(|error| {
                        format!("Could not destroy color picker surface: {error}")
                    })?;
                }
            }
            self.0 = HWND::default();
            Ok(())
        }
    }
    impl Drop for Window {
        fn drop(&mut self) {
            let _ = self.close();
        }
    }

    fn run_inner(
        snapshot: Arc<CapturedRegion>,
        cancel: Arc<AtomicBool>,
        ready: Arc<AtomicBool>,
    ) -> Result<ColorPickOutcome, String> {
        if cancel.load(Ordering::Acquire) {
            return Ok(ColorPickOutcome::Cancelled);
        }
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if previous.0.is_null() {
            return Err("Could not establish physical per-monitor picker coordinates".into());
        }
        let _dpi = ThreadDpi(previous);
        let backing = BackingDib::new(&snapshot)?;
        let picker = FrozenPicker::new(snapshot)?;
        let mut state = Box::new(WindowState {
            picker,
            backing,
            cancel,
            outcome: None,
            input_enabled: true,
        });
        if !state.geometry_unchanged() {
            return Err("Display geometry changed before picker startup".into());
        }
        let instance = HINSTANCE(
            unsafe { GetModuleHandleW(None) }
                .map_err(|error| error.to_string())?
                .0,
        );
        let class_name = w!("MultiLauncherFrozenColorPicker");
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class_name,
            hCursor: unsafe { LoadCursorW(None, IDC_CROSS) }.map_err(|error| error.to_string())?,
            ..Default::default()
        };
        static CLASS: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
        CLASS
            .get_or_init(|| {
                if unsafe { RegisterClassW(&class) } == 0 {
                    Err("Could not register color picker window class".into())
                } else {
                    Ok(())
                }
            })
            .clone()?;
        let bounds = state.picker.snapshot().rect();
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                class_name,
                PCWSTR::null(),
                WS_POPUP,
                bounds.x,
                bounds.y,
                bounds.width as i32,
                bounds.height as i32,
                None,
                None,
                instance,
                Some((&mut *state as *mut WindowState).cast::<c_void>()),
            )
        }
        .map_err(|error| format!("Could not create color picker window: {error}"))?;
        let mut window = Window(hwnd);
        if unsafe { SetTimer(hwnd, 1, 33, None) } == 0 {
            return Err("Could not start picker cancellation timer".into());
        }
        if state.cancel.load(Ordering::Acquire) {
            state.input_enabled = false;
            window.close()?;
            return Ok(ColorPickOutcome::Cancelled);
        }
        if let Some(outcome) = state.outcome.take() {
            state.input_enabled = false;
            window.close()?;
            return Ok(outcome);
        }
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                bounds.x,
                bounds.y,
                bounds.width as i32,
                bounds.height as i32,
                SWP_SHOWWINDOW,
            )
            .map_err(|error| format!("Could not present color picker: {error}"))?;
            if !SetForegroundWindow(hwnd).as_bool() {
                return Err("Could not activate color picker for keyboard input".into());
            }
        }
        let pointer = state.pointer()?;
        state.picker.hover(pointer);
        ready.store(true, Ordering::Release);
        unsafe {
            let _ = InvalidateRect(hwnd, None, false);
        }
        while state.outcome.is_none() {
            let mut message = MSG::default();
            let status = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
            if status == -1 {
                return Err("Color picker message loop failed".into());
            }
            if status == 0 {
                state.finish(ColorPickOutcome::Cancelled);
                break;
            }
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        state.input_enabled = false;
        window.close()?; // Destroy HWND before state/backing or outcome publication.
        Ok(state.outcome.take().unwrap_or(ColorPickOutcome::Cancelled))
    }
    pub(super) fn run(
        snapshot: Arc<CapturedRegion>,
        cancel: Arc<AtomicBool>,
        ready: Arc<AtomicBool>,
    ) -> ColorPickOutcome {
        run_inner(snapshot, cancel, ready).unwrap_or_else(ColorPickOutcome::Failed)
    }
}
