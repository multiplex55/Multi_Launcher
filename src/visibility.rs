use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

use crate::hotkey::HotkeyTrigger;

/// Toggle the visibility flag when the given hotkey trigger fires.
pub fn handle_visibility_trigger(trigger: &HotkeyTrigger, visibility: &Arc<AtomicBool>) {
    if trigger.take() {
        let old = visibility.load(Ordering::SeqCst);
        let next = !old;
        tracing::debug!(from=?old, to=?next, "visibility updated");
        visibility.store(next, Ordering::SeqCst);
    }
}

