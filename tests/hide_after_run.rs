use eframe::egui;
use multi_launcher::actions::Action;
use multi_launcher::gui::LauncherApp;
use multi_launcher::plugin::PluginManager;
use multi_launcher::settings::Settings;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

fn new_app_with_settings(
    ctx: &egui::Context,
    actions: Vec<Action>,
    settings: Settings,
) -> (LauncherApp, Arc<AtomicBool>) {
    let custom_len = actions.len();
    let visible = Arc::new(AtomicBool::new(true));
    let actions_arc = Arc::new(actions);
    (
        LauncherApp::new(
            ctx,
            actions_arc,
            custom_len,
            PluginManager::new(),
            "actions.json".into(),
            "settings.json".into(),
            settings,
            None,
            None,
            None,
            None,
            visible.clone(),
            Arc::new(AtomicBool::new(false)),
            visible.clone(),
        ),
        visible,
    )
}

fn run_action(action: &str) -> bool {
    let ctx = egui::Context::default();
    let actions = vec![Action {
        label: "test".into(),
        desc: "".into(),
        action: action.into(),
        args: None,
    }];
    let (mut app, flag) = new_app_with_settings(&ctx, actions, Settings::default());
    app.update_paths(
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(true),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    flag.store(true, Ordering::SeqCst);
    let a = app.results[0].clone();
    if multi_launcher::launcher::launch_action(&a).is_ok() {
        if app.hide_after_run
            && !a.action.starts_with("bookmark:add:")
            && !a.action.starts_with("bookmark:remove:")
            && !a.action.starts_with("folder:add:")
            && !a.action.starts_with("folder:remove:")
            && !a.action.starts_with("calc:")
            && !a.action.starts_with("todo:done:")
        {
            flag.store(false, Ordering::SeqCst);
        }
    }
    !flag.load(Ordering::SeqCst)
}

#[test]
fn hide_after_run_updates_visibility() {
    assert!(run_action("history:clear"));
}

#[test]
fn hide_after_run_not_for_bookmark_add() {
    assert!(!run_action("bookmark:add:https://example.com"));
}

#[test]
fn hide_after_run_not_for_bookmark_remove() {
    assert!(!run_action("bookmark:remove:https://example.com"));
}

#[test]
fn hide_after_run_not_for_folder_add() {
    assert!(!run_action("folder:add:/tmp"));
}

#[test]
fn hide_after_run_not_for_folder_remove() {
    assert!(!run_action("folder:remove:/tmp"));
}

#[test]
fn hide_after_run_not_for_calc_copy() {
    assert!(!run_action("calc:1+2"));
}

#[test]
fn hide_after_run_not_for_todo_done() {
    assert!(!run_action("todo:done:0"));
}
