//! Bounded deterministic projection of persisted visual objects into paint geometry.

use super::scene::{PreparedText, PreparedTextLine, ScenePoint, SceneRect};
use super::visual::{
    VisualAnchor, VisualAsset, VisualColor, VisualConnector, VisualConnectorDirection,
    VisualConnectorEndpoint, VisualConnectorRoute, VisualDiagramDocument, VisualDocumentError,
    VisualLayerId, VisualManualRoute, VisualRoutePoint, VisualObject, VisualObjectId, VisualObjectKind, VisualPrimitiveComponent,
    VisualPrimitiveShape, VisualScene, VisualShadow, VisualStyle, VisualSwimlaneOrientation,
    VisualTransform,
};
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BinaryHeap};
use std::fmt;

pub const MAX_VISUAL_PROJECTED_ITEMS: usize = 4_096;
pub const MAX_VISUAL_OUTLINE_POINTS: usize = 64;
pub const MAX_VISUAL_TEXT_LINES_PER_OBJECT: usize = 256;
pub const MAX_VISUAL_TEXT_LINES_PER_SCENE: usize = 8_192;
pub const MAX_VISUAL_DASH_SEGMENTS_PER_ITEM: usize = 256;
pub const MAX_VISUAL_DASH_SEGMENTS_PER_SCENE: usize = 32_768;
pub const MAX_VISUAL_PROJECTION_COORDINATE_ABS: f64 = 1_000_000_000.0;
pub const MAX_VISUAL_CONNECTOR_LABEL_LINES: usize = 8;
pub const MAX_VISUAL_CONNECTOR_ROUTE_POINTS: usize = 128;
const MAX_CONNECTOR_ROUTER_OBSTACLES: usize = 24;
const MAX_CONNECTOR_ROUTER_GRID_NODES: usize = 4_096;
const MAX_CONNECTOR_ROUTER_WORK_UNITS: usize = 2_000_000;
const CONNECTOR_CLEARANCE: f64 = 8.0;
const CONNECTOR_EGRESS_STUB: f64 = CONNECTOR_CLEARANCE + 2.0;
const CONNECTOR_BEND_COST: f64 = 12.0;
const CONNECTOR_PARALLEL_SPACING: f64 = 12.0;
const CONNECTOR_CURVE_SEGMENTS: usize = 24;
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
    Image,
    MissingImagePlaceholder,
    Container,
    SwimlaneHorizontal,
    SwimlaneVertical,
    Connector,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedVisualItem {
    pub object_id: VisualObjectId,
    pub component_index: Option<usize>,
    pub layer_id: VisualLayerId,
    pub kind: VisualProjectedKind,
    pub image_asset: Option<VisualAsset>,
    pub transform: VisualTransform,
    pub style: VisualStyle,
    /// Closed, rotated outline path in document coordinates.
    pub outline: Vec<ScenePoint>,
    /// Transformed swimlane/container header area, shared by paint and interaction.
    pub header: Option<Vec<ScenePoint>>,
    /// Additional closed paths such as a cylinder's top cap or missing-image X.
    pub details: Vec<Vec<ScenePoint>>,
    /// Open route and arrowhead paths, used only by connectors.
    pub open_paths: Vec<Vec<ScenePoint>>,
    pub text: Option<PreparedText>,
    /// Connector labels have a route-derived text frame independent of the inert object transform.
    pub text_transform: Option<VisualTransform>,
    pub text_bounds: Option<SceneRect>,
    pub text_truncated: bool,
    pub text_rotation_degrees: f64,
    /// Union of fill, text ink, stroke, shadow and detail geometry.
    pub bounds: SceneRect,
    /// Large dashed paths remain visible with a solid outline and are counted here.
    pub dash_simplified: bool,
    /// Exact bounded dash/dot strokes in document coordinates, in outline-then-detail order.
    /// `None` means a solid line (including a path simplified by the projection budget).
    pub dash_paths: Option<Vec<Vec<ProjectedStrokePrimitive>>>,
    pub route_fallback: bool,
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
    pub connector_route_fallbacks: usize,
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
    let mut items_by_object = BTreeMap::<VisualObjectId, Vec<ProjectedVisualItem>>::new();
    let mut connectors = Vec::<&VisualObject>::new();
    let mut projected_count = 0usize;
    for layer in &scene.layers {
        for object in scene
            .objects
            .iter()
            .filter(|object| object.layer_id == layer.id)
        {
            if matches!(&object.kind, VisualObjectKind::Connector { .. }) {
                connectors.push(object);
                continue;
            }
            let projected = match &object.kind {
                VisualObjectKind::Primitive { shape } => {
                    let mut items = vec![build_item(
                        object.id,
                        None,
                        layer.id,
                        VisualProjectedKind::Primitive(*shape),
                        object.transform,
                        object.style.clone(),
                        None,
                        false,
                    )?];
                    if let Some(label) = &object.owned_label {
                        let mut transform = object.transform;
                        transform.width = (transform.width - 24.0).max(transform.width * 0.5);
                        transform.height = (transform.height - 16.0).max(transform.height * 0.5);
                        items.push(build_item(
                            object.id,
                            None,
                            layer.id,
                            VisualProjectedKind::Text,
                            transform,
                            label.style.clone(),
                            Some(&label.text),
                            false,
                        )?);
                    }
                    items
                }
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
                VisualObjectKind::Container { label } => vec![build_frame_item(
                    object.id,
                    layer.id,
                    VisualProjectedKind::Container,
                    object.transform,
                    object.style.clone(),
                    label,
                    false,
                )?],
                VisualObjectKind::Swimlane { orientation, label } => vec![build_frame_item(
                    object.id,
                    layer.id,
                    match orientation {
                        VisualSwimlaneOrientation::Horizontal => {
                            VisualProjectedKind::SwimlaneHorizontal
                        }
                        VisualSwimlaneOrientation::Vertical => {
                            VisualProjectedKind::SwimlaneVertical
                        }
                    },
                    object.transform,
                    object.style.clone(),
                    label,
                    *orientation == VisualSwimlaneOrientation::Vertical,
                )?],
                VisualObjectKind::ImageReference { asset_id } => {
                    if let Some(asset) = scene.assets.iter().find(|asset| asset.id() == asset_id) {
                        let mut item = build_item(
                            object.id,
                            None,
                            layer.id,
                            VisualProjectedKind::Image,
                            object.transform,
                            object.style.clone(),
                            None,
                            false,
                        )?;
                        item.image_asset = Some(asset.clone());
                        vec![item]
                    } else {
                        vec![build_item(
                            object.id,
                            None,
                            layer.id,
                            VisualProjectedKind::MissingImagePlaceholder,
                            object.transform,
                            object.style.clone(),
                            Some("Missing image"),
                            true,
                        )?]
                    }
                }
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
                    if projected_count.saturating_add(preset.components.len())
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
                VisualObjectKind::Connector { .. } => unreachable!("connectors are deferred"),
            };

            if projected_count.saturating_add(projected.len()) > MAX_VISUAL_PROJECTED_ITEMS {
                return Err(VisualProjectionError::WorkLimitExceeded {
                    kind: "projected item",
                    max: MAX_VISUAL_PROJECTED_ITEMS,
                });
            }
            projected_count += projected.len();
            items_by_object.insert(object.id, projected);
        }
    }

    let owner_bounds = projected_owner_bounds(&items_by_object);
    let obstacle_bounds = projected_obstacle_bounds(&items_by_object);
    let parallel_lanes = connector_parallel_lanes(&connectors);
    let object_rotations = scene
        .objects
        .iter()
        .map(|object| (object.id, object.transform.rotation_degrees))
        .collect::<BTreeMap<_, _>>();
    let mut route_budget = MAX_CONNECTOR_ROUTER_WORK_UNITS;
    let mut connector_route_fallbacks = 0usize;
    for object in &connectors {
        let VisualObjectKind::Connector { connector } = &object.kind else {
            unreachable!("connector list contains only connectors");
        };
        let lane = parallel_lanes.get(&object.id).copied().unwrap_or(0.0);
        let canonical_direction = connector.from <= connector.to;
        let from_lane = if canonical_direction { lane } else { -lane };
        let to_lane = -from_lane;
        let from = project_connector_anchor(
            items_by_object.get(&connector.from.object_id),
            connector.from.anchor,
            from_lane,
            object_rotations
                .get(&connector.from.object_id)
                .copied()
                .unwrap_or(0.0),
        )
        .ok_or_else(|| {
            VisualProjectionError::InvalidScene("connector source outline is unavailable".into())
        })?;
        let to = project_connector_anchor(
            items_by_object.get(&connector.to.object_id),
            connector.to.anchor,
            to_lane,
            object_rotations
                .get(&connector.to.object_id)
                .copied()
                .unwrap_or(0.0),
        )
        .ok_or_else(|| {
            VisualProjectionError::InvalidScene("connector target outline is unavailable".into())
        })?;
        let (route, route_fallback) = route_connector(
            connector,
            from,
            to,
            &owner_bounds,
            &obstacle_bounds,
            &mut route_budget,
        );
        if route_fallback {
            connector_route_fallbacks = connector_route_fallbacks.saturating_add(1);
        }
        let mut open_paths = vec![route.clone()];
        match connector.direction {
            VisualConnectorDirection::None => {}
            VisualConnectorDirection::Forward => {
                open_paths.push(connector_arrowhead(&route, false));
            }
            VisualConnectorDirection::Reverse => {
                open_paths.push(connector_arrowhead(&route, true));
            }
            VisualConnectorDirection::Both => {
                open_paths.push(connector_arrowhead(&route, true));
                open_paths.push(connector_arrowhead(&route, false));
            }
        }
        let (text, text_transform, text_bounds, text_truncated) =
            connector_label_geometry(&connector.label, &route, object.style.font_size);
        let mut item_bounds = bounds_for_open_paths(&open_paths).ok_or_else(|| {
            VisualProjectionError::InvalidScene("connector route is empty".into())
        })?;
        item_bounds = expand(item_bounds, object.style.border_width / 2.0);
        if let Some(label_bounds) = text_bounds {
            item_bounds = union(item_bounds, label_bounds);
        }
        let item = ProjectedVisualItem {
            object_id: object.id,
            component_index: None,
            layer_id: object.layer_id,
            kind: VisualProjectedKind::Connector,
            image_asset: None,
            transform: object.transform,
            style: object.style.clone(),
            outline: Vec::new(),
            header: None,
            details: Vec::new(),
            open_paths,
            text,
            text_transform,
            text_bounds,
            text_truncated,
            text_rotation_degrees: 0.0,
            bounds: item_bounds,
            dash_simplified: false,
            dash_paths: None,
            route_fallback,
        };
        validate_projected_bounds(item.bounds)?;
        if projected_count == MAX_VISUAL_PROJECTED_ITEMS {
            return Err(VisualProjectionError::WorkLimitExceeded {
                kind: "projected item",
                max: MAX_VISUAL_PROJECTED_ITEMS,
            });
        }
        projected_count += 1;
        items_by_object.insert(object.id, vec![item]);
    }

    let mut items = Vec::with_capacity(projected_count);
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
            let Some(projected) = items_by_object.remove(&object.id) else {
                continue;
            };
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
        connector_route_fallbacks,
    })
}

