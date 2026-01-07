use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};

fn default_queries() -> Vec<String> {
    vec![
        "sys".into(),
        "net".into(),
        "info cpu".into(),
        "vol".into(),
        "bright".into(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickToolsConfig {
    #[serde(default = "default_queries")]
    pub queries: Vec<String>,
}

impl Default for QuickToolsConfig {
    fn default() -> Self {
        Self {
            queries: default_queries(),
        }
    }
}

pub struct QuickToolsWidget {
    cfg: QuickToolsConfig,
}

impl QuickToolsWidget {
    pub fn new(cfg: QuickToolsConfig) -> Self {
        Self { cfg }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut QuickToolsConfig, _ctx| {
            let mut changed = false;
            let mut remove_idx = None;
            for (idx, query) in cfg.queries.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("Tool {}", idx + 1));
                    changed |= ui.text_edit_singleline(query).changed();
                    if ui.small_button("✕").clicked() {
                        remove_idx = Some(idx);
                    }
                });
            }
            if let Some(idx) = remove_idx {
                cfg.queries.remove(idx);
                changed = true;
            }
            if ui.button("Add tool").clicked() {
                cfg.queries.push(String::new());
                changed = true;
            }
            changed
        })
    }

    fn action_for(query: &str) -> WidgetAction {
        WidgetAction {
            action: Action {
                label: query.to_string(),
                desc: "Tool".into(),
                action: format!("query:{query}"),
                args: None,
            },
            query_override: Some(query.to_string()),
        }
    }
}

impl Default for QuickToolsWidget {
    fn default() -> Self {
        Self::new(QuickToolsConfig::default())
    }
}

impl Widget for QuickToolsWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        if self.cfg.queries.is_empty() {
            ui.label("Add tools in the widget settings.");
            return None;
        }

        let mut clicked = None;
        ui.horizontal_wrapped(|ui| {
            for query in &self.cfg.queries {
                let query = query.trim();
                if query.is_empty() {
                    continue;
                }
                if ui.button(query).clicked() {
                    clicked = Some(Self::action_for(query));
                }
            }
        });
        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<QuickToolsConfig>(settings.clone()) {
            self.cfg = cfg;
        }
    }
}
