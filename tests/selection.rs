use multi_launcher::gui::{LauncherApp, APP_PREFIX};
use multi_launcher::plugin::PluginManager;
use multi_launcher::actions::Action;
use multi_launcher::settings::Settings;
use std::sync::{Arc, atomic::AtomicBool};
use eframe::egui;

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
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    )
}

#[test]
fn arrow_keys_update_selection() {
    let ctx = egui::Context::default();
    let acts = vec![
        Action { label: "one".into(), desc: "".into(), action: "one".into(), args: None },
        Action { label: "two".into(), desc: "".into(), action: "two".into(), args: None },
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
        Action { label: "one".into(), desc: "".into(), action: "one".into(), args: None },
        Action { label: "two".into(), desc: "".into(), action: "two".into(), args: None },
    ];
    let mut app = new_app(&ctx, acts);
    app.query = APP_PREFIX.into();
    app.search();
    app.handle_key(egui::Key::ArrowDown);
    let idx = app.handle_key(egui::Key::Enter);
    assert_eq!(idx, Some(0));
}
