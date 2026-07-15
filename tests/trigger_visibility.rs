use eframe::egui;
use multi_launcher::actions::Action;
use multi_launcher::hotkey::{Hotkey, HotkeyTrigger};
use multi_launcher::plugin::PluginManager;
use multi_launcher::settings::Settings;
use multi_launcher::visibility::handle_visibility_trigger;
use multi_launcher::window_manager::{
    MOCK_MOUSE_LOCK, clear_mock_mouse_position, set_mock_mouse_position,
};
use multi_launcher::{gui::ActivationSource, gui::LauncherApp};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[path = "mock_ctx.rs"]
mod mock_ctx;
use mock_ctx::MockCtx;

fn new_app(ctx: &egui::Context) -> LauncherApp {
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
fn visibility_toggle_immediate_when_context_present() {
    let _mouse_lock = MOCK_MOUSE_LOCK.lock().unwrap();
    set_mock_mouse_position(Some((200.0, 110.0)));
    let trigger = HotkeyTrigger::new(Hotkey::default());
    let visibility = Arc::new(AtomicBool::new(false));
    let restore = Arc::new(AtomicBool::new(false));
    let ctx = MockCtx::default();
    let ctx_handle: Arc<Mutex<Option<MockCtx>>> = Arc::new(Mutex::new(Some(ctx.clone())));
    let mut queued_visibility: Option<bool> = None;

    // simulate hotkey press
    *trigger.open.lock().unwrap() = true;

    let changed = handle_visibility_trigger(
        &trigger,
        &visibility,
        &restore,
        &ctx_handle,
        &mut queued_visibility,
        (0.0, 0.0),
        true,
        false,
        None,
        None,
        (400.0, 220.0),
    );
    assert!(changed);

    assert!(visibility.load(Ordering::SeqCst));
    assert!(queued_visibility.is_none());
    clear_mock_mouse_position();

    let cmds = ctx.commands.lock().unwrap();
    assert_eq!(cmds.len(), 4);
    match cmds[0] {
        egui::ViewportCommand::OuterPosition(_) => {}
        _ => panic!("unexpected command"),
    }
    match cmds[1] {
        egui::ViewportCommand::Visible(v) => assert!(v),
        _ => panic!("unexpected command"),
    }
    match cmds[2] {
        egui::ViewportCommand::Minimized(m) => assert!(!m),
        _ => panic!("unexpected command"),
    }
    match cmds[3] {
        egui::ViewportCommand::Focus => {}
        _ => panic!("unexpected command"),
    }
}

#[test]
fn launcher_toggle_action_sets_visibility_and_restore() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    assert!(!app.visible_flag_state());
    assert!(!app.restore_flag_state());

    let action = Action {
        label: "Toggle launcher".into(),
        desc: "".into(),
        action: "launcher:toggle".into(),
        args: None,
    };
    app.activate_action(action, None, ActivationSource::Enter);

    assert!(app.visible_flag_state());
    assert!(app.restore_flag_state());
}
