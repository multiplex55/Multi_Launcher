use multi_launcher::plugins::clipboard_modify::migrate_enablement;
use multi_launcher::settings::{ClipboardModifyPluginSettings, Settings};
use std::collections::HashSet;

#[test]
fn migration_inserts_default_settings_without_changing_default_enablement() {
    let mut settings = Settings::default();

    assert!(migrate_enablement(&mut settings));
    assert!(settings.enabled_plugins.is_none());
    let stored: ClipboardModifyPluginSettings =
        serde_json::from_value(settings.plugin_settings["clipboard_modify"].clone()).unwrap();
    assert_eq!(stored, ClipboardModifyPluginSettings::default());
}

#[test]
fn migration_adds_clipboard_modify_to_an_explicit_enabled_set() {
    let mut settings = Settings {
        enabled_plugins: Some(HashSet::from(["calculator".to_owned()])),
        ..Settings::default()
    };

    assert!(migrate_enablement(&mut settings));
    assert!(
        settings
            .enabled_plugins
            .unwrap()
            .contains("clipboard_modify")
    );
}

#[test]
fn migration_does_not_reenable_after_the_marker_settings_exist() {
    let mut settings = Settings {
        enabled_plugins: Some(HashSet::from(["calculator".to_owned()])),
        ..Settings::default()
    };
    settings.plugin_settings.insert(
        "clipboard_modify".into(),
        serde_json::to_value(ClipboardModifyPluginSettings::default()).unwrap(),
    );

    assert!(!migrate_enablement(&mut settings));
    assert!(
        !settings
            .enabled_plugins
            .unwrap()
            .contains("clipboard_modify")
    );
}

#[test]
fn serialized_plugin_settings_contain_only_presentation_preferences() {
    let value = serde_json::to_value(ClipboardModifyPluginSettings::default()).unwrap();
    let object = value.as_object().unwrap();

    for sensitive in [
        "clipboard",
        "source",
        "preview",
        "undo",
        "templates",
        "pipelines",
    ] {
        assert!(!object.contains_key(sensitive));
    }
    assert!(object.contains_key("dialog_width"));
    assert!(object.contains_key("source_preview_split_ratio"));
}
