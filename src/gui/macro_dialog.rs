use crate::gui::LauncherApp;
use crate::plugins::macros::{load_macros, save_macros, MacroEntry, MacroStep, MACROS_FILE};
use eframe::egui;

#[derive(Default)]
pub struct MacroDialog {
    pub open: bool,
    entries: Vec<MacroEntry>,
    edit_idx: Option<usize>,
    label: String,
    desc: String,
    steps: Vec<MacroStep>,
    add_plugin: String,
    add_filter: String,
    add_args: String,
}

impl MacroDialog {
    pub fn open(&mut self) {
        self.entries = load_macros(MACROS_FILE).unwrap_or_default();
        self.open = true;
        self.edit_idx = None;
        self.label.clear();
        self.desc.clear();
        self.steps.clear();
        self.add_plugin.clear();
        self.add_filter.clear();
        self.add_args.clear();
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
                    ui.label("Steps");
                    let mut remove_step: Option<usize> = None;
                    for i in 0..self.steps.len() {
                        ui.horizontal(|ui| {
                            ui.label(format!("{}.", i + 1));
                            ui.label(&self.steps[i].action);
                            ui.label("Args");
                            let args = self.steps[i].args.get_or_insert_with(String::new);
                            ui.text_edit_singleline(args);
                            if ui.button("Remove").clicked() {
                                remove_step = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove_step {
                        self.steps.remove(i);
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Category");
                        egui::ComboBox::from_id_source("macro_cat")
                            .selected_text(if self.add_plugin.is_empty() {
                                "Select".to_string()
                            } else {
                                self.add_plugin.clone()
                            })
                            .show_ui(ui, |ui| {
                                for p in app.plugins.iter() {
                                    let name = p.name();
                                    ui.selectable_value(
                                        &mut self.add_plugin,
                                        name.to_string(),
                                        name,
                                    );
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Filter");
                        ui.text_edit_singleline(&mut self.add_filter);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Args");
                        ui.text_edit_singleline(&mut self.add_args);
                    });
                    if let Some(plugin) = app.plugins.iter().find(|p| p.name() == self.add_plugin) {
                        let filter = self.add_filter.trim().to_lowercase();
                        let mut actions = if plugin.name() == "folders" {
                            plugin.search(&format!("f {}", self.add_filter))
                        } else if plugin.name() == "bookmarks" {
                            plugin.search(&format!("bm {}", self.add_filter))
                        } else {
                            plugin.commands()
                        };
                        egui::ScrollArea::vertical()
                            .max_height(100.0)
                            .show(ui, |ui| {
                                for act in actions.drain(..) {
                                    if !filter.is_empty()
                                        && !act.label.to_lowercase().contains(&filter)
                                        && !act.desc.to_lowercase().contains(&filter)
                                        && !act.action.to_lowercase().contains(&filter)
                                    {
                                        continue;
                                    }
                                    if ui.button(format!("{} - {}", act.label, act.desc)).clicked()
                                    {
                                        self.steps.push(MacroStep {
                                            action: act.action.clone(),
                                            args: if self.add_args.trim().is_empty() {
                                                None
                                            } else {
                                                Some(self.add_args.clone())
                                            },
                                        });
                                        self.add_args.clear();
                                    }
                                }
                            });
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.label.trim().is_empty() {
                                app.set_error("Label required".into());
                            } else {
                                if idx == self.entries.len() {
                                    self.entries.push(MacroEntry {
                                        label: self.label.clone(),
                                        desc: self.desc.clone(),
                                        steps: self.steps.clone(),
                                    });
                                } else if let Some(e) = self.entries.get_mut(idx) {
                                    e.label = self.label.clone();
                                    e.desc = self.desc.clone();
                                    e.steps = self.steps.clone();
                                }
                                self.edit_idx = None;
                                self.label.clear();
                                self.desc.clear();
                                self.steps.clear();
                                self.add_plugin.clear();
                                self.add_filter.clear();
                                self.add_args.clear();
                                save_now = true;
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.edit_idx = None;
                            self.add_plugin.clear();
                            self.add_filter.clear();
                            self.add_args.clear();
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
                                        self.steps = entry.steps.clone();
                                        self.add_plugin.clear();
                                        self.add_filter.clear();
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
                        self.add_plugin.clear();
                        self.add_filter.clear();
                        self.add_args.clear();
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