pub fn project_visual_document(
    document: &VisualDiagramDocument,
) -> Result<VisualSceneProjection, VisualProjectionError> {
    project_visual_scene(document.scene())
}

#[derive(Clone, Copy)]
struct ConnectorAnchorPoint {
    point: ScenePoint,
    normal: ScenePoint,
    tangent: ScenePoint,
    lane_remainder: f64,
}

#[derive(Clone, Copy)]
struct ConnectorEgress {
    normal_point: ScenePoint,
    lane_point: ScenePoint,
}

#[derive(Clone, Copy)]
struct ConnectorObstacleView<'a> {
    base: &'a BTreeMap<VisualObjectId, SceneRect>,
    endpoint_frames: [Option<(VisualObjectId, SceneRect)>; 2],
}

impl<'a> ConnectorObstacleView<'a> {
    fn new(
        base: &'a BTreeMap<VisualObjectId, SceneRect>,
        owner_bounds: &BTreeMap<VisualObjectId, SceneRect>,
        connector: &VisualConnector,
    ) -> Self {
        let endpoint_frame = |id| {
            if base.contains_key(&id) {
                None
            } else {
                owner_bounds.get(&id).copied().map(|bounds| (id, bounds))
            }
        };
        let from = endpoint_frame(connector.from.object_id);
        let to = (connector.to.object_id != connector.from.object_id)
            .then(|| endpoint_frame(connector.to.object_id))
            .flatten();
        Self {
            base,
            endpoint_frames: [from, to],
        }
    }

    fn len(self) -> usize {
        self.base.len() + self.endpoint_frames.iter().flatten().count()
    }

    fn contains_key(self, id: &VisualObjectId) -> bool {
        self.base.contains_key(id)
            || self
                .endpoint_frames
                .iter()
                .flatten()
                .any(|(endpoint_id, _)| endpoint_id == id)
    }

    fn iter(self) -> impl Iterator<Item = (VisualObjectId, SceneRect)> + 'a {
        self.base
            .iter()
            .map(|(id, bounds)| (*id, *bounds))
            .chain(self.endpoint_frames.into_iter().flatten())
    }
}

#[derive(Clone, Copy, Debug)]
struct RouteQueueEntry {
    cost: f64,
    state: usize,
}

impl PartialEq for RouteQueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.cost.total_cmp(&other.cost) == Ordering::Equal && self.state == other.state
    }
}

impl Eq for RouteQueueEntry {}

impl PartialOrd for RouteQueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RouteQueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cost
            .total_cmp(&other.cost)
            .then_with(|| self.state.cmp(&other.state))
    }
}

fn projected_owner_bounds(
    items_by_object: &BTreeMap<VisualObjectId, Vec<ProjectedVisualItem>>,
) -> BTreeMap<VisualObjectId, SceneRect> {
    items_by_object
        .iter()
        .filter_map(|(id, items)| {
            let bounds = items
                .iter()
                .map(|item| {
                    if matches!(
                        item.kind,
                        VisualProjectedKind::Container
                            | VisualProjectedKind::SwimlaneHorizontal
                            | VisualProjectedKind::SwimlaneVertical
                    ) {
                        bounds_for_points(&item.outline).unwrap_or(item.bounds)
                    } else {
                        item.bounds
                    }
                })
                .reduce(union);
            bounds.map(|bounds| (*id, bounds))
        })
        .collect()
}

fn projected_obstacle_bounds(
    items_by_object: &BTreeMap<VisualObjectId, Vec<ProjectedVisualItem>>,
) -> BTreeMap<VisualObjectId, SceneRect> {
    items_by_object
        .iter()
        .filter_map(|(id, items)| {
            let is_frame = items.iter().any(|item| {
                matches!(
                    item.kind,
                    VisualProjectedKind::Container
                        | VisualProjectedKind::SwimlaneHorizontal
                        | VisualProjectedKind::SwimlaneVertical
                )
            });
            if is_frame {
                None
            } else {
                items
                    .iter()
                    .map(|item| item.bounds)
                    .reduce(union)
                    .map(|bounds| (*id, bounds))
            }
        })
        .collect()
}

fn connector_parallel_lanes(connectors: &[&VisualObject]) -> BTreeMap<VisualObjectId, f64> {
    let mut groups = BTreeMap::<(VisualConnectorEndpoint, VisualConnectorEndpoint), Vec<_>>::new();
    for object in connectors {
        let VisualObjectKind::Connector { connector } = &object.kind else {
            continue;
        };
        let key = if connector.from <= connector.to {
            (connector.from, connector.to)
        } else {
            (connector.to, connector.from)
        };
        groups.entry(key).or_default().push(object.id);
    }
    let mut lanes = BTreeMap::new();
    for ids in groups.values_mut() {
        ids.sort_unstable();
        let midpoint = ids.len().saturating_sub(1) as f64 / 2.0;
        for (index, id) in ids.iter().enumerate() {
            lanes.insert(*id, (index as f64 - midpoint) * CONNECTOR_PARALLEL_SPACING);
        }
    }
    lanes
}

fn project_connector_anchor(
    items: Option<&Vec<ProjectedVisualItem>>,
    anchor: VisualAnchor,
    lane_offset: f64,
    rotation_degrees: f64,
) -> Option<ConnectorAnchorPoint> {
    let local_normal = match anchor {
        VisualAnchor::North => point(0.0, -1.0),
        VisualAnchor::NorthEast => normalize(point(1.0, -1.0)),
        VisualAnchor::East => point(1.0, 0.0),
        VisualAnchor::SouthEast => normalize(point(1.0, 1.0)),
        VisualAnchor::South => point(0.0, 1.0),
        VisualAnchor::SouthWest => normalize(point(-1.0, 1.0)),
        VisualAnchor::West => point(-1.0, 0.0),
        VisualAnchor::NorthWest => normalize(point(-1.0, -1.0)),
    };
    let angle = rotation_degrees.to_radians();
    let (sin, cos) = angle.sin_cos();
    let normal = normalize(point(
        local_normal.x * cos - local_normal.y * sin,
        local_normal.x * sin + local_normal.y * cos,
    ));
    let tangent = point(-normal.y, normal.x);
    let items = items?;
    // Owned text is painted as a second item with the same owner; its
    // rectangular text frame is not part of the primitive's outline.
    let primitive_owner = items.iter().any(|item| matches!(item.kind, VisualProjectedKind::Primitive(_)));
    let anchor_items = || items.iter().filter(|item| !primitive_owner || item.kind != VisualProjectedKind::Text);
    let support = anchor_items()
        .flat_map(|item| item.outline.iter())
        .map(|candidate| dot(*candidate, normal))
        .max_by(f64::total_cmp)?;
    let epsilon = support.abs().max(1.0) * 1.0e-10;
    let mut tangent_min = f64::INFINITY;
    let mut tangent_max = f64::NEG_INFINITY;
    for item in anchor_items() {
        for candidate in &item.outline {
            if support - dot(*candidate, normal) <= epsilon {
                let coordinate = dot(*candidate, tangent);
                tangent_min = tangent_min.min(coordinate);
                tangent_max = tangent_max.max(coordinate);
            }
        }
    }
    if !tangent_min.is_finite() || !tangent_max.is_finite() {
        return None;
    }
    let mid_tangent = (tangent_min + tangent_max) / 2.0;
    let desired_tangent = mid_tangent + lane_offset;
    let desired = ScenePoint {
        x: normal.x * support + tangent.x * desired_tangent,
        y: normal.y * support + tangent.y * desired_tangent,
    };
    let mut best: Option<(f64, usize, usize, ScenePoint)> = None;
    for (component_order, item) in anchor_items().enumerate() {
        for edge in 0..item.outline.len() {
            let start = item.outline[edge];
            let end = item.outline[(edge + 1) % item.outline.len()];
            let candidate = closest_point_on_segment(desired, start, end);
            let candidate_distance = distance(candidate, desired);
            let component_index = item.component_index.unwrap_or(component_order);
            let replace =
                best.as_ref()
                    .is_none_or(|(best_distance, best_component, best_edge, _)| {
                        candidate_distance
                            .total_cmp(best_distance)
                            .then_with(|| component_index.cmp(best_component))
                            .then_with(|| edge.cmp(best_edge))
                            .is_lt()
                    });
            if replace {
                best = Some((candidate_distance, component_index, edge, candidate));
            }
        }
    }
    let (_, _, _, anchor) = best?;
    let applied_lane = dot(anchor, tangent) - mid_tangent;
    let lane_remainder = lane_offset - applied_lane;
    (anchor.x.is_finite() && anchor.y.is_finite()).then_some(ConnectorAnchorPoint {
        point: anchor,
        normal,
        tangent,
        lane_remainder,
    })
}

