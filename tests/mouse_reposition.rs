use multi_launcher::hotkey::{Hotkey, HotkeyTrigger};
use multi_launcher::visibility::handle_visibility_trigger;
use eframe::egui;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

#[path = "mock_ctx.rs"]
mod mock_ctx;
use mock_ctx::MockCtx;

#[test]
fn unminimize_moves_to_mouse() {
    let trigger = HotkeyTrigger::new(Hotkey::default());
    let ctx = MockCtx::default();
    let ctx_handle: Arc<Mutex<Option<MockCtx>>> = Arc::new(Mutex::new(Some(ctx.clone())));
    let visibility = Arc::new(AtomicBool::new(false));
    let mut queued_visibility: Option<bool> = None;

    // simulate recorded mouse position
    *trigger.mouse_pos.lock().unwrap() = (150.0, 250.0);
    // simulate hotkey press
    *trigger.open.lock().unwrap() = true;

    handle_visibility_trigger(&trigger, &visibility, &ctx_handle, &mut queued_visibility);

    assert!(visibility.load(Ordering::SeqCst));
    let cmds = ctx.commands.lock().unwrap();
    assert!(cmds.iter().any(|c| matches!(c, egui::ViewportCommand::Minimized(false))));
    assert!(cmds.iter().any(|c| match c {
        egui::ViewportCommand::OuterPosition(pos) => {
            (pos.x - 150.0).abs() < f32::EPSILON && (pos.y - 250.0).abs() < f32::EPSILON
        }
        _ => false,
    }));
    assert!(cmds.iter().any(|c| matches!(c, egui::ViewportCommand::Focus)));
}
