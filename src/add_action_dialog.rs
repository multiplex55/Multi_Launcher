use crate::actions::{Action, save_actions};
use crate::gui::LauncherApp;
use eframe::egui;
#[cfg(target_os = "windows")]
use rfd::FileDialog;

/// Dialog state used when adding a new user defined command.
///
/// The struct holds the text the user has entered as well as an `open`
/// flag indicating if the dialog should currently be visible.
pub struct AddActionDialog {
    /// `true` while the dialog window is displayed.
    pub open: bool,
    /// Command label being edited.
    label: String,
    /// Command description being edited.
    desc: String,
    /// Path to the executable or file to launch.
    path: String,
    /// Whether the arguments field is visible.
    show_args: bool,
    /// Additional arguments to pass when launching.
    args: String,
}

impl Default for AddActionDialog {
    fn default() -> Self {
        Self {
            open: false,
            label: String::new(),
            desc: String::new(),
            path: String::new(),
            show_args: false,
            args: String::new(),
        }
    }
}

impl AddActionDialog {
    /// Draw the "Add Command" dialog and update `app` with any new action.
    ///
    /// * `ctx` - Egui context used to render the window.
    /// * `app` - Application state that will receive the new action if the
    ///   user confirms the dialog. The actions list is persisted on success.
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut should_close = false;
        egui::Window::new("Add Command")
            .open(&mut self.open)
            .show(ctx, |ui| {
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
                            #[cfg(target_os = "windows")]
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
                        ui.checkbox(&mut self.show_args, "Add arguments");
                        if self.show_args {
                            ui.text_edit_singleline(&mut self.args);
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
                                    args: if self.show_args && !self.args.trim().is_empty() {
                                        Some(self.args.clone())
                                    } else {
                                        None
                                    },
                                });
                                self.label.clear();
                                self.desc.clear();
                                self.path.clear();
                                self.args.clear();
                                self.show_args = false;
                                should_close = true;
                                app.search();
                                if let Err(e) = save_actions(&app.actions_path, &app.actions) {
                                    app.error = Some(format!("Failed to save: {e}"));
                                }
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            should_close = true;
                        }
                    });
                });
                if should_close {
                    // defer closing until after borrow ends
                }
            });
        if should_close {
            self.open = false;
        }
    }
}

