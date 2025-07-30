use crate::gui::LauncherApp;
use crate::plugins::fav::{load_favs, save_favs, FavEntry, FAV_FILE};
use eframe::egui;

pub struct FavDialog {
    pub open: bool,
    entries: Vec<FavEntry>,
    edit_idx: Option<usize>,
    label: String,
    action: String,
    args: String,
    add_plugin: String,
    add_filter: String,
}

impl Default for FavDialog {
    fn default() -> Self {
        Self {
            open: false,
            entries: Vec::new(),
            edit_idx: None,
            label: String::new(),
            action: String::new(),
            args: String::new(),
            add_plugin: String::new(),
            add_filter: String::new(),
        }
    }
}

impl FavDialog {
    pub fn open(&mut self) {
        self.entries = load_favs(FAV_FILE).unwrap_or_default();
        self.open = true;
        self.edit_idx = None;
        self.label.clear();
        self.action.clear();
        self.args.clear();
        self.add_plugin.clear();
        self.add_filter.clear();
    }

    pub fn open_add(&mut self, label: &str) {
        self.entries = load_favs(FAV_FILE).unwrap_or_default();
        self.open = true;
        self.edit_idx = Some(self.entries.len());
        self.label = label.to_string();
        self.action.clear();
        self.args.clear();
        self.add_plugin.clear();
        self.add_filter.clear();
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
            .resizable(true)
            .default_size((360.0, 240.0))
            .show(ctx, |ui| {
                if let Some(idx) = self.edit_idx {
                    ui.horizontal(|ui| {
                        ui.label("Label");
                        ui.text_edit_singleline(&mut self.label);
                    });
                    if !self.action.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label("Command");
                            ui.monospace(&self.action);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Args");
                            ui.text_edit_singleline(&mut self.args);
                        });
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Category");
                        egui::ComboBox::from_id_source("fav_plugin")
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
                    if let Some(plugin) = app.plugins.iter().find(|p| p.name() == self.add_plugin) {
                        let filter = self.add_filter.trim().to_lowercase();
                        let mut actions = plugin.commands();
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
                                        self.action = act.action;
                                        self.args = act.args.unwrap_or_default();
                                        self.add_plugin.clear();
                                        self.add_filter.clear();
                                    }
                                }
                            });
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.label.trim().is_empty() || self.action.is_empty() {
                                app.set_error("Label and command required".into());
                            } else {
                                let args = if self.args.trim().is_empty() {
                                    None
                                } else {
                                    Some(self.args.clone())
                                };
                                if idx == self.entries.len() {
                                    self.entries.push(FavEntry {
                                        label: self.label.clone(),
                                        action: self.action.clone(),
                                        args,
                                    });
                                } else if let Some(e) = self.entries.get_mut(idx) {
                                    e.label = self.label.clone();
                                    e.action = self.action.clone();
                                    e.args = args;
                                }
                                self.edit_idx = None;
                                self.label.clear();
                                self.action.clear();
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
                    let mut remove: Option<usize> = None;
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
                            for idx in 0..self.entries.len() {
                                let entry = &self.entries[idx];
                                ui.horizontal(|ui| {
                                    ui.label(&entry.label);
                                    if ui.button("Edit").clicked() {
                                        self.edit_idx = Some(idx);
                                        self.label = entry.label.clone();
                                        self.action = entry.action.clone();
                                        self.args = entry.args.clone().unwrap_or_default();
                                        self.add_plugin.clear();
                                        self.add_filter.clear();
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
                        self.action.clear();
                        self.args.clear();
                        self.add_plugin.clear();
                        self.add_filter.clear();
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
