use super::RecordingSession;
use std::sync::{
    Arc, OnceLock, Weak,
    atomic::{AtomicU64, Ordering},
};

/// App-owned edge admission for the optional Stop shortcut. The hook and the
/// legacy listener can read the current session through this weak reference,
/// but only the launcher owner can install it.
#[derive(Default)]
pub struct RecordingStopHotkeyBridge {
    stoppable_generation: OnceLock<Weak<AtomicU64>>,
}

impl RecordingStopHotkeyBridge {
    pub fn attach(&self, session: &Arc<RecordingSession>) {
        let _ = self
            .stoppable_generation
            .set(Arc::downgrade(&session.stoppable_generation_projection()));
    }

    /// Captures the exact generation only while the session is in a state
    /// where Stop can be accepted. Callers must place the returned value in
    /// their event payload before waking another thread.
    pub fn capture_stoppable_generation(&self) -> Option<u64> {
        let generation = self
            .stoppable_generation
            .get()?
            .upgrade()?
            .load(Ordering::Acquire);
        (generation != 0).then_some(generation)
    }

    #[cfg(test)]
    pub(crate) fn attach_projection_for_test(&self, projection: &Arc<AtomicU64>) {
        let _ = self.stoppable_generation.set(Arc::downgrade(projection));
    }
}
