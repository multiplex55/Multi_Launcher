use crate::actions::save_actions;
use crate::gui::AddActionDialog;
use crate::gui::LauncherApp;
use eframe::egui;
use std::sync::Arc;

/// State container for the app editor window.
///
/// It tracks the current search filter and manages the nested
/// [`AddActionDialog`] used when creating new apps.
pub struct ActionsEditor {
    /// Search text used to filter the displayed actions.
    search: String,
    /// Dialog used for creating a new action.
    dialog: AddActionDialog,
}

impl Default for ActionsEditor {
    fn default() -> Self {
        Self {
            search: String::new(),
            dialog: AddActionDialog::default(),
        }
    }
}

impl ActionsEditor {
    /// Open the dialog for editing an existing app.
    pub fn open_edit(&mut self, idx: usize, act: &crate::actions::Action) {
        self.dialog.open_edit(idx, act);
    }

    /// Returns whether the add dialog is currently open.
    pub fn is_dialog_open(&self) -> bool {
        self.dialog.open
    }

    /// Open the add dialog with `path` pre-filled.
    pub fn open_add_with_path(&mut self, path: &str) {
        self.dialog.open_add_with_path(path);
    }

    /// Render the app editor window.
    ///
    /// * `ctx` - Egui context used for drawing the editor UI.
    /// * `app` - Mutable reference to the application state. Actions can be
    ///   added or removed and will be persisted when modified.
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        let mut open = app.show_editor;
        egui::Window::new("App Editor")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Search");
                    ui.text_edit_singleline(&mut self.search);
                    if ui.button("New App").clicked() {
                        self.dialog.open_add();
                    }
                });

                self.dialog.ui(ctx, app);

                ui.separator();
                let mut remove: Option<usize> = None;
                // Allow horizontal scrolling to avoid clipping long command strings
                egui::ScrollArea::both().max_height(200.0).show(ui, |ui| {
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
                            if ui.button("Edit").clicked() {
                                self.dialog.open_edit(idx, act);
                            }
                            if ui.button("Remove").clicked() {
                                remove = Some(idx);
                            }
                        });
                    }
                });

                if let Some(i) = remove {
                    let mut new_actions = (*app.actions).clone();
                    new_actions.remove(i);
                    if i < app.custom_len {
                        app.custom_len -= 1;
                    }
                    app.actions = Arc::new(new_actions);
                    app.search();
                    if let Err(e) = save_actions(&app.actions_path, &app.actions[..app.custom_len])
                    {
                        app.set_error(format!("Failed to save: {e}"));
                    }
                }
            });

        app.show_editor = open;
    }
}
