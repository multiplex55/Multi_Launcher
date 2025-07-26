use eframe::egui;
use multi_launcher::gui::TodoDialog;
use multi_launcher::plugins::todo::TodoEntry;
use multi_launcher::gui::LauncherApp;
use multi_launcher::plugin::PluginManager;
use multi_launcher::settings::Settings;
use multi_launcher::actions::Action;
use tempfile::tempdir;
use std::sync::{Arc, atomic::AtomicBool};

fn new_app(ctx: &egui::Context) -> LauncherApp {
    LauncherApp::new(
        ctx,
        Vec::<Action>::new(),
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
        TodoEntry { text: "alpha".into(), done: false, priority: 0, tags: vec![] },
        TodoEntry { text: "beta".into(), done: false, priority: 0, tags: vec!["x".into()] },
    ];
    let idx = TodoDialog::filtered_indices(&entries, "beta");
    assert_eq!(idx, vec![1]);
}

#[test]
fn filter_by_tag() {
    let entries = vec![
        TodoEntry { text: "alpha".into(), done: false, priority: 0, tags: vec!["rs3".into()] },
        TodoEntry { text: "beta".into(), done: false, priority: 0, tags: vec!["other".into()] },
    ];
    let idx = TodoDialog::filtered_indices(&entries, "#rs3");
    assert_eq!(idx, vec![0]);
}

#[test]
fn empty_filter_returns_all() {
    let entries = vec![
        TodoEntry { text: "one".into(), done: false, priority: 0, tags: vec![] },
        TodoEntry { text: "two".into(), done: false, priority: 0, tags: vec![] },
    ];
    let idx = TodoDialog::filtered_indices(&entries, "");
    assert_eq!(idx, vec![0, 1]);
}

#[test]
fn enter_adds_todo_without_focus() {
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let ctx = egui::Context::default();
    let _ = ctx.run(Default::default(), |_| {});
    let mut app = new_app(&ctx);
    let mut dlg = TodoDialog::default();
    dlg.open();
    dlg.set_text("task");
    dlg.set_tags("a,b");
    dlg.set_priority(3);

    ctx.input_mut(|i| i.events.push(egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    }));

    dlg.ui(&ctx, &mut app);

    let todos = multi_launcher::plugins::todo::load_todos(multi_launcher::plugins::todo::TODO_FILE).unwrap();
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].text, "task");
    assert_eq!(todos[0].priority, 3);
    assert_eq!(todos[0].tags, vec!["a", "b"]);
}
