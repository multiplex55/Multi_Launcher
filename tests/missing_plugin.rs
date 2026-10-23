use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::bookmarks::BOOKMARKS_FILE;
use multi_launcher::plugins::fav::FAV_FILE;
use multi_launcher::plugins::folders::{FOLDERS_FILE, FolderEntry};
use multi_launcher::plugins::missing::MissingPlugin;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

struct CurrentDirectoryGuard(std::path::PathBuf);

impl CurrentDirectoryGuard {
    fn change(path: &std::path::Path) -> Self {
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(path).unwrap();
        Self(previous)
    }
}

impl Drop for CurrentDirectoryGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.0).unwrap();
    }
}

fn wait_for_scan(plugin: &MissingPlugin) -> Vec<multi_launcher::actions::Action> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let result = plugin.search("check missing");
        if result
            .first()
            .is_some_and(|action| action.action != "missing:status:checking")
        {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "explicit missing scan did not complete"
        );
        std::thread::yield_now();
    }
}

#[test]
fn prefix_triggers_actions() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    let _cwd = CurrentDirectoryGuard::change(dir.path());

    std::fs::write(BOOKMARKS_FILE, "[]").unwrap();
    std::fs::write(FAV_FILE, "[]").unwrap();
    std::fs::write(FOLDERS_FILE, "[]").unwrap();

    let plugin = MissingPlugin::default();
    let partial = plugin.search("check miss");
    assert_eq!(partial.len(), 1);
    assert_eq!(partial[0].action, "missing:status:complete-query");

    let full = wait_for_scan(&plugin);
    assert_eq!(full.len(), 1);
    assert_eq!(full[0].action, "missing:status:complete");
}

#[test]
fn explicit_scan_returns_missing_folder_as_repairable_saved_result() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    let _cwd = CurrentDirectoryGuard::change(dir.path());
    std::fs::write(BOOKMARKS_FILE, "[]").unwrap();
    std::fs::write(FAV_FILE, "[]").unwrap();
    let absent = dir.path().join("missing-folder");
    let folder = FolderEntry {
        label: "Saved folder".into(),
        path: absent.to_string_lossy().into_owned(),
        alias: Some("Projects".into()),
    };
    std::fs::write(FOLDERS_FILE, serde_json::to_vec(&vec![folder]).unwrap()).unwrap();

    let plugin = MissingPlugin::default();
    let found = wait_for_scan(&plugin);
    let absent = absent.to_string_lossy().into_owned();
    assert!(
        found
            .iter()
            .any(|action| { action.label == "Projects" && action.action == absent })
    );
    assert!(
        found
            .iter()
            .any(|action| { action.action == format!("folder:remove:{absent}") })
    );
}

#[test]
fn explicit_scan_reports_unreadable_saved_store_as_uncertain_not_missing() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    let _cwd = CurrentDirectoryGuard::change(dir.path());
    std::fs::write(BOOKMARKS_FILE, "[]").unwrap();
    std::fs::write(FAV_FILE, "[]").unwrap();
    std::fs::write(FOLDERS_FILE, b"{ malformed json").unwrap();

    let plugin = MissingPlugin::default();
    let result = wait_for_scan(&plugin);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].action, "missing:status:uncertain");
    assert!(!result.iter().any(|action| action.action == "noop:"));
    assert!(!result[0].label.contains("malformed json"));
}
