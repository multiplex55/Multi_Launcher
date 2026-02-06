use multi_launcher::gui::ThemeSettingsDialogState;
use multi_launcher::settings::{Settings, ThemeMode, ThemeSettings};
use tempfile::tempdir;

#[test]
fn save_updates_settings_and_clears_dirty_flag() {
    let dir = tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    let settings = Settings::default();
    settings.save(settings_path.to_str().unwrap()).unwrap();

    let mut state = ThemeSettingsDialogState::default();
    state.reload_from_path(settings_path.to_str().unwrap());
    state.draft.mode = ThemeMode::Light;
    state.dirty = true;

    state.save_to_path(settings_path.to_str().unwrap()).unwrap();

    let updated = Settings::load(settings_path.to_str().unwrap()).unwrap();
    assert_eq!(updated.theme.mode, ThemeMode::Light);
    assert!(!state.dirty);
}

#[test]
fn reload_reflects_latest_saved_values() {
    let dir = tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    let mut settings = Settings::default();
    settings.theme.mode = ThemeMode::Dark;
    settings.save(settings_path.to_str().unwrap()).unwrap();

    let mut state = ThemeSettingsDialogState::default();
    state.reload_from_path(settings_path.to_str().unwrap());
    assert_eq!(state.draft.mode, ThemeMode::Dark);

    settings.theme.mode = ThemeMode::Custom;
    settings.save(settings_path.to_str().unwrap()).unwrap();

    state.request_reload();
    state.reload_from_path(settings_path.to_str().unwrap());
    assert_eq!(state.draft.mode, ThemeMode::Custom);
}

#[test]
fn save_writes_theme_json_file() {
    let dir = tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    let settings = Settings::default();
    settings.save(settings_path.to_str().unwrap()).unwrap();

    let mut state = ThemeSettingsDialogState::default();
    state.reload_from_path(settings_path.to_str().unwrap());
    state.draft.mode = ThemeMode::Light;
    state.dirty = true;

    state.save_to_path(settings_path.to_str().unwrap()).unwrap();

    let theme_file = dir.path().join("theme_settings.json");
    assert!(theme_file.exists());
    let content = std::fs::read_to_string(theme_file).unwrap();
    let theme: ThemeSettings = serde_json::from_str(&content).unwrap();
    assert_eq!(theme.mode, ThemeMode::Light);
}
