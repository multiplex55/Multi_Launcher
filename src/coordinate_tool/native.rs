//! Passive Windows sampler and layered surfaces for the coordinate inspector.
//!
//! All HWND and GDI ownership remains on the single controller worker. The
//! backend draws the compact HUD, crosshair and narrow virtual-desktop guides,
//! and presents lazily owned live magnification effects on that same worker.

use super::controller::{CoordinateRuntimeFactory, CoordinateSampler, CoordinateSurfaceBackend};

/// Per-monitor-aware click-time sampler shared by the capture hook callback.
/// Its DPI context is installed on construction and restored when dropped.
pub(crate) struct NativeCoordinatePointSampler {
    #[cfg(windows)]
    sampler: windows_runtime::WindowsSampler,
}

impl NativeCoordinatePointSampler {
    pub(crate) fn new() -> Result<Self, String> {
        #[cfg(windows)]
        {
            return Ok(Self {
                sampler: windows_runtime::WindowsSampler::new()?,
            });
        }
        #[cfg(not(windows))]
        {
            Err("Coordinate point sampling is available only on Windows".into())
        }
    }

    pub(crate) fn sample_at(
        &mut self,
        point: super::model::PhysicalPoint,
    ) -> Result<super::model::CoordinateSample, String> {
        #[cfg(windows)]
        {
            self.sampler.sample_at(point)
        }
        #[cfg(not(windows))]
        {
            let _ = point;
            Err("Coordinate point sampling is available only on Windows".into())
        }
    }
}

#[derive(Default)]
pub struct NativeCoordinateRuntimeFactory;

impl CoordinateRuntimeFactory for NativeCoordinateRuntimeFactory {
    fn create_sampler(&self) -> Result<Box<dyn CoordinateSampler>, String> {
        #[cfg(windows)]
        {
            return Ok(Box::new(windows_runtime::WindowsSampler::new()?));
        }
        #[cfg(not(windows))]
        {
            Err("The coordinate inspector passive runtime is available only on Windows".into())
        }
    }

    fn create_backend(&self) -> Result<Box<dyn CoordinateSurfaceBackend>, String> {
        #[cfg(windows)]
        {
            return Ok(Box::new(windows_runtime::WindowsSurfaceBackend::new()?));
        }
        #[cfg(not(windows))]
        {
            Err("The coordinate inspector passive runtime is available only on Windows".into())
        }
    }
}

#[cfg(windows)]
mod windows_runtime {
    use std::mem;
    use std::ptr;
    use std::sync::OnceLock;

    use image::RgbaImage;
    use windows::Win32::Foundation::{
        BOOL, COLORREF, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
    };
    use windows::Win32::Graphics::Gdi::{
        AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
        CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, ClientToScreen, CreateCompatibleDC,
        CreateDIBSection, CreateEllipticRgn, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET,
        DEFAULT_PITCH, DIB_RGB_COLORS, DeleteDC, DeleteObject, FW_NORMAL, FillRect,
        GetMonitorInfoW, HBITMAP, HDC, HGDIOBJ, InvalidateRect, MONITOR_DEFAULTTONEAREST,
        MONITORINFOEXW, MonitorFromPoint, OUT_DEFAULT_PRECIS, SelectObject, SetBkMode,
        SetTextColor, SetWindowRgn, TRANSPARENT, TextOutW,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor,
        MDT_EFFECTIVE_DPI, SetThreadDpiAwarenessContext,
    };
    use windows::Win32::UI::Magnification::{
        MAGCOLOREFFECT, MAGTRANSFORM, MW_FILTERMODE_EXCLUDE, MagInitialize, MagSetColorEffect,
        MagSetWindowFilterList, MagSetWindowSource, MagSetWindowTransform, MagUninitialize,
        WC_MAGNIFIER,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
        GetCursorPos, GetForegroundWindow, GetSystemMetrics, GetWindowThreadProcessId, HMENU,
        HWND_TOPMOST, IsWindowVisible, LWA_ALPHA, MA_NOACTIVATE, MSG, PM_REMOVE, PeekMessageW,
        PostMessageW, RegisterClassW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SetLayeredWindowAttributes, SetWindowPos,
        ShowWindow, TranslateMessage, ULW_ALPHA, UpdateLayeredWindow, WM_APP, WM_DISPLAYCHANGE,
        WM_DPICHANGED, WM_MOUSEACTIVATE, WM_NCHITTEST, WNDCLASSW, WS_CHILD, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    };
    use windows::core::{PCWSTR, w};

    use super::super::controller::{
        CoordinateRenderFrame, CoordinateSampler, CoordinateSurfaceBackend,
    };
    use super::super::model::{
        CoordinateEffectsStatus, CoordinateSample, CursorEffectStatus, ForegroundClientGeometry,
        MonitorGeometry, MonitorId, PhysicalPoint, PhysicalRect, PhysicalSize,
    };
    use super::super::native_effects::{
        CursorEffectsRuntime, EffectConfiguration, EffectKind, EffectNativeOperations,
        EffectRequests,
    };
    use super::super::render::{
        GuideOrientation, HaloColorTransform, crosshair_bitmap, guide_bitmap, guide_geometry,
        halo_geometry, halo_outline_bitmap, hud_font_size, hud_layout, hud_lines,
    };
    use super::super::settings::{CrosshairPreferences, HaloPreferences};
    use crate::platform::pixels::premultiplied_bgra;

    struct ThreadDpiContext(DPI_AWARENESS_CONTEXT);

    impl ThreadDpiContext {
        fn per_monitor_v2() -> Result<Self, String> {
            let previous =
                unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
            if previous.0.is_null() {
                return Err("Could not establish physical per-monitor coordinates".into());
            }
            Ok(Self(previous))
        }
    }

    impl Drop for ThreadDpiContext {
        fn drop(&mut self) {
            unsafe {
                SetThreadDpiAwarenessContext(self.0);
            }
        }
    }

    pub(super) struct WindowsSampler {
        _dpi_context: ThreadDpiContext,
    }

    impl WindowsSampler {
        pub(super) fn new() -> Result<Self, String> {
            Ok(Self {
                _dpi_context: ThreadDpiContext::per_monitor_v2()?,
            })
        }

        pub(super) fn sample_at(
            &mut self,
            desktop_point: super::super::model::PhysicalPoint,
        ) -> Result<CoordinateSample, String> {
            let point = POINT {
                x: desktop_point.x,
                y: desktop_point.y,
            };
            Ok(CoordinateSample::new(
                desktop_point,
                virtual_desktop_bounds(),
                monitor_geometry(point),
                foreground_client_geometry(),
            ))
        }
    }

    impl CoordinateSampler for WindowsSampler {
        fn sample(&mut self) -> Result<CoordinateSample, String> {
            let mut point = POINT::default();
            unsafe { GetCursorPos(&mut point) }
                .map_err(|error| format!("Could not sample the physical cursor: {error}"))?;
            self.sample_at(PhysicalPoint::new(point.x, point.y))
        }
    }

    fn virtual_desktop_bounds() -> Option<PhysicalRect> {
        let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
        let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
        if width <= 0 || height <= 0 {
            return None;
        }
        let right = left.checked_add(width)?;
        let bottom = top.checked_add(height)?;
        PhysicalRect::new(left, top, right, bottom)
    }

