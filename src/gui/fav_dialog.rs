use crate::gui::LauncherApp;
use crate::plugins::fav::{
    FAV_FILE, FavEntry, join_command_args, load_favs, replace_favs, resolve_with_plugin,
};
use eframe::egui;

#[derive(Default)]
pub struct FavDialog {
    pub open: bool,
    entries: Vec<FavEntry>,
    edit_idx: Option<usize>,
    label: String,
    command: String,
    args: String,
    add_plugin: String,
    add_filter: String,
    load_error: Option<String>,
}

impl FavDialog {
    pub fn open(&mut self) {
        let _ = self.load_from(FAV_FILE);
        self.open = true;
        self.edit_idx = None;
        self.label.clear();
        self.command.clear();
        self.args.clear();
        self.add_plugin.clear();
        self.add_filter.clear();
    }

    pub fn open_edit(&mut self, label: &str) {
        if self.load_from(FAV_FILE).is_err() {
            self.edit_idx = None;
            self.open = true;
            return;
        }
        if let Some(pos) = self.entries.iter().position(|e| e.label == label) {
            self.edit_idx = Some(pos);
            let entry = &self.entries[pos];
            self.label = entry.label.clone();
            self.command = entry.action.clone();
            self.args = entry.args.clone().unwrap_or_default();
        } else {
            self.edit_idx = Some(self.entries.len());
            self.label = label.to_string();
            self.command.clear();
            self.args.clear();
        }
        self.open = true;
    }