fn route_connector(
    connector: &VisualConnector,
    from: ConnectorAnchorPoint,
    to: ConnectorAnchorPoint,
    owner_bounds: &BTreeMap<VisualObjectId, SceneRect>,
    obstacle_bounds: &BTreeMap<VisualObjectId, SceneRect>,
    remaining_work: &mut usize,
) -> (Vec<ScenePoint>, bool) {
    if let Some(manual) = &connector.manual_route {
        let convert = |point: &VisualRoutePoint| ScenePoint { x: point.x, y: point.y };
        let path = match manual {
            VisualManualRoute::Polyline { points } => std::iter::once(from.point)
                .chain(points.iter().map(convert))
                .chain(std::iter::once(to.point))
                .collect(),
            VisualManualRoute::Cubic { control1, control2 } => sample_cubic(
                from.point, convert(control1), convert(control2), to.point, CONNECTOR_CURVE_SEGMENTS,
            ),
        };
        return (path, false);
    }
    let route_obstacles = ConnectorObstacleView::new(obstacle_bounds, owner_bounds, connector);
    if connector.from.object_id == connector.to.object_id {
        return match connector.route {
            VisualConnectorRoute::Curved => route_curved_self_loop(
                connector,
                from,
                to,
                owner_bounds,
                route_obstacles,
                remaining_work,
            ),
            VisualConnectorRoute::Straight => {
                let (path, _) = route_elbow(
                    connector,
                    from,
                    to,
                    owner_bounds,
                    route_obstacles,
                    remaining_work,
                );
                (path, true)
            }
            VisualConnectorRoute::Elbow => route_elbow(
                connector,
                from,
                to,
                owner_bounds,
                route_obstacles,
                remaining_work,
            ),
        };
    }
    match connector.route {
        VisualConnectorRoute::Straight if distance(from.point, to.point) <= f64::EPSILON => {
            let (path, _) = route_elbow(
                connector,
                from,
                to,
                owner_bounds,
                route_obstacles,
                remaining_work,
            );
            (path, true)
        }
        VisualConnectorRoute::Straight => (
            vec![from.point, to.point],
            from.lane_remainder.abs() > f64::EPSILON || to.lane_remainder.abs() > f64::EPSILON,
        ),
        VisualConnectorRoute::Curved if distance(from.point, to.point) <= f64::EPSILON => {
            (connector_self_loop(from, to), true)
        }
        VisualConnectorRoute::Curved => (
            sample_cubic(
                from.point,
                add(
                    offset(
                        from.point,
                        from.normal,
                        curved_control_distance(from.point, to.point),
                    ),
                    point(
                        from.tangent.x * from.lane_remainder,
                        from.tangent.y * from.lane_remainder,
                    ),
                ),
                add(
                    offset(
                        to.point,
                        to.normal,
                        curved_control_distance(from.point, to.point),
                    ),
                    point(
                        to.tangent.x * to.lane_remainder,
                        to.tangent.y * to.lane_remainder,
                    ),
                ),
                to.point,
                CONNECTOR_CURVE_SEGMENTS,
            ),
            false,
        ),
        VisualConnectorRoute::Elbow => route_elbow(
            connector,
            from,
            to,
            owner_bounds,
            route_obstacles,
            remaining_work,
        ),
    }
}

/// Seed editable geometry from the already projected route, so the editor
/// never needs a second implementation of automatic routing.
pub(crate) fn seed_manual_route(
    connector: &VisualConnector,
    item: &ProjectedVisualItem,
) -> Option<VisualManualRoute> {
    if let Some(route) = &connector.manual_route {
        return Some(route.clone());
    }
    let path = item.open_paths.first()?;
    if path.len() < 2 {
        return None;
    }
    let start = *path.first()?;
    let end = *path.last()?;
    let route_point = |point: ScenePoint| VisualRoutePoint {
        x: point.x,
        y: point.y,
    };
    if connector.route == VisualConnectorRoute::Curved {
        // Inversion below uses exact samples at one and two thirds of a single cubic.
        if item.route_fallback
            || !CONNECTOR_CURVE_SEGMENTS.is_multiple_of(3)
            || path.len() != CONNECTOR_CURVE_SEGMENTS + 1
        {
            return None;
        }
        let a = path[CONNECTOR_CURVE_SEGMENTS / 3];
        let b = path[CONNECTOR_CURVE_SEGMENTS * 2 / 3];
        let control = |p0: f64, p1: f64, p2: f64, p3: f64| {
            let a = 27.0 * p1 - 8.0 * p0 - p3;
            let b = 27.0 * p2 - p0 - 8.0 * p3;
            ((2.0 * a - b) / 18.0, (2.0 * b - a) / 18.0)
        };
        let (x1, x2) = control(start.x, a.x, b.x, end.x);
        let (y1, y2) = control(start.y, a.y, b.y, end.y);
        Some(VisualManualRoute::Cubic {
            control1: VisualRoutePoint { x: x1, y: y1 },
            control2: VisualRoutePoint { x: x2, y: y2 },
        })
    } else {
        if path.len().saturating_sub(2) > super::visual::MAX_VISUAL_CONNECTOR_POINTS {
            return None;
        }
        let mut points = path[1..path.len().saturating_sub(1)]
            .iter()
            .copied()
            .map(route_point)
            .collect::<Vec<_>>();
        if points.is_empty() {
            points.push(route_point(ScenePoint {
                x: (start.x + end.x) / 2.0,
                y: (start.y + end.y) / 2.0,
            }));
        }
        (points.len() <= super::visual::MAX_VISUAL_CONNECTOR_POINTS)
            .then_some(VisualManualRoute::Polyline { points })
    }
}

fn route_elbow(
    connector: &VisualConnector,
    mut from: ConnectorAnchorPoint,
    mut to: ConnectorAnchorPoint,
    owner_bounds: &BTreeMap<VisualObjectId, SceneRect>,
    obstacle_bounds: ConnectorObstacleView<'_>,
    remaining_work: &mut usize,
) -> (Vec<ScenePoint>, bool) {
    let Some(from_bounds) = owner_bounds.get(&connector.from.object_id) else {
        return (fallback_elbow(from.point, to.point), true);
    };
    let Some(to_bounds) = owner_bounds.get(&connector.to.object_id) else {
        return (fallback_elbow(from.point, to.point), true);
    };
    if connector.from.object_id == connector.to.object_id
        && distance(from.point, to.point) <= f64::EPSILON
    {
        // Give a same-port loop two deterministic points along its tangent while
        // preserving the persisted anchor. Both egress segments remain outside
        // the convex owner's support plane.
        from.lane_remainder += CONNECTOR_PARALLEL_SPACING;
        to.lane_remainder -= CONNECTOR_PARALLEL_SPACING;
    }
    let from_egress = connector_egress(from, *from_bounds);
    let to_egress = connector_egress(to, *to_bounds);
    let stubs_clear = route_egress_is_clear(
        from.point,
        from_egress.normal_point,
        connector.from.object_id,
        obstacle_bounds,
        remaining_work,
    ) && route_egress_is_clear(
        from_egress.normal_point,
        from_egress.lane_point,
        connector.from.object_id,
        obstacle_bounds,
        remaining_work,
    ) && route_egress_is_clear(
        to.point,
        to_egress.normal_point,
        connector.to.object_id,
        obstacle_bounds,
        remaining_work,
    ) && route_egress_is_clear(
        to_egress.normal_point,
        to_egress.lane_point,
        connector.to.object_id,
        obstacle_bounds,
        remaining_work,
    );
    let route = stubs_clear
        .then(|| {
            route_orthogonal(
                from_egress.lane_point,
                to_egress.lane_point,
                obstacle_bounds,
                remaining_work,
            )
        })
        .flatten();
    match route {
        Some(path) => (
            join_connector_stubs(from.point, from_egress, &path, to_egress, to.point),
            false,
        ),
        None => (
            join_connector_stubs(
                from.point,
                from_egress,
                &fallback_elbow(from_egress.lane_point, to_egress.lane_point),
                to_egress,
                to.point,
            ),
            true,
        ),
    }
}

fn route_curved_self_loop(
    connector: &VisualConnector,
    from: ConnectorAnchorPoint,
    to: ConnectorAnchorPoint,
    owner_bounds: &BTreeMap<VisualObjectId, SceneRect>,
    obstacle_bounds: ConnectorObstacleView<'_>,
    remaining_work: &mut usize,
) -> (Vec<ScenePoint>, bool) {
    let path = connector_self_loop(from, to);
    if curved_self_loop_is_clear(
        connector.from.object_id,
        &path,
        obstacle_bounds,
        remaining_work,
    ) {
        return (path, false);
    }
    let (fallback, _) = route_elbow(
        connector,
        from,
        to,
        owner_bounds,
        obstacle_bounds,
        remaining_work,
    );
    (fallback, true)
}

fn curved_self_loop_is_clear(
    owner_id: VisualObjectId,
    path: &[ScenePoint],
    owner_bounds: ConnectorObstacleView<'_>,
    remaining_work: &mut usize,
) -> bool {
    let required = path
        .len()
        .saturating_sub(1)
        .saturating_mul(owner_bounds.len());
    if required > *remaining_work {
        *remaining_work = 0;
        return false;
    }
    *remaining_work -= required;
    let last_segment = path.len().saturating_sub(2);
    !owner_bounds.iter().any(|(id, bounds)| {
        let obstacle = if id == owner_id {
            expand(bounds, 0.1)
        } else {
            expand(bounds, CONNECTOR_CLEARANCE + 0.1)
        };
        path.windows(2).enumerate().any(|(index, segment)| {
            (id != owner_id || (index != 0 && index != last_segment))
                && segment_crosses_rect(segment[0], segment[1], obstacle)
        })
    })
}

fn route_egress_is_clear(
    start: ScenePoint,
    end: ScenePoint,
    owner_id: VisualObjectId,
    owner_bounds: ConnectorObstacleView<'_>,
    remaining_work: &mut usize,
) -> bool {
    let obstacle_count =
        owner_bounds
            .len()
            .saturating_sub(if owner_bounds.contains_key(&owner_id) {
                1
            } else {
                0
            });
    if obstacle_count > *remaining_work {
        *remaining_work = 0;
        return false;
    }
    *remaining_work -= obstacle_count;
    !owner_bounds
        .iter()
        .filter(|(id, _)| *id != owner_id)
        .any(|(_, bounds)| {
            segment_crosses_rect(start, end, expand(bounds, CONNECTOR_CLEARANCE + 0.1))
        })
}

