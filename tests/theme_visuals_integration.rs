use eframe::egui;
use multi_launcher::{
    gui::LauncherApp,
    plugin::PluginManager,
    settings::{Settings, ThemeMode},
};
use std::sync::{atomic::AtomicBool, Arc};
use tempfile::tempdir;

fn build_app(ctx: &egui::Context, settings: Settings, settings_path: &str) -> LauncherApp {
    let actions = Arc::new(Vec::new());
    let mut plugins = PluginManager::new();
    plugins.reload_from_dirs(
        &[],
        settings.clipboard_limit,
        settings.net_unit,
        false,
        &std::collections::HashMap::new(),
        Arc::clone(&actions),
    );

    LauncherApp::new(
        ctx,
        actions,
        0,
        plugins,
        "actions.json".into(),
        settings_path.into(),
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
fn launcher_app_startup_uses_mapped_theme_visuals() {
    let ctx = egui::Context::default();
    let mut settings = Settings::default();
    settings.theme.mode = ThemeMode::Custom;
    settings.theme.custom_scheme.window_fill.r = 12;
    settings.theme.custom_scheme.window_fill.g = 34;
    settings.theme.custom_scheme.window_fill.b = 56;

    let _app = build_app(&ctx, settings, "settings.json");

    assert_eq!(
        ctx.style().visuals.window_fill,
        egui::Color32::from_rgb(12, 34, 56)
    );
}

#[test]
fn persisted_theme_rehydrates_across_app_instances() {
    let dir = tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");

    let mut initial = Settings::default();
    initial.theme.mode = ThemeMode::Custom;
    initial.theme.custom_scheme.window_fill.r = 90;
    initial.theme.custom_scheme.window_fill.g = 80;
    initial.theme.custom_scheme.window_fill.b = 70;
    initial.save(settings_path.to_str().unwrap()).unwrap();

    let loaded = Settings::load(settings_path.to_str().unwrap()).unwrap();
    let ctx = egui::Context::default();
    let _app = build_app(&ctx, loaded, settings_path.to_str().unwrap());

    assert_eq!(
        ctx.style().visuals.window_fill,
        egui::Color32::from_rgb(90, 80, 70)
    );
}

#[test]
fn legacy_settings_without_theme_keep_default_visuals() {
    let legacy_json = r#"{
        "hotkey":"F2",
        "clipboard_limit":99
    }"#;
    let settings: Settings = serde_json::from_str(legacy_json).unwrap();

    let ctx = egui::Context::default();
    let default_visuals = ctx.style().visuals.clone();
    let _app = build_app(&ctx, settings, "settings.json");

    assert_eq!(ctx.style().visuals.window_fill, default_visuals.window_fill);
}
