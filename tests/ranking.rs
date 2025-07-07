use multi_launcher::gui::LauncherApp;
use multi_launcher::plugin::PluginManager;
use multi_launcher::actions::Action;
use multi_launcher::settings::Settings;
use std::sync::{Arc, atomic::AtomicBool};
use eframe::egui;

fn new_app_with_settings(ctx: &egui::Context, actions: Vec<Action>, settings: Settings) -> LauncherApp {
    let custom_len = actions.len();
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
    )
}

#[test]
fn usage_ranking() {
    let ctx = egui::Context::default();
    let actions = vec![
        Action { label: "foo".into(), desc: "".into(), action: "a".into(), args: None },
        Action { label: "foo".into(), desc: "".into(), action: "b".into(), args: None },
    ];
    let settings = Settings::default();
    let mut app = new_app_with_settings(&ctx, actions, settings);
    app.usage.insert("b".into(), 5);
    app.query = "foo".into();
    app.search();
    assert_eq!(app.results[0].action, "b");
}

#[test]
fn fuzzy_vs_usage_weight() {
    let ctx = egui::Context::default();
    let actions = vec![
        Action { label: "abc".into(), desc: "".into(), action: "a".into(), args: None },
        Action { label: "defabc".into(), desc: "".into(), action: "b".into(), args: None },
    ];
    let mut settings = Settings::default();
    settings.fuzzy_weight = 5.0;
    settings.usage_weight = 1.0;
    let mut app = new_app_with_settings(&ctx, actions, settings);
    app.usage.insert("b".into(), 20);
    app.query = "abc".into();
    app.search();
    assert_eq!(app.results[0].action, "a");
}
