//! Bounded, current-only Mermaid, SVG, and PNG export from the shared Diagram scene.

use super::assets::decode_visual_asset_payload;
use super::model::NodeShape;
use super::persistence::DiagramExportPublicationError;
use super::rendering::{DiagramRenderOutput, RasterSize, rasterize_output_svg, unpremultiply_rgba};
use super::scene::{
    ArrowHead, CubicBezier, DiagramScene, PreparedText, PreparedTextLine, SceneMetrics, SceneNode,
    ScenePoint, SceneRect, TextHorizontalAlignment, build_scene,
};
use super::session::{DiagramSession, PreviewState};
use super::visual::{
    MAX_VISUAL_ASSET_DIMENSION, VisualAsset, VisualAssetFormat, VisualAssetId, VisualColor,
    VisualLineStyle, VisualPrimitiveShape, VisualShadow,
};
use super::visual_scene::{
    MAX_VISUAL_OUTLINE_POINTS, MAX_VISUAL_PROJECTED_ITEMS, MAX_VISUAL_PROJECTION_COORDINATE_ABS,
    MAX_VISUAL_TEXT_LINES_PER_SCENE, ProjectedStrokePrimitive, ProjectedVisualItem,
    VisualProjectedKind, VisualSceneProjection, color_with_opacity,
};
use ab_glyph::{Font, FontArc, FontRef, FontVec, OutlineCurve, ScaleFont, point};
use base64::Engine as _;
use eframe::egui::{FontDefinitions, FontFamily, FontTweak};
use image::{ImageEncoder, Rgba, RgbaImage};
use resvg::{tiny_skia, usvg};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, OnceLock};

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
const MAX_VISUAL_EXPORT_DECODED_PIXELS: u64 = MAX_EXPORT_PIXELS / 2;
const MIN_VISUAL_EXPORT_SCALE: f64 = 0.025;
const MAX_VISUAL_EXPORT_SCALE: f64 = 8.0;

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
    visual_asset_placeholders: u32,
    visual_truncated_text_items: u32,
    visual_simplified_dash_paths: u32,
    visual_connector_route_fallbacks: u32,
}

impl DiagramExportWarnings {
    pub const fn glyph_substitutions(self) -> u32 {
        self.glyph_substitutions
    }

    pub const fn visual_asset_placeholders(self) -> u32 {
        self.visual_asset_placeholders
    }

    pub const fn visual_truncated_text_items(self) -> u32 {
        self.visual_truncated_text_items
    }

    pub const fn visual_simplified_dash_paths(self) -> u32 {
        self.visual_simplified_dash_paths
    }

    pub const fn visual_connector_route_fallbacks(self) -> u32 {
        self.visual_connector_route_fallbacks
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

/// Bounds and background for encoding a committed visual projection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiagramVisualExportOptions {
    /// Export scale in the supported 0.025..=8 range.
    pub scale: f64,
    /// `None` preserves transparent output.
    pub background: Option<VisualColor>,
}

impl Default for DiagramVisualExportOptions {
    fn default() -> Self {
        Self {
            scale: 1.0,
            background: None,
        }
    }
}

/// Immutable encoder input tied to one committed visual revision.
#[derive(Clone, Debug)]
pub struct DiagramVisualSnapshot {
    revision: u64,
    projection: Arc<VisualSceneProjection>,
}

impl DiagramVisualSnapshot {
    pub fn capture(revision: u64, projection: Arc<VisualSceneProjection>) -> Self {
        Self {
            revision,
            projection,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn projection(&self) -> &Arc<VisualSceneProjection> {
        &self.projection
    }

    pub fn encode_svg(
        &self,
        options: DiagramVisualExportOptions,
    ) -> Result<DiagramExportArtifact, DiagramExportError> {
        encode_visual_svg(self, options)
    }

    pub fn encode_png(
        &self,
        options: DiagramVisualExportOptions,
    ) -> Result<DiagramExportArtifact, DiagramExportError> {
        encode_visual_png(self, options, default_font_sets()?)
    }
}

/// Immutable generic-render result captured for image export. It contains the
/// exact adapter output that was accepted for the displayed revision; PNG
/// dimensions are rasterized from its cached SVG without reparsing Mermaid.
#[derive(Clone, Debug)]
pub struct DiagramRenderedImageSnapshot {
    revision: u64,
    output: std::sync::Arc<DiagramRenderOutput>,
}

impl DiagramRenderedImageSnapshot {
    pub fn capture(revision: u64, output: std::sync::Arc<DiagramRenderOutput>) -> Self {
        Self { revision, output }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn encode_svg(&self, scale: f64) -> Result<DiagramExportArtifact, DiagramExportError> {
        let (width, height, _) =
            scaled_render_dimensions(self.output.natural_width, self.output.natural_height, scale)?;
        let bytes = scale_svg_dimensions(&self.output.svg, width, height)?;
        Ok(DiagramExportArtifact {
            revision: self.revision,
            format: DiagramExportFormat::Svg,
            bytes,
            dimensions: Some((width, height)),
            warnings: DiagramExportWarnings::default(),
        })
    }

    pub fn encode_png(&self, scale: f64) -> Result<DiagramExportArtifact, DiagramExportError> {
        let (width, height, raster_size) =
            scaled_render_dimensions(self.output.natural_width, self.output.natural_height, scale)?;
        let raster = rasterize_output_svg(&self.output, raster_size)
            .map_err(|error| DiagramExportError::Encode(error.to_string()))?;
        let mut writer = BoundedBytes::new(MAX_PNG_BYTES);
        let result = image::codecs::png::PngEncoder::new(&mut writer).write_image(
            &raster.rgba,
            width,
            height,
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
            revision: self.revision,
            format: DiagramExportFormat::Png,
            bytes: writer.bytes,
            dimensions: Some((width, height)),
            warnings: DiagramExportWarnings::default(),
        })
    }
}

fn scaled_render_dimensions(
    natural_width: f32,
    natural_height: f32,
    scale: f64,
) -> Result<(u32, u32, RasterSize), DiagramExportError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(DiagramExportError::InvalidScale);
    }
    let width = (f64::from(natural_width) * scale).ceil();
    let height = (f64::from(natural_height) * scale).ceil();
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
    let width = width as u32;
    let height = height as u32;
    let raster_size =
        RasterSize::new(width, height).map_err(|_| DiagramExportError::OutputTooLarge {
            width,
            height,
            max_side: MAX_EXPORT_SIDE,
            max_pixels: MAX_EXPORT_PIXELS,
        })?;
    Ok((width, height, raster_size))
}

fn scale_svg_dimensions(svg: &str, width: u32, height: u32) -> Result<Vec<u8>, DiagramExportError> {
    if svg.len() > MAX_SVG_BYTES {
        return Err(DiagramExportError::EncodedSizeLimit {
            format: DiagramExportFormat::Svg,
            max_bytes: MAX_SVG_BYTES,
        });
    }
    let root_start = svg.find("<svg").ok_or_else(|| {
        DiagramExportError::Encode("renderer output has no SVG root element".to_owned())
    })?;
    let root_end = svg[root_start..]
        .find('>')
        .map(|index| root_start + index + 1)
        .ok_or_else(|| DiagramExportError::Encode("renderer SVG root is incomplete".to_owned()))?;
    let mut root = svg[root_start..root_end].to_owned();
    set_svg_root_attribute(&mut root, "width", &width.to_string());
    set_svg_root_attribute(&mut root, "height", &height.to_string());

    let extra = root.len().saturating_sub(root_end - root_start);
    if svg.len().saturating_add(extra) > MAX_SVG_BYTES {
        return Err(DiagramExportError::EncodedSizeLimit {
            format: DiagramExportFormat::Svg,
            max_bytes: MAX_SVG_BYTES,
        });
    }
    let mut scaled = String::new();
    scaled
        .try_reserve_exact(svg.len().saturating_add(extra))
        .map_err(|_| DiagramExportError::ImageAllocation)?;
    scaled.push_str(&svg[..root_start]);
    scaled.push_str(&root);
    scaled.push_str(&svg[root_end..]);
    Ok(scaled.into_bytes())
}

fn set_svg_root_attribute(root: &mut String, name: &str, value: &str) {
    let marker = format!(" {name}=\"");
    if let Some(start) = root.find(&marker) {
        let value_start = start + marker.len();
        if let Some(end_offset) = root[value_start..].find('"') {
            let value_end = value_start + end_offset;
            root.replace_range(value_start..value_end, value);
            return;
        }
    }
    let insert_at = root.len().saturating_sub(1);
    root.insert_str(insert_at, &format!(" {name}=\"{value}\""));
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
            ..DiagramExportWarnings::default()
        },
    })
}

