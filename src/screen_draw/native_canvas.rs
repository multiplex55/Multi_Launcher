//! Retained active Screen Draw canvas and its Windows window implementation.

use super::{
    AnnotationDocument, AnnotationKind, CanvasBackground, DesktopPoint, DesktopRect, RgbaColor,
    ScreenDrawSessionSnapshot, ScreenDrawTool, Stroke,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointerButton {
    Left,
    Right,
}

#[derive(Debug)]
pub(crate) struct CanvasDocument {
    bounds: DesktopRect,
    document: AnnotationDocument,
    active: Option<Stroke>,
}

impl CanvasDocument {
    pub(crate) fn new(bounds: DesktopRect) -> Self {
        Self {
            bounds,
            document: AnnotationDocument::default(),
            active: None,
        }
    }

    pub(crate) fn document(&self) -> &AnnotationDocument {
        &self.document
    }

    pub(crate) fn document_mut(&mut self) -> &mut AnnotationDocument {
        &mut self.document
    }

    pub(crate) fn begin_pen(
        &mut self,
        _button: PointerButton,
        point: DesktopPoint,
        color: RgbaColor,
        thickness: f32,
    ) -> Option<DesktopRect> {
        if !self.bounds.contains_point(point) || self.active.is_some() {
            return None;
        }
        self.document.begin_drawing();
        let mut stroke = Stroke::new(color, thickness);
        // A duplicate initial sample makes a click a real zero-length segment:
        // ink starts on button-down and survives a later backing-store rebuild.
        stroke.push_mouse_point(point);
        stroke.push_mouse_point(point);
        self.active = Some(stroke);
        dirty_segment(self.bounds, point, point, thickness)
    }

    pub(crate) fn extend_pen(&mut self, point: DesktopPoint) -> Option<PenSegment> {
        let stroke = self.active.as_mut()?;
        let from = stroke.points.last()?.position;
        stroke.push_mouse_point(point);
        Some(PenSegment {
            from,
            to: point,
            color: stroke.color,
            thickness: stroke.thickness,
            dirty: dirty_segment(self.bounds, from, point, stroke.thickness)?,
        })
    }

    pub(crate) fn finish_pen(&mut self) -> bool {
        let Some(stroke) = self.active.take() else {
            return false;
        };
        self.document.commit(AnnotationKind::Pen(stroke)).is_ok()
    }

    pub(crate) fn cancel_pen(&mut self) -> bool {
        self.active.take().is_some()
    }

    pub(crate) fn has_active_pen(&self) -> bool {
        self.active.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PenSegment {
    pub(crate) from: DesktopPoint,
    pub(crate) to: DesktopPoint,
    pub(crate) color: RgbaColor,
    pub(crate) thickness: f32,
    pub(crate) dirty: DesktopRect,
}

pub(crate) fn dirty_segment(
    canvas: DesktopRect,
    from: DesktopPoint,
    to: DesktopPoint,
    thickness: f32,
) -> Option<DesktopRect> {
    let margin = (thickness.max(1.0) * 0.5).ceil() as i64 + 2;
    let left = i64::from(from.x.min(to.x)) - margin;
    let top = i64::from(from.y.min(to.y)) - margin;
    let right = i64::from(from.x.max(to.x)) + margin + 1;
    let bottom = i64::from(from.y.max(to.y)) + margin + 1;
    let clipped_left = left.max(i64::from(canvas.x));
    let clipped_top = top.max(i64::from(canvas.y));
    let clipped_right = right.min(canvas.right());
    let clipped_bottom = bottom.min(canvas.bottom());
    if clipped_right <= clipped_left || clipped_bottom <= clipped_top {
        return None;
    }
    Some(DesktopRect::new(
        i32::try_from(clipped_left).ok()?,
        i32::try_from(clipped_top).ok()?,
        u32::try_from(clipped_right - clipped_left).ok()?,
        u32::try_from(clipped_bottom - clipped_top).ok()?,
    ))
}

#[cfg(windows)]
mod windows_canvas {
    use std::{ffi::c_void, mem, ptr};

    use image::RgbaImage;
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, BitBlt, CreateCompatibleDC,
        CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, EndPaint, HBITMAP, HDC,
        InvalidateRect, PAINTSTRUCT, SRCCOPY, SelectObject,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture};
    use windows::Win32::UI::WindowsAndMessaging::{
        CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
        GWLP_USERDATA, GetCursorPos, GetWindowLongPtrW, IDC_CROSS, LoadCursorW, RegisterClassW,
        SW_HIDE, SW_SHOW, SetCursor, SetWindowLongPtrW, ShowWindow, WM_CAPTURECHANGED, WM_CLOSE,
        WM_DESTROY, WM_DISPLAYCHANGE, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
        WM_NCCREATE, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WNDCLASSW,
        WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    };
    use windows::core::{PCWSTR, w};

    use super::*;
    use crate::screen_draw::{render_document_into, selected_background};

    pub(crate) const WM_CANVAS_ESCAPE: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 0x35;
    pub(crate) const WM_CANVAS_DISPLAY_CHANGED: u32 =
        windows::Win32::UI::WindowsAndMessaging::WM_APP + 0x36;
    pub(crate) const WM_CANVAS_CLOSED: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 0x37;

    struct BackingDib {
        dc: HDC,
        bitmap: HBITMAP,
        old_bitmap: windows::Win32::Graphics::Gdi::HGDIOBJ,
        bits: *mut u8,
        byte_len: usize,
    }

    impl BackingDib {
        unsafe fn new(width: u32, height: u32) -> Result<Self, String> {
            let width_i32 = i32::try_from(width).map_err(|_| "canvas width is too large")?;
            let height_i32 = i32::try_from(height).map_err(|_| "canvas height is too large")?;
            let byte_len = (width as usize)
                .checked_mul(height as usize)
                .and_then(|count| count.checked_mul(4))
                .ok_or("canvas allocation is too large")?;
            let dc = unsafe { CreateCompatibleDC(None) };
            if dc.0.is_null() {
                return Err("CreateCompatibleDC failed".into());
            }
            let info = BITMAPINFO {
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
            let bitmap =
                match unsafe { CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, None, 0) } {
                    Ok(value) if !bits.is_null() => value,
                    _ => {
                        let _ = unsafe { DeleteDC(dc) };
                        return Err("CreateDIBSection failed".into());
                    }
                };
            let old_bitmap = unsafe { SelectObject(dc, bitmap) };
            Ok(Self {
                dc,
                bitmap,
                old_bitmap,
                bits: bits.cast(),
                byte_len,
            })
        }

        unsafe fn copy_rgba(&mut self, image: &RgbaImage) {
            let target = unsafe { std::slice::from_raw_parts_mut(self.bits, self.byte_len) };
            for (source, destination) in image
                .as_raw()
                .chunks_exact(4)
                .zip(target.chunks_exact_mut(4))
            {
                destination.copy_from_slice(&[source[2], source[1], source[0], source[3]]);
            }
        }
    }

    impl Drop for BackingDib {
        fn drop(&mut self) {
            unsafe {
                let _ = SelectObject(self.dc, self.old_bitmap);
                let _ = DeleteObject(self.bitmap);
                let _ = DeleteDC(self.dc);
            }
        }
    }

    struct WindowState {
        hwnd: HWND,
        bounds: DesktopRect,
        snapshot: ScreenDrawSessionSnapshot,
        canvas: CanvasDocument,
        backing: BackingDib,
        background: CanvasBackground,
        color: RgbaColor,
        thickness: f32,
        tool: ScreenDrawTool,
        input_enabled: bool,
        event: Box<dyn Fn(CanvasEvent) + Send>,
    }

    #[derive(Clone, Copy)]
    pub(crate) enum CanvasEvent {
        DocumentChanged,
        Escape,
        DisplayChanged,
        Closed,
    }

    impl WindowState {
        fn rebuild(&mut self) -> Result<(), String> {
            let capture = self.snapshot.capture();
            let mut image = RgbaImage::new(self.bounds.width, self.bounds.height);
            render_document_into(
                &mut image,
                self.bounds.origin(),
                selected_background(self.background, &capture.image, self.bounds),
                self.canvas.document(),
                &[],
            )
            .map_err(|error| format!("failed to rebuild Screen Draw canvas: {error:?}"))?;
            unsafe {
                self.backing.copy_rgba(&image);
            }
            unsafe {
                let _ = InvalidateRect(self.hwnd, None, false);
            }
            Ok(())
        }

        fn cursor_point(&self) -> Option<DesktopPoint> {
            let mut point = POINT::default();
            unsafe {
                GetCursorPos(&mut point).ok()?;
            }
            Some(DesktopPoint::new(point.x, point.y))
        }

        fn down(&mut self, button: PointerButton) {
            if !self.input_enabled || self.tool != ScreenDrawTool::Pen {
                return;
            }
            let Some(point) = self.cursor_point() else {
                return;
            };
            if let Some(dirty) = self
                .canvas
                .begin_pen(button, point, self.color, self.thickness)
            {
                unsafe {
                    SetCapture(self.hwnd);
                }
                // Paint the zero-length first segment immediately.
                self.draw_segment(PenSegment {
                    from: point,
                    to: point,
                    color: self.color,
                    thickness: self.thickness,
                    dirty,
                });
            }
        }

        fn move_pointer(&mut self) {
            if !self.input_enabled || !self.canvas.has_active_pen() {
                return;
            }
            if let Some(point) = self.cursor_point() {
                if let Some(segment) = self.canvas.extend_pen(point) {
                    self.draw_segment(segment);
                }
            }
        }

        fn up(&mut self) {
            self.move_pointer();
            let committed = self.canvas.finish_pen();
            unsafe {
                if GetCapture() == self.hwnd {
                    let _ = ReleaseCapture();
                }
            }
            if committed {
                (self.event)(CanvasEvent::DocumentChanged);
            }
        }

        fn draw_segment(&mut self, segment: PenSegment) {
            let rgb = segment.color.channels();
            unsafe {
                crate::platform::gdi_stroke::draw_solid_segment(
                    self.backing.dc,
                    (segment.from.x as f32, segment.from.y as f32),
                    (segment.to.x as f32, segment.to.y as f32),
                    (self.bounds.x, self.bounds.y),
                    [rgb[0], rgb[1], rgb[2]],
                    segment.thickness,
                );
            }
            let local = RECT {
                left: segment.dirty.x - self.bounds.x,
                top: segment.dirty.y - self.bounds.y,
                right: (segment.dirty.right() - i64::from(self.bounds.x)) as i32,
                bottom: (segment.dirty.bottom() - i64::from(self.bounds.y)) as i32,
            };
            unsafe {
                let _ = InvalidateRect(self.hwnd, Some(&local as *const RECT), false);
            }
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
            let state = create.lpCreateParams as *mut WindowState;
            unsafe {
                (*state).hwnd = hwnd;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
            }
            return LRESULT(1);
        }
        let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
        let Some(state) = (unsafe { (state as *mut WindowState).as_mut() }) else {
            return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
        };
        match message {
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let dc = unsafe { BeginPaint(hwnd, &mut paint) };
                let r = paint.rcPaint;
                let _ = unsafe {
                    BitBlt(
                        dc,
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        state.backing.dc,
                        r.left,
                        r.top,
                        SRCCOPY,
                    )
                };
                unsafe {
                    let _ = EndPaint(hwnd, &paint);
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                state.down(PointerButton::Left);
                LRESULT(0)
            }
            WM_RBUTTONDOWN => {
                state.down(PointerButton::Right);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                state.move_pointer();
                LRESULT(0)
            }
            WM_LBUTTONUP | WM_RBUTTONUP => {
                state.up();
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                if state.canvas.cancel_pen() {
                    let _ = state.rebuild();
                }
                LRESULT(0)
            }
            WM_KEYDOWN if wparam.0 == 0x1b => {
                (state.event)(CanvasEvent::Escape);
                LRESULT(0)
            }
            WM_SETCURSOR => {
                if let Ok(cursor) = unsafe { LoadCursorW(None, IDC_CROSS) } {
                    unsafe {
                        SetCursor(cursor);
                    }
                }
                LRESULT(1)
            }
            WM_DISPLAYCHANGE => {
                (state.event)(CanvasEvent::DisplayChanged);
                LRESULT(0)
            }
            WM_CLOSE => {
                (state.event)(CanvasEvent::Closed);
                LRESULT(0)
            }
            WM_DESTROY => {
                state.input_enabled = false;
                unsafe {
                    if GetCapture() == hwnd {
                        let _ = ReleaseCapture();
                    }
                }
                (state.event)(CanvasEvent::Closed);
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    pub(crate) struct NativeCanvasSurface {
        state: Box<WindowState>,
    }
    // The surface is created, used, and destroyed by the dedicated native
    // worker. This marker only permits storage behind its `Send` trait seam;
    // ownership is never transferred after the HWND is created.
    unsafe impl Send for NativeCanvasSurface {}

    impl NativeCanvasSurface {
        pub(crate) fn create(
            snapshot: ScreenDrawSessionSnapshot,
            color: RgbaColor,
            thickness: f32,
            tool: ScreenDrawTool,
            event: Box<dyn Fn(CanvasEvent) + Send>,
        ) -> Result<Self, String> {
            let capture = snapshot.capture();
            let bounds = DesktopRect::new(
                capture.origin.0,
                capture.origin.1,
                capture.image.width(),
                capture.image.height(),
            );
            if bounds.is_empty() {
                return Err("Screen Draw capture is empty".into());
            }
            let mut state = Box::new(WindowState {
                hwnd: HWND::default(),
                bounds,
                snapshot,
                canvas: CanvasDocument::new(bounds),
                backing: unsafe { BackingDib::new(bounds.width, bounds.height)? },
                background: CanvasBackground::FrozenDesktop,
                color,
                thickness,
                tool,
                input_enabled: true,
                event,
            });
            let instance = HINSTANCE(
                unsafe { GetModuleHandleW(None) }
                    .map_err(|e| e.to_string())?
                    .0,
            );
            let class_name = w!("MultiLauncherScreenDrawCanvas");
            let class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: class_name,
                ..Default::default()
            };
            unsafe {
                RegisterClassW(&class);
            }
            let hwnd = unsafe {
                CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                    class_name,
                    PCWSTR::null(),
                    WS_POPUP,
                    bounds.x,
                    bounds.y,
                    i32::try_from(bounds.width).map_err(|_| "canvas width is too large")?,
                    i32::try_from(bounds.height).map_err(|_| "canvas height is too large")?,
                    None,
                    None,
                    instance,
                    Some((&mut *state as *mut WindowState).cast::<c_void>()),
                )
            };
            let hwnd = hwnd.map_err(|error| format!("CreateWindowExW failed: {error}"))?;
            state.hwnd = hwnd;
            if let Err(error) = state.rebuild() {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return Err(error);
            }
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
            Ok(Self { state })
        }

        pub(crate) fn release_pointer_capture(&mut self) {
            unsafe {
                if GetCapture() == self.state.hwnd {
                    let _ = ReleaseCapture();
                }
            }
        }
        pub(crate) fn hide(&mut self) {
            self.state.input_enabled = false;
            unsafe {
                let _ = ShowWindow(self.state.hwnd, SW_HIDE);
            }
        }
        pub(crate) fn resume(&mut self) {
            self.state.input_enabled = true;
            unsafe {
                let _ = ShowWindow(self.state.hwnd, SW_SHOW);
            }
        }
        pub(crate) fn cancel_active(&mut self) -> bool {
            let changed = self.state.canvas.cancel_pen();
            if changed {
                let _ = self.state.rebuild();
            }
            changed
        }
        pub(crate) fn set_color(&mut self, color: RgbaColor) {
            self.state.color = color;
        }
        pub(crate) fn set_tool(&mut self, tool: ScreenDrawTool) {
            if self.state.canvas.cancel_pen() {
                let _ = self.state.rebuild();
            }
            self.state.tool = tool;
        }
        pub(crate) fn set_thickness(&mut self, thickness: f32) {
            self.state.thickness = thickness;
        }
        pub(crate) fn undo(&mut self) -> bool {
            let cancelled = self.state.canvas.cancel_pen();
            let changed = self.state.canvas.document_mut().undo();
            if changed || cancelled {
                let _ = self.state.rebuild();
            }
            changed
        }
        pub(crate) fn redo(&mut self) -> bool {
            let cancelled = self.state.canvas.cancel_pen();
            let changed = self.state.canvas.document_mut().redo();
            if changed || cancelled {
                let _ = self.state.rebuild();
            }
            changed
        }
        pub(crate) fn clear(&mut self) -> bool {
            let cancelled = self.state.canvas.cancel_pen();
            let changed = self.state.canvas.document_mut().clear_all() != 0;
            if changed || cancelled {
                let _ = self.state.rebuild();
            }
            changed
        }
        pub(crate) fn set_visible(&mut self, visible: bool) {
            self.state.canvas.cancel_pen();
            self.state
                .canvas
                .document_mut()
                .set_annotations_visible(visible);
            let _ = self.state.rebuild();
        }
        pub(crate) fn set_background(&mut self, background: CanvasBackground) {
            self.state.canvas.cancel_pen();
            self.state.background = background;
            let _ = self.state.rebuild();
        }
    }

    impl Drop for NativeCanvasSurface {
        fn drop(&mut self) {
            self.release_pointer_capture();
            unsafe {
                let _ = DestroyWindow(self.state.hwnd);
            }
        }
    }
}

#[cfg(windows)]
pub(crate) use windows_canvas::{
    CanvasEvent, NativeCanvasSurface, WM_CANVAS_CLOSED, WM_CANVAS_DISPLAY_CHANGED, WM_CANVAS_ESCAPE,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen_draw::{render_document_into, selected_background};
    use image::RgbaImage;

    #[test]
    fn both_buttons_start_pen_immediately_without_deadzone() {
        for button in [PointerButton::Left, PointerButton::Right] {
            let mut canvas = CanvasDocument::new(DesktopRect::new(-100, -50, 200, 100));
            assert!(
                canvas
                    .begin_pen(button, DesktopPoint::new(-20, 5), RgbaColor::RED, 3.0)
                    .is_some()
            );
            assert_eq!(canvas.active.as_ref().unwrap().points.len(), 2);
            assert_eq!(
                canvas.active.as_ref().unwrap().points[0].position,
                DesktopPoint::new(-20, 5)
            );
            assert!(canvas.finish_pen());
            assert_eq!(canvas.document().objects().len(), 1);
        }
    }

    #[test]
    fn signed_desktop_points_and_dirty_bounds_are_preserved_and_clipped() {
        let bounds = DesktopRect::new(-1920, -1080, 3840, 2160);
        let dirty = dirty_segment(
            bounds,
            DesktopPoint::new(-1919, -1079),
            DesktopPoint::new(-1800, -900),
            6.0,
        )
        .unwrap();
        assert_eq!(dirty, DesktopRect::new(-1920, -1080, 126, 186));
        let mut canvas = CanvasDocument::new(bounds);
        canvas.begin_pen(
            PointerButton::Left,
            DesktopPoint::new(-1900, -1000),
            RgbaColor::RED,
            2.0,
        );
        assert_eq!(
            canvas
                .extend_pen(DesktopPoint::new(-1800, -950))
                .unwrap()
                .to,
            DesktopPoint::new(-1800, -950)
        );
    }

    #[test]
    fn cancel_discards_only_unfinished_pen_and_preserves_history() {
        let mut canvas = CanvasDocument::new(DesktopRect::new(0, 0, 100, 100));
        canvas.begin_pen(
            PointerButton::Left,
            DesktopPoint::new(1, 1),
            RgbaColor::RED,
            2.0,
        );
        canvas.finish_pen();
        canvas.begin_pen(
            PointerButton::Right,
            DesktopPoint::new(5, 5),
            RgbaColor::WHITE,
            4.0,
        );
        canvas.extend_pen(DesktopPoint::new(8, 8));
        assert!(canvas.cancel_pen());
        assert_eq!(canvas.document().objects().len(), 1);
        assert!(canvas.document_mut().undo());
        assert!(canvas.document().objects().is_empty());
    }

    #[test]
    fn background_rebuilds_do_not_mutate_annotation_history() {
        let bounds = DesktopRect::new(-10, -10, 20, 20);
        let mut canvas = CanvasDocument::new(bounds);
        canvas.begin_pen(
            PointerButton::Left,
            DesktopPoint::new(-5, -5),
            RgbaColor::RED,
            2.0,
        );
        canvas.extend_pen(DesktopPoint::new(5, 5));
        canvas.finish_pen();
        let frozen = RgbaImage::from_pixel(20, 20, image::Rgba([12, 34, 56, 255]));
        for background in [
            CanvasBackground::FrozenDesktop,
            CanvasBackground::White,
            CanvasBackground::Black,
            CanvasBackground::Solid(RgbaColor::rgba(4, 5, 6, 255)),
        ] {
            let mut output = RgbaImage::new(20, 20);
            render_document_into(
                &mut output,
                bounds.origin(),
                selected_background(background, &frozen, bounds),
                canvas.document(),
                &[],
            )
            .unwrap();
        }
        assert!(canvas.document().can_undo());
        assert_eq!(canvas.document().objects().len(), 1);
    }
}
