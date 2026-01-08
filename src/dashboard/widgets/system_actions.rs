use super::{
    edit_typed_settings, find_plugin, refresh_interval_setting, TimedCache, Widget, WidgetAction,
    WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::Duration;

fn default_refresh_interval() -> f32 {
    30.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: f32,
    #[serde(default)]
    pub manual_refresh_only: bool,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            refresh_interval_secs: default_refresh_interval(),
            manual_refresh_only: false,
        }
    }
}

pub struct SystemWidget {
    cfg: SystemConfig,
    cache: TimedCache<Vec<Action>>,
    error: Option<String>,
    refresh_pending: bool,
}

impl SystemWidget {
    pub fn new(cfg: SystemConfig) -> Self {
        let interval = Duration::from_secs_f32(cfg.refresh_interval_secs.max(1.0));
        Self {
            cfg,
            cache: TimedCache::new(Vec::new(), interval),
            error: None,
            refresh_pending: false,
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut SystemConfig, _ctx| {
            refresh_interval_setting(
                ui,
                &mut cfg.refresh_interval_secs,
                &mut cfg.manual_refresh_only,
                "System actions are cached. The widget will skip refreshing until this many seconds have passed. Use Refresh to update immediately.",
            )
        })
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_secs_f32(self.cfg.refresh_interval_secs.max(1.0))
    }

    fn update_interval(&mut self) {
        self.cache.set_interval(self.refresh_interval());
    }

    fn refresh(&mut self, ctx: &DashboardContext<'_>) {
        self.update_interval();
        let (actions, error) = Self::load_actions(ctx);
        self.error = error;
        self.cache.refresh(|data| *data = actions);
    }

    fn maybe_refresh(&mut self, ctx: &DashboardContext<'_>) {
        self.update_interval();
        if self.refresh_pending {
            self.refresh_pending = false;
            self.refresh(ctx);
        } else if !self.cfg.manual_refresh_only && self.cache.should_refresh() {
            self.refresh(ctx);
        }
    }

    fn load_actions(ctx: &DashboardContext<'_>) -> (Vec<Action>, Option<String>) {
        let Some(plugin) = find_plugin(ctx, "system") else {
            return (Vec::new(), Some("System plugin not available.".into()));
        };
        (plugin.search("sys"), None)
    }
}

impl Default for SystemWidget {
    fn default() -> Self {
        Self::new(SystemConfig::default())
    }
}

impl Widget for SystemWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        self.maybe_refresh(ctx);

        if let Some(err) = &self.error {
            ui.colored_label(egui::Color32::YELLOW, err);
        }

        if self.cache.data.is_empty() {
            ui.label("No system actions available.");
            return None;
        }

        let mut clicked = None;
        for action in &self.cache.data {
            if ui.button(&action.label).clicked() {
                clicked = Some(action.clone());
            }
        }

        clicked.map(|action| WidgetAction {
            query_override: Some(action.label.clone()),
            action,
        })
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<SystemConfig>(settings.clone()) {
            self.cfg = cfg;
            self.update_interval();
            self.cache.invalidate();
            self.refresh_pending = true;
        }
    }

    fn header_ui(&mut self, ui: &mut egui::Ui, ctx: &DashboardContext<'_>) -> Option<WidgetAction> {
        let tooltip = format!(
            "Cached for {:.0}s. Refresh to update system actions immediately.",
            self.cfg.refresh_interval_secs
        );
        if ui.small_button("Refresh").on_hover_text(tooltip).clicked() {
            self.refresh(ctx);
        }
        None
    }
}
