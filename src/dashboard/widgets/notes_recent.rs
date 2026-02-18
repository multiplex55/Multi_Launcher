use super::{
    recent_notes::RecentNotesConfig, RecentNotesWidget, WidgetSettingsContext,
    WidgetSettingsUiResult,
};
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotesRecentConfig {
    #[serde(default = "super::recent_notes::default_count")]
    pub count: usize,
    #[serde(default = "super::recent_notes::default_true")]
    pub show_snippet: bool,
    #[serde(default = "super::recent_notes::default_true")]
    pub show_tags: bool,
}

impl Default for NotesRecentConfig {
    fn default() -> Self {
        Self {
            count: super::recent_notes::default_count(),
            show_snippet: true,
            show_tags: true,
        }
    }
}

impl From<NotesRecentConfig> for RecentNotesConfig {
    fn from(value: NotesRecentConfig) -> Self {
        RecentNotesConfig {
            count: value.count,
            show_snippet: value.show_snippet,
            show_tags: value.show_tags,
            include_query_override: true,
            ..RecentNotesConfig::default()
        }
    }
}

pub struct NotesRecentWidget(RecentNotesWidget);

impl NotesRecentWidget {
    pub fn new(cfg: NotesRecentConfig) -> Self {
        Self(RecentNotesWidget::new(cfg.into()))
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        super::edit_typed_settings(ui, value, ctx, |ui, cfg: &mut NotesRecentConfig, _ctx| {
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
}

impl Default for NotesRecentWidget {
    fn default() -> Self {
        Self::new(NotesRecentConfig::default())
    }
}

impl super::Widget for NotesRecentWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &crate::dashboard::dashboard::DashboardContext<'_>,
        activation: crate::dashboard::dashboard::WidgetActivation,
    ) -> Option<super::WidgetAction> {
        self.0.render(ui, ctx, activation)
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<NotesRecentConfig>(settings.clone()) {
            self.0 = RecentNotesWidget::new(cfg.into());
        }
    }
}
