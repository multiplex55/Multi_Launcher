use eframe::egui;
use multi_launcher::actions::Action;
use multi_launcher::gui::{ActivationSource, LauncherApp, set_execute_action_hook};
use multi_launcher::plugin::PluginManager;
use multi_launcher::settings::Settings;
use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::sync::{Arc, atomic::AtomicBool};
use std::sync::{Mutex, MutexGuard};

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

fn new_app(ctx: &egui::Context, actions: Vec<Action>, preserve: bool) -> LauncherApp {
    let custom_len = actions.len();
    let settings = Settings {
        preserve_command: preserve,
        ..Default::default()
    };
    LauncherApp::new(
        ctx,
        Arc::new(actions),
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

fn activate_successfully(app: &mut LauncherApp) {
    let _guard = ActivationTestGuard::new();
    let action = app.results[0].clone();
    set_execute_action_hook(Some(Box::new(|_| Ok(()))));
    app.activate_action(action, None, ActivationSource::Enter);
}

#[test]
fn bookmark_add_preserves_prefix() {
    let ctx = egui::Context::default();
    let url = "https://example.com";
    let actions = vec![Action {
        label: "test".into(),
        desc: "".into(),
        action: format!("bookmark:add:{url}"),
        args: None,
    }];
    let mut app = new_app(&ctx, actions, true);
    app.query = format!("bm add {url}");
    activate_successfully(&mut app);
    assert_eq!(app.query, "bm add ");
}

#[test]
fn bookmark_add_clears_without_setting() {
    let ctx = egui::Context::default();
    let url = "https://example.com";
    let actions = vec![Action {
        label: "test".into(),
        desc: "".into(),
        action: format!("bookmark:add:{url}"),
        args: None,
    }];
    let mut app = new_app(&ctx, actions, false);
    app.query = format!("bm add {url}");
    activate_successfully(&mut app);
    assert_eq!(app.query, "");
}

#[test]
fn timer_add_preserves_prefix() {
    let ctx = egui::Context::default();
    let actions = vec![Action {
        label: "t".into(),
        desc: "".into(),
        action: "timer:start:1s".into(),
        args: None,
    }];
    let mut app = new_app(&ctx, actions, true);
    app.query = "timer add 1s".into();
    activate_successfully(&mut app);
    assert_eq!(app.query, "timer add ");
}

#[test]
fn todo_add_preserves_prefix() {
    let ctx = egui::Context::default();
    let actions = vec![Action {
        label: "todo".into(),
        desc: "".into(),
        action: "todo:add:test|0|".into(),
        args: None,
    }];
    let mut app = new_app(&ctx, actions, true);
    app.query = "todo add test".into();
    activate_successfully(&mut app);
    assert_eq!(app.query, "todo add ");
}

#[test]
fn tmp_new_preserves_prefix() {
    let ctx = egui::Context::default();
    let actions = vec![Action {
        label: "tmp".into(),
        desc: "".into(),
        action: "tempfile:new".into(),
        args: None,
    }];
    let mut app = new_app(&ctx, actions, true);
    app.query = "tmp new".into();
    activate_successfully(&mut app);
    assert_eq!(app.query, "tmp new ");
}