    fn load_from(&mut self, path: &str) -> anyhow::Result<()> {
        match load_favs(path) {
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

    fn commit_entries(&mut self, path: &str, candidate: Vec<FavEntry>) -> anyhow::Result<()> {
        match replace_favs(path, candidate) {
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

    fn save(&mut self, app: &mut LauncherApp, candidate: Vec<FavEntry>) -> bool {
        if let Err(e) = self.commit_entries(FAV_FILE, candidate) {
            app.report_error_message("ui operation", format!("Failed to save favorites: {e}"));
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
        egui::Window::new("Favorites")
            .open(&mut self.open)
            .show(ctx, |ui| {
                if let Some(error) = &self.load_error {
                    ui.colored_label(
                        egui::Color32::RED,
                        format!("Favorites are read-only because loading failed: {error}"),
                    );
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                    return;
                }
                if let Some(idx) = self.edit_idx {
                    ui.horizontal(|ui| {
                        ui.label("Label");
                        ui.text_edit_singleline(&mut self.label);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Command");
                        ui.text_edit_singleline(&mut self.command);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Args");
                        ui.text_edit_singleline(&mut self.args);
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Category");
                        let mut plugin_names: Vec<_> =
                            app.plugins.iter().map(|p| p.name().to_string()).collect();
                        plugin_names.sort_unstable();
                        egui::ComboBox::from_id_source("fav_cat")
                            .selected_text(if self.add_plugin.is_empty() {
                                "Select".to_string()
                            } else {
                                self.add_plugin.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in plugin_names.iter() {
                                    ui.selectable_value(
                                        &mut self.add_plugin,
                                        name.to_string(),
                                        name,
                                    );
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Filter");
                        ui.text_edit_singleline(&mut self.add_filter);
                    });
                    if let Some(plugin) = app.plugins.iter().find(|p| p.name() == self.add_plugin) {
                        let filter = self.add_filter.trim().to_lowercase();
                        let mut actions = if plugin.name() == "folders" {
                            plugin.search(&format!("f {}", self.add_filter))
                        } else if plugin.name() == "bookmarks" {
                            plugin.search(&format!("bm {}", self.add_filter))
                        } else {
                            plugin.commands()
                        };
                        egui::ScrollArea::vertical()
                            .max_height(80.0)
                            .show(ui, |ui| {
                                for act in actions.drain(..) {
                                    if !filter.is_empty()
                                        && !act.label.to_lowercase().contains(&filter)
                                        && !act.desc.to_lowercase().contains(&filter)
                                        && !act.action.to_lowercase().contains(&filter)
                                    {
                                        continue;
                                    }
                                    if ui.button(format!("{} - {}", act.label, act.desc)).clicked()
                                    {
                                        let mut cmd = act.action.clone();
                                        let mut args = if self.args.trim().is_empty() {
                                            None
                                        } else {
                                            Some(self.args.clone())
                                        };
                                        if let Some(q) = cmd.strip_prefix("query:") {
                                            let q = join_command_args(q, args.as_deref());
                                            if let Some(res) = plugin.search(&q).into_iter().next()
                                            {
                                                cmd = res.action;
                                                args = res.args;
                                            } else {
                                                cmd = q;
                                                args = None;
                                            }
                                        }
                                        self.command = cmd;
                                        self.args = args.unwrap_or_default();
                                    }
                                }
                            });
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.label.trim().is_empty() || self.command.trim().is_empty() {
                                app.report_error_message(
                                    "ui operation",
                                    "Label and command required",
                                );
                            } else {
                                let mut cmd = self.command.clone();
                                let mut args_opt = if self.args.trim().is_empty() {
                                    None
                                } else {
                                    Some(self.args.clone())
                                };
                                if let Some(plugin) =
                                    app.plugins.iter().find(|p| p.name() == self.add_plugin)
                                {
                                    let (c, a) = resolve_with_plugin(
                                        plugin.as_ref(),
                                        &cmd,
                                        args_opt.as_deref(),
                                    );
                                    cmd = c;
                                    args_opt = a;
                                }
                                let mut candidate = self.entries.clone();
                                if idx == candidate.len() {
                                    candidate.push(FavEntry {
                                        label: self.label.clone(),
                                        action: cmd.clone(),
                                        args: args_opt.clone(),
                                    });
                                } else if let Some(e) = candidate.get_mut(idx) {
                                    e.label = self.label.clone();
                                    e.action = cmd.clone();
                                    e.args = args_opt.clone();
                                }
                                save_candidate = Some(candidate);
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.edit_idx = None;
                            self.add_plugin.clear();
                            self.add_filter.clear();
                        }
                    });
                } else {
                    ui.horizontal(|ui| {
                        if ui.button("Add Fav").clicked() {
                            self.edit_idx = Some(self.entries.len());
                            self.label.clear();
                            self.command.clear();
                            self.args.clear();
                            self.add_plugin.clear();
                            self.add_filter.clear();
                        }
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                    let mut remove: Option<usize> = None;
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
                            for idx in 0..self.entries.len() {
                                let entry = self.entries[idx].clone();
                                ui.horizontal(|ui| {
                                    if ui.button("Edit").clicked() {
                                        self.edit_idx = Some(idx);
                                        self.label = entry.label.clone();
                                        self.command = entry.action.clone();
                                        self.args = entry.args.clone().unwrap_or_default();
                                    }
                                    ui.label(format!("{} - {}", entry.label, entry.action));
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
                }
            });
        if let Some(candidate) = save_candidate
            && self.save(app, candidate)
        {
            self.edit_idx = None;
            self.label.clear();
            self.command.clear();
            self.args.clear();
            self.add_plugin.clear();
            self.add_filter.clear();
        }
        if close {
            self.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FavDialog;
    use crate::plugins::fav::{FavEntry, save_favs};

    fn fav(label: &str) -> FavEntry {
        FavEntry {
            label: label.into(),
            action: format!("noop:{label}"),
            args: Some("--arg".into()),
        }
    }

    #[test]
    fn invalid_reload_and_commit_keep_dialog_last_good_and_read_only() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fav.json");
        let initial = vec![fav("saved")];
        save_favs(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = FavDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        let invalid = b"invalid favorites";
        std::fs::write(&path, invalid).unwrap();

        assert!(dialog.load_from(path.to_str().unwrap()).is_err());
        assert_eq!(dialog.entries, initial);
        assert!(dialog.load_error.is_some());
        assert!(
            dialog
                .commit_entries(path.to_str().unwrap(), vec![fav("lost")])
                .is_err()
        );
        assert_eq!(dialog.entries, initial);
        assert_eq!(std::fs::read(path).unwrap(), invalid);
        assert!(dialog.load_error.is_some());
    }
}
