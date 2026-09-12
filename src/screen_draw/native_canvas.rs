//! Retained active Screen Draw canvas and its Windows window implementation.

use std::time::Duration;

use image::RgbaImage;

use super::geometry::{DesktopPointF64, VisibleSegment, visible_segment_fragments};

use super::{
    AnnotationDocument, AnnotationKind, ArrowAnnotation, CanvasBackground, DesktopPoint,
    DesktopRect, EraserDrag, LineAnnotation, RgbaColor, ScreenDrawSessionSnapshot, ScreenDrawTool,
    ShapeAnnotation, ShapeStyle, Stroke, TextAnnotation, TransientInk, annotation_hit_test,
    stroke_hit_test,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointerButton {
    Left,
    Right,
}

fn passive_fade_refresh_region(
    passive_mode: bool,
    annotations_visible: bool,
    transient_bounds: Option<DesktopRect>,
) -> Option<DesktopRect> {
    (passive_mode && annotations_visible)
        .then_some(transient_bounds)
        .flatten()
}

#[derive(Debug)]
pub(crate) struct CanvasDocument {
    bounds: DesktopRect,
    document: AnnotationDocument,
    active_stroke: Option<ActiveStroke>,
    active_shape: Option<(ScreenDrawTool, DesktopPoint, DesktopPoint, ShapeStyle)>,
    pending_text: Option<PendingText>,
    eraser_drag: Option<EraserDrag>,
    transient: TransientInk,
}

#[derive(Debug)]
struct ActiveStroke {
    tool: ScreenDrawTool,
    stroke: Stroke,
    /// Last raw desktop sample, even when that sample was hidden by the input island.
    raw_point: DesktopPoint,
}

#[derive(Debug, Clone, PartialEq)]
struct PendingText {
    origin: DesktopPoint,
    text: String,
    color: RgbaColor,
    font_size: f32,
}

impl CanvasDocument {
    pub(crate) fn new(bounds: DesktopRect) -> Self {
        Self {
            bounds,
            document: AnnotationDocument::default(),
            active_stroke: None,
            active_shape: None,
            pending_text: None,
            eraser_drag: None,
            transient: TransientInk::default(),
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
        button: PointerButton,
        point: DesktopPoint,
        color: RgbaColor,
        thickness: f32,
    ) -> Option<DesktopRect> {
        self.begin_pen_excluding(button, point, color, thickness, None)
    }

    fn begin_pen_excluding(
        &mut self,
        _button: PointerButton,
        point: DesktopPoint,
        color: RgbaColor,
        thickness: f32,
        input_exclusion: Option<DesktopRect>,
    ) -> Option<DesktopRect> {
        if !self.bounds.contains_point(point)
            || point_is_excluded(point, input_exclusion)
            || self.has_active_operation()
        {
            return None;
        }
        self.document.begin_drawing();
        let mut stroke = Stroke::new(color, thickness);
        // A duplicate initial sample makes a click a real zero-length segment:
        // ink starts on button-down and survives a later backing-store rebuild.
        stroke.push_mouse_point(point);
        stroke.push_mouse_point(point);
        self.active_stroke = Some(ActiveStroke {
            tool: ScreenDrawTool::Pen,
            stroke,
            raw_point: point,
        });
        dirty_segment(self.bounds, point, point, thickness)
    }

    pub(crate) fn extend_pen(&mut self, point: DesktopPoint) -> Option<PenSegment> {
        self.extend_active_stroke(point, None)?.first()
    }

    pub(crate) fn finish_pen(&mut self) -> bool {
        let Some(ActiveStroke {
            tool: ScreenDrawTool::Pen,
            stroke,
            ..
        }) = self.active_stroke.take()
        else {
            return false;
        };
        self.document.commit(AnnotationKind::Pen(stroke)).is_ok()
    }

    pub(crate) fn cancel_pen(&mut self) -> bool {
        self.cancel_active()
    }

    pub(crate) fn has_active_pen(&self) -> bool {
        self.active_stroke.is_some()
    }

    pub(crate) fn begin_tool(
        &mut self,
        tool: ScreenDrawTool,
        button: PointerButton,
        point: DesktopPoint,
        color: RgbaColor,
        thickness: f32,
        text_size: f32,
        now: Duration,
    ) -> Option<DesktopRect> {
        self.begin_tool_excluding(tool, button, point, color, thickness, text_size, now, None)
    }

    fn begin_tool_excluding(
        &mut self,
        tool: ScreenDrawTool,
        button: PointerButton,
        point: DesktopPoint,
        color: RgbaColor,
        thickness: f32,
        text_size: f32,
        now: Duration,
        input_exclusion: Option<DesktopRect>,
    ) -> Option<DesktopRect> {
        if tool == ScreenDrawTool::Pen {
            return self.begin_pen_excluding(button, point, color, thickness, input_exclusion);
        }
        if button != PointerButton::Left
            || !self.bounds.contains_point(point)
            || point_is_excluded(point, input_exclusion)
            || self.has_active_operation()
        {
            return None;
        }
        self.document.begin_drawing();
        match tool {
            ScreenDrawTool::Highlighter | ScreenDrawTool::FadingInk => {
                let mut stroke = Stroke::new(color, thickness);
                stroke.push_mouse_point(point);
                stroke.push_mouse_point(point);
                self.active_stroke = Some(ActiveStroke {
                    tool,
                    stroke,
                    raw_point: point,
                });
                dirty_segment(self.bounds, point, point, thickness)
            }
            ScreenDrawTool::StraightLine
            | ScreenDrawTool::Arrow
            | ScreenDrawTool::Rectangle
            | ScreenDrawTool::Ellipse => {
                self.active_shape = Some((tool, point, point, ShapeStyle { color, thickness }));
                primitive_bounds(self.bounds, tool, point, point, thickness)
            }
            ScreenDrawTool::Text => {
                self.pending_text = Some(PendingText {
                    origin: point,
                    text: String::new(),
                    color,
                    font_size: text_size,
                });
                Some(text_layout_bounds(point, "", text_size))
            }
            ScreenDrawTool::Eraser => {
                self.eraser_drag = Some(self.document.begin_eraser_drag());
                let (_, dirty) = self.erase_at(point, thickness.max(8.0) * 0.5, now);
                dirty.or_else(|| dirty_segment(self.bounds, point, point, thickness.max(8.0)))
            }
            ScreenDrawTool::Pen | ScreenDrawTool::Eyedropper => None,
        }
    }

    pub(crate) fn extend_tool(
        &mut self,
        point: DesktopPoint,
        eraser_tolerance: f32,
        now: Duration,
    ) -> Option<CanvasUpdate> {
        self.extend_tool_excluding(point, eraser_tolerance, now, None)
    }

    fn extend_tool_excluding(
        &mut self,
        point: DesktopPoint,
        eraser_tolerance: f32,
        now: Duration,
        input_exclusion: Option<DesktopRect>,
    ) -> Option<CanvasUpdate> {
        if self.active_stroke.is_some() {
            return self
                .extend_active_stroke(point, input_exclusion)
                .filter(|segments| !segments.is_empty())
                .map(CanvasUpdate::Segments);
        }
        if let Some((tool, from, to, style)) = self.active_shape.as_mut() {
            let old = primitive_bounds(self.bounds, *tool, *from, *to, style.thickness)?;
            *to = point;
            let new = primitive_bounds(self.bounds, *tool, *from, *to, style.thickness)?;
            return Some(CanvasUpdate::Preview(union_rect(old, new)));
        }
        if self.eraser_drag.is_some() {
            if point_is_excluded(point, input_exclusion) {
                return None;
            }
            let (removed, dirty) = self.erase_at(point, eraser_tolerance, now);
            return (removed > 0).then(|| CanvasUpdate::Rebuild(dirty.unwrap_or(self.bounds)));
        }
        None
    }

    fn extend_active_stroke(
        &mut self,
        point: DesktopPoint,
        input_exclusion: Option<DesktopRect>,
    ) -> Option<PenSegments> {
        let active = self.active_stroke.as_mut()?;
        let raw_from = active.raw_point;
        // Raw continuity advances even when the whole segment is hidden. The
        // retained endpoint deliberately does not, preventing a later exit
        // sample from reconnecting ink across the toolbar.
        active.raw_point = point;
        let visible = match input_exclusion {
            Some(exclusion) => visible_segment_fragments(raw_from, point, exclusion),
            None => vec![VisibleSegment {
                from: raw_from.into(),
                to: point.into(),
            }],
        };
        let mut updates = PenSegments::default();
        for fragment in visible {
            let (from, to) = visible_fragment_to_physical(fragment, input_exclusion);
            if from == to && raw_from != point {
                continue;
            }
            let retained = active.stroke.points.last()?.position;
            if retained != from {
                active.stroke.push_mouse_break(from);
            }
            active.stroke.push_mouse_point(to);
            if let Some(dirty) = dirty_segment(self.bounds, from, to, active.stroke.thickness) {
                updates.push(PenSegment {
                    from,
                    to,
                    color: active.stroke.color,
                    thickness: active.stroke.thickness,
                    dirty,
                });
            }
        }
        Some(updates)
    }

    fn erase_at(
        &mut self,
        point: DesktopPoint,
        tolerance: f32,
        now: Duration,
    ) -> (usize, Option<DesktopRect>) {
        let permanent_dirty = self
            .document
            .objects()
            .iter()
            .filter(|object| annotation_hit_test(object, point, tolerance))
            .filter_map(|object| annotation_bounds(self.bounds, &object.kind))
            .reduce(union_rect);
        let transient_dirty = self
            .transient
            .strokes()
            .iter()
            .filter(|entry| {
                entry.expires_at <= now || stroke_hit_test(&entry.stroke, point, tolerance)
            })
            .filter_map(|entry| stroke_bounds(self.bounds, &entry.stroke))
            .reduce(union_rect);
        let permanent = if let Some(drag) = self.eraser_drag.as_mut() {
            self.document.erase_at(drag, point, tolerance)
        } else {
            0
        };
        let transient = self.transient.erase_at(now, point, tolerance);
        let dirty = match (permanent_dirty, transient_dirty) {
            (Some(a), Some(b)) => Some(union_rect(a, b)),
            (a, b) => a.or(b),
        };
        (permanent + transient, dirty)
    }

    pub(crate) fn finish_tool(&mut self, now: Duration, fade_lifetime: Duration) -> bool {
        if let Some(ActiveStroke { tool, stroke, .. }) = self.active_stroke.take() {
            return match tool {
                ScreenDrawTool::Pen => self.document.commit(AnnotationKind::Pen(stroke)).is_ok(),
                ScreenDrawTool::Highlighter => self
                    .document
                    .commit(AnnotationKind::Highlighter(stroke))
                    .is_ok(),
                ScreenDrawTool::FadingInk => self.transient.add(stroke, now, fade_lifetime).is_ok(),
                _ => false,
            };
        }
        if let Some((tool, from, to, style)) = self.active_shape.take() {
            let kind = match tool {
                ScreenDrawTool::StraightLine => {
                    AnnotationKind::Line(LineAnnotation { from, to, style })
                }
                ScreenDrawTool::Arrow => AnnotationKind::Arrow(ArrowAnnotation { from, to, style }),
                ScreenDrawTool::Rectangle => {
                    AnnotationKind::Rectangle(ShapeAnnotation { from, to, style })
                }
                ScreenDrawTool::Ellipse => {
                    AnnotationKind::Ellipse(ShapeAnnotation { from, to, style })
                }
                _ => return false,
            };
            return self.document.commit(kind).is_ok();
        }
        if let Some(drag) = self.eraser_drag.take() {
            return self.document.commit_eraser_drag(drag) > 0;
        }
        false
    }

    pub(crate) fn preview(&self) -> Option<AnnotationKind> {
        if let Some(ActiveStroke { tool, stroke, .. }) = &self.active_stroke {
            return match tool {
                ScreenDrawTool::Pen => Some(AnnotationKind::Pen(stroke.clone())),
                ScreenDrawTool::Highlighter | ScreenDrawTool::FadingInk => {
                    Some(AnnotationKind::Highlighter(stroke.clone()))
                }
                _ => None,
            };
        }
        if let Some(shape) = self.active_shape.map(|(tool, from, to, style)| match tool {
            ScreenDrawTool::StraightLine => {
                AnnotationKind::Line(LineAnnotation { from, to, style })
            }
            ScreenDrawTool::Arrow => AnnotationKind::Arrow(ArrowAnnotation { from, to, style }),
            ScreenDrawTool::Rectangle => {
                AnnotationKind::Rectangle(ShapeAnnotation { from, to, style })
            }
            ScreenDrawTool::Ellipse => AnnotationKind::Ellipse(ShapeAnnotation { from, to, style }),
            _ => unreachable!("only shape tools are stored as active shapes"),
        }) {
            return Some(shape);
        }
        self.pending_text.as_ref().map(|text| {
            AnnotationKind::Text(TextAnnotation {
                text: text.text.clone(),
                bounds: text_layout_bounds(text.origin, &text.text, text.font_size),
                color: text.color,
                font_size: text.font_size,
            })
        })
    }

    pub(crate) fn transient_strokes(&self, now: Duration) -> Vec<Stroke> {
        self.transient.render_strokes_at(now)
    }

    pub(crate) fn export_snapshot(
        &self,
    ) -> (Vec<super::AnnotationObject>, Vec<super::TransientStroke>) {
        (self.document.objects().to_vec(), self.transient.snapshot())
    }

    pub(crate) fn next_fade_deadline(&self) -> Option<Duration> {
        self.transient.next_deadline()
    }

    pub(crate) fn prune_fading(&mut self, now: Duration) -> usize {
        self.transient.prune(now)
    }

    pub(crate) fn clear_all(&mut self) -> bool {
        let permanent = self.document.clear_all();
        let transient = self.transient.clear();
        permanent > 0 || transient > 0
    }

    pub(crate) fn transient_bounds(&self) -> Option<DesktopRect> {
        self.transient
            .strokes()
            .iter()
            .filter_map(|entry| stroke_bounds(self.bounds, &entry.stroke))
            .reduce(union_rect)
    }

    pub(crate) fn pending_text_bounds(&self) -> Option<DesktopRect> {
        self.pending_text
            .as_ref()
            .map(|text| text_layout_bounds(text.origin, &text.text, text.font_size))
    }

    pub(crate) fn text_editing(&self) -> bool {
        self.pending_text.is_some()
    }

    pub(crate) fn append_text(&mut self, character: char) -> bool {
        let Some(text) = self.pending_text.as_mut() else {
            return false;
        };
        if !character.is_control() || character == '\n' {
            text.text.push(character);
            true
        } else {
            false
        }
    }

    pub(crate) fn backspace_text(&mut self) -> bool {
        self.pending_text
            .as_mut()
            .is_some_and(|text| text.text.pop().is_some())
    }

    pub(crate) fn commit_text(&mut self) -> bool {
        let Some(text) = self.pending_text.take() else {
            return false;
        };
        if text.text.is_empty() {
            return false;
        }
        let bounds = text_layout_bounds(text.origin, &text.text, text.font_size);
        self.document
            .commit(AnnotationKind::Text(TextAnnotation {
                text: text.text,
                bounds,
                color: text.color,
                font_size: text.font_size,
            }))
            .is_ok()
    }

    pub(crate) fn cancel_active(&mut self) -> bool {
        let mut changed = self.active_stroke.take().is_some()
            | self.active_shape.take().is_some()
            | self.pending_text.take().is_some();
        if let Some(drag) = self.eraser_drag.take() {
            self.document.cancel_eraser_drag(drag);
            changed = true;
        }
        changed
    }

    pub(crate) fn has_active_operation(&self) -> bool {
        self.active_stroke.is_some()
            || self.active_shape.is_some()
            || self.pending_text.is_some()
            || self.eraser_drag.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum CanvasUpdate {
    Segments(PenSegments),
    Preview(DesktopRect),
    Rebuild(DesktopRect),
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct PenSegments {
    segments: [Option<PenSegment>; 2],
}

impl PenSegments {
    fn push(&mut self, segment: PenSegment) {
        if let Some(slot) = self.segments.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(segment);
        }
    }

    fn first(self) -> Option<PenSegment> {
        self.segments[0]
    }

    fn is_empty(self) -> bool {
        self.segments[0].is_none()
    }

    fn iter(self) -> impl Iterator<Item = PenSegment> {
        self.segments.into_iter().flatten()
    }

    fn dirty(self) -> Option<DesktopRect> {
        self.iter().map(|segment| segment.dirty).reduce(union_rect)
    }
}

fn point_is_excluded(point: DesktopPoint, exclusion: Option<DesktopRect>) -> bool {
    exclusion.is_some_and(|rect| rect.contains_point(point))
}

fn visible_fragment_to_physical(
    fragment: VisibleSegment,
    exclusion: Option<DesktopRect>,
) -> (DesktopPoint, DesktopPoint) {
    (
        visible_point_to_physical(fragment.from, fragment.to, exclusion),
        visible_point_to_physical(fragment.to, fragment.from, exclusion),
    )
}

/// Converts a clipped subpixel intersection to a physical sample. Rounding is
/// directed into the visible fragment. If that still lands on the toolbar's
/// closed boundary, the sample advances one physical pixel farther outward;
/// this may enlarge the gap but can never persist a sample inside the island.
fn visible_point_to_physical(
    point: DesktopPointF64,
    visible_toward: DesktopPointF64,
    exclusion: Option<DesktopRect>,
) -> DesktopPoint {
    fn directed(value: f64, toward: f64) -> i32 {
        let rounded = if toward < value {
            value.floor()
        } else if toward > value {
            value.ceil()
        } else {
            value.round()
        };
        rounded.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
    }

    let mut physical = DesktopPoint::new(
        directed(point.x, visible_toward.x),
        directed(point.y, visible_toward.y),
    );
    if let Some(rect) = exclusion {
        let on_or_inside_closed = |point: DesktopPoint| {
            i64::from(point.x) >= i64::from(rect.x)
                && i64::from(point.x) <= rect.right()
                && i64::from(point.y) >= i64::from(rect.y)
                && i64::from(point.y) <= rect.bottom()
        };
        if on_or_inside_closed(physical) {
            physical.x =
                physical
                    .x
                    .saturating_add((visible_toward.x - point.x).partial_cmp(&0.0).map_or(
                        0,
                        |ordering| match ordering {
                            std::cmp::Ordering::Less => -1,
                            std::cmp::Ordering::Equal => 0,
                            std::cmp::Ordering::Greater => 1,
                        },
                    ));
            physical.y =
                physical
                    .y
                    .saturating_add((visible_toward.y - point.y).partial_cmp(&0.0).map_or(
                        0,
                        |ordering| match ordering {
                            std::cmp::Ordering::Less => -1,
                            std::cmp::Ordering::Equal => 0,
                            std::cmp::Ordering::Greater => 1,
                        },
                    ));
        }
    }
    physical
}

pub(crate) fn text_layout_bounds(origin: DesktopPoint, text: &str, font_size: f32) -> DesktopRect {
    let (width, height) = crate::screen_draw::raster::measure_text(text, font_size);
    DesktopRect::new(origin.x, origin.y, width, height)
}

fn union_rect(a: DesktopRect, b: DesktopRect) -> DesktopRect {
    let left = a.x.min(b.x);
    let top = a.y.min(b.y);
    let right = a.right().max(b.right());
    let bottom = a.bottom().max(b.bottom());
    DesktopRect::new(
        left,
        top,
        u32::try_from(right - i64::from(left)).unwrap_or(u32::MAX),
        u32::try_from(bottom - i64::from(top)).unwrap_or(u32::MAX),
    )
}

fn stroke_bounds(canvas: DesktopRect, stroke: &Stroke) -> Option<DesktopRect> {
    let first = stroke.points.first()?.position;
    let mut bounds = dirty_segment(canvas, first, first, stroke.thickness)?;
    // Bounds deliberately remain conservative across subpath breaks. Including
    // every sample preserves the pre-break invalidation behavior without
    // causing any renderer or hit tester to connect the gap.
    for point in stroke.points.iter().skip(1) {
        if let Some(dirty) = dirty_segment(canvas, point.position, point.position, stroke.thickness)
        {
            bounds = union_rect(bounds, dirty);
        }
    }
    Some(bounds)
}

fn annotation_bounds(canvas: DesktopRect, kind: &AnnotationKind) -> Option<DesktopRect> {
    match kind {
        AnnotationKind::Pen(stroke) | AnnotationKind::Highlighter(stroke) => {
            stroke_bounds(canvas, stroke)
        }
        AnnotationKind::Line(line) => {
            dirty_segment(canvas, line.from, line.to, line.style.thickness)
        }
        AnnotationKind::Arrow(arrow) => primitive_bounds(
            canvas,
            ScreenDrawTool::Arrow,
            arrow.from,
            arrow.to,
            arrow.style.thickness,
        ),
        AnnotationKind::Rectangle(shape) | AnnotationKind::Ellipse(shape) => {
            dirty_segment(canvas, shape.from, shape.to, shape.style.thickness)
        }
        AnnotationKind::Text(text) => text.bounds.intersection(canvas),
    }
}

fn primitive_bounds(
    canvas: DesktopRect,
    tool: ScreenDrawTool,
    from: DesktopPoint,
    to: DesktopPoint,
    thickness: f32,
) -> Option<DesktopRect> {
    let effective = if tool == ScreenDrawTool::Arrow {
        24.0 + thickness.max(1.0) * 4.0
    } else {
        thickness
    };
    dirty_segment(canvas, from, to, effective)
}

pub(crate) fn sample_frozen_pixel(
    frozen: &RgbaImage,
    bounds: DesktopRect,
    point: DesktopPoint,
) -> Option<RgbaColor> {
    if frozen.dimensions() != (bounds.width, bounds.height) {
        return None;
    }
    let local = bounds.desktop_to_local(point)?;
    let pixel = frozen.get_pixel(local.x, local.y).0;
    Some(RgbaColor::rgba(pixel[0], pixel[1], pixel[2], pixel[3]))
}

fn sample_frozen_pixel_outside_exclusion(
    frozen: &RgbaImage,
    bounds: DesktopRect,
    point: DesktopPoint,
    input_exclusion: Option<DesktopRect>,
) -> Option<RgbaColor> {
    (!point_is_excluded(point, input_exclusion))
        .then(|| sample_frozen_pixel(frozen, bounds, point))
        .flatten()
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
    use std::{ffi::c_void, mem, ptr, time::Instant};

    use image::RgbaImage;
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, BitBlt, CreateCompatibleDC,
        CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, EndPaint, HBITMAP, HDC,
        InvalidateRect, PAINTSTRUCT, SRCCOPY, SelectObject,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetCapture, GetKeyState, ReleaseCapture, SetCapture,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
        GWLP_USERDATA, GetCursorPos, GetWindowLongPtrW, IDC_ARROW, IDC_CROSS, IDC_IBEAM, KillTimer,
        LoadCursorW, RegisterClassW, SW_HIDE, SW_SHOW, SetCursor, SetTimer, SetWindowLongPtrW,
        ShowWindow, WM_CAPTURECHANGED, WM_CHAR, WM_CLOSE, WM_DESTROY, WM_DISPLAYCHANGE, WM_KEYDOWN,
        WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WM_PAINT, WM_RBUTTONDOWN,
        WM_RBUTTONUP, WM_SETCURSOR, WM_TIMER, WNDCLASSW,
    };
    use windows::core::{PCWSTR, w};

    use super::*;
    use crate::screen_draw::hotkeys::{
        LocalShortcutAction, LocalShortcutInput, resolve_local_shortcut,
    };
    use crate::screen_draw::native_overlay::NativeOverlaySurface;
    use crate::screen_draw::window_layers::{
        ToolbarWindowInfo, ToolbarZOrderCeiling, interactive_canvas_extended_style,
        interactive_canvas_style, raise_interactive_canvas,
    };
    use crate::screen_draw::{ScreenDrawSettings, render_document_into, selected_background};

    const FADE_TIMER_ID: usize = 0x5344_4641;

    pub(crate) const WM_CANVAS_ESCAPE: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 0x35;
    pub(crate) const WM_CANVAS_DISPLAY_CHANGED: u32 =
        windows::Win32::UI::WindowsAndMessaging::WM_APP + 0x36;
    pub(crate) const WM_CANVAS_CLOSED: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 0x37;

    fn should_emit_closed(destroying_surfaces: bool) -> bool {
        !destroying_surfaces
    }

    struct BackingDib {
        dc: HDC,
        bitmap: HBITMAP,
        old_bitmap: windows::Win32::Graphics::Gdi::HGDIOBJ,
        bits: *mut u8,
        byte_len: usize,
        width: u32,
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
                width,
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

        unsafe fn copy_rgba_region(&mut self, image: &RgbaImage, offset_x: u32, offset_y: u32) {
            let target = unsafe { std::slice::from_raw_parts_mut(self.bits, self.byte_len) };
            for (y, row) in image.rows().enumerate() {
                for (x, source) in row.enumerate() {
                    let destination_index =
                        (((offset_y as usize + y) * self.width as usize) + offset_x as usize + x)
                            * 4;
                    target[destination_index..destination_index + 4]
                        .copy_from_slice(&[source[2], source[1], source[0], source[3]]);
                }
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
        backing: Option<BackingDib>,
        passive: Option<NativeOverlaySurface>,
        passive_transient: Vec<(DesktopRect, NativeOverlaySurface)>,
        toolbar_ceiling: ToolbarZOrderCeiling,
        toolbar_bounds: Option<DesktopRect>,
        passive_mode: bool,
        destroying_surfaces: bool,
        background: CanvasBackground,
        color: RgbaColor,
        thickness: f32,
        tool: ScreenDrawTool,
        settings: ScreenDrawSettings,
        started_at: Instant,
        fade_timer_armed: bool,
        input_enabled: bool,
        event: Box<dyn Fn(CanvasEvent) + Send>,
    }

    #[derive(Clone, Copy)]
    pub(crate) enum CanvasEvent {
        DocumentChanged,
        ToolChanged(ScreenDrawTool),
        ColorChanged(RgbaColor),
        ThicknessChanged(f32),
        AnnotationsVisibilityChanged(bool),
        Escape,
        DisplayChanged,
        Closed,
    }

    impl WindowState {
        fn rebuild(&mut self) -> Result<(), String> {
            let capture = self.snapshot.capture();
            let mut image = RgbaImage::new(self.bounds.width, self.bounds.height);
            let transient = self.canvas.transient_strokes(self.elapsed());
            render_document_into(
                &mut image,
                self.bounds.origin(),
                selected_background(self.background, &capture.image, self.bounds),
                self.canvas.document(),
                &transient,
            )
            .map_err(|error| format!("failed to rebuild Screen Draw canvas: {error:?}"))?;
            let Some(backing) = self.backing.as_mut() else {
                return Ok(());
            };
            unsafe {
                backing.copy_rgba(&image);
            }
            unsafe {
                let _ = InvalidateRect(self.hwnd, None, false);
            }
            if self.passive_mode {
                self.rebuild_passive()?;
            }
            Ok(())
        }

        fn rebuild_passive(&mut self) -> Result<(), String> {
            let mut image = RgbaImage::new(self.bounds.width, self.bounds.height);
            render_document_into(
                &mut image,
                self.bounds.origin(),
                crate::screen_draw::RasterBackground::Transparent,
                self.canvas.document(),
                &[],
            )
            .map_err(|error| format!("failed to rebuild passive Screen Draw overlay: {error:?}"))?;
            let Some(passive) = self.passive.as_mut() else {
                return Ok(());
            };
            passive.update(&image)?;
            self.rebuild_passive_transient()
        }

        fn rebuild_passive_transient(&mut self) -> Result<(), String> {
            if !self.canvas.document().annotations_visible() {
                for (_, surface) in &mut self.passive_transient {
                    surface.hide();
                }
                return Ok(());
            }
            let transient = self.canvas.transient_strokes(self.elapsed());
            if transient.is_empty() {
                self.passive_transient.clear();
                return Ok(());
            }
            let mut bounds: Vec<_> = transient
                .iter()
                .flat_map(|stroke| {
                    crate::screen_draw::raster::stroke_mask_tiles(self.bounds, stroke)
                })
                .collect();
            bounds.sort_by_key(|tile| (tile.y, tile.x));
            bounds.dedup();
            if self
                .passive_transient
                .iter()
                .map(|(bounds, _)| *bounds)
                .ne(bounds.iter().copied())
            {
                self.passive_transient.clear();
                for bounds in &bounds {
                    self.passive_transient.push((
                        *bounds,
                        NativeOverlaySurface::create(*bounds, self.toolbar_ceiling)?,
                    ));
                }
            }
            for (bounds, surface) in &mut self.passive_transient {
                let mut image = RgbaImage::new(bounds.width, bounds.height);
                crate::screen_draw::raster::render_annotations_into(
                    &mut image,
                    bounds.origin(),
                    crate::screen_draw::RasterBackground::Transparent,
                    &[],
                    true,
                    &transient,
                )
                .map_err(|error| format!("failed to rebuild passive fading ink: {error:?}"))?;
                surface.update(&image)?;
                surface.show();
            }
            Ok(())
        }

        fn rebuild_region(&mut self, dirty: DesktopRect, preview: bool) -> Result<(), String> {
            let Some(dirty) = dirty.intersection(self.bounds) else {
                return Ok(());
            };
            let capture = self.snapshot.capture();
            let mut image = RgbaImage::new(dirty.width, dirty.height);
            let transient = self.canvas.transient_strokes(self.elapsed());
            render_document_into(
                &mut image,
                dirty.origin(),
                selected_background(self.background, &capture.image, self.bounds),
                self.canvas.document(),
                &transient,
            )
            .map_err(|error| format!("failed to rebuild Screen Draw dirty region: {error:?}"))?;
            if preview {
                if let Some(kind) = self.canvas.preview() {
                    crate::screen_draw::raster::render_preview_into(
                        &mut image,
                        dirty.origin(),
                        &kind,
                    )
                    .map_err(|error| format!("failed to render tool preview: {error:?}"))?;
                }
            }
            let local = self
                .bounds
                .desktop_to_local(dirty.origin())
                .ok_or_else(|| "dirty region is outside Screen Draw canvas".to_string())?;
            let Some(backing) = self.backing.as_mut() else {
                return Ok(());
            };
            unsafe {
                backing.copy_rgba_region(&image, local.x, local.y);
            }
            self.invalidate(dirty);
            Ok(())
        }

        fn invalidate(&self, dirty: DesktopRect) {
            let local = RECT {
                left: dirty.x - self.bounds.x,
                top: dirty.y - self.bounds.y,
                right: (dirty.right() - i64::from(self.bounds.x)) as i32,
                bottom: (dirty.bottom() - i64::from(self.bounds.y)) as i32,
            };
            unsafe {
                let _ = InvalidateRect(self.hwnd, Some(&local), false);
            }
        }

        fn elapsed(&self) -> Duration {
            self.started_at.elapsed()
        }

        fn update_fade_timer(&mut self) {
            unsafe {
                if self.fade_timer_armed {
                    let _ = KillTimer(self.hwnd, FADE_TIMER_ID);
                    self.fade_timer_armed = false;
                }
            }
            if self.hwnd.0.is_null() {
                return;
            }
            if let Some(deadline) = self.canvas.next_fade_deadline() {
                let delay = deadline.saturating_sub(self.elapsed());
                let millis = delay.as_millis().clamp(1, 33) as u32;
                let armed = unsafe { SetTimer(self.hwnd, FADE_TIMER_ID, millis, None) };
                self.fade_timer_armed = armed != 0;
            }
        }

        fn cursor_point(&self) -> Option<DesktopPoint> {
            let mut point = POINT::default();
            unsafe {
                GetCursorPos(&mut point).ok()?;
            }
            Some(DesktopPoint::new(point.x, point.y))
        }

        fn down(&mut self, button: PointerButton) {
            if !self.input_enabled {
                return;
            }
            let Some(point) = self.cursor_point() else {
                return;
            };
            // Z-order normally routes toolbar clicks away from this HWND, but
            // captured or reordered native input must remain harmless too.
            if point_is_excluded(point, self.toolbar_bounds) {
                return;
            }
            if self.tool == ScreenDrawTool::Eyedropper {
                if button == PointerButton::Left {
                    let capture = self.snapshot.capture();
                    if let Some(sampled) = sample_frozen_pixel_outside_exclusion(
                        &capture.image,
                        self.bounds,
                        point,
                        self.toolbar_bounds,
                    ) {
                        self.color = sampled;
                        (self.event)(CanvasEvent::ColorChanged(self.color));
                    }
                }
                return;
            }
            let color = if self.tool == ScreenDrawTool::Highlighter {
                let [r, g, b, _] = self.color.channels();
                RgbaColor::rgba(r, g, b, self.settings.highlighter_alpha)
            } else {
                self.color
            };
            let annotations_were_visible = self.canvas.document().annotations_visible();
            if let Some(dirty) = self.canvas.begin_tool_excluding(
                self.tool,
                button,
                point,
                color,
                self.thickness,
                self.settings.text_size,
                self.elapsed(),
                self.toolbar_bounds,
            ) {
                if !annotations_were_visible && self.canvas.document().annotations_visible() {
                    (self.event)(CanvasEvent::AnnotationsVisibilityChanged(true));
                }
                if self.tool == ScreenDrawTool::Text {
                    let _ = self.rebuild_region(dirty, true);
                    return;
                }
                unsafe {
                    SetCapture(self.hwnd);
                }
                if self.tool == ScreenDrawTool::Pen {
                    self.draw_segment(PenSegment {
                        from: point,
                        to: point,
                        color,
                        thickness: self.thickness,
                        dirty,
                    });
                } else {
                    let _ = self.rebuild_region(dirty, true);
                }
            }
        }

        fn move_pointer(&mut self) {
            if !self.input_enabled || !self.canvas.has_active_operation() {
                return;
            }
            if let Some(point) = self.cursor_point() {
                if let Some(update) = self.canvas.extend_tool_excluding(
                    point,
                    self.thickness.max(8.0) * 0.5,
                    self.elapsed(),
                    self.toolbar_bounds,
                ) {
                    match update {
                        CanvasUpdate::Segments(segments) if self.tool == ScreenDrawTool::Pen => {
                            for segment in segments.iter() {
                                self.draw_segment(segment);
                            }
                        }
                        CanvasUpdate::Segments(segments) => {
                            if let Some(dirty) = segments.dirty() {
                                let _ = self.rebuild_region(dirty, true);
                            }
                        }
                        CanvasUpdate::Preview(dirty) => {
                            let _ = self.rebuild_region(dirty, true);
                        }
                        CanvasUpdate::Rebuild(dirty) => {
                            let _ = self.rebuild_region(dirty, true);
                        }
                    }
                }
            }
        }

        fn up(&mut self) {
            self.move_pointer();
            let committed = self.canvas.finish_tool(
                self.elapsed(),
                Duration::from_secs(u64::from(self.settings.fade_duration_seconds)),
            );
            unsafe {
                if GetCapture() == self.hwnd {
                    let _ = ReleaseCapture();
                }
            }
            if committed {
                (self.event)(CanvasEvent::DocumentChanged);
            }
            self.update_fade_timer();
        }

        fn handle_character(&mut self, character: char) {
            if !self.canvas.text_editing() || character == '\r' || character == '\u{8}' {
                return;
            }
            let old = self.canvas.pending_text_bounds();
            if self.canvas.append_text(character) {
                if let (Some(old), Some(new)) = (old, self.canvas.pending_text_bounds()) {
                    let _ = self.rebuild_region(union_rect(old, new), true);
                }
            }
        }

        fn handle_key(&mut self, virtual_key: u32, repeat: bool) -> bool {
            if virtual_key == 0x1B {
                return false;
            }
            if self.canvas.text_editing() {
                match virtual_key {
                    0x08 => {
                        let old = self.canvas.pending_text_bounds();
                        if self.canvas.backspace_text() {
                            if let (Some(old), Some(new)) = (old, self.canvas.pending_text_bounds())
                            {
                                let _ = self.rebuild_region(union_rect(old, new), true);
                            }
                        }
                        return true;
                    }
                    0x0D => {
                        let shift = unsafe { GetKeyState(0x10) } < 0;
                        if shift {
                            let old = self.canvas.pending_text_bounds();
                            self.canvas.append_text('\n');
                            if let (Some(old), Some(new)) = (old, self.canvas.pending_text_bounds())
                            {
                                let _ = self.rebuild_region(union_rect(old, new), true);
                            }
                        } else if self.canvas.commit_text() {
                            let _ = self.rebuild();
                            (self.event)(CanvasEvent::DocumentChanged);
                        }
                        return true;
                    }
                    _ => {}
                }
            }
            if repeat {
                return false;
            }
            let modifiers = current_modifiers();
            let Some(input) =
                LocalShortcutInput::from_native(virtual_key, modifiers, self.canvas.text_editing())
            else {
                return false;
            };
            let Some(action) = resolve_local_shortcut(&self.settings, input) else {
                return false;
            };
            match action {
                LocalShortcutAction::Tool(tool) => {
                    self.set_tool_value(tool);
                    (self.event)(CanvasEvent::ToolChanged(tool));
                }
                LocalShortcutAction::Undo => {
                    if self.undo_value() {
                        (self.event)(CanvasEvent::DocumentChanged);
                    }
                }
                LocalShortcutAction::Redo => {
                    if self.redo_value() {
                        (self.event)(CanvasEvent::DocumentChanged);
                    }
                }
                LocalShortcutAction::IncreaseThickness => {
                    self.thickness = (self.thickness + 1.0).clamp(0.5, 64.0);
                    (self.event)(CanvasEvent::ThicknessChanged(self.thickness));
                }
                LocalShortcutAction::DecreaseThickness => {
                    self.thickness = (self.thickness - 1.0).clamp(0.5, 64.0);
                    (self.event)(CanvasEvent::ThicknessChanged(self.thickness));
                }
                LocalShortcutAction::Color(color) => {
                    self.color = color;
                    (self.event)(CanvasEvent::ColorChanged(color));
                }
            }
            true
        }

        fn set_tool_value(&mut self, tool: ScreenDrawTool) {
            if self.canvas.cancel_active() {
                let _ = self.rebuild();
            }
            self.tool = tool;
        }

        fn undo_value(&mut self) -> bool {
            let cancelled = self.canvas.cancel_active();
            let changed = self.canvas.document_mut().undo();
            if changed || cancelled {
                let _ = self.rebuild();
            }
            changed
        }

        fn redo_value(&mut self) -> bool {
            let cancelled = self.canvas.cancel_active();
            let changed = self.canvas.document_mut().redo();
            if changed || cancelled {
                let _ = self.rebuild();
            }
            changed
        }

        fn draw_segment(&mut self, segment: PenSegment) {
            let Some(backing) = self.backing.as_ref() else {
                return;
            };
            let rgb = segment.color.channels();
            unsafe {
                crate::platform::gdi_stroke::draw_solid_segment(
                    backing.dc,
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

        fn destroy_surfaces(&mut self) {
            if self.hwnd.0.is_null()
                && self.passive.is_none()
                && self.passive_transient.is_empty()
                && self.backing.is_none()
            {
                return;
            }
            self.input_enabled = false;
            self.passive_mode = false;
            if self.fade_timer_armed && !self.hwnd.0.is_null() {
                let _ = unsafe { KillTimer(self.hwnd, FADE_TIMER_ID) };
                self.fade_timer_armed = false;
            }
            unsafe {
                if GetCapture() == self.hwnd {
                    let _ = ReleaseCapture();
                }
            }
            // Drop the passive HWND/DIB first, then destroy the interactive
            // HWND while its WndProc can still access this retained state.
            self.passive.take();
            self.passive_transient.clear();
            let hwnd = std::mem::take(&mut self.hwnd);
            if !hwnd.0.is_null() {
                self.destroying_surfaces = true;
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                self.destroying_surfaces = false;
            }
            self.backing.take();
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
                if let Some(backing) = state.backing.as_ref() {
                    let _ = unsafe {
                        BitBlt(
                            dc,
                            r.left,
                            r.top,
                            r.right - r.left,
                            r.bottom - r.top,
                            backing.dc,
                            r.left,
                            r.top,
                            SRCCOPY,
                        )
                    };
                }
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
                if state.canvas.cancel_active() {
                    let _ = state.rebuild();
                }
                LRESULT(0)
            }
            WM_KEYDOWN => {
                let repeat = lparam.0 & (1 << 30) != 0;
                if wparam.0 == 0x1b && !repeat {
                    state.canvas.cancel_active();
                    (state.event)(CanvasEvent::Escape);
                } else {
                    state.handle_key(wparam.0 as u32, repeat);
                }
                LRESULT(0)
            }
            WM_CHAR => {
                if let Some(character) = char::from_u32(wparam.0 as u32) {
                    state.handle_character(character);
                }
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == FADE_TIMER_ID => {
                let dirty = state.canvas.transient_bounds();
                let removed = state.canvas.prune_fading(state.elapsed());
                if let Some(dirty) = dirty {
                    let preview = state.canvas.has_active_operation();
                    let _ = state.rebuild_region(dirty, preview);
                }
                if removed > 0 {
                    (state.event)(CanvasEvent::DocumentChanged);
                }
                if passive_fade_refresh_region(
                    state.passive_mode,
                    state.canvas.document().annotations_visible(),
                    dirty,
                )
                .is_some()
                {
                    let _ = state.rebuild_passive_transient();
                }
                state.update_fade_timer();
                LRESULT(0)
            }
            WM_SETCURSOR => {
                let cursor_id = match state.tool {
                    ScreenDrawTool::Text => IDC_IBEAM,
                    ScreenDrawTool::Eyedropper => IDC_ARROW,
                    _ => IDC_CROSS,
                };
                if let Ok(cursor) = unsafe { LoadCursorW(None, cursor_id) } {
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
                if should_emit_closed(state.destroying_surfaces) {
                    (state.event)(CanvasEvent::Closed);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                state.input_enabled = false;
                if state.fade_timer_armed {
                    let _ = unsafe { KillTimer(hwnd, FADE_TIMER_ID) };
                    state.fade_timer_armed = false;
                }
                unsafe {
                    if GetCapture() == hwnd {
                        let _ = ReleaseCapture();
                    }
                }
                if should_emit_closed(state.destroying_surfaces) {
                    (state.event)(CanvasEvent::Closed);
                }
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        }
    }

    fn current_modifiers() -> u32 {
        let mut modifiers = 0;
        unsafe {
            if GetKeyState(0x11) < 0 {
                modifiers |= 0x0002;
            }
            if GetKeyState(0x10) < 0 {
                modifiers |= 0x0004;
            }
            if GetKeyState(0x12) < 0 {
                modifiers |= 0x0001;
            }
            if GetKeyState(0x5B) < 0 || GetKeyState(0x5C) < 0 {
                modifiers |= 0x0008;
            }
        }
        modifiers
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
            settings: ScreenDrawSettings,
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
            let initial_background = settings.default_background;
            let mut state = Box::new(WindowState {
                hwnd: HWND::default(),
                bounds,
                snapshot,
                canvas: CanvasDocument::new(bounds),
                backing: Some(unsafe { BackingDib::new(bounds.width, bounds.height)? }),
                passive: Some(NativeOverlaySurface::create(
                    bounds,
                    ToolbarZOrderCeiling::default(),
                )?),
                passive_transient: Vec::new(),
                toolbar_ceiling: ToolbarZOrderCeiling::default(),
                toolbar_bounds: None,
                passive_mode: false,
                destroying_surfaces: false,
                background: initial_background,
                color,
                thickness,
                tool,
                settings,
                started_at: Instant::now(),
                fade_timer_armed: false,
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
                    interactive_canvas_extended_style(),
                    class_name,
                    PCWSTR::null(),
                    interactive_canvas_style(),
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
                state.destroy_surfaces();
                return Err(error);
            }
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
            raise_interactive_canvas(hwnd);
            Ok(Self { state })
        }

        pub(crate) fn release_pointer_capture(&mut self) {
            unsafe {
                if !self.state.hwnd.0.is_null() && GetCapture() == self.state.hwnd {
                    let _ = ReleaseCapture();
                }
            }
        }
        pub(crate) fn hide(&mut self) {
            self.state.input_enabled = false;
            if !self.state.hwnd.0.is_null() {
                unsafe {
                    let _ = ShowWindow(self.state.hwnd, SW_HIDE);
                }
            }
        }
        pub(crate) fn show_passive(&mut self, visible: bool) {
            self.state.passive_mode = true;
            let Some(passive) = self.state.passive.as_mut() else {
                return;
            };
            passive.hide();
            if visible {
                if self.state.rebuild_passive().is_ok() {
                    if let Some(passive) = self.state.passive.as_mut() {
                        passive.show();
                    }
                }
            }
        }
        pub(crate) fn hide_passive(&mut self) {
            self.state.passive_mode = false;
            if let Some(passive) = self.state.passive.as_mut() {
                passive.hide();
            }
            for (_, passive) in &mut self.state.passive_transient {
                passive.hide();
            }
        }
        pub(crate) fn show_export_preview(&mut self, image: &RgbaImage) -> Result<(), String> {
            self.state.passive_mode = true;
            for (_, transient) in &mut self.state.passive_transient {
                transient.hide();
            }
            let passive =
                self.state.passive.as_mut().ok_or_else(|| {
                    "Screen Draw passive preview surface is unavailable".to_string()
                })?;
            passive.hide();
            passive.update(image)?;
            passive.show();
            Ok(())
        }
        pub(crate) fn hide_all(&mut self) {
            self.hide();
            self.hide_passive();
        }
        pub(crate) fn destroy_surfaces(&mut self) {
            self.state.destroy_surfaces();
        }
        pub(crate) fn resume(&mut self) {
            self.hide_passive();
            self.state.input_enabled = true;
            let _ = self.state.rebuild();
            if !self.state.hwnd.0.is_null() {
                unsafe {
                    let _ = ShowWindow(self.state.hwnd, SW_SHOW);
                }
                raise_interactive_canvas(self.state.hwnd);
            }
        }
        pub(crate) fn set_toolbar_window(&mut self, info: Option<ToolbarWindowInfo>) {
            self.state.toolbar_bounds = info.map(|info| info.bounds);
            self.state
                .toolbar_ceiling
                .set(info.and_then(|info| info.handle));
            let ceiling = self.state.toolbar_ceiling;
            if let Some(passive) = self.state.passive.as_mut() {
                passive.set_toolbar_z_order_ceiling(ceiling);
            }
            for (_, passive) in &mut self.state.passive_transient {
                passive.set_toolbar_z_order_ceiling(ceiling);
            }
        }

        pub(crate) fn toolbar_bounds(&self) -> Option<DesktopRect> {
            self.state.toolbar_bounds
        }
        pub(crate) fn cancel_active(&mut self) -> bool {
            let changed = self.state.canvas.cancel_active();
            if changed {
                let _ = self.state.rebuild();
            }
            changed
        }
        pub(crate) fn set_color(&mut self, color: RgbaColor) {
            self.state.color = color;
        }
        pub(crate) fn set_tool(&mut self, tool: ScreenDrawTool) {
            self.state.set_tool_value(tool);
        }
        pub(crate) fn set_thickness(&mut self, thickness: f32) {
            self.state.thickness = thickness;
        }
        pub(crate) fn undo(&mut self) -> bool {
            self.state.undo_value()
        }
        pub(crate) fn redo(&mut self) -> bool {
            self.state.redo_value()
        }
        pub(crate) fn clear(&mut self) -> bool {
            let cancelled = self.state.canvas.cancel_active();
            let changed = self.state.canvas.clear_all();
            if changed || cancelled {
                let _ = self.state.rebuild();
            }
            self.state.update_fade_timer();
            changed
        }
        pub(crate) fn set_visible(&mut self, visible: bool) {
            self.state.canvas.cancel_active();
            self.state
                .canvas
                .document_mut()
                .set_annotations_visible(visible);
            let _ = self.state.rebuild();
            if self.state.passive_mode {
                if let Some(passive) = self.state.passive.as_mut() {
                    if visible {
                        passive.show();
                    } else {
                        passive.hide();
                    }
                }
                for (_, passive) in &mut self.state.passive_transient {
                    if visible {
                        passive.show();
                    } else {
                        passive.hide();
                    }
                }
            }
        }
        pub(crate) fn annotations_visible(&self) -> bool {
            self.state.canvas.document().annotations_visible()
        }
        pub(crate) fn export_source(&self) -> super::super::ExportSource {
            let (objects, transient) = self.state.canvas.export_snapshot();
            super::super::ExportSource {
                snapshot: self.state.snapshot.clone(),
                objects,
                transient,
                now: self.state.elapsed(),
            }
        }
        pub(crate) fn set_background(&mut self, background: CanvasBackground) {
            self.state.canvas.cancel_active();
            self.state.background = background;
            let _ = self.state.rebuild();
        }
    }

    impl Drop for NativeCanvasSurface {
        fn drop(&mut self) {
            self.state.destroy_surfaces();
        }
    }

    #[cfg(test)]
    mod tests {
        use super::should_emit_closed;

        #[test]
        fn intentional_surface_destruction_does_not_report_unexpected_close() {
            assert!(!should_emit_closed(true));
            assert!(should_emit_closed(false));
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
    use crate::screen_draw::{RasterBackground, render_document_into, selected_background};
    use image::RgbaImage;

    const TOOLBAR: DesktopRect = DesktopRect::new(100, 100, 250, 700);

    fn canvas_with_toolbar() -> CanvasDocument {
        CanvasDocument::new(DesktopRect::new(0, 0, 500, 900))
    }

    fn begin_excluding_toolbar(
        canvas: &mut CanvasDocument,
        tool: ScreenDrawTool,
        point: DesktopPoint,
    ) -> Option<DesktopRect> {
        canvas.begin_tool_excluding(
            tool,
            PointerButton::Left,
            point,
            RgbaColor::RED,
            4.0,
            24.0,
            Duration::ZERO,
            Some(TOOLBAR),
        )
    }

    fn committed_stroke(canvas: &CanvasDocument) -> &Stroke {
        match &canvas.document().objects()[0].kind {
            AnnotationKind::Pen(stroke) | AnnotationKind::Highlighter(stroke) => stroke,
            other => panic!("expected committed stroke, got {other:?}"),
        }
    }

    fn segment_pairs(update: Option<CanvasUpdate>) -> Vec<(DesktopPoint, DesktopPoint)> {
        let Some(CanvasUpdate::Segments(segments)) = update else {
            return Vec::new();
        };
        segments
            .iter()
            .map(|segment| (segment.from, segment.to))
            .collect()
    }

    #[test]
    fn passive_fade_refresh_is_bounded_and_idle_without_transient_ink() {
        let dirty = DesktopRect::new(-1800, -100, 120, 80);
        assert_eq!(
            passive_fade_refresh_region(true, true, Some(dirty)),
            Some(dirty)
        );
        assert_eq!(passive_fade_refresh_region(false, true, Some(dirty)), None);
        assert_eq!(passive_fade_refresh_region(true, false, Some(dirty)), None);
        assert_eq!(passive_fade_refresh_region(true, true, None), None);
    }

    #[test]
    fn every_tool_rejects_an_initial_press_inside_toolbar_before_an_operation_starts() {
        for tool in [
            ScreenDrawTool::Pen,
            ScreenDrawTool::Highlighter,
            ScreenDrawTool::FadingInk,
            ScreenDrawTool::Eraser,
            ScreenDrawTool::StraightLine,
            ScreenDrawTool::Arrow,
            ScreenDrawTool::Rectangle,
            ScreenDrawTool::Ellipse,
            ScreenDrawTool::Text,
            ScreenDrawTool::Eyedropper,
        ] {
            let mut canvas = canvas_with_toolbar();
            assert!(
                begin_excluding_toolbar(&mut canvas, tool, DesktopPoint::new(150, 200)).is_none(),
                "{tool:?}"
            );
            assert!(!canvas.has_active_operation(), "{tool:?}");
            assert!(canvas.document().objects().is_empty(), "{tool:?}");
        }

        // `down` uses this same guard before eyedropper sampling and before the
        // only branch that can call SetCapture.
        assert!(point_is_excluded(
            DesktopPoint::new(150, 200),
            Some(TOOLBAR)
        ));
        let frozen = RgbaImage::from_pixel(500, 900, image::Rgba([1, 2, 3, 255]));
        assert_eq!(
            sample_frozen_pixel_outside_exclusion(
                &frozen,
                DesktopRect::new(0, 0, 500, 900),
                DesktopPoint::new(150, 200),
                Some(TOOLBAR),
            ),
            None
        );
    }

    #[test]
    fn full_pen_crossing_emits_two_live_fragments_and_one_break_aware_undo_item() {
        let mut canvas = canvas_with_toolbar();
        assert!(
            begin_excluding_toolbar(&mut canvas, ScreenDrawTool::Pen, DesktopPoint::new(50, 200))
                .is_some()
        );
        assert_eq!(
            segment_pairs(canvas.extend_tool_excluding(
                DesktopPoint::new(400, 200),
                4.0,
                Duration::ZERO,
                Some(TOOLBAR),
            )),
            vec![
                (DesktopPoint::new(50, 200), DesktopPoint::new(99, 200)),
                (DesktopPoint::new(351, 200), DesktopPoint::new(400, 200)),
            ]
        );
        assert!(canvas.finish_tool(Duration::ZERO, Duration::from_secs(3)));

        let stroke = committed_stroke(&canvas);
        assert_eq!(
            stroke
                .points
                .iter()
                .map(|point| (point.position, point.break_before))
                .collect::<Vec<_>>(),
            vec![
                (DesktopPoint::new(50, 200), false),
                (DesktopPoint::new(50, 200), false),
                (DesktopPoint::new(99, 200), false),
                (DesktopPoint::new(351, 200), true),
                (DesktopPoint::new(400, 200), false),
            ]
        );
        assert_eq!(
            stroke
                .segments()
                .map(|segment| (segment.from, segment.to))
                .collect::<Vec<_>>(),
            vec![
                (DesktopPoint::new(50, 200), DesktopPoint::new(50, 200)),
                (DesktopPoint::new(50, 200), DesktopPoint::new(99, 200)),
                (DesktopPoint::new(351, 200), DesktopPoint::new(400, 200)),
            ]
        );
        let mut rendered = RgbaImage::new(500, 900);
        render_document_into(
            &mut rendered,
            DesktopPoint::new(0, 0),
            RasterBackground::Transparent,
            canvas.document(),
            &[],
        )
        .unwrap();
        assert_ne!(rendered.get_pixel(50, 200).0[3], 0);
        assert_eq!(rendered.get_pixel(200, 200).0[3], 0);
        assert_ne!(rendered.get_pixel(400, 200).0[3], 0);
        assert!(canvas.document().can_undo());
        assert!(canvas.document_mut().undo());
        assert!(canvas.document().objects().is_empty());
        assert!(!canvas.document_mut().undo());
    }

    #[test]
    fn pen_enter_stay_leave_uses_raw_continuity_and_reverse_crossing_is_symmetric() {
        let mut canvas = canvas_with_toolbar();
        begin_excluding_toolbar(&mut canvas, ScreenDrawTool::Pen, DesktopPoint::new(50, 200));
        assert_eq!(
            segment_pairs(canvas.extend_tool_excluding(
                DesktopPoint::new(150, 200),
                4.0,
                Duration::ZERO,
                Some(TOOLBAR),
            )),
            vec![(DesktopPoint::new(50, 200), DesktopPoint::new(99, 200))]
        );
        assert!(
            canvas
                .extend_tool_excluding(
                    DesktopPoint::new(250, 200),
                    4.0,
                    Duration::ZERO,
                    Some(TOOLBAR),
                )
                .is_none()
        );
        assert_eq!(
            segment_pairs(canvas.extend_tool_excluding(
                DesktopPoint::new(400, 200),
                4.0,
                Duration::ZERO,
                Some(TOOLBAR),
            )),
            vec![(DesktopPoint::new(351, 200), DesktopPoint::new(400, 200))]
        );

        let mut reverse = canvas_with_toolbar();
        begin_excluding_toolbar(
            &mut reverse,
            ScreenDrawTool::Pen,
            DesktopPoint::new(400, 200),
        );
        assert_eq!(
            segment_pairs(reverse.extend_tool_excluding(
                DesktopPoint::new(50, 200),
                4.0,
                Duration::ZERO,
                Some(TOOLBAR),
            )),
            vec![
                (DesktopPoint::new(400, 200), DesktopPoint::new(351, 200)),
                (DesktopPoint::new(99, 200), DesktopPoint::new(50, 200)),
            ]
        );
    }

    #[test]
    fn moving_toolbar_mid_drag_uses_current_bounds_without_reconnecting_old_hidden_samples() {
        let mut canvas = canvas_with_toolbar();
        begin_excluding_toolbar(&mut canvas, ScreenDrawTool::Pen, DesktopPoint::new(50, 200));
        canvas.extend_tool_excluding(
            DesktopPoint::new(150, 200),
            4.0,
            Duration::ZERO,
            Some(TOOLBAR),
        );
        let moved = DesktopRect::new(200, 100, 250, 700);
        assert_eq!(
            segment_pairs(canvas.extend_tool_excluding(
                DesktopPoint::new(180, 200),
                4.0,
                Duration::ZERO,
                Some(moved),
            )),
            vec![(DesktopPoint::new(150, 200), DesktopPoint::new(180, 200))]
        );
        let active = &canvas.active_stroke.as_ref().unwrap().stroke;
        assert!(
            active.points.iter().any(|point| {
                point.position == DesktopPoint::new(150, 200) && point.break_before
            })
        );
    }

    #[test]
    fn native_fragment_rounding_is_conservative_on_negative_desktop_coordinates() {
        let bounds = DesktopRect::new(-500, -300, 500, 500);
        let toolbar = DesktopRect::new(-300, -200, 100, 100);
        let mut canvas = CanvasDocument::new(bounds);
        assert!(
            canvas
                .begin_pen_excluding(
                    PointerButton::Left,
                    DesktopPoint::new(-400, -150),
                    RgbaColor::RED,
                    4.0,
                    Some(toolbar),
                )
                .is_some()
        );
        assert_eq!(
            segment_pairs(canvas.extend_tool_excluding(
                DesktopPoint::new(-100, -150),
                4.0,
                Duration::ZERO,
                Some(toolbar),
            )),
            vec![
                (DesktopPoint::new(-400, -150), DesktopPoint::new(-301, -150)),
                (DesktopPoint::new(-199, -150), DesktopPoint::new(-100, -150)),
            ]
        );
    }

    #[test]
    fn highlighter_and_fading_ink_preserve_the_same_toolbar_gap() {
        for tool in [ScreenDrawTool::Highlighter, ScreenDrawTool::FadingInk] {
            let mut canvas = canvas_with_toolbar();
            begin_excluding_toolbar(&mut canvas, tool, DesktopPoint::new(50, 200));
            assert_eq!(
                segment_pairs(canvas.extend_tool_excluding(
                    DesktopPoint::new(400, 200),
                    4.0,
                    Duration::ZERO,
                    Some(TOOLBAR),
                ))
                .len(),
                2
            );
            assert!(canvas.preview().is_some());
            assert!(canvas.finish_tool(Duration::ZERO, Duration::from_secs(3)));
            let stroke = if tool == ScreenDrawTool::Highlighter {
                committed_stroke(&canvas).clone()
            } else {
                canvas.transient_strokes(Duration::ZERO)[0].clone()
            };
            assert_eq!(
                stroke
                    .points
                    .iter()
                    .filter(|point| point.break_before)
                    .count(),
                1
            );

            let mut rendered = RgbaImage::new(500, 900);
            render_document_into(
                &mut rendered,
                DesktopPoint::new(0, 0),
                RasterBackground::Transparent,
                canvas.document(),
                &canvas.transient_strokes(Duration::ZERO),
            )
            .unwrap();
            assert_ne!(rendered.get_pixel(50, 200).0[3], 0, "{tool:?}");
            assert_eq!(rendered.get_pixel(200, 200).0[3], 0, "{tool:?}");
            assert_ne!(rendered.get_pixel(400, 200).0[3], 0, "{tool:?}");
        }
    }

    #[test]
    fn eraser_skips_toolbar_samples_resumes_outside_and_keeps_one_transaction() {
        let mut canvas = canvas_with_toolbar();
        for point in [DesktopPoint::new(150, 200), DesktopPoint::new(400, 200)] {
            canvas.begin_pen(PointerButton::Left, point, RgbaColor::RED, 8.0);
            canvas.finish_pen();
        }
        assert_eq!(canvas.document().objects().len(), 2);

        begin_excluding_toolbar(
            &mut canvas,
            ScreenDrawTool::Eraser,
            DesktopPoint::new(50, 50),
        );
        assert!(
            canvas
                .extend_tool_excluding(
                    DesktopPoint::new(150, 200),
                    12.0,
                    Duration::ZERO,
                    Some(TOOLBAR),
                )
                .is_none()
        );
        assert_eq!(canvas.document().objects().len(), 2);
        assert!(matches!(
            canvas.extend_tool_excluding(
                DesktopPoint::new(400, 200),
                12.0,
                Duration::ZERO,
                Some(TOOLBAR),
            ),
            Some(CanvasUpdate::Rebuild(_))
        ));
        assert!(canvas.finish_tool(Duration::ZERO, Duration::from_secs(3)));
        assert_eq!(canvas.document().objects().len(), 1);
        assert_eq!(
            committed_stroke(&canvas).points[0].position,
            DesktopPoint::new(150, 200)
        );
        assert!(canvas.document_mut().undo());
        assert_eq!(canvas.document().objects().len(), 2);
    }

    #[test]
    fn shapes_started_outside_remain_complete_across_toolbar() {
        for tool in [
            ScreenDrawTool::StraightLine,
            ScreenDrawTool::Arrow,
            ScreenDrawTool::Rectangle,
            ScreenDrawTool::Ellipse,
        ] {
            let mut canvas = canvas_with_toolbar();
            assert!(
                begin_excluding_toolbar(&mut canvas, tool, DesktopPoint::new(50, 200)).is_some()
            );
            assert!(matches!(
                canvas.extend_tool_excluding(
                    DesktopPoint::new(400, 200),
                    4.0,
                    Duration::ZERO,
                    Some(TOOLBAR),
                ),
                Some(CanvasUpdate::Preview(_))
            ));
            let preview = canvas.preview().unwrap();
            match preview {
                AnnotationKind::Line(line) => {
                    assert_eq!(
                        (line.from, line.to),
                        (DesktopPoint::new(50, 200), DesktopPoint::new(400, 200))
                    );
                }
                AnnotationKind::Arrow(arrow) => {
                    assert_eq!(
                        (arrow.from, arrow.to),
                        (DesktopPoint::new(50, 200), DesktopPoint::new(400, 200))
                    );
                }
                AnnotationKind::Rectangle(shape) | AnnotationKind::Ellipse(shape) => {
                    assert_eq!(
                        (shape.from, shape.to),
                        (DesktopPoint::new(50, 200), DesktopPoint::new(400, 200))
                    );
                }
                other => panic!("unexpected shape preview: {other:?}"),
            }
            assert!(canvas.finish_tool(Duration::ZERO, Duration::from_secs(3)));
            assert_eq!(canvas.document().objects().len(), 1);
        }
    }

    #[test]
    fn both_buttons_start_pen_immediately_without_deadzone() {
        for button in [PointerButton::Left, PointerButton::Right] {
            let mut canvas = CanvasDocument::new(DesktopRect::new(-100, -50, 200, 100));
            assert!(
                canvas
                    .begin_pen(button, DesktopPoint::new(-20, 5), RgbaColor::RED, 3.0)
                    .is_some()
            );
            assert_eq!(
                canvas.active_stroke.as_ref().unwrap().stroke.points.len(),
                2
            );
            assert_eq!(
                canvas.active_stroke.as_ref().unwrap().stroke.points[0].position,
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

    #[test]
    fn shape_preview_is_ephemeral_and_commits_exactly_once_on_release() {
        let bounds = DesktopRect::new(-100, -100, 200, 200);
        for tool in [
            ScreenDrawTool::StraightLine,
            ScreenDrawTool::Arrow,
            ScreenDrawTool::Rectangle,
            ScreenDrawTool::Ellipse,
        ] {
            let mut canvas = CanvasDocument::new(bounds);
            canvas.begin_tool(
                tool,
                PointerButton::Left,
                DesktopPoint::new(-50, -40),
                RgbaColor::WHITE,
                3.0,
                24.0,
                Duration::ZERO,
            );
            canvas.extend_tool(DesktopPoint::new(30, 20), 4.0, Duration::ZERO);
            assert!(canvas.preview().is_some());
            assert!(canvas.document().objects().is_empty());
            assert!(canvas.finish_tool(Duration::ZERO, Duration::from_secs(3)));
            assert_eq!(canvas.document().objects().len(), 1);
            assert!(!canvas.finish_tool(Duration::ZERO, Duration::from_secs(3)));
        }
    }

    #[test]
    fn non_pen_tools_reject_right_button_and_highlighter_preserves_alpha() {
        let mut canvas = CanvasDocument::new(DesktopRect::new(0, 0, 100, 100));
        assert!(
            canvas
                .begin_tool(
                    ScreenDrawTool::Highlighter,
                    PointerButton::Right,
                    DesktopPoint::new(10, 10),
                    RgbaColor::rgba(1, 2, 3, 80),
                    12.0,
                    24.0,
                    Duration::ZERO,
                )
                .is_none()
        );
        canvas.begin_tool(
            ScreenDrawTool::Highlighter,
            PointerButton::Left,
            DesktopPoint::new(10, 10),
            RgbaColor::rgba(1, 2, 3, 80),
            12.0,
            24.0,
            Duration::ZERO,
        );
        canvas.extend_tool(DesktopPoint::new(20, 10), 4.0, Duration::ZERO);
        assert!(canvas.finish_tool(Duration::ZERO, Duration::from_secs(3)));
        let AnnotationKind::Highlighter(stroke) = &canvas.document().objects()[0].kind else {
            panic!("expected highlighter")
        };
        assert_eq!(stroke.color, RgbaColor::rgba(1, 2, 3, 80));
    }

    #[test]
    fn native_text_state_supports_edit_multiline_commit_and_cancel() {
        let mut canvas = CanvasDocument::new(DesktopRect::new(-50, -50, 200, 200));
        canvas.begin_tool(
            ScreenDrawTool::Text,
            PointerButton::Left,
            DesktopPoint::new(-20, 5),
            RgbaColor::WHITE,
            2.0,
            20.0,
            Duration::ZERO,
        );
        assert!(canvas.text_editing());
        assert!(canvas.append_text('A'));
        assert!(canvas.append_text('B'));
        assert!(canvas.backspace_text());
        assert!(canvas.append_text('\n'));
        assert!(canvas.append_text('C'));
        assert!(canvas.commit_text());
        let AnnotationKind::Text(text) = &canvas.document().objects()[0].kind else {
            panic!("expected text")
        };
        assert_eq!(text.text, "A\nC");
        assert_eq!(text.bounds.origin(), DesktopPoint::new(-20, 5));
        assert!(text.bounds.width > 0);
        assert!(text.bounds.height >= 30);

        canvas.begin_tool(
            ScreenDrawTool::Text,
            PointerButton::Left,
            DesktopPoint::new(1, 1),
            RgbaColor::RED,
            2.0,
            20.0,
            Duration::ZERO,
        );
        canvas.append_text('X');
        assert!(canvas.cancel_active());
        assert_eq!(canvas.document().objects().len(), 1);
    }

    #[test]
    fn fading_ink_is_transient_fades_and_never_enters_undo_history() {
        let mut canvas = CanvasDocument::new(DesktopRect::new(0, 0, 100, 100));
        canvas.begin_tool(
            ScreenDrawTool::FadingInk,
            PointerButton::Left,
            DesktopPoint::new(10, 10),
            RgbaColor::rgba(10, 20, 30, 200),
            4.0,
            24.0,
            Duration::ZERO,
        );
        canvas.extend_tool(DesktopPoint::new(20, 20), 4.0, Duration::ZERO);
        assert!(canvas.finish_tool(Duration::ZERO, Duration::from_secs(2)));
        assert!(!canvas.document().can_undo());
        assert_eq!(canvas.next_fade_deadline(), Some(Duration::from_secs(2)));
        assert_eq!(
            canvas.transient_strokes(Duration::from_secs(1))[0]
                .color
                .channels()[3],
            100
        );
        assert_eq!(canvas.prune_fading(Duration::from_secs(2)), 1);
        assert!(canvas.next_fade_deadline().is_none());
    }

    #[test]
    fn eraser_drag_removes_permanent_and_transient_as_one_permanent_edit() {
        let mut canvas = CanvasDocument::new(DesktopRect::new(0, 0, 100, 100));
        for y in [10, 20] {
            canvas.begin_pen(
                PointerButton::Left,
                DesktopPoint::new(5, y),
                RgbaColor::RED,
                4.0,
            );
            canvas.extend_pen(DesktopPoint::new(30, y));
            canvas.finish_pen();
        }
        canvas.begin_tool(
            ScreenDrawTool::FadingInk,
            PointerButton::Left,
            DesktopPoint::new(5, 30),
            RgbaColor::WHITE,
            4.0,
            24.0,
            Duration::ZERO,
        );
        canvas.extend_tool(DesktopPoint::new(30, 30), 4.0, Duration::ZERO);
        canvas.finish_tool(Duration::ZERO, Duration::from_secs(3));

        canvas.begin_tool(
            ScreenDrawTool::Eraser,
            PointerButton::Left,
            DesktopPoint::new(10, 10),
            RgbaColor::WHITE,
            12.0,
            24.0,
            Duration::from_secs(1),
        );
        canvas.extend_tool(DesktopPoint::new(10, 20), 6.0, Duration::from_secs(1));
        canvas.extend_tool(DesktopPoint::new(10, 30), 6.0, Duration::from_secs(1));
        assert!(canvas.finish_tool(Duration::from_secs(1), Duration::from_secs(3)));
        assert!(canvas.document().objects().is_empty());
        assert!(canvas.transient_strokes(Duration::from_secs(1)).is_empty());
        assert!(canvas.document_mut().undo());
        assert_eq!(canvas.document().objects().len(), 2);
        assert!(canvas.transient_strokes(Duration::from_secs(1)).is_empty());
    }

    #[test]
    fn eyedropper_samples_only_signed_frozen_snapshot_coordinates() {
        let frozen = RgbaImage::from_fn(3, 2, |x, y| image::Rgba([x as u8, y as u8, 9, 255]));
        let bounds = DesktopRect::new(-10, -5, 3, 2);
        assert_eq!(
            sample_frozen_pixel(&frozen, bounds, DesktopPoint::new(-8, -4)),
            Some(RgbaColor::rgba(2, 1, 9, 255))
        );
        assert_eq!(
            sample_frozen_pixel(&frozen, bounds, DesktopPoint::new(0, 0)),
            None
        );
    }
}
