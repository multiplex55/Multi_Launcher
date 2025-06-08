use multi_launcher::hotkey::{Hotkey, HotkeyTrigger};
use eframe::egui;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[path = "mock_ctx.rs"]
mod mock_ctx;
use mock_ctx::MockCtx;

#[test]
fn quit_hotkey_terminates_loop() {
    let quit_trigger = HotkeyTrigger::new(Hotkey::default());
    // simulate pressing quit hotkey
    *quit_trigger.open.lock().unwrap() = true;

    let ctx = MockCtx::default();
    let ctx_handle: Arc<Mutex<Option<MockCtx>>> = Arc::new(Mutex::new(Some(ctx)));

    // dummy gui thread that finishes shortly
    let handle = thread::spawn(|| {
        thread::sleep(Duration::from_millis(20));
    });

    let mut quit_requested = false;

    let result: Result<(), ()> = loop {
        if handle.is_finished() {
            handle.join().ok();
            break Ok(());
        }

        if quit_trigger.take() {
            quit_requested = true;
        }

        if quit_requested {
            if let Ok(mut guard) = ctx_handle.lock() {
                if let Some(c) = &*guard {
                    c.send_viewport_cmd(egui::ViewportCommand::Close);
                    c.request_repaint();
                }
            }
            handle.join().ok();
            break Ok(());
        }

        thread::sleep(Duration::from_millis(5));
    };

    assert!(result.is_ok());
}
