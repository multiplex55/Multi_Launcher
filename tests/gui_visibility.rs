use multi_launcher::hotkey::{Hotkey, HotkeyTrigger};
use eframe::egui;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

#[path = "mock_ctx.rs"]
mod mock_ctx;
use mock_ctx::MockCtx;

#[test]
fn queued_visibility_applies_when_context_available() {
    let trigger = HotkeyTrigger::new(Hotkey::default());
    let visibility = Arc::new(AtomicBool::new(false));
    let ctx_handle: Arc<Mutex<Option<MockCtx>>> = Arc::new(Mutex::new(None));
    let mut queued_visibility: Option<bool> = None;

    // simulate hotkey press
    *trigger.open.lock().unwrap() = true;

    if trigger.take() {
        let next = !visibility.load(Ordering::SeqCst);
        visibility.store(next, Ordering::SeqCst);
        if let Ok(mut guard) = ctx_handle.lock() {
            if let Some(c) = &*guard {
                c.send_viewport_cmd(egui::ViewportCommand::Minimized(!next));
                c.request_repaint();
                queued_visibility = None;
            } else {
                queued_visibility = Some(next);
            }
        } else {
            queued_visibility = Some(next);
        }
    }

    assert_eq!(visibility.load(Ordering::SeqCst), true);
    assert_eq!(queued_visibility, Some(true));

    // now context becomes available
    let ctx = MockCtx::default();
    {
        let mut guard = ctx_handle.lock().unwrap();
        *guard = Some(ctx.clone());
    }

    if let Some(next) = queued_visibility {
        if let Ok(mut guard) = ctx_handle.lock() {
            if let Some(c) = &*guard {
                c.send_viewport_cmd(egui::ViewportCommand::Minimized(!next));
                c.request_repaint();
                queued_visibility = None;
            }
        }
    }

    assert!(queued_visibility.is_none());
    let cmds = ctx.commands.lock().unwrap();
    assert_eq!(cmds.len(), 1);
    match cmds[0] {
        egui::ViewportCommand::Minimized(v) => assert!(!v),
        _ => panic!("unexpected command"),
    }
}

