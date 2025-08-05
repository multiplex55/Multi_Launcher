use chrono::Local;
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::note::{append_note, save_notes, NotePlugin};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn setup() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    let notes_dir = dir.path().join("notes");
    std::fs::create_dir_all(&notes_dir).unwrap();
    std::env::set_var("ML_NOTES_DIR", &notes_dir);
    std::env::set_var("HOME", dir.path());
    save_notes(&[]).unwrap();
    dir
}

#[test]
fn note_new_generates_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    let plugin = NotePlugin::default();
    let results = plugin.search("note new Hello World");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:new:hello-world");
}

#[test]
fn note_open_returns_matching_note() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    append_note("alpha", "alpha content").unwrap();
    let plugin = NotePlugin::default();
    let results = plugin.search("note open alpha");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:open:alpha");
}

#[test]
fn note_list_handles_slug_collisions() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    append_note("alpha", "one").unwrap();
    append_note("alpha", "two").unwrap();
    let plugin = NotePlugin::default();
    let results = plugin.search("note list");
    assert_eq!(results.len(), 2);
    let actions: Vec<String> = results.into_iter().map(|a| a.action).collect();
    assert!(actions.contains(&"note:open:alpha".to_string()));
    assert!(actions.contains(&"note:open:alpha-1".to_string()));
}

#[test]
fn note_search_finds_content() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    append_note("alpha", "lorem ipsum").unwrap();
    append_note("beta", "unique needle").unwrap();
    let plugin = NotePlugin::default();
    let results = plugin.search("note search needle");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:open:beta");
}

#[test]
fn note_tags_parses_edge_cases() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    append_note("alpha", "#foo #bar-baz #baz_1 #dup #dup").unwrap();
    let plugin = NotePlugin::default();
    let results = plugin.search("note tags");
    assert_eq!(results.len(), 4);
    let labels: Vec<String> = results.iter().map(|a| a.label.clone()).collect();
    assert!(labels.contains(&"#foo".to_string()));
    assert!(labels.contains(&"#bar".to_string()));
    assert!(labels.contains(&"#baz_1".to_string()));
    assert!(labels.contains(&"#dup".to_string()));
    let list_results = plugin.search("note list #bar");
    assert_eq!(list_results.len(), 1);
    assert_eq!(list_results[0].action, "note:open:alpha");
}

#[test]
fn note_link_dedupes_backlinks() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    append_note("alpha", "link to [[beta note]] and [[Beta Note]]").unwrap();
    append_note("beta note", "beta").unwrap();
    let plugin = NotePlugin::default();
    let results = plugin.search("note link beta note");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "note:open:alpha");
}

#[test]
fn note_today_opens_daily_note() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    let plugin = NotePlugin::default();
    let today = Local::now().format("%Y-%m-%d").to_string();
    let results = plugin.search("note today");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, format!("note:open:{}", today));
}
