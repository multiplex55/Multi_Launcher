use super::{Widget, WidgetAction};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
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
