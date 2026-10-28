use multi_launcher::dashboard::config::{DashboardConfig, GridConfig, OverflowMode, SlotConfig};
use multi_launcher::dashboard::layout::normalize_slots;
use multi_launcher::dashboard::widgets::WidgetRegistry;
use multi_launcher::settings::Settings;

#[test]
fn dashboard_config_defaults_present() {
    let cfg = DashboardConfig::default();
    assert_eq!(cfg.version, 1);
    assert_eq!(cfg.grid.rows, GridConfig::default().rows);
    assert!(!cfg.slots.is_empty());
}

#[test]
fn dashboard_enabled_by_default() {
    let settings = Settings::default();
    assert!(settings.dashboard.enabled);
    assert!(settings.dashboard.show_when_query_empty);
}

#[test]
fn gesture_widgets_registered() {
    let registry = WidgetRegistry::with_defaults();
    assert!(registry.contains("gesture_cheat_sheet"));
    assert!(registry.contains("gesture_recent"));
    assert!(registry.contains("gesture_health"));
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
fn legacy_todo_widgets_are_migrated() {
    let mut cfg = DashboardConfig {
        version: 1,
        grid: GridConfig { rows: 1, cols: 2 },
        slots: vec![
            SlotConfig::with_widget("todo_list", 0, 0),
            SlotConfig::with_widget("todo_summary", 0, 1),
        ],
    };
    let registry = WidgetRegistry::with_defaults();
    let warnings = cfg.sanitize(&registry);
    assert_eq!(cfg.slots.len(), 2);
    assert!(warnings.iter().any(|w| w.contains("todo_list")));
    assert!(warnings.iter().any(|w| w.contains("todo_summary")));
    for slot in cfg.slots {
        assert_eq!(slot.widget, "todo");
    }
}

#[test]
fn null_settings_are_defaulted() {
    let mut cfg = DashboardConfig {
        version: 1,
        grid: GridConfig { rows: 1, cols: 1 },
        slots: vec![SlotConfig {
            id: None,
            widget: "weather_site".into(),
            row: 0,
            col: 0,
            row_span: 1,
            col_span: 1,
            settings: serde_json::Value::Null,
            overflow: OverflowMode::Scroll,
        }],
    };
    let registry = WidgetRegistry::with_defaults();
    let warnings = cfg.sanitize(&registry);
    assert!(warnings.is_empty());
    assert_eq!(
        cfg.slots[0].settings,
        registry.default_settings("weather_site").unwrap()
    );
}
