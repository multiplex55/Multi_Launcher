use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
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

pub struct PluginHomeWidget {
    cfg: PluginHomeConfig,
}

impl PluginHomeWidget {
    pub fn new(cfg: PluginHomeConfig) -> Self {
        Self { cfg }
    }

    fn plugin<'a>(&self, ctx: &'a DashboardContext<'a>) -> Option<&'a (dyn crate::plugin::Plugin)> {
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

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut PluginHomeConfig, ctx| {
            let mut changed = false;
            let plugin_names = ctx.plugins.map(|p| p.plugin_names()).unwrap_or_default();
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

impl Default for PluginHomeWidget {
    fn default() -> Self {
        Self {
            cfg: PluginHomeConfig::default(),
        }
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

        let actions = match self.cfg.mode {
            PluginHomeMode::Commands => plugin.commands(),
            PluginHomeMode::Search => {
                let query = self.cfg.query_seed.clone().unwrap_or_default();
                if query.trim().is_empty() {
                    ui.label("Set a query to preview search results.");
                    Vec::new()
                } else {
                    plugin.search(&query)
                }
            }
        };

        if actions.is_empty() {
            ui.label("No actions available for this plugin.");
            return None;
        }

        self.render_actions(ui, &actions)
    }
}
