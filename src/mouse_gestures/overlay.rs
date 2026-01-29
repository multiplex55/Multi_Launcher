pub trait OverlayBackend: Send {
    fn draw_trail_segment(&mut self, from: (f32, f32), to: (f32, f32), color: [u8; 4], width: f32);
    fn clear_trail(&mut self);
    fn show_hint(&mut self, text: &str, position: (f32, f32));
    fn hide_hint(&mut self);
}

#[derive(Debug)]
pub struct TrailOverlay<B: OverlayBackend> {
    backend: B,
    enabled: bool,
    color: [u8; 4],
    width: f32,
    start_move_px: f32,
    start_point: Option<(f32, f32)>,
    last_point: Option<(f32, f32)>,
    started: bool,
}

impl<B: OverlayBackend> TrailOverlay<B> {
    pub fn new(backend: B, enabled: bool, color: [u8; 4], width: f32, start_move_px: f32) -> Self {
        Self {
            backend,
            enabled,
            color,
            width,
            start_move_px,
            start_point: None,
            last_point: None,
            started: false,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.backend.clear_trail();
            self.started = false;
            self.last_point = None;
            self.start_point = None;
        }
    }

    pub fn reset(&mut self, start_point: (f32, f32)) {
        if self.enabled {
            self.backend.clear_trail();
        }
        self.start_point = Some(start_point);
        self.last_point = None;
        self.started = false;
    }

    pub fn clear(&mut self) {
        if self.enabled {
            self.backend.clear_trail();
        }
        self.started = false;
        self.last_point = None;
        self.start_point = None;
    }

    pub fn update_position(&mut self, point: (f32, f32)) {
        if !self.enabled {
            return;
        }

        let start = match self.start_point {
            Some(start) => start,
            None => {
                self.start_point = Some(point);
                return;
            }
        };

        if !self.started {
            let dx = point.0 - start.0;
            let dy = point.1 - start.1;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < self.start_move_px * self.start_move_px {
                return;
            }
            self.backend
                .draw_trail_segment(start, point, self.color, self.width);
            self.last_point = Some(point);
            self.started = true;
            return;
        }

        let last = self.last_point.unwrap_or(point);
        let dx = point.0 - last.0;
        let dy = point.1 - last.1;
        self.backend
            .draw_trail_segment(last, point, self.color, self.width);
        self.last_point = Some(point);
    }
}

#[derive(Debug)]
pub struct HintOverlay<B: OverlayBackend> {
    backend: B,
    enabled: bool,
    offset: (f32, f32),
    last_tokens: String,
    last_match: Option<String>,
    last_position: Option<(f32, f32)>,
    visible: bool,
}

impl<B: OverlayBackend> HintOverlay<B> {
    pub fn new(backend: B, enabled: bool, offset: (f32, f32)) -> Self {
        Self {
            backend,
            enabled,
            offset,
            last_tokens: String::new(),
            last_match: None,
            last_position: None,
            visible: false,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.hide();
        }
    }

    pub fn reset(&mut self) {
        self.hide();
        self.last_tokens.clear();
        self.last_match = None;
        self.last_position = None;
    }

    pub fn update(&mut self, tokens: &str, best_match: Option<&str>, cursor: (f32, f32)) {
        if !self.enabled {
            if self.visible {
                self.hide();
            }
            return;
        }

        let match_owned = best_match.map(|value| value.to_string());
        let position = (cursor.0 + self.offset.0, cursor.1 + self.offset.1);
        let same_tokens = tokens == self.last_tokens;
        let same_match = match_owned.as_deref() == self.last_match.as_deref();
        let same_position = self
            .last_position
            .map(|pos| pos == position)
            .unwrap_or(false);

        let mut text = tokens.to_string();
        if let Some(name) = best_match {
            if !text.is_empty() {
                text.push_str(" - ");
            }
            text.push_str(name);
        }

        if text.is_empty() {
            self.hide();
            return;
        }

        if same_tokens && same_match && same_position {
            return;
        }

        self.last_tokens = tokens.to_string();
        self.last_match = match_owned;
        self.last_position = Some(position);
        self.backend.show_hint(&text, position);
        self.visible = true;
    }

    fn hide(&mut self) {
        if self.visible {
            self.backend.hide_hint();
            self.visible = false;
        }
    }
}

