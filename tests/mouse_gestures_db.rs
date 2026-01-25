use eframe::egui;
use multi_launcher::actions::{save_actions, Action};
use multi_launcher::gui::{
    send_event, set_execute_action_hook, LauncherApp, WatchEvent, EXECUTE_ACTION_COUNT,
};
use multi_launcher::mouse_gestures::db::{
    load_gestures, save_gestures, BindingEntry, GestureDb, GestureEntry, SCHEMA_VERSION,
};
use multi_launcher::mouse_gestures::engine::DirMode;
use multi_launcher::plugin::PluginManager;
use multi_launcher::plugins::bookmarks::{save_bookmarks, BOOKMARKS_FILE};
use multi_launcher::plugins::folders::{save_folders, FOLDERS_FILE};
use multi_launcher::settings::Settings;
use once_cell::sync::Lazy;
use std::sync::{atomic::Ordering, Arc, Mutex};
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn new_app(ctx: &egui::Context, actions: Vec<Action>) -> LauncherApp {
    let custom_len = actions.len();
    LauncherApp::new(
        ctx,
        Arc::new(actions),
        custom_len,
        PluginManager::new(),
        "actions.json".into(),
        "settings.json".into(),
        Settings::default(),
        None,
        None,
        None,
        None,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
}

#[test]
fn gesture_db_round_trip_serialization() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("mouse_gestures.json");
    let db = GestureDb {
        schema_version: SCHEMA_VERSION,
        gestures: vec![GestureEntry {
            label: "Test".into(),
            tokens: "LR".into(),
            dir_mode: DirMode::Four,
            enabled: true,
            bindings: vec![BindingEntry {
                label: "Launch".into(),
                action: "stopwatch:show:1".into(),
                args: None,
                enabled: true,
            }],
        }],
    };

    save_gestures(path.to_str().unwrap(), &db).unwrap();
    let loaded = load_gestures(path.to_str().unwrap()).unwrap();

    assert_eq!(db, loaded);
}

#[test]
fn gesture_db_rejects_unknown_schema_version() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("mouse_gestures.json");
    std::fs::write(
        &path,
        format!(
            "{{\"schema_version\":{},\"gestures\":[]}}",
            SCHEMA_VERSION + 1
        ),
    )
    .unwrap();

    let err = load_gestures(path.to_str().unwrap()).unwrap_err();
    assert!(err.to_string().contains("schema version"));
}

#[test]
fn matching_skips_disabled_gestures_and_bindings() {
    let db = GestureDb {
        schema_version: SCHEMA_VERSION,
        gestures: vec![
            GestureEntry {
                label: "Disabled gesture".into(),
                tokens: "LR".into(),
                dir_mode: DirMode::Four,
                enabled: false,
                bindings: vec![BindingEntry {
                    label: "Launch".into(),
                    action: "stopwatch:show:1".into(),
                    args: None,
                    enabled: true,
                }],
            },
            GestureEntry {
                label: "Disabled binding".into(),
                tokens: "UD".into(),
                dir_mode: DirMode::Four,
                enabled: true,
                bindings: vec![BindingEntry {
                    label: "Launch".into(),
                    action: "stopwatch:show:2".into(),
                    args: None,
                    enabled: false,
                }],
            },
        ],
    };

    assert!(db.match_binding("LR", DirMode::Four).is_none());
    assert!(db.match_binding("UD", DirMode::Four).is_none());
}

#[test]
fn binding_resolution_is_deterministic() {
    let db = GestureDb {
        schema_version: SCHEMA_VERSION,
        gestures: vec![
            GestureEntry {
                label: "First".into(),
                tokens: "LR".into(),
                dir_mode: DirMode::Four,
                enabled: true,
                bindings: vec![
                    BindingEntry {
                        label: "Primary".into(),
                        action: "stopwatch:show:1".into(),
                        args: None,
                        enabled: true,
                    },
                    BindingEntry {
                        label: "Secondary".into(),
                        action: "stopwatch:show:2".into(),
                        args: None,
                        enabled: true,
                    },
                ],
            },
            GestureEntry {
                label: "Second".into(),
                tokens: "LR".into(),
                dir_mode: DirMode::Four,
                enabled: true,
                bindings: vec![BindingEntry {
                    label: "Tertiary".into(),
                    action: "stopwatch:show:3".into(),
                    args: None,
                    enabled: true,
                }],
            },
        ],
    };

    let (gesture, binding) = db.match_binding("LR", DirMode::Four).unwrap();
    assert_eq!(gesture.label, "First");
    assert_eq!(binding.label, "Primary");
}

#[test]
fn watch_event_executes_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    save_actions("actions.json", &[]).unwrap();
    save_folders(FOLDERS_FILE, &[]).unwrap();
    save_bookmarks(BOOKMARKS_FILE, &[]).unwrap();

    let ctx = egui::Context::default();
    let mut app = new_app(&ctx, Vec::new());

    EXECUTE_ACTION_COUNT.store(0, Ordering::SeqCst);
    set_execute_action_hook(Some(Box::new(|_action| {
        EXECUTE_ACTION_COUNT.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })));
    send_event(WatchEvent::ExecuteAction(Action {
        label: "Test".into(),
        desc: "".into(),
        action: "stopwatch:show:1".into(),
        args: None,
    }));
    app.process_watch_events();
    set_execute_action_hook(None);

    assert_eq!(EXECUTE_ACTION_COUNT.load(Ordering::SeqCst), 1);
}
