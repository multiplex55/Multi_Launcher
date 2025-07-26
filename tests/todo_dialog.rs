use eframe::egui;
use multi_launcher::gui::LauncherApp;
use multi_launcher::gui::TodoDialog;
use multi_launcher::plugin::PluginManager;
use multi_launcher::plugins::todo::TodoEntry;
use multi_launcher::plugins::todo::{load_todos, TODO_FILE};
use multi_launcher::settings::Settings;
use std::sync::{atomic::AtomicBool, Arc};
use tempfile::tempdir;

fn new_app(ctx: &egui::Context) -> LauncherApp {
    LauncherApp::new(
        ctx,
        Vec::new(),
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
fn filter_by_text() {
    let entries = vec![
        TodoEntry {
            text: "alpha".into(),
            done: false,
            priority: 0,
            tags: vec![],
        },
        TodoEntry {
            text: "beta".into(),
            done: false,
            priority: 0,
            tags: vec!["x".into()],
        },
    ];
    let idx = TodoDialog::filtered_indices(&entries, "beta");
    assert_eq!(idx, vec![1]);
}

#[test]
fn filter_by_tag() {
    let entries = vec![
        TodoEntry {
            text: "alpha".into(),
            done: false,
            priority: 0,
            tags: vec!["rs3".into()],
        },
        TodoEntry {
            text: "beta".into(),
            done: false,
            priority: 0,
            tags: vec!["other".into()],
        },
    ];
    let idx = TodoDialog::filtered_indices(&entries, "#rs3");
    assert_eq!(idx, vec![0]);
}

#[test]
fn empty_filter_returns_all() {
    let entries = vec![
        TodoEntry {
            text: "one".into(),
            done: false,
            priority: 0,
            tags: vec![],
        },
        TodoEntry {
            text: "two".into(),
            done: false,
            priority: 0,
            tags: vec![],
        },
    ];
    let idx = TodoDialog::filtered_indices(&entries, "");
    assert_eq!(idx, vec![0, 1]);
}

#[test]
fn enter_adds_todo_without_focus() {
    let dir = tempdir().unwrap();
    std::env::set_current_dir(&dir.path()).unwrap();

    let mut app_ctx = None;
    let mut dlg = TodoDialog::default();
    dlg.open();
    dlg.set_text("demo");

    egui::__run_test_ui(|ui| {
        let ctx = ui.ctx().clone();
        app_ctx = Some(ctx.clone());
        ctx.input_mut(|i| {
            i.events.push(egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            });
        });
        let mut app = new_app(&ctx);
        dlg.ui(&ctx, &mut app);
    });
    let ctx = app_ctx.unwrap();

    let todos = load_todos(TODO_FILE).unwrap();
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].text, "demo");
    assert!(dlg.text().is_empty());
    assert!(!ctx.input(|i| i.key_pressed(egui::Key::Enter)));
}
