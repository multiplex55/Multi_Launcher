use super::{
    default_refresh_throttle_secs, edit_typed_settings, refresh_schedule, refresh_settings_ui,
    run_refresh_schedule, RefreshMode, TimedCache, Widget, WidgetAction, WidgetSettingsContext,
    WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::stopwatch;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::Duration;

fn default_refresh_interval() -> f32 {
    stopwatch::refresh_rate().max(1.0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopwatchConfig {
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: f32,
    #[serde(default)]
    pub refresh_mode: RefreshMode,
    #[serde(default = "default_refresh_throttle_secs")]
    pub refresh_throttle_secs: f32,
    #[serde(default)]
    pub manual_refresh_only: bool,
}

impl Default for StopwatchConfig {
    fn default() -> Self {
        Self {
            refresh_interval_secs: default_refresh_interval(),
            refresh_mode: RefreshMode::Auto,
            refresh_throttle_secs: default_refresh_throttle_secs(),
            manual_refresh_only: false,
        }
    }
}

#[derive(Debug, Clone)]
struct StopwatchRow {
    id: u64,
    label: String,
    running: bool,
}

pub struct StopwatchWidget {
    cfg: StopwatchConfig,
    cache: TimedCache<Vec<StopwatchRow>>,
    refresh_pending: bool,
}

impl StopwatchWidget {
    pub fn new(cfg: StopwatchConfig) -> Self {
        let interval = Duration::from_secs_f32(cfg.refresh_interval_secs.max(1.0));
        Self {
            cfg,
            cache: TimedCache::new(Vec::new(), interval),
            refresh_pending: true,
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut StopwatchConfig, _ctx| {
            refresh_settings_ui(
                ui,
                &mut cfg.refresh_interval_secs,
                &mut cfg.refresh_mode,
                &mut cfg.refresh_throttle_secs,
                Some(&mut cfg.manual_refresh_only),
                "Stopwatch list refreshes on this interval unless manual refresh is enabled.",
            )
        })
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_secs_f32(self.cfg.refresh_interval_secs.max(1.0))
    }

    fn refresh(&mut self) {
        self.cache.set_interval(self.refresh_interval());
        self.cache.refresh(|rows| {
            let mut next: Vec<StopwatchRow> = stopwatch::all_stopwatches()
                .into_iter()
                .map(|(id, label, _elapsed, running)| StopwatchRow { id, label, running })
                .collect();
            next.sort_by(|a, b| a.label.cmp(&b.label).then(a.id.cmp(&b.id)));
            *rows = next;
        });
    }

    fn maybe_refresh(&mut self, ctx: &DashboardContext<'_>) {
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
            self.refresh();
        }
    }

    fn action_for(id: u64, label: &str, action: &str, query: &str) -> WidgetAction {
        WidgetAction {
            action: Action {
                label: format!("{label} stopwatch {id}"),
                desc: "Stopwatch".into(),
                action: action.to_string(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            },
            query_override: Some(query.to_string()),
        }
    }
}

impl Default for StopwatchWidget {
    fn default() -> Self {
        Self::new(StopwatchConfig::default())
    }
}

impl Widget for StopwatchWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        self.maybe_refresh(ctx);

        let mut clicked = None;

        if self.cache.data.is_empty() {
            ui.label("No stopwatches running");
            return None;
        }

        for row in &self.cache.data {
            let status = if row.running { "Running" } else { "Paused" };
            let elapsed = stopwatch::format_elapsed(row.id).unwrap_or_else(|| "--".into());
            ui.horizontal(|ui| {
                ui.label(&row.label);
                ui.label(egui::RichText::new(elapsed).monospace());
                ui.label(egui::RichText::new(status).small());
                if row.running {
                    if ui.small_button("Pause").clicked() {
                        clicked = Some(Self::action_for(
                            row.id,
                            "Pause",
                            &format!("stopwatch:pause:{}", row.id),
                            "sw pause",
                        ));
                    }
                } else if ui.small_button("Resume").clicked() {
                    clicked = Some(Self::action_for(
                        row.id,
                        "Resume",
                        &format!("stopwatch:resume:{}", row.id),
                        "sw resume",
                    ));
                }
                if ui.small_button("Stop").clicked() {
                    clicked = Some(Self::action_for(
                        row.id,
                        "Stop",
                        &format!("stopwatch:stop:{}", row.id),
                        "sw stop",
                    ));
                }
            });
        }

        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<StopwatchConfig>(settings.clone()) {
            self.cfg = cfg;
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
                "Refreshes every {:.0}s unless you refresh manually.",
                self.cfg.refresh_interval_secs
            ),
        };
        if ui.small_button("Refresh").on_hover_text(tooltip).clicked() {
            self.refresh_pending = true;
        }
        None
    }
}
