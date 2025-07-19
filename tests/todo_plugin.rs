use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::todo::{
    append_todo, load_todos, remove_todo, mark_done, set_priority, set_tags,
    TodoPlugin, TODO_FILE,
};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn search_add_returns_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TodoPlugin::default();
    let results = plugin.search("todo add task   ");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "todo:add:task");
}

#[test]
fn search_add_without_text_opens_dialog() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TodoPlugin::default();
    let results = plugin.search("todo add");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "todo:dialog");
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
    assert_eq!(results[0].label, "todo: edit todos");
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
