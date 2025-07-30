use crate::gui::LauncherApp;
use crate::plugins::fav::{load_favs, save_favs, FavEntry, FAV_FILE};
use eframe::egui;

#[derive(Default)]
pub struct FavDialog {
    pub open: bool,
    entries: Vec<FavEntry>,
    edit_idx: Option<usize>,
    label: String,
    command: Option<(String, Option<String>)>,
    add_plugin: String,
    add_filter: String,
    add_args: String,
}

impl FavDialog {
    pub fn open(&mut self) {
        self.entries = load_favs(FAV_FILE).unwrap_or_default();
        self.open = true;
        self.edit_idx = None;
        self.label.clear();
        self.command = None;
        self.add_plugin.clear();
        self.add_filter.clear();
        self.add_args.clear();
    }

    pub fn open_add(&mut self, label: &str) {
        self.entries = load_favs(FAV_FILE).unwrap_or_default();
        self.open = true;
        self.edit_idx = Some(self.entries.len());
        self.label = label.to_string();
        self.command = None;
        self.add_plugin.clear();
        self.add_filter.clear();
        self.add_args.clear();
    }

    fn save(&mut self, app: &mut LauncherApp) {
        if let Err(e) = save_favs(FAV_FILE, &self.entries) {
            app.set_error(format!("Failed to save favorites: {e}"));
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
                    if let Some((ref label, ref args)) = self.command {
                        ui.label(format!(
                            "Command: {label} {}",
                            args.as_deref().unwrap_or("")
                        ));
                        if ui.button("Clear").clicked() {
                            self.command = None;
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.label("Category");
                        egui::ComboBox::from_id_source("fav_cat")
                            .selected_text(if self.add_plugin.is_empty() {
                                "Select".to_string()
                            } else {
                                self.add_plugin.clone()
                            })
                            .show_ui(ui, |ui| {
                                for p in app.plugins.iter() {
                                    let name = p.name();
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
                    ui.horizontal(|ui| {
                        ui.label("Args");
                        ui.text_edit_singleline(&mut self.add_args);
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
                            .max_height(100.0)
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
                                        let mut command = act.action.clone();
                                        let mut args = if self.add_args.trim().is_empty() {
                                            None
                                        } else {
                                            Some(self.add_args.clone())
                                        };
                                        if let Some(q) = command.strip_prefix("query:") {
                                            let mut q = q.to_string();
                                            if let Some(ref a) = args {
                                                q.push_str(a);
                                            }
                                            if let Some(res) = plugin.search(&q).into_iter().next()
                                            {
                                                command = res.action;
                                                args = res.args;
                                            } else {
                                                command = q;
                                                args = None;
                                            }
                                        }
                                        self.command = Some((command, args));
                                        self.add_args.clear();
                                    }
                                }
                            });
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.label.trim().is_empty() {
                                app.set_error("Label required".into());
                            } else if self.command.is_none() {
                                app.set_error("Select a command".into());
                            } else {
                                let (cmd, args) = self.command.clone().unwrap();
                                if idx == self.entries.len() {
                                    self.entries.push(FavEntry {
                                        label: self.label.clone(),
                                        action: cmd,
                                        args,
                                    });
                                } else if let Some(e) = self.entries.get_mut(idx) {
                                    e.label = self.label.clone();
                                    e.action = cmd;
                                    e.args = args;
                                }
                                self.edit_idx = None;
                                self.label.clear();
                                self.command = None;
                                self.add_plugin.clear();
                                self.add_filter.clear();
                                self.add_args.clear();
                                save_now = true;
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
                                let entry = self.entries[idx].clone();
                                ui.horizontal(|ui| {
                                    ui.label(format!("{} -> {}", entry.label, entry.action));
                                    if ui.button("Edit").clicked() {
                                        self.edit_idx = Some(idx);
                                        self.label = entry.label.clone();
                                        self.command =
                                            Some((entry.action.clone(), entry.args.clone()));
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
                    if ui.button("Add Favorite").clicked() {
                        self.edit_idx = Some(self.entries.len());
                        self.label.clear();
                        self.command = None;
                        self.add_plugin.clear();
                        self.add_filter.clear();
                        self.add_args.clear();
                    }
                    if ui.button("Close").clicked() {
                        close = true;
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
