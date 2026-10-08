//! Passive Windows sampler and layered surfaces for the coordinate inspector.
//!
//! All HWND and GDI ownership remains on the single controller worker. The
//! native backend draws only the compact HUD, center mark, and two narrow
//! virtual-desktop bands; cursor motion moves the latter three windows without
//! rebuilding a desktop-sized bitmap.

use super::controller::{CoordinateRuntimeFactory, CoordinateSampler, CoordinateSurfaceBackend};
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
        COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE,
    };
    use windows::Win32::Graphics::Gdi::{
        AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
        CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, ClientToScreen, CreateCompatibleDC,
        CreateDIBSection, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, FW_NORMAL, FillRect, GetMonitorInfoW, HBITMAP, HDC,
        HGDIOBJ, MONITOR_DEFAULTTONEAREST, MONITORINFOEXW, MonitorFromPoint, OUT_DEFAULT_PRECIS,
        SelectObject, SetBkMode, SetTextColor, TRANSPARENT, TextOutW,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor,
        MDT_EFFECTIVE_DPI, SetThreadDpiAwarenessContext,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
        GetCursorPos, GetForegroundWindow, GetSystemMetrics, GetWindowThreadProcessId,
        HWND_TOPMOST, MSG, PM_REMOVE, PeekMessageW, RegisterClassW, SM_CXVIRTUALSCREEN,
        SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_HIDE, SW_SHOWNOACTIVATE,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SetWindowPos, ShowWindow,
        TranslateMessage, ULW_ALPHA, UpdateLayeredWindow, WM_DISPLAYCHANGE, WM_DPICHANGED,
        WM_NCHITTEST, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        WS_EX_TRANSPARENT, WS_POPUP,
    };
    use windows::core::{PCWSTR, w};

    use super::super::controller::{
        CoordinateRenderFrame, CoordinateSampler, CoordinateSurfaceBackend,
    };
    use super::super::model::{
        CoordinateSample, ForegroundClientGeometry, MonitorGeometry, MonitorId, PhysicalPoint,
        PhysicalRect, PhysicalSize,
    };
    use super::super::render::{
        GuideOrientation, crosshair_bitmap, guide_bitmap, guide_geometry, hud_dimensions,
        hud_font_size, hud_lines,
    };
    use super::super::settings::CrosshairPreferences;
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
    }

    impl CoordinateSampler for WindowsSampler {
        fn sample(&mut self) -> Result<CoordinateSample, String> {
            let mut point = POINT::default();
            unsafe { GetCursorPos(&mut point) }
                .map_err(|error| format!("Could not sample the physical cursor: {error}"))?;
            let desktop_point = PhysicalPoint::new(point.x, point.y);
            Ok(CoordinateSample::new(
                desktop_point,
                virtual_desktop_bounds(),
                monitor_geometry(point),
                foreground_client_geometry(),
            ))
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

    unsafe extern "system" fn passive_window_proc(
        hwnd: HWND,
        message: u32,
        wparam: windows::Win32::Foundation::WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_NCHITTEST {
            return LRESULT(windows::Win32::UI::WindowsAndMessaging::HTTRANSPARENT as isize);
        }
        // The worker detects these notifications while pumping its queue and
        // immediately resamples geometry before redrawing the active surfaces.
        if message == WM_DISPLAYCHANGE || message == WM_DPICHANGED {
            return LRESULT(0);
        }
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
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
            unsafe { DeleteObject(brush) };
            if filled == 0 {
                return Err("Could not paint coordinate HUD background".into());
            }

            let font = unsafe {
                CreateFontW(
                    i32::try_from(font_size).unwrap_or(14),
                    0,
                    0,
                    0,
                    FW_NORMAL.0,
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
                unsafe { DeleteObject(font) };
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
            let (width, height) = image.dimensions();
            self.ensure_dib(width, height)?.copy_rgba(image)?;
            self.dirty = true;
            self.upload(origin)?;
            self.show();
            Ok(())
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
            self.show();
            Ok(())
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
            self.show();
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

        fn show(&mut self) {
            if !self.visible {
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                }
                self.visible = true;
            }
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
        crosshair_preferences: Option<CrosshairPreferences>,
        crosshair_image: Option<RgbaImage>,
        horizontal_guide_visual: Option<GuideVisual>,
        vertical_guide_visual: Option<GuideVisual>,
        hud_visual: Option<HudVisual>,
        refresh_requested: bool,
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
                crosshair_preferences: None,
                crosshair_image: None,
                horizontal_guide_visual: None,
                vertical_guide_visual: None,
                hud_visual: None,
                refresh_requested: false,
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
            let font_size = hud_font_size(dpi);
            let (width, height) = hud_dimensions(&lines, font_size);
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
    }

    impl CoordinateSurfaceBackend for WindowsSurfaceBackend {
        fn poll_events(&mut self) -> Result<bool, String> {
            let mut refresh = false;
            let mut message = MSG::default();
            while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                if message.message == WM_DISPLAYCHANGE || message.message == WM_DPICHANGED {
                    refresh = true;
                }
                unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            self.refresh_requested |= refresh;
            Ok(refresh)
        }

        fn render(&mut self, frame: &CoordinateRenderFrame) -> Result<(), String> {
            if self.shutdown {
                return Err("Coordinate surfaces are already shut down".into());
            }
            let force = mem::take(&mut self.refresh_requested);
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
            errors.into_iter().next().map_or(Ok(()), Err)
        }

        fn shutdown(&mut self) -> Result<(), String> {
            if self.shutdown {
                return Ok(());
            }
            self.shutdown = true;
            let mut errors = Vec::new();
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
