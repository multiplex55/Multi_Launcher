//! Bounded renderer-independent geometry prepared from a validated diagram.

use super::model::{
    DiagramDocument, DiagramEdge, EdgeId, FlowDirection, GraphValidationError, GroupId, NodeId,
    NodeShape,
};
use std::collections::HashMap;
use std::fmt;

pub const MAX_SCENE_COORDINATE_ABS: f64 = 1_000_000_000.0;
const EPSILON: f64 = 1.0e-9;
const ROUNDING_STEPS: usize = 48;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScenePoint {
    pub x: f64,
    pub y: f64,
}

impl ScenePoint {
    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }

    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }

    fn scale(self, factor: f64) -> Self {
        Self {
            x: self.x * factor,
            y: self.y * factor,
        }
    }

    fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    fn normalized_or(self, fallback: Self) -> Self {
        let length = self.length();
        if length > EPSILON && length.is_finite() {
            self.scale(1.0 / length)
        } else {
            fallback
        }
    }

    fn perpendicular(self) -> Self {
        Self {
            x: -self.y,
            y: self.x,
        }
    }

    fn lerp(self, other: Self, factor: f64) -> Self {
        self.add(other.sub(self).scale(factor))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneRect {
    pub min: ScenePoint,
    pub max: ScenePoint,
}

impl SceneRect {
    fn from_min_size(min: ScenePoint, width: f64, height: f64) -> Self {
        Self {
            min,
            max: ScenePoint {
                x: min.x + width,
                y: min.y + height,
            },
        }
    }

    fn from_center(center: ScenePoint, width: f64, height: f64) -> Self {
        let half_width = width / 2.0;
        let half_height = height / 2.0;
        Self {
            min: ScenePoint {
                x: center.x - half_width,
                y: center.y - half_height,
            },
            max: ScenePoint {
                x: center.x + half_width,
                y: center.y + half_height,
            },
        }
    }

    fn include_point(&mut self, point: ScenePoint) {
        self.min.x = self.min.x.min(point.x);
        self.min.y = self.min.y.min(point.y);
        self.max.x = self.max.x.max(point.x);
        self.max.y = self.max.y.max(point.y);
    }

    fn union(self, other: Self) -> Self {
        Self {
            min: ScenePoint {
                x: self.min.x.min(other.min.x),
                y: self.min.y.min(other.min.y),
            },
            max: ScenePoint {
                x: self.max.x.max(other.max.x),
                y: self.max.y.max(other.max.y),
            },
        }
    }

    pub fn width(self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(self) -> f64 {
        self.max.y - self.min.y
    }

    pub fn center(self) -> ScenePoint {
        ScenePoint {
            x: (self.min.x + self.max.x) / 2.0,
            y: (self.min.y + self.max.y) / 2.0,
        }
    }

    pub fn contains_point(self, point: ScenePoint) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }

    pub fn contains_rect(self, other: Self) -> bool {
        self.min.x <= other.min.x
            && self.min.y <= other.min.y
            && self.max.x >= other.max.x
            && self.max.y >= other.max.y
    }
}

/// Portable fixed-advance text metrics. Width is measured per Unicode scalar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneTextMetrics {
    /// Maximum horizontal advance reserved for each Unicode scalar.
    pub max_scalar_advance: f64,
    pub line_height: f64,
    pub wrap_width: f64,
}

impl Default for SceneTextMetrics {
    fn default() -> Self {
        Self {
            max_scalar_advance: 8.0,
            line_height: 16.0,
            wrap_width: 320.0,
        }
    }
}

/// Explicit geometry and text layout inputs used for a scene build.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneMetrics {
    pub text: SceneTextMetrics,
    pub node_padding_x: f64,
    pub node_padding_y: f64,
    pub minimum_node_width: f64,
    pub minimum_node_height: f64,
    pub rounded_corner_radius: f64,
    pub group_padding: f64,
    pub group_header_gap: f64,
    pub parallel_edge_spacing: f64,
    pub self_edge_spacing: f64,
    pub edge_label_gap: f64,
    pub arrowhead_length: f64,
    pub arrowhead_half_width: f64,
}

