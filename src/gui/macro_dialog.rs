use crate::gui::LauncherApp;
use crate::plugins::macros::{load_macros, save_macros, MacroEntry, MacroStep, MACROS_FILE};
use eframe::egui;

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
    add_filter: String,
    add_args: String,
    debug: Vec<String>,
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
            add_filter: String::new(),
            add_args: String::new(),
            debug: Vec::new(),
        }
    }
}

impl MacroDialog {
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
        self.add_filter.clear();
        self.add_args.clear();
        self.debug.clear();
    }

    pub fn push_debug(&mut self, msg: String) {
        self.debug.push(msg);
        if self.debug.len() > 20 {
            self.debug.drain(0..self.debug.len() - 20);
        }
    }

    fn save(&mut self, app: &mut LauncherApp) {
        if let Err(e) = save_macros(MACROS_FILE, &self.entries) {
            app.set_error(format!("Failed to save macros: {e}"));
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
        egui::Window::new("Macros")
            .open(&mut self.open)
            .show(ctx, |ui| {
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
                            ui.add(egui::DragValue::new(&mut self.auto_delay_secs).speed(0.1).clamp_range(0.0..=60.0).suffix("s"));
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
                                ui.add(egui::DragValue::new(&mut secs).speed(0.1).clamp_range(0.0..=60.0).suffix("s"));
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
                        egui::ComboBox::from_id_source("macro_cat")
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
                                    if ui.button(format!("{} - {}", act.label, act.desc)).clicked() {
                                        self.steps.push(MacroStep {
                                            label: act.label.clone(),
                                            command: act.action.clone(),
                                            args: if self.add_args.trim().is_empty() {
                                                None
                                            } else {
                                                Some(self.add_args.clone())
                                            },
                                            delay_ms: 0,
                                        });
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
                            self.add_plugin.clear();
                            self.add_filter.clear();
                            self.add_args.clear();
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

                if !self.debug.is_empty() {
                    ui.separator();
                    ui.label("Debug");
                    egui::ScrollArea::vertical()
                        .max_height(80.0)
                        .show(ui, |ui| {
                            for line in &self.debug {
                                ui.label(line);
                            }
                        });
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