#[cfg(windows)]
#[derive(Debug)]
struct HintOverlaySurface {
    hwnd: windows::Win32::Foundation::HWND,
    mem_dc: windows::Win32::Graphics::Gdi::HDC,
    dib: windows::Win32::Graphics::Gdi::HBITMAP,
    old_bitmap: windows::Win32::Graphics::Gdi::HGDIOBJ,
    bits: *mut u8,
    width: i32,
    height: i32,
    size_bytes: usize,
    last_text: String,
    visible: bool,
}

#[cfg(windows)]
unsafe impl Send for HintOverlaySurface {}

#[cfg(windows)]
impl HintOverlaySurface {
    fn new() -> Option<Self> {
        use std::sync::Once;
        use std::{mem, ptr};
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::{COLORREF, HWND};
        use windows::Win32::Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, DeleteDC, SelectObject, BITMAPINFO,
            BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        };
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, RegisterClassW, SetLayeredWindowAttributes, SetWindowLongPtrW,
            ShowWindow, GWLP_USERDATA, LWA_COLORKEY, SW_HIDE, WNDCLASSW, WS_EX_LAYERED,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
        };

        static REGISTER: Once = Once::new();
        let class_name = widestring("MultiLauncherHintOverlay");
        let hinstance = unsafe { GetModuleHandleW(PCWSTR::null()) }.ok()?;

        REGISTER.call_once(|| unsafe {
            let wnd_class = WNDCLASSW {
                hInstance: hinstance.into(),
                lpszClassName: PCWSTR(class_name.as_ptr()),
                lpfnWndProc: Some(trail_overlay_wndproc), // reuse existing blit wndproc
                ..Default::default()
            };
            let _ = RegisterClassW(&wnd_class);
        });

        let width = 320;
        let height = 64;

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST
                    | WS_EX_NOACTIVATE,
                PCWSTR(class_name.as_ptr()),
                PCWSTR::null(),
                WS_POPUP,
                0,
                0,
                width,
                height,
                HWND::default(),
                None,
                hinstance,
                None,
            )
        }
        .ok()?;

        let mem_dc = unsafe { CreateCompatibleDC(None) };
        if mem_dc.0.is_null() {
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
            }
            return None;
        }

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            },
            bmiColors: [Default::default()],
        };

        let mut bits: *mut core::ffi::c_void = ptr::null_mut();
        let dib =
            unsafe { CreateDIBSection(mem_dc, &mut info, DIB_RGB_COLORS, &mut bits, None, 0) }
                .ok()?;
        if bits.is_null() {
            unsafe {
                DeleteDC(mem_dc);
                windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
            }
            return None;
        }

        let old_bitmap = unsafe { SelectObject(mem_dc, dib) };
        let size_bytes = width as usize * height as usize * 4;

        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, mem_dc.0 as isize);
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_COLORKEY);
            ptr::write_bytes(bits as *mut u8, 0, size_bytes);
            let _ = ShowWindow(hwnd, SW_HIDE);
        }

        Some(Self {
            hwnd,
            mem_dc,
            dib,
            old_bitmap,
            bits: bits as *mut u8,
            width,
            height,
            size_bytes,
            last_text: String::new(),
            visible: false,
        })
    }

    fn clear(&mut self) {
        use std::ptr;
        use windows::Win32::Graphics::Gdi::InvalidateRect;

        unsafe {
            ptr::write_bytes(self.bits, 0, self.size_bytes);
            let _ = InvalidateRect(self.hwnd, None, false);
        }
    }

    fn move_to(&mut self, position: (f32, f32)) {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::Controls::TTM_TRACKPOSITION;
        use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

        // TTM_TRACKPOSITION packs x/y into 16-bit signed halves. Mask to avoid corrupting the
        // high word when x/y are negative (common on multi-monitor layouts).
        let x = position.0.round() as i32;
        let y = position.1.round() as i32;
        let packed = ((x as u32) & 0xFFFF) | (((y as u32) & 0xFFFF) << 16);

        unsafe {
            let _ = SendMessageW(
                self.hwnd,
                TTM_TRACKPOSITION,
                WPARAM(0),
                LPARAM(packed as isize),
            );
        }
    }

    fn set_text(&mut self, text: &str) {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use std::ptr;
        use windows::Win32::Foundation::COLORREF;
        use windows::Win32::Graphics::Gdi::{
            CreateFontW, DeleteObject, InvalidateRect, SelectObject, SetBkMode, SetTextColor,
            TextOutW, TRANSPARENT,
        };

        if text == self.last_text {
            return;
        }
        self.last_text = text.to_string();

        unsafe {
            // Clear backing buffer
            ptr::write_bytes(self.bits, 0, self.size_bytes);

            let wide: Vec<u16> = OsStr::new(text).encode_wide().collect();

            let font = CreateFontW(
                18,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                windows::core::PCWSTR::null(),
            );
            let old_font = SelectObject(self.mem_dc, font);

            let _ = SetBkMode(self.mem_dc, TRANSPARENT);
            let _ = SetTextColor(self.mem_dc, COLORREF(0x00FFFFFF));
            let _ = TextOutW(self.mem_dc, 0, 0, &wide);

            let _ = SelectObject(self.mem_dc, old_font);
            let _ = DeleteObject(font);

            let _ = InvalidateRect(self.hwnd, None, false);
        }
    }

    fn hide(&mut self) {
        use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        self.visible = false;
    }
}

