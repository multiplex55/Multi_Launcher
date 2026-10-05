use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

use super::NativeEmergencyHandle;

/// Process-wide, UI-independent recovery coordination for Screen Draw.
///
/// Admissions and native emergency delivery cross threads here. Visibility
/// publication and full controller/session ownership remain on the GUI thread.
#[derive(Debug, Default)]
pub struct ScreenDrawRecoveryBridge {
    activity: AtomicU64,
    next_intent: AtomicU64,
    emergency: Mutex<EmergencyDelivery>,
}

#[derive(Debug, Default)]
struct EmergencyDelivery {
    handle: Option<(ScreenDrawRecoveryLifetime, NativeEmergencyHandle)>,
    delivered_intent: Option<u64>,
}

/// Captured by the actual hook priority owner before its admission is queued.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenDrawRecoveryLifetime(u64);

impl ScreenDrawRecoveryLifetime {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// One hook admission, separate from normal launcher InvocationId.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenDrawRecoveryAdmission {
    pub serial: u64,
    pub lifetime: ScreenDrawRecoveryLifetime,
    pub kind: ScreenDrawRecoveryKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenDrawRecoveryKind {
    LauncherToggle,
    Emergency,
}

/// Minted once when an actual admission or legacy trigger is consumed.
/// Copies retain the same effect identity through GUI delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenDrawRecoveryIntent {
    id: u64,
    activity_epoch: u64,
    pub kind: ScreenDrawRecoveryKind,
    pub admission: Option<ScreenDrawRecoveryAdmission>,
}

impl ScreenDrawRecoveryIntent {
    pub const fn id(self) -> u64 {
        self.id
    }

