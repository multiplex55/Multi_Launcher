use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::omni_search::OmniSearchPlugin;
use std::sync::Arc;
use multi_launcher::plugins::bookmarks::{save_bookmarks, BookmarkEntry, BOOKMARKS_FILE};
use multi_launcher::plugins::folders::{save_folders, FolderEntry, FOLDERS_FILE};
use multi_launcher::actions::Action;
use tempfile::tempdir;
use once_cell::sync::Lazy;
use std::sync::Mutex;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn o_list_combines_all_sources() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    save_bookmarks(BOOKMARKS_FILE, &[BookmarkEntry { url: "https://example.com".into(), alias: None }]).unwrap();
    save_folders(FOLDERS_FILE, &[FolderEntry { label: "Foo".into(), path: "/foo".into(), alias: None }]).unwrap();

    let actions = Arc::new(vec![Action { label: "myapp".into(), desc: "app".into(), action: "myapp".into(), args: None }]);
    let plugin = OmniSearchPlugin::new(actions);

    let results = plugin.search("o list");

    assert!(results.iter().any(|a| a.action == "myapp"));
    assert!(results.iter().any(|a| a.action == "https://example.com"));
    assert!(results.iter().any(|a| a.action == "/foo"));
}

#[test]
fn o_list_filters_results() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    save_bookmarks(BOOKMARKS_FILE, &[BookmarkEntry { url: "https://example.com".into(), alias: None }]).unwrap();
    save_folders(FOLDERS_FILE, &[FolderEntry { label: "Foo".into(), path: "/foo".into(), alias: None }]).unwrap();

    let actions = Arc::new(vec![Action { label: "barapp".into(), desc: "app".into(), action: "bar".into(), args: None }]);
    let plugin = OmniSearchPlugin::new(actions);

    let results = plugin.search("o list bar");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "bar");
}

#[test]
fn label_and_desc_same_returns_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let action = Action {
        label: "dup".into(),
        desc: "dup".into(),
        action: "dup_action".into(),
        args: None,
    };
    let actions = Arc::new(vec![action]);

    let plugin = OmniSearchPlugin::new(actions);

    let results = plugin.search("o dup");

    assert!(results.iter().any(|a| a.action == "dup_action"));
}

