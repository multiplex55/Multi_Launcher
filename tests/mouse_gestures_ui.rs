use eframe::egui::Pos2;
use multi_launcher::gui::{GestureRecorder, RecorderConfig};
use multi_launcher::mouse_gestures::db::{format_gesture_label, BindingEntry, GestureEntry};
use multi_launcher::mouse_gestures::engine::DirMode;

#[test]
fn gesture_label_formatting_includes_tokens_and_bindings() {
    let gesture = GestureEntry {
        label: "Back".into(),
        tokens: "LR".into(),
        dir_mode: DirMode::Four,
        enabled: true,
        bindings: vec![
            BindingEntry {
                label: "Browser back".into(),
                action: "app:back".into(),
                args: None,
                enabled: true,
            },
            BindingEntry {
                label: "Disabled action".into(),
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
