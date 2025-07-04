use multi_launcher::hotkey::{Hotkey, HotkeyTrigger};
use multi_launcher::visibility::handle_visibility_trigger;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

#[path = "mock_ctx.rs"]
mod mock_ctx;
use mock_ctx::MockCtx;

#[test]
fn hotkey_shows_after_force_hide() {
    let trigger = HotkeyTrigger::new(Hotkey::default());
    let visibility = Arc::new(AtomicBool::new(true));
    let ctx = MockCtx::default();
    let ctx_handle: Arc<Mutex<Option<MockCtx>>> = Arc::new(Mutex::new(Some(ctx.clone())));
    let mut queued: Option<bool> = None;

    // simulate force hide by directly updating the flag
    visibility.store(false, Ordering::SeqCst);

    // press hotkey to show again
    *trigger.open.lock().unwrap() = true;
    handle_visibility_trigger(&trigger, &visibility, &ctx_handle, &mut queued);

    assert!(visibility.load(Ordering::SeqCst));
}
