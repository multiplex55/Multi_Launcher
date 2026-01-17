use super::{
    default_refresh_throttle_secs, edit_typed_settings, refresh_schedule, refresh_settings_ui,
    run_refresh_schedule, RefreshMode, Widget, WidgetAction, WidgetSettingsContext,
    WidgetSettingsUiResult,
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
pub struct ProcessesConfig {
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

impl Default for ProcessesConfig {
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

pub struct ProcessesWidget {
    cfg: ProcessesConfig,
    refresh_pending: bool,
    last_refresh: std::time::Instant,
}

impl ProcessesWidget {
    pub fn new(cfg: ProcessesConfig) -> Self {
        let interval = Duration::from_secs_f32(cfg.refresh_interval_secs.max(1.0));
        Self {
            cfg,
            refresh_pending: false,
            last_refresh: std::time::Instant::now() - interval,
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut ProcessesConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Show up to");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.limit).clamp_range(1..=25))
                    .changed();
                ui.label("processes");
            });
            changed |= refresh_settings_ui(
                ui,
                &mut cfg.refresh_interval_secs,
                &mut cfg.refresh_mode,
                &mut cfg.refresh_throttle_secs,
                Some(&mut cfg.manual_refresh_only),
                "Process enumeration is cached. The widget will skip refreshing until this many seconds have passed. Use Refresh to update immediately.",
            );
            changed
        })
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_secs_f32(self.cfg.refresh_interval_secs.max(1.0))
    }
    fn grouped_actions(&self, actions: &[Action]) -> Vec<(String, Option<Action>, Option<Action>)> {
        let mut grouped: Vec<(String, Option<Action>, Option<Action>)> = Vec::new();
        for action in actions {
            let label = action.label.to_lowercase();
            let idx = grouped.iter().position(|(desc, _, _)| desc == &action.desc);
            let entry = if let Some(idx) = idx {
                &mut grouped[idx]
            } else {
                grouped.push((action.desc.clone(), None, None));
                grouped.last_mut().unwrap()
            };
            if label.starts_with("switch to") {
                entry.1 = Some(action.clone());
            } else if label.starts_with("kill") {
                entry.2 = Some(action.clone());
            } else {
                entry.1.get_or_insert_with(|| action.clone());
            }
        }
        grouped.truncate(self.cfg.limit);
        grouped
    }
}

impl Default for ProcessesWidget {
    fn default() -> Self {
        Self::new(ProcessesConfig::default())
    }
}

impl Widget for ProcessesWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
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
            &mut self.last_refresh,
        ) {
            ctx.data_cache.refresh_processes(ctx.plugins);
            self.last_refresh = std::time::Instant::now();
        }
        let snapshot = ctx.data_cache.snapshot();

        if let Some(err) = &snapshot.process_error {
            ui.colored_label(egui::Color32::YELLOW, err);
        }

        if snapshot.processes.is_empty() {
            ui.label("No processes found.");
            return None;
        }

        let mut clicked = None;
        let grouped = self.grouped_actions(snapshot.processes.as_ref());
        let row_height =
            ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y + 8.0;
        let scroll_id = ui.id().with("process_list_scroll");
        egui::ScrollArea::both()
            .id_source(scroll_id)
            .auto_shrink([false; 2])
            .show_rows(ui, row_height, grouped.len(), |ui, range| {
                for (desc, switch, kill) in &grouped[range] {
                    ui.horizontal(|ui| {
                        if let Some(action) = switch {
                            if ui
                                .add(egui::Button::new(&action.label).wrap(false))
                                .clicked()
                            {
                                clicked = Some(action.clone());
                            }
                        }
                        if let Some(action) = kill {
                            if ui.small_button("Kill").clicked() {
                                clicked = Some(action.clone());
                            }
                        }
                        ui.add(egui::Label::new(egui::RichText::new(desc).small()).wrap(false));
                    });
                }
            });

        clicked.map(|action| WidgetAction {
            query_override: Some(action.label.clone()),
            action,
        })
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<ProcessesConfig>(settings.clone()) {
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
                "Cached for {:.0}s. Refresh to enumerate processes immediately.",
                self.cfg.refresh_interval_secs
            ),
        };
        if ui.small_button("Refresh").on_hover_text(tooltip).clicked() {
            self.refresh_pending = true;
        }
        None
    }
}
