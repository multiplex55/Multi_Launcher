use crate::gui::LauncherApp;
use crate::plugin::PluginManager;
use crate::settings::Settings;
use eframe::egui;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]

pub struct PluginEditor {
    enabled_plugins: Vec<String>,
    enabled_capabilities: HashMap<String, Vec<String>>,
    available: Vec<(String, String, Vec<String>)>,
    filter: String,
}

impl PluginEditor {
    pub fn new(settings: &Settings) -> Self {
        let info = Self::gather_available(&settings.plugin_dirs.clone().unwrap_or_default());
        Self::with_available(settings, info)
    }

    #[cfg(test)]
    pub(crate) fn new_with_manager_for_test(settings: &Settings, manager: &PluginManager) -> Self {
        Self::with_available(settings, manager.plugin_infos())
    }

    fn with_available(settings: &Settings, mut info: Vec<(String, String, Vec<String>)>) -> Self {
        info.sort_by_key(|plugin| plugin.0.to_lowercase());
        // If settings don't specify enabled plugins, enable all gathered ones by
        // default.
        let enabled_plugins = match &settings.enabled_plugins {
            Some(set) => set.iter().cloned().collect(),
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
            enabled_plugins,
            enabled_capabilities,
            available: info,
            filter: String::new(),
        }
    }

    pub(crate) fn sync_committed_settings(
        &mut self,
        settings: &Settings,
        available: Vec<(String, String, Vec<String>)>,
    ) {
        let filter = std::mem::take(&mut self.filter);
        *self = Self::with_available(settings, available);
        self.filter = filter;
    }

    fn gather_available(plugin_dirs: &[String]) -> Vec<(String, String, Vec<String>)> {
        let mut pm = PluginManager::new();
        pm.reload_from_dirs(
            plugin_dirs,
            Settings::default().clipboard_limit,
            Settings::default().net_unit,
            false,
            &std::collections::HashMap::new(),
            Arc::new(Vec::new()),
        );
        pm.plugin_infos()
    }

    fn save_settings(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if app.settings_save_pending() {
            return;
        }
        tracing::debug!(?self.enabled_plugins, "saving plugin settings");
        let all_plugins_enabled = self.enabled_plugins.len() == self.available.len()
            && self
                .available
                .iter()
                .all(|(name, _, _)| self.enabled_plugins.contains(name));
        let enabled_plugins =
            (!all_plugins_enabled).then(|| self.enabled_plugins.iter().cloned().collect());

        let mut default_caps = true;
        for (name, _, caps) in &self.available {
            if !self.enabled_plugins.contains(name) {
                continue;
            }
            match self.enabled_capabilities.get(name) {
                Some(enabled) => {
                    if enabled.len() != caps.len() || !caps.iter().all(|cap| enabled.contains(cap))
                    {
                        default_caps = false;
                        break;
                    }
                }
                None if !caps.is_empty() => {
                    default_caps = false;
                    break;
                }
                None => {}
            }
        }
        let enabled_capabilities =
            if default_caps && self.enabled_capabilities.len() == self.enabled_plugins.len() {
                None
            } else {
                Some(self.enabled_capabilities.clone())
            };
        app.begin_settings_save(
            ctx,
            crate::settings_save::SettingsSaveIntent::plugin_selection(
                enabled_plugins,
                enabled_capabilities,
            ),
        );
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        let mut open = app.show_plugins;
        let mut changed = false;
        let pending = app.settings_save_pending();
        egui::Window::new("Plugin Settings")
            .open(&mut open)
            .show(ctx, |ui| {
                ui.add_enabled_ui(!pending, |ui| {
                    ui.separator();
                    ui.label("Plugins:");
                    ui.horizontal(|ui| {
                        ui.label("Filter:");
                        ui.text_edit_singleline(&mut self.filter);
                    });
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
                            for (name, desc, caps) in &self.available {
                                if !self.filter.is_empty()
                                    && !name.to_lowercase().contains(&self.filter.to_lowercase())
                                    && !desc.to_lowercase().contains(&self.filter.to_lowercase())
                                {
                                    continue;
                                }
                                let mut enabled = self.enabled_plugins.contains(name);
                                ui.horizontal(|ui| {
                                    if ui.checkbox(&mut enabled, name).changed() {
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
                                    ui.label(desc);
                                });
                                ui.indent(name, |ui| {
                                    ui.add_enabled_ui(enabled, |ui| {
                                        for cap in caps {
                                            let entry = self
                                                .enabled_capabilities
                                                .entry(name.clone())
                                                .or_default();
                                            let mut cap_enabled = entry.contains(cap);
                                            let label = if cap == "show_full_path" {
                                                "show full path always".to_string()
                                            } else {
                                                cap.to_string()
                                            };
                                            if ui
                                                .checkbox(&mut cap_enabled, label)
                                                .on_hover_text(cap)
                                                .changed()
                                            {
                                                if cap_enabled {
                                                    if !entry.contains(cap) {
                                                        entry.push(cap.clone());
                                                    }
                                                } else if let Some(pos) =
                                                    entry.iter().position(|c| c == cap)
                                                {
                                                    entry.remove(pos);
                                                }
                                                changed = true;
                                            }
                                        }
                                    });
                                });
                            }
                        });
                });
                if pending {
                    ui.label("Saving plugin settings…");
                }
            });
        app.show_plugins = open;
        if changed {
            self.save_settings(ctx, app);
        }
    }
}
