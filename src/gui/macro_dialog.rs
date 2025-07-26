use crate::gui::LauncherApp;
use crate::plugins::macros::{load_macros, save_macros, MacroEntry, MACROS_FILE};
use eframe::egui;

#[derive(Default)]
pub struct MacroDialog {
    pub open: bool,
    entries: Vec<MacroEntry>,
    edit_idx: Option<usize>,
    label: String,
    desc: String,
    steps: String,
}

impl MacroDialog {
    pub fn open(&mut self) {
        self.entries = load_macros(MACROS_FILE).unwrap_or_default();
        self.open = true;
        self.edit_idx = None;
        self.label.clear();
        self.desc.clear();
        self.steps.clear();
    }

    fn save(&mut self, app: &mut LauncherApp) {
        if let Err(e) = save_macros(MACROS_FILE, &self.entries) {
            app.set_error(format!("Failed to save macros: {e}"));
        } else {
            app.search();
            app.focus_input();
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut close = false;
        let mut save_now = false;
        egui::Window::new("Macros")
            .open(&mut self.open)
            .show(ctx, |ui| {
                if let Some(idx) = self.edit_idx {
                    ui.horizontal(|ui| {
                        ui.label("Label");
                        ui.text_edit_singleline(&mut self.label);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Description");
                        ui.text_edit_singleline(&mut self.desc);
                    });
                    ui.label("Steps (one per line)");
                    ui.text_edit_multiline(&mut self.steps);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.label.trim().is_empty() {
                                app.set_error("Label required".into());
                            } else {
                                let steps: Vec<String> = self
                                    .steps
                                    .lines()
                                    .map(|l| l.trim())
                                    .filter(|l| !l.is_empty())
                                    .map(|l| l.to_string())
                                    .collect();
                                if idx == self.entries.len() {
                                    self.entries.push(MacroEntry {
                                        label: self.label.clone(),
                                        desc: self.desc.clone(),
                                        steps,
                                    });
                                } else if let Some(e) = self.entries.get_mut(idx) {
                                    e.label = self.label.clone();
                                    e.desc = self.desc.clone();
                                    e.steps = steps;
                                }
                                self.edit_idx = None;
                                self.label.clear();
                                self.desc.clear();
                                self.steps.clear();
                                save_now = true;
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.edit_idx = None;
                        }
                    });
                } else {
                    let mut remove: Option<usize> = None;
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
                            for idx in 0..self.entries.len() {
                                let entry = &self.entries[idx];
                                ui.horizontal(|ui| {
                                    ui.label(format!("{}: {}", entry.label, entry.desc));
                                    if ui.button("Edit").clicked() {
                                        self.edit_idx = Some(idx);
                                        self.label = entry.label.clone();
                                        self.desc = entry.desc.clone();
                                        self.steps = entry.steps.join("\n");
                                    }
                                    if ui.button("Remove").clicked() {
                                        remove = Some(idx);
                                    }
                                });
                            }
                        });
                    if let Some(idx) = remove {
                        self.entries.remove(idx);
                        save_now = true;
                    }
                    if ui.button("Add Macro").clicked() {
                        self.edit_idx = Some(self.entries.len());
                        self.label.clear();
                        self.desc.clear();
                        self.steps.clear();
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                }
            });
        if save_now {
            self.save(app);
        }
        if close {
            self.open = false;
        }
    }
}
