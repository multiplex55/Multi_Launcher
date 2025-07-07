use crate::settings::Settings;
use crate::plugin::PluginManager;
use crate::gui::LauncherApp;
use eframe::egui;
#[cfg(target_os = "windows")]
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
    pub fn new(settings: &Settings) -> Self {
        let plugin_dirs = settings.plugin_dirs.clone().unwrap_or_default();
        let info = Self::gather_available(&plugin_dirs);

        // If settings don't specify enabled plugins, enable all gathered ones by
        // default.
        let enabled_plugins = match &settings.enabled_plugins {
            Some(list) => list.clone(),
            None => info.iter().map(|(n, _, _)| n.clone()).collect(),
        };

        // Likewise enable all advertised capabilities per plugin when the
        // settings don't provide a map.
        let enabled_capabilities = match &settings.enabled_capabilities {
            Some(map) => map.clone(),
            None => info
                .iter()
                .map(|(name, _, caps)| (name.clone(), caps.clone()))
                .collect(),
        };

        Self {
            plugin_dirs,
            enabled_plugins,
            enabled_capabilities,
            plugin_input: String::new(),
            available: info,
        }
    }

    fn gather_available(plugin_dirs: &[String]) -> Vec<(String, String, Vec<String>)> {
        let mut pm = PluginManager::new();
        pm.reload_from_dirs(plugin_dirs);
        pm.plugin_infos()
    }

    fn save_settings(&mut self, app: &mut LauncherApp) {
        tracing::debug!(?self.plugin_dirs, ?self.enabled_plugins, "saving plugin settings");
        match Settings::load(&app.settings_path) {
            Ok(mut s) => {
                s.plugin_dirs = if self.plugin_dirs.is_empty() {
                    None
                } else {
                    Some(self.plugin_dirs.clone())
                };
                let all_plugins_enabled = self.enabled_plugins.len() == self.available.len()
                    && self
                        .available
                        .iter()
                        .all(|(name, _, _)| self.enabled_plugins.contains(name));
                s.enabled_plugins = if all_plugins_enabled {
                    None
                } else {
                    Some(self.enabled_plugins.clone())
                };

                let mut default_caps = true;
                for (name, _, caps) in &self.available {
                    if !self.enabled_plugins.contains(name) {
                        continue;
                    }
                    match self.enabled_capabilities.get(name) {
                        Some(en) => {
                            if en.len() != caps.len()
                                || !caps.iter().all(|c| en.contains(c))
                            {
                                default_caps = false;
                                break;
                            }
                        }
                        None => {
                            if !caps.is_empty() {
                                default_caps = false;
                                break;
                            }
                        }
                    }
                }
                if default_caps && self.enabled_capabilities.len() == self.enabled_plugins.len() {
                    s.enabled_capabilities = None;
                } else {
                    s.enabled_capabilities = Some(self.enabled_capabilities.clone());
                }
                if let Err(e) = s.save(&app.settings_path) {
                    app.error = Some(format!("Failed to save: {e}"));
                } else {
                    app.update_paths(
                        s.plugin_dirs.clone(),
                        s.index_paths.clone(),
                        s.enabled_plugins.clone(),
                        s.enabled_capabilities.clone(),
                        s.offscreen_pos,
                        Some(s.enable_toasts),
                    );

                    app.plugins.reload_from_dirs(&self.plugin_dirs);
                    tracing::debug!(available=?app.plugins.plugin_names(), "plugins reloaded");
                    self.available = Self::gather_available(&self.plugin_dirs);
                    app.search();

                    crate::request_hotkey_restart(s);
                }
            }
            Err(e) => app.error = Some(format!("Failed to read settings: {e}")),
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        let mut open = app.show_plugins;
        let mut changed = false;
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
                    changed = true;
                }
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.plugin_input);
                    if ui.button("Browse").clicked() {
                        #[cfg(target_os = "windows")]
                        if let Some(dir) = FileDialog::new().pick_folder() {
                            self.plugin_input = dir.display().to_string();
                        }
                    }
                    if ui.button("Add").clicked() {
                        if !self.plugin_input.is_empty() {
                            self.plugin_dirs.push(self.plugin_input.clone());
                            self.plugin_input.clear();
                            changed = true;
                        }
                    }
                });

                ui.separator();
                ui.label("Plugins:");
                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                    for (name, desc, caps) in &self.available {
                        let mut enabled = self.enabled_plugins.contains(name);
                        ui.horizontal(|ui| {
                            if ui
                                .checkbox(&mut enabled, name)
                                .on_hover_text(desc)
                                .changed()
                            {
                                if enabled {
                                    if !self.enabled_plugins.contains(name) {
                                        self.enabled_plugins.push(name.clone());
                                    }
                                } else if let Some(pos) =
                                    self.enabled_plugins.iter().position(|n| n == name)
                                {
                                    self.enabled_plugins.remove(pos);
                                    self.enabled_capabilities.remove(name);
                                }
                                changed = true;
                            }
                        });
                        ui.indent(name, |ui| {
                            ui.add_enabled_ui(enabled, |ui| {
                                for cap in caps {
                                    let entry =
                                        self.enabled_capabilities.entry(name.clone()).or_default();
                                    let mut cap_enabled = entry.contains(cap);
                                    let label = if cap == "show_full_path" {
                                        "show full path always".to_string()
                                    } else {
                                        cap.to_string()
                                    };
                                    if ui.checkbox(&mut cap_enabled, label).changed() {
                                        if cap_enabled {
                                            if !entry.contains(cap) {
                                                entry.push(cap.clone());
                                            }
                                        } else if let Some(pos) = entry.iter().position(|c| c == cap) {
                                            entry.remove(pos);
                                        }
                                        changed = true;
                                    }
                                }
                            });
                        });
                    }
                });

                if ui.button("Save").clicked() {
                    changed = true;
                }
            });
        app.show_plugins = open;
        if changed {
            self.save_settings(app);
        }
    }
}
