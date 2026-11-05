//! Pure selection, mapping, and transform geometry for the visual canvas.

use super::scene::{ScenePoint, SceneRect};
use super::visual::{
    MAX_VISUAL_COORDINATE_ABS, MAX_VISUAL_DIMENSION, MAX_VISUAL_TRANSACTION_OPERATIONS,
    MIN_VISUAL_DIMENSION, VisualObject, VisualObjectId, VisualOperation, VisualScene,
    VisualTransform,
};
use super::visual_scene::VisualSceneProjection;
use std::collections::{BTreeMap, BTreeSet};

const HANDLE_WORLD_GAP_SCREEN_POINTS: f64 = 22.0;
pub const VISUAL_LAYOUT_GRID_SPACING: f64 = 10.0;
pub const VISUAL_GUIDE_TOLERANCE_SCREEN_POINTS: f64 = 8.0;
const MINIMAP_SCREEN_MARGIN: f64 = 5.0;
const MINIMAP_MAX_ITEMS: usize = 128;
const MINIMAP_MAX_OUTLINE_POINTS: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasMapping {
    /// Full logical canvas area used as the camera transform origin.
    pub viewport: SceneRect,
    /// Portion of the canvas currently visible and interactive after clipping.
    pub clip: SceneRect,
    pub center: ScenePoint,
    pub zoom: f64,
}

impl CanvasMapping {
    pub fn world_to_screen(self, world: ScenePoint) -> ScenePoint {
        let center = rect_center(self.viewport);
        ScenePoint {
            x: center.x + (world.x - self.center.x) * self.zoom,
            y: center.y + (world.y - self.center.y) * self.zoom,
        }
    }

    pub fn screen_to_world(self, screen: ScenePoint) -> ScenePoint {
        let center = rect_center(self.viewport);
        ScenePoint {
            x: self.center.x + (screen.x - center.x) / self.zoom,
            y: self.center.y + (screen.y - center.y) / self.zoom,
        }
    }