fn connector_egress(anchor: ConnectorAnchorPoint, bounds: SceneRect) -> ConnectorEgress {
    let corners = [
        bounds.min,
        point(bounds.min.x, bounds.max.y),
        point(bounds.max.x, bounds.min.y),
        bounds.max,
    ];
    let support = corners
        .iter()
        .map(|corner| dot(*corner, anchor.normal))
        .fold(f64::NEG_INFINITY, f64::max);
    let outward_distance = (support - dot(anchor.point, anchor.normal) + CONNECTOR_EGRESS_STUB)
        .max(CONNECTOR_EGRESS_STUB);
    let (normal_point, lane_axis) = if anchor.normal.x.abs() >= anchor.normal.y.abs() {
        let direction = anchor.normal.x.signum();
        (
            point(
                anchor.point.x + direction * outward_distance / anchor.normal.x.abs(),
                anchor.point.y,
            ),
            point(0.0, anchor.tangent.y.signum()),
        )
    } else {
        let direction = anchor.normal.y.signum();
        (
            point(
                anchor.point.x,
                anchor.point.y + direction * outward_distance / anchor.normal.y.abs(),
            ),
            point(anchor.tangent.x.signum(), 0.0),
        )
    };
    ConnectorEgress {
        normal_point,
        lane_point: point(
            normal_point.x + lane_axis.x * anchor.lane_remainder,
            normal_point.y + lane_axis.y * anchor.lane_remainder,
        ),
    }
}

fn join_connector_stubs(
    start: ScenePoint,
    start_egress: ConnectorEgress,
    route: &[ScenePoint],
    end_egress: ConnectorEgress,
    end: ScenePoint,
) -> Vec<ScenePoint> {
    let mut points = Vec::with_capacity(route.len() + 4);
    points.extend([start, start_egress.normal_point, start_egress.lane_point]);
    points.extend(route.iter().copied());
    points.extend([end_egress.normal_point, end]);
    clean_path(points)
}

fn connector_self_loop(from: ConnectorAnchorPoint, to: ConnectorAnchorPoint) -> Vec<ScenePoint> {
    let mut normal = from.normal;
    if normal.x == -to.normal.x && normal.y == -to.normal.y {
        normal = point(0.0, -1.0);
    }
    let tangent = point(-normal.y, normal.x);
    let start = from.point;
    let end = to.point;
    sample_cubic(
        start,
        add(
            offset(start, normal, 36.0),
            point(
                tangent.x * (22.0 + from.lane_remainder),
                tangent.y * (22.0 + from.lane_remainder),
            ),
        ),
        add(
            offset(end, normal, 36.0),
            point(
                tangent.x * (-22.0 + to.lane_remainder),
                tangent.y * (-22.0 + to.lane_remainder),
            ),
        ),
        end,
        CONNECTOR_CURVE_SEGMENTS,
    )
}

fn route_orthogonal(
    start: ScenePoint,
    end: ScenePoint,
    owner_bounds: ConnectorObstacleView<'_>,
    remaining_work: &mut usize,
) -> Option<Vec<ScenePoint>> {
    if owner_bounds.len() > *remaining_work {
        *remaining_work = 0;
        return None;
    }
    *remaining_work -= owner_bounds.len();
    let all_obstacles = owner_bounds
        .iter()
        .map(|(id, bounds)| (id, expand(bounds, CONNECTOR_CLEARANCE + 0.1)))
        .collect::<Vec<_>>();
    let mut relevant = all_obstacles
        .iter()
        .map(|(id, bounds)| (*id, *bounds, distance_to_segment_box(*bounds, start, end)))
        .filter(|(_, _, distance)| *distance <= 512.0)
        .collect::<Vec<_>>();
    relevant.sort_by(|left, right| {
        left.2
            .total_cmp(&right.2)
            .then_with(|| left.0.cmp(&right.0))
    });
    relevant.truncate(MAX_CONNECTOR_ROUTER_OBSTACLES);
    let relevant = relevant
        .into_iter()
        .map(|(_, bounds, _)| bounds)
        .collect::<Vec<_>>();
    if relevant.is_empty() {
        let path = fallback_elbow(start, end);
        return route_is_clear(&path, &all_obstacles, remaining_work).then_some(path);
    }
    if point_inside_any(start, &relevant) || point_inside_any(end, &relevant) {
        return None;
    }

    let mut xs = vec![start.x, end.x];
    let mut ys = vec![start.y, end.y];
    for obstacle in &relevant {
        xs.extend([obstacle.min.x, obstacle.max.x]);
        ys.extend([obstacle.min.y, obstacle.max.y]);
    }
    sort_unique_coordinates(&mut xs);
    sort_unique_coordinates(&mut ys);
    let node_count = xs.len().checked_mul(ys.len())?;
    if node_count > MAX_CONNECTOR_ROUTER_GRID_NODES {
        return None;
    }
    let estimated_work =
        node_count.checked_mul(relevant.len().saturating_mul(5).saturating_add(4))?;
    if estimated_work > *remaining_work {
        *remaining_work = 0;
        return None;
    }
    *remaining_work -= estimated_work;

    let nx = xs.len();
    let node = |x: usize, y: usize| y * nx + x;
    let Some(start_x) = coordinate_index(&xs, start.x) else {
        return None;
    };
    let Some(start_y) = coordinate_index(&ys, start.y) else {
        return None;
    };
    let Some(end_x) = coordinate_index(&xs, end.x) else {
        return None;
    };
    let Some(end_y) = coordinate_index(&ys, end.y) else {
        return None;
    };
    let start_node = node(start_x, start_y);
    let end_node = node(end_x, end_y);
    let mut valid = vec![true; node_count];
    for (y, y_value) in ys.iter().enumerate() {
        for (x, x_value) in xs.iter().enumerate() {
            let point = point(*x_value, *y_value);
            valid[node(x, y)] = !point_inside_any(point, &relevant);
        }
    }
    if !valid[start_node] || !valid[end_node] {
        return None;
    }

    let state_count = node_count.checked_mul(3)?;
    let mut distances = vec![f64::INFINITY; state_count];
    let mut previous = vec![None::<usize>; state_count];
    let start_state = start_node * 3;
    distances[start_state] = 0.0;
    let mut queue = BinaryHeap::new();
    queue.push(Reverse(RouteQueueEntry {
        cost: 0.0,
        state: start_state,
    }));
    while let Some(Reverse(entry)) = queue.pop() {
        if entry.cost > distances[entry.state] {
            continue;
        }
        let current_node = entry.state / 3;
        let previous_axis = entry.state % 3;
        if current_node == end_node {
            break;
        }
        let x_index = current_node % nx;
        let y_index = current_node / nx;
        let neighbors = [
            (x_index.checked_sub(1).map(|x| node(x, y_index)), 1usize),
            (
                (x_index + 1 < xs.len()).then(|| node(x_index + 1, y_index)),
                1usize,
            ),
            (y_index.checked_sub(1).map(|y| node(x_index, y)), 2usize),
            (
                (y_index + 1 < ys.len()).then(|| node(x_index, y_index + 1)),
                2usize,
            ),
        ];
        for (neighbor, axis) in neighbors {
            let Some(neighbor) = neighbor else {
                continue;
            };
            if !valid[neighbor] {
                continue;
            }
            let from = point(xs[x_index], ys[y_index]);
            let to = point(xs[neighbor % nx], ys[neighbor / nx]);
            if segment_crosses_any(from, to, &relevant) {
                continue;
            }
            let length = distance(from, to);
            let bend = if previous_axis != 0 && previous_axis != axis {
                CONNECTOR_BEND_COST
            } else {
                0.0
            };
            let next_state = neighbor * 3 + axis;
            let candidate = entry.cost + length + bend;
            let current = distances[next_state];
            if candidate.total_cmp(&current).is_lt()
                || (candidate.total_cmp(&current) == Ordering::Equal
                    && previous[next_state].is_none_or(|old| entry.state < old))
            {
                distances[next_state] = candidate;
                previous[next_state] = Some(entry.state);
                queue.push(Reverse(RouteQueueEntry {
                    cost: candidate,
                    state: next_state,
                }));
            }
        }
    }

    let end_state = (0..3)
        .map(|axis| end_node * 3 + axis)
        .filter(|state| distances[*state].is_finite())
        .min_by(|left, right| {
            distances[*left]
                .total_cmp(&distances[*right])
                .then_with(|| left.cmp(right))
        })?;
    let mut states = vec![end_state];
    let mut cursor = end_state;
    while cursor != start_state {
        cursor = previous[cursor]?;
        states.push(cursor);
        if states.len() > MAX_VISUAL_CONNECTOR_ROUTE_POINTS.saturating_sub(4) {
            return None;
        }
    }
    states.reverse();
    let path = clean_path(
        states
            .iter()
            .map(|state| {
                let node = state / 3;
                point(xs[node % nx], ys[node / nx])
            })
            .collect(),
    );
    if path.len() > MAX_VISUAL_CONNECTOR_ROUTE_POINTS.saturating_sub(4)
        || !route_is_clear(&path, &all_obstacles, remaining_work)
    {
        return None;
    }
    Some(path)
}

fn fallback_elbow(start: ScenePoint, end: ScenePoint) -> Vec<ScenePoint> {
    let bend = if (end.x - start.x).abs() >= (end.y - start.y).abs() {
        point(end.x, start.y)
    } else {
        point(start.x, end.y)
    };
    clean_path(vec![start, bend, end])
}

fn route_is_clear(
    path: &[ScenePoint],
    obstacles: &[(VisualObjectId, SceneRect)],
    remaining_work: &mut usize,
) -> bool {
    let required = path.len().saturating_sub(1).saturating_mul(obstacles.len());
    if required > *remaining_work {
        *remaining_work = 0;
        return false;
    }
    *remaining_work -= required;
    !path.windows(2).any(|segment| {
        obstacles
            .iter()
            .any(|(_, bounds)| segment_crosses_rect(segment[0], segment[1], *bounds))
    })
}

fn point_inside_any(point: ScenePoint, obstacles: &[SceneRect]) -> bool {
    obstacles.iter().any(|rect| {
        point.x > rect.min.x && point.x < rect.max.x && point.y > rect.min.y && point.y < rect.max.y
    })
}

