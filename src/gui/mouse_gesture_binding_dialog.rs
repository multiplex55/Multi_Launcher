use crate::gui::LauncherApp;
use crate::mouse_gestures::db::{
    format_gesture_label, load_gestures, save_gestures, BindingEntry, GestureDb, GESTURES_FILE,
};
use eframe::egui;

pub struct MgBindingDialog {
    pub open: bool,
    db: GestureDb,
    selected_gesture: Option<usize>,
    edit_idx: Option<usize>,
    label: String,
    action: String,
    args: String,
    enabled: bool,
    use_query: bool,
    add_plugin: String,
    add_filter: String,
    add_args: String,
}

impl Default for MgBindingDialog {
    fn default() -> Self {
        Self {
            open: false,
            db: GestureDb::default(),
            selected_gesture: None,
            edit_idx: None,
            label: String::new(),
            action: String::new(),
            args: String::new(),
            enabled: true,
            use_query: false,
            add_plugin: String::new(),
            add_filter: String::new(),
            add_args: String::new(),
        }
    }
}

impl MgBindingDialog {
    pub fn open(&mut self) {
        self.db = load_gestures(GESTURES_FILE).unwrap_or_default();
        self.open = true;
        self.selected_gesture = if self.db.gestures.is_empty() {
            None
        } else {
            Some(0)
        };
        self.reset_edit();
    }

    fn reset_edit(&mut self) {
        self.edit_idx = None;
        self.label.clear();
        self.action.clear();
        self.args.clear();
        self.enabled = true;
        self.use_query = false;
        self.add_plugin.clear();
        self.add_filter.clear();
        self.add_args.clear();
    }

    fn start_edit(&mut self, binding: Option<&BindingEntry>, idx: usize) {
        if let Some(binding) = binding {
            let (action, use_query) = if let Some(rest) = binding.action.strip_prefix("query:") {
                (rest.to_string(), true)
            } else {
                (binding.action.clone(), false)
            };
            self.label = binding.label.clone();
            self.action = action;
            self.args = binding.args.clone().unwrap_or_default();
            self.enabled = binding.enabled;
            self.use_query = use_query;
        } else {
            self.label.clear();
            self.action.clear();
            self.args.clear();
            self.enabled = true;
            self.use_query = false;
        }
        self.edit_idx = Some(idx);
        self.add_plugin.clear();
        self.add_filter.clear();
        self.add_args.clear();
    }

