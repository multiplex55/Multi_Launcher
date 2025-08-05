use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::note::{append_note, load_notes, remove_note, NotePlugin};
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
fn search_add_returns_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    setup();
    let plugin = NotePlugin::default();
    let results = plugin.search("note add demo");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:add:demo");
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
    assert!(results.iter().all(|a| a.action.starts_with("note:copy:")));
}

#[test]
fn remove_action_returns_indices() {
    let _lock = TEST_MUTEX.lock().unwrap();
    setup();

    append_note("first", "first").unwrap();
    append_note("second", "second").unwrap();

    let plugin = NotePlugin::default();
    let results = plugin.search("note rm first");
    assert_eq!(results.len(), 1);
    let idx: usize = results[0]
        .action
        .strip_prefix("note:remove:")
        .unwrap()
        .parse()
        .unwrap();
    remove_note(idx).unwrap();
    let notes = load_notes().unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].title, "second");
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
