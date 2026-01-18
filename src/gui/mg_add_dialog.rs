use crate::mouse_gestures::mouse_gesture_service;
use crate::plugins::mouse_gestures::db::{
    load_gestures, save_gestures, MouseGestureBinding, MouseGestureDb, MouseGestureProfile,
    MOUSE_GESTURES_FILE,
};
use eframe::egui;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use log::debug;

#[derive(Default)]
pub struct MouseGesturesAddDialog {
    pub open: bool,
    loaded: bool,
    db: MouseGestureDb,
    selected_gesture: Option<String>,
    binding_label: String,
    binding_priority: i32,
    add_plugin: String,
    category_filter: String,
    add_filter: String,
    add_args: String,
    selected_action: Option<ActionChoice>,
    status: Option<String>,
}

#[derive(Clone)]
struct ActionChoice {
    label: String,
    action: String,
    args: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_plugins_returns_all_when_filter_empty() {
        let dlg = MouseGesturesAddDialog::default();
        let plugins = ["alpha", "beta", "app"];
        let matches =
            MouseGesturesAddDialog::matching_plugins(&dlg.category_filter, plugins.iter().copied());
        assert_eq!(matches, vec!["alpha", "app", "beta"]);
    }

    #[test]
    fn matching_plugins_returns_empty_when_no_match() {
        let dlg = MouseGesturesAddDialog {
            category_filter: "zzz".into(),
            ..Default::default()
        };
        let plugins = ["alpha", "beta", "app"];
        let matches =
            MouseGesturesAddDialog::matching_plugins(&dlg.category_filter, plugins.iter().copied());
        assert!(matches.is_empty());
    }

    #[test]
    fn selecting_plugin_after_filtering_updates_state() {
        let mut dlg = MouseGesturesAddDialog {
            category_filter: "ap".into(),
            ..Default::default()
        };
        let plugins = ["alpha", "app"];
        let matches =
            MouseGesturesAddDialog::matching_plugins(&dlg.category_filter, plugins.iter().copied());
        MouseGesturesAddDialog::select_plugin(
            &mut dlg.add_plugin,
            &mut dlg.category_filter,
            matches[0],
        );
        assert_eq!(dlg.add_plugin, "alpha");
        assert!(dlg.category_filter.is_empty());
    }
}

