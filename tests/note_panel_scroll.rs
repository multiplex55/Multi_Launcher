use eframe::egui;
use multi_launcher::gui::{LauncherApp, NotePanel};
use multi_launcher::plugin::PluginManager;
use multi_launcher::plugins::note::{save_notes, Note};
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
    std::env::set_var("ML_NOTES_DIR", &notes_dir);
    std::env::set_var("HOME", dir.path());
    save_notes(&[]).unwrap();
    dir
}

fn new_app(ctx: &egui::Context) -> LauncherApp {
    let settings = Settings::default();
    LauncherApp::new(
        ctx,
        Arc::new(Vec::new()),
        0,
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
fn long_note_panel_respects_max_height() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);

    let long_content = (0..5000).map(|i| format!("line {i}\n")).collect::<String>();
    let note = Note {
        title: "Long note".into(),
        path: std::path::PathBuf::new(),
        content: long_content,
        tags: Vec::new(),
        links: Vec::new(),
        slug: String::new(),
        alias: None,
    };
    let mut panel = NotePanel::from_note(note);

    ctx.begin_frame(egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1000.0, 1000.0),
        )),
        ..Default::default()
    });
    panel.ui(&ctx, &mut app);
    let _ = ctx.end_frame();

    let rect = ctx
        .memory(|m| m.area_rect(egui::Id::new("Long note")))
        .expect("window rect");
    assert!(rect.height() <= 600.0);
}
