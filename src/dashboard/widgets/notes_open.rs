use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotesOpenConfig {
    pub query: Option<String>,
}

pub struct NotesOpenWidget {
    cfg: NotesOpenConfig,
}

impl NotesOpenWidget {
    pub fn new(cfg: NotesOpenConfig) -> Self {
        Self { cfg }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut NotesOpenConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Query override");
                let mut query = cfg.query.clone().unwrap_or_default();
                if ui.text_edit_singleline(&mut query).changed() {
                    cfg.query = if query.trim().is_empty() {
                        None
                    } else {
                        Some(query)
                    };
                    changed = true;
                }
            });
            changed
        })
    }
}

impl Default for NotesOpenWidget {
    fn default() -> Self {
        Self {
            cfg: NotesOpenConfig::default(),
        }
    }
}

impl Widget for NotesOpenWidget {
    fn render(
        &mut self,
        ui: &mut eframe::egui::Ui,
        _ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let label = "Open Notes";
        if ui.button(label).clicked() {
            return Some(WidgetAction {
                action: Action {
                    label: label.into(),
                    desc: "Note".into(),
                    action: "note:dialog".into(),
                    args: None,
                },
                query_override: self.cfg.query.clone().or_else(|| Some("note".into())),
            });
        }
        None
    }
}
