use crate::draw::model::{CanvasModel, Color, DrawObject, Geometry, ObjectStyle, Tool};
use crate::draw::overlay::OverlayWindow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const DEFAULT_WIDE_STROKE_THRESHOLD: u32 = 10;
static WIDE_STROKE_THRESHOLD: AtomicU32 = AtomicU32::new(DEFAULT_WIDE_STROKE_THRESHOLD);

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
                    active_wide_stroke_threshold(settings),
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
                    active_wide_stroke_threshold(settings),
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
    #[cfg(test)]
    last_presented_dirty: Option<DirtyRect>,
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
        self.render_to_window_with_overlay(
            window,
            committed_canvas,
            active_object,
            settings,
            size,
            dirty,
            force_full_redraw,
            committed_revision,
            |_, _| None,
        );
    }

    pub fn render_to_window_with_overlay<F>(
        &mut self,
        window: &mut OverlayWindow,
        committed_canvas: &CanvasModel,
        active_object: Option<&DrawObject>,
        settings: RenderSettings,
        size: (u32, u32),
        dirty: Option<DirtyRect>,
        force_full_redraw: bool,
        committed_revision: u64,
        overlay_draw: F,
    ) where
        F: FnOnce(&mut [u8], (u32, u32)) -> Option<DirtyRect>,
    {
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

        let mut presented_dirty = dirty;
        if let Some(active) = active_object {
            if full_redraw {
                render_draw_object_rgba(
                    active,
                    settings.clear_mode,
                    active_wide_stroke_threshold(settings),
                    &mut self.composed.rgba,
                    size.0,
                    size.1,
                    None,
                );
                convert_rgba_to_dib_bgra(&self.composed.rgba, &mut self.composed.bgra);
            } else if let Some(rect) = dirty {
                if let Some((tool, style, start, end)) = freehand_preview_segment(active) {
                    let segment_dirty = render_incremental_segment_update(
                        &mut self.composed.rgba,
                        size.0,
                        size.1,
                        tool,
                        style,
                        start,
                        end,
                        settings.clear_mode,
                    );
                    if let Some(segment_dirty) = segment_dirty {
                        let merged = rect.union(segment_dirty).clamp(size.0, size.1);
                        if let Some(merged) = merged {
                            convert_rgba_to_bgra_rect(
                                &self.composed.rgba,
                                &mut self.composed.bgra,
                                size.0,
                                size.1,
                                merged,
                            );
                            presented_dirty = Some(merged);
                        }
                    } else {
                        presented_dirty = Some(rect);
                    }
                } else {
                    render_draw_object_rgba(
                        active,
                        settings.clear_mode,
                        active_wide_stroke_threshold(settings),
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
        }

        let overlay_dirty =
            overlay_draw(&mut self.composed.rgba, size).and_then(|rect| rect.clamp(size.0, size.1));
        if let Some(overlay_dirty) = overlay_dirty {
            presented_dirty = Some(
                presented_dirty
                    .map(|rect| rect.union(overlay_dirty))
                    .unwrap_or(overlay_dirty),
            );
        }

        if full_redraw {
            if overlay_dirty.is_some() {
                convert_rgba_to_dib_bgra(&self.composed.rgba, &mut self.composed.bgra);
            }
            self.composed.copy_to_window(window);
            #[cfg(test)]
            {
                self.last_presented_dirty = None;
            }
        } else if let Some(rect) = presented_dirty {
            self.composed.copy_rect_to_window(window, rect);
            #[cfg(test)]
            {
                self.last_presented_dirty = Some(rect);
            }
        }
    }

    #[cfg(test)]
    pub fn committed_rebuild_count(&self) -> usize {
        self.committed_rebuild_count
    }

    #[cfg(test)]
    pub fn last_presented_dirty(&self) -> Option<DirtyRect> {
        self.last_presented_dirty
    }
}

fn freehand_preview_segment(
    active: &DrawObject,
) -> Option<(Tool, ObjectStyle, (i32, i32), (i32, i32))> {
    match &active.geometry {
        Geometry::Pen { points } | Geometry::Eraser { points } => {
            if points.len() < 2 {
                return None;
            }
            let start = points[points.len() - 2];
            let end = points[points.len() - 1];
            Some((active.tool, active.style, start, end))
        }
        Geometry::Line { .. } | Geometry::Rect { .. } | Geometry::Ellipse { .. } => None,
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
    pub wide_stroke_threshold: u32,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            clear_mode: BackgroundClearMode::Transparent,
            wide_stroke_threshold: DEFAULT_WIDE_STROKE_THRESHOLD,
        }
    }
}

pub fn set_wide_stroke_threshold(threshold: u32) {
    WIDE_STROKE_THRESHOLD.store(threshold.max(1), Ordering::Relaxed);
}

fn active_wide_stroke_threshold(settings: RenderSettings) -> u32 {
    if settings.wide_stroke_threshold == DEFAULT_WIDE_STROKE_THRESHOLD {
        WIDE_STROKE_THRESHOLD.load(Ordering::Relaxed).max(1)
    } else {
        settings.wide_stroke_threshold.max(1)
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
            active_wide_stroke_threshold(settings),
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

pub fn segment_dirty_bounds(start: (i32, i32), end: (i32, i32), stroke_width: u32) -> DirtyRect {
    let radius = stroke_width.max(1) as i32;
    DirtyRect::from_points(start, end, radius + 2)
}

pub fn render_incremental_segment_update(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    tool: Tool,
    style: ObjectStyle,
    start: (i32, i32),
    end: (i32, i32),
    clear_mode: BackgroundClearMode,
) -> Option<DirtyRect> {
    if width == 0 || height == 0 {
        return None;
    }
    let dirty = segment_dirty_bounds(start, end, style.stroke.width.max(1)).clamp(width, height)?;
    let color = if matches!(tool, Tool::Eraser) {
        match clear_mode {
            BackgroundClearMode::Transparent => {
                crate::draw::model::FIRST_PASS_TRANSPARENCY_COLORKEY
            }
            BackgroundClearMode::Solid(color) => color,
        }
    } else {
        style.stroke.color
    };

    draw_segment(
        start,
        end,
        color,
        style.stroke.width.max(1),
        WIDE_STROKE_THRESHOLD.load(Ordering::Relaxed),
        pixels,
        width,
        height,
        Some(dirty),
    );
    Some(dirty)
}

pub fn render_shape_preview_update(
    preview_pixels: &mut [u8],
    width: u32,
    height: u32,
    object: &DrawObject,
    previous_bounds: Option<DirtyRect>,
    clear_mode: BackgroundClearMode,
) -> Option<DirtyRect> {
    let next_bounds = geometry_bounds(&object.geometry, object.style.stroke.width.max(1))?
        .clamp(width, height)?;
    if let Some(old) = previous_bounds.and_then(|r| r.clamp(width, height)) {
        clear_rect_rgba(
            preview_pixels,
            width,
            height,
            old,
            BackgroundClearMode::Solid(Color::rgba(0, 0, 0, 0)),
        );
        let dirty = old.union(next_bounds).clamp(width, height)?;
        render_draw_object_rgba(
            object,
            clear_mode,
            WIDE_STROKE_THRESHOLD.load(Ordering::Relaxed),
            preview_pixels,
            width,
            height,
            Some(next_bounds),
        );
        return Some(dirty);
    }

    render_draw_object_rgba(
        object,
        clear_mode,
        WIDE_STROKE_THRESHOLD.load(Ordering::Relaxed),
        preview_pixels,
        width,
        height,
        Some(next_bounds),
    );
    Some(next_bounds)
}

fn clear_rgba_pixels(pixels: &mut [u8], mode: BackgroundClearMode) {
    let clear = match mode {
        BackgroundClearMode::Transparent => crate::draw::model::FIRST_PASS_TRANSPARENCY_COLORKEY,
        BackgroundClearMode::Solid(color) => color,
    };
    for px in pixels.chunks_exact_mut(4) {
        px.copy_from_slice(&[clear.r, clear.g, clear.b, clear.a]);
    }
}

fn render_draw_object_rgba(
    object: &DrawObject,
    clear_mode: BackgroundClearMode,
    wide_stroke_threshold: u32,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    clip_rect: Option<DirtyRect>,
) {
    let color = match object.geometry {
        Geometry::Eraser { .. } => match clear_mode {
            BackgroundClearMode::Transparent => {
                crate::draw::model::FIRST_PASS_TRANSPARENCY_COLORKEY
            }
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
            wide_stroke_threshold,
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
            wide_stroke_threshold,
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
            wide_stroke_threshold,
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
            wide_stroke_threshold,
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
    wide_stroke_threshold: u32,
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
            stroke_width >= wide_stroke_threshold,
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
            wide_stroke_threshold,
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
    wide_stroke_threshold: u32,
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
        wide_stroke_threshold,
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
        wide_stroke_threshold,
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
        wide_stroke_threshold,
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
        wide_stroke_threshold,
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
    wide_stroke_threshold: u32,
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
            stroke_width >= wide_stroke_threshold,
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
    wide_stroke_threshold: u32,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    clip_rect: Option<DirtyRect>,
) {
    let started = Instant::now();
    let path = select_segment_render_path(start, end, stroke_width, wide_stroke_threshold);
    let operations = match path {
        SegmentRenderPath::LegacyDense => draw_segment_dense_stamped(
            start,
            end,
            color,
            stroke_width,
            pixels,
            width,
            height,
            clip_rect,
        ),
        SegmentRenderPath::AdaptiveStamp => draw_segment_adaptive_stamped(
            start,
            end,
            color,
            stroke_width,
            pixels,
            width,
            height,
            clip_rect,
        ),
        SegmentRenderPath::CapsuleRaster => draw_segment_capsule(
            start,
            end,
            color,
            stroke_width,
            pixels,
            width,
            height,
            clip_rect,
        ),
    };
    record_segment_cost(
        stroke_width,
        path,
        started.elapsed().as_nanos() as u64,
        operations,
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SegmentRenderPath {
    LegacyDense,
    AdaptiveStamp,
    CapsuleRaster,
}

impl SegmentRenderPath {
    fn as_label(self) -> &'static str {
        match self {
            SegmentRenderPath::LegacyDense => "legacy_dense",
            SegmentRenderPath::AdaptiveStamp => "adaptive_stamp",
            SegmentRenderPath::CapsuleRaster => "capsule_raster",
        }
    }
}

fn select_segment_render_path(
    start: (i32, i32),
    end: (i32, i32),
    stroke_width: u32,
    wide_stroke_threshold: u32,
) -> SegmentRenderPath {
    if stroke_width < wide_stroke_threshold {
        return SegmentRenderPath::LegacyDense;
    }

    let dx = (end.0 - start.0) as i64;
    let dy = (end.1 - start.1) as i64;
    let length_sq = dx * dx + dy * dy;
    if length_sq <= 2 {
        return SegmentRenderPath::LegacyDense;
    }

    if stroke_width >= 14 || length_sq >= (stroke_width as i64).saturating_mul(12) {
        SegmentRenderPath::CapsuleRaster
    } else {
        SegmentRenderPath::AdaptiveStamp
    }
}

fn draw_segment_dense_stamped(
    start: (i32, i32),
    end: (i32, i32),
    color: Color,
    stroke_width: u32,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    clip_rect: Option<DirtyRect>,
) -> u64 {
    let use_mask_cache = stroke_width >= 4;
    let mut x0 = start.0;
    let mut y0 = start.1;
    let x1 = end.0;
    let y1 = end.1;

    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut operations: u64 = 0;

    loop {
        operations = operations.saturating_add(draw_brush(
            (x0, y0),
            color,
            stroke_width,
            use_mask_cache,
            pixels,
            width,
            height,
            clip_rect,
        ));
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
    operations
}

fn draw_segment_adaptive_stamped(
    start: (i32, i32),
    end: (i32, i32),
    color: Color,
    stroke_width: u32,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    clip_rect: Option<DirtyRect>,
) -> u64 {
    let radius = (stroke_width.saturating_sub(1) / 2).max(1) as f32;
    let spacing = radius.max(1.0);
    let dx = (end.0 - start.0) as f32;
    let dy = (end.1 - start.1) as f32;
    let distance = (dx * dx + dy * dy).sqrt();
    let steps = (distance / spacing).ceil().max(1.0) as i32;
    let mut operations: u64 = 0;
    let mut last = (i32::MIN, i32::MIN);

    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let point = (
            (start.0 as f32 + dx * t).round() as i32,
            (start.1 as f32 + dy * t).round() as i32,
        );
        if point == last {
            continue;
        }
        last = point;
        operations = operations.saturating_add(draw_brush(
            point,
            color,
            stroke_width,
            true,
            pixels,
            width,
            height,
            clip_rect,
        ));
    }

    operations
}

fn draw_segment_capsule(
    start: (i32, i32),
    end: (i32, i32),
    color: Color,
    stroke_width: u32,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    clip_rect: Option<DirtyRect>,
) -> u64 {
    let radius = (stroke_width.saturating_sub(1) / 2) as f32;
    let pad = radius.ceil() as i32 + 1;
    let bounds = DirtyRect::from_points(start, end, pad);
    let clip = clip_rect
        .and_then(|clip| intersect_dirty_rect(bounds, clip))
        .unwrap_or(bounds)
        .clamp(width, height);
    let Some(clip) = clip else {
        return 0;
    };

    let radius_sq = radius * radius;
    let mut operations: u64 = 0;
    for y in clip.y..(clip.y + clip.height) {
        for x in clip.x..(clip.x + clip.width) {
            if point_segment_distance_sq((x, y), start, end) <= radius_sq {
                set_pixel_rgba(pixels, width, height, x, y, color, clip_rect);
                operations = operations.saturating_add(1);
            }
        }
    }
    operations
}

fn intersect_dirty_rect(a: DirtyRect, b: DirtyRect) -> Option<DirtyRect> {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.width).min(b.x + b.width);
    let y1 = (a.y + a.height).min(b.y + b.height);
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

fn point_segment_distance_sq(point: (i32, i32), start: (i32, i32), end: (i32, i32)) -> f32 {
    let px = point.0 as f32;
    let py = point.1 as f32;
    let x0 = start.0 as f32;
    let y0 = start.1 as f32;
    let x1 = end.0 as f32;
    let y1 = end.1 as f32;
    let vx = x1 - x0;
    let vy = y1 - y0;
    let wx = px - x0;
    let wy = py - y0;
    let len_sq = vx * vx + vy * vy;
    if len_sq <= f32::EPSILON {
        let dx = px - x0;
        let dy = py - y0;
        return dx * dx + dy * dy;
    }
    let t = ((wx * vx + wy * vy) / len_sq).clamp(0.0, 1.0);
    let cx = x0 + vx * t;
    let cy = y0 + vy * t;
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy
}

fn draw_brush(
    center: (i32, i32),
    color: Color,
    stroke_width: u32,
    use_mask_cache: bool,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    clip_rect: Option<DirtyRect>,
) -> u64 {
    let radius = (stroke_width.saturating_sub(1) / 2) as i32;
    if use_mask_cache {
        return draw_brush_mask(
            center,
            color,
            stroke_width,
            pixels,
            width,
            height,
            clip_rect,
        );
    }
    let mut writes: u64 = 0;
    for y in (center.1 - radius)..=(center.1 + radius) {
        for x in (center.0 - radius)..=(center.0 + radius) {
            let dx = x - center.0;
            let dy = y - center.1;
            if dx * dx + dy * dy <= radius * radius {
                set_pixel_rgba(pixels, width, height, x, y, color, clip_rect);
                writes = writes.saturating_add(1);
            }
        }
    }
    writes
}

#[derive(Clone)]
struct BrushMask {
    rows: Vec<BrushMaskRow>,
}

#[derive(Clone)]
struct BrushMaskRow {
    dy: i32,
    min_dx: i32,
    max_dx: i32,
}

fn brush_mask_cache() -> &'static Mutex<HashMap<u32, BrushMask>> {
    static CACHE: OnceLock<Mutex<HashMap<u32, BrushMask>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_brush_mask(stroke_width: u32) -> BrushMask {
    let cache = brush_mask_cache();
    if let Ok(guard) = cache.lock() {
        if let Some(mask) = guard.get(&stroke_width) {
            return mask.clone();
        }
    }

    let radius = (stroke_width.saturating_sub(1) / 2) as i32;
    let mut rows = Vec::with_capacity((radius.saturating_mul(2) + 1) as usize);
    for dy in -radius..=radius {
        let mut max_dx = radius;
        while max_dx >= 0 && max_dx * max_dx + dy * dy > radius * radius {
            max_dx -= 1;
        }
        if max_dx >= 0 {
            rows.push(BrushMaskRow {
                dy,
                min_dx: -max_dx,
                max_dx,
            });
        }
    }
    let mask = BrushMask { rows };
    if let Ok(mut guard) = cache.lock() {
        let _ = guard.insert(stroke_width, mask.clone());
    }
    mask
}

fn draw_brush_mask(
    center: (i32, i32),
    color: Color,
    stroke_width: u32,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    clip_rect: Option<DirtyRect>,
) -> u64 {
    let mask = get_brush_mask(stroke_width);
    let clip = clip_rect.unwrap_or(DirtyRect {
        x: 0,
        y: 0,
        width: width as i32,
        height: height as i32,
    });
    let Some(clip) = clip.clamp(width, height) else {
        return 0;
    };

    let clip_x0 = clip.x;
    let clip_x1 = clip.x + clip.width - 1;
    let clip_y0 = clip.y;
    let clip_y1 = clip.y + clip.height - 1;

    let mut writes: u64 = 0;
    for row in &mask.rows {
        let y = center.1 + row.dy;
        if y < clip_y0 || y > clip_y1 {
            continue;
        }
        let x0 = (center.0 + row.min_dx).max(clip_x0);
        let x1 = (center.0 + row.max_dx).min(clip_x1);
        if x0 > x1 {
            continue;
        }
        let row_base = ((y as u32 * width) * 4) as usize;
        for x in x0..=x1 {
            let idx = row_base + (x as usize * 4);
            pixels[idx] = color.r;
            pixels[idx + 1] = color.g;
            pixels[idx + 2] = color.b;
            pixels[idx + 3] = color.a;
            writes = writes.saturating_add(1);
        }
    }
    writes
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StrokeWidthBucketStat {
    pub count: u64,
    pub total_ns: u64,
    pub total_ops: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SegmentBenchmarkSnapshot {
    pub buckets: Vec<(String, StrokeWidthBucketStat)>,
    pub paths: Vec<(String, StrokeWidthBucketStat)>,
}

fn segment_bench_store() -> &'static Mutex<HashMap<&'static str, StrokeWidthBucketStat>> {
    static STORE: OnceLock<Mutex<HashMap<&'static str, StrokeWidthBucketStat>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn segment_path_store() -> &'static Mutex<HashMap<&'static str, StrokeWidthBucketStat>> {
    static STORE: OnceLock<Mutex<HashMap<&'static str, StrokeWidthBucketStat>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn segment_bucket(width: u32) -> &'static str {
    match width {
        0..=2 => "w1_2",
        3..=4 => "w3_4",
        5..=8 => "w5_8",
        9..=16 => "w9_16",
        _ => "w17p",
    }
}

fn record_segment_cost(width: u32, path: SegmentRenderPath, cost_ns: u64, operations: u64) {
    if !crate::draw::perf::draw_perf_runtime_enabled(false) {
        return;
    }
    if let Ok(mut store) = segment_bench_store().lock() {
        let entry = store.entry(segment_bucket(width)).or_default();
        entry.count = entry.count.saturating_add(1);
        entry.total_ns = entry.total_ns.saturating_add(cost_ns);
        entry.total_ops = entry.total_ops.saturating_add(operations);
    }
    if let Ok(mut store) = segment_path_store().lock() {
        let entry = store.entry(path.as_label()).or_default();
        entry.count = entry.count.saturating_add(1);
        entry.total_ns = entry.total_ns.saturating_add(cost_ns);
        entry.total_ops = entry.total_ops.saturating_add(operations);
    }
}

pub fn stroke_segment_benchmark_snapshot() -> SegmentBenchmarkSnapshot {
    let mut buckets = Vec::new();
    let mut paths = Vec::new();
    if let Ok(store) = segment_bench_store().lock() {
        for (name, stats) in store.iter() {
            buckets.push(((*name).to_string(), *stats));
        }
    }
    if let Ok(store) = segment_path_store().lock() {
        for (name, stats) in store.iter() {
            paths.push(((*name).to_string(), *stats));
        }
    }
    buckets.sort_by(|a, b| a.0.cmp(&b.0));
    paths.sort_by(|a, b| a.0.cmp(&b.0));
    SegmentBenchmarkSnapshot { buckets, paths }
}

#[cfg(test)]
fn reset_stroke_segment_benchmark() {
    if let Ok(mut store) = segment_bench_store().lock() {
        store.clear();
    }
    if let Ok(mut store) = segment_path_store().lock() {
        store.clear();
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

fn geometry_bounds(geometry: &Geometry, stroke_width: u32) -> Option<DirtyRect> {
    match geometry {
        Geometry::Pen { points } | Geometry::Eraser { points } => {
            let first = points.first().copied()?;
            let mut rect = segment_dirty_bounds(first, first, stroke_width);
            let mut last = first;
            for point in points.iter().copied().skip(1) {
                rect = rect.union(segment_dirty_bounds(last, point, stroke_width));
                last = point;
            }
            Some(rect)
        }
        Geometry::Line { start, end }
        | Geometry::Rect { start, end }
        | Geometry::Ellipse { start, end } => {
            Some(segment_dirty_bounds(*start, *end, stroke_width))
        }
    }
}

fn clear_rect_rgba(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    rect: DirtyRect,
    mode: BackgroundClearMode,
) {
    let clear = match mode {
        BackgroundClearMode::Transparent => crate::draw::model::FIRST_PASS_TRANSPARENCY_COLORKEY,
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
        convert_rgba_to_dib_bgra, render_canvas_to_pixels, render_canvas_to_rgba,
        render_incremental_segment_update, render_shape_preview_update, segment_dirty_bounds,
        set_wide_stroke_threshold, stroke_segment_benchmark_snapshot, DirtyRect, LayeredRenderer,
        RenderFrameBuffer,
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
            .filter(|px| *px != [255, 0, 255, 255])
            .count()
    }

    fn pixel_hash(pixels: &[u8]) -> u64 {
        let mut hasher = DefaultHasher::new();
        pixels.hash(&mut hasher);
        hasher.finish()
    }

    fn render_line_canvas(
        width: u32,
        threshold: u32,
        start: (i32, i32),
        end: (i32, i32),
    ) -> Vec<u8> {
        render_canvas_to_rgba(
            &CanvasModel {
                objects: vec![object_with_width(
                    Tool::Line,
                    Geometry::Line { start, end },
                    width,
                )],
            },
            super::RenderSettings {
                clear_mode: super::BackgroundClearMode::Transparent,
                wide_stroke_threshold: threshold,
            },
            (64, 64),
        )
    }

    #[test]
    fn render_empty_canvas_respects_background_clear_mode() {
        let transparent = render_canvas_to_rgba(
            &CanvasModel::default(),
            super::RenderSettings {
                clear_mode: super::BackgroundClearMode::Transparent,
                wide_stroke_threshold: super::DEFAULT_WIDE_STROKE_THRESHOLD,
            },
            (2, 1),
        );
        assert_eq!(transparent, vec![255, 0, 255, 255, 255, 0, 255, 255]);

        let solid = render_canvas_to_rgba(
            &CanvasModel::default(),
            super::RenderSettings {
                clear_mode: super::BackgroundClearMode::Solid(Color::rgba(7, 8, 9, 255)),
                wide_stroke_threshold: super::DEFAULT_WIDE_STROKE_THRESHOLD,
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
        assert!(pixels.chunks_exact(4).any(|px| px != [255, 0, 255, 255]));
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
        let transparent = [255, 0, 255, 255];
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
                wide_stroke_threshold: super::DEFAULT_WIDE_STROKE_THRESHOLD,
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
                wide_stroke_threshold: super::DEFAULT_WIDE_STROKE_THRESHOLD,
            },
            (3, 2),
            None,
            true,
        );

        assert!(framebuffer
            .rgba_pixels()
            .chunks_exact(4)
            .all(|px| px == [255, 0, 255, 255]));
    }

    #[test]
    fn framebuffer_blank_mode_initializes_surface_with_solid_color() {
        let mut framebuffer = RenderFrameBuffer::default();
        framebuffer.render(
            &CanvasModel::default(),
            super::RenderSettings {
                clear_mode: super::BackgroundClearMode::Solid(Color::rgba(9, 8, 7, 255)),
                wide_stroke_threshold: super::DEFAULT_WIDE_STROKE_THRESHOLD,
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
    fn segment_dirty_bounds_expands_by_stroke_radius_and_unions() {
        let first = segment_dirty_bounds((10, 10), (20, 20), 4);
        let second = segment_dirty_bounds((18, 18), (24, 24), 4);
        let union = first.union(second);

        assert!(first.x <= 4 && first.y <= 4);
        assert!(union.width >= first.width);
        assert!(union.height >= first.height);
    }

    #[test]
    fn shape_preview_update_clears_previous_preview_region() {
        let mut preview = vec![0u8; 64 * 64 * 4];
        let first = object(
            Tool::Rect,
            Geometry::Rect {
                start: (4, 4),
                end: (14, 14),
            },
        );
        let second = object(
            Tool::Rect,
            Geometry::Rect {
                start: (20, 20),
                end: (30, 30),
            },
        );

        let first_dirty = render_shape_preview_update(
            &mut preview,
            64,
            64,
            &first,
            None,
            super::BackgroundClearMode::Transparent,
        )
        .expect("first dirty");
        let second_dirty = render_shape_preview_update(
            &mut preview,
            64,
            64,
            &second,
            Some(first_dirty),
            super::BackgroundClearMode::Transparent,
        )
        .expect("second dirty");

        assert!(second_dirty.width >= first_dirty.width);
        assert_eq!(preview[((6 * 64 + 6) * 4 + 3) as usize], 0);
    }

    #[test]
    fn incremental_segment_updates_do_not_require_committed_rebuild() {
        let mut pixels = vec![0u8; 64 * 64 * 4];
        let style = ObjectStyle::default();
        let dirty = render_incremental_segment_update(
            &mut pixels,
            64,
            64,
            Tool::Pen,
            style,
            (2, 2),
            (20, 20),
            super::BackgroundClearMode::Transparent,
        );

        assert!(dirty.is_some());
        assert!(pixels.chunks_exact(4).any(|px| px[3] > 0));
    }

    fn make_test_overlay_window() -> OverlayWindow {
        #[cfg(windows)]
        {
            OverlayWindow::create_for_monitor(crate::draw::service::MonitorRect {
                x: 0,
                y: 0,
                width: 64,
                height: 64,
            })
            .expect("create test overlay window")
        }

        #[cfg(not(windows))]
        {
            OverlayWindow::default()
        }
    }

    #[test]
    fn committed_layer_rebuilds_only_on_history_mutation() {
        let mut renderer = LayeredRenderer::default();
        let mut window = make_test_overlay_window();
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

    #[test]
    fn freehand_incremental_preview_keeps_committed_cache_and_small_dirty_rect() {
        let mut renderer = LayeredRenderer::default();
        let mut window = make_test_overlay_window();
        let settings = super::RenderSettings::default();
        let committed = CanvasModel::default();

        let active = object(
            Tool::Pen,
            Geometry::Pen {
                points: vec![(4, 4), (8, 8), (12, 12)],
            },
        );
        let dirty = DirtyRect::from_points((8, 8), (12, 12), 4);

        renderer.render_to_window(
            &mut window,
            &committed,
            Some(&active),
            settings,
            (64, 64),
            Some(dirty),
            true,
            1,
        );
        renderer.render_to_window(
            &mut window,
            &committed,
            Some(&active),
            settings,
            (64, 64),
            Some(dirty),
            false,
            1,
        );

        assert_eq!(renderer.committed_rebuild_count(), 1);
        let presented = renderer.last_presented_dirty().expect("incremental dirty");
        assert!(presented.width <= 20 && presented.height <= 20);
    }

    #[test]
    fn repeated_freehand_preview_growth_reuses_committed_layer() {
        let mut renderer = LayeredRenderer::default();
        let mut window = make_test_overlay_window();
        let settings = super::RenderSettings::default();
        let committed = CanvasModel::default();

        let mut points = vec![(2, 2), (4, 4)];
        renderer.render_to_window(
            &mut window,
            &committed,
            Some(&object(
                Tool::Pen,
                Geometry::Pen {
                    points: points.clone(),
                },
            )),
            settings,
            (64, 64),
            Some(DirtyRect::from_points((2, 2), (4, 4), 4)),
            true,
            7,
        );

        for next in [(8, 8), (12, 12), (16, 16), (20, 20)] {
            let start = *points.last().expect("existing point");
            points.push(next);
            let active = object(
                Tool::Pen,
                Geometry::Pen {
                    points: points.clone(),
                },
            );
            let dirty = DirtyRect::from_points(start, next, 4);
            renderer.render_to_window(
                &mut window,
                &committed,
                Some(&active),
                settings,
                (64, 64),
                Some(dirty),
                false,
                7,
            );

            let presented = renderer
                .last_presented_dirty()
                .expect("presented dirty rect");
            assert!(presented.width <= 24 && presented.height <= 24);
        }

        assert_eq!(renderer.committed_rebuild_count(), 1);
    }

    #[test]
    fn narrow_strokes_remain_equivalent_with_legacy_and_heuristic_paths() {
        let widths = [1, 2, 3, 4];
        let segments = [((2, 2), (50, 2)), ((2, 2), (40, 30)), ((30, 3), (4, 40))];
        for width in widths {
            for (start, end) in segments {
                let legacy = render_line_canvas(width, u32::MAX, start, end);
                let heuristic = render_line_canvas(width, 1, start, end);
                assert_eq!(
                    legacy, heuristic,
                    "width={width} start={start:?} end={end:?}"
                );
            }
        }
    }

    #[test]
    fn threshold_switch_is_configurable() {
        let width = 12;
        let thin = render_line_canvas(width, width + 10, (4, 5), (48, 28));
        let wide = render_line_canvas(width, 1, (4, 5), (48, 28));
        assert_eq!(thin, wide);

        set_wide_stroke_threshold(1);
        let global_wide = render_canvas_to_rgba(
            &CanvasModel {
                objects: vec![object_with_width(
                    Tool::Line,
                    Geometry::Line {
                        start: (4, 5),
                        end: (48, 28),
                    },
                    width,
                )],
            },
            super::RenderSettings::default(),
            (64, 64),
        );
        assert_eq!(global_wide, wide);
        set_wide_stroke_threshold(super::DEFAULT_WIDE_STROKE_THRESHOLD);
    }

    #[test]
    fn clipping_at_edges_is_safe_and_writes_only_canvas() {
        let mut pixels = vec![0u8; 16 * 16 * 4];
        let dirty = render_incremental_segment_update(
            &mut pixels,
            16,
            16,
            Tool::Pen,
            ObjectStyle {
                stroke: StrokeStyle {
                    width: 22,
                    color: Color::rgba(3, 4, 5, 6),
                },
                fill: None,
            },
            (-30, -30),
            (40, 40),
            super::BackgroundClearMode::Transparent,
        );
        assert!(dirty.is_some());
        assert_eq!(pixels.len(), 16 * 16 * 4);
        assert!(pixels.chunks_exact(4).any(|px| px == [3, 4, 5, 6]));
    }

    #[test]
    fn offscreen_wide_segment_dirty_bounds_are_clamped_to_canvas() {
        let mut pixels = vec![0u8; 32 * 24 * 4];
        let dirty = render_incremental_segment_update(
            &mut pixels,
            32,
            24,
            Tool::Pen,
            ObjectStyle {
                stroke: StrokeStyle {
                    width: 28,
                    color: Color::rgba(9, 10, 11, 255),
                },
                fill: None,
            },
            (-200, 10),
            (90, 10),
            super::BackgroundClearMode::Transparent,
        )
        .expect("dirty bounds");

        assert_eq!(dirty.x, 0);
        assert_eq!(dirty.y, 0);
        assert_eq!(dirty.width, 32);
        assert_eq!(dirty.height, 24);
        assert!(pixels.chunks_exact(4).any(|px| px == [9, 10, 11, 255]));
    }

    #[test]
    fn wide_polyline_has_round_join_continuity_without_gaps() {
        let canvas = CanvasModel {
            objects: vec![object_with_width(
                Tool::Pen,
                Geometry::Pen {
                    points: vec![(10, 45), (30, 20), (52, 45)],
                },
                15,
            )],
        };
        let pixels = render_canvas_to_rgba(&canvas, super::RenderSettings::default(), (64, 64));
        let join_idx = ((20 * 64 + 30) * 4 + 3) as usize;
        let cap_a_idx = ((45 * 64 + 10) * 4 + 3) as usize;
        let cap_b_idx = ((45 * 64 + 52) * 4 + 3) as usize;
        assert!(pixels[join_idx] > 0);
        assert!(pixels[cap_a_idx] > 0);
        assert!(pixels[cap_b_idx] > 0);
    }

    #[test]
    fn segment_benchmark_buckets_record_samples() {
        std::env::set_var(crate::draw::perf::DRAW_PERF_DEBUG_ENV, "1");
        super::reset_stroke_segment_benchmark();
        let _ = render_line_canvas(14, 1, (2, 2), (50, 30));
        let snapshot = stroke_segment_benchmark_snapshot();
        assert!(snapshot.buckets.iter().any(|(_, stats)| stats.count > 0));
        assert!(snapshot.paths.iter().any(|(_, stats)| stats.count > 0));
        std::env::remove_var(crate::draw::perf::DRAW_PERF_DEBUG_ENV);
    }

    #[test]
    fn wide_segment_heuristic_reports_lower_structural_ops_than_legacy_dense() {
        std::env::set_var(crate::draw::perf::DRAW_PERF_DEBUG_ENV, "1");
        super::reset_stroke_segment_benchmark();
        let _ = render_line_canvas(18, u32::MAX, (2, 2), (58, 40));
        let legacy = stroke_segment_benchmark_snapshot();
        let legacy_ops = legacy
            .paths
            .iter()
            .find(|(name, _)| name == "legacy_dense")
            .map(|(_, stats)| stats.total_ops)
            .unwrap_or_default();

        super::reset_stroke_segment_benchmark();
        let _ = render_line_canvas(18, 1, (2, 2), (58, 40));
        let heuristic = stroke_segment_benchmark_snapshot();
        let heuristic_ops = heuristic
            .paths
            .iter()
            .filter(|(name, _)| name == "capsule_raster" || name == "adaptive_stamp")
            .map(|(_, stats)| stats.total_ops)
            .sum::<u64>();

        assert!(legacy_ops > 0);
        assert!(heuristic_ops > 0);
        assert!(heuristic_ops < legacy_ops);
        std::env::remove_var(crate::draw::perf::DRAW_PERF_DEBUG_ENV);
    }
}
