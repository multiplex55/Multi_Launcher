//! Bounded, current-only Mermaid, SVG, and PNG export from the shared Diagram scene.

use super::model::NodeShape;
use super::persistence::DiagramExportPublicationError;
use super::scene::{
    ArrowHead, CubicBezier, DiagramScene, PreparedText, SceneMetrics, SceneNode, ScenePoint,
    SceneRect, TextHorizontalAlignment, build_scene,
};
use super::session::{DiagramSession, PreviewState};
use ab_glyph::{Font, FontArc, FontRef, FontVec, ScaleFont, point};
use eframe::egui::{FontDefinitions, FontFamily, FontTweak};
use image::{ImageEncoder, Rgba, RgbaImage};
use std::borrow::Cow;
use std::fmt;
use std::io::{self, Write};
use std::path::Path;
use std::sync::OnceLock;

pub const MAX_EXPORT_SIDE: u32 = 4_096;
pub const MAX_EXPORT_PIXELS: u64 = 16_777_216;
pub const MAX_PREPARED_GLYPHS: usize = 65_536;
pub const MAX_RASTER_PIXEL_VISITS: u64 = 128_000_000;
pub const MAX_PNG_BYTES: usize = 80 * 1024 * 1024;
pub const MAX_SVG_BYTES: usize = 8 * 1024 * 1024;

const EXPORT_PADDING: f64 = 24.0;
const EMPTY_IMAGE_SIDE: u32 = 48;
const CUBIC_SEGMENTS: usize = 64;
const STROKE_WIDTH: f64 = 2.0;

/// An encoded Diagram artifact. Its payload and metadata are immutable after encoding.
#[derive(Debug)]
pub struct DiagramExportArtifact {
    revision: u64,
    format: DiagramExportFormat,
    bytes: Vec<u8>,
    dimensions: Option<(u32, u32)>,
    warnings: DiagramExportWarnings,
}

impl DiagramExportArtifact {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn format(&self) -> DiagramExportFormat {
        self.format
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn dimensions(&self) -> Option<(u32, u32)> {
        self.dimensions
    }

    pub fn warnings(&self) -> DiagramExportWarnings {
        self.warnings
    }

    /// Publish this already encoded artifact only if its destination is unoccupied.
    /// The check is immediately before the atomic writer; this is not a filesystem CAS.
    pub fn publish_unoccupied(&self, path: &Path) -> Result<(), DiagramExportPublicationError> {
        self.publish_unoccupied_with_writer(path, crate::common::atomic_file::save_atomic)
    }