    pub fn contains_screen_point(self, point: ScenePoint) -> bool {
        self.clip.contains_point(point)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanvasHandleKind {
    ResizeCorner {
        object_id: VisualObjectId,
        sign_x: i8,
        sign_y: i8,
    },
    RotateObject {
        object_id: VisualObjectId,
    },
    ResizeSelection,
    RotateSelection,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasHandle {
    pub kind: CanvasHandleKind,
    pub center: ScenePoint,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CanvasHit {
    Handle(CanvasHandleKind),
    Object(VisualObjectId),
    Empty,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VisualTransformMode {
    Translate,
    ResizeCorner { sign_x: i8, sign_y: i8 },
    RotateObject,
    RotateSelection { pivot: ScenePoint },
    ResizeSelection { pivot: ScenePoint },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CanvasGuideAxis {
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CanvasAnchor {
    Min,
    Center,
    Max,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasGuide {
    pub axis: CanvasGuideAxis,
    pub coordinate: f64,
    pub target_object: VisualObjectId,
    pub moving_anchor: CanvasAnchor,
    pub target_anchor: CanvasAnchor,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SnappedTranslation {
    pub delta: ScenePoint,
    pub guides: Vec<CanvasGuide>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualAlign {
    Left,
    HorizontalCenter,
    Right,
    Top,
    VerticalCenter,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualDistribution {
    Horizontal,
    Vertical,
}

/// Bounded minimap geometry shared by the editor and read-only viewer.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasMinimap {
    pub frame: SceneRect,
    pub outlines: Vec<Vec<ScenePoint>>,
    pub viewport: SceneRect,
    world_center: ScenePoint,
    screen_center: ScenePoint,
    scale: f64,
}

impl CanvasMinimap {
    pub fn screen_to_world(&self, point: ScenePoint) -> Option<ScenePoint> {
        if !self.frame.contains_point(point) || !self.scale.is_finite() || self.scale <= 0.0 {
            return None;
        }
        let world = ScenePoint {
            x: self.world_center.x + (point.x - self.screen_center.x) / self.scale,
            y: self.world_center.y + (point.y - self.screen_center.y) / self.scale,
        };
        (world.x.is_finite() && world.y.is_finite()).then_some(world)
    }
}

/// Compute a single translation delta for a selection from its press-time bounds.
/// Nearby guides are measured against the raw pointer movement; otherwise the
/// group's minimum edge snaps to the shared ten-world-unit grid.
pub fn snap_translation_delta(
    projection: &VisualSceneProjection,
    selected: &BTreeSet<VisualObjectId>,
    painted_item_prefix: usize,
    raw_delta: ScenePoint,
    zoom: f64,
) -> Option<SnappedTranslation> {
    if selected.is_empty()
        || selected.len() > MAX_VISUAL_TRANSACTION_OPERATIONS
        || !raw_delta.x.is_finite()
        || !raw_delta.y.is_finite()
        || !zoom.is_finite()
        || zoom <= 0.0
    {
        return None;
    }
    if raw_delta.x == 0.0 && raw_delta.y == 0.0 {
        return Some(SnappedTranslation {
            delta: ScenePoint::default(),
            guides: Vec::new(),
        });
    }
    let bounds_by_object = visible_outline_bounds(projection, painted_item_prefix);
    let all_object_bounds = visible_outline_bounds(projection, projection.items.len());
    let selection_bounds = selected_outline_bounds(selected, &all_object_bounds)?;
    let tolerance = VISUAL_GUIDE_TOLERANCE_SCREEN_POINTS / zoom;
    let x_guide = nearest_guide(
        CanvasGuideAxis::Vertical,
        selection_bounds.min.x,
        selection_bounds.max.x,
        raw_delta.x,
        tolerance,
        selected,
        &bounds_by_object,
    );
    let y_guide = nearest_guide(
        CanvasGuideAxis::Horizontal,
        selection_bounds.min.y,
        selection_bounds.max.y,
        raw_delta.y,
        tolerance,
        selected,
        &bounds_by_object,
    );
    let x_delta = x_guide.map_or_else(
        || grid_delta(selection_bounds.min.x, raw_delta.x),
        |(delta, _)| Some(delta),
    )?;
    let y_delta = y_guide.map_or_else(
        || grid_delta(selection_bounds.min.y, raw_delta.y),
        |(delta, _)| Some(delta),
    )?;
    Some(SnappedTranslation {
        delta: ScenePoint {
            x: x_delta,
            y: y_delta,
        },
        guides: x_guide
            .map(|(_, guide)| guide)
            .into_iter()
            .chain(y_guide.map(|(_, guide)| guide))
            .collect(),
    })
}

fn grid_delta(anchor: f64, raw_delta: f64) -> Option<f64> {
    let raw_position = anchor + raw_delta;
    let snapped_position =
        (raw_position / VISUAL_LAYOUT_GRID_SPACING).round() * VISUAL_LAYOUT_GRID_SPACING;
    let delta = snapped_position - anchor;
    delta.is_finite().then_some(delta)
}

fn nearest_guide(
    axis: CanvasGuideAxis,
    selection_min: f64,
    selection_max: f64,
    raw_delta: f64,
    tolerance: f64,
    selected: &BTreeSet<VisualObjectId>,
    bounds_by_object: &BTreeMap<VisualObjectId, SceneRect>,
) -> Option<(f64, CanvasGuide)> {
    let moving_anchors = [
        (CanvasAnchor::Min, selection_min),
        (
            CanvasAnchor::Center,
            selection_min + (selection_max - selection_min) / 2.0,
        ),
        (CanvasAnchor::Max, selection_max),
    ];
    let mut best: Option<(f64, f64, CanvasGuide)> = None;
    for (object_id, bounds) in bounds_by_object {
        if selected.contains(object_id) {
            continue;
        }
        let (target_min, target_max) = match axis {
            CanvasGuideAxis::Vertical => (bounds.min.x, bounds.max.x),
            CanvasGuideAxis::Horizontal => (bounds.min.y, bounds.max.y),
        };
        let target_anchors = [
            (CanvasAnchor::Min, target_min),
            (
                CanvasAnchor::Center,
                target_min + (target_max - target_min) / 2.0,
            ),
            (CanvasAnchor::Max, target_max),
        ];
        for (moving_anchor, coordinate) in moving_anchors {
            let moved_coordinate = coordinate + raw_delta;
            for (target_anchor, target_coordinate) in target_anchors {
                let distance = (target_coordinate - moved_coordinate).abs();
                if distance > tolerance {
                    continue;
                }
                let candidate = CanvasGuide {
                    axis,
                    coordinate: target_coordinate,
                    target_object: *object_id,
                    moving_anchor,
                    target_anchor,
                };
                let replace = best.as_ref().is_none_or(|(best_distance, _, best_guide)| {
                    distance
                        .total_cmp(best_distance)
                        .then_with(|| candidate.target_object.cmp(&best_guide.target_object))
                        .then_with(|| candidate.target_anchor.cmp(&best_guide.target_anchor))
                        .then_with(|| candidate.moving_anchor.cmp(&best_guide.moving_anchor))
                        .is_lt()
                });
                if replace {
                    let delta = target_coordinate - coordinate;
                    if delta.is_finite() {
                        best = Some((distance, delta, candidate));
                    }
                }
            }
        }
    }
    best.map(|(_, delta, guide)| (delta, guide))
}

/// Return one operation per changed object while preserving stack order,
/// dimensions, rotation, style, and stable identity.
pub fn align_selection(
    scene: &VisualScene,
    projection: &VisualSceneProjection,
    selection: &BTreeSet<VisualObjectId>,
    painted_item_prefix: usize,
    alignment: VisualAlign,
) -> Option<Vec<VisualOperation>> {
    if selection.len() < 2 || selection.len() > MAX_VISUAL_TRANSACTION_OPERATIONS {
        return None;
    }
    let bounds_by_object = visible_outline_bounds(projection, painted_item_prefix);
    let selected_bounds = selected_outline_bounds(selection, &bounds_by_object)?;
    let mut operations = Vec::with_capacity(selection.len());
    for id in selection {
        let bounds = *bounds_by_object.get(id)?;
        let (dx, dy) = match alignment {
            VisualAlign::Left => (selected_bounds.min.x - bounds.min.x, 0.0),
            VisualAlign::HorizontalCenter => (selected_bounds.center().x - bounds.center().x, 0.0),
            VisualAlign::Right => (selected_bounds.max.x - bounds.max.x, 0.0),
            VisualAlign::Top => (0.0, selected_bounds.min.y - bounds.min.y),
            VisualAlign::VerticalCenter => (0.0, selected_bounds.center().y - bounds.center().y),
            VisualAlign::Bottom => (0.0, selected_bounds.max.y - bounds.max.y),
        };
        if dx == 0.0 && dy == 0.0 {
            continue;
        }
        let mut object = scene
            .objects
            .iter()
            .find(|object| object.id == *id)?
            .clone();
        object.transform.x += dx;
        object.transform.y += dy;
        if !valid_transform(object.transform) {
            return None;
        }
        operations.push(VisualOperation::ReplaceObject(object));
    }
    Some(operations)
}

/// Distribute selected object outline edges with equal gaps. The two outer
/// objects remain fixed; the intervening objects are stable-sorted by their
/// leading edge and ID.
pub fn distribute_selection(
    scene: &VisualScene,
    projection: &VisualSceneProjection,
    selection: &BTreeSet<VisualObjectId>,
    painted_item_prefix: usize,
    distribution: VisualDistribution,
) -> Option<Vec<VisualOperation>> {
    if selection.len() < 3 || selection.len() > MAX_VISUAL_TRANSACTION_OPERATIONS {
        return None;
    }
    let bounds_by_object = visible_outline_bounds(projection, painted_item_prefix);
    let mut objects = selection
        .iter()
        .map(|id| Some((*id, *bounds_by_object.get(id)?)))
        .collect::<Option<Vec<_>>>()?;
    let horizontal = distribution == VisualDistribution::Horizontal;
    let leading_edge = |bounds: &SceneRect| {
        if horizontal {
            bounds.min.x
        } else {
            bounds.min.y
        }
    };
    let trailing_edge = |bounds: &SceneRect| {
        if horizontal {
            bounds.max.x
        } else {
            bounds.max.y
        }
    };
    objects.sort_by(|(left_id, left), (right_id, right)| {
        leading_edge(left)
            .total_cmp(&leading_edge(right))
            .then_with(|| left_id.cmp(right_id))
    });
    let first = *objects.first()?;
    let last = *objects
        .iter()
        .max_by(|(left_id, left), (right_id, right)| {
            trailing_edge(left)
                .total_cmp(&trailing_edge(right))
                .then_with(|| right_id.cmp(left_id))
        })?;
    if first.0 == last.0 {
        return Some(Vec::new());
    }
    let mut ordered = Vec::with_capacity(objects.len());
    ordered.push(first);
    ordered.extend(
        objects
            .into_iter()
            .filter(|(id, _)| *id != first.0 && *id != last.0),
    );
    ordered.push(last);
    let outer_min = leading_edge(&first.1);
    let outer_max = trailing_edge(&last.1);
    let total_object_extent = ordered
        .iter()
        .map(|(_, bounds)| trailing_edge(bounds) - leading_edge(bounds))
        .sum::<f64>();
    let gap = (outer_max - outer_min - total_object_extent) / (ordered.len() - 1) as f64;
    if !gap.is_finite() {
        return None;
    }
    let mut cursor = trailing_edge(&first.1);
    let mut operations = Vec::with_capacity(ordered.len().saturating_sub(2));
    for (id, bounds) in ordered.iter().skip(1).take(ordered.len() - 2) {
        let leading = leading_edge(bounds);
        let trailing = trailing_edge(bounds);
        let target_min = cursor + gap;
        let delta = target_min - leading;
        cursor = target_min + (trailing - leading);
        if delta == 0.0 {
            continue;
        }
        let mut object = scene
            .objects
            .iter()
            .find(|object| object.id == *id)?
            .clone();
        if horizontal {
            object.transform.x += delta;
        } else {
            object.transform.y += delta;
        }
        if !valid_transform(object.transform) {
            return None;
        }
        operations.push(VisualOperation::ReplaceObject(object));
    }
    Some(operations)
}

pub fn minimap_layout(
    projection: &VisualSceneProjection,
    visible_world: SceneRect,
    frame: SceneRect,
) -> Option<CanvasMinimap> {
    if !valid_rect(visible_world) || !valid_rect(frame) {
        return None;
    }
    let frame_width = frame.max.x - frame.min.x;
    let frame_height = frame.max.y - frame.min.y;
    let available_width = frame_width - MINIMAP_SCREEN_MARGIN * 2.0;
    let available_height = frame_height - MINIMAP_SCREEN_MARGIN * 2.0;
    if available_width <= 0.0 || available_height <= 0.0 {
        return None;
    }
    let mut world_bounds = projection
        .bounds
        .filter(|bounds| valid_rect(*bounds))
        .map_or(visible_world, |bounds| union_rect(bounds, visible_world));
    if world_bounds.width() <= f64::EPSILON {
        let center = world_bounds.center().x;
        world_bounds.min.x = center - 0.5;
        world_bounds.max.x = center + 0.5;
    }
    if world_bounds.height() <= f64::EPSILON {
        let center = world_bounds.center().y;
        world_bounds.min.y = center - 0.5;
        world_bounds.max.y = center + 0.5;
    }
    let world_width = world_bounds.max.x - world_bounds.min.x;
    let world_height = world_bounds.max.y - world_bounds.min.y;
    let scale = (available_width / world_width).min(available_height / world_height);
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let world_center = rect_center(world_bounds);
    let screen_center = rect_center(frame);
    let map_point = |point: ScenePoint| ScenePoint {
        x: screen_center.x + (point.x - world_center.x) * scale,
        y: screen_center.y + (point.y - world_center.y) * scale,
    };
    let mut remaining_points = MINIMAP_MAX_OUTLINE_POINTS;
    let mut outlines = Vec::new();
    for item in projection.items.iter().take(MINIMAP_MAX_ITEMS) {
        if item.outline.len() < 3 || item.outline.len() > remaining_points {
            break;
        }
        let outline = item
            .outline
            .iter()
            .copied()
            .map(map_point)
            .collect::<Vec<_>>();
        if outline
            .iter()
            .any(|point| !point.x.is_finite() || !point.y.is_finite())
        {
            return None;
        }
        remaining_points -= outline.len();
        outlines.push(outline);
        if remaining_points == 0 {
            break;
        }
    }
    let viewport = normalized_rect(SceneRect {
        min: map_point(visible_world.min),
        max: map_point(visible_world.max),
    });
    valid_rect(viewport).then_some(CanvasMinimap {
        frame,
        outlines,
        viewport,
        world_center,
        screen_center,
        scale,
    })
}

fn selected_outline_bounds(
    selection: &BTreeSet<VisualObjectId>,
    bounds_by_object: &BTreeMap<VisualObjectId, SceneRect>,
) -> Option<SceneRect> {
    selection
        .iter()
        .map(|id| bounds_by_object.get(id).copied())
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .reduce(union_rect)
}

fn visible_outline_bounds(
    projection: &VisualSceneProjection,
    painted_item_prefix: usize,
) -> BTreeMap<VisualObjectId, SceneRect> {
    let mut prefix = painted_item_prefix.min(projection.items.len());
    if prefix < projection.items.len() && prefix > 0 {
        let partial_owner = projection.items[prefix].object_id;
        while prefix > 0 && projection.items[prefix - 1].object_id == partial_owner {
            prefix -= 1;
        }
    }
    let mut bounds = BTreeMap::new();
    for item in projection.items.iter().take(prefix) {
        for point in &item.outline {
            if !point.x.is_finite() || !point.y.is_finite() {
                continue;
            }
            bounds
                .entry(item.object_id)
                .and_modify(|rect: &mut SceneRect| {
                    rect.min.x = rect.min.x.min(point.x);
                    rect.min.y = rect.min.y.min(point.y);
                    rect.max.x = rect.max.x.max(point.x);
                    rect.max.y = rect.max.y.max(point.y);
                })
                .or_insert(SceneRect {
                    min: *point,
                    max: *point,
                });
        }
    }
    bounds
}

fn valid_rect(rect: SceneRect) -> bool {
    rect.min.x.is_finite()
        && rect.min.y.is_finite()
        && rect.max.x.is_finite()
        && rect.max.y.is_finite()
        && rect.min.x <= rect.max.x
        && rect.min.y <= rect.max.y
}

pub fn selection_handles(
    scene: &VisualScene,
    projection: &VisualSceneProjection,
    selection: &BTreeSet<VisualObjectId>,
    painted_item_prefix: usize,
    mapping: CanvasMapping,
) -> Vec<CanvasHandle> {
    if selection.is_empty()
        || selection.iter().any(|id| {
            let mut projected_items = projection
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.object_id == *id)
                .peekable();
            projected_items.peek().is_none()
                || projected_items.any(|(index, _)| index >= painted_item_prefix)
        })
    {
        return Vec::new();
    }
    let gap = HANDLE_WORLD_GAP_SCREEN_POINTS / mapping.zoom.max(f64::MIN_POSITIVE);
    if selection.len() == 1 {
        let object_id = *selection.first().expect("selection has one item");
        let Some(object) = scene.objects.iter().find(|object| object.id == object_id) else {
            return Vec::new();
        };
        let transform = object.transform;
        let mut handles = Vec::with_capacity(5);
        for (sign_x, sign_y) in [(-1, -1), (1, -1), (1, 1), (-1, 1)] {
            let local = ScenePoint {
                x: f64::from(sign_x) * transform.width / 2.0,
                y: f64::from(sign_y) * transform.height / 2.0,
            };
            handles.push(CanvasHandle {
                kind: CanvasHandleKind::ResizeCorner {
                    object_id,
                    sign_x,
                    sign_y,
                },
                center: transform_point(transform, local),
            });
        }
        let rotation_handle = transform_point(
            transform,
            ScenePoint {
                x: 0.0,
                y: -transform.height / 2.0 - gap,
            },
        );
        handles.push(CanvasHandle {
            kind: CanvasHandleKind::RotateObject { object_id },
            center: rotation_handle,
        });
        return handles;
    }

    let Some(bounds) = selection_bounds(projection, selection, painted_item_prefix) else {
        return Vec::new();
    };
    group_handles(bounds, gap)
}

fn group_handles(bounds: SceneRect, gap: f64) -> Vec<CanvasHandle> {
    vec![
        CanvasHandle {
            kind: CanvasHandleKind::RotateSelection,
            center: ScenePoint {
                x: (bounds.min.x + bounds.max.x) / 2.0,
                y: bounds.min.y - gap,
            },
        },
        CanvasHandle {
            kind: CanvasHandleKind::ResizeSelection,
            center: bounds.max,
        },
    ]
}

pub fn hit_test(
    projection: &VisualSceneProjection,
    painted_item_prefix: usize,
    handles: &[CanvasHandle],
    mapping: CanvasMapping,
    screen_point: ScenePoint,
    handle_radius_screen_points: f64,
) -> CanvasHit {
    if !mapping.contains_screen_point(screen_point) {
        return CanvasHit::Empty;
    }
    let radius_squared = handle_radius_screen_points * handle_radius_screen_points;
    if let Some(handle) = handles.iter().rev().find(|handle| {
        let center = mapping.world_to_screen(handle.center);
        squared_distance(center, screen_point) <= radius_squared
    }) {
        return CanvasHit::Handle(handle.kind);
    }
    let world_point = mapping.screen_to_world(screen_point);
    projection
        .items
        .iter()
        .take(painted_item_prefix)
        .rev()
        .find(|item| {
            item.bounds.contains_point(world_point) && point_in_polygon(world_point, &item.outline)
        })
        .map_or(CanvasHit::Empty, |item| CanvasHit::Object(item.object_id))
}

pub fn marquee_selection(
    projection: &VisualSceneProjection,
    painted_item_prefix: usize,
    marquee: SceneRect,
) -> Vec<VisualObjectId> {
    let marquee = normalized_rect(marquee);
    let mut selected = Vec::new();
    for item in projection.items.iter().take(painted_item_prefix) {
        if rects_intersect(item.bounds, marquee) && !selected.contains(&item.object_id) {
            selected.push(item.object_id);
        }
    }
    selected
}

pub fn selection_bounds(
    projection: &VisualSceneProjection,
    selection: &BTreeSet<VisualObjectId>,
    painted_item_prefix: usize,
) -> Option<SceneRect> {
    projection
        .items
        .iter()
        .take(painted_item_prefix)
        .filter(|item| selection.contains(&item.object_id))
        .map(|item| item.bounds)
        .reduce(union_rect)
}

pub fn preview_transforms(
    initial_objects: &[VisualObject],
    mode: VisualTransformMode,
    pointer_start: ScenePoint,
    pointer_current: ScenePoint,
) -> Option<Vec<VisualObject>> {
    if initial_objects.is_empty() || initial_objects.len() > MAX_VISUAL_TRANSACTION_OPERATIONS {
        return None;
    }
    let delta = ScenePoint {
        x: pointer_current.x - pointer_start.x,
        y: pointer_current.y - pointer_start.y,
    };
    let mut objects = initial_objects.to_vec();
    match mode {
        VisualTransformMode::Translate => {
            for object in &mut objects {
                object.transform.x += delta.x;
                object.transform.y += delta.y;
            }
        }
        VisualTransformMode::ResizeCorner { sign_x, sign_y } => {
            if objects.len() != 1 || !matches!(sign_x, -1 | 1) || !matches!(sign_y, -1 | 1) {
                return None;
            }
            let transform = &mut objects[0].transform;
            let local_delta = rotate(delta, -transform.rotation_degrees.to_radians());
            let width = (transform.width + f64::from(sign_x) * local_delta.x)
                .clamp(MIN_VISUAL_DIMENSION, MAX_VISUAL_DIMENSION);
            let height = (transform.height + f64::from(sign_y) * local_delta.y)
                .clamp(MIN_VISUAL_DIMENSION, MAX_VISUAL_DIMENSION);
            let center_shift = rotate(
                ScenePoint {
                    x: f64::from(sign_x) * (width - transform.width) / 2.0,
                    y: f64::from(sign_y) * (height - transform.height) / 2.0,
                },
                transform.rotation_degrees.to_radians(),
            );
            transform.x += center_shift.x;
            transform.y += center_shift.y;
            transform.width = width;
            transform.height = height;
        }
        VisualTransformMode::RotateObject => {
            if objects.len() != 1 {
                return None;
            }
            let transform = &mut objects[0].transform;
            let angle = angle_delta(
                ScenePoint {
                    x: transform.x,
                    y: transform.y,
                },
                pointer_start,
                pointer_current,
            )?;
            if angle.abs() > 1e-12 {
                transform.rotation_degrees =
                    normalize_degrees(transform.rotation_degrees + angle.to_degrees());
            }
        }
        VisualTransformMode::RotateSelection { pivot } => {
            if objects.len() < 2 {
                return None;
            }
            let angle = angle_delta(pivot, pointer_start, pointer_current)?;
            if angle.abs() <= 1e-12 {
                return Some(objects);
            }
            for object in &mut objects {
                let center = rotate(
                    ScenePoint {
                        x: object.transform.x - pivot.x,
                        y: object.transform.y - pivot.y,
                    },
                    angle,
                );
                object.transform.x = pivot.x + center.x;
                object.transform.y = pivot.y + center.y;
                object.transform.rotation_degrees =
                    normalize_degrees(object.transform.rotation_degrees + angle.to_degrees());
            }
        }
        VisualTransformMode::ResizeSelection { pivot } => {
            if objects.len() < 2 {
                return None;
            }
            let start_distance = distance(pointer_start, pivot);
            if start_distance <= f64::EPSILON {
                return None;
            }
            let mut factor = distance(pointer_current, pivot) / start_distance;
            if (factor - 1.0).abs() <= 1e-12 {
                return Some(objects);
            }
            let minimum_factor = objects
                .iter()
                .map(|object| {
                    (MIN_VISUAL_DIMENSION / object.transform.width)
                        .max(MIN_VISUAL_DIMENSION / object.transform.height)
                })
                .fold(0.0_f64, f64::max);
            let maximum_factor = objects
                .iter()
                .map(|object| {
                    (MAX_VISUAL_DIMENSION / object.transform.width)
                        .min(MAX_VISUAL_DIMENSION / object.transform.height)
                })
                .fold(f64::INFINITY, f64::min);
            factor = factor.clamp(minimum_factor, maximum_factor);
            for object in &mut objects {
                object.transform.x = pivot.x + (object.transform.x - pivot.x) * factor;
                object.transform.y = pivot.y + (object.transform.y - pivot.y) * factor;
                object.transform.width *= factor;
                object.transform.height *= factor;
            }
        }
    }
    objects
        .iter()
        .all(|object| valid_transform(object.transform))
        .then_some(objects)
}

fn valid_transform(transform: VisualTransform) -> bool {
    transform.x.is_finite()
        && transform.y.is_finite()
        && transform.width.is_finite()
        && transform.height.is_finite()
        && transform.rotation_degrees.is_finite()
        && transform.x.abs() <= MAX_VISUAL_COORDINATE_ABS
        && transform.y.abs() <= MAX_VISUAL_COORDINATE_ABS
        && (MIN_VISUAL_DIMENSION..=MAX_VISUAL_DIMENSION).contains(&transform.width)
        && (MIN_VISUAL_DIMENSION..=MAX_VISUAL_DIMENSION).contains(&transform.height)
        && transform.rotation_degrees.abs() <= 36_000.0
}

fn transform_point(transform: VisualTransform, local: ScenePoint) -> ScenePoint {
    let rotated = rotate(local, transform.rotation_degrees.to_radians());
    ScenePoint {
        x: transform.x + rotated.x,
        y: transform.y + rotated.y,
    }
}

fn rotate(point: ScenePoint, angle: f64) -> ScenePoint {
    let (sin, cos) = angle.sin_cos();
    ScenePoint {
        x: point.x * cos - point.y * sin,
        y: point.x * sin + point.y * cos,
    }
}

fn angle_delta(pivot: ScenePoint, start: ScenePoint, current: ScenePoint) -> Option<f64> {
    let start_angle = (start.y - pivot.y).atan2(start.x - pivot.x);
    let current_angle = (current.y - pivot.y).atan2(current.x - pivot.x);
    let delta = (current_angle - start_angle + std::f64::consts::PI)
        .rem_euclid(2.0 * std::f64::consts::PI)
        - std::f64::consts::PI;
    delta.is_finite().then_some(delta)
}

fn normalize_degrees(degrees: f64) -> f64 {
    (degrees + 180.0).rem_euclid(360.0) - 180.0
}

fn point_in_polygon(point: ScenePoint, polygon: &[ScenePoint]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut previous = *polygon.last().expect("polygon has points");
    for current in polygon.iter().copied() {
        if point_on_segment(point, previous, current) {
            return true;
        }
        if (current.y > point.y) != (previous.y > point.y) {
            let crossing_x = (previous.x - current.x) * (point.y - current.y)
                / (previous.y - current.y)
                + current.x;
            if point.x < crossing_x {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

fn point_on_segment(point: ScenePoint, start: ScenePoint, end: ScenePoint) -> bool {
    let cross = (point.y - start.y) * (end.x - start.x) - (point.x - start.x) * (end.y - start.y);
    if cross.abs() > 1e-9 {
        return false;
    }
    point.x >= start.x.min(end.x) - 1e-9
        && point.x <= start.x.max(end.x) + 1e-9
        && point.y >= start.y.min(end.y) - 1e-9
        && point.y <= start.y.max(end.y) + 1e-9
}

fn rect_center(rect: SceneRect) -> ScenePoint {
    ScenePoint {
        x: (rect.min.x + rect.max.x) / 2.0,
        y: (rect.min.y + rect.max.y) / 2.0,
    }
}

fn rects_intersect(left: SceneRect, right: SceneRect) -> bool {
    left.min.x <= right.max.x
        && left.max.x >= right.min.x
        && left.min.y <= right.max.y
        && left.max.y >= right.min.y
}

fn normalized_rect(rect: SceneRect) -> SceneRect {
    SceneRect {
        min: ScenePoint {
            x: rect.min.x.min(rect.max.x),
            y: rect.min.y.min(rect.max.y),
        },
        max: ScenePoint {
            x: rect.min.x.max(rect.max.x),
            y: rect.min.y.max(rect.max.y),
        },
    }
}

fn union_rect(left: SceneRect, right: SceneRect) -> SceneRect {
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

fn squared_distance(a: ScenePoint, b: ScenePoint) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

fn distance(a: ScenePoint, b: ScenePoint) -> f64 {
    squared_distance(a, b).sqrt()
}

#[cfg(test)]
mod tests {
    use super::{
        CanvasAnchor, CanvasGuideAxis, CanvasHandle, CanvasHandleKind, CanvasHit, CanvasMapping,
        VisualAlign, VisualDistribution, VisualTransformMode, align_selection,
        distribute_selection, hit_test, marquee_selection, minimap_layout, preview_transforms,
        selection_bounds, selection_handles, snap_translation_delta, visible_outline_bounds,
    };
    use crate::diagram::{
        DEFAULT_LAYER_ID, MAX_VISUAL_COORDINATE_ABS, ScenePoint, SceneRect, VisualDiagramDocument,
        VisualObject, VisualObjectDraft, VisualObjectKind, VisualOperation, VisualPresetDraft,
        VisualPrimitiveComponent, VisualPrimitiveShape, VisualSceneProjection, VisualShadow,
        VisualStyle, VisualTransform, project_visual_scene,
    };
    use std::collections::{BTreeMap, BTreeSet};

    fn object(transform: VisualTransform, shape: VisualPrimitiveShape) -> VisualObjectDraft {
        VisualObjectDraft {
            layer_id: DEFAULT_LAYER_ID,
            transform,
            style: VisualStyle::default(),
            kind: VisualObjectKind::Primitive { shape },
        }
    }

    fn added_object(
        document: &mut VisualDiagramDocument,
        transform: VisualTransform,
    ) -> VisualObject {
        let id = document
            .apply_transaction(&[VisualOperation::AddObject(object(
                transform,
                VisualPrimitiveShape::Rectangle,
            ))])
            .unwrap()
            .created_objects[0];
        document
            .scene()
            .objects
            .iter()
            .find(|object| object.id == id)
            .unwrap()
            .clone()
    }

    fn map() -> CanvasMapping {
        CanvasMapping {
            viewport: SceneRect {
                min: ScenePoint { x: 0.0, y: 0.0 },
                max: ScenePoint { x: 400.0, y: 300.0 },
            },
            clip: SceneRect {
                min: ScenePoint { x: 0.0, y: 0.0 },
                max: ScenePoint { x: 400.0, y: 300.0 },
            },
            center: ScenePoint::default(),
            zoom: 2.0,
        }
    }

    #[test]
    fn visual_canvas_geometry_maps_scaled_coordinates_and_rejects_outside_hits() {
        let mapping = map();
        let world = ScenePoint { x: 10.0, y: -5.0 };
        let screen = mapping.world_to_screen(world);
        assert_eq!(screen, ScenePoint { x: 220.0, y: 140.0 });
        assert_eq!(mapping.screen_to_world(screen), world);
        assert!(!mapping.contains_screen_point(ScenePoint { x: 401.0, y: 150.0 }));
        let clipped = CanvasMapping {
            clip: SceneRect {
                min: ScenePoint { x: 50.0, y: 40.0 },
                max: ScenePoint { x: 350.0, y: 260.0 },
            },
            ..mapping
        };
        assert_eq!(clipped.world_to_screen(world), screen);
        assert!(!clipped.contains_screen_point(ScenePoint { x: 20.0, y: 150.0 }));
    }

    #[test]
    fn visual_canvas_geometry_picks_rotated_topmost_outlines_and_preset_owner() {
        let mut document = VisualDiagramDocument::new();
        let first = document
            .apply_transaction(&[
                VisualOperation::AddObject(object(
                    VisualTransform {
                        width: 20.0,
                        height: 10.0,
                        rotation_degrees: 45.0,
                        ..VisualTransform::default()
                    },
                    VisualPrimitiveShape::Rectangle,
                )),
                VisualOperation::AddObject(object(
                    VisualTransform {
                        width: 8.0,
                        height: 8.0,
                        ..VisualTransform::default()
                    },
                    VisualPrimitiveShape::Ellipse,
                )),
            ])
            .unwrap()
            .created_objects;
        let mut title = document
            .scene()
            .objects
            .iter()
            .find(|object| object.id == crate::diagram::TITLE_OBJECT_ID)
            .unwrap()
            .clone();
        title.transform.x = 500.0;
        title.transform.y = 500.0;
        document
            .apply_transaction(&[VisualOperation::ReplaceObject(title)])
            .unwrap();
        let projection = project_visual_scene(document.scene()).unwrap();
        let prefix = projection.items.len();
        let mapping = map();
        let overlapping = mapping.world_to_screen(ScenePoint::default());
        assert_eq!(
            hit_test(&projection, prefix, &[], mapping, overlapping, 8.0),
            CanvasHit::Object(first[1])
        );

        let rotated_corner_only_in_aabb = mapping.world_to_screen(ScenePoint { x: 9.0, y: 9.0 });
        assert_eq!(
            hit_test(
                &projection,
                2,
                &[],
                mapping,
                rotated_corner_only_in_aabb,
                8.0,
            ),
            CanvasHit::Empty
        );

        let preset = document
            .apply_transaction(&[VisualOperation::AddPreset(VisualPresetDraft {
                name: "Pick parts".into(),
                components: vec![
                    VisualPrimitiveComponent {
                        shape: VisualPrimitiveShape::Diamond,
                        transform: VisualTransform {
                            x: -20.0,
                            width: 20.0,
                            height: 20.0,
                            ..VisualTransform::default()
                        },
                        style: VisualStyle::default(),
                    },
                    VisualPrimitiveComponent {
                        shape: VisualPrimitiveShape::Rectangle,
                        transform: VisualTransform {
                            x: 20.0,
                            width: 20.0,
                            height: 20.0,
                            ..VisualTransform::default()
                        },
                        style: VisualStyle::default(),
                    },
                ],
            })])
            .unwrap()
            .created_presets[0];
        let owner = document
            .apply_transaction(&[VisualOperation::AddObject(VisualObjectDraft {
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform {
                    x: 40.0,
                    y: 30.0,
                    width: 100.0,
                    height: 100.0,
                    rotation_degrees: 0.0,
                },
                style: VisualStyle::default(),
                kind: VisualObjectKind::PresetInstance { preset_id: preset },
            })])
            .unwrap()
            .created_objects[0];
        let mut title = document
            .scene()
            .objects
            .iter()
            .find(|object| object.id == crate::diagram::TITLE_OBJECT_ID)
            .unwrap()
            .clone();
        title.transform.x = 500.0;
        title.transform.y = 500.0;
        document
            .apply_transaction(&[VisualOperation::ReplaceObject(title)])
            .unwrap();
        let projection = project_visual_scene(document.scene()).unwrap();
        let prefix = projection.items.len();
        let part_center = mapping.world_to_screen(ScenePoint { x: 20.0, y: 30.0 });
        assert_eq!(
            hit_test(&projection, prefix, &[], mapping, part_center, 8.0),
            CanvasHit::Object(owner)
        );
        let partial_prefix = prefix - 1;
        let omitted_part_center = mapping.world_to_screen(ScenePoint { x: 60.0, y: 30.0 });
        assert_eq!(
            hit_test(
                &projection,
                partial_prefix,
                &[],
                mapping,
                omitted_part_center,
                8.0,
            ),
            CanvasHit::Empty
        );
        let owner_selection = [owner].into_iter().collect::<BTreeSet<_>>();
        assert!(
            selection_handles(
                document.scene(),
                &projection,
                &owner_selection,
                partial_prefix,
                mapping,
            )
            .is_empty()
        );
    }

    #[test]
    fn visual_canvas_geometry_handles_precede_bodies_and_marquee_uses_visible_bounds() {
        let mut document = VisualDiagramDocument::new();
        let id = document
            .apply_transaction(&[VisualOperation::AddObject(object(
                VisualTransform {
                    x: 10.0,
                    width: 20.0,
                    height: 10.0,
                    rotation_degrees: 30.0,
                    ..VisualTransform::default()
                },
                VisualPrimitiveShape::Rectangle,
            ))])
            .unwrap()
            .created_objects[0];
        let mut title = document
            .scene()
            .objects
            .iter()
            .find(|object| object.id == crate::diagram::TITLE_OBJECT_ID)
            .unwrap()
            .clone();
        title.transform.x = 500.0;
        title.transform.y = 500.0;
        document
            .apply_transaction(&[VisualOperation::ReplaceObject(title)])
            .unwrap();
        let projection = project_visual_scene(document.scene()).unwrap();
        let prefix = projection.items.len();
        let selection = [id].into_iter().collect::<BTreeSet<_>>();
        let mapping = map();
        let handles = selection_handles(document.scene(), &projection, &selection, prefix, mapping);
        assert_eq!(handles.len(), 5);
        let corner = handles[0];
        let screen = mapping.world_to_screen(corner.center);
        assert_eq!(
            hit_test(&projection, prefix, &handles, mapping, screen, 8.0),
            CanvasHit::Handle(corner.kind)
        );

        let selected = marquee_selection(
            &projection,
            prefix,
            SceneRect {
                min: ScenePoint { x: 15.0, y: -1.0 },
                max: ScenePoint { x: 8.0, y: 1.0 },
            },
        );
        assert_eq!(selected, vec![id]);
        assert!(marquee_selection(&projection, 0, projection.items[1].bounds).is_empty());
    }

    #[test]
    fn visual_canvas_geometry_previews_local_resize_and_single_rotation() {
        let mut document = VisualDiagramDocument::new();
        let initial = added_object(
            &mut document,
            VisualTransform {
                width: 20.0,
                height: 10.0,
                rotation_degrees: 25.0,
                ..VisualTransform::default()
            },
        );
        let resize_initial = added_object(
            &mut document,
            VisualTransform {
                width: 20.0,
                height: 10.0,
                rotation_degrees: 90.0,
                ..VisualTransform::default()
            },
        );
        let resized = preview_transforms(
            std::slice::from_ref(&resize_initial),
            VisualTransformMode::ResizeCorner {
                sign_x: 1,
                sign_y: 1,
            },
            ScenePoint { x: 0.0, y: 0.0 },
            ScenePoint { x: -2.0, y: 4.0 },
        )
        .unwrap();
        assert_eq!(resized[0].transform.width, 24.0);
        assert_eq!(resized[0].transform.height, 12.0);
        assert!((resized[0].transform.x + 1.0).abs() < 1e-9);
        assert!((resized[0].transform.y - 2.0).abs() < 1e-9);

        let rotated = preview_transforms(
            std::slice::from_ref(&initial),
            VisualTransformMode::RotateObject,
            ScenePoint { x: 10.0, y: 0.0 },
            ScenePoint { x: 0.0, y: 10.0 },
        )
        .unwrap();
        assert!((rotated[0].transform.rotation_degrees - 115.0).abs() < 1e-9);
        let unchanged = preview_transforms(
            std::slice::from_ref(&initial),
            VisualTransformMode::RotateObject,
            ScenePoint { x: 0.0, y: 10.0 },
            ScenePoint { x: 0.0, y: 10.0 },
        )
        .unwrap();
        assert_eq!(unchanged[0], initial);
    }

    #[test]
    fn visual_canvas_geometry_group_rotation_and_uniform_resize_preserve_arrangement() {
        let mut document = VisualDiagramDocument::new();
        let initial = [-10.0, 10.0]
            .into_iter()
            .map(|x| {
                added_object(
                    &mut document,
                    VisualTransform {
                        x,
                        y: 0.0,
                        width: 10.0,
                        height: 20.0,
                        rotation_degrees: 15.0,
                    },
                )
            })
            .collect::<Vec<_>>();
        let pivot = ScenePoint::default();
        let rotated = preview_transforms(
            &initial,
            VisualTransformMode::RotateSelection { pivot },
            ScenePoint { x: 10.0, y: 0.0 },
            ScenePoint { x: 0.0, y: 10.0 },
        )
        .unwrap();
        assert!((rotated[0].transform.x).abs() < 1e-9);
        assert!((rotated[0].transform.y + 10.0).abs() < 1e-9);
        assert!((rotated[0].transform.rotation_degrees - 105.0).abs() < 1e-9);

        let resized = preview_transforms(
            &initial,
            VisualTransformMode::ResizeSelection { pivot },
            ScenePoint { x: 10.0, y: 0.0 },
            ScenePoint { x: 20.0, y: 0.0 },
        )
        .unwrap();
        assert_eq!(resized[0].transform.x, -20.0);
        assert_eq!(resized[1].transform.x, 20.0);
        assert_eq!(resized[0].transform.width, 20.0);
        assert_eq!(resized[0].transform.height, 40.0);
        assert_eq!(
            preview_transforms(
                &initial,
                VisualTransformMode::RotateSelection { pivot },
                ScenePoint { x: 10.0, y: 0.0 },
                ScenePoint { x: 10.0, y: 0.0 },
            )
            .unwrap(),
            initial
        );
        assert_eq!(
            preview_transforms(
                &initial,
                VisualTransformMode::ResizeSelection { pivot },
                ScenePoint { x: 10.0, y: 0.0 },
                ScenePoint { x: 10.0, y: 0.0 },
            )
            .unwrap(),
            initial
        );
    }

    #[test]
    fn visual_canvas_geometry_bounds_group_handle_box_to_selected_items() {
        let mut document = VisualDiagramDocument::new();
        let first = document
            .apply_transaction(&[VisualOperation::AddObject(object(
                VisualTransform {
                    x: -40.0,
                    width: 20.0,
                    height: 10.0,
                    ..VisualTransform::default()
                },
                VisualPrimitiveShape::Rectangle,
            ))])
            .unwrap()
            .created_objects[0];
        let second = document
            .apply_transaction(&[VisualOperation::AddObject(object(
                VisualTransform {
                    x: 40.0,
                    width: 20.0,
                    height: 10.0,
                    ..VisualTransform::default()
                },
                VisualPrimitiveShape::Rectangle,
            ))])
            .unwrap()
            .created_objects[0];
        let projection = project_visual_scene(document.scene()).unwrap();
        let selection = [first, second].into_iter().collect::<BTreeSet<_>>();
        let bounds = selection_bounds(&projection, &selection, projection.items.len()).unwrap();
        assert!(bounds.min.x < -49.0 && bounds.max.x > 49.0);
        let handles = selection_handles(
            document.scene(),
            &projection,
            &selection,
            projection.items.len(),
            map(),
        );
        assert_eq!(handles.len(), 2);
        assert_eq!(handles[0].kind, CanvasHandleKind::RotateSelection);
        assert_eq!(handles[1].kind, CanvasHandleKind::ResizeSelection);
    }

    #[test]
    fn visual_canvas_layout_snaps_group_to_grid_and_nearby_guide_with_zoom_scaled_ties() {
        let mut document = VisualDiagramDocument::new();
        let selected_id = document
            .apply_transaction(&[VisualOperation::AddObject(object(
                VisualTransform {
                    x: 3.0,
                    width: 20.0,
                    height: 20.0,
                    ..VisualTransform::default()
                },
                VisualPrimitiveShape::Rectangle,
            ))])
            .unwrap()
            .created_objects[0];
        let other_id = document
            .apply_transaction(&[VisualOperation::AddObject(object(
                VisualTransform {
                    x: 50.0,
                    y: 100.0,
                    width: 10.0,
                    height: 10.0,
                    ..VisualTransform::default()
                },
                VisualPrimitiveShape::Ellipse,
            ))])
            .unwrap()
            .created_objects[0];
        let second_selected = document
            .apply_transaction(&[VisualOperation::AddObject(object(
                VisualTransform {
                    x: -30.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                    ..VisualTransform::default()
                },
                VisualPrimitiveShape::Rectangle,
            ))])
            .unwrap()
            .created_objects[0];
        let tied_candidate = document
            .apply_transaction(&[VisualOperation::AddObject(object(
                VisualTransform {
                    x: 50.0,
                    y: -100.0,
                    width: 10.0,
                    height: 10.0,
                    ..VisualTransform::default()
                },
                VisualPrimitiveShape::Rectangle,
            ))])
            .unwrap()
            .created_objects[0];
        assert!(other_id < tied_candidate);
        let mut title = document
            .scene()
            .objects
            .iter()
            .find(|candidate| candidate.id == crate::diagram::TITLE_OBJECT_ID)
            .unwrap()
            .clone();
        title.transform.x = 800.0;
        title.transform.y = 800.0;
        document
            .apply_transaction(&[VisualOperation::ReplaceObject(title)])
            .unwrap();
        let projection = project_visual_scene(document.scene()).unwrap();
        let selected = [selected_id, second_selected]
            .into_iter()
            .collect::<BTreeSet<_>>();

        let grid = snap_translation_delta(
            &projection,
            &selected,
            projection.items.len(),
            ScenePoint { x: 14.0, y: 3.0 },
            2.0,
        )
        .unwrap();
        assert_eq!(grid.delta, ScenePoint { x: 15.0, y: 0.0 });
        assert!(grid.guides.is_empty());

        let guided = snap_translation_delta(
            &projection,
            &selected,
            projection.items.len(),
            ScenePoint { x: 34.0, y: 3.0 },
            1.0,
        )
        .unwrap();
        assert_eq!(guided.delta, ScenePoint { x: 32.0, y: 0.0 });
        assert_eq!(guided.guides.len(), 1);
        assert_eq!(guided.guides[0].axis, CanvasGuideAxis::Vertical);
        assert_eq!(guided.guides[0].target_object, other_id);
        assert_eq!(guided.guides[0].moving_anchor, CanvasAnchor::Max);
        assert_eq!(guided.guides[0].target_anchor, CanvasAnchor::Min);
        let initial_objects = document
            .scene()
            .objects
            .iter()
            .filter(|object| selected.contains(&object.id))
            .cloned()
            .collect::<Vec<_>>();
        let moved = preview_transforms(
            &initial_objects,
            VisualTransformMode::Translate,
            ScenePoint::default(),
            guided.delta,
        )
        .unwrap();
        assert_eq!(moved.len(), 2);
        assert_eq!(
            moved[0].transform.x - moved[1].transform.x,
            initial_objects[0].transform.x - initial_objects[1].transform.x,
        );

        let too_far_at_zoom = snap_translation_delta(
            &projection,
            &selected,
            projection.items.len(),
            ScenePoint { x: 34.0, y: 3.0 },
            10.0,
        )
        .unwrap();
        assert_eq!(too_far_at_zoom.delta, ScenePoint { x: 35.0, y: 0.0 });
        assert!(too_far_at_zoom.guides.is_empty());
        let before_unpainted_candidate = projection
            .items
            .iter()
            .position(|item| item.object_id == other_id)
            .unwrap();
        let bounded = snap_translation_delta(
            &projection,
            &selected,
            before_unpainted_candidate,
            ScenePoint { x: 34.0, y: 3.0 },
            1.0,
        )
        .unwrap();
        assert_eq!(bounded.delta, ScenePoint { x: 35.0, y: 0.0 });
        assert!(bounded.guides.is_empty());
        let returned_to_origin = snap_translation_delta(
            &projection,
            &selected,
            projection.items.len(),
            ScenePoint::default(),
            1.0,
        )
        .unwrap();
        assert_eq!(returned_to_origin.delta, ScenePoint::default());
        assert!(returned_to_origin.guides.is_empty());
    }

    #[test]
    fn visual_canvas_layout_uses_rotated_and_preset_owner_outlines_without_shadow() {
        let mut document = VisualDiagramDocument::new();
        let mut selected_draft = object(
            VisualTransform {
                width: 20.0,
                height: 10.0,
                rotation_degrees: 90.0,
                ..VisualTransform::default()
            },
            VisualPrimitiveShape::Rectangle,
        );
        selected_draft.style.shadow = Some(VisualShadow {
            offset_x: 200.0,
            offset_y: 0.0,
            blur: 0.0,
            color: crate::diagram::VisualColor::rgba(0, 0, 0, 80),
        });
        let selected_id = document
            .apply_transaction(&[VisualOperation::AddObject(selected_draft)])
            .unwrap()
            .created_objects[0];
        let preset = document
            .apply_transaction(&[VisualOperation::AddPreset(VisualPresetDraft {
                name: "Layout target".into(),
                components: vec![
                    VisualPrimitiveComponent {
                        shape: VisualPrimitiveShape::Rectangle,
                        transform: VisualTransform {
                            x: -20.0,
                            width: 20.0,
                            height: 20.0,
                            ..VisualTransform::default()
                        },
                        style: VisualStyle::default(),
                    },
                    VisualPrimitiveComponent {
                        shape: VisualPrimitiveShape::Diamond,
                        transform: VisualTransform {
                            x: 20.0,
                            width: 20.0,
                            height: 20.0,
                            ..VisualTransform::default()
                        },
                        style: VisualStyle::default(),
                    },
                ],
            })])
            .unwrap()
            .created_presets[0];
        let preset_owner = document
            .apply_transaction(&[VisualOperation::AddObject(VisualObjectDraft {
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform {
                    x: 100.0,
                    y: 100.0,
                    width: 100.0,
                    height: 100.0,
                    rotation_degrees: 0.0,
                },
                style: VisualStyle::default(),
                kind: VisualObjectKind::PresetInstance { preset_id: preset },
            })])
            .unwrap()
            .created_objects[0];
        let mut title = document
            .scene()
            .objects
            .iter()
            .find(|candidate| candidate.id == crate::diagram::TITLE_OBJECT_ID)
            .unwrap()
            .clone();
        title.transform.x = 900.0;
        title.transform.y = 900.0;
        document
            .apply_transaction(&[VisualOperation::ReplaceObject(title)])
            .unwrap();
        let projection = project_visual_scene(document.scene()).unwrap();
        let selected = [selected_id].into_iter().collect::<BTreeSet<_>>();
        let snapped = snap_translation_delta(
            &projection,
            &selected,
            projection.items.len(),
            ScenePoint { x: 57.0, y: 0.0 },
            1.0,
        )
        .unwrap();
        assert_eq!(snapped.delta.x, 65.0);
        assert_eq!(snapped.guides.len(), 1);
        assert_eq!(snapped.guides[0].target_object, preset_owner);
        assert_eq!(snapped.guides[0].target_anchor, CanvasAnchor::Min);
    }

    #[test]
    fn visual_canvas_layout_aligns_rotated_extents_and_distributes_with_fixed_endpoints() {
        let mut document = VisualDiagramDocument::new();
        let ids = [
            VisualTransform {
                x: 0.0,
                y: -10.0,
                width: 20.0,
                height: 10.0,
                rotation_degrees: 15.0,
            },
            VisualTransform {
                x: 40.0,
                y: 0.0,
                width: 10.0,
                height: 20.0,
                rotation_degrees: 0.0,
            },
            VisualTransform {
                x: 100.0,
                y: 10.0,
                width: 30.0,
                height: 8.0,
                rotation_degrees: 0.0,
            },
        ]
        .into_iter()
        .map(|transform| {
            document
                .apply_transaction(&[VisualOperation::AddObject(object(
                    transform,
                    VisualPrimitiveShape::Rectangle,
                ))])
                .unwrap()
                .created_objects[0]
        })
        .collect::<Vec<_>>();
        let mut title = document
            .scene()
            .objects
            .iter()
            .find(|candidate| candidate.id == crate::diagram::TITLE_OBJECT_ID)
            .unwrap()
            .clone();
        title.transform.x = 900.0;
        title.transform.y = 900.0;
        document
            .apply_transaction(&[VisualOperation::ReplaceObject(title)])
            .unwrap();
        let projection = project_visual_scene(document.scene()).unwrap();
        let selection = ids.iter().copied().collect::<BTreeSet<_>>();
        let aligned = align_selection(
            document.scene(),
            &projection,
            &selection,
            projection.items.len(),
            VisualAlign::Top,
        )
        .unwrap();
        assert_eq!(aligned.len(), 2);
        let before_by_id = ids
            .iter()
            .map(|id| {
                (
                    *id,
                    document
                        .scene()
                        .objects
                        .iter()
                        .find(|object| object.id == *id)
                        .unwrap()
                        .clone(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut aligned_doc = document.clone();
        aligned_doc.apply_transaction(&aligned).unwrap();
        let aligned_projection = project_visual_scene(aligned_doc.scene()).unwrap();
        let aligned_bounds =
            visible_outline_bounds(&aligned_projection, aligned_projection.items.len());
        let tops = ids
            .iter()
            .map(|id| aligned_bounds[id].min.y)
            .collect::<Vec<_>>();
        assert!(tops.iter().all(|top| (top - tops[0]).abs() < 1e-8));

        let distributed = distribute_selection(
            document.scene(),
            &projection,
            &selection,
            projection.items.len(),
            VisualDistribution::Horizontal,
        )
        .unwrap();
        assert_eq!(distributed.len(), 1);
        let mut distributed_doc = document.clone();
        distributed_doc.apply_transaction(&distributed).unwrap();
        let distributed_projection = project_visual_scene(distributed_doc.scene()).unwrap();
        let distributed_bounds =
            visible_outline_bounds(&distributed_projection, distributed_projection.items.len());
        let left = distributed_bounds[&ids[0]];
        let middle = distributed_bounds[&ids[1]];
        let right = distributed_bounds[&ids[2]];
        let first_gap = middle.min.x - left.max.x;
        let second_gap = right.min.x - middle.max.x;
        assert!((first_gap - second_gap).abs() < 1e-8);
        for endpoint in [ids[0], ids[2]] {
            assert_eq!(
                distributed_doc
                    .scene()
                    .objects
                    .iter()
                    .find(|object| object.id == endpoint)
                    .unwrap(),
                &before_by_id[&endpoint],
            );
        }
    }

    #[test]
    fn visual_canvas_layout_minimap_handles_empty_and_extreme_projection_bounds() {
        let empty = VisualSceneProjection {
            items: Vec::new(),
            bounds: None,
            simplified_dash_paths: 0,
            truncated_text_items: 0,
            text_lines: 0,
        };
        let visible = SceneRect {
            min: ScenePoint { x: -50.0, y: -30.0 },
            max: ScenePoint { x: 50.0, y: 30.0 },
        };
        let frame = SceneRect {
            min: ScenePoint { x: 10.0, y: 20.0 },
            max: ScenePoint { x: 150.0, y: 100.0 },
        };
        let layout = minimap_layout(&empty, visible, frame).unwrap();
        assert!(layout.outlines.is_empty());
        assert!(frame.contains_rect(layout.viewport));
        assert_eq!(
            layout.screen_to_world(layout.frame.center()).unwrap(),
            visible.center()
        );
        assert!(
            layout
                .screen_to_world(ScenePoint { x: 200.0, y: 200.0 })
                .is_none()
        );
        assert!(minimap_layout(&empty, visible, SceneRect::default()).is_none());

        let mut document = VisualDiagramDocument::new();
        let _ = added_object(
            &mut document,
            VisualTransform {
                x: MAX_VISUAL_COORDINATE_ABS,
                y: -MAX_VISUAL_COORDINATE_ABS,
                width: 20.0,
                height: 10.0,
                rotation_degrees: 0.0,
            },
        );
        let projection = project_visual_scene(document.scene()).unwrap();
        let bounded = minimap_layout(&projection, visible, frame).unwrap();
        assert!(bounded.outlines.len() <= 128);
        assert!(
            bounded
                .outlines
                .iter()
                .flatten()
                .all(|point| { point.x.is_finite() && point.y.is_finite() })
        );
        let repeated_item = projection.items[0].clone();
        let mut large_projection = projection;
        large_projection.items = vec![repeated_item; 200];
        let bounded = minimap_layout(&large_projection, visible, frame).unwrap();
        assert_eq!(bounded.outlines.len(), 128);
    }
}
