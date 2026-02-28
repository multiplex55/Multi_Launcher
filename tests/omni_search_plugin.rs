use chrono::{Local, NaiveDate};
use multi_launcher::actions::Action;
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::bookmarks::{save_bookmarks, BookmarkEntry, BOOKMARKS_FILE};
use multi_launcher::plugins::calendar::{save_events, CalendarEvent, CALENDAR_EVENTS_FILE};
use multi_launcher::plugins::folders::{save_folders, FolderEntry, FOLDERS_FILE};
use multi_launcher::plugins::note::{save_notes, Note};
use multi_launcher::plugins::omni_search::{OmniSearchPlugin, OmniSearchSettings};
use multi_launcher::plugins::todo::{save_todos, TodoEntry, TODO_FILE};
use once_cell::sync::Lazy;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

struct EnvGuard {
    cwd: PathBuf,
    notes_dir: Option<String>,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.cwd);
        if let Some(path) = &self.notes_dir {
            std::env::set_var("ML_NOTES_DIR", path);
        } else {
            std::env::remove_var("ML_NOTES_DIR");
        }
    }
}

fn setup_fixture() -> (tempfile::TempDir, EnvGuard) {
    let cwd = std::env::current_dir().unwrap();
    let notes_dir = std::env::var("ML_NOTES_DIR").ok();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    std::env::set_var("ML_NOTES_DIR", dir.path().join("notes"));

    save_bookmarks(
        BOOKMARKS_FILE,
        &[BookmarkEntry {
            url: "https://plan.example.com".into(),
            alias: Some("Plan Bookmark".into()),
        }],
    )
    .unwrap();
    save_folders(
        FOLDERS_FILE,
        &[FolderEntry {
            label: "Plan Folder".into(),
            path: "/workspace/plan".into(),
            alias: None,
        }],
    )
    .unwrap();

    save_notes(&[Note {
        title: "Project Plan".into(),
        path: PathBuf::new(),
        content: "# Project Plan\n\noutline".into(),
        tags: Vec::new(),
        links: Vec::new(),
        slug: "project-plan".into(),
        alias: None,
        entity_refs: Vec::new(),
    }])
    .unwrap();

    save_todos(
        TODO_FILE,
        &[TodoEntry {
            id: "todo-plan".into(),
            text: "Plan sprint".into(),
            done: false,
            priority: 3,
            tags: vec!["planning".into()],
            entity_refs: Vec::new(),
        }],
    )
    .unwrap();

    let now = Local::now().naive_local();
    save_events(
        CALENDAR_EVENTS_FILE,
        &[CalendarEvent {
            id: "evt-plan".into(),
            title: "Planning session".into(),
            start: NaiveDate::from_ymd_opt(2026, 1, 15)
                .unwrap()
                .and_hms_opt(9, 0, 0)
                .unwrap(),
            end: None,
            duration_minutes: Some(30),
            all_day: false,
            notes: None,
            recurrence: None,
            reminders: Vec::new(),
            tags: vec!["planning".into()],
            category: None,
            created_at: now,
            updated_at: None,
            entity_refs: Vec::new(),
        }],
    )
    .unwrap();

    (dir, EnvGuard { cwd, notes_dir })
}

#[test]
fn o_list_includes_notes_and_todos() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let (_dir, _guard) = setup_fixture();

    let actions = Arc::new(vec![Action {
        label: "plan app".into(),
        desc: "app".into(),
        action: "app:plan".into(),
        args: None,
    }]);
    let plugin = OmniSearchPlugin::new(actions);

    let results = plugin.search("o list");

    assert!(results.iter().any(|a| a.action == "app:plan"));
    assert!(results
        .iter()
        .any(|a| a.action == "https://plan.example.com"));
    assert!(results.iter().any(|a| a.action == "/workspace/plan"));
    assert!(results.iter().any(|a| a.action == "note:open:project-plan"));
    assert!(results.iter().any(|a| a.action == "todo:done:0"));
    assert!(results.iter().any(|a| a.action == "calendar:upcoming"));
}

#[test]
fn o_list_query_includes_calendar_search_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let (_dir, _guard) = setup_fixture();

    let plugin = OmniSearchPlugin::new(Arc::new(Vec::new()));
    let results = plugin.search("o list planning");

    assert!(results
        .iter()
        .any(|a| a.action == "calendar:search:planning"));
}

#[test]
fn o_list_with_query_filters_notes_todos_and_apps() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let (_dir, _guard) = setup_fixture();

    let actions = Arc::new(vec![
        Action {
            label: "plan app".into(),
            desc: "launcher".into(),
            action: "app:plan".into(),
            args: None,
        },
        Action {
            label: "unrelated app".into(),
            desc: "launcher".into(),
            action: "app:other".into(),
            args: None,
        },
    ]);
    let plugin = OmniSearchPlugin::new(actions);

    let results = plugin.search("o list plan");
    let actions: Vec<&str> = results.iter().map(|a| a.action.as_str()).collect();

    assert!(actions.contains(&"app:plan"));
    assert!(actions.contains(&"https://plan.example.com"));
    assert!(actions.contains(&"/workspace/plan"));
    assert!(!actions.contains(&"note:open:project-plan"));
    assert!(!actions.contains(&"todo:done:0"));
    assert!(!actions.contains(&"app:other"));
}

