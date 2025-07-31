use multi_launcher::actions::Action;
use multi_launcher::gui::LauncherApp;
use multi_launcher::plugin::PluginManager;
use multi_launcher::plugins::stopwatch::{refresh_rate, start_stopwatch_named, stop_stopwatch};
use multi_launcher::settings::Settings;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::{Duration, Instant};

fn new_app(ctx: &eframe::egui::Context) -> LauncherApp {
    let actions: Vec<Action> = Vec::new();
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
fn refresh_after_sw_list_command() {
    let ctx = eframe::egui::Context::default();
    let mut app = new_app(&ctx);
    let id = start_stopwatch_named(Some("test".into()));
    app.query = "sw list".into();
    app.search();
    assert!(app.last_stopwatch_query_flag());
    app.set_last_search_query("old".into());
    let rate = refresh_rate();
    app.set_last_stopwatch_update(Instant::now() - Duration::from_secs_f32(rate + 0.1));
    app.maybe_refresh_stopwatch_list();
    assert_eq!(app.get_last_search_query(), app.query);
    stop_stopwatch(id);
}

#[test]
fn refresh_while_sw_query_active() {
    let ctx = eframe::egui::Context::default();
    let mut app = new_app(&ctx);
    let id = start_stopwatch_named(Some("test".into()));
    app.query = "sw list".into();
    app.search();
    assert!(app.last_stopwatch_query_flag());
    let rate = refresh_rate();
    app.set_last_stopwatch_update(Instant::now() - Duration::from_secs_f32(rate + 0.1));
    let prev = app.last_stopwatch_update();
    app.maybe_refresh_stopwatch_list();
    assert!(app.last_stopwatch_update() > prev);
    stop_stopwatch(id);
}
