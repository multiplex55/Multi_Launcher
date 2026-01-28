use super::{
    default_refresh_throttle_secs, edit_typed_settings, refresh_schedule, refresh_settings_ui,
    run_refresh_schedule, RefreshMode, TimedCache, Widget, WidgetAction, WidgetSettingsContext,
    WidgetSettingsUiResult,
};
use crate::actions::system::{
    get_main_display_brightness, get_power_plans, get_system_mute, get_system_volume, PowerPlan,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::Duration;

fn default_refresh_interval() -> f32 {
    5.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemControlsConfig {
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: f32,
    #[serde(default)]
    pub refresh_mode: RefreshMode,
    #[serde(default = "default_refresh_throttle_secs")]
    pub refresh_throttle_secs: f32,
    #[serde(default)]
    pub manual_refresh_only: bool,
}

impl Default for SystemControlsConfig {
    fn default() -> Self {
        Self {
            refresh_interval_secs: default_refresh_interval(),
            refresh_mode: RefreshMode::Auto,
            refresh_throttle_secs: default_refresh_throttle_secs(),
            manual_refresh_only: false,
        }
    }
}

#[derive(Clone, Default)]
struct SystemControlsSnapshot {
    brightness_percent: Option<u8>,
    brightness_error: Option<String>,
    volume_percent: Option<u8>,
    volume_error: Option<String>,
    muted: Option<bool>,
    mute_error: Option<String>,
    power_plans: Vec<PowerPlan>,
    power_plan_error: Option<String>,
    active_power_plan: Option<String>,
}

pub struct SystemControlsWidget {
    cfg: SystemControlsConfig,
    cache: TimedCache<SystemControlsSnapshot>,
    refresh_pending: bool,
}

impl SystemControlsWidget {
    pub fn new(cfg: SystemControlsConfig) -> Self {
        let interval = Duration::from_secs_f32(cfg.refresh_interval_secs.max(1.0));
        Self {
            cfg,
            cache: TimedCache::new(SystemControlsSnapshot::default(), interval),
            refresh_pending: true,
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(
            ui,
            value,
            ctx,
            |ui, cfg: &mut SystemControlsConfig, _ctx| {
                refresh_settings_ui(
                ui,
                &mut cfg.refresh_interval_secs,
                &mut cfg.refresh_mode,
                &mut cfg.refresh_throttle_secs,
                Some(&mut cfg.manual_refresh_only),
                "System control data is cached. The widget will skip refreshing until this many seconds have passed. Use Refresh to update immediately.",
            )
            },
        )
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_secs_f32(self.cfg.refresh_interval_secs.max(1.0))
    }

    fn update_interval(&mut self) {
        self.cache.set_interval(self.refresh_interval());
    }

    fn refresh(&mut self) {
        self.update_interval();
        let mut snapshot = SystemControlsSnapshot::default();

        if cfg!(target_os = "windows") {
            snapshot.brightness_percent = get_main_display_brightness();
            if snapshot.brightness_percent.is_none() {
                snapshot.brightness_error = Some("Brightness unavailable.".into());
            }
        } else {
            snapshot.brightness_error =
                Some("Brightness controls are not supported on this OS.".into());
        }

        if cfg!(target_os = "windows") {
            snapshot.volume_percent = get_system_volume();
            snapshot.muted = get_system_mute();
            if snapshot.volume_percent.is_none() {
                snapshot.volume_error = Some("System volume unavailable.".into());
            }
            if snapshot.muted.is_none() {
                snapshot.mute_error = Some("Mute status unavailable.".into());
            }
        } else {
            snapshot.volume_error = Some("Volume controls are not supported on this OS.".into());
            snapshot.mute_error = Some("Mute controls are not supported on this OS.".into());
        }

        if cfg!(target_os = "windows") {
            match get_power_plans() {
                Ok(plans) => {
                    snapshot.active_power_plan =
                        plans.iter().find(|p| p.active).map(|p| p.guid.clone());
                    if snapshot.active_power_plan.is_none() {
                        snapshot.active_power_plan = plans.first().map(|p| p.guid.clone());
                    }
                    snapshot.power_plans = plans;
                }
                Err(err) => snapshot.power_plan_error = Some(err),
            }
        } else {
            snapshot.power_plan_error = Some("Power plans are not supported on this OS.".into());
        }

        self.cache.refresh(|data| *data = snapshot);
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
            self.refresh();
        }
    }

    fn action(label: String, action: String) -> WidgetAction {
        WidgetAction {
            action: Action {
                label,
                desc: "System controls".into(),
                action,
                args: None,
            },
            query_override: None,
        }
    }

    fn power_plan_label<'a>(plans: &'a [PowerPlan], guid: &str) -> &'a str {
        plans
            .iter()
            .find(|plan| plan.guid == guid)
            .map(|plan| plan.name.as_str())
            .unwrap_or("Select power plan")
    }
}

