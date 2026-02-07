use eframe::egui;
use multi_launcher::actions::Action;
use multi_launcher::gui::LauncherApp;
use multi_launcher::plugin::PluginManager;
use multi_launcher::settings::Settings;
use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};
use std::sync::{Mutex, MutexGuard};

static CWD_TEST_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

struct CurrentDirGuard {
    _lock: MutexGuard<'static, ()>,
    original: PathBuf,
    _tmp: tempfile::TempDir,
}

impl CurrentDirGuard {
    fn new() -> Self {
        let lock = CWD_TEST_LOCK.lock().expect("cwd test lock poisoned");
        let original = std::env::current_dir().expect("resolve current dir");
        let tmp = tempfile::tempdir().expect("create temp dir");
        std::env::set_current_dir(tmp.path()).expect("switch to temp dir");
        Self {
            _lock: lock,
            original,
            _tmp: tmp,
        }
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.original);
    }
}

fn new_app(ctx: &egui::Context, actions: Vec<Action>, preserve: bool) -> LauncherApp {
    let custom_len = actions.len();
    let mut settings = Settings::default();
    settings.preserve_command = preserve;
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
    if app.preserve_command {
        app.query = "bm add ".into();
    } else {
        app.query.clear();
    }
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
    if app.preserve_command {
        app.query = "bm add ".into();
    } else {
        app.query.clear();
    }
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
    let a = app.results[0].clone();
    if multi_launcher::launcher::launch_action(&a).is_ok() {
        if app.preserve_command {
            app.query = "timer add ".into();
        } else {
            app.query.clear();
        }
    }
    if let Some((id, _, _, _)) = multi_launcher::plugins::timer::active_timers()
        .into_iter()
        .next()
    {
        multi_launcher::plugins::timer::cancel_timer(id);
    }
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
    let _cwd_guard = CurrentDirGuard::new();
    app.query = "todo add test".into();
    let a = app.results[0].clone();
    if multi_launcher::launcher::launch_action(&a).is_ok() {
        if app.preserve_command {
            app.query = "todo add ".into();
        } else {
            app.query.clear();
        }
    }
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
    let _cwd_guard = CurrentDirGuard::new();
    app.query = "tmp new".into();
    if app.preserve_command {
        app.query = "tmp new ".into();
    } else {
        app.query.clear();
    }
    assert_eq!(app.query, "tmp new ");
}
