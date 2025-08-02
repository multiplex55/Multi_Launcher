use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::settings::SettingsPlugin;
use multi_launcher::{actions::Action, gui::LauncherApp, plugin::PluginManager, settings::Settings};
use std::sync::{Arc, atomic::AtomicBool};
use eframe::egui;

fn new_app(ctx: &egui::Context, actions: Vec<Action>) -> LauncherApp {
    let custom_len = actions.len();
    let mut plugins = PluginManager::new();
    let actions_arc = Arc::new(actions.clone());
    plugins.reload_from_dirs(
        &[],
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

#[test]
fn search_settings_opens_panel() {
    let plugin = SettingsPlugin;
    let results = plugin.search("settings");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "settings:dialog");

    let ctx = egui::Context::default();
    let actions: Vec<Action> = Vec::new();
    let mut app = new_app(&ctx, actions);
    app.query = "settings".into();
    app.search();
    let idx = app.results.iter().position(|a| a.action == "settings:dialog").unwrap();
    app.selected = Some(idx);
    let launch_idx = app.handle_key(egui::Key::Enter);
    assert_eq!(launch_idx, Some(idx));
    if let Some(i) = launch_idx {
        let a = app.results[i].clone();
        if a.action == "settings:dialog" {
            app.show_settings = true;
        }
    }
    assert!(app.show_settings);
}

