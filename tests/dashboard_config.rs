use multi_launcher::actions::Action;
use multi_launcher::dashboard::config::{DashboardConfig, GridConfig, SlotConfig};
use multi_launcher::dashboard::layout::normalize_slots;
use multi_launcher::dashboard::widgets::WidgetRegistry;
use multi_launcher::gui::{ActivationSource, LauncherApp};
use multi_launcher::plugin::PluginManager;
use multi_launcher::settings::Settings;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn new_app(ctx: &eframe::egui::Context) -> LauncherApp {
    LauncherApp::new(
        ctx,
        Arc::new(Vec::new()),
        0,
        PluginManager::new(),
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
fn dashboard_config_defaults_present() {
    let cfg = DashboardConfig::default();
    assert_eq!(cfg.version, 1);
    assert_eq!(cfg.grid.rows, GridConfig::default().rows);
    assert!(!cfg.slots.is_empty());
}

#[test]
fn unknown_widgets_removed_during_normalization() {
    let mut cfg = DashboardConfig {
        version: 1,
        grid: GridConfig { rows: 2, cols: 2 },
        slots: vec![SlotConfig::with_widget("does_not_exist", 0, 0)],
    };
    let registry = WidgetRegistry::with_defaults();
    let warnings = cfg.sanitize(&registry);
    let (slots, _) = normalize_slots(&cfg, &registry);
    assert!(slots.is_empty());
    assert!(!warnings.is_empty());
}

#[test]
fn layout_clamps_to_grid_and_prevents_overlap() {
    let cfg = DashboardConfig {
        version: 1,
        grid: GridConfig { rows: 1, cols: 1 },
        slots: vec![
            SlotConfig::with_widget("weather_site", 0, 0),
            SlotConfig::with_widget("weather_site", 0, 0),
            SlotConfig::with_widget("weather_site", 5, 5),
        ],
    };
    let registry = WidgetRegistry::with_defaults();
    let (slots, warnings) = normalize_slots(&cfg, &registry);
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].row_span, 1);
    assert_eq!(slots[0].col_span, 1);
    assert!(!warnings.is_empty());
}

#[test]
fn activation_applies_query_override_first() {
    let ctx = eframe::egui::Context::default();
    let mut app = new_app(&ctx);
    app.query = "start".into();
    app.clear_query_after_run = false;
    app.hide_after_run = false;
    let action = Action {
        label: "Query".into(),
        desc: "".into(),
        action: "query:after".into(),
        args: None,
    };
    app.activate_action(action, Some("override".into()), ActivationSource::Dashboard);
    assert_eq!(app.query, "after");
    assert!(app.move_cursor_end_flag());
}

#[test]
fn should_show_dashboard_when_empty_query() {
    let ctx = eframe::egui::Context::default();
    let mut app = new_app(&ctx);
    app.dashboard_enabled = true;
    app.dashboard_show_when_empty = true;
    app.query.clear();
    let trimmed = app.query.trim().to_string();
    assert!(app.should_show_dashboard(&trimmed));
}
