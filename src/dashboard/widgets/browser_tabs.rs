use super::{
    edit_typed_settings, find_plugin, refresh_interval_setting, TimedCache, Widget, WidgetAction,
    WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::Duration;

fn default_limit() -> usize {
    10
}

fn default_refresh_interval() -> f32 {
    5.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserTabsConfig {
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: f32,
    #[serde(default)]
    pub manual_refresh_only: bool,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

impl Default for BrowserTabsConfig {
    fn default() -> Self {
        Self {
            refresh_interval_secs: default_refresh_interval(),
            manual_refresh_only: false,
            limit: default_limit(),
        }
    }
}

pub struct BrowserTabsWidget {
    cfg: BrowserTabsConfig,
    cache: TimedCache<Vec<Action>>,
    error: Option<String>,
    refresh_pending: bool,
}

impl BrowserTabsWidget {
    pub fn new(cfg: BrowserTabsConfig) -> Self {
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
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut BrowserTabsConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Show up to");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.limit).clamp_range(1..=50))
                    .changed();
                ui.label("tabs");
            });
            changed |= refresh_interval_setting(
                ui,
                &mut cfg.refresh_interval_secs,
                &mut cfg.manual_refresh_only,
                "Tab enumeration is cached. The widget will skip refreshing until this many seconds have passed. Use Refresh to update immediately.",
            );
            changed
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
        let (actions, error) = Self::load_tabs(ctx, self.cfg.limit.max(1));
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

    fn load_tabs(ctx: &DashboardContext<'_>, limit: usize) -> (Vec<Action>, Option<String>) {
        let Some(plugin) = find_plugin(ctx, "browser_tabs") else {
            return (
                Vec::new(),
                Some("Browser tabs plugin not available.".into()),
            );
        };

        let mut actions = plugin.search("tab");
        if actions.len() > limit {
            actions.truncate(limit);
        }
        (actions, None)
    }
}

impl Default for BrowserTabsWidget {
    fn default() -> Self {
        Self::new(BrowserTabsConfig::default())
    }
}

impl Widget for BrowserTabsWidget {
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
            ui.label("No browser tabs found.");
            return None;
        }

        let mut clicked = None;
        for action in self.cache.data.iter() {
            if ui
                .button(&action.label)
                .on_hover_text(&action.desc)
                .clicked()
            {
                clicked = Some(WidgetAction {
                    query_override: Some(action.label.clone()),
                    action: action.clone(),
                });
            }
        }

        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<BrowserTabsConfig>(settings.clone()) {
            self.cfg = cfg;
            self.update_interval();
            self.cache.invalidate();
            self.refresh_pending = true;
        }
    }

    fn header_ui(&mut self, ui: &mut egui::Ui, ctx: &DashboardContext<'_>) -> Option<WidgetAction> {
        let tooltip = format!(
            "Cached for {:.0}s. Refresh to enumerate tabs immediately.",
            self.cfg.refresh_interval_secs
        );
        if ui.small_button("Refresh").on_hover_text(tooltip).clicked() {
            self.refresh(ctx);
        }
        None
    }
}
