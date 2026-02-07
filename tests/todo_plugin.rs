use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use eframe::egui;
use multi_launcher::gui::{
    todo_view_layout_sizes, todo_view_window_constraints, LauncherApp, TodoDialog, TodoViewDialog,
};
use multi_launcher::plugin::Plugin;
use multi_launcher::plugin::PluginManager;
use multi_launcher::plugins::todo::{
    append_todo, load_todos, mark_done, remove_todo, set_priority, set_tags, TodoAddActionPayload,
    TodoEntry, TodoPlugin, TodoTagActionPayload, TODO_FILE,
};
use multi_launcher::settings::Settings;
use once_cell::sync::Lazy;
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn decode_payload<T: serde::de::DeserializeOwned>(encoded: &str) -> T {
    let json = URL_SAFE_NO_PAD.decode(encoded).unwrap();
    serde_json::from_slice(&json).unwrap()
}

fn new_app(ctx: &egui::Context) -> LauncherApp {
    LauncherApp::new(
        ctx,
        Arc::new(Vec::new()),
        0,
        PluginManager::new(),
        "actions.json".into(),
        "settings.json".into(),
        Settings::default(),
        None,
        None,
        None,
        None,
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    )
}

#[test]
fn search_add_returns_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TodoPlugin::default();
    let results = plugin.search("todo add task   ");
    assert_eq!(results.len(), 1);
    let encoded = results[0].action.strip_prefix("todo:add:").unwrap();
    let payload: TodoAddActionPayload = decode_payload(encoded);
    assert_eq!(
        payload,
        TodoAddActionPayload {
            text: "task".into(),
            priority: 0,
            tags: vec![],
        }
    );
    assert_eq!(results[0].label, "Add todo task");
}

#[test]
fn search_add_with_priority_and_tags() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TodoPlugin::default();
    let results = plugin.search("todo add task p=3 #a #b");
    assert_eq!(results.len(), 1);
    let encoded = results[0].action.strip_prefix("todo:add:").unwrap();
    let payload: TodoAddActionPayload = decode_payload(encoded);
    assert_eq!(
        payload,
        TodoAddActionPayload {
            text: "task".into(),
            priority: 3,
            tags: vec!["a".into(), "b".into()],
        }
    );
    assert_eq!(results[0].label, "Add todo task Tag: a, b; priority: 3");
}

#[test]
fn search_add_with_at_tags() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TodoPlugin::default();
    let results = plugin.search("todo add task @a @b");
    assert_eq!(results.len(), 1);
    let encoded = results[0].action.strip_prefix("todo:add:").unwrap();
    let payload: TodoAddActionPayload = decode_payload(encoded);
    assert_eq!(
        payload,
        TodoAddActionPayload {
            text: "task".into(),
            priority: 0,
            tags: vec!["a".into(), "b".into()],
        }
    );
    assert_eq!(results[0].label, "Add todo task Tag: a, b");
}

#[test]
fn search_add_without_text_opens_dialog() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TodoPlugin::default();
    let results = plugin.search("todo add");
    assert_eq!(results.len(), 2);
    assert!(results.iter().any(|action| action.action == "todo:dialog"));
    assert!(results
        .iter()
        .any(|action| action.label.starts_with("Usage: todo add")));
}

#[test]
fn list_returns_saved_items() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    append_todo(TODO_FILE, "alpha", 0, &[]).unwrap();
    append_todo(TODO_FILE, "beta", 0, &[]).unwrap();

    let plugin = TodoPlugin::default();
    let results = plugin.search("todo list");
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|a| a.action.starts_with("todo:done:")));
}

#[test]
fn remove_action_deletes_entry() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    append_todo(TODO_FILE, "remove me", 0, &[]).unwrap();
    append_todo(TODO_FILE, "keep", 0, &[]).unwrap();

    let plugin = TodoPlugin::default();
    let results = plugin.search("todo rm remove");
    assert_eq!(results.len(), 1);
    assert!(results[0].action.starts_with("todo:remove:"));

    let idx: usize = results[0]
        .action
        .strip_prefix("todo:remove:")
        .unwrap()
        .parse()
        .unwrap();
    remove_todo(TODO_FILE, idx).unwrap();
    let todos = load_todos(TODO_FILE).unwrap();
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].text, "keep");
}

#[test]
fn search_plain_todo_opens_dialog() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TodoPlugin::default();
    let results = plugin.search("todo");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "todo:dialog");
}

