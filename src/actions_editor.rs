use crate::actions::{Action, save_actions};
use crate::gui::LauncherApp;
use eframe::egui;
use rfd::FileDialog;

pub struct ActionsEditor {
    label: String,
    desc: String,
    path: String,
}

impl Default for ActionsEditor {
    fn default() -> Self {
        Self {
            label: String::new(),
            desc: String::new(),
            path: String::new(),
        }
    }
}

impl ActionsEditor {
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        egui::Window::new("Command Editor").show(ctx, |ui| {
            let mut remove: Option<usize> = None;
            for (idx, act) in app.actions.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{} : {} -> {}", act.label, act.desc, act.action));
                    if ui.button("Remove").clicked() {
                        remove = Some(idx);
                    }
                });
            }
            if let Some(i) = remove {
                app.actions.remove(i);
                app.search();
                if let Err(e) = save_actions(&app.actions_path, &app.actions) {
                    app.error = Some(format!("Failed to save: {e}"));
                }
            }

            ui.separator();
            ui.label("Add new command:");
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
            if ui.button("Add").clicked() {
                if !self.path.is_empty() {
                    app.actions.push(Action {
                        label: self.label.clone(),
                        desc: self.desc.clone(),
                        action: self.path.clone(),
                    });
                    self.label.clear();
                    self.desc.clear();
                    self.path.clear();
                    app.search();
                    if let Err(e) = save_actions(&app.actions_path, &app.actions) {
                        app.error = Some(format!("Failed to save: {e}"));
                    }
                }
            }
        });
    }
}
