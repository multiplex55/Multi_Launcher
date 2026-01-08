use super::{
    edit_typed_settings, refresh_interval_setting, Widget, WidgetAction, WidgetSettingsContext,
    WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::Duration;

fn default_refresh_interval() -> f32 {
    60.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecycleBinConfig {
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: f32,
    #[serde(default)]
    pub manual_refresh_only: bool,
}

impl Default for RecycleBinConfig {
    fn default() -> Self {
        Self {
            refresh_interval_secs: default_refresh_interval(),
            manual_refresh_only: false,
        }
    }
}

pub struct RecycleBinWidget {
    cfg: RecycleBinConfig,
    refresh_pending: bool,
}

impl RecycleBinWidget {
    pub fn new(cfg: RecycleBinConfig) -> Self {
        Self {
            cfg,
            refresh_pending: false,
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut RecycleBinConfig, _ctx| {
            refresh_interval_setting(
                ui,
                &mut cfg.refresh_interval_secs,
                &mut cfg.manual_refresh_only,
                "Recycle bin data is cached between refreshes.",
            )
        })
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_secs_f32(self.cfg.refresh_interval_secs.max(1.0))
    }

    fn fmt_size(bytes: u64) -> String {
        const KB: f64 = 1024.0;
        const MB: f64 = KB * 1024.0;
        const GB: f64 = MB * 1024.0;
        const TB: f64 = GB * 1024.0;
        let bytes_f = bytes as f64;
        if bytes_f >= TB {
            format!("{:.2} TB", bytes_f / TB)
        } else if bytes_f >= GB {
            format!("{:.2} GB", bytes_f / GB)
        } else if bytes_f >= MB {
            format!("{:.1} MB", bytes_f / MB)
        } else if bytes_f >= KB {
            format!("{:.1} KB", bytes_f / KB)
        } else {
            format!("{} B", bytes)
        }
    }

    fn clean_action() -> Action {
        Action {
            label: "Clean Recycle Bin".into(),
            desc: "Recycle Bin".into(),
            action: "recycle:clean".into(),
            args: None,
        }
    }
}

impl Default for RecycleBinWidget {
    fn default() -> Self {
        Self::new(RecycleBinConfig::default())
    }
}

impl Widget for RecycleBinWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        if self.refresh_pending {
            ctx.data_cache.refresh_recycle_bin();
            self.refresh_pending = false;
        } else if !self.cfg.manual_refresh_only {
            ctx.data_cache
                .maybe_refresh_recycle_bin(self.refresh_interval());
        }

        let snapshot = ctx.data_cache.snapshot();
        let Some(info) = snapshot.recycle_bin.as_ref() else {
            ui.label("Recycle bin data unavailable.");
            return None;
        };

        ui.label(format!("Size: {}", Self::fmt_size(info.size_bytes)));
        ui.label(format!("Items: {}", info.items));

        if ui.button("Clean").clicked() {
            let action = Self::clean_action();
            return Some(WidgetAction {
                query_override: Some(action.label.clone()),
                action,
            });
        }

        None
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<RecycleBinConfig>(settings.clone()) {
            self.cfg = cfg;
            self.refresh_pending = true;
        }
    }

    fn header_ui(&mut self, ui: &mut egui::Ui, ctx: &DashboardContext<'_>) -> Option<WidgetAction> {
        let tooltip = if self.cfg.manual_refresh_only {
            "Manual refresh only.".to_string()
        } else {
            format!(
                "Cached for {:.0}s. Refresh to update recycle bin stats immediately.",
                self.cfg.refresh_interval_secs
            )
        };
        if ui.small_button("Refresh").on_hover_text(tooltip).clicked() {
            ctx.data_cache.refresh_recycle_bin();
        }
        None
    }
}
