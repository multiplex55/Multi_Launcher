use eframe::egui;
use multi_launcher::hotkey::{Hotkey, HotkeyTrigger};
use multi_launcher::visibility::handle_visibility_trigger;
use multi_launcher::window_manager::{
    MOCK_MOUSE_LOCK, clear_mock_mouse_position, set_mock_mouse_position,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[path = "mock_ctx.rs"]
mod mock_ctx;
use mock_ctx::MockCtx;

#[test]
fn queued_visibility_applies_when_context_available() {
    let _mouse_lock = MOCK_MOUSE_LOCK.lock().unwrap();
    set_mock_mouse_position(Some((200.0, 110.0)));
    let trigger = HotkeyTrigger::new(Hotkey::default());
    let visibility = Arc::new(AtomicBool::new(false));
    let restore = Arc::new(AtomicBool::new(false));
    let ctx_handle: Arc<Mutex<Option<MockCtx>>> = Arc::new(Mutex::new(None));
    let mut queued_visibility: Option<bool> = None;

    // simulate hotkey press while no context is available
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

    assert_eq!(visibility.load(Ordering::SeqCst), true);
    assert_eq!(queued_visibility, Some(true));

    // now context becomes available
    let ctx = MockCtx::default();
    {
        let mut guard = ctx_handle.lock().unwrap();
        *guard = Some(ctx.clone());
    }

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
    assert!(!changed);

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