#[derive(Debug)]
pub enum DiagramExportError {
    PreviewUnavailable(PreviewState),
    Scene(super::scene::SceneError),
    InvalidScale,
    VisualScaleOutOfRange,
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
    VisualAssetDecodeLimit {
        estimated_pixels: u64,
        max_pixels: u64,
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
            Self::VisualScaleOutOfRange => formatter.write_str(
                "visual export scale must be finite and within the supported 0.025..=8 range",
            ),
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
            Self::VisualAssetDecodeLimit {
                estimated_pixels,
                max_pixels,
            } => write!(
                formatter,
                "visual export requires decoding {estimated_pixels} embedded image pixels; the limit is {max_pixels}"
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

const MAX_VISUAL_EXPORT_VECTOR_OPS: usize = MAX_VISUAL_PROJECTED_ITEMS.saturating_mul(4);

enum VisualExportOp {
    Vector {
        fragment: String,
    },
    Image {
        asset_id: VisualAssetId,
        placement: VisualImagePlacement,
        bounds: SceneRect,
        opacity: f64,
    },
}

#[derive(Clone, Copy)]
struct VisualImagePlacement {
    /// SVG/tiny-skia affine coefficients: x'=a*x+c*y+e, y'=b*x+d*y+f.
    matrix: [f64; 6],
}

struct VisualExportAsset {
    asset: VisualAsset,
    pixmap: Option<Arc<tiny_skia::Pixmap>>,
}

struct VisualExportPlan {
    transform: ExportTransform,
    ops: Vec<VisualExportOp>,
    assets: HashMap<VisualAssetId, VisualExportAsset>,
    asset_order: Vec<VisualAssetId>,
    asset_indices: HashMap<VisualAssetId, usize>,
    warnings: DiagramExportWarnings,
    raster_work: u64,
    vector_bytes: usize,
    vector_work: u64,
    glyph_count: usize,
}

#[derive(Clone, Copy)]
enum VisualFragmentMode {
    Full,
    ImageBorder,
    ImagePlaceholder,
}

fn build_visual_export_plan(
    snapshot: &DiagramVisualSnapshot,
    options: DiagramVisualExportOptions,
    fonts: (&FontSet, &FontSet),
) -> Result<VisualExportPlan, DiagramExportError> {
    if !options.scale.is_finite()
        || !(MIN_VISUAL_EXPORT_SCALE..=MAX_VISUAL_EXPORT_SCALE).contains(&options.scale)
    {
        return Err(DiagramExportError::VisualScaleOutOfRange);
    }
    let projection = snapshot.projection.as_ref();
    if projection.items.len() > MAX_VISUAL_PROJECTED_ITEMS {
        return Err(DiagramExportError::Encode(
            "visual projection exceeds the export item limit".into(),
        ));
    }

    let mut bounds = None;
    let mut total_lines = 0usize;
    for item in &projection.items {
        validate_visual_export_item(item)?;
        bounds = Some(bounds.map_or(item.bounds, |current: SceneRect| {
            union_scene_rect(current, item.bounds)
        }));
        if let Some(text) = &item.text {
            total_lines = total_lines.saturating_add(text.lines.len());
            if total_lines > MAX_VISUAL_TEXT_LINES_PER_SCENE {
                return Err(DiagramExportError::Encode(
                    "visual projection exceeds its prepared text line limit".into(),
                ));
            }
        }
    }
    let transform = ExportTransform::new(bounds, options.scale)?;
    let mut plan = VisualExportPlan {
        transform,
        ops: Vec::with_capacity(projection.items.len().saturating_mul(2)),
        assets: HashMap::new(),
        asset_order: Vec::new(),
        asset_indices: HashMap::new(),
        warnings: DiagramExportWarnings::default(),
        raster_work: 0,
        vector_bytes: 0,
        vector_work: 0,
        glyph_count: 0,
    };
    let mut decoded_pixels = 0u64;

    for item in &projection.items {
        plan.warnings.visual_truncated_text_items = plan
            .warnings
            .visual_truncated_text_items
            .saturating_add(u32::from(item.text_truncated));
        plan.warnings.visual_simplified_dash_paths = plan
            .warnings
            .visual_simplified_dash_paths
            .saturating_add(u32::from(item.dash_simplified));
        plan.warnings.visual_connector_route_fallbacks = plan
            .warnings
            .visual_connector_route_fallbacks
            .saturating_add(u32::from(item.route_fallback));

        if item.kind == VisualProjectedKind::MissingImagePlaceholder {
            plan.warnings.visual_asset_placeholders =
                plan.warnings.visual_asset_placeholders.saturating_add(1);
        }

        if item.kind == VisualProjectedKind::Image {
            if item.outline.len() != 4 {
                return Err(DiagramExportError::Encode(
                    "visual image reference requires a four-corner projected outline".into(),
                ));
            }
            let usable_asset = if let Some(asset) = &item.image_asset {
                ensure_visual_export_asset(&mut plan, asset, &mut decoded_pixels)?
            } else {
                false
            };
            if usable_asset {
                push_item_shadow(&mut plan, item, fonts)?;
                let asset = item.image_asset.as_ref().ok_or_else(|| {
                    DiagramExportError::Encode("visual image reference lost its asset".into())
                })?;
                let placement = image_placement(asset, &item.outline)?;
                push_visual_image(
                    &mut plan,
                    asset.id().clone(),
                    placement,
                    item.bounds,
                    item.style.opacity,
                )?;
                push_item_image_border(&mut plan, item, fonts)?;
            } else {
                plan.warnings.visual_asset_placeholders =
                    plan.warnings.visual_asset_placeholders.saturating_add(1);
                push_item_shadow(&mut plan, item, fonts)?;
                push_item_fragment(&mut plan, item, fonts, VisualFragmentMode::ImagePlaceholder)?;
            }
        } else {
            if item.kind != VisualProjectedKind::Connector {
                push_item_shadow(&mut plan, item, fonts)?;
            }
            push_item_fragment(&mut plan, item, fonts, VisualFragmentMode::Full)?;
        }
    }
    plan.raster_work = estimate_visual_raster_work(&plan);
    Ok(plan)
}

fn validate_visual_export_item(item: &ProjectedVisualItem) -> Result<(), DiagramExportError> {
    if !valid_export_rect(item.bounds)
        || !item.style.opacity.is_finite()
        || !(0.0..=1.0).contains(&item.style.opacity)
        || !item.style.border_width.is_finite()
        || item.style.border_width < 0.0
        || !item.style.font_size.is_finite()
        || item.style.font_size <= 0.0
        || !item.text_rotation_degrees.is_finite()
    {
        return Err(DiagramExportError::Encode(
            "visual projection contains invalid geometry or style values".into(),
        ));
    }
    if item
        .header
        .as_ref()
        .is_some_and(|header| header.len() > MAX_VISUAL_OUTLINE_POINTS)
    {
        return Err(DiagramExportError::Encode(
            "visual frame header exceeds its point limit".into(),
        ));
    }
    let points = item
        .outline
        .iter()
        .chain(item.header.iter().flatten())
        .chain(item.details.iter().flatten())
        .chain(item.open_paths.iter().flatten());
    if points.into_iter().any(|point| !valid_export_point(*point)) {
        return Err(DiagramExportError::Encode(
            "visual projection contains an invalid path coordinate".into(),
        ));
    }
    if let Some(shadow) = item.style.shadow
        && (!shadow.offset_x.is_finite()
            || !shadow.offset_y.is_finite()
            || !shadow.blur.is_finite()
            || shadow.blur < 0.0)
    {
        return Err(DiagramExportError::Encode(
            "visual projection contains an invalid shadow".into(),
        ));
    }
    if let Some(text) = &item.text
        && (!text.height.is_finite()
            || text.height < 0.0
            || !text.line_height.is_finite()
            || text.line_height < 0.0
            || !text.reserved_width.is_finite()
            || text.reserved_width < 0.0
            || text
                .lines
                .iter()
                .any(|line| !line.reserved_width.is_finite() || line.reserved_width < 0.0))
    {
        return Err(DiagramExportError::Encode(
            "visual projection contains invalid prepared text geometry".into(),
        ));
    }
    Ok(())
}

fn valid_export_point(point: ScenePoint) -> bool {
    point.x.is_finite()
        && point.y.is_finite()
        && point.x.abs() <= MAX_VISUAL_PROJECTION_COORDINATE_ABS
        && point.y.abs() <= MAX_VISUAL_PROJECTION_COORDINATE_ABS
}

fn valid_export_rect(rect: SceneRect) -> bool {
    valid_export_point(rect.min)
        && valid_export_point(rect.max)
        && rect.max.x >= rect.min.x
        && rect.max.y >= rect.min.y
}

fn union_scene_rect(left: SceneRect, right: SceneRect) -> SceneRect {
    SceneRect {
        min: ScenePoint {
            x: left.min.x.min(right.min.x),
            y: left.min.y.min(right.min.y),
        },
        max: ScenePoint {
            x: left.max.x.max(right.max.x),
            y: left.max.y.max(right.max.y),
        },
    }
}

fn ensure_visual_export_asset(
    plan: &mut VisualExportPlan,
    asset: &VisualAsset,
    decoded_pixels: &mut u64,
) -> Result<bool, DiagramExportError> {
    if let Some(existing) = plan.assets.get(asset.id()) {
        if existing.asset != *asset {
            return Err(DiagramExportError::Encode(
                "visual projection reuses an asset ID for different payloads".into(),
            ));
        }
        return Ok(existing.pixmap.is_some());
    }
    let (width, height) = asset.dimensions();
    let pixels = u64::from(width).checked_mul(u64::from(height)).ok_or(
        DiagramExportError::VisualAssetDecodeLimit {
            estimated_pixels: u64::MAX,
            max_pixels: MAX_VISUAL_EXPORT_DECODED_PIXELS,
        },
    )?;
    let next_pixels =
        decoded_pixels
            .checked_add(pixels)
            .ok_or(DiagramExportError::VisualAssetDecodeLimit {
                estimated_pixels: u64::MAX,
                max_pixels: MAX_VISUAL_EXPORT_DECODED_PIXELS,
            })?;
    if next_pixels > MAX_VISUAL_EXPORT_DECODED_PIXELS {
        return Err(DiagramExportError::VisualAssetDecodeLimit {
            estimated_pixels: next_pixels,
            max_pixels: MAX_VISUAL_EXPORT_DECODED_PIXELS,
        });
    }
    *decoded_pixels = next_pixels;

    let decoded = decode_visual_asset_payload(asset, MAX_VISUAL_ASSET_DIMENSION as usize);
    let pixmap = decoded.ok().and_then(|decoded| {
        if decoded.id != *asset.id() {
            return None;
        }
        let mut rgba = decoded.rgba.to_vec();
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let alpha = u16::from(pixel[3]);
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
            }
        }
        let size = tiny_skia::IntSize::from_wh(decoded.width, decoded.height)?;
        tiny_skia::Pixmap::from_vec(rgba, size).map(Arc::new)
    });
    let usable = pixmap.is_some();
    if usable {
        let index = plan.asset_order.len();
        plan.asset_order.push(asset.id().clone());
        plan.asset_indices.insert(asset.id().clone(), index);
    }
    plan.assets.insert(
        asset.id().clone(),
        VisualExportAsset {
            asset: asset.clone(),
            pixmap,
        },
    );
    Ok(usable)
}

fn push_item_shadow(
    plan: &mut VisualExportPlan,
    item: &ProjectedVisualItem,
    fonts: (&FontSet, &FontSet),
) -> Result<(), DiagramExportError> {
    let Some(shadow) = item.style.shadow else {
        return Ok(());
    };
    if item.kind == VisualProjectedKind::Connector {
        return Ok(());
    }
    let mut fragment = BoundedString::new(MAX_SVG_BYTES);
    if item.kind != VisualProjectedKind::Text {
        let opacity = item.style.opacity;
        write_visual_filled_path(
            &mut fragment,
            item.header.as_deref().unwrap_or(&item.outline),
            true,
            shadow.color,
            opacity,
            if item.header.is_some() {
                None
            } else {
                Some((shadow.color, opacity, shadow.blur * 2.0))
            },
            ScenePoint {
                x: shadow.offset_x,
                y: shadow.offset_y,
            },
        )?;
        if item.header.is_some() && shadow.blur > 0.0 {
            for (path, closed) in std::iter::once((&item.outline, true))
                .chain(item.details.iter().map(|path| (path, false)))
            {
                let points = path
                    .iter()
                    .map(|point| ScenePoint {
                        x: point.x + shadow.offset_x,
                        y: point.y + shadow.offset_y,
                    })
                    .collect::<Vec<_>>();
                write_visual_stroke_path(
                    &mut fragment,
                    &points,
                    closed,
                    shadow.color,
                    opacity,
                    shadow.blur * 2.0,
                    VisualLineStyle::Solid,
                    None,
                )?;
            }
        }
    }
    if let Some(text) = &item.text {
        write_visual_text(
            &mut fragment,
            item,
            text,
            fonts,
            shadow.color,
            item.style.opacity,
            0.0,
            ScenePoint {
                x: shadow.offset_x,
                y: shadow.offset_y,
            },
            false,
            &mut plan.warnings,
            &mut plan.glyph_count,
        )?;
    }
    push_visual_vector(plan, fragment.value, item.bounds, 0)
}

fn push_item_fragment(
    plan: &mut VisualExportPlan,
    item: &ProjectedVisualItem,
    fonts: (&FontSet, &FontSet),
    mode: VisualFragmentMode,
) -> Result<(), DiagramExportError> {
    let mut fragment = BoundedString::new(MAX_SVG_BYTES);
    let connector = item.kind == VisualProjectedKind::Connector;
    let is_text = item.kind == VisualProjectedKind::Text;
    let image_border = matches!(mode, VisualFragmentMode::ImageBorder);
    let placeholder = matches!(mode, VisualFragmentMode::ImagePlaceholder);
    let draw_fill = !connector
        && !is_text
        && !image_border
        && (item.kind != VisualProjectedKind::Image || placeholder);

    if draw_fill {
        write_visual_filled_path(
            &mut fragment,
            item.header.as_deref().unwrap_or(&item.outline),
            true,
            item.style.fill,
            item.style.opacity,
            None,
            ScenePoint::default(),
        )?;
    }

    if connector {
        let color = if item.route_fallback {
            VisualColor::rgba(196, 91, 42, item.style.border.alpha)
        } else {
            item.style.border
        };
        let dash_paths = item.dash_paths.as_deref();
        for (index, points) in item.open_paths.iter().enumerate() {
            write_visual_stroke_path(
                &mut fragment,
                points,
                false,
                color,
                item.style.opacity,
                item.style.border_width,
                effective_line_style(item),
                dash_paths
                    .and_then(|paths| paths.get(index))
                    .map(Vec::as_slice),
            )?;
        }
    } else {
        let draw_border =
            !is_text && (image_border || placeholder || item.kind != VisualProjectedKind::Image);
        if draw_border {
            write_visual_stroke_path(
                &mut fragment,
                &item.outline,
                true,
                item.style.border,
                item.style.opacity,
                item.style.border_width,
                effective_line_style(item),
                item.dash_paths
                    .as_deref()
                    .and_then(|paths| paths.first())
                    .map(Vec::as_slice),
            )?;
            let details_closed = matches!(
                item.kind,
                VisualProjectedKind::Primitive(VisualPrimitiveShape::Cylinder)
                    | VisualProjectedKind::PresetPrimitive(VisualPrimitiveShape::Cylinder)
            );
            for (index, detail) in item.details.iter().enumerate() {
                write_visual_stroke_path(
                    &mut fragment,
                    detail,
                    details_closed,
                    item.style.border,
                    item.style.opacity,
                    item.style.border_width,
                    effective_line_style(item),
                    item.dash_paths
                        .as_deref()
                        .and_then(|paths| paths.get(index + 1))
                        .map(Vec::as_slice),
                )?;
            }
            if placeholder {
                let cross = [
                    vec![item.outline[0], item.outline[2]],
                    vec![item.outline[1], item.outline[3]],
                ];
                for path in cross {
                    write_visual_stroke_path(
                        &mut fragment,
                        &path,
                        false,
                        item.style.border,
                        item.style.opacity,
                        item.style.border_width,
                        VisualLineStyle::Solid,
                        None,
                    )?;
                }
            }
        }
    }

    if let Some(text) = &item.text {
        write_visual_text(
            &mut fragment,
            item,
            text,
            fonts,
            item.style.border,
            item.style.opacity,
            0.0,
            ScenePoint::default(),
            true,
            &mut plan.warnings,
            &mut plan.glyph_count,
        )?;
    } else if placeholder {
        write_visual_placeholder_label(&mut fragment, item, fonts, plan)?;
    }

    let path_points = item
        .outline
        .len()
        .saturating_add(item.details.iter().map(Vec::len).sum::<usize>())
        .saturating_add(item.open_paths.iter().map(Vec::len).sum::<usize>());
    push_visual_vector(
        plan,
        fragment.value,
        item.bounds,
        u64::try_from(path_points)
            .unwrap_or(u64::MAX)
            .saturating_mul(8),
    )
}

fn push_item_image_border(
    plan: &mut VisualExportPlan,
    item: &ProjectedVisualItem,
    fonts: (&FontSet, &FontSet),
) -> Result<(), DiagramExportError> {
    push_item_fragment(plan, item, fonts, VisualFragmentMode::ImageBorder)
}

fn push_visual_image(
    plan: &mut VisualExportPlan,
    asset_id: VisualAssetId,
    placement: VisualImagePlacement,
    bounds: SceneRect,
    opacity: f64,
) -> Result<(), DiagramExportError> {
    if plan.ops.len() >= MAX_VISUAL_EXPORT_VECTOR_OPS {
        return Err(DiagramExportError::Encode(
            "visual export exceeds its ordered operation limit".into(),
        ));
    }
    if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
        return Err(DiagramExportError::Encode(
            "visual image opacity is invalid".into(),
        ));
    }
    plan.ops.push(VisualExportOp::Image {
        asset_id,
        placement,
        bounds,
        opacity,
    });
    Ok(())
}

