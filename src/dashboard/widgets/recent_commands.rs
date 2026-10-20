use super::{
    Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
    dashboard_action_row_egui_id, edit_typed_settings, jump_action_identity, jump_response_clicked,
};
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentCommandsConfig {
    #[serde(default = "default_count")]
    pub count: usize,
}

impl Default for RecentCommandsConfig {
    fn default() -> Self {
        Self {
            count: default_count(),
        }
    }
}

fn default_count() -> usize {
    5
}

#[derive(Default)]
pub struct RecentCommandsWidget {
    cfg: RecentCommandsConfig,
}

impl RecentCommandsWidget {
    pub fn new(cfg: RecentCommandsConfig) -> Self {
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
            |ui, cfg: &mut RecentCommandsConfig, _ctx| {
                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label("Count");
                    changed |= ui
                        .add(egui::DragValue::new(&mut cfg.count).range(1..=50))
                        .changed();
                });
                changed
            },
        )
    }
}

impl Widget for RecentCommandsWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let mut clicked = None;
        ui.label("Recent commands");
        if let Some(history) = crate::history::with_history(|h| {
            h.iter().take(self.cfg.count).cloned().collect::<Vec<_>>()
        }) {
            for (order, entry) in history.iter().enumerate() {
                let occurrence = history[..order]
                    .iter()
                    .filter(|previous| {
                        previous.action.action == entry.action.action
                            && previous.action.args == entry.action.args
                            && previous.query == entry.query
                            && previous.timestamp == entry.timestamp
                    })
                    .count();
                let native_id = dashboard_action_row_egui_id(
                    ui,
                    &entry.action,
                    None,
                    Some(&entry.query),
                    Some(entry.timestamp),
                    occurrence,
                );
                ui.push_id(native_id, |ui| {
                    let response = ui.button(&entry.action.label);
                    if jump_response_clicked(
                        ui,
                        ctx,
                        &response,
                        || {
                            jump_action_identity(
                                &entry.action,
                                None,
                                Some(&entry.query),
                                Some(entry.timestamp),
                                occurrence,
                            )
                        },
                        || entry.action.label.clone(),
                        order as u64,
                    ) {
                        clicked = Some(WidgetAction {
                            action: entry.action.clone(),
                            query_override: Some(entry.query.clone()),
                        });
                    }
                });
            }
        }
        clicked
    }
}
