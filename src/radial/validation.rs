use super::model::*;
use crate::universal_actions::{PersistableActionTargetRef, PersistedUniversalActionRef};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationIssue {
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationErrors(pub Vec<ValidationIssue>);

impl std::fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "radial configuration has {} validation error(s)",
            self.0.len()
        )
    }
}
impl std::error::Error for ValidationErrors {}

pub fn validate(document: &RadialDocument) -> Result<(), ValidationErrors> {
    let mut errors = Vec::new();
    if document.schema_version != CURRENT_SCHEMA_VERSION {
        errors.push(issue(
            "schema_version",
            format!(
                "unsupported schema version {}; expected {CURRENT_SCHEMA_VERSION}",
                document.schema_version
            ),
        ));
    }
    bounded(
        "menus",
        document.menus.len(),
        limits::MAX_MENUS,
        &mut errors,
    );
    bounded(
        "skins",
        document.skins.len(),
        limits::MAX_SKINS,
        &mut errors,
    );
    bounded(
        "context_rules",
        document.context_rules.len(),
        limits::MAX_CONTEXT_RULES,
        &mut errors,
    );
    bounded(
        "custom_triggers",
        document.custom_triggers.len(),
        limits::MAX_CUSTOM_TRIGGERS,
        &mut errors,
    );

    let menu_ids = unique_ids(
        document.menus.iter().map(|v| (&v.id, v.id.as_str())),
        "menus",
        &mut errors,
    );
    let skin_ids = unique_ids(
        document.skins.iter().map(|v| (&v.id, v.id.as_str())),
        "skins",
        &mut errors,
    );
    unique_ids(
        document
            .context_rules
            .iter()
            .map(|v| (&v.id, v.id.as_str())),
        "context_rules",
        &mut errors,
    );
    unique_ids(
        document
            .custom_triggers
            .iter()
            .map(|v| (&v.id, v.id.as_str())),
        "custom_triggers",
        &mut errors,
    );
    if !menu_ids.contains(&document.default_menu_id) {
        errors.push(issue(
            "default_menu_id",
            format!("missing menu {}", document.default_menu_id),
        ));
    }

    let mut graph: BTreeMap<MenuId, Vec<MenuId>> = BTreeMap::new();
    let mut total_cells = 0;
    for (mi, menu) in document.menus.iter().enumerate() {
        let path = format!("menus[{mi}]");
        if menu.hover_dwell_ms.is_some_and(|value| value > 60_000) {
            errors.push(issue(
                format!("{path}.hover_dwell_ms"),
                "hover dwell exceeds 60 seconds",
            ));
        }
        if menu.name.trim().is_empty() {
            errors.push(issue(format!("{path}.name"), "name cannot be empty"));
        }
        if !skin_ids.contains(&menu.skin_id) {
            errors.push(issue(
                format!("{path}.skin_id"),
                format!("missing skin {}", menu.skin_id),
            ));
        }
        if let Some(binding) = &menu.center_action {
            validate_binding(binding, &format!("{path}.center_action"), &mut errors);
            validate_keep_open(
                document,
                menu,
                menu.center_primary_after_action,
                binding,
                &format!("{path}.center_action"),
                &mut errors,
            );
        }
        if menu.center_control == Some(Control::Drag) && menu.center_action.is_some() {
            errors.push(issue(
                format!("{path}.center_action"),
                "primary center action is unreachable while the center control is Drag",
            ));
        }
        if let Some(binding) = &menu.center_secondary_action {
            validate_binding(
                binding,
                &format!("{path}.center_secondary_action"),
                &mut errors,
            );
            validate_keep_open(
                document,
                menu,
                menu.center_secondary_after_action,
                binding,
                &format!("{path}.center_secondary_action"),
                &mut errors,
            );
        }
        if let Some(binding) = &menu.background_action {
            validate_binding(binding, &format!("{path}.background_action"), &mut errors);
            validate_keep_open(
                document,
                menu,
                menu.background_primary_after_action,
                binding,
                &format!("{path}.background_action"),
                &mut errors,
            );
        }
        if let Some(binding) = &menu.background_secondary_action {
            validate_binding(
                binding,
                &format!("{path}.background_secondary_action"),
                &mut errors,
            );
            validate_keep_open(
                document,
                menu,
                menu.background_secondary_after_action,
                binding,
                &format!("{path}.background_secondary_action"),
                &mut errors,
            );
        }
        finite_range(
            format!("{path}.center_radius"),
            menu.center_radius,
            4.0,
            512.0,
            &mut errors,
        );
        bounded(
            format!("{path}.rings"),
            menu.rings.len(),
            limits::MAX_RINGS_PER_MENU,
            &mut errors,
        );
        if menu.rings.is_empty() {
            errors.push(issue(
                format!("{path}.rings"),
                "menu must contain at least one ring",
            ));
        }
        unique_ids(
            menu.rings.iter().map(|v| (&v.id, v.id.as_str())),
            &format!("{path}.rings"),
            &mut errors,
        );
        for (left_index, left) in menu.rings.iter().enumerate() {
            for (right_index, right) in menu.rings.iter().enumerate().skip(left_index + 1) {
                let radial_separation = (left.radius - right.radius).abs();
                let required = left.cell_radius + right.cell_radius + left.gap.max(right.gap);
                if radial_separation + 0.001 < required {
                    errors.push(issue(
                        format!("{path}.rings[{right_index}].radius"),
                        format!(
                            "ring overlaps rings[{left_index}]; radial separation must be at least {required}"
                        ),
                    ));
                }
            }
        }
        let mut cell_ids = BTreeSet::new();
        for (ri, ring) in menu.rings.iter().enumerate() {
            let rp = format!("{path}.rings[{ri}]");
            finite_range(
                format!("{rp}.radius"),
                ring.radius,
                1.0,
                8192.0,
                &mut errors,
            );
            finite_range(
                format!("{rp}.cell_radius"),
                ring.cell_radius,
                4.0,
                512.0,
                &mut errors,
            );
            finite_range(
                format!("{rp}.rotation_degrees"),
                ring.rotation_degrees,
                -36000.0,
                36000.0,
                &mut errors,
            );
            finite_range(format!("{rp}.gap"), ring.gap, 0.0, 512.0, &mut errors);
            bounded(
                format!("{rp}.cells"),
                ring.cells.len(),
                limits::MAX_CELLS_PER_RING,
                &mut errors,
            );
            let static_cells = ring
                .cells
                .iter()
                .filter(|cell| !matches!(cell.content, CellContent::Dynamic { .. }))
                .count();
            if ring
                .cells
                .iter()
                .any(|cell| matches!(cell.content, CellContent::Dynamic { .. }))
                && crate::radial::bindings::ring_accessible_capacity(ring)
                    .saturating_sub(static_cells)
                    < 3
            {
                errors.push(issue(
                    format!("{rp}.cells"),
                    "dynamic ring geometry must leave room for an entry and paging controls",
                ));
            }
            total_cells += ring.cells.len();
            if ring.radius <= menu.center_radius + ring.cell_radius {
                errors.push(issue(
                    format!("{rp}.radius"),
                    "ring overlaps the center control",
                ));
            }
            let n = ring.cells.len();
            if menu.layout == LayoutKind::CircularCells && n >= 2 {
                let spacing = 2.0 * ring.radius * (std::f32::consts::PI / n as f32).sin();
                if spacing + 0.001 < 2.0 * ring.cell_radius + ring.gap {
                    errors.push(issue(
                        format!("{rp}.cells"),
                        "adjacent circular cells overlap or violate the requested gap",
                    ));
                }
            }
            for (ci, cell) in ring.cells.iter().enumerate() {
                let cp = format!("{rp}.cells[{ci}]");
                if cell.id.as_str().trim().is_empty() || !cell_ids.insert(cell.id.clone()) {
                    errors.push(issue(
                        format!("{cp}.id"),
                        "cell ID is empty or duplicated within its menu",
                    ));
                }
                let generated_page_control = [
                    ("__radial_page_previous:", Control::PreviousPage),
                    ("__radial_page_next:", Control::NextPage),
                ]
                .into_iter()
                .any(|(prefix, control)| {
                    cell.id.as_str().strip_prefix(prefix) == Some(ring.id.as_str())
                        && matches!(&cell.content, CellContent::Control { control: actual } if *actual == control)
                });
                if (cell.id.as_str() == "__center"
                    || cell.id.as_str() == "__background"
                    || cell.id.as_str().starts_with("__radial_page_previous")
                    || cell.id.as_str().starts_with("__radial_page_next"))
                    && !generated_page_control
                {
                    errors.push(issue(
                        format!("{cp}.id"),
                        "cell ID is reserved by the runtime",
                    ));
                }
                validate_content(
                    &cell.content,
                    &cp,
                    &menu_ids,
                    &mut graph,
                    &menu.id,
                    &mut errors,
                );
                if let CellContent::Action { binding } = &cell.content {
                    validate_keep_open(
                        document,
                        menu,
                        cell.after_action,
                        binding,
                        &cp,
                        &mut errors,
                    );
                }
                let mut gestures = BTreeSet::new();
                for (bi, binding) in cell.alternate_clicks.iter().enumerate() {
                    if !gestures.insert(binding.gesture) {
                        errors.push(issue(
                            format!("{cp}.alternate_clicks[{bi}].gesture"),
                            "duplicate click gesture",
                        ));
                    }
                    validate_binding(
                        &binding.action,
                        &format!("{cp}.alternate_clicks[{bi}]"),
                        &mut errors,
                    );
                    validate_keep_open(
                        document,
                        menu,
                        binding.after_action,
                        &binding.action,
                        &format!("{cp}.alternate_clicks[{bi}]"),
                        &mut errors,
                    );
                }
            }
        }
    }
    bounded(
        "all menu cells",
        total_cells,
        limits::MAX_TOTAL_CELLS,
        &mut errors,
    );
    for (si, skin) in document.skins.iter().enumerate() {
        finite_range(
            format!("skins[{si}].scale"),
            skin.scale,
            0.25,
            4.0,
            &mut errors,
        );
    }
    for (ri, rule) in document.context_rules.iter().enumerate() {
        if !menu_ids.contains(&rule.menu_id) {
            errors.push(issue(
                format!("context_rules[{ri}].menu_id"),
                "missing menu",
            ));
        }
        for (field, value) in [
            ("process_name", rule.process_name.as_deref()),
            (
                "window_title_contains",
                rule.window_title_contains.as_deref(),
            ),
            ("monitor_id", rule.monitor_id.as_deref()),
        ] {
            if value.is_some_and(|value| value.len() > 512) {
                errors.push(issue(
                    format!("context_rules[{ri}].{field}"),
                    "context matcher exceeds 512 bytes",
                ));
            }
        }
    }
    let mut chords = BTreeMap::<String, usize>::new();
    for (ti, trigger) in document.custom_triggers.iter().enumerate() {
        match crate::hotkey::parse_hotkey(&trigger.chord) {
            Some(parsed) => {
                let canonical = format!(
                    "{:?}:{}:{}:{}:{}:{}",
                    parsed.key, parsed.ctrl, parsed.shift, parsed.alt, parsed.alt_gr, parsed.win
                );
                if let Some(previous) = chords.insert(canonical, ti) {
                    errors.push(issue(
                        format!("custom_triggers[{ti}].chord"),
                        format!("conflicts with custom_triggers[{previous}]"),
                    ));
                }
            }
            None => errors.push(issue(
                format!("custom_triggers[{ti}].chord"),
                "invalid hotkey chord",
            )),
        }
        if !menu_ids.contains(&trigger.menu_id) {
            errors.push(issue(
                format!("custom_triggers[{ti}].menu_id"),
                "missing menu",
            ));
        }
    }
    validate_graph(document, &graph, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ValidationErrors(errors))
    }
}

