use eframe::egui;
use multi_launcher::{
    actions::Action, gui::LauncherApp, launcher::launch_action, plugin::PluginManager,
    plugins::tempfile::create_file, settings::Settings,
};
use once_cell::sync::Lazy;
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

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
fn tmp_rm_refreshes_results() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let file = create_file().unwrap();
    let ctx = egui::Context::default();
    let actions: Vec<Action> = Vec::new();
    let mut app = new_app(&ctx, actions);

    app.query = "tmp rm".into();
    app.search();
    assert_eq!(app.results.len(), 1);
    let a = app.results[0].clone();

    let mut refresh = false;
    if a.action.starts_with("tempfile:remove:") {
        refresh = true;
    }
    launch_action(&a).unwrap();
    if refresh {
        app.query.push(' ');
        app.search();
        app.query.pop();
        app.search();
    }

    assert!(!app
        .results
        .iter()
        .any(|a| a.action.starts_with("tempfile:remove:")));
    assert!(!file.exists());
}
