use super::assets::{PreparedAssetSnapshot, PreparedImage, PreparedMedia, reference_identity};
use super::font_cache::{FontDiagnostic, PreparedTextLayout, ScriptClass};
use super::geometry::{
    HitShape, LayoutSnapshot, LogicalPoint, LogicalRect, PhysicalRect, compose_layered_layout,
};
use super::model::{CellId, MediaReference, Override};
use super::session::FrameId;
use super::tooltip::{PreparedTooltip, place_tooltip};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

#[derive(Clone, Debug, PartialEq)]
pub enum VectorPrimitive {
    FilledCircle {
        center: LogicalPoint,
        radius: f32,
        color: Rgba,
    },
    FilledWedge {
        center: LogicalPoint,
        inner_radius: f32,
        outer_radius: f32,
        start_angle: f32,
        end_angle: f32,
        color: Rgba,
    },
    Text {
        origin: LogicalPoint,
        text: String,
        prepared: Arc<PreparedTextLayout>,
        size: f32,
        font_family: String,
        bold: bool,
        italic: bool,
        underline: bool,
        strikeout: bool,
        quality: super::model::RenderingQuality,
        shadow: Option<(Rgba, super::model::Offset2D)>,
        color: Rgba,
    },
    Image {
        bounds: LogicalRect,
        image: Arc<PreparedImage>,
        opacity: u8,
        quality: super::model::RenderingQuality,
    },
    Tooltip {
        bounds: LogicalRect,
        text: Arc<PreparedTooltip>,
        background: Rgba,
        color: Rgba,
    },
    /// Everything before this marker is invariant under hover/selection.
    StaticBoundary,
}

