use std::sync::Mutex;

/// Process-local identity for one egui Settings → GPUI restart handshake.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RestartAttemptId(u64);

impl RestartAttemptId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuiExitReceipt {
    pub attempt: Option<RestartAttemptId>,
    pub failures: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReverseRestartState {
    Idle,
    Requested(RestartAttemptId),
    Acknowledged(RestartAttemptId),
    Committed(RestartAttemptId),
    RootCloseAccepted(RestartAttemptId),
    AppExitReceived {
        attempt: RestartAttemptId,
        receipt: GuiExitReceipt,
    },
    Denied {
        attempt: RestartAttemptId,
        reason: String,
    },
    Cancelled(RestartAttemptId),
    OrdinaryClose,
    OrdinaryExit(GuiExitReceipt),
}

struct ReverseRestartInner {
    /// Zero is the exhausted sentinel. `checked_add` means IDs never wrap.
    next_attempt: u64,
    committed_attempt: Option<RestartAttemptId>,
    state: ReverseRestartState,
}

/// Shared request/ack/commit state for the egui Settings → GPUI restart.
/// The GUI owns confirmation and close attribution; main owns admission
/// reservations and only acknowledges an exact current request.
pub struct ReverseRestartProtocol {
    inner: Mutex<ReverseRestartInner>,
}

impl Default for ReverseRestartProtocol {
    fn default() -> Self {
        Self {
            inner: Mutex::new(ReverseRestartInner {
                next_attempt: 1,
                committed_attempt: None,
                state: ReverseRestartState::Idle,
            }),
        }
    }
}

impl ReverseRestartProtocol {
    pub fn begin(&self) -> Result<RestartAttemptId, String> {
        let mut inner = self.lock();
        if inner.committed_attempt.is_some() {
            return Err("a GPUI restart attempt was already committed".to_owned());
        }
        if matches!(
            inner.state,
            ReverseRestartState::Requested(_)
                | ReverseRestartState::Acknowledged(_)
                | ReverseRestartState::Committed(_)
                | ReverseRestartState::RootCloseAccepted(_)
                | ReverseRestartState::AppExitReceived { .. }
        ) {
            return Err("a GPUI restart attempt is already active".to_owned());
        }
        if matches!(
            inner.state,
            ReverseRestartState::OrdinaryClose | ReverseRestartState::OrdinaryExit(_)
        ) {
            return Err("ordinary application close already owns the exit".to_owned());
        }
        let raw = inner.next_attempt;
        if raw == 0 {
            return Err("GPUI restart attempt IDs are exhausted".to_owned());
        }
        inner.next_attempt = raw.checked_add(1).unwrap_or(0);
        let attempt = RestartAttemptId(raw);
        inner.state = ReverseRestartState::Requested(attempt);
        Ok(attempt)
    }

    pub fn state(&self) -> ReverseRestartState {
        self.lock().state.clone()
    }

    pub fn committed_attempt(&self) -> Option<RestartAttemptId> {
        self.lock().committed_attempt
    }

    pub fn acknowledge(&self, attempt: RestartAttemptId) -> bool {
        let mut inner = self.lock();
        if inner.state != ReverseRestartState::Requested(attempt) {
            return false;
        }
        inner.state = ReverseRestartState::Acknowledged(attempt);
        true
    }

    pub fn deny(&self, attempt: RestartAttemptId, reason: impl Into<String>) -> bool {
        let mut inner = self.lock();
        if !matches!(
            inner.state,
            ReverseRestartState::Requested(current)
                | ReverseRestartState::Acknowledged(current)
                if current == attempt
        ) {
            return false;
        }
        inner.state = ReverseRestartState::Denied {
            attempt,
            reason: reason.into(),
        };
        true
    }

    pub fn commit(&self, attempt: RestartAttemptId) -> bool {
        let mut inner = self.lock();
        if inner.state != ReverseRestartState::Acknowledged(attempt) {
            return false;
        }
        inner.committed_attempt = Some(attempt);
        inner.state = ReverseRestartState::Committed(attempt);
        true
    }

    pub fn cancel(&self, attempt: RestartAttemptId) -> bool {
        let mut inner = self.lock();
        if !matches!(
            inner.state,
            ReverseRestartState::Requested(current)
                | ReverseRestartState::Acknowledged(current)
                if current == attempt
        ) {
            return false;
        }
        inner.state = ReverseRestartState::Cancelled(attempt);
        true
    }

    /// A root Close observed before explicit commitment remains an ordinary
    /// close and permanently removes restart authority for this process exit.
    pub fn observe_ordinary_close(&self) -> bool {
        let mut inner = self.lock();
        if matches!(
            inner.state,
            ReverseRestartState::Idle
                | ReverseRestartState::Requested(_)
                | ReverseRestartState::Acknowledged(_)
                | ReverseRestartState::Denied { .. }
                | ReverseRestartState::Cancelled(_)
        ) {
            inner.state = ReverseRestartState::OrdinaryClose;
            return true;
        }
        false
    }

    pub fn accept_root_close(&self) -> Option<RestartAttemptId> {
        let mut inner = self.lock();
        let ReverseRestartState::Committed(attempt) = inner.state else {
            return None;
        };
        inner.state = ReverseRestartState::RootCloseAccepted(attempt);
        Some(attempt)
    }

