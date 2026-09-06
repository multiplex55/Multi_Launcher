use super::{
    RefreshMode, TimedCache, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
    default_refresh_throttle_secs, edit_typed_settings, observe_owned_search_publication,
    refresh_schedule, refresh_settings_ui, run_refresh_schedule,
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
    pub refresh_mode: RefreshMode,
    #[serde(default = "default_refresh_throttle_secs")]
    pub refresh_throttle_secs: f32,
    #[serde(default)]
    pub manual_refresh_only: bool,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

impl Default for BrowserTabsConfig {
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

pub struct BrowserTabsWidget {
    cfg: BrowserTabsConfig,
    cache: TimedCache<Vec<Action>>,
    error: Option<String>,
    refresh_pending: bool,
    last_search_generation: u64,
    awaiting_ticket: Option<u64>,
}

impl BrowserTabsWidget {
    pub fn new(cfg: BrowserTabsConfig) -> Self {
        let interval = Duration::from_secs_f32(cfg.refresh_interval_secs.max(1.0));
        Self {
            cfg,
            cache: TimedCache::new(Vec::new(), interval),
            error: None,
            refresh_pending: false,
            last_search_generation: 0,
            awaiting_ticket: None,
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
            changed |= refresh_settings_ui(
                ui,
                &mut cfg.refresh_interval_secs,
                &mut cfg.refresh_mode,
                &mut cfg.refresh_throttle_secs,
                Some(&mut cfg.manual_refresh_only),
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
        let (actions, error, ticket) = Self::load_tabs(ctx, self.cfg.limit.max(1));
        self.error = error;
        self.cache.refresh(|data| *data = actions);
        self.awaiting_ticket = ticket;
    }

    fn maybe_refresh(&mut self, ctx: &DashboardContext<'_>) {
        self.update_interval();
        let generation = ctx.plugins.search_generation_for("browser_tabs");
        let schedule = refresh_schedule(
            self.refresh_interval(),
            self.cfg.refresh_mode,
            self.cfg.manual_refresh_only,
            self.cfg.refresh_throttle_secs,
        );
        let request_resolved = self
            .awaiting_ticket
            .is_some_and(|ticket| ctx.plugins.search_ticket_resolved("browser_tabs", ticket));
        observe_owned_search_publication(
            schedule.mode,
            generation,
            &mut self.last_search_generation,
            request_resolved,
            &mut self.awaiting_ticket,
            &mut self.refresh_pending,
        );
        if run_refresh_schedule(
            ctx,
            schedule,
            &mut self.refresh_pending,
            &mut self.cache.last_refresh,
        ) {
            self.refresh(ctx);
            if schedule.mode != RefreshMode::Manual {
                self.awaiting_ticket = None;
            }
        }
    }

    fn load_tabs(
        ctx: &DashboardContext<'_>,
        limit: usize,
    ) -> (Vec<Action>, Option<String>, Option<u64>) {
        let (mut actions, ticket) =
            match ctx.plugins.search_plugin_with_ticket("browser_tabs", "tab") {
                Ok(result) => result,
                Err(error) => return (Vec::new(), Some(error.into()), None),
            };
        if actions.len() > limit {
            actions.truncate(limit);
        }
        (actions, None, ticket)
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
                "Cached for {:.0}s. Refresh to enumerate tabs immediately.",
                self.cfg.refresh_interval_secs
            ),
        };
        if ui.small_button("Refresh").on_hover_text(tooltip).clicked() {
            self.refresh_pending = true;
        }
        None
    }
}
