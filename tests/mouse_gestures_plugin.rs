use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::mouse_gestures::{MouseGestureSettings, MouseGesturesPlugin};

#[test]
fn mouse_gestures_commands_match_expected_labels() {
    let plugin = MouseGesturesPlugin::default();
    let actions = plugin.commands();
    let labels: Vec<_> = actions.iter().map(|a| a.label.as_str()).collect();
    assert_eq!(
        labels,
        vec!["mg", "mg settings", "mg edit", "mg add", "mg list"]
    );
    let action_strings: Vec<_> = actions.iter().map(|a| a.action.as_str()).collect();
    assert_eq!(
        action_strings,
        vec![
            "query:mg ",
            "settings:dialog",
            "mg:dialog",
            "mg:dialog:binding",
            "query:mg list",
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
    let defaults = MouseGestureSettings::default();
    assert_eq!(parsed, defaults);
}
