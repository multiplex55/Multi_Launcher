use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::notes::{append_note, remove_note, load_notes, NotesPlugin, QUICK_NOTES_FILE};
use chrono::TimeZone;
use tempfile::tempdir;
use once_cell::sync::Lazy;
use std::sync::Mutex;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn search_add_returns_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = NotesPlugin::default();
    let results = plugin.search("note add demo");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:add:demo");
}

#[test]
fn list_returns_saved_notes() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    append_note(QUICK_NOTES_FILE, "alpha").unwrap();
    append_note(QUICK_NOTES_FILE, "beta").unwrap();

    let plugin = NotesPlugin::default();
    let results = plugin.search("note list");
    println!("results: {:?}", results.iter().map(|r| &r.action).collect::<Vec<_>>());
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|a| a.action.starts_with("note:copy:")));
}

#[test]
fn remove_action_returns_indices() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    append_note(QUICK_NOTES_FILE, "first").unwrap();
    append_note(QUICK_NOTES_FILE, "second").unwrap();

    let plugin = NotesPlugin::default();
    let results = plugin.search("note rm first");
    println!("rm results: {:?}", results.iter().map(|r| &r.action).collect::<Vec<_>>());
    assert_eq!(results.len(), 1);
    assert!(results[0].action.starts_with("note:remove:"));

    let idx: usize = results[0].action.strip_prefix("note:remove:").unwrap().parse().unwrap();
    remove_note(QUICK_NOTES_FILE, idx).unwrap();
    let notes = load_notes(QUICK_NOTES_FILE).unwrap();
    println!("remaining notes: {}", notes.len());
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].text, "second");
}

#[test]
fn search_plain_note_opens_dialog() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let plugin = NotesPlugin::default();
    let results = plugin.search("note");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:dialog");
    assert_eq!(results[0].label, "note: edit notes");
}

#[test]
fn list_handles_invalid_timestamp() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    use multi_launcher::plugins::notes::{save_notes, NoteEntry};
    let notes = vec![NoteEntry { ts: 10_000_000_000_000u64, text: "demo".into() }];
    save_notes(QUICK_NOTES_FILE, &notes).unwrap();

    let plugin = NotesPlugin::default();
    let results = plugin.search("note list");
    assert_eq!(results.len(), 1);
    let expected_ts = chrono::Local
        .timestamp_opt(0, 0)
        .single()
        .unwrap()
        .format("%Y-%m-%d %H:%M")
        .to_string();
    assert_eq!(results[0].label, format!("{expected_ts} - demo"));
}
