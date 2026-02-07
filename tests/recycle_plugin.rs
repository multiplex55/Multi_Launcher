use eframe::egui;
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::recycle::RecyclePlugin;
use multi_launcher::{
    actions::Action,
    gui::{LauncherApp, WatchEvent},
    launcher::launch_action,
    plugin::PluginManager,
    settings::Settings,
};
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
fn search_returns_action() {
    let plugin = RecyclePlugin;
    let results = plugin.search("rec");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "Clean Recycle Bin");
    assert_eq!(results[0].action, "recycle:clean");
}

#[test]
fn command_returns_immediately_and_cleans() {
    let ctx = egui::Context::default();
    let actions: Vec<Action> = Vec::new();
    let mut app = new_app(&ctx, actions);
    app.query = "rec".into();
    app.search();
    assert_eq!(app.results.len(), 1);
    let a = app.results[0].clone();
    let rx = app.watch_receiver();
    // Clear any events that may have fired during app initialization
    while rx.try_recv().is_ok() {}
    let start = std::time::Instant::now();
    launch_action(&a).unwrap();
    assert!(start.elapsed() < std::time::Duration::from_millis(100));
    let start_wait = std::time::Instant::now();
    loop {
        let remaining = match std::time::Duration::from_secs(3).checked_sub(start_wait.elapsed()) {
            Some(dur) => dur,
            None => panic!("unexpected event"),
        };
        match rx.recv_timeout(remaining) {
            Ok(WatchEvent::Recycle(_)) => break,
            Ok(_) => continue,
            Err(_) => panic!("unexpected event"),
        }
    }
}

#[test]
fn search_has_metadata() {
    let plugin = RecyclePlugin;
    let results = plugin.search("rec");
    assert!(results[0].preview_text.is_some());
    assert!(results[0].risk_level.is_some());
    assert!(results[0].icon.is_some());
}
