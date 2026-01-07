use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn default_count() -> usize {
    10
}

fn default_show_counts() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotesTagsConfig {
    #[serde(default = "default_count")]
    pub count: usize,
    #[serde(default = "default_show_counts")]
    pub show_counts: bool,
}

impl Default for NotesTagsConfig {
    fn default() -> Self {
        Self {
            count: default_count(),
            show_counts: true,
        }
    }
}

pub struct NotesTagsWidget {
    cfg: NotesTagsConfig,
    cached: Vec<(String, usize)>,
    last_notes_version: u64,
}

impl NotesTagsWidget {
    pub fn new(cfg: NotesTagsConfig) -> Self {
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
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut NotesTagsConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Show up to");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.count).clamp_range(1..=50))
                    .changed();
                ui.label("tags");
            });
            changed |= ui.checkbox(&mut cfg.show_counts, "Show counts").changed();
            changed
        })
    }

    fn refresh_cache(&mut self, ctx: &DashboardContext<'_>) {
        if self.last_notes_version == ctx.notes_version {
            return;
        }
        let snapshot = ctx.data_cache.snapshot();
        let mut counts: HashMap<String, usize> = HashMap::new();
        for note in snapshot.notes.as_ref() {
            for tag in &note.tags {
                *counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        let mut tags: Vec<(String, usize)> = counts.into_iter().collect();
        tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        tags.truncate(self.cfg.count);
        self.cached = tags;
        self.last_notes_version = ctx.notes_version;
    }
}

impl Default for NotesTagsWidget {
    fn default() -> Self {
        Self::new(NotesTagsConfig::default())
    }
}

impl Widget for NotesTagsWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        self.refresh_cache(ctx);

        if self.cached.is_empty() {
            ui.label("No tags found.");
            return None;
        }

        let mut clicked = None;
        for (tag, count) in &self.cached {
            let label = if self.cfg.show_counts {
                format!("#{tag} ({count})")
            } else {
                format!("#{tag}")
            };
            if ui.button(label).clicked() {
                clicked = Some(WidgetAction {
                    action: Action {
                        label: format!("#{tag}"),
                        desc: "Note".into(),
                        action: format!("query:note list #{tag}"),
                        args: None,
                    },
                    query_override: Some(format!("note list #{tag}")),
                });
            }
        }

        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<NotesTagsConfig>(settings.clone()) {
            self.cfg = cfg;
            self.last_notes_version = u64::MAX;
        }
    }
}
