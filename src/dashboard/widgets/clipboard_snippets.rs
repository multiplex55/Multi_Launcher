use super::{
    Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult, edit_typed_settings,
};
use crate::actions::Action;
use crate::dashboard::DashboardRefreshRequest;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::snippets::{SnippetEntry, snippet_preview_text, snippet_run_action};
use eframe::egui;
use serde::{Deserialize, Serialize};

const HIDDEN_SNIPPET_TOOLTIP: &str = "Contents are hidden in previews.";

fn shorten_snippet_preview(preview: &str, max_chars: usize) -> String {
    let mut characters = preview.chars();
    let shortened: String = characters.by_ref().take(max_chars).collect();
    if characters.next().is_some() {
        format!("{shortened}…")
    } else {
        shortened
    }
}

fn snippet_button_label(snippet: &SnippetEntry) -> String {
    let preview = shorten_snippet_preview(&snippet_preview_text(snippet), 40);
    format!("{}: {preview}", snippet.alias)
}

fn snippet_button_tooltip(snippet: &SnippetEntry) -> &str {
    if snippet.hide_contents {
        HIDDEN_SNIPPET_TOOLTIP
    } else {
        &snippet.text
    }
}

fn snippet_widget_action(snippet: &SnippetEntry) -> WidgetAction {
    WidgetAction {
        action: Action {
            label: snippet.alias.clone(),
            desc: "Snippet".into(),
            action: snippet_run_action(&snippet.alias),
            args: None,
        },
        query_override: Some(format!("cs {}", snippet.alias)),
    }
}

fn clipboard_history_widget_action(index: usize) -> WidgetAction {
    WidgetAction {
        action: Action {
            label: "Copy from clipboard history".into(),
            desc: "Clipboard".into(),
            action: format!("clipboard:copy:{index}"),
            args: None,
        },
        query_override: Some("cb list".into()),
    }
}

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
    system_refresh_requested: bool,
}

impl Default for ClipboardSnippetsWidget {
    fn default() -> Self {
        Self::new(ClipboardSnippetsConfig::default())
    }
}

impl ClipboardSnippetsWidget {
    pub fn new(cfg: ClipboardSnippetsConfig) -> Self {
        Self {
            cfg,
            system_refresh_requested: false,
        }
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
                            clicked = Some(clipboard_history_widget_action(idx));
                        }
                    }
                });
        }

        if self.cfg.snippet_count > 0 && !snippets.is_empty() {
            ui.separator();
            ui.label("Snippets");
            for snippet in snippets.iter().take(self.cfg.snippet_count) {
                let response = ui
                    .button(snippet_button_label(snippet))
                    .on_hover_text(snippet_button_tooltip(snippet));
                if response.clicked() {
                    clicked = Some(snippet_widget_action(snippet));
                }
            }
        }

        if self.cfg.show_system {
            if !self.system_refresh_requested {
                ctx.data_cache
                    .request_refresh(DashboardRefreshRequest::SystemStatus);
                self.system_refresh_requested = true;
            }
            ui.separator();
            ui.label("System snapshot");
            Self::render_system_snapshot(ui, ctx);
        }

        clicked
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HIDDEN_SNIPPET_TOOLTIP, clipboard_history_widget_action, shorten_snippet_preview,
        snippet_button_label, snippet_button_tooltip, snippet_widget_action,
    };
    use crate::plugins::snippets::{SnippetEntry, SnippetFieldDefinition, snippet_run_action};

    fn snippet(alias: &str, text: &str, hide_contents: bool) -> SnippetEntry {
        SnippetEntry {
            alias: alias.into(),
            text: text.into(),
            hide_contents,
            prompt_for_fields: false,
            fields: Vec::new(),
        }
    }

    #[test]
    fn hidden_snippet_dashboard_preview_and_tooltip_do_not_expose_body() {
        let body = "private λ\nsecond line 🧪";
        let hidden = snippet("private", body, true);

        assert_eq!(snippet_button_label(&hidden), "private: ******");
        assert_eq!(snippet_button_tooltip(&hidden), HIDDEN_SNIPPET_TOOLTIP);
        assert!(!snippet_button_label(&hidden).contains("private λ"));
        assert!(!snippet_button_tooltip(&hidden).contains("private λ"));

        let action = snippet_widget_action(&hidden);
        assert_eq!(action.action.action, snippet_run_action("private"));
        assert!(!action.action.action.contains("private λ"));
        assert_eq!(action.action.desc, "Snippet");
        assert_eq!(action.action.label, "private");
        assert_eq!(action.action.args, None);
        assert_eq!(action.query_override.as_deref(), Some("cs private"));
    }

    #[test]
    fn plain_and_prompted_snippet_rows_use_alias_identity_without_template_text() {
        let alias = "team: λ|%";
        let plain = snippet(alias, "Plain body with {{literal}}", false);
        let mut prompted = snippet(alias, "Hello {{name}}", false);
        prompted.prompt_for_fields = true;
        prompted.fields = vec![SnippetFieldDefinition::new("name")];

        let plain_action = snippet_widget_action(&plain);
        let prompted_action = snippet_widget_action(&prompted);
        let canonical = snippet_run_action(alias);
        assert_eq!(plain_action.action.action, canonical);
        assert_eq!(prompted_action.action.action, canonical);
        assert_eq!(
            crate::plugins::snippets::decode_snippet_run_action(&canonical).as_deref(),
            Some(alias)
        );
        assert_eq!(prompted_action.action.label, alias);
        assert_eq!(prompted_action.action.desc, "Snippet");
        assert_eq!(prompted_action.action.args, None);
        assert_eq!(
            prompted_action.query_override.as_deref(),
            Some("cs team: λ|%")
        );
        assert!(!prompted_action.action.action.contains("{{name}}"));
    }

    #[test]
    fn visible_snippet_keeps_full_hover_text_and_normalizes_preview() {
        let body = "  first\t世界\nsecond 🧪   ";
        let visible = snippet("normal", body, false);

        assert_eq!(
            snippet_button_label(&visible),
            "normal: first 世界 second 🧪"
        );
        assert_eq!(snippet_button_tooltip(&visible), body);
    }

    #[test]
    fn snippet_preview_limit_counts_unicode_characters() {
        let preview = format!("{}Z", "λ".repeat(40));
        assert_eq!(
            shorten_snippet_preview(&preview, 40),
            format!("{}…", "λ".repeat(40))
        );
    }

    #[test]
    fn clipboard_history_action_preserves_index_and_clipboard_identity() {
        let action = clipboard_history_widget_action(7);
        assert_eq!(action.action.desc, "Clipboard");
        assert_eq!(action.action.action, "clipboard:copy:7");
        assert_eq!(action.action.args, None);
        assert_eq!(action.query_override.as_deref(), Some("cb list"));
    }
}
