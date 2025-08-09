use eframe::egui;
use multi_launcher::actions::Action;
use multi_launcher::gui::{LauncherApp, APP_PREFIX};
use multi_launcher::plugin::PluginManager;
use multi_launcher::settings::Settings;
use std::sync::{atomic::AtomicBool, Arc};

fn new_app(ctx: &egui::Context, actions: Vec<Action>) -> LauncherApp {
    let custom_len = actions.len();
    LauncherApp::new(
        ctx,
        Arc::new(actions),
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
fn arrow_keys_update_selection() {
    let ctx = egui::Context::default();
    let acts = vec![
        Action {
            label: "one".into(),
            desc: "".into(),
            action: "one".into(),
            args: None,
        },
        Action {
            label: "two".into(),
            desc: "".into(),
            action: "two".into(),
            args: None,
        },
    ];
    let mut app = new_app(&ctx, acts);
    app.query = APP_PREFIX.into();
    app.search();
    assert_eq!(app.selected, None);
    app.handle_key(egui::Key::ArrowDown);
    assert_eq!(app.selected, Some(0));
    app.handle_key(egui::Key::ArrowDown);
    assert_eq!(app.selected, Some(1));
    app.handle_key(egui::Key::ArrowUp);
    assert_eq!(app.selected, Some(0));
}

#[test]
fn enter_returns_selected_index() {
    let ctx = egui::Context::default();
    let acts = vec![
        Action {
            label: "one".into(),
            desc: "".into(),
            action: "one".into(),
            args: None,
        },
        Action {
            label: "two".into(),
            desc: "".into(),
            action: "two".into(),
            args: None,
        },
    ];
    let mut app = new_app(&ctx, acts);
    app.query = APP_PREFIX.into();
    app.search();
    app.handle_key(egui::Key::ArrowDown);
    let idx = app.handle_key(egui::Key::Enter);
    assert_eq!(idx, Some(0));
}

#[test]
fn page_keys_update_selection() {
    let ctx = egui::Context::default();
    let acts: Vec<Action> = (0..10)
        .map(|i| Action {
            label: format!("act{i}"),
            desc: "".into(),
            action: format!("act{i}"),
            args: None,
        })
        .collect();
    let mut app = new_app(&ctx, acts);
    app.query = APP_PREFIX.into();
    app.search();
    assert_eq!(app.selected, None);
    app.handle_key(egui::Key::PageDown);
    assert_eq!(app.selected, Some(0));
    app.handle_key(egui::Key::PageDown);
    assert_eq!(app.selected, Some(5));
    app.handle_key(egui::Key::PageDown);
    assert_eq!(app.selected, Some(9));
    app.handle_key(egui::Key::PageUp);
    assert_eq!(app.selected, Some(4));
    app.handle_key(egui::Key::PageUp);
    assert_eq!(app.selected, Some(0));
    app.handle_key(egui::Key::PageUp);
    assert_eq!(app.selected, Some(0));
}

#[test]
fn page_keys_respect_setting() {
    let ctx = egui::Context::default();
    let acts: Vec<Action> = (0..10)
        .map(|i| Action {
            label: format!("act{i}"),
            desc: "".into(),
            action: format!("act{i}"),
            args: None,
        })
        .collect();
    let mut settings = Settings::default();
    settings.page_jump = 3;
    let custom_len = acts.len();
    let mut app = LauncherApp::new(
        &ctx,
        Arc::new(acts),
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
    );
    app.query = APP_PREFIX.into();
    app.search();
    assert_eq!(app.selected, None);
    app.handle_key(egui::Key::PageDown);
    assert_eq!(app.selected, Some(0));
    app.handle_key(egui::Key::PageDown);
    assert_eq!(app.selected, Some(3));
    app.handle_key(egui::Key::PageUp);
    assert_eq!(app.selected, Some(0));
}
