use crate::gui::LauncherApp;
use crate::plugins::tempfile::create_named_file;
use eframe::egui;
use egui_toast::{Toast, ToastKind, ToastOptions};

pub struct TempfileDialog {
    pub open: bool,
    alias: String,
    text: String,
}

impl Default for TempfileDialog {
    fn default() -> Self {
        Self {
            open: false,
            alias: String::new(),
            text: String::new(),
        }
    }
}

impl TempfileDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.alias.clear();
        self.text.clear();
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut close = false;
        egui::Window::new("Create Temp File")
            .open(&mut self.open)
            .resizable(true)
            .default_size((360.0, 240.0))
            .min_width(200.0)
            .min_height(150.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Alias");
                    ui.text_edit_singleline(&mut self.alias);
                });
                ui.label("Text");
                ui.text_edit_multiline(&mut self.text);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        if self.alias.trim().is_empty() {
                            app.set_error("Alias required".into());
                        } else {
                            match create_named_file(&self.alias, &self.text) {
                                Ok(path) => {
                                    if let Err(e) = open::that(&path) {
                                        app.set_error(format!("Failed to open: {e}"));
                                    } else {
                                        if app.enable_toasts {
                                            app.add_toast(Toast {
                                                text: format!(
                                                    "Created {}",
                                                    path.file_name()
                                                        .map(|n| n.to_string_lossy())
                                                        .unwrap_or_else(|| path.to_string_lossy())
                                                )
                                                .into(),
                                                kind: ToastKind::Success,
                                                options: ToastOptions::default()
                                                    .duration_in_seconds(app.toast_duration as f64),
                                            });
                                        }
                                        close = true;
                                        app.search();
                                        app.focus_input();
                                    }
                                }
                                Err(e) => app.set_error(format!("Failed to save: {e}")),
                            }
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
