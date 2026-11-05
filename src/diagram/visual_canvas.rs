//! Pure selection, mapping, and transform geometry for the visual canvas.

use super::scene::{ScenePoint, SceneRect};
use super::visual::{
    MAX_VISUAL_COORDINATE_ABS, MAX_VISUAL_DIMENSION, MAX_VISUAL_TRANSACTION_OPERATIONS,
    MIN_VISUAL_DIMENSION, VisualObject, VisualObjectId, VisualScene, VisualTransform,
};
use super::visual_scene::VisualSceneProjection;
use std::collections::BTreeSet;

const HANDLE_WORLD_GAP_SCREEN_POINTS: f64 = 22.0;

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
        CanvasHandle, CanvasHandleKind, CanvasHit, CanvasMapping, VisualTransformMode, hit_test,
        marquee_selection, preview_transforms, selection_bounds, selection_handles,
    };
    use crate::diagram::{
        DEFAULT_LAYER_ID, ScenePoint, SceneRect, VisualDiagramDocument, VisualObject,
        VisualObjectDraft, VisualObjectKind, VisualOperation, VisualPresetDraft,
        VisualPrimitiveComponent, VisualPrimitiveShape, VisualStyle, VisualTransform,
        project_visual_scene,
    };
    use std::collections::BTreeSet;

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
}
