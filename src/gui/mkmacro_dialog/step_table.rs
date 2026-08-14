use super::MkMacroDialog;
use crate::mkmacro::{MkStep, validate_document};
use std::collections::BTreeSet;

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
        schema_version: crate::mkmacro::SCHEMA_VERSION,
        macros: vec![crate::mkmacro::MkMacro {
            id: 1,
            name: "draft".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            playback: Default::default(),
            steps: steps.clone(),
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
    InsertAbove(u64),
    InsertBelow(u64),
    RunOne,
    RunFrom,
}
fn structural(a: &crate::mkmacro::MkAction) -> bool {
    matches!(
        a,
        crate::mkmacro::MkAction::If(_)
            | crate::mkmacro::MkAction::Else
            | crate::mkmacro::MkAction::EndIf
            | crate::mkmacro::MkAction::RepeatStart { .. }
            | crate::mkmacro::MkAction::RepeatEnd
            | crate::mkmacro::MkAction::WhileStart { .. }
            | crate::mkmacro::MkAction::WhileEnd
    )
}
pub(super) fn show(ui: &mut eframe::egui::Ui, d: &mut MkMacroDialog) {
    let Some(mid) = d.selected_macro_id else {
        ui.label("Select a macro");
        return;
    };
    let diagnostics = validate_document(&d.draft, None);
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
    egui_extras::TableBuilder::new(ui)
        .striped(true)
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
                        response.context_menu(|ui| context_menu(ui,s.id,&mut command));
                    });
                    r.col(|ui| {
                        let full=super::action_catalog::action_details(&s.action); let short=if full.chars().count()>80 {format!("{}…",full.chars().take(80).collect::<String>())}else{full.clone()}; let response=ui.label(short).on_hover_text(full);if response.double_clicked(){command=Some(Command::Edit(s.id));}response.context_menu(|ui|context_menu(ui,s.id,&mut command));
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
                            let (label, color) = match state {
                                crate::mkmacro::StepState::Pending => ("pending", eframe::egui::Color32::GRAY),
                                crate::mkmacro::StepState::Running => ("running", eframe::egui::Color32::YELLOW),
                                crate::mkmacro::StepState::Success => ("success", eframe::egui::Color32::GREEN),
                                crate::mkmacro::StepState::Skipped => ("skipped", eframe::egui::Color32::GRAY),
                                crate::mkmacro::StepState::Failed => ("failed", eframe::egui::Color32::RED),
                            };
                            let response = ui.colored_label(color, label);
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
                        for x in diagnostics.iter().filter(|x| x.step_id == Some(s.id)) {
                            ui.colored_label(eframe::egui::Color32::RED, &x.message);
                        }
                    });
                });
                updates.push((s.id,s.enabled,s.repeat,s.delay_after_ms));
            }
        });
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
    quick_insert(ui, d);
}

fn context_menu(ui: &mut eframe::egui::Ui, id: u64, out: &mut Option<Command>) {
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
        ("Delete", Command::Delete),
    ] {
        if ui.button(label).clicked() {
            *out = Some(c);
            ui.close_menu();
        }
    }
}
fn apply_command(d: &mut MkMacroDialog, c: Command) {
    if let Command::Edit(id) = c {
        if let Some(s) = d
            .selected_macro()
            .and_then(|m| m.steps.iter().find(|s| s.id == id))
            .cloned()
        {
            d.action_editor.begin_edit(&s);
        }
        return;
    }
    if matches!(c, Command::RunOne | Command::RunFrom) {
        return;
    } // runtime commands are intentionally draft-only unsupported until a compiled slice API exists.
    let ids = d.selection.ids.clone();
    let unsafe_structure = d.selected_macro().is_some_and(|m| {
        m.steps
            .iter()
            .any(|s| ids.contains(&s.id) && structural(&s.action))
    });
    if unsafe_structure && matches!(c, Command::Delete | Command::Up | Command::Down) {
        return;
    }
    let mut new_selection = None;
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
            Command::Up => move_steps(&mut m.steps, &ids, false),
            Command::Down => move_steps(&mut m.steps, &ids, true),
            Command::Delete => {
                let first = m
                    .steps
                    .iter()
                    .position(|s| ids.contains(&s.id))
                    .unwrap_or(0);
                m.steps.retain(|s| !ids.contains(&s.id));
                new_selection = Some(
                    m.steps
                        .get(first)
                        .or_else(|| first.checked_sub(1).and_then(|i| m.steps.get(i)))
                        .map(|s| BTreeSet::from([s.id]))
                        .unwrap_or_default(),
                );
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
    crate::mkmacro::repair_ids(&mut d.draft);
    if let Some(s) = new_selection {
        d.selection.ids = s;
    }
    d.mark_dirty();
}
fn quick_insert(ui: &mut eframe::egui::Ui, d: &mut MkMacroDialog) {
    ui.separator();
    ui.heading("Quick Insert");
    let text = if d.quick_insert.keys.is_empty() {
        "No key captured".into()
    } else {
        d.quick_insert
            .keys
            .iter()
            .map(super::macro_properties::key_name)
            .collect::<Vec<_>>()
            .join(" + ")
    };
    ui.horizontal(|ui| {
        ui.label(text);
        if ui.button("Capture key/chord").clicked() {
            d.quick_insert.capturing = true;
        }
        ui.label("Repeat");
        ui.add(eframe::egui::DragValue::new(&mut d.quick_insert.repeat).clamp_range(1..=1_000_000));
        ui.label("Delay (ms)");
        ui.add(
            eframe::egui::DragValue::new(&mut d.quick_insert.delay_after_ms)
                .clamp_range(0..=86_400_000),
        );
    });
    if d.quick_insert.capturing {
        ui.label("Press a key or combination; Escape cancels capture.");
        if let Some(keys) = ui.input(super::action_editor::captured_from_input) {
            d.quick_insert.capturing = false;
            d.quick_insert.keys = keys;
        }
    }
    let enabled = d.quick_insert.action().is_some();
    if ui
        .add_enabled(enabled, eframe::egui::Button::new("Insert"))
        .clicked()
    {
        let mut q = std::mem::take(&mut d.quick_insert);
        q.insert(d);
        d.quick_insert = q;
    }
}
