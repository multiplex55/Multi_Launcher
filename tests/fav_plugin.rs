use multi_launcher::history::{append_history, get_history, HistoryEntry};
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::fav::{save_favs, FavEntry, FavPlugin, FAV_FILE};
use multi_launcher::{actions::Action, launcher::launch_action};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn list_returns_saved_entries() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![
        FavEntry {
            label: "a".into(),
            action: "history:clear".into(),
            args: None,
        },
        FavEntry {
            label: "b".into(),
            action: "history:clear".into(),
            args: None,
        },
    ];
    save_favs(FAV_FILE, &entries).unwrap();

    let plugin = FavPlugin::default();
    let results = plugin.search("fav list");
    assert_eq!(results.len(), 2);
}

#[test]
fn launching_entry_executes_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    // prepare history file with one entry
    let entry = HistoryEntry {
        query: "q".into(),
        query_lc: String::new(),
        action: Action {
            label: "l".into(),
            desc: String::new(),
            action: "run".into(),
            args: None,
        },
    };
    append_history(entry, 10).unwrap();
    assert!(!get_history().is_empty());

    save_favs(
        FAV_FILE,
        &[FavEntry {
            label: "clr".into(),
            action: "history:clear".into(),
            args: None,
        }],
    )
    .unwrap();

    let plugin = FavPlugin::default();
    let results = plugin.search("fav list");
    assert_eq!(results.len(), 1);
    launch_action(&results[0]).unwrap();
    assert!(get_history().is_empty());
}
