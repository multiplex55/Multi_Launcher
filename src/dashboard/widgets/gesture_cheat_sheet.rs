use super::{
    edit_typed_settings, gesture_focus_action, gesture_toggle_action, Widget, WidgetAction,
    WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::mouse_gestures::db::{format_tokens, BindingEntry, GestureEntry};
use crate::mouse_gestures::engine::DirMode;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn default_count() -> usize {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GestureCheatSheetConfig {
    #[serde(default = "default_count")]
    pub count: usize,
    #[serde(default)]
    pub show_disabled: bool,
}

impl Default for GestureCheatSheetConfig {
    fn default() -> Self {
        Self {
            count: default_count(),
            show_disabled: false,
        }
    }
}

pub struct GestureCheatSheetWidget {
    cfg: GestureCheatSheetConfig,
}

impl GestureCheatSheetWidget {
    pub fn new(cfg: GestureCheatSheetConfig) -> Self {
        Self { cfg }
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
            |ui, cfg: &mut GestureCheatSheetConfig, _ctx| {
                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label("Count");
                    changed |= ui
                        .add(egui::DragValue::new(&mut cfg.count).clamp_range(1..=50))
                        .changed();
                });
                changed |= ui.checkbox(&mut cfg.show_disabled, "Show disabled").changed();
                changed
            },
        )
    }

    fn primary_binding_label(bindings: &[BindingEntry]) -> String {
        bindings
            .iter()
            .find(|binding| binding.enabled)
            .or_else(|| bindings.first())
            .map(|binding| binding.label.clone())
            .unwrap_or_else(|| "Unbound".into())
    }

    fn usage_counts(
        usage: &[crate::mouse_gestures::usage::GestureUsageEntry],
    ) -> HashMap<(String, String, DirMode), usize> {
        let mut counts: HashMap<(String, String, DirMode), usize> = HashMap::new();
        for entry in usage {
            *counts
                .entry((
                    entry.gesture_label.clone(),
                    entry.tokens.clone(),
                    entry.dir_mode,
                ))
                .or_insert(0) += 1;
        }
        counts
    }
}

impl Default for GestureCheatSheetWidget {
    fn default() -> Self {
        Self::new(GestureCheatSheetConfig::default())
    }
}

impl Widget for GestureCheatSheetWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let snapshot = ctx.data_cache.snapshot();
        let gestures = &snapshot.gestures.db.gestures;
        let usage = &snapshot.gestures.usage;
        let counts = Self::usage_counts(usage);
        let mut rows: Vec<(GestureEntry, usize)> = gestures
            .iter()
            .filter(|gesture| !gesture.bindings.is_empty())
            .cloned()
            .map(|gesture| {
                let count = counts
                    .get(&(gesture.label.clone(), gesture.tokens.clone(), gesture.dir_mode))
                    .copied()
                    .unwrap_or(0);
                (gesture, count)
            })
            .collect();

        if !self.cfg.show_disabled {
            rows.retain(|(gesture, _)| gesture.enabled);
        }

        rows.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| a.0.label.cmp(&b.0.label))
        });

        let mut clicked = None;
        let count = self.cfg.count.max(1);
        if rows.is_empty() {
            ui.label("No bound gestures configured.");
            return None;
        }

        egui::Grid::new("gesture_cheat_sheet")
            .striped(true)
            .show(ui, |ui| {
                ui.label("On");
                ui.label("Gesture");
                ui.label("Tokens");
                ui.label("Primary binding");
                ui.end_row();

                for (gesture, _) in rows.into_iter().take(count) {
                    let mut enabled = gesture.enabled;
                    if ui.checkbox(&mut enabled, "").changed() {
                        clicked = Some(gesture_toggle_action(
                            &gesture.label,
                            &gesture.tokens,
                            gesture.dir_mode,
                            enabled,
                        ));
                    }
                    if ui
                        .selectable_label(false, gesture.label.clone())
                        .clicked()
                    {
                        clicked = Some(gesture_focus_action(
                            &gesture.label,
                            &gesture.tokens,
                            gesture.dir_mode,
                            None,
                        ));
                    }
                    ui.label(format_tokens(&gesture.tokens));
                    ui.label(Self::primary_binding_label(&gesture.bindings));
                    ui.end_row();
                }
            });

        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<GestureCheatSheetConfig>(settings.clone()) {
            self.cfg = cfg;
        }
    }
}
