use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::timer;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::Duration;

fn default_count() -> usize {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveTimersConfig {
    #[serde(default = "default_count")]
    pub count: usize,
    #[serde(default)]
    pub show_completed_recently: bool,
}

impl Default for ActiveTimersConfig {
    fn default() -> Self {
        Self {
            count: default_count(),
            show_completed_recently: true,
        }
    }
}

pub struct ActiveTimersWidget {
    cfg: ActiveTimersConfig,
}

impl ActiveTimersWidget {
    pub fn new(cfg: ActiveTimersConfig) -> Self {
        Self { cfg }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut ActiveTimersConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Show up to");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.count).clamp_range(1..=20))
                    .changed();
                ui.label("timers");
            });
            changed |= ui
                .checkbox(&mut cfg.show_completed_recently, "Show recent completions")
                .changed();
            changed
        })
    }

    fn format_duration(dur: Duration) -> String {
        let secs = dur.as_secs();
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        if h > 0 {
            format!("{:02}:{:02}:{:02}", h, m, s)
        } else {
            format!("{:02}:{:02}", m, s)
        }
    }
}

impl Default for ActiveTimersWidget {
    fn default() -> Self {
        Self {
            cfg: ActiveTimersConfig::default(),
        }
    }
}

impl Widget for ActiveTimersWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let mut timers = timer::running_timers();
        timers.sort_by_key(|t| t.2);
        let mut clicked = None;

        if timers.is_empty() {
            ui.label("No running timers");
        }

        for (id, label, remaining, start_ts) in timers.into_iter().take(self.cfg.count) {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} – {} left",
                    label,
                    Self::format_duration(remaining)
                ));
                ui.label(egui::RichText::new(timer::format_ts(start_ts)).small());
                if ui.small_button("Pause").clicked() {
                    clicked = Some(WidgetAction {
                        action: Action {
                            label: format!("Pause timer {id}"),
                            desc: "Timer".into(),
                            action: format!("timer:pause:{id}"),
                            args: None,
                        },
                        query_override: Some("timer pause".into()),
                    });
                }
                if ui.small_button("Cancel").clicked() {
                    clicked = Some(WidgetAction {
                        action: Action {
                            label: format!("Cancel timer {id}"),
                            desc: "Timer".into(),
                            action: format!("timer:cancel:{id}"),
                            args: None,
                        },
                        query_override: Some("timer cancel".into()),
                    });
                }
            });
        }

        if self.cfg.show_completed_recently {
            if let Some(list) = timer::FINISHED_MESSAGES.lock().ok() {
                if !list.is_empty() {
                    ui.separator();
                    ui.label("Recently completed");
                    for msg in list.iter().rev().take(self.cfg.count) {
                        ui.label(egui::RichText::new(msg).small());
                    }
                }
            }
        }

        clicked
    }
}
