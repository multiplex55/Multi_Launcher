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
    std::env::set_var("ML_NOTES_DIR", &notes_dir);
    std::env::set_var("HOME", dir.path());
    save_notes(&[]).unwrap();
    dir
}

fn new_app(ctx: &egui::Context) -> LauncherApp {
    let mut settings = Settings::default();
    settings.note_save_on_close = true;
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
fn note_panel_auto_saves_on_close() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    append_note("alpha", "original").unwrap();
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let mut note = load_notes()
        .unwrap()
        .into_iter()
        .find(|n| n.slug == "alpha")
        .unwrap();
    note.content.push_str(" updated");
    let mut panel = NotePanel::from_note(note);

    ctx.begin_frame(egui::RawInput {
        events: vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }],
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        ..Default::default()
    });
    panel.ui(&ctx, &mut app);
    let _ = ctx.end_frame();

    let notes = load_notes().unwrap();
    let note = notes.into_iter().find(|n| n.slug == "alpha").unwrap();
    assert!(note.content.contains("updated"));
}

#[test]
fn note_panel_auto_saves_on_window_close() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let _tmp = setup();
    append_note("beta", "original").unwrap();
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let mut note = load_notes()
        .unwrap()
        .into_iter()
        .find(|n| n.slug == "beta")
        .unwrap();
    note.content.push_str(" updated");
    let title = note.title.clone();
    let mut panel = NotePanel::from_note(note);

    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        },
        |ctx| {
            panel.ui(ctx, &mut app);
        },
    );

    let window_id = egui::Id::new(title);
    let window_rect = ctx.memory(|m| m.area_rect(window_id).unwrap());
    let title_h = ctx.style().spacing.interact_size.y;
    let icon = ctx.style().spacing.icon_width;
    let pad = (title_h - icon) / 2.0;
    let close_rect = egui::Rect::from_min_size(
        egui::pos2(window_rect.right() - pad - icon, window_rect.top() + pad),
        egui::vec2(icon, icon),
    );
    let pos = close_rect.center();

    let _ = ctx.run(
        egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        },
        |ctx| {
            panel.ui(ctx, &mut app);
        },
    );

    let _ = ctx.run(
        egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::default(),
                },
            ],
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        },
        |ctx| {
            panel.ui(ctx, &mut app);
        },
    );

    let notes = load_notes().unwrap();
    let note = notes.into_iter().find(|n| n.slug == "beta").unwrap();
    assert!(note.content.contains("updated"));
}