fn validate_keep_open(
    document: &RadialDocument,
    menu: &MenuDefinition,
    policy: AfterActionPolicy,
    binding: &ActionBinding,
    path: &str,
    errors: &mut Vec<ValidationIssue>,
) {
    if effective_after_action(document, menu, policy) != AfterActionPolicy::KeepOpen {
        return;
    }
    let ActionBinding::Persisted { action } = binding else {
        return;
    };
    let requirement = match action.target.as_ref() {
        Some(
            PersistableActionTargetRef::LegacyAction { action }
            | PersistableActionTargetRef::CustomAction { action },
        ) => crate::commands::parse_action(action)
            .map(|command| crate::radial::handoff::command_requirement(&command))
            .unwrap_or(crate::radial::handoff::InteractionRequirement::ExternalInput),
        _ => match crate::radial::handoff::action_id_requirement(&action.action_id) {
            Some(requirement) => requirement,
            None => return,
        },
    };
    if requirement != crate::radial::handoff::InteractionRequirement::None {
        errors.push(issue(
            format!("{path}.after_action"),
            format!("KeepOpen is incompatible with {requirement:?}"),
        ));
    }
}

fn validate_content(
    content: &CellContent,
    path: &str,
    menu_ids: &BTreeSet<MenuId>,
    graph: &mut BTreeMap<MenuId, Vec<MenuId>>,
    owner: &MenuId,
    errors: &mut Vec<ValidationIssue>,
) {
    match content {
        CellContent::Action { binding } => validate_binding(binding, path, errors),
        CellContent::Submenu { menu_id } => {
            if !menu_ids.contains(menu_id) {
                errors.push(issue(
                    format!("{path}.content.menu_id"),
                    format!("missing menu {menu_id}"),
                ));
            }
            graph
                .entry(owner.clone())
                .or_default()
                .push(menu_id.clone());
        }
        CellContent::Dynamic {
            source:
                DynamicSource::LauncherResults { max_items }
                | DynamicSource::LauncherQuery { max_items, .. },
        } if *max_items == 0 || *max_items > limits::MAX_CELLS_PER_RING => errors.push(issue(
            format!("{path}.content.max_items"),
            "dynamic result limit is out of range",
        )),
        CellContent::Dynamic {
            source: DynamicSource::LauncherQuery { query, .. },
        } if query.len() > 512 => errors.push(issue(
            format!("{path}.content.query"),
            "dynamic launcher query is too long",
        )),
        _ => {}
    }
}

