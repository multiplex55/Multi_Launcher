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
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut NotesOpenConfig, ctx| {
            let mut changed = false;
            let suggestions = super::query_suggestions(
                ctx,
                &["note"],
                &["note", "note search", "note list", "note open"],
            );
            if cfg.query.is_none() {
                if let Some(s) = suggestions.first() {
                    cfg.query = Some(s.clone());
                    changed = true;
                }
            }
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
            if !suggestions.is_empty() {
                egui::ComboBox::from_label("Suggestions")
                    .selected_text(
                        cfg.query
                            .as_deref()
                            .unwrap_or("Pick a note query from your plugins"),
                    )
                    .show_ui(ui, |ui| {
                        for suggestion in &suggestions {
                            changed |= ui
                                .selectable_value(
                                    &mut cfg.query,
                                    Some(suggestion.clone()),
                                    suggestion,
                                )
                                .changed();
                        }
                    });
            }
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