fn push_visual_vector(
    plan: &mut VisualExportPlan,
    fragment: String,
    bounds: SceneRect,
    additional_work: u64,
) -> Result<(), DiagramExportError> {
    if fragment.is_empty() {
        return Ok(());
    }
    if plan.ops.len() >= MAX_VISUAL_EXPORT_VECTOR_OPS {
        return Err(DiagramExportError::Encode(
            "visual export exceeds its ordered operation limit".into(),
        ));
    }
    plan.vector_bytes = plan.vector_bytes.saturating_add(fragment.len());
    if plan.vector_bytes > MAX_SVG_BYTES {
        return Err(DiagramExportError::EncodedSizeLimit {
            format: DiagramExportFormat::Svg,
            max_bytes: MAX_SVG_BYTES,
        });
    }
    plan.vector_work = plan.vector_work.saturating_add(additional_work);
    let footprint = plan
        .transform
        .rect(bounds)
        .area_pixels()
        .unwrap_or(u64::MAX);
    plan.vector_work = plan.vector_work.saturating_add(footprint);
    plan.ops.push(VisualExportOp::Vector { fragment });
    Ok(())
}

fn effective_line_style(item: &ProjectedVisualItem) -> VisualLineStyle {
    if item.dash_simplified || item.dash_paths.is_none() {
        VisualLineStyle::Solid
    } else {
        item.style.line_style
    }
}

fn estimate_visual_raster_work(plan: &VisualExportPlan) -> u64 {
    let mut work = plan.transform.pixels;
    let mut in_vector_run = false;
    for operation in &plan.ops {
        match operation {
            VisualExportOp::Vector { .. } => {
                if !in_vector_run {
                    work = work.saturating_add(plan.transform.pixels);
                    in_vector_run = true;
                }
            }
            VisualExportOp::Image { bounds, .. } => {
                in_vector_run = false;
                let area = plan
                    .transform
                    .rect(*bounds)
                    .area_pixels()
                    .unwrap_or(u64::MAX);
                work = work.saturating_add(area);
            }
        }
    }
    work.saturating_add(plan.vector_work).saturating_add(
        u64::try_from(plan.glyph_count)
            .unwrap_or(u64::MAX)
            .saturating_mul(32),
    )
}

