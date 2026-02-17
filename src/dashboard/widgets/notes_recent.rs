use super::{Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult};
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;

pub use super::recent_notes::RecentNotesConfig as NotesRecentConfig;
use super::recent_notes::RecentNotesWidget;

pub struct NotesRecentWidget {
    inner: RecentNotesWidget,
}

impl NotesRecentWidget {
    pub fn new(cfg: NotesRecentConfig) -> Self {
        Self {
            inner: RecentNotesWidget::new_legacy(cfg),
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        RecentNotesWidget::settings_ui(ui, value, ctx)
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
        activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        self.inner.render(ui, ctx, activation)
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        self.inner.on_config_updated(settings);
    }
}
