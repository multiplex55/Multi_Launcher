use crate::gui::LauncherApp;
use crate::plugins::tempfile::set_alias;
use eframe::egui;
use std::path::Path;

pub struct TempfileAliasDialog {
    pub open: bool,
    path: String,
    alias: String,
}

impl Default for TempfileAliasDialog {
    fn default() -> Self {
        Self {
            open: false,
            path: String::new(),
            alias: String::new(),
        }
    }
}

impl TempfileAliasDialog {
    pub fn open(&mut self, path: &str) {
        self.path = path.to_string();
        let name = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        self.alias = name.trim_start_matches("temp_").to_string();
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut close = false;
        egui::Window::new("Set Tempfile Alias")
            .open(&mut self.open)
            .show(ctx, |ui| {
                ui.label(&self.path);
                ui.text_edit_singleline(&mut self.alias);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        if let Err(e) = set_alias(Path::new(&self.path), &self.alias) {
                            app.report_error_message(
                                "ui operation",
                                format!("Failed to rename: {e}"),
                            );
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
