//! Pure, stable-ID menu graph edits used by the egui authoring surface.

use std::collections::{BTreeMap, BTreeSet};

use super::{RadialAuthoringSession, StableSelection};
use crate::radial::model::{
    CellContent, CellDefinition, CellId, MenuDefinition, MenuId, RadialDocument, RingDefinition,
    RingId,
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
        crate::radial::model::SubmenuPresentation::Cascade,
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
        .map_err(|_| MenuEditError::MissingEntity)?;
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
    let before = document.menus.len();
    document.menus.retain(|menu| &menu.id != id);
    if document.menus.len() == before {
        return Err(MenuEditError::MissingEntity);
    }
    session
        .replace_document_atomic(document)
        .map_err(|_| MenuEditError::MissingEntity)
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
        .map_err(|_| MenuEditError::MissingEntity)?;
    session.select(Some(StableSelection::Menu(id.clone())));
    Ok(())
}

pub fn add_ring(
    session: &mut RadialAuthoringSession,
    menu_id: &MenuId,
) -> Result<RingId, MenuEditError> {
    let id = session.allocate_ring_id("ring");
    let mut document = (*session.draft).clone();
    let menu = document
        .menus
        .iter_mut()
        .find(|menu| &menu.id == menu_id)
        .ok_or(MenuEditError::MissingEntity)?;
    let ordinal = menu.rings.len() as f32;
    menu.rings.push(RingDefinition {
        id: id.clone(),
        radius: 92.0 + ordinal * 64.0,
        cell_radius: 28.0,
        rotation_degrees: -90.0,
        gap: 4.0,
        cells: Vec::new(),
        style: Default::default(),
    });
    session
        .replace_document_atomic(document)
        .map_err(|_| MenuEditError::MissingEntity)?;
    session.select(Some(StableSelection::Ring {
        menu_id: menu_id.clone(),
        ring_id: id.clone(),
    }));
    Ok(id)
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
        .map_err(|_| MenuEditError::MissingEntity)?;
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
    let mut document = (*session.draft).clone();
    let source_ring = find_ring_mut(&mut document, source.0, source.1)?;
    let index = source_ring
        .cells
        .iter()
        .position(|cell| &cell.id == source.2)
        .ok_or(MenuEditError::MissingEntity)?;
    let cell = source_ring.cells.remove(index);
    let destination_ring = find_ring_mut(&mut document, destination.0, destination.1)?;
    let insert = destination.2.min(destination_ring.cells.len());
    destination_ring.cells.insert(insert, cell.clone());
    session
        .replace_document_atomic(document)
        .map_err(|_| MenuEditError::MissingEntity)?;
    session.select(Some(StableSelection::Cell {
        menu_id: destination.0.clone(),
        ring_id: destination.1.clone(),
        cell_id: cell.id,
    }));
    Ok(())
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
    find_ring_mut(&mut document, &plan.menu_id, &plan.ring_id)?.cells = plan.retained;
    match resolution {
        Some(ResizeResolution::Relocate { menu_id, ring_id }) => {
            find_ring_mut(&mut document, &menu_id, &ring_id)?
                .cells
                .extend(plan.removed);
        }
        Some(ResizeResolution::OverflowRing) => {
            let id = session.allocate_ring_id("overflow");
            let menu = document
                .menus
                .iter_mut()
                .find(|menu| menu.id == plan.menu_id)
                .ok_or(MenuEditError::MissingEntity)?;
            menu.rings.push(RingDefinition {
                id,
                radius: menu.rings.last().map_or(92.0, |ring| ring.radius + 64.0),
                cell_radius: 28.0,
                rotation_degrees: -90.0,
                gap: 4.0,
                cells: plan.removed,
                style: Default::default(),
            });
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
    session
        .replace_document_atomic(document)
        .map_err(|_| MenuEditError::MissingEntity)
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
    use crate::radial::authoring::{AuthoringSnapshot, DiskSha256};

    fn session() -> RadialAuthoringSession {
        let document = RadialDocument::starter();
        RadialAuthoringSession::new(AuthoringSnapshot {
            revision: document.revision,
            document: std::sync::Arc::new(document),
            disk_sha256: DiskSha256("test".into()),
        })
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
