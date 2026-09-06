use eframe::egui;
use multi_launcher::actions::Action;
use multi_launcher::gui::{ActivationSource, LauncherApp, set_execute_action_hook};
use multi_launcher::plugin::PluginManager;
use multi_launcher::settings::Settings;
use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicBool, Ordering},
};

static EXECUTION_HOOK_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

struct ActivationTestGuard {
    original_dir: PathBuf,
    _temp_dir: tempfile::TempDir,
    _lock: MutexGuard<'static, ()>,
}

impl ActivationTestGuard {
    fn new() -> Self {
        let lock = EXECUTION_HOOK_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let original_dir = std::env::current_dir().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();
        Self {
            original_dir,
            _temp_dir: temp_dir,
            _lock: lock,
        }
    }
}

impl Drop for ActivationTestGuard {
    fn drop(&mut self) {
        set_execute_action_hook(None);
        let _ = std::env::set_current_dir(&self.original_dir);
    }
}

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
    let _guard = ActivationTestGuard::new();
    let ctx = egui::Context::default();
    let actions = vec![Action {
        label: "test".into(),
        desc: "".into(),
        action: action.into(),
        args: None,
    }];
    let (mut app, flag) = new_app_with_settings(&ctx, actions, Settings::default());
    app.update_paths(
        None,       // plugin_dirs
        None,       // index_paths
        None,       // enabled_plugins
        None,       // enabled_capabilities
        None,       // offscreen_pos
        None,       // enable_toasts
        None,       // show_inline_errors
        None,       // show_error_toasts
        None,       // toast_duration
        None,       // fuzzy_weight
        None,       // usage_weight
        None,       // match_exact
        None,       // follow_mouse
        None,       // static_enabled
        None,       // static_pos
        None,       // static_size
        Some(true), // hide_after_run
        None,       // clear_query_after_run
        None,       // require_confirm_destructive
        None,       // timer_refresh
        None,       // disable_timer_updates
        None,       // preserve_command
        None,       // query_autocomplete
        None,       // net_refresh
        None,       // net_unit
        None,       // screenshot_dir
        None,       // screenshot_save_file
        None,       // screenshot_use_editor
        None,       // screenshot_auto_save
        None,       // always_on_top
        None,       // page_jump
        None,       // note_settings
        None,       // note_panel_default_size
        None,       // note_save_on_close
        None,       // note_confirm_discard_unsaved_changes
        None,       // note_always_overwrite
        None,       // note_images_as_links
        None,       // note_show_details
        None,       // note_more_limit
        None,       // show_dashboard_diagnostics
    );
    flag.store(true, Ordering::SeqCst);
    let a = app.results[0].clone();
    set_execute_action_hook(Some(Box::new(|_| Ok(()))));
    app.activate_action(a, None, ActivationSource::Enter);
    !flag.load(Ordering::SeqCst)
}

#[test]
fn hide_after_run_updates_visibility() {
    assert!(run_action("exec:test"));
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
