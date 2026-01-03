use super::{Widget, WidgetAction};
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

pub struct RecentCommandsWidget {
    cfg: RecentCommandsConfig,
}

impl RecentCommandsWidget {
    pub fn new(cfg: RecentCommandsConfig) -> Self {
        Self { cfg }
    }
}

impl Default for RecentCommandsWidget {
    fn default() -> Self {
        Self {
            cfg: RecentCommandsConfig::default(),
        }
    }
}

impl Widget for RecentCommandsWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let mut clicked = None;
        ui.label("Recent commands");
        if let Some(history) = crate::history::with_history(|h| {
            h.iter().take(self.cfg.count).cloned().collect::<Vec<_>>()
        }) {
            for entry in history {
                if ui.button(&entry.action.label).clicked() {
                    clicked = Some(WidgetAction {
                        action: entry.action.clone(),
                        query_override: Some(entry.query.clone()),
                    });
                }
            }
        }
        clicked
    }
}
