//! Shared straight-alpha software compositor for native presentation and M5 previews.

use super::assets::{PreparedImage, PreparedImageFrame};
use super::geometry::{LogicalPoint, LogicalRect, ScaleFactor};
use super::render::{Rgba, VectorPrimitive, VectorScene};
use image::{Rgba as ImageRgba, RgbaImage};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

pub const MAX_COMPOSITOR_PIXELS: usize = 32 * 1024 * 1024;
pub const MAX_FRAME_CACHE_ENTRIES: usize = 12;
/// Aggregate retained raster budget across completed and reusable static frames.
pub const MAX_COMPOSITOR_CACHE_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct RasterFrame {
    pub logical_bounds: LogicalRect,
    pub scale_factor: ScaleFactor,
    pub image: RgbaImage,
    pub generation: u64,
    pub animation_deadline_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompositorError {
    InvalidBounds,
    PixelBudgetExceeded,
    MissingSafeFallback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimationLease {
    session: super::model::SessionId,
    generation: u64,
    visible: bool,
}

impl AnimationLease {
    pub fn new(session: super::model::SessionId, generation: u64) -> Self {
        Self {
            session,
            generation,
            visible: true,
        }
    }

    pub fn accepts(&self, session: &super::model::SessionId, generation: u64) -> bool {
        self.visible && &self.session == session && self.generation == generation
    }

    pub fn close(&mut self) {
        self.visible = false;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FrameKey {
    generation: u64,
    dpi_milli: u32,
    animation_tick_ms: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompositorMetricsSnapshot {
    pub compositions: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub static_layer_hits: u64,
    pub static_layer_misses: u64,
    pub evictions: u64,
    pub rejected_admissions: u64,
    pub generation_invalidations: u64,
    pub completed_frames: usize,
    pub static_layers: usize,
    pub completed_bytes: usize,
    pub static_bytes: usize,
    pub total_bytes: usize,
    pub budget_bytes: usize,
    pub last_generation: u64,
    pub total_compose_ns: u128,
    pub max_compose_ns: u128,
}

#[derive(Default)]
struct CompositorMetrics {
    snapshot: CompositorMetricsSnapshot,
}

/// A small deterministic completed-frame cache. Prepared media and font data
/// are already immutable; neither cache hits nor misses perform filesystem IO.
pub struct CompositorCache {
    frames: BTreeMap<FrameKey, (VectorScene, u64, Arc<RasterFrame>)>,
    static_layer: Option<(LogicalRect, ScaleFactor, Vec<VectorPrimitive>, RgbaImage)>,
    clock: u64,
    frame_bytes: usize,
    static_bytes: usize,
    max_bytes: usize,
    metrics: Option<CompositorMetrics>,
}

impl Default for CompositorCache {
    fn default() -> Self {
        Self {
            frames: BTreeMap::new(),
            static_layer: None,
            clock: 0,
            frame_bytes: 0,
            static_bytes: 0,
            max_bytes: MAX_COMPOSITOR_CACHE_BYTES,
            metrics: crate::performance::enabled().then(CompositorMetrics::default),
        }
    }
}

impl CompositorCache {
    pub fn with_budget(max_bytes: usize) -> Self {
        Self::with_budget_and_observability(max_bytes, crate::performance::enabled())
    }

    pub fn with_budget_and_observability(max_bytes: usize, observable: bool) -> Self {
        Self {
            frames: BTreeMap::new(),
            static_layer: None,
            clock: 0,
            frame_bytes: 0,
            static_bytes: 0,
            max_bytes: max_bytes.max(1),
            metrics: observable.then(CompositorMetrics::default),
        }
    }

    pub fn metrics(&self) -> Option<CompositorMetricsSnapshot> {
        self.metrics.as_ref().map(|metrics| {
            let mut snapshot = metrics.snapshot;
            snapshot.completed_frames = self.frames.len();
            snapshot.static_layers = usize::from(self.static_layer.is_some());
            snapshot.completed_bytes = self.frame_bytes;
            snapshot.static_bytes = self.static_bytes;
            snapshot.total_bytes = self.retained_bytes();
            snapshot.budget_bytes = self.max_bytes;
            snapshot
        })
    }

    pub fn retained_bytes(&self) -> usize {
        self.frame_bytes.saturating_add(self.static_bytes)
    }

    pub fn compose(
        &mut self,
        scene: &VectorScene,
        scale: ScaleFactor,
        animation_time_ms: u64,
    ) -> Result<Arc<RasterFrame>, CompositorError> {
        let started = self.metrics.is_some().then(Instant::now);
        let key = FrameKey {
            generation: scene.generation,
            dpi_milli: (scale.get() * 1_000.0).round().max(1.0) as u32,
            animation_tick_ms: animation_time_ms,
        };
        self.clock = self.clock.wrapping_add(1);
        if let Some((cached_scene, touched, frame)) = self.frames.get_mut(&key)
            && cached_scene == scene
        {
            *touched = self.clock;
            let frame = Arc::clone(frame);
            self.finish_observation(scene.generation, true, started);
            return Ok(frame);
        }
        if let Some(metrics) = &mut self.metrics {
            metrics.snapshot.cache_misses = metrics.snapshot.cache_misses.saturating_add(1);
        }
        let composed = if let Some(boundary) = scene
            .primitives
            .iter()
            .position(|primitive| matches!(primitive, VectorPrimitive::StaticBoundary))
            .filter(|boundary| {
                !scene.primitives[..*boundary].iter().any(|primitive| {
                    matches!(primitive, VectorPrimitive::Image { image, .. } if image.frames.len() > 1)
                })
            })
        {
            let prefix = &scene.primitives[..boundary];
            let cached_base = match &self.static_layer {
                Some((bounds, cached_scale, cached, image))
                    if *bounds == scene.bounds && *cached_scale == scale && cached == prefix =>
                {
                    Some(image.clone())
                }
                _ => None,
            };
            let base = if let Some(image) = cached_base {
                if let Some(metrics) = &mut self.metrics {
                    metrics.snapshot.static_layer_hits =
                        metrics.snapshot.static_layer_hits.saturating_add(1);
                }
                image
            } else {
                if let Some(metrics) = &mut self.metrics {
                    metrics.snapshot.static_layer_misses =
                        metrics.snapshot.static_layer_misses.saturating_add(1);
                }
                let static_scene = VectorScene {
                    bounds: scene.bounds,
                    generation: scene.generation,
                    shape_quality: scene.shape_quality,
                    primitives: prefix.to_vec(),
                };
                let image = rasterize(&static_scene, scale, 0)?.image;
                self.replace_static_layer(Some((
                    scene.bounds,
                    scale,
                    prefix.to_vec(),
                    image.clone(),
                )));
                image
            };
            let mut image = base;
            let deadline = rasterize_primitives(
                &mut image,
                scene,
                scale,
                animation_time_ms,
                &scene.primitives[boundary + 1..],
            );
            RasterFrame {
                logical_bounds: scene.bounds,
                scale_factor: scale,
                image,
                generation: scene.generation,
                animation_deadline_ms: deadline,
            }
        } else {
            rasterize_or_fallback(scene, scale, animation_time_ms)?
        };
        let frame = Arc::new(composed);
        self.admit_frame(key, scene, &frame);
        self.finish_observation(scene.generation, false, started);
        Ok(frame)
    }

    pub fn invalidate_generation(&mut self, generation: u64) {
        let keys = self
            .frames
            .keys()
            .filter(|key| key.generation == generation)
            .copied()
            .collect::<Vec<_>>();
        for key in keys {
            self.remove_frame(&key, false);
        }
        self.replace_static_layer(None);
        if let Some(metrics) = &mut self.metrics {
            metrics.snapshot.generation_invalidations =
                metrics.snapshot.generation_invalidations.saturating_add(1);
        }
    }

    fn admit_frame(&mut self, key: FrameKey, scene: &VectorScene, frame: &Arc<RasterFrame>) {
        let bytes = image_bytes(&frame.image);
        if bytes > self.max_bytes {
            self.record_rejected_admission();
            return;
        }
        if let Some(previous) = self.frames.remove(&key) {
            self.frame_bytes = self
                .frame_bytes
                .saturating_sub(image_bytes(&previous.2.image));
        }
        if scene_has_animation(scene) {
            let stale_ticks = self
                .frames
                .keys()
                .filter(|candidate| {
                    candidate.generation == key.generation
                        && candidate.dpi_milli == key.dpi_milli
                        && candidate.animation_tick_ms != key.animation_tick_ms
                })
                .copied()
                .collect::<Vec<_>>();
            for stale in stale_ticks {
                self.remove_frame(&stale, true);
            }
        }
        self.make_room(bytes);
        if self.retained_bytes().saturating_add(bytes) > self.max_bytes {
            self.record_rejected_admission();
            return;
        }
        self.frame_bytes = self.frame_bytes.saturating_add(bytes);
        self.frames
            .insert(key, (scene.clone(), self.clock, Arc::clone(frame)));
        while self.frames.len() > MAX_FRAME_CACHE_ENTRIES {
            if !self.evict_oldest_frame() {
                break;
            }
        }
    }

    fn replace_static_layer(
        &mut self,
        layer: Option<(LogicalRect, ScaleFactor, Vec<VectorPrimitive>, RgbaImage)>,
    ) {
        self.static_layer = None;
        self.static_bytes = 0;
        let Some(layer) = layer else {
            return;
        };
        let bytes = image_bytes(&layer.3);
        if bytes > self.max_bytes {
            self.record_rejected_admission();
            return;
        }
        self.make_room(bytes);
        if self.retained_bytes().saturating_add(bytes) <= self.max_bytes {
            self.static_bytes = bytes;
            self.static_layer = Some(layer);
        } else {
            self.record_rejected_admission();
        }
    }

    fn make_room(&mut self, incoming: usize) {
        while self.retained_bytes().saturating_add(incoming) > self.max_bytes {
            if self.evict_oldest_frame() {
                continue;
            }
            if self.static_layer.take().is_some() {
                self.static_bytes = 0;
                self.record_eviction();
                continue;
            }
            break;
        }
    }

    fn evict_oldest_frame(&mut self) -> bool {
        let Some(key) = self
            .frames
            .iter()
            .min_by_key(|(key, (_, touched, _))| (*touched, *key))
            .map(|(key, _)| *key)
        else {
            return false;
        };
        self.remove_frame(&key, true);
        true
    }

    fn remove_frame(&mut self, key: &FrameKey, eviction: bool) {
        if let Some((_, _, frame)) = self.frames.remove(key) {
            self.frame_bytes = self.frame_bytes.saturating_sub(image_bytes(&frame.image));
            if eviction {
                self.record_eviction();
            }
        }
    }

    fn record_eviction(&mut self) {
        if let Some(metrics) = &mut self.metrics {
            metrics.snapshot.evictions = metrics.snapshot.evictions.saturating_add(1);
        }
    }

    fn record_rejected_admission(&mut self) {
        if let Some(metrics) = &mut self.metrics {
            metrics.snapshot.rejected_admissions =
                metrics.snapshot.rejected_admissions.saturating_add(1);
        }
    }

    fn finish_observation(&mut self, generation: u64, hit: bool, started: Option<Instant>) {
        let elapsed_ns = started.map(|started| started.elapsed().as_nanos());
        let Some(metrics) = &mut self.metrics else {
            return;
        };
        metrics.snapshot.compositions = metrics.snapshot.compositions.saturating_add(1);
        metrics.snapshot.last_generation = generation;
        if hit {
            metrics.snapshot.cache_hits = metrics.snapshot.cache_hits.saturating_add(1);
        }
        if let Some(elapsed) = elapsed_ns {
            metrics.snapshot.total_compose_ns =
                metrics.snapshot.total_compose_ns.saturating_add(elapsed);
            metrics.snapshot.max_compose_ns = metrics.snapshot.max_compose_ns.max(elapsed);
        }
        let snapshot = metrics.snapshot;
        let total_bytes = self.frame_bytes.saturating_add(self.static_bytes);
        let compose_ns = elapsed_ns.unwrap_or(0).min(u64::MAX as u128) as u64;
        tracing::info!(
            target: "multi_launcher::performance",
            phase = "radial.compositor",
            generation,
            compositions = snapshot.compositions,
            cache_hits = snapshot.cache_hits,
            cache_misses = snapshot.cache_misses,
            static_layer_hits = snapshot.static_layer_hits,
            static_layer_misses = snapshot.static_layer_misses,
            evictions = snapshot.evictions,
            rejected_admissions = snapshot.rejected_admissions,
            generation_invalidations = snapshot.generation_invalidations,
            completed_frames = self.frames.len(),
            static_layers = usize::from(self.static_layer.is_some()),
            completed_bytes = self.frame_bytes,
            static_bytes = self.static_bytes,
            total_bytes,
            budget_bytes = self.max_bytes,
            compose_ns,
            "perf"
        );
    }
}

fn image_bytes(image: &RgbaImage) -> usize {
    image.as_raw().len()
}

fn scene_has_animation(scene: &VectorScene) -> bool {
    scene.primitives.iter().any(
        |primitive| matches!(primitive, VectorPrimitive::Image { image, .. } if image.frames.len() > 1),
    )
}

pub fn rasterize_or_fallback(
    scene: &VectorScene,
    scale: ScaleFactor,
    animation_time_ms: u64,
) -> Result<RasterFrame, CompositorError> {
    match rasterize(scene, scale, animation_time_ms) {
        Ok(frame) => Ok(frame),
        Err(CompositorError::PixelBudgetExceeded) => Err(CompositorError::PixelBudgetExceeded),
        Err(_) => {
            let bounds = scene.bounds;
            let center = LogicalPoint {
                x: (bounds.min.x + bounds.max.x) * 0.5,
                y: (bounds.min.y + bounds.max.y) * 0.5,
            };
            let radius =
                ((bounds.max.x - bounds.min.x).min(bounds.max.y - bounds.min.y) * 0.45).max(1.0);
            let fallback = VectorScene {
                bounds,
                generation: scene.generation,
                shape_quality: scene.shape_quality,
                primitives: vec![VectorPrimitive::FilledCircle {
                    center,
                    radius,
                    color: Rgba(32, 35, 41, 245),
                }],
            };
            rasterize(&fallback, scale, animation_time_ms)
                .map_err(|_| CompositorError::MissingSafeFallback)
        }
    }
}

pub fn rasterize(
    scene: &VectorScene,
    scale: ScaleFactor,
    animation_time_ms: u64,
) -> Result<RasterFrame, CompositorError> {
    let width_logical = scene.bounds.max.x - scene.bounds.min.x;
    let height_logical = scene.bounds.max.y - scene.bounds.min.y;
    if !width_logical.is_finite()
        || !height_logical.is_finite()
        || width_logical <= 0.0
        || height_logical <= 0.0
    {
        return Err(CompositorError::InvalidBounds);
    }
    let width = (width_logical as f64 * scale.get()).ceil().max(1.0) as u32;
    let height = (height_logical as f64 * scale.get()).ceil().max(1.0) as u32;
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or(CompositorError::PixelBudgetExceeded)?;
    if pixels > MAX_COMPOSITOR_PIXELS {
        return Err(CompositorError::PixelBudgetExceeded);
    }
    let mut image = RgbaImage::new(width, height);
    let next_animation = rasterize_primitives(
        &mut image,
        scene,
        scale,
        animation_time_ms,
        &scene.primitives,
    );
    Ok(RasterFrame {
        logical_bounds: scene.bounds,
        scale_factor: scale,
        image,
        generation: scene.generation,
        animation_deadline_ms: next_animation,
    })
}

fn rasterize_primitives(
    image: &mut RgbaImage,
    scene: &VectorScene,
    scale: ScaleFactor,
    animation_time_ms: u64,
    primitives: &[VectorPrimitive],
) -> Option<u64> {
    let mut next_animation = None;
    for primitive in primitives {
        match primitive {
            VectorPrimitive::FilledCircle {
                center,
                radius,
                color,
            } => draw_circle(
                image,
                scene.bounds,
                scale,
                *center,
                *radius,
                *color,
                scene.shape_quality,
            ),
            VectorPrimitive::FilledWedge {
                center,
                inner_radius,
                outer_radius,
                start_angle,
                end_angle,
                color,
            } => draw_wedge(
                image,
                scene.bounds,
                scale,
                *center,
                *inner_radius,
                *outer_radius,
                *start_angle,
                *end_angle,
                *color,
                scene.shape_quality,
            ),
            VectorPrimitive::Image {
                bounds,
                image: prepared,
                opacity,
                quality,
            } => {
                if let Some((frame, deadline)) = animation_frame(prepared, animation_time_ms) {
                    next_animation = min_deadline(next_animation, deadline);
                    draw_image(
                        image,
                        scene.bounds,
                        scale,
                        *bounds,
                        frame,
                        *opacity,
                        *quality,
                    );
                }
            }
            VectorPrimitive::Text {
                origin,
                prepared,
                size,
                bold,
                italic,
                underline,
                strikeout,
                quality,
                shadow,
                color,
                ..
            } => draw_text_blocks(
                image,
                scene.bounds,
                scale,
                *origin,
                prepared,
                *size,
                *bold,
                *italic,
                *underline,
                *strikeout,
                *quality,
                *shadow,
                *color,
            ),
            VectorPrimitive::Tooltip {
                bounds,
                text,
                background,
                color,
            } => {
                fill_rect(image, scene.bounds, scale, *bounds, *background);
                draw_text_blocks(
                    image,
                    scene.bounds,
                    scale,
                    super::geometry::LogicalPoint {
                        x: bounds.min.x + super::tooltip::TOOLTIP_PADDING_LOGICAL,
                        y: bounds.min.y + super::tooltip::TOOLTIP_PADDING_LOGICAL,
                    },
                    &text.layout,
                    text.font_size,
                    false,
                    false,
                    false,
                    false,
                    super::model::RenderingQuality::Balanced,
                    None,
                    *color,
                );
            }
            VectorPrimitive::StaticBoundary => {}
        }
    }
    next_animation
}

fn animation_frame(
    image: &PreparedImage,
    elapsed: u64,
) -> Option<(&PreparedImageFrame, Option<u64>)> {
    if image.frames.len() <= 1 {
        return image.frames.first().map(|frame| (frame, None));
    }
    let total = image
        .frames
        .iter()
        .map(|frame| u64::from(frame.duration_ms.max(1)))
        .sum::<u64>()
        .max(1);
    let mut cursor = elapsed % total;
    for frame in &image.frames {
        let duration = u64::from(frame.duration_ms.max(1));
        if cursor < duration {
            return Some((frame, Some(elapsed.saturating_add(duration - cursor))));
        }
        cursor -= duration;
    }
    Some((&image.frames[0], Some(elapsed.saturating_add(1))))
}

fn min_deadline(current: Option<u64>, candidate: Option<u64>) -> Option<u64> {
    match (current, candidate) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (value @ Some(_), None) | (None, value @ Some(_)) => value,
        (None, None) => None,
    }
}

fn draw_circle(
    target: &mut RgbaImage,
    scene: LogicalRect,
    scale: ScaleFactor,
    center: LogicalPoint,
    radius: f32,
    color: Rgba,
    quality: super::model::RenderingQuality,
) {
    let min = logical_to_pixel(
        scene,
        scale,
        LogicalPoint {
            x: center.x - radius,
            y: center.y - radius,
        },
    );
    let max = logical_to_pixel(
        scene,
        scale,
        LogicalPoint {
            x: center.x + radius,
            y: center.y + radius,
        },
    );
    let r2 = radius * radius;
    for y in min.1.max(0)..max.1.min(target.height() as i32) {
        for x in min.0.max(0)..max.0.min(target.width() as i32) {
            let coverage = shape_coverage(quality, |ox, oy| {
                let point = pixel_sample_to_logical(scene, scale, x, y, ox, oy);
                let dx = point.x - center.x;
                let dy = point.y - center.y;
                dx * dx + dy * dy <= r2
            });
            if coverage > 0 {
                blend(
                    target.get_pixel_mut(x as u32, y as u32),
                    scaled_alpha(color, coverage),
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_wedge(
    target: &mut RgbaImage,
    scene: LogicalRect,
    scale: ScaleFactor,
    center: LogicalPoint,
    inner: f32,
    outer: f32,
    start: f32,
    end: f32,
    color: Rgba,
    quality: super::model::RenderingQuality,
) {
    let min = logical_to_pixel(
        scene,
        scale,
        LogicalPoint {
            x: center.x - outer,
            y: center.y - outer,
        },
    );
    let max = logical_to_pixel(
        scene,
        scale,
        LogicalPoint {
            x: center.x + outer,
            y: center.y + outer,
        },
    );
    let span = (end - start).rem_euclid(std::f32::consts::TAU);
    for y in min.1.max(0)..max.1.min(target.height() as i32) {
        for x in min.0.max(0)..max.0.min(target.width() as i32) {
            let coverage = shape_coverage(quality, |ox, oy| {
                let point = pixel_sample_to_logical(scene, scale, x, y, ox, oy);
                let dx = point.x - center.x;
                let dy = point.y - center.y;
                let radius = (dx * dx + dy * dy).sqrt();
                let angle = (dy.atan2(dx) - start).rem_euclid(std::f32::consts::TAU);
                radius >= inner && radius <= outer && (span == 0.0 || angle < span)
            });
            if coverage > 0 {
                blend(
                    target.get_pixel_mut(x as u32, y as u32),
                    scaled_alpha(color, coverage),
                );
            }
        }
    }
}

fn shape_coverage(
    quality: super::model::RenderingQuality,
    contains: impl Fn(f32, f32) -> bool,
) -> u8 {
    let grid = match quality {
        super::model::RenderingQuality::Fast => 1,
        super::model::RenderingQuality::Balanced => 2,
        super::model::RenderingQuality::HighQuality => 4,
    };
    let mut inside = 0_u32;
    for y in 0..grid {
        for x in 0..grid {
            if contains(
                (x as f32 + 0.5) / grid as f32,
                (y as f32 + 0.5) / grid as f32,
            ) {
                inside += 1;
            }
        }
    }
    ((inside * 255 + (grid * grid) / 2) / (grid * grid)) as u8
}

fn scaled_alpha(color: Rgba, coverage: u8) -> Rgba {
    Rgba(
        color.0,
        color.1,
        color.2,
        ((color.3 as u16 * coverage as u16 + 127) / 255) as u8,
    )
}

fn pixel_sample_to_logical(
    scene: LogicalRect,
    scale: ScaleFactor,
    x: i32,
    y: i32,
    offset_x: f32,
    offset_y: f32,
) -> LogicalPoint {
    LogicalPoint {
        x: scene.min.x + (x as f32 + offset_x) / scale.get() as f32,
        y: scene.min.y + (y as f32 + offset_y) / scale.get() as f32,
    }
}

fn draw_image(
    target: &mut RgbaImage,
    scene: LogicalRect,
    scale: ScaleFactor,
    bounds: LogicalRect,
    frame: &PreparedImageFrame,
    opacity: u8,
    quality: super::model::RenderingQuality,
) {
    let required = (frame.width as usize)
        .checked_mul(frame.height as usize)
        .and_then(|pixels| pixels.checked_mul(4));
    if frame.width == 0 || frame.height == 0 || required != Some(frame.rgba.len()) {
        return;
    }
    let min = logical_to_pixel(scene, scale, bounds.min);
    let max = logical_to_pixel(scene, scale, bounds.max);
    let width = (max.0 - min.0).max(1);
    let height = (max.1 - min.1).max(1);
    for y in min.1.max(0)..max.1.min(target.height() as i32) {
        for x in min.0.max(0)..max.0.min(target.width() as i32) {
            let x_round = if quality == super::model::RenderingQuality::Fast {
                0
            } else {
                width as u64 / 2
            };
            let sx = ((((x - min.0) as u64 * frame.width as u64) + x_round) / width as u64)
                .min(frame.width.saturating_sub(1) as u64) as usize;
            let y_round = if quality == super::model::RenderingQuality::Fast {
                0
            } else {
                height as u64 / 2
            };
            let sy = ((((y - min.1) as u64 * frame.height as u64) + y_round) / height as u64)
                .min(frame.height.saturating_sub(1) as u64) as usize;
            let offset = (sy * frame.width as usize + sx) * 4;
            let rgba = &frame.rgba[offset..offset + 4];
            blend(
                target.get_pixel_mut(x as u32, y as u32),
                Rgba(
                    rgba[0],
                    rgba[1],
                    rgba[2],
                    ((rgba[3] as u16 * opacity as u16 + 127) / 255) as u8,
                ),
            );
        }
    }
}

fn draw_text_blocks(
    target: &mut RgbaImage,
    scene: LogicalRect,
    scale: ScaleFactor,
    origin: LogicalPoint,
    text: &Arc<super::font_cache::PreparedTextLayout>,
    size: f32,
    bold: bool,
    italic: bool,
    underline: bool,
    strikeout: bool,
    quality: super::model::RenderingQuality,
    shadow: Option<(Rgba, super::model::Offset2D)>,
    color: Rgba,
) {
    if let Some((shadow_color, offset)) = shadow {
        draw_text_blocks(
            target,
            scene,
            scale,
            LogicalPoint {
                x: origin.x + offset.x,
                y: origin.y + offset.y,
            },
            text,
            size,
            bold,
            italic,
            underline,
            strikeout,
            quality,
            None,
            shadow_color,
        );
    }
    let width = (text.estimated_width_milli as f32 / 1_000.0).max(size);
    let top_left = LogicalPoint {
        x: origin.x - width * 0.5,
        y: origin.y - size * 0.5,
    };
    let origin_px = logical_to_pixel(scene, scale, origin);
    for glyph in &text.glyphs {
        for y in 0..glyph.height {
            for x in 0..glyph.width {
                let Some(coverage) = glyph.alpha.get((y * glyph.width + x) as usize).copied()
                else {
                    continue;
                };
                if coverage == 0 {
                    continue;
                }
                if quality == super::model::RenderingQuality::Fast && coverage < 128 {
                    continue;
                }
                let px = origin_px.0
                    + glyph.x
                    + x as i32
                    + if italic {
                        (glyph.height - y) as i32 / 8
                    } else {
                        0
                    };
                let py = origin_px.1 + glyph.y + y as i32;
                if px >= 0 && py >= 0 && px < target.width() as i32 && py < target.height() as i32 {
                    let alpha = ((color.3 as u16 * coverage as u16 + 127) / 255) as u8;
                    blend(
                        target.get_pixel_mut(px as u32, py as u32),
                        Rgba(color.0, color.1, color.2, alpha),
                    );
                    if bold && px + 1 < target.width() as i32 {
                        blend(
                            target.get_pixel_mut((px + 1) as u32, py as u32),
                            Rgba(color.0, color.1, color.2, alpha),
                        );
                    }
                }
            }
        }
    }
    if underline {
        fill_rect(
            target,
            scene,
            scale,
            LogicalRect {
                min: LogicalPoint {
                    x: top_left.x,
                    y: top_left.y + size * 0.88,
                },
                max: LogicalPoint {
                    x: top_left.x + width,
                    y: top_left.y + size,
                },
            },
            color,
        );
    }
    if strikeout {
        fill_rect(
            target,
            scene,
            scale,
            LogicalRect {
                min: LogicalPoint {
                    x: top_left.x,
                    y: top_left.y + size * 0.45,
                },
                max: LogicalPoint {
                    x: top_left.x + width,
                    y: top_left.y + size * 0.55,
                },
            },
            color,
        );
    }
}

fn fill_rect(
    target: &mut RgbaImage,
    scene: LogicalRect,
    scale: ScaleFactor,
    rect: LogicalRect,
    color: Rgba,
) {
    let min = logical_to_pixel(scene, scale, rect.min);
    let max = logical_to_pixel(scene, scale, rect.max);
    for y in min.1.max(0)..max.1.min(target.height() as i32) {
        for x in min.0.max(0)..max.0.min(target.width() as i32) {
            blend(target.get_pixel_mut(x as u32, y as u32), color);
        }
    }
}

fn logical_to_pixel(bounds: LogicalRect, scale: ScaleFactor, point: LogicalPoint) -> (i32, i32) {
    (
        ((point.x - bounds.min.x) as f64 * scale.get()).floor() as i32,
        ((point.y - bounds.min.y) as f64 * scale.get()).floor() as i32,
    )
}

fn blend(destination: &mut ImageRgba<u8>, source: Rgba) {
    let source_alpha = source.3 as u32;
    let destination_alpha = destination[3] as u32;
    let out_alpha = source_alpha + (destination_alpha * (255 - source_alpha) + 127) / 255;
    if out_alpha == 0 {
        *destination = ImageRgba([0, 0, 0, 0]);
        return;
    }
    let channels = [source.0, source.1, source.2];
    for index in 0..3 {
        let source_premult = channels[index] as u32 * source_alpha;
        let destination_premult = destination[index] as u32 * destination_alpha;
        let out_premult = source_premult + (destination_premult * (255 - source_alpha) + 127) / 255;
        destination[index] = ((out_premult + out_alpha / 2) / out_alpha).min(255) as u8;
    }
    destination[3] = out_alpha.min(255) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::geometry::{PhysicalPoint, PhysicalRect, layout_menu};
    use crate::radial::model::RadialDocument;
    use crate::radial::render::build_scene;

    #[test]
    fn native_and_preview_use_identical_straight_alpha_frame() {
        let layout = layout_menu(
            &RadialDocument::starter().menus[0],
            PhysicalPoint { x: 250.0, y: 250.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 500.0, y: 500.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        let scene = build_scene(&layout, 4);
        let native = rasterize(&scene, layout.scale_factor, 0).unwrap();
        let preview = rasterize(&scene, layout.scale_factor, 0).unwrap();
        assert_eq!(native.image.as_raw(), preview.image.as_raw());
        assert!(
            native
                .image
                .pixels()
                .any(|pixel| pixel[3] > 0 && pixel[3] < 255)
        );
    }

    #[test]
    fn completed_frame_cache_is_stable_and_generation_invalidates() {
        let scene = VectorScene {
            bounds: LogicalRect {
                min: LogicalPoint { x: -10.0, y: -10.0 },
                max: LogicalPoint { x: 10.0, y: 10.0 },
            },
            generation: 7,
            shape_quality: super::super::model::RenderingQuality::Balanced,
            primitives: vec![VectorPrimitive::FilledCircle {
                center: LogicalPoint { x: 0.0, y: 0.0 },
                radius: 8.0,
                color: Rgba(200, 100, 50, 128),
            }],
        };
        let mut cache = CompositorCache::default();
        let first = cache
            .compose(&scene, ScaleFactor::new(1.0).unwrap(), 0)
            .unwrap();
        let second = cache
            .compose(&scene, ScaleFactor::new(1.0).unwrap(), 0)
            .unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        cache.invalidate_generation(7);
        let third = cache
            .compose(&scene, ScaleFactor::new(1.0).unwrap(), 0)
            .unwrap();
        assert!(!Arc::ptr_eq(&first, &third));
    }

    #[test]
    fn serialized_shape_quality_changes_edge_rasterization() {
        let mut scene = VectorScene {
            bounds: LogicalRect {
                min: LogicalPoint { x: -5.0, y: -5.0 },
                max: LogicalPoint { x: 5.0, y: 5.0 },
            },
            generation: 1,
            shape_quality: super::super::model::RenderingQuality::Fast,
            primitives: vec![VectorPrimitive::FilledCircle {
                center: LogicalPoint { x: 0.15, y: 0.15 },
                radius: 3.4,
                color: Rgba(255, 255, 255, 255),
            }],
        };
        let fast = rasterize(&scene, ScaleFactor::new(1.0).unwrap(), 0).unwrap();
        scene.shape_quality = super::super::model::RenderingQuality::HighQuality;
        let high = rasterize(&scene, ScaleFactor::new(1.0).unwrap(), 0).unwrap();
        assert_ne!(fast.image.as_raw(), high.image.as_raw());
        assert!(
            high.image
                .pixels()
                .any(|pixel| pixel[3] > 0 && pixel[3] < 255)
        );
    }

    #[test]
    fn stale_animation_generation_and_closed_session_stop_ticking() {
        let session = crate::radial::model::SessionId::new("animation");
        let mut lease = AnimationLease::new(session.clone(), 3);
        assert!(lease.accepts(&session, 3));
        assert!(!lease.accepts(&session, 4));
        lease.close();
        assert!(!lease.accepts(&session, 3));
    }

    #[test]
    fn prepared_animation_exposes_only_its_next_visible_frame_deadline() {
        let prepared = Arc::new(PreparedImage {
            frames: vec![
                PreparedImageFrame {
                    width: 1,
                    height: 1,
                    duration_ms: 10,
                    rgba: Arc::from(&[255, 0, 0, 255][..]),
                },
                PreparedImageFrame {
                    width: 1,
                    height: 1,
                    duration_ms: 20,
                    rgba: Arc::from(&[0, 255, 0, 255][..]),
                },
            ],
            animated: true,
        });
        let scene = VectorScene {
            bounds: LogicalRect {
                min: LogicalPoint { x: 0.0, y: 0.0 },
                max: LogicalPoint { x: 2.0, y: 2.0 },
            },
            generation: 9,
            shape_quality: super::super::model::RenderingQuality::Balanced,
            primitives: vec![VectorPrimitive::Image {
                bounds: LogicalRect {
                    min: LogicalPoint { x: 0.0, y: 0.0 },
                    max: LogicalPoint { x: 2.0, y: 2.0 },
                },
                image: prepared,
                opacity: 255,
                quality: super::super::model::RenderingQuality::Balanced,
            }],
        };
        let scale = ScaleFactor::new(1.0).unwrap();
        let first = rasterize(&scene, scale, 0).unwrap();
        let second = rasterize(&scene, scale, 10).unwrap();
        assert_eq!(first.animation_deadline_ms, Some(10));
        assert_eq!(second.animation_deadline_ms, Some(30));
        assert_ne!(first.image, second.image);
    }

    #[test]
    fn hover_change_reuses_static_layer_but_recomposes_dynamic_suffix() {
        let bounds = LogicalRect {
            min: LogicalPoint { x: -10.0, y: -10.0 },
            max: LogicalPoint { x: 10.0, y: 10.0 },
        };
        let mut scene = VectorScene {
            bounds,
            generation: 8,
            shape_quality: super::super::model::RenderingQuality::Balanced,
            primitives: vec![
                VectorPrimitive::FilledCircle {
                    center: LogicalPoint { x: 0.0, y: 0.0 },
                    radius: 9.0,
                    color: Rgba(20, 20, 20, 220),
                },
                VectorPrimitive::StaticBoundary,
                VectorPrimitive::FilledCircle {
                    center: LogicalPoint { x: 0.0, y: 0.0 },
                    radius: 4.0,
                    color: Rgba(40, 40, 40, 255),
                },
            ],
        };
        let scale = ScaleFactor::new(1.0).unwrap();
        let mut cache =
            CompositorCache::with_budget_and_observability(MAX_COMPOSITOR_CACHE_BYTES, true);
        let first = cache.compose(&scene, scale, 0).unwrap();
        let static_before = cache.static_layer.as_ref().unwrap().3.clone();
        if let VectorPrimitive::FilledCircle { color, .. } = &mut scene.primitives[2] {
            *color = Rgba(200, 100, 50, 255);
        }
        let second = cache.compose(&scene, scale, 0).unwrap();
        assert_eq!(cache.static_layer.as_ref().unwrap().3, static_before);
        assert_ne!(first.image, second.image);
        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.static_layer_misses, 1);
        assert_eq!(metrics.static_layer_hits, 1);
    }

    fn solid_scene(generation: u64, side: f32) -> VectorScene {
        VectorScene {
            bounds: LogicalRect {
                min: LogicalPoint { x: 0.0, y: 0.0 },
                max: LogicalPoint { x: side, y: side },
            },
            generation,
            shape_quality: super::super::model::RenderingQuality::Fast,
            primitives: vec![VectorPrimitive::FilledCircle {
                center: LogicalPoint {
                    x: side * 0.5,
                    y: side * 0.5,
                },
                radius: side * 0.4,
                color: Rgba(20, 40, 60, 255),
            }],
        }
    }

    #[test]
    fn aggregate_byte_budget_evicts_deterministically_and_invalidation_is_exact() {
        let scale = ScaleFactor::new(1.0).unwrap();
        let frame_bytes = 8 * 8 * 4;
        let mut cache = CompositorCache::with_budget_and_observability(frame_bytes * 2, true);
        for generation in [1, 2] {
            cache
                .compose(&solid_scene(generation, 8.0), scale, 0)
                .unwrap();
        }
        // Refresh generation 1, making generation 2 the deterministic LRU victim.
        cache.compose(&solid_scene(1, 8.0), scale, 0).unwrap();
        cache.compose(&solid_scene(3, 8.0), scale, 0).unwrap();

        assert!(cache.frames.keys().any(|key| key.generation == 1));
        assert!(!cache.frames.keys().any(|key| key.generation == 2));
        assert!(cache.frames.keys().any(|key| key.generation == 3));
        assert_eq!(cache.retained_bytes(), frame_bytes * 2);
        assert_eq!(cache.metrics().unwrap().evictions, 1);

        cache.invalidate_generation(1);
        assert_eq!(cache.retained_bytes(), frame_bytes);
        cache.invalidate_generation(3);
        assert_eq!(cache.retained_bytes(), 0);
        assert!(cache.frames.is_empty());
    }

    #[test]
    fn oversized_completed_frame_is_returned_without_cache_admission() {
        let mut cache = CompositorCache::with_budget_and_observability(64, true);
        let frame = cache
            .compose(&solid_scene(4, 8.0), ScaleFactor::new(1.0).unwrap(), 0)
            .unwrap();
        assert_eq!(image_bytes(&frame.image), 8 * 8 * 4);
        assert!(cache.frames.is_empty());
        assert_eq!(cache.retained_bytes(), 0);
        assert_eq!(cache.metrics().unwrap().rejected_admissions, 1);
    }

    #[test]
    fn animation_timestamps_replace_the_prior_full_frame_variant() {
        let prepared = Arc::new(PreparedImage {
            frames: vec![
                PreparedImageFrame {
                    width: 1,
                    height: 1,
                    duration_ms: 10,
                    rgba: Arc::from(&[255, 0, 0, 255][..]),
                },
                PreparedImageFrame {
                    width: 1,
                    height: 1,
                    duration_ms: 10,
                    rgba: Arc::from(&[0, 255, 0, 255][..]),
                },
            ],
            animated: true,
        });
        let mut scene = solid_scene(5, 8.0);
        scene.primitives = vec![VectorPrimitive::Image {
            bounds: scene.bounds,
            image: prepared,
            opacity: 255,
            quality: super::super::model::RenderingQuality::Fast,
        }];
        let mut cache = CompositorCache::with_budget(8 * 8 * 4 * MAX_FRAME_CACHE_ENTRIES);
        let scale = ScaleFactor::new(1.0).unwrap();
        for timestamp in 0..24 {
            cache.compose(&scene, scale, timestamp).unwrap();
        }
        assert_eq!(cache.frames.len(), 1);
        assert_eq!(cache.retained_bytes(), 8 * 8 * 4);
        assert_eq!(cache.frames.keys().next().unwrap().animation_tick_ms, 23);
    }

    #[test]
    fn completed_and_static_layers_share_one_budget() {
        let bounds = LogicalRect {
            min: LogicalPoint { x: 0.0, y: 0.0 },
            max: LogicalPoint { x: 8.0, y: 8.0 },
        };
        let scene = VectorScene {
            bounds,
            generation: 6,
            shape_quality: super::super::model::RenderingQuality::Fast,
            primitives: vec![
                VectorPrimitive::FilledCircle {
                    center: LogicalPoint { x: 4.0, y: 4.0 },
                    radius: 3.0,
                    color: Rgba(10, 20, 30, 255),
                },
                VectorPrimitive::StaticBoundary,
                VectorPrimitive::FilledCircle {
                    center: LogicalPoint { x: 4.0, y: 4.0 },
                    radius: 1.0,
                    color: Rgba(200, 210, 220, 255),
                },
            ],
        };
        let frame_bytes = 8 * 8 * 4;
        let mut cache = CompositorCache::with_budget(frame_bytes);
        cache
            .compose(&scene, ScaleFactor::new(1.0).unwrap(), 0)
            .unwrap();
        assert_eq!(cache.frames.len(), 1);
        assert!(cache.static_layer.is_none());
        assert_eq!(cache.retained_bytes(), frame_bytes);
    }

    #[test]
    fn observability_is_owned_and_has_no_state_when_disabled() {
        let scale = ScaleFactor::new(1.0).unwrap();
        let scene = solid_scene(19, 4.0);
        let mut disabled = CompositorCache::with_budget_and_observability(1_024, false);
        disabled.compose(&scene, scale, 0).unwrap();
        assert_eq!(disabled.metrics(), None);

        let mut enabled = CompositorCache::with_budget_and_observability(1_024, true);
        enabled.compose(&scene, scale, 0).unwrap();
        enabled.compose(&scene, scale, 0).unwrap();
        let metrics = enabled.metrics().unwrap();
        assert_eq!(metrics.compositions, 2);
        assert_eq!(metrics.cache_hits, 1);
        assert_eq!(metrics.cache_misses, 1);
        assert_eq!(metrics.last_generation, 19);
        assert_eq!(metrics.completed_frames, 1);
        assert_eq!(metrics.total_bytes, 4 * 4 * 4);
    }

    #[test]
    fn warm_hover_and_one_hundred_open_close_cycles_remain_bounded() {
        let scale = ScaleFactor::new(1.0).unwrap();
        let budget = 8 * 8 * 4 * 2;
        let mut cache = CompositorCache::with_budget_and_observability(budget, true);
        for generation in 0..100 {
            let mut scene = solid_scene(generation, 8.0);
            scene.primitives.push(VectorPrimitive::StaticBoundary);
            scene.primitives.push(VectorPrimitive::FilledCircle {
                center: LogicalPoint { x: 4.0, y: 4.0 },
                radius: if generation % 2 == 0 { 1.0 } else { 2.0 },
                color: Rgba(220, 120, 20, 255),
            });
            cache.compose(&scene, scale, 0).unwrap();
            cache.compose(&scene, scale, 0).unwrap();
            assert!(cache.retained_bytes() <= budget);
            cache.invalidate_generation(generation);
            assert_eq!(cache.retained_bytes(), 0);
        }
        let metrics = cache.metrics().unwrap();
        assert_eq!(metrics.compositions, 200);
        assert_eq!(metrics.cache_hits, 100);
        assert_eq!(metrics.completed_frames, 0);
        assert_eq!(metrics.static_layers, 0);
        assert_eq!(metrics.generation_invalidations, 100);
        // The warm path accepts only immutable scene snapshots and has no file
        // or provider capability; these compositions therefore cannot perform IO.
    }
}
