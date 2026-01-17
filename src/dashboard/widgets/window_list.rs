use super::{
    default_refresh_throttle_secs, edit_typed_settings, find_plugin, refresh_schedule,
    refresh_settings_ui, run_refresh_schedule, RefreshMode, TimedCache, Widget, WidgetAction,
    WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::Duration;

fn default_limit() -> usize {
    8
}

fn default_refresh_interval() -> f32 {
    5.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsConfig {
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: f32,
    #[serde(default)]
    pub refresh_mode: RefreshMode,
    #[serde(default = "default_refresh_throttle_secs")]
    pub refresh_throttle_secs: f32,
    #[serde(default)]
    pub manual_refresh_only: bool,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

impl Default for WindowsConfig {
    fn default() -> Self {
        Self {
            refresh_interval_secs: default_refresh_interval(),
            refresh_mode: RefreshMode::Auto,
            refresh_throttle_secs: default_refresh_throttle_secs(),
            manual_refresh_only: false,
            limit: default_limit(),
        }
    }
}

pub struct WindowsWidget {
    cfg: WindowsConfig,
    cache: TimedCache<Vec<Action>>,
    error: Option<String>,
    refresh_pending: bool,
}

impl WindowsWidget {
    pub fn new(cfg: WindowsConfig) -> Self {
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
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut WindowsConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Show up to");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.limit).clamp_range(1..=25))
                    .changed();
                ui.label("windows");
            });
            changed |= refresh_settings_ui(
                ui,
                &mut cfg.refresh_interval_secs,
                &mut cfg.refresh_mode,
                &mut cfg.refresh_throttle_secs,
                Some(&mut cfg.manual_refresh_only),
                "Window enumeration is cached. The widget will skip refreshing until this many seconds have passed. Use Refresh to update immediately.",
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
        let (actions, error) = Self::load_windows(ctx);
        self.error = error;
        self.cache.refresh(|data| *data = actions);
    }

    fn maybe_refresh(&mut self, ctx: &DashboardContext<'_>) {
        self.update_interval();
        let schedule = refresh_schedule(
            self.refresh_interval(),
            self.cfg.refresh_mode,
            self.cfg.manual_refresh_only,
            self.cfg.refresh_throttle_secs,
        );
        if run_refresh_schedule(
            ctx,
            schedule,
            &mut self.refresh_pending,
            &mut self.cache.last_refresh,
        ) {
            self.refresh(ctx);
        }
    }

    fn load_windows(ctx: &DashboardContext<'_>) -> (Vec<Action>, Option<String>) {
        let Some(plugin) = find_plugin(ctx, "windows") else {
            return (Vec::new(), Some("Windows plugin not available.".into()));
        };
        (plugin.search("win"), None)
    }

    fn grouped_actions(&self) -> Vec<(String, Option<Action>, Option<Action>)> {
        let mut grouped: Vec<(String, Option<Action>, Option<Action>)> = Vec::new();
        for action in &self.cache.data {
            let label = action.label.to_lowercase();
            let title = label
                .strip_prefix("switch to ")
                .or_else(|| label.strip_prefix("close "))
                .unwrap_or(&action.label)
                .trim()
                .to_string();
            let idx = grouped.iter().position(|(t, _, _)| t == &title);
            let entry = if let Some(idx) = idx {
                &mut grouped[idx]
            } else {
                grouped.push((title, None, None));
                grouped.last_mut().unwrap()
            };
            if label.starts_with("switch to") {
                entry.1 = Some(action.clone());
            } else if label.starts_with("close") {
                entry.2 = Some(action.clone());
            } else {
                entry.1.get_or_insert_with(|| action.clone());
            }
        }
        grouped.truncate(self.cfg.limit);
        grouped
    }
}

impl Default for WindowsWidget {
    fn default() -> Self {
        Self::new(WindowsConfig::default())
    }
}

impl Widget for WindowsWidget {
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
            ui.label("No windows found.");
            return None;
        }

        let mut clicked = None;
        for (title, switch, close) in self.grouped_actions() {
            ui.horizontal(|ui| {
                if let Some(action) = &switch {
                    if ui.button(&action.label).clicked() {
                        clicked = Some(action.clone());
                    }
                }
                if let Some(action) = &close {
                    if ui.small_button("Close").clicked() {
                        clicked = Some(action.clone());
                    }
                }
                ui.label(egui::RichText::new(title).small());
            });
        }

        clicked.map(|action| WidgetAction {
            query_override: Some(action.label.clone()),
            action,
        })
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<WindowsConfig>(settings.clone()) {
            self.cfg = cfg;
            self.update_interval();
            self.cache.invalidate();
            self.refresh_pending = true;
        }
    }

    fn header_ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &DashboardContext<'_>,
    ) -> Option<WidgetAction> {
        let schedule = refresh_schedule(
            self.refresh_interval(),
            self.cfg.refresh_mode,
            self.cfg.manual_refresh_only,
            self.cfg.refresh_throttle_secs,
        );
        let tooltip = match schedule.mode {
            RefreshMode::Manual => "Manual refresh only.".to_string(),
            RefreshMode::Throttled => {
                format!(
                    "Minimum refresh interval {:.0}s.",
                    schedule.throttle.as_secs_f32()
                )
            }
            RefreshMode::Auto => format!(
                "Cached for {:.0}s. Refresh to enumerate windows immediately.",
                self.cfg.refresh_interval_secs
            ),
        };
        if ui.small_button("Refresh").on_hover_text(tooltip).clicked() {
            self.refresh_pending = true;
        }
        None
    }
}
