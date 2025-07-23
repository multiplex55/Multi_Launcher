use eframe::egui;
use multi_launcher::actions::{save_actions, Action};
use multi_launcher::gui::{LauncherApp, TestWatchEvent};
use multi_launcher::plugin::PluginManager;
use multi_launcher::plugins::bookmarks::{save_bookmarks, BookmarkEntry, BOOKMARKS_FILE};
use multi_launcher::plugins::folders::{save_folders, FolderEntry, FOLDERS_FILE};
use multi_launcher::settings::Settings;
use once_cell::sync::Lazy;
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn new_app(ctx: &egui::Context, actions: Vec<Action>) -> LauncherApp {
    let custom_len = actions.len();
    LauncherApp::new(
        ctx,
        actions,
        custom_len,
        PluginManager::new(),
        "actions.json".into(),
        "settings.json".into(),
        Settings::default(),
        None,
        None,
        None,
        None,
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    )
}

#[test]
fn actions_watcher_sends_event() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let acts = vec![Action {
        label: "one".into(),
        desc: "".into(),
        action: "a".into(),
        args: None,
    }];
    save_actions("actions.json", &acts).unwrap();
    save_folders(FOLDERS_FILE, &[]).unwrap();
    save_bookmarks(BOOKMARKS_FILE, &[]).unwrap();

    let ctx = egui::Context::default();
    let mut app = new_app(&ctx, acts);

    save_actions("actions.json", &[]).unwrap();
    sleep(Duration::from_millis(200));
    let ev = multi_launcher::gui::recv_test_event(app.watch_receiver()).unwrap();
    assert!(matches!(ev, TestWatchEvent::Actions));
}

#[test]
fn folders_watcher_sends_event() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    save_actions("actions.json", &[]).unwrap();
    save_folders(FOLDERS_FILE, &[]).unwrap();
    save_bookmarks(BOOKMARKS_FILE, &[]).unwrap();

    let ctx = egui::Context::default();
    let mut app = new_app(&ctx, Vec::new());

    save_folders(
        FOLDERS_FILE,
        &[FolderEntry {
            label: "l".into(),
            path: "p".into(),
            alias: None,
        }],
    )
    .unwrap();
    sleep(Duration::from_millis(200));
    let ev = multi_launcher::gui::recv_test_event(app.watch_receiver()).unwrap();
    assert!(matches!(ev, TestWatchEvent::Folders));
}

#[test]
fn bookmarks_watcher_sends_event() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    save_actions("actions.json", &[]).unwrap();
    save_folders(FOLDERS_FILE, &[]).unwrap();
    save_bookmarks(BOOKMARKS_FILE, &[]).unwrap();

    let ctx = egui::Context::default();
    let mut app = new_app(&ctx, Vec::new());

    save_bookmarks(
        BOOKMARKS_FILE,
        &[BookmarkEntry {
            url: "u".into(),
            alias: None,
        }],
    )
    .unwrap();
    sleep(Duration::from_millis(200));
    let ev = multi_launcher::gui::recv_test_event(app.watch_receiver()).unwrap();
    assert!(matches!(ev, TestWatchEvent::Bookmarks));
}
