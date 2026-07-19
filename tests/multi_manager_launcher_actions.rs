use eframe::egui;
use multi_launcher::actions::Action;
use multi_launcher::gui::{ActivationSource, LauncherApp};
use multi_launcher::plugin::PluginManager;
use multi_launcher::settings::{MultiManagerSettings, Settings};
use std::path::Path;
use std::sync::{Arc, atomic::AtomicBool};

fn action(id: &str) -> Action {
    Action {
        label: id.into(),
        desc: "MultiManager".into(),
        action: id.into(),
        args: None,
    }
}

fn new_app(settings: Settings) -> LauncherApp {
    let ctx = egui::Context::default();
    LauncherApp::new(
        &ctx,
        Arc::new(Vec::new()),
        0,
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

fn settings_with_multi_manager_paths(dir: &Path) -> Settings {
    Settings {
        multi_manager: MultiManagerSettings {
            enabled: false,
            workspaces_path: dir.join("workspaces.json").display().to_string(),
            bindings_path: dir.join("bindings.json").display().to_string(),
            ..MultiManagerSettings::default()
        },
        ..Settings::default()
    }
}

#[test]
fn activating_mm_open_opens_multi_manager_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let settings = settings_with_multi_manager_paths(dir.path());
    let mut app = new_app(settings);

    app.activate_action(action("mm:open"), None, ActivationSource::Enter);

    assert!(app.multi_manager_dialog.open);
}

#[test]
fn activating_mm_settings_opens_multi_manager_settings_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let settings = settings_with_multi_manager_paths(dir.path());
    let mut app = new_app(settings);

    app.activate_action(action("mm:settings"), None, ActivationSource::Enter);

    assert!(app.multi_manager_settings_dialog.open);
}

#[test]
fn activating_mm_save_reports_success_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let settings = settings_with_multi_manager_paths(dir.path());
    let mut app = new_app(settings);
    app.enable_toasts = true;

    app.activate_action(action("mm:save"), None, ActivationSource::Enter);

    assert!(dir.path().join("workspaces.json").exists());
    assert!(app.error.is_none());
}

#[test]
fn activating_mm_save_reports_error_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("not_a_dir");
    std::fs::write(blocker.as_path(), "block parent creation").unwrap();
    let settings = Settings {
        multi_manager: MultiManagerSettings {
            enabled: false,
            workspaces_path: blocker.join("workspaces.json").display().to_string(),
            bindings_path: dir.path().join("bindings.json").display().to_string(),
            ..MultiManagerSettings::default()
        },
        ..Settings::default()
    };
    let mut app = new_app(settings);
    app.show_inline_errors = true;

    app.activate_action(action("mm:save"), None, ActivationSource::Enter);

    assert!(
        app.error
            .as_deref()
            .is_some_and(|msg| msg.contains("Failed to save MultiManager workspaces"))
    );
}

#[test]
fn activating_mm_send_all_home_reports_success_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let settings = settings_with_multi_manager_paths(dir.path());
    let mut app = new_app(settings);
    app.enable_toasts = true;

    app.activate_action(action("mm:send-all-home"), None, ActivationSource::Enter);

    assert!(app.error.is_none());
}

#[test]
fn activating_mm_reconnect_reports_success_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let settings = settings_with_multi_manager_paths(dir.path());
    let mut app = new_app(settings);
    app.enable_toasts = true;

    app.activate_action(action("mm:reconnect"), None, ActivationSource::Enter);

    assert!(app.error.is_none());
}
