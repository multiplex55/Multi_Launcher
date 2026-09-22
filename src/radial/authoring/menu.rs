//! Pure, stable-ID menu graph edits used by the egui authoring surface.

use std::collections::{BTreeMap, BTreeSet};

use super::{DraftGeneration, RadialAuthoringSession, StableSelection};
use crate::radial::model::{
    AfterActionPolicy, CellContent, CellDefinition, CellId, InteractionMode, MenuDefinition,
    MenuId, RadialDocument, RingDefinition, RingId, SubmenuPresentation,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmenuDuplication {
    LinkExisting,
    CloneSubmenuClosure,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeleteImpact {
    pub submenu_cells: Vec<(MenuId, RingId, CellId)>,
    pub context_rules: usize,
    pub custom_triggers: usize,
    pub is_default: bool,
}

impl DeleteImpact {
    pub fn is_referenced(&self) -> bool {
        self.is_default
            || self.context_rules != 0
            || self.custom_triggers != 0
            || !self.submenu_cells.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResizePlan {
    pub menu_id: MenuId,
    pub ring_id: RingId,
    pub requested_len: usize,
    pub retained: Vec<CellDefinition>,
    pub removed: Vec<CellDefinition>,
    pub populated_removed: Vec<CellId>,
}

/// A validated, side-effect-free candidate for one ring authoring operation.
///
/// The complete document is retained so geometry and cell changes cross the
/// authoring boundary as one undo entry. `base_generation` prevents a preview
/// from being applied after any unrelated draft edit.
#[derive(Clone, Debug, PartialEq)]
pub struct RingEditProposal {
    pub menu_id: MenuId,
    pub ring_id: RingId,
    pub requested_len: usize,
    pub base_generation: DraftGeneration,
    pub document: RadialDocument,
    pub previous_radius: Option<f32>,
    pub proposed_radius: f32,
    pub resolution_summary: Option<String>,
}

impl RingEditProposal {
    pub fn radius_changed(&self) -> bool {
        self.previous_radius
            .is_none_or(|previous| (previous - self.proposed_radius).abs() > 0.001)
    }
}

impl ResizePlan {
    pub fn requires_resolution(&self) -> bool {
        !self.populated_removed.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResizeResolution {
    Relocate { menu_id: MenuId, ring_id: RingId },
    OverflowRing,
    ConfirmDiscard,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEditError {
    MissingEntity,
    DuplicateId,
    LastMenu,
    LastRing,
    Referenced(DeleteImpact),
    SubmenuCycle,
    ResolutionRequired,
    InvalidDestination,
    DestinationOccupied,
    DynamicCell,
    StaleGeneration,
    LimitExceeded(&'static str),
    InvalidGeometry(String),
}

/// Resolution chosen by the user after a drag reaches an authored slot.
///
/// A move into a spacer exchanges the two stable cell identities.  An
/// occupied destination is never overwritten implicitly: callers must choose
/// [`Self::Swap`] explicitly or cancel the gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellDropResolution {
    MoveIntoSpacer,
    Swap,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StableWidgetKey {
    pub entity_kind: String,
    pub entity_id: String,
    pub field: String,
}

pub fn widget_key(entity_kind: &str, entity_id: &str, field: &str) -> StableWidgetKey {
    StableWidgetKey {
        entity_kind: entity_kind.to_owned(),
        entity_id: entity_id.to_owned(),
        field: field.to_owned(),
    }
}

pub fn delete_impact(document: &RadialDocument, id: &MenuId) -> DeleteImpact {
    let mut impact = DeleteImpact {
        context_rules: document
            .context_rules
            .iter()
            .filter(|rule| &rule.menu_id == id)
            .count(),
        custom_triggers: document
            .custom_triggers
            .iter()
            .filter(|trigger| &trigger.menu_id == id)
            .count(),
        is_default: &document.default_menu_id == id,
        ..Default::default()
    };
    for menu in &document.menus {
        for ring in &menu.rings {
            for cell in &ring.cells {
                if matches!(&cell.content, CellContent::Submenu { menu_id } if menu_id == id) {
                    impact
                        .submenu_cells
                        .push((menu.id.clone(), ring.id.clone(), cell.id.clone()));
                }
            }
        }
    }
    impact
}

pub fn create_menu(
    session: &mut RadialAuthoringSession,
    requested_id: &str,
    name: &str,
) -> Result<MenuId, MenuEditError> {
    create_menu_with_defaults(
        session,
        requested_id,
        name,
        crate::radial::model::InteractionMode::StickyClick,
        crate::radial::model::SubmenuPresentation::SameCenter,
    )
}

pub fn create_menu_with_defaults(
    session: &mut RadialAuthoringSession,
    requested_id: &str,
    name: &str,
    interaction: crate::radial::model::InteractionMode,
    submenu_presentation: crate::radial::model::SubmenuPresentation,
) -> Result<MenuId, MenuEditError> {
    let id = session.allocate_menu_id(requested_id);
    let mut document = (*session.draft).clone();
    let skin_id = document
        .skins
        .first()
        .map(|skin| skin.id.clone())
        .ok_or(MenuEditError::MissingEntity)?;
    let menu = MenuDefinition {
        id: id.clone(),
        name: name.to_owned(),
        layout: crate::radial::model::LayoutKind::CircularCells,
        interaction,
        hover_dwell_ms: None,
        submenu_presentation,
        after_action: crate::radial::model::AfterActionPolicy::Inherit,
        center_action: None,
        center_primary_after_action: crate::radial::model::AfterActionPolicy::Inherit,
        center_secondary_action: None,
        center_secondary_after_action: crate::radial::model::AfterActionPolicy::Inherit,
        center_control: None,
        center_secondary_control: None,
        background_action: None,
        background_primary_after_action: crate::radial::model::AfterActionPolicy::Inherit,
        background_secondary_action: None,
        background_control: None,
        background_secondary_control: None,
        background_secondary_after_action: crate::radial::model::AfterActionPolicy::Inherit,
        mirror_primary_to_secondary: false,
        skin_id,
        center_radius: 30.0,
        rings: vec![RingDefinition {
            id: RingId::new("ring-0"),
            radius: 92.0,
            cell_radius: 28.0,
            rotation_degrees: -90.0,
            gap: 4.0,
            cells: (0..8)
                .map(|cell_index| CellDefinition {
                    id: CellId::new(format!("cell-{cell_index}")),
                    label: "Spacer".into(),
                    content: CellContent::Spacer,
                    alternate_clicks: Vec::new(),
                    alternate_controls: Vec::new(),
                    after_action: crate::radial::model::AfterActionPolicy::Inherit,
                    secondary_after_action: crate::radial::model::AfterActionPolicy::Inherit,
                    icon: Default::default(),
                    tooltip: Default::default(),
                    style: Default::default(),
                    shortcuts: Vec::new(),
                    hotstrings: Vec::new(),
                })
                .collect(),
            style: Default::default(),
        }],
        style: Default::default(),
    };
    document.menus.push(menu);
    session
        .replace_document_atomic(document)
        .map_err(|error| MenuEditError::InvalidGeometry(format!("{error:?}")))?;
    session.select(Some(StableSelection::Menu(id.clone())));
    Ok(id)
}

pub fn rename_menu(
    session: &mut RadialAuthoringSession,
    id: MenuId,
    name: String,
    phase: super::EditPhase,
) -> Result<(), MenuEditError> {
    session
        .mutate(
            super::DocumentMutation::RenameMenu {
                id: id.clone(),
                name,
            },
            Some(super::EditKey {
                entity: format!("menu:{}", id.as_str()),
                field: "name".into(),
            }),
            phase,
        )
        .map_err(|_| MenuEditError::MissingEntity)
}

pub fn set_cell_content(
    session: &mut RadialAuthoringSession,
    menu_id: &MenuId,
    ring_id: &RingId,
    cell_id: &CellId,
    content: CellContent,
) -> Result<(), MenuEditError> {
    let mut document = (*session.draft).clone();
    if let CellContent::Submenu { menu_id: child_id } = &content {
        if !document.menus.iter().any(|menu| &menu.id == child_id) {
            return Err(MenuEditError::MissingEntity);
        }
        if would_create_submenu_cycle(&document, menu_id, child_id) {
            return Err(MenuEditError::SubmenuCycle);
        }
    }
    let cell = find_ring_mut(&mut document, menu_id, ring_id)?
        .cells
        .iter_mut()
        .find(|cell| &cell.id == cell_id)
        .ok_or(MenuEditError::MissingEntity)?;
    cell.content = content;
    session
        .replace_document_atomic(document)
        .map_err(|_| MenuEditError::MissingEntity)
}

pub fn delete_menu(session: &mut RadialAuthoringSession, id: &MenuId) -> Result<(), MenuEditError> {
    if session.draft.menus.len() <= 1 {
        return Err(MenuEditError::LastMenu);
    }
    let impact = delete_impact(&session.draft, id);
    if impact.is_referenced() {
        return Err(MenuEditError::Referenced(impact));
    }
    let mut document = (*session.draft).clone();
    document
        .menus
        .iter()
        .position(|menu| &menu.id == id)
        .ok_or(MenuEditError::MissingEntity)?;
    let before = document.menus.len();
    document.menus.retain(|menu| &menu.id != id);
    if document.menus.len() == before {
        return Err(MenuEditError::MissingEntity);
    }
    session
        .replace_document_atomic(document)
        .map_err(|error| MenuEditError::InvalidGeometry(format!("{error:?}")))?;
    let fallback = session.draft.default_menu_id.clone();
    session.select(Some(StableSelection::Menu(fallback)));
    Ok(())
}

pub fn move_menu(
    session: &mut RadialAuthoringSession,
    id: &MenuId,
    destination_index: usize,
) -> Result<(), MenuEditError> {
    let mut document = (*session.draft).clone();
    let source = document
        .menus
        .iter()
        .position(|menu| &menu.id == id)
        .ok_or(MenuEditError::MissingEntity)?;
    let menu = document.menus.remove(source);
    let destination = destination_index.min(document.menus.len());
    document.menus.insert(destination, menu);
    session
        .replace_document_atomic(document)
        .map_err(|error| MenuEditError::InvalidGeometry(format!("{error:?}")))?;
    session.select(Some(StableSelection::Menu(id.clone())));
    Ok(())
}

pub fn add_ring(
    session: &mut RadialAuthoringSession,
    menu_id: &MenuId,
) -> Result<RingId, MenuEditError> {
    let id = session.allocate_ring_id("ring");
    let cell_ids = (0..8)
        .map(|cell_index| session.allocate_cell_id(&format!("cell-{cell_index}")))
        .collect();
    let proposal = propose_new_ring(
        &session.draft,
        session.generation,
        menu_id,
        id.clone(),
        cell_ids,
    )?;
    apply_ring_proposal(session, proposal)?;
    Ok(id)
}

/// Build a new outer ring without modifying the authoring session.
pub fn propose_new_ring(
    document: &RadialDocument,
    base_generation: DraftGeneration,
    menu_id: &MenuId,
    ring_id: RingId,
    cell_ids: Vec<CellId>,
) -> Result<RingEditProposal, MenuEditError> {
    if cell_ids.len() > crate::radial::model::limits::MAX_CELLS_PER_RING {
        return Err(MenuEditError::LimitExceeded("slots per ring"));
    }
    let mut candidate = document.clone();
    let menu = candidate
        .menus
        .iter_mut()
        .find(|menu| &menu.id == menu_id)
        .ok_or(MenuEditError::MissingEntity)?;
    if menu.rings.len() >= crate::radial::model::limits::MAX_RINGS_PER_MENU {
        return Err(MenuEditError::LimitExceeded("rings per menu"));
    }
    let mut ring = RingDefinition {
        id: ring_id.clone(),
        radius: 92.0,
        cell_radius: 28.0,
        rotation_degrees: -90.0,
        gap: 4.0,
        cells: cell_ids.into_iter().map(spacer_cell).collect(),
        style: Default::default(),
    };
    ring.radius = proposed_outer_radius(document, menu_id, &ring)?;
    let proposed_radius = ring.radius;
    menu.rings.push(ring);
    validate_candidate(&candidate)?;
    Ok(RingEditProposal {
        menu_id: menu_id.clone(),
        ring_id,
        requested_len: candidate
            .menus
            .iter()
            .find(|menu| &menu.id == menu_id)
            .and_then(|menu| menu.rings.last())
            .map_or(0, |ring| ring.cells.len()),
        base_generation,
        document: candidate,
        previous_radius: None,
        proposed_radius,
        resolution_summary: None,
    })
}

/// Build a slot-count and geometry candidate without modifying the session.
/// Populated shrink victims are deliberately rejected here and must continue
/// through the explicit [`ResizePlan`] resolution flow.
pub fn propose_ring_resize(
    document: &RadialDocument,
    base_generation: DraftGeneration,
    menu_id: &MenuId,
    ring_id: &RingId,
    requested_len: usize,
    new_cell_ids: Vec<CellId>,
) -> Result<RingEditProposal, MenuEditError> {
    if requested_len > crate::radial::model::limits::MAX_CELLS_PER_RING {
        return Err(MenuEditError::LimitExceeded("slots per ring"));
    }
    let plan = resize_plan(document, menu_id, ring_id, requested_len)?;
    if plan.requires_resolution() {
        return Err(MenuEditError::ResolutionRequired);
    }
    let current = find_ring(document, menu_id, ring_id)?;
    let growth = requested_len.saturating_sub(current.cells.len());
    if new_cell_ids.len() < growth {
        return Err(MenuEditError::InvalidDestination);
    }
    let previous_radius = current.radius;
    let mut candidate = document.clone();
    let ring = find_ring_mut(&mut candidate, menu_id, ring_id)?;
    ring.cells.truncate(requested_len);
    ring.cells
        .extend(new_cell_ids.into_iter().take(growth).map(spacer_cell));
    if crate::radial::validation::validate(&candidate).is_err() {
        let proposed = find_ring(&candidate, menu_id, ring_id)?.clone();
        let radius = proposed_outer_radius_excluding(document, menu_id, &proposed, Some(ring_id))?;
        find_ring_mut(&mut candidate, menu_id, ring_id)?.radius = radius;
    }
    let proposed_radius = find_ring(&candidate, menu_id, ring_id)?.radius;
    validate_candidate(&candidate)?;
    Ok(RingEditProposal {
        menu_id: menu_id.clone(),
        ring_id: ring_id.clone(),
        requested_len,
        base_generation,
        document: candidate,
        previous_radius: Some(previous_radius),
        proposed_radius,
        resolution_summary: None,
    })
}

pub fn propose_resolved_resize(
    document: &RadialDocument,
    base_generation: DraftGeneration,
    plan: &ResizePlan,
    resolution: ResizeResolution,
    overflow_ring_id: Option<RingId>,
) -> Result<RingEditProposal, MenuEditError> {
    let current = find_ring(document, &plan.menu_id, &plan.ring_id)?;
    let expected = plan
        .retained
        .iter()
        .chain(plan.removed.iter())
        .collect::<Vec<_>>();
    if current.cells.iter().collect::<Vec<_>>() != expected {
        return Err(MenuEditError::StaleGeneration);
    }
    let previous_radius = current.radius;
    let mut candidate = document.clone();
    find_ring_mut(&mut candidate, &plan.menu_id, &plan.ring_id)?.cells = plan.retained.clone();
    let resolution_summary = match resolution {
        ResizeResolution::Relocate { menu_id, ring_id } => {
            find_ring_mut(&mut candidate, &menu_id, &ring_id)?
                .cells
                .extend(plan.removed.clone());
            format!(
                "Relocate {} removed slot(s), including {} populated slot(s), to ring {}",
                plan.removed.len(),
                plan.populated_removed.len(),
                ring_id
            )
        }
        ResizeResolution::OverflowRing => {
            let id = overflow_ring_id.ok_or(MenuEditError::InvalidDestination)?;
            let mut overflow = RingDefinition {
                id,
                radius: 92.0,
                cell_radius: 28.0,
                rotation_degrees: -90.0,
                gap: 4.0,
                cells: plan.removed.clone(),
                style: Default::default(),
            };
            overflow.radius = proposed_outer_radius(&candidate, &plan.menu_id, &overflow)?;
            candidate
                .menus
                .iter_mut()
                .find(|menu| menu.id == plan.menu_id)
                .ok_or(MenuEditError::MissingEntity)?
                .rings
                .push(overflow);
            let overflow = candidate
                .menus
                .iter()
                .find(|menu| menu.id == plan.menu_id)
                .and_then(|menu| menu.rings.last())
                .ok_or(MenuEditError::MissingEntity)?;
            format!(
                "Move {} removed slot(s), including {} populated slot(s), to new overflow ring {} at radius {:.1}",
                plan.removed.len(),
                plan.populated_removed.len(),
                overflow.id,
                overflow.radius
            )
        }
        ResizeResolution::ConfirmDiscard => format!(
            "Discard {} populated slot(s): {}",
            plan.populated_removed.len(),
            plan.populated_removed
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    validate_candidate(&candidate)?;
    Ok(RingEditProposal {
        menu_id: plan.menu_id.clone(),
        ring_id: plan.ring_id.clone(),
        requested_len: plan.requested_len,
        base_generation,
        document: candidate,
        previous_radius: Some(previous_radius),
        proposed_radius: previous_radius,
        resolution_summary: Some(resolution_summary),
    })
}

pub fn apply_ring_proposal(
    session: &mut RadialAuthoringSession,
    proposal: RingEditProposal,
) -> Result<(), MenuEditError> {
    if session.generation != proposal.base_generation {
        return Err(MenuEditError::StaleGeneration);
    }
    validate_candidate(&proposal.document)?;
    let selection = StableSelection::Ring {
        menu_id: proposal.menu_id.clone(),
        ring_id: proposal.ring_id.clone(),
    };
    session
        .replace_document_atomic(proposal.document)
        .map_err(|error| MenuEditError::InvalidGeometry(format!("{error:?}")))?;
    session.select(Some(selection));
    Ok(())
}

fn spacer_cell(id: CellId) -> CellDefinition {
    CellDefinition {
        id,
        label: "Spacer".into(),
        content: CellContent::Spacer,
        alternate_clicks: Vec::new(),
        alternate_controls: Vec::new(),
        after_action: AfterActionPolicy::Inherit,
        secondary_after_action: AfterActionPolicy::Inherit,
        icon: Default::default(),
        tooltip: Default::default(),
        style: Default::default(),
        shortcuts: Vec::new(),
        hotstrings: Vec::new(),
    }
}

fn validate_candidate(document: &RadialDocument) -> Result<(), MenuEditError> {
    crate::radial::validation::validate(document)
        .map_err(|errors| MenuEditError::InvalidGeometry(errors.to_string()))
}

fn proposed_outer_radius(
    document: &RadialDocument,
    menu_id: &MenuId,
    ring: &RingDefinition,
) -> Result<f32, MenuEditError> {
    proposed_outer_radius_excluding(document, menu_id, ring, None)
}

fn proposed_outer_radius_excluding(
    document: &RadialDocument,
    menu_id: &MenuId,
    ring: &RingDefinition,
    exclude: Option<&RingId>,
) -> Result<f32, MenuEditError> {
    let menu = document
        .menus
        .iter()
        .find(|menu| &menu.id == menu_id)
        .ok_or(MenuEditError::MissingEntity)?;
    let mut radius = menu.center_radius + ring.cell_radius + 1.0;
    if menu.layout == crate::radial::model::LayoutKind::CircularCells && ring.cells.len() >= 2 {
        let sine = (std::f32::consts::PI / ring.cells.len() as f32).sin();
        radius = radius.max((2.0 * ring.cell_radius + ring.gap) / (2.0 * sine));
    }
    for other in &menu.rings {
        if exclude.is_some_and(|excluded| &other.id == excluded) {
            continue;
        }
        radius = radius.max(
            other.radius + other.cell_radius + ring.cell_radius + other.gap.max(ring.gap) + 1.0,
        );
    }

    // Resolve the effective style once and derive its stricter center,
    // capacity, and inter-ring bounds. The production validator below remains
    // authoritative; this calculation only chooses a useful candidate.
    let mut probe = document.clone();
    {
        let probe_menu = probe
            .menus
            .iter_mut()
            .find(|menu| &menu.id == menu_id)
            .ok_or(MenuEditError::MissingEntity)?;
        probe_menu
            .rings
            .retain(|existing| !exclude.is_some_and(|excluded| &existing.id == excluded));
        probe_menu.rings.push(ring.clone());
    }
    let probe_menu = probe
        .menus
        .iter()
        .find(|menu| &menu.id == menu_id)
        .ok_or(MenuEditError::MissingEntity)?;
    if let Ok(style) = crate::radial::skin::compile_menu_tree(&probe, probe_menu)
        && let Some(ring_style) = style.rings.get(&ring.id)
    {
        let menu_scale = crate::radial::skin::resolved_f32(&ring_style.values.geometry.menu_scale);
        let radius_scale =
            crate::radial::skin::resolved_f32(&ring_style.values.geometry.radius_scale);
        let denominator = (menu_scale * radius_scale).max(f32::EPSILON);
        let item_radius = if style.menu.source(crate::radial::skin::StyleField::ItemSize)
            != Some(&crate::radial::skin::StyleSource::ApplicationFallback)
        {
            crate::radial::skin::resolved_f32(&ring_style.values.geometry.item_size)
                * menu_scale
                * 0.5
        } else {
            ring.cell_radius * menu_scale
        };
        let center_radius = if style
            .menu
            .source(crate::radial::skin::StyleField::CenterSize)
            != Some(&crate::radial::skin::StyleSource::ApplicationFallback)
        {
            crate::radial::skin::resolved_f32(&ring_style.values.geometry.center_size)
                * menu_scale
                * 0.5
        } else {
            menu.center_radius * menu_scale
        };
        radius = radius.max((center_radius + item_radius + 1.0) / denominator);
        if menu.layout == crate::radial::model::LayoutKind::CircularCells && ring.cells.len() >= 2 {
            let sine = (std::f32::consts::PI / ring.cells.len() as f32).sin();
            radius = radius.max(
                (2.0 * item_radius + ring.gap * menu_scale + 1.0) / (2.0 * sine * denominator),
            );
        }
        let inter_menu_scale =
            crate::radial::skin::resolved_f32(&style.menu.values.geometry.menu_scale);
        let menu_radius_scale =
            crate::radial::skin::resolved_f32(&style.menu.values.geometry.radius_scale);
        let menu_denominator = (inter_menu_scale * menu_radius_scale).max(f32::EPSILON);
        let common_item_radius = if style.menu.source(crate::radial::skin::StyleField::ItemSize)
            != Some(&crate::radial::skin::StyleSource::ApplicationFallback)
        {
            crate::radial::skin::resolved_f32(&style.menu.values.geometry.item_size)
                * inter_menu_scale
                * 0.5
        } else {
            ring.cell_radius * inter_menu_scale
        };
        for other in &menu.rings {
            if exclude.is_some_and(|excluded| &other.id == excluded) {
                continue;
            }
            let other_item_radius = if style.menu.source(crate::radial::skin::StyleField::ItemSize)
                != Some(&crate::radial::skin::StyleSource::ApplicationFallback)
            {
                common_item_radius
            } else {
                other.cell_radius * inter_menu_scale
            };
            let required = common_item_radius
                + other_item_radius
                + other.gap.max(ring.gap) * inter_menu_scale
                + 1.0;
            radius = radius.max(other.radius + required / menu_denominator);
        }
    }
    if !radius.is_finite() || radius > 8192.0 {
        return Err(MenuEditError::InvalidGeometry(
            "No valid radius fits the current layout and effective style".into(),
        ));
    }
    Ok(radius)
}

pub fn delete_ring(
    session: &mut RadialAuthoringSession,
    menu_id: &MenuId,
    ring_id: &RingId,
) -> Result<(), MenuEditError> {
    let mut document = (*session.draft).clone();
    let menu = document
        .menus
        .iter_mut()
        .find(|menu| &menu.id == menu_id)
        .ok_or(MenuEditError::MissingEntity)?;
    if menu.rings.len() <= 1 {
        return Err(MenuEditError::LastRing);
    }
    let before = menu.rings.len();
    menu.rings.retain(|ring| &ring.id != ring_id);
    if before == menu.rings.len() {
        return Err(MenuEditError::MissingEntity);
    }
    session
        .replace_document_atomic(document)
        .map_err(|_| MenuEditError::MissingEntity)?;
    session.select(Some(StableSelection::Menu(menu_id.clone())));
    Ok(())
}

pub fn move_ring(
    session: &mut RadialAuthoringSession,
    menu_id: &MenuId,
    ring_id: &RingId,
    destination_index: usize,
) -> Result<(), MenuEditError> {
    let mut document = (*session.draft).clone();
    let menu = document
        .menus
        .iter_mut()
        .find(|menu| &menu.id == menu_id)
        .ok_or(MenuEditError::MissingEntity)?;
    let source = menu
        .rings
        .iter()
        .position(|ring| &ring.id == ring_id)
        .ok_or(MenuEditError::MissingEntity)?;
    let ring = menu.rings.remove(source);
    let destination = destination_index.min(menu.rings.len());
    menu.rings.insert(destination, ring);
    session
        .replace_document_atomic(document)
        .map_err(|_| MenuEditError::MissingEntity)?;
    session.select(Some(StableSelection::Ring {
        menu_id: menu_id.clone(),
        ring_id: ring_id.clone(),
    }));
    Ok(())
}

pub fn set_cell_label(
    session: &mut RadialAuthoringSession,
    menu_id: &MenuId,
    ring_id: &RingId,
    cell_id: &CellId,
    label: String,
    phase: super::EditPhase,
) -> Result<(), MenuEditError> {
    let mut document = (*session.draft).clone();
    let cell = find_ring_mut(&mut document, menu_id, ring_id)?
        .cells
        .iter_mut()
        .find(|cell| &cell.id == cell_id)
        .ok_or(MenuEditError::MissingEntity)?;
    cell.label = label;
    session
        .replace_document_edit(
            document,
            super::EditKey {
                entity: format!("cell:{menu_id}/{ring_id}/{cell_id}"),
                field: "label".into(),
            },
            phase,
        )
        .map_err(|_| MenuEditError::MissingEntity)
}

pub fn delete_cell(
    session: &mut RadialAuthoringSession,
    menu_id: &MenuId,
    ring_id: &RingId,
    cell_id: &CellId,
) -> Result<(), MenuEditError> {
    let mut document = (*session.draft).clone();
    let ring = find_ring_mut(&mut document, menu_id, ring_id)?;
    let before = ring.cells.len();
    ring.cells.retain(|cell| &cell.id != cell_id);
    if before == ring.cells.len() {
        return Err(MenuEditError::MissingEntity);
    }
    session
        .replace_document_atomic(document)
        .map_err(|_| MenuEditError::MissingEntity)?;
    session.select(Some(StableSelection::Ring {
        menu_id: menu_id.clone(),
        ring_id: ring_id.clone(),
    }));
    Ok(())
}

pub fn add_spacer(
    session: &mut RadialAuthoringSession,
    menu_id: &MenuId,
    ring_id: &RingId,
) -> Result<CellId, MenuEditError> {
    let id = session.allocate_cell_id("cell");
    let mut document = (*session.draft).clone();
    let ring = find_ring_mut(&mut document, menu_id, ring_id)?;
    ring.cells.push(CellDefinition {
        id: id.clone(),
        label: "Spacer".into(),
        content: CellContent::Spacer,
        alternate_clicks: Vec::new(),
        alternate_controls: Vec::new(),
        after_action: Default::default(),
        secondary_after_action: Default::default(),
        icon: Default::default(),
        tooltip: Default::default(),
        style: Default::default(),
        shortcuts: Vec::new(),
        hotstrings: Vec::new(),
    });
    session
        .replace_document_atomic(document)
        .map_err(|error| MenuEditError::InvalidGeometry(format!("{error:?}")))?;
    session.select(Some(StableSelection::Cell {
        menu_id: menu_id.clone(),
        ring_id: ring_id.clone(),
        cell_id: id.clone(),
    }));
    Ok(id)
}

pub fn move_cell(
    session: &mut RadialAuthoringSession,
    source: (&MenuId, &RingId, &CellId),
    destination: (&MenuId, &RingId, usize),
) -> Result<(), MenuEditError> {
    let expected_generation = session.generation;
    move_cell_to_slot(
        session,
        source,
        destination,
        CellDropResolution::MoveIntoSpacer,
        expected_generation,
    )
}

/// Apply one stable-ID slot operation after a drag has completed.
///
/// `expected_generation` is captured at drag start.  Any intervening edit
/// invalidates the operation rather than applying a stale borrowed pointer or
/// silently moving a newer version of the document.
pub fn move_cell_to_slot(
    session: &mut RadialAuthoringSession,
    source: (&MenuId, &RingId, &CellId),
    destination: (&MenuId, &RingId, usize),
    resolution: CellDropResolution,
    expected_generation: super::DraftGeneration,
) -> Result<(), MenuEditError> {
    if resolution == CellDropResolution::Cancel {
        return Ok(());
    }
    if session.generation != expected_generation {
        return Err(MenuEditError::StaleGeneration);
    }
    let source_ring = find_ring(&session.draft, source.0, source.1)?;
    let source_index = source_ring
        .cells
        .iter()
        .position(|cell| &cell.id == source.2)
        .ok_or(MenuEditError::MissingEntity)?;
    let destination_ring = find_ring(&session.draft, destination.0, destination.1)?;
    let destination_cell = destination_ring
        .cells
        .get(destination.2)
        .ok_or(MenuEditError::InvalidDestination)?;
    if source.0 == destination.0 && source.1 == destination.1 && source_index == destination.2 {
        return Ok(());
    }
    let destination_is_spacer = matches!(&destination_cell.content, CellContent::Spacer);
    if !destination_is_spacer && resolution != CellDropResolution::Swap {
        return Err(MenuEditError::DestinationOccupied);
    }

    let mut document = (*session.draft).clone();
    if source.0 == destination.0 && source.1 == destination.1 {
        let ring = find_ring_mut(&mut document, source.0, source.1)?;
        ring.cells.swap(source_index, destination.2);
    } else {
        let source_cell = find_ring(&document, source.0, source.1)?
            .cells
            .get(source_index)
            .cloned()
            .ok_or(MenuEditError::MissingEntity)?;
        let destination_cell = find_ring(&document, destination.0, destination.1)?
            .cells
            .get(destination.2)
            .cloned()
            .ok_or(MenuEditError::InvalidDestination)?;
        find_ring_mut(&mut document, source.0, source.1)?.cells[source_index] = destination_cell;
        find_ring_mut(&mut document, destination.0, destination.1)?.cells[destination.2] =
            source_cell.clone();
    }
    session
        .replace_document_atomic(document)
        .map_err(|_| MenuEditError::MissingEntity)?;
    session.select(Some(StableSelection::Cell {
        menu_id: destination.0.clone(),
        ring_id: destination.1.clone(),
        cell_id: source.2.clone(),
    }));
    Ok(())
}

/// Create a fresh SameCenter submenu and link it to one empty authored slot
/// as one undoable stable-ID transaction.
pub fn create_submenu_and_link(
    session: &mut RadialAuthoringSession,
    parent_menu_id: &MenuId,
    parent_ring_id: &RingId,
    parent_cell_id: &CellId,
    name: &str,
) -> Result<MenuId, MenuEditError> {
    let mut document = (*session.draft).clone();
    let parent = document
        .menus
        .iter()
        .find(|menu| &menu.id == parent_menu_id)
        .ok_or(MenuEditError::MissingEntity)?;
    let parent_ring = parent
        .rings
        .iter()
        .find(|ring| &ring.id == parent_ring_id)
        .ok_or(MenuEditError::MissingEntity)?;
    let parent_cell = parent_ring
        .cells
        .iter()
        .find(|cell| &cell.id == parent_cell_id)
        .ok_or(MenuEditError::MissingEntity)?;
    if !matches!(&parent_cell.content, CellContent::Spacer) {
        return Err(MenuEditError::DestinationOccupied);
    }
    let skin_id = document
        .skins
        .first()
        .map(|skin| skin.id.clone())
        .ok_or(MenuEditError::MissingEntity)?;
    let child_id = session.allocate_menu_id(if name.trim().is_empty() {
        "submenu"
    } else {
        name
    });
    let ring_id = session.allocate_ring_id("ring");
    let cells = (0..8)
        .map(|_| CellDefinition {
            id: session.allocate_cell_id("cell"),
            label: "Spacer".into(),
            content: CellContent::Spacer,
            alternate_clicks: Vec::new(),
            alternate_controls: Vec::new(),
            after_action: AfterActionPolicy::Inherit,
            secondary_after_action: AfterActionPolicy::Inherit,
            icon: Default::default(),
            tooltip: Default::default(),
            style: Default::default(),
            shortcuts: Vec::new(),
            hotstrings: Vec::new(),
        })
        .collect();
    document.menus.push(MenuDefinition {
        id: child_id.clone(),
        name: if name.trim().is_empty() {
            "New submenu".into()
        } else {
            name.trim().to_owned()
        },
        layout: crate::radial::model::LayoutKind::CircularCells,
        interaction: InteractionMode::StickyClick,
        hover_dwell_ms: None,
        submenu_presentation: SubmenuPresentation::SameCenter,
        after_action: AfterActionPolicy::Inherit,
        center_action: None,
        center_primary_after_action: AfterActionPolicy::Inherit,
        center_secondary_action: None,
        center_secondary_after_action: AfterActionPolicy::Inherit,
        center_control: Some(crate::radial::model::Control::Back),
        center_secondary_control: None,
        background_action: None,
        background_primary_after_action: AfterActionPolicy::Inherit,
        background_secondary_action: None,
        background_control: None,
        background_secondary_control: None,
        background_secondary_after_action: AfterActionPolicy::Inherit,
        mirror_primary_to_secondary: false,
        skin_id,
        center_radius: 30.0,
        rings: vec![RingDefinition {
            id: ring_id,
            radius: 92.0,
            cell_radius: 28.0,
            rotation_degrees: -90.0,
            gap: 4.0,
            cells,
            style: Default::default(),
        }],
        style: Default::default(),
    });
    let parent_cell = find_ring_mut(&mut document, parent_menu_id, parent_ring_id)?
        .cells
        .iter_mut()
        .find(|cell| &cell.id == parent_cell_id)
        .ok_or(MenuEditError::MissingEntity)?;
    parent_cell.content = CellContent::Submenu {
        menu_id: child_id.clone(),
    };
    session
        .replace_document_atomic(document)
        .map_err(|_| MenuEditError::MissingEntity)?;
    session.select(Some(StableSelection::Cell {
        menu_id: parent_menu_id.clone(),
        ring_id: parent_ring_id.clone(),
        cell_id: parent_cell_id.clone(),
    }));
    Ok(child_id)
}

/// Link an existing menu to a slot after validating the complete graph.
pub fn link_existing_submenu(
    session: &mut RadialAuthoringSession,
    parent_menu_id: &MenuId,
    parent_ring_id: &RingId,
    parent_cell_id: &CellId,
    child_menu_id: &MenuId,
) -> Result<(), MenuEditError> {
    set_cell_content(
        session,
        parent_menu_id,
        parent_ring_id,
        parent_cell_id,
        CellContent::Submenu {
            menu_id: child_menu_id.clone(),
        },
    )
}

pub fn resize_plan(
    document: &RadialDocument,
    menu_id: &MenuId,
    ring_id: &RingId,
    requested_len: usize,
) -> Result<ResizePlan, MenuEditError> {
    let ring = find_ring(document, menu_id, ring_id)?;
    let split = requested_len.min(ring.cells.len());
    let removed = ring.cells[split..].to_vec();
    let populated_removed = removed
        .iter()
        .filter(|cell| !matches!(cell.content, CellContent::Spacer))
        .map(|cell| cell.id.clone())
        .collect();
    Ok(ResizePlan {
        menu_id: menu_id.clone(),
        ring_id: ring_id.clone(),
        requested_len,
        retained: ring.cells[..split].to_vec(),
        removed,
        populated_removed,
    })
}

pub fn apply_resize(
    session: &mut RadialAuthoringSession,
    plan: ResizePlan,
    resolution: Option<ResizeResolution>,
) -> Result<(), MenuEditError> {
    if plan.requires_resolution() && resolution.is_none() {
        return Err(MenuEditError::ResolutionRequired);
    }
    let mut document = (*session.draft).clone();
    let current = find_ring(&document, &plan.menu_id, &plan.ring_id)?;
    let expected = plan
        .retained
        .iter()
        .chain(plan.removed.iter())
        .collect::<Vec<_>>();
    if current.cells.iter().collect::<Vec<_>>() != expected {
        return Err(MenuEditError::StaleGeneration);
    }
    find_ring_mut(&mut document, &plan.menu_id, &plan.ring_id)?.cells = plan.retained;
    match resolution {
        Some(ResizeResolution::Relocate { menu_id, ring_id }) => {
            find_ring_mut(&mut document, &menu_id, &ring_id)?
                .cells
                .extend(plan.removed);
        }
        Some(ResizeResolution::OverflowRing) => {
            let id = session.allocate_ring_id("overflow");
            let mut overflow = RingDefinition {
                id,
                radius: 92.0,
                cell_radius: 28.0,
                rotation_degrees: -90.0,
                gap: 4.0,
                cells: plan.removed,
                style: Default::default(),
            };
            overflow.radius = proposed_outer_radius(&document, &plan.menu_id, &overflow)?;
            document
                .menus
                .iter_mut()
                .find(|menu| menu.id == plan.menu_id)
                .ok_or(MenuEditError::MissingEntity)?
                .rings
                .push(overflow);
        }
        Some(ResizeResolution::ConfirmDiscard) | None => {}
    }
    while find_ring(&document, &plan.menu_id, &plan.ring_id)?
        .cells
        .len()
        < plan.requested_len
    {
        let id = session.allocate_cell_id("cell");
        let ring = find_ring_mut(&mut document, &plan.menu_id, &plan.ring_id)?;
        ring.cells.push(CellDefinition {
            id,
            label: "Spacer".into(),
            content: CellContent::Spacer,
            alternate_clicks: Vec::new(),
            alternate_controls: Vec::new(),
            after_action: Default::default(),
            secondary_after_action: Default::default(),
            icon: Default::default(),
            tooltip: Default::default(),
            style: Default::default(),
            shortcuts: Vec::new(),
            hotstrings: Vec::new(),
        });
    }
    validate_candidate(&document)?;
    session
        .replace_document_atomic(document)
        .map_err(|error| MenuEditError::InvalidGeometry(format!("{error:?}")))
}

pub fn duplicate_menu(
    session: &mut RadialAuthoringSession,
    root: &MenuId,
    mode: SubmenuDuplication,
) -> Result<MenuId, MenuEditError> {
    ensure_acyclic(&session.draft)?;
    let mut document = (*session.draft).clone();
    let root_menu = document
        .menus
        .iter()
        .find(|menu| &menu.id == root)
        .cloned()
        .ok_or(MenuEditError::MissingEntity)?;
    let mut ids = BTreeSet::new();
    collect_closure(&document, &root_menu.id, &mut BTreeSet::new(), &mut ids)?;
    if mode == SubmenuDuplication::LinkExisting {
        ids = BTreeSet::from([root_menu.id.clone()]);
    }
    let mut mapping: BTreeMap<MenuId, MenuId> = BTreeMap::new();
    for old in &ids {
        let new_id = session.allocate_menu_id(old.as_str());
        mapping.insert(old.clone(), new_id);
    }
    let originals: Vec<MenuDefinition> = document
        .menus
        .iter()
        .filter(|menu| ids.contains(&menu.id))
        .cloned()
        .collect();
    for mut menu in originals {
        let new_id = mapping[&menu.id].clone();
        menu.id = new_id;
        menu.name = format!("{} Copy", menu.name);
        for ring in &mut menu.rings {
            ring.id = session.allocate_ring_id(ring.id.as_str());
            for cell in &mut ring.cells {
                cell.id = session.allocate_cell_id(cell.id.as_str());
                for shortcut in &mut cell.shortcuts {
                    shortcut.id = session.allocate_shortcut_id(shortcut.id.as_str());
                }
                for hotstring in &mut cell.hotstrings {
                    hotstring.id = session.allocate_hotstring_id(hotstring.id.as_str());
                }
                if let CellContent::Submenu { menu_id } = &mut cell.content
                    && let Some(replacement) = mapping.get(menu_id)
                {
                    *menu_id = replacement.clone();
                }
            }
        }
        document.menus.push(menu);
    }
    let new_root = mapping[root].clone();
    session
        .replace_document_atomic(document)
        .map_err(|_| MenuEditError::MissingEntity)?;
    session.select(Some(StableSelection::Menu(new_root.clone())));
    Ok(new_root)
}

/// Copy one cell while remapping every identity owned by that cell.
pub fn copy_cell(
    session: &mut RadialAuthoringSession,
    menu_id: &MenuId,
    ring_id: &RingId,
    cell_id: &CellId,
) -> Result<CellId, MenuEditError> {
    let mut copy = find_ring(&session.draft, menu_id, ring_id)?
        .cells
        .iter()
        .find(|cell| &cell.id == cell_id)
        .cloned()
        .ok_or(MenuEditError::MissingEntity)?;
    copy.id = session.allocate_cell_id(cell_id.as_str());
    for shortcut in &mut copy.shortcuts {
        shortcut.id = session.allocate_shortcut_id(shortcut.id.as_str());
    }
    for hotstring in &mut copy.hotstrings {
        hotstring.id = session.allocate_hotstring_id(hotstring.id.as_str());
    }
    copy.label = format!("{} Copy", copy.label);
    let copied_id = copy.id.clone();
    let mut document = (*session.draft).clone();
    find_ring_mut(&mut document, menu_id, ring_id)?
        .cells
        .push(copy);
    session
        .replace_document_atomic(document)
        .map_err(|_| MenuEditError::MissingEntity)?;
    session.select(Some(StableSelection::Cell {
        menu_id: menu_id.clone(),
        ring_id: ring_id.clone(),
        cell_id: copied_id.clone(),
    }));
    Ok(copied_id)
}

fn collect_closure(
    document: &RadialDocument,
    id: &MenuId,
    active: &mut BTreeSet<MenuId>,
    found: &mut BTreeSet<MenuId>,
) -> Result<(), MenuEditError> {
    if !active.insert(id.clone()) {
        return Err(MenuEditError::SubmenuCycle);
    }
    found.insert(id.clone());
    let menu = document
        .menus
        .iter()
        .find(|menu| &menu.id == id)
        .ok_or(MenuEditError::MissingEntity)?;
    for child in menu
        .rings
        .iter()
        .flat_map(|ring| &ring.cells)
        .filter_map(|cell| match &cell.content {
            CellContent::Submenu { menu_id } => Some(menu_id),
            _ => None,
        })
    {
        if !found.contains(child) {
            collect_closure(document, child, active, found)?;
        } else if active.contains(child) {
            return Err(MenuEditError::SubmenuCycle);
        }
    }
    active.remove(id);
    Ok(())
}

fn ensure_acyclic(document: &RadialDocument) -> Result<(), MenuEditError> {
    let mut found = BTreeSet::new();
    for menu in &document.menus {
        collect_closure(document, &menu.id, &mut BTreeSet::new(), &mut found)?;
    }
    Ok(())
}

/// Validate the complete submenu graph before a compound UI edit is
/// committed.  Callers that edit a cloned cell/document can use this same
/// guard as [`set_cell_content`] so the graph cannot bypass the typed link
/// boundary.
pub fn validate_submenu_graph(document: &RadialDocument) -> Result<(), MenuEditError> {
    ensure_acyclic(document)
}

fn would_create_submenu_cycle(
    document: &RadialDocument,
    parent_menu_id: &MenuId,
    child_menu_id: &MenuId,
) -> bool {
    if parent_menu_id == child_menu_id {
        return true;
    }
    let mut visiting = BTreeSet::new();
    menu_reaches(document, child_menu_id, parent_menu_id, &mut visiting)
}

fn menu_reaches(
    document: &RadialDocument,
    current: &MenuId,
    target: &MenuId,
    visiting: &mut BTreeSet<MenuId>,
) -> bool {
    if !visiting.insert(current.clone()) {
        return false;
    }
    let reaches = document
        .menus
        .iter()
        .find(|menu| &menu.id == current)
        .is_some_and(|menu| {
            menu.rings.iter().flat_map(|ring| &ring.cells).any(|cell| {
                let CellContent::Submenu { menu_id } = &cell.content else {
                    return false;
                };
                menu_id == target || menu_reaches(document, menu_id, target, visiting)
            })
        });
    visiting.remove(current);
    reaches
}

fn find_ring<'a>(
    document: &'a RadialDocument,
    menu_id: &MenuId,
    ring_id: &RingId,
) -> Result<&'a RingDefinition, MenuEditError> {
    document
        .menus
        .iter()
        .find(|menu| &menu.id == menu_id)
        .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
        .ok_or(MenuEditError::MissingEntity)
}

fn find_ring_mut<'a>(
    document: &'a mut RadialDocument,
    menu_id: &MenuId,
    ring_id: &RingId,
) -> Result<&'a mut RingDefinition, MenuEditError> {
    document
        .menus
        .iter_mut()
        .find(|menu| &menu.id == menu_id)
        .and_then(|menu| menu.rings.iter_mut().find(|ring| &ring.id == ring_id))
        .ok_or(MenuEditError::MissingEntity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_menu_uses_feature_interaction_and_submenu_defaults() {
        let mut session = session();
        let id = create_menu_with_defaults(
            &mut session,
            "work",
            "Work",
            crate::radial::model::InteractionMode::HoldAndClick,
            crate::radial::model::SubmenuPresentation::SameCenter,
        )
        .unwrap();
        let menu = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == id)
            .unwrap();
        assert_eq!(
            menu.interaction,
            crate::radial::model::InteractionMode::HoldAndClick
        );
        assert_eq!(
            menu.submenu_presentation,
            crate::radial::model::SubmenuPresentation::SameCenter
        );
    }

    #[test]
    fn new_menu_does_not_clone_hidden_inputs_actions_or_styles() {
        let mut session = session();
        let mut poisoned = (*session.draft).clone();
        let source = &mut poisoned.menus[0];
        source.center_action = Some(crate::radial::model::ActionBinding::Persisted {
            action: crate::universal_actions::PersistedUniversalActionRef {
                target: None,
                action_id: crate::universal_actions::action_ids::RESULT_EXECUTE,
            },
        });
        source.background_control = Some(crate::radial::model::Control::Close);
        source.hover_dwell_ms = Some(777);
        source.mirror_primary_to_secondary = true;
        let source_cell = &mut source.rings[0].cells[0];
        source_cell
            .shortcuts
            .push(crate::radial::model::ItemShortcut {
                id: crate::radial::model::ShortcutId::new("hidden"),
                chord: "Ctrl+Shift+H".into(),
                gesture: crate::radial::model::ClickGesture::Primary,
                scope: crate::radial::model::TriggerScope::Global,
            });
        source_cell
            .hotstrings
            .push(crate::radial::model::ItemHotstring {
                id: crate::radial::model::HotstringId::new("hidden"),
                text: ":hidden".into(),
                gesture: crate::radial::model::ClickGesture::Primary,
                case_sensitive: false,
                scope: crate::radial::model::TriggerScope::Global,
            });
        session.replace_document_atomic(poisoned).unwrap();

        let id = create_menu_with_defaults(
            &mut session,
            "clean",
            "Clean",
            crate::radial::model::InteractionMode::HoldAndClick,
            crate::radial::model::SubmenuPresentation::SameCenter,
        )
        .unwrap();
        let menu = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == id)
            .unwrap();
        assert!(menu.center_action.is_none());
        assert!(menu.background_action.is_none());
        assert!(menu.center_control.is_none());
        assert!(menu.background_control.is_none());
        assert_eq!(menu.hover_dwell_ms, None);
        assert!(!menu.mirror_primary_to_secondary);
        assert_eq!(menu.style, Default::default());
        assert_eq!(menu.rings.len(), 1);
        assert_eq!(menu.rings[0].style, Default::default());
        assert!(menu.rings[0].cells.iter().all(|cell| {
            matches!(cell.content, CellContent::Spacer)
                && cell.alternate_clicks.is_empty()
                && cell.alternate_controls.is_empty()
                && cell.shortcuts.is_empty()
                && cell.hotstrings.is_empty()
                && cell.style == Default::default()
        }));
    }

    #[test]
    fn adding_ring_creates_valid_starter_spacer_slots() {
        let mut session = session();
        let menu = session.draft.menus[0].id.clone();
        let ring = add_ring(&mut session, &menu).unwrap();
        let ring = session.draft.menus[0]
            .rings
            .iter()
            .find(|candidate| candidate.id == ring)
            .unwrap();
        assert_eq!(ring.cells.len(), 8);
        assert!(ring.cells.iter().all(|cell| {
            matches!(&cell.content, CellContent::Spacer) && cell.label == "Spacer"
        }));
        assert!(crate::radial::validation::validate(&session.draft).is_ok());
    }

    #[test]
    fn new_ring_proposal_uses_actual_outer_extent_and_applies_atomically() {
        let mut session = session();
        let menu_id = session.draft.menus[0].id.clone();
        let mut customized = (*session.draft).clone();
        customized.menus[0].rings[0].radius = 240.0;
        customized.menus[0].rings[0].cell_radius = 36.0;
        session.replace_document_atomic(customized).unwrap();
        let before = session.draft.clone();
        let ring_id = session.allocate_ring_id("ring");
        let cell_ids = (0..8).map(|_| session.allocate_cell_id("cell")).collect();
        let proposal = propose_new_ring(
            &session.draft,
            session.generation,
            &menu_id,
            ring_id.clone(),
            cell_ids,
        )
        .unwrap();
        assert!(proposal.proposed_radius > 240.0 + 36.0);
        assert_eq!(&*session.draft, &*before, "proposal is preview-only");
        apply_ring_proposal(&mut session, proposal).unwrap();
        assert_eq!(
            session.selection,
            Some(StableSelection::Ring { menu_id, ring_id })
        );
        assert!(session.undo());
        assert_eq!(&*session.draft, &*before);
    }

    #[test]
    fn valid_inner_ring_resize_preserves_existing_radius() {
        let mut session = session();
        let menu_id = session.draft.menus[0].id.clone();
        let outer = add_ring(&mut session, &menu_id).unwrap();
        let inner = session.draft.menus[0].rings[0].id.clone();
        let inner_radius = session.draft.menus[0].rings[0].radius;
        let ids = vec![session.allocate_cell_id("cell")];
        let proposal =
            propose_ring_resize(&session.draft, session.generation, &menu_id, &inner, 9, ids)
                .unwrap();
        assert_eq!(proposal.proposed_radius, inner_radius);
        assert_eq!(
            proposal.document.menus[0]
                .rings
                .iter()
                .find(|ring| ring.id == outer)
                .unwrap()
                .radius,
            session.draft.menus[0]
                .rings
                .iter()
                .find(|ring| ring.id == outer)
                .unwrap()
                .radius
        );
    }

    use crate::radial::authoring::{AuthoringSnapshot, DiskSha256};

    fn session() -> RadialAuthoringSession {
        let document = RadialDocument::starter();
        RadialAuthoringSession::new(AuthoringSnapshot {
            revision: document.revision,
            document: std::sync::Arc::new(document),
            disk_sha256: DiskSha256("test".into()),
        })
    }

    fn make_root_slot_spacer(session: &mut RadialAuthoringSession, cell_index: usize) {
        // The production starter document intentionally has no empty root
        // slot.  These authoring tests need a real Spacer destination so they
        // exercise the typed drop/link semantics rather than an occupied slot.
        let mut document = (*session.draft).clone();
        let cell = &mut document.menus[0].rings[0].cells[cell_index];
        cell.label = "Spacer".into();
        cell.content = CellContent::Spacer;
        session.draft = std::sync::Arc::new(document);
    }

    #[test]
    fn create_submenu_and_link_is_one_undoable_same_center_edit() {
        let mut session = session();
        make_root_slot_spacer(&mut session, 7);
        let menu = session.draft.menus[0].id.clone();
        let ring = session.draft.menus[0].rings[0].id.clone();
        let cell = session.draft.menus[0].rings[0].cells[7].id.clone();
        let before = session.draft.clone();
        let child = create_submenu_and_link(&mut session, &menu, &ring, &cell, "Child").unwrap();
        assert_eq!(session.draft.menus.len(), before.menus.len() + 1);
        assert!(session.draft.menus.iter().any(|menu| menu.id == child));
        assert!(matches!(
            session.draft.menus[0].rings[0].cells[7].content,
            CellContent::Submenu { ref menu_id } if menu_id == &child
        ));
        assert!(session.undo());
        assert_eq!(&*session.draft, &*before);
        assert!(!session.undo(), "create/link is a single undo step");
    }

    #[test]
    fn slot_drop_swaps_with_spacer_and_rejects_occupied_without_overwrite() {
        let mut session = session();
        make_root_slot_spacer(&mut session, 7);
        let menu = session.draft.menus[0].id.clone();
        let ring = session.draft.menus[0].rings[0].id.clone();
        let source = session.draft.menus[0].rings[0].cells[0].id.clone();
        let destination = session.draft.menus[0].rings[0].cells[7].id.clone();
        let expected = session.generation;
        let unchanged = session.draft.clone();
        move_cell_to_slot(
            &mut session,
            (&menu, &ring, &source),
            (&menu, &ring, 7),
            CellDropResolution::Cancel,
            expected,
        )
        .unwrap();
        assert_eq!(&*session.draft, &*unchanged);
        let expected = session.generation;
        move_cell_to_slot(
            &mut session,
            (&menu, &ring, &source),
            (&menu, &ring, 7),
            CellDropResolution::MoveIntoSpacer,
            expected,
        )
        .unwrap();
        assert_eq!(session.draft.menus[0].rings[0].cells[7].id, source);
        assert_eq!(session.draft.menus[0].rings[0].cells[0].id, destination);

        let occupied = session.draft.menus[0].rings[0].cells[1].id.clone();
        let unchanged = session.draft.clone();
        let expected = session.generation;
        assert_eq!(
            move_cell_to_slot(
                &mut session,
                (&menu, &ring, &source),
                (&menu, &ring, 1),
                CellDropResolution::MoveIntoSpacer,
                expected,
            ),
            Err(MenuEditError::DestinationOccupied)
        );
        assert_eq!(&*session.draft, &*unchanged);
        let expected = session.generation;
        move_cell_to_slot(
            &mut session,
            (&menu, &ring, &source),
            (&menu, &ring, 1),
            CellDropResolution::Swap,
            expected,
        )
        .unwrap();
        assert_eq!(session.draft.menus[0].rings[0].cells[1].id, source);
        assert_eq!(session.draft.menus[0].rings[0].cells[7].id, occupied);
    }

    #[test]
    fn stale_slot_drop_is_rejected() {
        let mut session = session();
        let menu = session.draft.menus[0].id.clone();
        let ring = session.draft.menus[0].rings[0].id.clone();
        let source = session.draft.menus[0].rings[0].cells[0].id.clone();
        let expected = session.generation;
        rename_menu(
            &mut session,
            menu.clone(),
            "changed".into(),
            super::super::EditPhase::Atomic,
        )
        .unwrap();
        assert_eq!(
            move_cell_to_slot(
                &mut session,
                (&menu, &ring, &source),
                (&menu, &ring, 7),
                CellDropResolution::Swap,
                expected,
            ),
            Err(MenuEditError::StaleGeneration)
        );
    }

    #[test]
    fn authored_dynamic_source_can_move_to_a_spacer() {
        let mut session = session();
        make_root_slot_spacer(&mut session, 7);
        let menu = session.draft.menus[0].id.clone();
        let ring = session.draft.menus[0].rings[0].id.clone();
        let mut document = (*session.draft).clone();
        document.menus[0].rings[0].cells[0].content = CellContent::Dynamic {
            source: crate::radial::model::DynamicSource::Favorites,
        };
        let source = document.menus[0].rings[0].cells[0].id.clone();
        session.draft = std::sync::Arc::new(document);
        let generation = session.generation;
        move_cell_to_slot(
            &mut session,
            (&menu, &ring, &source),
            (&menu, &ring, 7),
            CellDropResolution::MoveIntoSpacer,
            generation,
        )
        .expect("authored Dynamic source definitions are movable");
        assert_eq!(session.draft.menus[0].rings[0].cells[7].id, source);
        assert!(matches!(
            session.draft.menus[0].rings[0].cells[7].content,
            CellContent::Dynamic { .. }
        ));
    }

    #[test]
    fn authored_dyn_prefixed_id_is_still_movable() {
        let mut session = session();
        make_root_slot_spacer(&mut session, 7);
        let menu = session.draft.menus[0].id.clone();
        let ring = session.draft.menus[0].rings[0].id.clone();
        let mut document = (*session.draft).clone();
        document.menus[0].rings[0].cells[0].id = CellId::new("dyn:authored-slot");
        session.draft = std::sync::Arc::new(document);
        let source = session.draft.menus[0].rings[0].cells[0].id.clone();
        let generation = session.generation;
        move_cell_to_slot(
            &mut session,
            (&menu, &ring, &source),
            (&menu, &ring, 7),
            CellDropResolution::MoveIntoSpacer,
            generation,
        )
        .expect("an authored dyn:-prefixed ID remains a stable authored cell");
        assert_eq!(session.draft.menus[0].rings[0].cells[7].id, source);
    }

    #[test]
    fn submenu_links_reject_graph_cycles() {
        let mut session = session();
        make_root_slot_spacer(&mut session, 7);
        let root = session.draft.menus[0].id.clone();
        let child = create_menu(&mut session, "child", "Child").unwrap();
        let ring = session.draft.menus[0].rings[0].id.clone();
        let cell = session.draft.menus[0].rings[0].cells[7].id.clone();
        link_existing_submenu(&mut session, &root, &ring, &cell, &child).unwrap();
        let child_ring = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == child)
            .unwrap()
            .rings[0]
            .id
            .clone();
        let child_cell = session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == child)
            .unwrap()
            .rings[0]
            .cells[0]
            .id
            .clone();
        assert_eq!(
            link_existing_submenu(&mut session, &child, &child_ring, &child_cell, &root),
            Err(MenuEditError::SubmenuCycle)
        );
    }

    #[test]
    fn shrink_requires_explicit_resolution_and_undo_restores_cells() {
        let mut session = session();
        let menu = session.draft.menus[0].id.clone();
        let ring = session.draft.menus[0].rings[0].id.clone();
        let before = session.draft.menus[0].rings[0].cells.clone();
        let plan = resize_plan(&session.draft, &menu, &ring, 1).unwrap();
        assert!(plan.requires_resolution());
        assert_eq!(
            apply_resize(&mut session, plan.clone(), None),
            Err(MenuEditError::ResolutionRequired)
        );
        apply_resize(&mut session, plan, Some(ResizeResolution::OverflowRing)).unwrap();
        assert_eq!(session.draft.menus[0].rings[0].cells.len(), 1);
        assert!(session.undo());
        assert_eq!(session.draft.menus[0].rings[0].cells, before);
    }

    #[test]
    fn stale_resize_plan_cannot_replace_newer_cell_content() {
        let mut session = session();
        let menu = session.draft.menus[0].id.clone();
        let ring = session.draft.menus[0].rings[0].id.clone();
        let plan = resize_plan(&session.draft, &menu, &ring, 4).unwrap();
        let cell = session.draft.menus[0].rings[0].cells[0].id.clone();
        set_cell_label(
            &mut session,
            &menu,
            &ring,
            &cell,
            "Newer label".into(),
            super::super::EditPhase::Atomic,
        )
        .unwrap();
        assert_eq!(
            apply_resize(&mut session, plan, Some(ResizeResolution::ConfirmDiscard)),
            Err(MenuEditError::StaleGeneration)
        );
        assert_eq!(
            session.draft.menus[0].rings[0].cells[0].label,
            "Newer label"
        );
    }

    #[test]
    fn populated_shrink_resolution_is_preview_only_until_atomic_apply() {
        let mut session = session();
        let menu = session.draft.menus[0].id.clone();
        let ring = session.draft.menus[0].rings[0].id.clone();
        let before = session.draft.clone();
        let plan = resize_plan(&session.draft, &menu, &ring, 1).unwrap();
        let overflow_id = session.allocate_ring_id("overflow");
        let proposal = propose_resolved_resize(
            &session.draft,
            session.generation,
            &plan,
            ResizeResolution::OverflowRing,
            Some(overflow_id.clone()),
        )
        .unwrap();
        assert_eq!(&*session.draft, &*before);
        assert!(
            proposal
                .resolution_summary
                .as_deref()
                .unwrap()
                .contains("overflow")
        );
        assert!(
            proposal.document.menus[0]
                .rings
                .iter()
                .any(|ring| { ring.id == overflow_id && ring.cells.len() == plan.removed.len() })
        );
        apply_ring_proposal(&mut session, proposal).unwrap();
        assert!(session.undo());
        assert_eq!(&*session.draft, &*before);
    }

    #[test]
    fn clone_closure_retargets_children_while_link_mode_reuses_them() {
        let mut session = session();
        let root = session.draft.menus[0].id.clone();
        let child = create_menu(&mut session, "child", "Child").unwrap();
        let mut document = (*session.draft).clone();
        document.menus[0].rings[0].cells[0].content = CellContent::Submenu {
            menu_id: child.clone(),
        };
        session.replace_document_atomic(document).unwrap();
        let linked = duplicate_menu(&mut session, &root, SubmenuDuplication::LinkExisting).unwrap();
        let linked_target = match &session
            .draft
            .menus
            .iter()
            .find(|m| m.id == linked)
            .unwrap()
            .rings[0]
            .cells[0]
            .content
        {
            CellContent::Submenu { menu_id } => menu_id,
            _ => panic!(),
        };
        assert_eq!(linked_target, &child);
        let cloned =
            duplicate_menu(&mut session, &root, SubmenuDuplication::CloneSubmenuClosure).unwrap();
        let cloned_target = match &session
            .draft
            .menus
            .iter()
            .find(|m| m.id == cloned)
            .unwrap()
            .rings[0]
            .cells[0]
            .content
        {
            CellContent::Submenu { menu_id } => menu_id,
            _ => panic!(),
        };
        assert_ne!(cloned_target, &child);
        assert!(session.draft.menus.iter().any(|m| &m.id == cloned_target));
    }

    #[test]
    fn duplicate_and_copy_remap_nested_input_ids_and_never_reuse_deleted_ids() {
        let mut session = session();
        let root = session.draft.menus[0].id.clone();
        let ring = session.draft.menus[0].rings[0].id.clone();
        let source_cell = session.draft.menus[0].rings[0].cells[0].id.clone();
        let mut document = (*session.draft).clone();
        let cell = &mut document.menus[0].rings[0].cells[0];
        cell.shortcuts.push(crate::radial::model::ItemShortcut {
            id: crate::radial::model::ShortcutId::new("shortcut"),
            chord: "Ctrl+1".into(),
            gesture: crate::radial::model::ClickGesture::Primary,
            scope: crate::radial::model::TriggerScope::MenuLocal,
        });
        cell.hotstrings.push(crate::radial::model::ItemHotstring {
            id: crate::radial::model::HotstringId::new("hotstring"),
            text: ";one".into(),
            gesture: crate::radial::model::ClickGesture::Primary,
            case_sensitive: false,
            scope: crate::radial::model::TriggerScope::MenuLocal,
        });
        session.replace_document_atomic(document).unwrap();

        let copied = copy_cell(&mut session, &root, &ring, &source_cell).unwrap();
        let copied_cell = session.draft.menus[0].rings[0]
            .cells
            .iter()
            .find(|cell| cell.id == copied)
            .unwrap();
        assert_ne!(copied_cell.shortcuts[0].id.as_str(), "shortcut");
        assert_ne!(copied_cell.hotstrings[0].id.as_str(), "hotstring");

        let duplicated =
            duplicate_menu(&mut session, &root, SubmenuDuplication::LinkExisting).unwrap();
        let duplicated_cell = &session
            .draft
            .menus
            .iter()
            .find(|menu| menu.id == duplicated)
            .unwrap()
            .rings[0]
            .cells[0];
        assert_ne!(duplicated_cell.id, source_cell);
        assert_ne!(duplicated_cell.shortcuts[0].id.as_str(), "shortcut");
        assert_ne!(duplicated_cell.hotstrings[0].id.as_str(), "hotstring");

        let deleted = add_spacer(&mut session, &root, &ring).unwrap();
        delete_cell(&mut session, &root, &ring, &deleted).unwrap();
        let replacement = add_spacer(&mut session, &root, &ring).unwrap();
        assert_ne!(deleted, replacement);
    }

    #[test]
    fn reference_impact_guards_delete_and_widget_ids_ignore_labels_and_indexes() {
        let mut session = session();
        let root = session.draft.menus[0].id.clone();
        let child = create_menu(&mut session, "child", "Child").unwrap();
        let mut document = (*session.draft).clone();
        document.menus[0].rings.push(RingDefinition {
            id: RingId::new("other"),
            radius: 150.0,
            cell_radius: 20.0,
            rotation_degrees: 0.0,
            gap: 0.0,
            cells: vec![CellDefinition {
                id: CellId::new("link"),
                label: "Any label".into(),
                content: CellContent::Submenu {
                    menu_id: child.clone(),
                },
                alternate_clicks: vec![],
                alternate_controls: vec![],
                after_action: Default::default(),
                secondary_after_action: Default::default(),
                icon: Default::default(),
                tooltip: Default::default(),
                style: Default::default(),
                shortcuts: vec![],
                hotstrings: vec![],
            }],
            style: Default::default(),
        });
        session.replace_document_atomic(document).unwrap();
        assert!(matches!(
            delete_menu(&mut session, &child),
            Err(MenuEditError::Referenced(_))
        ));
        assert_eq!(
            widget_key("menu", root.as_str(), "name"),
            widget_key("menu", root.as_str(), "name")
        );
        assert_ne!(
            widget_key("menu", root.as_str(), "name"),
            widget_key("menu", root.as_str(), "layout")
        );
    }

    #[test]
    fn deleting_selected_entity_restores_selection_to_a_stable_survivor() {
        let mut session = session();
        let root = session.draft.menus[0].id.clone();
        let child = create_menu(&mut session, "child", "Child").unwrap();
        assert_eq!(
            session.selection,
            Some(StableSelection::Menu(child.clone()))
        );

        delete_menu(&mut session, &child).unwrap();

        assert_eq!(session.selection, Some(StableSelection::Menu(root)));
    }

    #[test]
    fn cyclic_submenu_graph_is_rejected_before_link_or_clone() {
        let mut session = session();
        let root = session.draft.menus[0].id.clone();
        let child = create_menu(&mut session, "child", "Child").unwrap();
        let mut document = (*session.draft).clone();
        document.menus[0].rings[0].cells[0].content = CellContent::Submenu {
            menu_id: child.clone(),
        };
        let root_ring = document.menus[0].rings[0].clone();
        let child_menu = document
            .menus
            .iter_mut()
            .find(|menu| menu.id == child)
            .unwrap();
        child_menu.rings.push(root_ring);
        child_menu.rings[0].cells[0].content = CellContent::Submenu {
            menu_id: root.clone(),
        };
        session.replace_document_atomic(document).unwrap();
        assert_eq!(
            duplicate_menu(&mut session, &root, SubmenuDuplication::LinkExisting),
            Err(MenuEditError::SubmenuCycle)
        );
        assert_eq!(
            duplicate_menu(&mut session, &root, SubmenuDuplication::CloneSubmenuClosure),
            Err(MenuEditError::SubmenuCycle)
        );
    }
}