#[cfg(windows)]
impl Drop for HintOverlaySurface {
    fn drop(&mut self) {
        use windows::Win32::Graphics::Gdi::{DeleteDC, DeleteObject, SelectObject};
        use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;

        unsafe {
            if !self.mem_dc.0.is_null() {
                let _ = SelectObject(self.mem_dc, self.old_bitmap);
            }
            if !self.dib.0.is_null() {
                let _ = DeleteObject(self.dib);
            }
            if !self.mem_dc.0.is_null() {
                DeleteDC(self.mem_dc);
            }
            if !self.hwnd.0.is_null() {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}
#[cfg(windows)]
#[derive(Debug)]
struct TrailOverlaySurface {
    hwnd: windows::Win32::Foundation::HWND,
    mem_dc: windows::Win32::Graphics::Gdi::HDC,
    dib: windows::Win32::Graphics::Gdi::HBITMAP,
    old_bitmap: windows::Win32::Graphics::Gdi::HGDIOBJ,
    bits: *mut u8,
    size_bytes: usize,
    origin_x: i32,
    origin_y: i32,
}

#[cfg(windows)]
unsafe impl Send for TrailOverlaySurface {}

#[cfg(windows)]
impl TrailOverlaySurface {
    fn new() -> Option<Self> {
        use std::mem;
        use std::ptr;
        use std::sync::Once;
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::{COLORREF, HWND};
        use windows::Win32::Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, DeleteDC, SelectObject, BITMAPINFO,
            BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        };
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, RegisterClassW, SetLayeredWindowAttributes, SetWindowLongPtrW,
            ShowWindow, GWLP_USERDATA, LWA_COLORKEY, SW_SHOW, WNDCLASSW, WS_EX_LAYERED,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
        };

        static REGISTER: Once = Once::new();
        let class_name = widestring("MultiLauncherTrailOverlay");
        let hinstance = unsafe { GetModuleHandleW(PCWSTR::null()) }.ok()?;

        REGISTER.call_once(|| unsafe {
            let wnd_class = WNDCLASSW {
                hInstance: hinstance.into(),
                lpszClassName: PCWSTR(class_name.as_ptr()),
                lpfnWndProc: Some(trail_overlay_wndproc),
                ..Default::default()
            };
            let _ = RegisterClassW(&wnd_class);
        });

        use windows::Win32::UI::WindowsAndMessaging::{
            SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
        };

        let vx = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let vy = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
        let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST
                    | WS_EX_NOACTIVATE,
                PCWSTR(class_name.as_ptr()),
                PCWSTR::null(),
                WS_POPUP,
                vx,
                vy,
                width,
                height,
                HWND::default(),
                None,
                hinstance,
                None,
            )
        }
        .ok()?;

        let mem_dc = unsafe { CreateCompatibleDC(None) };
        if mem_dc.0.is_null() {
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
            }
            return None;
        }

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            },
            bmiColors: [Default::default()],
        };
        let mut bits: *mut core::ffi::c_void = ptr::null_mut();
        let dib =
            unsafe { CreateDIBSection(mem_dc, &mut info, DIB_RGB_COLORS, &mut bits, None, 0) }
                .ok()?;
        if bits.is_null() {
            unsafe {
                DeleteDC(mem_dc);
                windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
            }
            return None;
        }
        let old_bitmap = unsafe { SelectObject(mem_dc, dib) };
        let size_bytes = width as usize * height as usize * 4;

        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, mem_dc.0 as isize);
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_COLORKEY);
            let _ = ShowWindow(hwnd, SW_SHOW);

            if !bits.is_null() {
                ptr::write_bytes(bits as *mut u8, 0, size_bytes);
            }
        }

        Some(Self {
            hwnd,
            mem_dc,
            dib,
            old_bitmap,
            bits: bits as *mut u8,
            size_bytes,
            origin_x: vx,
            origin_y: vy,
        })
    }

    fn clear(&mut self) {
        use std::ptr;
        use windows::Win32::Graphics::Gdi::InvalidateRect;
        if !self.bits.is_null() && self.size_bytes > 0 {
            unsafe {
                ptr::write_bytes(self.bits, 0, self.size_bytes);
                let _ = InvalidateRect(self.hwnd, None, false);
            }
        }
    }
}

