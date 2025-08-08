use multi_launcher::actions::Action;
use multi_launcher::gui::LauncherApp;
use multi_launcher::plugin::PluginManager;
use multi_launcher::settings::Settings;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::{Duration, Instant};

fn new_app(ctx: &eframe::egui::Context) -> LauncherApp {
    let actions: Vec<Action> = Vec::new();
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
fn refresh_after_timer_list_command() {
    let ctx = eframe::egui::Context::default();
    let mut app = new_app(&ctx);
    app.query = "timer list".into();
    app.search();
    assert!(app.last_timer_query_flag());
    app.set_last_search_query("old".into());
    app.set_last_timer_update(Instant::now() - Duration::from_secs_f32(app.timer_refresh + 1.0));
    app.maybe_refresh_timer_list();
    assert_eq!(app.get_last_search_query(), app.query);
}

#[test]
fn refresh_after_alarm_list_command() {
    let ctx = eframe::egui::Context::default();
    let mut app = new_app(&ctx);
    app.query = "alarm list".into();
    app.search();
    assert!(app.last_timer_query_flag());
    app.set_last_search_query("old".into());
    app.set_last_timer_update(Instant::now() - Duration::from_secs_f32(app.timer_refresh + 1.0));
    app.maybe_refresh_timer_list();
    assert_eq!(app.get_last_search_query(), app.query);
}

#[test]
fn refresh_while_timer_query_active() {
    let ctx = eframe::egui::Context::default();
    let mut app = new_app(&ctx);
    app.query = "timer list".into();
    app.search();
    assert!(app.last_timer_query_flag());
    app.set_last_timer_update(Instant::now() - Duration::from_secs_f32(app.timer_refresh + 1.0));
    let prev = app.last_timer_update();
    app.maybe_refresh_timer_list();
    assert!(app.last_timer_update() > prev);
}
