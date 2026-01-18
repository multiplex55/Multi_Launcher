use multi_launcher::plugins::mouse_gestures::db::{
    load_gestures, save_gestures, MouseGestureDb, MOUSE_GESTURES_FILE,
};
use multi_launcher::plugins::mouse_gestures::settings::MouseGesturePluginSettings;
use std::fs;

#[test]
fn mouse_gesture_settings_serialize_defaults() {
    let settings = MouseGesturePluginSettings::default();
    let value = serde_json::to_value(&settings).expect("serialize settings");
    let obj = value.as_object().expect("settings as object");
    assert_eq!(obj.get("enabled"), Some(&serde_json::Value::Bool(true)));
    assert_eq!(
        obj.get("triggerButton"),
        Some(&serde_json::Value::String("right".to_string()))
    );
    assert!(obj.contains_key("minTrackLen"));
    assert!(obj.contains_key("maxDistance"));
    let overlay = obj
        .get("overlay")
        .and_then(|v| v.as_object())
        .expect("overlay object");
    assert!(overlay.contains_key("color"));
    assert!(overlay.contains_key("thickness"));
    assert!(overlay.contains_key("fade"));
    assert!(obj.contains_key("noMatchAction"));
    assert!(obj.contains_key("smoothingEnabled"));
    assert!(obj.contains_key("samplingEnabled"));
}

#[test]
fn mouse_gesture_db_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(MOUSE_GESTURES_FILE);
    let mut db = MouseGestureDb::default();
    db.gestures = vec!["up".to_string(), "down".to_string()];
    save_gestures(path.to_str().expect("path"), &db).expect("save");
    let loaded = load_gestures(path.to_str().expect("path")).expect("load");
    assert_eq!(db, loaded);
}

#[test]
fn mouse_gesture_db_handles_empty_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("empty.json");
    fs::write(&path, " ").expect("write empty");
    let loaded = load_gestures(path.to_str().expect("path")).expect("load");
    assert_eq!(loaded, MouseGestureDb::default());
}

#[test]
fn mouse_gesture_db_schema_mismatch_falls_back() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("legacy.json");
    let legacy = serde_json::json!({
        "schema_version": 99,
        "gestures": ["legacy"],
        "profiles": [],
        "bindings": {}
    });
    fs::write(&path, serde_json::to_string_pretty(&legacy).unwrap()).expect("write");
    let loaded = load_gestures(path.to_str().expect("path")).expect("load");
    assert_eq!(loaded, MouseGestureDb::default());
}
