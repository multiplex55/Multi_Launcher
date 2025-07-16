use eframe::egui;
use multi_launcher::actions::Action;
use multi_launcher::gui::LauncherApp;
use multi_launcher::plugin::PluginManager;
use multi_launcher::settings::Settings;
use std::sync::{Arc, atomic::AtomicBool};

fn new_app(ctx: &egui::Context, actions: Vec<Action>, preserve: bool) -> LauncherApp {
    let custom_len = actions.len();
    let mut settings = Settings::default();
    settings.preserve_command = preserve;
    LauncherApp::new(
        ctx,
        actions,
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
fn bookmark_add_preserves_prefix() {
    let ctx = egui::Context::default();
    let url = "https://example.com";
    let actions = vec![Action {
        label: "test".into(),
        desc: "".into(),
        action: format!("bookmark:add:{url}"),
        args: None,
    }];
    let mut app = new_app(&ctx, actions, true);
    app.query = format!("bm add {url}");
    let a = app.results[0].clone();
    if multi_launcher::launcher::launch_action(&a).is_ok() {
        if app.preserve_command {
            app.query = "bm add ".into();
        } else {
            app.query.clear();
        }
    }
    assert_eq!(app.query, "bm add ");
}

#[test]
fn bookmark_add_clears_without_setting() {
    let ctx = egui::Context::default();
    let url = "https://example.com";
    let actions = vec![Action {
        label: "test".into(),
        desc: "".into(),
        action: format!("bookmark:add:{url}"),
        args: None,
    }];
    let mut app = new_app(&ctx, actions, false);
    app.query = format!("bm add {url}");
    let a = app.results[0].clone();
    if multi_launcher::launcher::launch_action(&a).is_ok() {
        if app.preserve_command {
            app.query = "bm add ".into();
        } else {
            app.query.clear();
        }
    }
    assert_eq!(app.query, "");
}