#[test]
fn o_prefix_matches_non_list_path() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let (_dir, _guard) = setup_fixture();

    let plugin = OmniSearchPlugin::new(Arc::new(vec![Action {
        label: "plan app".into(),
        desc: "launcher".into(),
        action: "app:plan".into(),
        args: None,
    }]));

    let prefix_results: Vec<String> = plugin
        .search("o plan")
        .into_iter()
        .map(|a| a.action)
        .collect();
    let list_results: Vec<String> = plugin
        .search("o list plan")
        .into_iter()
        .map(|a| a.action)
        .collect();

    assert_eq!(prefix_results, list_results);
    assert!(!prefix_results.contains(&"note:open:project-plan".to_string()));
    assert!(!prefix_results.contains(&"todo:done:0".to_string()));
}

#[test]
fn o_list_dedups_duplicate_rows_across_sources() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let (_dir, _guard) = setup_fixture();

    save_notes(&[Note {
        title: "Shared Item".into(),
        path: PathBuf::new(),
        content: "# Shared Item\n\ncontent".into(),
        tags: Vec::new(),
        links: Vec::new(),
        slug: "shared-item".into(),
        alias: None,
        entity_refs: Vec::new(),
    }])
    .unwrap();
    save_todos(
        TODO_FILE,
        &[TodoEntry {
            id: "todo-shared".into(),
            text: "Shared todo".into(),
            done: false,
            priority: 1,
            tags: Vec::new(),
            entity_refs: Vec::new(),
        }],
    )
    .unwrap();

    let plugin = OmniSearchPlugin::new(Arc::new(vec![
        Action {
            label: "Shared Item".into(),
            desc: "app".into(),
            action: "note:open:shared-item".into(),
            args: None,
        },
        Action {
            label: "[ ] Shared todo".into(),
            desc: "app".into(),
            action: "todo:done:0".into(),
            args: None,
        },
    ]));

    let results = plugin.search("o list shared");
    let actions: Vec<String> = results.into_iter().map(|a| a.action).collect();

    assert_eq!(
        actions
            .iter()
            .filter(|a| a.as_str() == "note:open:shared-item")
            .count(),
        1
    );
    assert_eq!(
        actions
            .iter()
            .filter(|a| a.as_str() == "todo:done:0")
            .count(),
        1
    );
}

#[test]
fn o_list_order_is_deterministic_for_same_input() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let (_dir, _guard) = setup_fixture();

    let plugin = OmniSearchPlugin::new(Arc::new(vec![
        Action {
            label: "plan app".into(),
            desc: "launcher".into(),
            action: "app:plan".into(),
            args: None,
        },
        Action {
            label: "helper".into(),
            desc: "plan helper".into(),
            action: "app:helper".into(),
            args: None,
        },
    ]));

    let first: Vec<String> = plugin
        .search("o list plan")
        .into_iter()
        .map(|a| a.action)
        .collect();
    let second: Vec<String> = plugin
        .search("o list plan")
        .into_iter()
        .map(|a| a.action)
        .collect();

    assert_eq!(first, second);
}

#[test]
fn apply_settings_can_disable_calendar_and_todos() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let (_dir, _guard) = setup_fixture();

    let mut plugin = OmniSearchPlugin::new(Arc::new(vec![Action {
        label: "plan app".into(),
        desc: "launcher".into(),
        action: "app:plan".into(),
        args: None,
    }]));

    plugin.apply_settings(&json!({
        "include_calendar": false,
        "include_todos": false,
    }));

    let results = plugin.search("o list");
    let actions: Vec<&str> = results.iter().map(|a| a.action.as_str()).collect();

    assert!(!actions.contains(&"calendar:upcoming"));
    assert!(!actions.contains(&"todo:done:0"));
    assert!(actions.contains(&"app:plan"));
}

#[test]
fn omni_settings_deserialization_defaults_missing_keys() {
    let parsed: OmniSearchSettings =
        serde_json::from_value(json!({ "include_calendar": false })).unwrap();

    assert!(!parsed.include_calendar);
    assert!(parsed.include_apps);
    assert!(parsed.include_notes);
    assert!(parsed.include_todos);
    assert!(parsed.include_folders);
    assert!(parsed.include_bookmarks);
}

#[test]
fn label_and_desc_same_returns_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let cwd = std::env::current_dir().unwrap();
    let notes_dir = std::env::var("ML_NOTES_DIR").ok();
    let _guard = EnvGuard { cwd, notes_dir };
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let action = Action {
        label: "dup".into(),
        desc: "dup".into(),
        action: "dup_action".into(),
        args: None,
    };
    let actions = Arc::new(vec![action]);

    let plugin = OmniSearchPlugin::new(actions);

    let results = plugin.search("o dup");

    assert!(results.iter().any(|a| a.action == "dup_action"));
}
