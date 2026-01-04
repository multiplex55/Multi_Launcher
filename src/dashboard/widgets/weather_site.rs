use super::{Widget, WidgetAction};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
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
