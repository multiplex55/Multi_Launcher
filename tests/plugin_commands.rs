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
    let actions_arc = Arc::new(actions.clone());
    plugins.reload_from_dirs(
        &dirs,
        Settings::default().clipboard_limit,
        Settings::default().net_unit,
        false,
        &std::collections::HashMap::new(),
        actions_arc,
    );
    LauncherApp::new(
        ctx,
        actions,
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

fn new_app_with_settings(
    ctx: &egui::Context,
    actions: Vec<Action>,
    settings: Settings,
) -> LauncherApp {
    let custom_len = actions.len();
    let mut plugins = PluginManager::new();
    let dirs: Vec<String> = Vec::new();
    let plugin_settings = settings.plugin_settings.clone();
    let actions_arc = Arc::new(actions.clone());
    plugins.reload_from_dirs(
        &dirs,
        settings.clipboard_limit,
        settings.net_unit,
        false,
        &plugin_settings,
        actions_arc,
    );
    let enabled_plugins = settings.enabled_plugins.clone();
    LauncherApp::new(
        ctx,
        actions,
        custom_len,
        plugins,
        "actions.json".into(),
        "settings.json".into(),
        settings,
        None,
        None,
        enabled_plugins,
        None,
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    )
}

#[test]
fn empty_query_lists_commands() {
    let ctx = egui::Context::default();
    let actions = vec![Action {
        label: "chrome".into(),
        desc: "web".into(),
        action: "chrome".into(),
        args: None,
    }];
    let mut app = new_app(&ctx, actions);
    app.query.clear();
    app.search();
    assert!(app.results.iter().any(|a| a.label == "help"));
    assert!(app.results.iter().any(|a| a.label == "app chrome"));
}

#[test]
fn query_matches_commands_when_plugins_empty() {
    let ctx = egui::Context::default();
    let actions: Vec<Action> = Vec::new();
    let mut app = new_app(&ctx, actions);
    app.query = "hel".into();
    app.search();
    assert!(app.results.iter().any(|a| a.label == "help"));
}

#[test]
fn disabled_plugin_commands_hidden() {
    let ctx = egui::Context::default();
    let actions: Vec<Action> = Vec::new();
    let mut settings = Settings::default();
    settings.enabled_plugins = Some(std::collections::HashSet::from(["web_search".to_string()]));
    let mut app = new_app_with_settings(&ctx, actions, settings);
    app.query.clear();
    app.search();
    assert!(!app.results.iter().any(|a| a.label == "help"));
}
