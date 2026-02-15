use crate::draw::model::{CanvasModel, Color, DrawObject, Geometry};
use crate::draw::overlay::OverlayWindow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl DirtyRect {
    pub fn from_points(a: (i32, i32), b: (i32, i32), pad: i32) -> Self {
        let min_x = a.0.min(b.0) - pad;
        let max_x = a.0.max(b.0) + pad;
        let min_y = a.1.min(b.1) - pad;
        let max_y = a.1.max(b.1) + pad;
        Self {
            x: min_x,
            y: min_y,
            width: (max_x - min_x + 1).max(1),
            height: (max_y - min_y + 1).max(1),
        }
    }

    pub fn union(self, other: DirtyRect) -> DirtyRect {
        let min_x = self.x.min(other.x);
        let min_y = self.y.min(other.y);
        let max_x = (self.x + self.width).max(other.x + other.width);
        let max_y = (self.y + self.height).max(other.y + other.height);
        DirtyRect {
            x: min_x,
            y: min_y,
            width: (max_x - min_x).max(1),
            height: (max_y - min_y).max(1),
        }
    }

    pub fn clamp(self, width: u32, height: u32) -> Option<DirtyRect> {
        let max_w = width as i32;
        let max_h = height as i32;
        let x0 = self.x.clamp(0, max_w);
        let y0 = self.y.clamp(0, max_h);
        let x1 = (self.x + self.width).clamp(0, max_w);
        let y1 = (self.y + self.height).clamp(0, max_h);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some(DirtyRect {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        })
    }
}

#[derive(Debug, Default)]
pub struct RenderFrameBuffer {
    rgba: Vec<u8>,
    bgra: Vec<u8>,
    size: (u32, u32),
    initialized: bool,
    #[cfg(test)]
    allocation_count: usize,
}

impl RenderFrameBuffer {
    fn ensure_size(&mut self, size: (u32, u32)) -> bool {
        let target_len = (size.0 as usize)
            .saturating_mul(size.1 as usize)
            .saturating_mul(4);
        let resized =
            self.size != size || self.rgba.len() != target_len || self.bgra.len() != target_len;
        if resized {
            self.rgba = vec![0; target_len];
            self.bgra = vec![0; target_len];
            self.size = size;
            self.initialized = false;
            #[cfg(test)]
            {
                self.allocation_count += 1;
            }
        }
        resized
    }

    pub fn render(
        &mut self,
        canvas: &CanvasModel,
        settings: RenderSettings,
        size: (u32, u32),
        dirty: Option<DirtyRect>,
        force_full_redraw: bool,
    ) {
        let resized = self.ensure_size(size);
        let full_redraw = force_full_redraw || resized || !self.initialized || dirty.is_none();

        if full_redraw {
            clear_rgba_pixels(&mut self.rgba, settings.clear_mode);
            for object in &canvas.objects {
                render_draw_object_rgba(
                    object,
                    settings.clear_mode,
                    &mut self.rgba,
                    size.0,
                    size.1,
                    None,
                );
            }
            convert_rgba_to_dib_bgra(&self.rgba, &mut self.bgra);
            self.initialized = true;
            return;
        }

        if let Some(dirty) = dirty.and_then(|d| d.clamp(size.0, size.1)) {
            clear_rect_rgba(&mut self.rgba, size.0, size.1, dirty, settings.clear_mode);
            for object in &canvas.objects {
                render_draw_object_rgba(
                    object,
                    settings.clear_mode,
                    &mut self.rgba,
                    size.0,
                    size.1,
                    Some(dirty),
                );
            }
            convert_rgba_to_bgra_rect(&self.rgba, &mut self.bgra, size.0, size.1, dirty);
            self.initialized = true;
        }
    }

    pub fn copy_to_window(&self, window: &mut OverlayWindow) {
        window.with_bitmap_mut(|dib, width, height| {
            if width == 0 || height == 0 || dib.len() != self.bgra.len() {
                return;
            }
            dib.copy_from_slice(&self.bgra);
        });
    }

    pub fn copy_rect_to_window(&self, window: &mut OverlayWindow, rect: DirtyRect) {
        let Some(rect) = rect.clamp(self.size.0, self.size.1) else {
            return;
        };
        window.with_bitmap_mut(|dib, width, height| {
            if width == 0 || height == 0 || dib.len() != self.bgra.len() {
                return;
            }
            for y in rect.y..(rect.y + rect.height) {
                for x in rect.x..(rect.x + rect.width) {
                    let idx = ((y as u32 * width + x as u32) * 4) as usize;
                    dib[idx..idx + 4].copy_from_slice(&self.bgra[idx..idx + 4]);
                }
            }
        });
    }