#[cfg(windows)]
impl Drop for TrailOverlaySurface {
    fn drop(&mut self) {
        use windows::Win32::Graphics::Gdi::{DeleteDC, DeleteObject, SelectObject};
        use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;
        unsafe {
            if !self.mem_dc.0.is_null() {
                let _ = SelectObject(self.mem_dc, self.old_bitmap);
            }
            if !self.dib.0.is_null() {
                let _ = DeleteObject(self.dib);
            }
            if !self.mem_dc.0.is_null() {
                DeleteDC(self.mem_dc);
            }
            if !self.hwnd.0.is_null() {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

#[cfg(windows)]
unsafe extern "system" fn trail_overlay_wndproc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::{LRESULT, RECT};
    use windows::Win32::Graphics::Gdi::PAINTSTRUCT;
    use windows::Win32::Graphics::Gdi::{BeginPaint, BitBlt, EndPaint, SRCCOPY};
    use windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA;
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, GetClientRect, GetWindowLongPtrW, HTTRANSPARENT, WM_ERASEBKGND,
        WM_NCHITTEST, WM_PAINT,
    };

    unsafe {
        match msg {
            WM_NCHITTEST => return LRESULT(HTTRANSPARENT as isize),
            WM_ERASEBKGND => return LRESULT(1),
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut paint);
                if !hdc.0.is_null() {
                    let mem_dc =
                        windows::Win32::Graphics::Gdi::HDC(
                            GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut core::ffi::c_void
                        );
                    if !mem_dc.0.is_null() {
                        let mut rect = RECT::default();
                        let _ = GetClientRect(hwnd, &mut rect);
                        let width = rect.right - rect.left;
                        let height = rect.bottom - rect.top;
                        let _ = BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);
                    }
                }
                EndPaint(hwnd, &paint);
                return LRESULT(0);
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

#[cfg(windows)]
fn widestring(value: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
impl DefaultOverlayBackend {
    fn ensure_trail_surface(&mut self) -> Option<&mut TrailOverlaySurface> {
        if self.trail_surface.is_none() {
            self.trail_surface = TrailOverlaySurface::new();
            self.trail_needs_raise = true;
        }
        self.trail_surface.as_mut()
    }

    fn ensure_hint_tooltip(&mut self) -> Option<&mut HintTooltip> {
        if self.hint_tooltip.is_none() {
            self.hint_tooltip = HintTooltip::new();
        }
        self.hint_tooltip.as_mut()
    }
}

#[cfg(windows)]
impl OverlayBackend for DefaultOverlayBackend {
    fn draw_trail_segment(&mut self, from: (f32, f32), to: (f32, f32), color: [u8; 4], width: f32) {
        use windows::Win32::Foundation::COLORREF;
        use windows::Win32::Graphics::Gdi::InvalidateRect;
        use windows::Win32::Graphics::Gdi::{
            CreatePen, DeleteObject, LineTo, MoveToEx, SelectObject, PS_SOLID,
        };

        let needs_raise = self.trail_needs_raise;

        let hwnd = {
            let Some(surface) = self.ensure_trail_surface() else {
                return;
            };
            surface.hwnd
        };

        if needs_raise {
            self.trail_needs_raise = false;
            use windows::Win32::UI::WindowsAndMessaging::{
                SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
            };
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }

        let Some(surface) = self.ensure_trail_surface() else {
            return;
        };

        let colorref =
            COLORREF((color[0] as u32) | ((color[1] as u32) << 8) | ((color[2] as u32) << 16));
        let pen = unsafe { CreatePen(PS_SOLID, width.max(1.0) as i32, colorref) };
        let old_pen = unsafe { SelectObject(surface.mem_dc, pen) };

        let fx = from.0 as i32 - surface.origin_x;
        let fy = from.1 as i32 - surface.origin_y;
        let tx = to.0 as i32 - surface.origin_x;
        let ty = to.1 as i32 - surface.origin_y;

        let _ = unsafe { MoveToEx(surface.mem_dc, fx, fy, None) };
        let _ = unsafe { LineTo(surface.mem_dc, tx, ty) };

        unsafe {
            SelectObject(surface.mem_dc, old_pen);
            let _ = DeleteObject(pen);
            let _ = InvalidateRect(surface.hwnd, None, false);
        }
    }

    fn clear_trail(&mut self) {
        // Next draw should re-raise the overlay (some apps create TOPMOST windows that can cover us).
        self.trail_needs_raise = true;
        if let Some(surface) = self.ensure_trail_surface() {
            surface.clear();
        }
    }

    fn show_hint(&mut self, text: &str, position: (f32, f32)) {
        // Decide + update cached text BEFORE borrowing tooltip mutably.
        let text_changed = self.last_hint_text != text;
        if text_changed {
            self.last_hint_text.clear();
            self.last_hint_text.push_str(text);
        }

        let Some(tt) = self.ensure_hint_tooltip() else {
            return;
        };

        // Always follow cursor (cheap)
        tt.move_to(position);

        // Only change text when it actually changes (prevents flicker/churn)
        if text_changed {
            tt.set_text(text);
        }

        tt.show();
    }

    fn hide_hint(&mut self) {
        if let Some(tt) = self.hint_tooltip.as_mut() {
            tt.hide();
        }
        self.last_hint_text.clear();
    }
}

#[cfg(windows)]
#[derive(Debug, Default)]
pub struct DefaultOverlayBackend {
    trail_surface: Option<TrailOverlaySurface>,
    /// Set to true when starting a new gesture so the overlay is re-raised to the top of the TOPMOST band.
    trail_needs_raise: bool,
    hint_tooltip: Option<HintTooltip>,
    last_hint_text: String,
}

#[cfg(windows)]
#[derive(Debug)]
struct HintTooltip {
    owner_hwnd: windows::Win32::Foundation::HWND,
    hwnd: windows::Win32::Foundation::HWND,
    toolinfo: windows::Win32::UI::Controls::TTTOOLINFOW,
    text_buf: Vec<u16>,
    visible: bool,
}

#[cfg(windows)]
unsafe impl Send for HintTooltip {}

#[cfg(windows)]
impl HintTooltip {
    fn new() -> Option<Self> {
        use std::mem;
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::Controls::{
            InitCommonControlsEx, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX, TOOLTIPS_CLASSW,
            TTDT_AUTOPOP, TTDT_INITIAL, TTDT_RESHOW, TTF_ABSOLUTE, TTF_TRACK, TTM_ADDTOOLW,
            TTM_SETDELAYTIME, TTM_SETMAXTIPWIDTH, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, GetDesktopWindow, SendMessageW, ShowWindow, HWND_MESSAGE, SW_HIDE,
            WINDOW_EX_STYLE, WINDOW_STYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
            WS_POPUP,
        };

        unsafe {
            let icc = INITCOMMONCONTROLSEX {
                dwSize: mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_WIN95_CLASSES,
            };
            let _ = InitCommonControlsEx(&icc);

            let hinstance = GetModuleHandleW(PCWSTR::null()).ok()?;
            let parent = GetDesktopWindow();
            // Use a tool/owner window we actually own.
            // Using GetDesktopWindow() here is tempting, but it belongs to another process.
            let owner_class = widestring("STATIC");
            let owner_hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                PCWSTR(owner_class.as_ptr()),
                PCWSTR::null(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                hinstance,
                None,
            )
            .ok()?;
            let _ = ShowWindow(owner_hwnd, SW_HIDE);

            // Tooltip window (topmost, no-activate)
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                TOOLTIPS_CLASSW,
                PCWSTR::null(),
                // windows-rs wants WINDOW_STYLE, so wrap raw u32 bits
                WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP) | WINDOW_STYLE(TTS_NOPREFIX),
                0,
                0,
                0,
                0,
                owner_hwnd,
                None,
                hinstance,
                None,
            )
            .ok()?;

            // initial empty text
            let mut text_buf: Vec<u16> = vec![0];

            let mut toolinfo = TTTOOLINFOW::default();
            toolinfo.cbSize =
                (mem::size_of::<TTTOOLINFOW>() - mem::size_of::<*const core::ffi::c_void>()) as u32;
            toolinfo.hwnd = owner_hwnd;
            toolinfo.uId = 1;
            toolinfo.uFlags = TTF_TRACK | TTF_ABSOLUTE;
            toolinfo.lpszText = windows::core::PWSTR(text_buf.as_mut_ptr());

            // REQUIRED: register the tool, otherwise TTM_TRACKACTIVATE won't show anything.
            let add_ok = SendMessageW(
                hwnd,
                TTM_ADDTOOLW,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(&toolinfo as *const _ as isize),
            );

            // Make it feel "live" while drawing gestures.
            let _ = SendMessageW(
                hwnd,
                TTM_SETDELAYTIME,
                windows::Win32::Foundation::WPARAM(TTDT_INITIAL as usize),
                windows::Win32::Foundation::LPARAM(0),
            );
            let _ = SendMessageW(
                hwnd,
                TTM_SETDELAYTIME,
                windows::Win32::Foundation::WPARAM(TTDT_RESHOW as usize),
                windows::Win32::Foundation::LPARAM(0),
            );
            let _ = SendMessageW(
                hwnd,
                TTM_SETDELAYTIME,
                windows::Win32::Foundation::WPARAM(TTDT_AUTOPOP as usize),
                windows::Win32::Foundation::LPARAM(10_000),
            );

            let _ = SendMessageW(
                hwnd,
                TTM_SETMAXTIPWIDTH,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(600),
            );

            Some(Self {
                owner_hwnd,
                hwnd,
                toolinfo,
                text_buf,
                visible: false,
            })
        }
    }

    fn set_text(&mut self, text: &str) {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::UI::Controls::TTM_UPDATETIPTEXTW;
        use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

        // build wide string with trailing 0
        self.text_buf.clear();
        self.text_buf.extend(OsStr::new(text).encode_wide());
        self.text_buf.push(0);

        self.toolinfo.lpszText = windows::core::PWSTR(self.text_buf.as_mut_ptr());

        unsafe {
            let _ = SendMessageW(
                self.hwnd,
                TTM_UPDATETIPTEXTW,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(&self.toolinfo as *const _ as isize),
            );
        }
    }

    fn move_to(&mut self, position: (f32, f32)) {
        use windows::Win32::UI::Controls::TTM_TRACKPOSITION;
        use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

        // TTM_TRACKPOSITION packs x/y into 16-bit halves.
        // Preserve negative coordinates (common on multi-monitor layouts).
        let x = position.0.round() as i32;
        let y = position.1.round() as i32;
        let packed = ((x as u32) & 0xFFFF) | (((y as u32) & 0xFFFF) << 16);

        unsafe {
            let _ = SendMessageW(
                self.hwnd,
                TTM_TRACKPOSITION,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(packed as isize),
            );
        }
    }

    fn show(&mut self) {
        use windows::Win32::UI::Controls::TTM_TRACKACTIVATE;
        use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

        // if self.visible {
        //     return;
        // }
        unsafe {
            let _ = SendMessageW(
                self.hwnd,
                TTM_TRACKACTIVATE,
                windows::Win32::Foundation::WPARAM(1),
                windows::Win32::Foundation::LPARAM(&self.toolinfo as *const _ as isize),
            );
        }

        self.visible = true;
    }

    fn hide(&mut self) {
        use windows::Win32::UI::Controls::TTM_TRACKACTIVATE;
        use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

        if !self.visible {
            return;
        }
        unsafe {
            let _ = SendMessageW(
                self.hwnd,
                TTM_TRACKACTIVATE,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(&self.toolinfo as *const _ as isize),
            );
        }
        self.visible = false;
    }
}

#[cfg(windows)]
impl Drop for HintTooltip {
    fn drop(&mut self) {
        use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;
        unsafe {
            let _ = DestroyWindow(self.owner_hwnd);
        }
    }
}

#[cfg(not(windows))]
#[derive(Default, Debug)]
pub struct DefaultOverlayBackend;

#[cfg(not(windows))]
impl OverlayBackend for DefaultOverlayBackend {
    fn draw_trail_segment(
        &mut self,
        _from: (f32, f32),
        _to: (f32, f32),
        _color: [u8; 4],
        _width: f32,
    ) {
    }

    fn clear_trail(&mut self) {}

    fn show_hint(&mut self, _text: &str, _position: (f32, f32)) {}

    fn hide_hint(&mut self) {}
}
