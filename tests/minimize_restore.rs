use eframe::egui;
use multi_launcher::hotkey::{Hotkey, HotkeyTrigger};
use multi_launcher::visibility::handle_visibility_trigger;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

#[path = "mock_ctx.rs"]
mod mock_ctx;
use mock_ctx::MockCtx;

#[test]
fn hotkey_twice_minimize_then_restore() {
    let trigger = HotkeyTrigger::new(Hotkey::default());
    let visibility = Arc::new(AtomicBool::new(true));
    let ctx = MockCtx::default();
    let ctx_handle: Arc<Mutex<Option<MockCtx>>> = Arc::new(Mutex::new(Some(ctx.clone())));
    let mut queued_visibility: Option<bool> = None;

    // first press - minimize
    *trigger.open.lock().unwrap() = true;
    handle_visibility_trigger(&trigger, &visibility, &ctx_handle, &mut queued_visibility);

    // second press - restore
    *trigger.open.lock().unwrap() = true;
    handle_visibility_trigger(&trigger, &visibility, &ctx_handle, &mut queued_visibility);

    assert_eq!(visibility.load(Ordering::SeqCst), true);
    let cmds = ctx.commands.lock().unwrap();
    assert_eq!(cmds.len(), 6);

    assert!(matches!(cmds[0], egui::ViewportCommand::Visible(true)));
    assert!(matches!(cmds[1], egui::ViewportCommand::Minimized(true)));
    assert!(matches!(cmds[2], egui::ViewportCommand::Visible(true)));
    assert!(matches!(cmds[3], egui::ViewportCommand::Minimized(false)));
    assert!(matches!(cmds[4], egui::ViewportCommand::OuterPosition(_)));
    assert!(matches!(cmds[5], egui::ViewportCommand::Focus));
}
