use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::mouse_gestures::{MouseGestureSettings, MouseGesturesPlugin};

#[test]
fn mouse_gestures_commands_match_expected_labels() {
    let plugin = MouseGesturesPlugin::default();
    let actions = plugin.commands();
    let labels: Vec<_> = actions.iter().map(|a| a.label.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "mg",
            "mg settings",
            "mg edit",
            "mg add",
            "mg list",
            "mg find",
            "mg where",
            "mg conflicts"
        ]
    );
    let action_strings: Vec<_> = actions.iter().map(|a| a.action.as_str()).collect();
    assert_eq!(
        action_strings,
        vec![
            "query:mg ",
            "mg:dialog:settings",
            "mg:dialog",
            "mg:dialog:binding",
            "query:mg list",
            "query:mg find ",
            "query:mg where ",
            "query:mg conflicts",
        ]
    );
}

#[test]
fn mouse_gestures_default_settings_round_trip() {
    let plugin = MouseGesturesPlugin::default();
    let value = plugin.default_settings().expect("default settings");
    let parsed: MouseGestureSettings =
        serde_json::from_value(value.clone()).expect("deserialize mouse gesture settings");
    let serialized = serde_json::to_value(&parsed).expect("serialize mouse gesture settings");
    assert_eq!(value, serialized);
    assert!(value.get("min_distance_px").is_none());
    assert!(value.get("max_duration_ms").is_none());
    let defaults = MouseGestureSettings::default();
    assert_eq!(parsed, defaults);

    let mut custom_value = value.clone();
    custom_value["ignore_window_titles"] = serde_json::json!(["Notepad", "Firefox"]);
    let custom_parsed: MouseGestureSettings =
        serde_json::from_value(custom_value.clone()).expect("deserialize custom settings");
    let custom_serialized =
        serde_json::to_value(&custom_parsed).expect("serialize custom settings");
    assert_eq!(custom_value, custom_serialized);
}

#[test]
fn mouse_gestures_settings_ignore_legacy_fields() {
    let value = serde_json::json!({
        "enabled": true,
        "require_button": false,
        "debug_logging": true,
        "show_trail": true,
        "trail_color": [255, 0, 0, 255],
        "trail_width": 2.0,
        "trail_start_move_px": 8.0,
        "show_hint": true,
        "hint_offset": [16.0, 16.0],
        "cancel_behavior": "do_nothing",
        "no_match_behavior": "do_nothing",
        "min_distance_px": 20.0,
        "max_duration_ms": 5000
    });
    let parsed: MouseGestureSettings =
        serde_json::from_value(value).expect("deserialize legacy settings");
    assert!(parsed.enabled);
    assert!(parsed.debug_logging);
}
