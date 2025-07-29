use multi_launcher::gui::LauncherApp;
use multi_launcher::plugin::PluginManager;
use multi_launcher::actions::Action;
use multi_launcher::settings::Settings;
use once_cell::sync::Lazy;
use std::sync::{Arc, Mutex, atomic::AtomicBool};

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn new_app(ctx: &eframe::egui::Context) -> LauncherApp {
    let mut settings = Settings::default();
    settings.timer_refresh = 0.0;
    let custom_len = 0usize;
    LauncherApp::new(
        ctx,
        Vec::<Action>::new(),
        custom_len,
        PluginManager::new(),
        "actions.json".into(),
        "settings.json".into(),
        settings,
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
fn auto_refresh_case_insensitive() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let ctx = eframe::egui::Context::default();
    let mut app = new_app(&ctx);

    let initial_len = app.results.len();

    app.query = "TiMeR LiSt".into();
    app.handle_auto_refresh();
    assert_ne!(app.results.len(), initial_len);

    let after_timer = app.results.clone();

    app.query = "AlArM LiSt".into();
    app.handle_auto_refresh();
    assert_ne!(app.results, after_timer);
}
