use crate::gui::LauncherApp;
use crate::plugins::macros::{load_macros, save_macros, MacroEntry, MacroStep, MACROS_FILE};
use eframe::egui;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use log::debug;

/// Dialog for creating and editing macros.
///
/// `category_filter` stores user input for a fuzzy search over plugin names,
/// including a special `app` category listing all configured applications.
/// Matching names are shown in a scrollable list; selecting one writes it to
/// `add_plugin` and clears the filter.
pub struct MacroDialog {
    pub open: bool,
    entries: Vec<MacroEntry>,
    edit_idx: Option<usize>,
    label: String,
    desc: String,
    steps: Vec<MacroStep>,
    auto_delay: bool,
    auto_delay_secs: f32,
    add_plugin: String,
    category_filter: String,
    add_filter: String,
    add_args: String,
}

impl Default for MacroDialog {
    fn default() -> Self {
        Self {
            open: false,
            entries: Vec::new(),
            edit_idx: None,
            label: String::new(),
            desc: String::new(),
            steps: Vec::new(),
            auto_delay: false,
            auto_delay_secs: 1.0,
            add_plugin: String::new(),
            category_filter: String::new(),
            add_filter: String::new(),
            add_args: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_plugins_returns_all_when_filter_empty() {
        let dlg = MacroDialog::default();
        let plugins = ["alpha", "beta", "app"];
        let matches = MacroDialog::matching_plugins(&dlg.category_filter, plugins.iter().copied());
        assert_eq!(matches, vec!["alpha", "app", "beta"]);
    }

    #[test]
    fn matching_plugins_returns_empty_when_no_match() {
        let dlg = MacroDialog {
            category_filter: "zzz".into(),
            ..Default::default()
        };
        let plugins = ["alpha", "beta", "app"];
        let matches = MacroDialog::matching_plugins(&dlg.category_filter, plugins.iter().copied());
        assert!(matches.is_empty());
    }

    #[test]
    fn matching_plugins_is_case_insensitive() {
        let dlg = MacroDialog {
            category_filter: "AP".into(),
            ..Default::default()
        };
        let plugins = ["alpha", "beta", "app"];
        let matches = MacroDialog::matching_plugins(&dlg.category_filter, plugins.iter().copied());
        assert_eq!(matches, vec!["alpha", "app"]);
    }

    #[test]
    fn fuzzy_filter_lists_matching_plugins() {
        let dlg = MacroDialog {
            category_filter: "ap".into(),
            ..Default::default()
        };
        let plugins = ["alpha", "beta", "app"];
        let matches = MacroDialog::matching_plugins(&dlg.category_filter, plugins.iter().copied());
        assert_eq!(matches, vec!["alpha", "app"]);
    }

    #[test]
    fn selecting_plugin_after_filtering_updates_state() {
        let mut dlg = MacroDialog {
            category_filter: "ap".into(),
            ..Default::default()
        };
        let plugins = ["alpha", "app"];
        let matches = MacroDialog::matching_plugins(&dlg.category_filter, plugins.iter().copied());
        MacroDialog::select_plugin(&mut dlg.add_plugin, &mut dlg.category_filter, matches[0]);
        assert_eq!(dlg.add_plugin, "alpha");
        assert!(dlg.category_filter.is_empty());
    }

    #[test]
    fn app_category_is_included_and_selectable() {
        let mut dlg = MacroDialog {
            category_filter: "ap".into(),
            ..Default::default()
        };
        let plugins = ["alpha", "app"];
        let matches = MacroDialog::matching_plugins(&dlg.category_filter, plugins.iter().copied());
        assert!(matches.contains(&"app"));
        MacroDialog::select_plugin(&mut dlg.add_plugin, &mut dlg.category_filter, "app");
        assert_eq!(dlg.add_plugin, "app");
        assert!(dlg.category_filter.is_empty());
    }
}

impl MacroDialog {
    /// Load macros and reset dialog state, including the category filter.
    pub fn open(&mut self) {
        self.entries = load_macros(MACROS_FILE).unwrap_or_default();
        self.open = true;
        self.edit_idx = None;
        self.label.clear();
        self.desc.clear();
        self.steps.clear();
        self.auto_delay = false;
        self.auto_delay_secs = 1.0;
        self.add_plugin.clear();
        self.category_filter.clear();
        self.add_filter.clear();
        self.add_args.clear();
    }

    fn save(&mut self, app: &mut LauncherApp) {
        if let Err(e) = save_macros(MACROS_FILE, &self.entries) {
            app.set_error(format!("Failed to save macros: {e}"));
        } else {
            app.search();
            app.focus_input();
        }
    }

    /// Return plugin names sorted alphabetically and filtered by `filter`.
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

    /// Record the selected plugin and clear the category filter.
    fn select_plugin(add_plugin: &mut String, category_filter: &mut String, name: &str) {
        debug!("select_plugin: {name}");
        *add_plugin = name.to_string();
        category_filter.clear();
    }

    /// Render the dialog, including the fuzzy plugin picker used when adding
    /// macro steps. Plugin names are filtered with `SkimMatcherV2`; choosing a
    /// name stores it in `add_plugin` and clears `category_filter`.
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut close = false;
        let mut save_now = false;
        let mut open = self.open;
        egui::Window::new("Macros").open(&mut open).show(ctx, |ui| {
            if let Some(idx) = self.edit_idx {
                ui.horizontal(|ui| {
                    ui.label("Label");
                    ui.text_edit_singleline(&mut self.label);
                });
                ui.horizontal(|ui| {
                    ui.label("Description");
                    ui.text_edit_singleline(&mut self.desc);
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.auto_delay, "Automatic delay");
                    if self.auto_delay {
                        ui.add(
                            egui::DragValue::new(&mut self.auto_delay_secs)
                                .speed(0.1)
                                .clamp_range(0.0..=60.0)
                                .suffix("s"),
                        );
                    }
                });
                ui.label("Steps");
                let mut remove_step: Option<usize> = None;
                let mut move_up: Option<usize> = None;
                let mut move_down: Option<usize> = None;
                for i in 0..self.steps.len() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}.", i + 1));
                        ui.label(&self.steps[i].label);
                        ui.label("Args");
                        let args = self.steps[i].args.get_or_insert_with(String::new);
                        ui.text_edit_singleline(args);
                        if !self.auto_delay {
                            let mut secs = self.steps[i].delay_ms as f32 / 1000.0;
                            ui.add(
                                egui::DragValue::new(&mut secs)
                                    .speed(0.1)
                                    .clamp_range(0.0..=60.0)
                                    .suffix("s"),
                            );
                            self.steps[i].delay_ms = (secs * 1000.0) as u64;
                        }
                        if ui.button("Up").clicked() {
                            move_up = Some(i);
                        }
                        if ui.button("Down").clicked() {
                            move_down = Some(i);
                        }
                        if ui.button("Remove").clicked() {
                            remove_step = Some(i);
                        }
                    });
                }
                if let Some(i) = move_up {
                    if i > 0 {
                        self.steps.swap(i, i - 1);
                    }
                }
                if let Some(i) = move_down {
                    if i + 1 < self.steps.len() {
                        self.steps.swap(i, i + 1);
                    }
                }
                if let Some(i) = remove_step {
                    self.steps.remove(i);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Category");
                    // Free-form text input used to fuzzily match plugin names.
                    let resp = ui.text_edit_singleline(&mut self.category_filter);
                    if resp.changed() {
                        debug!("category_filter set to '{}'", self.category_filter);
                    }
                });
                // Collect plugin names matching the fuzzy category filter,
                // including the special `app` category.
                let plugin_names = MacroDialog::matching_plugins(
                    &self.category_filter,
                    app.plugins
                        .iter()
                        .map(|p| p.name())
                        .chain(std::iter::once("app")),
                );
                egui::ScrollArea::vertical()
                    .id_source("macro_plugin_list")
                    .max_height(100.0)
                    .show(ui, |ui| {
                        for name in plugin_names {
                            // Choosing a plugin stores it and clears the filter.
                            if ui.button(name).clicked() {
                                debug!("selected plugin {name}");
                                MacroDialog::select_plugin(
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
                        .id_source("macro_app_list")
                        .max_height(100.0)
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
                                    let args = if self.add_args.trim().is_empty() {
                                        act.args.clone()
                                    } else {
                                        Some(self.add_args.clone())
                                    };
                                    self.steps.push(MacroStep {
                                        label: act.label.clone(),
                                        command: act.action.clone(),
                                        args,
                                        delay_ms: 0,
                                    });
                                    self.add_args.clear();
                                }
                            }
                        });
                } else if let Some(plugin) =
                    app.plugins.iter().find(|p| p.name() == self.add_plugin)
                {
                    let filter = self.add_filter.trim().to_lowercase();
                    let mut actions = if plugin.name() == "folders" {
                        plugin.search(&format!("f {}", self.add_filter))
                    } else if plugin.name() == "bookmarks" {
                        plugin.search(&format!("bm {}", self.add_filter))
                    } else {
                        plugin.commands()
                    };
                    egui::ScrollArea::vertical()
                        .id_source("macro_action_list")
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

                                    self.steps.push(MacroStep {
                                        label: act.label.clone(),
                                        command,
                                        args,
                                        delay_ms: 0,
                                    });
                                    // Reset pending args after adding a step.
                                    self.add_args.clear();
                                }
                            }
                        });
                }
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        if self.label.trim().is_empty() {
                            app.set_error("Label required".into());
                        } else {
                            for step in &mut self.steps {
                                if let Some(a) = &step.args {
                                    if a.trim().is_empty() {
                                        step.args = None;
                                    }
                                }
                            }
                            if idx == self.entries.len() {
                                self.entries.push(MacroEntry {
                                    label: self.label.clone(),
                                    desc: self.desc.clone(),
                                    auto_delay_ms: if self.auto_delay {
                                        Some((self.auto_delay_secs * 1000.0) as u64)
                                    } else {
                                        None
                                    },
                                    steps: self.steps.clone(),
                                });
                            } else if let Some(e) = self.entries.get_mut(idx) {
                                e.label = self.label.clone();
                                e.desc = self.desc.clone();
                                e.auto_delay_ms = if self.auto_delay {
                                    Some((self.auto_delay_secs * 1000.0) as u64)
                                } else {
                                    None
                                };
                                e.steps = self.steps.clone();
                            }
                            self.edit_idx = None;
                            self.label.clear();
                            self.desc.clear();
                            self.steps.clear();
                            self.auto_delay = false;
                            self.auto_delay_secs = 1.0;
                            // Clear temporary plugin selection state after saving.
                            self.add_plugin.clear();
                            self.add_filter.clear();
                            self.add_args.clear();
                            save_now = true;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.edit_idx = None;
                        self.auto_delay = false;
                        self.auto_delay_secs = 1.0;
                        // Cancel editing and reset plugin selection state.
                        self.add_plugin.clear();
                        self.add_filter.clear();
                        self.add_args.clear();
                    }
                });
            } else {
                let mut remove: Option<usize> = None;
                egui::ScrollArea::vertical()
                    .id_source("macro_entry_list")
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for idx in 0..self.entries.len() {
                            let entry = &self.entries[idx];
                            ui.horizontal(|ui| {
                                ui.label(format!("{}: {}", entry.label, entry.desc));
                                if ui.button("Edit").clicked() {
                                    self.edit_idx = Some(idx);
                                    self.label = entry.label.clone();
                                    self.desc = entry.desc.clone();
                                    self.steps = entry.steps.clone();
                                    if let Some(ms) = entry.auto_delay_ms {
                                        self.auto_delay = true;
                                        self.auto_delay_secs = ms as f32 / 1000.0;
                                    } else {
                                        self.auto_delay = false;
                                        self.auto_delay_secs = 1.0;
                                    }
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
                if ui.button("Add Macro").clicked() {
                    self.edit_idx = Some(self.entries.len());
                    self.label.clear();
                    self.desc.clear();
                    self.steps.clear();
                    self.auto_delay = false;
                    self.auto_delay_secs = 1.0;
                    self.add_plugin.clear();
                    self.add_filter.clear();
                    self.add_args.clear();
                }
                if ui.button("Close").clicked() {
                    close = true;
                }
            }
        });
        self.open = open;
        if save_now {
            self.save(app);
        }
        if close {
            self.open = false;
        }
    }
}