fn validate_binding(binding: &ActionBinding, path: &str, errors: &mut Vec<ValidationIssue>) {
    let (action_id, target) = match binding {
        ActionBinding::Persisted {
            action: PersistedUniversalActionRef { action_id, target },
        } => (action_id.as_str(), target.as_ref()),
        ActionBinding::Contextual { action_id, .. } => (action_id.as_str(), None),
    };
    if action_id.trim().is_empty() {
        errors.push(issue(
            format!("{path}.action_id"),
            "action ID cannot be empty",
        ));
    }
    if let Some(target) = target {
        let valid = match target {
            PersistableActionTargetRef::LegacyAction { action }
            | PersistableActionTargetRef::CustomAction { action } => {
                !action.action.trim().is_empty()
            }
            PersistableActionTargetRef::Folder { path }
            | PersistableActionTargetRef::Tempfile { path } => !path.trim().is_empty(),
            PersistableActionTargetRef::Bookmark { url } => !url.trim().is_empty(),
            PersistableActionTargetRef::Snippet { alias } => !alias.trim().is_empty(),
            PersistableActionTargetRef::Note { slug } => !slug.trim().is_empty(),
            PersistableActionTargetRef::MkMacro { id } => *id != 0,
        };
        if !valid {
            errors.push(issue(
                format!("{path}.target"),
                "persistent target identity is empty or invalid",
            ));
        }
    }
}

