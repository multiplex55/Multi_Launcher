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
pub fn duplicate_steps(steps: &mut Vec<MkStep>, ids: &BTreeSet<u64>) {
    let mut copies: Vec<_> = steps
        .iter()
        .filter(|s| ids.contains(&s.id))
        .cloned()
        .collect();
    for s in &mut copies {
        s.id = 0;
    }
    steps.extend(copies);
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
                        if structural { ui.strong(label); } else { ui.label(label); }
                    });
                    r.col(|ui| {
                        let full=super::action_catalog::action_details(&s.action); let short=if full.chars().count()>80 {format!("{}…",full.chars().take(80).collect::<String>())}else{full.clone()}; ui.label(short).on_hover_text(full);
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
}
