use super::{Widget, WidgetAction};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugin::Plugin;
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PluginHomeMode {
    Commands,
    Search,
}

impl Default for PluginHomeMode {
    fn default() -> Self {
        Self::Commands
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginHomeConfig {
    pub plugin: Option<String>,
    #[serde(default)]
    pub home_query: Option<String>,
    #[serde(default)]
    pub query_seed: Option<String>,
    #[serde(default)]
    pub mode: PluginHomeMode,
    #[serde(default = "default_limit")]
    pub limit: usize,
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

    fn selected_plugin<'a>(
        &self,
        ctx: &'a DashboardContext<'_>,
        name: &str,
    ) -> Option<&'a dyn Plugin> {
        ctx.plugins
            .iter()
            .find(|p| p.name().eq_ignore_ascii_case(name))
            .map(|p| p.as_ref())
    }

    fn derive_query_seed(&self, fallback: &str) -> String {
        self.cfg
            .query_seed
            .clone()
            .unwrap_or_else(|| fallback.to_string())
    }

    fn default_query(&self, commands: &[Action], plugin: &dyn Plugin) -> String {
        self.cfg
            .home_query
            .clone()
            .or_else(|| self.cfg.query_seed.clone())
            .or_else(|| {
                commands
                    .iter()
                    .find_map(|c| c.action.strip_prefix("query:").map(|q| q.to_string()))
            })
            .or_else(|| commands.first().map(|c| c.label.clone()))
            .unwrap_or_else(|| plugin.name().to_string())
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
        let mut infos = ctx.plugins.plugin_infos();
        infos.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

        let mut chosen_plugin = self.cfg.plugin.clone();
        egui::ComboBox::from_label("Plugin")
            .selected_text(
                chosen_plugin
                    .as_deref()
                    .unwrap_or("Select a plugin to render here"),
            )
            .show_ui(ui, |ui| {
                for (name, _, _) in &infos {
                    ui.selectable_value(&mut chosen_plugin, Some(name.clone()), name);
                }
            });
        if self.cfg.plugin != chosen_plugin {
            self.cfg.plugin = chosen_plugin.clone();
        }

        let plugin_name = match &self.cfg.plugin {
            Some(p) => p.clone(),
            None => {
                ui.label("Select a plugin to render here.");
                return None;
            }
        };

        let plugin = match self.selected_plugin(ctx, &plugin_name) {
            Some(p) => p,
            None => {
                ui.label(format!("Plugin '{plugin_name}' is not available."));
                return None;
            }
        };

        let mut mode = self.cfg.mode;
        egui::ComboBox::from_label("Mode")
            .selected_text(match mode {
                PluginHomeMode::Commands => "Commands",
                PluginHomeMode::Search => "Search",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut mode, PluginHomeMode::Commands, "Commands");
                ui.selectable_value(&mut mode, PluginHomeMode::Search, "Search");
            });
        self.cfg.mode = mode;

        ui.label(format!("{} home", plugin_name));
        let commands = plugin.commands();
        let limit = self.cfg.limit.max(1);

        match mode {
            PluginHomeMode::Commands => {
                if commands.is_empty() {
                    ui.label("No commands available for this plugin.");
                    return None;
                }
                for action in commands.into_iter().take(limit) {
                    if ui.button(&action.label).clicked() {
                        return Some(WidgetAction {
                            query_override: Some(self.derive_query_seed(&action.label)),
                            action,
                        });
                    }
                }
            }
            PluginHomeMode::Search => {
                let home_query = self.default_query(&commands, plugin);
                let actions: Vec<Action> =
                    plugin.search(&home_query).into_iter().take(limit).collect();

                if actions.is_empty() {
                    ui.label("No results—try a different query.");
                    return None;
                }

                for act in actions {
                    if ui.button(&act.label).clicked() {
                        return Some(WidgetAction {
                            query_override: Some(self.derive_query_seed(&home_query)),
                            action: act,
                        });
                    }
                }
            }
        }

        None
    }
}
