use crate::gui::LauncherApp;
use crate::plugins::fav::{
    join_command_args, load_favs, resolve_with_plugin, save_favs, FavEntry, FAV_FILE,
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
}

impl FavDialog {
    pub fn open(&mut self) {
        self.entries = load_favs(FAV_FILE).unwrap_or_default();
        self.open = true;
        self.edit_idx = None;
        self.label.clear();
        self.command.clear();
        self.args.clear();
        self.add_plugin.clear();
        self.add_filter.clear();
    }

    pub fn open_edit(&mut self, label: &str) {
        self.entries = load_favs(FAV_FILE).unwrap_or_default();
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

    fn save(&mut self, app: &mut LauncherApp) {
        if let Err(e) = save_favs(FAV_FILE, &self.entries) {
            app.report_error_message("ui operation", format!("Failed to save favorites: {e}"));
        } else {
            app.search();
            app.focus_input();
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut close = false;
        let mut save_now = false;
        egui::Window::new("Favorites")
            .open(&mut self.open)
            .show(ctx, |ui| {
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
                                    "Label and command required".into(),
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
                                if idx == self.entries.len() {
                                    self.entries.push(FavEntry {
                                        label: self.label.clone(),
                                        action: cmd.clone(),
                                        args: args_opt.clone(),
                                    });
                                } else if let Some(e) = self.entries.get_mut(idx) {
                                    e.label = self.label.clone();
                                    e.action = cmd.clone();
                                    e.args = args_opt.clone();
                                }
                                self.edit_idx = None;
                                self.label.clear();
                                self.command.clear();
                                self.args.clear();
                                self.add_plugin.clear();
                                self.add_filter.clear();
                                save_now = true;
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
                        self.entries.remove(idx);
                        save_now = true;
                    }
                }
            });
        if save_now {
            self.save(app);
        }
        if close {
            self.open = false;
        }
    }
}
