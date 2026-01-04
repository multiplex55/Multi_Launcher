use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WeatherSiteConfig {
    pub location: Option<String>,
    pub url: Option<String>,
}

pub struct WeatherSiteWidget {
    cfg: WeatherSiteConfig,
}

impl WeatherSiteWidget {
    pub fn new(cfg: WeatherSiteConfig) -> Self {
        Self { cfg }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut WeatherSiteConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Location");
                let mut text = cfg.location.clone().unwrap_or_default();
                if ui.text_edit_singleline(&mut text).changed() {
                    cfg.location = if text.trim().is_empty() {
                        None
                    } else {
                        Some(text)
                    };
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("URL template");
                let mut url = cfg.url.clone().unwrap_or_default();
                if ui.text_edit_singleline(&mut url).changed() {
                    cfg.url = if url.trim().is_empty() {
                        None
                    } else {
                        Some(url)
                    };
                    changed = true;
                }
            });
            changed
        })
    }

    fn effective_location<'a>(&'a self, ctx: &'a DashboardContext<'_>) -> Option<&'a str> {
        self.cfg
            .location
            .as_deref()
            .or_else(|| ctx.default_location)
    }
}

impl Default for WeatherSiteWidget {
    fn default() -> Self {
        Self {
            cfg: WeatherSiteConfig::default(),
        }
    }
}

impl Widget for WeatherSiteWidget {
    fn render(
        &mut self,
        ui: &mut eframe::egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let loc = self.effective_location(ctx).unwrap_or("your city");
        let url = self
            .cfg
            .url
            .clone()
            .unwrap_or_else(|| format!("https://www.google.com/search?q=weather+{loc}"));
        let label = format!("Weather: {loc}");
        let clicked = ui.button(&label).clicked();
        ui.label("Opens weather for the configured location.");
        if clicked {
            return Some(WidgetAction {
                action: Action {
                    label,
                    desc: "Weather".into(),
                    action: url,
                    args: None,
                },
                query_override: Some(format!("weather {loc}")),
            });
        }
        None
    }
}