    pub const fn activity_epoch(self) -> u64 {
        self.activity_epoch
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScreenDrawRestoreCause {
    LauncherRecovery,
    EmergencyRecovery,
    CapturePoll,
    ParkingReconciliation,
    NewCapture,
    ResumeFailure,
    SessionClose,
}

impl ScreenDrawRestoreCause {
    pub(crate) const COUNT: usize = 7;
    pub(crate) const fn slot(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScreenDrawRestoreOutcome {
    Published,
    Duplicate,
    StaleIntent,
    Reconciled,
    Superseded,
    Error,
}

impl ScreenDrawRecoveryBridge {
    pub fn admit(
        &self,
        kind: ScreenDrawRecoveryKind,
        admission: Option<ScreenDrawRecoveryAdmission>,
    ) -> Option<ScreenDrawRecoveryIntent> {
        let activity = self.activity.load(Ordering::Acquire);
        if activity & 1 == 0 {
            return None;
        }
        let activity_epoch = activity >> 1;
        if admission.is_some_and(|admission| {
            admission.lifetime.get() != activity_epoch || admission.kind != kind
        }) {
            return None;
        }
        let previous = self
            .next_intent
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_add(1)
            })
            .ok()?;
        if self.activity.load(Ordering::Acquire) != activity {
            return None;
        }
        Some(ScreenDrawRecoveryIntent {
            id: previous + 1,
            activity_epoch,
            kind,
            admission,
        })
    }

    pub fn activity_epoch(&self) -> u64 {
        self.activity.load(Ordering::Acquire) >> 1
    }

    pub fn active_lifetime(&self) -> Option<ScreenDrawRecoveryLifetime> {
        let activity = self.activity.load(Ordering::Acquire);
        (activity & 1 != 0).then_some(ScreenDrawRecoveryLifetime(activity >> 1))
    }

    pub fn is_active(&self) -> bool {
        self.activity.load(Ordering::Acquire) & 1 != 0
    }

    /// Optimistically publishes a process trigger before its GUI start event
    /// is reduced. The controller subsequently reconciles authoritative state.
    pub fn stage_start(&self) {
        self.set_active(true);
    }

    /// Consume the process launch edge and publish its staged lifetime before
    /// main queues Start or any same-cycle recovery for the GUI.
    pub fn stage_start_if_triggered(&self, trigger: &crate::hotkey::HotkeyTrigger) -> bool {
        let fired = trigger.take();
        if fired {
            self.stage_start();
        }
        fired
    }

    pub(crate) fn set_active(&self, active: bool) {
        if let Err(error) = self.transition_activity(active, false, true) {
            tracing::error!(%error, "could not publish Screen Draw activity");
        }
    }

    pub(crate) fn replace_active_session(&self) {
        if let Err(error) = self.transition_activity(true, true, false) {
            tracing::error!(%error, "could not replace Screen Draw recovery lifetime");
        }
    }

    /// A new park/resume operation invalidates undelivered requests while the
    /// same native worker remains owned and can accept a fresh emergency.
    pub(crate) fn advance_active_lifetime(&self) -> Result<(), String> {
        if !self.is_active() {
            return Err("Screen Draw parking has no active recovery lifetime".into());
        }
        self.transition_activity(true, true, true)
    }

    fn transition_activity(
        &self,
        active: bool,
        force: bool,
        retain_handle: bool,
    ) -> Result<(), String> {
        let mut emergency = self
            .emergency
            .lock()
            .map_err(|_| "Screen Draw emergency bridge lock is poisoned".to_string())?;
        self.transition_activity_locked(active, force, retain_handle, &mut emergency)
    }

    fn transition_activity_locked(
        &self,
        active: bool,
        force: bool,
        retain_handle: bool,
        emergency: &mut EmergencyDelivery,
    ) -> Result<(), String> {
        let previous = self.activity.load(Ordering::Acquire);
        let changed = force || (previous & 1 != 0) != active;
        let next = if changed {
            let Some(next) = (previous & !1)
                .checked_add(2)
                .map(|next| next | u64::from(active))
            else {
                self.activity.fetch_and(!1, Ordering::AcqRel);
                emergency.handle = None;
                emergency.delivered_intent = None;
                crate::hotkey::launcher_invocation::set_exclusive_owner(
                    crate::hotkey::launcher_invocation::ExclusiveOwner::ScreenDraw,
                    false,
                );
                return Err("Screen Draw activity identity exhausted".into());
            };
            next
        } else {
            previous
        };
        // The native-delivery lock covers validation, epoch replacement, handle
        // association and actual send. A stale A intent cannot send to B.
        self.activity.store(next, Ordering::Release);
        if !active || !retain_handle {
            emergency.handle = None;
        } else if let Some((lifetime, _)) = emergency.handle.as_mut() {
            *lifetime = ScreenDrawRecoveryLifetime(next >> 1);
        }
        if changed {
            emergency.delivered_intent = None;
        }
        crate::hotkey::launcher_invocation::set_exclusive_owner(
            crate::hotkey::launcher_invocation::ExclusiveOwner::ScreenDraw,
            active,
        );
        Ok(())
    }

    pub(crate) fn install_emergency_handle(&self, handle: NativeEmergencyHandle) {
        if let Ok(mut emergency) = self.emergency.lock() {
            if !self.is_active() {
                return;
            }
            if emergency.handle.is_some() {
                if let Err(error) =
                    self.transition_activity_locked(true, true, false, &mut emergency)
                {
                    tracing::error!(%error, "could not replace Screen Draw native emergency handle");
                    return;
                }
            }
            if let Some(lifetime) = self.active_lifetime() {
                emergency.handle = Some((lifetime, handle));
            }
        } else {
            tracing::error!("failed to install Screen Draw emergency handle");
        }
    }

    pub(crate) fn clear_emergency_handle(&self) {
        if let Ok(mut emergency) = self.emergency.lock() {
            if emergency.handle.is_some() {
                let active = self.is_active();
                if let Err(error) =
                    self.transition_activity_locked(active, true, false, &mut emergency)
                {
                    tracing::error!(%error, "could not clear Screen Draw emergency lifetime");
                }
            }
        } else {
            tracing::error!("failed to clear Screen Draw emergency handle");
        }
    }

    /// Delivers an emergency pause directly to the native worker when one is
    /// installed for this admitted lifetime. Stale/duplicate requests and an
    /// active startup without a handle return false without sending input.
    pub fn emergency_pause(&self, intent: ScreenDrawRecoveryIntent) -> Result<bool, String> {
        let mut emergency = self
            .emergency
            .lock()
            .map_err(|_| "Screen Draw emergency bridge lock is poisoned".to_string())?;
        if intent.kind != ScreenDrawRecoveryKind::Emergency
            || self
                .active_lifetime()
                .is_none_or(|lifetime| lifetime.get() != intent.activity_epoch())
            || emergency
                .delivered_intent
                .is_some_and(|latest| latest >= intent.id())
        {
            return Ok(false);
        }
        let Some((lifetime, handle)) = emergency.handle.as_ref() else {
            return Ok(false);
        };
        if lifetime.get() != intent.activity_epoch() {
            return Ok(false);
        }
        handle.emergency_pause()?;
        emergency.delivered_intent = Some(intent.id());
        Ok(true)
    }

    #[cfg(test)]
    pub(crate) fn has_emergency_handle(&self) -> bool {
        self.emergency.lock().unwrap().handle.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emergency_delivery_rejects_replaced_worker_lifetime_and_delivers_current_intent_once() {
        let bridge = ScreenDrawRecoveryBridge::default();
        let (a, a_commands) = super::super::NativeSessionHandle::test_stub();
        let (b, b_commands) = super::super::NativeSessionHandle::test_stub();
        bridge.stage_start();
        bridge.install_emergency_handle(a.emergency_handle());
        let old = bridge
            .admit(ScreenDrawRecoveryKind::Emergency, None)
            .unwrap();
        bridge.install_emergency_handle(b.emergency_handle());
        assert!(bridge.activity_epoch() > old.activity_epoch());
        assert!(!bridge.emergency_pause(old).unwrap());
        assert!(a_commands.try_recv().is_err() && b_commands.try_recv().is_err());
        let fresh = bridge
            .admit(ScreenDrawRecoveryKind::Emergency, None)
            .unwrap();
        assert!(bridge.emergency_pause(fresh).unwrap());
        assert!(!bridge.emergency_pause(fresh).unwrap());
        assert!(matches!(
            b_commands.try_recv(),
            Ok(super::super::NativeSessionCommand::EmergencyPause)
        ));
        assert!(b_commands.try_recv().is_err() && a_commands.try_recv().is_err());
        bridge.clear_emergency_handle();
        assert!(!bridge.has_emergency_handle());
        assert!(!bridge.emergency_pause(fresh).unwrap());
        assert!(b_commands.try_recv().is_err());
        bridge.set_active(false);
    }

    #[test]
    fn emergency_startup_without_handle_and_fresh_repark_keep_owned_delivery_semantics() {
        let bridge = ScreenDrawRecoveryBridge::default();
        bridge.stage_start();
        let startup = bridge
            .admit(ScreenDrawRecoveryKind::Emergency, None)
            .unwrap();
        assert!(!bridge.emergency_pause(startup).unwrap());
        let (worker, commands) = super::super::NativeSessionHandle::test_stub();
        bridge.install_emergency_handle(worker.emergency_handle());
        assert!(bridge.emergency_pause(startup).unwrap());
        assert!(matches!(
            commands.try_recv(),
            Ok(super::super::NativeSessionCommand::EmergencyPause)
        ));
        assert!(!bridge.emergency_pause(startup).unwrap());
        bridge.advance_active_lifetime().unwrap();
        assert!(!bridge.emergency_pause(startup).unwrap());
        let fresh = bridge
            .admit(ScreenDrawRecoveryKind::Emergency, None)
            .unwrap();
        assert!(bridge.emergency_pause(fresh).unwrap());
        assert!(matches!(
            commands.try_recv(),
            Ok(super::super::NativeSessionCommand::EmergencyPause)
        ));
        assert!(commands.try_recv().is_err());
        let launcher = bridge
            .admit(ScreenDrawRecoveryKind::LauncherToggle, None)
            .unwrap();
        assert!(!bridge.emergency_pause(launcher).unwrap());
        bridge.set_active(false);
        assert!(!bridge.emergency_pause(fresh).unwrap());
        bridge.install_emergency_handle(worker.emergency_handle());
        assert!(!bridge.is_active() && !bridge.has_emergency_handle());
        assert!(commands.try_recv().is_err());
    }

    #[test]
    fn recovery_bridge_mints_distinct_one_shot_intents_in_an_atomic_activity_lifetime() {
        let bridge = ScreenDrawRecoveryBridge::default();
        assert!(
            bridge
                .admit(ScreenDrawRecoveryKind::LauncherToggle, None)
                .is_none()
        );
        bridge.stage_start();
        let first = bridge
            .admit(
                ScreenDrawRecoveryKind::LauncherToggle,
                Some(ScreenDrawRecoveryAdmission {
                    serial: 1,
                    lifetime: bridge.active_lifetime().unwrap(),
                    kind: ScreenDrawRecoveryKind::LauncherToggle,
                }),
            )
            .unwrap();
        bridge.set_active(true);
        let second = bridge
            .admit(ScreenDrawRecoveryKind::Emergency, None)
            .unwrap();
        assert_ne!(first.id(), second.id());
        assert_eq!(first.activity_epoch(), second.activity_epoch());
        bridge.set_active(false);
        assert!(
            bridge
                .admit(ScreenDrawRecoveryKind::Emergency, None)
                .is_none()
        );
        bridge.stage_start();
        let next = bridge
            .admit(
                ScreenDrawRecoveryKind::LauncherToggle,
                Some(ScreenDrawRecoveryAdmission {
                    serial: 1,
                    lifetime: bridge.active_lifetime().unwrap(),
                    kind: ScreenDrawRecoveryKind::LauncherToggle,
                }),
            )
            .unwrap();
        assert!(next.id() > second.id());
        assert!(next.activity_epoch() > second.activity_epoch());
        bridge.set_active(false);
    }

    #[test]
    fn recovery_bridge_refuses_exhausted_intent_identity_without_reusing_a_token() {
        let bridge = ScreenDrawRecoveryBridge::default();
        bridge.stage_start();
        bridge.next_intent.store(u64::MAX, Ordering::Release);
        assert!(
            bridge
                .admit(ScreenDrawRecoveryKind::LauncherToggle, None)
                .is_none()
        );
        assert_eq!(bridge.next_intent.load(Ordering::Acquire), u64::MAX);
        bridge.set_active(false);
    }

    #[test]
    fn inactive_bridge_clears_installed_emergency_handle() {
        let (session, commands) = super::super::NativeSessionHandle::test_stub();
        let bridge = ScreenDrawRecoveryBridge::default();
        bridge.set_active(true);
        bridge.install_emergency_handle(session.emergency_handle());
        assert!(bridge.is_active());
        assert!(bridge.has_emergency_handle());
        let intent = bridge
            .admit(ScreenDrawRecoveryKind::Emergency, None)
            .unwrap();
        bridge.emergency_pause(intent).unwrap();
        assert!(matches!(
            commands.try_recv(),
            Ok(super::super::NativeSessionCommand::EmergencyPause)
        ));

        bridge.set_active(false);
        assert!(!bridge.is_active());
        assert!(!bridge.has_emergency_handle());
        assert!(!bridge.emergency_pause(intent).unwrap());
    }
}
