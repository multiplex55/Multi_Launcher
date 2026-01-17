use super::{
    default_refresh_throttle_secs, edit_typed_settings, refresh_schedule, refresh_settings_ui,
    run_refresh_schedule, RefreshMode, Widget, WidgetAction, WidgetSettingsContext,
    WidgetSettingsUiResult,
};
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::Duration;

fn default_refresh_interval() -> f32 {
    5.0
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatusConfig {
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: f32,
    #[serde(default)]
    pub refresh_mode: RefreshMode,
    #[serde(default = "default_refresh_throttle_secs")]
    pub refresh_throttle_secs: f32,
    #[serde(default)]
    pub manual_refresh_only: bool,
    #[serde(default = "default_true")]
    pub show_cpu: bool,
    #[serde(default = "default_true")]
    pub show_memory: bool,
    #[serde(default = "default_true")]
    pub show_disk: bool,
    #[serde(default = "default_true")]
    pub show_network: bool,
    #[serde(default = "default_true")]
    pub show_volume: bool,
    #[serde(default = "default_true")]
    pub show_brightness: bool,
}

impl Default for SystemStatusConfig {
    fn default() -> Self {
        Self {
            refresh_interval_secs: default_refresh_interval(),
            refresh_mode: RefreshMode::Auto,
            refresh_throttle_secs: default_refresh_throttle_secs(),
            manual_refresh_only: false,
            show_cpu: true,
            show_memory: true,
            show_disk: true,
            show_network: true,
            show_volume: true,
            show_brightness: true,
        }
    }
}

pub struct SystemStatusWidget {
    cfg: SystemStatusConfig,
    refresh_pending: bool,
    last_refresh: std::time::Instant,
}

impl SystemStatusWidget {
    pub fn new(cfg: SystemStatusConfig) -> Self {
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
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut SystemStatusConfig, _ctx| {
            let mut changed = false;
            changed |= refresh_settings_ui(
                ui,
                &mut cfg.refresh_interval_secs,
                &mut cfg.refresh_mode,
                &mut cfg.refresh_throttle_secs,
                Some(&mut cfg.manual_refresh_only),
                "System stats are cached between refreshes.",
            );
            ui.separator();
            ui.label("Show");
            changed |= ui.checkbox(&mut cfg.show_cpu, "CPU usage").changed();
            changed |= ui.checkbox(&mut cfg.show_memory, "Memory usage").changed();
            changed |= ui.checkbox(&mut cfg.show_disk, "Disk usage").changed();
            changed |= ui
                .checkbox(&mut cfg.show_network, "Network throughput")
                .changed();
            changed |= ui.checkbox(&mut cfg.show_volume, "Volume level").changed();
            changed |= ui
                .checkbox(&mut cfg.show_brightness, "Brightness level")
                .changed();
            changed
        })
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_secs_f32(self.cfg.refresh_interval_secs.max(1.0))
    }

    fn fmt_speed(bytes_per_sec: f64) -> String {
        const KB: f64 = 1024.0;
        const MB: f64 = 1024.0 * 1024.0;
        if bytes_per_sec >= MB {
            format!("{:.2} MB/s", bytes_per_sec / MB)
        } else if bytes_per_sec >= KB {
            format!("{:.1} kB/s", bytes_per_sec / KB)
        } else {
            format!("{:.0} B/s", bytes_per_sec)
        }
    }
}

impl Default for SystemStatusWidget {
    fn default() -> Self {
        Self::new(SystemStatusConfig::default())
    }
}

impl Widget for SystemStatusWidget {
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
            ctx.data_cache.request_refresh_system_status();
            self.last_refresh = std::time::Instant::now();
        }
        let snapshot = ctx.data_cache.snapshot();
        let Some(status) = snapshot.system_status.as_ref() else {
            ui.label("System data unavailable.");
            return None;
        };

        if self.cfg.show_cpu {
            ui.label(format!("CPU: {:.0}%", status.cpu_percent));
        }
        if self.cfg.show_memory {
            ui.label(format!("Mem: {:.0}%", status.mem_percent));
        }
        if self.cfg.show_disk {
            ui.label(format!("Disk: {:.0}%", status.disk_percent));
        }
        if self.cfg.show_network {
            ui.label(format!(
                "Net: ↓ {}  ↑ {}",
                Self::fmt_speed(status.net_rx_per_sec),
                Self::fmt_speed(status.net_tx_per_sec)
            ));
        }
        if self.cfg.show_volume {
            if let Some(volume) = status.volume_percent {
                ui.label(format!("Volume: {volume}%"));
            } else {
                ui.label("Volume: unavailable");
            }
        }
        if self.cfg.show_brightness {
            if let Some(brightness) = status.brightness_percent {
                ui.label(format!("Brightness: {brightness}%"));
            } else {
                ui.label("Brightness: unavailable");
            }
        }
        None
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<SystemStatusConfig>(settings.clone()) {
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
                "Cached for {:.0}s. Refresh to update immediately.",
                self.cfg.refresh_interval_secs
            ),
        };
        if ui.small_button("Refresh").on_hover_text(tooltip).clicked() {
            self.refresh_pending = true;
        }
        None
    }
}
