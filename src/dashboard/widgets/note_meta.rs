use super::{Widget, WidgetAction};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
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
