use super::{
    edit_typed_settings, note_list_shared::render_note_rows, note_list_shared::CachedRecentNotes,
    Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};

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

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentNotesConfig {
    #[serde(default = "default_count", alias = "limit")]
    pub count: usize,
    #[serde(default, alias = "tag")]
    pub filter_tag: Option<String>,
    #[serde(default = "default_true")]
    pub show_snippet: bool,
    #[serde(default = "default_true")]
    pub show_tags: bool,
    #[serde(default)]
    pub open_mode: NoteOpenMode,
    #[serde(default, alias = "query_override")]
    pub query_override_on_open: Option<bool>,
}

impl Default for RecentNotesConfig {
    fn default() -> Self {
        Self {
            count: default_count(),
            filter_tag: None,
            show_snippet: true,
            show_tags: true,
            open_mode: NoteOpenMode::default(),
            query_override_on_open: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecentNotesProfile {
    RecentNotes,
    NotesRecentLegacy,
}

impl RecentNotesProfile {
    fn default_query_override_on_open(self) -> bool {
        match self {
            RecentNotesProfile::RecentNotes => false,
            RecentNotesProfile::NotesRecentLegacy => true,
        }
    }

    fn no_notes_message(self) -> &'static str {
        match self {
            RecentNotesProfile::RecentNotes => "No notes found",
            RecentNotesProfile::NotesRecentLegacy => "No notes found.",
        }
    }

    fn scroll_id(self) -> &'static str {
        match self {
            RecentNotesProfile::RecentNotes => "recent_notes_scroll",
            RecentNotesProfile::NotesRecentLegacy => "notes_recent_scroll",
        }
    }
}

pub struct RecentNotesWidget {
    cfg: RecentNotesConfig,
    profile: RecentNotesProfile,
    cached: CachedRecentNotes,
}

impl RecentNotesWidget {
    pub fn new(cfg: RecentNotesConfig) -> Self {
        Self::new_with_profile(cfg, RecentNotesProfile::RecentNotes)
    }

    pub fn new_legacy(cfg: RecentNotesConfig) -> Self {
        Self::new_with_profile(cfg, RecentNotesProfile::NotesRecentLegacy)
    }

    pub fn new_with_profile(cfg: RecentNotesConfig, profile: RecentNotesProfile) -> Self {
        Self {
            cfg,
            profile,
            cached: CachedRecentNotes::new(),
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
            changed |= ui.checkbox(&mut cfg.show_tags, "Show tags").changed();
            changed |= ui
                .checkbox(
                    cfg.query_override_on_open.get_or_insert(false),
                    "Set query to note-open command when clicked",
                )
                .changed();
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

    fn build_action(&self, note_slug: &str, note_title: &str) -> (Action, Option<String>) {
        let override_for_panel = self
            .cfg
            .query_override_on_open
            .unwrap_or_else(|| self.profile.default_query_override_on_open());

        match self.cfg.open_mode {
            NoteOpenMode::Panel => (
                Action {
                    label: note_title.to_string(),
                    desc: "Note".into(),
                    action: format!("note:open:{note_slug}"),
                    args: None,
                },
                override_for_panel.then(|| format!("note open {note_slug}")),
            ),
            NoteOpenMode::Dialog => (
                Action {
                    label: "Notes".into(),
                    desc: "Note".into(),
                    action: "note:dialog".into(),
                    args: None,
                },
                Some(format!("note open {note_slug}")),
            ),
            NoteOpenMode::Query => (
                Action {
                    label: "Open note".into(),
                    desc: "Note".into(),
                    action: "query:note open ".into(),
                    args: None,
                },
                Some(format!("note open {note_title}")),
            ),
        }
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
        self.cached
            .refresh(ctx, self.cfg.count, self.cfg.filter_tag.as_deref());

        render_note_rows(
            ui,
            self.profile.scroll_id(),
            &self.cached.entries,
            self.cfg.show_snippet,
            self.cfg.show_tags,
            self.profile.no_notes_message(),
            |note| {
                let (action, query_override) = self.build_action(&note.slug, &note.title);
                WidgetAction {
                    action,
                    query_override,
                }
            },
        )
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<RecentNotesConfig>(settings.clone()) {
            self.cfg = cfg;
            self.cached.last_notes_version = u64::MAX;
        }
    }
}
