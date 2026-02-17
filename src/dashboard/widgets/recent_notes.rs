use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::note::Note;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NoteOpenMode {
    Panel,
    Dialog,
    Query,
}

impl Default for NoteOpenMode {
    fn default() -> Self {
        NoteOpenMode::Panel
    }
}

fn default_count() -> usize {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentNotesConfig {
    #[serde(default = "default_count")]
    pub count: usize,
    #[serde(default)]
    pub filter_tag: Option<String>,
    #[serde(default = "default_show_snippet")]
    pub show_snippet: bool,
    #[serde(default)]
    pub open_mode: NoteOpenMode,
}

fn default_show_snippet() -> bool {
    true
}

impl Default for RecentNotesConfig {
    fn default() -> Self {
        Self {
            count: default_count(),
            filter_tag: None,
            show_snippet: default_show_snippet(),
            open_mode: NoteOpenMode::default(),
        }
    }
}

pub struct RecentNotesWidget {
    cfg: RecentNotesConfig,
    cached: Vec<CachedRecentNote>,
    last_notes_version: u64,
}

#[derive(Clone)]
struct CachedRecentNote {
    title: String,
    slug: String,
    tags: Vec<String>,
    snippet: String,
}

impl RecentNotesWidget {
    pub fn new(cfg: RecentNotesConfig) -> Self {
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
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut RecentNotesConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Show");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.count).clamp_range(1..=50))
                    .changed();
                ui.label("notes");
            });
            ui.horizontal(|ui| {
                ui.label("Filter by tag");
                let mut tag = cfg.filter_tag.clone().unwrap_or_default();
                if ui.text_edit_singleline(&mut tag).changed() {
                    cfg.filter_tag = if tag.trim().is_empty() {
                        None
                    } else {
                        Some(tag.trim().to_string())
                    };
                    changed = true;
                }
            });
            changed |= ui.checkbox(&mut cfg.show_snippet, "Show snippet").changed();
            egui::ComboBox::from_label("Open mode")
                .selected_text(match cfg.open_mode {
                    NoteOpenMode::Panel => "Open note panel",
                    NoteOpenMode::Dialog => "Open note dialog",
                    NoteOpenMode::Query => "Fill query only",
                })
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(
                            &mut cfg.open_mode,
                            NoteOpenMode::Panel,
                            "Open note panel",
                        )
                        .changed();
                    changed |= ui
                        .selectable_value(
                            &mut cfg.open_mode,
                            NoteOpenMode::Dialog,
                            "Open note dialog",
                        )
                        .changed();
                    changed |= ui
                        .selectable_value(
                            &mut cfg.open_mode,
                            NoteOpenMode::Query,
                            "Fill query only",
                        )
                        .changed();
                });
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

    fn build_cached_action(&self, note: &CachedRecentNote) -> (Action, Option<String>) {
        match self.cfg.open_mode {
            NoteOpenMode::Panel => (
                Action {
                    label: note.title.clone(),
                    desc: "Note".into(),
                    action: format!("note:open:{}", note.slug),
                    args: None,
                },
                None,
            ),
            NoteOpenMode::Dialog => (
                Action {
                    label: "Notes".into(),
                    desc: "Note".into(),
                    action: "note:dialog".into(),
                    args: None,
                },
                Some(format!("note open {}", note.slug)),
            ),
            NoteOpenMode::Query => (
                Action {
                    label: "Open note".into(),
                    desc: "Note".into(),
                    action: "query:note open ".into(),
                    args: None,
                },
                Some(format!("note open {}", note.title)),
            ),
        }
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
        let mut notes_with_ts: Vec<(u64, Note)> = snapshot
            .notes
            .as_ref()
            .iter()
            .filter(|note| {
                self.cfg
                    .filter_tag
                    .as_ref()
                    .is_none_or(|tag| note.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)))
            })
            .map(|note| (Self::modified_ts(note), note.clone()))
            .collect();

        notes_with_ts.sort_by(|a, b| b.0.cmp(&a.0));
        notes_with_ts.truncate(self.cfg.count);

        self.cached = notes_with_ts
            .into_iter()
            .map(|(_, note)| CachedRecentNote {
                title: note.alias.as_ref().unwrap_or(&note.title).clone(),
                slug: note.slug,
                tags: note.tags,
                snippet: Self::snippet(&note),
            })
            .collect();

        self.last_notes_version = ctx.notes_version;
    }
}

impl Default for RecentNotesWidget {
    fn default() -> Self {
        Self::new(RecentNotesConfig::default())
    }
}

impl Widget for RecentNotesWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        self.refresh_cache(ctx);

        if self.cached.is_empty() {
            ui.label("No notes found");
            return None;
        }

        let mut clicked = None;
        let body_height = ui.text_style_height(&egui::TextStyle::Body);
        let small_height = ui.text_style_height(&egui::TextStyle::Small);
        let mut row_height = body_height + ui.spacing().item_spacing.y + 8.0;
        if self.cfg.show_snippet {
            row_height += small_height + 2.0;
        }
        row_height += small_height + 2.0;

        let scroll_id = ui.id().with("recent_notes_scroll");
        egui::ScrollArea::both()
            .id_source(scroll_id)
            .auto_shrink([false; 2])
            .show_rows(ui, row_height, self.cached.len(), |ui, range| {
                for note in &self.cached[range] {
                    let (action, query_override) = self.build_cached_action(note);
                    let mut clicked_row = false;
                    ui.vertical(|ui| {
                        clicked_row |= ui.add(egui::Button::new(&note.title).wrap(false)).clicked();
                        if self.cfg.show_snippet {
                            ui.add(
                                egui::Label::new(egui::RichText::new(&note.snippet).small())
                                    .wrap(false),
                            );
                        }
                        if !note.tags.is_empty() {
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
                            action,
                            query_override,
                        });
                    }
                }
            });

        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<RecentNotesConfig>(settings.clone()) {
            self.cfg = cfg;
            self.last_notes_version = u64::MAX;
        }
    }
}
