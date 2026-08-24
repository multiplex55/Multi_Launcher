use super::MkMacroDialog;
use crate::mkmacro::{
    MkStep, MonitorValidation, ValidationContext, validate_document_with_context,
};
use std::collections::{BTreeSet, HashMap};

#[derive(Default, Debug, Clone)]
pub struct Selection {
    pub ids: BTreeSet<u64>,
    anchor: Option<usize>,
}
impl Selection {
    pub fn clear(&mut self) {
        self.ids.clear();
        self.anchor = None;
    }
    pub fn click(&mut self, rows: &[u64], index: usize, ctrl: bool, shift: bool) {
        if shift {
            let a = self.anchor.unwrap_or(index);
            if !ctrl {
                self.ids.clear();
            }
            for id in &rows[a.min(index)..=a.max(index)] {
                self.ids.insert(*id);
            }
        } else {
            if !ctrl {
                self.ids.clear();
            }
            if ctrl && self.ids.contains(&rows[index]) {
                self.ids.remove(&rows[index]);
            } else {
                self.ids.insert(rows[index]);
            }
            self.anchor = Some(index);
        }
    }
}
pub fn duplicate_steps_with_ids(steps: &mut Vec<MkStep>, ids: &BTreeSet<u64>) -> BTreeSet<u64> {
    let mut copies: Vec<_> = steps
        .iter()
        .filter(|s| ids.contains(&s.id))
        .cloned()
        .collect();
    for s in &mut copies {
        s.id = 0;
    }
    let insert_at = steps
        .iter()
        .rposition(|s| ids.contains(&s.id))
        .map_or(steps.len(), |i| i + 1);
    steps.splice(insert_at..insert_at, copies);
    let mut d = crate::mkmacro::MkMacroDocument {
        settings: Default::default(),
        schema_version: crate::mkmacro::SCHEMA_VERSION,
        macros: vec![crate::mkmacro::MkMacro {
            id: 1,
            name: "draft".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            playback: Default::default(),
            steps: steps.clone(),
            image_assets: vec![],
        }],
    };
    crate::mkmacro::repair_ids(&mut d);
    *steps = d.macros.remove(0).steps;
    steps[insert_at..]
        .iter()
        .take(ids.len())
        .map(|s| s.id)
        .collect()
}
pub fn duplicate_steps(steps: &mut Vec<MkStep>, ids: &BTreeSet<u64>) {
    let _ = duplicate_steps_with_ids(steps, ids);
}
pub fn move_steps(steps: &mut [MkStep], ids: &BTreeSet<u64>, down: bool) {
    if down {
        for i in (0..steps.len().saturating_sub(1)).rev() {
            if ids.contains(&steps[i].id) && !ids.contains(&steps[i + 1].id) {
                steps.swap(i, i + 1);
            }
        }
    } else {
        for i in 1..steps.len() {
            if ids.contains(&steps[i].id) && !ids.contains(&steps[i - 1].id) {
                steps.swap(i, i - 1);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Command {
    Edit(u64),
    Duplicate,
    Toggle,
    Up,
    Down,
    Delete,
    DeleteRow(u64),
    DeleteBlock(u64),
    UnwrapBlock(u64),
    InsertAbove(u64),
    InsertBelow(u64),
    RunOne,
    RunFrom,
}
const MIN_TABLE_VIEWPORT_HEIGHT: f32 = 48.0;

/// Uses all height assigned to the step area; the number of rows is deliberately
/// irrelevant because rows belong to the table's scroll area.
fn table_viewport_height(available_height: f32) -> f32 {
    available_height.max(MIN_TABLE_VIEWPORT_HEIGHT)
}

pub(super) fn show(ui: &mut eframe::egui::Ui, d: &mut MkMacroDialog) {
    let Some(mid) = d.selected_macro_id else {
        ui.label("Select a macro");
        return;
    };
    let monitor_result = crate::mkmacro::monitor_descriptors();
    let monitor_validation = match &monitor_result {
        Ok(descriptors) => MonitorValidation::Available(descriptors),
        Err(_) => MonitorValidation::EnumerationFailed,
    };
    let asset_root = d.store.asset_root();
    let diagnostics = validate_document_with_context(
        &d.draft,
        ValidationContext {
            asset_root: Some(&asset_root),
            monitors: monitor_validation,
        },
    );
    let mut row_diagnostics = HashMap::<u64, Vec<_>>::new();
    for diagnostic in diagnostics.iter().filter(|x| x.macro_id == mid) {
        if let Some(step_id) = diagnostic.step_id {
            row_diagnostics.entry(step_id).or_default().push(diagnostic);
        }
    }
    for diagnostic in diagnostics
        .iter()
        .filter(|x| x.macro_id == mid && x.step_id.is_none())
    {
        let color = match diagnostic.severity {
            crate::mkmacro::DiagnosticSeverity::Fatal => eframe::egui::Color32::RED,
            crate::mkmacro::DiagnosticSeverity::Warning => eframe::egui::Color32::YELLOW,
        };
        ui.colored_label(color, format!("⚠ {}", diagnostic.message))
            .on_hover_text(format!("{}\nCode: {}", diagnostic.message, diagnostic.code));
    }
    let runtime = crate::mkmacro::runtime::snapshot();
    let Some(m) = d.draft.macros.iter().find(|m| m.id == mid) else {
        return;
    };
    let rows: Vec<u64> = m.steps.iter().map(|s| s.id).collect();
    let depths = super::action_catalog::action_depths(m);
    let mut clicked = None;
    let mut changed = false;
    let mut updates = Vec::new();
    let mut command = None;
    let table_height = table_viewport_height(ui.available_height());
    ui.allocate_ui_with_layout(
        eframe::egui::vec2(ui.available_width(), table_height),
        eframe::egui::Layout::top_down(eframe::egui::Align::Min),
        |ui| {
    let max_scroll_height = ui.available_height().max(MIN_TABLE_VIEWPORT_HEIGHT);
    egui_extras::TableBuilder::new(ui)
        .striped(true)
        .auto_shrink([false, false])
        .max_scroll_height(max_scroll_height)
        .column(egui_extras::Column::exact(28.0))
        .column(egui_extras::Column::exact(55.0))
        .column(egui_extras::Column::initial(100.0))
        .column(egui_extras::Column::remainder())
        .column(egui_extras::Column::exact(50.0))
        .column(egui_extras::Column::exact(55.0))
        .column(egui_extras::Column::initial(90.0))
        .header(20.0, |mut h| {
            for x in [
                "#", "Enabled", "Action", "Details", "Repeat", "Delay", "Status",
            ] {
                h.col(|ui| {
                    ui.label(x);
                });
            }
        })
        .body(|mut body| {
            for (i, source) in m.steps.iter().enumerate() {
                let mut s = source.clone();
                body.row(22.0, |mut r| {
                    r.col(|ui| {
                        if ui
                            .selectable_label(d.selection.ids.contains(&s.id), (i + 1).to_string())
                            .clicked()
                        {
                            let mods = ui.input(|x| x.modifiers);
                            clicked = Some((i, mods.ctrl, mods.shift));
                        }
                    });
                    r.col(|ui| {
                        changed |= ui.add_enabled(
                            s.action.can_be_disabled(),
                            eframe::egui::Checkbox::without_text(&mut s.enabled),
                        ).changed();
                    });
                    r.col(|ui| {
                        let structural = matches!(s.action, crate::mkmacro::MkAction::Else|crate::mkmacro::MkAction::EndIf|crate::mkmacro::MkAction::RepeatEnd|crate::mkmacro::MkAction::WhileEnd);
                        let label = format!("{}{}", "  ".repeat(depths[i]), super::action_catalog::action_name(&s.action));
                        let response=if structural { ui.strong(label) } else { ui.label(label) };
                        if response.double_clicked(){command=Some(Command::Edit(s.id));}
                        let block = crate::mkmacro::analyze_structure(&m.steps).block_for_marker(s.id).is_some();
                        response.context_menu(|ui| context_menu(ui,s.id,block,&mut command));
                        if let Some(items) = row_diagnostics.get(&s.id) {
                            let first = items[0];
                            let color = match first.severity {
                                crate::mkmacro::DiagnosticSeverity::Fatal => eframe::egui::Color32::RED,
                                crate::mkmacro::DiagnosticSeverity::Warning => eframe::egui::Color32::YELLOW,
                            };
                            let hover = items.iter().map(|x| format!("{}\nCode: {}", x.message, x.code)).collect::<Vec<_>>().join("\n\n");
                            ui.colored_label(color, format!("⚠ {}", first.message)).on_hover_text(hover);
                        }
                    });
                    r.col(|ui| {
                        let full=super::action_catalog::action_details_with_assets(&s.action, &m.image_assets); let short=if full.chars().count()>80 {format!("{}…",full.chars().take(80).collect::<String>())}else{full.clone()}; let response=ui.label(short).on_hover_text(full);if response.double_clicked(){command=Some(Command::Edit(s.id));}let block=crate::mkmacro::analyze_structure(&m.steps).block_for_marker(s.id).is_some();response.context_menu(|ui|context_menu(ui,s.id,block,&mut command));
                    });
                    r.col(|ui| {
                        changed |= ui.add(eframe::egui::DragValue::new(&mut s.repeat).clamp_range(1..=1_000_000)).changed();
                    });
                    r.col(|ui| {
                        changed |= ui.add(eframe::egui::DragValue::new(&mut s.delay_after_ms).clamp_range(0..=86_400_000)).changed();
                    });
                    r.col(|ui| {
                        if let Some(state) = runtime.as_ref().and_then(|run| {
                            (run.macro_id == Some(mid)).then(|| run.steps.get(&s.id)).flatten()
                        }) {
                            let (label, full, color) = match state {
                                crate::mkmacro::StepState::Pending => ("○", "Pending", eframe::egui::Color32::GRAY),
                                crate::mkmacro::StepState::Running => ("▶", "Running", eframe::egui::Color32::YELLOW),
                                crate::mkmacro::StepState::Success => ("✓", "Success", eframe::egui::Color32::GREEN),
                                crate::mkmacro::StepState::Skipped => ("–", "Skipped", eframe::egui::Color32::GRAY),
                                crate::mkmacro::StepState::Failed => ("✕", "Failed", eframe::egui::Color32::RED),
                            };
                            let response = ui.colored_label(color, label).on_hover_text(full);
                            if let Some(run) = runtime.as_ref()
                                && let Some(failure) = run.failures.get(&crate::mkmacro::DiagnosticKey { run_id: run.run_id, step_id: s.id })
                            {
                                response.on_hover_ui(|ui| {
                                    ui.strong(&failure.message);
                                    for (key, value) in &failure.context { ui.label(format!("{key}: {value}")); }
                                    if failure.kind == crate::mkmacro::DiagnosticKind::InputRejected {
                                        ui.label("Likely integrity/UIPI restriction: SendInput accepted zero events.");
                                    }
                                });
                            }
                        }
                    });
                });
                updates.push((s.id,s.enabled,s.repeat,s.delay_after_ms));
            }
        });
        },
    );
    if let Some((i, ctrl, shift)) = clicked {
        d.selection.click(&rows, i, ctrl, shift);
    }
    if changed {
        if let Some(m) = d.selected_macro_mut() {
            for (id, en, repeat, delay) in updates {
                if let Some(s) = m.steps.iter_mut().find(|s| s.id == id) {
                    s.enabled = en;
                    s.repeat = repeat;
                    s.delay_after_ms = delay;
                }
            }
        }
        d.mark_dirty();
    }
    // Only route table shortcuts while no modal/editor or text edit owns input.
    if d.action_editor.draft.is_none() && !ui.ctx().wants_keyboard_input() {
        ui.input(|i| {
            if i.key_pressed(eframe::egui::Key::Enter) {
                if let Some(id) = d.selection.ids.iter().next() {
                    command = Some(Command::Edit(*id))
                }
            } else if i.key_pressed(eframe::egui::Key::Delete) {
                command = Some(Command::Delete)
            } else if i.modifiers.ctrl && i.key_pressed(eframe::egui::Key::D) {
                command = Some(Command::Duplicate)
            } else if i.modifiers.alt && i.key_pressed(eframe::egui::Key::ArrowUp) {
                command = Some(Command::Up)
            } else if i.modifiers.alt && i.key_pressed(eframe::egui::Key::ArrowDown) {
                command = Some(Command::Down)
            }
        });
    }
    if let Some(c) = command {
        apply_command(d, c);
    }
}

fn context_menu(
    ui: &mut eframe::egui::Ui,
    id: u64,
    complete_block: bool,
    out: &mut Option<Command>,
) {
    let delete = if complete_block {
        Command::DeleteBlock(id)
    } else {
        Command::DeleteRow(id)
    };
    for (label, c) in [
        ("Edit", Command::Edit(id)),
        ("Run This Step", Command::RunOne),
        ("Run From Here", Command::RunFrom),
        ("Insert Above", Command::InsertAbove(id)),
        ("Insert Below", Command::InsertBelow(id)),
        ("Duplicate", Command::Duplicate),
        ("Enable/Disable", Command::Toggle),
        ("Move Up", Command::Up),
        ("Move Down", Command::Down),
        (
            if complete_block {
                "Delete Block"
            } else {
                "Delete"
            },
            delete,
        ),
    ] {
        if ui.button(label).clicked() {
            *out = Some(c);
            ui.close_menu();
        }
    }
    if complete_block && ui.button("Unwrap Block").clicked() {
        *out = Some(Command::UnwrapBlock(id));
        ui.close_menu();
    }
}
fn apply_command(d: &mut MkMacroDialog, c: Command) {
    let selection_before_command = d.selection.clone();
    if let Command::DeleteBlock(id) | Command::DeleteRow(id) | Command::UnwrapBlock(id) = c {
        if !d.selection.ids.contains(&id) {
            d.selection.ids = BTreeSet::from([id]);
            d.selection.anchor = None;
        }
    }
    if let Command::Edit(id) = c {
        if let Some(s) = d
            .selected_macro()
            .and_then(|m| m.steps.iter().find(|s| s.id == id))
            .cloned()
        {
            if matches!(
                s.action,
                crate::mkmacro::MkAction::UiInvoke(_)
                    | crate::mkmacro::MkAction::UiSetValue { .. }
                    | crate::mkmacro::MkAction::UiReadValue { .. }
                    | crate::mkmacro::MkAction::UiToggle(_)
                    | crate::mkmacro::MkAction::UiSelect(_)
                    | crate::mkmacro::MkAction::UiFocus(_)
                    | crate::mkmacro::MkAction::UiWait(_)
            ) {
                d.command_error = Some(
                    "UI Automation actions are currently unavailable; the saved action was left unchanged."
                        .into(),
                );
            } else {
                d.action_editor.begin_edit(&s);
            }
        }
        return;
    }
    if matches!(c, Command::RunOne | Command::RunFrom) {
        let result = match (c, d.selection.ids.iter().next().copied()) {
            (Command::RunOne, _) => d.run_selected_steps(),
            (Command::RunFrom, Some(id)) => d.run_from_step(id),
            _ => Err(anyhow::anyhow!("Select a step")),
        };
        if let Err(e) = result {
            d.command_error = Some(e.to_string());
        }
        return;
    }
    if let Command::UnwrapBlock(id) = c {
        let Some(block) = d.selected_macro().and_then(|m| {
            crate::mkmacro::analyze_structure(&m.steps)
                .block_for_marker(id)
                .cloned()
        }) else {
            d.command_error = Some("The selected marker is not part of a complete block".into());
            return;
        };
        if block.else_marker.is_some() {
            d.pending_unwrap_block = Some(id);
            d.pending_unwrap_selection = Some(selection_before_command);
            d.unwrap_confirmation.open_custom("Unwrap If block", "Unwrapping this If will preserve both branches and make them execute sequentially.");
        } else {
            apply_confirmed_unwrap(d, id);
        }
        return;
    }
    let ids = d.selection.ids.clone();
    let mut new_selection = None;
    let mut mutation_error = None;
    if let Some(m) = d.selected_macro_mut() {
        match c {
            Command::Duplicate => {
                new_selection = Some(duplicate_steps_with_ids(&mut m.steps, &ids))
            }
            Command::Toggle => {
                for s in &mut m.steps {
                    if ids.contains(&s.id) && s.action.can_be_disabled() {
                        s.enabled = !s.enabled
                    }
                }
            }
            Command::Up | Command::Down => {
                match move_selection_structurally(&mut m.steps, &ids, matches!(c, Command::Down)) {
                    Ok(s) => new_selection = Some(s),
                    Err(e) => mutation_error = Some(e),
                }
            }
            Command::Delete | Command::DeleteRow(_) | Command::DeleteBlock(_) => {
                match delete_selection(&mut m.steps, &ids) {
                    Ok(s) => new_selection = Some(s),
                    Err(e) => mutation_error = Some(e),
                }
            }
            Command::InsertAbove(id) | Command::InsertBelow(id) => {
                let mut i = m
                    .steps
                    .iter()
                    .position(|s| s.id == id)
                    .unwrap_or(m.steps.len());
                if matches!(c, Command::InsertBelow(_)) {
                    i += 1
                }
                m.steps.insert(
                    i,
                    MkStep {
                        id: 0,
                        enabled: true,
                        repeat: 1,
                        delay_after_ms: 0,
                        on_error: Default::default(),
                        action: crate::mkmacro::MkAction::Delay { milliseconds: 1000 },
                    },
                );
            }
            _ => {}
        }
    }
    if let Some(error) = mutation_error {
        d.command_error = Some(error);
        return;
    }
    crate::mkmacro::repair_ids(&mut d.draft);
    if let Some(s) = new_selection {
        d.selection.ids = s;
        d.selection.anchor = None;
    }
    d.mark_dirty();
}

fn delete_selection(steps: &mut Vec<MkStep>, ids: &BTreeSet<u64>) -> Result<BTreeSet<u64>, String> {
    if ids.is_empty() {
        return Ok(BTreeSet::new());
    }
    let analysis = crate::mkmacro::analyze_structure(steps);
    let mut remove = BTreeSet::new();
    for id in ids {
        let Some((i, s)) = steps.iter().enumerate().find(|(_, s)| s.id == *id) else {
            continue;
        };
        if s.action.is_block_marker() {
            let b = analysis
                .block_for_marker(*id)
                .ok_or_else(|| format!("Step {id} is not part of a complete block"))?;
            remove.extend(b.range.clone());
        } else {
            remove.insert(i);
        }
    }
    if remove.is_empty() {
        return Ok(BTreeSet::new());
    }
    let first = *remove.first().unwrap();
    let last = *remove.last().unwrap();
    let next = (last + 1..steps.len())
        .find(|i| !remove.contains(i))
        .map(|i| steps[i].id);
    let prev = (0..first)
        .rev()
        .find(|i| !remove.contains(i))
        .map(|i| steps[i].id);
    let mut i = 0;
    steps.retain(|_| {
        let keep = !remove.contains(&i);
        i += 1;
        keep
    });
    Ok(next
        .or(prev)
        .map(|id| BTreeSet::from([id]))
        .unwrap_or_default())
}

fn expanded_move_ids(steps: &[MkStep], ids: &BTreeSet<u64>) -> Result<BTreeSet<u64>, String> {
    let a = crate::mkmacro::analyze_structure(steps);
    let mut out = ids.clone();
    for id in ids {
        if let Some(s) = steps.iter().find(|s| s.id == *id)
            && s.action.is_block_marker()
        {
            let b = a
                .block_for_marker(*id)
                .ok_or_else(|| format!("Step {id} is not part of a complete block"))?;
            out.extend(steps[b.range.clone()].iter().map(|s| s.id));
        }
    }
    Ok(out)
}
fn move_selection_structurally(
    steps: &mut [MkStep],
    ids: &BTreeSet<u64>,
    down: bool,
) -> Result<BTreeSet<u64>, String> {
    let expanded = expanded_move_ids(steps, ids)?;
    let before = crate::mkmacro::analyze_structure(steps).diagnostics.len();
    let mut candidate = steps.to_vec();
    move_steps(&mut candidate, &expanded, down);
    if crate::mkmacro::analyze_structure(&candidate)
        .diagnostics
        .len()
        > before
    {
        return Err("The move would invalidate block nesting".into());
    }
    steps.clone_from_slice(&candidate);
    Ok(expanded)
}

pub(super) fn apply_confirmed_unwrap(d: &mut MkMacroDialog, id: u64) {
    let result = if let Some(m) = d.selected_macro_mut() {
        crate::mkmacro::unwrap_block(&mut m.steps, id)
    } else {
        return;
    };
    match result {
        Ok(r) => {
            let selected = r
                .first_preserved_body_id
                .or(r.following_id)
                .or(r.preceding_id);
            d.selection.ids = selected.map(|x| BTreeSet::from([x])).unwrap_or_default();
            d.selection.anchor = None;
            d.mark_dirty();
        }
        Err(e) => d.command_error = Some(e),
    }
}
#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::mkmacro::{MkAction, MkCondition, MkErrorPolicy};

    fn step(id: u64, action: MkAction) -> MkStep {
        MkStep {
            id,
            enabled: true,
            repeat: 1,
            delay_after_ms: 0,
            on_error: MkErrorPolicy::Stop,
            action,
        }
    }
    fn delay(id: u64) -> MkStep {
        step(id, MkAction::Delay { milliseconds: 1 })
    }

    #[test]
    fn table_uses_all_remaining_height() {
        assert_eq!(table_viewport_height(500.0), 500.0);
        assert_eq!(table_viewport_height(250.0), 250.0);
        assert_eq!(table_viewport_height(20.0), MIN_TABLE_VIEWPORT_HEIGHT);
    }

    #[test]
    fn row_count_cannot_affect_table_height() {
        let allocations = [0_usize, 10, 500].map(|_row_count| table_viewport_height(500.0));
        assert_eq!(allocations, [500.0; 3]);
    }

    #[test]
    fn deletion_resolves_every_if_marker_and_deduplicates_nested_selections() {
        for selected in [1, 3, 7] {
            let mut rows = vec![
                delay(9),
                step(1, MkAction::If(MkCondition::All { conditions: vec![] })),
                step(2, MkAction::RepeatStart { count: 2 }),
                step(4, MkAction::Break),
                step(5, MkAction::RepeatEnd),
                step(3, MkAction::Else),
                delay(6),
                step(7, MkAction::EndIf),
                delay(10),
            ];
            let ids = BTreeSet::from([selected, 2, 4]);
            let selection = delete_selection(&mut rows, &ids).unwrap();
            assert_eq!(rows.iter().map(|s| s.id).collect::<Vec<_>>(), vec![9, 10]);
            assert_eq!(selection, BTreeSet::from([10]));
        }
    }

    #[test]
    fn controls_delete_ordinary_and_selection_falls_back() {
        let mut rows = vec![
            delay(1),
            step(2, MkAction::Break),
            step(3, MkAction::Continue),
        ];
        assert_eq!(
            delete_selection(&mut rows, &BTreeSet::from([2])).unwrap(),
            BTreeSet::from([3])
        );
        assert_eq!(
            delete_selection(&mut rows, &BTreeSet::from([3])).unwrap(),
            BTreeSet::from([1])
        );
        assert_eq!(
            delete_selection(&mut rows, &BTreeSet::from([1])).unwrap(),
            BTreeSet::new()
        );
    }

    #[test]
    fn moving_marker_moves_complete_block_as_one_unit() {
        let mut rows = vec![
            delay(9),
            step(
                1,
                MkAction::WhileStart {
                    condition: MkCondition::All { conditions: vec![] },
                },
            ),
            delay(2),
            step(3, MkAction::WhileEnd),
            delay(10),
        ];
        let selected = move_selection_structurally(&mut rows, &BTreeSet::from([3]), false).unwrap();
        assert_eq!(selected, BTreeSet::from([1, 2, 3]));
        assert_eq!(
            rows.iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![1, 2, 3, 9, 10]
        );
        let mut malformed = vec![step(1, MkAction::RepeatStart { count: 1 }), delay(2)];
        assert!(move_selection_structurally(&mut malformed, &BTreeSet::from([1]), true).is_err());
    }
}
