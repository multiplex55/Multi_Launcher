use super::context::{InvocationContext, WindowIdentity};
use super::dynamic::{FrozenAvailability, FrozenBinding, FrozenDynamicFrame};
use super::handoff::{InteractionRequirement, interaction_requirement};
use super::model::{
    ActionBinding, AfterActionPolicy, CellContent, CellDefinition, CellId, InvocationId,
    MenuDefinition, MenuId, RadialDocument, TargetSelector,
};
use crate::actions::Action;
use crate::universal_actions::{
    ActionResolutionContext, ActionSurface, ActionTarget, PersistedActionCatalog,
    PersistedActionUnavailable, ResolvedActionTarget, UniversalAction, UniversalActionRegistry,
};
use std::collections::BTreeMap;
use std::sync::{Arc, mpsc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PreparationGeneration(pub u64);

#[derive(Clone, Debug)]
pub struct RadialPrepareRequest {
    pub generation: PreparationGeneration,
    pub invocation_id: InvocationId,
    pub requested_menu_id: MenuId,
    pub document: Arc<RadialDocument>,
    pub context: InvocationContext,
    pub invocation_query: String,
    /// Explicit direct-menu invocations never participate in context routing.
    pub allow_context_rules: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadialPrepareReply {
    pub generation: PreparationGeneration,
    pub invocation_id: InvocationId,
    pub menu_id: MenuId,
    pub unavailable: BTreeMap<CellId, BindingUnavailable>,
    pub dynamic: BTreeMap<CellId, FrozenDynamicFrame>,
    pub frame: PreparedMenuFrame,
    pub static_cells: BTreeMap<CellId, PreparedCell>,
    pub frames: BTreeMap<MenuId, PreparedMenuFrame>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedCell {
    pub binding: FrozenBinding,
    pub availability: FrozenAvailability,
    pub requirement: InteractionRequirement,
    pub after_action: AfterActionPolicy,
    pub history_query: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedMenuFrame {
    pub base_menu: MenuDefinition,
    pub menu: MenuDefinition,
    pub cells: BTreeMap<CellId, PreparedCell>,
    pub static_cells: BTreeMap<CellId, PreparedCell>,
    pub dynamic: BTreeMap<CellId, FrozenDynamicFrame>,
    pub alternates: BTreeMap<(CellId, super::model::ClickGesture), PreparedCell>,
    pub page: usize,
    pub page_count: usize,
}

/// Projects invocation-frozen dynamic entries into the immutable geometry
/// definition used by both rendering and hit testing. The returned synthetic
/// IDs are stable for this source frame and never encode a mutable catalog
/// index as their dispatch identity.
pub fn project_menu_frame(
    menu: &MenuDefinition,
    static_cells: BTreeMap<CellId, PreparedCell>,
    dynamic: &BTreeMap<CellId, FrozenDynamicFrame>,
    page: usize,
    _legacy_page_size: usize,
) -> PreparedMenuFrame {
    project_menu_frame_with_style(menu, static_cells, dynamic, page, None)
}

pub fn project_menu_frame_with_style(
    menu: &MenuDefinition,
    static_cells: BTreeMap<CellId, PreparedCell>,
    dynamic: &BTreeMap<CellId, FrozenDynamicFrame>,
    page: usize,
    style: Option<&super::skin::EffectiveMenuTree>,
) -> PreparedMenuFrame {
    let mut projected = menu.clone();
    let source_static = static_cells.clone();
    let mut cells = static_cells;
    let ring_pages: Vec<_> = menu
        .rings
        .iter()
        .map(|ring| {
            let slots = style
                .and_then(|style| style.rings.get(&ring.id))
                .map_or_else(
                    || ring_accessible_capacity(ring),
                    |style| {
                        let menu_scale =
                            super::skin::resolved_f32(&style.values.geometry.menu_scale);
                        let item_radius = if style_source_is_application(
                            style,
                            super::skin::StyleField::ItemSize,
                        ) {
                            ring.cell_radius * menu_scale
                        } else {
                            super::skin::resolved_f32(&style.values.geometry.item_size)
                                * menu_scale
                                * 0.5
                        };
                        ring_accessible_capacity_for(
                            ring.radius
                                * super::skin::resolved_f32(&style.values.geometry.radius_scale)
                                * menu_scale,
                            item_radius,
                            ring.gap * menu_scale,
                        )
                    },
                );
            let static_count = ring
                .cells
                .iter()
                .filter(|cell| !matches!(cell.content, CellContent::Dynamic { .. }))
                .count();
            let total = ring
                .cells
                .iter()
                .filter_map(|cell| dynamic.get(&cell.id))
                .map(|frame| frame.entries.len())
                .sum::<usize>();
            let without_controls = slots.saturating_sub(static_count).max(1);
            let entry_capacity = if total > without_controls {
                slots.saturating_sub(static_count + 2).max(1)
            } else {
                without_controls
            };
            (entry_capacity, total.div_ceil(entry_capacity).max(1))
        })
        .collect();
    let page_count = ring_pages
        .iter()
        .map(|(_, pages)| *pages)
        .max()
        .unwrap_or(1);
    let page = page.min(page_count - 1);
    for (ring_index, ring) in projected.rings.iter_mut().enumerate() {
        let (entry_capacity, this_ring_pages) = ring_pages[ring_index];
        let page_start = page.saturating_mul(entry_capacity);
        let page_end = page_start.saturating_add(entry_capacity);
        let mut ring_entry_index = 0usize;
        let mut expanded = Vec::new();
        for source_cell in std::mem::take(&mut ring.cells) {
            let CellContent::Dynamic { source } = &source_cell.content else {
                expanded.push(source_cell);
                continue;
            };
            let Some(frame) = dynamic.get(&source_cell.id) else {
                continue;
            };
            for (source_index, entry) in frame.entries.iter().enumerate() {
                let ordinal = ring_entry_index;
                ring_entry_index = ring_entry_index.saturating_add(1);
                if ordinal < page_start || ordinal >= page_end {
                    continue;
                }
                let id = CellId::new(format!(
                    "dyn:{}:{}:{}",
                    source_cell.id.as_str(),
                    source_index,
                    entry.id.0
                ));
                let binding = entry
                    .binding
                    .clone()
                    .unwrap_or(FrozenBinding::Informational);
                cells.insert(
                    id.clone(),
                    PreparedCell {
                        binding,
                        availability: entry.availability.clone(),
                        requirement: entry.requirement,
                        after_action: source_cell.after_action,
                        history_query: entry.history_query.clone(),
                    },
                );
                expanded.push(CellDefinition {
                    id,
                    label: entry.label.clone(),
                    content: CellContent::Dynamic {
                        source: source.clone(),
                    },
                    alternate_clicks: Vec::new(),
                    alternate_controls: Vec::new(),
                    after_action: source_cell.after_action,
                    secondary_after_action: source_cell.secondary_after_action,
                    icon: source_cell.icon.clone(),
                    tooltip: source_cell.tooltip.clone(),
                    style: source_cell.style.clone(),
                    shortcuts: source_cell.shortcuts.clone(),
                    hotstrings: source_cell.hotstrings.clone(),
                });
            }
        }
        if this_ring_pages > 1 && page > 0 {
            expanded.push(page_control(&ring.id, false));
        }
        if page + 1 < this_ring_pages {
            expanded.push(page_control(&ring.id, true));
        }
        ring.cells = expanded;
    }
    PreparedMenuFrame {
        base_menu: menu.clone(),
        menu: projected,
        cells,
        static_cells: source_static,
        dynamic: dynamic.clone(),
        alternates: BTreeMap::new(),
        page,
        page_count,
    }
}

fn style_source_is_application(
    style: &super::skin::EffectiveRingStyle,
    field: super::skin::StyleField,
) -> bool {
    style.source(field) == Some(&super::skin::StyleSource::ApplicationFallback)
}

pub fn ring_accessible_capacity(ring: &super::model::RingDefinition) -> usize {
    ring_accessible_capacity_for(ring.radius, ring.cell_radius, ring.gap)
}

pub fn ring_accessible_capacity_for(radius: f32, cell_radius: f32, gap: f32) -> usize {
    let required = 2.0 * cell_radius + gap;
    if !required.is_finite() || !radius.is_finite() || radius <= 0.0 {
        return 1;
    }
    let ratio = required / (2.0 * radius);
    if ratio >= 1.0 {
        1
    } else {
        ((std::f32::consts::PI / ratio.max(f32::EPSILON).asin())
            .floor()
            .max(1.0) as usize)
            .min(super::model::limits::MAX_CELLS_PER_RING)
    }
}

fn page_control(ring_id: &super::model::RingId, next: bool) -> CellDefinition {
    CellDefinition {
        id: CellId::new(format!(
            "__radial_page_{}:{}",
            if next { "next" } else { "previous" },
            ring_id.as_str()
        )),
        label: if next { "Next" } else { "Previous" }.into(),
        content: CellContent::Control {
            control: if next {
                super::model::Control::NextPage
            } else {
                super::model::Control::PreviousPage
            },
        },
        alternate_clicks: Vec::new(),
        alternate_controls: Vec::new(),
        after_action: AfterActionPolicy::KeepOpen,
        secondary_after_action: AfterActionPolicy::KeepOpen,
        icon: Default::default(),
        tooltip: Default::default(),
        style: Default::default(),
        shortcuts: Vec::new(),
        hotstrings: Vec::new(),
    }
}

#[derive(Clone, Debug)]
pub struct RadialPrepareEnvelope {
    pub request: RadialPrepareRequest,
    pub reply: mpsc::Sender<RadialPrepareReply>,
    pub wake: mpsc::Sender<()>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingUnavailable {
    Stable(PersistedActionUnavailable),
    ContextTargetMissing {
        selector: TargetSelector,
    },
    ContextActionMissing {
        action_id: crate::universal_actions::ActionId,
    },
    Informational,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedBinding {
    pub binding: ActionBinding,
    pub action: UniversalAction,
    pub requirement: InteractionRequirement,
}

pub struct RadialBindingResolver<'a> {
    pub catalog: &'a PersistedActionCatalog,
    pub registry: &'a UniversalActionRegistry,
}

impl RadialBindingResolver<'_> {
    pub fn resolve_frozen(
        &self,
        binding: &FrozenBinding,
        invocation: &InvocationContext,
        query: &str,
    ) -> Result<PreparedBinding, BindingUnavailable> {
        match binding {
            FrozenBinding::Stable(binding) => self.resolve(binding, invocation, query),
            FrozenBinding::Contextual {
                selector,
                action_id,
                ..
            } => self.resolve(
                &ActionBinding::Contextual {
                    selector: selector.clone(),
                    action_id: action_id.clone(),
                },
                invocation,
                query,
            ),
            FrozenBinding::Informational => Err(BindingUnavailable::Informational),
            FrozenBinding::Runtime {
                target,
                selected_action,
                action_id,
                ..
            } => {
                let resolved = self
                    .catalog
                    .entries()
                    .iter()
                    .find(|candidate| {
                        candidate.target == *target && candidate.selected_action == *selected_action
                    })
                    .cloned()
                    .ok_or_else(|| BindingUnavailable::ContextActionMissing {
                        action_id: action_id.clone(),
                    })?;
                let action = self
                    .registry
                    .resolve(
                        &resolved,
                        &ActionResolutionContext::new(ActionSurface::RadialMenu, query),
                    )
                    .into_iter()
                    .find(|action| &action.id == action_id)
                    .ok_or_else(|| BindingUnavailable::ContextActionMissing {
                        action_id: action_id.clone(),
                    })?;
                Ok(PreparedBinding {
                    binding: ActionBinding::Contextual {
                        selector: TargetSelector::CapturedForeground,
                        action_id: action_id.clone(),
                    },
                    requirement: interaction_requirement(&action),
                    action,
                })
            }
        }
    }

    pub fn resolve(
        &self,
        binding: &ActionBinding,
        invocation: &InvocationContext,
        query: &str,
    ) -> Result<PreparedBinding, BindingUnavailable> {
        let action = match binding {
            ActionBinding::Persisted { action } => {
                self.catalog
                    .resolve(
                        action,
                        self.registry,
                        &ActionResolutionContext::new(ActionSurface::RadialMenu, query),
                    )
                    .map_err(BindingUnavailable::Stable)?
                    .action
            }
            ActionBinding::Contextual {
                selector,
                action_id,
            } => {
                let window = selected_window(invocation, selector).ok_or_else(|| {
                    BindingUnavailable::ContextTargetMissing {
                        selector: selector.clone(),
                    }
                })?;
                let selected_action = Action {
                    label: window.title.clone(),
                    desc: "Windows".into(),
                    action: format!("window:switch:{}", window.hwnd as isize),
                    args: None,
                };
                let target = ResolvedActionTarget {
                    target: ActionTarget::Window {
                        hwnd: window.hwnd as isize,
                    },
                    selected_action,
                    custom_action_index: None,
                };
                self.registry
                    .resolve(
                        &target,
                        &ActionResolutionContext::new(ActionSurface::RadialMenu, query),
                    )
                    .into_iter()
                    .find(|action| &action.id == action_id)
                    .ok_or_else(|| BindingUnavailable::ContextActionMissing {
                        action_id: action_id.clone(),
                    })?
            }
        };
        Ok(PreparedBinding {
            binding: binding.clone(),
            requirement: interaction_requirement(&action),
            action,
        })
    }
}

fn selected_window<'a>(
    invocation: &'a InvocationContext,
    selector: &TargetSelector,
) -> Option<&'a WindowIdentity> {
    match selector {
        TargetSelector::CapturedForeground => invocation.foreground.as_ref(),
        TargetSelector::UnderPointer => invocation.under_pointer.as_ref(),
        TargetSelector::LastExternal => invocation.last_external.as_ref(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::dynamic::{FrozenAvailability, FrozenEntryId, FrozenRadialEntry};
    use crate::radial::model::TargetSelector;
    use crate::universal_actions::action_ids;

    fn context() -> InvocationContext {
        InvocationContext {
            token: 1,
            foreground: Some(WindowIdentity {
                hwnd: 44,
                pid: 55,
                process_name: Some("editor.exe".into()),
                process_path: None,
                class_name: None,
                title: "Editor".into(),
            }),
            under_pointer: None,
            last_external: None,
            monitor_id: "monitor:1".into(),
            pointer_physical: (1, 2),
        }
    }

    #[test]
    fn contextual_target_is_runtime_only_and_missing_target_is_typed() {
        let catalog = PersistedActionCatalog::default();
        let registry = UniversalActionRegistry;
        let resolver = RadialBindingResolver {
            catalog: &catalog,
            registry: &registry,
        };
        let foreground = ActionBinding::Contextual {
            selector: TargetSelector::CapturedForeground,
            action_id: action_ids::WINDOW_ACTIVATE,
        };
        let prepared = resolver.resolve(&foreground, &context(), "").unwrap();
        assert!(matches!(
            prepared.action.target,
            ActionTarget::Window { hwnd: 44 }
        ));
        let under_pointer = ActionBinding::Contextual {
            selector: TargetSelector::UnderPointer,
            action_id: action_ids::WINDOW_ACTIVATE,
        };
        assert!(matches!(
            resolver.resolve(&under_pointer, &context(), ""),
            Err(BindingUnavailable::ContextTargetMissing { .. })
        ));
    }

    #[test]
    fn frozen_dynamic_entries_project_to_stable_selectable_geometry_cells() {
        let mut menu = RadialDocument::starter().menus.remove(0);
        menu.rings[0].cells[0].content = CellContent::Dynamic {
            source: super::super::model::DynamicSource::Favorites,
        };
        let source = menu.rings[0].cells[0].id.clone();
        let binding = FrozenBinding::Stable(ActionBinding::Contextual {
            selector: TargetSelector::CapturedForeground,
            action_id: action_ids::WINDOW_ACTIVATE,
        });
        let dynamic = [(
            source.clone(),
            FrozenDynamicFrame {
                fingerprint: super::super::dynamic::SourceFingerprint {
                    generation: 7,
                    source: "favorites".into(),
                    query: None,
                },
                entries: vec![FrozenRadialEntry {
                    id: FrozenEntryId("favorite:editor".into()),
                    label: "Editor".into(),
                    kind: super::super::dynamic::FrozenEntryKind::Action,
                    binding: Some(binding.clone()),
                    availability: FrozenAvailability::Available,
                    history_query: "fav".into(),
                    requirement: InteractionRequirement::None,
                }],
            },
        )]
        .into_iter()
        .collect();
        let projected = project_menu_frame(&menu, BTreeMap::new(), &dynamic, 0, 12);
        let cell = projected
            .menu
            .rings
            .iter()
            .flat_map(|ring| &ring.cells)
            .find(|cell| cell.label == "Editor")
            .unwrap();
        assert!(
            cell.id
                .as_str()
                .starts_with(&format!("dyn:{}:0:", source.as_str()))
        );
        assert_eq!(projected.cells.get(&cell.id).unwrap().binding, binding);
        let layout = crate::radial::geometry::layout_menu(
            &projected.menu,
            crate::radial::geometry::PhysicalPoint { x: 300.0, y: 300.0 },
            crate::radial::geometry::PhysicalRect {
                min: crate::radial::geometry::PhysicalPoint { x: 0.0, y: 0.0 },
                max: crate::radial::geometry::PhysicalPoint { x: 600.0, y: 600.0 },
            },
            crate::radial::geometry::ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        assert!(
            layout
                .cells
                .iter()
                .any(|layout| layout.cell_id == cell.id && layout.actionable)
        );
    }

    #[test]
    fn frozen_status_entry_is_visible_but_never_dispatchable() {
        let mut menu = RadialDocument::starter().menus.remove(0);
        menu.rings[0].cells.truncate(1);
        menu.rings[0].cells[0].content = CellContent::Dynamic {
            source: super::super::model::DynamicSource::Applications,
        };
        let source = menu.rings[0].cells[0].id.clone();
        let dynamic = [(
            source,
            FrozenDynamicFrame {
                fingerprint: super::super::dynamic::SourceFingerprint {
                    generation: 1,
                    source: "applications".into(),
                    query: None,
                },
                entries: vec![FrozenRadialEntry {
                    id: FrozenEntryId("status:empty".into()),
                    label: "No applications available".into(),
                    kind: super::super::dynamic::FrozenEntryKind::Empty,
                    binding: None,
                    availability: FrozenAvailability::Empty {
                        reason: "No applications available".into(),
                    },
                    history_query: String::new(),
                    requirement: InteractionRequirement::None,
                }],
            },
        )]
        .into_iter()
        .collect();
        let projected = project_menu_frame(&menu, BTreeMap::new(), &dynamic, 0, 12);
        let cell = projected.menu.rings[0]
            .cells
            .iter()
            .find(|cell| cell.label == "No applications available")
            .unwrap();
        let prepared = projected.cells.get(&cell.id).unwrap();
        assert_eq!(prepared.binding, FrozenBinding::Informational);
        assert!(matches!(
            prepared.availability,
            FrozenAvailability::Empty { .. }
        ));
    }

    #[test]
    fn projected_pages_keep_distinct_stable_cells_and_bindings() {
        let mut menu = RadialDocument::starter().menus.remove(0);
        menu.rings[0].cells.truncate(1);
        menu.rings[0].cells[0].content = CellContent::Dynamic {
            source: super::super::model::DynamicSource::Windows,
        };
        let source = menu.rings[0].cells[0].id.clone();
        let entries = (0..10)
            .map(|index| FrozenRadialEntry {
                id: FrozenEntryId(format!("window:{index}")),
                label: format!("Window {index}"),
                kind: super::super::dynamic::FrozenEntryKind::Action,
                binding: Some(FrozenBinding::Runtime {
                    target: ActionTarget::Window { hwnd: index },
                    selected_action: Action {
                        label: format!("Window {index}"),
                        desc: "Windows".into(),
                        action: format!("window:switch:{index}"),
                        args: None,
                    },
                    action_id: action_ids::WINDOW_ACTIVATE,
                    identity: None,
                }),
                availability: FrozenAvailability::Available,
                history_query: String::new(),
                requirement: InteractionRequirement::None,
            })
            .collect();
        let dynamic = [(
            source,
            FrozenDynamicFrame {
                fingerprint: super::super::dynamic::SourceFingerprint {
                    generation: 1,
                    source: "windows".into(),
                    query: None,
                },
                entries,
            },
        )]
        .into_iter()
        .collect();
        let first = project_menu_frame(&menu, BTreeMap::new(), &dynamic, 0, 4);
        let second = project_menu_frame(&menu, BTreeMap::new(), &dynamic, 1, 4);
        assert_eq!(first.page_count, 2);
        assert_eq!(first.menu.rings[0].cells.len(), 8);
        assert_eq!(second.menu.rings[0].cells.len(), 4);
        assert_eq!(
            first.menu.rings[0].cells.last().unwrap().id.as_str(),
            format!("__radial_page_next:{}", menu.rings[0].id.as_str())
        );
        assert_eq!(
            second.menu.rings[0].cells.last().unwrap().id.as_str(),
            format!("__radial_page_previous:{}", menu.rings[0].id.as_str())
        );
        assert_ne!(
            first.menu.rings[0].cells[0].id,
            second.menu.rings[0].cells[0].id
        );
        assert!(matches!(
            second
                .cells
                .get(&second.menu.rings[0].cells[0].id)
                .unwrap()
                .binding,
            FrozenBinding::Runtime {
                target: ActionTarget::Window { hwnd: 7 },
                ..
            }
        ));
        for frame in [&first, &second] {
            let mut document = RadialDocument::starter();
            document.menus[0] = frame.menu.clone();
            crate::radial::validation::validate(&document).unwrap();
            crate::radial::geometry::layout_menu(
                &frame.menu,
                crate::radial::geometry::PhysicalPoint { x: 400.0, y: 400.0 },
                crate::radial::geometry::PhysicalRect {
                    min: crate::radial::geometry::PhysicalPoint { x: 0.0, y: 0.0 },
                    max: crate::radial::geometry::PhysicalPoint { x: 800.0, y: 800.0 },
                },
                crate::radial::geometry::ScaleFactor::new(1.0).unwrap(),
                0.5,
            )
            .unwrap();
        }
    }

    #[test]
    fn multiple_dynamic_sources_share_one_bounded_page_capacity() {
        let mut menu = RadialDocument::starter().menus.remove(0);
        menu.rings[0].cells.truncate(2);
        menu.rings[0].cells[0].content = CellContent::Dynamic {
            source: super::super::model::DynamicSource::Favorites,
        };
        menu.rings[0].cells[1].content = CellContent::Dynamic {
            source: super::super::model::DynamicSource::Applications,
        };
        let dynamic: BTreeMap<_, _> = menu.rings[0]
            .cells
            .iter()
            .enumerate()
            .map(|(source_index, cell)| {
                let entries = (0..6)
                    .map(|index| FrozenRadialEntry {
                        id: FrozenEntryId(format!("{source_index}:{index}")),
                        label: format!("{source_index}:{index}"),
                        kind: super::super::dynamic::FrozenEntryKind::Action,
                        binding: Some(FrozenBinding::Stable(ActionBinding::Contextual {
                            selector: TargetSelector::CapturedForeground,
                            action_id: action_ids::WINDOW_ACTIVATE,
                        })),
                        availability: FrozenAvailability::Available,
                        history_query: String::new(),
                        requirement: InteractionRequirement::None,
                    })
                    .collect();
                (
                    cell.id.clone(),
                    FrozenDynamicFrame {
                        fingerprint: super::super::dynamic::SourceFingerprint {
                            generation: 1,
                            source: cell.id.as_str().into(),
                            query: None,
                        },
                        entries,
                    },
                )
            })
            .collect();
        let pages: Vec<_> = (0..2)
            .map(|page| project_menu_frame(&menu, BTreeMap::new(), &dynamic, page, 5))
            .collect();
        assert_eq!(pages[0].page_count, 2);
        let capacity = ring_accessible_capacity(&menu.rings[0]);
        assert!(
            pages
                .iter()
                .all(|frame| frame.menu.rings[0].cells.len() <= capacity)
        );
        let labels: Vec<_> = pages
            .iter()
            .flat_map(|frame| &frame.menu.rings[0].cells)
            .filter(|cell| matches!(cell.content, CellContent::Dynamic { .. }))
            .map(|cell| cell.label.clone())
            .collect();
        assert_eq!(
            labels,
            vec![
                "0:0", "0:1", "0:2", "0:3", "0:4", "0:5", "1:0", "1:1", "1:2", "1:3", "1:4", "1:5"
            ]
        );
        for frame in &pages {
            let mut document = RadialDocument::starter();
            document.menus[0] = frame.menu.clone();
            crate::radial::validation::validate(&document).unwrap();
        }
    }

    #[test]
    fn multiple_dynamic_rings_page_with_ring_local_geometry_safe_controls() {
        let mut menu = RadialDocument::starter().menus.remove(0);
        menu.rings[0].cells.truncate(1);
        menu.rings[0].cells[0].content = CellContent::Dynamic {
            source: super::super::model::DynamicSource::Favorites,
        };
        let mut outer = menu.rings[0].clone();
        outer.id = super::super::model::RingId::new("outer");
        outer.radius = 170.0;
        outer.cell_radius = 24.0;
        outer.cells[0].id = CellId::new("outer-source");
        outer.cells[0].content = CellContent::Dynamic {
            source: super::super::model::DynamicSource::Applications,
        };
        menu.rings.push(outer);
        let dynamic: BTreeMap<_, _> = menu
            .rings
            .iter()
            .map(|ring| {
                let source = ring.cells[0].id.clone();
                let entries = (0..25)
                    .map(|index| FrozenRadialEntry {
                        id: FrozenEntryId(format!("{}:{index}", ring.id.as_str())),
                        label: format!("{}:{index}", ring.id.as_str()),
                        kind: super::super::dynamic::FrozenEntryKind::Action,
                        binding: Some(FrozenBinding::Stable(ActionBinding::Contextual {
                            selector: TargetSelector::CapturedForeground,
                            action_id: action_ids::WINDOW_ACTIVATE,
                        })),
                        availability: FrozenAvailability::Available,
                        history_query: String::new(),
                        requirement: InteractionRequirement::None,
                    })
                    .collect();
                (
                    source,
                    FrozenDynamicFrame {
                        fingerprint: super::super::dynamic::SourceFingerprint {
                            generation: 1,
                            source: ring.id.as_str().into(),
                            query: None,
                        },
                        entries,
                    },
                )
            })
            .collect();
        let first = project_menu_frame(&menu, BTreeMap::new(), &dynamic, 0, 1);
        assert!(first.page_count > 1);
        for (definition, projected) in menu.rings.iter().zip(&first.menu.rings) {
            assert!(projected.cells.len() <= ring_accessible_capacity(definition));
            assert!(projected.cells.iter().any(|cell| {
                cell.id.as_str() == format!("__radial_page_next:{}", definition.id.as_str())
            }));
        }
        let mut seen = std::collections::BTreeSet::new();
        for page in 0..first.page_count {
            let frame = project_menu_frame(&menu, BTreeMap::new(), &dynamic, page, 1);
            let mut document = RadialDocument::starter();
            document.menus[0] = frame.menu.clone();
            crate::radial::validation::validate(&document).unwrap();
            let layout = crate::radial::geometry::layout_menu(
                &frame.menu,
                crate::radial::geometry::PhysicalPoint { x: 500.0, y: 500.0 },
                crate::radial::geometry::PhysicalRect {
                    min: crate::radial::geometry::PhysicalPoint { x: 0.0, y: 0.0 },
                    max: crate::radial::geometry::PhysicalPoint {
                        x: 1000.0,
                        y: 1000.0,
                    },
                },
                crate::radial::geometry::ScaleFactor::new(1.0).unwrap(),
                0.5,
            )
            .unwrap();
            assert_eq!(
                layout.cells.len(),
                frame
                    .menu
                    .rings
                    .iter()
                    .map(|r| r.cells.len())
                    .sum::<usize>()
            );
            for (index, left) in layout.cells.iter().enumerate() {
                for right in layout.cells.iter().skip(index + 1) {
                    let (
                        crate::radial::geometry::HitShape::Circle {
                            center: left_center,
                            radius: left_radius,
                        },
                        crate::radial::geometry::HitShape::Circle {
                            center: right_center,
                            radius: right_radius,
                        },
                    ) = (&left.shape, &right.shape)
                    else {
                        continue;
                    };
                    let dx = left_center.x - right_center.x;
                    let dy = left_center.y - right_center.y;
                    assert!(
                        (dx * dx + dy * dy).sqrt() + 0.001 >= left_radius + right_radius,
                        "projected cells {} and {} overlap",
                        left.cell_id,
                        right.cell_id
                    );
                }
            }
            seen.extend(
                frame
                    .menu
                    .rings
                    .iter()
                    .flat_map(|ring| &ring.cells)
                    .filter(|cell| matches!(cell.content, CellContent::Dynamic { .. }))
                    .map(|cell| cell.label.clone()),
            );
        }
        assert_eq!(seen.len(), 50);
    }

    #[test]
    fn effective_item_size_and_radius_scale_bound_dynamic_pagination() {
        let mut document = RadialDocument::starter();
        document.menus[0].rings[0].cells.truncate(1);
        document.menus[0].rings[0].cells[0].content = CellContent::Dynamic {
            source: super::super::model::DynamicSource::Favorites,
        };
        document.menus[0].style.values.geometry.item_size =
            super::super::model::Override::Value(88.0);
        let menu = &document.menus[0];
        let source = menu.rings[0].cells[0].id.clone();
        let entries = (0..18)
            .map(|index| FrozenRadialEntry {
                id: FrozenEntryId(format!("styled:{index}")),
                label: format!("Styled {index}"),
                kind: super::super::dynamic::FrozenEntryKind::Action,
                binding: Some(FrozenBinding::Stable(ActionBinding::Contextual {
                    selector: TargetSelector::CapturedForeground,
                    action_id: action_ids::WINDOW_ACTIVATE,
                })),
                availability: FrozenAvailability::Available,
                history_query: String::new(),
                requirement: InteractionRequirement::None,
            })
            .collect();
        let dynamic = [(
            source,
            FrozenDynamicFrame {
                fingerprint: super::super::dynamic::SourceFingerprint {
                    generation: 1,
                    source: "styled".into(),
                    query: None,
                },
                entries,
            },
        )]
        .into_iter()
        .collect();
        let unstyled = project_menu_frame(menu, BTreeMap::new(), &dynamic, 0, 12);
        let style = super::super::skin::compile_menu_tree(&document, menu).unwrap();
        let styled =
            project_menu_frame_with_style(menu, BTreeMap::new(), &dynamic, 0, Some(&style));
        assert!(styled.page_count > unstyled.page_count);
        let capacity = ring_accessible_capacity_for(92.0, 44.0, menu.rings[0].gap);
        assert!(styled.menu.rings[0].cells.len() <= capacity);
        assert!(styled.menu.rings[0].cells.iter().any(|cell| {
            matches!(
                cell.content,
                CellContent::Control {
                    control: super::super::model::Control::NextPage
                }
            )
        }));
    }

    #[test]
    fn runtime_frozen_binding_revalidates_identity_and_content_not_position() {
        let selected = Action {
            label: "Editor".into(),
            desc: "Windows".into(),
            action: "window:switch:44".into(),
            args: None,
        };
        let binding = FrozenBinding::Runtime {
            target: ActionTarget::Window { hwnd: 44 },
            selected_action: selected.clone(),
            action_id: action_ids::WINDOW_ACTIVATE,
            identity: None,
        };
        let catalog = PersistedActionCatalog::new(vec![ResolvedActionTarget {
            target: ActionTarget::Window { hwnd: 44 },
            selected_action: selected.clone(),
            custom_action_index: None,
        }]);
        let registry = UniversalActionRegistry;
        assert!(
            RadialBindingResolver {
                catalog: &catalog,
                registry: &registry,
            }
            .resolve_frozen(&binding, &context(), "")
            .is_ok()
        );

        let mutated = PersistedActionCatalog::new(vec![ResolvedActionTarget {
            target: ActionTarget::Window { hwnd: 44 },
            selected_action: Action {
                label: "Different window".into(),
                ..selected
            },
            custom_action_index: None,
        }]);
        assert!(matches!(
            RadialBindingResolver {
                catalog: &mutated,
                registry: &registry,
            }
            .resolve_frozen(&binding, &context(), ""),
            Err(BindingUnavailable::ContextActionMissing { .. })
        ));
        assert!(matches!(
            RadialBindingResolver {
                catalog: &PersistedActionCatalog::default(),
                registry: &registry,
            }
            .resolve_frozen(&binding, &context(), ""),
            Err(BindingUnavailable::ContextActionMissing { .. })
        ));
    }

    #[test]
    fn ephemeral_clipboard_list_browser_and_window_targets_revalidate_exactly() {
        let cases = [
            (
                ActionTarget::ClipboardEntry { index: 3 },
                ActionTarget::ClipboardEntry { index: 4 },
                action_ids::CLIPBOARD_EDIT,
            ),
            (
                ActionTarget::Todo { index: 5 },
                ActionTarget::Todo { index: 6 },
                action_ids::TODO_EDIT,
            ),
            (
                ActionTarget::BrowserTab {
                    runtime_id: vec![7, 8],
                    url: Some("https://example.test/a".into()),
                },
                ActionTarget::BrowserTab {
                    runtime_id: vec![7, 9],
                    url: Some("https://example.test/a".into()),
                },
                action_ids::BROWSER_TAB_ACTIVATE,
            ),
            (
                ActionTarget::Window { hwnd: 10 },
                ActionTarget::Window { hwnd: 11 },
                action_ids::WINDOW_ACTIVATE,
            ),
        ];
        let registry = UniversalActionRegistry;

        for (target, churned_target, action_id) in cases {
            assert_eq!(target.persistent_ref(), None, "ephemeral identity leaked");
            let selected_action = Action {
                label: "Captured target".into(),
                desc: "Runtime".into(),
                action: "runtime:captured".into(),
                args: None,
            };
            let binding = FrozenBinding::Runtime {
                target: target.clone(),
                selected_action: selected_action.clone(),
                action_id: action_id.clone(),
                identity: None,
            };
            let exact = PersistedActionCatalog::new(vec![ResolvedActionTarget {
                target: target.clone(),
                selected_action: selected_action.clone(),
                custom_action_index: None,
            }]);
            assert!(
                RadialBindingResolver {
                    catalog: &exact,
                    registry: &registry,
                }
                .resolve_frozen(&binding, &context(), "captured query")
                .is_ok(),
                "exact target should remain resolvable: {target:?}"
            );

            let churned = PersistedActionCatalog::new(vec![ResolvedActionTarget {
                target: churned_target,
                selected_action: selected_action.clone(),
                custom_action_index: None,
            }]);
            assert!(matches!(
                RadialBindingResolver {
                    catalog: &churned,
                    registry: &registry,
                }
                .resolve_frozen(&binding, &context(), "captured query"),
                Err(BindingUnavailable::ContextActionMissing { action_id: missing })
                    if missing == action_id
            ));

            let changed_content = PersistedActionCatalog::new(vec![ResolvedActionTarget {
                target,
                selected_action: Action {
                    label: "Replacement target".into(),
                    ..selected_action
                },
                custom_action_index: None,
            }]);
            assert!(matches!(
                RadialBindingResolver {
                    catalog: &changed_content,
                    registry: &registry,
                }
                .resolve_frozen(&binding, &context(), "captured query"),
                Err(BindingUnavailable::ContextActionMissing { .. })
            ));
        }
    }
}