    fn publish_unoccupied_with_writer(
        &self,
        path: &Path,
        writer: impl FnOnce(&Path, &[u8]) -> anyhow::Result<()>,
    ) -> Result<(), DiagramExportPublicationError> {
        super::persistence::publish_export_unoccupied(path, self.format, &self.bytes, writer)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagramExportFormat {
    Mermaid,
    Svg,
    Png,
}

impl DiagramExportFormat {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Mermaid => "mmd",
            Self::Svg => "svg",
            Self::Png => "png",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiagramExportWarnings {
    glyph_substitutions: u32,
}

impl DiagramExportWarnings {
    pub const fn glyph_substitutions(self) -> u32 {
        self.glyph_substitutions
    }
}

/// A source snapshot is available for every draft, including unsupported Mermaid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramSourceSnapshot {
    revision: u64,
    source: String,
}

impl DiagramSourceSnapshot {
    pub fn capture(session: &DiagramSession) -> Self {
        Self {
            revision: session.revision(),
            source: session.draft().to_owned(),
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn encode(&self) -> DiagramExportArtifact {
        DiagramExportArtifact {
            revision: self.revision,
            format: DiagramExportFormat::Mermaid,
            bytes: self.source.as_bytes().to_vec(),
            dimensions: None,
            warnings: DiagramExportWarnings::default(),
        }
    }
}

/// Immutable scene and revision captured together from a current session graph.
#[derive(Clone, Debug)]
pub struct DiagramImageSnapshot {
    revision: u64,
    scene: DiagramScene,
}

impl DiagramImageSnapshot {
    pub fn capture(
        session: &DiagramSession,
        metrics: SceneMetrics,
    ) -> Result<Self, DiagramExportError> {
        let preview = session.status().preview;
        if preview != PreviewState::Current {
            return Err(DiagramExportError::PreviewUnavailable(preview));
        }
        let graph = session
            .current_graph()
            .ok_or(DiagramExportError::PreviewUnavailable(PreviewState::Stale))?;
        let scene = build_scene(graph, metrics).map_err(DiagramExportError::Scene)?;
        Ok(Self {
            revision: session.revision(),
            scene,
        })
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn encode_svg(&self, scale: f64) -> Result<DiagramExportArtifact, DiagramExportError> {
        let transform = ExportTransform::new(self.scene.bounds, scale)?;
        let bytes = encode_svg(&self.scene, transform)?;
        Ok(DiagramExportArtifact {
            revision: self.revision,
            format: DiagramExportFormat::Svg,
            bytes,
            dimensions: Some((transform.width, transform.height)),
            warnings: DiagramExportWarnings::default(),
        })
    }

    pub fn encode_png(&self, scale: f64) -> Result<DiagramExportArtifact, DiagramExportError> {
        let fonts = default_font_set()?;
        encode_png_with_font_set(self, scale, fonts)
    }

    #[cfg(test)]
    fn encode_png_with_fonts(
        &self,
        scale: f64,
        fonts: &FontSet,
    ) -> Result<DiagramExportArtifact, DiagramExportError> {
        encode_png_with_font_set(self, scale, fonts)
    }
}

fn encode_png_with_font_set(
    snapshot: &DiagramImageSnapshot,
    scale: f64,
    fonts: &FontSet,
) -> Result<DiagramExportArtifact, DiagramExportError> {
    let transform = ExportTransform::new(snapshot.scene.bounds, scale)?;
    let text = prepare_raster_text(&snapshot.scene, transform, fonts)?;
    let work = estimate_raster_work(&snapshot.scene, transform, &text)?;
    if work > MAX_RASTER_PIXEL_VISITS {
        return Err(DiagramExportError::RasterWorkLimit {
            estimated: work,
            max: MAX_RASTER_PIXEL_VISITS,
        });
    }
    let byte_len = usize::try_from(transform.pixels)
        .ok()
        .and_then(|count| count.checked_mul(4))
        .ok_or(DiagramExportError::ImageAllocation)?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(byte_len)
        .map_err(|_| DiagramExportError::ImageAllocation)?;
    pixels.resize(byte_len, 0);
    let mut image = RgbaImage::from_raw(transform.width, transform.height, pixels)
        .ok_or(DiagramExportError::ImageAllocation)?;
    draw_scene(&mut image, &snapshot.scene, transform, &text, fonts);
    let mut writer = BoundedBytes::new(MAX_PNG_BYTES);
    let result = image::codecs::png::PngEncoder::new(&mut writer).write_image(
        image.as_raw(),
        image.width(),
        image.height(),
        image::ColorType::Rgba8,
    );
    if writer.exceeded {
        return Err(DiagramExportError::EncodedSizeLimit {
            format: DiagramExportFormat::Png,
            max_bytes: MAX_PNG_BYTES,
        });
    }
    result.map_err(|error| DiagramExportError::Encode(error.to_string()))?;
    Ok(DiagramExportArtifact {
        revision: snapshot.revision,
        format: DiagramExportFormat::Png,
        bytes: writer.bytes,
        dimensions: Some((transform.width, transform.height)),
        warnings: DiagramExportWarnings {
            glyph_substitutions: text.glyph_substitutions,
        },
    })
}

#[derive(Debug)]
pub enum DiagramExportError {
    PreviewUnavailable(PreviewState),
    Scene(super::scene::SceneError),
    InvalidScale,
    OutputTooLarge {
        width: u32,
        height: u32,
        max_side: u32,
        max_pixels: u64,
    },
    TooManyGlyphs {
        max_glyphs: usize,
    },
    RasterWorkLimit {
        estimated: u64,
        max: u64,
    },
    EncodedSizeLimit {
        format: DiagramExportFormat,
        max_bytes: usize,
    },
    InvalidXmlControl(char),
    Fonts(String),
    ImageAllocation,
    Encode(String),
}

impl fmt::Display for DiagramExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PreviewUnavailable(state) => {
                write!(
                    formatter,
                    "image export requires a current graph preview ({state:?})"
                )
            }
            Self::Scene(error) => write!(formatter, "cannot prepare Diagram export scene: {error}"),
            Self::InvalidScale => formatter.write_str("export scale must be finite and positive"),
            Self::OutputTooLarge {
                width,
                height,
                max_side,
                max_pixels,
            } => write!(
                formatter,
                "export size {width}×{height} exceeds the {max_side}-pixel side or {max_pixels}-pixel area limit"
            ),
            Self::TooManyGlyphs { max_glyphs } => write!(
                formatter,
                "export text exceeds the {max_glyphs}-glyph preparation limit"
            ),
            Self::RasterWorkLimit { estimated, max } => write!(
                formatter,
                "raster export requires {estimated} pixel visits; the limit is {max}"
            ),
            Self::EncodedSizeLimit { format, max_bytes } => write!(
                formatter,
                "{} output exceeds the {max_bytes}-byte limit",
                format.extension().to_ascii_uppercase()
            ),
            Self::InvalidXmlControl(character) => write!(
                formatter,
                "SVG cannot encode XML control character U+{:04X}",
                *character as u32
            ),
            Self::Fonts(error) => write!(formatter, "cannot load bundled Diagram fonts: {error}"),
            Self::ImageAllocation => formatter.write_str("cannot allocate bounded Diagram image"),
            Self::Encode(error) => write!(formatter, "cannot encode Diagram image: {error}"),
        }
    }
}

impl std::error::Error for DiagramExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Scene(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ExportTransform {
    scale: f64,
    width: u32,
    height: u32,
    pixels: u64,
    scene_min: ScenePoint,
}

impl ExportTransform {
    fn new(bounds: Option<SceneRect>, scale: f64) -> Result<Self, DiagramExportError> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err(DiagramExportError::InvalidScale);
        }
        let (width, height, scene_min) = if let Some(bounds) = bounds {
            let world_width = bounds.width();
            let world_height = bounds.height();
            let width = (world_width * scale + EXPORT_PADDING * 2.0).ceil();
            let height = (world_height * scale + EXPORT_PADDING * 2.0).ceil();
            if !width.is_finite()
                || !height.is_finite()
                || width < 1.0
                || height < 1.0
                || width > f64::from(u32::MAX)
                || height > f64::from(u32::MAX)
            {
                return Err(DiagramExportError::OutputTooLarge {
                    width: u32::MAX,
                    height: u32::MAX,
                    max_side: MAX_EXPORT_SIDE,
                    max_pixels: MAX_EXPORT_PIXELS,
                });
            }
            (width as u32, height as u32, bounds.min)
        } else {
            (EMPTY_IMAGE_SIDE, EMPTY_IMAGE_SIDE, ScenePoint::default())
        };
        let pixels = u64::from(width).checked_mul(u64::from(height)).ok_or(
            DiagramExportError::OutputTooLarge {
                width,
                height,
                max_side: MAX_EXPORT_SIDE,
                max_pixels: MAX_EXPORT_PIXELS,
            },
        )?;
        if width > MAX_EXPORT_SIDE || height > MAX_EXPORT_SIDE || pixels > MAX_EXPORT_PIXELS {
            return Err(DiagramExportError::OutputTooLarge {
                width,
                height,
                max_side: MAX_EXPORT_SIDE,
                max_pixels: MAX_EXPORT_PIXELS,
            });
        }
        Ok(Self {
            scale,
            width,
            height,
            pixels,
            scene_min,
        })
    }

    fn point(self, point: ScenePoint) -> PixelPoint {
        PixelPoint {
            x: EXPORT_PADDING + (point.x - self.scene_min.x) * self.scale,
            y: EXPORT_PADDING + (point.y - self.scene_min.y) * self.scale,
        }
    }

    fn rect(self, rect: SceneRect) -> PixelRect {
        PixelRect {
            min_x: EXPORT_PADDING + (rect.min.x - self.scene_min.x) * self.scale,
            min_y: EXPORT_PADDING + (rect.min.y - self.scene_min.y) * self.scale,
            max_x: EXPORT_PADDING + (rect.max.x - self.scene_min.x) * self.scale,
            max_y: EXPORT_PADDING + (rect.max.y - self.scene_min.y) * self.scale,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PixelPoint {
    x: f64,
    y: f64,
}

#[derive(Clone, Copy, Debug)]
struct PixelRect {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl PixelRect {
    fn width(self) -> f64 {
        self.max_x - self.min_x
    }

    fn height(self) -> f64 {
        self.max_y - self.min_y
    }

    fn area_pixels(self) -> Result<u64, DiagramExportError> {
        let width = checked_pixel_span(self.min_x, self.max_x)?;
        let height = checked_pixel_span(self.min_y, self.max_y)?;
        u64::from(width)
            .checked_mul(u64::from(height))
            .ok_or(DiagramExportError::RasterWorkLimit {
                estimated: u64::MAX,
                max: MAX_RASTER_PIXEL_VISITS,
            })
    }
}

fn checked_pixel_span(min: f64, max: f64) -> Result<u32, DiagramExportError> {
    if !min.is_finite() || !max.is_finite() || max < min {
        return Err(DiagramExportError::ImageAllocation);
    }
    let span = max.ceil() - min.floor();
    if !span.is_finite() || span < 0.0 || span > f64::from(u32::MAX) {
        return Err(DiagramExportError::OutputTooLarge {
            width: u32::MAX,
            height: u32::MAX,
            max_side: MAX_EXPORT_SIDE,
            max_pixels: MAX_EXPORT_PIXELS,
        });
    }
    Ok(span as u32)
}

struct FontFace {
    font: FontArc,
    tweak: FontTweak,
}

struct FontSet {
    faces: Vec<FontFace>,
}

impl FontSet {
    fn from_definitions(definitions: &FontDefinitions) -> Result<Self, String> {
        let family = definitions
            .families
            .get(&FontFamily::Proportional)
            .ok_or_else(|| "proportional font family is missing".to_owned())?;
        let mut faces = Vec::with_capacity(family.len());
        for name in family {
            let data = definitions
                .font_data
                .get(name)
                .ok_or_else(|| format!("proportional font {name:?} is missing"))?;
            let font = match &data.font {
                Cow::Borrowed(bytes) => FontRef::try_from_slice_and_index(bytes, data.index)
                    .map(FontArc::from)
                    .map_err(|error| format!("{name}: {error}"))?,
                Cow::Owned(bytes) => FontVec::try_from_vec_and_index(bytes.clone(), data.index)
                    .map(FontArc::from)
                    .map_err(|error| format!("{name}: {error}"))?,
            };
            if !data.tweak.scale.is_finite()
                || data.tweak.scale <= 0.0
                || !data.tweak.y_offset.is_finite()
                || !data.tweak.y_offset_factor.is_finite()
                || !data.tweak.tab_size.is_finite()
            {
                return Err(format!("{name}: font tweak is not finite and positive"));
            }
            faces.push(FontFace {
                font,
                tweak: data.tweak.clone(),
            });
        }
        if faces.is_empty() {
            return Err("proportional font family has no faces".to_owned());
        }
        Ok(Self { faces })
    }

    fn supports(&self, face_index: usize, character: char) -> bool {
        let face = &self.faces[face_index];
        let glyph_id = face.font.glyph_id(character);
        glyph_id.0 != 0 && (character.is_whitespace() || face.font.outline(glyph_id).is_some())
    }

    fn choose(&self, character: char) -> (usize, char, bool) {
        if character == '\t' {
            return (0, character, false);
        }
        if let Some((index, _)) = self
            .faces
            .iter()
            .enumerate()
            .find(|(index, _)| self.supports(*index, character))
        {
            return (index, character, false);
        }
        if character.is_whitespace() {
            return (0, ' ', false);
        }
        let replacement = if self
            .faces
            .iter()
            .enumerate()
            .any(|(index, _)| self.supports(index, '□'))
        {
            '□'
        } else {
            '?'
        };
        let face = self
            .faces
            .iter()
            .enumerate()
            .find(|(index, _)| self.supports(*index, replacement))
            .map_or(0, |(index, _)| index);
        (face, replacement, true)
    }
}

static DEFAULT_FONT_SET: OnceLock<Result<FontSet, String>> = OnceLock::new();

fn default_font_set() -> Result<&'static FontSet, DiagramExportError> {
    match DEFAULT_FONT_SET.get_or_init(|| FontSet::from_definitions(&FontDefinitions::default())) {
        Ok(fonts) => Ok(fonts),
        Err(error) => Err(DiagramExportError::Fonts(error.clone())),
    }
}

#[derive(Clone, Debug)]
struct GlyphPlacement {
    face_index: usize,
    character: char,
    font_size: f32,
    position_x: f32,
    position_y: f32,
    bounds: Option<PixelRect>,
    clip: PixelRect,
}

#[derive(Clone, Debug, Default)]
struct TextBlockPlan {
    lines: Vec<Vec<GlyphPlacement>>,
}

#[derive(Clone, Debug, Default)]
struct RasterTextPlan {
    groups: Vec<TextBlockPlan>,
    nodes: Vec<TextBlockPlan>,
    edges: Vec<Option<TextBlockPlan>>,
    glyph_substitutions: u32,
}

fn prepare_raster_text(
    scene: &DiagramScene,
    transform: ExportTransform,
    fonts: &FontSet,
) -> Result<RasterTextPlan, DiagramExportError> {
    let mut glyph_count = 0usize;
    for text in scene
        .groups
        .iter()
        .map(|group| &group.label)
        .chain(scene.nodes.iter().map(|node| &node.label))
        .chain(scene.edges.iter().filter_map(|edge| edge.label.as_ref()))
    {
        for line in &text.lines {
            glyph_count = add_glyph_count(glyph_count, line.text.chars().count())?;
        }
    }

    let mut plan = RasterTextPlan::default();
    plan.groups.reserve(scene.groups.len());
    for group in &scene.groups {
        plan.groups.push(prepare_text_block(
            &group.label,
            group.label_bounds,
            group.label_alignment,
            transform,
            fonts,
            &mut plan.glyph_substitutions,
        )?);
    }
    plan.nodes.reserve(scene.nodes.len());
    for node in &scene.nodes {
        plan.nodes.push(prepare_text_block(
            &node.label,
            node.label_bounds,
            node.label_alignment,
            transform,
            fonts,
            &mut plan.glyph_substitutions,
        )?);
    }
    plan.edges.reserve(scene.edges.len());
    for edge in &scene.edges {
        plan.edges.push(
            match (&edge.label, edge.label_bounds, edge.label_alignment) {
                (Some(label), Some(bounds), Some(alignment)) => Some(prepare_text_block(
                    label,
                    bounds,
                    alignment,
                    transform,
                    fonts,
                    &mut plan.glyph_substitutions,
                )?),
                _ => None,
            },
        );
    }
    Ok(plan)
}

fn add_glyph_count(current: usize, added: usize) -> Result<usize, DiagramExportError> {
    let count = current
        .checked_add(added)
        .ok_or(DiagramExportError::TooManyGlyphs {
            max_glyphs: MAX_PREPARED_GLYPHS,
        })?;
    if count > MAX_PREPARED_GLYPHS {
        return Err(DiagramExportError::TooManyGlyphs {
            max_glyphs: MAX_PREPARED_GLYPHS,
        });
    }
    Ok(count)
}

#[derive(Clone, Copy)]
struct SelectedGlyph {
    face_index: usize,
    character: char,
}

fn prepare_text_block(
    text: &PreparedText,
    bounds: SceneRect,
    alignment: TextHorizontalAlignment,
    transform: ExportTransform,
    fonts: &FontSet,
    substitutions: &mut u32,
) -> Result<TextBlockPlan, DiagramExportError> {
    let clip = transform.rect(bounds);
    let mut lines = Vec::with_capacity(text.lines.len());
    for (line_index, line) in text.lines.iter().enumerate() {
        let selected = line
            .text
            .chars()
            .map(|character| {
                let (face_index, character, substituted) = fonts.choose(character);
                if substituted {
                    *substitutions = (*substitutions).saturating_add(1);
                }
                SelectedGlyph {
                    face_index,
                    character,
                }
            })
            .collect::<Vec<_>>();
        let line_top = clip.min_y + line_index as f64 * text.line_height * transform.scale;
        let line_height = text.line_height * transform.scale;
        let clip_line = PixelRect {
            min_x: clip.min_x,
            max_x: clip.max_x,
            min_y: line_top,
            max_y: (line_top + line_height).min(clip.max_y),
        };
        if selected.is_empty() || clip_line.height() <= 0.0 || clip_line.width() <= 0.0 {
            lines.push(Vec::new());
            continue;
        }

        let mut font_size = (line_height * 0.86)
            .max(f64::from(f32::MIN_POSITIVE))
            .min(f64::from(f32::MAX)) as f32;
        let mut placements = Vec::new();
        for _ in 0..5 {
            let (measured, total_advance, ink_bounds) = measure_glyph_line(
                &selected,
                fonts,
                font_size,
                line_top,
                line_height,
                transform.scale,
            )?;
            placements = measured;
            let ink_width = ink_bounds.map_or(0.0, PixelRect::width);
            let ink_height = ink_bounds.map_or(0.0, PixelRect::height);
            let mut factor: f64 = 1.0;
            if total_advance > clip_line.width() && total_advance > 0.0 {
                factor = factor.min(clip_line.width() / total_advance);
            }
            if ink_width > clip_line.width() && ink_width > 0.0 {
                factor = factor.min(clip_line.width() / ink_width);
            }
            if ink_height > clip_line.height() && ink_height > 0.0 {
                factor = factor.min(clip_line.height() / ink_height);
            }
            if factor >= 0.999 || !factor.is_finite() || factor <= 0.0 {
                break;
            }
            font_size =
                (f64::from(font_size) * factor * 0.98).max(f64::from(f32::MIN_POSITIVE)) as f32;
        }

        let total_advance = placements
            .iter()
            .map(|placement| placement.position_x as f64 + glyph_advance(placement, fonts))
            .fold(0.0, f64::max);
        let start_x = match alignment {
            TextHorizontalAlignment::Leading => clip_line.min_x,
            TextHorizontalAlignment::Center => {
                clip_line.min_x + (clip_line.width() - total_advance).max(0.0) / 2.0
            }
        };
        for placement in &mut placements {
            placement.position_x += start_x as f32;
            placement.clip = clip_line;
            let bounds = glyph_bounds_for_placement(placement, fonts);
            placement.bounds = bounds;
        }
        lines.push(placements);
    }
    Ok(TextBlockPlan { lines })
}

fn glyph_advance(placement: &GlyphPlacement, fonts: &FontSet) -> f64 {
    let face = &fonts.faces[placement.face_index];
    // FontTweak.scale changes drawn ink, not the epaint layout advance.
    let scaled = face.font.as_scaled(placement.font_size);
    if placement.character == '\t' {
        f64::from(scaled.h_advance(scaled.glyph_id(' ')) * face.tweak.tab_size)
    } else {
        f64::from(scaled.h_advance(scaled.glyph_id(placement.character)))
    }
}

fn measure_glyph_line(
    selected: &[SelectedGlyph],
    fonts: &FontSet,
    font_size: f32,
    line_top: f64,
    line_height: f64,
    export_scale: f64,
) -> Result<(Vec<GlyphPlacement>, f64, Option<PixelRect>), DiagramExportError> {
    let primary = &fonts.faces[0];
    let primary_scaled = primary.font.as_scaled(font_size * primary.tweak.scale);
    let primary_ascent = f64::from(primary_scaled.ascent());
    let baseline =
        line_top + (line_height - f64::from(primary_scaled.height())) / 2.0 + primary_ascent;
    let mut cursor = 0.0f64;
    let mut placements = Vec::with_capacity(selected.len());
    let mut ink_bounds: Option<PixelRect> = None;
    for chosen in selected {
        let face = &fonts.faces[chosen.face_index];
        let visual_scaled = face.font.as_scaled(font_size * face.tweak.scale);
        let layout_scaled = face.font.as_scaled(font_size);
        let glyph_id = visual_scaled.glyph_id(chosen.character);
        let advance = if chosen.character == '\t' {
            f64::from(layout_scaled.h_advance(layout_scaled.glyph_id(' ')) * face.tweak.tab_size)
        } else {
            f64::from(layout_scaled.h_advance(glyph_id))
        };
        let offset = font_vertical_offset_pixels(font_size, &face.tweak, export_scale);
        let y = baseline + f64::from(visual_scaled.ascent()) - primary_ascent + offset;
        let x = cursor;
        let bounds = if chosen.character == '\t' || chosen.character.is_whitespace() {
            None
        } else {
            let mut glyph = visual_scaled.scaled_glyph(chosen.character);
            glyph.position = point(x as f32, y as f32);
            visual_scaled.font().outline_glyph(glyph).map(|outlined| {
                let px = outlined.px_bounds();
                PixelRect {
                    min_x: f64::from(px.min.x),
                    min_y: f64::from(px.min.y),
                    max_x: f64::from(px.max.x),
                    max_y: f64::from(px.max.y),
                }
            })
        };
        if let Some(current) = bounds {
            ink_bounds = Some(match ink_bounds {
                Some(mut union) => {
                    union.min_x = union.min_x.min(current.min_x);
                    union.min_y = union.min_y.min(current.min_y);
                    union.max_x = union.max_x.max(current.max_x);
                    union.max_y = union.max_y.max(current.max_y);
                    union
                }
                None => current,
            });
        }
        placements.push(GlyphPlacement {
            face_index: chosen.face_index,
            character: chosen.character,
            font_size,
            position_x: x as f32,
            position_y: y as f32,
            bounds,
            clip: PixelRect {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 0.0,
                max_y: 0.0,
            },
        });
        cursor += advance;
    }
    if !cursor.is_finite()
        || placements
            .iter()
            .any(|placement| !placement.position_x.is_finite() || !placement.position_y.is_finite())
    {
        return Err(DiagramExportError::ImageAllocation);
    }
    Ok((placements, cursor, ink_bounds))
}

fn font_vertical_offset_pixels(font_size_pixels: f32, tweak: &FontTweak, export_scale: f64) -> f64 {
    // The font size already includes export scale; the absolute tweak is in points.
    f64::from(font_size_pixels * tweak.scale * tweak.y_offset_factor)
        + f64::from(tweak.y_offset) * export_scale
}

fn glyph_bounds_for_placement(placement: &GlyphPlacement, fonts: &FontSet) -> Option<PixelRect> {
    if placement.character == '\t' || placement.character.is_whitespace() {
        return None;
    }
    let face = &fonts.faces[placement.face_index];
    let scaled = face.font.as_scaled(placement.font_size * face.tweak.scale);
    let mut glyph = scaled.scaled_glyph(placement.character);
    glyph.position = point(placement.position_x, placement.position_y);
    scaled.font().outline_glyph(glyph).map(|outlined| {
        let bounds = outlined.px_bounds();
        PixelRect {
            min_x: f64::from(bounds.min.x),
            min_y: f64::from(bounds.min.y),
            max_x: f64::from(bounds.max.x),
            max_y: f64::from(bounds.max.y),
        }
    })
}

fn estimate_raster_work(
    scene: &DiagramScene,
    transform: ExportTransform,
    text: &RasterTextPlan,
) -> Result<u64, DiagramExportError> {
    let mut work = transform.pixels;
    for group in &scene.groups {
        charge_work(&mut work, transform.rect(group.bounds).area_pixels()?)?;
        charge_polyline_work(
            &mut work,
            &rounded_rect_points(transform.rect(group.bounds), 9.0),
        )?;
    }
    for edge in &scene.edges {
        let mut previous = transform.point(edge.path.start);
        for step in 1..=CUBIC_SEGMENTS {
            let current =
                transform.point(cubic_point(edge.path, step as f64 / CUBIC_SEGMENTS as f64));
            charge_line_work(&mut work, previous, current)?;
            previous = current;
        }
        let triangle = arrow_points(edge.arrowhead, transform);
        charge_work(&mut work, points_bounds(&triangle).area_pixels()?)?;
        charge_polyline_work(&mut work, &closed_points(&triangle))?;
        if let Some(bounds) = edge.label_bounds {
            charge_work(&mut work, transform.rect(bounds).area_pixels()?)?;
        }
    }
    for node in &scene.nodes {
        charge_work(&mut work, transform.rect(node.bounds).area_pixels()?)?;
        charge_polyline_work(
            &mut work,
            &node_outline_points(node, transform, scene.metrics.rounded_corner_radius),
        )?;
    }
    for block in text
        .groups
        .iter()
        .chain(text.edges.iter().filter_map(Option::as_ref))
        .chain(text.nodes.iter())
    {
        for line in &block.lines {
            for glyph in line {
                if let Some(bounds) = glyph.bounds {
                    charge_work(&mut work, bounds.area_pixels()?)?;
                }
            }
        }
    }
    Ok(work)
}

fn charge_work(work: &mut u64, amount: u64) -> Result<(), DiagramExportError> {
    *work = work
        .checked_add(amount)
        .ok_or(DiagramExportError::RasterWorkLimit {
            estimated: u64::MAX,
            max: MAX_RASTER_PIXEL_VISITS,
        })?;
    if *work > MAX_RASTER_PIXEL_VISITS {
        return Err(DiagramExportError::RasterWorkLimit {
            estimated: *work,
            max: MAX_RASTER_PIXEL_VISITS,
        });
    }
    Ok(())
}

fn charge_line_work(
    work: &mut u64,
    start: PixelPoint,
    end: PixelPoint,
) -> Result<(), DiagramExportError> {
    let dx = (end.x - start.x).abs();
    let dy = (end.y - start.y).abs();
    if !dx.is_finite() || !dy.is_finite() {
        return Err(DiagramExportError::ImageAllocation);
    }
    let steps = dx.max(dy).ceil().max(1.0);
    if steps > f64::from(u32::MAX) {
        return Err(DiagramExportError::RasterWorkLimit {
            estimated: u64::MAX,
            max: MAX_RASTER_PIXEL_VISITS,
        });
    }
    let brush_width = (STROKE_WIDTH / 2.0).ceil() as u64 * 2 + 1;
    charge_work(
        work,
        (steps as u64 + 1).saturating_mul(brush_width * brush_width),
    )
}

fn charge_polyline_work(work: &mut u64, points: &[PixelPoint]) -> Result<(), DiagramExportError> {
    for pair in points.windows(2) {
        charge_line_work(work, pair[0], pair[1])?;
    }
    Ok(())
}

fn cubic_point(path: CubicBezier, t: f64) -> ScenePoint {
    let inverse = 1.0 - t;
    ScenePoint {
        x: path.start.x * inverse * inverse * inverse
            + 3.0 * path.control1.x * inverse * inverse * t
            + 3.0 * path.control2.x * inverse * t * t
            + path.end.x * t * t * t,
        y: path.start.y * inverse * inverse * inverse
            + 3.0 * path.control1.y * inverse * inverse * t
            + 3.0 * path.control2.y * inverse * t * t
            + path.end.y * t * t * t,
    }
}

fn arrow_points(arrow: ArrowHead, transform: ExportTransform) -> [PixelPoint; 3] {
    [
        transform.point(arrow.tip),
        transform.point(arrow.left),
        transform.point(arrow.right),
    ]
}

fn points_bounds(points: &[PixelPoint]) -> PixelRect {
    let mut bounds = PixelRect {
        min_x: f64::INFINITY,
        min_y: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
        max_y: f64::NEG_INFINITY,
    };
    for point in points {
        bounds.min_x = bounds.min_x.min(point.x);
        bounds.min_y = bounds.min_y.min(point.y);
        bounds.max_x = bounds.max_x.max(point.x);
        bounds.max_y = bounds.max_y.max(point.y);
    }
    bounds
}

fn closed_points(points: &[PixelPoint]) -> Vec<PixelPoint> {
    let mut closed = Vec::with_capacity(points.len() + 1);
    closed.extend_from_slice(points);
    if let Some(first) = points.first() {
        closed.push(*first);
    }
    closed
}

fn node_outline_points(
    node: &SceneNode,
    transform: ExportTransform,
    radius: f64,
) -> Vec<PixelPoint> {
    let rect = transform.rect(node.bounds);
    match node.shape {
        NodeShape::Rectangle | NodeShape::Diamond => {
            if node.shape == NodeShape::Diamond {
                vec![
                    PixelPoint {
                        x: (rect.min_x + rect.max_x) / 2.0,
                        y: rect.min_y,
                    },
                    PixelPoint {
                        x: rect.max_x,
                        y: (rect.min_y + rect.max_y) / 2.0,
                    },
                    PixelPoint {
                        x: (rect.min_x + rect.max_x) / 2.0,
                        y: rect.max_y,
                    },
                    PixelPoint {
                        x: rect.min_x,
                        y: (rect.min_y + rect.max_y) / 2.0,
                    },
                    PixelPoint {
                        x: (rect.min_x + rect.max_x) / 2.0,
                        y: rect.min_y,
                    },
                ]
            } else {
                vec![
                    PixelPoint {
                        x: rect.min_x,
                        y: rect.min_y,
                    },
                    PixelPoint {
                        x: rect.max_x,
                        y: rect.min_y,
                    },
                    PixelPoint {
                        x: rect.max_x,
                        y: rect.max_y,
                    },
                    PixelPoint {
                        x: rect.min_x,
                        y: rect.max_y,
                    },
                    PixelPoint {
                        x: rect.min_x,
                        y: rect.min_y,
                    },
                ]
            }
        }
        NodeShape::Rounded => rounded_rect_points(rect, radius * transform.scale),
    }
}

fn rounded_rect_points(rect: PixelRect, radius: f64) -> Vec<PixelPoint> {
    let radius = radius
        .min(rect.width() / 2.0)
        .min(rect.height() / 2.0)
        .max(0.0);
    let corners = [
        (
            rect.max_x - radius,
            rect.min_y + radius,
            -std::f64::consts::FRAC_PI_2,
        ),
        (rect.max_x - radius, rect.max_y - radius, 0.0),
        (
            rect.min_x + radius,
            rect.max_y - radius,
            std::f64::consts::FRAC_PI_2,
        ),
        (
            rect.min_x + radius,
            rect.min_y + radius,
            std::f64::consts::PI,
        ),
    ];
    let mut points = Vec::with_capacity(49);
    for (center_x, center_y, start_angle) in corners {
        for step in 0..=12 {
            let angle = start_angle + std::f64::consts::FRAC_PI_2 * step as f64 / 12.0;
            points.push(PixelPoint {
                x: center_x + radius * angle.cos(),
                y: center_y + radius * angle.sin(),
            });
        }
    }
    points.push(points[0]);
    points
}

fn draw_scene(
    image: &mut RgbaImage,
    scene: &DiagramScene,
    transform: ExportTransform,
    text: &RasterTextPlan,
    fonts: &FontSet,
) {
    image
        .pixels_mut()
        .for_each(|pixel| *pixel = Rgba([249, 250, 251, 255]));
    for group in &scene.groups {
        let bounds = transform.rect(group.bounds);
        let radius = 9.0;
        fill_shape(
            image,
            bounds,
            ShapeFill::Rounded,
            radius,
            Rgba([239, 246, 255, 255]),
        );
        draw_polyline(
            image,
            &rounded_rect_points(bounds, radius),
            Rgba([100, 116, 139, 255]),
        );
    }
    for edge in &scene.edges {
        let mut previous = transform.point(edge.path.start);
        for step in 1..=CUBIC_SEGMENTS {
            let current =
                transform.point(cubic_point(edge.path, step as f64 / CUBIC_SEGMENTS as f64));
            draw_line(image, previous, current, Rgba([71, 85, 105, 255]));
            previous = current;
        }
        let triangle = arrow_points(edge.arrowhead, transform);
        fill_triangle(image, triangle, Rgba([71, 85, 105, 255]));
        draw_polyline(image, &closed_points(&triangle), Rgba([71, 85, 105, 255]));
        if let Some(bounds) = edge.label_bounds {
            fill_shape(
                image,
                transform.rect(bounds),
                ShapeFill::Rounded,
                3.0,
                Rgba([255, 255, 255, 255]),
            );
        }
    }
    for node in &scene.nodes {
        let bounds = transform.rect(node.bounds);
        let shape = match node.shape {
            NodeShape::Rectangle => ShapeFill::Rectangle,
            NodeShape::Rounded => ShapeFill::Rounded,
            NodeShape::Diamond => ShapeFill::Diamond,
        };
        fill_shape(
            image,
            bounds,
            shape,
            scene.metrics.rounded_corner_radius * transform.scale,
            Rgba([255, 255, 255, 255]),
        );
        draw_polyline(
            image,
            &node_outline_points(node, transform, scene.metrics.rounded_corner_radius),
            Rgba([51, 65, 85, 255]),
        );
    }
    for group in &text.groups {
        draw_text_block(image, group, fonts, Rgba([15, 23, 42, 255]));
    }
    for label in text.edges.iter().flatten() {
        draw_text_block(image, label, fonts, Rgba([15, 23, 42, 255]));
    }
    for node in &text.nodes {
        draw_text_block(image, node, fonts, Rgba([15, 23, 42, 255]));
    }
}

#[derive(Clone, Copy)]
enum ShapeFill {
    Rectangle,
    Rounded,
    Diamond,
}

fn fill_shape(
    image: &mut RgbaImage,
    rect: PixelRect,
    shape: ShapeFill,
    corner_radius: f64,
    color: Rgba<u8>,
) {
    let Some((left, top, right, bottom)) =
        clipped_pixel_bounds(rect, image.width(), image.height())
    else {
        return;
    };
    let center_x = (rect.min_x + rect.max_x) / 2.0;
    let center_y = (rect.min_y + rect.max_y) / 2.0;
    let half_width = rect.width() / 2.0;
    let half_height = rect.height() / 2.0;
    let radius = match shape {
        ShapeFill::Rounded => corner_radius.min(half_width).min(half_height),
        _ => 0.0,
    };
    for y in top..bottom {
        for x in left..right {
            let px = f64::from(x) + 0.5;
            let py = f64::from(y) + 0.5;
            let inside = match shape {
                ShapeFill::Rectangle => true,
                ShapeFill::Diamond => {
                    half_width > 0.0
                        && half_height > 0.0
                        && (px - center_x).abs() / half_width + (py - center_y).abs() / half_height
                            <= 1.0
                }
                ShapeFill::Rounded => {
                    let dx = (px - center_x).abs() - (half_width - radius);
                    let dy = (py - center_y).abs() - (half_height - radius);
                    dx.max(0.0).powi(2) + dy.max(0.0).powi(2) <= radius.powi(2)
                }
            };
            if inside {
                blend_pixel(image, x, y, color);
            }
        }
    }
}

fn fill_triangle(image: &mut RgbaImage, points: [PixelPoint; 3], color: Rgba<u8>) {
    let bounds = points_bounds(&points);
    let Some((left, top, right, bottom)) =
        clipped_pixel_bounds(bounds, image.width(), image.height())
    else {
        return;
    };
    let [a, b, c] = points;
    for y in top..bottom {
        for x in left..right {
            let point = PixelPoint {
                x: f64::from(x) + 0.5,
                y: f64::from(y) + 0.5,
            };
            let s1 = triangle_sign(point, a, b);
            let s2 = triangle_sign(point, b, c);
            let s3 = triangle_sign(point, c, a);
            if (s1 >= 0.0 && s2 >= 0.0 && s3 >= 0.0) || (s1 <= 0.0 && s2 <= 0.0 && s3 <= 0.0) {
                blend_pixel(image, x, y, color);
            }
        }
    }
}

fn triangle_sign(point: PixelPoint, a: PixelPoint, b: PixelPoint) -> f64 {
    (point.x - b.x) * (a.y - b.y) - (a.x - b.x) * (point.y - b.y)
}

fn draw_polyline(image: &mut RgbaImage, points: &[PixelPoint], color: Rgba<u8>) {
    for pair in points.windows(2) {
        draw_line(image, pair[0], pair[1], color);
    }
}

fn draw_line(image: &mut RgbaImage, start: PixelPoint, end: PixelPoint, color: Rgba<u8>) {
    let steps = (end.x - start.x)
        .abs()
        .max((end.y - start.y).abs())
        .ceil()
        .max(1.0) as u32;
    let radius = (STROKE_WIDTH / 2.0).ceil() as i32;
    for step in 0..=steps {
        let t = f64::from(step) / f64::from(steps);
        let center_x = (start.x + (end.x - start.x) * t).round() as i32;
        let center_y = (start.y + (end.y - start.y) * t).round() as i32;
        for y in center_y - radius..=center_y + radius {
            for x in center_x - radius..=center_x + radius {
                if (x - center_x).pow(2) + (y - center_y).pow(2) <= radius.pow(2)
                    && x >= 0
                    && y >= 0
                    && x < image.width() as i32
                    && y < image.height() as i32
                {
                    blend_pixel(image, x as u32, y as u32, color);
                }
            }
        }
    }
}

fn clipped_pixel_bounds(rect: PixelRect, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    if !rect.min_x.is_finite()
        || !rect.min_y.is_finite()
        || !rect.max_x.is_finite()
        || !rect.max_y.is_finite()
    {
        return None;
    }
    let left = rect.min_x.floor().clamp(0.0, f64::from(width)) as u32;
    let top = rect.min_y.floor().clamp(0.0, f64::from(height)) as u32;
    let right = rect.max_x.ceil().clamp(0.0, f64::from(width)) as u32;
    let bottom = rect.max_y.ceil().clamp(0.0, f64::from(height)) as u32;
    (left < right && top < bottom).then_some((left, top, right, bottom))
}

fn blend_pixel(image: &mut RgbaImage, x: u32, y: u32, source: Rgba<u8>) {
    let destination = image.get_pixel_mut(x, y);
    let alpha = u32::from(source[3]);
    let inverse = 255 - alpha;
    for channel in 0..3 {
        destination[channel] =
            ((u32::from(source[channel]) * alpha + u32::from(destination[channel]) * inverse + 127)
                / 255) as u8;
    }
    destination[3] = (alpha + u32::from(destination[3]) * inverse / 255).min(255) as u8;
}

fn draw_text_block(image: &mut RgbaImage, block: &TextBlockPlan, fonts: &FontSet, color: Rgba<u8>) {
    for line in &block.lines {
        for placement in line {
            // Whitespace placements reserve layout advance but have no measured ink.
            if placement.character == '\t' || placement.character.is_whitespace() {
                continue;
            }
            let face = &fonts.faces[placement.face_index];
            let scaled = face.font.as_scaled(placement.font_size * face.tweak.scale);
            let mut glyph = scaled.scaled_glyph(placement.character);
            glyph.position = point(placement.position_x, placement.position_y);
            let Some(outlined) = scaled.font().outline_glyph(glyph) else {
                continue;
            };
            let bounds = outlined.px_bounds();
            let origin_x = bounds.min.x.floor() as i64;
            let origin_y = bounds.min.y.floor() as i64;
            let clip_left = placement.clip.min_x.floor() as i64;
            let clip_top = placement.clip.min_y.floor() as i64;
            let clip_right = placement.clip.max_x.ceil() as i64;
            let clip_bottom = placement.clip.max_y.ceil() as i64;
            outlined.draw(|x, y, coverage| {
                let pixel_x = origin_x + i64::from(x);
                let pixel_y = origin_y + i64::from(y);
                if pixel_x < clip_left
                    || pixel_y < clip_top
                    || pixel_x >= clip_right
                    || pixel_y >= clip_bottom
                    || pixel_x < 0
                    || pixel_y < 0
                    || pixel_x >= i64::from(image.width())
                    || pixel_y >= i64::from(image.height())
                {
                    return;
                }
                let alpha = (f32::from(color[3]) * coverage).round().clamp(0.0, 255.0) as u8;
                blend_pixel(
                    image,
                    pixel_x as u32,
                    pixel_y as u32,
                    Rgba([color[0], color[1], color[2], alpha]),
                );
            });
        }
    }
}

fn encode_svg(
    scene: &DiagramScene,
    transform: ExportTransform,
) -> Result<Vec<u8>, DiagramExportError> {
    validate_svg_labels(scene)?;
    let mut output = BoundedString::new(MAX_SVG_BYTES);
    svg_fmt(
        &mut output,
        format_args!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",
            transform.width, transform.height, transform.width, transform.height
        ),
    )?;
    svg_fmt(
        &mut output,
        format_args!(
            "<rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#f9fafb\"/>\n",
            transform.width, transform.height
        ),
    )?;
    for group in &scene.groups {
        let rect = transform.rect(group.bounds);
        let radius = 9.0;
        svg_fmt(
            &mut output,
            format_args!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"#eff6ff\" stroke=\"#64748b\" stroke-width=\"{}\"/>\n",
                svg_num(rect.min_x),
                svg_num(rect.min_y),
                svg_num(rect.width()),
                svg_num(rect.height()),
                svg_num(radius),
                svg_num(STROKE_WIDTH)
            ),
        )?;
    }
    for edge in &scene.edges {
        let path = edge.path;
        let start = transform.point(path.start);
        let control1 = transform.point(path.control1);
        let control2 = transform.point(path.control2);
        let end = transform.point(path.end);
        svg_fmt(
            &mut output,
            format_args!(
                "<path d=\"M {} {} C {} {}, {} {}, {} {}\" fill=\"none\" stroke=\"#475569\" stroke-width=\"{}\" stroke-linecap=\"round\"/>\n",
                svg_num(start.x),
                svg_num(start.y),
                svg_num(control1.x),
                svg_num(control1.y),
                svg_num(control2.x),
                svg_num(control2.y),
                svg_num(end.x),
                svg_num(end.y),
                svg_num(STROKE_WIDTH)
            ),
        )?;
        let triangle = arrow_points(edge.arrowhead, transform);
        svg_fmt(
            &mut output,
            format_args!(
                "<polygon points=\"{} {},{} {},{} {}\" fill=\"#475569\"/>\n",
                svg_num(triangle[0].x),
                svg_num(triangle[0].y),
                svg_num(triangle[1].x),
                svg_num(triangle[1].y),
                svg_num(triangle[2].x),
                svg_num(triangle[2].y)
            ),
        )?;
        if let Some(bounds) = edge.label_bounds {
            let rect = transform.rect(bounds);
            svg_fmt(
                &mut output,
                format_args!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"3\" fill=\"#ffffff\"/>\n",
                    svg_num(rect.min_x),
                    svg_num(rect.min_y),
                    svg_num(rect.width()),
                    svg_num(rect.height())
                ),
            )?;
        }
    }
    for node in &scene.nodes {
        write_svg_node(
            &mut output,
            node,
            transform,
            scene.metrics.rounded_corner_radius,
        )?;
    }
    let mut clip_id = 0usize;
    for group in &scene.groups {
        write_svg_text_block(
            &mut output,
            &group.label,
            group.label_bounds,
            group.label_alignment,
            transform,
            &mut clip_id,
        )?;
    }
    for edge in &scene.edges {
        if let (Some(label), Some(bounds), Some(alignment)) =
            (&edge.label, edge.label_bounds, edge.label_alignment)
        {
            write_svg_text_block(
                &mut output,
                label,
                bounds,
                alignment,
                transform,
                &mut clip_id,
            )?;
        }
    }
    for node in &scene.nodes {
        write_svg_text_block(
            &mut output,
            &node.label,
            node.label_bounds,
            node.label_alignment,
            transform,
            &mut clip_id,
        )?;
    }
    output.push_str("</svg>\n")?;
    Ok(output.value.into_bytes())
}

fn write_svg_node(
    output: &mut BoundedString,
    node: &SceneNode,
    transform: ExportTransform,
    radius: f64,
) -> Result<(), DiagramExportError> {
    let rect = transform.rect(node.bounds);
    match node.shape {
        NodeShape::Rectangle => svg_fmt(
            output,
            format_args!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#ffffff\" stroke=\"#334155\" stroke-width=\"{}\"/>\n",
                svg_num(rect.min_x),
                svg_num(rect.min_y),
                svg_num(rect.width()),
                svg_num(rect.height()),
                svg_num(STROKE_WIDTH)
            ),
        ),
        NodeShape::Rounded => svg_fmt(
            output,
            format_args!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"#ffffff\" stroke=\"#334155\" stroke-width=\"{}\"/>\n",
                svg_num(rect.min_x),
                svg_num(rect.min_y),
                svg_num(rect.width()),
                svg_num(rect.height()),
                svg_num(
                    (radius * transform.scale)
                        .min(rect.width() / 2.0)
                        .min(rect.height() / 2.0)
                ),
                svg_num(STROKE_WIDTH)
            ),
        ),
        NodeShape::Diamond => svg_fmt(
            output,
            format_args!(
                "<polygon points=\"{} {},{} {},{} {},{} {}\" fill=\"#ffffff\" stroke=\"#334155\" stroke-width=\"{}\"/>\n",
                svg_num((rect.min_x + rect.max_x) / 2.0),
                svg_num(rect.min_y),
                svg_num(rect.max_x),
                svg_num((rect.min_y + rect.max_y) / 2.0),
                svg_num((rect.min_x + rect.max_x) / 2.0),
                svg_num(rect.max_y),
                svg_num(rect.min_x),
                svg_num((rect.min_y + rect.max_y) / 2.0),
                svg_num(STROKE_WIDTH)
            ),
        ),
    }
}

