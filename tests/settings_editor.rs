use multi_launcher::settings::Settings;
use multi_launcher::settings_editor::SettingsEditor;

#[test]
fn new_handles_none_default_hotkey() {
    std::env::set_var("ML_DEFAULT_HOTKEY_NONE", "1");
    let settings = Settings::default();
    assert!(settings.hotkey.is_none());
    assert!(std::panic::catch_unwind(|| SettingsEditor::new(&settings)).is_ok());
    std::env::remove_var("ML_DEFAULT_HOTKEY_NONE");
}

#[test]
fn always_on_top_persists() {
    let settings = Settings::default();
    let mut editor = SettingsEditor::new(&settings);
    assert!(editor.always_on_top);
    editor.always_on_top = false;
    let new_settings = editor.to_settings(&settings);
    assert!(!new_settings.always_on_top);
}