fn visual_view_box(transform: ExportTransform) -> (f64, f64, f64, f64) {
    let x = transform.scene_min.x - EXPORT_PADDING / transform.scale;
    let y = transform.scene_min.y - EXPORT_PADDING / transform.scale;
    let width = f64::from(transform.width) / transform.scale;
    let height = f64::from(transform.height) / transform.scale;
    (x, y, width, height)
}

fn write_visual_svg_root(
    output: &mut BoundedString,
    transform: ExportTransform,
    background: Option<VisualColor>,
    declaration: bool,
) -> Result<(), DiagramExportError> {
    let (view_x, view_y, view_width, view_height) = visual_view_box(transform);
    if declaration {
        output.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n")?;
    }
    svg_fmt(
        output,
        format_args!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" version=\"1.1\">\n",
            transform.width,
            transform.height,
            visual_svg_num(view_x),
            visual_svg_num(view_y),
            visual_svg_num(view_width),
            visual_svg_num(view_height),
        ),
    )?;
    if let Some(background) = background {
        let (rgb, alpha) = svg_color(background, 1.0);
        svg_fmt(
            output,
            format_args!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{rgb}\" fill-opacity=\"{}\"/>\n",
                visual_svg_num(view_x),
                visual_svg_num(view_y),
                visual_svg_num(view_width),
                visual_svg_num(view_height),
                visual_svg_num(alpha),
            ),
        )?;
    }
    Ok(())
}

fn write_visual_svg_asset_definitions(
    output: &mut BoundedString,
    plan: &VisualExportPlan,
) -> Result<(), DiagramExportError> {
    if plan.asset_order.is_empty() {
        return Ok(());
    }
    output.push_str("<defs>\n")?;
    for (index, asset_id) in plan.asset_order.iter().enumerate() {
        let visual_asset = plan.assets.get(asset_id).ok_or_else(|| {
            DiagramExportError::Encode("visual asset index is inconsistent".into())
        })?;
        let asset = &visual_asset.asset;
        let mime = match asset.format() {
            VisualAssetFormat::Png => "image/png",
            VisualAssetFormat::Jpeg => "image/jpeg",
        };
        let encoded = base64::engine::general_purpose::STANDARD.encode(asset.payload().as_ref());
        let (width, height) = asset.dimensions();
        svg_fmt(
            output,
            format_args!(
                "<image id=\"visual-asset-{index}\" x=\"0\" y=\"0\" width=\"{width}\" height=\"{height}\" preserveAspectRatio=\"none\" xlink:href=\"data:{mime};base64,{encoded}\"/>\n"
            ),
        )?;
    }
    output.push_str("</defs>\n")
}

fn write_svg_image_use(
    output: &mut BoundedString,
    asset_index: usize,
    placement: VisualImagePlacement,
    opacity: f64,
) -> Result<(), DiagramExportError> {
    let [a, b, c, d, e, f] = placement.matrix;
    svg_fmt(
        output,
        format_args!(
            "<use xlink:href=\"#visual-asset-{asset_index}\" href=\"#visual-asset-{asset_index}\" opacity=\"{}\" transform=\"matrix({} {} {} {} {} {})\"/>\n",
            visual_svg_num(opacity),
            visual_svg_num(a),
            visual_svg_num(b),
            visual_svg_num(c),
            visual_svg_num(d),
            visual_svg_num(e),
            visual_svg_num(f),
        ),
    )
}

fn image_placement(
    asset: &VisualAsset,
    outline: &[ScenePoint],
) -> Result<VisualImagePlacement, DiagramExportError> {
    if outline.len() != 4 {
        return Err(DiagramExportError::Encode(
            "visual image does not have a four-corner projected outline".into(),
        ));
    }
    let (width, height) = asset.dimensions();
    if width == 0 || height == 0 {
        return Err(DiagramExportError::Encode(
            "visual image has invalid source dimensions".into(),
        ));
    }
    let top_left = outline[0];
    let top_right = outline[1];
    let bottom_left = outline[3];
    let sx = (top_right.x - top_left.x) / f64::from(width);
    let ky = (top_right.y - top_left.y) / f64::from(width);
    let kx = (bottom_left.x - top_left.x) / f64::from(height);
    let sy = (bottom_left.y - top_left.y) / f64::from(height);
    let matrix = [sx, ky, kx, sy, top_left.x, top_left.y];
    if matrix.iter().any(|coefficient| !coefficient.is_finite()) {
        return Err(DiagramExportError::Encode(
            "visual image transform is not finite".into(),
        ));
    }
    Ok(VisualImagePlacement { matrix })
}

fn image_pixel_transform(
    placement: VisualImagePlacement,
    transform: ExportTransform,
) -> Result<tiny_skia::Transform, DiagramExportError> {
    let [a, b, c, d, e, f] = placement.matrix;
    let scale = transform.scale;
    let pixel_matrix = [
        a * scale,
        b * scale,
        c * scale,
        d * scale,
        (e - transform.scene_min.x) * scale + EXPORT_PADDING,
        (f - transform.scene_min.y) * scale + EXPORT_PADDING,
    ];
    if pixel_matrix
        .iter()
        .any(|coefficient| !coefficient.is_finite() || coefficient.abs() > f64::from(f32::MAX))
    {
        return Err(DiagramExportError::ImageAllocation);
    }
    let [sx, ky, kx, sy, tx, ty] = pixel_matrix.map(|coefficient| coefficient as f32);
    let transform = tiny_skia::Transform::from_row(sx, ky, kx, sy, tx, ty);
    transform
        .is_finite()
        .then_some(transform)
        .ok_or(DiagramExportError::ImageAllocation)
}

fn render_visual_vector_run(
    operations: &[VisualExportOp],
    transform: ExportTransform,
    pixmap: &mut tiny_skia::Pixmap,
) -> Result<(), DiagramExportError> {
    let mut svg = BoundedString::new(MAX_SVG_BYTES);
    write_visual_svg_root(&mut svg, transform, None, false)?;
    for operation in operations {
        if let VisualExportOp::Vector { fragment, .. } = operation {
            svg.push_str(fragment)?;
        }
    }
    svg.push_str("</svg>")?;
    let mut options = usvg::Options::default();
    options.resources_dir = None;
    options.image_href_resolver = usvg::ImageHrefResolver {
        resolve_data: Box::new(|_, _, _| None),
        resolve_string: Box::new(|_, _| None),
    };
    let tree = usvg::Tree::from_str(&svg.value, &options)
        .map_err(|_| DiagramExportError::Encode("visual vector run is invalid SVG".into()))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    Ok(())
}

fn svg_color(color: VisualColor, opacity: f64) -> (String, f64) {
    let [red, green, blue, alpha] = color_with_opacity(color, opacity);
    (
        format!("#{red:02x}{green:02x}{blue:02x}"),
        f64::from(alpha) / 255.0,
    )
}

fn visual_svg_num(value: f64) -> String {
    if value == 0.0 {
        "0".to_owned()
    } else {
        format!("{value:.6}")
    }
}

fn write_visual_filled_path(
    output: &mut BoundedString,
    points: &[ScenePoint],
    closed: bool,
    color: VisualColor,
    opacity: f64,
    stroke: Option<(VisualColor, f64, f64)>,
    offset: ScenePoint,
) -> Result<(), DiagramExportError> {
    if points.len() < 2 {
        return Ok(());
    }
    let (fill, fill_opacity) = svg_color(color, opacity);
    let mut stroke_attributes = String::from("stroke=\"none\"");
    if let Some((stroke_color, stroke_opacity, width)) = stroke
        && width.is_finite()
        && width > 0.0
    {
        let (stroke, stroke_alpha) = svg_color(stroke_color, stroke_opacity);
        stroke_attributes = format!(
            "stroke=\"{stroke}\" stroke-opacity=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"",
            visual_svg_num(stroke_alpha),
            visual_svg_num(width),
        );
    }
    svg_fmt(
        output,
        format_args!(
            "<path d=\"{}\" fill=\"{fill}\" fill-opacity=\"{}\" {stroke_attributes} transform=\"translate({} {})\"/>\n",
            visual_path_data(points, closed)?,
            visual_svg_num(fill_opacity),
            visual_svg_num(offset.x),
            visual_svg_num(offset.y),
        ),
    )
}

fn write_visual_stroke_path(
    output: &mut BoundedString,
    points: &[ScenePoint],
    closed: bool,
    color: VisualColor,
    opacity: f64,
    width: f64,
    line_style: VisualLineStyle,
    pattern: Option<&[ProjectedStrokePrimitive]>,
) -> Result<(), DiagramExportError> {
    if width <= 0.0 || points.len() < 2 {
        return Ok(());
    }
    if line_style != VisualLineStyle::Solid
        && let Some(pattern) = pattern
    {
        let (stroke, stroke_alpha) = svg_color(color, opacity);
        for primitive in pattern {
            match *primitive {
                ProjectedStrokePrimitive::Segment { start, end } => {
                    if !valid_export_point(start) || !valid_export_point(end) {
                        return Err(DiagramExportError::Encode(
                            "visual dash path contains an invalid coordinate".into(),
                        ));
                    }
                    svg_fmt(
                        output,
                        format_args!(
                            "<path d=\"M {} {} L {} {}\" fill=\"none\" stroke=\"{stroke}\" stroke-opacity=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>\n",
                            visual_svg_num(start.x),
                            visual_svg_num(start.y),
                            visual_svg_num(end.x),
                            visual_svg_num(end.y),
                            visual_svg_num(stroke_alpha),
                            visual_svg_num(width),
                        ),
                    )?;
                }
                ProjectedStrokePrimitive::Dot { center } => {
                    if !valid_export_point(center) {
                        return Err(DiagramExportError::Encode(
                            "visual dash dot contains an invalid coordinate".into(),
                        ));
                    }
                    svg_fmt(
                        output,
                        format_args!(
                            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{stroke}\" fill-opacity=\"{}\"/>\n",
                            visual_svg_num(center.x),
                            visual_svg_num(center.y),
                            visual_svg_num(width / 2.0),
                            visual_svg_num(stroke_alpha),
                        ),
                    )?;
                }
            }
        }
        return Ok(());
    }
    let (stroke, stroke_alpha) = svg_color(color, opacity);
    svg_fmt(
        output,
        format_args!(
            "<path d=\"{}\" fill=\"none\" stroke=\"{stroke}\" stroke-opacity=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>\n",
            visual_path_data(points, closed)?,
            visual_svg_num(stroke_alpha),
            visual_svg_num(width),
        ),
    )
}

