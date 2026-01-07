use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};

fn default_count() -> usize {
    6
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardRecentConfig {
    #[serde(default = "default_count")]
    pub count: usize,
}

impl Default for ClipboardRecentConfig {
    fn default() -> Self {
        Self {
            count: default_count(),
        }
    }
}

pub struct ClipboardRecentWidget {
    cfg: ClipboardRecentConfig,
}

impl ClipboardRecentWidget {
    pub fn new(cfg: ClipboardRecentConfig) -> Self {
        Self { cfg }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut ClipboardRecentConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Show");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.count).clamp_range(1..=50))
                    .changed();
                ui.label("clipboard items");
            });
            changed
        })
    }

    fn shorten(text: &str, len: usize) -> String {
        let trimmed = text.trim();
        if trimmed.len() > len {
            format!("{}…", &trimmed[..len])
        } else {
            trimmed.to_string()
        }
    }
}

impl Default for ClipboardRecentWidget {
    fn default() -> Self {
        Self::new(ClipboardRecentConfig::default())
    }
}

impl Widget for ClipboardRecentWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let snapshot = ctx.data_cache.snapshot();
        let history = snapshot.clipboard_history.as_ref();
        if history.is_empty() {
            ui.label("No clipboard history.");
            return None;
        }

        let mut clicked = None;
        let rows = history.len().min(self.cfg.count);
        let row_height =
            ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y + 6.0;
        let scroll_id = ui.id().with("clipboard_recent_scroll");
        egui::ScrollArea::vertical()
            .id_source(scroll_id)
            .auto_shrink([false; 2])
            .show_rows(ui, row_height, rows, |ui, range| {
                for idx in range {
                    let entry = &history[idx];
                    if ui
                        .button(Self::shorten(entry, 60))
                        .on_hover_text(entry)
                        .clicked()
                    {
                        clicked = Some(WidgetAction {
                            action: Action {
                                label: "Copy from clipboard history".into(),
                                desc: "Clipboard".into(),
                                action: format!("clipboard:copy:{idx}"),
                                args: None,
                            },
                            query_override: Some("cb list".into()),
                        });
                    }
                }
            });

        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<ClipboardRecentConfig>(settings.clone()) {
            self.cfg = cfg;
        }
    }
}
