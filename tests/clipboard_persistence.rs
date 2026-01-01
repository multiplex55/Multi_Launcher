use arboard::Clipboard;
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::clipboard::{save_history, ClipboardPlugin, CLIPBOARD_FILE};
use once_cell::sync::Lazy;
use std::collections::VecDeque;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn history_survives_instances() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let mut list = VecDeque::new();
    list.push_back("first".into());
    save_history(CLIPBOARD_FILE, &list).unwrap();

    let mut cb = Clipboard::new().ok();
    if let Some(ref mut clipboard) = cb {
        let _ = clipboard.set_text("first".to_string());
    }

    let plugin1 = ClipboardPlugin::new(20);
    let results1 = plugin1.search("cb first");
    assert_eq!(results1.len(), 1);
    assert_eq!(results1[0].label, "first");

    let plugin2 = ClipboardPlugin::new(20);
    let results2 = plugin2.search("cb first");
    assert_eq!(results2.len(), 1);
    assert_eq!(results2[0].label, "first");
}

#[test]
fn cb_list_returns_all_entries() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let mut list = VecDeque::new();
    list.push_back("alpha".into());
    list.push_back("beta".into());
    save_history(CLIPBOARD_FILE, &list).unwrap();

    let mut cb = Clipboard::new().ok();
    if let Some(ref mut clipboard) = cb {
        let _ = clipboard.set_text("alpha".to_string());
    }

    let plugin = ClipboardPlugin::new(20);
    let results = plugin.search("cb list");
    assert_eq!(results.len(), 2);
    assert!(results[0].action.starts_with("clipboard:copy:"));
    assert!(results[1].action.starts_with("clipboard:copy:"));
}
