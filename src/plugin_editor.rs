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
        let mut infos = pm.plugin_infos();
        infos.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        infos
    }

    fn save_settings(&mut self, app: &mut LauncherApp) {
        tracing::debug!(?self.enabled_plugins, "saving plugin settings");
        match Settings::load(&app.settings_path) {
            Ok(mut s) => {
                let all_plugins_enabled = self.enabled_plugins.len() == self.available.len()
                    && self
                        .available
                        .iter()
                        .all(|(name, _, _)| self.enabled_plugins.contains(name));
                s.enabled_plugins = if all_plugins_enabled {
                    None
                } else {
                    Some(self.enabled_plugins.iter().cloned().collect())
                };

                let mut default_caps = true;
                for (name, _, caps) in &self.available {
                    if !self.enabled_plugins.contains(name) {
                        continue;
                    }
                    match self.enabled_capabilities.get(name) {
                        Some(en) => {
                            if en.len() != caps.len() || !caps.iter().all(|c| en.contains(c)) {
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
                    app.set_error(format!("Failed to save: {e}"));
                } else {
                    app.update_paths(
                        s.plugin_dirs.clone(),
                        s.index_paths.clone(),
                        s.enabled_plugins.clone(),
                        s.enabled_capabilities.clone(),
                        s.offscreen_pos,
                        Some(s.enable_toasts),
                        Some(s.show_inline_errors),
                        Some(s.show_error_toasts),
                        Some(s.toast_duration),
                        Some(s.fuzzy_weight),
                        Some(s.usage_weight),
                        Some(s.match_exact),
                        Some(s.follow_mouse),
                        Some(s.static_location_enabled),
                        s.static_pos,
                        s.static_size,
                        Some(s.hide_after_run),
                        Some(s.clear_query_after_run),
                        Some(s.require_confirm_destructive),
                        Some(s.timer_refresh),
                        Some(s.disable_timer_updates),
                        Some(s.preserve_command),
                        Some(s.query_autocomplete),
                        Some(s.net_refresh),
                        Some(s.net_unit),
                        s.screenshot_dir.clone(),
                        Some(s.screenshot_save_file),
                        Some(s.screenshot_use_editor),
                        Some(s.screenshot_auto_save),
                        Some(s.always_on_top),
                        Some(s.page_jump),
                        Some(s.note_panel_default_size),
                        Some(s.note_save_on_close),
                        Some(s.note_always_overwrite),
                        Some(s.note_images_as_links),
                        Some(s.note_show_details),
                        Some(s.note_more_limit),
                        Some(s.show_dashboard_diagnostics),
                    );
                    let dirs = s.plugin_dirs.clone().unwrap_or_default();
                    let actions_arc = Arc::clone(&app.actions);
                    app.plugins.reload_from_dirs(
                        &dirs,
                        app.clipboard_limit,
                        app.net_unit,
                        false,
                        &s.plugin_settings,
                        actions_arc,
                    );
                    tracing::debug!(available=?app.plugins.plugin_names(), "plugins reloaded");
                    self.available = Self::gather_available(&dirs);
                    app.search();

                    crate::request_hotkey_restart(s);
                }
            }
            Err(e) => app.set_error(format!("Failed to read settings: {e}")),
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        let mut open = app.show_plugins;
        let mut changed = false;
        egui::Window::new("Plugin Settings")
            .open(&mut open)
            .show(ctx, |ui| {
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
        app.show_plugins = open;
        if changed {
            self.save_settings(app);
        }
    }
}