    #[cfg(test)]
    pub fn allocation_count(&self) -> usize {
        self.allocation_count
    }

    pub fn rgba_pixels(&self) -> &[u8] {
        &self.rgba
    }
}

#[derive(Debug, Default)]
pub struct LayeredRenderer {
    committed: RenderFrameBuffer,
    composed: RenderFrameBuffer,
    last_committed_revision: Option<u64>,
    #[cfg(test)]
    committed_rebuild_count: usize,
}

impl LayeredRenderer {
    pub fn render_to_window(
        &mut self,
        window: &mut OverlayWindow,
        committed_canvas: &CanvasModel,
        active_object: Option<&DrawObject>,
        settings: RenderSettings,
        size: (u32, u32),
        dirty: Option<DirtyRect>,
        force_full_redraw: bool,
        committed_revision: u64,
    ) {
        let committed_changed = force_full_redraw
            || self.last_committed_revision != Some(committed_revision)
            || self.committed.size != size
            || !self.committed.initialized;
        if committed_changed {
            self.committed
                .render(committed_canvas, settings, size, None, true);
            self.last_committed_revision = Some(committed_revision);
            #[cfg(test)]
            {
                self.committed_rebuild_count += 1;
            }
        }

        self.composed.ensure_size(size);
        let full_redraw = force_full_redraw || committed_changed || dirty.is_none();
        let dirty = dirty.and_then(|d| d.clamp(size.0, size.1));

        if full_redraw {
            self.composed.rgba.copy_from_slice(&self.committed.rgba);
            self.composed.bgra.copy_from_slice(&self.committed.bgra);
        } else if let Some(rect) = dirty {
            copy_rect(&self.committed.rgba, &mut self.composed.rgba, size.0, rect);
            copy_rect(&self.committed.bgra, &mut self.composed.bgra, size.0, rect);
        }

        if let Some(active) = active_object {
            if full_redraw {
                render_draw_object_rgba(
                    active,
                    settings.clear_mode,
                    &mut self.composed.rgba,
                    size.0,
                    size.1,
                    None,
                );
                convert_rgba_to_dib_bgra(&self.composed.rgba, &mut self.composed.bgra);
            } else if let Some(rect) = dirty {
                render_draw_object_rgba(
                    active,
                    settings.clear_mode,
                    &mut self.composed.rgba,
                    size.0,
                    size.1,
                    Some(rect),
                );
                convert_rgba_to_bgra_rect(
                    &self.composed.rgba,
                    &mut self.composed.bgra,
                    size.0,
                    size.1,
                    rect,
                );
            }
        }

        if full_redraw {
            self.composed.copy_to_window(window);
        } else if let Some(rect) = dirty {
            self.composed.copy_rect_to_window(window, rect);
        }
    }

    #[cfg(test)]
    pub fn committed_rebuild_count(&self) -> usize {
        self.committed_rebuild_count
    }
}

