use multi_launcher::actions::Action;
use multi_launcher::gui::LauncherApp;
use multi_launcher::plugin::{Plugin, PluginManager};
use multi_launcher::plugins::convert_panel::ConvertPanelPlugin;
use multi_launcher::settings::Settings;
use eframe::egui;
use std::sync::{atomic::AtomicBool, Arc};

fn new_app(ctx: &egui::Context, actions: Vec<Action>) -> LauncherApp {
    let custom_len = actions.len();
    let mut plugins = PluginManager::new();
    plugins.reload_from_dirs(
        &[],
        Settings::default().clipboard_limit,
        Settings::default().net_unit,
        false,
        &std::collections::HashMap::new(),
        &actions,
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
fn search_conv_opens_panel() {
    let plugin = ConvertPanelPlugin;
    let results = plugin.search("conv");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "convert:panel");

    let ctx = egui::Context::default();
    let actions: Vec<Action> = Vec::new();
    let mut app = new_app(&ctx, actions);
    app.query = "conv".into();
    app.search();
    let idx = app.results.iter().position(|a| a.action == "convert:panel").unwrap();
    app.selected = Some(idx);
    let launch_idx = app.handle_key(egui::Key::Enter);
    assert_eq!(launch_idx, Some(idx));
    if let Some(i) = launch_idx {
        let a = app.results[i].clone();
        if a.action == "convert:panel" {
            app.convert_panel.open();
        }
    }
    assert!(app.convert_panel.open);
}