#[test]
fn search_todo_space_shows_submenu() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TodoPlugin::default();
    let results = plugin.search("todo ");
    let labels: Vec<&str> = results.iter().map(|action| action.label.as_str()).collect();
    assert!(labels.contains(&"todo: edit todos"));
    assert!(labels.contains(&"todo edit"));
    assert!(labels.contains(&"todo list"));
    assert!(labels.contains(&"todo tag"));
    assert!(labels.contains(&"todo view"));
    assert!(labels.contains(&"todo add"));
    assert!(labels.contains(&"todo rm"));
    assert!(labels.contains(&"todo clear"));
    assert!(labels.contains(&"todo pset"));
    assert!(labels.contains(&"todo export"));
}

#[test]
fn fuzzish_partial_todo_queries() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TodoPlugin::default();
    let results = plugin.search("todo !");
    assert_eq!(results.len(), 1);
    assert!(results[0].label.starts_with("Usage: todo"));
}

#[test]
fn mark_done_toggles_status() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    append_todo(TODO_FILE, "task", 0, &[]).unwrap();

    mark_done(TODO_FILE, 0).unwrap();
    let todos = load_todos(TODO_FILE).unwrap();
    assert!(todos[0].done);

    mark_done(TODO_FILE, 0).unwrap();
    let todos = load_todos(TODO_FILE).unwrap();
    assert!(!todos[0].done);
}

#[test]
fn search_reflects_done_state_immediately() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    append_todo(TODO_FILE, "task", 0, &[]).unwrap();
    let plugin = TodoPlugin::default();

    mark_done(TODO_FILE, 0).unwrap();
    let results = plugin.search("todo list");
    assert!(results[0].label.starts_with("[x]"));

    mark_done(TODO_FILE, 0).unwrap();
    let results = plugin.search("todo list");
    assert!(results[0].label.starts_with("[ ]"));
}

#[test]
fn set_priority_and_tags_update_entry() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    append_todo(TODO_FILE, "task", 0, &[]).unwrap();
    set_priority(TODO_FILE, 0, 5).unwrap();
    set_tags(TODO_FILE, 0, &["a".into(), "b".into()]).unwrap();
    let todos = load_todos(TODO_FILE).unwrap();
    assert_eq!(todos[0].priority, 5);
    assert_eq!(todos[0].tags, vec!["a", "b"]);
}

#[test]
fn set_priority_persists_to_file() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    append_todo(TODO_FILE, "task", 0, &[]).unwrap();
    set_priority(TODO_FILE, 0, 7).unwrap();
    let todos = load_todos(TODO_FILE).unwrap();
    assert_eq!(todos[0].priority, 7);
}

#[test]
fn set_tags_persists_to_file() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    append_todo(TODO_FILE, "task", 0, &[]).unwrap();
    set_tags(TODO_FILE, 0, &["x".into(), "y".into()]).unwrap();
    let todos = load_todos(TODO_FILE).unwrap();
    assert_eq!(todos[0].tags, vec!["x", "y"]);
}

#[test]
fn search_pset_and_tag_actions() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TodoPlugin::default();
    let res = plugin.search("todo pset 1 4");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].action, "todo:pset:1|4");
    let res = plugin.search("todo tag 2 #x #y");
    assert_eq!(res.len(), 1);
    let encoded = res[0].action.strip_prefix("todo:tag:").unwrap();
    let payload: TodoTagActionPayload = decode_payload(encoded);
    assert_eq!(
        payload,
        TodoTagActionPayload {
            idx: 2,
            tags: vec!["x".into(), "y".into()],
        }
    );
}

#[test]
fn list_without_filter_sorts_by_priority() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    append_todo(TODO_FILE, "low", 1, &[]).unwrap();
    append_todo(TODO_FILE, "high", 5, &[]).unwrap();

    let plugin = TodoPlugin::default();
    let results = plugin.search("todo list");
    assert_eq!(results.len(), 2);
    assert!(results[0].label.contains("high"));
    assert!(results[1].label.contains("low"));
}

#[test]
fn list_filters_by_tag() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    append_todo(TODO_FILE, "alpha", 1, &["rs3".into()]).unwrap();
    append_todo(TODO_FILE, "beta", 1, &["work".into()]).unwrap();
    append_todo(TODO_FILE, "gamma", 1, &["workshop".into()]).unwrap();
    append_todo(TODO_FILE, "delta", 1, &["ui-kit".into()]).unwrap();
    append_todo(TODO_FILE, "epsilon", 1, &["backend".into()]).unwrap();

    let plugin = TodoPlugin::default();
    let results = plugin.search("todo list #rs");
    assert_eq!(results.len(), 1);
    assert!(results[0].label.contains("alpha"));

    let results = plugin.search("todo list @wor");
    let labels: Vec<&str> = results.iter().map(|action| action.label.as_str()).collect();
    assert_eq!(results.len(), 2);
    assert!(labels.iter().any(|label| label.contains("beta")));
    assert!(labels.iter().any(|label| label.contains("gamma")));

    let results = plugin.search("todo list !#ui");
    let labels: Vec<&str> = results.iter().map(|action| action.label.as_str()).collect();
    assert_eq!(results.len(), 4);
    assert!(!labels.iter().any(|label| label.contains("delta")));
}

