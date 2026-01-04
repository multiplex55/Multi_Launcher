use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NoteMetaConfig {
    pub label: Option<String>,
}

pub struct NoteMetaWidget {
    cfg: NoteMetaConfig,
}

impl NoteMetaWidget {
    pub fn new(cfg: NoteMetaConfig) -> Self {
        Self { cfg }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut NoteMetaConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Label");
                let mut label = cfg.label.clone().unwrap_or_else(|| "Recent Note".into());
                if ui.text_edit_singleline(&mut label).changed() {
                    cfg.label = if label.trim().is_empty() {
                        None
                    } else {
                        Some(label)
                    };
                    changed = true;
                }
            });
            changed
        })
    }
}

impl Default for NoteMetaWidget {
    fn default() -> Self {
        Self {
            cfg: NoteMetaConfig::default(),
        }
    }
}

impl Widget for NoteMetaWidget {
    fn render(
        &mut self,
        ui: &mut eframe::egui::Ui,
        _ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let label = self
            .cfg
            .label
            .clone()
            .unwrap_or_else(|| "Recent Note".into());
        if ui.button(&label).clicked() {
            return Some(WidgetAction {
                action: Action {
                    label: label.clone(),
                    desc: "Note".into(),
                    action: "note:dialog".into(),
                    args: None,
                },
                query_override: Some("note list".into()),
            });
        }
        None
    }
}
