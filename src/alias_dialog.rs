use crate::gui::LauncherApp;
use crate::plugins::folders::{set_alias, FOLDERS_FILE, load_folders};
use eframe::egui;

pub struct AliasDialog {
    pub open: bool,
    path: String,
    alias: String,
}

impl Default for AliasDialog {
    fn default() -> Self {
        Self { open: false, path: String::new(), alias: String::new() }
    }
}

impl AliasDialog {
    pub fn open(&mut self, path: &str) {
        self.path = path.to_string();
        // pre-fill alias with current value if exists
        if let Ok(list) = load_folders(FOLDERS_FILE) {
            if let Some(entry) = list.into_iter().find(|f| f.path == self.path) {
                if let Some(a) = entry.alias {
                    self.alias = a;
                } else {
                    self.alias = String::new();
                }
            }
        }
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut close = false;
        egui::Window::new("Set Folder Alias")
            .open(&mut self.open)
            .show(ctx, |ui| {
                ui.label(&self.path);
                ui.text_edit_singleline(&mut self.alias);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        if let Err(e) = set_alias(FOLDERS_FILE, &self.path, &self.alias) {
                            app.error = Some(format!("Failed to save alias: {e}"));
                        } else {
                            close = true;
                            app.search();
                            app.focus_input();
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if close {
            self.open = false;
        }
    }
}

