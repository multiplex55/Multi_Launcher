use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::fav::{save_favs, FavEntry, FavPlugin, FAV_FILE};
use multi_launcher::plugins::bookmarks::{load_bookmarks, save_bookmarks, BOOKMARKS_FILE};
use multi_launcher::launcher::launch_action;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn list_returns_entries() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![FavEntry { label: "one".into(), action: "noop".into(), args: None }];
    save_favs(FAV_FILE, &entries).unwrap();

    let plugin = FavPlugin::default();
    let results = plugin.search("fav list");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "one");
    assert_eq!(results[0].action, "noop");
}

#[test]
fn launch_runs_command() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    save_favs(
        FAV_FILE,
        &[FavEntry {
            label: "bm".into(),
            action: "bookmark:add:https://example.com".into(),
            args: None,
        }],
    )
    .unwrap();
    save_bookmarks(BOOKMARKS_FILE, &[]).unwrap();

    let plugin = FavPlugin::default();
    let action = plugin.search("fav list")[0].clone();
    launch_action(&action).unwrap();
    let list = load_bookmarks(BOOKMARKS_FILE).unwrap();
    assert_eq!(list.len(), 1);
}

#[test]
fn query_fav_opens_dialog() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let plugin = FavPlugin::default();
    let results = plugin.search("fav");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "fav:dialog");
}
