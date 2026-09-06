use super::{
    Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult, edit_typed_settings,
};
use crate::actions::Action;
use crate::common::query::{apply_action_filters, split_action_filters};
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum PluginHomeMode {
    #[default]
    Commands,
    Search,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginHomeConfig {
    #[serde(default)]
    pub plugin: Option<String>,
    #[serde(default)]
    pub mode: PluginHomeMode,
    #[serde(default)]
    pub query_seed: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

impl Default for PluginHomeConfig {
    fn default() -> Self {
        Self {
            plugin: None,
            mode: PluginHomeMode::Commands,
            query_seed: None,
            limit: default_limit(),
        }
    }
}

fn default_limit() -> usize {
    5
}

pub(crate) fn search_plugin_actions(
    plugin: &dyn crate::plugin::Plugin,
    query: &str,
) -> Vec<Action> {
    let (filtered_query, filters) = split_action_filters(query);
    let actions = plugin.search(filtered_query.trim());
    apply_action_filters(actions, &filters)
}

#[derive(Default)]
pub struct PluginHomeWidget {
    cfg: PluginHomeConfig,
    cached_source: Option<PluginHomeSource>,
    cached_actions: Vec<Action>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PluginHomeSource {
    plugin: String,
    mode: PluginHomeMode,
    query: String,
    generation: u64,
}

impl PluginHomeWidget {
    pub fn new(cfg: PluginHomeConfig) -> Self {
        Self {
            cfg,
            cached_source: None,
            cached_actions: Vec::new(),
        }
    }

    fn plugin<'a>(&self, ctx: &'a DashboardContext<'a>) -> Option<&'a dyn crate::plugin::Plugin> {
        let name = self.plugin_name(ctx)?;
        ctx.plugins
            .iter()
            .find_map(|p| if p.name() == name { Some(&**p) } else { None })
    }

    fn plugin_name<'a>(&'a self, ctx: &'a DashboardContext<'_>) -> Option<String> {
        self.cfg
            .plugin
            .clone()
            .or_else(|| ctx.plugins.plugin_names().into_iter().next())
    }

    fn source_generation(ctx: &DashboardContext<'_>, plugin_name: &str) -> u64 {
        match plugin_name {
            "clipboard" => ctx.clipboard_version,
            "layout" => crate::plugins::layouts_storage::layouts_version(),
            "shell" => crate::plugins::shell::shell_version(),
            _ => ctx.plugins.search_generation_for(plugin_name),
        }
    }

    fn render_actions(&self, ui: &mut egui::Ui, actions: &[Action]) -> Option<WidgetAction> {
        let mut clicked = None;
        for action in actions.iter().take(self.cfg.limit.max(1)) {
            if ui.button(&action.label).clicked() {
                clicked = Some(WidgetAction {
                    query_override: Some(action.label.clone()),
                    action: action.clone(),
                });
            }
        }
        clicked
    }

    fn actions_for(
        &mut self,
        plugin_name: &str,
        plugin: &dyn crate::plugin::Plugin,
        generation: u64,
    ) -> &[Action] {
        let query = self.cfg.query_seed.clone().unwrap_or_default();
        let source = PluginHomeSource {
            plugin: plugin_name.to_string(),
            mode: self.cfg.mode,
            query: query.clone(),
            generation: if self.cfg.mode == PluginHomeMode::Search {
                generation
            } else {
                0
            },
        };
        if self.cached_source.as_ref() != Some(&source) {
            self.cached_actions = match self.cfg.mode {
                PluginHomeMode::Commands => plugin.commands(),
                PluginHomeMode::Search if query.trim().is_empty() => Vec::new(),
                PluginHomeMode::Search => search_plugin_actions(plugin, &query),
            };
            self.cached_source = Some(source);
        }
        &self.cached_actions
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut PluginHomeConfig, ctx| {
            let mut changed = false;
            let plugin_names = super::plugin_names(ctx);
            if cfg.plugin.is_none() && !plugin_names.is_empty() {
                cfg.plugin = plugin_names.first().cloned();
                changed = true;
            }
            egui::ComboBox::from_label("Plugin")
                .selected_text(cfg.plugin.as_deref().unwrap_or_else(|| {
                    plugin_names
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("Select a plugin")
                }))
                .show_ui(ui, |ui| {
                    for name in plugin_names {
                        changed |= ui
                            .selectable_value(&mut cfg.plugin, Some(name.clone()), name)
                            .changed();
                    }
                });

            ui.horizontal(|ui| {
                ui.label("Mode");
                changed |= ui
                    .selectable_value(&mut cfg.mode, PluginHomeMode::Commands, "Commands")
                    .changed();
                changed |= ui
                    .selectable_value(&mut cfg.mode, PluginHomeMode::Search, "Search")
                    .changed();
            });

            ui.horizontal(|ui| {
                ui.label("Query seed");
                let mut text = cfg.query_seed.clone().unwrap_or_default();
                if ui.text_edit_singleline(&mut text).changed() {
                    cfg.query_seed = if text.trim().is_empty() {
                        None
                    } else {
                        Some(text)
                    };
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Limit");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.limit).clamp_range(1..=25))
                    .changed();
            });

            changed
        })
    }
}

impl Widget for PluginHomeWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let Some(plugin_name) = self.plugin_name(ctx) else {
            ui.label("No plugins available.");
            return None;
        };

        let Some(plugin) = self.plugin(ctx) else {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!("Plugin '{plugin_name}' not found."),
            );
            return None;
        };

        if self.cfg.mode == PluginHomeMode::Search
            && self
                .cfg
                .query_seed
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
        {
            ui.label("Set a query to preview search results.");
        }
        let generation = Self::source_generation(ctx, &plugin_name);
        let actions = self.actions_for(&plugin_name, plugin, generation).to_vec();

        if actions.is_empty() {
            ui.label("No actions available for this plugin.");
            return None;
        }

        self.render_actions(ui, &actions)
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<PluginHomeConfig>(settings.clone()) {
            self.cfg = cfg;
            self.cached_source = None;
            self.cached_actions.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{TryRecvError, channel};

    struct BlockedPlugin(std::sync::mpsc::Sender<()>);

    impl crate::plugin::Plugin for BlockedPlugin {
        fn search(&self, _query: &str) -> Vec<Action> {
            self.0.send(()).unwrap();
            std::thread::park();
            Vec::new()
        }
        fn name(&self) -> &str {
            "blocked"
        }
        fn description(&self) -> &str {
            "blocked"
        }
        fn capabilities(&self) -> &[&str] {
            &["search"]
        }
    }

    struct CountingPlugin(std::sync::Arc<std::sync::atomic::AtomicUsize>);

    impl crate::plugin::Plugin for CountingPlugin {
        fn search(&self, _query: &str) -> Vec<Action> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Vec::new()
        }
        fn name(&self) -> &str {
            "counting"
        }
        fn description(&self) -> &str {
            "counting"
        }
        fn capabilities(&self) -> &[&str] {
            &["search"]
        }
    }

    #[test]
    fn cached_render_source_does_not_reenter_blocked_provider() {
        let (started_tx, started_rx) = channel();
        let plugin = BlockedPlugin(started_tx);
        let mut widget = PluginHomeWidget::new(PluginHomeConfig {
            plugin: Some("blocked".into()),
            mode: PluginHomeMode::Search,
            query_seed: Some("blocked query".into()),
            limit: 5,
        });
        widget.cached_source = Some(PluginHomeSource {
            plugin: "blocked".into(),
            mode: PluginHomeMode::Search,
            query: "blocked query".into(),
            generation: 7,
        });
        widget.cached_actions = vec![Action {
            label: "cached".into(),
            desc: "cached".into(),
            action: "cached".into(),
            args: None,
        }];

        assert_eq!(widget.actions_for("blocked", &plugin, 7).len(), 1);
        assert!(matches!(started_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn source_generation_invalidates_cached_results_once() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let plugin = CountingPlugin(std::sync::Arc::clone(&calls));
        let mut widget = PluginHomeWidget::new(PluginHomeConfig {
            plugin: Some("counting".into()),
            mode: PluginHomeMode::Search,
            query_seed: Some("query".into()),
            limit: 5,
        });

        widget.actions_for("counting", &plugin, 3);
        widget.actions_for("counting", &plugin, 3);
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
        widget.actions_for("counting", &plugin, 4);
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 2);
    }
}