    fn monitor_geometry(point: POINT) -> Option<MonitorGeometry> {
        let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
        if monitor.0.is_null() {
            return None;
        }

        let mut info = MONITORINFOEXW {
            monitorInfo: windows::Win32::Graphics::Gdi::MONITORINFO {
                cbSize: mem::size_of::<MONITORINFOEXW>() as u32,
                ..Default::default()
            },
            ..Default::default()
        };
        if !unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo) }.as_bool() {
            return None;
        }
        let bounds = physical_rect(info.monitorInfo.rcMonitor)?;
        let work_area = physical_rect(info.monitorInfo.rcWork)?;
        let end = info
            .szDevice
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(info.szDevice.len());
        let mut device_name = String::from_utf16_lossy(&info.szDevice[..end]);
        if device_name.is_empty() {
            device_name = format!("monitor:{:x}", monitor.0 as usize);
        }

        let mut dpi_x = 0;
        let mut dpi_y = 0;
        let effective_dpi =
            unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }
                .ok()
                .filter(|_| dpi_x > 0 && dpi_y > 0)
                .map(|_| (dpi_x, dpi_y));

        Some(MonitorGeometry {
            id: MonitorId::new(device_name),
            bounds,
            work_area,
            effective_dpi,
        })
    }

    fn physical_rect(rect: RECT) -> Option<PhysicalRect> {
        PhysicalRect::new(rect.left, rect.top, rect.right, rect.bottom)
    }

    fn foreground_client_geometry() -> Option<ForegroundClientGeometry> {
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.0.is_null() {
            return None;
        }
        let mut foreground_pid = 0;
        unsafe { GetWindowThreadProcessId(foreground, Some(&mut foreground_pid)) };
        let target = if foreground_pid == std::process::id() {
            let hwnd = crate::active_window::resolve_previous_active_window().ok()?;
            HWND(hwnd as *mut _)
        } else {
            foreground
        };

        let mut client = RECT::default();
        unsafe { GetClientRect(target, &mut client) }.ok()?;
        let mut origin = POINT {
            x: client.left,
            y: client.top,
        };
        let mut bottom_right = POINT {
            x: client.right,
            y: client.bottom,
        };
        if !unsafe { ClientToScreen(target, &mut origin) }.as_bool()
            || !unsafe { ClientToScreen(target, &mut bottom_right) }.as_bool()
        {
            return None;
        }
        let bounds = PhysicalRect::new(origin.x, origin.y, bottom_right.x, bottom_right.y);
        Some(ForegroundClientGeometry::new(
            PhysicalPoint::new(origin.x, origin.y),
            bounds,
        ))
    }

    const WM_COORDINATE_TOPOLOGY_INVALIDATED: u32 = WM_APP + 0x3A1;

    fn post_topology_invalidation(hwnd: HWND) {
        if let Err(error) = unsafe {
            PostMessageW(
                hwnd,
                WM_COORDINATE_TOPOLOGY_INVALIDATED,
                WPARAM(0),
                LPARAM(0),
            )
        } {
            eprintln!("Could not queue coordinate topology refresh: {error}");
        }
    }

    unsafe extern "system" fn passive_window_proc(
        hwnd: HWND,
        message: u32,
        wparam: windows::Win32::Foundation::WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_NCHITTEST => {
                LRESULT(windows::Win32::UI::WindowsAndMessaging::HTTRANSPARENT as isize)
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED => {
                // These are sent directly to each top-level window, so they do
                // not necessarily appear as MSG values in the worker's queue.
                post_topology_invalidation(hwnd);
                LRESULT(0)
            }
            WM_COORDINATE_TOPOLOGY_INVALIDATED => LRESULT(0),
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    fn register_surface_class() -> Result<HINSTANCE, String> {
        static REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();
        let instance = HINSTANCE(
            unsafe { GetModuleHandleW(None) }
                .map_err(|error| format!("Could not load the application module: {error}"))?
                .0,
        );
        REGISTERED
            .get_or_init(|| {
                let class = WNDCLASSW {
                    lpfnWndProc: Some(passive_window_proc),
                    hInstance: instance,
                    lpszClassName: w!("MultiLauncherCoordinatePassiveSurface"),
                    ..Default::default()
                };
                if unsafe { RegisterClassW(&class) } == 0 {
                    Err("Could not register coordinate passive-surface window class".into())
                } else {
                    Ok(())
                }
            })
            .clone()?;
        Ok(instance)
    }

    struct LayeredDib {
        dc: HDC,
        bitmap: HBITMAP,
        original_bitmap: HGDIOBJ,
        bits: *mut u8,
        width: u32,
        height: u32,
        byte_len: usize,
    }

    impl LayeredDib {
        fn new(width: u32, height: u32) -> Result<Self, String> {
            let width_i32 = i32::try_from(width).map_err(|_| "Surface width is too large")?;
            let height_i32 = i32::try_from(height).map_err(|_| "Surface height is too large")?;
            if width == 0 || height == 0 {
                return Err("Surface dimensions must be non-empty".into());
            }
            let byte_len = (width as usize)
                .checked_mul(height as usize)
                .and_then(|pixels| pixels.checked_mul(4))
                .ok_or("Surface allocation is too large")?;
            let dc = unsafe { CreateCompatibleDC(None) };
            if dc.0.is_null() {
                return Err("Could not allocate coordinate surface DC".into());
            }
            let bitmap_info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width_i32,
                    biHeight: -height_i32,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                bmiColors: [Default::default()],
            };
            let mut bits = ptr::null_mut();
            let bitmap = match unsafe {
                CreateDIBSection(dc, &bitmap_info, DIB_RGB_COLORS, &mut bits, None, 0)
            } {
                Ok(bitmap) if !bits.is_null() => bitmap,
                Ok(bitmap) => {
                    unsafe {
                        let _ = DeleteObject(bitmap);
                        let _ = DeleteDC(dc);
                    }
                    return Err("Coordinate surface DIB has no pixel storage".into());
                }
                Err(error) => {
                    unsafe {
                        let _ = DeleteDC(dc);
                    }
                    return Err(format!(
                        "Could not allocate coordinate surface DIB: {error}"
                    ));
                }
            };
            let original_bitmap = unsafe { SelectObject(dc, bitmap) };
            if original_bitmap.0.is_null() || original_bitmap.0 as isize == -1 {
                unsafe {
                    let _ = DeleteObject(bitmap);
                    let _ = DeleteDC(dc);
                }
                return Err("Could not select coordinate surface DIB".into());
            }
            unsafe { ptr::write_bytes(bits.cast::<u8>(), 0, byte_len) };
            Ok(Self {
                dc,
                bitmap,
                original_bitmap,
                bits: bits.cast(),
                width,
                height,
                byte_len,
            })
        }

        fn copy_rgba(&mut self, image: &RgbaImage) -> Result<(), String> {
            if image.dimensions() != (self.width, self.height) {
                return Err("Coordinate surface image dimensions changed unexpectedly".into());
            }
            let output = unsafe { std::slice::from_raw_parts_mut(self.bits, self.byte_len) };
            premultiplied_bgra(image, output).map_err(str::to_string)
        }

        fn draw_hud(
            &mut self,
            width: u32,
            height: u32,
            font_size: u32,
            lines: &[String],
        ) -> Result<(), String> {
            if (width, height) != (self.width, self.height) {
                return Err("HUD dimensions do not match its backing DIB".into());
            }
            unsafe { ptr::write_bytes(self.bits, 0, self.byte_len) };
            let brush = unsafe { CreateSolidBrush(COLORREF(0x001B_1B_1B)) };
            if brush.0.is_null() {
                return Err("Could not create coordinate HUD background".into());
            }
            let bounds = RECT {
                left: 0,
                top: 0,
                right: i32::try_from(width).map_err(|_| "HUD width is too large")?,
                bottom: i32::try_from(height).map_err(|_| "HUD height is too large")?,
            };
            let filled = unsafe { FillRect(self.dc, &bounds, brush) };
            let _ = unsafe { DeleteObject(brush) };
            if filled == 0 {
                return Err("Could not paint coordinate HUD background".into());
            }

            let font = unsafe {
                CreateFontW(
                    i32::try_from(font_size).unwrap_or(14),
                    0,
                    0,
                    0,
                    FW_NORMAL.0 as i32,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET.0 as u32,
                    OUT_DEFAULT_PRECIS.0 as u32,
                    CLIP_DEFAULT_PRECIS.0 as u32,
                    CLEARTYPE_QUALITY.0 as u32,
                    DEFAULT_PITCH.0 as u32,
                    w!("Segoe UI"),
                )
            };
            if font.0.is_null() {
                return Err("Could not create coordinate HUD font".into());
            }
            let original_font = unsafe { SelectObject(self.dc, font) };
            if original_font.0.is_null() || original_font.0 as isize == -1 {
                let _ = unsafe { DeleteObject(font) };
                return Err("Could not select coordinate HUD font".into());
            }
            let result = (|| {
                unsafe { SetBkMode(self.dc, TRANSPARENT) };
                unsafe { SetTextColor(self.dc, COLORREF(0x00F0_F0F0)) };
                let mut y = 9_i32;
                let line_height = i32::try_from(font_size).unwrap_or(14).saturating_add(5);
                for line in lines {
                    let wide: Vec<u16> = line.encode_utf16().collect();
                    if !wide.is_empty() && !unsafe { TextOutW(self.dc, 10, y, &wide) }.as_bool() {
                        return Err("Could not paint coordinate HUD text".to_string());
                    }
                    y = y.saturating_add(line_height);
                }
                Ok(())
            })();
            unsafe {
                let _ = SelectObject(self.dc, original_font);
                let _ = DeleteObject(font);
            }
            result?;

            // GDI text rendering updates BGR but leaves the reserved alpha byte
            // undefined. The HUD is an intentionally opaque dark panel.
            for pixel in unsafe { std::slice::from_raw_parts_mut(self.bits, self.byte_len) }
                .chunks_exact_mut(4)
            {
                pixel[3] = 255;
            }
            Ok(())
        }
    }

    impl Drop for LayeredDib {
        fn drop(&mut self) {
            unsafe {
                let _ = SelectObject(self.dc, self.original_bitmap);
                let _ = DeleteObject(self.bitmap);
                let _ = DeleteDC(self.dc);
            }
        }
    }

    struct LayeredSurface {
        hwnd: HWND,
        dib: Option<LayeredDib>,
        position: Option<PhysicalPoint>,
        visible: bool,
        dirty: bool,
    }

    impl LayeredSurface {
        fn new(instance: HINSTANCE) -> Result<Self, String> {
            let extended_style = WS_EX_LAYERED
                | WS_EX_TRANSPARENT
                | WS_EX_TOOLWINDOW
                | WS_EX_TOPMOST
                | WS_EX_NOACTIVATE;
            let hwnd = unsafe {
                CreateWindowExW(
                    extended_style,
                    w!("MultiLauncherCoordinatePassiveSurface"),
                    PCWSTR::null(),
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    None,
                    None,
                    instance,
                    None,
                )
            }
            .map_err(|error| format!("Could not create coordinate passive surface: {error}"))?;
            Ok(Self {
                hwnd,
                dib: None,
                position: None,
                visible: false,
                dirty: true,
            })
        }

        fn ensure_dib(&mut self, width: u32, height: u32) -> Result<&mut LayeredDib, String> {
            let replace = self
                .dib
                .as_ref()
                .is_none_or(|dib| (dib.width, dib.height) != (width, height));
            if replace {
                self.dirty = true;
                self.dib = Some(LayeredDib::new(width, height)?);
            }
            self.dib
                .as_mut()
                .ok_or_else(|| "Coordinate surface DIB was not initialized".into())
        }

        fn upload(&mut self, origin: PhysicalPoint) -> Result<(), String> {
            let dib = self
                .dib
                .as_ref()
                .ok_or_else(|| "Coordinate surface has no backing DIB".to_string())?;
            let position = POINT {
                x: origin.x,
                y: origin.y,
            };
            let size = SIZE {
                cx: i32::try_from(dib.width).map_err(|_| "Surface width is too large")?,
                cy: i32::try_from(dib.height).map_err(|_| "Surface height is too large")?,
            };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            unsafe {
                UpdateLayeredWindow(
                    self.hwnd,
                    None,
                    Some(&position),
                    Some(&size),
                    dib.dc,
                    Some(&POINT::default()),
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                )
            }
            .map_err(|error| format!("Could not update coordinate surface: {error}"))?;
            self.position = Some(origin);
            self.dirty = false;
            Ok(())
        }

        fn present_image(
            &mut self,
            origin: PhysicalPoint,
            image: &RgbaImage,
        ) -> Result<(), String> {
            self.prepare_image(origin, image)?;
            self.show()
        }

        fn prepare_image(
            &mut self,
            origin: PhysicalPoint,
            image: &RgbaImage,
        ) -> Result<(), String> {
            let (width, height) = image.dimensions();
            self.ensure_dib(width, height)?.copy_rgba(image)?;
            self.dirty = true;
            self.upload(origin)
        }

        fn present_hud(
            &mut self,
            origin: PhysicalPoint,
            width: u32,
            height: u32,
            font_size: u32,
            lines: &[String],
        ) -> Result<(), String> {
            self.ensure_dib(width, height)?;
            self.dirty = true;
            self.dib
                .as_mut()
                .ok_or_else(|| "Coordinate HUD backing DIB was not initialized".to_string())?
                .draw_hud(width, height, font_size, lines)?;
            self.upload(origin)?;
            self.show()
        }

        fn reposition_and_show(&mut self, origin: PhysicalPoint) -> Result<(), String> {
            if self.position != Some(origin) {
                unsafe {
                    SetWindowPos(
                        self.hwnd,
                        HWND_TOPMOST,
                        origin.x,
                        origin.y,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOSIZE,
                    )
                }
                .map_err(|error| format!("Could not move coordinate passive surface: {error}"))?;
                self.position = Some(origin);
            }
            self.show()
        }

        fn reposition_without_raising(&mut self, origin: PhysicalPoint) -> Result<(), String> {
            if self.position != Some(origin) {
                unsafe {
                    SetWindowPos(
                        self.hwnd,
                        HWND::default(),
                        origin.x,
                        origin.y,
                        0,
                        0,
                        SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOSIZE,
                    )
                }
                .map_err(|error| format!("Could not move coordinate passive surface: {error}"))?;
                self.position = Some(origin);
            }
            Ok(())
        }

        fn raise_topmost(&mut self) -> Result<(), String> {
            unsafe {
                SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                )
            }
            .map_err(|error| format!("Could not order coordinate passive surface: {error}"))
        }

        fn show(&mut self) -> Result<(), String> {
            if !self.visible {
                // Raise only on a hidden-to-visible transition. This keeps
                // newly re-enabled cheap surfaces above an active halo while
                // avoiding per-frame z-order churn for stationary windows.
                self.raise_topmost()?;
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                }
                if !unsafe { IsWindowVisible(self.hwnd) }.as_bool() {
                    return Err("Coordinate passive surface did not become visible".into());
                }
                self.visible = true;
            }
            Ok(())
        }

        fn hide(&mut self) {
            if self.visible {
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                }
                self.visible = false;
            }
        }

        fn needs_upload(&self) -> bool {
            self.dirty || self.dib.is_none()
        }

        fn mark_dirty(&mut self) {
            self.dirty = true;
        }

        fn is_visible(&self) -> bool {
            unsafe { IsWindowVisible(self.hwnd) }.as_bool()
        }

        fn shutdown(&mut self) -> Result<(), String> {
            self.hide();
            if !self.hwnd.0.is_null() {
                unsafe { DestroyWindow(self.hwnd) }
                    .map_err(|error| format!("Could not destroy coordinate surface: {error}"))?;
                self.hwnd = HWND::default();
            }
            self.dib.take();
            self.position = None;
            Ok(())
        }
    }

    impl Drop for LayeredSurface {
        fn drop(&mut self) {
            let _ = self.shutdown();
        }
    }

    const EFFECT_HOST_CLASS: windows::core::PCWSTR = w!("MultiLauncherCoordinateEffectPassiveHost");

    unsafe extern "system" fn effect_host_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_NCHITTEST => {
                LRESULT(windows::Win32::UI::WindowsAndMessaging::HTTRANSPARENT as isize)
            }
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_DISPLAYCHANGE | WM_DPICHANGED => {
                post_topology_invalidation(hwnd);
                LRESULT(0)
            }
            WM_COORDINATE_TOPOLOGY_INVALIDATED => LRESULT(0),
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    fn register_effect_host_class(instance: HINSTANCE) -> Result<(), String> {
        static REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();
        REGISTERED
            .get_or_init(|| {
                let class = WNDCLASSW {
                    lpfnWndProc: Some(effect_host_proc),
                    hInstance: instance,
                    lpszClassName: EFFECT_HOST_CLASS,
                    ..Default::default()
                };
                if unsafe { RegisterClassW(&class) } == 0 {
                    Err(format!(
                        "Could not register cursor-effect passive host class (GetLastError={})",
                        unsafe { GetLastError().0 }
                    ))
                } else {
                    Ok(())
                }
            })
            .clone()
    }

    struct NativeEffectSurface {
        host: HWND,
        magnifier: HWND,
        diameter: i32,
        scale: f32,
        position: Option<PhysicalPoint>,
    }

    /// Worker-thread owner for the process' single Magnification session and
    /// its independent hidden effect hosts. The host is retained immediately
    /// after creation so later setup errors can still be cleaned by reconcile.
    struct WindowsEffectOperations {
        instance: HINSTANCE,
        session_initialized: bool,
        halo: Option<NativeEffectSurface>,
        zoom: Option<NativeEffectSurface>,
        halo_outline: Option<LayeredSurface>,
        halo_outline_image: Option<RgbaImage>,
        halo_outline_preferences: Option<HaloPreferences>,
    }

    impl WindowsEffectOperations {
        fn new(instance: HINSTANCE) -> Self {
            Self {
                instance,
                session_initialized: false,
                halo: None,
                zoom: None,
                halo_outline: None,
                halo_outline_image: None,
                halo_outline_preferences: None,
            }
        }

        fn surface(&self, kind: EffectKind) -> Option<&NativeEffectSurface> {
            match kind {
                EffectKind::Halo => self.halo.as_ref(),
                EffectKind::Zoom => self.zoom.as_ref(),
            }
        }

        fn surface_mut(&mut self, kind: EffectKind) -> &mut Option<NativeEffectSurface> {
            match kind {
                EffectKind::Halo => &mut self.halo,
                EffectKind::Zoom => &mut self.zoom,
            }
        }

        fn dimensions_and_scale(configuration: &EffectConfiguration) -> (i32, f32) {
            match configuration {
                EffectConfiguration::Halo(preferences) => {
                    (preferences.radius.clamp(8, 256) * 2, 1.0)
                }
                EffectConfiguration::Zoom(preferences) => {
                    let factor = if preferences.zoom_factor.is_finite() {
                        preferences.zoom_factor.clamp(1.25, 4.0)
                    } else {
                        2.0
                    };
                    (preferences.diameter.clamp(64, 480), factor)
                }
            }
        }

        fn color_identity() -> MAGCOLOREFFECT {
            MAGCOLOREFFECT {
                transform: [
                    1.0, 0.0, 0.0, 0.0, 0.0, // output red = input red
                    0.0, 1.0, 0.0, 0.0, 0.0, // output green = input green
                    0.0, 0.0, 1.0, 0.0, 0.0, // output blue = input blue
                    0.0, 0.0, 0.0, 1.0, 0.0, // preserve alpha
                    0.0, 0.0, 0.0, 0.0, 1.0, // no additive color
                ],
            }
        }

        fn set_color_effect(magnifier: HWND, mut effect: MAGCOLOREFFECT) -> Result<(), String> {
            if !unsafe { MagSetColorEffect(magnifier, &mut effect) }.as_bool() {
                return Err(format!(
                    "MagSetColorEffect failed (GetLastError={})",
                    unsafe { GetLastError().0 }
                ));
            }
            Ok(())
        }

        fn set_halo_color(magnifier: HWND, strength: f32) -> Result<(), String> {
            let effect = MAGCOLOREFFECT {
                transform: HaloColorTransform::from_strength(strength).matrix(),
            };
            Self::set_color_effect(magnifier, effect)
        }

        fn set_identity_color(magnifier: HWND) -> Result<(), String> {
            Self::set_color_effect(magnifier, Self::color_identity())
        }

        fn configure_outline_surface(
            &mut self,
            preferences: HaloPreferences,
        ) -> Result<(), String> {
            let preferences = preferences.normalized();
            if self.halo_outline_preferences == Some(preferences) {
                return Ok(());
            }

            if preferences.outline_enabled {
                let image = halo_outline_bitmap(preferences)
                    .ok_or_else(|| "Could not create halo outline bitmap".to_string())?;
                if self.halo_outline.is_none() {
                    self.halo_outline = Some(LayeredSurface::new(self.instance)?);
                }
                self.halo_outline_image = Some(image);
                if let Some(outline) = self.halo_outline.as_mut() {
                    outline.mark_dirty();
                }
            } else {
                self.hide_halo_outline()?;
                self.halo_outline_image = None;
            }

            self.halo_outline_preferences = Some(preferences);
            Ok(())
        }

        fn hide_halo_outline(&mut self) -> Result<(), String> {
            let Some(outline) = self.halo_outline.as_mut() else {
                return Ok(());
            };
            outline.hide();
            if outline.is_visible() {
                return Err("Halo outline remained visible after SW_HIDE".into());
            }
            Ok(())
        }

        fn present_halo_outline(&mut self, origin: PhysicalPoint) -> Result<(), String> {
            let Some(image) = self.halo_outline_image.as_ref() else {
                return self.hide_halo_outline();
            };
            let outline = self
                .halo_outline
                .as_mut()
                .ok_or_else(|| "Halo outline window is missing".to_string())?;
            if outline.needs_upload() {
                outline.prepare_image(origin, image)?;
            } else {
                outline.reposition_without_raising(origin)?;
            }
            outline.show()
        }

        fn update_live_source(
            &mut self,
            kind: EffectKind,
            current_point: PhysicalPoint,
        ) -> Result<Option<PhysicalPoint>, String> {
            let (magnifier, host, diameter, scale, previous_position) = self
                .surface(kind)
                .map(|surface| {
                    (
                        surface.magnifier,
                        surface.host,
                        surface.diameter,
                        surface.scale,
                        surface.position,
                    )
                })
                .ok_or_else(|| format!("{} effect host is missing", kind.label()))?;
            if magnifier.0.is_null() {
                return Err(format!("{} magnifier child is missing", kind.label()));
            }

            let (source, origin) = if kind == EffectKind::Halo {
                let geometry = halo_geometry(current_point, diameter / 2)
                    .ok_or_else(|| "Halo geometry overflows physical coordinates".to_string())?;
                (
                    RECT {
                        left: geometry.source.left(),
                        top: geometry.source.top(),
                        right: geometry.source.right(),
                        bottom: geometry.source.bottom(),
                    },
                    Some(geometry.origin),
                )
            } else {
                let source_size = ((diameter as f32) / scale)
                    .round()
                    .clamp(1.0, diameter as f32) as i32;
                let half = source_size / 2;
                let left = current_point
                    .x
                    .checked_sub(half)
                    .ok_or_else(|| format!("{} source x coordinate overflowed", kind.label()))?;
                let top = current_point
                    .y
                    .checked_sub(half)
                    .ok_or_else(|| format!("{} source y coordinate overflowed", kind.label()))?;
                (
                    RECT {
                        left,
                        top,
                        right: left.checked_add(source_size).ok_or_else(|| {
                            format!("{} source right coordinate overflowed", kind.label())
                        })?,
                        bottom: top.checked_add(source_size).ok_or_else(|| {
                            format!("{} source bottom coordinate overflowed", kind.label())
                        })?,
                    },
                    None,
                )
            };

            if !unsafe { MagSetWindowSource(magnifier, source) }.as_bool() {
                return Err(format!(
                    "MagSetWindowSource({}) failed (GetLastError={})",
                    kind.label(),
                    unsafe { GetLastError().0 }
                ));
            }
            if !unsafe { InvalidateRect(magnifier, None, BOOL(0)) }.as_bool() {
                return Err(format!(
                    "InvalidateRect({}) failed (GetLastError={})",
                    kind.label(),
                    unsafe { GetLastError().0 }
                ));
            }

            if let Some(origin) = origin {
                if previous_position != Some(origin) {
                    unsafe {
                        SetWindowPos(
                            host,
                            HWND::default(),
                            origin.x,
                            origin.y,
                            diameter,
                            diameter,
                            SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                        )
                    }
                    .map_err(|error| format!("Could not position halo host: {error}"))?;
                    if let Some(surface) = self.surface_mut(kind).as_mut() {
                        surface.position = Some(origin);
                    }
                }
            }
            Ok(origin)
        }

        fn show_halo_host(&self) -> Result<(), String> {
            let (host, magnifier) = self
                .surface(EffectKind::Halo)
                .map(|surface| (surface.host, surface.magnifier))
                .ok_or_else(|| "Halo host is missing".to_string())?;
            if magnifier.0.is_null() {
                return Err("Halo magnifier child is missing".into());
            }
            if !unsafe { IsWindowVisible(magnifier) }.as_bool() {
                unsafe {
                    let _ = ShowWindow(magnifier, SW_SHOWNOACTIVATE);
                }
            }
            if !unsafe { IsWindowVisible(host) }.as_bool() {
                unsafe {
                    let _ = ShowWindow(host, SW_SHOWNOACTIVATE);
                }
            }
            if !unsafe { IsWindowVisible(host) }.as_bool()
                || !unsafe { IsWindowVisible(magnifier) }.as_bool()
            {
                return Err("Halo host and magnifier child did not become visible".into());
            }
            Ok(())
        }

        fn set_circle_region(host: HWND, diameter: i32) -> Result<(), String> {
            let region = unsafe { CreateEllipticRgn(0, 0, diameter, diameter) };
            if region.0.is_null() {
                return Err(format!(
                    "CreateEllipticRgn failed (GetLastError={})",
                    unsafe { GetLastError().0 }
                ));
            }
            if unsafe { SetWindowRgn(host, region, BOOL(1)) } == 0 {
                let error = unsafe { GetLastError().0 };
                if !unsafe { DeleteObject(region) }.as_bool() {
                    return Err(format!(
                        "SetWindowRgn failed (GetLastError={error}); DeleteObject(region) also failed (GetLastError={})",
                        unsafe { GetLastError().0 }
                    ));
                }
                return Err(format!("SetWindowRgn failed (GetLastError={error})"));
            }
            // A successful SetWindowRgn transfers the region to USER32.
            Ok(())
        }

        fn set_transform(magnifier: HWND, scale: f32) -> Result<(), String> {
            let mut transform = MAGTRANSFORM {
                v: [scale, 0.0, 0.0, 0.0, scale, 0.0, 0.0, 0.0, 1.0],
            };
            if !unsafe { MagSetWindowTransform(magnifier, &mut transform) }.as_bool() {
                return Err(format!(
                    "MagSetWindowTransform failed (GetLastError={})",
                    unsafe { GetLastError().0 }
                ));
            }
            Ok(())
        }

        fn create_host(&mut self, kind: EffectKind, diameter: i32) -> Result<(), String> {
            if !self.session_initialized {
                return Err("Magnification session is not initialized".into());
            }
            if self.surface(kind).is_some() {
                return Err(format!("{} effect host already exists", kind.label()));
            }
            register_effect_host_class(self.instance)?;
            let extended_style = WS_EX_LAYERED
                | WS_EX_TRANSPARENT
                | WS_EX_NOACTIVATE
                | WS_EX_TOOLWINDOW
                | WS_EX_TOPMOST;
            let host = unsafe {
                CreateWindowExW(
                    extended_style,
                    EFFECT_HOST_CLASS,
                    windows::core::PCWSTR::null(),
                    WS_POPUP,
                    -diameter - 16,
                    -diameter - 16,
                    diameter,
                    diameter,
                    None,
                    None,
                    self.instance,
                    None,
                )
            }
            .map_err(|error| format!("Could not create {} effect host: {error}", kind.label()))?;

            *self.surface_mut(kind) = Some(NativeEffectSurface {
                host,
                magnifier: HWND::default(),
                diameter,
                scale: 1.0,
                position: None,
            });
            unsafe { SetLayeredWindowAttributes(host, COLORREF(0), 255, LWA_ALPHA) }.map_err(
                |error| format!("Could not configure {} host alpha: {error}", kind.label()),
            )?;
            Self::set_circle_region(host, diameter)
                .map_err(|error| format!("Could not shape {} host: {error}", kind.label()))
        }

        fn create_magnifier_child(
            &mut self,
            kind: EffectKind,
            configuration: &EffectConfiguration,
        ) -> Result<(), String> {
            let (diameter, _) = Self::dimensions_and_scale(configuration);
            let host = self
                .surface(kind)
                .map(|surface| surface.host)
                .ok_or_else(|| format!("{} effect host is missing", kind.label()))?;
            let child = unsafe {
                CreateWindowExW(
                    Default::default(),
                    WC_MAGNIFIER,
                    windows::core::PCWSTR::null(),
                    WS_CHILD,
                    0,
                    0,
                    diameter,
                    diameter,
                    host,
                    HMENU::default(),
                    self.instance,
                    None,
                )
            }
            .map_err(|error| {
                format!(
                    "Could not create {} WC_MAGNIFIER child: {error}",
                    kind.label()
                )
            })?;
            if let Some(surface) = self.surface_mut(kind).as_mut() {
                surface.magnifier = child;
            }
            self.configure_surface(kind, configuration)
        }

        fn configure_surface(
            &mut self,
            kind: EffectKind,
            configuration: &EffectConfiguration,
        ) -> Result<(), String> {
            let (diameter, scale) = Self::dimensions_and_scale(configuration);
            let Some(surface) = self.surface(kind) else {
                return Err(format!("{} effect host is missing", kind.label()));
            };
            let host = surface.host;
            let magnifier = surface.magnifier;
            let previous_diameter = surface.diameter;

            if previous_diameter != diameter {
                Self::set_circle_region(host, diameter).map_err(|error| {
                    format!("Could not resize {} host region: {error}", kind.label())
                })?;
                unsafe {
                    SetWindowPos(
                        host,
                        HWND::default(),
                        0,
                        0,
                        diameter,
                        diameter,
                        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                    )
                }
                .map_err(|error| format!("Could not resize {} host: {error}", kind.label()))?;
                if !magnifier.0.is_null() {
                    unsafe {
                        SetWindowPos(
                            magnifier,
                            HWND::default(),
                            0,
                            0,
                            diameter,
                            diameter,
                            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                        )
                    }
                    .map_err(|error| {
                        format!("Could not resize {} magnifier child: {error}", kind.label())
                    })?;
                }
            }
            if let Some(surface) = self.surface_mut(kind).as_mut() {
                surface.diameter = diameter;
                surface.scale = scale;
            }
            if !magnifier.0.is_null() {
                Self::set_transform(magnifier, scale).map_err(|error| {
                    format!("Could not configure {} scale: {error}", kind.label())
                })?;
                match (kind, configuration) {
                    (EffectKind::Halo, EffectConfiguration::Halo(preferences)) => {
                        Self::set_halo_color(magnifier, preferences.inversion_strength).map_err(
                            |error| format!("Could not configure halo inversion: {error}"),
                        )?;
                    }
                    (EffectKind::Zoom, EffectConfiguration::Zoom(_)) => {
                        Self::set_identity_color(magnifier).map_err(|error| {
                            format!("Could not configure neutral zoom color: {error}")
                        })?;
                    }
                    _ => return Err("Effect configuration kind did not match its surface".into()),
                }
            }
            Ok(())
        }
    }

    impl EffectNativeOperations for WindowsEffectOperations {
        fn initialize_session(&mut self) -> Result<(), String> {
            if self.session_initialized {
                return Ok(());
            }
            if !unsafe { MagInitialize() }.as_bool() {
                return Err(format!(
                    "MagInitialize returned FALSE (GetLastError={})",
                    unsafe { GetLastError().0 }
                ));
            }
            self.session_initialized = true;
            Ok(())
        }

        fn has_surface(&self, kind: EffectKind) -> bool {
            self.surface(kind).is_some()
        }

        fn create_surface(
            &mut self,
            kind: EffectKind,
            configuration: &EffectConfiguration,
        ) -> Result<(), String> {
            let (diameter, _) = Self::dimensions_and_scale(configuration);
            self.create_host(kind, diameter)?;
            self.create_magnifier_child(kind, configuration)
        }

        fn configure_surface(
            &mut self,
            kind: EffectKind,
            configuration: &EffectConfiguration,
        ) -> Result<(), String> {
            Self::configure_surface(self, kind, configuration)
        }

        fn host_window(&self, kind: EffectKind) -> Option<usize> {
            self.surface(kind).map(|surface| surface.host.0 as usize)
        }

        fn auxiliary_window_ids(&self, kind: EffectKind) -> Vec<usize> {
            if kind == EffectKind::Halo {
                self.halo_outline
                    .as_ref()
                    .filter(|outline| !outline.hwnd.0.is_null())
                    .map(|outline| vec![outline.hwnd.0 as usize])
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        }

        fn configure_halo_outline(&mut self, preferences: HaloPreferences) -> Result<(), String> {
            Self::configure_outline_surface(self, preferences)
        }

        fn set_filter_list(
            &mut self,
            kind: EffectKind,
            excluded_windows: &[usize],
        ) -> Result<(), String> {
            let magnifier = self
                .surface(kind)
                .map(|surface| surface.magnifier)
                .filter(|hwnd| !hwnd.0.is_null())
                .ok_or_else(|| format!("{} magnifier child is missing", kind.label()))?;
            if excluded_windows.len() > i32::MAX as usize {
                return Err("too many HWNDs for MagSetWindowFilterList".into());
            }
            let mut handles = excluded_windows
                .iter()
                .map(|window| HWND(*window as *mut _))
                .collect::<Vec<_>>();
            if !unsafe {
                MagSetWindowFilterList(
                    magnifier,
                    MW_FILTERMODE_EXCLUDE,
                    handles.len() as i32,
                    handles.as_mut_ptr(),
                )
            }
            .as_bool()
            {
                return Err(format!(
                    "MagSetWindowFilterList({}) failed (GetLastError={})",
                    kind.label(),
                    unsafe { GetLastError().0 }
                ));
            }
            Ok(())
        }

        fn is_visible(&self, kind: EffectKind) -> bool {
            self.surface(kind)
                .is_some_and(|surface| unsafe { IsWindowVisible(surface.host) }.as_bool())
        }

        fn hide_surface(&mut self, kind: EffectKind) -> Result<(), String> {
            let mut errors = Vec::new();
            if let Some(host) = self.surface(kind).map(|surface| surface.host) {
                if unsafe { IsWindowVisible(host) }.as_bool() {
                    unsafe {
                        let _ = ShowWindow(host, SW_HIDE);
                    }
                }
                if unsafe { IsWindowVisible(host) }.as_bool() {
                    errors.push(format!(
                        "{} effect host remained visible after SW_HIDE",
                        kind.label()
                    ));
                }
            }
            if kind == EffectKind::Halo
                && let Err(error) = self.hide_halo_outline()
            {
                errors.push(error);
            }
            errors.into_iter().next().map_or(Ok(()), Err)
        }

        fn refresh_visible_source(
            &mut self,
            kind: EffectKind,
            current_point: PhysicalPoint,
        ) -> Result<(), String> {
            let origin = self.update_live_source(kind, current_point)?;
            if kind == EffectKind::Halo
                && let Some(origin) = origin
            {
                self.present_halo_outline(origin)?;
            }
            Ok(())
        }

        fn present_live_source(
            &mut self,
            kind: EffectKind,
            current_point: PhysicalPoint,
        ) -> Result<(), String> {
            if kind != EffectKind::Halo {
                return Err("Only the halo can be presented in this checkpoint".into());
            }
            let origin = self
                .update_live_source(kind, current_point)?
                .ok_or_else(|| "Halo geometry did not provide a destination".to_string())?;
            self.show_halo_host()?;
            self.present_halo_outline(origin)
        }

        fn destroy_surface(&mut self, kind: EffectKind) -> Result<(), String> {
            let Some(host) = self.surface(kind).map(|surface| surface.host) else {
                return Ok(());
            };
            unsafe { DestroyWindow(host) }.map_err(|error| {
                format!("Could not destroy {} effect host: {error}", kind.label())
            })?;
            *self.surface_mut(kind) = None;
            Ok(())
        }

        fn release_auxiliary_surface(&mut self, kind: EffectKind) -> Result<(), String> {
            if kind != EffectKind::Halo {
                return Ok(());
            }
            if let Some(outline) = self.halo_outline.as_mut() {
                outline.shutdown()?;
            }
            self.halo_outline = None;
            self.halo_outline_image = None;
            self.halo_outline_preferences = None;
            Ok(())
        }

        fn uninitialize_session(&mut self) -> Result<(), String> {
            if self.halo.is_some() || self.zoom.is_some() {
                return Err(
                    "Cannot uninitialize Magnification while magnifier hosts remain".into(),
                );
            }
            if !self.session_initialized {
                return Ok(());
            }
            if !unsafe { MagUninitialize() }.as_bool() {
                return Err(format!(
                    "MagUninitialize returned FALSE (GetLastError={})",
                    unsafe { GetLastError().0 }
                ));
            }
            self.session_initialized = false;
            Ok(())
        }
    }

    impl Drop for WindowsEffectOperations {
        fn drop(&mut self) {
            for kind in [EffectKind::Zoom, EffectKind::Halo] {
                if let Err(error) = self.hide_surface(kind) {
                    eprintln!("coordinate effect cleanup: {error}");
                }
                if self.has_surface(kind)
                    && let Err(error) = self.destroy_surface(kind)
                {
                    eprintln!("coordinate effect cleanup: {error}");
                }
            }
            if let Err(error) = self.release_auxiliary_surface(EffectKind::Halo) {
                eprintln!("coordinate halo outline cleanup: {error}");
            }
            if self.halo.is_none() && self.zoom.is_none() && self.session_initialized {
                if let Err(error) = self.uninitialize_session() {
                    eprintln!("coordinate Magnification cleanup: {error}");
                }
            } else if self.halo.is_some() || self.zoom.is_some() {
                eprintln!(
                    "coordinate Magnification session retained because magnifier hosts remain"
                );
            }
            if self.halo_outline.is_some() {
                eprintln!("coordinate halo outline window remains after cleanup");
            }
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    struct HudVisual {
        lines: Vec<String>,
        font_size: u32,
        width: u32,
        height: u32,
    }

    struct GuideVisual {
        preferences: CrosshairPreferences,
        width: u32,
        height: u32,
        image: RgbaImage,
    }

    pub(super) struct WindowsSurfaceBackend {
        hud: LayeredSurface,
        crosshair: LayeredSurface,
        horizontal_guide: LayeredSurface,
        vertical_guide: LayeredSurface,
        effects: CursorEffectsRuntime<WindowsEffectOperations>,
        crosshair_preferences: Option<CrosshairPreferences>,
        crosshair_image: Option<RgbaImage>,
        horizontal_guide_visual: Option<GuideVisual>,
        vertical_guide_visual: Option<GuideVisual>,
        hud_visual: Option<HudVisual>,
        refresh_requested: bool,
        topology_invalidated: bool,
        ordered_halo_outline_enabled: Option<bool>,
        shutdown: bool,
    }

    impl WindowsSurfaceBackend {
        pub(super) fn new() -> Result<Self, String> {
            let instance = register_surface_class()?;
            Ok(Self {
                hud: LayeredSurface::new(instance)?,
                crosshair: LayeredSurface::new(instance)?,
                horizontal_guide: LayeredSurface::new(instance)?,
                vertical_guide: LayeredSurface::new(instance)?,
                effects: CursorEffectsRuntime::new(WindowsEffectOperations::new(instance)),
                crosshair_preferences: None,
                crosshair_image: None,
                horizontal_guide_visual: None,
                vertical_guide_visual: None,
                hud_visual: None,
                refresh_requested: false,
                topology_invalidated: false,
                ordered_halo_outline_enabled: None,
                shutdown: false,
            })
        }

        fn render_hud(&mut self, frame: &CoordinateRenderFrame, force: bool) -> Result<(), String> {
            if !frame.runtime_state.hud_enabled() {
                self.hud.hide();
                return Ok(());
            }

            let lines = hud_lines(frame);
            let dpi = frame
                .placement_sample
                .as_ref()
                .and_then(|sample| sample.monitor.as_ref())
                .and_then(|monitor| monitor.effective_dpi.map(|(x, _)| x));
            let work_area = frame
                .placement_sample
                .as_ref()
                .and_then(|sample| sample.monitor.as_ref())
                .and_then(|monitor| {
                    PhysicalSize::new(monitor.work_area.width(), monitor.work_area.height())
                });
            let (font_size, width, height) = hud_layout(&lines, hud_font_size(dpi), work_area);
            let visual = HudVisual {
                lines,
                font_size,
                width,
                height,
            };
            let visual_changed = self.hud_visual.as_ref() != Some(&visual);
            let origin = hud_origin(frame, width, height);
            if visual_changed || force || self.hud.needs_upload() {
                self.hud.present_hud(
                    origin,
                    visual.width,
                    visual.height,
                    visual.font_size,
                    &visual.lines,
                )?;
                self.hud_visual = Some(visual);
            } else {
                self.hud.reposition_and_show(origin)?;
            }
            Ok(())
        }

        fn render_crosshair(
            &mut self,
            frame: &CoordinateRenderFrame,
            force: bool,
        ) -> Result<(), String> {
            if !frame.runtime_state.crosshair_enabled() {
                self.crosshair.hide();
                self.horizontal_guide.hide();
                self.vertical_guide.hide();
                return Ok(());
            }
            let Some(sample) = frame.current_sample.as_ref() else {
                self.crosshair.hide();
                self.horizontal_guide.hide();
                self.vertical_guide.hide();
                return Ok(());
            };

            let style = frame.preferences.crosshair.clone();
            let style_changed = self.crosshair_preferences.as_ref() != Some(&style);
            if style_changed || self.crosshair_image.is_none() {
                self.crosshair_image = Some(crosshair_bitmap(&style));
                self.crosshair_preferences = Some(style.clone());
            }
            let image = self
                .crosshair_image
                .as_ref()
                .ok_or_else(|| "Coordinate crosshair image was not initialized".to_string())?;
            let x_offset = i32::try_from(image.width() / 2)
                .map_err(|_| "Coordinate crosshair width is too large")?;
            let y_offset = i32::try_from(image.height() / 2)
                .map_err(|_| "Coordinate crosshair height is too large")?;
            let origin = PhysicalPoint::new(
                sample
                    .desktop_point
                    .x
                    .checked_sub(x_offset)
                    .ok_or("Coordinate crosshair x position overflowed")?,
                sample
                    .desktop_point
                    .y
                    .checked_sub(y_offset)
                    .ok_or("Coordinate crosshair y position overflowed")?,
            );
            if style_changed || force || self.crosshair.needs_upload() {
                self.crosshair.present_image(origin, image)?;
            } else {
                self.crosshair.reposition_and_show(origin)?;
            }

            if !style.virtual_desktop_guides {
                self.horizontal_guide.hide();
                self.vertical_guide.hide();
                return Ok(());
            }
            let Some(desktop) = sample.virtual_desktop_bounds else {
                self.horizontal_guide.hide();
                self.vertical_guide.hide();
                return Ok(());
            };
            let horizontal = guide_geometry(
                GuideOrientation::Horizontal,
                desktop,
                sample.desktop_point,
                &style,
            );
            let vertical = guide_geometry(
                GuideOrientation::Vertical,
                desktop,
                sample.desktop_point,
                &style,
            );
            self.render_guide(true, horizontal, &style, force)?;
            self.render_guide(false, vertical, &style, force)?;
            self.crosshair.raise_topmost()?;
            Ok(())
        }

        fn render_guide(
            &mut self,
            horizontal: bool,
            geometry: Option<super::super::render::GuideGeometry>,
            style: &CrosshairPreferences,
            force: bool,
        ) -> Result<(), String> {
            let Some(geometry) = geometry else {
                if horizontal {
                    self.horizontal_guide.hide();
                } else {
                    self.vertical_guide.hide();
                }
                return Ok(());
            };
            let (visual_slot, surface) = if horizontal {
                (
                    &mut self.horizontal_guide_visual,
                    &mut self.horizontal_guide,
                )
            } else {
                (&mut self.vertical_guide_visual, &mut self.vertical_guide)
            };
            let visual_changed = visual_slot.as_ref().is_none_or(|visual| {
                visual.preferences != *style
                    || visual.width != geometry.width
                    || visual.height != geometry.height
            });
            if visual_changed {
                let image = guide_bitmap(geometry, style);
                *visual_slot = Some(GuideVisual {
                    preferences: style.clone(),
                    width: geometry.width,
                    height: geometry.height,
                    image,
                });
            }
            let visual = visual_slot
                .as_ref()
                .ok_or_else(|| "Coordinate guide image was not initialized".to_string())?;
            if visual_changed || force || surface.needs_upload() {
                surface.present_image(geometry.origin, &visual.image)?;
            } else {
                surface.reposition_and_show(geometry.origin)?;
            }
            Ok(())
        }

        fn raise_visible_cheap_surfaces_above_halo(&mut self) {
            // The halo host and its independently owned outline are raised
            // when presented. Re-establish the cheap overlay stack only when
            // the halo first becomes visible or its outline is introduced;
            // cursor movement does not churn topmost order.
            for surface in [
                &mut self.horizontal_guide,
                &mut self.vertical_guide,
                &mut self.crosshair,
                &mut self.hud,
            ] {
                if surface.is_visible()
                    && let Err(error) = surface.raise_topmost()
                {
                    eprintln!("coordinate overlay ordering: {error}");
                }
            }
        }
    }

    impl CoordinateSurfaceBackend for WindowsSurfaceBackend {
        fn poll_events(&mut self) -> Result<bool, String> {
            let mut refresh = false;
            let mut message = MSG::default();
            while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                if message.message == WM_COORDINATE_TOPOLOGY_INVALIDATED {
                    refresh = true;
                    self.topology_invalidated = true;
                }
                unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            if !self.topology_invalidated {
                let cheap_window_ids = [
                    self.hud.hwnd.0 as usize,
                    self.crosshair.hwnd.0 as usize,
                    self.horizontal_guide.hwnd.0 as usize,
                    self.vertical_guide.hwnd.0 as usize,
                ];
                self.effects.poll_visible_sources(&cheap_window_ids);
            }
            self.refresh_requested |= refresh;
            Ok(refresh)
        }

        fn render(&mut self, frame: &CoordinateRenderFrame) -> Result<(), String> {
            if self.shutdown {
                return Err("Coordinate surfaces are already shut down".into());
            }
            let force = mem::take(&mut self.refresh_requested);
            let topology_invalidated = mem::take(&mut self.topology_invalidated);
            let ordered_outline_enabled = self.ordered_halo_outline_enabled;
            let cheap_window_ids = [
                self.hud.hwnd.0 as usize,
                self.crosshair.hwnd.0 as usize,
                self.horizontal_guide.hwnd.0 as usize,
                self.vertical_guide.hwnd.0 as usize,
            ];
            self.effects.reconcile(
                EffectRequests::from_runtime(&frame.runtime_state),
                &frame.preferences,
                frame
                    .current_sample
                    .as_ref()
                    .map(|sample| sample.desktop_point),
                &cheap_window_ids,
                topology_invalidated,
            );
            let mut errors = Vec::new();
            if let Err(error) = self.render_crosshair(frame, force) {
                self.crosshair.hide();
                self.horizontal_guide.hide();
                self.vertical_guide.hide();
                errors.push(error);
            }
            if let Err(error) = self.render_hud(frame, force) {
                self.hud.hide();
                errors.push(error);
            }
            if matches!(self.effects.status().halo(), CursorEffectStatus::Active) {
                if ordered_outline_enabled != Some(frame.preferences.halo.outline_enabled) {
                    self.raise_visible_cheap_surfaces_above_halo();
                }
                self.ordered_halo_outline_enabled = Some(frame.preferences.halo.outline_enabled);
            } else {
                self.ordered_halo_outline_enabled = None;
            }
            errors.into_iter().next().map_or(Ok(()), Err)
        }

        fn effects_status(&self) -> CoordinateEffectsStatus {
            self.effects.status()
        }

        fn shutdown(&mut self) -> Result<(), String> {
            if self.shutdown {
                return Ok(());
            }
            self.shutdown = true;
            let mut errors = Vec::new();
            if let Err(error) = self.effects.shutdown() {
                errors.push(error);
            }
            for result in [
                self.hud.shutdown(),
                self.crosshair.shutdown(),
                self.horizontal_guide.shutdown(),
                self.vertical_guide.shutdown(),
            ] {
                if let Err(error) = result {
                    errors.push(error);
                }
            }
            self.hud_visual = None;
            self.crosshair_image = None;
            self.horizontal_guide_visual = None;
            self.vertical_guide_visual = None;
            self.crosshair_preferences = None;
            errors.into_iter().next().map_or(Ok(()), Err)
        }
    }

    impl Drop for WindowsSurfaceBackend {
        fn drop(&mut self) {
            let _ = self.shutdown();
        }
    }

    fn hud_origin(frame: &CoordinateRenderFrame, width: u32, height: u32) -> PhysicalPoint {
        let size = PhysicalSize::new(i64::from(width), i64::from(height));
        if let (Some(sample), Some(size)) = (frame.placement_sample.as_ref(), size) {
            if let Some(origin) = frame.preferences.hud_origin(sample, size) {
                return origin;
            }
            return PhysicalPoint::new(
                sample
                    .desktop_point
                    .x
                    .saturating_add(frame.preferences.cursor_offset.x),
                sample
                    .desktop_point
                    .y
                    .saturating_add(frame.preferences.cursor_offset.y),
            );
        }
        PhysicalPoint::new(16, 16)
    }
}

#[cfg(test)]
mod tests {
    use super::NativeCoordinateRuntimeFactory;
    use crate::coordinate_tool::controller::CoordinateRuntimeFactory;

    #[cfg(not(windows))]
    #[test]
    fn native_factory_reports_windows_requirement_instead_of_creating_fake_runtime() {
        let factory = NativeCoordinateRuntimeFactory;
        let sampler_error = match factory.create_sampler() {
            Err(error) => error.contains("Windows"),
            Ok(_) => false,
        };
        let backend_error = match factory.create_backend() {
            Err(error) => error.contains("Windows"),
            Ok(_) => false,
        };
        assert!(sampler_error);
        assert!(backend_error);
    }
}