fn write_svg_text_block(
    output: &mut BoundedString,
    text: &PreparedText,
    bounds: SceneRect,
    alignment: TextHorizontalAlignment,
    transform: ExportTransform,
    clip_id: &mut usize,
) -> Result<(), DiagramExportError> {
    let rect = transform.rect(bounds);
    let id = *clip_id;
    *clip_id = clip_id.saturating_add(1);
    svg_fmt(
        output,
        format_args!(
            "<defs><clipPath id=\"label-{id}\" clipPathUnits=\"userSpaceOnUse\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath></defs>\n<g clip-path=\"url(#label-{id})\" fill=\"#0f172a\" font-family=\"sans-serif\">\n",
            svg_num(rect.min_x),
            svg_num(rect.min_y),
            svg_num(rect.width()),
            svg_num(rect.height())
        ),
    )?;
    let line_height = text.line_height * transform.scale;
    for (index, line) in text.lines.iter().enumerate() {
        if line.scalar_count == 0 || line.text.is_empty() {
            continue;
        }
        let line_width = (line.reserved_width * transform.scale).min(rect.width());
        if line_width <= 0.0 {
            continue;
        }
        let x = match alignment {
            TextHorizontalAlignment::Leading => rect.min_x,
            TextHorizontalAlignment::Center => rect.min_x + (rect.width() - line_width) / 2.0,
        };
        let y = rect.min_y + index as f64 * line_height + line_height * 0.82;
        let font_size =
            (line_height * 0.86).min(line_width / (line.scalar_count as f64 * 0.42).max(1.0));
        svg_fmt(
            output,
            format_args!(
                "<text xml:space=\"preserve\" x=\"{}\" y=\"{}\" font-size=\"{}\" textLength=\"{}\" lengthAdjust=\"spacingAndGlyphs\">",
                svg_num(x),
                svg_num(y),
                svg_num(font_size),
                svg_num(line_width)
            ),
        )?;
        write_svg_escaped(output, &line.text)?;
        output.push_str("</text>\n")?;
    }
    output.push_str("</g>\n")
}