fn segment_crosses_any(start: ScenePoint, end: ScenePoint, obstacles: &[SceneRect]) -> bool {
    obstacles
        .iter()
        .any(|rect| segment_crosses_rect(start, end, *rect))
}

fn segment_crosses_rect(start: ScenePoint, end: ScenePoint, rect: SceneRect) -> bool {
    let mut enter = 0.0_f64;
    let mut leave = 1.0_f64;
    for (origin, delta, min, max) in [
        (start.x, end.x - start.x, rect.min.x, rect.max.x),
        (start.y, end.y - start.y, rect.min.y, rect.max.y),
    ] {
        if delta.abs() <= f64::EPSILON {
            if origin <= min || origin >= max {
                return false;
            }
            continue;
        }
        let first = (min - origin) / delta;
        let second = (max - origin) / delta;
        enter = enter.max(first.min(second));
        leave = leave.min(first.max(second));
        if enter >= leave {
            return false;
        }
    }
    leave > 0.0 && enter < 1.0
}

fn distance_to_segment_box(bounds: SceneRect, start: ScenePoint, end: ScenePoint) -> f64 {
    let segment_min = point(start.x.min(end.x), start.y.min(end.y));
    let segment_max = point(start.x.max(end.x), start.y.max(end.y));
    let dx = (bounds.min.x - segment_max.x)
        .max(segment_min.x - bounds.max.x)
        .max(0.0);
    let dy = (bounds.min.y - segment_max.y)
        .max(segment_min.y - bounds.max.y)
        .max(0.0);
    dx.hypot(dy)
}

fn sort_unique_coordinates(values: &mut Vec<f64>) {
    values.sort_by(f64::total_cmp);
    values.dedup_by(|left, right| (*left - *right).abs() <= 1.0e-9);
}

fn coordinate_index(values: &[f64], coordinate: f64) -> Option<usize> {
    values
        .binary_search_by(|value| value.total_cmp(&coordinate))
        .ok()
        .or_else(|| {
            values
                .iter()
                .position(|value| (*value - coordinate).abs() <= 1.0e-9)
        })
}

fn connector_arrowhead(path: &[ScenePoint], at_start: bool) -> Vec<ScenePoint> {
    let tip = if at_start {
        *path.first().unwrap_or(&ScenePoint::default())
    } else {
        *path.last().unwrap_or(&ScenePoint::default())
    };
    let neighbor = if at_start {
        path.iter().copied().find(|point| distance(*point, tip) > f64::EPSILON)
    } else {
        path.iter().rev().copied().find(|point| distance(*point, tip) > f64::EPSILON)
    };
    let Some(neighbor) = neighbor else { return Vec::new(); };
    let direction = normalize(point(tip.x - neighbor.x, tip.y - neighbor.y));
    let base = offset(tip, direction, -10.0);
    let tangent = point(-direction.y, direction.x);
    clean_path(vec![
        offset(base, tangent, 5.0),
        tip,
        offset(base, tangent, -5.0),
    ])
}

fn connector_label_geometry(
    label: &str,
    path: &[ScenePoint],
    font_size: f64,
) -> (
    Option<PreparedText>,
    Option<VisualTransform>,
    Option<SceneRect>,
    bool,
) {
    if label.is_empty() {
        return (None, None, None, false);
    }
    let (mut center, tangent) = path_midpoint(path);
    let normal = normalize(point(-tangent.y, tangent.x));
    center = offset(center, normal, (font_size * 0.9).max(8.0));
    let transform = VisualTransform {
        x: center.x,
        y: center.y,
        width: path_length(path, false).clamp(48.0, 240.0),
        height: ((font_size * 1.25).max(1.0) * MAX_VISUAL_CONNECTOR_LABEL_LINES as f64)
            .clamp(24.0, 8_192.0),
        rotation_degrees: 0.0,
    };
    let (prepared, truncated) = prepare_text(label, transform, font_size);
    let actual_width = prepared
        .lines
        .iter()
        .map(|line| line.reserved_width)
        .fold(0.0, f64::max);
    let bounds = rotated_rect_bounds(center, actual_width, prepared.height, 0.0);
    (Some(prepared), Some(transform), Some(bounds), truncated)
}

fn path_midpoint(path: &[ScenePoint]) -> (ScenePoint, ScenePoint) {
    let total = path_length(path, false);
    if path.len() < 2 || total <= f64::EPSILON {
        return (path.first().copied().unwrap_or_default(), point(1.0, 0.0));
    }
    let mut remaining = total / 2.0;
    for pair in path.windows(2) {
        let segment = distance(pair[0], pair[1]);
        if remaining <= segment {
            let tangent = normalize(point(pair[1].x - pair[0].x, pair[1].y - pair[0].y));
            return (
                point(
                    pair[0].x + tangent.x * remaining,
                    pair[0].y + tangent.y * remaining,
                ),
                tangent,
            );
        }
        remaining -= segment;
    }
    let last = path[path.len() - 1];
    let previous = path[path.len() - 2];
    (
        last,
        normalize(point(last.x - previous.x, last.y - previous.y)),
    )
}

fn sample_cubic(
    start: ScenePoint,
    control1: ScenePoint,
    control2: ScenePoint,
    end: ScenePoint,
    segments: usize,
) -> Vec<ScenePoint> {
    (0..=segments)
        .map(|step| {
            let t = step as f64 / segments.max(1) as f64;
            let inverse = 1.0 - t;
            point(
                start.x * inverse.powi(3)
                    + 3.0 * control1.x * inverse.powi(2) * t
                    + 3.0 * control2.x * inverse * t.powi(2)
                    + end.x * t.powi(3),
                start.y * inverse.powi(3)
                    + 3.0 * control1.y * inverse.powi(2) * t
                    + 3.0 * control2.y * inverse * t.powi(2)
                    + end.y * t.powi(3),
            )
        })
        .collect()
}

fn curved_control_distance(start: ScenePoint, end: ScenePoint) -> f64 {
    (distance(start, end) / 3.0).clamp(16.0, 128.0)
}

fn bounds_for_open_paths(paths: &[Vec<ScenePoint>]) -> Option<SceneRect> {
    paths
        .iter()
        .flatten()
        .copied()
        .filter(|point| point.x.is_finite() && point.y.is_finite())
        .map(|point| SceneRect {
            min: point,
            max: point,
        })
        .reduce(union)
}

fn clean_path(points: Vec<ScenePoint>) -> Vec<ScenePoint> {
    let mut clean = Vec::with_capacity(points.len());
    for point in points {
        if clean
            .last()
            .is_none_or(|previous: &ScenePoint| distance(*previous, point) > f64::EPSILON)
        {
            clean.push(point);
        }
    }
    clean
}

fn add(left: ScenePoint, right: ScenePoint) -> ScenePoint {
    point(left.x + right.x, left.y + right.y)
}

fn offset(origin: ScenePoint, direction: ScenePoint, amount: f64) -> ScenePoint {
    point(
        origin.x + direction.x * amount,
        origin.y + direction.y * amount,
    )
}

fn dot(left: ScenePoint, right: ScenePoint) -> f64 {
    left.x * right.x + left.y * right.y
}

fn distance(left: ScenePoint, right: ScenePoint) -> f64 {
    (left.x - right.x).hypot(left.y - right.y)
}

fn closest_point_on_segment(query: ScenePoint, start: ScenePoint, end: ScenePoint) -> ScenePoint {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared <= f64::EPSILON {
        return start;
    }
    let amount =
        (((query.x - start.x) * dx + (query.y - start.y) * dy) / length_squared).clamp(0.0, 1.0);
    point(start.x + dx * amount, start.y + dy * amount)
}

fn normalize(vector: ScenePoint) -> ScenePoint {
    let length = vector.x.hypot(vector.y);
    if length.is_finite() && length > f64::EPSILON {
        point(vector.x / length, vector.y / length)
    } else {
        point(1.0, 0.0)
    }
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

fn build_frame_item(
    object_id: VisualObjectId,
    layer_id: VisualLayerId,
    kind: VisualProjectedKind,
    transform: VisualTransform,
    style: VisualStyle,
    label: &str,
    vertical_header: bool,
) -> Result<ProjectedVisualItem, VisualProjectionError> {
    let mut item = build_item(
        object_id,
        None,
        layer_id,
        kind,
        transform,
        style.clone(),
        None,
        false,
    )?;
    let header_extent = if vertical_header {
        (transform.width * 0.22).min(28.0)
    } else {
        (transform.height * 0.28).min(28.0)
    };
    let header = if vertical_header {
        vec![
            point(-transform.width / 2.0, -transform.height / 2.0),
            point(
                -transform.width / 2.0 + header_extent,
                -transform.height / 2.0,
            ),
            point(
                -transform.width / 2.0 + header_extent,
                transform.height / 2.0,
            ),
            point(-transform.width / 2.0, transform.height / 2.0),
        ]
    } else {
        vec![
            point(-transform.width / 2.0, -transform.height / 2.0),
            point(transform.width / 2.0, -transform.height / 2.0),
            point(
                transform.width / 2.0,
                -transform.height / 2.0 + header_extent,
            ),
            point(
                -transform.width / 2.0,
                -transform.height / 2.0 + header_extent,
            ),
        ]
    }
    .into_iter()
    .map(|point| rotate_translate(point, transform))
    .collect::<Vec<_>>();
    let divider = if vertical_header {
        let x = -transform.width / 2.0 + header_extent;
        vec![
            rotate_translate(point(x, -transform.height / 2.0), transform),
            rotate_translate(point(x, transform.height / 2.0), transform),
        ]
    } else {
        let y = -transform.height / 2.0 + header_extent;
        vec![
            rotate_translate(point(-transform.width / 2.0, y), transform),
            rotate_translate(point(transform.width / 2.0, y), transform),
        ]
    };
    item.bounds = union(
        item.bounds,
        expand(
            bounds_for_points(&divider).ok_or_else(|| {
                VisualProjectionError::InvalidScene("frame header is empty".into())
            })?,
            style.border_width / 2.0,
        ),
    );
    item.details.push(divider);
    item.header = Some(header);

    let text_transform = if vertical_header {
        let local_center = point(-transform.width / 2.0 + header_extent / 2.0, 0.0);
        VisualTransform {
            x: rotate_translate(local_center, transform).x,
            y: rotate_translate(local_center, transform).y,
            width: (transform.height - 12.0).max(1.0),
            height: (header_extent - 4.0).max(1.0),
            rotation_degrees: transform.rotation_degrees - 90.0,
        }
    } else {
        let local_center = point(0.0, -transform.height / 2.0 + header_extent / 2.0);
        let center = rotate_translate(local_center, transform);
        VisualTransform {
            x: center.x,
            y: center.y,
            width: (transform.width - 12.0).max(1.0),
            height: (header_extent - 4.0).max(1.0),
            rotation_degrees: transform.rotation_degrees,
        }
    };
    let (prepared, truncated) = prepare_text(label, text_transform, style.font_size);
    let text_bounds = rotated_rect_bounds(
        point(text_transform.x, text_transform.y),
        prepared.reserved_width,
        prepared.height,
        text_transform.rotation_degrees,
    );
    item.bounds = union(item.bounds, text_bounds);
    item.text = Some(prepared);
    item.text_transform = Some(text_transform);
    item.text_bounds = Some(text_bounds);
    item.text_rotation_degrees = text_transform.rotation_degrees;
    item.text_truncated = truncated;
    Ok(item)
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
        image_asset: None,
        transform,
        style,
        outline,
        header: None,
        details,
        open_paths: Vec::new(),
        text: prepared_text,
        text_transform: None,
        text_bounds: None,
        text_truncated,
        text_rotation_degrees: transform.rotation_degrees,
        bounds: item_bounds,
        dash_simplified: false,
        dash_paths: None,
        route_fallback: false,
    })
}

