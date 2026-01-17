use super::{
    default_refresh_throttle_secs, edit_typed_settings, refresh_schedule, refresh_settings_ui,
    run_refresh_schedule, RefreshMode, TimedCache, Widget, WidgetAction, WidgetSettingsContext,
    WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::Duration;

fn default_refresh_ms() -> u64 {
    2000
}

fn default_count() -> usize {
    6
}

fn default_show_desc() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryListConfig {
    #[serde(default = "default_refresh_ms")]
    pub refresh_ms: u64,
    #[serde(default)]
    pub refresh_mode: RefreshMode,
    #[serde(default = "default_refresh_throttle_secs")]
    pub refresh_throttle_secs: f32,
    #[serde(default)]
    pub manual_refresh_only: bool,
    #[serde(default = "default_count")]
    pub count: usize,
    #[serde(default = "default_show_desc")]
    pub show_desc: bool,
    #[serde(default)]
    pub query: String,
}

impl Default for QueryListConfig {
    fn default() -> Self {
        Self {
            refresh_ms: default_refresh_ms(),
            refresh_mode: RefreshMode::Auto,
            refresh_throttle_secs: default_refresh_throttle_secs(),
            manual_refresh_only: false,
            count: default_count(),
            show_desc: true,
            query: String::new(),
        }
    }
}

pub struct QueryListWidget {
    cfg: QueryListConfig,
    cache: TimedCache<Vec<Action>>,
    last_query: String,
    refresh_pending: bool,
}

impl QueryListWidget {
    pub fn new(cfg: QueryListConfig) -> Self {
        let interval = Duration::from_millis(cfg.refresh_ms.max(250));
        let last_query = cfg.query.clone();
        Self {
            cfg,
            cache: TimedCache::new(Vec::new(), interval),
            last_query,
            refresh_pending: false,
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut QueryListConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Query");
                changed |= ui.text_edit_singleline(&mut cfg.query).changed();
            });
            ui.horizontal(|ui| {
                ui.label("Show up to");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.count).clamp_range(1..=25))
                    .changed();
                ui.label("results");
            });
            let mut refresh_secs = cfg.refresh_ms as f32 / 1000.0;
            changed |= refresh_settings_ui(
                ui,
                &mut refresh_secs,
                &mut cfg.refresh_mode,
                &mut cfg.refresh_throttle_secs,
                Some(&mut cfg.manual_refresh_only),
                "Results are cached between refreshes.",
            );
            cfg.refresh_ms = (refresh_secs * 1000.0) as u64;
            changed |= ui
                .checkbox(&mut cfg.show_desc, "Show descriptions")
                .changed();
            changed
        })
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_millis(self.cfg.refresh_ms.max(250))
    }

    fn refresh(&mut self, ctx: &DashboardContext<'_>) {
        let query = self.cfg.query.trim();
        let actions = if query.is_empty() {
            Vec::new()
        } else {
            ctx.plugins.search_filtered(query, None, None)
        };
        self.cache.refresh(|data| *data = actions);
    }

    fn maybe_refresh(&mut self, ctx: &DashboardContext<'_>) {
        self.cache.set_interval(self.refresh_interval());
        if self.last_query != self.cfg.query {
            self.last_query = self.cfg.query.clone();
            self.refresh_pending = true;
        }
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
}

impl Default for QueryListWidget {
    fn default() -> Self {
        Self::new(QueryListConfig::default())
    }
}

impl Widget for QueryListWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        self.maybe_refresh(ctx);

        if self.cfg.query.trim().is_empty() {
            ui.label("Set a query in settings to show results.");
            return None;
        }

        if self.cache.data.is_empty() {
            ui.label("No results.");
            return None;
        }

        let mut clicked = None;
        let row_height =
            ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y + 8.0;
        let max_rows = self.cache.data.len().min(self.cfg.count);
        let scroll_id = ui.id().with("query_list_scroll");
        egui::ScrollArea::both()
            .id_source(scroll_id)
            .auto_shrink([false; 2])
            .show_rows(ui, row_height, max_rows, |ui, range| {
                for action in &self.cache.data[range] {
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Button::new(&action.label).wrap(false))
                            .clicked()
                        {
                            clicked = Some(action.clone());
                        }
                        if self.cfg.show_desc {
                            ui.add(
                                egui::Label::new(egui::RichText::new(&action.desc).small())
                                    .wrap(false),
                            );
                        }
                    });
                }
            });

        clicked.map(|action| WidgetAction {
            query_override: Some(action.label.clone()),
            action,
        })
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<QueryListConfig>(settings.clone()) {
            self.cfg = cfg;
            self.cache.set_interval(self.refresh_interval());
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
                "Cached for {:.1}s. Refresh to query immediately.",
                self.refresh_interval().as_secs_f32()
            ),
        };
        if ui.small_button("Refresh").on_hover_text(tooltip).clicked() {
            self.refresh_pending = true;
        }
        None
    }
}
