use super::model::{
    CellContent, CellId, Control, LayoutKind, MediaReference, MenuDefinition, Override,
    RenderingQuality, RingId, TooltipMode,
};
use super::skin::{
    EffectiveCellStyle, EffectiveMenuTree, StyleField, StyleSource, compile_menu_tree,
    resolved_bool, resolved_clone, resolved_f32,
};
use std::f32::consts::TAU;

/// A point in physical desktop coordinates. These coordinates can be negative
/// on monitors positioned above or to the left of the primary display.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PhysicalPoint {
    pub x: f64,
    pub y: f64,
}
/// A point in desktop-logical coordinates. Layout snapshots store absolute
/// desktop-logical positions, not coordinates relative to their HWND.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LogicalPoint {
    pub x: f32,
    pub y: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LogicalRect {
    pub min: LogicalPoint,
    pub max: LogicalPoint,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PhysicalRect {
    pub min: PhysicalPoint,
    pub max: PhysicalPoint,
}

/// Spatial inputs captured for one radial session. The requested anchor is
/// retained for diagnostics; navigation is positioned from `visible_center`.
/// Work area and scale stay frozen for the lifetime of this topology epoch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrozenSpatialContext {
    pub requested_root_anchor: PhysicalPoint,
    pub visible_center: PhysicalPoint,
    pub work_area: PhysicalRect,
    pub scale_factor: ScaleFactor,
    pub spatial_generation: u64,
    pub topology_generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleFactor(f64);
impl ScaleFactor {
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value > 0.0).then_some(Self(value))
    }
    pub fn get(self) -> f64 {
        self.0
    }
    pub fn physical_to_logical(self, point: PhysicalPoint) -> LogicalPoint {
        LogicalPoint {
            x: (point.x / self.0) as f32,
            y: (point.y / self.0) as f32,
        }
    }
    pub fn logical_to_physical(self, point: LogicalPoint) -> PhysicalPoint {
        PhysicalPoint {
            x: point.x as f64 * self.0,
            y: point.y as f64 * self.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum HitShape {
    Circle {
        center: LogicalPoint,
        radius: f32,
    },
    Wedge {
        center: LogicalPoint,
        inner_radius: f32,
        outer_radius: f32,
        start_angle: f32,
        end_angle: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CellLayout {
    pub cell_id: CellId,
    pub ring_id: RingId,
    pub label: String,
    pub icon: Override<MediaReference>,
    pub control: Option<Control>,
    pub secondary_control: Option<Control>,
    pub shape: HitShape,
    pub actionable: bool,
    pub visual: CellVisualStyle,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellVisualStyle {
    pub item_background: Override<MediaReference>,
    pub item_foreground: Override<MediaReference>,
    pub item_shadow: Override<MediaReference>,
    pub submenu_indicator: Override<MediaReference>,
    pub item_image_scale: f32,
    pub item_image_y_ratio: f32,
    pub item_background_scale: f32,
    pub item_foreground_scale: f32,
    pub item_shadow_scale: f32,
    pub submenu_indicator_size: f32,
    pub submenu_indicator_y_ratio: f32,
    pub glow_enabled: bool,
    pub text_visible: bool,
    pub font_size: f32,
    pub font_family: String,
    pub text_color: super::model::ColorRgba,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
    pub shadow_enabled: bool,
    pub shadow_color: super::model::ColorRgba,
    pub shadow_offset: super::model::Offset2D,
    pub text_box_scale: f32,
    pub text_y_ratio: f32,
    pub image_quality: RenderingQuality,
    pub text_quality: RenderingQuality,
    pub item_background_opacity: f32,
    pub item_foreground_opacity: f32,
    pub item_shadow_opacity: f32,
    pub submenu_indicator_opacity: f32,
    pub icon_opacity: f32,
    pub item_background_visible: bool,
    pub tooltip_mode: TooltipMode,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutStyleSnapshot {
    pub item_glow: Override<MediaReference>,
    pub menu_outer_rim: Override<MediaReference>,
    pub menu_background: Override<MediaReference>,
    pub menu_foreground: Override<MediaReference>,
    pub center_background: Override<MediaReference>,
    pub center_image: Override<MediaReference>,
    pub item_size: f32,
    pub menu_scale: f32,
    pub radius_scale: f32,
    pub center_size: f32,
    pub center_image_scale: f32,
    pub menu_background_scale: f32,
    pub menu_foreground_scale: f32,
    pub center_background_scale: f32,
    pub outer_ring_margin: f32,
    pub outer_rim_width: f32,
    pub glow_extent: f32,
    pub text_extent: f32,
    pub fill_center_hit_zone: bool,
    pub fill_item_hit_zones: bool,
    pub item_background_on_center: bool,
    pub image_quality: RenderingQuality,
    pub shape_quality: RenderingQuality,
    pub menu_outer_rim_opacity: f32,
    pub menu_background_opacity: f32,
    pub menu_foreground_opacity: f32,
    pub center_background_opacity: f32,
    pub center_image_opacity: f32,
    pub item_glow_opacity: f32,
    pub menu_shadow_width: f32,
    pub menu_shadow_inner_color: super::model::ColorRgba,
    pub menu_shadow_outer_color: super::model::ColorRgba,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutSnapshot {
    /// Physical desktop point requested for this placement. For a root this
    /// is the sampled cursor; children store their requested local/fixed point.
    /// Navigation must use the session's actual visible center instead.
    pub requested_anchor: PhysicalPoint,
    /// Physical desktop position of `center`, after root fitting.
    pub origin: PhysicalPoint,
    /// DPI scale used to convert desktop-logical geometry to physical pixels.
    pub scale_factor: ScaleFactor,
    pub scale: f32,
    pub center: LogicalPoint,
    pub center_radius: f32,
    pub background_extent: LogicalRect,
    pub rim_extent: LogicalRect,
    pub visual_extent: LogicalRect,
    pub input_extent: LogicalRect,
    /// Disjoint owned wheel/tree regions. Keeping these explicit preserves
    /// click-through between cascaded menus while their internal gaps remain protected.
    pub input_regions: Vec<HitShape>,
    pub cells: Vec<CellLayout>,
    pub style: LayoutStyleSnapshot,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LayoutError {
    InvalidScale,
    InvalidWorkArea,
    InvalidStyle,
    Oversized {
        required_scale: f32,
        minimum_scale: f32,
    },
    FixedCenterDoesNotFit {
        required_scale: f32,
        minimum_scale: f32,
    },
    InvalidTranslation,
}

pub fn layout_document_menu(
    document: &super::model::RadialDocument,
    menu: &MenuDefinition,
    requested_anchor: PhysicalPoint,
    work_area: PhysicalRect,
    scale_factor: ScaleFactor,
    minimum_scale: f32,
) -> Result<LayoutSnapshot, LayoutError> {
    let style = compile_menu_tree(document, menu).map_err(|_| LayoutError::InvalidStyle)?;
    layout_menu_with_style(
        menu,
        &style,
        requested_anchor,
        work_area,
        scale_factor,
        minimum_scale,
    )
}

/// Lay out a child or restored frame at exactly `center`. Unlike the flexible
/// root fit, this placement never clamps the center or silently shifts it.
pub fn layout_document_menu_fixed_center(
    document: &super::model::RadialDocument,
    menu: &MenuDefinition,
    center: PhysicalPoint,
    work_area: PhysicalRect,
    scale_factor: ScaleFactor,
    minimum_scale: f32,
) -> Result<LayoutSnapshot, LayoutError> {
    let style = compile_menu_tree(document, menu).map_err(|_| LayoutError::InvalidStyle)?;
    layout_menu_impl(
        menu,
        Some(&style),
        center,
        work_area,
        scale_factor,
        minimum_scale,
        Placement::FixedCenter,
    )
}

#[derive(Clone, Copy)]
enum Placement {
    FlexibleRoot,
    FixedCenter,
}

pub fn layout_menu(
    menu: &MenuDefinition,
    requested_anchor: PhysicalPoint,
    work_area: PhysicalRect,
    scale_factor: ScaleFactor,
    minimum_scale: f32,
) -> Result<LayoutSnapshot, LayoutError> {
    layout_menu_impl(
        menu,
        None,
        requested_anchor,
        work_area,
        scale_factor,
        minimum_scale,
        Placement::FlexibleRoot,
    )
}

pub fn layout_menu_with_style(
    menu: &MenuDefinition,
    style: &EffectiveMenuTree,
    requested_anchor: PhysicalPoint,
    work_area: PhysicalRect,
    scale_factor: ScaleFactor,
    minimum_scale: f32,
) -> Result<LayoutSnapshot, LayoutError> {
    layout_menu_impl(
        menu,
        Some(style),
        requested_anchor,
        work_area,
        scale_factor,
        minimum_scale,
        Placement::FlexibleRoot,
    )
}

fn layout_menu_impl(
    menu: &MenuDefinition,
    style: Option<&EffectiveMenuTree>,
    requested_anchor: PhysicalPoint,
    work_area: PhysicalRect,
    scale_factor: ScaleFactor,
    minimum_scale: f32,
    placement: Placement,
) -> Result<LayoutSnapshot, LayoutError> {
    if !minimum_scale.is_finite() || minimum_scale <= 0.0 || minimum_scale > 1.0 {
        return Err(LayoutError::InvalidScale);
    }
    let work_min = scale_factor.physical_to_logical(work_area.min);
    let work_max = scale_factor.physical_to_logical(work_area.max);
    let width = work_max.x - work_min.x;
    let height = work_max.y - work_min.y;
    if width <= 0.0 || height <= 0.0 || !width.is_finite() || !height.is_finite() {
        return Err(LayoutError::InvalidWorkArea);
    }
    let menu_values = style.map(|style| &style.menu.values);
    let menu_scale = menu_values
        .map(|values| resolved_f32(&values.geometry.menu_scale))
        .unwrap_or(1.0);
    let radius_scale = menu_values
        .map(|values| resolved_f32(&values.geometry.radius_scale))
        .unwrap_or(1.0);
    let styled_item_size = style
        .filter(|style| {
            style.menu.source(StyleField::ItemSize) != Some(&StyleSource::ApplicationFallback)
        })
        .map(|style| resolved_f32(&style.menu.values.geometry.item_size) * menu_scale);
    let center_radius = style
        .filter(|style| {
            style.menu.source(StyleField::CenterSize) != Some(&StyleSource::ApplicationFallback)
        })
        .map(|style| resolved_f32(&style.menu.values.geometry.center_size) * menu_scale * 0.5)
        .unwrap_or(menu.center_radius * menu_scale);
    let outer_ring_margin = menu_values
        .map(|values| resolved_f32(&values.geometry.outer_ring_margin))
        .unwrap_or(0.0)
        * menu_scale;
    let outer_rim_width = menu_values
        .map(|values| resolved_f32(&values.geometry.outer_rim_width))
        .unwrap_or(0.0)
        * menu_scale;
    let glow_enabled = menu_values
        .map(|values| resolved_bool(&values.effects.glow_enabled))
        .unwrap_or(false);
    let text_visible = menu_values
        .map(|values| resolved_bool(&values.text.visible))
        .unwrap_or(true);
    let font_size = menu_values
        .map(|values| resolved_f32(&values.text.font_size))
        .unwrap_or(12.0);
    let text_box_scale = menu_values
        .map(|values| resolved_f32(&values.text.text_box_scale))
        .unwrap_or(1.0);
    let glow_extent = if glow_enabled {
        styled_item_size.map(|size| size * 0.5).unwrap_or_else(|| {
            menu.rings
                .iter()
                .map(|ring| ring.cell_radius)
                .fold(0.0, f32::max)
                * menu_scale
        }) * 0.25
    } else {
        0.0
    };
    let text_extent = style.map_or_else(
        || {
            if text_visible {
                font_size * menu_scale * text_box_scale * 0.75
            } else {
                0.0
            }
        },
        |style| {
            style
                .cells
                .values()
                .filter(|cell| resolved_bool(&cell.values.text.visible))
                .map(|cell| {
                    resolved_f32(&cell.values.text.font_size)
                        * menu_scale
                        * resolved_f32(&cell.values.text.text_box_scale)
                        * 0.75
                })
                .fold(0.0, f32::max)
        },
    );
    let menu_shadow_width = menu_values
        .map(|values| resolved_f32(&values.effects.menu_shadow_width))
        .unwrap_or(0.0)
        * menu_scale;
    let content_nominal = menu
        .rings
        .iter()
        .map(|ring| {
            ring.radius * radius_scale * menu_scale
                + styled_item_size.map_or(ring.cell_radius * menu_scale, |size| size * 0.5)
        })
        .fold(center_radius, f32::max);
    let background_nominal = content_nominal + outer_ring_margin;
    let rim_nominal = background_nominal + outer_rim_width;
    // Reserve deterministic room for labels, glow, and the outer rim. Later
    // renderers may use less, but may not draw outside this snapshot extent.
    let visual_padding = 12.0 + glow_extent + text_extent + menu_shadow_width;
    let requested_logical = scale_factor.physical_to_logical(requested_anchor);
    let (fit_width, fit_height) = match placement {
        Placement::FlexibleRoot => (width, height),
        Placement::FixedCenter => {
            let available_left = (requested_logical.x - work_min.x).max(0.0);
            let available_right = (work_max.x - requested_logical.x).max(0.0);
            let available_top = (requested_logical.y - work_min.y).max(0.0);
            let available_bottom = (work_max.y - requested_logical.y).max(0.0);
            (
                2.0 * available_left.min(available_right),
                2.0 * available_top.min(available_bottom),
            )
        }
    };
    let required_scale =
        (fit_width.min(fit_height) / ((rim_nominal + visual_padding) * 2.0)).min(1.0);
    if required_scale < minimum_scale {
        return Err(match placement {
            Placement::FlexibleRoot => LayoutError::Oversized {
                required_scale,
                minimum_scale,
            },
            Placement::FixedCenter => LayoutError::FixedCenterDoesNotFit {
                required_scale,
                minimum_scale,
            },
        });
    }
    let scale = required_scale;
    let input_margin = background_nominal * scale;
    let rim_margin = rim_nominal * scale;
    let visual_margin = (rim_nominal + visual_padding) * scale;
    let center = match placement {
        Placement::FlexibleRoot => LogicalPoint {
            x: requested_logical
                .x
                .clamp(work_min.x + visual_margin, work_max.x - visual_margin),
            y: requested_logical
                .y
                .clamp(work_min.y + visual_margin, work_max.y - visual_margin),
        },
        Placement::FixedCenter => requested_logical,
    };
    let origin = scale_factor.logical_to_physical(center);
    let mut cells = Vec::new();
    for ring in &menu.rings {
        let n = ring.cells.len();
        if n == 0 {
            continue;
        }
        let start = ring.rotation_degrees.to_radians();
        for (index, cell) in ring.cells.iter().enumerate() {
            let effective_cell = style.and_then(|style| style.cells.get(&cell.id));
            let cell_radius =
                styled_item_size.map_or(ring.cell_radius * menu_scale, |size| size * 0.5);
            let styled_radius = ring.radius * radius_scale * menu_scale;
            let shape = match menu.layout {
                LayoutKind::CircularCells => {
                    let angle = if n == 1 {
                        start
                    } else {
                        start + TAU * index as f32 / n as f32
                    };
                    HitShape::Circle {
                        center: LogicalPoint {
                            x: center.x + styled_radius * scale * angle.cos(),
                            y: center.y + styled_radius * scale * angle.sin(),
                        },
                        radius: cell_radius * scale,
                    }
                }
                LayoutKind::Wedges => {
                    let width = TAU / n as f32;
                    let half_gap = if n <= 1 {
                        0.0
                    } else if styled_radius > 0.0 {
                        (ring.gap / styled_radius).min(width * 0.8) * 0.5
                    } else {
                        0.0
                    };
                    HitShape::Wedge {
                        center,
                        inner_radius: (styled_radius - cell_radius).max(center_radius) * scale,
                        outer_radius: (styled_radius + cell_radius) * scale,
                        start_angle: start + width * index as f32 + half_gap,
                        end_angle: start + width * (index + 1) as f32 - half_gap,
                    }
                }
            };
            cells.push(CellLayout {
                cell_id: cell.id.clone(),
                ring_id: ring.id.clone(),
                label: cell.label.clone(),
                icon: cell.icon.clone(),
                control: match &cell.content {
                    CellContent::Control { control } => Some(*control),
                    _ => None,
                },
                secondary_control: None,
                shape,
                actionable: !matches!(cell.content, super::model::CellContent::Spacer),
                visual: cell_visual(
                    effective_cell,
                    glow_enabled,
                    text_visible,
                    font_size,
                    text_box_scale,
                    menu_scale,
                ),
            });
        }
    }
    let input_extent = LogicalRect {
        min: LogicalPoint {
            x: center.x - input_margin,
            y: center.y - input_margin,
        },
        max: LogicalPoint {
            x: center.x + input_margin,
            y: center.y + input_margin,
        },
    };
    let visual_extent = LogicalRect {
        min: LogicalPoint {
            x: center.x - visual_margin,
            y: center.y - visual_margin,
        },
        max: LogicalPoint {
            x: center.x + visual_margin,
            y: center.y + visual_margin,
        },
    };
    let rim_extent = LogicalRect {
        min: LogicalPoint {
            x: center.x - rim_margin,
            y: center.y - rim_margin,
        },
        max: LogicalPoint {
            x: center.x + rim_margin,
            y: center.y + rim_margin,
        },
    };
    Ok(LayoutSnapshot {
        requested_anchor,
        origin,
        scale_factor,
        scale,
        center,
        center_radius: center_radius * scale,
        background_extent: input_extent,
        rim_extent,
        visual_extent,
        input_extent,
        input_regions: vec![HitShape::Circle {
            center,
            radius: input_margin,
        }],
        cells,
        style: LayoutStyleSnapshot {
            item_glow: menu_values
                .map(|values| values.images.item_glow.clone())
                .unwrap_or(Override::Clear),
            menu_outer_rim: menu_values
                .map(|values| values.images.menu_outer_rim.clone())
                .unwrap_or(Override::Clear),
            menu_background: menu_values
                .map(|values| values.images.menu_background.clone())
                .unwrap_or(Override::Clear),
            menu_foreground: menu_values
                .map(|values| values.images.menu_foreground.clone())
                .unwrap_or(Override::Clear),
            center_background: menu_values
                .map(|values| values.images.center_background.clone())
                .unwrap_or(Override::Clear),
            center_image: menu_values
                .map(|values| values.images.center_image.clone())
                .unwrap_or(Override::Clear),
            item_size: styled_item_size.unwrap_or_else(|| {
                menu.rings
                    .first()
                    .map_or(56.0, |ring| ring.cell_radius * 2.0)
                    * menu_scale
            }),
            menu_scale,
            radius_scale,
            center_size: center_radius * 2.0,
            center_image_scale: menu_values
                .map(|values| resolved_f32(&values.geometry.center_image_scale))
                .unwrap_or(1.0),
            menu_background_scale: menu_values
                .map(|values| resolved_f32(&values.geometry.menu_background_scale))
                .unwrap_or(1.0),
            menu_foreground_scale: menu_values
                .map(|values| resolved_f32(&values.geometry.menu_foreground_scale))
                .unwrap_or(1.0),
            center_background_scale: menu_values
                .map(|values| resolved_f32(&values.geometry.center_background_scale))
                .unwrap_or(1.0),
            outer_ring_margin,
            outer_rim_width,
            glow_extent,
            text_extent,
            fill_center_hit_zone: menu_values
                .map(|values| resolved_bool(&values.window.fill_center_hit_zone))
                .unwrap_or(true),
            fill_item_hit_zones: menu_values
                .map(|values| resolved_bool(&values.window.fill_item_hit_zones))
                .unwrap_or(true),
            item_background_on_center: menu_values
                .map(|values| resolved_bool(&values.geometry.item_background_on_center))
                .unwrap_or(true),
            image_quality: menu_values
                .map(|values| resolved_clone(&values.quality.interpolation))
                .unwrap_or_default(),
            shape_quality: menu_values
                .map(|values| resolved_clone(&values.quality.shape))
                .unwrap_or_default(),
            menu_outer_rim_opacity: menu_values
                .map(|v| resolved_f32(&v.images.menu_outer_rim_opacity))
                .unwrap_or(1.0),
            menu_background_opacity: menu_values
                .map(|v| resolved_f32(&v.images.menu_background_opacity))
                .unwrap_or(1.0),
            menu_foreground_opacity: menu_values
                .map(|v| resolved_f32(&v.images.menu_foreground_opacity))
                .unwrap_or(1.0),
            center_background_opacity: menu_values
                .map(|v| resolved_f32(&v.images.center_background_opacity))
                .unwrap_or(1.0),
            center_image_opacity: menu_values
                .map(|v| resolved_f32(&v.images.center_image_opacity))
                .unwrap_or(1.0),
            item_glow_opacity: menu_values
                .map(|v| resolved_f32(&v.images.item_glow_opacity))
                .unwrap_or(1.0),
            menu_shadow_width,
            menu_shadow_inner_color: menu_values
                .map(|v| resolved_clone(&v.effects.menu_shadow_inner_color))
                .unwrap_or_default(),
            menu_shadow_outer_color: menu_values
                .map(|v| resolved_clone(&v.effects.menu_shadow_outer_color))
                .unwrap_or_default(),
        },
    })
}

/// Translate an already fitted desktop-logical layout by a physical desktop
/// delta. The original requested anchor remains diagnostic history.
pub fn translate_layout(
    layout: &mut LayoutSnapshot,
    physical_delta: PhysicalPoint,
) -> Result<(), LayoutError> {
    if !physical_delta.x.is_finite() || !physical_delta.y.is_finite() {
        return Err(LayoutError::InvalidTranslation);
    }
    let logical_delta = LogicalPoint {
        x: (physical_delta.x / layout.scale_factor.get()) as f32,
        y: (physical_delta.y / layout.scale_factor.get()) as f32,
    };
    if !logical_delta.x.is_finite() || !logical_delta.y.is_finite() {
        return Err(LayoutError::InvalidTranslation);
    }
    let mut translated = layout.clone();
    translated.origin.x += physical_delta.x;
    translated.origin.y += physical_delta.y;
    translate_logical_point(&mut translated.center, logical_delta);
    translate_rect(&mut translated.background_extent, logical_delta);
    translate_rect(&mut translated.rim_extent, logical_delta);
    translate_rect(&mut translated.visual_extent, logical_delta);
    translate_rect(&mut translated.input_extent, logical_delta);
    for shape in &mut translated.input_regions {
        translate_shape(shape, logical_delta);
    }
    for cell in &mut translated.cells {
        translate_shape(&mut cell.shape, logical_delta);
    }
    if !translated.origin.x.is_finite()
        || !translated.origin.y.is_finite()
        || !logical_point_is_finite(translated.center)
        || !logical_rect_is_finite(translated.background_extent)
        || !logical_rect_is_finite(translated.rim_extent)
        || !logical_rect_is_finite(translated.visual_extent)
        || !logical_rect_is_finite(translated.input_extent)
        || translated
            .input_regions
            .iter()
            .any(|shape| !shape_center_is_finite(shape))
        || translated
            .cells
            .iter()
            .any(|cell| !shape_center_is_finite(&cell.shape))
    {
        return Err(LayoutError::InvalidTranslation);
    }
    *layout = translated;
    Ok(())
}

/// Convert an absolute desktop-logical hit-shape center to physical desktop
/// coordinates. `LayoutSnapshot::origin` is intentionally not added again.
pub fn shape_center(shape: &HitShape, scale_factor: ScaleFactor) -> PhysicalPoint {
    let logical = match shape {
        HitShape::Circle { center, .. } | HitShape::Wedge { center, .. } => *center,
    };
    scale_factor.logical_to_physical(logical)
}

/// Compose the displayed ancestor regions with a Cascade child. Ancestor
/// cells remain visible but non-actionable; their owned hit regions remain
/// available for correct native input parity.
pub fn cascade_layout(parent: &LayoutSnapshot, mut child: LayoutSnapshot) -> LayoutSnapshot {
    let mut ancestors = parent.cells.clone();
    for cell in &mut ancestors {
        cell.actionable = false;
    }
    ancestors.extend(child.cells);
    child.cells = ancestors;
    let mut input_regions = parent.input_regions.clone();
    input_regions.extend(child.input_regions);
    child.input_regions = input_regions;
    child.input_extent.min.x = child.input_extent.min.x.min(parent.input_extent.min.x);
    child.input_extent.min.y = child.input_extent.min.y.min(parent.input_extent.min.y);
    child.input_extent.max.x = child.input_extent.max.x.max(parent.input_extent.max.x);
    child.input_extent.max.y = child.input_extent.max.y.max(parent.input_extent.max.y);
    child.visual_extent.min.x = child.visual_extent.min.x.min(parent.visual_extent.min.x);
    child.visual_extent.min.y = child.visual_extent.min.y.min(parent.visual_extent.min.y);
    child.visual_extent.max.x = child.visual_extent.max.x.max(parent.visual_extent.max.x);
    child.visual_extent.max.y = child.visual_extent.max.y.max(parent.visual_extent.max.y);
    child
}

fn translate_logical_point(point: &mut LogicalPoint, delta: LogicalPoint) {
    point.x += delta.x;
    point.y += delta.y;
}

fn logical_point_is_finite(point: LogicalPoint) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

fn logical_rect_is_finite(rect: LogicalRect) -> bool {
    logical_point_is_finite(rect.min) && logical_point_is_finite(rect.max)
}

fn shape_center_is_finite(shape: &HitShape) -> bool {
    match shape {
        HitShape::Circle { center, .. } | HitShape::Wedge { center, .. } => {
            logical_point_is_finite(*center)
        }
    }
}

fn translate_rect(rect: &mut LogicalRect, delta: LogicalPoint) {
    translate_logical_point(&mut rect.min, delta);
    translate_logical_point(&mut rect.max, delta);
}

fn translate_shape(shape: &mut HitShape, delta: LogicalPoint) {
    match shape {
        HitShape::Circle { center, .. } | HitShape::Wedge { center, .. } => {
            translate_logical_point(center, delta)
        }
    }
}

fn cell_visual(
    style: Option<&EffectiveCellStyle>,
    glow_enabled: bool,
    text_visible: bool,
    font_size: f32,
    text_box_scale: f32,
    menu_scale: f32,
) -> CellVisualStyle {
    let values = style.map(|style| &style.values);
    CellVisualStyle {
        item_background: values
            .map(|values| values.images.item_background.clone())
            .unwrap_or(Override::Clear),
        item_foreground: values
            .map(|values| values.images.item_foreground.clone())
            .unwrap_or(Override::Clear),
        item_shadow: values
            .map(|values| values.images.item_shadow.clone())
            .unwrap_or(Override::Clear),
        submenu_indicator: values
            .map(|values| values.images.submenu_indicator.clone())
            .unwrap_or(Override::Clear),
        item_image_scale: values
            .map(|values| resolved_f32(&values.geometry.item_image_scale))
            .unwrap_or(1.0),
        item_image_y_ratio: values
            .map(|values| resolved_f32(&values.geometry.item_image_y_ratio))
            .unwrap_or(0.5),
        item_background_scale: values
            .map(|values| resolved_f32(&values.geometry.item_background_scale))
            .unwrap_or(1.0),
        item_foreground_scale: values
            .map(|values| resolved_f32(&values.geometry.item_foreground_scale))
            .unwrap_or(1.0),
        item_shadow_scale: values
            .map(|values| resolved_f32(&values.geometry.item_shadow_scale))
            .unwrap_or(1.0),
        submenu_indicator_size: values
            .map(|values| resolved_f32(&values.geometry.submenu_indicator_size) * menu_scale)
            .unwrap_or(12.0 * menu_scale),
        submenu_indicator_y_ratio: values
            .map(|values| resolved_f32(&values.geometry.submenu_indicator_y_ratio))
            .unwrap_or(0.75),
        glow_enabled,
        text_visible: values
            .map(|values| resolved_bool(&values.text.visible))
            .unwrap_or(text_visible),
        font_size: values
            .map(|values| resolved_f32(&values.text.font_size) * menu_scale)
            .unwrap_or(font_size * menu_scale),
        font_family: values
            .map(|values| resolved_clone(&values.text.font_family))
            .unwrap_or_default(),
        text_color: values
            .map(|values| resolved_clone(&values.text.color))
            .unwrap_or(super::model::ColorRgba {
                red: 245,
                green: 245,
                blue: 245,
                alpha: 255,
            }),
        bold: values
            .map(|values| resolved_bool(&values.text.bold))
            .unwrap_or(false),
        italic: values
            .map(|values| resolved_bool(&values.text.italic))
            .unwrap_or(false),
        underline: values
            .map(|values| resolved_bool(&values.text.underline))
            .unwrap_or(false),
        strikeout: values
            .map(|values| resolved_bool(&values.text.strikeout))
            .unwrap_or(false),
        shadow_enabled: values
            .map(|values| resolved_bool(&values.text.shadow_enabled))
            .unwrap_or(false),
        shadow_color: values
            .map(|values| resolved_clone(&values.text.shadow_color))
            .unwrap_or_default(),
        shadow_offset: values
            .map(|values| {
                let offset = resolved_clone(&values.text.shadow_offset);
                super::model::Offset2D {
                    x: offset.x * menu_scale,
                    y: offset.y * menu_scale,
                }
            })
            .unwrap_or_default(),
        text_box_scale: values
            .map(|values| resolved_f32(&values.text.text_box_scale))
            .unwrap_or(text_box_scale),
        text_y_ratio: values
            .map(|values| resolved_f32(&values.text.vertical_ratio))
            .unwrap_or(0.5),
        image_quality: values
            .map(|values| resolved_clone(&values.quality.interpolation))
            .unwrap_or_default(),
        text_quality: values
            .map(|values| resolved_clone(&values.quality.text))
            .unwrap_or_default(),
        item_background_opacity: values
            .map(|v| resolved_f32(&v.images.item_background_opacity))
            .unwrap_or(1.0),
        item_foreground_opacity: values
            .map(|v| resolved_f32(&v.images.item_foreground_opacity))
            .unwrap_or(1.0),
        item_shadow_opacity: values
            .map(|v| resolved_f32(&v.images.item_shadow_opacity))
            .unwrap_or(1.0),
        submenu_indicator_opacity: values
            .map(|v| resolved_f32(&v.images.submenu_indicator_opacity))
            .unwrap_or(1.0),
        icon_opacity: values
            .map(|v| resolved_f32(&v.images.icon_opacity))
            .unwrap_or(1.0),
        item_background_visible: values
            .map(|v| resolved_bool(&v.geometry.item_background_on_items))
            .unwrap_or(true),
        tooltip_mode: values
            .map(|v| resolved_clone(&v.effects.tooltip_mode))
            .unwrap_or(TooltipMode::Explicit),
    }
}

impl LayoutSnapshot {
    /// Later entries are topmost. Half-open wedge edges ensure a boundary has one owner.
    pub fn hit_test(&self, point: LogicalPoint) -> Option<&CellLayout> {
        self.cells
            .iter()
            .rev()
            .find(|cell| cell.actionable && contains(&cell.shape, point))
    }

    /// Returns the topmost cell whose authored geometry contains `point`,
    /// regardless of whether that cell owns click/action input. Tooltip hover
    /// uses this lookup so protective spacer/ancestor regions can still
    /// identify a visual cell without changing the input ownership contract
    /// of [`Self::hit_test`].
    pub fn geometric_hover_cell(&self, point: LogicalPoint) -> Option<&CellLayout> {
        self.cells
            .iter()
            .rev()
            .find(|cell| contains(&cell.shape, point))
    }
}

fn contains(shape: &HitShape, point: LogicalPoint) -> bool {
    match shape {
        HitShape::Circle { center, radius } => distance2(*center, point) <= radius * radius,
        HitShape::Wedge {
            center,
            inner_radius,
            outer_radius,
            start_angle,
            end_angle,
        } => {
            let dx = point.x - center.x;
            let dy = point.y - center.y;
            let radius2 = dx * dx + dy * dy;
            if radius2 < inner_radius * inner_radius || radius2 > outer_radius * outer_radius {
                return false;
            }
            let angle = dy.atan2(dx);
            angle_in_interval(angle, *start_angle, *end_angle)
        }
    }
}
fn distance2(a: LogicalPoint, b: LogicalPoint) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}
fn normalize(angle: f32) -> f32 {
    angle.rem_euclid(TAU)
}
fn angle_in_interval(angle: f32, start: f32, end: f32) -> bool {
    let span = end - start;
    if span.abs() >= TAU - 1e-5 {
        return true;
    }
    let width = span.rem_euclid(TAU);
    let offset = (normalize(angle) - normalize(start)).rem_euclid(TAU);
    // Every sector owns its start and excludes its end. At the wrap boundary,
    // the first sector therefore owns the point and the last does not.
    offset < width
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::model::*;
    fn menu(layout: LayoutKind) -> MenuDefinition {
        let mut m = RadialDocument::starter().menus.remove(0);
        m.layout = layout;
        m
    }
    fn work() -> PhysicalRect {
        PhysicalRect {
            min: PhysicalPoint {
                x: -1920.0,
                y: -200.0,
            },
            max: PhysicalPoint { x: 0.0, y: 880.0 },
        }
    }

    #[test]
    fn conversions_preserve_negative_coordinates_at_fractional_dpi() {
        let s = ScaleFactor::new(1.25).unwrap();
        let p = PhysicalPoint {
            x: -1234.5,
            y: 78.75,
        };
        let round = s.logical_to_physical(s.physical_to_logical(p));
        assert!((round.x - p.x).abs() < 0.001);
        assert!((round.y - p.y).abs() < 0.001);
    }
    #[test]
    fn requested_anchor_is_retained_while_center_is_clamped() {
        let l = layout_menu(
            &menu(LayoutKind::CircularCells),
            PhysicalPoint {
                x: -2500.0,
                y: -500.0,
            },
            work(),
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        assert_eq!(l.requested_anchor.x, -2500.0);
        assert!(l.origin.x >= -1920.0);
        assert!(l.visual_extent.min.x >= -1920.0);
    }

    #[test]
    fn fixed_center_preserves_visible_center_at_negative_fractional_dpi() {
        let document = RadialDocument::starter();
        let menu = &document.menus[0];
        let scale = ScaleFactor::new(1.25).unwrap();
        let center = PhysicalPoint {
            x: -1437.5,
            y: 318.75,
        };
        let work_area = PhysicalRect {
            min: PhysicalPoint {
                x: -2500.0,
                y: -500.0,
            },
            max: PhysicalPoint { x: 0.0, y: 1500.0 },
        };
        let layout =
            layout_document_menu_fixed_center(&document, menu, center, work_area, scale, 0.55)
                .unwrap();
        assert_eq!(layout.origin, center);
        assert_eq!(layout.center, scale.physical_to_logical(center));
        assert_eq!(layout.requested_anchor, center);
        if let Some(cell) = layout.cells.first() {
            let shape = shape_center(&cell.shape, scale);
            let logical = match &cell.shape {
                HitShape::Circle { center, .. } | HitShape::Wedge { center, .. } => *center,
            };
            assert_eq!(shape, scale.logical_to_physical(logical));
        }
    }

    #[test]
    fn fixed_fit_checks_all_work_area_edges_without_clamping() {
        let document = RadialDocument::starter();
        let menu = &document.menus[0];
        let work_area = PhysicalRect {
            min: PhysicalPoint { x: 0.0, y: 0.0 },
            max: PhysicalPoint {
                x: 2000.0,
                y: 1200.0,
            },
        };
        let scale = ScaleFactor::new(1.25).unwrap();
        for center in [
            PhysicalPoint { x: 40.0, y: 600.0 },
            PhysicalPoint {
                x: 1960.0,
                y: 600.0,
            },
            PhysicalPoint { x: 1000.0, y: 40.0 },
            PhysicalPoint {
                x: 1000.0,
                y: 1160.0,
            },
        ] {
            assert!(matches!(
                layout_document_menu_fixed_center(&document, menu, center, work_area, scale, 0.55,),
                Err(LayoutError::FixedCenterDoesNotFit { .. })
            ));
        }
        let center = PhysicalPoint {
            x: 1000.0,
            y: 600.0,
        };
        let layout =
            layout_document_menu_fixed_center(&document, menu, center, work_area, scale, 0.55)
                .unwrap();
        assert_eq!(layout.origin, center);
    }

    #[test]
    fn translation_moves_every_layout_coordinate_once_and_preserves_request() {
        let mut layout = layout_menu(
            &menu(LayoutKind::CircularCells),
            PhysicalPoint {
                x: -700.0,
                y: 300.0,
            },
            work(),
            ScaleFactor::new(1.25).unwrap(),
            0.5,
        )
        .unwrap();
        let original = layout.clone();
        let delta = PhysicalPoint { x: 137.5, y: -62.5 };
        translate_layout(&mut layout, delta).unwrap();
        let logical_delta = LogicalPoint { x: 110.0, y: -50.0 };
        assert_eq!(layout.origin.x, original.origin.x + delta.x);
        assert_eq!(layout.origin.y, original.origin.y + delta.y);
        assert_eq!(layout.center.x, original.center.x + logical_delta.x);
        assert_eq!(layout.center.y, original.center.y + logical_delta.y);
        assert_eq!(layout.requested_anchor, original.requested_anchor);
        assert_eq!(
            layout.visual_extent.min.x,
            original.visual_extent.min.x + logical_delta.x
        );
        assert_eq!(
            layout.input_extent.max.y,
            original.input_extent.max.y + logical_delta.y
        );
        assert_eq!(
            shape_center(&layout.cells[0].shape, layout.scale_factor).x,
            shape_center(&original.cells[0].shape, original.scale_factor).x + delta.x
        );
    }

    #[test]
    fn overflowing_translation_is_rejected_without_mutating_layout() {
        let mut layout = layout_menu(
            &menu(LayoutKind::CircularCells),
            PhysicalPoint::default(),
            work(),
            ScaleFactor::new(1.25).unwrap(),
            0.5,
        )
        .unwrap();
        layout.center.x = f32::MAX;
        let before = layout.clone();

        assert_eq!(
            translate_layout(
                &mut layout,
                PhysicalPoint {
                    x: f64::from(f32::MAX),
                    y: 0.0,
                }
            ),
            Err(LayoutError::InvalidTranslation)
        );
        assert_eq!(layout, before);
    }

    #[test]
    fn circular_spacer_reserves_geometry_but_cannot_hit() {
        let mut m = menu(LayoutKind::CircularCells);
        m.rings[0].cells[0].content = CellContent::Spacer;
        let l = layout_menu(
            &m,
            PhysicalPoint {
                x: -800.0,
                y: 300.0,
            },
            work(),
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        let p = match l.cells[0].shape {
            HitShape::Circle { center, .. } => center,
            _ => unreachable!(),
        };
        assert!(l.hit_test(p).is_none());
        assert_eq!(
            l.geometric_hover_cell(p).map(|cell| &cell.cell_id),
            Some(&l.cells[0].cell_id)
        );
        assert!(!l.geometric_hover_cell(p).unwrap().actionable);
    }
    #[test]
    fn wedge_gap_and_center_are_non_actionable_and_edges_have_one_owner() {
        let l = layout_menu(
            &menu(LayoutKind::Wedges),
            PhysicalPoint {
                x: -800.0,
                y: 300.0,
            },
            work(),
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        assert!(l.hit_test(l.center).is_none());
        let first = &l.cells[0];
        if let HitShape::Wedge {
            center,
            outer_radius,
            end_angle,
            ..
        } = first.shape
        {
            let p = LogicalPoint {
                x: center.x + outer_radius * 0.8 * end_angle.cos(),
                y: center.y + outer_radius * 0.8 * end_angle.sin(),
            };
            let owners = l
                .cells
                .iter()
                .filter(|cell| contains(&cell.shape, p))
                .count();
            assert!(owners <= 1);
        } else {
            unreachable!()
        }
    }

    #[test]
    fn one_cell_wedge_owns_the_full_annulus() {
        let mut menu = menu(LayoutKind::Wedges);
        menu.rings[0].cells.truncate(1);
        let layout = layout_menu(
            &menu,
            PhysicalPoint {
                x: -800.0,
                y: 300.0,
            },
            work(),
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        for angle in [0.0_f32, 1.0, 3.0, 5.5] {
            let point = LogicalPoint {
                x: layout.center.x + 90.0 * angle.cos(),
                y: layout.center.y + 90.0 * angle.sin(),
            };
            assert_eq!(
                layout.hit_test(point).map(|cell| &cell.cell_id),
                Some(&layout.cells[0].cell_id)
            );
        }
    }

    #[test]
    fn gapless_wedge_wrap_boundary_has_exactly_one_owner() {
        let mut menu = menu(LayoutKind::Wedges);
        menu.rings[0].gap = 0.0;
        let layout = layout_menu(
            &menu,
            PhysicalPoint {
                x: -800.0,
                y: 300.0,
            },
            work(),
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        let start = menu.rings[0].rotation_degrees.to_radians();
        let point = LogicalPoint {
            x: layout.center.x + 90.0 * start.cos(),
            y: layout.center.y + 90.0 * start.sin(),
        };
        assert_eq!(
            layout
                .cells
                .iter()
                .filter(|cell| contains(&cell.shape, point))
                .count(),
            1
        );
    }
    #[test]
    fn zero_and_one_cell_rings_are_explicit() {
        let mut m = menu(LayoutKind::CircularCells);
        m.rings.push(RingDefinition {
            id: RingId::new("empty"),
            radius: 150.0,
            cell_radius: 20.0,
            rotation_degrees: 0.0,
            gap: 0.0,
            cells: vec![],
            style: Default::default(),
        });
        m.rings[0].cells.truncate(1);
        let l = layout_menu(
            &m,
            PhysicalPoint {
                x: -800.0,
                y: 300.0,
            },
            work(),
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        assert_eq!(l.cells.len(), 1);
    }

    #[test]
    fn generated_ring_counts_rotation_spacers_and_overlap_are_deterministic() {
        for (ring_count, counts) in [(1, vec![1]), (2, vec![3, 5]), (4, vec![1, 2, 7, 9])] {
            let mut definition = menu(LayoutKind::CircularCells);
            let template = definition.rings[0].cells[0].clone();
            definition.rings.clear();
            for (ring_index, count) in counts.into_iter().enumerate().take(ring_count) {
                let mut cells = Vec::new();
                for cell_index in 0..count {
                    let mut cell = template.clone();
                    cell.id = CellId::new(format!("r{ring_index}-c{cell_index}"));
                    if cell_index == 1 {
                        cell.content = CellContent::Spacer;
                    }
                    cells.push(cell);
                }
                definition.rings.push(RingDefinition {
                    id: RingId::new(format!("ring-{ring_index}")),
                    radius: 70.0 + ring_index as f32 * 55.0,
                    cell_radius: 22.0,
                    rotation_degrees: -33.0 + ring_index as f32 * 17.5,
                    gap: 3.0,
                    cells,
                    style: Default::default(),
                });
            }
            let layout = layout_menu(
                &definition,
                PhysicalPoint {
                    x: -900.0,
                    y: 300.0,
                },
                work(),
                ScaleFactor::new(1.25).unwrap(),
                0.5,
            )
            .unwrap();
            assert_eq!(
                layout.cells.len(),
                definition
                    .rings
                    .iter()
                    .map(|ring| ring.cells.len())
                    .sum::<usize>()
            );
            for cell in &layout.cells {
                let point = match cell.shape {
                    HitShape::Circle { center, .. } => center,
                    _ => unreachable!(),
                };
                assert_eq!(layout.hit_test(point).is_some(), cell.actionable);
            }
        }

        let mut definition = menu(LayoutKind::CircularCells);
        definition.rings.push(definition.rings[0].clone());
        definition.rings[1].id = RingId::new("overlap-top");
        definition.rings[1].cells[0].id = CellId::new("overlap-top-cell");
        let layout = layout_menu(
            &definition,
            PhysicalPoint {
                x: -900.0,
                y: 300.0,
            },
            work(),
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        let point = match layout.cells.last().unwrap().shape {
            HitShape::Circle { center, .. } => center,
            _ => unreachable!(),
        };
        assert_eq!(layout.hit_test(point), layout.cells.last());
    }

    #[test]
    fn clamping_respects_every_work_area_edge_at_fractional_dpi() {
        let work_area = PhysicalRect {
            min: PhysicalPoint {
                x: -2048.0,
                y: -120.0,
            },
            max: PhysicalPoint {
                x: -128.0,
                y: 920.0,
            },
        };
        for anchor in [
            work_area.min,
            PhysicalPoint {
                x: work_area.max.x,
                y: work_area.min.y,
            },
            PhysicalPoint {
                x: work_area.min.x,
                y: work_area.max.y,
            },
            work_area.max,
        ] {
            let layout = layout_menu(
                &menu(LayoutKind::Wedges),
                anchor,
                work_area,
                ScaleFactor::new(1.25).unwrap(),
                0.5,
            )
            .unwrap();
            assert!(layout.visual_extent.min.x >= work_area.min.x as f32 / 1.25 - 0.001);
            assert!(layout.visual_extent.min.y >= work_area.min.y as f32 / 1.25 - 0.001);
            assert!(layout.visual_extent.max.x <= work_area.max.x as f32 / 1.25 + 0.001);
            assert!(layout.visual_extent.max.y <= work_area.max.y as f32 / 1.25 + 0.001);
        }
    }
    #[test]
    fn refuses_wheel_below_usable_minimum() {
        let tiny = PhysicalRect {
            min: PhysicalPoint { x: 0.0, y: 0.0 },
            max: PhysicalPoint { x: 20.0, y: 20.0 },
        };
        assert!(matches!(
            layout_menu(
                &menu(LayoutKind::CircularCells),
                PhysicalPoint::default(),
                tiny,
                ScaleFactor::new(1.0).unwrap(),
                0.5
            ),
            Err(LayoutError::Oversized { .. })
        ));
    }

    #[test]
    fn effective_geometry_changes_layout_and_preserves_negative_monitor_bounds() {
        let plain_menu = menu(LayoutKind::CircularCells);
        let plain = layout_menu(
            &plain_menu,
            PhysicalPoint {
                x: -1200.0,
                y: 300.0,
            },
            work(),
            ScaleFactor::new(1.5).unwrap(),
            0.5,
        )
        .unwrap();
        let mut document = RadialDocument::starter();
        document.menus[0].style.values.geometry.item_size = Override::Value(80.0);
        document.menus[0].style.values.geometry.radius_scale = Override::Value(1.25);
        document.menus[0].style.values.geometry.center_size = Override::Value(100.0);
        document.menus[0].style.values.geometry.center_image_scale = Override::Value(0.6);
        document.menus[0].style.values.geometry.outer_ring_margin = Override::Value(10.0);
        document.menus[0].style.values.geometry.outer_rim_width = Override::Value(6.0);
        document.menus[0].style.values.effects.glow_enabled = Override::Value(true);
        document.menus[0].style.values.text.font_size = Override::Value(24.0);
        document.menus[0].style.values.window.fill_item_hit_zones = Override::Value(false);
        document.menus[0].rings[0].cells[0]
            .style
            .geometry
            .item_image_scale = Override::Value(0.4);
        let styled = layout_document_menu(
            &document,
            &document.menus[0],
            PhysicalPoint {
                x: -1200.0,
                y: 300.0,
            },
            work(),
            ScaleFactor::new(1.5).unwrap(),
            0.5,
        )
        .unwrap();
        assert_eq!(styled.style.item_size, 80.0);
        assert_eq!(styled.style.center_size, 100.0);
        assert_eq!(styled.style.center_image_scale, 0.6);
        assert_eq!(styled.cells[0].visual.item_image_scale, 0.4);
        assert!(styled.style.glow_extent > 0.0);
        assert!(!styled.style.fill_item_hit_zones);
        let first_center = match styled.cells[0].shape {
            HitShape::Circle { center, .. } => center,
            _ => unreachable!(),
        };
        assert_eq!(
            styled.hit_test(first_center).map(|cell| &cell.cell_id),
            Some(&styled.cells[0].cell_id)
        );
        assert!(
            styled.rim_extent.max.x - styled.rim_extent.min.x
                > plain.rim_extent.max.x - plain.rim_extent.min.x
        );
        assert!(styled.visual_extent.min.x >= work().min.x as f32 / 1.5);
        assert!(styled.input_extent.max.x <= work().max.x as f32 / 1.5);
    }
}
