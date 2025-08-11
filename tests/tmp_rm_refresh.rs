use eframe::egui;
use multi_launcher::{
    actions::Action, gui::LauncherApp, launcher::launch_action, plugin::PluginManager,
    plugins::tempfile::{clear_files, create_file}, settings::Settings,
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
    std::env::set_var("ML_TMP_DIR", dir.path());

    clear_files().unwrap();

    let file = create_file().unwrap();
    let ctx = egui::Context::default();
    let actions: Vec<Action> = Vec::new();
    let mut app = new_app(&ctx, actions);

    app.query = "tmp rm".into();
    app.search();
    let remove_action = format!("tempfile:remove:{}", file.to_string_lossy());
    let a = app
        .results
        .iter()
        .find(|a| a.action == remove_action)
        .cloned()
        .expect("missing tempfile remove action");

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
        .any(|a| a.action == remove_action));
    assert!(!file.exists());
}
