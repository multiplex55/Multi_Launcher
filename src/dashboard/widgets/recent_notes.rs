use super::{
    edit_typed_settings, recent_notes_shared::*, Widget, WidgetAction, WidgetSettingsContext,
    WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum NoteOpenMode {
    #[default]
    Panel,
    Dialog,
    Query,
}

pub(crate) fn default_count() -> usize {
    5
}

pub(crate) fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentNotesConfig {
    #[serde(default = "default_count", alias = "limit")]
    pub count: usize,
    #[serde(default, alias = "tag")]
    pub filter_tag: Option<String>,
    #[serde(default = "default_true", alias = "snippet")]
    pub show_snippet: bool,
    #[serde(default = "default_true", alias = "tags")]
    pub show_tags: bool,
    #[serde(default)]
    pub open_mode: NoteOpenMode,
    #[serde(default)]
    pub include_query_override: bool,
}

impl Default for RecentNotesConfig {
    fn default() -> Self {
        Self {
            count: default_count(),
            filter_tag: None,
            show_snippet: default_true(),
            show_tags: default_true(),
            open_mode: NoteOpenMode::default(),
            include_query_override: false,
        }
    }
}

pub struct RecentNotesWidget {
    cfg: RecentNotesConfig,
    cached: Vec<CachedRecentNote>,
    last_notes_version: u64,
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
            changed |= ui.checkbox(&mut cfg.show_tags, "Show tags").changed();
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

    fn build_cached_action(&self, note: &CachedRecentNote) -> (Action, Option<String>) {
        let query_override = match self.cfg.open_mode {
            NoteOpenMode::Panel if self.cfg.include_query_override => {
                Some(format!("note open {}", note.slug))
            }
            NoteOpenMode::Panel => None,
            NoteOpenMode::Dialog => Some(format!("note open {}", note.slug)),
            NoteOpenMode::Query => Some(format!("note open {}", note.title)),
        };

        let action = match self.cfg.open_mode {
            NoteOpenMode::Panel => Action {
                label: note.title.clone(),
                desc: "Note".into(),
                action: format!("note:open:{}", note.slug),
                args: None,
            },
            NoteOpenMode::Dialog => Action {
                label: "Notes".into(),
                desc: "Note".into(),
                action: "note:dialog".into(),
                args: None,
            },
            NoteOpenMode::Query => Action {
                label: "Open note".into(),
                desc: "Note".into(),
                action: "query:note open ".into(),
                args: None,
            },
        };

        (action, query_override)
    }

    fn refresh_cache(&mut self, ctx: &DashboardContext<'_>) {
        refresh_cached_notes(
            &mut self.cached,
            &mut self.last_notes_version,
            ctx,
            self.cfg.count,
            self.cfg.filter_tag.as_deref(),
        );
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
            ui.label("No notes found.");
            return None;
        }

        let mut clicked = None;
        render_note_rows(
            ui,
            "recent_notes_scroll",
            &self.cached,
            self.cfg.show_snippet,
            self.cfg.show_tags,
            |note| {
                let (action, query_override) = self.build_cached_action(note);
                clicked = Some(WidgetAction {
                    action,
                    query_override,
                });
            },
        );

        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<RecentNotesConfig>(settings.clone()) {
            self.cfg = cfg;
            self.last_notes_version = u64::MAX;
        }
    }
}
