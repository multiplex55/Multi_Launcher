use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::snippets::{save_snippets, load_snippets, SnippetEntry, SnippetsPlugin, SNIPPETS_FILE};
use tempfile::tempdir;
use once_cell::sync::Lazy;
use std::sync::Mutex;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn load_save_roundtrip() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![SnippetEntry { alias: "hw".into(), text: "hello".into() }];
    save_snippets(SNIPPETS_FILE, &entries).unwrap();
    let loaded = load_snippets(SNIPPETS_FILE).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].alias, "hw");
    assert_eq!(loaded[0].text, "hello");
}

#[test]
fn search_returns_clipboard_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![SnippetEntry { alias: "hi".into(), text: "hello world".into() }];
    save_snippets(SNIPPETS_FILE, &entries).unwrap();

    let plugin = SnippetsPlugin::default();
    let results = plugin.search("cs hi");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "hi");
    assert_eq!(results[0].action, "clipboard:hello world");
    assert_eq!(results[0].desc, "Snippet");
}

#[test]
fn list_command_returns_entries() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![
        SnippetEntry { alias: "a".into(), text: "alpha".into() },
        SnippetEntry { alias: "b".into(), text: "beta".into() },
    ];
    save_snippets(SNIPPETS_FILE, &entries).unwrap();

    let plugin = SnippetsPlugin::default();
    let results = plugin.search("cs list");
    assert_eq!(results.len(), 2);
}

#[test]
fn rm_command_returns_remove_actions() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![SnippetEntry { alias: "todelete".into(), text: "bye".into() }];
    save_snippets(SNIPPETS_FILE, &entries).unwrap();

    let plugin = SnippetsPlugin::default();
    let results = plugin.search("cs rm todelete");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "snippet:remove:todelete");
}

#[test]
fn search_preserves_newlines() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entries = vec![SnippetEntry { alias: "multi".into(), text: "a\nb".into() }];
    save_snippets(SNIPPETS_FILE, &entries).unwrap();

    let plugin = SnippetsPlugin::default();
    let results = plugin.search("cs multi");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "clipboard:a\nb");
}
