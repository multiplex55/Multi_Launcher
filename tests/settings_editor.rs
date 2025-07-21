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
