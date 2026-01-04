use super::{Widget, WidgetAction};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginHomeConfig {
    pub plugin: Option<String>,
    #[serde(default)]
    pub home_query: Option<String>,
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
        let plugin_name = match &self.cfg.plugin {
            Some(p) => p,
            None => {
                ui.label("Configure plugin name");
                return None;
            }
        };

        let mut actions: Vec<Action> = Vec::new();
        for p in ctx.plugins.iter() {
            if p.name().eq_ignore_ascii_case(plugin_name) {
                actions = p
                    .search(self.cfg.home_query.as_deref().unwrap_or_default())
                    .into_iter()
                    .take(self.cfg.limit)
                    .collect();
                break;
            }
        }

        ui.label(format!("{} home", plugin_name));
        for act in actions {
            if ui.button(&act.label).clicked() {
                return Some(WidgetAction {
                    query_override: self
                        .cfg
                        .home_query
                        .clone()
                        .or_else(|| Some(act.label.clone())),
                    action: act,
                });
            }
        }

        None
    }
}
