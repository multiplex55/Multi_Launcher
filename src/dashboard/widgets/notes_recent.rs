use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::note::Note;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

fn default_count() -> usize {
    5
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotesRecentConfig {
    #[serde(default = "default_count")]
    pub count: usize,
    #[serde(default = "default_true")]
    pub show_snippet: bool,
    #[serde(default = "default_true")]
    pub show_tags: bool,
}

impl Default for NotesRecentConfig {
    fn default() -> Self {
        Self {
            count: default_count(),
            show_snippet: true,
            show_tags: true,
        }
    }
}

#[derive(Clone)]
struct NoteSummary {
    title: String,
    slug: String,
    tags: Vec<String>,
    snippet: String,
}

pub struct NotesRecentWidget {
    cfg: NotesRecentConfig,
    cached: Vec<NoteSummary>,
    last_notes_version: u64,
}

impl NotesRecentWidget {
    pub fn new(cfg: NotesRecentConfig) -> Self {
        Self {
            cfg,
            cached: Vec::new(),
            last_notes_version: u64::MAX,
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut NotesRecentConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Show");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.count).clamp_range(1..=50))
                    .changed();
                ui.label("notes");
            });
            changed |= ui.checkbox(&mut cfg.show_snippet, "Show snippet").changed();
            changed |= ui.checkbox(&mut cfg.show_tags, "Show tags").changed();
            changed
        })
    }

    fn modified_ts(note: &Note) -> u64 {
        note.path
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| m.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn snippet(note: &Note) -> String {
        let first_line = note
            .content
            .lines()
            .skip_while(|l| l.starts_with("# ") || l.starts_with("Alias:"))
            .find(|l| !l.trim().is_empty())
            .unwrap_or_default();
        let clean = first_line.trim();
        if clean.len() > 120 {
            format!("{}…", &clean[..120])
        } else {
            clean.to_string()
        }
    }

    fn refresh_cache(&mut self, ctx: &DashboardContext<'_>) {
        if self.last_notes_version == ctx.notes_version {
            return;
        }
        let snapshot = ctx.data_cache.snapshot();
        let mut notes: Vec<Note> = snapshot.notes.as_ref().clone();
        notes.sort_by(|a, b| Self::modified_ts(b).cmp(&Self::modified_ts(a)));
        notes.truncate(self.cfg.count);
        self.cached = notes
            .iter()
            .map(|note| NoteSummary {
                title: note.alias.as_ref().unwrap_or(&note.title).clone(),
                slug: note.slug.clone(),
                tags: note.tags.clone(),
                snippet: Self::snippet(note),
            })
            .collect();
        self.last_notes_version = ctx.notes_version;
    }
}

impl Default for NotesRecentWidget {
    fn default() -> Self {
        Self::new(NotesRecentConfig::default())
    }
}

impl Widget for NotesRecentWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        self.refresh_cache(ctx);

        if self.cached.is_empty() {
            ui.label("No notes found.");
            return None;
        }

        let mut clicked = None;
        let body_height = ui.text_style_height(&egui::TextStyle::Body);
        let small_height = ui.text_style_height(&egui::TextStyle::Small);
        let mut row_height = body_height + ui.spacing().item_spacing.y + 8.0;
        if self.cfg.show_snippet {
            row_height += small_height + 2.0;
        }
        if self.cfg.show_tags {
            row_height += small_height + 2.0;
        }
        let scroll_id = ui.id().with("notes_recent_scroll");
        egui::ScrollArea::both()
            .id_source(scroll_id)
            .auto_shrink([false; 2])
            .show_rows(ui, row_height, self.cached.len(), |ui, range| {
                for note in &self.cached[range] {
                    let mut clicked_row = false;
                    ui.vertical(|ui| {
                        clicked_row |= ui.add(egui::Button::new(&note.title).wrap(false)).clicked();
                        if self.cfg.show_snippet {
                            ui.add(
                                egui::Label::new(egui::RichText::new(&note.snippet).small())
                                    .wrap(false),
                            );
                        }
                        if self.cfg.show_tags && !note.tags.is_empty() {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!("#{}", note.tags.join(" #")))
                                        .small(),
                                )
                                .wrap(false),
                            );
                        }
                        ui.add_space(4.0);
                    });
                    if clicked_row {
                        clicked = Some(WidgetAction {
                            action: Action {
                                label: note.title.clone(),
                                desc: "Note".into(),
                                action: format!("note:open:{}", note.slug),
                                args: None,
                                preview_text: None,
                                risk_level: None,
                                icon: None,
                            },
                            query_override: Some(format!("note open {}", note.slug)),
                        });
                    }
                }
            });

        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<NotesRecentConfig>(settings.clone()) {
            self.cfg = cfg;
            self.last_notes_version = u64::MAX;
        }
    }
}
