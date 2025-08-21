use crate::gui::LauncherApp;
use crate::plugins::shell::{load_shell_cmds, save_shell_cmds, ShellCmdEntry, SHELL_CMDS_FILE};
use eframe::egui;

#[derive(Default)]
pub struct ShellCmdDialog {
    pub open: bool,
    entries: Vec<ShellCmdEntry>,
    edit_idx: Option<usize>,
    name: String,
    args: String,
    autocomplete: bool,
}

impl ShellCmdDialog {
    pub fn open(&mut self) {
        self.entries = load_shell_cmds(SHELL_CMDS_FILE).unwrap_or_default();
        self.open = true;
        self.edit_idx = None;
        self.name.clear();
        self.args.clear();
        self.autocomplete = true;
    }

    fn save(&mut self, app: &mut LauncherApp) {
        if let Err(e) = save_shell_cmds(SHELL_CMDS_FILE, &self.entries) {
            app.set_error(format!("Failed to save commands: {e}"));
        } else {
            app.search();
            app.focus_input();
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open { return; }
        let mut close = false;
        let mut save_now = false;
        egui::Window::new("Shell Commands")
            .open(&mut self.open)
            .show(ctx, |ui| {
                if let Some(idx) = self.edit_idx {
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(&mut self.name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Command");
                        ui.add(
                            egui::TextEdit::multiline(&mut self.args)
                                .id_source("shell_cmd_args"),
                        );
                        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.args.push('\n');
                            let modifiers = ui.input(|i| i.modifiers);
                            ui.input_mut(|i| i.consume_key(modifiers, egui::Key::Enter));
                        }
                    });
                    ui.checkbox(&mut self.autocomplete, "Autocomplete");
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.name.trim().is_empty() || self.args.trim().is_empty() {
                                app.set_error("Both fields required".into());
                            } else {
                                if idx == self.entries.len() {
                                    self.entries.push(ShellCmdEntry {
                                        name: self.name.clone(),
                                        args: self.args.clone(),
                                        autocomplete: self.autocomplete,
                                    });
                                } else if let Some(e) = self.entries.get_mut(idx) {
                                    e.name = self.name.clone();
                                    e.args = self.args.clone();
                                    e.autocomplete = self.autocomplete;
                                }
                                self.edit_idx = None;
                                self.name.clear();
                                self.args.clear();
                                self.autocomplete = true;
                                save_now = true;
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.edit_idx = None;
                        }
                    });
                } else {
                    let mut remove: Option<usize> = None;
                    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        for idx in 0..self.entries.len() {
                            let name = self.entries[idx].name.clone();
                            let args = self.entries[idx].args.clone();
                            ui.horizontal(|ui| {
                                ui.label(&name);
                                ui.label(&args);
                                if ui.button("Edit").clicked() {
                                    self.edit_idx = Some(idx);
                                    self.name = name.clone();
                                    self.args = args.clone();
                                    self.autocomplete = self.entries[idx].autocomplete;
                                }
                                if ui.button("Remove").clicked() {
                                    remove = Some(idx);
                                }
                            });
                        }
                    });
                    if let Some(idx) = remove {
                        self.entries.remove(idx);
                        save_now = true;
                    }
                    if ui.button("Add Command").clicked() {
                        self.edit_idx = Some(self.entries.len());
                        self.name.clear();
                        self.args.clear();
                        self.autocomplete = true;
                    }
                    if ui.button("Close").clicked() { close = true; }
                }
            });
        if save_now {
            self.save(app);
        }
        if close { self.open = false; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::PluginManager;
    use crate::settings::Settings;
    use eframe::egui;
    use std::sync::{Arc, atomic::AtomicBool};
    use tempfile::tempdir;

    fn new_app(ctx: &egui::Context) -> LauncherApp {
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            "actions.json".into(),
            "settings.json".into(),
            Settings::default(),
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn run_enter_test(modifiers: egui::Modifiers) -> String {
        let dir = tempdir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut dlg = ShellCmdDialog::default();
        dlg.open();
        dlg.edit_idx = Some(0);
        dlg.args = "echo hi".into();

        ctx.begin_frame(Default::default());
        dlg.ui(&ctx, &mut app);
        let _ = ctx.end_frame();
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("shell_cmd_args")));

        ctx.begin_frame(egui::RawInput {
            modifiers,
            events: vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        });
        dlg.ui(&ctx, &mut app);
        let _ = ctx.end_frame();

        dlg.args
    }

    #[test]
    fn enter_inserts_newline() {
        let args = run_enter_test(egui::Modifiers::default());
        assert_eq!(args, "echo hi\n");
    }

    #[test]
    fn shift_enter_inserts_newline() {
        let args = run_enter_test(egui::Modifiers { shift: true, ..Default::default() });
        assert_eq!(args, "echo hi\n");
    }
}
