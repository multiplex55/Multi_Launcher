use super::{
    Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult, edit_typed_settings,
    jump_response_clicked,
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

#[derive(Default)]
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
                let response = ui.add(egui::DragValue::new(&mut cfg.count).range(1..=20));
                ctx.register_jump_mode_target(
                    ui,
                    &response,
                    "count",
                    "Timer count",
                    crate::gui::jump_mode::TargetKind::Slider,
                );
                changed |= response.changed();
                ui.label("timers");
            });
            let response = ui.checkbox(&mut cfg.show_completed_recently, "Show recent completions");
            ctx.register_jump_mode_target(
                ui,
                &response,
                "show-completed-recently",
                "Show recent completions",
                crate::gui::jump_mode::TargetKind::Checkbox,
            );
            changed |= response.changed();
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

    fn new_timer_action() -> WidgetAction {
        WidgetAction {
            action: Action {
                label: "New Timer".into(),
                desc: "Timer".into(),
                action: "timer:dialog:timer".into(),
                args: None,
            },
            query_override: None,
        }
    }
}

impl Widget for ActiveTimersWidget {
    fn header_ui(&mut self, ui: &mut egui::Ui, ctx: &DashboardContext<'_>) -> Option<WidgetAction> {
        let response = ui.small_button("New Timer");
        jump_response_clicked(
            ui,
            ctx,
            &response,
            || "timer:new".into(),
            || "New Timer".into(),
            0,
        )
        .then(Self::new_timer_action)
    }

    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let mut timers = timer::running_timers();
        timers.sort_by_key(|t| t.2);
        let mut clicked = None;

        if timers.is_empty() {
            ui.label("No running timers");
        }

        for (order, (id, label, remaining, start_ts)) in
            timers.into_iter().take(self.cfg.count).enumerate()
        {
            ui.push_id(("timer", id), |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{} – {} left",
                        label,
                        Self::format_duration(remaining)
                    ));
                    ui.label(egui::RichText::new(timer::format_ts(start_ts)).small());
                    let pause_response = ui.small_button("Pause");
                    if jump_response_clicked(
                        ui,
                        ctx,
                        &pause_response,
                        || format!("timer:{id}:pause"),
                        || "Pause".into(),
                        (order * 2) as u64,
                    ) {
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
                    let cancel_response = ui.small_button("Cancel");
                    if jump_response_clicked(
                        ui,
                        ctx,
                        &cancel_response,
                        || format!("timer:{id}:cancel"),
                        || "Cancel".into(),
                        (order * 2 + 1) as u64,
                    ) {
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
            });
        }

        if self.cfg.show_completed_recently
            && let Ok(list) = timer::FINISHED_MESSAGES.lock()
            && !list.is_empty()
        {
            ui.separator();
            ui.label("Recently completed");
            for msg in list.iter().rev().take(self.cfg.count) {
                ui.label(egui::RichText::new(msg).small());
            }
        }

        clicked
    }
}

#[cfg(test)]
mod tests {
    use super::ActiveTimersWidget;

    #[test]
    fn new_timer_action_uses_existing_dialog_route_without_overrides() {
        let action = ActiveTimersWidget::new_timer_action();

        assert_eq!(action.action.label, "New Timer");
        assert_eq!(action.action.desc, "Timer");
        assert_eq!(action.action.action, "timer:dialog:timer");
        assert_eq!(action.action.args, None);
        assert_eq!(action.query_override, None);
    }
}