impl Default for SystemControlsWidget {
    fn default() -> Self {
        Self::new(SystemControlsConfig::default())
    }
}

impl Widget for SystemControlsWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        self.maybe_refresh(ctx);
        let mut clicked = None;

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label("Brightness");
                if let Some(level) = self.cache.data.brightness_percent.as_mut() {
                    let resp = ui.add(egui::Slider::new(level, 0..=100).text("Level"));
                    if resp.changed() {
                        let label = format!("Set brightness to {level}%");
                        let action = format!("brightness:set:{level}");
                        clicked.get_or_insert_with(|| Self::action(label, action));
                    }
                    ui.label(format!("{level}%"));
                } else if let Some(err) = &self.cache.data.brightness_error {
                    ui.colored_label(egui::Color32::YELLOW, err);
                }
            });

            ui.horizontal(|ui| {
                ui.label("Volume");
                if let Some(level) = self.cache.data.volume_percent.as_mut() {
                    let resp = ui.add(egui::Slider::new(level, 0..=100).text("Level"));
                    if resp.changed() {
                        let label = format!("Set volume to {level}%");
                        let action = format!("volume:set:{level}");
                        clicked.get_or_insert_with(|| Self::action(label, action));
                    }
                    ui.label(format!("{level}%"));
                } else if let Some(err) = &self.cache.data.volume_error {
                    ui.colored_label(egui::Color32::YELLOW, err);
                }

                if let Some(muted) = self.cache.data.muted.as_mut() {
                    if ui.checkbox(muted, "Mute").changed() {
                        clicked.get_or_insert_with(|| {
                            Self::action("Toggle system mute".into(), "volume:toggle_mute".into())
                        });
                    }
                } else if let Some(err) = &self.cache.data.mute_error {
                    ui.colored_label(egui::Color32::YELLOW, err);
                }
            });

            ui.horizontal(|ui| {
                ui.label("Power plan");
                if self.cache.data.power_plans.is_empty() {
                    if let Some(err) = &self.cache.data.power_plan_error {
                        ui.colored_label(egui::Color32::YELLOW, err);
                    } else {
                        ui.colored_label(egui::Color32::YELLOW, "No power plans available.");
                    }
                } else {
                    let mut selection = self
                        .cache
                        .data
                        .active_power_plan
                        .clone()
                        .unwrap_or_else(|| self.cache.data.power_plans[0].guid.clone());
                    let mut selection_changed = None;
                    let selected_label =
                        Self::power_plan_label(&self.cache.data.power_plans, &selection);
                    egui::ComboBox::from_id_source("system_controls_power_plan")
                        .selected_text(selected_label)
                        .show_ui(ui, |ui| {
                            for plan in &self.cache.data.power_plans {
                                if ui
                                    .selectable_value(
                                        &mut selection,
                                        plan.guid.clone(),
                                        plan.name.clone(),
                                    )
                                    .changed()
                                {
                                    selection_changed =
                                        Some((plan.name.clone(), plan.guid.clone()));
                                }
                            }
                        });
                    if let Some((name, guid)) = selection_changed {
                        let label = format!("Set power plan to {}", name);
                        let action = format!("power:plan:set:{}", guid);
                        clicked.get_or_insert_with(|| Self::action(label, action));
                        self.cache.data.active_power_plan = Some(selection.clone());
                        for plan in &mut self.cache.data.power_plans {
                            plan.active = plan.guid == selection;
                        }
                    }
                }
            });
        });

        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<SystemControlsConfig>(settings.clone()) {
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
                "Cached for {:.0}s. Refresh to update system controls immediately.",
                self.cfg.refresh_interval_secs
            ),
        };
        if ui.small_button("Refresh").on_hover_text(tooltip).clicked() {
            self.refresh_pending = true;
        }
        None
    }
}
