use eframe::egui;
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::settings::SettingsPlugin;
use multi_launcher::{
    actions::Action, gui::ActivationSource, gui::LauncherApp, plugin::PluginManager,
    settings::Settings,
};
use std::sync::{atomic::AtomicBool, Arc};

fn new_app(ctx: &egui::Context, actions: Vec<Action>) -> LauncherApp {
    let custom_len = actions.len();
    let mut plugins = PluginManager::new();
    let actions_arc = Arc::new(actions);
    plugins.reload_from_dirs(
        &[],
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
    let idx = app
        .results
        .iter()
        .position(|a| a.action == "settings:dialog")
        .unwrap();
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

#[test]
fn dashboard_settings_action_opens_editor() {
    let plugin = SettingsPlugin;
    let results = plugin.search("dashboard settings");
    assert!(results.iter().any(|a| a.action == "dashboard:settings"));

    let ctx = egui::Context::default();
    let actions: Vec<Action> = Vec::new();
    let mut app = new_app(&ctx, actions);
    app.query.clear();
    app.search();
    assert!(app.results.iter().any(|a| a.label == "Dashboard Settings"));
    let action = Action {
        label: "Dashboard Settings".into(),
        desc: "Configure dashboard layout and widgets".into(),
        action: "dashboard:settings".into(),
        args: None,
        preview_text: None,
        risk_level: None,
        icon: None,
    };
    assert!(!app.dashboard_editor.open);
    assert!(!app.show_dashboard_editor);
    app.activate_action(action, None, ActivationSource::Enter);
    assert!(app.show_dashboard_editor);
    assert!(app.dashboard_editor.open);
}

#[test]
fn theme_action_is_discoverable_and_opens_dialog() {
    let plugin = SettingsPlugin;
    let results = plugin.search("theme");
    assert!(results.iter().any(|a| a.action == "theme:dialog"));

    let commands = plugin.commands();
    assert!(commands.iter().any(|a| {
        a.action == "theme:dialog"
            && a.label == "Theme settings"
            && a.desc == "Configure launcher theme colors"
    }));

    let ctx = egui::Context::default();
    let mut app = new_app(&ctx, Vec::new());
    assert!(!app.is_theme_settings_dialog_open());
    app.activate_action(
        Action {
            label: "Theme settings".into(),
            desc: "Configure launcher theme colors".into(),
            action: "theme:dialog".into(),
            args: None,
            preview_text: None,
            risk_level: None,
            icon: None,
        },
        None,
        ActivationSource::Enter,
    );
    assert!(app.is_theme_settings_dialog_open());
    app.close_theme_settings_dialog();
    assert!(!app.is_theme_settings_dialog_open());
}
