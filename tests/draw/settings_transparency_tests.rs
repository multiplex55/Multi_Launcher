use multi_launcher::draw::settings::{DrawColor, DrawSettings, TransparencyMethod};

#[test]
fn transparency_method_defaults_to_colorkey() {
    let settings = DrawSettings::default();
    assert_eq!(settings.transparency_method, TransparencyMethod::Colorkey);
}

#[test]
fn transparency_method_parses_alpha_and_preserves_colorkey_color_values() {
    let mut settings: DrawSettings = serde_json::from_value(serde_json::json!({
        "transparency_method": "alpha",
        "last_color": {"r": 255, "g": 0, "b": 255, "a": 255},
        "default_outline_color": {"r": 255, "g": 0, "b": 255, "a": 255},
        "quick_colors": [{"r": 255, "g": 0, "b": 255, "a": 255}],
    }))
    .expect("deserialize draw settings");

    assert_eq!(settings.transparency_method, TransparencyMethod::Alpha);
    let changed = settings.sanitize_for_configured_transparency();
    assert!(!changed);
    assert_eq!(settings.last_color, DrawColor::rgba(255, 0, 255, 255));
}

#[test]
fn configured_sanitization_applies_only_in_colorkey_mode() {
    let mut settings = DrawSettings::default();
    settings.transparency_method = TransparencyMethod::Colorkey;
    settings.last_color = DrawColor::rgba(255, 0, 255, 255);

    let changed = settings.sanitize_for_configured_transparency();

    assert!(changed);
    assert_eq!(settings.last_color, DrawColor::rgba(254, 0, 255, 255));
}
