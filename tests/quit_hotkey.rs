use eframe::egui;
use multi_launcher::hotkey::{Hotkey, HotkeyTrigger};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;

#[path = "mock_ctx.rs"]
mod mock_ctx;
use mock_ctx::MockCtx;

fn run_quit_loop(
    trigger: &HotkeyTrigger,
    ctx_handle: Arc<Mutex<Option<MockCtx>>>,
    kill: Arc<AtomicBool>,
    handle: thread::JoinHandle<()>,
) {
    let mut quit_requested = false;

    loop {
        if trigger.take() {
            quit_requested = true;
        }

        if quit_requested {
            if let Ok(guard) = ctx_handle.lock() {
                if let Some(c) = &*guard {
                    c.send_viewport_cmd(egui::ViewportCommand::Close);
                    c.request_repaint();
                    kill.store(true, Ordering::SeqCst);
                }
            }
            let _ = handle.join();
            break;
        }

        if handle.is_finished() {
            let _ = handle.join();
            break;
        }

        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn quit_hotkey_exits_main_loop() {
    let trigger = HotkeyTrigger::new(Hotkey::default());
    let ctx = MockCtx::default();
    let ctx_handle: Arc<Mutex<Option<MockCtx>>> = Arc::new(Mutex::new(Some(ctx.clone())));
    let kill = Arc::new(AtomicBool::new(false));
    let kill_thread = kill.clone();

    let handle = thread::spawn(move || {
        while !kill_thread.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(10));
        }
    });

    // simulate pressing the quit hotkey
    *trigger.open.lock().unwrap() = true;

    run_quit_loop(&trigger, ctx_handle.clone(), kill.clone(), handle);

    assert!(kill.load(Ordering::SeqCst));
    let cmds = ctx.commands.lock().unwrap();
    assert_eq!(cmds.len(), 1);
    match cmds[0] {
        egui::ViewportCommand::Close => {}
        _ => panic!("unexpected command"),
    }
}
