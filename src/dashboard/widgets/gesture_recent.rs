use super::{
    edit_typed_settings, gesture_focus_action, Widget, WidgetAction, WidgetSettingsContext,
    WidgetSettingsUiResult,
};
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::mouse_gestures::db::format_tokens;
use eframe::egui;
use serde::{Deserialize, Serialize};

fn default_count() -> usize {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GestureRecentConfig {
    #[serde(default = "default_count")]
    pub count: usize,
}

impl Default for GestureRecentConfig {
    fn default() -> Self {
        Self {
            count: default_count(),
        }
    }
}

pub struct GestureRecentWidget {
    cfg: GestureRecentConfig,
}

impl GestureRecentWidget {
    pub fn new(cfg: GestureRecentConfig) -> Self {
        Self { cfg }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut GestureRecentConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Count");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.count).clamp_range(1..=50))
                    .changed();
            });
            changed
        })
    }

    fn binding_label(
        snapshot: &crate::dashboard::data_cache::DashboardDataSnapshot,
        entry: &crate::mouse_gestures::usage::GestureUsageEntry,
    ) -> String {
        let gesture = snapshot.gestures.db.gestures.iter().find(|gesture| {
            gesture.label == entry.gesture_label
                && gesture.tokens == entry.tokens
                && gesture.dir_mode == entry.dir_mode
        });
        let Some(gesture) = gesture else {
            return "Unknown".into();
        };
        let mut enabled_bindings = gesture.bindings.iter().filter(|binding| binding.enabled);
        enabled_bindings
            .nth(entry.binding_idx)
            .map(|binding| binding.label.clone())
            .unwrap_or_else(|| "Unknown".into())
    }
}

impl Default for GestureRecentWidget {
    fn default() -> Self {
        Self::new(GestureRecentConfig::default())
    }
}

impl Widget for GestureRecentWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let snapshot = ctx.data_cache.snapshot();
        let usage = &snapshot.gestures.usage;
        if usage.is_empty() {
            ui.label("No recent gestures.");
            return None;
        }

        let mut clicked = None;
        let count = self.cfg.count.max(1);
        egui::Grid::new("gesture_recent")
            .striped(true)
            .show(ui, |ui| {
                ui.label("Gesture");
                ui.label("Tokens");
                ui.label("Binding");
                ui.label("Action");
                ui.end_row();

                for entry in usage.iter().rev().take(count) {
                    let binding_label = Self::binding_label(&snapshot, entry);
                    if ui
                        .selectable_label(false, entry.gesture_label.clone())
                        .clicked()
                    {
                        clicked = Some(gesture_focus_action(
                            &entry.gesture_label,
                            &entry.tokens,
                            entry.dir_mode,
                            Some(entry.binding_idx),
                        ));
                    }
                    ui.label(format_tokens(&entry.tokens));
                    ui.label(format!("#{}", entry.binding_idx + 1));
                    ui.label(binding_label);
                    ui.end_row();
                }
            });

        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<GestureRecentConfig>(settings.clone()) {
            self.cfg = cfg;
        }
    }
}