fn validate_graph(
    document: &RadialDocument,
    graph: &BTreeMap<MenuId, Vec<MenuId>>,
    errors: &mut Vec<ValidationIssue>,
) {
    fn depth(
        node: &MenuId,
        graph: &BTreeMap<MenuId, Vec<MenuId>>,
        path: &mut Vec<MenuId>,
        memo: &mut BTreeMap<MenuId, usize>,
        errors: &mut Vec<ValidationIssue>,
    ) -> usize {
        if let Some(depth) = memo.get(node) {
            return *depth;
        }
        if let Some(index) = path.iter().position(|id| id == node) {
            let mut cycle = path[index..]
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            cycle.push(node.to_string());
            errors.push(issue(
                "menus",
                format!("submenu cycle: {}", cycle.join(" -> ")),
            ));
            return 0;
        }
        path.push(node.clone());
        let mut child_depth = 0;
        if let Some(children) = graph.get(node) {
            for child in children {
                child_depth = child_depth.max(depth(child, graph, path, memo, errors));
            }
        }
        path.pop();
        let result = child_depth
            .saturating_add(1)
            .min(limits::MAX_SUBMENU_DEPTH + 1);
        memo.insert(node.clone(), result);
        result
    }
    let mut memo = BTreeMap::new();
    for menu in &document.menus {
        let depth = depth(&menu.id, graph, &mut Vec::new(), &mut memo, errors);
        if depth > limits::MAX_SUBMENU_DEPTH {
            errors.push(issue(
                "menus",
                format!(
                    "submenu depth exceeds {} at {}",
                    limits::MAX_SUBMENU_DEPTH,
                    menu.id
                ),
            ));
        }
    }
}

