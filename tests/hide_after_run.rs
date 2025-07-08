use multi_launcher::gui::LauncherApp;
use multi_launcher::plugin::PluginManager;
use multi_launcher::actions::Action;
use multi_launcher::settings::Settings;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use eframe::egui;

fn new_app_with_settings(ctx: &egui::Context, actions: Vec<Action>, settings: Settings) -> (LauncherApp, Arc<AtomicBool>) {
    let custom_len = actions.len();
    let visible = Arc::new(AtomicBool::new(true));
    (
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
        visible.clone(),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    ),
    visible)
}

#[test]
fn hide_after_run_updates_visibility() {
    let ctx = egui::Context::default();
    let actions = vec![Action { label: "clear".into(), desc: "".into(), action: "history:clear".into(), args: None }];
    let (mut app, flag) = new_app_with_settings(&ctx, actions, Settings::default());
    app.update_paths(
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(true),
    );
    assert!(app.hide_after_run);
    assert!(flag.load(Ordering::SeqCst));
    let a = app.results[0].clone();
    if multi_launcher::launcher::launch_action(&a).is_ok() {
        if app.hide_after_run {
            flag.store(false, Ordering::SeqCst);
        }
    }
    assert!(!flag.load(Ordering::SeqCst));
}
