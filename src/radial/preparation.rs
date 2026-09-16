//! Shared preparation boundary for runtime and authoring preview frames.

use super::assets::{AssetService, ManagedAssetOverlay, PrepareVariant, reference_identity};
use super::bindings::project_menu_frame_with_style;
use super::diagnostics::{
    MAX_EXPECTED_LAYOUT_DIAGNOSTICS, MAX_RADIAL_DIAGNOSTICS, RadialDiagnostic,
    RadialDiagnosticKind, RadialDiagnosticSeverity, RadialDiagnosticSource, bound_diagnostics,
};
use super::dynamic::{
    FrozenAvailability, FrozenBinding, FrozenDynamicFrame, FrozenEntryId, FrozenEntryKind,
    FrozenRadialEntry, SourceFingerprint,
};
use super::font_cache::{
    FontAlignment, FontLayoutPurpose, FontLayoutService, FontRequest, FontWrapPolicy,
    MAX_LAYOUT_CACHE_ENTRIES, MAX_TOOLTIP_LINES, SystemFontCatalog,
};
use super::geometry::{
    CellLayout, HitShape, LayoutSnapshot, PhysicalPoint, PhysicalRect, ScaleFactor,
    layout_document_menu, layout_document_menu_fixed_center,
};
use super::model::{
    CellId, MenuDefinition, MenuId, Override, RadialDocument, RingId, SkinId, TooltipMode,
};
use super::render::{PreparedSceneResources, VectorScene, build_scene_prepared_selected};
use super::tooltip::{
    MAX_TOOLTIP_HEIGHT_FRACTION, MAX_TOOLTIP_WIDTH_LOGICAL, PreparedTooltip, TooltipPreferences,
};
use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedFrameInput {
    pub layout: LayoutSnapshot,
    pub work_area: super::geometry::PhysicalRect,
    pub tooltip_preferences: TooltipPreferences,
    pub placement: PreparedPlacement,
    pub scene: VectorScene,
    pub resources: PreparedSceneResources,
    pub diagnostics: Vec<RadialDiagnostic>,
    pub page: usize,
    pub page_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreviewProjection {
    pub page: usize,
    pub placement: PreviewPlacement,
    pub dynamic: BTreeMap<CellId, FrozenDynamicFrame>,
    pub selected_skin: Option<SkinId>,
    pub assets: ManagedAssetOverlay,
    pub tooltip_preferences: TooltipPreferences,
}

/// Preview callers explicitly choose whether the supplied point is a flexible
/// root request or an exact frozen center for a child/page restoration.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PreviewPlacement {
    #[default]
    FlexibleRoot,
    FixedCenter,
    /// A local Cascade point with an explicit SameCenter fallback when the
    /// strict local fit cannot preserve that point at the minimum scale.
    Cascade {
        fallback_center: PhysicalPoint,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparedPlacement {
    FlexibleRoot,
    FixedCenter,
    Cascade,
    SameCenterFallback,
}

pub fn synthetic_preview_dynamic(menu: &MenuDefinition) -> BTreeMap<CellId, FrozenDynamicFrame> {
    menu.rings
        .iter()
        .flat_map(|ring| &ring.cells)
        .filter_map(|cell| {
            let super::model::CellContent::Dynamic { source } = &cell.content else {
                return None;
            };
            let entries = (0..24)
                .map(|index| FrozenRadialEntry {
                    id: FrozenEntryId(format!("preview-{index}")),
                    label: format!("Preview item {}", index + 1),
                    kind: FrozenEntryKind::Action,
                    binding: Some(FrozenBinding::Informational),
                    availability: FrozenAvailability::Available,
                    history_query: String::new(),
                    requirement: super::handoff::InteractionRequirement::None,
                })
                .collect();
            Some((
                cell.id.clone(),
                FrozenDynamicFrame {
                    fingerprint: SourceFingerprint {
                        generation: 0,
                        source: format!("preview:{source:?}"),
                        query: None,
                    },
                    entries,
                },
            ))
        })
        .collect()
}

pub(crate) fn scene_resource_fingerprint(layout: &LayoutSnapshot) -> u64 {
    let mut hasher = DefaultHasher::new();
    format!("{:?}", layout.style).hash(&mut hasher);
    for cell in &layout.cells {
        format!("{:?}{:?}", cell.icon, cell.visual).hash(&mut hasher);
    }
    hasher.finish()
}

pub(crate) fn prepare_visual_resources(
    document: &RadialDocument,
    menu: &MenuDefinition,
    layout: &LayoutSnapshot,
    work_area: super::geometry::PhysicalRect,
    tooltip_preferences: TooltipPreferences,
    asset_service: Option<&mut AssetService>,
    font_service: Option<&mut FontLayoutService>,
    overlay: &ManagedAssetOverlay,
) -> (PreparedSceneResources, Vec<RadialDiagnostic>) {
    let mut resources = PreparedSceneResources::default();
    let variant = PrepareVariant {
        effective_style: scene_resource_fingerprint(layout),
        dpi_milli: (layout.scale_factor.get() * 1_000.0)
            .round()
            .clamp(1.0, u32::MAX as f64) as u32,
        logical_width_milli: (layout.style.item_size.max(1.0) * 1_000.0) as u32,
        logical_height_milli: (layout.style.item_size.max(1.0) * 1_000.0) as u32,
        quality: layout.style.image_quality,
    };
    let mut diagnostics = Vec::new();
    if let Some(service) = asset_service {
        let mut refs = vec![
            layout.style.item_glow.clone(),
            layout.style.menu_outer_rim.clone(),
            layout.style.menu_background.clone(),
            layout.style.menu_foreground.clone(),
            layout.style.center_background.clone(),
            layout.style.center_image.clone(),
        ];
        for cell in &layout.cells {
            refs.extend([
                cell.icon.clone(),
                cell.visual.item_background.clone(),
                cell.visual.item_foreground.clone(),
                cell.visual.item_shadow.clone(),
                cell.visual.submenu_indicator.clone(),
            ]);
        }
        for reference in refs {
            if let Override::Value(reference) = reference {
                match service.prepare_with_overlay(
                    &reference,
                    super::model::MediaKind::Image,
                    &document.assets,
                    variant,
                    overlay,
                ) {
                    Ok(snapshot) => {
                        resources
                            .media
                            .insert(reference_identity(&reference), snapshot);
                    }
                    Err(error) => {
                        let identity = reference_identity(&reference);
                        diagnostics.push(RadialDiagnostic::new(
                            RadialDiagnosticSeverity::Error,
                            RadialDiagnosticKind::AssetUnavailable(error.clone()),
                            RadialDiagnosticSource::Asset {
                                menu_id: menu.id.clone(),
                                identity: identity.clone(),
                            },
                            (variant.effective_style, variant.dpi_milli, error.clone()),
                            format!("radial image {identity} unavailable: {error}"),
                        ));
                    }
                }
            }
        }
    }
    if let Some(service) = font_service {
        for cell in &layout.cells {
            // The layout label is the wheel's rendered display string and may
            // contain a frozen availability annotation. Tooltip content must
            // remain the immutable projected/authored label instead. Synthetic
            // cells (for example the center/background controls) have no
            // definition and therefore intentionally stay empty here.
            let definition = menu
                .rings
                .iter()
                .flat_map(|ring| &ring.cells)
                .find(|candidate| candidate.id == cell.cell_id);
            let display_label = cell.label.as_str();
            let tooltip_label = definition
                .map(|definition| definition.label.as_str())
                .unwrap_or_default();
            let label_request = FontRequest {
                family: (!cell.visual.font_family.is_empty())
                    .then(|| cell.visual.font_family.clone()),
                size_milli: (cell.visual.font_size.max(1.0) * 1_000.0) as u32,
                bold: cell.visual.bold,
                italic: cell.visual.italic,
                dpi_milli: variant.dpi_milli,
                max_width_milli: (layout.style.item_size.max(1.0)
                    * cell.visual.text_box_scale
                    * 1_000.0) as u32,
                max_height_milli: 0,
                max_lines: 1,
                purpose: FontLayoutPurpose::CellLabel,
                wrap: FontWrapPolicy::Ellipsis,
                alignment: FontAlignment::Center,
            };
            let prepared_label = service.prepare(display_label, label_request.clone());
            diagnostics.extend(prepared_label.diagnostics.iter().map(|diagnostic| {
                RadialDiagnostic::from_font(
                    &menu.id,
                    &cell.cell_id,
                    display_label,
                    &label_request,
                    diagnostic,
                )
            }));
            resources.text.insert(cell.cell_id.clone(), prepared_label);
            if cell.visual.tooltip_mode != TooltipMode::Disabled {
                let description = definition.and_then(|definition| match &definition.tooltip {
                    Override::Value(value) if !value.trim().is_empty() => {
                        Some(value.trim().to_owned())
                    }
                    _ => None,
                });
                let was_truncated = resources.text.get(&cell.cell_id).is_some_and(|layout| {
                    layout
                        .diagnostics
                        .contains(&super::font_cache::FontDiagnostic::LabelTruncated)
                });
                let show_label = !tooltip_label.trim().is_empty()
                    && tooltip_preferences.label_is_eligible(was_truncated);
                let description =
                    description.filter(|description| description != tooltip_label.trim());
                if show_label || description.is_some() {
                    let full_label: Arc<str> = Arc::from(tooltip_label);
                    let source = match description.as_deref() {
                        Some(description) if show_label => {
                            format!("{}\n{}", full_label, description)
                        }
                        Some(description) => description.to_owned(),
                        None => full_label.to_string(),
                    };
                    let work_min = layout.scale_factor.physical_to_logical(work_area.min);
                    let work_max = layout.scale_factor.physical_to_logical(work_area.max);
                    let max_width = ((work_max.x - work_min.x - 24.0)
                        .max(1.0)
                        .min(MAX_TOOLTIP_WIDTH_LOGICAL)
                        * 1_000.0) as u32;
                    let max_height = ((work_max.y - work_min.y).max(1.0)
                        * MAX_TOOLTIP_HEIGHT_FRACTION
                        * 1_000.0) as u32;
                    let tooltip_request = FontRequest {
                        family: label_request.family.clone(),
                        size_milli: 13_000,
                        bold: false,
                        italic: false,
                        dpi_milli: variant.dpi_milli,
                        max_width_milli: max_width,
                        max_height_milli: max_height,
                        max_lines: MAX_TOOLTIP_LINES as u16,
                        purpose: FontLayoutPurpose::Tooltip,
                        wrap: FontWrapPolicy::Word,
                        alignment: FontAlignment::Left,
                    };
                    let layout = service.prepare(&source, tooltip_request.clone());
                    diagnostics.extend(layout.diagnostics.iter().map(|diagnostic| {
                        RadialDiagnostic::from_font(
                            &menu.id,
                            &cell.cell_id,
                            &source,
                            &tooltip_request,
                            diagnostic,
                        )
                    }));
                    resources.tooltips.insert(
                        cell.cell_id.clone(),
                        Arc::new(PreparedTooltip {
                            full_label,
                            description: description.map(Arc::from),
                            layout,
                            font_size: 13.0,
                            show_label,
                            label_was_truncated: was_truncated,
                        }),
                    );
                }
            }
        }
    }
    (
        resources,
        bound_diagnostics(
            diagnostics,
            MAX_EXPECTED_LAYOUT_DIAGNOSTICS,
            MAX_RADIAL_DIAGNOSTICS,
        ),
    )
}

pub(crate) fn augment_preview_special_cells(layout: &mut LayoutSnapshot, menu: &MenuDefinition) {
    let special_visual = layout
        .cells
        .first()
        .map(|cell| cell.visual.clone())
        .unwrap_or_default();
    if menu.center_action.is_some()
        || menu.center_secondary_action.is_some()
        || menu.center_control.is_some()
        || menu.center_secondary_control.is_some()
    {
        layout.cells.push(CellLayout {
            cell_id: CellId::new("__center"),
            ring_id: RingId::new("__special"),
            label: String::new(),
            icon: Override::Inherit,
            control: menu.center_control,
            secondary_control: menu.center_secondary_control,
            shape: HitShape::Circle {
                center: layout.center,
                radius: layout.center_radius,
            },
            actionable: true,
            visual: special_visual.clone(),
        });
    }
    if menu.background_action.is_some()
        || menu.background_secondary_action.is_some()
        || menu.background_control.is_some()
        || menu.background_secondary_control.is_some()
    {
        let radius = (layout.input_extent.max.x - layout.input_extent.min.x)
            .max(layout.input_extent.max.y - layout.input_extent.min.y)
            * 0.5;
        layout.cells.insert(
            0,
            CellLayout {
                cell_id: CellId::new("__background"),
                ring_id: RingId::new("__special"),
                label: String::new(),
                icon: Override::Inherit,
                control: menu.background_control,
                secondary_control: menu.background_secondary_control,
                shape: HitShape::Circle {
                    center: layout.center,
                    radius,
                },
                actionable: true,
                visual: special_visual,
            },
        );
    }
}

/// Child menus reserve their center hit target for navigation Back even when
/// the authored menu has no center action or control. Runtime native roles
/// decide the child-center semantics; this helper only guarantees geometry.
pub(crate) fn ensure_preview_center_back(layout: &mut LayoutSnapshot, menu: &MenuDefinition) {
    let center_id = CellId::new("__center");
    if let Some(center) = layout
        .cells
        .iter_mut()
        .find(|cell| cell.cell_id == center_id)
    {
        center.actionable = true;
        return;
    }
    let visual = layout
        .cells
        .first()
        .map(|cell| cell.visual.clone())
        .unwrap_or_default();
    layout.cells.push(CellLayout {
        cell_id: center_id,
        ring_id: RingId::new("__special"),
        label: String::new(),
        icon: Override::Inherit,
        control: menu.center_control,
        secondary_control: menu.center_secondary_control,
        shape: HitShape::Circle {
            center: layout.center,
            radius: layout.center_radius,
        },
        actionable: true,
        visual,
    });
}

pub struct PreviewFramePreparer {
    application_data: PathBuf,
    asset_service: Option<AssetService>,
    font_service: Option<FontLayoutService>,
    search_roots: Option<super::model::MediaSearchRoots>,
}

impl PreviewFramePreparer {
    pub fn new(application_data: PathBuf) -> Self {
        Self {
            application_data,
            asset_service: None,
            font_service: None,
            search_roots: None,
        }
    }

    fn ensure_resources(&mut self, document: &RadialDocument) {
        if self.asset_service.is_none() {
            self.asset_service = Some(AssetService::new(
                self.application_data.clone(),
                document.media_search_roots.clone(),
            ));
            self.search_roots = Some(document.media_search_roots.clone());
            self.font_service = Some(FontLayoutService::with_catalog(
                SystemFontCatalog::discover(),
                MAX_LAYOUT_CACHE_ENTRIES,
            ));
        } else if self.search_roots.as_ref() != Some(&document.media_search_roots)
            && let Some(service) = &mut self.asset_service
        {
            service.replace_search_roots(document.media_search_roots.clone());
            self.search_roots = Some(document.media_search_roots.clone());
        }
    }

    pub fn release(&mut self) {
        self.asset_service = None;
        self.font_service = None;
        self.search_roots = None;
    }

    pub fn prepare(
        &mut self,
        document: &RadialDocument,
        menu_id: &MenuId,
        anchor: PhysicalPoint,
        work_area: PhysicalRect,
        scale: ScaleFactor,
        generation: u64,
        selected: Option<&CellId>,
        projection: &PreviewProjection,
    ) -> Result<PreparedFrameInput, String> {
        self.ensure_resources(document);
        let menu = document
            .menus
            .iter()
            .find(|menu| &menu.id == menu_id)
            .ok_or_else(|| format!("preview menu {menu_id} no longer exists"))?;
        let mut base_menu = menu.clone();
        if let Some(skin_id) = &projection.selected_skin {
            if !document.skins.iter().any(|skin| &skin.id == skin_id) {
                return Err(format!("preview skin {skin_id} no longer exists"));
            }
            base_menu.skin_id = skin_id.clone();
        }
        let effective = super::skin::compile_menu_tree(document, &base_menu)
            .map_err(|error| format!("preview style failed: {error:?}"))?;
        let projected = project_menu_frame_with_style(
            &base_menu,
            BTreeMap::new(),
            &projection.dynamic,
            projection.page,
            Some(&effective),
        );
        let menu = &projected.menu;
        let (placement, prepared_placement) = match projection.placement {
            PreviewPlacement::FlexibleRoot => (
                layout_document_menu(document, menu, anchor, work_area, scale, 0.55),
                PreparedPlacement::FlexibleRoot,
            ),
            PreviewPlacement::FixedCenter => (
                layout_document_menu_fixed_center(document, menu, anchor, work_area, scale, 0.55),
                PreparedPlacement::FixedCenter,
            ),
            PreviewPlacement::Cascade { fallback_center } => {
                match layout_document_menu_fixed_center(
                    document, menu, anchor, work_area, scale, 0.55,
                ) {
                    Ok(layout) => (Ok(layout), PreparedPlacement::Cascade),
                    Err(cascade_error) => (
                        layout_document_menu_fixed_center(
                            document,
                            menu,
                            fallback_center,
                            work_area,
                            scale,
                            0.55,
                        )
                        .map_err(|_| cascade_error),
                        PreparedPlacement::SameCenterFallback,
                    ),
                }
            }
        };
        let mut layout = placement.map_err(|error| format!("preview layout failed: {error:?}"))?;
        augment_preview_special_cells(&mut layout, menu);
        let (resources, mut diagnostics) = prepare_visual_resources(
            document,
            menu,
            &layout,
            work_area,
            projection.tooltip_preferences,
            self.asset_service.as_mut(),
            self.font_service.as_mut(),
            &projection.assets,
        );
        let variant = PrepareVariant {
            effective_style: scene_resource_fingerprint(&layout),
            dpi_milli: (layout.scale_factor.get() * 1_000.0)
                .round()
                .clamp(1.0, u32::MAX as f64) as u32,
            logical_width_milli: (layout.style.item_size.max(1.0) * 1_000.0) as u32,
            logical_height_milli: (layout.style.item_size.max(1.0) * 1_000.0) as u32,
            quality: layout.style.image_quality,
        };
        if let Some(service) = self.asset_service.as_mut() {
            for reference in [
                &effective.menu.values.sounds.on_show,
                &effective.menu.values.sounds.on_close,
                &effective.menu.values.sounds.on_select,
                &effective.menu.values.sounds.on_submenu_show,
                &effective.menu.values.sounds.on_submenu_close,
            ] {
                if let Override::Value(reference) = reference
                    && let Err(error) = service.prepare_with_overlay(
                        reference,
                        super::model::MediaKind::Sound,
                        &document.assets,
                        variant,
                        &projection.assets,
                    )
                {
                    let identity = reference_identity(reference);
                    diagnostics.push(RadialDiagnostic::new(
                        RadialDiagnosticSeverity::Error,
                        RadialDiagnosticKind::SoundUnavailable(error.clone()),
                        RadialDiagnosticSource::Asset {
                            menu_id: menu.id.clone(),
                            identity: identity.clone(),
                        },
                        (variant.effective_style, variant.dpi_milli, error.clone()),
                        format!("radial sound {identity} unavailable: {error}"),
                    ));
                }
            }
        }
        diagnostics = bound_diagnostics(
            diagnostics,
            MAX_EXPECTED_LAYOUT_DIAGNOSTICS,
            MAX_RADIAL_DIAGNOSTICS,
        );
        let scene = build_scene_prepared_selected(&layout, generation, &resources, selected);
        Ok(PreparedFrameInput {
            layout,
            work_area,
            tooltip_preferences: projection.tooltip_preferences,
            placement: prepared_placement,
            scene,
            resources,
            diagnostics,
            page: projected.page,
            page_count: projected.page_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::diagnostics::RadialDiagnosticKind;
    use crate::radial::model::{
        AssetId, AssetRecord, CellContent, Control, DynamicSource, MediaKind, MediaReference,
    };
    use image::{DynamicImage, ImageOutputFormat, Rgba, RgbaImage};
    use sha2::{Digest, Sha256};
    use std::collections::BTreeSet;
    use std::io::Cursor;
    use std::sync::Arc;

    fn geometry() -> (PhysicalPoint, PhysicalRect, ScaleFactor) {
        (
            PhysicalPoint { x: 300.0, y: 300.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 600.0, y: 600.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
        )
    }

    #[test]
    fn prepared_tooltip_keeps_complete_label_and_distinct_description_order() {
        let mut document = RadialDocument::starter();
        let cell = &mut document.menus[0].rings[0].cells[0];
        cell.label = "Complete 日本語 label".into();
        cell.tooltip = Override::Value("  Description remains distinct.  ".into());
        let cell_id = cell.id.clone();
        let (anchor, work, scale) = geometry();
        let mut preparer = PreviewFramePreparer::new(PathBuf::new());
        let frame = preparer
            .prepare(
                &document,
                &document.default_menu_id,
                anchor,
                work,
                scale,
                1,
                None,
                &PreviewProjection::default(),
            )
            .unwrap();
        let tooltip = frame.resources.tooltips.get(&cell_id).unwrap();
        assert_eq!(&*tooltip.full_label, "Complete 日本語 label");
        assert_eq!(
            tooltip.description.as_deref(),
            Some("Description remains distinct.")
        );
        assert_eq!(
            tooltip.combined_source(),
            "Complete 日本語 label\nDescription remains distinct."
        );
        assert_eq!(
            &*tooltip.layout.source_text,
            "Complete 日本語 label\nDescription remains distinct."
        );

        let dynamic_document = RadialDocument::starter();
        let dynamic_menu = dynamic_document
            .menus
            .iter()
            .find(|menu| menu.id.as_str() == "starter-applications")
            .unwrap()
            .clone();
        let dynamic_id = dynamic_menu
            .rings
            .iter()
            .flat_map(|ring| &ring.cells)
            .find(|cell| matches!(&cell.content, CellContent::Dynamic { .. }))
            .unwrap()
            .id
            .clone();
        let projection = PreviewProjection {
            dynamic: synthetic_preview_dynamic(&dynamic_menu),
            ..PreviewProjection::default()
        };
        let dynamic = preparer
            .prepare(
                &dynamic_document,
                &dynamic_menu.id,
                anchor,
                work,
                scale,
                2,
                None,
                &projection,
            )
            .unwrap();
        let projected_cell = dynamic
            .layout
            .cells
            .iter()
            .find(|cell| cell.cell_id.as_str().starts_with("dyn:"))
            .unwrap();
        let projected_tooltip = dynamic
            .resources
            .tooltips
            .get(&projected_cell.cell_id)
            .unwrap();
        assert!(
            projected_cell
                .cell_id
                .as_str()
                .starts_with(&format!("dyn:{}:", dynamic_id))
        );
        assert_eq!(
            projected_tooltip.full_label.as_ref(),
            projected_cell.label.as_str()
        );
        assert!(projected_tooltip.description.is_none());

        document.menus[0].rings[0].cells[0].tooltip = Override::Value("  ".into());
        let empty = preparer
            .prepare(
                &document,
                &document.default_menu_id,
                anchor,
                work,
                scale,
                3,
                None,
                &PreviewProjection::default(),
            )
            .unwrap();
        let empty_tooltip = empty.resources.tooltips.get(&cell_id).unwrap();
        assert!(empty_tooltip.description.is_none());
        let label = document.menus[0].rings[0].cells[0].label.clone();
        document.menus[0].rings[0].cells[0].tooltip = Override::Value(label);
        let equal = preparer
            .prepare(
                &document,
                &document.default_menu_id,
                anchor,
                work,
                scale,
                4,
                None,
                &PreviewProjection::default(),
            )
            .unwrap();
        assert!(
            equal
                .resources
                .tooltips
                .get(&cell_id)
                .unwrap()
                .description
                .is_none()
        );
    }

    #[test]
    fn tooltip_full_label_ignores_availability_annotation_on_wheel_label() {
        let document = RadialDocument::starter();
        let (anchor, work, scale) = geometry();
        let mut preparer = PreviewFramePreparer::new(PathBuf::new());
        let frame = preparer
            .prepare(
                &document,
                &document.default_menu_id,
                anchor,
                work,
                scale,
                1,
                None,
                &PreviewProjection::default(),
            )
            .unwrap();
        let cell_id = document.menus[0].rings[0].cells[0].id.clone();
        let authored_label = document.menus[0].rings[0].cells[0].label.clone();
        let mut annotated_layout = frame.layout.clone();
        let wheel_label = format!("{authored_label} [Left: unavailable]");
        annotated_layout
            .cells
            .iter_mut()
            .find(|cell| cell.cell_id == cell_id)
            .unwrap()
            .label = wheel_label.clone();

        let (resources, _) = prepare_visual_resources(
            &document,
            &document.menus[0],
            &annotated_layout,
            work,
            TooltipPreferences::default(),
            None,
            preparer.font_service.as_mut(),
            &ManagedAssetOverlay::default(),
        );
        assert_eq!(
            &*resources.text.get(&cell_id).unwrap().source_text,
            wheel_label
        );
        let tooltip = resources.tooltips.get(&cell_id).unwrap();
        assert_eq!(&*tooltip.full_label, authored_label);
        assert_eq!(&*tooltip.layout.source_text, authored_label);
        assert!(!tooltip.layout.source_text.contains("Left: unavailable"));
    }

    #[test]
    fn empty_display_labels_preserve_source_and_do_not_create_id_tooltips() {
        let mut document = RadialDocument::starter();
        let empty_cell_id = document.menus[0].rings[0].cells[0].id.clone();
        document.menus[0].rings[0].cells[0].label.clear();
        document.menus[0].background_control = Some(Control::Close);
        let (anchor, work, scale) = geometry();
        let mut preparer = PreviewFramePreparer::new(PathBuf::new());
        let frame = preparer
            .prepare(
                &document,
                &document.default_menu_id,
                anchor,
                work,
                scale,
                1,
                None,
                &PreviewProjection::default(),
            )
            .unwrap();

        for cell_id in [
            empty_cell_id,
            CellId::new("__center"),
            CellId::new("__background"),
        ] {
            assert!(
                frame
                    .layout
                    .cells
                    .iter()
                    .find(|cell| cell.cell_id == cell_id)
                    .is_some_and(|cell| cell.label.is_empty())
            );
            let text = frame.resources.text.get(&cell_id).unwrap();
            assert_eq!(&*text.source_text, "");
            assert_eq!(&*text.text, "");
            assert!(!frame.resources.tooltips.contains_key(&cell_id));
        }
    }

    #[test]
    fn production_projection_pages_cover_real_dynamic_entries_without_overlap() {
        let document = RadialDocument::starter();
        let menu = document
            .menus
            .iter()
            .find(|menu| menu.id.as_str() == "starter-applications")
            .unwrap()
            .clone();
        let dynamic = synthetic_preview_dynamic(&menu);
        let expected = dynamic
            .values()
            .map(|frame| frame.entries.len())
            .sum::<usize>();
        let (anchor, work, scale) = geometry();
        let mut preparer = PreviewFramePreparer::new(PathBuf::new());
        let first = preparer
            .prepare(
                &document,
                &menu.id,
                anchor,
                work,
                scale,
                1,
                None,
                &PreviewProjection {
                    dynamic: dynamic.clone(),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(first.page_count > 1);
        let mut all = BTreeSet::new();
        let mut scenes = Vec::new();
        for page in 0..first.page_count {
            let frame = preparer
                .prepare(
                    &document,
                    &menu.id,
                    anchor,
                    work,
                    scale,
                    page as u64 + 2,
                    None,
                    &PreviewProjection {
                        page,
                        dynamic: dynamic.clone(),
                        ..Default::default()
                    },
                )
                .unwrap();
            let page_ids = frame
                .layout
                .cells
                .iter()
                .filter(|cell| cell.cell_id.as_str().starts_with("dyn:"))
                .map(|cell| cell.cell_id.clone())
                .collect::<BTreeSet<_>>();
            assert!(page_ids.iter().all(|id| all.insert(id.clone())));
            scenes.push(frame.scene);
        }
        assert_eq!(all.len(), expected);
        assert!(scenes.windows(2).all(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn explicit_unreferenced_preview_skin_changes_layout_and_scene() {
        let mut document = RadialDocument::starter();
        let mut alternate = document.skins[0].clone();
        alternate.id = SkinId::new("unreferenced-preview-skin");
        alternate.style.values.geometry.center_size = Override::Value(180.0);
        document.skins.push(alternate.clone());
        let menu = document.menus[0].id.clone();
        let (anchor, work, scale) = geometry();
        let mut preparer = PreviewFramePreparer::new(PathBuf::new());
        let ordinary = preparer
            .prepare(
                &document,
                &menu,
                anchor,
                work,
                scale,
                1,
                None,
                &PreviewProjection::default(),
            )
            .unwrap();
        let selected = preparer
            .prepare(
                &document,
                &menu,
                anchor,
                work,
                scale,
                1,
                None,
                &PreviewProjection {
                    selected_skin: Some(alternate.id),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_ne!(ordinary.layout.center_radius, selected.layout.center_radius);
        assert_ne!(ordinary.scene, selected.scene);
    }

    #[test]
    fn cascade_placement_uses_explicit_same_center_fallback_when_local_fit_fails() {
        let mut document = RadialDocument::starter();
        let menu_id = document.default_menu_id.clone();
        document.menus[0].rings[0].radius = 500.0;
        let local_anchor = PhysicalPoint { x: 150.0, y: 500.0 };
        let fallback_center = PhysicalPoint { x: 500.0, y: 500.0 };
        let work_area = PhysicalRect {
            min: PhysicalPoint { x: 0.0, y: 0.0 },
            max: PhysicalPoint {
                x: 1_000.0,
                y: 1_000.0,
            },
        };
        let scale = ScaleFactor::new(1.0).unwrap();
        let mut preparer = PreviewFramePreparer::new(PathBuf::new());

        let frame = preparer
            .prepare(
                &document,
                &menu_id,
                local_anchor,
                work_area,
                scale,
                1,
                None,
                &PreviewProjection {
                    placement: PreviewPlacement::Cascade { fallback_center },
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(frame.placement, PreparedPlacement::SameCenterFallback);
        assert_eq!(frame.layout.origin, fallback_center);
    }

    #[test]
    fn unsaved_image_gif_sound_and_font_draft_inputs_prepare_without_disk_writes() {
        let root = tempfile::tempdir().unwrap();
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba([20, 40, 60, 255])))
            .write_to(&mut png, ImageOutputFormat::Png)
            .unwrap();
        let png = png.into_inner();
        let mut gif = Vec::new();
        image::codecs::gif::GifEncoder::new(&mut gif)
            .encode_frames((0..2).map(|index| {
                image::Frame::from_parts(
                    RgbaImage::from_pixel(1, 1, Rgba([index, 2, 3, 255])),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(10, 1),
                )
            }))
            .unwrap();
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&44_u32.to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
        wav.extend_from_slice(&8_000_u32.to_le_bytes());
        wav.extend_from_slice(&8_000_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&8_u16.to_le_bytes());
        wav.extend_from_slice(b"data\x08\0\0\0\0\0\0\0\0\0\0\0");
        let sources = [
            ("draft-png", MediaKind::Image, "draft.png", png),
            ("draft-gif", MediaKind::Image, "draft.gif", gif),
            ("draft-sound", MediaKind::Sound, "draft.wav", wav),
        ];
        let entries = sources
            .into_iter()
            .map(|(id, kind, path, bytes)| {
                let record = AssetRecord {
                    id: AssetId::new(id),
                    kind,
                    relative_path: path.into(),
                    content_sha256: hex::encode(Sha256::digest(&bytes)),
                    byte_len: bytes.len() as u64,
                };
                (record, Arc::<[u8]>::from(bytes))
            })
            .collect::<Vec<_>>();
        let mut document = RadialDocument::starter();
        document.assets = entries.iter().map(|(record, _)| record.clone()).collect();
        document.menus[0].rings[0].cells[0].icon = Override::Value(MediaReference::Managed {
            asset_id: AssetId::new("draft-png"),
        });
        document.menus[0].rings[0].cells[1].icon = Override::Value(MediaReference::Managed {
            asset_id: AssetId::new("draft-gif"),
        });
        document.menus[0].style.values.sounds.on_show = Override::Value(MediaReference::Managed {
            asset_id: AssetId::new("draft-sound"),
        });
        document.menus[0].style.values.text.font_family =
            Override::Value("Unsaved Preview Font".into());
        let overlay = ManagedAssetOverlay::validated(entries).unwrap();
        let (anchor, work, scale) = geometry();
        let mut preparer = PreviewFramePreparer::new(root.path().to_path_buf());
        let frame = preparer
            .prepare(
                &document,
                &document.default_menu_id,
                anchor,
                work,
                scale,
                5,
                None,
                &PreviewProjection {
                    assets: overlay,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(frame.resources.media.len(), 2);
        assert!(frame.resources.media.values().any(|snapshot| matches!(
            &*snapshot.media,
            super::super::assets::PreparedMedia::Image(image) if image.animated
        )));
        assert!(frame.resources.text.len() == frame.layout.cells.len());
        assert!(!frame.diagnostics.iter().any(|diagnostic| {
            matches!(&diagnostic.kind, RadialDiagnosticKind::SoundUnavailable(_))
                && diagnostic.message.contains("radial sound")
        }));
        assert!(
            !root
                .path()
                .join(super::super::model::RADIAL_ASSETS_DIRECTORY)
                .exists()
        );
    }
}
