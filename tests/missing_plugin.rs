use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::missing::MissingPlugin;
use multi_launcher::plugins::bookmarks::BOOKMARKS_FILE;
use multi_launcher::plugins::fav::FAV_FILE;
use multi_launcher::plugins::folders::FOLDERS_FILE;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn prefix_triggers_actions() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    std::fs::write(BOOKMARKS_FILE, "[]").unwrap();
    std::fs::write(FAV_FILE, "[]").unwrap();
    std::fs::write(FOLDERS_FILE, "[]").unwrap();

    let plugin = MissingPlugin;
    let partial = plugin.search("check miss");
    assert_eq!(partial.len(), 1);
    assert_eq!(partial[0].action, "noop:");

    let full = plugin.search("check missing");
    assert_eq!(full.len(), 1);
    assert_eq!(full[0].action, "noop:");
}