fn visual_path_data(points: &[ScenePoint], closed: bool) -> Result<String, DiagramExportError> {
    if points.iter().any(|point| !valid_export_point(*point)) {
        return Err(DiagramExportError::Encode(
            "visual path contains an invalid coordinate".into(),
        ));
    }
    let mut path = String::with_capacity(points.len().saturating_mul(28).min(MAX_SVG_BYTES));
    if let Some(first) = points.first() {
        path.push_str(&format!(
            "M {} {}",
            visual_svg_num(first.x),
            visual_svg_num(first.y)
        ));
        for point in &points[1..] {
            path.push_str(&format!(
                " L {} {}",
                visual_svg_num(point.x),
                visual_svg_num(point.y)
            ));
        }
        if closed {
            path.push_str(" Z");
        }
    }
    Ok(path)
}

#[derive(Clone)]
struct VisualGlyphSelection {
    face_index: usize,
    character: char,
}

struct VisualGlyphGeometry {
    outline: Option<ab_glyph::Outline>,
    advance: f64,
    scale_x: f64,
    scale_y: f64,
    y_offset: f64,
}

fn write_visual_placeholder_label(
    output: &mut BoundedString,
    item: &ProjectedVisualItem,
    fonts: (&FontSet, &FontSet),
    plan: &mut VisualExportPlan,
) -> Result<(), DiagramExportError> {
    if item.transform.width < 76.0 || item.transform.height < 24.0 {
        return Ok(());
    }
    let width = item.transform.width * 0.84;
    let height = item.transform.height * 0.75;
    let line = PreparedTextLine {
        text: "Image unavailable".to_owned(),
        scalar_count: "Image unavailable".chars().count(),
        reserved_width: width,
    };
    let prepared = PreparedText {
        lines: vec![line],
        reserved_width: width,
        height,
        line_height: height,
    };
    write_visual_text(
        output,
        item,
        &prepared,
        fonts,
        item.style.border,
        item.style.opacity,
        0.0,
        ScenePoint::default(),
        true,
        &mut plan.warnings,
        &mut plan.glyph_count,
    )
}

#[allow(clippy::too_many_arguments)]
fn write_visual_text(
    output: &mut BoundedString,
    item: &ProjectedVisualItem,
    text: &PreparedText,
    fonts: (&FontSet, &FontSet),
    color: VisualColor,
    opacity: f64,
    stroke_width: f64,
    offset: ScenePoint,
    count_glyphs: bool,
    warnings: &mut DiagramExportWarnings,
    glyph_count: &mut usize,
) -> Result<(), DiagramExportError> {
    if text.lines.is_empty() {
        return Ok(());
    }
    let center = item.text_transform.unwrap_or(item.transform);
    let center_point = ScenePoint {
        x: center.x + offset.x,
        y: center.y + offset.y,
    };
    let line_count = text.lines.len();
    let line_height = text.line_height.min(text.height / line_count as f64);
    if !line_height.is_finite() || line_height <= 0.0 {
        return Ok(());
    }
    let rotation = item.text_rotation_degrees;
    if !center_point.x.is_finite() || !center_point.y.is_finite() || !rotation.is_finite() {
        return Err(DiagramExportError::Encode(
            "visual text transform is not finite".into(),
        ));
    }
    let font_set = if item.style.font_family.eq_ignore_ascii_case("monospace") {
        fonts.1
    } else {
        fonts.0
    };
    let (fill, fill_alpha) = svg_color(color, opacity);
    let stroke_attributes = if stroke_width > 0.0 && stroke_width.is_finite() {
        let (stroke, stroke_alpha) = svg_color(color, opacity);
        format!(
            " stroke=\"{stroke}\" stroke-opacity=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\"",
            visual_svg_num(stroke_alpha),
            visual_svg_num(stroke_width),
        )
    } else {
        String::new()
    };
    let group_transform = format!(
        "translate({} {}) rotate({})",
        visual_svg_num(center_point.x),
        visual_svg_num(center_point.y),
        visual_svg_num(rotation),
    );
    svg_fmt(
        output,
        format_args!(
            "<g transform=\"{group_transform}\" fill=\"{fill}\" fill-opacity=\"{}\">\n",
            visual_svg_num(fill_alpha)
        ),
    )?;

    let total_text_height = line_height * line_count as f64;
    let mut suppressed_line = false;
    for (line_index, line) in text.lines.iter().enumerate() {
        if line.text.is_empty() {
            continue;
        }
        let mut selections = Vec::with_capacity(line.text.chars().count());
        for character in line.text.chars() {
            if !is_xml_character(character) {
                return Err(DiagramExportError::InvalidXmlControl(character));
            }
            if count_glyphs {
                *glyph_count = add_glyph_count(*glyph_count, 1)?;
            }
            let (face_index, character, substituted) = font_set.choose(character);
            if substituted && count_glyphs {
                warnings.glyph_substitutions = warnings.glyph_substitutions.saturating_add(1);
            }
            selections.push(VisualGlyphSelection {
                face_index,
                character,
            });
        }
        if selections.is_empty() {
            continue;
        }
        let reserved_width = line.reserved_width.min(text.reserved_width);
        if !reserved_width.is_finite() || reserved_width <= 0.0 {
            continue;
        }
        let mut font_size = item.style.font_size.min(line_height * 0.8);
        if !font_size.is_finite() || font_size < 0.5 || font_size > f64::from(f32::MAX) {
            suppressed_line = true;
            continue;
        }
        let line_center_y = -total_text_height / 2.0 + line_height * (line_index as f64 + 0.5);
        let mut layout = Vec::new();
        let mut total_advance = 0.0;
        let mut baseline_y = 0.0;
        let mut fits = false;
        for _ in 0..6 {
            let (candidate, advance, ink_bounds, baseline) =
                visual_glyph_line(&selections, font_size, line_center_y, font_set)?;
            layout = candidate;
            total_advance = advance;
            baseline_y = baseline;
            let advance_width = total_advance.max(0.0);
            let ink_width = ink_bounds.map_or(0.0, |(left, _, right, _)| {
                let left_from_center = left - total_advance / 2.0;
                let right_from_center = right - total_advance / 2.0;
                2.0 * (-left_from_center).max(right_from_center).max(0.0)
            });
            let ink_height = ink_bounds.map_or(0.0, |(_, top, _, bottom)| {
                let top_from_center = top - line_center_y;
                let bottom_from_center = bottom - line_center_y;
                2.0 * (-top_from_center).max(bottom_from_center).max(0.0)
            });
            let needed_width = advance_width.max(ink_width);
            let mut fit = 1.0f64;
            if needed_width > reserved_width && needed_width > 0.0 {
                fit = fit.min(reserved_width / needed_width);
            }
            if ink_height > line_height && ink_height > 0.0 {
                fit = fit.min(line_height / ink_height);
            }
            if fit >= 0.999 {
                fits = true;
                break;
            }
            font_size *= fit * 0.98;
            if font_size < 0.5 {
                layout.clear();
                break;
            }
        }
        if !fits || layout.is_empty() || total_advance <= 0.0 {
            suppressed_line = true;
            continue;
        }
        let mut cursor_x = -total_advance / 2.0;
        for glyph in &layout {
            if let Some(outline) = &glyph.outline {
                write_visual_glyph_outline(
                    output,
                    outline,
                    cursor_x,
                    glyph.scale_x,
                    glyph.scale_y,
                    glyph.y_offset,
                    line_center_y,
                    baseline_y,
                    &stroke_attributes,
                )?;
            }
            cursor_x += glyph.advance;
        }
    }
    if suppressed_line && count_glyphs && !item.text_truncated {
        warnings.visual_truncated_text_items =
            warnings.visual_truncated_text_items.saturating_add(1);
    }
    output.push_str("</g>\n")
}

fn visual_glyph_line(
    selections: &[VisualGlyphSelection],
    font_size: f64,
    line_center_y: f64,
    fonts: &FontSet,
) -> Result<
    (
        Vec<VisualGlyphGeometry>,
        f64,
        Option<(f64, f64, f64, f64)>,
        f64,
    ),
    DiagramExportError,
