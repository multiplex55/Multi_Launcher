use multi_launcher::actions::Action;
use multi_launcher::gui::{ActivationSource, LauncherApp};
use multi_launcher::history::HistoryEntry;
use multi_launcher::history::{append_history, clear_history, get_history};
use multi_launcher::plugin::PluginManager;
use multi_launcher::settings::Settings;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tempfile::tempdir;

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
fn clear_history_empties_file() {
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entry = HistoryEntry {
        query: "test".into(),
        query_lc: String::new(),
        action: Action {
            label: "l".into(),
            desc: "".into(),
            action: "run".into(),
            args: None,
            preview_text: None,
            risk_level: None,
            icon: None,
        },
        source: None,
        timestamp: 0,
    };
    append_history(entry, 10).unwrap();
    assert!(!get_history().is_empty());

    clear_history().unwrap();
    assert!(get_history().is_empty());

    let content = std::fs::read_to_string(dir.path().join("history.json")).unwrap();
    assert_eq!(content.trim(), "[]");
}

#[test]
fn plugin_clear_action() {
    use multi_launcher::plugin::Plugin;
    use multi_launcher::plugins::history::HistoryPlugin;

    let plugin = HistoryPlugin;
    let results = plugin.search("hi clear");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "history:clear");
}

#[test]
fn gesture_activation_increments_history() {
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    clear_history().unwrap();

    let ctx = eframe::egui::Context::default();
    let mut app = new_app(&ctx);
    app.require_confirm_destructive = false;

    let action = Action {
        label: "Clear history".into(),
        desc: "".into(),
        action: "history:clear".into(),
        args: None,
        preview_text: None,
        risk_level: None,
        icon: None,
    };

    app.activate_action(action, None, ActivationSource::Gesture);

    let history = get_history();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].source.as_deref(), Some("gesture"));
}

#[test]
fn gesture_query_action_sets_query_without_execution() {
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    clear_history().unwrap();

    let ctx = eframe::egui::Context::default();
    let mut app = new_app(&ctx);
    app.clear_query_after_run = false;

    let action = Action {
        label: "Query".into(),
        desc: "".into(),
        action: "query:hello".into(),
        args: None,
        preview_text: None,
        risk_level: None,
        icon: None,
    };

    app.activate_action(action, None, ActivationSource::Gesture);

    assert_eq!(app.query, "hello");
    assert!(app.error.is_none());
    assert!(get_history().is_empty());
}
