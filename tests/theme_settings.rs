use multi_launcher::settings::{ColorScheme, Settings, ThemeMode, ThemeSettings};

#[test]
fn theme_settings_round_trip_serialization() {
    let theme = ThemeSettings {
        mode: ThemeMode::Custom,
        named_presets: std::collections::HashMap::from([(
            "sunset".to_string(),
            ColorScheme::light(),
        )]),
        custom_scheme: ColorScheme::dark(),
    };

    let json = serde_json::to_string(&theme).expect("theme settings should serialize");
    let parsed: ThemeSettings =
        serde_json::from_str(&json).expect("theme settings should deserialize");

    assert_eq!(parsed.mode, ThemeMode::Custom);
    assert!(parsed.named_presets.contains_key("sunset"));
    assert_eq!(
        parsed.custom_scheme.selection_stroke,
        theme.custom_scheme.selection_stroke
    );
}

#[test]
fn legacy_settings_without_theme_use_defaults() {
    let legacy_json = r#"{
        "hotkey":"F2",
        "clipboard_limit":99
    }"#;

    let settings: Settings =
        serde_json::from_str(legacy_json).expect("legacy settings should deserialize");

    assert_eq!(settings.clipboard_limit, 99);
    assert_eq!(settings.theme.mode, ThemeMode::System);
    assert!(settings.theme.named_presets.contains_key("dark"));
    assert!(settings.theme.named_presets.contains_key("light"));
}

#[test]
fn partial_theme_json_uses_defaults_for_missing_fields() {
    let partial = r#"{
        "mode":"light",
        "custom_scheme": {
            "window_fill": {"r": 12, "g": 34, "b": 56}
        }
    }"#;

    let theme: ThemeSettings =
        serde_json::from_str(partial).expect("partial theme should deserialize");

    assert_eq!(theme.mode, ThemeMode::Light);
    assert_eq!(theme.custom_scheme.window_fill.r, 12);
    assert_eq!(theme.custom_scheme.window_fill.g, 34);
    assert_eq!(theme.custom_scheme.window_fill.b, 56);
    assert_eq!(theme.custom_scheme.window_fill.a, 255);
    assert_eq!(theme.custom_scheme.panel_fill, Default::default());
}

#[test]
fn invalid_theme_mode_is_an_error() {
    let invalid = r#"{"mode":"neon"}"#;
    let result = serde_json::from_str::<ThemeSettings>(invalid);

    assert!(result.is_err());
}
