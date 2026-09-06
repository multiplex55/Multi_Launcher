use super::{
    BackgroundLoader, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
    edit_typed_settings,
};
use crate::actions::Action;
use crate::common::query::{apply_action_filters, split_action_filters};
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

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

pub struct PluginHomeWidget {
    cfg: PluginHomeConfig,
    cached_source: Option<PluginHomeSource>,
    cached_actions: Vec<Action>,
    cached_at: Option<Instant>,
    requested_source: Option<PluginHomeSource>,
    loader: BackgroundLoader<PluginHomeRequest, PluginHomeResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PluginHomeSource {
    plugin: String,
    mode: PluginHomeMode,
    query: String,
    generation: u64,
}

struct PluginHomeRequest {
    source: PluginHomeSource,
    plugin: crate::plugin::OwnedPluginHandle,
}

struct PluginHomeResult {
    source: PluginHomeSource,
    actions: Vec<Action>,
}

impl PluginHomeWidget {
    pub fn new(cfg: PluginHomeConfig) -> Self {
        Self {
            cfg,
            cached_source: None,
            cached_actions: Vec::new(),
            cached_at: None,
            requested_source: None,
            loader: BackgroundLoader::new(|request: PluginHomeRequest| {
                let actions = request
                    .plugin
                    .plugin
                    .read()
                    .ok()
                    .map(|plugin| match request.source.mode {
                        PluginHomeMode::Commands => plugin.commands(),
                        PluginHomeMode::Search if request.source.query.trim().is_empty() => {
                            Vec::new()
                        }
                        PluginHomeMode::Search => {
                            search_plugin_actions(&**plugin, &request.source.query)
                        }
                    })
                    .unwrap_or_default();
                PluginHomeResult {
                    source: request.source,
                    actions,
                }
            }),
        }
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

    fn update_actions(
        &mut self,
        source: PluginHomeSource,
        plugin: crate::plugin::OwnedPluginHandle,
        repaint: &egui::Context,
    ) -> &[Action] {
        if let Some(result) = self.loader.poll() {
            self.requested_source = None;
            if result.source == source {
                self.cached_source = Some(result.source);
                self.cached_actions = result.actions;
                self.cached_at = Some(Instant::now());
            }
        }
        let stale = self.cached_source.as_ref() != Some(&source)
            || self
                .cached_at
                .is_none_or(|cached_at| cached_at.elapsed() >= Duration::from_secs(2));
        if stale && self.requested_source.as_ref() != Some(&source) {
            if self.loader.request(
                PluginHomeRequest {
                    source: source.clone(),
                    plugin,
                },
                repaint,
            ) {
                self.requested_source = Some(source);
            }
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

        let Some(plugin) = ctx.plugins.owned_plugin(&plugin_name) else {
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
        let source = PluginHomeSource {
            plugin: plugin_name,
            mode: self.cfg.mode,
            query: self.cfg.query_seed.clone().unwrap_or_default(),
            generation: if self.cfg.mode == PluginHomeMode::Search {
                generation
            } else {
                0
            },
        };
        let actions = self.update_actions(source, plugin, ui.ctx()).to_vec();

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
            self.cached_at = None;
            self.requested_source = None;
        }
    }
}

impl Default for PluginHomeWidget {
    fn default() -> Self {
        Self::new(PluginHomeConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    struct BlockedPlugin {
        started: std::sync::mpsc::Sender<()>,
        release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl crate::plugin::Plugin for BlockedPlugin {
        fn search(&self, _query: &str) -> Vec<Action> {
            self.started.send(()).unwrap();
            self.release.lock().unwrap().recv().unwrap();
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
    fn cold_render_boundary_returns_before_blocked_provider() {
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let plugin = crate::plugin::OwnedPluginHandle::for_test(Box::new(BlockedPlugin {
            started: started_tx,
            release: std::sync::Mutex::new(release_rx),
        }));
        let mut widget = PluginHomeWidget::new(PluginHomeConfig {
            plugin: Some("blocked".into()),
            mode: PluginHomeMode::Search,
            query_seed: Some("blocked query".into()),
            limit: 5,
        });
        let source = PluginHomeSource {
            plugin: "blocked".into(),
            mode: PluginHomeMode::Search,
            query: "blocked query".into(),
            generation: 7,
        };

        assert!(
            widget
                .update_actions(source, plugin, &egui::Context::default())
                .is_empty()
        );
        started_rx.recv().unwrap();
        release_tx.send(()).unwrap();
    }

    #[test]
    fn source_generation_invalidates_cached_results_once() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let plugin = crate::plugin::OwnedPluginHandle::for_test(Box::new(CountingPlugin(
            std::sync::Arc::clone(&calls),
        )));
        let mut widget = PluginHomeWidget::new(PluginHomeConfig {
            plugin: Some("counting".into()),
            mode: PluginHomeMode::Search,
            query_seed: Some("query".into()),
            limit: 5,
        });

        let source = PluginHomeSource {
            plugin: "counting".into(),
            mode: PluginHomeMode::Search,
            query: "query".into(),
            generation: 3,
        };
        widget.update_actions(source.clone(), plugin.clone(), &egui::Context::default());
        while calls.load(std::sync::atomic::Ordering::Relaxed) == 0 {
            std::thread::yield_now();
        }
        while widget.cached_source.as_ref() != Some(&source) {
            widget.update_actions(source.clone(), plugin.clone(), &egui::Context::default());
            std::thread::yield_now();
        }
        widget.update_actions(source.clone(), plugin.clone(), &egui::Context::default());
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
        let changed = PluginHomeSource {
            generation: 4,
            ..source
        };
        widget.update_actions(changed, plugin, &egui::Context::default());
        while calls.load(std::sync::atomic::Ordering::Relaxed) < 2 {
            std::thread::yield_now();
        }
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 2);
    }
}
