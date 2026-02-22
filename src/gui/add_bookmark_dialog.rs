use crate::gui::LauncherApp;
use crate::plugins::bookmarks::{append_bookmark, set_alias, BOOKMARKS_FILE};
use eframe::egui;
use egui_toast::{Toast, ToastKind, ToastOptions};

pub struct AddBookmarkDialog {
    pub open: bool,
    url: String,
    alias: String,
}

impl Default for AddBookmarkDialog {
    fn default() -> Self {
        Self {
            open: false,
            url: String::new(),
            alias: String::new(),
        }
    }
}

impl AddBookmarkDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.url.clear();
        self.alias.clear();
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut close = false;
        egui::Window::new("Add Bookmark")
            .open(&mut self.open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("URL");
                    ui.text_edit_singleline(&mut self.url);
                });
                ui.horizontal(|ui| {
                    ui.label("Alias");
                    ui.text_edit_singleline(&mut self.alias);
                });
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        if self.url.trim().is_empty() {
                            app.report_error_message("ui operation", "URL required".into());
                        } else {
                            if let Err(e) = append_bookmark(BOOKMARKS_FILE, &self.url) {
                                app.report_error_message(
                                    "ui operation",
                                    format!("Failed to save: {e}"),
                                );
                            } else if let Err(e) = set_alias(BOOKMARKS_FILE, &self.url, &self.alias)
                            {
                                app.report_error_message(
                                    "ui operation",
                                    format!("Failed to save alias: {e}"),
                                );
                            } else {
                                close = true;
                                if app.enable_toasts {
                                    app.add_toast(Toast {
                                        text: format!("Saved bookmark {}", self.url).into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(app.toast_duration as f64),
                                    });
                                }
                                app.search();
                                app.focus_input();
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