impl Default for SceneMetrics {
    fn default() -> Self {
        Self {
            text: SceneTextMetrics::default(),
            node_padding_x: 16.0,
            node_padding_y: 10.0,
            minimum_node_width: 64.0,
            minimum_node_height: 36.0,
            rounded_corner_radius: 12.0,
            group_padding: 24.0,
            group_header_gap: 8.0,
            parallel_edge_spacing: 28.0,
            self_edge_spacing: 40.0,
            edge_label_gap: 12.0,
            arrowhead_length: 12.0,
            arrowhead_half_width: 5.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedTextLine {
    pub text: String,
    pub scalar_count: usize,
    /// Reserved width for this line; renderers must fit or clip glyph ink to it.
    pub reserved_width: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedText {
    pub lines: Vec<PreparedTextLine>,
    pub reserved_width: f64,
    pub height: f64,
    pub line_height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextHorizontalAlignment {
    Center,
    Leading,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneNode {
    pub colors: super::style::EffectiveMermaidColors,
    pub id: NodeId,
    pub shape: NodeShape,
    pub bounds: SceneRect,
    pub label: PreparedText,
    pub label_bounds: SceneRect,
    pub label_alignment: TextHorizontalAlignment,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneGroup {
    pub id: GroupId,
    pub bounds: SceneRect,
    pub label: PreparedText,
    pub label_bounds: SceneRect,
    pub label_alignment: TextHorizontalAlignment,
    pub members: Vec<NodeId>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    pub start: ScenePoint,
    pub control1: ScenePoint,
    pub control2: ScenePoint,
    pub end: ScenePoint,
}

impl CubicBezier {
    fn point_at(self, t: f64) -> ScenePoint {
        let inverse = 1.0 - t;
        self.start
            .scale(inverse * inverse * inverse)
            .add(self.control1.scale(3.0 * inverse * inverse * t))
            .add(self.control2.scale(3.0 * inverse * t * t))
            .add(self.end.scale(t * t * t))
    }

    fn tangent_at(self, t: f64) -> ScenePoint {
        let inverse = 1.0 - t;
        self.control1
            .sub(self.start)
            .scale(3.0 * inverse * inverse)
            .add(self.control2.sub(self.control1).scale(6.0 * inverse * t))
            .add(self.end.sub(self.control2).scale(3.0 * t * t))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArrowHead {
    pub tip: ScenePoint,
    pub left: ScenePoint,
    pub right: ScenePoint,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneEdge {
    pub id: EdgeId,
    pub from: NodeId,
    pub to: NodeId,
    pub parallel_index: usize,
    pub path: CubicBezier,
    pub arrowhead: ArrowHead,
    pub label: Option<PreparedText>,
    pub label_bounds: Option<SceneRect>,
    pub label_alignment: Option<TextHorizontalAlignment>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiagramScene {
    pub colors: super::style::EffectiveMermaidColors,
    pub direction: FlowDirection,
    pub metrics: SceneMetrics,
    pub nodes: Vec<SceneNode>,
    pub groups: Vec<SceneGroup>,
    pub edges: Vec<SceneEdge>,
    pub bounds: Option<SceneRect>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SceneError {
    InvalidGraph(GraphValidationError),
    InvalidMetrics,
    SceneExtentExceeded { max_coordinate_abs: f64 },
}

impl fmt::Display for SceneError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGraph(error) => error.fmt(formatter),
            Self::InvalidMetrics => formatter.write_str("scene metrics are invalid or unbounded"),
            Self::SceneExtentExceeded { max_coordinate_abs } => write!(
                formatter,
                "scene geometry exceeds the {max_coordinate_abs}-unit coordinate limit"
            ),
        }
    }
}

impl std::error::Error for SceneError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidGraph(error) => Some(error),
            Self::InvalidMetrics | Self::SceneExtentExceeded { .. } => None,
        }
    }
}

/// Build stable, bounded geometry from a validated renderer-independent graph.
pub fn build_scene(
    document: &DiagramDocument,
    metrics: SceneMetrics,
) -> Result<DiagramScene, SceneError> {
    document.validate().map_err(SceneError::InvalidGraph)?;
    validate_metrics(metrics)?;

    let colors = document.style.effective();
    let mut nodes = Vec::with_capacity(document.nodes.len());
    let mut bounds = None;
    let mut node_bounds = HashMap::with_capacity(document.nodes.len());
    for node in &document.nodes {
        let label = prepare_text(&node.label, metrics.text);
        let mut width =
            (label.reserved_width + metrics.node_padding_x * 2.0).max(metrics.minimum_node_width);
        let mut height =
            (label.height + metrics.node_padding_y * 2.0).max(metrics.minimum_node_height);
        if node.shape == NodeShape::Diamond {
            width = width.max((label.reserved_width + metrics.node_padding_x * 2.0) * 2.0);
            height = height.max((label.height + metrics.node_padding_y * 2.0) * 2.0);
        } else if node.shape == NodeShape::Rounded {
            width = width.max(label.reserved_width + metrics.rounded_corner_radius * 2.0);
            height = height.max(label.height + metrics.rounded_corner_radius * 2.0);
        }
        let center = ScenePoint {
            x: f64::from(node.position.x),
            y: f64::from(node.position.y),
        };
        let node_rect = SceneRect::from_center(center, width, height);
        let label_rect = SceneRect::from_center(center, label.reserved_width, label.height);
        include_rect(&mut bounds, node_rect);
        node_bounds.insert(node.id.clone(), node_rect);
        nodes.push(SceneNode {
            colors: document.node_styles.get(&node.id).map_or(colors, |style| style.effective(colors)),
            id: node.id.clone(),
            shape: node.shape,
            bounds: node_rect,
            label,
            label_bounds: label_rect,
            label_alignment: TextHorizontalAlignment::Center,
        });
    }

    let groups = build_groups(document, metrics, &node_bounds, &mut bounds);
    let edges = build_edges(document, metrics, &nodes, &mut bounds)?;
    ensure_scene_bounds(bounds)?;

    Ok(DiagramScene {
        colors,
        direction: document.direction,
        metrics,
        nodes,
        groups,
        edges,
        bounds,
    })
}

fn validate_metrics(metrics: SceneMetrics) -> Result<(), SceneError> {
    let positive =
        |value: f64, min: f64, max: f64| value.is_finite() && value >= min && value <= max;
    let nonnegative = |value: f64, max: f64| value.is_finite() && value >= 0.0 && value <= max;
    if !positive(metrics.text.max_scalar_advance, 0.01, 256.0)
        || !positive(metrics.text.line_height, 0.01, 1_024.0)
        || !positive(metrics.text.wrap_width, 0.01, 16_384.0)
        || metrics.text.wrap_width < metrics.text.max_scalar_advance
        || !nonnegative(metrics.node_padding_x, 8_192.0)
        || !nonnegative(metrics.node_padding_y, 8_192.0)
        || !positive(metrics.minimum_node_width, 0.01, 32_768.0)
        || !positive(metrics.minimum_node_height, 0.01, 32_768.0)
        || !nonnegative(metrics.rounded_corner_radius, 4_096.0)
        || !nonnegative(metrics.group_padding, 8_192.0)
        || !nonnegative(metrics.group_header_gap, 8_192.0)
        || !positive(metrics.parallel_edge_spacing, 0.01, 8_192.0)
        || !positive(metrics.self_edge_spacing, 0.01, 8_192.0)
        || !nonnegative(metrics.edge_label_gap, 8_192.0)
        || !positive(metrics.arrowhead_length, 0.01, 1_024.0)
        || !positive(metrics.arrowhead_half_width, 0.01, 1_024.0)
    {
        return Err(SceneError::InvalidMetrics);
    }
    Ok(())
}

fn prepare_text(text: &str, metrics: SceneTextMetrics) -> PreparedText {
    let max_scalars = (metrics.wrap_width / metrics.max_scalar_advance)
        .floor()
        .max(1.0) as usize;
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(PreparedTextLine {
                text: String::new(),
                scalar_count: 0,
                reserved_width: 0.0,
            });
            continue;
        }
        let mut line = String::new();
        let mut scalar_count = 0usize;
        for scalar in paragraph.chars() {
            if scalar_count == max_scalars {
                lines.push(PreparedTextLine {
                    text: std::mem::take(&mut line),
                    scalar_count,
                    reserved_width: scalar_count as f64 * metrics.max_scalar_advance,
                });
                scalar_count = 0;
            }
            line.push(scalar);
            scalar_count += 1;
        }
        lines.push(PreparedTextLine {
            text: line,
            scalar_count,
            reserved_width: scalar_count as f64 * metrics.max_scalar_advance,
        });
    }
    let reserved_width = lines
        .iter()
        .map(|line| line.reserved_width)
        .fold(0.0, f64::max);
    PreparedText {
        height: lines.len() as f64 * metrics.line_height,
        lines,
        reserved_width,
        line_height: metrics.line_height,
    }
}

fn build_groups(
    document: &DiagramDocument,
    metrics: SceneMetrics,
    node_bounds: &HashMap<NodeId, SceneRect>,
    total_bounds: &mut Option<SceneRect>,
) -> Vec<SceneGroup> {
    let node_union = node_bounds.values().copied().reduce(SceneRect::union);
    let mut groups = Vec::with_capacity(document.groups.len());
    let mut empty_group_y = node_union.map_or(0.0, |rect| rect.max.y + metrics.group_padding);
    for group in &document.groups {
        let label = prepare_text(&group.label, metrics.text);
        let members = document
            .nodes
            .iter()
            .filter(|node| node.group.as_ref() == Some(&group.id))
            .map(|node| node.id.clone())
            .collect::<Vec<_>>();
        let member_bounds = members
            .iter()
            .filter_map(|id| node_bounds.get(id))
            .copied()
            .reduce(SceneRect::union);

        let (bounds, label_bounds) = if let Some(member_bounds) = member_bounds {
            let min_x = member_bounds.min.x - metrics.group_padding;
            let min_y = member_bounds.min.y
                - metrics.group_padding
                - metrics.group_header_gap
                - label.height;
            let max_x = (member_bounds.max.x + metrics.group_padding)
                .max(min_x + metrics.group_padding + label.reserved_width + metrics.group_padding);
            let max_y = member_bounds.max.y + metrics.group_padding;
            let group_bounds = SceneRect {
                min: ScenePoint { x: min_x, y: min_y },
                max: ScenePoint { x: max_x, y: max_y },
            };
            let label_bounds = SceneRect::from_min_size(
                ScenePoint {
                    x: min_x + metrics.group_padding,
                    y: min_y + metrics.group_padding,
                },
                label.reserved_width,
                label.height,
            );
            (group_bounds, label_bounds)
        } else {
            let min_x = node_union.map_or(0.0, |rect| rect.min.x);
            let label_bounds = SceneRect::from_min_size(
                ScenePoint {
                    x: min_x + metrics.group_padding,
                    y: empty_group_y + metrics.group_padding,
                },
                label.reserved_width,
                label.height,
            );
            let group_bounds = SceneRect {
                min: ScenePoint {
                    x: min_x,
                    y: empty_group_y,
                },
                max: ScenePoint {
                    x: label_bounds.max.x + metrics.group_padding,
                    y: label_bounds.max.y + metrics.group_padding,
                },
            };
            empty_group_y = group_bounds.max.y + metrics.group_header_gap;
            (group_bounds, label_bounds)
        };
        include_rect(total_bounds, bounds);
        groups.push(SceneGroup {
            id: group.id.clone(),
            bounds,
            label,
            label_bounds,
            label_alignment: TextHorizontalAlignment::Leading,
            members,
        });
    }
    groups
}

fn build_edges(
    document: &DiagramDocument,
    metrics: SceneMetrics,
    nodes: &[SceneNode],
    total_bounds: &mut Option<SceneRect>,
) -> Result<Vec<SceneEdge>, SceneError> {
    let nodes_by_id: HashMap<NodeId, &SceneNode> =
        nodes.iter().map(|node| (node.id.clone(), node)).collect();
    let mut parallel_groups: HashMap<(NodeId, NodeId), Vec<&DiagramEdge>> = HashMap::new();
    for edge in &document.edges {
        parallel_groups
            .entry((edge.from.clone(), edge.to.clone()))
            .or_default()
            .push(edge);
    }
    let mut edge_order = HashMap::with_capacity(document.edges.len());
    for edges in parallel_groups.values_mut() {
        edges.sort_by_key(|edge| edge.id);
        let centered_offset = (edges.len().saturating_sub(1)) as f64 / 2.0;
        for (index, edge) in edges.iter().enumerate() {
            edge_order.insert(edge.id, (index as f64 - centered_offset, index));
        }
    }

    let mut result = Vec::with_capacity(document.edges.len());
    for edge in &document.edges {
        let source = nodes_by_id.get(&edge.from).copied().ok_or_else(|| {
            SceneError::InvalidGraph(GraphValidationError::MissingNode(edge.from.clone()))
        })?;
        let target = nodes_by_id.get(&edge.to).copied().ok_or_else(|| {
            SceneError::InvalidGraph(GraphValidationError::MissingNode(edge.to.clone()))
        })?;
        let (lane, parallel_index) = edge_order[&edge.id];
        let path = edge_path(edge, source, target, lane, parallel_index, metrics);
        let arrowhead = arrowhead(path, metrics);
        let (label, label_bounds, label_alignment) = if let Some(text) = &edge.label {
            let label = prepare_text(text, metrics.text);
            let midpoint = path.point_at(0.5);
            let tangent = path
                .tangent_at(0.5)
                .normalized_or(ScenePoint { x: 1.0, y: 0.0 });
            let center = midpoint.add(tangent.perpendicular().scale(metrics.edge_label_gap));
            let label_bounds = SceneRect::from_center(center, label.reserved_width, label.height);
            (
                Some(label),
                Some(label_bounds),
                Some(TextHorizontalAlignment::Center),
            )
        } else {
            (None, None, None)
        };
        let edge_bounds = edge_bounds(path, arrowhead, label_bounds);
        include_rect(total_bounds, edge_bounds);
        result.push(SceneEdge {
            id: edge.id,
            from: edge.from.clone(),
            to: edge.to.clone(),
            parallel_index,
            path,
            arrowhead,
            label,
            label_bounds,
            label_alignment,
        });
    }
    Ok(result)
}

fn edge_path(
    edge: &DiagramEdge,
    source: &SceneNode,
    target: &SceneNode,
    lane: f64,
    parallel_index: usize,
    metrics: SceneMetrics,
) -> CubicBezier {
    let source_center = source.bounds.center();
    let target_center = target.bounds.center();
    if edge.from == edge.to {
        return self_loop_path(source, parallel_index, metrics);
    }

    let center_delta = target_center.sub(source_center);
    let center_distance = center_delta.length();
    if center_distance <= EPSILON {
        let axis = ScenePoint { x: 1.0, y: 0.0 };
        let start = shape_boundary(source, axis, metrics);
        let end = shape_boundary(target, axis.scale(-1.0), metrics);
        let maximum_half_extent = (source.bounds.width() / 2.0)
            .max(source.bounds.height() / 2.0)
            .max(target.bounds.width() / 2.0)
            .max(target.bounds.height() / 2.0);
        let clearance = maximum_half_extent
            + metrics.self_edge_spacing
            + lane.abs() * metrics.parallel_edge_spacing;
        let control_clearance = clearance * 4.0 / 3.0;
        let side = if lane < 0.0 { -1.0 } else { 1.0 };
        let control1 = start.add(ScenePoint {
            x: clearance,
            y: side * control_clearance,
        });
        let control2 = end.add(ScenePoint {
            x: -clearance,
            y: side * control_clearance,
        });
        return CubicBezier {
            start,
            control1,
            control2,
            end,
        };
    }

    let direction = center_delta.scale(1.0 / center_distance);
    let normal = direction.perpendicular();
    let control = source_center
        .lerp(target_center, 0.5)
        .add(normal.scale(lane * metrics.parallel_edge_spacing));
    let start_direction = control.sub(source_center).normalized_or(direction);
    let end_direction = control
        .sub(target_center)
        .normalized_or(direction.scale(-1.0));
    let start = shape_boundary(source, start_direction, metrics);
    let end = shape_boundary(target, end_direction, metrics);
    CubicBezier {
        start,
        control1: start.lerp(control, 2.0 / 3.0),
        control2: end.lerp(control, 2.0 / 3.0),
        end,
    }
}

fn self_loop_path(node: &SceneNode, parallel_index: usize, metrics: SceneMetrics) -> CubicBezier {
    let center = node.bounds.center();
    let half_width = node.bounds.width() / 2.0;
    let half_height = node.bounds.height() / 2.0;
    let start = shape_boundary(node, ScenePoint { x: 1.0, y: -0.5 }, metrics);
    let end = shape_boundary(node, ScenePoint { x: 0.5, y: -1.0 }, metrics);
    let clearance = half_width.max(half_height)
        + metrics.self_edge_spacing
        + parallel_index as f64 * metrics.parallel_edge_spacing;
    CubicBezier {
        start,
        control1: ScenePoint {
            x: center.x + half_width + clearance,
            y: center.y - half_height - clearance * 0.5,
        },
        control2: ScenePoint {
            x: center.x + half_width + clearance * 0.5,
            y: center.y - half_height - clearance,
        },
        end,
    }
}

fn shape_boundary(node: &SceneNode, direction: ScenePoint, metrics: SceneMetrics) -> ScenePoint {
    let center = node.bounds.center();
    let half_width = node.bounds.width() / 2.0;
    let half_height = node.bounds.height() / 2.0;
    let direction = direction.normalized_or(ScenePoint { x: 1.0, y: 0.0 });
    let distance = match node.shape {
        NodeShape::Rectangle => {
            let x_distance = if direction.x.abs() <= EPSILON {
                f64::INFINITY
            } else {
                half_width / direction.x.abs()
            };
            let y_distance = if direction.y.abs() <= EPSILON {
                f64::INFINITY
            } else {
                half_height / direction.y.abs()
            };
            x_distance.min(y_distance)
        }
        NodeShape::Diamond => {
            1.0 / (direction.x.abs() / half_width + direction.y.abs() / half_height)
        }
        NodeShape::Rounded => rounded_rect_ray_distance(
            direction,
            half_width,
            half_height,
            metrics
                .rounded_corner_radius
                .min(half_width)
                .min(half_height),
        ),
    };
    center.add(direction.scale(distance))
}

fn rounded_rect_ray_distance(
    direction: ScenePoint,
    half_width: f64,
    half_height: f64,
    radius: f64,
) -> f64 {
    let x_exit = if direction.x.abs() <= EPSILON {
        f64::INFINITY
    } else {
        half_width / direction.x.abs()
    };
    let y_exit = if direction.y.abs() <= EPSILON {
        f64::INFINITY
    } else {
        half_height / direction.y.abs()
    };
    let mut low = 0.0;
    let rectangle_exit = x_exit.min(y_exit);
    let mut high = rectangle_exit + rectangle_exit.abs().max(1.0) * 1.0e-9;
    for _ in 0..ROUNDING_STEPS {
        let middle = (low + high) / 2.0;
        let point = direction.scale(middle);
        if inside_rounded_rect(point, half_width, half_height, radius) {
            low = middle;
        } else {
            high = middle;
        }
    }
    (low + high) / 2.0
}

fn inside_rounded_rect(point: ScenePoint, half_width: f64, half_height: f64, radius: f64) -> bool {
    let corner_x = (point.x.abs() - (half_width - radius)).max(0.0);
    let corner_y = (point.y.abs() - (half_height - radius)).max(0.0);
    corner_x * corner_x + corner_y * corner_y <= radius * radius
}

fn arrowhead(path: CubicBezier, metrics: SceneMetrics) -> ArrowHead {
    let direction = path.end.sub(path.control2).normalized_or(
        path.end
            .sub(path.start)
            .normalized_or(ScenePoint { x: 1.0, y: 0.0 }),
    );
    let base = path.end.sub(direction.scale(metrics.arrowhead_length));
    let half_width = direction
        .perpendicular()
        .scale(metrics.arrowhead_half_width);
    ArrowHead {
        tip: path.end,
        left: base.add(half_width),
        right: base.sub(half_width),
    }
}

fn edge_bounds(
    path: CubicBezier,
    arrowhead: ArrowHead,
    label_bounds: Option<SceneRect>,
) -> SceneRect {
    let mut bounds = SceneRect {
        min: path.start,
        max: path.start,
    };
    for point in [
        path.control1,
        path.control2,
        path.end,
        arrowhead.left,
        arrowhead.right,
    ] {
        bounds.include_point(point);
    }
    if let Some(label_bounds) = label_bounds {
        bounds = bounds.union(label_bounds);
    }
    bounds
}

fn include_rect(bounds: &mut Option<SceneRect>, rect: SceneRect) {
    *bounds = Some((*bounds).map_or(rect, |current| current.union(rect)));
}

fn ensure_scene_bounds(bounds: Option<SceneRect>) -> Result<(), SceneError> {
    let Some(bounds) = bounds else {
        return Ok(());
    };
    let coordinates = [bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y];
    if coordinates
        .into_iter()
        .any(|coordinate| !coordinate.is_finite() || coordinate.abs() > MAX_SCENE_COORDINATE_ABS)
    {
        return Err(SceneError::SceneExtentExceeded {
            max_coordinate_abs: MAX_SCENE_COORDINATE_ABS,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DiagramScene, SceneError, SceneMetrics, ScenePoint, SceneTextMetrics,
        TextHorizontalAlignment, build_scene,
    };
    use crate::diagram::{
        DiagramDocument, FlowDirection, MAX_DIAGRAM_EDGES, MAX_DIAGRAM_GROUPS, MAX_DIAGRAM_NODES,
        NodeId, NodeShape, Position, parse_mermaid,
    };

    fn scene_from_source(source: &str, metrics: SceneMetrics) -> (DiagramDocument, DiagramScene) {
        let document = parse_mermaid(source).unwrap();
        let scene = build_scene(&document, metrics).unwrap();
        (document, scene)
    }

    fn all_scene_points(scene: &DiagramScene) -> Vec<ScenePoint> {
        let mut points = Vec::new();
        for node in &scene.nodes {
            points.extend([
                node.bounds.min,
                node.bounds.max,
                node.label_bounds.min,
                node.label_bounds.max,
            ]);
        }
        for group in &scene.groups {
            points.extend([
                group.bounds.min,
                group.bounds.max,
                group.label_bounds.min,
                group.label_bounds.max,
            ]);
        }
        for edge in &scene.edges {
            points.extend([
                edge.path.start,
                edge.path.control1,
                edge.path.control2,
                edge.path.end,
                edge.arrowhead.tip,
                edge.arrowhead.left,
                edge.arrowhead.right,
            ]);
            if let Some(label_bounds) = edge.label_bounds {
                points.extend([label_bounds.min, label_bounds.max]);
            }
        }
        points
    }

    #[test]
    fn diagram_scene_prepares_bounded_text_shapes_and_group_bounds() {
        let metrics = SceneMetrics {
            text: SceneTextMetrics {
                max_scalar_advance: 8.0,
                line_height: 12.0,
                wrap_width: 32.0,
            },
            rounded_corner_radius: 32.0,
            ..SceneMetrics::default()
        };
        let (mut document, _) = scene_from_source(
            "flowchart TD\nA[\"alpha βeta 🧭\"]\nB(\"rounded\")\nsubgraph G[\"Group title\"]\nC{\"member\"}\nend\nsubgraph Empty[\"Vacant\"]\nend\n",
            metrics,
        );
        document.nodes[0].label = format!("alpha βeta 🧭\n{}", "界".repeat(80));
        document.nodes[0].shape = NodeShape::Rounded;
        document.nodes[1].shape = NodeShape::Diamond;
        document.nodes[0].position = Position::new(-200.0, 0.0).unwrap();
        document.nodes[1].position = Position::new(100.0, 0.0).unwrap();
        document.nodes[2].position = Position::new(100.0, 250.0).unwrap();
        let scene = build_scene(&document, metrics).unwrap();

        assert_eq!(scene.nodes[0].shape, NodeShape::Rounded);
        assert_eq!(scene.nodes[1].shape, NodeShape::Diamond);
        assert_eq!(scene.nodes[0].label.lines.len(), 23);
        assert!(
            scene.nodes[0].label.lines[3]
                .text
                .chars()
                .all(|scalar| scalar == '界')
        );
        assert_eq!(
            scene.nodes[0]
                .label
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .concat(),
            document.nodes[0].label.replace('\n', "")
        );
        assert!(
            scene.nodes[0].label.lines.iter().all(
                |line| line.scalar_count <= 4 && line.text.chars().count() == line.scalar_count
            )
        );
        assert!(scene.nodes[0].label.reserved_width <= metrics.text.wrap_width);
        assert!(
            scene.nodes[0]
                .label
                .lines
                .iter()
                .all(|line| line.reserved_width <= metrics.text.wrap_width)
        );
        assert_eq!(scene.nodes[0].label.line_height, 12.0);
        assert_eq!(
            scene.nodes[0].label_bounds.center(),
            scene.nodes[0].bounds.center()
        );
        assert_eq!(
            scene.nodes[0].label_alignment,
            TextHorizontalAlignment::Center
        );
        let rounded = &scene.nodes[0];
        let rounded_center = rounded.bounds.center();
        let rounded_half_width = rounded.bounds.width() / 2.0;
        let rounded_half_height = rounded.bounds.height() / 2.0;
        let rounded_radius = metrics
            .rounded_corner_radius
            .min(rounded_half_width)
            .min(rounded_half_height);
        for corner in [
            rounded.label_bounds.min,
            ScenePoint {
                x: rounded.label_bounds.min.x,
                y: rounded.label_bounds.max.y,
            },
            ScenePoint {
                x: rounded.label_bounds.max.x,
                y: rounded.label_bounds.min.y,
            },
            rounded.label_bounds.max,
        ] {
            assert!(super::inside_rounded_rect(
                ScenePoint {
                    x: corner.x - rounded_center.x,
                    y: corner.y - rounded_center.y,
                },
                rounded_half_width,
                rounded_half_height,
                rounded_radius,
            ));
        }
        let diamond = &scene.nodes[1];
        let diamond_center = diamond.bounds.center();
        for corner in [
            diamond.label_bounds.min,
            ScenePoint {
                x: diamond.label_bounds.min.x,
                y: diamond.label_bounds.max.y,
            },
            ScenePoint {
                x: diamond.label_bounds.max.x,
                y: diamond.label_bounds.min.y,
            },
            diamond.label_bounds.max,
        ] {
            let half_width = diamond.bounds.width() / 2.0;
            let half_height = diamond.bounds.height() / 2.0;
            let inside = (corner.x - diamond_center.x).abs() / half_width
                + (corner.y - diamond_center.y).abs() / half_height;
            assert!(inside <= 1.0);
        }

        let group = scene
            .groups
            .iter()
            .find(|group| group.id.as_str() == "G")
            .unwrap();
        let member = scene
            .nodes
            .iter()
            .find(|node| node.id.as_str() == "C")
            .unwrap();
        assert!(group.bounds.contains_rect(member.bounds));
        assert!(group.bounds.contains_rect(group.label_bounds));
        assert_eq!(
            group.members.iter().map(NodeId::as_str).collect::<Vec<_>>(),
            ["C"]
        );
        assert_eq!(group.label_alignment, TextHorizontalAlignment::Leading);
        let empty = scene
            .groups
            .iter()
            .find(|group| group.id.as_str() == "Empty")
            .unwrap();
        assert!(empty.members.is_empty());
        assert!(empty.bounds.contains_rect(empty.label_bounds));
        let total = scene.bounds.unwrap();
        assert!(
            all_scene_points(&scene)
                .into_iter()
                .all(|point| total.contains_point(point))
        );
    }

    #[test]
    fn diagram_scene_routes_shape_aware_arrows_parallel_edges_and_self_loops() {
        fn on_rectangle_boundary(rect: super::SceneRect, point: ScenePoint) -> bool {
            let epsilon = 1.0e-9;
            rect.contains_point(point)
                && ((point.x - rect.min.x).abs() <= epsilon
                    || (point.x - rect.max.x).abs() <= epsilon
                    || (point.y - rect.min.y).abs() <= epsilon
                    || (point.y - rect.max.y).abs() <= epsilon)
        }

        let source = "flowchart LR\nA[\"source\"]\nB{\"target\"}\nC(\"rounded\")\nA -->|one| B\nA -->|two| B\nB --> C\nA -->|loop| A\n";
        let (mut document, _) = scene_from_source(source, SceneMetrics::default());
        document.nodes[0].position = Position::new(0.0, 0.0).unwrap();
        document.nodes[1].position = Position::new(300.0, 0.0).unwrap();
        document.nodes[2].position = Position::new(600.0, 0.0).unwrap();
        let scene = build_scene(&document, SceneMetrics::default()).unwrap();
        assert_eq!(scene.direction, FlowDirection::LeftRight);

        let node_a = scene
            .nodes
            .iter()
            .find(|node| node.id.as_str() == "A")
            .unwrap();
        let node_b = scene
            .nodes
            .iter()
            .find(|node| node.id.as_str() == "B")
            .unwrap();
        let one = scene
            .edges
            .iter()
            .find(|edge| {
                edge.label.as_ref().is_some_and(|label| {
                    label
                        .lines
                        .iter()
                        .map(|line| line.text.as_str())
                        .collect::<Vec<_>>()
                        .concat()
                        == "one"
                })
            })
            .unwrap();
        let two = scene
            .edges
            .iter()
            .find(|edge| {
                edge.label.as_ref().is_some_and(|label| {
                    label
                        .lines
                        .iter()
                        .map(|line| line.text.as_str())
                        .collect::<Vec<_>>()
                        .concat()
                        == "two"
                })
            })
            .unwrap();
        assert_eq!(one.from.as_str(), "A");
        assert_eq!(one.to.as_str(), "B");
        assert_eq!(one.path.start.x, node_a.bounds.max.x);
        let diamond_center = node_b.bounds.center();
        let diamond_boundary = (one.path.end.x - diamond_center.x).abs()
            / (node_b.bounds.width() / 2.0)
            + (one.path.end.y - diamond_center.y).abs() / (node_b.bounds.height() / 2.0);
        assert!((diamond_boundary - 1.0).abs() < 1.0e-9);
        assert_eq!(one.arrowhead.tip, one.path.end);
        assert_ne!(one.parallel_index, two.parallel_index);
        assert_ne!(one.path.control1, two.path.control1);
        assert!(one.label_bounds.is_some());
        assert_eq!(one.label_alignment, Some(TextHorizontalAlignment::Center));

        let rounded_node = scene
            .nodes
            .iter()
            .find(|node| node.id.as_str() == "C")
            .unwrap();
        let rounded_edge = scene
            .edges
            .iter()
            .find(|edge| edge.from.as_str() == "B" && edge.to.as_str() == "C")
            .unwrap();
        let rounded_center = rounded_node.bounds.center();
        let relative_end = ScenePoint {
            x: rounded_edge.path.end.x - rounded_center.x,
            y: rounded_edge.path.end.y - rounded_center.y,
        };
        let rounded_radius = SceneMetrics::default()
            .rounded_corner_radius
            .min(rounded_node.bounds.width() / 2.0)
            .min(rounded_node.bounds.height() / 2.0);
        let corner_x =
            (relative_end.x.abs() - (rounded_node.bounds.width() / 2.0 - rounded_radius)).max(0.0);
        let corner_y =
            (relative_end.y.abs() - (rounded_node.bounds.height() / 2.0 - rounded_radius)).max(0.0);
        assert!(
            (corner_x * corner_x + corner_y * corner_y - rounded_radius.powi(2)).abs() < 1.0e-6
        );

        let self_edge = scene
            .edges
            .iter()
            .find(|edge| edge.from == edge.to)
            .unwrap();
        assert_eq!(self_edge.from.as_str(), "A");
        assert_ne!(self_edge.path.start, self_edge.path.end);
        assert_ne!(self_edge.path.control1, self_edge.path.control2);
        assert!(on_rectangle_boundary(node_a.bounds, self_edge.path.start));
        assert!(on_rectangle_boundary(node_a.bounds, self_edge.path.end));
        for t in [0.1, 0.25, 0.5, 0.75, 0.9] {
            assert!(
                !node_a.bounds.contains_point(self_edge.path.point_at(t)),
                "self-loop at {t} entered node bounds {:?}",
                node_a.bounds
            );
        }
        assert!(
            !node_a.bounds.contains_point(self_edge.arrowhead.left)
                && !node_a.bounds.contains_point(self_edge.arrowhead.right)
        );
        let self_label = self_edge.label_bounds.unwrap();
        assert!(
            self_label.max.x <= node_a.bounds.min.x
                || self_label.min.x >= node_a.bounds.max.x
                || self_label.max.y <= node_a.bounds.min.y
                || self_label.min.y >= node_a.bounds.max.y
        );
        assert!(
            scene
                .bounds
                .unwrap()
                .contains_point(self_edge.path.control1)
        );
        let total = scene.bounds.unwrap();
        assert!(
            all_scene_points(&scene)
                .into_iter()
                .all(|point| total.contains_point(point))
        );

        let large_metrics = SceneMetrics {
            minimum_node_width: 1_000.0,
            minimum_node_height: 1_000.0,
            ..SceneMetrics::default()
        };
        let (large_document, _) = scene_from_source(
            "flowchart TD\nA[\"large node\"]\nA -->|large loop| A\n",
            large_metrics,
        );
        let large_scene = build_scene(&large_document, large_metrics).unwrap();
        let large_node = &large_scene.nodes[0];
        let large_loop = &large_scene.edges[0];
        let large_midpoint = large_loop.path.point_at(0.5);
        assert!(large_midpoint.x > large_node.bounds.max.x);
        assert!(large_midpoint.y < large_node.bounds.min.y);
        for t in [0.1, 0.25, 0.5, 0.75, 0.9] {
            assert!(
                !large_node
                    .bounds
                    .contains_point(large_loop.path.point_at(t)),
                "large self-loop at {t} entered node bounds {:?}",
                large_node.bounds
            );
        }
        assert!(on_rectangle_boundary(
            large_node.bounds,
            large_loop.path.start
        ));
        assert!(on_rectangle_boundary(
            large_node.bounds,
            large_loop.path.end
        ));
        assert!(
            !large_node.bounds.contains_point(large_loop.arrowhead.left)
                && !large_node.bounds.contains_point(large_loop.arrowhead.right)
        );
        let large_label = large_loop.label_bounds.unwrap();
        assert!(
            large_label.min.x >= large_node.bounds.max.x
                || large_label.max.y <= large_node.bounds.min.y
        );

        let mut reordered = document.clone();
        reordered.edges.reverse();
        let reordered_scene = build_scene(&reordered, SceneMetrics::default()).unwrap();
        for edge in &scene.edges {
            let same_id = reordered_scene
                .edges
                .iter()
                .find(|candidate| candidate.id == edge.id)
                .unwrap();
            assert_eq!(same_id.path, edge.path);
        }
    }

    #[test]
    fn diagram_scene_handles_coincident_nodes_with_finite_geometry() {
        fn rects_intersect(left: super::SceneRect, right: super::SceneRect) -> bool {
            left.min.x < right.max.x
                && left.max.x > right.min.x
                && left.min.y < right.max.y
                && left.max.y > right.min.y
        }

        fn assert_arc_clear_of_nodes(scene: &DiagramScene, edge: &super::SceneEdge) {
            let source = scene
                .nodes
                .iter()
                .find(|node| node.id == edge.from)
                .unwrap();
            let target = scene.nodes.iter().find(|node| node.id == edge.to).unwrap();
            for t in [0.1, 0.25, 0.5, 0.75, 0.9] {
                let point = edge.path.point_at(t);
                assert!(
                    !source.bounds.contains_point(point),
                    "coincident edge {t} sample {point:?} crossed source {:?}",
                    source.bounds
                );
                assert!(
                    !target.bounds.contains_point(point),
                    "coincident edge {t} sample {point:?} crossed target {:?}",
                    target.bounds
                );
            }
            assert_eq!(edge.arrowhead.tip, edge.path.end);
            assert!(
                !target.bounds.contains_point(edge.arrowhead.left)
                    && !target.bounds.contains_point(edge.arrowhead.right),
                "arrowhead was covered by target bounds {:?}: {:?}",
                target.bounds,
                edge.arrowhead
            );
            let label_bounds = edge.label_bounds.unwrap();
            assert!(!rects_intersect(label_bounds, source.bounds));
            assert!(!rects_intersect(label_bounds, target.bounds));
        }

        let (mut single_document, _) = scene_from_source(
            "flowchart TD\nA[\"a wider source label\"]\nB{\"target\"}\nA -->|single| B\n",
            SceneMetrics::default(),
        );
        single_document.nodes[0].position = Position::default();
        single_document.nodes[1].position = Position::default();
        let single_scene = build_scene(&single_document, SceneMetrics::default()).unwrap();
        let single_edge = &single_scene.edges[0];
        assert_eq!(single_edge.parallel_index, 0);
        assert_arc_clear_of_nodes(&single_scene, single_edge);
        assert!(single_scene.bounds.is_some());

        let (mut parallel_document, _) = scene_from_source(
            "flowchart TD\nA[\"a wider source label\"]\nB{\"target\"}\nA -->|upper| B\nA -->|lower| B\n",
            SceneMetrics::default(),
        );
        parallel_document.nodes[0].position = Position::default();
        parallel_document.nodes[1].position = Position::default();
        let parallel_scene = build_scene(&parallel_document, SceneMetrics::default()).unwrap();
        assert_eq!(parallel_scene.edges.len(), 2);
        assert_ne!(
            parallel_scene.edges[0].path.control1.y.signum(),
            parallel_scene.edges[1].path.control1.y.signum()
        );
        for edge in &parallel_scene.edges {
            assert_arc_clear_of_nodes(&parallel_scene, edge);
        }
        assert!(
            all_scene_points(&parallel_scene)
                .iter()
                .all(|point| point.x.is_finite() && point.y.is_finite())
        );
        assert!(parallel_scene.bounds.is_some());
    }

    #[test]
    fn diagram_scene_rejects_invalid_metrics_and_graph_resource_bounds() {
        let (document, _) = scene_from_source("flowchart TD\nA\n", SceneMetrics::default());
        let bad_metrics = SceneMetrics {
            text: SceneTextMetrics {
                max_scalar_advance: 0.0,
                ..SceneTextMetrics::default()
            },
            ..SceneMetrics::default()
        };
        assert!(matches!(
            build_scene(&document, bad_metrics),
            Err(SceneError::InvalidMetrics)
        ));

        let mut too_many_nodes = document.clone();
        let node_template = too_many_nodes.nodes[0].clone();
        too_many_nodes.nodes = (0..=MAX_DIAGRAM_NODES)
            .map(|index| {
                let mut node = node_template.clone();
                node.id = NodeId::new(format!("N{index}")).unwrap();
                node
            })
            .collect();
        assert!(matches!(
            build_scene(&too_many_nodes, SceneMetrics::default()),
            Err(SceneError::InvalidGraph(_))
        ));

        let (mut one_edge, _) =
            scene_from_source("flowchart TD\nA\nB\nA --> B\n", SceneMetrics::default());
        let edge_template = one_edge.edges[0].clone();
        one_edge.edges = (0..=MAX_DIAGRAM_EDGES)
            .map(|_| edge_template.clone())
            .collect();
        assert!(matches!(
            build_scene(&one_edge, SceneMetrics::default()),
            Err(SceneError::InvalidGraph(_))
        ));

        let mut one_group = parse_mermaid("flowchart TD\nsubgraph G[\"Group\"]\nend\n").unwrap();
        let group_template = one_group.groups[0].clone();
        one_group.groups = (0..=MAX_DIAGRAM_GROUPS)
            .map(|_| group_template.clone())
            .collect();
        assert!(matches!(
            build_scene(&one_group, SceneMetrics::default()),
            Err(SceneError::InvalidGraph(_))
        ));

        let mut invalid_coordinate = document;
        invalid_coordinate.nodes[0].position.x = f32::INFINITY;
        assert!(matches!(
            build_scene(&invalid_coordinate, SceneMetrics::default()),
            Err(SceneError::InvalidGraph(_))
        ));

        let empty = parse_mermaid("").unwrap();
        assert!(
            build_scene(&empty, SceneMetrics::default())
                .unwrap()
                .bounds
                .is_none()
        );
    }

    #[test]
    fn diagram_scene_retains_stable_node_group_and_edge_identities() {
        let document = parse_mermaid(
            "flowchart LR\nsubgraph G[\"Group\"]\nA[\"Alpha\"]\nB[\"Beta\"]\nend\nA --> B\n",
        )
        .unwrap();
        let scene = build_scene(&document, SceneMetrics::default()).unwrap();
        assert_eq!(
            scene
                .nodes
                .iter()
                .map(|node| node.id.clone())
                .collect::<Vec<_>>(),
            document
                .nodes
                .iter()
                .map(|node| node.id.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(scene.groups[0].id, document.groups[0].id);
        assert_eq!(scene.edges[0].id, document.edges[0].id);
        assert_eq!(scene.edges[0].from, document.edges[0].from);
        assert_eq!(scene.edges[0].to, document.edges[0].to);
        assert!(document.edges[0].id.get() > 0);
    }
}
