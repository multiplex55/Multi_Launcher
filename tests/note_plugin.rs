use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::note::{append_note, NotePlugin};
use once_cell::sync::Lazy;
use std::sync::Mutex;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn setup() {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("notes");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn search_new_returns_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    setup();
    let plugin = NotePlugin::default();
    let results = plugin.search("note new demo");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:new:demo");
}

#[test]
fn list_returns_saved_notes() {
    let _lock = TEST_MUTEX.lock().unwrap();
    setup();

    append_note("alpha", "alpha").unwrap();
    append_note("beta", "beta").unwrap();

    let plugin = NotePlugin::default();
    let results = plugin.search("note list");
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|a| a.action.starts_with("note:open:")));
}

#[test]
fn delete_returns_slug() {
    let _lock = TEST_MUTEX.lock().unwrap();
    setup();

    append_note("first", "first").unwrap();
    append_note("second", "second").unwrap();

    let plugin = NotePlugin::default();
    let results = plugin.search("note delete first");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:delete:first");
}

#[test]
fn search_plain_note_opens_dialog() {
    let _lock = TEST_MUTEX.lock().unwrap();
    setup();
    let plugin = NotePlugin::default();
    let results = plugin.search("note");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:dialog");
    assert_eq!(results[0].label, "note: edit notes");
}

#[test]
fn list_filters_by_tag() {
    let _lock = TEST_MUTEX.lock().unwrap();
    setup();

    append_note("alpha", "alpha #foo").unwrap();
    append_note("beta", "beta #bar").unwrap();

    let plugin = NotePlugin::default();
    let results = plugin.search("note list #foo");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:open:alpha");
}
