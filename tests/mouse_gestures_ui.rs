use eframe::egui::Pos2;
use multi_launcher::gui::{GestureRecorder, RecorderConfig};
use multi_launcher::mouse_gestures::db::{
    format_gesture_label, load_gestures, save_gestures, BindingEntry, BindingKind, GestureDb,
    GestureEntry, SCHEMA_VERSION,
};
use multi_launcher::mouse_gestures::engine::DirMode;
use tempfile::tempdir;

#[test]
fn gesture_label_formatting_includes_tokens_and_bindings() {
    let gesture = GestureEntry {
        label: "Back".into(),
        tokens: "LR".into(),
        dir_mode: DirMode::Four,
        stroke: Vec::new(),
        enabled: true,
        bindings: vec![
            BindingEntry {
                label: "Browser back".into(),
                kind: BindingKind::Execute,
                action: "app:back".into(),
                args: None,
                enabled: true,
            },
            BindingEntry {
                label: "Disabled action".into(),
                kind: BindingKind::Execute,
                action: "app:noop".into(),
                args: None,
                enabled: false,
            },
        ],
    };

    assert_eq!(
        format_gesture_label(&gesture),
        "Back [LR] → Browser back, Disabled action (disabled)"
    );
}

#[test]
fn recorder_uses_shared_gesture_tracker_for_tokens() {
    let config = RecorderConfig::new(1.0, 1.0, 1.0, 1);
    let tracker = config.tracker(DirMode::Four);
    let mut recorder = GestureRecorder::with_tracker(tracker, config);
    recorder.push_point(Pos2::new(0.0, 0.0));
    recorder.push_point(Pos2::new(10.0, 0.0));
    recorder.push_point(Pos2::new(10.0, 10.0));
    assert_eq!(recorder.tokens_string(), "R");
}

#[test]
fn binding_order_changes_persist_after_save_load() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("mouse_gestures.json");
    let mut db = GestureDb {
        schema_version: SCHEMA_VERSION,
        gestures: vec![GestureEntry {
            label: "Order Test".into(),
            tokens: "LR".into(),
            dir_mode: DirMode::Four,
            stroke: Vec::new(),
            enabled: true,
            bindings: vec![
                BindingEntry {
                    label: "First".into(),
                    kind: BindingKind::Execute,
                    action: "app:first".into(),
                    args: None,
                    enabled: true,
                },
                BindingEntry {
                    label: "Second".into(),
                    kind: BindingKind::Execute,
                    action: "app:second".into(),
                    args: None,
                    enabled: true,
                },
            ],
        }],
    };

    db.gestures[0].bindings.swap(0, 1);
    save_gestures(path.to_str().unwrap(), &db).unwrap();
    let loaded = load_gestures(path.to_str().unwrap()).unwrap();

    let bindings = &loaded.gestures[0].bindings;
    assert_eq!(bindings[0].label, "Second");
    assert_eq!(bindings[1].label, "First");
}
