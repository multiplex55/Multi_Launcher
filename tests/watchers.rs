use eframe::egui;
use multi_launcher::actions::{Action, save_actions};
use multi_launcher::gui::{LauncherApp, TestWatchEvent};
use multi_launcher::plugin::PluginManager;
use multi_launcher::plugins::bookmarks::{BOOKMARKS_FILE, BookmarkEntry, save_bookmarks};
use multi_launcher::plugins::folders::{FOLDERS_FILE, FolderEntry, save_folders};
use multi_launcher::settings::Settings;
use std::sync::{Arc, atomic::AtomicBool};
use std::time::Duration;

mod common;
use common::ScopedWorkspace;

fn new_app(ctx: &egui::Context, actions: Vec<Action>) -> LauncherApp {
    let custom_len = actions.len();
    let mut settings = Settings::default();
    settings.paste_sequence.next_hotkey = None;
    settings.paste_sequence.cancel_hotkey = None;
    LauncherApp::new(
        ctx,
        Arc::new(actions),
        custom_len,
        PluginManager::new(),
        "actions.json".into(),
        "settings.json".into(),
        settings,
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
    let _workspace = ScopedWorkspace::new();

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
    let app = new_app(&ctx, acts);

    save_actions("actions.json", &[]).unwrap();
    let ev =
        multi_launcher::gui::recv_test_event_timeout(app.watch_receiver(), Duration::from_secs(3))
            .unwrap();
    assert!(matches!(ev, TestWatchEvent::Actions));
}

#[test]
fn folders_watcher_sends_event() {
    let _workspace = ScopedWorkspace::new();

    save_actions("actions.json", &[]).unwrap();
    save_folders(FOLDERS_FILE, &[]).unwrap();
    save_bookmarks(BOOKMARKS_FILE, &[]).unwrap();

    let ctx = egui::Context::default();
    let app = new_app(&ctx, Vec::new());

    save_folders(
        FOLDERS_FILE,
        &[FolderEntry {
            label: "l".into(),
            path: "p".into(),
            alias: None,
        }],
    )
    .unwrap();
    let ev =
        multi_launcher::gui::recv_test_event_timeout(app.watch_receiver(), Duration::from_secs(3))
            .unwrap();
    assert!(matches!(ev, TestWatchEvent::Folders));
}

#[test]
fn bookmarks_watcher_sends_event() {
    let _workspace = ScopedWorkspace::new();

    save_actions("actions.json", &[]).unwrap();
    save_folders(FOLDERS_FILE, &[]).unwrap();
    save_bookmarks(BOOKMARKS_FILE, &[]).unwrap();

    let ctx = egui::Context::default();
    let app = new_app(&ctx, Vec::new());

    save_bookmarks(
        BOOKMARKS_FILE,
        &[BookmarkEntry {
            url: "u".into(),
            alias: None,
        }],
    )
    .unwrap();
    let ev =
        multi_launcher::gui::recv_test_event_timeout(app.watch_receiver(), Duration::from_secs(3))
            .unwrap();
    assert!(matches!(ev, TestWatchEvent::Bookmarks));
}
