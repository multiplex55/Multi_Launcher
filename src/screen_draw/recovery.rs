use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use super::NativeEmergencyHandle;

/// Process-wide, UI-independent recovery coordination for Screen Draw.
///
/// The bridge deliberately exposes only lifecycle activity and the emergency
/// pause operation. Full controller/session ownership remains on the GUI
/// thread.
#[derive(Debug, Default)]
pub struct ScreenDrawRecoveryBridge {
    active: AtomicBool,
    emergency: Mutex<Option<NativeEmergencyHandle>>,
}

impl ScreenDrawRecoveryBridge {
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    /// Optimistically publishes a process trigger before its GUI start event
    /// is reduced. The controller subsequently reconciles authoritative state.
    pub fn stage_start(&self) {
        self.active.store(true, Ordering::Release);
    }

    pub(crate) fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Release);
        if !active {
            self.clear_emergency_handle();
        }
    }

    pub(crate) fn install_emergency_handle(&self, handle: NativeEmergencyHandle) {
        if let Ok(mut emergency) = self.emergency.lock() {
            *emergency = Some(handle);
        } else {
            tracing::error!("failed to install Screen Draw emergency handle");
        }
    }

    pub(crate) fn clear_emergency_handle(&self) {
        if let Ok(mut emergency) = self.emergency.lock() {
            *emergency = None;
        } else {
            tracing::error!("failed to clear Screen Draw emergency handle");
        }
    }

    /// Delivers an emergency pause directly to the native worker when one is
    /// installed. `Ok(false)` means Screen Draw is active before native startup.
    pub fn emergency_pause(&self) -> Result<bool, String> {
        let handle = self
            .emergency
            .lock()
            .map_err(|_| "Screen Draw emergency bridge lock is poisoned".to_string())?
            .clone();
        match handle {
            Some(handle) => handle.emergency_pause().map(|()| true),
            None => Ok(false),
        }
    }

    #[cfg(test)]
    pub(crate) fn has_emergency_handle(&self) -> bool {
        self.emergency.lock().unwrap().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_bridge_clears_installed_emergency_handle() {
        let (session, commands) = super::super::NativeSessionHandle::test_stub();
        let bridge = ScreenDrawRecoveryBridge::default();
        bridge.set_active(true);
        bridge.install_emergency_handle(session.emergency_handle());
        assert!(bridge.is_active());
        assert!(bridge.has_emergency_handle());
        bridge.emergency_pause().unwrap();
        assert!(matches!(
            commands.try_recv(),
            Ok(super::super::NativeSessionCommand::EmergencyPause)
        ));

        bridge.set_active(false);
        assert!(!bridge.is_active());
        assert!(!bridge.has_emergency_handle());
        assert!(!bridge.emergency_pause().unwrap());
    }
}