fn kind_shape(kind: VisualProjectedKind) -> VisualPrimitiveShape {
    match kind {
        VisualProjectedKind::Primitive(shape) | VisualProjectedKind::PresetPrimitive(shape) => {
            shape
        }
        VisualProjectedKind::Text
        | VisualProjectedKind::Image
        | VisualProjectedKind::MissingImagePlaceholder
        | VisualProjectedKind::Container
        | VisualProjectedKind::SwimlaneHorizontal
        | VisualProjectedKind::SwimlaneVertical
        | VisualProjectedKind::Connector => VisualPrimitiveShape::Rectangle,
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
    if item.kind == VisualProjectedKind::Connector {
        let mut remaining = max_segments;
        return item
            .open_paths
            .iter()
            .map(|path| project_dash_path(path, false, item.style.line_style, &mut remaining))
            .collect();
    }
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
        CONNECTOR_CLEARANCE, MAX_VISUAL_CONNECTOR_LABEL_LINES, MAX_VISUAL_DASH_SEGMENTS_PER_ITEM,
        MAX_VISUAL_PROJECTED_ITEMS, MAX_VISUAL_PROJECTION_COORDINATE_ABS, VisualProjectedKind,
        bounds_for_open_paths, closest_point_on_segment, distance, expand, project_visual_scene,
        segment_crosses_rect,
    };
    use crate::diagram::{
        DEFAULT_LAYER_ID, ScenePoint, SceneRect, VisualAnchor, VisualAssetId, VisualColor,
        VisualConnector, VisualConnectorDirection, VisualConnectorEndpoint, VisualConnectorRoute,
        VisualDiagramDocument, VisualObjectDraft, VisualObjectId, VisualObjectKind,
        VisualOperation, VisualPresetDraft, VisualPrimitiveComponent, VisualPrimitiveShape,
        VisualShadow, VisualStyle, VisualSwimlaneOrientation, VisualTransform,
    };

    #[test]
    fn visual_container_projection_builds_rotated_headers_and_ignores_unrelated_frames_for_routes()
    {
        let mut document = VisualDiagramDocument::new();
        let mut title = document.scene().objects[0].clone();
        title.transform.x = 2_000.0;
        title.transform.y = 2_000.0;
        let ids = document
            .apply_transaction(&[
                VisualOperation::ReplaceObject(title),
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: 0.0,
                        y: 0.0,
                        width: 120.0,
                        height: 80.0,
                        rotation_degrees: 25.0,
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Container {
                        label: "Team".into(),
                    },
                )),
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: 0.0,
                        y: 180.0,
                        width: 120.0,
                        height: 180.0,
                        rotation_degrees: -15.0,
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Swimlane {
                        orientation: VisualSwimlaneOrientation::Vertical,
                        label: "Column".into(),
                    },
                )),
            ])
            .unwrap()
            .created_objects;
        let projection = project_visual_scene(document.scene()).unwrap();
        for (id, expected_kind) in [
            (ids[0], VisualProjectedKind::Container),
            (ids[1], VisualProjectedKind::SwimlaneVertical),
        ] {
            let item = projection
                .items
                .iter()
                .find(|item| item.object_id == id)
                .unwrap();
            assert_eq!(item.kind, expected_kind);
            assert_eq!(item.header.as_ref().unwrap().len(), 4);
            assert!(!item.details.is_empty());
            assert!(item.text_bounds.is_some());
            assert!(
                item.outline
                    .iter()
                    .all(|point| point.x.is_finite() && point.y.is_finite())
            );
        }

        let mut routing = VisualDiagramDocument::new();
        let mut title = routing.scene().objects[0].clone();
        title.transform.x = 2_000.0;
        title.transform.y = 2_000.0;
        let endpoints = routing
            .apply_transaction(&[
                VisualOperation::ReplaceObject(title),
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: -180.0,
                        width: 60.0,
                        height: 40.0,
                        ..VisualTransform::default()
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Container {
                        label: "Source frame".into(),
                    },
                )),
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: 180.0,
                        width: 60.0,
                        height: 40.0,
                        ..VisualTransform::default()
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Swimlane {
                        orientation: VisualSwimlaneOrientation::Horizontal,
                        label: "Target frame".into(),
                    },
                )),
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        width: 100.0,
                        height: 100.0,
                        ..VisualTransform::default()
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Container {
                        label: "Transparent route area".into(),
                    },
                )),
            ])
            .unwrap()
            .created_objects;
        assert_eq!(endpoints.len(), 3);
        routing
            .apply_transaction(&[VisualOperation::AddObject(draft(
                VisualTransform::default(),
                VisualStyle::default(),
                VisualObjectKind::Connector {
                    connector: VisualConnector {
                        manual_route: None,
                        from: VisualConnectorEndpoint {
                            object_id: endpoints[0],
                            anchor: VisualAnchor::East,
                        },
                        to: VisualConnectorEndpoint {
                            object_id: endpoints[1],
                            anchor: VisualAnchor::West,
                        },
                        route: VisualConnectorRoute::Elbow,
                        direction: VisualConnectorDirection::None,
                        label: String::new(),
                    },
                },
            ))])
            .unwrap();
        let projection = project_visual_scene(routing.scene()).unwrap();
        let connector = projection
            .items
            .iter()
            .find(|item| item.kind == VisualProjectedKind::Connector)
            .unwrap();
        assert!(!connector.route_fallback);
        let unrelated_frame = SceneRect {
            min: ScenePoint { x: -50.0, y: -50.0 },
            max: ScenePoint { x: 50.0, y: 50.0 },
        };
        assert!(
            connector.open_paths[0]
                .windows(2)
                .any(|segment| segment_crosses_rect(segment[0], segment[1], unrelated_frame))
        );
    }

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
    fn visual_manual_route_projection_keeps_anchors_and_seed_parity() {
        let a = ScenePoint { x: 0.0, y: 0.0 };
        let b = ScenePoint { x: 10.0, y: 0.0 };
        assert_eq!(
            super::connector_arrowhead(&[a, b, b], false),
            super::connector_arrowhead(&[a, b], false)
        );
        assert!(super::connector_arrowhead(&[a, a], false).is_empty());
        let mut doc = VisualDiagramDocument::new();
        let ids = doc
            .apply_transaction(&[
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: -200.0,
                        rotation_degrees: 35.0,
                        ..base_transform(100.0, 70.0)
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Diamond,
                    },
                )),
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: 250.0,
                        ..base_transform(100.0, 70.0)
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Ellipse,
                    },
                )),
            ])
            .unwrap()
            .created_objects;
        let make_connector = |to| VisualObjectKind::Connector {
            connector: VisualConnector {
                from: VisualConnectorEndpoint {
                    object_id: ids[0],
                    anchor: VisualAnchor::NorthEast,
                },
                to: VisualConnectorEndpoint {
                    object_id: to,
                    anchor: VisualAnchor::West,
                },
                route: VisualConnectorRoute::Curved,
                direction: VisualConnectorDirection::Forward,
                label: "curve".into(),
                manual_route: None,
            },
        };
        let edges = doc
            .apply_transaction(&[
                VisualOperation::AddObject(draft(
                    VisualTransform::default(),
                    VisualStyle::default(),
                    make_connector(ids[1]),
                )),
                VisualOperation::AddObject(draft(
                    VisualTransform::default(),
                    VisualStyle::default(),
                    make_connector(ids[1]),
                )),
                VisualOperation::AddObject(draft(
                    VisualTransform::default(),
                    VisualStyle::default(),
                    make_connector(ids[0]),
                )),
            ])
            .unwrap()
            .created_objects;
        let automatic = project_visual_scene(doc.scene()).unwrap();
        let mut labeled = doc.clone();
        let mut shape = labeled
            .scene()
            .objects
            .iter()
            .find(|object| object.id == ids[0])
            .unwrap()
            .clone();
        shape.owned_label = Some(crate::diagram::VisualOwnedLabel {
            text: "Owned".into(),
            style: VisualStyle::default(),
        });
        labeled
            .apply_transaction(&[VisualOperation::ReplaceObject(shape)])
            .unwrap();
        let labeled_projection = project_visual_scene(labeled.scene()).unwrap();
        for id in [edges[0], edges[1], edges[2]] {
            let unlabeled_path = &automatic
                .items
                .iter()
                .find(|item| item.object_id == id)
                .unwrap()
                .open_paths[0];
            let labeled_path = &labeled_projection
                .items
                .iter()
                .find(|item| item.object_id == id)
                .unwrap()
                .open_paths[0];
            assert_eq!(unlabeled_path.first(), labeled_path.first());
            assert_eq!(unlabeled_path.last(), labeled_path.last());
        }
        let mut free_text = doc.clone();
        let mut text_object = free_text
            .scene()
            .objects
            .iter()
            .find(|object| object.id == ids[0])
            .unwrap()
            .clone();
        text_object.kind = VisualObjectKind::Text {
            text: "independent".into(),
        };
        free_text
            .apply_transaction(&[VisualOperation::ReplaceObject(text_object)])
            .unwrap();
        assert!(
            project_visual_scene(free_text.scene())
                .unwrap()
                .items
                .iter()
                .any(|item| item.object_id == edges[0] && !item.open_paths[0].is_empty())
        );
        for id in [edges[0], edges[2]] {
            let item = automatic
                .items
                .iter()
                .find(|item| item.object_id == id)
                .unwrap();
            let VisualObjectKind::Connector { connector } = &doc
                .scene()
                .objects
                .iter()
                .find(|object| object.id == id)
                .unwrap()
                .kind
            else {
                unreachable!()
            };
            if item.route_fallback {
                assert!(super::seed_manual_route(connector, item).is_none());
                continue;
            }
            let route = super::seed_manual_route(connector, item).unwrap();
            let mut candidate = doc.clone();
            candidate
                .apply_transaction(&[VisualOperation::SetConnectorManualRoute {
                    id,
                    route: Some(route),
                }])
                .unwrap();
            let manual = project_visual_scene(candidate.scene()).unwrap();
            let path = &manual
                .items
                .iter()
                .find(|item| item.object_id == id)
                .unwrap()
                .open_paths[0];
            for (left, right) in path.iter().zip(&item.open_paths[0]) {
                assert!((left.x - right.x).abs() < 1e-7 && (left.y - right.y).abs() < 1e-7);
            }
        }
        let item = automatic
            .items
            .iter()
            .find(|item| item.object_id == edges[0])
            .unwrap();
        let VisualObjectKind::Connector { connector } = &doc
            .scene()
            .objects
            .iter()
            .find(|object| object.id == edges[0])
            .unwrap()
            .kind
        else {
            unreachable!()
        };
        let mut fallback = item.clone();
        fallback.route_fallback = true;
        assert!(super::seed_manual_route(connector, &fallback).is_none());
        let route = super::seed_manual_route(connector, item).unwrap();
        doc.apply_transaction(&[VisualOperation::SetConnectorManualRoute {
            id: edges[0],
            route: Some(route.clone()),
        }])
        .unwrap();
        let before = project_visual_scene(doc.scene()).unwrap();
        let mut endpoint = doc
            .scene()
            .objects
            .iter()
            .find(|object| object.id == ids[0])
            .unwrap()
            .clone();
        endpoint.transform.x -= 50.0;
        doc.apply_transaction(&[VisualOperation::ReplaceObject(endpoint)])
            .unwrap();
        let after = project_visual_scene(doc.scene()).unwrap();
        let before_path = &before
            .items
            .iter()
            .find(|item| item.object_id == edges[0])
            .unwrap()
            .open_paths[0];
        let after_path = &after
            .items
            .iter()
            .find(|item| item.object_id == edges[0])
            .unwrap()
            .open_paths[0];
        assert_ne!(before_path[0], after_path[0]);
        let VisualObjectKind::Connector { connector } = &doc
            .scene()
            .objects
            .iter()
            .find(|object| object.id == edges[0])
            .unwrap()
            .kind
        else {
            unreachable!()
        };
        assert_eq!(connector.manual_route, Some(route));
        assert_ne!(
            after
                .items
                .iter()
                .find(|item| item.object_id == edges[0])
                .unwrap()
                .open_paths[0],
            after
                .items
                .iter()
                .find(|item| item.object_id == edges[1])
                .unwrap()
                .open_paths[0]
        );
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
    fn visual_connector_uses_rotated_local_side_anchors_and_open_arrowheads() {
        let mut document = VisualDiagramDocument::new();
        let endpoints = document
            .apply_transaction(&[
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: -100.0,
                        y: 40.0,
                        width: 80.0,
                        height: 40.0,
                        rotation_degrees: 90.0,
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle,
                    },
                )),
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: 100.0,
                        y: 40.0,
                        width: 80.0,
                        height: 40.0,
                        rotation_degrees: 0.0,
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Ellipse,
                    },
                )),
            ])
            .unwrap()
            .created_objects;
        let connector_id = document
            .apply_transaction(&[VisualOperation::AddObject(draft(
                VisualTransform::default(),
                VisualStyle::default(),
                VisualObjectKind::Connector {
                    connector: VisualConnector {
                        manual_route: None,
                        from: VisualConnectorEndpoint {
                            object_id: endpoints[0],
                            anchor: VisualAnchor::East,
                        },
                        to: VisualConnectorEndpoint {
                            object_id: endpoints[1],
                            anchor: VisualAnchor::West,
                        },
                        route: VisualConnectorRoute::Straight,
                        direction: VisualConnectorDirection::Both,
                        label: String::new(),
                    },
                },
            ))])
            .unwrap()
            .created_objects[0];

        let projection = project_visual_scene(document.scene()).unwrap();
        let connector = projection
            .items
            .iter()
            .find(|item| item.object_id == connector_id)
            .unwrap();
        assert_eq!(connector.kind, VisualProjectedKind::Connector);
        assert_eq!(connector.open_paths.len(), 3);
        let route = &connector.open_paths[0];
        assert!((route[0].x + 100.0).abs() < 1e-8);
        assert!((route[0].y - 80.0).abs() < 1e-8);
        assert!((route.last().unwrap().x - 60.0).abs() < 1e-8);
        assert!((route.last().unwrap().y - 40.0).abs() < 1e-8);
    }

    #[test]
    fn visual_connector_preset_anchor_uses_a_real_component_outline() {
        let mut document = VisualDiagramDocument::new();
        let preset = document
            .apply_transaction(&[VisualOperation::AddPreset(VisualPresetDraft {
                name: "Split marker".into(),
                components: vec![
                    VisualPrimitiveComponent {
                        shape: VisualPrimitiveShape::Rectangle,
                        transform: VisualTransform {
                            x: -25.0,
                            width: 20.0,
                            height: 30.0,
                            ..VisualTransform::default()
                        },
                        style: VisualStyle::default(),
                    },
                    VisualPrimitiveComponent {
                        shape: VisualPrimitiveShape::Rectangle,
                        transform: VisualTransform {
                            x: 25.0,
                            width: 20.0,
                            height: 30.0,
                            ..VisualTransform::default()
                        },
                        style: VisualStyle::default(),
                    },
                ],
            })])
            .unwrap()
            .created_presets[0];
        let source = document
            .apply_transaction(&[
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: -100.0,
                        width: 100.0,
                        height: 100.0,
                        ..VisualTransform::default()
                    },
                    VisualStyle::default(),
                    VisualObjectKind::PresetInstance { preset_id: preset },
                )),
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: 120.0,
                        width: 40.0,
                        height: 40.0,
                        ..VisualTransform::default()
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle,
                    },
                )),
            ])
            .unwrap()
            .created_objects;
        let connector_id = document
            .apply_transaction(&[VisualOperation::AddObject(draft(
                VisualTransform::default(),
                VisualStyle::default(),
                VisualObjectKind::Connector {
                    connector: VisualConnector {
                        manual_route: None,
                        from: VisualConnectorEndpoint {
                            object_id: source[0],
                            anchor: VisualAnchor::East,
                        },
                        to: VisualConnectorEndpoint {
                            object_id: source[1],
                            anchor: VisualAnchor::West,
                        },
                        route: VisualConnectorRoute::Straight,
                        direction: VisualConnectorDirection::None,
                        label: String::new(),
                    },
                },
            ))])
            .unwrap()
            .created_objects[0];

        let projection = project_visual_scene(document.scene()).unwrap();
        let connector = projection
            .items
            .iter()
            .find(|item| item.object_id == connector_id)
            .unwrap();
        let anchor = connector.open_paths[0][0];
        let component = projection
            .items
            .iter()
            .find(|item| item.object_id == source[0] && item.component_index == Some(1))
            .unwrap();
        assert!((anchor.x - component.bounds.max.x).abs() < 1.0);
        assert!((0..component.outline.len()).any(|index| {
            let start = component.outline[index];
            let end = component.outline[(index + 1) % component.outline.len()];
            distance(closest_point_on_segment(anchor, start, end), anchor) < 1.0
        }));
    }

    #[test]
    fn visual_connector_elbow_egresses_owners_and_routes_around_obstacles() {
        let mut document = VisualDiagramDocument::new();
        let ids = document
            .apply_transaction(&[
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: -100.0,
                        y: 300.0,
                        width: 40.0,
                        height: 40.0,
                        ..VisualTransform::default()
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle,
                    },
                )),
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: 100.0,
                        y: 300.0,
                        width: 40.0,
                        height: 40.0,
                        ..VisualTransform::default()
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle,
                    },
                )),
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: 0.0,
                        y: 300.0,
                        width: 40.0,
                        height: 90.0,
                        ..VisualTransform::default()
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Cylinder,
                    },
                )),
            ])
            .unwrap()
            .created_objects;
        let connector_id = document
            .apply_transaction(&[VisualOperation::AddObject(draft(
                VisualTransform::default(),
                VisualStyle::default(),
                VisualObjectKind::Connector {
                    connector: VisualConnector {
                        manual_route: None,
                        from: VisualConnectorEndpoint {
                            object_id: ids[0],
                            anchor: VisualAnchor::East,
                        },
                        to: VisualConnectorEndpoint {
                            object_id: ids[1],
                            anchor: VisualAnchor::West,
                        },
                        route: VisualConnectorRoute::Elbow,
                        direction: VisualConnectorDirection::None,
                        label: String::new(),
                    },
                },
            ))])
            .unwrap()
            .created_objects[0];

        let projection = project_visual_scene(document.scene()).unwrap();
        let connector = projection
            .items
            .iter()
            .find(|item| item.object_id == connector_id)
            .unwrap();
        assert!(!connector.route_fallback, "a clear bounded route was found");
        let route = &connector.open_paths[0];
        assert!(route.len() > 4, "the obstacle causes a route detour");
        assert!(route.windows(2).all(|pair| {
            (pair[0].x - pair[1].x).abs() < 1e-8 || (pair[0].y - pair[1].y).abs() < 1e-8
        }));
        let obstacle = projection
            .items
            .iter()
            .find(|item| item.object_id == ids[2])
            .unwrap()
            .bounds;
        let obstacle = expand(obstacle, CONNECTOR_CLEARANCE + 0.1);
        assert!(
            route
                .windows(2)
                .all(|pair| !segment_crosses_rect(pair[0], pair[1], obstacle))
        );
    }

    #[test]
    fn visual_connector_rotated_parallel_elbows_keep_orthogonal_egress() {
        let mut document = VisualDiagramDocument::new();
        let endpoints = document
            .apply_transaction(&[
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: -120.0,
                        y: 320.0,
                        width: 70.0,
                        height: 100.0,
                        rotation_degrees: 45.0,
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle,
                    },
                )),
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: 120.0,
                        y: 320.0,
                        width: 70.0,
                        height: 100.0,
                        rotation_degrees: -45.0,
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::RoundRect,
                    },
                )),
            ])
            .unwrap()
            .created_objects;
        let connector_draft = || {
            draft(
                VisualTransform::default(),
                VisualStyle::default(),
                VisualObjectKind::Connector {
                    connector: VisualConnector {
                        manual_route: None,
                        from: VisualConnectorEndpoint {
                            object_id: endpoints[0],
                            anchor: VisualAnchor::East,
                        },
                        to: VisualConnectorEndpoint {
                            object_id: endpoints[1],
                            anchor: VisualAnchor::West,
                        },
                        route: VisualConnectorRoute::Elbow,
                        direction: VisualConnectorDirection::None,
                        label: String::new(),
                    },
                },
            )
        };
        let connector_ids = document
            .apply_transaction(&[
                VisualOperation::AddObject(connector_draft()),
                VisualOperation::AddObject(connector_draft()),
            ])
            .unwrap()
            .created_objects;
        let projection = project_visual_scene(document.scene()).unwrap();
        let routed = connector_ids
            .iter()
            .map(|id| {
                projection
                    .items
                    .iter()
                    .find(|item| item.object_id == *id)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert!(routed.iter().all(|item| !item.route_fallback));
        for item in &routed {
            let path = &item.open_paths[0];
            assert!(path.windows(2).all(|pair| {
                (pair[0].x - pair[1].x).abs() < 1e-8 || (pair[0].y - pair[1].y).abs() < 1e-8
            }));
        }
        assert_ne!(routed[0].open_paths[0], routed[1].open_paths[0]);
    }

    #[test]
    fn visual_connector_coincident_straight_route_is_visible_and_flagged() {
        let mut document = VisualDiagramDocument::new();
        let endpoints = document
            .apply_transaction(&[
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: -20.0,
                        width: 40.0,
                        height: 40.0,
                        ..VisualTransform::default()
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle,
                    },
                )),
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: 20.0,
                        width: 40.0,
                        height: 40.0,
                        ..VisualTransform::default()
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle,
                    },
                )),
            ])
            .unwrap()
            .created_objects;
        let connector_id = document
            .apply_transaction(&[VisualOperation::AddObject(draft(
                VisualTransform::default(),
                VisualStyle::default(),
                VisualObjectKind::Connector {
                    connector: VisualConnector {
                        manual_route: None,
                        from: VisualConnectorEndpoint {
                            object_id: endpoints[0],
                            anchor: VisualAnchor::East,
                        },
                        to: VisualConnectorEndpoint {
                            object_id: endpoints[1],
                            anchor: VisualAnchor::West,
                        },
                        route: VisualConnectorRoute::Straight,
                        direction: VisualConnectorDirection::None,
                        label: String::new(),
                    },
                },
            ))])
            .unwrap()
            .created_objects[0];
        let projection = project_visual_scene(document.scene()).unwrap();
        let connector = projection
            .items
            .iter()
            .find(|item| item.object_id == connector_id)
            .unwrap();
        assert!(connector.route_fallback);
        assert!(connector.open_paths[0].len() >= 4);
        assert!(connector.bounds.width() > 10.0 || connector.bounds.height() > 10.0);
    }

    #[test]
    fn visual_connector_parallel_edges_and_self_loops_are_distinguishable() {
        let mut document = VisualDiagramDocument::new();
        let endpoints = document
            .apply_transaction(&[
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: -100.0,
                        y: 300.0,
                        width: 40.0,
                        height: 80.0,
                        ..VisualTransform::default()
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle,
                    },
                )),
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: 100.0,
                        y: 300.0,
                        width: 40.0,
                        height: 80.0,
                        ..VisualTransform::default()
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle,
                    },
                )),
            ])
            .unwrap()
            .created_objects;
        let make_connector = |from, from_anchor, to, to_anchor, route| {
            VisualOperation::AddObject(draft(
                VisualTransform::default(),
                VisualStyle::default(),
                VisualObjectKind::Connector {
                    connector: VisualConnector {
                        manual_route: None,
                        from: VisualConnectorEndpoint {
                            object_id: from,
                            anchor: from_anchor,
                        },
                        to: VisualConnectorEndpoint {
                            object_id: to,
                            anchor: to_anchor,
                        },
                        route,
                        direction: VisualConnectorDirection::None,
                        label: String::new(),
                    },
                },
            ))
        };
        let connector_ids = document
            .apply_transaction(&[
                make_connector(
                    endpoints[0],
                    VisualAnchor::East,
                    endpoints[1],
                    VisualAnchor::West,
                    VisualConnectorRoute::Straight,
                ),
                make_connector(
                    endpoints[1],
                    VisualAnchor::West,
                    endpoints[0],
                    VisualAnchor::East,
                    VisualConnectorRoute::Straight,
                ),
                make_connector(
                    endpoints[0],
                    VisualAnchor::East,
                    endpoints[1],
                    VisualAnchor::West,
                    VisualConnectorRoute::Straight,
                ),
                make_connector(
                    endpoints[0],
                    VisualAnchor::East,
                    endpoints[0],
                    VisualAnchor::East,
                    VisualConnectorRoute::Curved,
                ),
                make_connector(
                    endpoints[0],
                    VisualAnchor::East,
                    endpoints[0],
                    VisualAnchor::West,
                    VisualConnectorRoute::Curved,
                ),
            ])
            .unwrap()
            .created_objects;

        let projection = project_visual_scene(document.scene()).unwrap();
        let routes = connector_ids[..3]
            .iter()
            .map(|id| {
                &projection
                    .items
                    .iter()
                    .find(|item| item.object_id == *id)
                    .unwrap()
                    .open_paths[0]
            })
            .collect::<Vec<_>>();
        assert_ne!(routes[0][0], routes[1][0]);
        assert_ne!(routes[0][0], routes[2][0]);
        assert_ne!(routes[1][0], routes[2][0]);
        let self_loop = projection
            .items
            .iter()
            .find(|item| item.object_id == connector_ids[3])
            .unwrap();
        assert!(!self_loop.route_fallback);
        assert!(self_loop.open_paths[0].len() > 8);
        let self_bounds = bounds_for_open_paths(&self_loop.open_paths).unwrap();
        let owner_bounds = projection
            .items
            .iter()
            .find(|item| item.object_id == endpoints[0])
            .unwrap()
            .bounds;
        assert!(self_bounds.max.x > owner_bounds.max.x + 20.0);
        assert!(self_bounds.height() > 10.0);
        let interior_path = &self_loop.open_paths[0];
        assert!(
            interior_path
                .windows(2)
                .enumerate()
                .filter(|(index, _)| *index != 0 && *index + 1 < interior_path.len() - 1)
                .all(|(_, segment)| !segment_crosses_rect(
                    segment[0],
                    segment[1],
                    expand(owner_bounds, 0.1)
                ))
        );

        let opposite_loop = projection
            .items
            .iter()
            .find(|item| item.object_id == connector_ids[4])
            .unwrap();
        assert!(
            opposite_loop.route_fallback,
            "a curved route that would cross its owner is shown as a fallback"
        );
        assert!(opposite_loop.open_paths[0].len() > 4);
    }

    #[test]
    fn visual_connector_label_projection_is_bounded_by_eight_lines() {
        let mut document = VisualDiagramDocument::new();
        let endpoints = document
            .apply_transaction(&[
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: -100.0,
                        y: 300.0,
                        ..base_transform(40.0, 40.0)
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle,
                    },
                )),
                VisualOperation::AddObject(draft(
                    VisualTransform {
                        x: 100.0,
                        y: 300.0,
                        ..base_transform(40.0, 40.0)
                    },
                    VisualStyle::default(),
                    VisualObjectKind::Primitive {
                        shape: VisualPrimitiveShape::Rectangle,
                    },
                )),
            ])
            .unwrap()
            .created_objects;
        let connector_id = document
            .apply_transaction(&[VisualOperation::AddObject(draft(
                VisualTransform::default(),
                VisualStyle {
                    font_size: 4.0,
                    ..VisualStyle::default()
                },
                VisualObjectKind::Connector {
                    connector: VisualConnector {
                        manual_route: None,
                        from: VisualConnectorEndpoint {
                            object_id: endpoints[0],
                            anchor: VisualAnchor::East,
                        },
                        to: VisualConnectorEndpoint {
                            object_id: endpoints[1],
                            anchor: VisualAnchor::West,
                        },
                        route: VisualConnectorRoute::Straight,
                        direction: VisualConnectorDirection::None,
                        label: "long connector label ".repeat(60),
                    },
                },
            ))])
            .unwrap()
            .created_objects[0];
        let projection = project_visual_scene(document.scene()).unwrap();
        let connector = projection
            .items
            .iter()
            .find(|item| item.object_id == connector_id)
            .unwrap();
        let text = connector.text.as_ref().unwrap();
        assert!(text.lines.len() <= MAX_VISUAL_CONNECTOR_LABEL_LINES);
        assert!(connector.text_truncated);
        let bounds = connector.text_bounds.unwrap();
        assert!((bounds.width() - text.reserved_width).abs() < 1e-8);
        assert!(bounds.width() <= 240.0);
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
