use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TodoSummaryConfig {
    #[serde(default)]
    pub query: Option<String>,
}

pub struct TodoSummaryWidget {
    cfg: TodoSummaryConfig,
}

impl TodoSummaryWidget {
    pub fn new(cfg: TodoSummaryConfig) -> Self {
        Self { cfg }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut TodoSummaryConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Query override");
                let mut query = cfg.query.clone().unwrap_or_default();
                if ui.text_edit_singleline(&mut query).changed() {
                    cfg.query = if query.trim().is_empty() {
                        None
                    } else {
                        Some(query)
                    };
                    changed = true;
                }
            });
            changed
        })
    }
}

impl Default for TodoSummaryWidget {
    fn default() -> Self {
        Self {
            cfg: TodoSummaryConfig::default(),
        }
    }
}

impl Widget for TodoSummaryWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let todos = crate::plugins::todo::TODO_DATA
            .read()
            .ok()
            .map(|t| t.clone())
            .unwrap_or_default();
        let done = todos.iter().filter(|t| t.done).count();
        let total = todos.len();
        ui.label(format!("Todos: {done}/{total} done"));
        if ui.button("Open todos").clicked() {
            return Some(WidgetAction {
                action: Action {
                    label: "Todos".into(),
                    desc: "Todo".into(),
                    action: "todo:dialog".into(),
                    args: None,
                },
                query_override: self.cfg.query.clone().or_else(|| Some("todo".into())),
            });
        }
        None
    }
}
