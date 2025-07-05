use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

use crate::hotkey::HotkeyTrigger;

/// Toggle the launcher window when the given hotkey trigger fires.
pub fn handle_visibility_trigger(trigger: &HotkeyTrigger, visibility: &Arc<AtomicBool>) {
    if trigger.take() {
        let current = visibility.load(Ordering::SeqCst);
        let next = !current;
        tracing::debug!(from=?current, to=?next, "launcher hotkey toggled visibility");
        visibility.store(next, Ordering::SeqCst);
    }
}

