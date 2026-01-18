use eframe::egui;
use multi_launcher::actions::Action;
use multi_launcher::gui::LauncherApp;
use multi_launcher::plugin::PluginManager;
use multi_launcher::settings::Settings;
use std::sync::{atomic::AtomicBool, Arc};

fn new_app(ctx: &egui::Context, actions: Vec<Action>) -> LauncherApp {
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
fn mg_queries_return_expected_actions() {
    let ctx = egui::Context::default();
    let actions: Vec<Action> = Vec::new();
    let mut app = new_app(&ctx, actions);

    for query in ["mg", "mouse"] {
        app.query = query.into();
        app.search();
        assert!(app.results.iter().any(|a| a.action == "mg:settings"));
        assert!(app
            .results
            .iter()
            .any(|a| a.action == "mg:gesture_recorder"));
        assert!(app
            .results
            .iter()
            .any(|a| a.action == "mg:add_binding"));
    }
}

#[test]
fn mg_subcommands_resolve_to_actions() {
    let ctx = egui::Context::default();
    let actions: Vec<Action> = Vec::new();
    let mut app = new_app(&ctx, actions);

    let cases = [
        ("mg setting", "mg:settings"),
        ("mg gesture", "mg:gesture_recorder"),
        ("mg add", "mg:add_binding"),
    ];

    for (query, expected) in cases {
        app.query = query.into();
        app.search();
        assert!(app.results.iter().any(|a| a.action == expected));
    }
}
