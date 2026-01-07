use super::{edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult};
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};

fn default_show_progress() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoBurndownConfig {
    #[serde(default = "default_show_progress")]
    pub show_progress: bool,
}

impl Default for TodoBurndownConfig {
    fn default() -> Self {
        Self { show_progress: true }
    }
}

pub struct TodoBurndownWidget {
    cfg: TodoBurndownConfig,
}

impl TodoBurndownWidget {
    pub fn new(cfg: TodoBurndownConfig) -> Self {
        Self { cfg }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut TodoBurndownConfig, _ctx| {
            ui.checkbox(&mut cfg.show_progress, "Show progress bar")
                .changed()
        })
    }
}

impl Default for TodoBurndownWidget {
    fn default() -> Self {
        Self::new(TodoBurndownConfig::default())
    }
}

impl Widget for TodoBurndownWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let snapshot = ctx.data_cache.snapshot();
        let todos = snapshot.todos.as_ref();
        let total = todos.len();
        let done = todos.iter().filter(|t| t.done).count();
        let remaining = total.saturating_sub(done);

        ui.label(format!("Todos: {done}/{total} done"));
        ui.label(format!("Remaining: {remaining}"));
        if self.cfg.show_progress && total > 0 {
            let pct = done as f32 / total as f32;
            ui.add(egui::ProgressBar::new(pct).show_percentage());
        }

        None
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<TodoBurndownConfig>(settings.clone()) {
            self.cfg = cfg;
        }
    }
}
