use multi_launcher::hotkey::{Hotkey, HotkeyTrigger};
use multi_launcher::visibility::handle_visibility_trigger;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use eframe::egui;

#[path = "mock_ctx.rs"]
mod mock_ctx;
use mock_ctx::MockCtx;

#[test]
fn hide_moves_window_off_screen() {
    let trigger = HotkeyTrigger::new(Hotkey::default());
    let visibility = Arc::new(AtomicBool::new(true));
    let restore = Arc::new(AtomicBool::new(false));
    let ctx = MockCtx::default();
    let ctx_handle: Arc<Mutex<Option<MockCtx>>> = Arc::new(Mutex::new(Some(ctx.clone())));
    let mut queued_visibility: Option<bool> = None;

    // simulate hotkey press to hide
    *trigger.open.lock().unwrap() = true;

    handle_visibility_trigger(
        &trigger,
        &visibility,
        &restore,
        &ctx_handle,
        &mut queued_visibility,
        (123.0, 456.0),
    );

    assert_eq!(visibility.load(Ordering::SeqCst), false);
    let cmds = ctx.commands.lock().unwrap();
    assert_eq!(cmds.len(), 1);
    match cmds[0] {
        egui::ViewportCommand::OuterPosition(pos) => {
            assert_eq!(pos.x, 123.0);
            assert_eq!(pos.y, 456.0);
        }
        _ => panic!("unexpected command"),
    }
}
