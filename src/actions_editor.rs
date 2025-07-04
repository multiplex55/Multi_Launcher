use crate::actions::{Action, save_actions};
use crate::gui::LauncherApp;
use eframe::egui;
use rfd::FileDialog;

pub struct ActionsEditor {
    label: String,
    desc: String,
    path: String,
    search: String,
    show_new: bool,
}

impl Default for ActionsEditor {
    fn default() -> Self {
        Self {
            label: String::new(),
            desc: String::new(),
            path: String::new(),
            search: String::new(),
            show_new: false,
        }
    }
}

impl ActionsEditor {
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        let mut open = app.show_editor;
        egui::Window::new("Command Editor")
            .open(&mut open)
            .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search");
                ui.text_edit_singleline(&mut self.search);
                if self.show_new {
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("Label");
                            ui.text_edit_singleline(&mut self.label);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Description");
                            ui.text_edit_singleline(&mut self.desc);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Path");
                            ui.text_edit_singleline(&mut self.path);
                            if ui.button("Browse").clicked() {
                                if let Some(file) = FileDialog::new().pick_file() {
                                    if let Some(p) = file.to_str() {
                                        self.path = p.to_owned();
                                    } else {
                                        self.path = file.display().to_string();
                                    }
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Add").clicked() {
                                use std::path::Path;
                                if self.path.is_empty() || !Path::new(&self.path).exists() {
                                    app.error = Some("Path does not exist".into());
                                } else {
                                    app.actions.push(Action {
                                        label: self.label.clone(),
                                        desc: self.desc.clone(),
                                        action: self.path.clone(),
                                    });
                                    self.label.clear();
                                    self.desc.clear();
                                    self.path.clear();
                                    self.show_new = false;
                                    app.search();
                                    if let Err(e) = save_actions(&app.actions_path, &app.actions) {
                                        app.error = Some(format!("Failed to save: {e}"));
                                    }
                                }
                            }
                            if ui.button("Cancel").clicked() {
                                self.show_new = false;
                            }
                        });
                    });
                } else if ui.button("New Command").clicked() {
                    self.show_new = true;
                }
            });

            ui.separator();
            let mut remove: Option<usize> = None;
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                for (idx, act) in app.actions.iter().enumerate() {
                    if !self.search.trim().is_empty() {
                        let q = self.search.to_lowercase();
                        let label = act.label.to_lowercase();
                        let desc = act.desc.to_lowercase();
                        let action = act.action.to_lowercase();
                        if !label.contains(&q) && !desc.contains(&q) && !action.contains(&q) {
                            continue;
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.label(format!("{} : {} -> {}", act.label, act.desc, act.action));
                        if ui.button("Remove").clicked() {
                            remove = Some(idx);
                        }
                    });
                }
            });

            if let Some(i) = remove {
                app.actions.remove(i);
                app.search();
                if let Err(e) = save_actions(&app.actions_path, &app.actions) {
                    app.error = Some(format!("Failed to save: {e}"));
                }
            }

        });

        app.show_editor = open;
    }
}
