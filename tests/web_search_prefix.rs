use eframe::egui;
use multi_launcher::actions::Action;
use multi_launcher::gui::LauncherApp;
use multi_launcher::plugin::PluginManager;
use multi_launcher::settings::Settings;
use std::sync::{atomic::AtomicBool, Arc};

fn new_app_with_plugins(ctx: &egui::Context, actions: Vec<Action>) -> LauncherApp {
    let custom_len = actions.len();
    let mut plugins = PluginManager::new();
    let dirs: Vec<String> = Vec::new();
    let actions_arc = Arc::new(actions);
    plugins.reload_from_dirs(
        &dirs,
        Settings::default().clipboard_limit,
        Settings::default().net_unit,
        false,
        &std::collections::HashMap::new(),
        Arc::clone(&actions_arc),
    );
    LauncherApp::new(
        ctx,
        actions_arc,
        custom_len,
        plugins,
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
fn g_prefix_filters_web_search() {
    let ctx = egui::Context::default();
    let actions = vec![Action {
        label: "g hello".into(),
        desc: "test".into(),
        action: "custom".into(),
        args: None,
        preview_text: None,
        risk_level: None,
        icon: None,
    }];
    let mut app = new_app_with_plugins(&ctx, actions);
    app.query = "g hello".into();
    app.search();
    assert_eq!(app.results.len(), 1);
    assert_eq!(
        app.results[0].action,
        "https://www.google.com/search?q=hello"
    );
}