    pub fn record_gui_exit(&self, receipt: GuiExitReceipt) {
        let mut inner = self.lock();
        match (inner.committed_attempt, receipt.attempt) {
            (Some(attempt), Some(receipt_attempt))
                if attempt == receipt_attempt
                    && inner.state == ReverseRestartState::RootCloseAccepted(attempt) =>
            {
                inner.state = ReverseRestartState::AppExitReceived { attempt, receipt };
            }
            (Some(attempt), _) => {
                inner.state = ReverseRestartState::Denied {
                    attempt,
                    reason: "the GUI exited without the matching accepted root Close".to_owned(),
                };
            }
            (None, Some(attempt)) => {
                inner.state = ReverseRestartState::Denied {
                    attempt,
                    reason: "the GUI exited without a committed restart attempt".to_owned(),
                };
            }
            (None, None) => inner.state = ReverseRestartState::OrdinaryExit(receipt),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ReverseRestartInner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_attempt_requires_current_ack_and_is_cancellable_before_close() {
        let protocol = ReverseRestartProtocol::default();
        let attempt = protocol.begin().unwrap();
        assert_eq!(protocol.state(), ReverseRestartState::Requested(attempt));
        assert!(!protocol.acknowledge(RestartAttemptId(attempt.get() + 1)));
        assert!(protocol.acknowledge(attempt));
        assert_eq!(
            protocol.state(),
            ReverseRestartState::Acknowledged(attempt)
        );
        assert!(protocol.cancel(attempt));
        assert_eq!(protocol.state(), ReverseRestartState::Cancelled(attempt));
        assert!(!protocol.commit(attempt));
    }

    #[test]
    fn ordinary_close_before_commit_cannot_become_restart_authority() {
        let protocol = ReverseRestartProtocol::default();
        let attempt = protocol.begin().unwrap();
        assert!(protocol.acknowledge(attempt));
        assert!(protocol.observe_ordinary_close());
        assert_eq!(protocol.state(), ReverseRestartState::OrdinaryClose);
        assert!(!protocol.commit(attempt));
        assert_eq!(protocol.accept_root_close(), None);
        let receipt = GuiExitReceipt {
            attempt: None,
            failures: Vec::new(),
        };
        protocol.record_gui_exit(receipt.clone());
        assert_eq!(
            protocol.state(),
            ReverseRestartState::OrdinaryExit(receipt)
        );
    }

    #[test]
    fn ordinary_close_before_first_request_permanently_fences_restart() {
        let protocol = ReverseRestartProtocol::default();
        assert!(protocol.observe_ordinary_close());
        assert_eq!(protocol.state(), ReverseRestartState::OrdinaryClose);
        assert!(protocol.begin().is_err());
        assert!(!protocol.observe_ordinary_close());
    }

    #[test]
    fn ordinary_quit_interrupts_requested_and_acknowledged_restart() {
        for acknowledge_first in [false, true] {
            let protocol = ReverseRestartProtocol::default();
            let attempt = protocol.begin().unwrap();
            if acknowledge_first {
                assert!(protocol.acknowledge(attempt));
            }
            assert!(protocol.observe_ordinary_close());
            assert!(!protocol.commit(attempt));
            assert_eq!(protocol.accept_root_close(), None);
            assert_eq!(protocol.state(), ReverseRestartState::OrdinaryClose);
        }
    }

    #[test]
    fn committed_restart_wins_a_stale_acknowledged_quit_observation() {
        let protocol = ReverseRestartProtocol::default();
        let attempt = protocol.begin().unwrap();
        assert!(protocol.acknowledge(attempt));
        assert!(protocol.commit(attempt));
        assert_eq!(protocol.accept_root_close(), Some(attempt));
        assert!(!protocol.observe_ordinary_close());
        let receipt = GuiExitReceipt {
            attempt: Some(attempt),
            failures: Vec::new(),
        };
        protocol.record_gui_exit(receipt.clone());
        assert_eq!(
            protocol.state(),
            ReverseRestartState::AppExitReceived { attempt, receipt }
        );
    }

    #[test]
    fn committed_restart_without_accepted_root_close_remains_a_failed_attempt() {
        let protocol = ReverseRestartProtocol::default();
        let attempt = protocol.begin().unwrap();
        assert!(protocol.acknowledge(attempt));
        assert!(protocol.commit(attempt));
        protocol.record_gui_exit(GuiExitReceipt {
            attempt: None,
            failures: Vec::new(),
        });
        assert_eq!(protocol.committed_attempt(), Some(attempt));
        assert!(matches!(
            protocol.state(),
            ReverseRestartState::Denied { attempt: current, .. } if current == attempt
        ));
        assert!(protocol.begin().is_err());
    }

    #[test]
    fn only_committed_attempt_can_receive_root_close_and_exit_receipt() {
        let protocol = ReverseRestartProtocol::default();
        let attempt = protocol.begin().unwrap();
        assert!(protocol.acknowledge(attempt));
        assert!(protocol.commit(attempt));
        assert!(!protocol.cancel(attempt));
        assert!(!protocol.observe_ordinary_close());
        assert_eq!(protocol.accept_root_close(), Some(attempt));
        let receipt = GuiExitReceipt {
            attempt: Some(attempt),
            failures: Vec::new(),
        };
        protocol.record_gui_exit(receipt.clone());
        assert_eq!(
            protocol.state(),
            ReverseRestartState::AppExitReceived { attempt, receipt }
        );
    }

    #[test]
    fn attempt_ids_exhaust_without_reuse() {
        let protocol = ReverseRestartProtocol {
            inner: Mutex::new(ReverseRestartInner {
                next_attempt: u64::MAX,
                committed_attempt: None,
                state: ReverseRestartState::Idle,
            }),
        };
        let last = protocol.begin().unwrap();
        assert_eq!(last.get(), u64::MAX);
        let _ = protocol.cancel(last);
        assert!(protocol.begin().is_err());
        assert_eq!(protocol.state(), ReverseRestartState::Cancelled(last));
    }
}