fn validate_svg_labels(scene: &DiagramScene) -> Result<(), DiagramExportError> {
    for label in scene
        .groups
        .iter()
        .map(|group| &group.label)
        .chain(scene.nodes.iter().map(|node| &node.label))
        .chain(scene.edges.iter().filter_map(|edge| edge.label.as_ref()))
    {
        for character in label.lines.iter().flat_map(|line| line.text.chars()) {
            if !is_xml_character(character) {
                return Err(DiagramExportError::InvalidXmlControl(character));
            }
        }
    }
    Ok(())
}

fn is_xml_character(character: char) -> bool {
    let value = character as u32;
    matches!(value, 0x9 | 0xA | 0xD)
        || (0x20..=0xD7FF).contains(&value)
        || (0xE000..=0xFFFD).contains(&value)
        || (0x10000..=0x10FFFF).contains(&value)
}

fn write_svg_escaped(output: &mut BoundedString, text: &str) -> Result<(), DiagramExportError> {
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;")?,
            '<' => output.push_str("&lt;")?,
            '>' => output.push_str("&gt;")?,
            '"' => output.push_str("&quot;")?,
            '\'' => output.push_str("&apos;")?,
            character if is_xml_character(character) => {
                let mut buffer = [0; 4];
                output.push_str(character.encode_utf8(&mut buffer))?;
            }
            character => return Err(DiagramExportError::InvalidXmlControl(character)),
        }
    }
    Ok(())
}