fn copy_rect(src: &[u8], dst: &mut [u8], width: u32, rect: DirtyRect) {
    for y in rect.y..(rect.y + rect.height) {
        for x in rect.x..(rect.x + rect.width) {
            let idx = ((y as u32 * width + x as u32) * 4) as usize;
            dst[idx..idx + 4].copy_from_slice(&src[idx..idx + 4]);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundClearMode {
    Transparent,
    Solid(Color),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderSettings {
    pub clear_mode: BackgroundClearMode,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            clear_mode: BackgroundClearMode::Transparent,
        }
    }
}

pub fn convert_rgba_to_dib_bgra(rgba: &[u8], dib_bgra: &mut [u8]) {
    assert_eq!(rgba.len(), dib_bgra.len());
    for (src, dst) in rgba.chunks_exact(4).zip(dib_bgra.chunks_exact_mut(4)) {
        dst[0] = src[2];
        dst[1] = src[1];
        dst[2] = src[0];
        dst[3] = src[3];
    }
}

pub fn render_canvas_to_rgba(
    canvas: &CanvasModel,
    settings: RenderSettings,
    size: (u32, u32),
) -> Vec<u8> {
    let (width, height) = size;
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
    clear_rgba_pixels(&mut pixels, settings.clear_mode);
    for object in &canvas.objects {
        render_draw_object_rgba(
            object,
            settings.clear_mode,
            &mut pixels,
            width,
            height,
            None,
        );
    }
    pixels
}

pub fn render_canvas_into_overlay(window: &mut OverlayWindow, canvas: &CanvasModel) {
    let rgba = render_canvas_to_rgba(canvas, RenderSettings::default(), window.bitmap_size());
    window.with_bitmap_mut(|pixels, width, height| {
        if width == 0 || height == 0 {
            return;
        }
        convert_rgba_to_dib_bgra(&rgba, pixels);
    });
}

pub fn render_canvas_to_pixels(canvas: &CanvasModel, pixels: &mut [u8], width: u32, height: u32) {
    let rgba = render_canvas_to_rgba(canvas, RenderSettings::default(), (width, height));
    convert_rgba_to_dib_bgra(&rgba, pixels);
}

fn clear_rgba_pixels(pixels: &mut [u8], mode: BackgroundClearMode) {
    let clear = match mode {
        BackgroundClearMode::Transparent => Color::rgba(0, 0, 0, 0),
        BackgroundClearMode::Solid(color) => color,
    };
    for px in pixels.chunks_exact_mut(4) {
        px.copy_from_slice(&[clear.r, clear.g, clear.b, clear.a]);
    }
}

fn render_draw_object_rgba(
    object: &DrawObject,
    clear_mode: BackgroundClearMode,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    clip_rect: Option<DirtyRect>,
) {
    let color = match object.geometry {
        Geometry::Eraser { .. } => match clear_mode {
            BackgroundClearMode::Transparent => Color::rgba(0, 0, 0, 0),
            BackgroundClearMode::Solid(color) => color,
        },
        _ => object.style.stroke.color,
    };
    let stroke_width = object.style.stroke.width.max(1);

    match &object.geometry {
        Geometry::Pen { points } | Geometry::Eraser { points } => draw_polyline(
            points,
            color,
            stroke_width,
            pixels,
            width,
            height,
            clip_rect,
        ),
        Geometry::Line { start, end } => draw_segment(
            *start,
            *end,
            color,
            stroke_width,
            pixels,
            width,
            height,
            clip_rect,
        ),
        Geometry::Rect { start, end } => draw_rect(
            *start,
            *end,
            color,
            stroke_width,
            pixels,
            width,
            height,
            clip_rect,
        ),
        Geometry::Ellipse { start, end } => draw_ellipse(
            *start,
            *end,
            color,
            stroke_width,
            pixels,
            width,
            height,
            clip_rect,
        ),
    }
}

fn draw_polyline(
    points: &[(i32, i32)],
    color: Color,
    stroke_width: u32,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    clip_rect: Option<DirtyRect>,
) {
    if points.is_empty() {
        return;
    }
    if points.len() == 1 {
        draw_brush(
            points[0],
            color,
            stroke_width,
            pixels,
            width,
            height,
            clip_rect,
        );
        return;
    }

    for segment in points.windows(2) {
        draw_segment(
            segment[0],
            segment[1],
            color,
            stroke_width,
            pixels,
            width,
            height,
            clip_rect,
        );
    }
}

fn draw_rect(
    start: (i32, i32),
    end: (i32, i32),
    color: Color,
    stroke_width: u32,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    clip_rect: Option<DirtyRect>,
) {
    let (x0, x1) = if start.0 <= end.0 {
        (start.0, end.0)
    } else {
        (end.0, start.0)
    };
    let (y0, y1) = if start.1 <= end.1 {
        (start.1, end.1)
    } else {
        (end.1, start.1)
    };

    draw_segment(
        (x0, y0),
        (x1, y0),
        color,
        stroke_width,
        pixels,
        width,
        height,
        clip_rect,
    );
    draw_segment(
        (x1, y0),
        (x1, y1),
        color,
        stroke_width,
        pixels,
        width,
        height,
        clip_rect,
    );
    draw_segment(
        (x1, y1),
        (x0, y1),
        color,
        stroke_width,
        pixels,
        width,
        height,
        clip_rect,
    );
    draw_segment(
        (x0, y1),
        (x0, y0),
        color,
        stroke_width,
        pixels,
        width,
        height,
        clip_rect,
    );
}

fn draw_ellipse(
    start: (i32, i32),
    end: (i32, i32),
    color: Color,
    stroke_width: u32,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    clip_rect: Option<DirtyRect>,
) {
    let min_x = start.0.min(end.0);
    let max_x = start.0.max(end.0);
    let min_y = start.1.min(end.1);
    let max_y = start.1.max(end.1);

    let rx = ((max_x - min_x).max(1) as f32) * 0.5;
    let ry = ((max_y - min_y).max(1) as f32) * 0.5;
    let cx = (min_x + max_x) as f32 * 0.5;
    let cy = (min_y + max_y) as f32 * 0.5;

    let circumference = std::f32::consts::TAU * rx.max(ry);
    let steps = circumference.max(12.0) as usize;

    for step in 0..=steps {
        let t = (step as f32 / steps as f32) * std::f32::consts::TAU;
        let x = (cx + rx * t.cos()).round() as i32;
        let y = (cy + ry * t.sin()).round() as i32;
        draw_brush(
            (x, y),
            color,
            stroke_width,
            pixels,
            width,
            height,
            clip_rect,
        );
    }
}

fn draw_segment(
    start: (i32, i32),
    end: (i32, i32),
    color: Color,
    stroke_width: u32,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    clip_rect: Option<DirtyRect>,
) {
    let mut x0 = start.0;
    let mut y0 = start.1;
    let x1 = end.0;
    let y1 = end.1;

    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        draw_brush(
            (x0, y0),
            color,
            stroke_width,
            pixels,
            width,
            height,
            clip_rect,
        );
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn draw_brush(
    center: (i32, i32),
    color: Color,
    stroke_width: u32,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    clip_rect: Option<DirtyRect>,
) {
    let radius = (stroke_width.saturating_sub(1) / 2) as i32;
    for y in (center.1 - radius)..=(center.1 + radius) {
        for x in (center.0 - radius)..=(center.0 + radius) {
            let dx = x - center.0;
            let dy = y - center.1;
            if dx * dx + dy * dy <= radius * radius {
                set_pixel_rgba(pixels, width, height, x, y, color, clip_rect);
            }
        }
    }
}

fn set_pixel_rgba(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    color: Color,
    clip_rect: Option<DirtyRect>,
) {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return;
    }

    if let Some(clip) = clip_rect {
        if x < clip.x || y < clip.y || x >= clip.x + clip.width || y >= clip.y + clip.height {
            return;
        }
    }

    let idx = ((y as u32 * width + x as u32) * 4) as usize;
    if idx + 3 >= pixels.len() {
        return;
    }

    pixels[idx] = color.r;
    pixels[idx + 1] = color.g;
    pixels[idx + 2] = color.b;
    pixels[idx + 3] = color.a;
}

fn clear_rect_rgba(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    rect: DirtyRect,
    mode: BackgroundClearMode,
) {
    let clear = match mode {
        BackgroundClearMode::Transparent => Color::rgba(0, 0, 0, 0),
        BackgroundClearMode::Solid(color) => color,
    };

    if let Some(rect) = rect.clamp(width, height) {
        for y in rect.y..(rect.y + rect.height) {
            for x in rect.x..(rect.x + rect.width) {
                let idx = ((y as u32 * width + x as u32) * 4) as usize;
                pixels[idx..idx + 4].copy_from_slice(&[clear.r, clear.g, clear.b, clear.a]);
            }
        }
    }
}

fn convert_rgba_to_bgra_rect(
    rgba: &[u8],
    bgra: &mut [u8],
    width: u32,
    height: u32,
    rect: DirtyRect,
) {
    let Some(rect) = rect.clamp(width, height) else {
        return;
    };
    for y in rect.y..(rect.y + rect.height) {
        for x in rect.x..(rect.x + rect.width) {
            let idx = ((y as u32 * width + x as u32) * 4) as usize;
            bgra[idx] = rgba[idx + 2];
            bgra[idx + 1] = rgba[idx + 1];
            bgra[idx + 2] = rgba[idx];
            bgra[idx + 3] = rgba[idx + 3];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        convert_rgba_to_dib_bgra, render_canvas_to_pixels, render_canvas_to_rgba, DirtyRect,
        LayeredRenderer, RenderFrameBuffer,
    };
    use crate::draw::{
        input::{DrawInputState, PointerModifiers},
        model::{CanvasModel, Color, DrawObject, Geometry, ObjectStyle, StrokeStyle, Tool},
        overlay::OverlayWindow,
    };
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn object(tool: Tool, geometry: Geometry) -> DrawObject {
        DrawObject {
            tool,
            style: ObjectStyle {
                stroke: StrokeStyle {
                    width: 3,
                    color: Color::rgba(255, 255, 255, 255),
                },
                fill: None,
            },
            geometry,
        }
    }

    fn object_with_width(tool: Tool, geometry: Geometry, width: u32) -> DrawObject {
        DrawObject {
            tool,
            style: ObjectStyle {
                stroke: StrokeStyle {
                    width,
                    color: Color::rgba(255, 255, 255, 255),
                },
                fill: None,
            },
            geometry,
        }
    }

    fn changed_pixels(canvas: CanvasModel) -> usize {
        let pixels = render_canvas_to_rgba(&canvas, super::RenderSettings::default(), (64, 64));
        pixels
            .chunks_exact(4)
            .filter(|px| px[0] != 0 || px[1] != 0 || px[2] != 0 || px[3] != 0)
            .count()
    }

    fn pixel_hash(pixels: &[u8]) -> u64 {
        let mut hasher = DefaultHasher::new();
        pixels.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn render_empty_canvas_respects_background_clear_mode() {
        let transparent = render_canvas_to_rgba(
            &CanvasModel::default(),
            super::RenderSettings {
                clear_mode: super::BackgroundClearMode::Transparent,
            },
            (2, 1),
        );
        assert_eq!(transparent, vec![0, 0, 0, 0, 0, 0, 0, 0]);

        let solid = render_canvas_to_rgba(
            &CanvasModel::default(),
            super::RenderSettings {
                clear_mode: super::BackgroundClearMode::Solid(Color::rgba(7, 8, 9, 255)),
            },
            (2, 1),
        );
        assert_eq!(solid, vec![7, 8, 9, 255, 7, 8, 9, 255]);
    }

    #[test]
    fn render_polyline_writes_nonzero_pixels() {
        let canvas = CanvasModel {
            objects: vec![object(
                Tool::Pen,
                Geometry::Pen {
                    points: vec![(2, 2), (8, 8), (12, 4)],
                },
            )],
        };

        let pixels = render_canvas_to_rgba(&canvas, super::RenderSettings::default(), (16, 16));
        assert!(pixels.chunks_exact(4).any(|px| px[3] != 0));
    }

    #[test]
    fn rasterizes_primitives_to_non_empty_pixels() {
        let pen_canvas = CanvasModel {
            objects: vec![object(
                Tool::Pen,
                Geometry::Pen {
                    points: vec![(2, 2), (8, 8), (14, 3)],
                },
            )],
        };
        assert!(changed_pixels(pen_canvas) > 0);

        let line_canvas = CanvasModel {
            objects: vec![object(
                Tool::Line,
                Geometry::Line {
                    start: (4, 4),
                    end: (30, 10),
                },
            )],
        };
        assert!(changed_pixels(line_canvas) > 0);

        let rect_canvas = CanvasModel {
            objects: vec![object(
                Tool::Rect,
                Geometry::Rect {
                    start: (8, 8),
                    end: (25, 22),
                },
            )],
        };
        assert!(changed_pixels(rect_canvas) > 0);

        let ellipse_canvas = CanvasModel {
            objects: vec![object(
                Tool::Ellipse,
                Geometry::Ellipse {
                    start: (12, 12),
                    end: (36, 28),
                },
            )],
        };
        assert!(changed_pixels(ellipse_canvas) > 0);
    }

    #[test]
    fn eraser_removes_pixels() {
        let base = object(
            Tool::Pen,
            Geometry::Pen {
                points: vec![(2, 2), (30, 30)],
            },
        );
        let eraser = object(
            Tool::Eraser,
            Geometry::Eraser {
                points: vec![(16, 16), (20, 20)],
            },
        );

        let without_eraser = changed_pixels(CanvasModel {
            objects: vec![base.clone()],
        });
        let with_eraser = changed_pixels(CanvasModel {
            objects: vec![base, eraser],
        });

        assert!(with_eraser < without_eraser);
    }

    #[test]
    fn undo_redo_changes_pixel_output_hash() {
        let mut input = DrawInputState::new(Tool::Line, ObjectStyle::default());
        let _ = input.handle_left_down((5, 5), PointerModifiers::default());
        input.handle_left_up((18, 18));
        let _ = input.handle_left_down((10, 30), PointerModifiers::default());
        input.handle_left_up((42, 30));

        let before = render_canvas_to_rgba(
            &input.history().canvas(),
            super::RenderSettings::default(),
            (64, 64),
        );
        let before_hash = pixel_hash(&before);

        let _ = input.handle_key_event(crate::draw::keyboard_hook::KeyEvent {
            key: crate::draw::keyboard_hook::KeyCode::U,
            modifiers: Default::default(),
        });
        let _ = input.handle_key_event(crate::draw::keyboard_hook::KeyEvent {
            key: crate::draw::keyboard_hook::KeyCode::KeyR,
            modifiers: crate::draw::keyboard_hook::KeyModifiers {
                ctrl: true,
                shift: false,
                alt: false,
                win: false,
            },
        });

        let after = render_canvas_to_rgba(
            &input.history().canvas(),
            super::RenderSettings::default(),
            (64, 64),
        );
        let after_hash = pixel_hash(&after);

        assert_eq!(before_hash, after_hash);
    }

    #[test]
    fn dib_upload_converts_channel_order_correctly() {
        let rgba = vec![10, 20, 30, 40, 50, 60, 70, 80];
        let mut bgra = vec![0; rgba.len()];
        convert_rgba_to_dib_bgra(&rgba, &mut bgra);
        assert_eq!(bgra, vec![30, 20, 10, 40, 70, 60, 50, 80]);
    }

    #[test]
    fn deterministic_snapshot_mixed_scene_matches_expected_pixels() {
        let canvas = CanvasModel {
            objects: vec![
                object_with_width(
                    Tool::Line,
                    Geometry::Line {
                        start: (0, 0),
                        end: (0, 0),
                    },
                    1,
                ),
                object_with_width(
                    Tool::Rect,
                    Geometry::Rect {
                        start: (1, 1),
                        end: (1, 1),
                    },
                    1,
                ),
                object_with_width(
                    Tool::Ellipse,
                    Geometry::Ellipse {
                        start: (2, 2),
                        end: (2, 2),
                    },
                    1,
                ),
            ],
        };

        let pixels = render_canvas_to_rgba(&canvas, super::RenderSettings::default(), (3, 3));
        let white = [255, 255, 255, 255];
        let transparent = [0, 0, 0, 0];
        let expected = vec![
            white[0],
            white[1],
            white[2],
            white[3],
            transparent[0],
            transparent[1],
            transparent[2],
            transparent[3],
            transparent[0],
            transparent[1],
            transparent[2],
            transparent[3],
            transparent[0],
            transparent[1],
            transparent[2],
            transparent[3],
            white[0],
            white[1],
            white[2],
            white[3],
            transparent[0],
            transparent[1],
            transparent[2],
            transparent[3],
            transparent[0],
            transparent[1],
            transparent[2],
            transparent[3],
            transparent[0],
            transparent[1],
            transparent[2],
            transparent[3],
            white[0],
            white[1],
            white[2],
            white[3],
        ];

        assert_eq!(pixels, expected);
    }

    #[test]
    fn rendering_is_bounds_safe_on_monitor_edges() {
        let canvas = CanvasModel {
            objects: vec![
                object(
                    Tool::Line,
                    Geometry::Line {
                        start: (-1000, -1000),
                        end: (1000, 1000),
                    },
                ),
                object(
                    Tool::Rect,
                    Geometry::Rect {
                        start: (-500, 2),
                        end: (4, 500),
                    },
                ),
                object(
                    Tool::Ellipse,
                    Geometry::Ellipse {
                        start: (5, -300),
                        end: (500, 5),
                    },
                ),
            ],
        };

        let mut pixels = vec![0u8; 8 * 8 * 4];
        render_canvas_to_pixels(&canvas, &mut pixels, 8, 8);

        assert_eq!(pixels.len(), 8 * 8 * 4);
    }

    #[test]
    fn untouched_pixels_are_initialized_to_colorkey_for_colorkey_pipeline() {
        let colorkey = crate::draw::model::FIRST_PASS_TRANSPARENCY_COLORKEY;
        let rgba = render_canvas_to_rgba(
            &CanvasModel::default(),
            super::RenderSettings {
                clear_mode: super::BackgroundClearMode::Solid(colorkey),
            },
            (2, 1),
        );
        assert_eq!(rgba, vec![255, 0, 255, 255, 255, 0, 255, 255]);

        let mut dib = vec![0u8; rgba.len()];
        super::convert_rgba_to_dib_bgra(&rgba, &mut dib);
        assert_eq!(dib, vec![255, 0, 255, 255, 255, 0, 255, 255]);
    }

    #[test]
    fn dirty_rect_incremental_render_matches_full_redraw() {
        let canvas = CanvasModel {
            objects: vec![
                object(
                    Tool::Pen,
                    Geometry::Pen {
                        points: vec![(2, 2), (20, 20), (30, 4)],
                    },
                ),
                object(
                    Tool::Rect,
                    Geometry::Rect {
                        start: (10, 10),
                        end: (35, 30),
                    },
                ),
            ],
        };
        let size = (64, 64);
        let settings = super::RenderSettings::default();

        let mut framebuffer = RenderFrameBuffer::default();
        framebuffer.render(&canvas, settings, size, None, true);
        framebuffer.render(
            &canvas,
            settings,
            size,
            Some(DirtyRect {
                x: 8,
                y: 8,
                width: 20,
                height: 20,
            }),
            false,
        );

        let full = render_canvas_to_rgba(&canvas, settings, size);
        assert_eq!(framebuffer.rgba_pixels(), full.as_slice());
    }

    #[test]
    fn framebuffer_transparent_mode_initializes_fully_transparent_surface() {
        let mut framebuffer = RenderFrameBuffer::default();
        framebuffer.render(
            &CanvasModel::default(),
            super::RenderSettings {
                clear_mode: super::BackgroundClearMode::Transparent,
            },
            (3, 2),
            None,
            true,
        );

        assert!(framebuffer
            .rgba_pixels()
            .chunks_exact(4)
            .all(|px| px == [0, 0, 0, 0]));
    }

    #[test]
    fn framebuffer_blank_mode_initializes_surface_with_solid_color() {
        let mut framebuffer = RenderFrameBuffer::default();
        framebuffer.render(
            &CanvasModel::default(),
            super::RenderSettings {
                clear_mode: super::BackgroundClearMode::Solid(Color::rgba(9, 8, 7, 255)),
            },
            (2, 2),
            None,
            true,
        );

        assert!(framebuffer
            .rgba_pixels()
            .chunks_exact(4)
            .all(|px| px == [9, 8, 7, 255]));
    }
    #[test]
    fn framebuffer_is_reused_for_same_dimensions() {
        let canvas = CanvasModel {
            objects: vec![object(
                Tool::Line,
                Geometry::Line {
                    start: (1, 1),
                    end: (30, 30),
                },
            )],
        };
        let mut framebuffer = RenderFrameBuffer::default();
        let settings = super::RenderSettings::default();
        let size = (48, 48);

        framebuffer.render(&canvas, settings, size, None, true);
        let initial_allocs = framebuffer.allocation_count();
        for _ in 0..4 {
            framebuffer.render(
                &canvas,
                settings,
                size,
                Some(DirtyRect {
                    x: 1,
                    y: 1,
                    width: 10,
                    height: 10,
                }),
                false,
            );
        }

        assert_eq!(framebuffer.allocation_count(), initial_allocs);
    }

    #[test]
    fn committed_layer_rebuilds_only_on_history_mutation() {
        let mut renderer = LayeredRenderer::default();
        let mut window = OverlayWindow::default();
        let settings = super::RenderSettings::default();
        let committed = CanvasModel {
            objects: vec![object(
                Tool::Line,
                Geometry::Line {
                    start: (1, 1),
                    end: (10, 10),
                },
            )],
        };
        let active = object(
            Tool::Line,
            Geometry::Line {
                start: (4, 4),
                end: (16, 16),
            },
        );

        renderer.render_to_window(
            &mut window,
            &committed,
            Some(&active),
            settings,
            (64, 64),
            Some(DirtyRect::from_points((4, 4), (16, 16), 5)),
            true,
            1,
        );
        renderer.render_to_window(
            &mut window,
            &committed,
            Some(&active),
            settings,
            (64, 64),
            Some(DirtyRect::from_points((4, 4), (20, 20), 5)),
            false,
            1,
        );
        assert_eq!(renderer.committed_rebuild_count(), 1);

        renderer.render_to_window(
            &mut window,
            &committed,
            None,
            settings,
            (64, 64),
            None,
            false,
            2,
        );
        assert_eq!(renderer.committed_rebuild_count(), 2);
    }
}
