use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowPlayingConfig {
    #[serde(default = "default_true")]
    pub show_play: bool,
    #[serde(default = "default_true")]
    pub show_pause: bool,
    #[serde(default = "default_true")]
    pub show_prev: bool,
    #[serde(default = "default_true")]
    pub show_next: bool,
}

impl Default for NowPlayingConfig {
    fn default() -> Self {
        Self {
            show_play: true,
            show_pause: true,
            show_prev: true,
            show_next: true,
        }
    }
}

fn default_true() -> bool {
    true
}

pub struct NowPlayingWidget {
    cfg: NowPlayingConfig,
}

impl NowPlayingWidget {
    pub fn new(cfg: NowPlayingConfig) -> Self {
        Self { cfg }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut NowPlayingConfig, _ctx| {
            let mut changed = false;
            ui.label("Show controls");
            changed |= ui.checkbox(&mut cfg.show_play, "Play").changed();
            changed |= ui.checkbox(&mut cfg.show_pause, "Pause").changed();
            changed |= ui.checkbox(&mut cfg.show_prev, "Previous").changed();
            changed |= ui.checkbox(&mut cfg.show_next, "Next").changed();
            changed
        })
    }

    fn action(label: &str, action: &str, query: &str) -> WidgetAction {
        WidgetAction {
            action: Action {
                label: label.into(),
                desc: "Media".into(),
                action: action.into(),
                args: None,
            },
            query_override: Some(query.into()),
        }
    }
}

impl Default for NowPlayingWidget {
    fn default() -> Self {
        Self::new(NowPlayingConfig::default())
    }
}

impl Widget for NowPlayingWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let mut action = None;
        ui.horizontal_wrapped(|ui| {
            if self.cfg.show_play && ui.button("Play").clicked() {
                action = Some(Self::action("Media play", "media:play", "media play"));
            }
            if self.cfg.show_pause && ui.button("Pause").clicked() {
                action = Some(Self::action("Media pause", "media:pause", "media pause"));
            }
            if self.cfg.show_prev && ui.button("Prev").clicked() {
                action = Some(Self::action("Media prev", "media:prev", "media prev"));
            }
            if self.cfg.show_next && ui.button("Next").clicked() {
                action = Some(Self::action("Media next", "media:next", "media next"));
            }
        });
        action
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<NowPlayingConfig>(settings.clone()) {
            self.cfg = cfg;
        }
    }
}
