use crate::actions::save_actions;
use crate::add_action_dialog::AddActionDialog;
use crate::gui::LauncherApp;
use eframe::egui;

pub struct ActionsEditor {
    search: String,
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
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        let mut open = app.show_editor;
        egui::Window::new("Command Editor")
            .open(&mut open)
            .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search");
                ui.text_edit_singleline(&mut self.search);
                if ui.button("New Command").clicked() {
                    self.dialog.open = true;
                }
            });

            self.dialog.ui(ctx, app);

            ui.separator();
            let mut remove: Option<usize> = None;
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
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
                        if ui.button("Remove").clicked() {
                            remove = Some(idx);
                        }
                    });
                }
            });

            if let Some(i) = remove {
                app.actions.remove(i);
                app.search();
                if let Err(e) = save_actions(&app.actions_path, &app.actions) {
                    app.error = Some(format!("Failed to save: {e}"));
                }
            }

        });

        app.show_editor = open;
    }
}
