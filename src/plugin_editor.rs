use crate::settings::Settings;
use crate::plugin::PluginManager;
use crate::gui::LauncherApp;
use eframe::egui;
use rfd::FileDialog;
use std::collections::HashMap;

#[derive(Default)]

pub struct PluginEditor {
    plugin_dirs: Vec<String>,
    enabled_plugins: Vec<String>,
    enabled_capabilities: HashMap<String, Vec<String>>,
    plugin_input: String,
    available: Vec<(String, String, Vec<String>)>,
}

impl PluginEditor {
    pub fn new(settings: &Settings, plugins: &PluginManager) -> Self {
        let info = plugins.plugin_infos();
        Self {
            plugin_dirs: settings.plugin_dirs.clone().unwrap_or_default(),
            enabled_plugins: settings.enabled_plugins.clone().unwrap_or_default(),
            enabled_capabilities: settings
                .enabled_capabilities
                .clone()
                .unwrap_or_default(),
            plugin_input: String::new(),
            available: info,
        }
    }

    fn save_settings(&self, app: &mut LauncherApp) {
        match Settings::load(&app.settings_path) {
            Ok(mut s) => {
                s.plugin_dirs = if self.plugin_dirs.is_empty() {
                    None
                } else {
                    Some(self.plugin_dirs.clone())
                };
                s.enabled_plugins = if self.enabled_plugins.is_empty() {
                    None
                } else {
                    Some(self.enabled_plugins.clone())
                };
                s.enabled_capabilities = if self.enabled_capabilities.is_empty() {
                    None
                } else {
                    Some(self.enabled_capabilities.clone())
                };
                if let Err(e) = s.save(&app.settings_path) {
                    app.error = Some(format!("Failed to save: {e}"));
                } else {
                    app.update_paths(
                        s.plugin_dirs.clone(),
                        s.index_paths.clone(),
                        s.enabled_plugins.clone(),
                        s.enabled_capabilities.clone(),
                        s.offscreen_pos,
                    );
                    crate::request_hotkey_restart(s);
                }
            }
            Err(e) => app.error = Some(format!("Failed to read settings: {e}")),
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        let mut open = app.show_plugins;
        egui::Window::new("Plugin Settings")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Plugin directories:");
                let mut remove: Option<usize> = None;
                for (idx, path) in self.plugin_dirs.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(path);
                        if ui.button("Remove").clicked() {
                            remove = Some(idx);
                        }
                    });
                }
                if let Some(i) = remove {
                    self.plugin_dirs.remove(i);
                }
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.plugin_input);
                    if ui.button("Browse").clicked() {
                        if let Some(dir) = FileDialog::new().pick_folder() {
                            self.plugin_input = dir.display().to_string();
                        }
                    }
                    if ui.button("Add").clicked() {
                        if !self.plugin_input.is_empty() {
                            self.plugin_dirs.push(self.plugin_input.clone());
                            self.plugin_input.clear();
                        }
                    }
                });

                ui.separator();
                ui.label("Plugins:");
                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                    for (name, desc, caps) in &self.available {
                        let mut enabled = self.enabled_plugins.contains(name);
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut enabled, name).on_hover_text(desc).changed() {
                                if enabled {
                                    if !self.enabled_plugins.contains(name) {
                                        self.enabled_plugins.push(name.clone());
                                    }
                                } else if let Some(pos) = self.enabled_plugins.iter().position(|n| n == name) {
                                    self.enabled_plugins.remove(pos);
                                }
                            }
                        });
                        ui.indent(name, |ui| {
                            for cap in caps {
                                let entry = self.enabled_capabilities.entry(name.clone()).or_default();
                                let mut cap_enabled = entry.contains(cap);
                                let label = format!("{}", cap);
                                if ui.checkbox(&mut cap_enabled, label).changed() {
                                    if cap_enabled {
                                        if !entry.contains(cap) {
                                            entry.push(cap.clone());
                                        }
                                    } else if let Some(pos) = entry.iter().position(|c| c == cap) {
                                        entry.remove(pos);
                                    }
                                }
                            }
                        });
                    }
                });

                if ui.button("Save").clicked() {
                    self.save_settings(app);
                }
            });
        app.show_plugins = open;
    }
}
