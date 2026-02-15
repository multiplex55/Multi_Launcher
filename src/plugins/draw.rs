use crate::actions::Action;
use crate::draw::service::runtime;
use crate::draw::settings::DrawSettings;
use crate::draw::settings_store;
use crate::draw::settings_ui::render_draw_settings_form;
use crate::plugin::Plugin;
use eframe::egui;

const PLUGIN_NAME: &str = "draw";

pub struct DrawPlugin {
    settings: DrawSettings,
}

impl Default for DrawPlugin {
    fn default() -> Self {
        let mut settings = settings_store::load_dedicated()
            .ok()
            .flatten()
            .unwrap_or_default();
        settings.sanitize_for_first_pass_transparency();
        runtime().apply_settings(settings.clone());
        Self { settings }
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
        let _ = settings_store::save(&settings);
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
        let mut settings = settings_store::load_dedicated()
            .ok()
            .flatten()
            .or_else(|| serde_json::from_value::<DrawSettings>(value.clone()).ok())
            .or_else(|| settings_store::load("settings.json").ok())
            .unwrap_or_default();
        settings.sanitize_for_first_pass_transparency();
        self.settings = settings.clone();
        runtime().apply_settings(settings);
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui, value: &mut serde_json::Value) {
        let mut settings = self.settings.clone();

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
    use crate::draw::settings_store;
    use crate::plugin::Plugin;
    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    static DRAW_SETTINGS_TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn set_draw_settings_path_for_test(path: &std::path::Path) -> Option<std::ffi::OsString> {
        let prev = std::env::var_os("ML_DRAW_SETTINGS_PATH");
        std::env::set_var("ML_DRAW_SETTINGS_PATH", path);
        prev
    }

    fn restore_draw_settings_path_for_test(prev: Option<std::ffi::OsString>) {
        if let Some(value) = prev {
            std::env::set_var("ML_DRAW_SETTINGS_PATH", value);
        } else {
            std::env::remove_var("ML_DRAW_SETTINGS_PATH");
        }
    }

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
        let _lock = DRAW_SETTINGS_TEST_MUTEX.lock().expect("lock");
        let path_dir = tempfile::tempdir().expect("temp dir");
        let path_prev = set_draw_settings_path_for_test(
            &path_dir
                .path()
                .join(settings_store::DRAW_SETTINGS_FILE_NAME),
        );
        let mut plugin = DrawPlugin::default();
        let default_value = plugin.default_settings().expect("default settings");
        plugin.apply_settings(&default_value);
        let applied: DrawSettings =
            serde_json::from_value(default_value).expect("deserialize draw settings");
        assert_eq!(plugin.settings, applied);
        restore_draw_settings_path_for_test(path_prev);
    }

    #[test]
    fn reset_action_restores_defaults_after_customization() {
        let _lock = DRAW_SETTINGS_TEST_MUTEX.lock().expect("lock");
        let path_dir = tempfile::tempdir().expect("temp dir");
        let path_prev = set_draw_settings_path_for_test(
            &path_dir
                .path()
                .join(settings_store::DRAW_SETTINGS_FILE_NAME),
        );
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
        restore_draw_settings_path_for_test(path_prev);
    }

    #[test]
    fn apply_settings_updates_runtime_settings() {
        let _lock = DRAW_SETTINGS_TEST_MUTEX.lock().expect("lock");
        let path_dir = tempfile::tempdir().expect("temp dir");
        let path_prev = set_draw_settings_path_for_test(
            &path_dir
                .path()
                .join(settings_store::DRAW_SETTINGS_FILE_NAME),
        );
        let rt = runtime();
        rt.reset_for_test();

        let mut plugin = DrawPlugin::default();
        let mut custom = DrawSettings::default();
        custom.exit_timeout_seconds = 321;
        let value = serde_json::to_value(&custom).expect("serialize settings");

        plugin.apply_settings(&value);

        assert_eq!(rt.settings_for_test(), Some(custom));
        restore_draw_settings_path_for_test(path_prev);
        rt.reset_for_test();
    }

    #[test]
    fn apply_settings_resolves_colorkey_collision_before_runtime_update() {
        let _lock = DRAW_SETTINGS_TEST_MUTEX.lock().expect("lock");
        let path_dir = tempfile::tempdir().expect("temp dir");
        let path_prev = set_draw_settings_path_for_test(
            &path_dir
                .path()
                .join(settings_store::DRAW_SETTINGS_FILE_NAME),
        );
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
        restore_draw_settings_path_for_test(path_prev);
        rt.reset_for_test();
    }

    #[test]
    fn apply_settings_prefers_dedicated_store_when_present() {
        let _lock = DRAW_SETTINGS_TEST_MUTEX.lock().expect("lock");
        let rt = runtime();
        rt.reset_for_test();

        let dir = tempfile::tempdir().expect("temp dir");
        let draw_settings_path = dir.path().join(settings_store::DRAW_SETTINGS_FILE_NAME);
        let path_prev = set_draw_settings_path_for_test(&draw_settings_path);

        let mut dedicated = DrawSettings::default();
        dedicated.last_width = 93;
        settings_store::save(&dedicated).expect("save dedicated settings");

        let mut legacy = DrawSettings::default();
        legacy.last_width = 11;
        let legacy_value = serde_json::to_value(&legacy).expect("serialize legacy value");

        let mut plugin = DrawPlugin::default();
        plugin.apply_settings(&legacy_value);

        assert_eq!(plugin.settings.last_width, 93);
        assert_eq!(
            rt.settings_for_test().expect("runtime settings").last_width,
            93
        );

        restore_draw_settings_path_for_test(path_prev);
        rt.reset_for_test();
    }

    #[test]
    fn apply_settings_migrates_from_legacy_plugin_settings_when_dedicated_missing() {
        let _lock = DRAW_SETTINGS_TEST_MUTEX.lock().expect("lock");
        let rt = runtime();
        rt.reset_for_test();

        let dir = tempfile::tempdir().expect("temp dir");
        let draw_settings_path = dir.path().join(settings_store::DRAW_SETTINGS_FILE_NAME);
        let path_prev = set_draw_settings_path_for_test(&draw_settings_path);

        let orig_dir = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir.path()).expect("set cwd");

        let mut legacy = crate::settings::Settings::default();
        let mut expected = DrawSettings::default();
        expected.exit_timeout_seconds = 909;
        legacy.plugin_settings.insert(
            "draw".to_string(),
            serde_json::to_value(&expected).expect("serialize legacy settings"),
        );
        legacy.save("settings.json").expect("save settings.json");

        let mut plugin = DrawPlugin::default();
        plugin.apply_settings(&serde_json::Value::Null);

        assert_eq!(plugin.settings.exit_timeout_seconds, 909);
        assert_eq!(
            rt.settings_for_test()
                .expect("runtime settings")
                .exit_timeout_seconds,
            909
        );

        std::env::set_current_dir(orig_dir).expect("restore cwd");
        restore_draw_settings_path_for_test(path_prev);
        rt.reset_for_test();
    }
}
