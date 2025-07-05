use multi_launcher::hotkey::{Hotkey, HotkeyTrigger};
use multi_launcher::visibility::handle_visibility_trigger;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use eframe::egui;

#[path = "mock_ctx.rs"]
mod mock_ctx;
use mock_ctx::MockCtx;

#[test]
fn visibility_toggle_immediate_when_context_present() {
    let trigger = HotkeyTrigger::new(Hotkey::default());
    let visibility = Arc::new(AtomicBool::new(false));
    let restore = Arc::new(AtomicBool::new(false));
    let ctx = MockCtx::default();
    let ctx_handle: Arc<Mutex<Option<MockCtx>>> = Arc::new(Mutex::new(Some(ctx.clone())));
    let mut queued_visibility: Option<bool> = None;

    // simulate hotkey press
    *trigger.open.lock().unwrap() = true;

    handle_visibility_trigger(
        &trigger,
        &visibility,
        &restore,
        &ctx_handle,
        &mut queued_visibility,
        (0.0, 0.0),
    );

    assert_eq!(visibility.load(Ordering::SeqCst), true);
    assert!(queued_visibility.is_none());

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