    fn save(&mut self, app: &mut LauncherApp) {
        if let Err(e) = save_gestures(GESTURES_FILE, &self.db) {
            app.set_error(format!("Failed to save mouse gesture bindings: {e}"));
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
        let mut open = self.open;
        egui::Window::new("Mouse Gesture Bindings")
            .open(&mut open)
            .show(ctx, |ui| {
                if self.db.gestures.is_empty() {
                    ui.label("No gestures found. Add a gesture first.");
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                    return;
                }
                ui.horizontal(|ui| {
                    ui.label("Gesture");
                    let mut selected = self.selected_gesture.unwrap_or(0);
                    egui::ComboBox::from_id_source("mg_binding_gesture")
                        .selected_text(
                            self.db
                                .gestures
                                .get(selected)
                                .map(format_gesture_label)
                                .unwrap_or_else(|| "Select".into()),
                        )
                        .show_ui(ui, |ui| {
                            for (idx, gesture) in self.db.gestures.iter().enumerate() {
                                ui.selectable_value(
                                    &mut selected,
                                    idx,
                                    format_gesture_label(gesture),
                                );
                            }
                        });
                    if Some(selected) != self.selected_gesture {
                        self.selected_gesture = Some(selected);
                        self.reset_edit();
                    }
                });
                if let Some(gesture_idx) = self.selected_gesture {
                    if let Some(edit_idx) = self.edit_idx {
                        let mut save_entry: Option<BindingEntry> = None;
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Label");
                            ui.text_edit_singleline(&mut self.label);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Action");
                            ui.text_edit_singleline(&mut self.action);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Args");
                            ui.text_edit_singleline(&mut self.args);
                        });
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut self.use_query, "Use query action");
                            ui.checkbox(&mut self.enabled, "Enabled");
                        });
                        ui.separator();
                        ui.label("Pick an action");
                        ui.horizontal(|ui| {
                            ui.label("Category");
                            let mut plugin_names: Vec<_> =
                                app.plugins.iter().map(|p| p.name().to_string()).collect();
                            plugin_names.push("app".to_string());
                            plugin_names.sort_unstable();
                            egui::ComboBox::from_id_source("mg_binding_category")
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
                        ui.horizontal(|ui| {
                            ui.label("Args");
                            ui.text_edit_singleline(&mut self.add_args);
                        });
                        if self.add_plugin == "app" {
                            let filter = self.add_filter.trim().to_lowercase();
                            egui::ScrollArea::vertical()
                                .id_source("mg_binding_app_list")
                                .max_height(120.0)
                                .show(ui, |ui| {
                                    for act in app.actions.iter() {
                                        if !filter.is_empty()
                                            && !act.label.to_lowercase().contains(&filter)
                                            && !act.desc.to_lowercase().contains(&filter)
                                            && !act.action.to_lowercase().contains(&filter)
                                        {
                                            continue;
                                        }
                                        if ui
                                            .button(format!("{} - {}", act.label, act.desc))
                                            .clicked()
                                        {
                                            self.label = act.label.clone();
                                            self.use_query = false;
                                            self.action = act.action.clone();
                                            self.args = act.args.clone().unwrap_or_default();
                                            self.add_args.clear();
                                        }
                                    }
                                });
                        } else if let Some(plugin) =
                            app.plugins.iter().find(|p| p.name() == self.add_plugin)
                        {
                            let filter = self.add_filter.trim().to_lowercase();
                            let mut actions = if plugin.name() == "folders" {
                                plugin.search(&format!("f list {}", self.add_filter))
                            } else if plugin.name() == "bookmarks" {
                                plugin.search(&format!("bm list {}", self.add_filter))
                            } else {
                                plugin.commands()
                            };
                            egui::ScrollArea::vertical()
                                .id_source("mg_binding_action_list")
                                .max_height(120.0)
                                .show(ui, |ui| {
                                    for act in actions.drain(..) {
                                        if !filter.is_empty()
                                            && !act.label.to_lowercase().contains(&filter)
                                            && !act.desc.to_lowercase().contains(&filter)
                                            && !act.action.to_lowercase().contains(&filter)
                                        {
                                            continue;
                                        }
                                        if ui
                                            .button(format!("{} - {}", act.label, act.desc))
                                            .clicked()
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
                                                if let Some(res) =
                                                    plugin.search(&q).into_iter().next()
                                                {
                                                    command = res.action;
                                                    args = res.args;
                                                } else {
                                                    command = q;
                                                    args = None;
                                                }
                                            }

                                            let (action, use_query) = if let Some(rest) =
                                                command.strip_prefix("query:")
                                            {
                                                (rest.to_string(), true)
                                            } else {
                                                (command, false)
                                            };
                                            self.label = act.label.clone();
                                            self.use_query = use_query;
                                            self.action = action;
                                            self.args = args.unwrap_or_default();
                                            self.add_args.clear();
                                        }
                                    }
                                });
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Save").clicked() {
                                if self.label.trim().is_empty() || self.action.trim().is_empty() {
                                    app.set_error("Label and action required".into());
                                } else {
                                    let action = if self.use_query {
                                        format!("query:{}", self.action.trim())
                                    } else {
                                        self.action.trim().to_string()
                                    };
                                    let args = if self.args.trim().is_empty() {
                                        None
                                    } else {
                                        Some(self.args.trim().to_string())
                                    };
                                    let entry = BindingEntry {
                                        label: self.label.trim().to_string(),
                                        action,
                                        args,
                                        enabled: self.enabled,
                                    };
                                    save_entry = Some(entry);
                                }
                            }
                            if ui.button("Cancel").clicked() {
                                self.reset_edit();
                            }
                        });
                        if let Some(entry) = save_entry {
                            let bindings = &mut self.db.gestures[gesture_idx].bindings;
                            if edit_idx >= bindings.len() {
                                bindings.push(entry);
                            } else if let Some(binding) = bindings.get_mut(edit_idx) {
                                *binding = entry;
                            }
                            self.reset_edit();
                            save_now = true;
                        }
                    } else {
                        ui.horizontal(|ui| {
                            if ui.button("Add Binding").clicked() {
                                let next_idx = self.db.gestures[gesture_idx].bindings.len();
                                self.start_edit(None, next_idx);
                            }
                            if ui.button("Close").clicked() {
                                close = true;
                            }
                        });
                        ui.separator();
                        let mut remove_idx: Option<usize> = None;
                        let mut edit_request: Option<(usize, BindingEntry)> = None;
                        egui::ScrollArea::vertical()
                            .max_height(220.0)
                            .show(ui, |ui| {
                                for (idx, binding) in self.db.gestures[gesture_idx]
                                    .bindings
                                    .iter_mut()
                                    .enumerate()
                                {
                                    ui.horizontal(|ui| {
                                        if ui.checkbox(&mut binding.enabled, "").changed() {
                                            save_now = true;
                                        }
                                        ui.label(&binding.label);
                                        ui.label(&binding.action);
                                        if ui.button("Edit").clicked() {
                                            edit_request = Some((idx, binding.clone()));
                                        }
                                        if ui.button("Remove").clicked() {
                                            remove_idx = Some(idx);
                                        }
                                    });
                                }
                            });
                        if let Some((idx, binding)) = edit_request {
                            self.start_edit(Some(&binding), idx);
                        }
                        if let Some(idx) = remove_idx {
                            self.db.gestures[gesture_idx].bindings.remove(idx);
                            save_now = true;
                        }
                    }
                }
            });
        if save_now {
            self.save(app);
        }
        if close {
            self.open = false;
        } else {
            self.open = open;
        }
    }
}
