use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::fav::FavEntry;
use eframe::egui;
use serde::{Deserialize, Serialize};

fn default_count() -> usize {
    6
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetsFavoritesConfig {
    #[serde(default = "default_count")]
    pub count: usize,
}

impl Default for SnippetsFavoritesConfig {
    fn default() -> Self {
        Self {
            count: default_count(),
        }
    }
}

pub struct SnippetsFavoritesWidget {
    cfg: SnippetsFavoritesConfig,
}

impl SnippetsFavoritesWidget {
    pub fn new(cfg: SnippetsFavoritesConfig) -> Self {
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
            |ui, cfg: &mut SnippetsFavoritesConfig, _ctx| {
                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label("Show");
                    changed |= ui
                        .add(egui::DragValue::new(&mut cfg.count).clamp_range(1..=50))
                        .changed();
                    ui.label("favorite snippets");
                });
                changed
            },
        )
    }

    fn is_snippet_favorite(entry: &FavEntry) -> bool {
        let action = entry.action.to_lowercase();
        action.starts_with("clipboard:")
            || action.starts_with("snippet:")
            || action.starts_with("query:cs")
    }
}

impl Default for SnippetsFavoritesWidget {
    fn default() -> Self {
        Self::new(SnippetsFavoritesConfig::default())
    }
}

impl Widget for SnippetsFavoritesWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let snapshot = ctx.data_cache.snapshot();
        let favorites = snapshot.favorites.as_ref();
        let mut entries: Vec<FavEntry> = favorites
            .iter()
            .filter(|e| Self::is_snippet_favorite(e))
            .cloned()
            .collect();
        entries.truncate(self.cfg.count);

        if entries.is_empty() {
            ui.label("No favorite snippets yet.");
            return None;
        }

        let mut clicked = None;
        for entry in entries {
            if ui.button(&entry.label).clicked() {
                clicked = Some(WidgetAction {
                    action: Action {
                        label: entry.label.clone(),
                        desc: "Fav".into(),
                        action: entry.action.clone(),
                        args: entry.args.clone(),
                    },
                    query_override: Some("fav".into()),
                });
            }
        }

        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<SnippetsFavoritesConfig>(settings.clone()) {
            self.cfg = cfg;
        }
    }
}