> {
    let primary = &fonts.faces[0];
    let primary_size = (font_size * f64::from(primary.tweak.scale)) as f32;
    let primary_scaled = primary.font.as_scaled(primary_size);
    let baseline = f64::from(primary_scaled.ascent()) - f64::from(primary_scaled.height()) / 2.0;
    let mut total_advance = 0.0;
    let mut ink_bounds = None::<(f64, f64, f64, f64)>;
    let mut output = Vec::with_capacity(selections.len());
    for selection in selections {
        let face = &fonts.faces[selection.face_index];
        let layout_scaled = face.font.as_scaled(font_size as f32);
        let visual_scaled = face
            .font
            .as_scaled((font_size * f64::from(face.tweak.scale)) as f32);
        let glyph_id = layout_scaled.glyph_id(selection.character);
        let space_advance = f64::from(layout_scaled.h_advance(layout_scaled.glyph_id(' ')));
        let advance = match selection.character {
            '\t' => space_advance * f64::from(face.tweak.tab_size),
            '\u{2009}' | '\u{202f}' => space_advance * f64::from(face.tweak.thin_space_width),
            _ => f64::from(layout_scaled.h_advance(glyph_id)),
        };
        if !advance.is_finite() || advance < 0.0 {
            return Err(DiagramExportError::ImageAllocation);
        }
        let outline = if selection.character.is_whitespace() {
            None
        } else {
            face.font
                .outline(visual_scaled.glyph_id(selection.character))
        };
        let scale = visual_scaled.scale_factor();
        let scale_x = f64::from(scale.horizontal);
        let scale_y = f64::from(scale.vertical);
        let y_offset =
            font_size * f64::from(face.tweak.scale) * f64::from(face.tweak.y_offset_factor)
                + f64::from(face.tweak.y_offset);
        if let Some(outline) = &outline {
            let left = total_advance + f64::from(outline.bounds.min.x) * scale_x;
            let right = total_advance + f64::from(outline.bounds.max.x) * scale_x;
            let top =
                line_center_y + baseline + y_offset - f64::from(outline.bounds.min.y) * scale_y;
            let bottom =
                line_center_y + baseline + y_offset - f64::from(outline.bounds.max.y) * scale_y;
            ink_bounds = Some(match ink_bounds {
                Some((min_x, min_y, max_x, max_y)) => (
                    min_x.min(left),
                    min_y.min(top),
                    max_x.max(right),
                    max_y.max(bottom),
                ),
                None => (left, top, right, bottom),
            });
        }
        output.push(VisualGlyphGeometry {
            outline,
            advance,
            scale_x,
            scale_y,
            y_offset,
        });
        total_advance += advance;
    }
    if !total_advance.is_finite()
        || output.iter().any(|glyph| {
            !glyph.scale_x.is_finite() || !glyph.scale_y.is_finite() || !glyph.y_offset.is_finite()
        })
    {
        return Err(DiagramExportError::ImageAllocation);
    }
    Ok((output, total_advance, ink_bounds, baseline))
}

