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
}

impl ShellCmdDialog {
    pub fn open(&mut self) {
        self.entries = load_shell_cmds(SHELL_CMDS_FILE).unwrap_or_default();
        self.open = true;
        self.edit_idx = None;
        self.name.clear();
        self.args.clear();
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
                        ui.text_edit_singleline(&mut self.args);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.name.trim().is_empty() || self.args.trim().is_empty() {
                                app.set_error("Both fields required".into());
                            } else {
                                if idx == self.entries.len() {
                                    self.entries.push(ShellCmdEntry { name: self.name.clone(), args: self.args.clone() });
                                } else if let Some(e) = self.entries.get_mut(idx) {
                                    e.name = self.name.clone();
                                    e.args = self.args.clone();
                                }
                                self.edit_idx = None;
                                self.name.clear();
                                self.args.clear();
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