fn unique_ids<'a, T: Clone + Ord + 'a>(
    items: impl Iterator<Item = (&'a T, &'a str)>,
    path: &str,
    errors: &mut Vec<ValidationIssue>,
) -> BTreeSet<T> {
    let mut ids = BTreeSet::new();
    for (index, (id, text)) in items.enumerate() {
        if text.trim().is_empty() || !ids.insert(id.clone()) {
            errors.push(issue(
                format!("{path}[{index}].id"),
                "ID is empty or duplicated",
            ));
        }
    }
    ids
}
fn finite_range(path: String, value: f32, min: f32, max: f32, errors: &mut Vec<ValidationIssue>) {
    if !value.is_finite() || value < min || value > max {
        errors.push(issue(path, format!("must be finite and in {min}..={max}")));
    }
}
fn bounded(path: impl Into<String>, actual: usize, max: usize, errors: &mut Vec<ValidationIssue>) {
    if actual > max {
        errors.push(issue(path, format!("contains {actual}; limit is {max}")));
    }
}
fn issue(path: impl Into<String>, message: impl Into<String>) -> ValidationIssue {
    ValidationIssue {
        path: path.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> RadialDocument {
        RadialDocument::starter()
    }

    #[test]
    fn starter_validates() {
        validate(&valid()).unwrap();
    }

    #[test]
    fn rejects_dynamic_ring_geometry_that_cannot_fit_paging_controls() {
        let mut document = valid();
        document.menus[0].rings[0].radius = 70.0;
        document.menus[0].rings[0].cell_radius = 28.0;
        document.menus[0].rings[0].gap = 4.0;
        for cell in &mut document.menus[0].rings[0].cells[1..] {
            cell.content = CellContent::Spacer;
        }
        for index in 0..3 {
            let mut spacer = document.menus[0].rings[0].cells[1].clone();
            spacer.id = CellId::new(format!("spacer-{index}"));
            document.menus[0].rings[0].cells.push(spacer);
        }
        let errors = validate(&document).unwrap_err();
        assert!(errors.0.iter().any(|issue| {
            issue.path.ends_with("rings[0].cells") && issue.message.contains("paging controls")
        }));
    }

    #[test]
    fn drag_center_rejects_an_unreachable_primary_action_but_starter_is_valid() {
        let mut document = valid();
        document.menus[0].center_action = Some(ActionBinding::Contextual {
            selector: TargetSelector::CapturedForeground,
            action_id: crate::universal_actions::action_ids::WINDOW_ACTIVATE,
        });
        let errors = validate(&document).unwrap_err();
        assert!(errors.0.iter().any(|issue| {
            issue.path.ends_with("center_action") && issue.message.contains("unreachable")
        }));
        document.menus[0].center_action = None;
        validate(&document).unwrap();
    }
    #[test]
    fn rejects_duplicates_missing_refs_and_bad_numbers() {
        let mut d = valid();
        d.menus[0].rings[0].id = RingId::new("");
        d.menus[0].skin_id = SkinId::new("missing");
        d.menus[0].center_radius = f32::NAN;
        let e = validate(&d).unwrap_err();
        assert!(e.0.iter().any(|e| e.path.ends_with("skin_id")));
        assert!(e.0.iter().any(|e| e.message.contains("finite")));
        assert!(e.0.iter().any(|e| e.message.contains("empty")));
    }
    #[test]
    fn reports_cycle_path_and_excessive_depth() {
        let mut d = valid();
        let original = d.menus[0].clone();
        d.menus[0].rings[0].cells[0].content = CellContent::Submenu {
            menu_id: MenuId::new("child"),
        };
        let mut child = original;
        child.id = MenuId::new("child");
        child.rings[0].cells[0].content = CellContent::Submenu {
            menu_id: MenuId::new("starter"),
        };
        d.menus.push(child);
        let e = validate(&d).unwrap_err();
        assert!(
            e.0.iter()
                .any(|e| e.message.contains("starter -> child -> starter"))
        );
    }

    #[test]
    fn rejects_excessive_depth_and_resource_count() {
        let template = valid().menus[0].clone();
        let mut d = valid();
        d.menus.clear();
        for index in 0..=limits::MAX_SUBMENU_DEPTH {
            let mut menu = template.clone();
            menu.id = MenuId::new(format!("menu-{index}"));
            if index < limits::MAX_SUBMENU_DEPTH {
                menu.rings[0].cells[0].content = CellContent::Submenu {
                    menu_id: MenuId::new(format!("menu-{}", index + 1)),
                };
            }
            d.menus.push(menu);
        }
        d.default_menu_id = MenuId::new("menu-0");
        assert!(
            validate(&d)
                .unwrap_err()
                .0
                .iter()
                .any(|error| error.message.contains("depth exceeds"))
        );

        d.menus = (0..=limits::MAX_MENUS)
            .map(|index| {
                let mut menu = template.clone();
                menu.id = MenuId::new(format!("flat-{index}"));
                menu
            })
            .collect();
        d.default_menu_id = MenuId::new("flat-0");
        assert!(
            validate(&d)
                .unwrap_err()
                .0
                .iter()
                .any(|error| error.message.contains("limit is 256"))
        );
    }
    #[test]
    fn catches_trigger_conflicts_and_invalid_targets() {
        let mut d = valid();
        d.custom_triggers = vec![
            TriggerDefinition {
                id: TriggerId::new("a"),
                chord: "Shift+Control+F2".into(),
                menu_id: MenuId::new("starter"),
                scope: TriggerScope::MenuLocal,
            },
            TriggerDefinition {
                id: TriggerId::new("b"),
                chord: "f2 + ctrl + shift".into(),
                menu_id: MenuId::new("starter"),
                scope: TriggerScope::MenuLocal,
            },
        ];
        let e = validate(&d).unwrap_err();
        assert!(e.0.iter().any(|e| e.message.contains("conflicts")));

        d.custom_triggers.clear();
        d.menus[0].rings[0].cells[0].content = CellContent::Action {
            binding: ActionBinding::Persisted {
                action: PersistedUniversalActionRef {
                    target: Some(PersistableActionTargetRef::Note {
                        slug: String::new(),
                    }),
                    action_id: crate::universal_actions::ActionId::new("note.open"),
                },
            },
        };
        assert!(
            validate(&d)
                .unwrap_err()
                .0
                .iter()
                .any(|error| error.message.contains("target identity"))
        );
    }

    #[test]
    fn keep_open_incompatibility_is_reported_for_secondary_special_surfaces() {
        let mut document = valid();
        let action = crate::actions::Action {
            label: "Draw".into(),
            desc: String::new(),
            action: "screen_draw:start".into(),
            args: None,
        };
        document.menus[0].center_secondary_action = Some(ActionBinding::Persisted {
            action: PersistedUniversalActionRef {
                target: Some(PersistableActionTargetRef::CustomAction { action }),
                action_id: crate::universal_actions::ActionId::new("result.execute"),
            },
        });
        document.menus[0].center_secondary_after_action = AfterActionPolicy::KeepOpen;
        let errors = validate(&document).unwrap_err();
        assert!(errors.0.iter().any(|issue| {
            issue.path.contains("center_secondary_action")
                && issue.message.contains("ExclusiveCapture")
        }));
    }

    #[test]
    fn primary_special_surface_policy_is_validated_independently() {
        let mut document = valid();
        document.menus[0].center_control = None;
        let binding = ActionBinding::Persisted {
            action: PersistedUniversalActionRef {
                target: Some(PersistableActionTargetRef::Note {
                    slug: "daily".into(),
                }),
                action_id: crate::universal_actions::action_ids::NOTE_OPEN_NOTEPAD,
            },
        };
        document.menus[0].center_action = Some(binding.clone());
        document.menus[0].center_primary_after_action = AfterActionPolicy::CloseTree;
        document.menus[0].background_action = Some(binding);
        document.menus[0].background_primary_after_action = AfterActionPolicy::KeepOpen;
        let errors = validate(&document).unwrap_err();
        assert!(errors.0.iter().any(|issue| {
            issue.path.contains("background_action.after_action")
                && issue.message.contains("ExternalInput")
        }));
        assert!(
            !errors
                .0
                .iter()
                .any(|issue| issue.path.contains("center_action.after_action"))
        );
    }

    #[test]
    fn shared_submenu_dag_is_memoized_and_bounded_by_edges() {
        let mut d = valid();
        let template = d.menus.remove(0);
        for level in 0..limits::MAX_SUBMENU_DEPTH {
            let mut menu = template.clone();
            menu.id = MenuId::new(format!("shared-{level}"));
            menu.layout = LayoutKind::Wedges;
            if level + 1 < limits::MAX_SUBMENU_DEPTH {
                menu.rings[0].cells = (0..limits::MAX_CELLS_PER_RING)
                    .map(|cell| {
                        let mut definition = template.rings[0].cells[0].clone();
                        definition.id = CellId::new(format!("cell-{level}-{cell}"));
                        definition.content = CellContent::Submenu {
                            menu_id: MenuId::new(format!("shared-{}", level + 1)),
                        };
                        definition
                    })
                    .collect();
            }
            d.menus.push(menu);
        }
        d.default_menu_id = MenuId::new("shared-0");
        validate(&d).unwrap();
    }
}
