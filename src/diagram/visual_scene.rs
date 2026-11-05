//! Bounded deterministic projection of persisted visual objects into paint geometry.

use super::scene::{PreparedText, PreparedTextLine, ScenePoint, SceneRect};
use super::visual::{
    VisualColor, VisualDiagramDocument, VisualDocumentError, VisualLayerId, VisualObjectId,
    VisualObjectKind, VisualPrimitiveComponent, VisualPrimitiveShape, VisualScene, VisualShadow,
    VisualStyle, VisualTransform,
};
use std::fmt;

pub const MAX_VISUAL_PROJECTED_ITEMS: usize = 4_096;
pub const MAX_VISUAL_OUTLINE_POINTS: usize = 64;
pub const MAX_VISUAL_TEXT_LINES_PER_OBJECT: usize = 256;
pub const MAX_VISUAL_TEXT_LINES_PER_SCENE: usize = 8_192;
pub const MAX_VISUAL_DASH_SEGMENTS_PER_ITEM: usize = 256;
pub const MAX_VISUAL_DASH_SEGMENTS_PER_SCENE: usize = 32_768;
pub const MAX_VISUAL_PROJECTION_COORDINATE_ABS: f64 = 1_000_000_000.0;
const ELLIPSE_STEPS: usize = 32;
const ROUND_CORNER_STEPS: usize = 5;
const PRESET_DESIGN_SIZE: f64 = 100.0;
const DASH_LENGTH: f64 = 6.0;
const DASH_GAP: f64 = 4.0;
const DOT_SPACING: f64 = 5.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualProjectedKind {
    Primitive(VisualPrimitiveShape),
    Text,
    PresetPrimitive(VisualPrimitiveShape),
    MissingImagePlaceholder,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedVisualItem {
    pub object_id: VisualObjectId,
    pub component_index: Option<usize>,
    pub layer_id: VisualLayerId,
    pub kind: VisualProjectedKind,
    pub transform: VisualTransform,
    pub style: VisualStyle,
    /// Closed, rotated outline path in document coordinates.
    pub outline: Vec<ScenePoint>,
    /// Additional closed paths such as a cylinder's top cap or missing-image X.
    pub details: Vec<Vec<ScenePoint>>,
    pub text: Option<PreparedText>,
    pub text_truncated: bool,
    pub text_rotation_degrees: f64,
    /// Union of fill, text ink, stroke, shadow and detail geometry.
    pub bounds: SceneRect,
    /// Large dashed paths remain visible with a solid outline and are counted here.
    pub dash_simplified: bool,
    /// Exact bounded dash/dot strokes in document coordinates, in outline-then-detail order.
    /// `None` means a solid line (including a path simplified by the projection budget).
    pub dash_paths: Option<Vec<Vec<ProjectedStrokePrimitive>>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProjectedStrokePrimitive {
    Segment { start: ScenePoint, end: ScenePoint },
    Dot { center: ScenePoint },
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisualSceneProjection {
    pub items: Vec<ProjectedVisualItem>,
    pub bounds: Option<SceneRect>,
    pub simplified_dash_paths: usize,
    pub truncated_text_items: usize,
    pub text_lines: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VisualProjectionError {
    InvalidScene(String),
    WorkLimitExceeded { kind: &'static str, max: usize },
    CoordinateLimitExceeded { max_coordinate_abs: f64 },
}

impl fmt::Display for VisualProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidScene(reason) => write!(formatter, "visual scene is invalid: {reason}"),
            Self::WorkLimitExceeded { kind, max } => {
                write!(
                    formatter,
                    "visual projection exceeds the {max}-item {kind} limit"
                )
            }
            Self::CoordinateLimitExceeded { max_coordinate_abs } => write!(
                formatter,
                "visual projection exceeds the {max_coordinate_abs}-unit coordinate limit"
            ),
        }
    }
}

impl std::error::Error for VisualProjectionError {}

impl From<VisualDocumentError> for VisualProjectionError {
    fn from(error: VisualDocumentError) -> Self {
        Self::InvalidScene(error.to_string())
    }
}

/// Project scene objects in layer order, preserving vector order within a layer.
/// A preset instance is a bounded list of primitive components in a 100×100
/// local design box. Its transform scales that box and its rotation pivots at
/// the instance center. Component styles are retained; instance opacity multiplies
/// component opacity.
pub fn project_visual_scene(
    scene: &VisualScene,
) -> Result<VisualSceneProjection, VisualProjectionError> {
    scene.validate()?;
    let mut items = Vec::new();
    let mut bounds = None;
    let mut text_lines = 0usize;
    let mut truncated_text_items = 0usize;
    let mut dash_budget = MAX_VISUAL_DASH_SEGMENTS_PER_SCENE;
    let mut simplified_dash_paths = 0usize;

    for layer in &scene.layers {
        for object in scene
            .objects
            .iter()
            .filter(|object| object.layer_id == layer.id)
        {
            let projected = match &object.kind {
                VisualObjectKind::Primitive { shape } => vec![build_item(
                    object.id,
                    None,
                    layer.id,
                    VisualProjectedKind::Primitive(*shape),
                    object.transform,
                    object.style.clone(),
                    None,
                    false,
                )?],
                VisualObjectKind::Text { text } => vec![build_item(
                    object.id,
                    None,
                    layer.id,
                    VisualProjectedKind::Text,
                    object.transform,
                    object.style.clone(),
                    Some(text),
                    false,
                )?],
                VisualObjectKind::ImageReference { .. } => vec![build_item(
                    object.id,
                    None,
                    layer.id,
                    VisualProjectedKind::MissingImagePlaceholder,
                    object.transform,
                    object.style.clone(),
                    Some("Missing image"),
                    true,
                )?],
                VisualObjectKind::PresetInstance { preset_id } => {
                    let preset = scene
                        .presets
                        .iter()
                        .find(|preset| preset.id == *preset_id)
                        .ok_or_else(|| {
                            VisualProjectionError::InvalidScene(format!(
                                "visual preset {} is missing",
                                preset_id.get()
                            ))
                        })?;
                    if items.len().saturating_add(preset.components.len())
                        > MAX_VISUAL_PROJECTED_ITEMS
                    {
                        return Err(VisualProjectionError::WorkLimitExceeded {
                            kind: "projected item",
                            max: MAX_VISUAL_PROJECTED_ITEMS,
                        });
                    }
                    preset
                        .components
                        .iter()
                        .enumerate()
                        .map(|(index, component)| {
                            let mut style = component.style.clone();
                            style.opacity *= object.style.opacity;
                            build_preset_item(
                                object.id,
                                Some(index),
                                layer.id,
                                VisualProjectedKind::PresetPrimitive(component.shape),
                                object.transform,
                                component,
                                style,
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()?
                }
            };

            if items.len().saturating_add(projected.len()) > MAX_VISUAL_PROJECTED_ITEMS {
                return Err(VisualProjectionError::WorkLimitExceeded {
                    kind: "projected item",
                    max: MAX_VISUAL_PROJECTED_ITEMS,
                });
            }
            for mut item in projected {
                if let Some(text) = item.text.as_ref() {
                    text_lines = text_lines.saturating_add(text.lines.len());
                    if text_lines > MAX_VISUAL_TEXT_LINES_PER_SCENE {
                        return Err(VisualProjectionError::WorkLimitExceeded {
                            kind: "prepared text line",
                            max: MAX_VISUAL_TEXT_LINES_PER_SCENE,
                        });
                    }
                }
                if item.text_truncated {
                    truncated_text_items += 1;
                }
                if item.style.line_style != super::visual::VisualLineStyle::Solid {
                    let item_budget = MAX_VISUAL_DASH_SEGMENTS_PER_ITEM.min(dash_budget);
                    if let Some(paths) = project_dash_paths(&item, item_budget) {
                        let used = paths.iter().map(Vec::len).sum::<usize>();
                        dash_budget = dash_budget.saturating_sub(used);
                        item.dash_paths = Some(paths);
                    } else {
                        item.dash_simplified = true;
                        simplified_dash_paths += 1;
                    }
                }
                validate_projected_bounds(item.bounds)?;
                bounds = Some(bounds.map_or(item.bounds, |current: SceneRect| {
                    union(current, item.bounds)
                }));
                items.push(item);
            }
        }
    }

    Ok(VisualSceneProjection {
        items,
        bounds,
        simplified_dash_paths,
        truncated_text_items,
        text_lines,
    })
}

pub fn project_visual_document(
    document: &VisualDiagramDocument,
) -> Result<VisualSceneProjection, VisualProjectionError> {
    project_visual_scene(document.scene())
}

fn build_item(
    object_id: VisualObjectId,
    component_index: Option<usize>,
    layer_id: VisualLayerId,
    kind: VisualProjectedKind,
    transform: VisualTransform,
    style: VisualStyle,
    text: Option<&str>,
    missing_image: bool,
) -> Result<ProjectedVisualItem, VisualProjectionError> {
    let (outline, details) = shape_paths(kind_shape(kind), transform);
    build_item_with_paths(
        object_id,
        component_index,
        layer_id,
        kind,
        transform,
        style,
        text,
        missing_image,
        outline,
        details,
    )
}

fn build_preset_item(
    object_id: VisualObjectId,
    component_index: Option<usize>,
    layer_id: VisualLayerId,
    kind: VisualProjectedKind,
    instance: VisualTransform,
    component: &VisualPrimitiveComponent,
    style: VisualStyle,
) -> Result<ProjectedVisualItem, VisualProjectionError> {
    let (mut outline, mut details) = shape_paths(component.shape, component.transform);
    for point in outline.iter_mut().chain(details.iter_mut().flatten()) {
        *point = transform_preset_point(*point, instance);
    }
    let transform = compose_preset_transform(instance, component);
    build_item_with_paths(
        object_id,
        component_index,
        layer_id,
        kind,
        transform,
        style,
        None,
        false,
        outline,
        details,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_item_with_paths(
    object_id: VisualObjectId,
    component_index: Option<usize>,
    layer_id: VisualLayerId,
    kind: VisualProjectedKind,
    transform: VisualTransform,
    style: VisualStyle,
    text: Option<&str>,
    missing_image: bool,
    outline: Vec<ScenePoint>,
    mut details: Vec<Vec<ScenePoint>>,
) -> Result<ProjectedVisualItem, VisualProjectionError> {
    if missing_image {
        details.push(vec![outline[0], outline[2]]);
        details.push(vec![outline[1], outline[3]]);
    }
    let (prepared_text, text_truncated) = text
        .map(|text| prepare_text(text, transform, style.font_size))
        .map_or((None, false), |(prepared, truncated)| {
            (Some(prepared), truncated)
        });
    let mut item_bounds = bounds_for_points(&outline)
        .ok_or_else(|| VisualProjectionError::InvalidScene("projected outline is empty".into()))?;
    for path in &details {
        if let Some(detail_bounds) = bounds_for_points(path) {
            item_bounds = union(item_bounds, detail_bounds);
        }
    }
    let stroke_margin = style.border_width / 2.0;
    item_bounds = expand(item_bounds, stroke_margin);
    if let Some(shadow) = style.shadow {
        item_bounds = union(item_bounds, shadow_bounds(&outline, shadow));
    }
    if let Some(text) = &prepared_text {
        let text_bounds = rotated_rect_bounds(
            ScenePoint {
                x: transform.x,
                y: transform.y,
            },
            text.reserved_width,
            text.height,
            transform.rotation_degrees,
        );
        item_bounds = union(item_bounds, text_bounds);
    }

    Ok(ProjectedVisualItem {
        object_id,
        component_index,
        layer_id,
        kind,
        transform,
        style,
        outline,
        details,
        text: prepared_text,
        text_truncated,
        text_rotation_degrees: transform.rotation_degrees,
        bounds: item_bounds,
        dash_simplified: false,
        dash_paths: None,
    })
}

fn kind_shape(kind: VisualProjectedKind) -> VisualPrimitiveShape {
    match kind {
        VisualProjectedKind::Primitive(shape) | VisualProjectedKind::PresetPrimitive(shape) => {
            shape
        }
        VisualProjectedKind::Text | VisualProjectedKind::MissingImagePlaceholder => {
            VisualPrimitiveShape::Rectangle
        }
    }
}

fn compose_preset_transform(
    instance: VisualTransform,
    component: &VisualPrimitiveComponent,
) -> VisualTransform {
    let scale_x = instance.width / PRESET_DESIGN_SIZE;
    let scale_y = instance.height / PRESET_DESIGN_SIZE;
    let local_x = component.transform.x * scale_x;
    let local_y = component.transform.y * scale_y;
    let (sin, cos) = instance.rotation_degrees.to_radians().sin_cos();
    VisualTransform {
        x: instance.x + local_x * cos - local_y * sin,
        y: instance.y + local_x * sin + local_y * cos,
        width: component.transform.width * scale_x,
        height: component.transform.height * scale_y,
        rotation_degrees: instance.rotation_degrees + component.transform.rotation_degrees,
    }
}

fn transform_preset_point(point: ScenePoint, instance: VisualTransform) -> ScenePoint {
    let scaled_x = point.x * instance.width / PRESET_DESIGN_SIZE;
    let scaled_y = point.y * instance.height / PRESET_DESIGN_SIZE;
    let (sin, cos) = instance.rotation_degrees.to_radians().sin_cos();
    ScenePoint {
        x: instance.x + scaled_x * cos - scaled_y * sin,
        y: instance.y + scaled_x * sin + scaled_y * cos,
    }
}

fn shape_paths(
    shape: VisualPrimitiveShape,
    transform: VisualTransform,
) -> (Vec<ScenePoint>, Vec<Vec<ScenePoint>>) {
    let half_width = transform.width / 2.0;
    let half_height = transform.height / 2.0;
    let mut details = Vec::new();
    let local = match shape {
        VisualPrimitiveShape::Rectangle => vec![
            point(-half_width, -half_height),
            point(half_width, -half_height),
            point(half_width, half_height),
            point(-half_width, half_height),
        ],
        VisualPrimitiveShape::RoundRect => rounded_rectangle_points(half_width, half_height),
        VisualPrimitiveShape::Diamond => vec![
            point(0.0, -half_height),
            point(half_width, 0.0),
            point(0.0, half_height),
            point(-half_width, 0.0),
        ],
        VisualPrimitiveShape::Circle => {
            let radius = transform.width.min(transform.height) / 2.0;
            ellipse_points(radius, radius)
        }
        VisualPrimitiveShape::Ellipse => ellipse_points(half_width, half_height),
        VisualPrimitiveShape::Cylinder => {
            let cap_height = (transform.height * 0.22).min(transform.width * 0.4);
            let cap_radius_y = cap_height / 2.0;
            let top_center_y = -half_height + cap_radius_y;
            let bottom_center_y = half_height - cap_radius_y;
            let mut points = Vec::with_capacity(2 * (ELLIPSE_STEPS / 2 + 1) + 2);
            for step in 0..=ELLIPSE_STEPS / 2 {
                let angle = std::f64::consts::PI
                    + std::f64::consts::PI * step as f64 / (ELLIPSE_STEPS / 2) as f64;
                points.push(point(
                    half_width * angle.cos(),
                    top_center_y + cap_radius_y * angle.sin(),
                ));
            }
            points.push(point(half_width, bottom_center_y));
            for step in 0..=ELLIPSE_STEPS / 2 {
                let angle = std::f64::consts::PI * step as f64 / (ELLIPSE_STEPS / 2) as f64;
                points.push(point(
                    half_width * angle.cos(),
                    bottom_center_y + cap_radius_y * angle.sin(),
                ));
            }
            let mut cap = ellipse_points(half_width, cap_radius_y)
                .into_iter()
                .map(|p| point(p.x, p.y + top_center_y))
                .collect::<Vec<_>>();
            if cap.len() > MAX_VISUAL_OUTLINE_POINTS {
                cap.truncate(MAX_VISUAL_OUTLINE_POINTS);
            }
            details.push(cap);
            points
        }
        VisualPrimitiveShape::Parallelogram => {
            let skew = (transform.width * 0.22).min(transform.height * 0.4);
            vec![
                point(-half_width + skew, -half_height),
                point(half_width, -half_height),
                point(half_width - skew, half_height),
                point(-half_width, half_height),
            ]
        }
        VisualPrimitiveShape::Triangle => vec![
            point(0.0, -half_height),
            point(half_width, half_height),
            point(-half_width, half_height),
        ],
    };
    let outline = local
        .into_iter()
        .map(|local| rotate_translate(local, transform))
        .collect();
    for path in &mut details {
        for point in path {
            *point = rotate_translate(*point, transform);
        }
    }
    (outline, details)
}

fn rounded_rectangle_points(half_width: f64, half_height: f64) -> Vec<ScenePoint> {
    let radius = half_width.min(half_height) * 0.16;
    let corners = [
        (
            half_width - radius,
            -half_height + radius,
            -std::f64::consts::FRAC_PI_2,
        ),
        (half_width - radius, half_height - radius, 0.0),
        (
            -half_width + radius,
            half_height - radius,
            std::f64::consts::FRAC_PI_2,
        ),
        (
            -half_width + radius,
            -half_height + radius,
            std::f64::consts::PI,
        ),
    ];
    let mut points = Vec::with_capacity(4 * (ROUND_CORNER_STEPS + 1));
    for (center_x, center_y, start) in corners {
        for step in 0..=ROUND_CORNER_STEPS {
            let angle =
                start + std::f64::consts::FRAC_PI_2 * step as f64 / ROUND_CORNER_STEPS as f64;
            points.push(point(
                center_x + radius * angle.cos(),
                center_y + radius * angle.sin(),
            ));
        }
    }
    points
}

fn ellipse_points(radius_x: f64, radius_y: f64) -> Vec<ScenePoint> {
    (0..ELLIPSE_STEPS)
        .map(|step| {
            let angle = std::f64::consts::TAU * step as f64 / ELLIPSE_STEPS as f64;
            point(radius_x * angle.cos(), radius_y * angle.sin())
        })
        .collect()
}

fn point(x: f64, y: f64) -> ScenePoint {
    ScenePoint { x, y }
}

fn rotate_translate(local: ScenePoint, transform: VisualTransform) -> ScenePoint {
    let (sin, cos) = transform.rotation_degrees.to_radians().sin_cos();
    ScenePoint {
        x: transform.x + local.x * cos - local.y * sin,
        y: transform.y + local.x * sin + local.y * cos,
    }
}

fn prepare_text(text: &str, transform: VisualTransform, font_size: f64) -> (PreparedText, bool) {
    let advance = (font_size * 0.62).max(1.0);
    let line_height = (font_size * 1.25).max(1.0);
    let max_chars = (transform.width / advance).floor().max(1.0) as usize;
    let max_lines = (transform.height / line_height)
        .floor()
        .max(1.0)
        .min(MAX_VISUAL_TEXT_LINES_PER_OBJECT as f64) as usize;
    let mut lines = Vec::with_capacity(max_lines.min(32));
    let mut was_truncated = false;

    'paragraphs: for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            if lines.len() == max_lines {
                was_truncated = true;
                break;
            }
            lines.push(String::new());
            continue;
        }
        let mut chars = paragraph.chars();
        loop {
            let line: String = chars.by_ref().take(max_chars).collect();
            if line.is_empty() {
                break;
            }
            if lines.len() == max_lines {
                was_truncated = true;
                break 'paragraphs;
            }
            lines.push(line);
            if chars.clone().next().is_none() {
                break;
            }
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    if was_truncated && let Some(last) = lines.last_mut() {
        let max_chars = max_chars.max(1);
        if last.chars().count() >= max_chars {
            let remove = last.char_indices().next_back().map(|(index, _)| index);
            if let Some(index) = remove {
                last.truncate(index);
            }
        }
        last.push('…');
    }
    let prepared_lines: Vec<_> = lines
        .into_iter()
        .map(|line| {
            let scalar_count = line.chars().count();
            PreparedTextLine {
                text: line,
                scalar_count,
                reserved_width: (scalar_count as f64 * advance).min(transform.width),
            }
        })
        .collect();
    let reserved_width = prepared_lines
        .iter()
        .map(|line| line.reserved_width)
        .fold(0.0_f64, f64::max);
    (
        PreparedText {
            height: (prepared_lines.len() as f64 * line_height).min(transform.height),
            lines: prepared_lines,
            reserved_width,
            line_height,
        },
        was_truncated,
    )
}

fn bounds_for_points(points: &[ScenePoint]) -> Option<SceneRect> {
    let first = *points.first()?;
    let (mut min_x, mut max_x) = (first.x, first.x);
    let (mut min_y, mut max_y) = (first.y, first.y);
    for point in points.iter().skip(1) {
        min_x = min_x.min(point.x);
        max_x = max_x.max(point.x);
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    }
    Some(SceneRect {
        min: ScenePoint { x: min_x, y: min_y },
        max: ScenePoint { x: max_x, y: max_y },
    })
}

fn rotated_rect_bounds(
    center: ScenePoint,
    width: f64,
    height: f64,
    rotation_degrees: f64,
) -> SceneRect {
    let transform = VisualTransform {
        x: center.x,
        y: center.y,
        width,
        height,
        rotation_degrees,
    };
    let corners = [
        point(-width / 2.0, -height / 2.0),
        point(width / 2.0, -height / 2.0),
        point(width / 2.0, height / 2.0),
        point(-width / 2.0, height / 2.0),
    ];
    bounds_for_points(
        &corners
            .into_iter()
            .map(|local| rotate_translate(local, transform))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_default()
}

fn shadow_bounds(outline: &[ScenePoint], shadow: VisualShadow) -> SceneRect {
    bounds_for_points(outline)
        .map(|bounds| {
            let offset = SceneRect {
                min: ScenePoint {
                    x: bounds.min.x + shadow.offset_x,
                    y: bounds.min.y + shadow.offset_y,
                },
                max: ScenePoint {
                    x: bounds.max.x + shadow.offset_x,
                    y: bounds.max.y + shadow.offset_y,
                },
            };
            expand(offset, shadow.blur)
        })
        .unwrap_or_default()
}

fn expand(bounds: SceneRect, margin: f64) -> SceneRect {
    SceneRect {
        min: ScenePoint {
            x: bounds.min.x - margin,
            y: bounds.min.y - margin,
        },
        max: ScenePoint {
            x: bounds.max.x + margin,
            y: bounds.max.y + margin,
        },
    }
}

fn union(left: SceneRect, right: SceneRect) -> SceneRect {
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

fn validate_projected_bounds(bounds: SceneRect) -> Result<(), VisualProjectionError> {
    let max = MAX_VISUAL_PROJECTION_COORDINATE_ABS;
    if !bounds.min.x.is_finite()
        || !bounds.min.y.is_finite()
        || !bounds.max.x.is_finite()
        || !bounds.max.y.is_finite()
        || bounds.min.x.abs() > max
        || bounds.min.y.abs() > max
        || bounds.max.x.abs() > max
        || bounds.max.y.abs() > max
    {
        return Err(VisualProjectionError::CoordinateLimitExceeded {
            max_coordinate_abs: max,
        });
    }
    Ok(())
}

fn project_dash_paths(
    item: &ProjectedVisualItem,
    max_segments: usize,
) -> Option<Vec<Vec<ProjectedStrokePrimitive>>> {
    let details_closed = matches!(
        item.kind,
        VisualProjectedKind::Primitive(VisualPrimitiveShape::Cylinder)
            | VisualProjectedKind::PresetPrimitive(VisualPrimitiveShape::Cylinder)
    );
    let mut remaining = max_segments;
    let mut paths = Vec::with_capacity(item.details.len() + 1);
    paths.push(project_dash_path(
        &item.outline,
        true,
        item.style.line_style,
        &mut remaining,
    )?);
    for detail in &item.details {
        paths.push(project_dash_path(
            detail,
            details_closed,
            item.style.line_style,
            &mut remaining,
        )?);
    }
    Some(paths)
}

fn project_dash_path(
    points: &[ScenePoint],
    closed: bool,
    style: super::visual::VisualLineStyle,
    remaining: &mut usize,
) -> Option<Vec<ProjectedStrokePrimitive>> {
    use super::visual::VisualLineStyle;

    let mut projected = Vec::new();
    if points.len() < 2 {
        return Some(projected);
    }
    let edge_count = points.len() - 1 + usize::from(closed && points.len() > 2);
    let total_length = path_length(points, closed);
    let mut travelled = 0.0;
    match style {
        VisualLineStyle::Solid => return Some(projected),
        VisualLineStyle::Dashed => {
            let period = DASH_LENGTH + DASH_GAP;
            for edge in 0..edge_count {
                let start = points[edge];
                let end = points[(edge + 1) % points.len()];
                let length = (end.x - start.x).hypot(end.y - start.y);
                if !length.is_finite() {
                    return None;
                }
                let mut offset = 0.0;
                while offset < length {
                    let phase = (travelled + offset) % period;
                    let (advance, visible) = if phase < DASH_LENGTH {
                        let run = (DASH_LENGTH - phase).min(length - offset);
                        (run, Some(run))
                    } else {
                        ((period - phase).min(length - offset), None)
                    };
                    if !advance.is_finite() || advance <= f64::EPSILON {
                        return None;
                    }
                    if let Some(run) = visible {
                        if *remaining == 0 {
                            return None;
                        }
                        let from = offset / length;
                        let to = (offset + run) / length;
                        projected.push(ProjectedStrokePrimitive::Segment {
                            start: interpolate(start, end, from),
                            end: interpolate(start, end, to),
                        });
                        *remaining -= 1;
                    }
                    offset += advance;
                }
                travelled += length;
            }
        }
        VisualLineStyle::Dotted => {
            let mut next_dot = 0.0;
            for edge in 0..edge_count {
                let start = points[edge];
                let end = points[(edge + 1) % points.len()];
                let length = (end.x - start.x).hypot(end.y - start.y);
                if !length.is_finite() {
                    return None;
                }
                while next_dot <= travelled + length + f64::EPSILON {
                    if closed && next_dot >= total_length - f64::EPSILON {
                        break;
                    }
                    let offset = (next_dot - travelled).clamp(0.0, length);
                    if *remaining == 0 {
                        return None;
                    }
                    projected.push(ProjectedStrokePrimitive::Dot {
                        center: interpolate(start, end, offset / length.max(f64::EPSILON)),
                    });
                    *remaining -= 1;
                    next_dot += DOT_SPACING;
                }
                travelled += length;
            }
        }
    }
    Some(projected)
}

fn interpolate(start: ScenePoint, end: ScenePoint, amount: f64) -> ScenePoint {
    ScenePoint {
        x: start.x + (end.x - start.x) * amount,
        y: start.y + (end.y - start.y) * amount,
    }
}

pub fn closed_path_length(points: &[ScenePoint]) -> f64 {
    path_length(points, true)
}

fn path_length(points: &[ScenePoint], close: bool) -> f64 {
    let mut length = points
        .windows(2)
        .map(|pair| (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y))
        .sum::<f64>();
    if close && points.len() > 2 {
        let first = points[0];
        let last = points[points.len() - 1];
        length += (first.x - last.x).hypot(first.y - last.y);
    }
    length
}

pub fn color_with_opacity(color: VisualColor, opacity: f64) -> [u8; 4] {
    let alpha = (f64::from(color.alpha) * opacity.clamp(0.0, 1.0)).round() as u8;
    [color.red, color.green, color.blue, alpha]
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_VISUAL_DASH_SEGMENTS_PER_ITEM, MAX_VISUAL_PROJECTED_ITEMS,
        MAX_VISUAL_PROJECTION_COORDINATE_ABS, VisualProjectedKind, project_visual_scene,
    };
    use crate::diagram::{
        DEFAULT_LAYER_ID, VisualAssetId, VisualColor, VisualDiagramDocument, VisualObjectDraft,
        VisualObjectId, VisualObjectKind, VisualOperation, VisualPresetDraft,
        VisualPrimitiveComponent, VisualPrimitiveShape, VisualShadow, VisualStyle, VisualTransform,
    };

    fn draft(
        transform: VisualTransform,
        style: VisualStyle,
        kind: VisualObjectKind,
    ) -> VisualObjectDraft {
        VisualObjectDraft {
            layer_id: DEFAULT_LAYER_ID,
            transform,
            style,
            kind,
        }
    }

    fn base_transform(width: f64, height: f64) -> VisualTransform {
        VisualTransform {
            width,
            height,
            ..VisualTransform::default()
        }
    }

    #[test]
    fn visual_projection_builds_all_outlines_with_true_circle_and_rotation() {
        let mut document = VisualDiagramDocument::new();
        let shapes = [
            VisualPrimitiveShape::Rectangle,
            VisualPrimitiveShape::RoundRect,
            VisualPrimitiveShape::Diamond,
            VisualPrimitiveShape::Circle,
            VisualPrimitiveShape::Ellipse,
            VisualPrimitiveShape::Cylinder,
            VisualPrimitiveShape::Parallelogram,
            VisualPrimitiveShape::Triangle,
        ];
        let operations: Vec<_> = shapes
            .into_iter()
            .map(|shape| {
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: 50.0,
                        y: -30.0,
                        width: 120.0,
                        height: 48.0,
                        rotation_degrees: 22.0,
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Primitive { shape },
                ))
            })
            .collect();
        document.apply_transaction(&operations).unwrap();
        let projection = project_visual_scene(document.scene()).unwrap();
        let shapes = projection
            .items
            .iter()
            .filter(|item| item.object_id != crate::diagram::TITLE_OBJECT_ID)
            .collect::<Vec<_>>();
        assert_eq!(shapes.len(), 8);
        assert!(shapes.iter().all(|item| item.outline.len() >= 3));
        assert!(shapes[5].details.iter().any(|path| path.len() == 32));
        let circle = shapes
            .iter()
            .find(|item| item.kind == VisualProjectedKind::Primitive(VisualPrimitiveShape::Circle))
            .unwrap();
        let circle_bounds = circle.bounds;
        assert!((circle_bounds.width() - circle_bounds.height()).abs() < 0.01);
        assert!(shapes[0].bounds.width() > 120.0);
    }

    #[test]
    fn visual_projection_expands_presets_in_stable_component_order_and_instance_space() {
        let mut document = VisualDiagramDocument::new();
        let preset = document
            .apply_transaction(&[VisualOperation::AddPreset(VisualPresetDraft {
                name: "Marker pair".into(),
                components: vec![
                    VisualPrimitiveComponent {
                        shape: VisualPrimitiveShape::Rectangle,
                        transform: VisualTransform {
                            x: -20.0,
                            y: 0.0,
                            width: 18.0,
                            height: 10.0,
                            rotation_degrees: 0.0,
                        },
                        style: VisualStyle::default(),
                    },
                    VisualPrimitiveComponent {
                        shape: VisualPrimitiveShape::Diamond,
                        transform: VisualTransform {
                            x: 20.0,
                            y: 0.0,
                            width: 12.0,
                            height: 12.0,
                            rotation_degrees: 15.0,
                        },
                        style: VisualStyle {
                            fill: VisualColor::rgba(1, 2, 3, 200),
                            ..VisualStyle::default()
                        },
                    },
                ],
            })])
            .unwrap()
            .created_presets[0];
        document
            .apply_transaction(&[VisualOperation::AddObject(draft(
                VisualTransform {
                    x: 300.0,
                    y: -50.0,
                    width: 200.0,
                    height: 100.0,
                    rotation_degrees: 90.0,
                },
                VisualStyle {
                    opacity: 0.5,
                    ..VisualStyle::default()
                },
                VisualObjectKind::PresetInstance { preset_id: preset },
            ))])
            .unwrap();
        let projection = project_visual_scene(document.scene()).unwrap();
        let parts: Vec<_> = projection
            .items
            .iter()
            .filter(|item| item.component_index.is_some())
            .collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].component_index, Some(0));
        assert_eq!(parts[1].component_index, Some(1));
        assert!((parts[0].transform.x - 300.0).abs() < 1e-9);
        // Design-space x offsets scale by 200 / 100, then rotate into world y.
        assert!((parts[0].transform.y + 90.0).abs() < 1e-9);
        assert!((parts[1].transform.y + 10.0).abs() < 1e-9);
        assert!((parts[1].style.opacity - 0.5).abs() < 1e-9);
    }

    #[test]
    fn visual_projection_composes_rotated_components_before_instance_scaling() {
        let mut document = VisualDiagramDocument::new();
        let preset = document
            .apply_transaction(&[VisualOperation::AddPreset(VisualPresetDraft {
                name: "Affine component".into(),
                components: vec![VisualPrimitiveComponent {
                    shape: VisualPrimitiveShape::Rectangle,
                    transform: VisualTransform {
                        width: 20.0,
                        height: 10.0,
                        rotation_degrees: 90.0,
                        ..VisualTransform::default()
                    },
                    style: VisualStyle {
                        border_width: 0.0,
                        ..VisualStyle::default()
                    },
                }],
            })])
            .unwrap()
            .created_presets[0];
        document
            .apply_transaction(&[VisualOperation::AddObject(draft(
                VisualTransform {
                    width: 200.0,
                    height: 100.0,
                    ..VisualTransform::default()
                },
                VisualStyle::default(),
                VisualObjectKind::PresetInstance { preset_id: preset },
            ))])
            .unwrap();
        let projection = project_visual_scene(document.scene()).unwrap();
        let component = projection
            .items
            .iter()
            .find(|item| item.component_index == Some(0))
            .unwrap();
        assert!((component.bounds.width() - 20.0).abs() < 0.01);
        assert!((component.bounds.height() - 20.0).abs() < 0.01);
        assert!(
            component
                .outline
                .iter()
                .all(|point| { point.x.abs() <= 10.0 + 1e-9 && point.y.abs() <= 10.0 + 1e-9 })
        );
    }

    #[test]
    fn visual_projection_materializes_shared_bounded_dash_and_dot_paths() {
        let mut document = VisualDiagramDocument::new();
        document
            .apply_transaction(&[
                VisualOperation::AddObject(draft(
                    base_transform(34.0, 20.0),
                    VisualStyle {
                        border_width: 0.0,
                        line_style: super::super::visual::VisualLineStyle::Dashed,
                        ..VisualStyle::default()
                    },
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle,
                    },
                )),
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: 100.0,
                        width: 60.0,
                        height: 40.0,
                        ..VisualTransform::default()
                    },
                    VisualStyle {
                        border_width: 0.0,
                        line_style: super::super::visual::VisualLineStyle::Dotted,
                        ..VisualStyle::default()
                    },
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Cylinder,
                    },
                )),
            ])
            .unwrap();

        let projection = project_visual_scene(document.scene()).unwrap();
        let rectangle = projection
            .items
            .iter()
            .find(|item| {
                item.kind == VisualProjectedKind::Primitive(VisualPrimitiveShape::Rectangle)
            })
            .unwrap();
        let rectangle_paths = rectangle.dash_paths.as_ref().unwrap();
        assert_eq!(rectangle_paths.len(), 1);
        assert!(!rectangle_paths[0].is_empty());
        assert!(rectangle_paths[0].iter().all(|stroke| match stroke {
            super::ProjectedStrokePrimitive::Segment { start, end } => {
                [start, end].iter().all(|point| {
                    (point.x.abs() - 17.0).abs() < 1e-8 || (point.y.abs() - 10.0).abs() < 1e-8
                })
            }
            super::ProjectedStrokePrimitive::Dot { .. } => false,
        }));

        let cylinder = projection
            .items
            .iter()
            .find(|item| {
                item.kind == VisualProjectedKind::Primitive(VisualPrimitiveShape::Cylinder)
            })
            .unwrap();
        let cylinder_paths = cylinder.dash_paths.as_ref().unwrap();
        assert_eq!(cylinder_paths.len(), 2);
        assert!(
            cylinder_paths[0]
                .iter()
                .all(|stroke| matches!(stroke, super::ProjectedStrokePrimitive::Dot { .. }))
        );
        assert!(
            cylinder_paths[1]
                .iter()
                .all(|stroke| matches!(stroke, super::ProjectedStrokePrimitive::Dot { .. }))
        );
        assert!(
            cylinder_paths.iter().map(Vec::len).sum::<usize>() <= MAX_VISUAL_DASH_SEGMENTS_PER_ITEM
        );
    }

    #[test]
    fn visual_projection_keeps_minimum_round_rect_geometry_finite_and_bounded() {
        let mut document = VisualDiagramDocument::new();
        document
            .apply_transaction(&[VisualOperation::AddObject(draft(
                base_transform(0.1, 0.1),
                VisualStyle::default(),
                VisualObjectKind::Primitive {
                    shape: VisualPrimitiveShape::RoundRect,
                },
            ))])
            .unwrap();
        let projection = project_visual_scene(document.scene()).unwrap();
        let round_rect = projection
            .items
            .iter()
            .find(|item| {
                item.kind == VisualProjectedKind::Primitive(VisualPrimitiveShape::RoundRect)
            })
            .unwrap();
        assert!(round_rect.outline.len() >= 4);
        assert!(round_rect.outline.iter().all(|point| {
            point.x.is_finite()
                && point.y.is_finite()
                && point.x.abs() <= 0.05
                && point.y.abs() <= 0.05
        }));
    }

    #[test]
    fn visual_projection_bounds_rotated_text_stroke_shadow_and_missing_assets() {
        let mut document = VisualDiagramDocument::new();
        let mut style = VisualStyle {
            border_width: 8.0,
            shadow: Some(VisualShadow {
                offset_x: -25.0,
                offset_y: -15.0,
                blur: 6.0,
                color: VisualColor::rgba(0, 0, 0, 80),
            }),
            ..VisualStyle::default()
        };
        style.font_size = 15.5;
        let mut text = draft(
            VisualTransform {
                x: -100.0,
                y: -80.0,
                width: 75.0,
                height: 60.0,
                rotation_degrees: 45.0,
            },
            style,
            VisualObjectKind::Text {
                text: "This text wraps into several lines around the rotated center".into(),
            },
        );
        text.layer_id = DEFAULT_LAYER_ID;
        document
            .apply_transaction(&[VisualOperation::AddObject(text)])
            .unwrap();
        document
            .apply_transaction(&[VisualOperation::AddObject(draft(
                base_transform(50.0, 25.0),
                VisualStyle::default(),
                VisualObjectKind::ImageReference {
                    asset_id: VisualAssetId::new("unloaded-image").unwrap(),
                },
            ))])
            .unwrap();
        let projection = project_visual_scene(document.scene()).unwrap();
        let text_item = projection.items.last().unwrap();
        assert_eq!(text_item.kind, VisualProjectedKind::MissingImagePlaceholder);
        assert_eq!(text_item.details.len(), 2);
        let rotated_text = &projection.items[1];
        assert!(rotated_text.text.as_ref().unwrap().lines.len() > 1);
        assert!(rotated_text.bounds.min.x < -100.0);
        assert!(rotated_text.bounds.min.y < -100.0);
        assert_eq!(
            projection.text_lines,
            rotated_text.text.as_ref().unwrap().lines.len() + 2
        );
    }

    #[test]
    fn visual_projection_bounds_dash_and_expansion_work_for_large_scenes() {
        let mut document = VisualDiagramDocument::new();
        let operations: Vec<_> = (0..64)
            .map(|_| {
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        width: 5_000.0,
                        height: 5_000.0,
                        ..VisualTransform::default()
                    },
                    VisualStyle {
                        line_style: super::super::visual::VisualLineStyle::Dashed,
                        ..VisualStyle::default()
                    },
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Ellipse,
                    },
                ))
            })
            .collect();
        document.apply_transaction(&operations).unwrap();
        let projection = project_visual_scene(document.scene()).unwrap();
        assert_eq!(projection.items.len(), 65);
        assert!(projection.simplified_dash_paths > 0);
        assert!(projection.simplified_dash_paths <= 64);
        assert!(projection.items[1].dash_paths.is_none());
        assert!(MAX_VISUAL_DASH_SEGMENTS_PER_ITEM > 0);
        assert!(MAX_VISUAL_PROJECTED_ITEMS >= projection.items.len());

        let mut huge = VisualDiagramDocument::new();
        let huge_preset = huge
            .apply_transaction(&[VisualOperation::AddPreset(VisualPresetDraft {
                name: "Far component".into(),
                components: vec![VisualPrimitiveComponent {
                    shape: VisualPrimitiveShape::Rectangle,
                    transform: VisualTransform {
                        x: 1_000_000.0,
                        y: 0.0,
                        width: 1.0,
                        height: 1.0,
                        rotation_degrees: 0.0,
                    },
                    style: VisualStyle::default(),
                }],
            })])
            .unwrap()
            .created_presets[0];
        huge.apply_transaction(&[VisualOperation::AddObject(draft(
            VisualTransform {
                x: 0.0,
                y: 0.0,
                width: 1_000_000.0,
                height: 1_000_000.0,
                rotation_degrees: 0.0,
            },
            VisualStyle::default(),
            VisualObjectKind::PresetInstance {
                preset_id: huge_preset,
            },
        ))])
        .unwrap();
        assert!(project_visual_scene(huge.scene()).is_err());
    }
}