#[test]
fn list_tag_filter_sorts_by_priority() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    append_todo(TODO_FILE, "low", 1, &["p".into()]).unwrap();
    append_todo(TODO_FILE, "high", 5, &["p".into()]).unwrap();
    append_todo(TODO_FILE, "mid", 3, &["p".into()]).unwrap();

    let plugin = TodoPlugin::default();
    let results = plugin.search("todo list #p");
    assert_eq!(results.len(), 3);
    assert!(results[0].label.contains("high"));
    assert!(results[1].label.contains("mid"));
    assert!(results[2].label.contains("low"));
}

#[test]
fn tag_command_filters_by_tag() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    append_todo(TODO_FILE, "urgent task", 1, &["urgent".into()]).unwrap();
    append_todo(TODO_FILE, "other task", 1, &["other".into()]).unwrap();

    let plugin = TodoPlugin::default();
    let results = plugin.search("todo tag urgent");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "#urgent (1)");
    assert_eq!(results[0].action, "query:todo list #urgent");
}

#[test]
fn tag_command_without_filter_lists_all_tags() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    append_todo(TODO_FILE, "alpha task", 1, &["alpha".into(), "beta".into()]).unwrap();
    append_todo(TODO_FILE, "beta task", 1, &["beta".into()]).unwrap();
    append_todo(
        TODO_FILE,
        "gamma task",
        1,
        &["gamma".into(), "alpha".into()],
    )
    .unwrap();

    let plugin = TodoPlugin::default();
    let results = plugin.search("todo tag");
    let labels: Vec<&str> = results.iter().map(|action| action.label.as_str()).collect();

    assert_eq!(labels, vec!["#alpha (2)", "#beta (2)", "#gamma (1)"]);
}
#[test]
fn search_view_opens_dialog() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TodoPlugin::default();
    let results = plugin.search("todo view");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "todo:view");
}

#[test]
fn search_export_returns_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TodoPlugin::default();
    let results = plugin.search("todo export");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "todo:export");
}

#[test]
fn list_negative_filters() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    append_todo(TODO_FILE, "urgent task", 1, &["urgent".into()]).unwrap();
    append_todo(TODO_FILE, "other task", 1, &["other".into()]).unwrap();
    let plugin = TodoPlugin::default();
    let results = plugin.search("todo list !#urgent");
    assert_eq!(results.len(), 1);
    assert!(results[0].label.contains("other task"));
    let results = plugin.search("todo list !urgent");
    assert_eq!(results.len(), 1);
    assert!(results[0].label.contains("other task"));
}

#[test]
fn dialog_filtered_indices_negation() {
    let entries = vec![
        TodoEntry {
            text: "alpha".into(),
            done: false,
            priority: 0,
            tags: vec!["work".into()],
        },
        TodoEntry {
            text: "beta".into(),
            done: false,
            priority: 0,
            tags: vec![],
        },
    ];
    let idx = TodoDialog::filtered_indices(&entries, "!#work");
    assert_eq!(idx, vec![1]);
    let idx = TodoDialog::filtered_indices(&entries, "!beta");
    assert_eq!(idx, vec![0]);
}

#[test]
fn dialog_scrolls_with_many_entries() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let mut dlg = TodoDialog::default();
    dlg.open();
    for i in 0..100 {
        dlg.test_set_text(&format!("task{i}"));
        dlg.test_add_todo();
    }

    ctx.begin_frame(egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1000.0, 2000.0),
        )),
        ..Default::default()
    });
    dlg.ui(&ctx, &mut app);
    let _ = ctx.end_frame();

    let rect = ctx
        .memory(|m| m.area_rect(egui::Id::new("Todos")))
        .expect("window rect");
    assert!(rect.height() < 800.0);
}

#[test]
fn todo_view_dialog_has_fixed_size() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let mut dlg = TodoViewDialog::default();
    dlg.open();

    ctx.begin_frame(egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1000.0, 2000.0),
        )),
        ..Default::default()
    });
    dlg.ui(&ctx, &mut app);
    let _ = ctx.end_frame();

    let (min_size, max_size) = todo_view_window_constraints();
    let _rect = ctx
        .memory(|m| m.area_rect(egui::Id::new("View Todos")))
        .expect("window rect");
    let (_window_size, _) = todo_view_layout_sizes();
    assert_eq!(min_size, max_size);
}
