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

    let plugin1 = ClipboardPlugin::new(20);
    let results1 = plugin1.search("cb");
    assert_eq!(results1.len(), 1);
    assert_eq!(results1[0].label, "first");

    let plugin2 = ClipboardPlugin::new(20);
    let results2 = plugin2.search("cb");
    assert_eq!(results2.len(), 1);
    assert_eq!(results2[0].label, "first");
}
