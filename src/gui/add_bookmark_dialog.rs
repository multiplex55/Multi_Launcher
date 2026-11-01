use crate::gui::LauncherApp;
use crate::gui::theme::widgets::{ButtonEmphasis, with_button_emphasis};
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::plugins::bookmarks::{BOOKMARKS_FILE, append_bookmark_with_alias};
use eframe::egui;
use egui_toast::{Toast, ToastKind, ToastOptions};

#[derive(Default)]
pub struct AddBookmarkDialog {
    pub open: bool,
    url: String,
    alias: String,
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
        egui::Window::new("Add Bookmark").show_utility(
            ctx,
            "add-bookmark",
            egui::Id::new("Add Bookmark"),
            Some(&mut self.open),
            crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(
                crate::gui::utility_dialog_size::UtilityWindowSizeDefault::intrinsic(),
                egui::Vec2::ZERO,
                [None, None],
            ),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    let label = ui.label("URL");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.url)
                            .desired_width(ui.available_width().min(320.0).max(72.0)),
                    )
                    .labelled_by(label.id);
                });
                ui.horizontal_wrapped(|ui| {
                    let label = ui.label("Alias");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.alias)
                            .desired_width(ui.available_width().min(320.0).max(72.0)),
                    )
                    .labelled_by(label.id);
                });
                ui.horizontal_wrapped(|ui| {
                    let save = ui.button("Save");
                    if with_button_emphasis(ui, save, ButtonEmphasis::Primary).clicked() {
                        if self.url.trim().is_empty() {
                            app.report_error_message("ui operation", "URL required");
                        } else if let Err(e) =
                            append_bookmark_with_alias(BOOKMARKS_FILE, &self.url, Some(&self.alias))
                        {
                            app.report_error_message(
                                "ui operation",
                                format!("Failed to save: {e}"),
                            );
                        } else {
                            close = true;
                            if app.enable_toasts {
                                app.add_toast(Toast {
                                    text: format!("Saved bookmark {}", self.url).into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(app.toast_duration as f64),

                                    style: Default::default(),
                                });
                            }
                            app.search();
                            app.focus_input();
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            },
        );
        if close {
            self.open = false;
        }
    }
}
