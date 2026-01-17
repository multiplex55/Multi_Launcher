use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::folders::{FoldersPlugin, FOLDERS_FILE};
use multi_launcher::plugins::timer::{active_timers, ACTIVE_TIMERS};
use multi_launcher::history::{append_history, clear_history, get_history, poison_history_lock, HistoryEntry};
use multi_launcher::actions::Action;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn folders_corrupt_file_does_not_panic() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    std::fs::write(FOLDERS_FILE, b"not json").unwrap();
    let plugin = FoldersPlugin::default();
    plugin.search("f");
}

#[test]
fn timer_poisoned_lock_does_not_panic() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _ = std::panic::catch_unwind(|| {
        let _guard = ACTIVE_TIMERS.lock().unwrap();
        panic!("poison");
    });
    assert!(std::panic::catch_unwind(|| {
        active_timers();
    })
    .is_ok());
}

#[test]
fn history_poisoned_lock_does_not_panic() {
    let _lock = TEST_MUTEX.lock().unwrap();
    poison_history_lock();
    let action = Action {
        label: "l".into(),
        desc: "d".into(),
        action: "a".into(),
        args: None,
    };
    let entry = HistoryEntry {
        query: "q".into(),
        query_lc: String::new(),
        action,
        timestamp: 0,
    };
    assert!(std::panic::catch_unwind(|| {
        let _ = append_history(entry.clone(), 10);
        let _ = get_history();
        let _ = clear_history();
    })
    .is_ok());
}