/// Immutable original-vector scene. Native backends may rasterize it, but the
/// retained source never becomes a scaled copy of a previous bitmap.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorScene {
    pub bounds: LogicalRect,
    pub generation: u64,
    pub shape_quality: super::model::RenderingQuality,
    pub primitives: Vec<VectorPrimitive>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreparedSceneResources {
    /// Keys are stable `reference_identity` values, never mutable indices.
    pub media: BTreeMap<String, PreparedAssetSnapshot>,
    pub text: BTreeMap<CellId, Arc<PreparedTextLayout>>,
    pub tooltips: BTreeMap<CellId, Arc<PreparedTooltip>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputOwner {
    Actionable(CellId),
    /// An exposed ancestor cell navigates to the exact retained frame.  It is
    /// never an action owner, even when the authored ancestor cell is
    /// actionable in its own frame.
    NavigateToFrame(FrameId),
    Protective,
    Exterior,
}

/// One independently renderable retained frame in a shared Cascade scene.
/// The layout and resources are intentionally kept per-frame so an ancestor's
/// skin, media and text cannot be replaced by the active child's style.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneLayer {
    pub frame_id: FrameId,
    pub layout: LayoutSnapshot,
    pub resources: PreparedSceneResources,
    pub selected: Option<CellId>,
    pub visible_tooltip: Option<CellId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayeredScene {
    pub layout: LayoutSnapshot,
    pub scene: VectorScene,
    pub frame_ids: Vec<FrameId>,
}

/// Compose oldest-ancestor-first layers into one visual scene and one
/// composite input snapshot.  This is the shared boundary used by runtime,
/// native authoring preview and embedded preview; callers only choose which
/// retained frames belong to the current Cascade path.
pub fn compose_layered_scene(
    layers: &[SceneLayer],
    generation: u64,
    work_area: PhysicalRect,
) -> Option<LayeredScene> {
    let layout_inputs: Vec<_> = layers
        .iter()
        .map(|layer| (layer.frame_id, &layer.layout))
        .collect();
    let layout = compose_layered_layout(&layout_inputs)?;
    let mut bounds = layout.visual_extent;
    let mut primitives = Vec::new();
    for (index, layer) in layers.iter().enumerate() {
        let child_scene = build_scene_prepared_selected_tooltip(
            &layer.layout,
            generation,
            &layer.resources,
            layer.selected.as_ref(),
            (index == layers.len().saturating_sub(1))
                .then_some(layer.visible_tooltip.as_ref())
                .flatten(),
            work_area,
        );
        bounds = union_rect(bounds, child_scene.bounds);
        // Only the oldest scene contributes the static boundary.  Later
        // boundaries would incorrectly split the retained compositor cache;
        // their complete primitives still remain in oldest-to-newest order.
        if index == 0 {
            primitives.extend(child_scene.primitives);
        } else {
            primitives.extend(
                child_scene
                    .primitives
                    .into_iter()
                    .filter(|primitive| !matches!(primitive, VectorPrimitive::StaticBoundary)),
            );
        }
    }
    Some(LayeredScene {
        layout,
        scene: VectorScene {
            bounds,
            generation,
            shape_quality: layers.last()?.layout.style.shape_quality,
            primitives,
        },
        frame_ids: layers.iter().map(|layer| layer.frame_id).collect(),
    })
}

pub fn build_scene(layout: &LayoutSnapshot, generation: u64) -> VectorScene {
    build_scene_internal(
        layout,
        generation,
        &PreparedSceneResources::default(),
        None,
        None,
        None,
    )
}

pub fn build_scene_prepared(
    layout: &LayoutSnapshot,
    generation: u64,
    resources: &PreparedSceneResources,
) -> VectorScene {
    build_scene_internal(layout, generation, resources, None, None, None)
}

pub fn build_scene_selected(
    layout: &LayoutSnapshot,
    generation: u64,
    selected: Option<&CellId>,
) -> VectorScene {
    build_scene_internal(
        layout,
        generation,
        &PreparedSceneResources::default(),
        selected,
        None,
        None,
    )
}

pub fn build_scene_prepared_selected(
    layout: &LayoutSnapshot,
    generation: u64,
    resources: &PreparedSceneResources,
    selected: Option<&CellId>,
) -> VectorScene {
    build_scene_internal(layout, generation, resources, selected, None, None)
}

pub fn build_scene_prepared_selected_tooltip(
    layout: &LayoutSnapshot,
    generation: u64,
    resources: &PreparedSceneResources,
    selected: Option<&CellId>,
    visible_tooltip: Option<&CellId>,
    work_area: PhysicalRect,
) -> VectorScene {
    build_scene_internal(
        layout,
        generation,
        resources,
        selected,
        visible_tooltip,
        Some(work_area),
    )
}

fn build_scene_internal(
    layout: &LayoutSnapshot,
    generation: u64,
    resources: &PreparedSceneResources,
    selected: Option<&CellId>,
    visible_tooltip: Option<&CellId>,
    work_area: Option<PhysicalRect>,
) -> VectorScene {
    let outer_radius = rect_width(layout.rim_extent).max(rect_height(layout.rim_extent)) * 0.5;
    let background_radius =
        rect_width(layout.background_extent).max(rect_height(layout.background_extent)) * 0.5;
    let mut primitives = Vec::new();
    if layout.style.menu_shadow_width > 0.0 {
        primitives.push(VectorPrimitive::FilledCircle {
            center: layout.center,
            radius: outer_radius + layout.style.menu_shadow_width,
            color: rgba_from_style(layout.style.menu_shadow_outer_color),
        });
        primitives.push(VectorPrimitive::FilledCircle {
            center: layout.center,
            radius: outer_radius + layout.style.menu_shadow_width * 0.5,
            color: rgba_from_style(layout.style.menu_shadow_inner_color),
        });
    }
    primitives.extend([
        VectorPrimitive::FilledCircle {
            center: layout.center,
            radius: outer_radius,
            color: Rgba(18, 20, 24, 220),
        },
        VectorPrimitive::FilledCircle {
            center: layout.center,
            radius: background_radius,
            color: Rgba(24, 27, 32, 232),
        },
    ]);
    push_image(
        &mut primitives,
        resources,
        &layout.style.menu_outer_rim,
        layout.rim_extent,
        layout.style.image_quality,
        opacity(layout.style.menu_outer_rim_opacity),
    );
    push_image(
        &mut primitives,
        resources,
        &layout.style.menu_background,
        scaled_rect(layout.background_extent, layout.style.menu_background_scale),
        layout.style.image_quality,
        opacity(layout.style.menu_background_opacity),
    );
    primitives.push(VectorPrimitive::FilledCircle {
        center: layout.center,
        radius: layout.center_radius,
        color: Rgba(30, 33, 39, 238),
    });
    let center_bounds = circle_bounds(layout.center, layout.center_radius);
    if layout.style.item_background_on_center {
        push_image(
            &mut primitives,
            resources,
            &layout.style.center_background,
            scaled_rect(center_bounds, layout.style.center_background_scale),
            layout.style.image_quality,
            opacity(layout.style.center_background_opacity),
        );
    }
    push_image(
        &mut primitives,
        resources,
        &layout.style.center_image,
        scaled_rect(center_bounds, layout.style.center_image_scale),
        layout.style.image_quality,
        opacity(layout.style.center_image_opacity),
    );
    primitives.push(VectorPrimitive::StaticBoundary);
    // Background actions own otherwise-protective gaps but render below the
    // concrete cells they must not obscure.
    for cell in layout
        .cells
        .iter()
        .filter(|cell| cell.cell_id.as_str() == "__background")
        .chain(
            layout
                .cells
                .iter()
                .filter(|cell| cell.cell_id.as_str() != "__background"),
        )
    {
        let color = if selected == Some(&cell.cell_id) {
            Rgba(92, 112, 148, 252)
        } else if cell.actionable && cell.visual.glow_enabled {
            Rgba(68, 82, 104, 250)
        } else if cell.actionable {
            Rgba(54, 61, 72, 244)
        } else {
            Rgba(42, 46, 54, 220)
        };
        let bounds = shape_bounds(&cell.shape);
        if cell.visual.glow_enabled {
            if matches!(layout.style.item_glow, Override::Value(_)) {
                push_image(
                    &mut primitives,
                    resources,
                    &layout.style.item_glow,
                    scaled_rect(bounds, 1.10),
                    cell.visual.image_quality,
                    opacity(layout.style.item_glow_opacity),
                );
            } else {
                push_shape(
                    &mut primitives,
                    &cell.shape,
                    Rgba(
                        126,
                        154,
                        210,
                        opacity(layout.style.item_glow_opacity * 0.24),
                    ),
                    1.10,
                );
            }
        }
        push_image(
            &mut primitives,
            resources,
            &cell.visual.item_shadow,
            scaled_rect(bounds, cell.visual.item_shadow_scale),
            cell.visual.image_quality,
            opacity(cell.visual.item_shadow_opacity),
        );
        match cell.shape {
            HitShape::Circle { center, radius } => primitives.push(VectorPrimitive::FilledCircle {
                center,
                radius,
                color,
            }),
            HitShape::Wedge {
                center,
                inner_radius,
                outer_radius,
                start_angle,
                end_angle,
            } => primitives.push(VectorPrimitive::FilledWedge {
                center,
                inner_radius,
                outer_radius,
                start_angle,
                end_angle,
                color,
            }),
        }
        if cell.visual.item_background_visible {
            push_image(
                &mut primitives,
                resources,
                &cell.visual.item_background,
                scaled_rect(bounds, cell.visual.item_background_scale),
                cell.visual.image_quality,
                opacity(cell.visual.item_background_opacity),
            );
        }
        push_image(
            &mut primitives,
            resources,
            &cell.visual.item_foreground,
            scaled_rect(bounds, cell.visual.item_foreground_scale),
            cell.visual.image_quality,
            opacity(cell.visual.item_foreground_opacity),
        );
        push_image(
            &mut primitives,
            resources,
            &cell.icon,
            shifted_y(
                scaled_rect(bounds, cell.visual.item_image_scale),
                bounds,
                cell.visual.item_image_y_ratio,
            ),
            cell.visual.image_quality,
            opacity(cell.visual.icon_opacity),
        );
        push_image(
            &mut primitives,
            resources,
            &cell.visual.submenu_indicator,
            shifted_y(
                scaled_rect(
                    bounds,
                    (cell.visual.submenu_indicator_size / rect_width(bounds)).min(1.0),
                ),
                bounds,
                cell.visual.submenu_indicator_y_ratio,
            ),
            cell.visual.image_quality,
            opacity(cell.visual.submenu_indicator_opacity),
        );
        let origin = match cell.shape {
            HitShape::Circle { center, .. } | HitShape::Wedge { center, .. } => center,
        };
        if cell.visual.text_visible {
            let vertical_offset = (cell.visual.text_y_ratio - 0.5)
                * cell.visual.font_size
                * cell.visual.text_box_scale;
            primitives.push(VectorPrimitive::Text {
                origin: LogicalPoint {
                    x: origin.x,
                    y: origin.y + vertical_offset,
                },
                text: cell.label.clone(),
                prepared: resources
                    .text
                    .get(&cell.cell_id)
                    .cloned()
                    .unwrap_or_else(|| fallback_text(&cell.label, &cell.visual.font_family)),
                size: cell.visual.font_size,
                font_family: cell.visual.font_family.clone(),
                bold: cell.visual.bold,
                italic: cell.visual.italic,
                underline: cell.visual.underline,
                strikeout: cell.visual.strikeout,
                quality: cell.visual.text_quality,
                shadow: cell.visual.shadow_enabled.then_some((
                    rgba_from_style(cell.visual.shadow_color),
                    cell.visual.shadow_offset,
                )),
                color: rgba_from_style(cell.visual.text_color),
            });
        }
    }
    let mut visual_bounds = layout.visual_extent;
    push_image(
        &mut primitives,
        resources,
        &layout.style.menu_foreground,
        scaled_rect(layout.background_extent, layout.style.menu_foreground_scale),
        layout.style.image_quality,
        opacity(layout.style.menu_foreground_opacity),
    );
    if let (Some(cell_id), Some(work_area), Some(tooltip)) = (
        visible_tooltip,
        work_area,
        visible_tooltip.and_then(|cell_id| resources.tooltips.get(cell_id)),
    ) && let Some(cell) = layout.cells.iter().find(|cell| &cell.cell_id == cell_id)
    {
        let anchor = shape_bounds(&cell.shape);
        let tooltip_bounds = place_tooltip(
            anchor,
            tooltip.logical_size(),
            work_area,
            layout.scale_factor,
        );
        visual_bounds = union_rect(visual_bounds, tooltip_bounds);
        primitives.push(VectorPrimitive::Tooltip {
            bounds: tooltip_bounds,
            text: Arc::clone(tooltip),
            background: Rgba(10, 11, 14, 242),
            color: Rgba(248, 248, 248, 255),
        });
    }
    VectorScene {
        bounds: visual_bounds,
        generation,
        shape_quality: layout.style.shape_quality,
        primitives,
    }
}

fn fallback_text(text: &str, family: &str) -> Arc<PreparedTextLayout> {
    let source_text: Arc<str> = Arc::from(text);
    Arc::new(PreparedTextLayout {
        text: Arc::clone(&source_text),
        source_text,
        selected_family: Arc::from(if family.is_empty() {
            "Segoe UI"
        } else {
            family
        }),
        script: ScriptClass::Mixed,
        estimated_width_milli: 0,
        measured_width_milli: 0,
        measured_height_milli: 0,
        line_count: 1,
        glyphs: Vec::new(),
        diagnostics: Vec::<FontDiagnostic>::new(),
    })
}

fn union_rect(left: LogicalRect, right: LogicalRect) -> LogicalRect {
    LogicalRect {
        min: LogicalPoint {
            x: left.min.x.min(right.min.x),
            y: left.min.y.min(right.min.y),
        },
        max: LogicalPoint {
            x: left.max.x.max(right.max.x),
            y: left.max.y.max(right.max.y),
        },
    }
}

fn push_image(
    output: &mut Vec<VectorPrimitive>,
    resources: &PreparedSceneResources,
    reference: &Override<MediaReference>,
    bounds: LogicalRect,
    quality: super::model::RenderingQuality,
    opacity: u8,
) {
    let Override::Value(reference) = reference else {
        return;
    };
    let Some(snapshot) = resources.media.get(&reference_identity(reference)) else {
        return;
    };
    let PreparedMedia::Image(image) = &*snapshot.media else {
        return;
    };
    output.push(VectorPrimitive::Image {
        bounds,
        image: Arc::new(image.clone()),
        opacity,
        quality,
    });
}

fn push_shape(output: &mut Vec<VectorPrimitive>, shape: &HitShape, color: Rgba, scale: f32) {
    match *shape {
        HitShape::Circle { center, radius } => output.push(VectorPrimitive::FilledCircle {
            center,
            radius: radius * scale,
            color,
        }),
        HitShape::Wedge {
            center,
            inner_radius,
            outer_radius,
            start_angle,
            end_angle,
        } => output.push(VectorPrimitive::FilledWedge {
            center,
            inner_radius: inner_radius / scale,
            outer_radius: outer_radius * scale,
            start_angle,
            end_angle,
            color,
        }),
    }
}

fn shape_bounds(shape: &HitShape) -> LogicalRect {
    match *shape {
        HitShape::Circle { center, radius } => circle_bounds(center, radius),
        HitShape::Wedge {
            center,
            outer_radius,
            ..
        } => circle_bounds(center, outer_radius),
    }
}

fn circle_bounds(center: LogicalPoint, radius: f32) -> LogicalRect {
    LogicalRect {
        min: LogicalPoint {
            x: center.x - radius,
            y: center.y - radius,
        },
        max: LogicalPoint {
            x: center.x + radius,
            y: center.y + radius,
        },
    }
}

fn scaled_rect(rect: LogicalRect, scale: f32) -> LogicalRect {
    let center = LogicalPoint {
        x: (rect.min.x + rect.max.x) * 0.5,
        y: (rect.min.y + rect.max.y) * 0.5,
    };
    let half_width = rect_width(rect) * scale.max(0.0) * 0.5;
    let half_height = rect_height(rect) * scale.max(0.0) * 0.5;
    LogicalRect {
        min: LogicalPoint {
            x: center.x - half_width,
            y: center.y - half_height,
        },
        max: LogicalPoint {
            x: center.x + half_width,
            y: center.y + half_height,
        },
    }
}

fn shifted_y(mut rect: LogicalRect, owner: LogicalRect, ratio: f32) -> LogicalRect {
    let desired = owner.min.y + rect_height(owner) * ratio.clamp(0.0, 1.0);
    let current = (rect.min.y + rect.max.y) * 0.5;
    rect.min.y += desired - current;
    rect.max.y += desired - current;
    rect
}

fn opacity(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn rect_width(rect: LogicalRect) -> f32 {
    rect.max.x - rect.min.x
}
fn rect_height(rect: LogicalRect) -> f32 {
    rect.max.y - rect.min.y
}

fn rgba_from_style(value: super::model::ColorRgba) -> Rgba {
    Rgba(value.red, value.green, value.blue, value.alpha)
}

/// Input ownership is geometric and deliberately independent from rendered alpha.
pub fn input_owner(layout: &LayoutSnapshot, point: LogicalPoint, ancestor: bool) -> InputOwner {
    if let Some(layered) = &layout.layered_input {
        return layered_input_owner(layered, point);
    }
    if !inside_rect(layout.input_extent, point) || !inside_owned_background(layout, point) {
        return InputOwner::Exterior;
    }
    if ancestor {
        return InputOwner::Protective;
    }
    if let Some(cell) = layout
        .cells
        .iter()
        .rev()
        .find(|cell| shape_contains(&cell.shape, point))
    {
        return if cell.actionable {
            InputOwner::Actionable(cell.cell_id.clone())
        } else {
            InputOwner::Protective
        };
    }
    let distance =
        ((point.x - layout.center.x).powi(2) + (point.y - layout.center.y).powi(2)).sqrt();
    if distance <= layout.center_radius {
        if layout.style.fill_center_hit_zone {
            InputOwner::Protective
        } else {
            InputOwner::Exterior
        }
    } else if layout.style.fill_item_hit_zones {
        InputOwner::Protective
    } else {
        InputOwner::Exterior
    }
}

fn layered_input_owner(layered: &super::geometry::LayeredInput, point: LogicalPoint) -> InputOwner {
    let active_index = layered.layers.len().saturating_sub(1);
    for (index, layer) in layered.layers.iter().enumerate().rev() {
        if !inside_rect(layer.input_extent, point)
            || !layer
                .input_regions
                .iter()
                .any(|shape| shape_contains(shape, point))
        {
            continue;
        }
        if index != active_index {
            if layer
                .cells
                .iter()
                .rev()
                .any(|cell| shape_contains(&cell.shape, point))
            {
                return InputOwner::NavigateToFrame(layer.frame_id);
            }
            // Every newer layer's owned footprint, including transparent
            // center/item gaps, protects against ancestor click-through.
            return InputOwner::Protective;
        }
        if let Some(cell) = layer
            .cells
            .iter()
            .rev()
            .find(|cell| shape_contains(&cell.shape, point))
        {
            return if cell.actionable {
                InputOwner::Actionable(cell.cell_id.clone())
            } else {
                InputOwner::Protective
            };
        }
        // A Cascade child owns its entire wheel footprint for hit routing;
        // this keeps its gaps protective even when authored fill settings are
        // disabled and prevents an ancestor from receiving the same gesture.
        return InputOwner::Protective;
    }
    InputOwner::Exterior
}

fn inside_owned_background(layout: &LayoutSnapshot, p: LogicalPoint) -> bool {
    // Cascades can contain disjoint wheels in one host. Their union is owned,
    // while transparent space between them remains true exterior.
    layout
        .input_regions
        .iter()
        .any(|shape| shape_contains(shape, p))
}

fn inside_rect(rect: LogicalRect, p: LogicalPoint) -> bool {
    p.x >= rect.min.x && p.x <= rect.max.x && p.y >= rect.min.y && p.y <= rect.max.y
}

fn shape_contains(shape: &HitShape, p: LogicalPoint) -> bool {
    match *shape {
        HitShape::Circle { center, radius } => {
            let dx = p.x - center.x;
            let dy = p.y - center.y;
            dx * dx + dy * dy <= radius * radius
        }
        HitShape::Wedge {
            center,
            inner_radius,
            outer_radius,
            start_angle,
            end_angle,
        } => {
            let dx = p.x - center.x;
            let dy = p.y - center.y;
            let radius = (dx * dx + dy * dy).sqrt();
            if radius < inner_radius || radius > outer_radius {
                return false;
            }
            let angle = dy.atan2(dx).rem_euclid(std::f32::consts::TAU);
            let start = start_angle.rem_euclid(std::f32::consts::TAU);
            let span = (end_angle - start_angle).rem_euclid(std::f32::consts::TAU);
            span == 0.0 || (angle - start).rem_euclid(std::f32::consts::TAU) < span
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::geometry::{
        PhysicalPoint, PhysicalRect, ScaleFactor, layout_menu, translate_layout,
    };
    use crate::radial::model::RadialDocument;
    use crate::radial::session::FrameId;

    fn layout() -> LayoutSnapshot {
        layout_menu(
            &RadialDocument::starter().menus[0],
            PhysicalPoint { x: 300.0, y: 300.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 600.0, y: 600.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap()
    }

    #[test]
    fn layered_scene_preserves_frame_identity_and_frontmost_hit_priority() {
        let parent = layout();
        let mut child = layout();
        translate_layout(&mut child, PhysicalPoint { x: 160.0, y: 18.0 }).unwrap();
        let parent_resources = PreparedSceneResources::default();
        let child_resources = PreparedSceneResources::default();
        let layers = vec![
            SceneLayer {
                frame_id: FrameId(4),
                layout: parent.clone(),
                resources: parent_resources,
                selected: None,
                visible_tooltip: None,
            },
            SceneLayer {
                frame_id: FrameId(9),
                layout: child.clone(),
                resources: child_resources,
                selected: None,
                visible_tooltip: None,
            },
        ];
        let layered = compose_layered_scene(
            &layers,
            17,
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 600.0, y: 600.0 },
            },
        )
        .unwrap();
        let parent_point = match parent.cells[0].shape {
            HitShape::Circle { center, .. } | HitShape::Wedge { center, .. } => center,
        };
        assert_eq!(
            input_owner(&layered.layout, parent_point, false),
            InputOwner::NavigateToFrame(FrameId(4))
        );
        let child_point = match child.cells[0].shape {
            HitShape::Circle { center, .. } | HitShape::Wedge { center, .. } => center,
        };
        assert!(matches!(
            input_owner(&layered.layout, child_point, false),
            InputOwner::Actionable(_)
        ));
        assert_eq!(layered.frame_ids, vec![FrameId(4), FrameId(9)]);
        assert!(layered.scene.primitives.len() > parent.cells.len());
    }
    #[test]
    fn transparent_pixels_inside_a_circle_still_belong_to_the_cell() {
        let l = layout();
        let c = &l.cells[0];
        let HitShape::Circle { center, radius } = c.shape else {
            panic!()
        };
        assert_eq!(
            input_owner(
                &l,
                LogicalPoint {
                    x: center.x + radius * 0.9,
                    y: center.y
                },
                false
            ),
            InputOwner::Actionable(c.cell_id.clone())
        );
    }
    #[test]
    fn internal_gap_is_protective_and_true_exterior_is_not_owned() {
        let l = layout();
        assert_eq!(input_owner(&l, l.center, false), InputOwner::Protective);
        assert_eq!(
            input_owner(
                &l,
                LogicalPoint {
                    x: l.input_extent.max.x + 1.0,
                    y: l.center.y
                },
                false
            ),
            InputOwner::Exterior
        );
    }

    #[test]
    fn disabled_fill_zones_relinquish_gaps_while_cells_remain_actionable() {
        let mut document = RadialDocument::starter();
        document.menus[0].style.values.window.fill_center_hit_zone = Override::Value(false);
        document.menus[0].style.values.window.fill_item_hit_zones = Override::Value(false);
        let layout = crate::radial::geometry::layout_document_menu(
            &document,
            &document.menus[0],
            PhysicalPoint { x: 300.0, y: 300.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 600.0, y: 600.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        assert_eq!(
            input_owner(&layout, layout.center, false),
            InputOwner::Exterior
        );
        let HitShape::Circle { center, .. } = layout.cells[0].shape else {
            panic!()
        };
        assert!(matches!(
            input_owner(&layout, center, false),
            InputOwner::Actionable(_)
        ));
    }
    #[test]
    fn ancestor_is_inert_but_protective() {
        let l = layout();
        let HitShape::Circle { center, .. } = l.cells[0].shape else {
            panic!()
        };
        assert_eq!(input_owner(&l, center, true), InputOwner::Protective);
    }

    #[test]
    fn overlapping_cells_share_the_layouts_topmost_winner() {
        let mut layout = layout();
        let point = match layout.cells[0].shape {
            HitShape::Circle { center, .. } | HitShape::Wedge { center, .. } => center,
        };
        let mut top = layout.cells[0].clone();
        top.cell_id = crate::radial::model::CellId::new("topmost-overlap");
        layout.cells.push(top.clone());
        assert_eq!(
            layout.hit_test(point).map(|cell| &cell.cell_id),
            Some(&top.cell_id)
        );
        assert_eq!(
            input_owner(&layout, point, false),
            InputOwner::Actionable(top.cell_id)
        );
    }

    #[test]
    fn compiled_text_and_glow_options_change_the_scene_contract() {
        let mut document = RadialDocument::starter();
        document.menus[0].style.values.effects.glow_enabled =
            crate::radial::model::Override::Value(true);
        document.menus[0].rings[0].cells[0].style.text.visible =
            crate::radial::model::Override::Value(false);
        document.menus[0].rings[0].cells[1].style.text.font_size =
            crate::radial::model::Override::Value(22.0);
        let layout = crate::radial::geometry::layout_document_menu(
            &document,
            &document.menus[0],
            PhysicalPoint { x: 300.0, y: 300.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 600.0, y: 600.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        let scene = build_scene(&layout, 9);
        assert!(layout.cells.iter().all(|cell| cell.visual.glow_enabled));
        assert!(!scene.primitives.iter().any(|primitive| {
            matches!(primitive, VectorPrimitive::Text { text, .. } if text == "Favorites")
        }));
        assert!(scene.primitives.iter().any(|primitive| {
            matches!(primitive, VectorPrimitive::Text { text, size, .. } if text == "Apps" && *size == 22.0)
        }));
    }

    #[test]
    fn tooltip_scene_overflow_is_visual_only_and_uses_measured_multiline_bounds() {
        let layout = layout();
        let frozen_layout = layout.clone();
        let cell_id = layout.cells[0].cell_id.clone();
        let text = Arc::new(crate::radial::font_cache::PreparedTextLayout {
            source_text: Arc::from("Complete label\nDistinct description"),
            text: Arc::from("Complete label\nDistinct description"),
            selected_family: Arc::from("Segoe UI"),
            script: ScriptClass::Latin,
            estimated_width_milli: 120_000,
            measured_width_milli: 120_000,
            measured_height_milli: 32_000,
            line_count: 2,
            glyphs: Vec::new(),
            diagnostics: Vec::new(),
        });
        let mut resources = PreparedSceneResources::default();
        resources.tooltips.insert(
            cell_id.clone(),
            Arc::new(PreparedTooltip {
                full_label: Arc::from("Complete label"),
                description: Some(Arc::from("Distinct description")),
                layout: text,
                font_size: 13.0,
                show_label: true,
                label_was_truncated: false,
            }),
        );
        let work_area = PhysicalRect {
            min: PhysicalPoint {
                x: -200.0,
                y: -100.0,
            },
            max: PhysicalPoint { x: 800.0, y: 700.0 },
        };
        let hidden = build_scene_prepared_selected_tooltip(
            &layout,
            10,
            &resources,
            Some(&cell_id),
            None,
            work_area,
        );
        let shown = build_scene_prepared_selected_tooltip(
            &layout,
            11,
            &resources,
            Some(&cell_id),
            Some(&cell_id),
            work_area,
        );
        let tooltip_bounds = shown
            .primitives
            .iter()
            .find_map(|primitive| match primitive {
                VectorPrimitive::Tooltip { bounds, text, .. } => {
                    assert_eq!(text.logical_size(), (136.0, 48.0));
                    Some(*bounds)
                }
                _ => None,
            });

        assert!(tooltip_bounds.is_some());
        assert_eq!(hidden.bounds, layout.visual_extent);
        assert!(hidden.primitives.iter().any(|primitive| matches!(
            primitive,
            VectorPrimitive::FilledCircle {
                color: Rgba(92, 112, 148, 252),
                ..
            } | VectorPrimitive::FilledWedge {
                color: Rgba(92, 112, 148, 252),
                ..
            }
        )));
        assert_ne!(shown.bounds, layout.visual_extent);
        assert_eq!(layout, frozen_layout);
        assert_eq!(layout.input_regions, frozen_layout.input_regions);
        assert_eq!(layout.input_extent, frozen_layout.input_extent);
        assert!(
            hidden
                .primitives
                .iter()
                .all(|primitive| { !matches!(primitive, VectorPrimitive::Tooltip { .. }) })
        );
    }

    #[test]
    fn prepared_media_is_embedded_as_data_without_asset_service_access() {
        let mut document = RadialDocument::starter();
        let reference = crate::radial::model::MediaReference::ExternalFile {
            path: "prepared.png".into(),
        };
        document.menus[0].rings[0].cells[0].icon = Override::Value(reference.clone());
        let layout = crate::radial::geometry::layout_document_menu(
            &document,
            &document.menus[0],
            PhysicalPoint { x: 300.0, y: 300.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 600.0, y: 600.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        let prepared = crate::radial::assets::PreparedImage {
            frames: vec![crate::radial::assets::PreparedImageFrame {
                width: 1,
                height: 1,
                duration_ms: 0,
                rgba: Arc::from(&[255, 0, 0, 128][..]),
            }],
            animated: false,
        };
        let mut resources = PreparedSceneResources::default();
        resources.media.insert(
            reference_identity(&reference),
            crate::radial::assets::PreparedAssetSnapshot {
                media: Arc::new(PreparedMedia::Image(prepared)),
                portability: crate::radial::assets::AssetPortability::ExternalLocation,
                source: "prepared.png".into(),
            },
        );
        let scene = build_scene_prepared(&layout, 10, &resources);
        assert!(
            scene
                .primitives
                .iter()
                .any(|primitive| matches!(primitive, VectorPrimitive::Image { .. }))
        );
        let frame = crate::radial::compositor::rasterize(&scene, layout.scale_factor, 0).unwrap();
        assert!(
            frame
                .image
                .pixels()
                .any(|pixel| pixel[0] > pixel[1] && pixel[3] > 0)
        );
    }
}
