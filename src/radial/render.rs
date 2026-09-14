use super::assets::{PreparedAssetSnapshot, PreparedImage, PreparedMedia, reference_identity};
use super::font_cache::{FontDiagnostic, PreparedTextLayout, ScriptClass};
use super::geometry::{HitShape, LayoutSnapshot, LogicalPoint, LogicalRect};
use super::model::{CellId, MediaReference, Override};
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
        text: Arc<PreparedTextLayout>,
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

#[derive(Clone, Debug, Default)]
pub struct PreparedSceneResources {
    /// Keys are stable `reference_identity` values, never mutable indices.
    pub media: BTreeMap<String, PreparedAssetSnapshot>,
    pub text: BTreeMap<CellId, Arc<PreparedTextLayout>>,
    pub tooltips: BTreeMap<CellId, Arc<PreparedTextLayout>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputOwner {
    Actionable(CellId),
    Protective,
    Exterior,
}

pub fn build_scene(layout: &LayoutSnapshot, generation: u64) -> VectorScene {
    build_scene_internal(layout, generation, &PreparedSceneResources::default(), None)
}

pub fn build_scene_prepared(
    layout: &LayoutSnapshot,
    generation: u64,
    resources: &PreparedSceneResources,
) -> VectorScene {
    build_scene_internal(layout, generation, resources, None)
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
    )
}

pub fn build_scene_prepared_selected(
    layout: &LayoutSnapshot,
    generation: u64,
    resources: &PreparedSceneResources,
    selected: Option<&CellId>,
) -> VectorScene {
    build_scene_internal(layout, generation, resources, selected)
}

fn build_scene_internal(
    layout: &LayoutSnapshot,
    generation: u64,
    resources: &PreparedSceneResources,
    selected: Option<&CellId>,
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
        if selected == Some(&cell.cell_id)
            && cell.visual.tooltip_mode != super::model::TooltipMode::Disabled
            && let Some(tooltip) = resources.tooltips.get(&cell.cell_id)
        {
            let tooltip_bounds = LogicalRect {
                min: LogicalPoint {
                    x: bounds.min.x,
                    y: bounds.max.y + 4.0,
                },
                max: LogicalPoint {
                    x: bounds.max.x.max(bounds.min.x + 80.0),
                    y: bounds.max.y + 24.0,
                },
            };
            primitives.push(VectorPrimitive::Tooltip {
                bounds: tooltip_bounds,
                text: Arc::clone(tooltip),
                background: Rgba(10, 11, 14, 235),
                color: Rgba(248, 248, 248, 255),
            });
        }
    }
    push_image(
        &mut primitives,
        resources,
        &layout.style.menu_foreground,
        scaled_rect(layout.background_extent, layout.style.menu_foreground_scale),
        layout.style.image_quality,
        opacity(layout.style.menu_foreground_opacity),
    );
    VectorScene {
        bounds: layout.visual_extent,
        generation,
        shape_quality: layout.style.shape_quality,
        primitives,
    }
}

fn fallback_text(text: &str, family: &str) -> Arc<PreparedTextLayout> {
    Arc::new(PreparedTextLayout {
        text: Arc::from(text),
        selected_family: Arc::from(if family.is_empty() {
            "Segoe UI"
        } else {
            family
        }),
        script: ScriptClass::Mixed,
        estimated_width_milli: 0,
        glyphs: Vec::new(),
        diagnostics: Vec::<FontDiagnostic>::new(),
    })
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
    if !inside_rect(layout.input_extent, point) || !inside_owned_background(layout, point) {
        return InputOwner::Exterior;
    }
    if ancestor {
        return InputOwner::Protective;
    }
    if let Some(cell) = layout
        .cells
        .iter()
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
    use crate::radial::geometry::{PhysicalPoint, PhysicalRect, ScaleFactor, layout_menu};
    use crate::radial::model::RadialDocument;

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
            matches!(primitive, VectorPrimitive::Text { text, size, .. } if text == "Recent" && *size == 22.0)
        }));
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
