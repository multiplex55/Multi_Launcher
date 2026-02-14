use crate::actions::Action;
use crate::draw::service::runtime;
use crate::draw::settings::DrawSettings;
use crate::draw::settings_ui::render_draw_settings_form;
use crate::plugin::Plugin;
use eframe::egui;

const PLUGIN_NAME: &str = "draw";

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

impl DrawPlugin {
    fn persist_settings(&mut self, value: &mut serde_json::Value, mut settings: DrawSettings) {
        settings.sanitize_for_first_pass_transparency();
        self.settings = settings.clone();
        runtime().apply_settings(settings.clone());
        if let Ok(serialized) = serde_json::to_value(&settings) {
            *value = serialized;
        }
    }

    fn reset_settings(&mut self, value: &mut serde_json::Value) {
        self.persist_settings(value, DrawSettings::default());
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
                action: "draw:enter".into(),
                args: None,
            },
            Action {
                label: "draw settings".into(),
                desc: "Draw".into(),
                action: "draw:dialog:settings".into(),
                args: None,
            },
        ]
    }

    fn default_settings(&self) -> Option<serde_json::Value> {
        serde_json::to_value(DrawSettings::default()).ok()
    }

    fn apply_settings(&mut self, value: &serde_json::Value) {
        if let Ok(mut settings) = serde_json::from_value::<DrawSettings>(value.clone()) {
            settings.sanitize_for_first_pass_transparency();
            self.settings = settings.clone();
            runtime().apply_settings(settings);
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut settings: DrawSettings =
            serde_json::from_value(value.clone()).unwrap_or_else(|_| self.settings.clone());

        let form_result = render_draw_settings_form(ui, &mut settings, "draw_plugin");
        if let Some(error) = form_result.toolbar_hotkey_error.as_ref() {
            ui.colored_label(egui::Color32::RED, error);
        }

        if ui.button("Reset Draw Settings").clicked() {
            self.reset_settings(value);
            return;
        }

        if form_result.toolbar_hotkey_error.is_none() {
            self.persist_settings(value, settings);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DrawPlugin;
    use crate::draw::service::runtime;
    use crate::draw::settings::{DrawColor, DrawSettings};
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
        assert_eq!(commands[0].action, "draw:enter");
        assert_eq!(commands[1].label, "draw settings");
        assert_eq!(commands[1].action, "draw:dialog:settings");
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

    #[test]
    fn reset_action_restores_defaults_after_customization() {
        let mut plugin = DrawPlugin::default();
        let mut settings = DrawSettings::default();
        settings.exit_timeout_seconds = 42;
        settings.quick_colors[0] = DrawColor::rgba(1, 2, 3, 255);
        let mut value = serde_json::to_value(settings).expect("serialize custom settings");

        plugin.reset_settings(&mut value);

        let reset: DrawSettings =
            serde_json::from_value(value).expect("deserialize reset settings");
        assert_eq!(reset, DrawSettings::default());
        assert_eq!(plugin.settings, DrawSettings::default());
    }

    #[test]
    fn apply_settings_updates_runtime_settings() {
        let rt = runtime();
        rt.reset_for_test();

        let mut plugin = DrawPlugin::default();
        let mut custom = DrawSettings::default();
        custom.exit_timeout_seconds = 321;
        let value = serde_json::to_value(&custom).expect("serialize settings");

        plugin.apply_settings(&value);

        assert_eq!(rt.settings_for_test(), Some(custom));
        rt.reset_for_test();
    }

    #[test]
    fn apply_settings_resolves_colorkey_collision_before_runtime_update() {
        let rt = runtime();
        rt.reset_for_test();

        let mut plugin = DrawPlugin::default();
        let mut custom = DrawSettings::default();
        custom.last_color = DrawColor::rgba(255, 0, 255, 12);
        let value = serde_json::to_value(&custom).expect("serialize settings");

        plugin.apply_settings(&value);

        let applied = rt.settings_for_test().expect("runtime settings");
        assert_eq!(applied.last_color, DrawColor::rgba(254, 0, 255, 255));
        assert_eq!(
            plugin.settings.last_color,
            DrawColor::rgba(254, 0, 255, 255)
        );
        rt.reset_for_test();
    }
}