fn write_visual_glyph_outline(
    output: &mut BoundedString,
    outline: &ab_glyph::Outline,
    cursor_x: f64,
    scale_x: f64,
    scale_y: f64,
    y_offset: f64,
    line_center_y: f64,
    baseline_y: f64,
    stroke_attributes: &str,
) -> Result<(), DiagramExportError> {
    let mut path = BoundedString::new(MAX_SVG_BYTES);
    let mut previous_end = None::<ab_glyph::Point>;
    for curve in &outline.curves {
        match curve {
            OutlineCurve::Line(start, end) => {
                begin_visual_glyph_contour(
                    &mut path,
                    *start,
                    &mut previous_end,
                    cursor_x,
                    scale_x,
                    scale_y,
                    y_offset,
                    line_center_y,
                    baseline_y,
                )?;
                let (x, y) = visual_glyph_point(
                    *end,
                    cursor_x,
                    scale_x,
                    scale_y,
                    y_offset,
                    line_center_y,
                    baseline_y,
                );
                svg_fmt(
                    &mut path,
                    format_args!(" L {} {}", visual_svg_num(x), visual_svg_num(y)),
                )?;
                previous_end = Some(*end);
            }
            OutlineCurve::Quad(start, control, end) => {
                begin_visual_glyph_contour(
                    &mut path,
                    *start,
                    &mut previous_end,
                    cursor_x,
                    scale_x,
                    scale_y,
                    y_offset,
                    line_center_y,
                    baseline_y,
                )?;
                let (cx, cy) = visual_glyph_point(
                    *control,
                    cursor_x,
                    scale_x,
                    scale_y,
                    y_offset,
                    line_center_y,
                    baseline_y,
                );
                let (x, y) = visual_glyph_point(
                    *end,
                    cursor_x,
                    scale_x,
                    scale_y,
                    y_offset,
                    line_center_y,
                    baseline_y,
                );
                svg_fmt(
                    &mut path,
                    format_args!(
                        " Q {} {}, {} {}",
                        visual_svg_num(cx),
                        visual_svg_num(cy),
                        visual_svg_num(x),
                        visual_svg_num(y)
                    ),
                )?;
                previous_end = Some(*end);
            }
            OutlineCurve::Cubic(start, control_a, control_b, end) => {
                begin_visual_glyph_contour(
                    &mut path,
                    *start,
                    &mut previous_end,
                    cursor_x,
                    scale_x,
                    scale_y,
                    y_offset,
                    line_center_y,
                    baseline_y,
                )?;
                let (ax, ay) = visual_glyph_point(
                    *control_a,
                    cursor_x,
                    scale_x,
                    scale_y,
                    y_offset,
                    line_center_y,
                    baseline_y,
                );
                let (bx, by) = visual_glyph_point(
                    *control_b,
                    cursor_x,
                    scale_x,
                    scale_y,
                    y_offset,
                    line_center_y,
                    baseline_y,
                );
                let (x, y) = visual_glyph_point(
                    *end,
                    cursor_x,
                    scale_x,
                    scale_y,
                    y_offset,
                    line_center_y,
                    baseline_y,
                );
                svg_fmt(
                    &mut path,
                    format_args!(
                        " C {} {}, {} {}, {} {}",
                        visual_svg_num(ax),
                        visual_svg_num(ay),
                        visual_svg_num(bx),
                        visual_svg_num(by),
                        visual_svg_num(x),
                        visual_svg_num(y)
                    ),
                )?;
                previous_end = Some(*end);
            }
        }
    }
    if !path.value.is_empty() {
        path.push_str(" Z")?;
        svg_fmt(
            output,
            format_args!("<path d=\"{}\"{stroke_attributes}/>\n", path.value),
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn begin_visual_glyph_contour(
    path: &mut BoundedString,
    start: ab_glyph::Point,
    previous_end: &mut Option<ab_glyph::Point>,
    cursor_x: f64,
    scale_x: f64,
    scale_y: f64,
    y_offset: f64,
    line_center_y: f64,
    baseline_y: f64,
) -> Result<(), DiagramExportError> {
    let starts_new_contour = previous_end.is_none_or(|previous| {
        (f64::from(previous.x) - f64::from(start.x)).abs() > 1.0e-5
            || (f64::from(previous.y) - f64::from(start.y)).abs() > 1.0e-5
    });
    if starts_new_contour {
        if previous_end.is_some() {
            path.push_str(" Z")?;
        }
        let (x, y) = visual_glyph_point(
            start,
            cursor_x,
            scale_x,
            scale_y,
            y_offset,
            line_center_y,
            baseline_y,
        );
        svg_fmt(
            path,
            format_args!(" M {} {}", visual_svg_num(x), visual_svg_num(y)),
        )?;
    }
    Ok(())
}

fn visual_glyph_point(
    point: ab_glyph::Point,
    cursor_x: f64,
    scale_x: f64,
    scale_y: f64,
    y_offset: f64,
    line_center_y: f64,
    baseline_y: f64,
) -> (f64, f64) {
    (
        cursor_x + f64::from(point.x) * scale_x,
        line_center_y + baseline_y + y_offset - f64::from(point.y) * scale_y,
    )
}

fn encode_visual_svg(
    snapshot: &DiagramVisualSnapshot,
    options: DiagramVisualExportOptions,
) -> Result<DiagramExportArtifact, DiagramExportError> {
    let plan = build_visual_export_plan(snapshot, options, default_font_sets()?)?;
    let mut output = BoundedString::new(MAX_SVG_BYTES);
    write_visual_svg_root(&mut output, plan.transform, options.background, true)?;
    write_visual_svg_asset_definitions(&mut output, &plan)?;
    for operation in &plan.ops {
        match operation {
            VisualExportOp::Vector { fragment, .. } => output.push_str(fragment)?,
            VisualExportOp::Image {
                asset_id,
                placement,
                opacity,
                ..
            } => {
                let asset_index = *plan.asset_indices.get(asset_id).ok_or_else(|| {
                    DiagramExportError::Encode("visual image asset order is inconsistent".into())
                })?;
                write_svg_image_use(&mut output, asset_index, *placement, *opacity)?;
            }
        }
    }
    output.push_str("</svg>\n")?;
    Ok(DiagramExportArtifact {
        revision: snapshot.revision,
        format: DiagramExportFormat::Svg,
        bytes: output.value.into_bytes(),
        dimensions: Some((plan.transform.width, plan.transform.height)),
        warnings: plan.warnings,
    })
}

fn encode_visual_png(
    snapshot: &DiagramVisualSnapshot,
    options: DiagramVisualExportOptions,
    fonts: (&FontSet, &FontSet),
) -> Result<DiagramExportArtifact, DiagramExportError> {
    let plan = build_visual_export_plan(snapshot, options, fonts)?;
    if plan.raster_work > MAX_RASTER_PIXEL_VISITS {
        return Err(DiagramExportError::RasterWorkLimit {
            estimated: plan.raster_work,
            max: MAX_RASTER_PIXEL_VISITS,
        });
    }
    let mut pixmap = tiny_skia::Pixmap::new(plan.transform.width, plan.transform.height)
        .ok_or(DiagramExportError::ImageAllocation)?;
    if let Some(background) = options.background {
        let [red, green, blue, alpha] = color_with_opacity(background, 1.0);
        pixmap.fill(tiny_skia::Color::from_rgba8(red, green, blue, alpha));
    }

    let mut index = 0usize;
    while index < plan.ops.len() {
        match &plan.ops[index] {
            VisualExportOp::Image {
                asset_id,
                placement,
                opacity,
                ..
            } => {
                let asset = plan.assets.get(asset_id).ok_or_else(|| {
                    DiagramExportError::Encode("visual image asset is unavailable".into())
                })?;
                let source = asset.pixmap.as_ref().ok_or_else(|| {
                    DiagramExportError::Encode(
                        "an invalid image was not converted to a placeholder".into(),
                    )
                })?;
                let transform = image_pixel_transform(*placement, plan.transform)?;
                let paint = tiny_skia::PixmapPaint {
                    opacity: opacity.clamp(0.0, 1.0) as f32,
                    quality: tiny_skia::FilterQuality::Bicubic,
                    ..tiny_skia::PixmapPaint::default()
                };
                pixmap.as_mut().draw_pixmap(
                    0,
                    0,
                    source.as_ref().as_ref(),
                    &paint,
                    transform,
                    None,
                );
                index += 1;
            }
            VisualExportOp::Vector { .. } => {
                let start = index;
                while index < plan.ops.len()
                    && matches!(plan.ops[index], VisualExportOp::Vector { .. })
                {
                    index += 1;
                }
                render_visual_vector_run(&plan.ops[start..index], plan.transform, &mut pixmap)?;
            }
        }
    }

    let mut writer = BoundedBytes::new(MAX_PNG_BYTES);
    let mut rgba = pixmap.take();
    unpremultiply_rgba(&mut rgba);
    let result = image::codecs::png::PngEncoder::new(&mut writer).write_image(
        &rgba,
        plan.transform.width,
        plan.transform.height,
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
        dimensions: Some((plan.transform.width, plan.transform.height)),
        warnings: plan.warnings,
    })
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
        Self::from_family(definitions, &FontFamily::Proportional)
    }

    fn from_family(definitions: &FontDefinitions, family: &FontFamily) -> Result<Self, String> {
        let family = definitions
            .families
            .get(family)
            .ok_or_else(|| "requested font family is missing".to_owned())?;
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
static DEFAULT_MONOSPACE_FONT_SET: OnceLock<Result<FontSet, String>> = OnceLock::new();

fn default_font_set() -> Result<&'static FontSet, DiagramExportError> {
    match DEFAULT_FONT_SET.get_or_init(|| FontSet::from_definitions(&FontDefinitions::default())) {
        Ok(fonts) => Ok(fonts),
        Err(error) => Err(DiagramExportError::Fonts(error.clone())),
    }
}

fn default_font_sets() -> Result<(&'static FontSet, &'static FontSet), DiagramExportError> {
    let proportional = default_font_set()?;
    let monospace = match DEFAULT_MONOSPACE_FONT_SET
        .get_or_init(|| FontSet::from_family(&FontDefinitions::default(), &FontFamily::Monospace))
    {
        Ok(fonts) => fonts,
        Err(error) => return Err(DiagramExportError::Fonts(error.clone())),
    };
    Ok((proportional, monospace))
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
        DiagramExportPublicationError, DiagramImageSnapshot, DiagramSourceSnapshot,
        DiagramVisualExportOptions, DiagramVisualSnapshot, FontSet, MAX_EXPORT_PIXELS,
        MAX_EXPORT_SIDE, MAX_PNG_BYTES, MAX_PREPARED_GLYPHS, MAX_RASTER_PIXEL_VISITS,
        is_xml_character,
    };
    use crate::diagram::{
        DEFAULT_LAYER_ID, DiagramDocumentController, DiagramSession, FlowDirection, GraphOperation,
        NodePosition, Position, ProjectedVisualItem, SceneMetrics, ScenePoint, SceneRect,
        VisualAsset, VisualAssetFormat, VisualAssetId, VisualColor, VisualDiagramDocument,
        VisualLineStyle, VisualObjectDraft, VisualObjectId, VisualObjectKind, VisualOperation,
        VisualPrimitiveShape, VisualSceneProjection, VisualShadow, VisualStyle, VisualTransform,
        project_visual_document,
    };
    use eframe::egui::{FontDefinitions, FontTweak};
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use resvg::usvg;
    use std::fs;
    use std::io::{Cursor, Write};
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

    fn visual_export_snapshot(
        document: &VisualDiagramDocument,
    ) -> (DiagramVisualSnapshot, VisualSceneProjection) {
        let projection = project_visual_document(document).unwrap();
        (
            DiagramVisualSnapshot::capture(document.revision(), Arc::new(projection.clone())),
            projection,
        )
    }

    fn visual_style(fill: VisualColor, border: VisualColor, opacity: f64) -> VisualStyle {
        VisualStyle {
            fill,
            border,
            border_width: 0.0,
            line_style: VisualLineStyle::Solid,
            font_family: "sans-serif".into(),
            font_size: 20.0,
            opacity,
            shadow: None,
        }
    }

    fn add_visual_object(
        document: &mut VisualDiagramDocument,
        transform: VisualTransform,
        style: VisualStyle,
        kind: VisualObjectKind,
    ) -> VisualObjectId {
        document
            .apply_transaction(&[VisualOperation::AddObject(VisualObjectDraft {
                layer_id: DEFAULT_LAYER_ID,
                transform,
                style,
                kind,
            })])
            .unwrap()
            .created_objects[0]
    }

    fn visual_png_bytes(image: RgbaImage) -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut output, ImageFormat::Png)
            .unwrap();
        output.into_inner()
    }

    fn visual_export_pixel(
        artifact: &super::DiagramExportArtifact,
        projection: &VisualSceneProjection,
        world: ScenePoint,
    ) -> image::Rgba<u8> {
        let transform = super::ExportTransform::new(projection.bounds, 1.0).unwrap();
        let position = transform.point(world);
        let image = image::load_from_memory(artifact.bytes())
            .unwrap()
            .to_rgba8();
        let x = position.x.floor().clamp(0.0, f64::from(image.width() - 1)) as u32;
        let y = position.y.floor().clamp(0.0, f64::from(image.height() - 1)) as u32;
        *image.get_pixel(x, y)
    }

    #[test]
    fn visual_frame_export_uses_shared_header_and_transparent_interior_with_shadow() {
        let mut document = VisualDiagramDocument::new();
        document.replace_label("").unwrap();
        let mut style = visual_style(
            VisualColor::rgba(35, 120, 200, 255),
            VisualColor::rgba(0, 0, 0, 255),
            1.0,
        );
        style.border_width = 2.0;
        style.shadow = Some(VisualShadow {
            offset_x: 8.0,
            offset_y: 8.0,
            blur: 2.0,
            color: VisualColor::rgba(20, 20, 20, 160),
        });
        let id = add_visual_object(
            &mut document,
            VisualTransform {
                x: 300.0,
                y: 0.0,
                width: 180.0,
                height: 100.0,
                rotation_degrees: 0.0,
            },
            style,
            VisualObjectKind::Container {
                label: String::new(),
            },
        );
        let (snapshot, projection) = visual_export_snapshot(&document);
        let item = projection
            .items
            .iter()
            .find(|item| item.object_id == id)
            .unwrap();
        let header = item
            .header
            .as_ref()
            .expect("frame projects its header geometry");
        let svg = snapshot
            .encode_svg(DiagramVisualExportOptions::default())
            .unwrap();
        let svg_text = std::str::from_utf8(svg.bytes()).unwrap();
        assert!(svg_text.contains(&super::visual_path_data(header, true).unwrap()));
        assert!(svg_text.contains(&super::visual_path_data(&item.outline, true).unwrap()));
        let png = snapshot
            .encode_png(DiagramVisualExportOptions::default())
            .unwrap();
        assert!(visual_export_pixel(&png, &projection, ScenePoint { x: 300.0, y: -40.0 })[3] > 0);
        assert_eq!(
            visual_export_pixel(&png, &projection, ScenePoint { x: 300.0, y: 10.0 })[3],
            0
        );
        assert!(
            visual_export_pixel(&png, &projection, ScenePoint { x: 398.0, y: 10.0 })[3] > 0,
            "shifted frame-border shadow is exported"
        );
        let mut malformed = item.clone();
        malformed.header.as_mut().unwrap()[0].x = f64::NAN;
        assert!(super::validate_visual_export_item(&malformed).is_err());
        malformed.header = Some(vec![
            ScenePoint::default();
            super::MAX_VISUAL_OUTLINE_POINTS + 1
        ]);
        assert!(super::validate_visual_export_item(&malformed).is_err());
    }

    #[test]
    fn visual_scene_export_vector_svg_and_png_preserve_rotated_outlines_text_and_alpha() {
        let mut document = VisualDiagramDocument::new();
        document.replace_label("").unwrap();
        let mut ellipse_style = visual_style(
            VisualColor::rgba(20, 70, 225, 128),
            VisualColor::rgba(15, 25, 45, 210),
            0.5,
        );
        ellipse_style.border_width = 2.0;
        ellipse_style.line_style = VisualLineStyle::Dashed;
        ellipse_style.shadow = Some(VisualShadow {
            offset_x: 4.0,
            offset_y: -3.0,
            blur: 2.0,
            color: VisualColor::rgba(30, 40, 50, 100),
        });
        add_visual_object(
            &mut document,
            VisualTransform {
                x: 300.0,
                y: 100.0,
                width: 120.0,
                height: 80.0,
                rotation_degrees: 31.0,
            },
            ellipse_style,
            VisualObjectKind::Primitive {
                shape: VisualPrimitiveShape::Ellipse,
            },
        );
        let text_transform = VisualTransform {
            x: 0.0,
            y: 0.0,
            width: 180.0,
            height: 88.0,
            rotation_degrees: -17.0,
        };
        add_visual_object(
            &mut document,
            text_transform,
            visual_style(
                VisualColor::rgba(0, 0, 0, 0),
                VisualColor::rgba(20, 25, 35, 255),
                1.0,
            ),
            VisualObjectKind::Text {
                text: "gyp\nO8".into(),
            },
        );

        let (snapshot, projection) = visual_export_snapshot(&document);
        let svg_artifact = snapshot
            .encode_svg(DiagramVisualExportOptions::default())
            .unwrap();
        let svg = std::str::from_utf8(svg_artifact.bytes()).unwrap();
        assert_eq!(svg_artifact.revision(), document.revision());
        assert_eq!(svg_artifact.format(), DiagramExportFormat::Svg);
        assert!(svg.contains("rotate(-17"));
        assert!(svg.contains("fill-opacity=\"0.250980\""));
        assert!(svg.contains("stroke-opacity="));
        assert!(svg.contains("<path d=\""));
        assert!(svg.contains(" Q ") || svg.contains(" C "));
        assert!(!svg.contains("<text"));
        assert!(!svg.contains("gyp"));
        let mut options = usvg::Options::default();
        options.resources_dir = None;
        options.image_href_resolver = usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        };
        usvg::Tree::from_str(svg, &options)
            .expect("visual SVG should parse without external resources");

        let png = snapshot
            .encode_png(DiagramVisualExportOptions::default())
            .unwrap();
        let pixels = image::load_from_memory(png.bytes()).unwrap().to_rgba8();
        assert_eq!(pixels.get_pixel(0, 0).0[3], 0);
        let ellipse_pixel =
            visual_export_pixel(&png, &projection, ScenePoint { x: 300.0, y: 100.0 });
        assert!(ellipse_pixel[2] > ellipse_pixel[0]);
        assert!((96..=106).contains(&ellipse_pixel[3]));
        let text_item = projection
            .items
            .iter()
            .find(|item| item.text.is_some())
            .expect("the rotated multiline text should be projected");
        let export_transform = super::ExportTransform::new(projection.bounds, 1.0).unwrap();
        assert!(dark_ink_bounds_in(&pixels, export_transform.rect(text_item.bounds)).is_some());
        assert_eq!(png.warnings().glyph_substitutions(), 0);
        assert_eq!(png.warnings().visual_simplified_dash_paths(), 0);
    }

    #[test]
    fn visual_scene_export_embeds_rasters_once_and_preserves_z_order_and_opacity() {
        let mut document = VisualDiagramDocument::new();
        document.replace_label("").unwrap();
        let source = RgbaImage::from_pixel(2, 2, Rgba([245, 20, 10, 255]));
        let prepared = crate::diagram::assets::prepare_visual_asset_bytes(
            visual_png_bytes(source),
            usize::MAX,
        )
        .unwrap();
        let image_style = visual_style(
            VisualColor::rgba(0, 0, 0, 0),
            VisualColor::rgba(0, 0, 0, 0),
            0.5,
        );
        document
            .apply_transaction(&[
                VisualOperation::AddImageReference {
                    asset: prepared.asset.clone(),
                    layer_id: DEFAULT_LAYER_ID,
                    transform: VisualTransform {
                        x: 0.0,
                        y: 0.0,
                        width: 140.0,
                        height: 100.0,
                        rotation_degrees: 27.0,
                    },
                    style: image_style.clone(),
                },
                VisualOperation::AddImageReference {
                    asset: prepared.asset.clone(),
                    layer_id: DEFAULT_LAYER_ID,
                    transform: VisualTransform {
                        x: 200.0,
                        y: 0.0,
                        width: 80.0,
                        height: 80.0,
                        rotation_degrees: 0.0,
                    },
                    style: image_style,
                },
                VisualOperation::AddObject(VisualObjectDraft {
                    layer_id: DEFAULT_LAYER_ID,
                    transform: VisualTransform {
                        x: 0.0,
                        y: 0.0,
                        width: 24.0,
                        height: 24.0,
                        rotation_degrees: 0.0,
                    },
                    style: visual_style(
                        VisualColor::rgba(15, 40, 220, 255),
                        VisualColor::rgba(0, 0, 0, 0),
                        1.0,
                    ),
                    kind: VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle,
                    },
                }),
            ])
            .unwrap();

        let (snapshot, projection) = visual_export_snapshot(&document);
        let svg_artifact = snapshot
            .encode_svg(DiagramVisualExportOptions::default())
            .unwrap();
        let svg = std::str::from_utf8(svg_artifact.bytes()).unwrap();
        assert_eq!(svg.matches("data:image/png;base64,").count(), 1);
        assert_eq!(svg.matches("<use ").count(), 2);
        assert!(svg.matches("opacity=\"0.500000\"").count() >= 2);
        assert!(svg.contains("xlink:href=\"#visual-asset-0\""));

        let png = snapshot
            .encode_png(DiagramVisualExportOptions::default())
            .unwrap();
        let blue_top = visual_export_pixel(&png, &projection, ScenePoint { x: 0.0, y: 0.0 });
        assert!(blue_top[2] > blue_top[0] + 80);
        let translucent_raster =
            visual_export_pixel(&png, &projection, ScenePoint { x: 40.0, y: 0.0 });
        assert!(translucent_raster[0] > translucent_raster[2]);
        assert!((120..=136).contains(&translucent_raster[3]));
    }

    #[test]
    fn visual_scene_export_reports_invalid_image_placeholders_and_rejects_unbounded_scale() {
        use sha2::Digest as _;

        let payload = b"not an image".to_vec();
        let digest = hex::encode(sha2::Sha256::digest(&payload));
        let asset = VisualAsset::from_verified_parts(
            VisualAssetId::new(digest).unwrap(),
            VisualAssetFormat::Png,
            2,
            2,
            Arc::from(payload.into_boxed_slice()),
        );
        let object_id = VisualObjectId::from_raw(2);
        let transform = VisualTransform {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 80.0,
            rotation_degrees: 0.0,
        };
        let bounds = SceneRect {
            min: ScenePoint { x: -50.0, y: -40.0 },
            max: ScenePoint { x: 50.0, y: 40.0 },
        };
        let item = ProjectedVisualItem {
            object_id,
            component_index: None,
            layer_id: DEFAULT_LAYER_ID,
            kind: crate::diagram::VisualProjectedKind::Image,
            image_asset: Some(asset),
            transform,
            style: visual_style(
                VisualColor::rgba(230, 230, 235, 255),
                VisualColor::rgba(20, 30, 40, 255),
                1.0,
            ),
            outline: vec![
                ScenePoint { x: -50.0, y: -40.0 },
                ScenePoint { x: 50.0, y: -40.0 },
                ScenePoint { x: 50.0, y: 40.0 },
                ScenePoint { x: -50.0, y: 40.0 },
            ],
            header: None,
            details: Vec::new(),
            open_paths: Vec::new(),
            text: None,
            text_transform: None,
            text_bounds: None,
            text_truncated: false,
            text_rotation_degrees: 0.0,
            bounds,
            dash_simplified: false,
            dash_paths: None,
            route_fallback: false,
        };
        let projection = VisualSceneProjection {
            items: vec![item],
            bounds: Some(bounds),
            simplified_dash_paths: 0,
            truncated_text_items: 0,
            text_lines: 0,
            connector_route_fallbacks: 0,
        };
        let snapshot = DiagramVisualSnapshot::capture(9, Arc::new(projection));
        assert!(matches!(
            snapshot.encode_svg(DiagramVisualExportOptions {
                scale: 0.001,
                background: None,
            }),
            Err(DiagramExportError::VisualScaleOutOfRange)
        ));

        let svg = snapshot
            .encode_svg(DiagramVisualExportOptions::default())
            .unwrap();
        assert_eq!(svg.warnings().visual_asset_placeholders(), 1);
        let png = snapshot
            .encode_png(DiagramVisualExportOptions::default())
            .unwrap();
        assert_eq!(png.warnings().visual_asset_placeholders(), 1);
        let pixels = image::load_from_memory(png.bytes()).unwrap().to_rgba8();
        assert!(pixels.pixels().any(|pixel| pixel.0[3] > 0));

        let mut malformed_projection = snapshot.projection().as_ref().clone();
        malformed_projection.items[0].outline.pop();
        let malformed = DiagramVisualSnapshot::capture(10, Arc::new(malformed_projection));
        assert!(matches!(
            malformed.encode_svg(DiagramVisualExportOptions::default()),
            Err(DiagramExportError::Encode(message))
                if message.contains("four-corner")
        ));
    }

    #[test]
    fn visual_scene_export_bounds_overlapping_vector_and_repeated_image_work() {
        let source = RgbaImage::from_pixel(2, 2, Rgba([40, 80, 120, 255]));
        let prepared = crate::diagram::assets::prepare_visual_asset_bytes(
            visual_png_bytes(source),
            usize::MAX,
        )
        .unwrap();
        let transform = VisualTransform {
            x: 0.0,
            y: 0.0,
            width: 3_500.0,
            height: 3_500.0,
            rotation_degrees: 0.0,
        };
        let mut operations = Vec::new();
        for _ in 0..6 {
            operations.push(VisualOperation::AddObject(VisualObjectDraft {
                layer_id: DEFAULT_LAYER_ID,
                transform,
                style: visual_style(
                    VisualColor::rgba(120, 150, 190, 255),
                    VisualColor::rgba(20, 30, 40, 255),
                    1.0,
                ),
                kind: VisualObjectKind::Primitive {
                    shape: VisualPrimitiveShape::Rectangle,
                },
            }));
        }
        for _ in 0..6 {
            operations.push(VisualOperation::AddImageReference {
                asset: prepared.asset.clone(),
                layer_id: DEFAULT_LAYER_ID,
                transform,
                style: visual_style(
                    VisualColor::rgba(0, 0, 0, 0),
                    VisualColor::rgba(0, 0, 0, 0),
                    1.0,
                ),
            });
        }
        let mut document = VisualDiagramDocument::new();
        document.replace_label("").unwrap();
        document.apply_transaction(&operations).unwrap();

        let (snapshot, _) = visual_export_snapshot(&document);
        assert!(matches!(
            snapshot.encode_png(DiagramVisualExportOptions::default()),
            Err(DiagramExportError::RasterWorkLimit {
                estimated,
                max: MAX_RASTER_PIXEL_VISITS,
            }) if estimated > MAX_RASTER_PIXEL_VISITS
        ));
    }
}
