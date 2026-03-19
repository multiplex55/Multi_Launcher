#[path = "settings_editor/mapping.rs"]
mod mapping;
#[path = "settings_editor/render.rs"]
mod render;
#[path = "settings_editor/save.rs"]
mod save;
#[path = "settings_editor/state.rs"]
mod state;

pub use state::SettingsEditor;

#[cfg(test)]
mod tests {
    use super::SettingsEditor;
    use crate::settings::Settings;

    #[test]
    fn settings_window_sizing_policy_uses_expected_defaults() {
        assert_eq!(SettingsEditor::SETTINGS_WINDOW_DEFAULT_WIDTH, 640.0);
        assert_eq!(SettingsEditor::SETTINGS_WINDOW_MIN_HEIGHT, 360.0);
        assert_eq!(
            SettingsEditor::settings_window_default_height(100.0),
            SettingsEditor::SETTINGS_WINDOW_MIN_HEIGHT
        );
        assert_eq!(
            SettingsEditor::settings_window_default_height(2000.0),
            SettingsEditor::SETTINGS_WINDOW_MAX_DEFAULT_HEIGHT
        );
        assert_eq!(SettingsEditor::settings_window_default_height(900.0), 450.0);
    }

    #[test]
    fn settings_content_height_has_sane_floor() {
        assert_eq!(SettingsEditor::settings_content_height(20.0), 180.0);
        assert_eq!(SettingsEditor::settings_content_height(320.0), 264.0);
        assert_eq!(SettingsEditor::settings_content_height(700.0), 644.0);
    }

    #[test]
    fn to_settings_preserves_static_values_when_follow_mouse_disabled() {
        let mut initial = Settings::default();
        initial.static_location_enabled = true;
        initial.static_pos = Some((100, 200));
        initial.static_size = Some((900, 700));

        let mut editor = SettingsEditor::new(&initial);
        editor.follow_mouse = false;
        editor.static_enabled = true;
        editor.static_x = 11;
        editor.static_y = 22;
        editor.static_w = 333;
        editor.static_h = 444;

        let saved = editor.to_settings(&initial);
        assert!(!saved.follow_mouse);
        assert!(saved.static_location_enabled);
        assert_eq!(saved.static_pos, Some((11, 22)));
        assert_eq!(saved.static_size, Some((333, 444)));
    }

    #[test]
    fn to_settings_disables_static_when_follow_mouse_enabled() {
        let mut initial = Settings::default();
        initial.static_location_enabled = true;
        initial.static_pos = Some((100, 200));
        initial.static_size = Some((900, 700));

        let mut editor = SettingsEditor::new(&initial);
        editor.follow_mouse = true;
        editor.static_enabled = true;
        editor.static_x = 1;
        editor.static_y = 2;
        editor.static_w = 3;
        editor.static_h = 4;

        let saved = editor.to_settings(&initial);
        assert!(saved.follow_mouse);
        assert!(!saved.static_location_enabled);
        assert_eq!(saved.static_pos, None);
        assert_eq!(saved.static_size, None);
    }
}
