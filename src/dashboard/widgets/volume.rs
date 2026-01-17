use super::{
    default_refresh_throttle_secs, edit_typed_settings, refresh_schedule, refresh_settings_ui,
    run_refresh_schedule, RefreshMode, TimedCache, Widget, WidgetAction, WidgetSettingsContext,
    WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::gui::volume_data::{get_process_volumes, get_system_volume, ProcessVolume};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::Duration;

fn default_refresh_interval() -> f32 {
    5.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeConfig {
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: f32,
    #[serde(default)]
    pub refresh_mode: RefreshMode,
    #[serde(default = "default_refresh_throttle_secs")]
    pub refresh_throttle_secs: f32,
    #[serde(default)]
    pub manual_refresh_only: bool,
}

impl Default for VolumeConfig {
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
struct VolumeSnapshot {
    system_volume: u8,
    processes: Vec<ProcessVolume>,
}

pub struct VolumeWidget {
    cfg: VolumeConfig,
    cache: TimedCache<VolumeSnapshot>,
    refresh_pending: bool,
}

impl VolumeWidget {
    pub fn new(cfg: VolumeConfig) -> Self {
        let interval = Duration::from_secs_f32(cfg.refresh_interval_secs.max(1.0));
        Self {
            cfg,
            cache: TimedCache::new(VolumeSnapshot::default(), interval),
            refresh_pending: true,
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut VolumeConfig, _ctx| {
            refresh_settings_ui(
                ui,
                &mut cfg.refresh_interval_secs,
                &mut cfg.refresh_mode,
                &mut cfg.refresh_throttle_secs,
                Some(&mut cfg.manual_refresh_only),
                "Volume data is cached. The widget will skip refreshing until this many seconds have passed. Use Refresh to update immediately.",
            )
        })
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_secs_f32(self.cfg.refresh_interval_secs.max(1.0))
    }

    fn update_interval(&mut self) {
        self.cache.set_interval(self.refresh_interval());
    }

    fn refresh(&mut self) {
        self.update_interval();
        let system_volume = get_system_volume().unwrap_or(50);
        let mut processes = get_process_volumes();
        processes.sort_by(|a, b| a.name.cmp(&b.name).then(a.pid.cmp(&b.pid)));
        self.cache.refresh(|data| {
            data.system_volume = system_volume;
            data.processes = processes;
        });
    }

    fn maybe_refresh(&mut self, ctx: &DashboardContext<'_>) {
        self.update_interval();
        let schedule = refresh_schedule(
            self.refresh_interval(),
            self.cfg.refresh_mode,
            self.cfg.manual_refresh_only,
            self.cfg.refresh_throttle_secs,
        );
        run_refresh_schedule(
            ctx,
            schedule,
            &mut self.refresh_pending,
            &mut self.cache.last_refresh,
            || self.refresh(),
        );
    }

    fn action(label: String, action: String) -> WidgetAction {
        WidgetAction {
            action: Action {
                label,
                desc: "Volume".into(),
                action,
                args: None,
            },
            query_override: None,
        }
    }
}

impl Default for VolumeWidget {
    fn default() -> Self {
        Self::new(VolumeConfig::default())
    }
}

impl Widget for VolumeWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        self.maybe_refresh(ctx);

        let mut clicked = None;

        ui.vertical(|ui| {
            ui.label("System volume");
            ui.horizontal(|ui| {
                let resp = ui.add(
                    egui::Slider::new(&mut self.cache.data.system_volume, 0..=100).text("Level"),
                );
                if resp.changed() {
                    let label = format!("Set system volume to {}%", self.cache.data.system_volume);
                    let action = format!("volume:set:{}", self.cache.data.system_volume);
                    clicked.get_or_insert_with(|| Self::action(label, action));
                }
                ui.label(format!("{}%", self.cache.data.system_volume));
                if ui.button("Mute active").clicked() {
                    clicked.get_or_insert_with(|| {
                        Self::action(
                            "Toggle mute for active window".into(),
                            "volume:mute_active".into(),
                        )
                    });
                }
            });
        });

        ui.separator();
        if self.cache.data.processes.is_empty() {
            ui.label("No audio sessions found.");
            return clicked;
        }

        for proc in &mut self.cache.data.processes {
            ui.horizontal(|ui| {
                ui.label(format!("{} (PID {})", proc.name, proc.pid));
                let resp = ui.add(egui::Slider::new(&mut proc.value, 0..=100).text("Level"));
                if resp.changed() {
                    let label = format!("Set PID {} volume to {}%", proc.pid, proc.value);
                    let action = format!("volume:pid:{}:{}", proc.pid, proc.value);
                    clicked.get_or_insert_with(|| Self::action(label, action));
                }
                ui.label(format!("{}%", proc.value));
                let mute_label = if proc.muted { "Unmute" } else { "Mute" };
                if ui.button(mute_label).clicked() {
                    let label = format!("Toggle mute for PID {}", proc.pid);
                    let action = format!("volume:pid_toggle_mute:{}", proc.pid);
                    clicked.get_or_insert_with(|| Self::action(label, action));
                    proc.muted = !proc.muted;
                }
                if proc.muted {
                    ui.colored_label(egui::Color32::RED, "muted");
                }
            });
        }

        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<VolumeConfig>(settings.clone()) {
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
                "Cached for {:.0}s. Refresh to update volume data immediately.",
                self.cfg.refresh_interval_secs
            ),
        };
        if ui.small_button("Refresh").on_hover_text(tooltip).clicked() {
            self.refresh_pending = true;
        }
        None
    }
}
