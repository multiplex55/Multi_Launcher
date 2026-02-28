use multi_launcher::actions::Action;
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::bookmarks::{save_bookmarks, BookmarkEntry, BOOKMARKS_FILE};
use multi_launcher::plugins::folders::{save_folders, FolderEntry, FOLDERS_FILE};
use multi_launcher::plugins::note::{save_notes, Note};
use multi_launcher::plugins::omni_search::OmniSearchPlugin;
use multi_launcher::plugins::todo::{save_todos, TodoEntry, TODO_FILE};
use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn setup_fixture() -> tempfile::TempDir {
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

    dir
}

#[test]
fn o_list_includes_notes_and_todos() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _dir = setup_fixture();

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
}

#[test]
fn o_list_with_query_filters_notes_todos_and_apps() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _dir = setup_fixture();

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
    assert!(actions.contains(&"note:open:project-plan"));
    assert!(actions.contains(&"todo:done:0"));
    assert!(!actions.contains(&"app:other"));
}

#[test]
fn o_prefix_matches_non_list_path() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _dir = setup_fixture();

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

    assert!(prefix_results.contains(&"note:open:project-plan".to_string()));
    assert!(prefix_results.contains(&"todo:done:0".to_string()));
    assert!(list_results.contains(&"note:open:project-plan".to_string()));
    assert!(list_results.contains(&"todo:done:0".to_string()));
}

#[test]
fn o_list_dedups_duplicate_rows_across_sources() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _dir = setup_fixture();

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
    let _dir = setup_fixture();

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
fn label_and_desc_same_returns_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
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
