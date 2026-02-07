use eframe::egui;
use multi_launcher::actions::{save_actions, Action};
use multi_launcher::gui::{
    send_event, set_execute_action_hook, ActivationSource, LauncherApp, WatchEvent,
};
use multi_launcher::mouse_gestures::db::{
    load_gestures, save_gestures, BindingEntry, BindingKind, BindingMatchField, GestureCandidate,
    GestureConflict, GestureConflictKind, GestureDb, GestureEntry, GestureMatchType,
    SCHEMA_VERSION,
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
            stroke: Vec::new(),
            enabled: true,
            bindings: vec![BindingEntry {
                label: "Launch".into(),
                kind: BindingKind::Execute,
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
fn gesture_db_migrates_legacy_schema() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("mouse_gestures.json");
    std::fs::write(
        &path,
        r#"{
  "schema_version": 1,
  "gestures": [
    {
      "label": "Legacy",
      "tokens": "LR",
      "dir_mode": "Four",
      "stroke": [],
      "enabled": true,
      "bindings": [
        {
          "label": "Query",
          "action": "query:calc",
          "args": null,
          "enabled": true
        },
        {
          "label": "UseQuery",
          "action": "note list",
          "args": null,
          "enabled": true,
          "use_query": true
        },
        {
          "label": "Toggle",
          "action": "launcher:toggle",
          "args": null,
          "enabled": true
        }
      ]
    }
  ]
}"#,
    )
    .unwrap();

    let loaded = load_gestures(path.to_str().unwrap()).unwrap();
    assert_eq!(loaded.schema_version, SCHEMA_VERSION);
    let bindings = &loaded.gestures[0].bindings;
    assert_eq!(bindings[0].kind, BindingKind::SetQuery);
    assert_eq!(bindings[0].action, "calc");
    assert_eq!(bindings[1].kind, BindingKind::SetQuery);
    assert_eq!(bindings[1].action, "note list");
    assert_eq!(bindings[2].kind, BindingKind::ToggleLauncher);
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
                stroke: Vec::new(),
                enabled: false,
                bindings: vec![BindingEntry {
                    label: "Launch".into(),
                    kind: BindingKind::Execute,
                    action: "stopwatch:show:1".into(),
                    args: None,
                    enabled: true,
                }],
            },
            GestureEntry {
                label: "Disabled binding".into(),
                tokens: "UD".into(),
                dir_mode: DirMode::Four,
                stroke: Vec::new(),
                enabled: true,
                bindings: vec![BindingEntry {
                    label: "Launch".into(),
                    kind: BindingKind::Execute,
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
                stroke: Vec::new(),
                enabled: true,
                bindings: vec![
                    BindingEntry {
                        label: "Primary".into(),
                        kind: BindingKind::Execute,
                        action: "stopwatch:show:1".into(),
                        args: None,
                        enabled: true,
                    },
                    BindingEntry {
                        label: "Secondary".into(),
                        kind: BindingKind::Execute,
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
                stroke: Vec::new(),
                enabled: true,
                bindings: vec![BindingEntry {
                    label: "Tertiary".into(),
                    kind: BindingKind::Execute,
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
fn binding_enabled_state_persists_and_controls_matching() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("mouse_gestures.json");
    let mut db = GestureDb {
        schema_version: SCHEMA_VERSION,
        gestures: vec![GestureEntry {
            label: "Toggle".into(),
            tokens: "LR".into(),
            dir_mode: DirMode::Four,
            stroke: Vec::new(),
            enabled: true,
            bindings: vec![BindingEntry {
                label: "Launch".into(),
                kind: BindingKind::Execute,
                action: "stopwatch:show:1".into(),
                args: None,
                enabled: false,
            }],
        }],
    };

    save_gestures(path.to_str().unwrap(), &db).unwrap();
    let loaded = load_gestures(path.to_str().unwrap()).unwrap();
    assert!(!loaded.gestures[0].bindings[0].enabled);
    assert!(loaded.match_binding("LR", DirMode::Four).is_none());

    db.gestures[0].bindings[0].enabled = true;
    save_gestures(path.to_str().unwrap(), &db).unwrap();
    let loaded_enabled = load_gestures(path.to_str().unwrap()).unwrap();
    assert!(loaded_enabled.match_binding("LR", DirMode::Four).is_some());
}

#[test]
fn candidate_matching_ranks_exact_over_prefix_over_fuzzy() {
    let db = GestureDb {
        schema_version: SCHEMA_VERSION,
        gestures: vec![
            GestureEntry {
                label: "Exact".into(),
                tokens: "L".into(),
                dir_mode: DirMode::Four,
                stroke: Vec::new(),
                enabled: true,
                bindings: vec![BindingEntry {
                    label: "Exact bind".into(),
                    kind: BindingKind::Execute,
                    action: "stopwatch:show:1".into(),
                    args: None,
                    enabled: true,
                }],
            },
            GestureEntry {
                label: "Prefix".into(),
                tokens: "LR".into(),
                dir_mode: DirMode::Four,
                stroke: Vec::new(),
                enabled: true,
                bindings: vec![BindingEntry {
                    label: "Prefix bind".into(),
                    kind: BindingKind::Execute,
                    action: "stopwatch:show:2".into(),
                    args: None,
                    enabled: true,
                }],
            },
            GestureEntry {
                label: "Fuzzy".into(),
                tokens: "UL".into(),
                dir_mode: DirMode::Four,
                stroke: Vec::new(),
                enabled: true,
                bindings: vec![BindingEntry {
                    label: "Fuzzy bind".into(),
                    kind: BindingKind::Execute,
                    action: "stopwatch:show:3".into(),
                    args: None,
                    enabled: true,
                }],
            },
        ],
    };

    let candidates = db.candidate_matches("L", DirMode::Four);
    assert_eq!(candidates.len(), 3);
    assert_match_type(&candidates[0], GestureMatchType::Exact, "Exact bind");
    assert_match_type(&candidates[1], GestureMatchType::Prefix, "Prefix bind");
    assert_match_type(&candidates[2], GestureMatchType::Fuzzy, "Fuzzy bind");
}

#[test]
fn search_bindings_matches_across_fields() {
    let db = GestureDb {
        schema_version: SCHEMA_VERSION,
        gestures: vec![GestureEntry {
            label: "Open Browser".into(),
            tokens: "UR".into(),
            dir_mode: DirMode::Four,
            stroke: Vec::new(),
            enabled: true,
            bindings: vec![BindingEntry {
                label: "Primary".into(),
                kind: BindingKind::Execute,
                action: "browser:open".into(),
                args: Some("profile=work".into()),
                enabled: true,
            }],
        }],
    };

    let results = db.search_bindings("browser");
    assert_eq!(results.len(), 1);
    let (gesture, binding, context) = &results[0];
    assert_eq!(gesture.label, "Open Browser");
    assert_eq!(binding.label, "Primary");
    assert!(context.fields.contains(&BindingMatchField::GestureLabel));
    assert!(context.fields.contains(&BindingMatchField::Action));

    let token_results = db.search_bindings("UR");
    assert_eq!(token_results.len(), 1);
    assert!(token_results[0]
        .2
        .fields
        .contains(&BindingMatchField::Tokens));

    let args_results = db.search_bindings("work");
    assert_eq!(args_results.len(), 1);
    assert!(args_results[0].2.fields.contains(&BindingMatchField::Args));
}

#[test]
fn find_by_action_matches_prefixes() {
    let db = GestureDb {
        schema_version: SCHEMA_VERSION,
        gestures: vec![GestureEntry {
            label: "Open Browser".into(),
            tokens: "UR".into(),
            dir_mode: DirMode::Four,
            stroke: Vec::new(),
            enabled: true,
            bindings: vec![BindingEntry {
                label: "Primary".into(),
                kind: BindingKind::Execute,
                action: "browser:open".into(),
                args: None,
                enabled: true,
            }],
        }],
    };

    let matches = db.find_by_action("browser");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].0.label, "Open Browser");
    assert_eq!(matches[0].1.label, "Primary");
}

#[test]
fn find_conflicts_groups_duplicates_and_prefixes() {
    let db = GestureDb {
        schema_version: SCHEMA_VERSION,
        gestures: vec![
            GestureEntry {
                label: "Open Browser".into(),
                tokens: "UR".into(),
                dir_mode: DirMode::Four,
                stroke: Vec::new(),
                enabled: true,
                bindings: vec![BindingEntry {
                    label: "Primary".into(),
                    kind: BindingKind::Execute,
                    action: "browser:open".into(),
                    args: None,
                    enabled: true,
                }],
            },
            GestureEntry {
                label: "Open Mail".into(),
                tokens: "UR".into(),
                dir_mode: DirMode::Four,
                stroke: Vec::new(),
                enabled: true,
                bindings: vec![BindingEntry {
                    label: "Secondary".into(),
                    kind: BindingKind::Execute,
                    action: "mail:open".into(),
                    args: None,
                    enabled: true,
                }],
            },
            GestureEntry {
                label: "Open Settings".into(),
                tokens: "URD".into(),
                dir_mode: DirMode::Four,
                stroke: Vec::new(),
                enabled: true,
                bindings: vec![BindingEntry {
                    label: "Tertiary".into(),
                    kind: BindingKind::Execute,
                    action: "settings:open".into(),
                    args: None,
                    enabled: true,
                }],
            },
            GestureEntry {
                label: "Eight Way".into(),
                tokens: "UR".into(),
                dir_mode: DirMode::Eight,
                stroke: Vec::new(),
                enabled: true,
                bindings: vec![BindingEntry {
                    label: "Alt".into(),
                    kind: BindingKind::Execute,
                    action: "other:open".into(),
                    args: None,
                    enabled: true,
                }],
            },
        ],
    };

    let conflicts = db.find_conflicts();
    let duplicate = conflicts.iter().find(|conflict| {
        conflict.kind == GestureConflictKind::DuplicateTokens
            && conflict.tokens == "UR"
            && conflict.dir_mode == DirMode::Four
    });
    let duplicate = duplicate.expect("duplicate token conflict");
    assert_eq!(duplicate.gestures.len(), 2);

    let prefix = conflicts.iter().find(|conflict| {
        conflict.kind == GestureConflictKind::PrefixOverlap
            && conflict.tokens == "UR"
            && conflict.dir_mode == DirMode::Four
    });
    let prefix = prefix.expect("prefix conflict");
    assert_eq!(prefix.gestures.len(), 3);

    assert!(conflicts.iter().all(|conflict| match conflict {
        GestureConflict {
            dir_mode: DirMode::Eight,
            ..
        } => false,
        _ => true,
    }));
}

fn assert_match_type(
    candidate: &GestureCandidate,
    match_type: GestureMatchType,
    binding_label: &str,
) {
    assert_eq!(candidate.match_type, match_type);
    assert_eq!(candidate.bindings.len(), 1);
    assert_eq!(candidate.bindings[0].label, binding_label);
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

    app.query = "before".into();
    send_event(WatchEvent::ExecuteAction(Action {
        label: "Test".into(),
        desc: "".into(),
        action: "query:after".into(),
        args: None,
        preview_text: None,
        risk_level: None,
        icon: None,
    }));
    app.process_watch_events();
    assert_eq!(app.query, "after");
    assert!(app.move_cursor_end_flag());
}

#[test]
fn set_query_binding_avoids_execute_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let executed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let executed_hook = Arc::clone(&executed);
    set_execute_action_hook(Some(Box::new(move |_| {
        executed_hook.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })));

    let ctx = egui::Context::default();
    let mut app = new_app(&ctx, Vec::new());
    let binding = BindingEntry {
        label: "Query".into(),
        kind: BindingKind::SetQuery,
        action: "timer list".into(),
        args: None,
        enabled: true,
    };
    let action = binding.to_action("Gesture");
    assert_eq!(action.action, "query:timer list");
    app.activate_action(action, None, ActivationSource::Gesture);

    assert_eq!(executed.load(Ordering::SeqCst), 0);
    set_execute_action_hook(None);
}
