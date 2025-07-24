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
    assert_eq!(results[0].action, "todo:add:task|0|");
}

#[test]
fn search_add_with_priority_and_tags() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TodoPlugin::default();
    let results = plugin.search("todo add task p=3 #a #b");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "todo:add:task|3|a,b");
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
    assert_eq!(res[0].action, "todo:tag:2|x,y");
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
    append_todo(TODO_FILE, "beta", 1, &["other".into()]).unwrap();

    let plugin = TodoPlugin::default();
    let results = plugin.search("todo list #rs3");
    assert_eq!(results.len(), 1);
    assert!(results[0].label.contains("alpha"));
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
