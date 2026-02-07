use crate::actions::{save_actions, Action};
use crate::gui::LauncherApp;
use eframe::egui;
use rfd::FileDialog;
use std::sync::Arc;

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
    /// Current dialog mode (add or edit existing command).
    mode: DialogMode,
}

/// Mode of the action dialog.
enum DialogMode {
    /// Add a new command when confirmed.
    Add,
    /// Edit the command at the given index when confirmed.
    Edit(usize),
}

impl Default for DialogMode {
    fn default() -> Self {
        DialogMode::Add
    }
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
            mode: DialogMode::Add,
        }
    }
}

impl AddActionDialog {
    /// Start the dialog for creating a new command.
    pub fn open_add(&mut self) {
        self.open_add_with_path("");
    }

    /// Start the dialog for creating a new command using `path` as the
    /// pre-filled executable.
    pub fn open_add_with_path(&mut self, path: &str) {
        self.open = true;
        self.label.clear();
        self.desc.clear();
        self.path = path.into();
        self.args.clear();
        self.show_args = false;
        self.mode = DialogMode::Add;
    }

    /// Start editing an existing command at `idx` with the given values.
    pub fn open_edit(&mut self, idx: usize, act: &Action) {
        self.open = true;
        self.label = act.label.clone();
        self.desc = act.desc.clone();
        self.path = act.action.clone();
        self.args = act.args.clone().unwrap_or_default();
        self.show_args = act.args.is_some();
        self.mode = DialogMode::Edit(idx);
    }

    /// Draw the "Add App" dialog and update `app` with any new action.
    ///
    /// * `ctx` - Egui context used to render the window.
    /// * `app` - Application state that will receive the new action if the
    ///   user confirms the dialog. The actions list is persisted on success.
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut should_close = false;
        let title = match self.mode {
            DialogMode::Add => "Add App",
            DialogMode::Edit(_) => "Edit App",
        };
        egui::Window::new(title)
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
                        let button = match self.mode {
                            DialogMode::Add => "Add",
                            DialogMode::Edit(_) => "Save",
                        };
                        if ui.button(button).clicked() {
                            use std::path::Path;
                            if self.path.is_empty() || !Path::new(&self.path).exists() {
                                app.set_error("Path does not exist".into());
                            } else {
                                match self.mode {
                                    DialogMode::Add => {
                                        let mut new_actions = (*app.actions).clone();
                                        new_actions.push(Action {
                                            label: self.label.clone(),
                                            desc: self.desc.clone(),
                                            action: self.path.clone(),
                                            args: if self.show_args && !self.args.trim().is_empty()
                                            {
                                                Some(self.args.clone())
                                            } else {
                                                None
                                            },
                                            preview_text: None,
                                            risk_level: None,
                                            icon: None,
                                        });
                                        app.custom_len += 1;
                                        app.actions = Arc::new(new_actions);
                                        app.update_action_cache();
                                        crate::actions::bump_actions_version();
                                    }
                                    DialogMode::Edit(idx) => {
                                        let mut new_actions = (*app.actions).clone();
                                        if let Some(act) = new_actions.get_mut(idx) {
                                            act.label = self.label.clone();
                                            act.desc = self.desc.clone();
                                            act.action = self.path.clone();
                                            act.args =
                                                if self.show_args && !self.args.trim().is_empty() {
                                                    Some(self.args.clone())
                                                } else {
                                                    None
                                                };
                                            app.actions = Arc::new(new_actions);
                                            app.update_action_cache();
                                            crate::actions::bump_actions_version();
                                        }
                                    }
                                }
                                self.label.clear();
                                self.desc.clear();
                                self.path.clear();
                                self.args.clear();
                                self.show_args = false;
                                should_close = true;
                                app.search();
                                if let Err(e) =
                                    save_actions(&app.actions_path, &app.actions[..app.custom_len])
                                {
                                    app.set_error(format!("Failed to save: {e}"));
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
