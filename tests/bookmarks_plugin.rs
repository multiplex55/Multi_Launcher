use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::bookmarks::{save_bookmarks, load_bookmarks, BookmarkEntry, BookmarksPlugin, BOOKMARKS_FILE};
use tempfile::tempdir;
use once_cell::sync::Lazy;
use std::sync::Mutex;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn alias_roundtrip() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![BookmarkEntry { url: "https://example.com".into(), alias: Some("ex".into()) }];
    save_bookmarks(BOOKMARKS_FILE, &entries).unwrap();
    let loaded = load_bookmarks(BOOKMARKS_FILE).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].alias.as_deref(), Some("ex"));
}

#[test]
fn search_uses_alias_label() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![BookmarkEntry { url: "https://example.com".into(), alias: Some("ex".into()) }];
    save_bookmarks(BOOKMARKS_FILE, &entries).unwrap();

    let plugin = BookmarksPlugin::default();
    let results = plugin.search("bm ex");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "ex");
    assert_eq!(results[0].action, "https://example.com");
}

#[test]
fn plain_bm_shows_dialog_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let plugin = BookmarksPlugin::default();
    let results = plugin.search("bm");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "bookmark:dialog");
}

#[test]
fn bm_add_without_url_shows_dialog() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let plugin = BookmarksPlugin::default();
    let results = plugin.search("bm add");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "bookmark:dialog");
}

