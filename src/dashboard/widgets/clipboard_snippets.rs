use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};

fn default_clipboard_count() -> usize {
    5
}

fn default_snippet_count() -> usize {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardSnippetsConfig {
    #[serde(default = "default_clipboard_count")]
    pub clipboard_count: usize,
    #[serde(default = "default_snippet_count")]
    pub snippet_count: usize,
    #[serde(default)]
    pub show_system: bool,
}

impl Default for ClipboardSnippetsConfig {
    fn default() -> Self {
        Self {
            clipboard_count: default_clipboard_count(),
            snippet_count: default_snippet_count(),
            show_system: true,
        }
    }
}

pub struct ClipboardSnippetsWidget {
    cfg: ClipboardSnippetsConfig,
}

impl ClipboardSnippetsWidget {
    pub fn new(cfg: ClipboardSnippetsConfig) -> Self {
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
            |ui, cfg: &mut ClipboardSnippetsConfig, _ctx| {
                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label("Clipboard items");
                    changed |= ui
                        .add(egui::DragValue::new(&mut cfg.clipboard_count).clamp_range(1..=50))
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label("Snippets");
                    changed |= ui
                        .add(egui::DragValue::new(&mut cfg.snippet_count).clamp_range(0..=50))
                        .changed();
                });
                changed |= ui
                    .checkbox(&mut cfg.show_system, "Show system snapshot")
                    .changed();
                changed
            },
        )
    }

    fn shorten(text: &str, len: usize) -> String {
        let trimmed = text.trim();
        if trimmed.len() > len {
            format!("{}…", &trimmed[..len])
        } else {
            trimmed.to_string()
        }
    }

    fn render_system_snapshot(ui: &mut egui::Ui, ctx: &DashboardContext<'_>) {
        ctx.data_cache.request_refresh_system_status();
        let snapshot = ctx.data_cache.snapshot();
        let Some(status) = snapshot.system_status.as_ref() else {
            ui.label("System data unavailable.");
            return;
        };
        ui.label(format!("CPU: {:.0}%", status.cpu_percent));
        ui.label(format!("Mem: {:.0}%", status.mem_percent));
        ui.label(format!("Disk: {:.0}%", status.disk_percent));
    }
}

impl Default for ClipboardSnippetsWidget {
    fn default() -> Self {
        Self {
            cfg: ClipboardSnippetsConfig::default(),
        }
    }
}

impl Widget for ClipboardSnippetsWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let snapshot = ctx.data_cache.snapshot();
        let history = snapshot.clipboard_history.as_ref();
        let snippets = snapshot.snippets.as_ref();
        let mut clicked = None;
        if !history.is_empty() {
            ui.label("Clipboard");
            let rows = history.len().min(self.cfg.clipboard_count);
            let row_height =
                ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y + 6.0;
            let scroll_id = ui.id().with("clipboard_snippets_scroll");
            egui::ScrollArea::both()
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
                                    preview_text: None,
                                    risk_level: None,
                                    icon: None,
                                },
                                query_override: Some("cb list".into()),
                            });
                        }
                    }
                });
        }

        if self.cfg.snippet_count > 0 && !snippets.is_empty() {
            ui.separator();
            ui.label("Snippets");
            for snippet in snippets.iter().take(self.cfg.snippet_count) {
                if ui
                    .button(format!(
                        "{} — {}",
                        snippet.alias,
                        Self::shorten(&snippet.text, 40)
                    ))
                    .on_hover_text(&snippet.text)
                    .clicked()
                {
                    clicked = Some(WidgetAction {
                        action: Action {
                            label: snippet.alias.clone(),
                            desc: "Snippet".into(),
                            action: format!("clipboard:{}", snippet.text),
                            args: None,
                            preview_text: None,
                            risk_level: None,
                            icon: None,
                        },
                        query_override: Some(format!("cs {}", snippet.alias)),
                    });
                }
            }
        }

        if self.cfg.show_system {
            ui.separator();
            ui.label("System snapshot");
            Self::render_system_snapshot(ui, ctx);
        }

        clicked
    }
}