impl MouseGesturesAddDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.loaded = false;
    }

    fn load_db(&mut self) {
        self.db = load_gestures(MOUSE_GESTURES_FILE).unwrap_or_default();
        if self.db.profiles.is_empty() {
            self.db.profiles.push(default_profile());
        }
        self.loaded = true;
    }

    fn persist_db(&mut self, app: &mut crate::gui::LauncherApp) {
        if let Err(e) = save_gestures(MOUSE_GESTURES_FILE, &self.db) {
            app.set_error(format!("Failed to save gestures: {e}"));
        } else {
            mouse_gesture_service().update_db(self.db.clone());
        }
    }

    fn matching_plugins<'a>(filter: &str, names: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
        let matcher = SkimMatcherV2::default();
        let mut names: Vec<&'a str> = names.collect();
        let total = names.len();
        let filter = filter.to_lowercase();
        names.sort_unstable();
        let filtered: Vec<&'a str> = names
            .into_iter()
            .filter(|name| {
                filter.is_empty() || matcher.fuzzy_match(&name.to_lowercase(), &filter).is_some()
            })
            .collect();
        if !filter.is_empty() {
            debug!(
                "matching_plugins: filter '{filter}' returned {} of {total}",
                filtered.len()
            );
        }
        filtered
    }

    fn select_plugin(add_plugin: &mut String, category_filter: &mut String, name: &str) {
        debug!("select_plugin: {name}");
        *add_plugin = name.to_string();
        category_filter.clear();
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut crate::gui::LauncherApp) {
        if !self.open {
            return;
        }
        if !self.loaded {
            self.load_db();
        }

        let mut open = self.open;
        egui::Window::new("Edit Mouse Gestures")
            .open(&mut open)
            .show(ctx, |ui| {
                let gestures = gesture_labels(&self.db);
                let profile = ensure_default_profile(&mut self.db);
                ui.label("Bindings");
                egui::ScrollArea::vertical()
                    .id_source("mg_add_binding_list")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        let mut remove_index: Option<usize> = None;
                        for (idx, binding) in profile.bindings.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut binding.enabled, "");
                                ui.text_edit_singleline(&mut binding.label);
                                let selected_text =
                                    gesture_label_for(&gestures, &binding.gesture_id)
                                        .unwrap_or_else(|| binding.gesture_id.clone());
                                egui::ComboBox::from_id_source(format!("mg_binding_gesture_{idx}"))
                                    .selected_text(selected_text)
                                    .show_ui(ui, |ui| {
                                        for (gesture_id, label) in &gestures {
                                            ui.selectable_value(
                                                &mut binding.gesture_id,
                                                gesture_id.clone(),
                                                format!("{label} ({gesture_id})"),
                                            );
                                        }
                                    });
                                ui.label(&binding.action);
                                if ui.button("Remove").clicked() {
                                    remove_index = Some(idx);
                                }
                            });
                        }
                        if let Some(idx) = remove_index {
                            profile.bindings.remove(idx);
                            self.persist_db(app);
                        }
                    });
                if ui.button("Save changes").clicked() {
                    self.persist_db(app);
                    self.status = Some("Bindings saved.".to_string());
                }

                ui.separator();
                ui.label("New binding");
                ui.horizontal(|ui| {
                    ui.label("Binding name");
                    ui.text_edit_singleline(&mut self.binding_label);
                });
                ui.label("Gesture");
                if ui.button("Open recorder").clicked() {
                    app.mouse_gestures_gesture_dialog.open();
                }
                egui::ScrollArea::vertical()
                    .id_source("mg_add_gesture_list")
                    .max_height(120.0)
                    .show(ui, |ui| {
                        for (gesture_id, label) in &gestures {
                            let selected = self
                                .selected_gesture
                                .as_deref()
                                .map(|id| id == gesture_id)
                                .unwrap_or(false);
                            if ui
                                .selectable_label(selected, format!("{label} ({gesture_id})"))
                                .clicked()
                            {
                                self.selected_gesture = Some(gesture_id.clone());
                            }
                        }
                    });

                ui.separator();
                ui.label("Action");
                ui.horizontal(|ui| {
                    ui.label("Category");
                    ui.text_edit_singleline(&mut self.category_filter);
                });
                let plugin_names = MouseGesturesAddDialog::matching_plugins(
                    &self.category_filter,
                    app.plugins
                        .iter()
                        .map(|p| p.name())
                        .chain(std::iter::once("app")),
                );
                egui::ScrollArea::vertical()
                    .id_source("mg_add_plugin_list")
                    .max_height(100.0)
                    .show(ui, |ui| {
                        for name in plugin_names {
                            if ui.button(name).clicked() {
                                MouseGesturesAddDialog::select_plugin(
                                    &mut self.add_plugin,
                                    &mut self.category_filter,
                                    name,
                                );
                            }
                        }
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
                        .id_source("mg_add_app_list")
                        .max_height(140.0)
                        .show(ui, |ui| {
                            for act in app.actions.iter() {
                                if !filter.is_empty()
                                    && !act.label.to_lowercase().contains(&filter)
                                    && !act.desc.to_lowercase().contains(&filter)
                                    && !act.action.to_lowercase().contains(&filter)
                                {
                                    continue;
                                }
                                if ui.button(format!("{} - {}", act.label, act.desc)).clicked() {
                                    self.selected_action = Some(ActionChoice {
                                        label: act.label.clone(),
                                        action: act.action.clone(),
                                        args: if self.add_args.trim().is_empty() {
                                            act.args.clone()
                                        } else {
                                            Some(self.add_args.clone())
                                        },
                                    });
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
                        .id_source("mg_add_action_list")
                        .max_height(140.0)
                        .show(ui, |ui| {
                            for act in actions.drain(..) {
                                if !filter.is_empty()
                                    && !act.label.to_lowercase().contains(&filter)
                                    && !act.desc.to_lowercase().contains(&filter)
                                    && !act.action.to_lowercase().contains(&filter)
                                {
                                    continue;
                                }
                                if ui.button(format!("{} - {}", act.label, act.desc)).clicked() {
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
                                        if let Some(res) = plugin.search(&q).into_iter().next() {
                                            command = res.action;
                                            args = res.args;
                                        } else {
                                            command = q;
                                            args = None;
                                        }
                                    }
                                    self.selected_action = Some(ActionChoice {
                                        label: act.label.clone(),
                                        action: command,
                                        args,
                                    });
                                }
                            }
                        });
                }

                if let Some(choice) = &self.selected_action {
                    ui.label(format!("Selected: {} ({})", choice.label, choice.action));
                }

                ui.horizontal(|ui| {
                    ui.label("Priority");
                    ui.add(egui::DragValue::new(&mut self.binding_priority));
                });

                if ui.button("Add binding").clicked() {
                    self.handle_add_binding(app);
                }

                if let Some(status) = &self.status {
                    ui.label(status);
                }
            });

        self.open = open;
    }

    fn handle_add_binding(&mut self, app: &mut crate::gui::LauncherApp) {
        let Some(gesture_id) = self.selected_gesture.clone() else {
            self.status = Some("Select a gesture first".to_string());
            return;
        };
        let Some(choice) = self.selected_action.clone() else {
            self.status = Some("Select an action first".to_string());
            return;
        };
        if self.binding_label.trim().is_empty() {
            self.status = Some("Binding name is required".to_string());
            return;
        }

        let profile = ensure_default_profile(&mut self.db);
        profile.bindings.push(MouseGestureBinding {
            gesture_id,
            label: self.binding_label.trim().to_string(),
            action: choice.action,
            args: choice.args,
            priority: self.binding_priority,
            enabled: true,
        });
        self.persist_db(app);
        self.binding_label.clear();
        self.selected_action = None;
        self.status = Some("Binding added".to_string());
    }
}

fn default_profile() -> MouseGestureProfile {
    MouseGestureProfile {
        id: "default".to_string(),
        label: "Default".to_string(),
        enabled: true,
        priority: 0,
        rules: Vec::new(),
        bindings: Vec::new(),
    }
}

fn ensure_default_profile(db: &mut MouseGestureDb) -> &mut MouseGestureProfile {
    if db.profiles.is_empty() {
        db.profiles.push(default_profile());
    }
    db.profiles
        .iter_mut()
        .find(|profile| profile.id == "default")
        .unwrap_or_else(|| {
            db.profiles.push(default_profile());
            db.profiles.last_mut().expect("default profile")
        })
}

fn gesture_labels(db: &MouseGestureDb) -> Vec<(String, String)> {
    let mut items: Vec<(String, String)> = db
        .bindings
        .iter()
        .map(|(id, serialized)| {
            let label = crate::plugins::mouse_gestures::engine::parse_gesture(serialized)
                .ok()
                .and_then(|g| g.name)
                .unwrap_or_else(|| "(unnamed)".to_string());
            (id.clone(), label)
        })
        .collect();
    items.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
    items
}

fn gesture_label_for(gestures: &[(String, String)], gesture_id: &str) -> Option<String> {
    gestures
        .iter()
        .find(|(id, _)| id == gesture_id)
        .map(|(_, label)| label.clone())
}
