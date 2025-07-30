use multi_launcher::history::{append_history, get_history, HistoryEntry};
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::fav::{save_favs, FavEntry, FavPlugin, FAV_FILE};
use multi_launcher::{actions::Action, launcher::launch_action};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn list_returns_entries() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![
        FavEntry {
            label: "clear".into(),
            action: "history:clear".into(),
            args: None,
        },
        FavEntry {
            label: "clip".into(),
            action: "clipboard:clear".into(),
            args: None,
        },
    ];
    save_favs(FAV_FILE, &entries).unwrap();

    let plugin = FavPlugin::new();
    let results = plugin.search("fav list");
    assert_eq!(results.len(), 2);
    assert!(results.iter().any(|a| a.action == "history:clear"));
    assert!(results.iter().any(|a| a.action == "clipboard:clear"));
}

#[test]
fn launch_executes_command() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    save_favs(
        FAV_FILE,
        &[FavEntry {
            label: "clear".into(),
            action: "history:clear".into(),
            args: None,
        }],
    )
    .unwrap();
    let action = Action {
        label: String::new(),
        desc: String::new(),
        action: "run".into(),
        args: None,
    };
    let entry = HistoryEntry {
        query: "t".into(),
        query_lc: String::new(),
        action: action.clone(),
    };
    append_history(entry, 10).unwrap();
    assert!(!get_history().is_empty());

    let plugin = FavPlugin::new();
    let fav_action = plugin.search("fav list")[0].clone();
    launch_action(&fav_action).unwrap();
    assert!(get_history().is_empty());
}
