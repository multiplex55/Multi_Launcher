use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

use crate::hotkey::HotkeyTrigger;

/// Hide the launcher window when the given hotkey trigger fires.
pub fn handle_visibility_trigger(trigger: &HotkeyTrigger, visibility: &Arc<AtomicBool>) {
    if trigger.take() {
        tracing::debug!("launcher hotkey pressed; hiding window");
        visibility.store(false, Ordering::SeqCst);
    }
}

