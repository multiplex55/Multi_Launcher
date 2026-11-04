use eframe::egui;
use multi_launcher::gui::{LauncherApp, NotePanel};
use multi_launcher::plugin::PluginManager;
use multi_launcher::plugins::note::{append_note, load_notes, save_notes};
use multi_launcher::settings::Settings;
use once_cell::sync::Lazy;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn setup() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    let notes_dir = dir.path().join("notes");
    std::fs::create_dir_all(&notes_dir).unwrap();
    unsafe { std::env::set_var("ML_NOTES_DIR", &notes_dir) };
    unsafe { std::env::set_var("HOME", dir.path()) };
    save_notes(&[]).unwrap();
    dir
}

fn new_app(ctx: &egui::Context, data_dir: &std::path::Path) -> LauncherApp {
    let mut settings = Settings {
        note_save_on_close: true,
        ..Default::default()
    };
    settings.paste_sequence.next_hotkey = None;
    settings.paste_sequence.cancel_hotkey = None;
    LauncherApp::new(
        ctx,
        Arc::new(Vec::new()),
        0,
        PluginManager::new(),
        data_dir.join("actions.json").to_string_lossy().into_owned(),
        data_dir
            .join("settings.json")
            .to_string_lossy()
            .into_owned(),
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
fn note_panel_auto_saves_on_close() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let tmp = setup();
    append_note("alpha", "original").unwrap();
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx, tmp.path());

    let mut note = load_notes()
        .unwrap()
        .into_iter()
        .find(|n| n.slug == "alpha")
        .unwrap();
    note.content.push_str(" updated");
    let panel = NotePanel::from_note(note);
    app.push_note_panel(panel);

    app.close_front_dialog();

    let notes = load_notes().unwrap();
    let note = notes.into_iter().find(|n| n.slug == "alpha").unwrap();
    assert!(note.content.contains("updated"));
}

#[test]
fn note_panel_remains_closable_after_auto_save_failure_then_retry() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = setup();
    let notes_dir = dir.path().join("notes");
    append_note("alpha", "original").unwrap();
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx, dir.path());

    let mut note = load_notes()
        .unwrap()
        .into_iter()
        .find(|note| note.slug == "alpha")
        .unwrap();
    note.content.push_str(" updated");
    app.push_note_panel(NotePanel::from_note(note));

    let invalid_notes_dir = dir.path().join("not-a-directory.md");
    std::fs::write(&invalid_notes_dir, "not a directory").unwrap();
    unsafe { std::env::set_var("ML_NOTES_DIR", &invalid_notes_dir) };
    assert!(app.close_front_dialog());

    unsafe { std::env::set_var("ML_NOTES_DIR", &notes_dir) };
    assert!(
        app.close_front_dialog(),
        "the failed first save must leave the note panel open for a successful retry"
    );
    let saved = load_notes()
        .unwrap()
        .into_iter()
        .find(|note| note.slug == "alpha")
        .unwrap();
    assert!(saved.content.contains("updated"));
}
