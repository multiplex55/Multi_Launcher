use crate::gui::LauncherApp;
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::plugin::PluginManager;
use crate::settings::Settings;
use eframe::egui;
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) struct PluginHostRuntime {
    pub pending: bool,
    pub can_close: bool,
    pub catalog_revision: u64,
    pub catalog: Option<Arc<crate::plugin::PluginCatalogSnapshot>>,
    pub navigation: Option<crate::settings_editor::render::SettingsNavigationFrameObserver>,
}

pub(crate) struct PluginHostActions {
    pub presentation_request: Option<crate::gui::utility_host::UtilityPresentation>,
    pub changed: bool,
    pub close: bool,
    pub catalog_revision: u64,
    pub before: PluginSelection,
    pub candidate: PluginSelection,
    pub selection_revision: u64,
    pub filter_before: String,
    pub filter_after: String,
    #[cfg(test)]
    pub test_button: Option<(egui::Rect, egui::Rect, bool)>,
    pub navigation: Option<crate::settings_editor::render::SettingsNavigationFrameObserver>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PluginSelection {
    enabled_plugins: Vec<String>,
    enabled_capabilities: HashMap<String, Vec<String>>,
}

#[derive(Clone, Default)]

pub struct PluginEditor {
    pub(crate) host_open: bool,
    host_epoch: u64,
    selection_revision: u64,
    enabled_plugins: Vec<String>,
    enabled_capabilities: HashMap<String, Vec<String>>,
    available: Vec<(String, String, Vec<String>)>,
    filter: String,
    #[cfg(test)]
    test_plugin_checkbox: Option<(String, egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    test_filter_response: Option<(egui::Id, egui::Rect)>,
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
            host_open: false,
            host_epoch: 0,
            selection_revision: 0,
            enabled_plugins,
            enabled_capabilities,
            available: info,
            filter: String::new(),
            #[cfg(test)]
            test_plugin_checkbox: None,
            #[cfg(test)]
            test_filter_response: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn test_plugin_checkbox(&self) -> Option<(String, egui::Rect, egui::Rect, bool)> { self.test_plugin_checkbox.clone() }

    #[cfg(test)]
    pub(crate) fn test_filter_response(&self) -> Option<(egui::Id, egui::Rect)> {
        self.test_filter_response
    }

    #[cfg(test)]
    pub(crate) fn set_test_filter(&mut self, filter: &str) {
        self.filter = filter.to_owned();
    }

    #[cfg(test)]
    pub(crate) fn test_filter(&self) -> &str {
        &self.filter
    }

    pub(crate) fn sync_committed_settings(
        &mut self,
        settings: &Settings,
        available: Vec<(String, String, Vec<String>)>,
    ) {
        let filter = std::mem::take(&mut self.filter);
        let host_open = self.host_open;
        let host_epoch = self.host_epoch;
        let selection_revision = self.selection_revision.wrapping_add(1);
        *self = Self::with_available(settings, available);
        self.selection_revision = selection_revision;
        self.filter = filter;
        self.host_open = host_open;
        self.host_epoch = host_epoch;
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

    pub(crate) fn save_settings(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
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

    pub(crate) fn start_host(&mut self) {
        if !self.host_open { self.host_epoch = self.host_epoch.wrapping_add(1).max(1); self.host_open = true; }
    }

    pub(crate) fn host_session(&self) -> u64 { self.host_epoch }

    pub(crate) fn selection_is_current(&self, revision: u64, selection: &PluginSelection) -> bool {
        self.selection_revision == revision && self.selection() == *selection
    }

    fn selection(&self) -> PluginSelection {
        PluginSelection { enabled_plugins: self.enabled_plugins.clone(), enabled_capabilities: self.enabled_capabilities.clone() }
    }

    pub(crate) fn apply_host_filter(&mut self, before: &str, after: String) {
        if self.filter == before { self.filter = after; }
    }

    pub(crate) fn restore_selection(&mut self, selection: PluginSelection) {
        self.enabled_plugins = selection.enabled_plugins;
        self.enabled_capabilities = selection.enabled_capabilities;
    }

    pub(crate) fn render_host(&mut self, ctx: &egui::Context, runtime: &PluginHostRuntime,
        allowed: bool, first: bool, child: Option<&mut egui::Ui>, presentation: crate::gui::utility_host::UtilityPresentation) -> PluginHostActions {
        let navigation = runtime.navigation.as_ref().filter(|_| allowed && first && !ctx.will_discard());
        let _navigation_scope = navigation.map(|observer| observer.install(ctx));
        let mut open = self.host_open;
        let mut changed = false;
        #[cfg(test)]
        let mut test_button = None;
        let pending = runtime.pending;
        let filter_before = self.filter.clone();
        let mut filter_after = filter_before.clone();
        let before = self.selection();
        let selection_revision = self.selection_revision;
        let navigation_request =
            crate::settings_editor::render::scoped_settings_navigation_request(ctx).filter(
                |request| {
                    request.section
                        == crate::settings_editor::catalog::SettingsSection::PluginEditor
                        && request.catalog_revision == runtime.catalog_revision
                },
            );
        let navigation_highlight_only =
            crate::settings_editor::render::scoped_settings_navigation_is_highlight_only(ctx);
        let catalog_snapshot = navigation_request
            .as_ref()
            .and_then(|_| runtime.catalog.clone());
        let requested_filter_row = navigation_request
            .as_ref()
            .zip(catalog_snapshot.as_ref())
            .and_then(|(request, snapshot)| {
                validated_navigation_plugin_row(
                    &request.control_id,
                    &request.owner,
                    snapshot,
                    &self.available,
                )
            })
            .map(|(name, description, _)| (name.to_owned(), description.to_owned()));
        if !navigation_highlight_only && let Some((name, description)) = requested_filter_row {
            let filter = filter_after.trim().to_lowercase();
            if !filter.is_empty()
                && !name.to_lowercase().contains(&filter)
                && !description.to_lowercase().contains(&filter)
            {
                filter_after.clear();
            }
        }
        let navigation_control =
            |plugin_name: &str, capability: Option<&str>, row_capabilities: &[String]| {
                let request = navigation_request.as_ref()?;
                let snapshot = catalog_snapshot.as_ref()?;
                let mut current = snapshot
                    .plugins
                    .iter()
                    .filter(|plugin| plugin.name == plugin_name);
                let plugin = current.next()?;
                if current.next().is_some() || plugin.capabilities != row_capabilities {
                    return None;
                }
                let control_id = plugin_editor_control_id(plugin, capability)?;
                let request_owner_matches = matches!(
                    &request.owner,
                    crate::settings_editor::catalog::SettingsOwner::Plugin {
                        name,
                        origin,
                        epoch: Some(epoch),
                    } if name == &plugin.name && *origin == plugin.origin && *epoch == plugin.epoch
                );
                Some(
                    if request_owner_matches && request.control_id == control_id {
                        control_id
                    } else {
                        format!("plugin_editor.observer.{control_id}")
                    },
                )
            };
        #[cfg(test)]
        {
            self.test_filter_response = None;
        }
        let mut presentation_request = None;
        let mut body = |ui: &mut egui::Ui| {
            let interactive = allowed && ui.is_visible() && !ui.is_sizing_pass();
            let (host, next) = crate::gui::utility_host::presentation_button(ui, presentation, interactive);
            #[cfg(test)] { test_button = Some((host.rect, ui.clip_rect(), host.enabled())); }
            if first && !ctx.will_discard() && host.clicked() { presentation_request = Some(next); }
                ui.add_enabled_ui(interactive && !pending, |ui| {
                    ui.separator();
                    ui.label("Plugins:");
                    ui.horizontal(|ui| {
                        ui.label("Filter:");
                        let response = ui.text_edit_singleline(&mut filter_after);
                        crate::settings_editor::render::observe_scoped_settings_navigation_response(
                            ui,
                            &response,
                            "plugin_editor.filter",
                            crate::gui::jump_mode::TargetKind::TextEntry,
                        );
                        #[cfg(test)]
                        {
                            self.test_filter_response = Some((response.id, response.rect));
                        }
                    });
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
                            for (name, desc, caps) in &self.available {
                                if !filter_after.is_empty()
                                    && !name.to_lowercase().contains(&filter_after.to_lowercase())
                                    && !desc.to_lowercase().contains(&filter_after.to_lowercase())
                                {
                                    continue;
                                }
                                let mut enabled = self.enabled_plugins.contains(name);
                                ui.horizontal(|ui| {
                                    let response = ui.checkbox(&mut enabled, name);
                                    #[cfg(test)]
                                    if name == "cross_owner_probe" {
                                        self.test_plugin_checkbox = Some((name.clone(), response.rect, ui.clip_rect(), response.enabled()));
                                    }
                                    if let Some(control_id) = navigation_control(name, None, caps) {
                                        crate::settings_editor::render::observe_scoped_settings_navigation_response(
                                            ui,
                                            &response,
                                            &control_id,
                                            crate::gui::jump_mode::TargetKind::Checkbox,
                                        );
                                    }
                                    if response.changed() {
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
                                            let response = ui.checkbox(&mut cap_enabled, label).on_hover_text(cap);
                                            if let Some(control_id) = navigation_control(name, Some(cap), caps) {
                                                crate::settings_editor::render::observe_scoped_settings_navigation_response(
                                                    ui,
                                                    &response,
                                                    &control_id,
                                                    crate::gui::jump_mode::TargetKind::Checkbox,
                                                );
                                            }
                                            if response.changed() {
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
        };
        if let Some(ui) = child { body(ui); } else {
            let id = egui::Id::new("Plugin Settings");
            egui::Window::new("Plugin Settings").id(id).show_utility(ctx, "plugin-settings", id, Some(&mut open),
                crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::outer(egui::vec2(540.0, 440.0)), body);
        }
        PluginHostActions { presentation_request, changed: first && !ctx.will_discard() && changed, close: runtime.can_close && first && allowed && !ctx.will_discard() && !open, catalog_revision: runtime.catalog_revision, before, candidate: self.selection(), selection_revision, filter_before, filter_after, navigation: navigation.cloned(), #[cfg(test)] test_button }
    }

}

fn plugin_editor_control_id(
    plugin: &crate::plugin::PluginCatalogEntry,
    capability: Option<&str>,
) -> Option<String> {
    let origin = match plugin.origin {
        crate::plugin::PluginCatalogOrigin::Builtin => "builtin",
        crate::plugin::PluginCatalogOrigin::DynamicLibrary => "dynamic",
        crate::plugin::PluginCatalogOrigin::Registered => "registered",
    };
    if let Some(capability) = capability {
        plugin
            .capabilities
            .iter()
            .any(|current| current == capability)
            .then(|| {
                format!(
                    "plugin_editor.{origin}.{}.{}.capability.{capability}",
                    plugin.name, plugin.epoch
                )
            })
    } else {
        Some(format!(
            "plugin_editor.{origin}.{}.{}.enabled",
            plugin.name, plugin.epoch
        ))
    }
}

fn validated_navigation_plugin_row<'a>(
    request_control_id: &str,
    request_owner: &crate::settings_editor::catalog::SettingsOwner,
    snapshot: &crate::plugin::PluginCatalogSnapshot,
    available: &'a [(String, String, Vec<String>)],
) -> Option<&'a (String, String, Vec<String>)> {
    let crate::settings_editor::catalog::SettingsOwner::Plugin {
        name,
        origin,
        epoch: Some(epoch),
    } = request_owner
    else {
        return None;
    };
    let mut matches = snapshot
        .plugins
        .iter()
        .filter(|plugin| plugin.name == *name);
    let plugin = matches.next()?;
    if matches.next().is_some() || plugin.origin != *origin || plugin.epoch != *epoch {
        return None;
    }
    let row = available.iter().find(|(available_name, _, capabilities)| {
        available_name == name && *capabilities == plugin.capabilities
    })?;
    let target_is_current = plugin_editor_control_id(plugin, None)
        .is_some_and(|control_id| request_control_id == control_id)
        || plugin.capabilities.iter().any(|capability| {
            plugin_editor_control_id(plugin, Some(capability))
                .is_some_and(|control_id| request_control_id == control_id)
        });
    target_is_current.then_some(row)
}

#[cfg(test)]
mod hosted_selection_tests {
    use super::*;

    #[test]
    fn newer_committed_plugin_selection_invalidates_staged_candidate() {
        let available = vec![("example".to_owned(), "Example".to_owned(), vec![])];
        let settings = Settings::default();
        let mut editor = PluginEditor::with_available(&settings, available.clone());
        let before = editor.selection();
        editor.enabled_plugins.clear();
        let candidate = editor.selection();
        let revision = editor.selection_revision;
        assert!(editor.selection_is_current(revision, &candidate));
        editor.sync_committed_settings(&settings, available);
        assert!(!editor.selection_is_current(revision, &candidate));
        assert_eq!(editor.selection().enabled_plugins, before.enabled_plugins);
    }
}
