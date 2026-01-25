pub trait OverlayBackend: Send {
    fn draw_trail_segment(&mut self, from: (f32, f32), to: (f32, f32), color: [u8; 4], width: f32);
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
            self.started = false;
            self.last_point = None;
            self.start_point = None;
        }
    }

    pub fn reset(&mut self, start_point: (f32, f32)) {
        self.start_point = Some(start_point);
        self.last_point = None;
        self.started = false;
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
            visible: false,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.hide();
            self.last_tokens.clear();
            self.last_match = None;
        }
    }

    pub fn reset(&mut self) {
        self.hide();
        self.last_tokens.clear();
        self.last_match = None;
    }

    pub fn update(&mut self, tokens: &str, best_match: Option<&str>, cursor: (f32, f32)) {
        if !self.enabled {
            if self.visible {
                self.hide();
            }
            return;
        }

        let match_owned = best_match.map(|value| value.to_string());
        if tokens == self.last_tokens && match_owned.as_deref() == self.last_match.as_deref() {
            return;
        }

        self.last_tokens = tokens.to_string();
        self.last_match = match_owned;

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

        let position = (cursor.0 + self.offset.0, cursor.1 + self.offset.1);
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
pub struct DefaultOverlayBackend;

#[cfg(windows)]
impl OverlayBackend for DefaultOverlayBackend {
    fn draw_trail_segment(&mut self, from: (f32, f32), to: (f32, f32), color: [u8; 4], width: f32) {
        use windows::Win32::Foundation::COLORREF;
        use windows::Win32::Graphics::Gdi::{
            CreatePen, DeleteObject, GetDC, LineTo, MoveToEx, ReleaseDC, SelectObject, PS_SOLID,
        };
        use windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow;

        let hwnd = unsafe { GetDesktopWindow() };
        let hdc = unsafe { GetDC(hwnd) };
        if hdc.0.is_null() {
            return;
        }

        let colorref =
            COLORREF((color[0] as u32) | ((color[1] as u32) << 8) | ((color[2] as u32) << 16));
        let pen = unsafe { CreatePen(PS_SOLID, width.max(1.0) as i32, colorref) };
        let old_pen = unsafe { SelectObject(hdc, pen) };

        let _ = unsafe { MoveToEx(hdc, from.0 as i32, from.1 as i32, None) };
        let _ = unsafe { LineTo(hdc, to.0 as i32, to.1 as i32) };

        unsafe {
            SelectObject(hdc, old_pen);
            let _ = DeleteObject(pen);
            ReleaseDC(hwnd, hdc);
        }
    }

    fn show_hint(&mut self, text: &str, position: (f32, f32)) {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::Foundation::COLORREF;
        use windows::Win32::Graphics::Gdi::{
            GetDC, ReleaseDC, SetBkMode, SetTextColor, TextOutW, TRANSPARENT,
        };
        use windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow;

        let hwnd = unsafe { GetDesktopWindow() };
        let hdc = unsafe { GetDC(hwnd) };
        if hdc.0.is_null() {
            return;
        }

        let wide: Vec<u16> = OsStr::new(text).encode_wide().collect();
        unsafe {
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, COLORREF(0x00ffffff));
            let _ = TextOutW(hdc, position.0 as i32, position.1 as i32, &wide);
            ReleaseDC(hwnd, hdc);
        }
    }

    fn hide_hint(&mut self) {}
}

#[cfg(not(windows))]
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

    fn show_hint(&mut self, _text: &str, _position: (f32, f32)) {}

    fn hide_hint(&mut self) {}
}
