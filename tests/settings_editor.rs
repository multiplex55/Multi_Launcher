use eframe::egui;
use multi_launcher::actions::Action;
use multi_launcher::plugin::PluginManager;
use multi_launcher::plugins::omni_search::OmniSearchSettings;
use multi_launcher::settings::Settings;
use multi_launcher::settings_editor::SettingsEditor;
use serde_json::json;
use std::sync::Arc;

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

#[test]
fn omni_plugin_settings_round_trip_preserves_all_fields() {
    let mut settings = Settings::default();
    settings.plugin_settings.insert(
        "omni_search".into(),
        json!({
            "include_apps": false,
            "include_notes": true,
            "include_todos": false,
            "include_calendar": true,
            "include_folders": false,
            "include_bookmarks": true,
        }),
    );

    let editor = SettingsEditor::new(&settings);
    let new_settings = editor.to_settings(&settings);
    let omni: OmniSearchSettings = serde_json::from_value(
        new_settings
            .plugin_settings
            .get("omni_search")
            .cloned()
            .expect("omni_search settings should exist"),
    )
    .expect("omni_search settings should deserialize");

    assert!(!omni.include_apps);
    assert!(omni.include_notes);
    assert!(!omni.include_todos);
    assert!(omni.include_calendar);
    assert!(!omni.include_folders);
    assert!(omni.include_bookmarks);
}

#[test]
fn omni_settings_missing_fields_fall_back_to_defaults() {
    let parsed: OmniSearchSettings =
        serde_json::from_value(json!({"include_apps": false})).expect("partial settings parse");

    assert!(!parsed.include_apps);
    assert!(parsed.include_notes);
    assert!(parsed.include_todos);
    assert!(parsed.include_calendar);
    assert!(parsed.include_folders);
    assert!(parsed.include_bookmarks);
}

#[test]
fn omni_settings_from_editor_are_persisted_and_applied_on_reload() {
    let ctx = egui::Context::default();
    let actions = Arc::new(vec![Action {
        label: "plan app".into(),
        desc: "launcher".into(),
        action: "app:plan".into(),
        args: None,
    }]);

    let initial = Settings::default();
    let mut editor = SettingsEditor::new(&initial);
    editor.set_plugin_setting_value(
        "omni_search",
        json!({
            "include_apps": true,
            "include_notes": true,
            "include_todos": false,
            "include_calendar": false,
            "include_folders": true,
            "include_bookmarks": true,
        }),
    );

    let saved = editor.to_settings(&initial);
    let mut plugins = PluginManager::new();
    plugins.reload_from_dirs(
        &Vec::new(),
        saved.clipboard_limit,
        saved.net_unit,
        false,
        &saved.plugin_settings,
        Arc::clone(&actions),
    );

    let results = plugins.search_filtered("o list", None, None);
    assert!(results.iter().any(|a| a.action == "app:plan"));
    assert!(!results.iter().any(|a| a.action == "todo:done:0"));
    assert!(!results.iter().any(|a| a.action == "calendar:upcoming"));

    // Keep context used in this test alive and considered initialized for egui headless runs.
    let _ = ctx;
}