fn svg_num(value: f64) -> String {
    if value == 0.0 {
        "0".to_owned()
    } else {
        format!("{value:.3}")
    }
}

struct BoundedString {
    value: String,
    max_bytes: usize,
}

impl BoundedString {
    fn new(max_bytes: usize) -> Self {
        Self {
            value: String::with_capacity(max_bytes.min(4 * 1024)),
            max_bytes,
        }
    }

    fn push_str(&mut self, value: &str) -> Result<(), DiagramExportError> {
        if self.value.len().saturating_add(value.len()) > self.max_bytes {
            return Err(DiagramExportError::EncodedSizeLimit {
                format: DiagramExportFormat::Svg,
                max_bytes: self.max_bytes,
            });
        }
        self.value.push_str(value);
        Ok(())
    }
}

impl fmt::Write for BoundedString {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.push_str(value).map_err(|_| fmt::Error)
    }
}

fn svg_fmt(output: &mut BoundedString, args: fmt::Arguments<'_>) -> Result<(), DiagramExportError> {
    fmt::write(output, args).map_err(|_| DiagramExportError::EncodedSizeLimit {
        format: DiagramExportFormat::Svg,
        max_bytes: output.max_bytes,
    })
}

struct BoundedBytes {
    bytes: Vec<u8>,
    max_bytes: usize,
    exceeded: bool,
}

