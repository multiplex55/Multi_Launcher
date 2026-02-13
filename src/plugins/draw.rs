use crate::actions::Action;
use crate::plugin::Plugin;
use eframe::egui;
use serde::{Deserialize, Serialize};

const PLUGIN_NAME: &str = "draw";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DrawSettings {
    #[serde(default = "default_enable_pressure")]
    pub enable_pressure: bool,
}

fn default_enable_pressure() -> bool {
    true
}

impl Default for DrawSettings {
    fn default() -> Self {
        Self {
            enable_pressure: default_enable_pressure(),
        }
    }
}

pub struct DrawPlugin {
    settings: DrawSettings,
}

impl Default for DrawPlugin {
    fn default() -> Self {
        Self {
            settings: DrawSettings::default(),
        }
    }
}

impl Plugin for DrawPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let query = query.trim();
        if query.eq_ignore_ascii_case("draw") {
            return vec![Action {
                label: "Enter drawing mode".into(),
                desc: "Draw".into(),
                action: "draw:enter".into(),
                args: None,
            }];
        }

        if query.eq_ignore_ascii_case("draw setting") || query.eq_ignore_ascii_case("draw settings")
        {
            return vec![Action {
                label: "Draw settings".into(),
                desc: "Draw".into(),
                action: "draw:dialog:settings".into(),
                args: None,
            }];
        }

        Vec::new()
    }

    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn description(&self) -> &str {
        "Drawing tools and settings"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "draw".into(),
                desc: "Draw".into(),
                action: "query:draw".into(),
                args: None,
            },
            Action {
                label: "draw settings".into(),
                desc: "Draw".into(),
                action: "query:draw settings".into(),
                args: None,
            },
        ]
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(DrawSettings::default()).ok()
    }

    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Ok(settings) = serde_json::from_value::<DrawSettings>(value.clone()) {
            self.settings = settings;
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut settings: DrawSettings =
            serde_json::from_value(value.clone()).unwrap_or_else(|_| self.settings.clone());
        ui.checkbox(&mut settings.enable_pressure, "Enable pressure sensitivity");
        self.settings = settings.clone();
        if let Ok(serialized) = serde_json::to_value(&settings) {
            *value = serialized;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DrawPlugin, DrawSettings};
    use crate::plugin::Plugin;

    #[test]
    fn search_draw_returns_enter_action() {
        let plugin = DrawPlugin::default();
        let actions = plugin.search("DrAw");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].label, "Enter drawing mode");
        assert_eq!(actions[0].desc, "Draw");
        assert_eq!(actions[0].action, "draw:enter");
    }

    #[test]
    fn search_draw_settings_returns_settings_action() {
        let plugin = DrawPlugin::default();
        let actions = plugin.search("DRAW SETTINGS");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].label, "Draw settings");
        assert_eq!(actions[0].desc, "Draw");
        assert_eq!(actions[0].action, "draw:dialog:settings");
    }

    #[test]
    fn commands_exposes_draw_and_draw_settings() {
        let plugin = DrawPlugin::default();
        let commands = plugin.commands();
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].label, "draw");
        assert_eq!(commands[1].label, "draw settings");
    }

    #[test]
    fn settings_roundtrip_default_apply() {
        let mut plugin = DrawPlugin::default();
        let default_value = plugin.default_settings().expect("default settings");
        plugin.apply_settings(&default_value);
        let applied: DrawSettings =
            serde_json::from_value(default_value).expect("deserialize draw settings");
        assert_eq!(plugin.settings, applied);
    }
}
