use crate::gui::LauncherApp;
use crate::plugins::shell::{SHELL_CMDS_FILE, ShellCmdEntry, load_shell_cmds, replace_shell_cmds};
use eframe::egui;

#[derive(Default)]
pub struct ShellCmdDialog {
    pub open: bool,
    entries: Vec<ShellCmdEntry>,
    edit_idx: Option<usize>,
    name: String,
    args: String,
    keep_open: bool,
    load_error: Option<String>,
}

impl ShellCmdDialog {
    pub fn open(&mut self) {
        let _ = self.load_from(SHELL_CMDS_FILE);
        self.open = true;
        self.edit_idx = None;
        self.name.clear();
        self.args.clear();
        self.keep_open = false;
    }

    fn load_from(&mut self, path: &str) -> anyhow::Result<()> {
        match load_shell_cmds(path) {
            Ok(entries) => {
                self.entries = entries;
                self.load_error = None;
                Ok(())
            }
            Err(error) => {
                self.load_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn commit_entries(&mut self, path: &str, candidate: Vec<ShellCmdEntry>) -> anyhow::Result<()> {
        match replace_shell_cmds(path, candidate) {
            Ok(committed) => {
                self.entries = committed;
                self.load_error = None;
                Ok(())
            }
            Err(error) => {
                self.load_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn save(&mut self, app: &mut LauncherApp, candidate: Vec<ShellCmdEntry>) -> bool {
        if let Err(e) = self.commit_entries(SHELL_CMDS_FILE, candidate) {
            app.report_error_message("ui operation", format!("Failed to save commands: {e}"));
            false
        } else {
            app.search();
            app.focus_input();
            true
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut close = false;
        let mut save_candidate = None;
        egui::Window::new("Shell Commands")
            .open(&mut self.open)
            .show(ctx, |ui| {
                if let Some(error) = &self.load_error {
                    ui.colored_label(
                        egui::Color32::RED,
                        format!("Shell commands are read-only because loading failed: {error}"),
                    );
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                    return;
                }
                if let Some(idx) = self.edit_idx {
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(&mut self.name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Command");
                        ui.add(
                            egui::TextEdit::multiline(&mut self.args).id_source("shell_cmd_args"),
                        );
                        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.args.push('\n');
                            let modifiers = ui.input(|i| i.modifiers);
                            ui.input_mut(|i| i.consume_key(modifiers, egui::Key::Enter));
                        }
                    });
                    ui.checkbox(&mut self.keep_open, "Keep command prompt open after run");
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.name.trim().is_empty() || self.args.trim().is_empty() {
                                app.report_error_message("ui operation", "Both fields required");
                            } else {
                                let mut candidate = self.entries.clone();
                                if idx == candidate.len() {
                                    candidate.push(ShellCmdEntry {
                                        name: self.name.clone(),
                                        args: self.args.clone(),
                                        autocomplete: true,
                                        keep_open: self.keep_open,
                                    });
                                } else if let Some(e) = candidate.get_mut(idx) {
                                    e.name = self.name.clone();
                                    e.args = self.args.clone();
                                    e.keep_open = self.keep_open;
                                }
                                save_candidate = Some(candidate);
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.edit_idx = None;
                        }
                    });
                } else {
                    let mut remove: Option<usize> = None;
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
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
                                        self.keep_open = self.entries[idx].keep_open;
                                    }
                                    if ui.button("Remove").clicked() {
                                        remove = Some(idx);
                                    }
                                });
                            }
                        });
                    if let Some(idx) = remove {
                        let mut candidate = self.entries.clone();
                        candidate.remove(idx);
                        save_candidate = Some(candidate);
                    }
                    if ui.button("Add Command").clicked() {
                        self.edit_idx = Some(self.entries.len());
                        self.name.clear();
                        self.args.clear();
                        self.keep_open = false;
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                }
            });
        if let Some(candidate) = save_candidate
            && self.save(app, candidate)
        {
            self.edit_idx = None;
            self.name.clear();
            self.args.clear();
            self.keep_open = false;
        }
        if close {
            self.open = false;
        }
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
        let args = run_enter_test(egui::Modifiers {
            shift: true,
            ..Default::default()
        });
        assert_eq!(args, "echo hi\n");
    }

    #[test]
    fn open_resets_keep_open() {
        let mut dlg = ShellCmdDialog {
            keep_open: true,
            ..Default::default()
        };
        dlg.open();
        assert!(!dlg.keep_open);
    }

    #[test]
    fn invalid_reload_and_commit_keep_last_good_and_read_only() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("commands.json");
        let initial = vec![ShellCmdEntry {
            name: "saved".into(),
            args: "echo saved".into(),
            autocomplete: true,
            keep_open: false,
        }];
        crate::plugins::shell::save_shell_cmds(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = ShellCmdDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        let invalid = b"invalid commands";
        std::fs::write(&path, invalid).unwrap();
        assert!(dialog.load_from(path.to_str().unwrap()).is_err());
        assert_eq!(dialog.entries, initial);
        assert!(dialog.load_error.is_some());
        assert!(
            dialog
                .commit_entries(path.to_str().unwrap(), Vec::new())
                .is_err()
        );
        assert_eq!(dialog.entries, initial);
        assert_eq!(std::fs::read(path).unwrap(), invalid);
    }
}