impl BoundedBytes {
    fn new(max_bytes: usize) -> Self {
        Self {
            bytes: Vec::new(),
            max_bytes,
            exceeded: false,
        }
    }
}

impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > self.max_bytes {
            self.exceeded = true;
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "encoded size limit exceeded",
            ));
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(|error| io::Error::other(error.to_string()))?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BoundedBytes, BoundedString, DiagramExportError, DiagramExportFormat,
        DiagramExportPublicationError, DiagramImageSnapshot, DiagramSourceSnapshot, FontSet,
        MAX_EXPORT_PIXELS, MAX_EXPORT_SIDE, MAX_PNG_BYTES, MAX_PREPARED_GLYPHS,
        MAX_RASTER_PIXEL_VISITS, is_xml_character,
    };
    use crate::diagram::{
        DiagramDocumentController, DiagramSession, FlowDirection, GraphOperation, NodePosition,
        Position, SceneMetrics,
    };
    use eframe::egui::{FontDefinitions, FontTweak};
    use image::RgbaImage;
    use std::fs;
    use std::io::Write;
    use std::sync::Arc;

    fn fresh_session(source: &str) -> DiagramSession {
        DiagramSession::from_fresh_source(source).unwrap()
    }

    fn scene_source() -> &'static str {
        concat!(
            "flowchart TD\n",
            "subgraph Group[\"Process #38; stages\"]\n",
            "A[\"Alpha #60;tag #38; #quot;quoted#quot;#62;\"]\n",
            "B(\"Rounded\")\n",
            "C{\"Choice\"}\n",
            "end\n",
            "A -->|go| B\n",
            "A --> B\n",
            "B --> C\n",
            "C --> C\n",
        )
    }

    #[test]
    fn diagram_export_image_snapshot_is_current_revision_bound_and_raw_source_is_exact() {
        let mut session = fresh_session("flowchart TD\nA[\"Alpha\"]\nB[\"Beta\"]\nA --> B\n");
        let snapshot = DiagramImageSnapshot::capture(&session, SceneMetrics::default()).unwrap();
        let original_revision = session.revision();
        let first = snapshot.encode_svg(1.0).unwrap();
        assert_eq!(first.revision(), original_revision);
        assert_eq!(first.format(), DiagramExportFormat::Svg);
        assert!(
            std::str::from_utf8(first.bytes())
                .unwrap()
                .contains("Alpha")
        );

        session
            .replace_source("flowchart TD\nA[\"Changed\"]\n")
            .unwrap();
        let again = snapshot.encode_svg(1.0).unwrap();
        assert_eq!(again.revision(), original_revision);
        assert_eq!(again.bytes(), first.bytes());

        session.replace_source("flowchart TD\nA -->\n").unwrap();
        assert!(matches!(
            DiagramImageSnapshot::capture(&session, SceneMetrics::default()),
            Err(DiagramExportError::PreviewUnavailable(
                crate::diagram::PreviewState::Stale
            ))
        ));
        let exact_source = session.draft().to_owned();
        let source_snapshot = DiagramSourceSnapshot::capture(&session);
        let source_artifact = source_snapshot.encode();
        assert_eq!(source_snapshot.source(), exact_source.as_str());
        assert_eq!(source_artifact.bytes(), exact_source.as_bytes());
        assert_eq!(source_artifact.revision(), session.revision());

        let raw_invalid = "\u{feff}flowchart TD\r\nA -- unsupported\r\n";
        let no_preview = fresh_session(raw_invalid);
        assert!(matches!(
            DiagramImageSnapshot::capture(&no_preview, SceneMetrics::default()),
            Err(DiagramExportError::PreviewUnavailable(
                crate::diagram::PreviewState::NoPreview
            ))
        ));
        assert_eq!(
            DiagramSourceSnapshot::capture(&no_preview).source(),
            raw_invalid
        );
    }

    #[test]
    fn diagram_export_blank_current_graph_produces_valid_small_svg_and_png() {
        let session = DiagramSession::new();
        let snapshot = DiagramImageSnapshot::capture(&session, SceneMetrics::default()).unwrap();
        let svg = snapshot.encode_svg(1.0).unwrap();
        let png = snapshot.encode_png(1.0).unwrap();
        assert_eq!(svg.dimensions(), Some((48, 48)));
        assert_eq!(png.dimensions(), Some((48, 48)));
        assert!(std::str::from_utf8(svg.bytes()).unwrap().contains("<svg "));
        let decoded = image::load_from_memory(png.bytes()).unwrap().to_rgba8();
        assert!(
            decoded
                .pixels()
                .all(|pixel| pixel.0 == [249, 250, 251, 255])
        );
    }

    #[test]
    fn diagram_export_svg_uses_shared_shapes_labels_and_escapes_hostile_text() {
        let session = fresh_session(scene_source());
        let snapshot = DiagramImageSnapshot::capture(&session, SceneMetrics::default()).unwrap();
        let artifact = snapshot.encode_svg(1.25).unwrap();
        let dimensions = artifact.dimensions().unwrap();
        assert_eq!(
            dimensions,
            (svg_width(artifact.bytes()), svg_height(artifact.bytes()))
        );
        assert!(artifact.bytes().starts_with(b"<?xml version=\"1.0\""));
        let svg = std::str::from_utf8(artifact.bytes()).unwrap();
        assert!(svg.contains("<path d=\"M "));
        assert!(svg.contains("<polygon points=\""));
        assert!(svg.contains("rx=\""));
        assert!(svg.contains("Alpha &lt;tag &amp; &quot;quoted&quot;&gt;"));
        assert!(svg.contains("Process &amp; stages"));
        assert!(!svg.contains("<script"));
        assert!(!svg.contains("foreignObject"));
        assert!(!svg.contains("href="));
        assert!(!is_xml_character('\u{1}'));
        assert!(is_xml_character('\u{1f600}'));
    }

    #[test]
    fn diagram_export_svg_rejects_xml_control_characters_in_graph_labels() {
        use crate::diagram::{
            DiagramDocument, DiagramNode, NodeId, NodeShape, Position, build_scene,
        };

        let document = DiagramDocument {
            direction: FlowDirection::TopDown,
            nodes: vec![DiagramNode {
                id: NodeId::new("A").unwrap(),
                label: "bad\u{1}label".to_owned(),
                shape: NodeShape::Rectangle,
                position: Position::default(),
                group: None,
            }],
            ..DiagramDocument::default()
        };
        let scene = build_scene(&document, SceneMetrics::default()).unwrap();
        let snapshot = DiagramImageSnapshot { revision: 3, scene };
        assert!(matches!(
            snapshot.encode_svg(1.0),
            Err(DiagramExportError::InvalidXmlControl('\u{1}'))
        ));
    }

    #[test]
    fn diagram_export_png_decodes_with_shapes_and_bundled_unicode_fallback() {
        let session = fresh_session(
            "flowchart TD\nA[\"Glyphs 😀 \u{10ffff}\"]\nB(\"Round\")\nC{\"Choice\"}\nA --> B\nB --> C\n",
        );
        let snapshot = DiagramImageSnapshot::capture(&session, SceneMetrics::default()).unwrap();
        let fonts = super::default_font_set().unwrap();
        let (emoji_face, emoji_character, emoji_substituted) = fonts.choose('😀');
        assert_eq!(emoji_character, '😀');
        assert!(!emoji_substituted);
        assert!(
            emoji_face > 0,
            "the bundled emoji fallback face should be used"
        );
        let (replacement_face, replacement, substituted) = fonts.choose('\u{10ffff}');
        assert!(substituted);
        assert!(matches!(replacement, '□' | '?'));

        let transform = super::ExportTransform::new(snapshot.scene.bounds, 1.0).unwrap();
        let text_plan = super::prepare_raster_text(&snapshot.scene, transform, fonts).unwrap();
        let mut placements = text_plan.nodes[0].lines.iter().flatten();
        let emoji_placement = placements
            .clone()
            .find(|placement| {
                placement.character == emoji_character && placement.face_index == emoji_face
            })
            .unwrap();
        let replacement_placement = placements
            .find(|placement| {
                placement.character == replacement && placement.face_index == replacement_face
            })
            .unwrap();
        assert!(emoji_placement.bounds.is_some());
        assert!(replacement_placement.bounds.is_some());

        let artifact = snapshot.encode_png(1.0).unwrap();
        let svg = snapshot.encode_svg(1.0).unwrap();
        assert_eq!(artifact.format(), DiagramExportFormat::Png);
        assert_eq!(artifact.dimensions(), svg.dimensions());
        assert!(artifact.dimensions().unwrap().0 > 48);
        assert!(artifact.bytes().len() < MAX_PNG_BYTES);
        let decoded = image::load_from_memory(artifact.bytes())
            .unwrap()
            .to_rgba8();
        assert_eq!(
            artifact.dimensions(),
            Some((decoded.width(), decoded.height()))
        );
        assert!(
            decoded
                .pixels()
                .any(|pixel| pixel.0 != [249, 250, 251, 255])
        );
        assert!(artifact.warnings().glyph_substitutions() > 0);
        assert!(artifact.warnings().glyph_substitutions() < MAX_PREPARED_GLYPHS as u32);
        let label_bounds = transform.rect(snapshot.scene.nodes[0].label_bounds);
        assert!(dark_ink_bounds_in(&decoded, label_bounds).is_some());
        for placement in [emoji_placement, replacement_placement] {
            let glyph_bounds = placement.bounds.unwrap();
            let visible_bounds = super::PixelRect {
                min_x: glyph_bounds.min_x.max(placement.clip.min_x),
                min_y: glyph_bounds.min_y.max(placement.clip.min_y),
                max_x: glyph_bounds.max_x.min(placement.clip.max_x),
                max_y: glyph_bounds.max_y.min(placement.clip.max_y),
            };
            let left = visible_bounds
                .min_x
                .floor()
                .clamp(0.0, f64::from(decoded.width())) as u32;
            let top = visible_bounds
                .min_y
                .floor()
                .clamp(0.0, f64::from(decoded.height())) as u32;
            let right = visible_bounds
                .max_x
                .ceil()
                .clamp(0.0, f64::from(decoded.width())) as u32;
            let bottom = visible_bounds
                .max_y
                .ceil()
                .clamp(0.0, f64::from(decoded.height())) as u32;
            assert!(visible_bounds.width() > 0.0 && visible_bounds.height() > 0.0);
            let has_contrasting_pixel = (top..bottom).any(|y| {
                (left..right).any(|x| {
                    let pixel = decoded.get_pixel(x, y);
                    (0..3).all(|channel| u8::MAX - pixel[channel] >= 64)
                })
            });
            assert!(
                has_contrasting_pixel,
                "prepared glyph {:?} from face {} has no pixel with at least 64 levels of contrast against its white fill",
                placement.character, placement.face_index
            );
        }
    }

    #[test]
    fn diagram_export_png_preserves_tab_spacing_without_drawing_ink() {
        use crate::diagram::{
            DiagramDocument, DiagramNode, NodeId, NodeShape, SceneTextMetrics, build_scene,
        };

        let metrics = SceneMetrics {
            text: SceneTextMetrics {
                max_scalar_advance: 32.0,
                line_height: 32.0,
                wrap_width: 256.0,
            },
            ..SceneMetrics::default()
        };
        let document = DiagramDocument {
            nodes: vec![DiagramNode {
                id: NodeId::new("Tab").unwrap(),
                label: "A\tB".to_owned(),
                shape: NodeShape::Rectangle,
                position: Position::default(),
                group: None,
            }],
            ..DiagramDocument::default()
        };
        let snapshot = DiagramImageSnapshot {
            revision: 1,
            scene: build_scene(&document, metrics).unwrap(),
        };
        let transform = super::ExportTransform::new(snapshot.scene.bounds, 1.0).unwrap();
        let fonts = super::default_font_set().unwrap();
        let plan = super::prepare_raster_text(&snapshot.scene, transform, fonts).unwrap();
        let placements = &plan.nodes[0].lines[0];
        let tab_index = placements
            .iter()
            .position(|placement| placement.character == '\t')
            .unwrap();
        let tab = &placements[tab_index];
        let next = &placements[tab_index + 1];
        assert!(tab.bounds.is_none());
        assert!(next.position_x > tab.position_x);
        assert!(
            (f64::from(next.position_x - tab.position_x) - super::glyph_advance(tab, fonts)).abs()
                < 0.01
        );

        let png = snapshot.encode_png_with_fonts(1.0, fonts).unwrap();
        let decoded = image::load_from_memory(png.bytes()).unwrap().to_rgba8();
        let tab_region = super::PixelRect {
            min_x: f64::from(tab.position_x) + 1.0,
            min_y: tab.clip.min_y,
            max_x: f64::from(next.position_x) - 1.0,
            max_y: tab.clip.max_y,
        };
        assert!(tab_region.width() > 2.0);
        assert_eq!(dark_ink_bounds_in(&decoded, tab_region), None);

        let measured_ink = placements
            .iter()
            .filter_map(|placement| placement.bounds)
            .reduce(|left, right| super::PixelRect {
                min_x: left.min_x.min(right.min_x),
                min_y: left.min_y.min(right.min_y),
                max_x: left.max_x.max(right.max_x),
                max_y: left.max_y.max(right.max_y),
            })
            .unwrap();
        let rendered_ink = dark_ink_bounds_in(
            &decoded,
            transform.rect(snapshot.scene.nodes[0].label_bounds),
        )
        .unwrap();
        assert!(f64::from(rendered_ink.0) >= measured_ink.min_x.floor());
        assert!(f64::from(rendered_ink.1) >= measured_ink.min_y.floor());
        assert!(f64::from(rendered_ink.2) < measured_ink.max_x.ceil());
        assert!(f64::from(rendered_ink.3) < measured_ink.max_y.ceil());
    }

    #[test]
    fn diagram_export_png_honors_bundled_font_scale_and_vertical_tweak() {
        let session = fresh_session("flowchart TD\nA[\"tweak\"]\n");
        let snapshot = DiagramImageSnapshot::capture(&session, SceneMetrics::default()).unwrap();
        let baseline = snapshot.encode_png(2.0).unwrap();

        let mut definitions = FontDefinitions::default();
        let data = Arc::make_mut(definitions.font_data.get_mut("Ubuntu-Light").unwrap());
        data.tweak = FontTweak {
            scale: 1.2,
            y_offset_factor: 0.35,
            y_offset: 2.0,
            ..FontTweak::default()
        };
        let tweaked_fonts = FontSet::from_definitions(&definitions).unwrap();
        let tweaked = snapshot.encode_png_with_fonts(2.0, &tweaked_fonts).unwrap();
        let advance_probe = super::GlyphPlacement {
            face_index: 0,
            character: 'A',
            font_size: 16.0,
            position_x: 0.0,
            position_y: 0.0,
            bounds: None,
            clip: super::PixelRect {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 0.0,
                max_y: 0.0,
            },
        };
        assert_eq!(
            super::glyph_advance(&advance_probe, super::default_font_set().unwrap()),
            super::glyph_advance(&advance_probe, &tweaked_fonts)
        );

        let mut zero_offset_definitions = FontDefinitions::default();
        let data = Arc::make_mut(
            zero_offset_definitions
                .font_data
                .get_mut("Ubuntu-Light")
                .unwrap(),
        );
        data.tweak.scale = 1.2;
        let zero_offset_fonts = FontSet::from_definitions(&zero_offset_definitions).unwrap();
        let zero_offset = snapshot
            .encode_png_with_fonts(2.0, &zero_offset_fonts)
            .unwrap();
        let test_tweak = FontTweak {
            scale: 1.2,
            y_offset_factor: 0.35,
            y_offset: 2.0,
            ..FontTweak::default()
        };
        let expected_offset = f64::from(16.0f32 * test_tweak.scale * test_tweak.y_offset_factor)
            + f64::from(test_tweak.y_offset) * 2.0;
        assert!(
            (super::font_vertical_offset_pixels(16.0, &test_tweak, 2.0) - expected_offset).abs()
                < 0.0001
        );
        assert_ne!(baseline.bytes(), tweaked.bytes());
        let baseline_image = image::load_from_memory(baseline.bytes())
            .unwrap()
            .to_rgba8();
        let tweaked_image = image::load_from_memory(tweaked.bytes()).unwrap().to_rgba8();
        let zero_offset_image = image::load_from_memory(zero_offset.bytes())
            .unwrap()
            .to_rgba8();
        let transform = super::ExportTransform::new(snapshot.scene.bounds, 2.0).unwrap();
        let clip = transform.rect(snapshot.scene.nodes[0].label_bounds);
        let baseline_ink = dark_ink_bounds_in(&baseline_image, clip).unwrap();
        let tweaked_ink = dark_ink_bounds_in(&tweaked_image, clip).unwrap();
        let zero_offset_ink = dark_ink_bounds_in(&zero_offset_image, clip).unwrap();
        assert_ne!(baseline_ink, tweaked_ink);
        assert!(tweaked_ink.1 > zero_offset_ink.1);
    }

    #[test]
    fn diagram_export_limits_reject_scale_dimensions_work_and_encoded_growth() {
        let snapshot = DiagramImageSnapshot::capture(
            &fresh_session("flowchart TD\nA[\"small\"]\n"),
            SceneMetrics::default(),
        )
        .unwrap();
        assert!(matches!(
            snapshot.encode_svg(0.0),
            Err(DiagramExportError::InvalidScale)
        ));
        assert!(matches!(
            snapshot.encode_png(f64::NAN),
            Err(DiagramExportError::InvalidScale)
        ));
        assert!(matches!(
            snapshot.encode_svg(1_000.0),
            Err(DiagramExportError::OutputTooLarge {
                max_side,
                max_pixels,
                ..
            }) if max_side == MAX_EXPORT_SIDE && max_pixels == MAX_EXPORT_PIXELS
        ));

        let mut source = String::from("flowchart TD\n");
        for index in 0..9 {
            source.push_str(&format!("N{index}[\"x\"]\n"));
        }
        let mut overlapping = fresh_session(&source);
        let positions = overlapping
            .current_graph()
            .unwrap()
            .nodes
            .iter()
            .map(|node| NodePosition {
                id: node.id.clone(),
                position: Position::default(),
            })
            .collect();
        overlapping
            .apply_transaction(&[GraphOperation::MoveNodes { positions }])
            .unwrap();
        let metrics = SceneMetrics {
            minimum_node_width: 4_000.0,
            minimum_node_height: 4_000.0,
            ..SceneMetrics::default()
        };
        let overlapping = DiagramImageSnapshot::capture(&overlapping, metrics).unwrap();
        assert!(matches!(
            overlapping.encode_png(1.0),
            Err(DiagramExportError::RasterWorkLimit { max, .. })
                if max == MAX_RASTER_PIXEL_VISITS
        ));

        let mut svg = BoundedString::new(8);
        assert!(svg.push_str("12345678").is_ok());
        assert!(matches!(
            svg.push_str("9"),
            Err(DiagramExportError::EncodedSizeLimit {
                format: DiagramExportFormat::Svg,
                max_bytes: 8
            })
        ));
        let mut png = BoundedBytes::new(4);
        assert!(png.write_all(b"1234").is_ok());
        assert!(png.write_all(b"5").is_err());
        assert!(png.exceeded);
    }

    #[test]
    fn diagram_export_glyph_limit_is_checked_before_text_preparation() {
        assert_eq!(
            super::add_glyph_count(MAX_PREPARED_GLYPHS - 1, 1).unwrap(),
            MAX_PREPARED_GLYPHS
        );
        assert!(matches!(
            super::add_glyph_count(MAX_PREPARED_GLYPHS, 1),
            Err(DiagramExportError::TooManyGlyphs { max_glyphs })
                if max_glyphs == MAX_PREPARED_GLYPHS
        ));
        assert!(matches!(
            super::add_glyph_count(usize::MAX, 1),
            Err(DiagramExportError::TooManyGlyphs { .. })
        ));
    }

    #[test]
    fn diagram_export_publication_is_unoccupied_atomic_and_leaves_document_state_unchanged() {
        let mut controller = DiagramDocumentController::new();
        controller
            .replace_source("flowchart TD\nA[\"publish\"]\n")
            .unwrap();
        let session = controller.session().unwrap();
        let revision = session.revision();
        let source = session.draft().to_owned();
        let dirty = session.is_dirty();
        let undo = session.undo_depth();
        let artifact = DiagramImageSnapshot::capture(session, SceneMetrics::default())
            .unwrap()
            .encode_svg(1.0)
            .unwrap();

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("diagram.svg");
        artifact.publish_unoccupied(&path).unwrap();
        let published = fs::read(&path).unwrap();
        assert_eq!(published, artifact.bytes());

        let mut writer_called = false;
        let occupied = artifact.publish_unoccupied_with_writer(&path, |_path, _bytes| {
            writer_called = true;
            anyhow::bail!("writer should not run")
        });
        assert!(matches!(
            occupied,
            Err(DiagramExportPublicationError::OccupiedPath(_))
        ));
        assert!(!writer_called);
        assert_eq!(fs::read(&path).unwrap(), published);

        let failure_path = directory.path().join("failure.svg");
        assert!(matches!(
            artifact.publish_unoccupied_with_writer(&failure_path, |_path, _bytes| {
                anyhow::bail!("injected writer failure")
            }),
            Err(DiagramExportPublicationError::AtomicWrite { .. })
        ));
        assert!(!failure_path.exists());
        assert!(matches!(
            artifact.publish_unoccupied_with_writer(
                &directory.path().join("wrong.png"),
                |_path, _bytes| Ok(())
            ),
            Err(DiagramExportPublicationError::WrongExtension {
                expected: "svg",
                ..
            })
        ));

        let session = controller.session().unwrap();
        assert_eq!(session.revision(), revision);
        assert_eq!(session.draft(), source.as_str());
        assert_eq!(session.is_dirty(), dirty);
        assert_eq!(session.undo_depth(), undo);
        assert!(controller.path().is_none());
    }

    fn svg_width(bytes: &[u8]) -> u32 {
        std::str::from_utf8(bytes)
            .unwrap()
            .split("width=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }

    fn svg_height(bytes: &[u8]) -> u32 {
        std::str::from_utf8(bytes)
            .unwrap()
            .split("height=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }

    fn dark_ink_bounds_in(
        image: &RgbaImage,
        bounds: super::PixelRect,
    ) -> Option<(u32, u32, u32, u32)> {
        let left = bounds.min_x.floor().clamp(0.0, f64::from(image.width())) as u32;
        let top = bounds.min_y.floor().clamp(0.0, f64::from(image.height())) as u32;
        let right = bounds.max_x.ceil().clamp(0.0, f64::from(image.width())) as u32;
        let bottom = bounds.max_y.ceil().clamp(0.0, f64::from(image.height())) as u32;
        let mut min_x = right;
        let mut min_y = bottom;
        let mut max_x = left;
        let mut max_y = top;
        let mut found = false;
        for y in top..bottom {
            for x in left..right {
                let pixel = image.get_pixel(x, y);
                if pixel[0] < 60 && pixel[1] < 75 && pixel[2] < 95 {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                    found = true;
                }
            }
        }
        found.then_some((min_x, min_y, max_x, max_y))
    }
}
