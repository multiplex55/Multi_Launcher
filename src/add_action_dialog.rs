use crate::actions::{Action, save_actions};
use crate::gui::LauncherApp;
use eframe::egui;
use rfd::FileDialog;

pub struct AddActionDialog {
    pub open: bool,
    label: String,
    desc: String,
    path: String,
}

impl Default for AddActionDialog {
    fn default() -> Self {
        Self {
            open: false,
            label: String::new(),
            desc: String::new(),
            path: String::new(),
        }
    }
}

impl AddActionDialog {
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Add Command")
            .open(&mut open)
            .show(ctx, |ui| {
                let mut close = false;
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
                                close = true;
                                app.search();
                                if let Err(e) = save_actions(&app.actions_path, &app.actions) {
                                    app.error = Some(format!("Failed to save: {e}"));
                                }
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
                if close {
                    open = false;
                }
            });
        self.open = open;
    }
}

