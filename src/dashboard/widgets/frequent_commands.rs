use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrequentCommandsConfig {
    #[serde(default = "default_count")]
    pub count: usize,
}

impl Default for FrequentCommandsConfig {
    fn default() -> Self {
        Self {
            count: default_count(),
        }
    }
}

fn default_count() -> usize {
    5
}

pub struct FrequentCommandsWidget {
    cfg: FrequentCommandsConfig,
}

impl FrequentCommandsWidget {
    pub fn new(cfg: FrequentCommandsConfig) -> Self {
        Self { cfg }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(
            ui,
            value,
            ctx,
            |ui, cfg: &mut FrequentCommandsConfig, _ctx| {
                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label("Count");
                    changed |= ui
                        .add(egui::DragValue::new(&mut cfg.count).clamp_range(1..=50))
                        .changed();
                });
                changed
            },
        )
    }

    fn resolve_action<'a>(&self, actions: &'a [Action], key: &str) -> Option<Action> {
        actions
            .iter()
            .find(|a| a.action == key)
            .cloned()
            .or_else(|| {
                Some(Action {
                    label: key.to_string(),
                    desc: "Command".into(),
                    action: key.to_string(),
                    args: None,
                })
            })
    }
}

impl Default for FrequentCommandsWidget {
    fn default() -> Self {
        Self {
            cfg: FrequentCommandsConfig::default(),
        }
    }
}

impl Widget for FrequentCommandsWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let mut usage: Vec<(&String, &u32)> = ctx.usage.iter().collect();
        usage.sort_by(|a, b| b.1.cmp(a.1));
        ui.label("Frequent commands");
        for (idx, (action_id, _)) in usage.into_iter().enumerate() {
            if idx >= self.cfg.count {
                break;
            }
            if let Some(action) = self.resolve_action(ctx.actions, action_id) {
                if ui.button(&action.label).clicked() {
                    return Some(WidgetAction {
                        query_override: Some(action.label.clone()),
                        action,
                    });
                }
            }
        }
        None
    }
}
