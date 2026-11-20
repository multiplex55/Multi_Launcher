//! Cooperative transfer pause state and short, race-safe mutation admission.
use super::CopyFailure;
use std::sync::{Arc, Condvar, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyPauseState {
    Running,
    PauseRequested,
    Paused,
    Cancelled,
}
#[derive(Default)]
struct State {
    requested: bool,
    paused: bool,
    cancelled: bool,
    claims: usize,
    epoch: u64,
}
#[derive(Default)]
pub(super) struct PauseGate {
    state: Mutex<State>,
    wake: Condvar,
}
impl PauseGate {
    pub(super) fn request_pause(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.cancelled || state.requested {
            return false;
        }
        let Some(epoch) = state.epoch.checked_add(1) else {
            state.cancelled = true;
            self.wake.notify_all();
            return false;
        };
        state.epoch = epoch;
        state.requested = true;
        self.wake.notify_all();
        true
    }
    pub(super) fn resume(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.cancelled || !state.requested {
            return false;
        }
        state.requested = false;
        state.paused = false;
        self.wake.notify_all();
        true
    }
    pub(super) fn cancel(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.cancelled = true;
        state.paused = false;
        self.wake.notify_all();
    }
    pub(super) fn state(&self) -> CopyPauseState {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.cancelled {
            CopyPauseState::Cancelled
        } else if state.paused {
            CopyPauseState::Paused
        } else if state.requested {
            CopyPauseState::PauseRequested
        } else {
            CopyPauseState::Running
        }
    }
    fn ready<'a>(
        &self,
        mut state: std::sync::MutexGuard<'a, State>,
    ) -> Result<std::sync::MutexGuard<'a, State>, CopyFailure> {
        while state.requested && !state.cancelled {
            if state.claims == 0 {
                state.paused = true;
                self.wake.notify_all();
            }
            state = self.wake.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        state.paused = false;
        if state.cancelled {
            Err(CopyFailure::Cancelled)
        } else {
            Ok(state)
        }
    }
    pub(super) fn checkpoint(&self) -> Result<(), CopyFailure> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        drop(self.ready(state)?);
        Ok(())
    }
    pub(super) fn epoch(&self) -> Result<Option<u64>, CopyFailure> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.cancelled {
            Err(CopyFailure::Cancelled)
        } else {
            Ok((!state.requested).then_some(state.epoch))
        }
    }
    pub(super) fn try_claim(
        self: &Arc<Self>,
        expected_epoch: u64,
    ) -> Result<Option<MutationClaim>, CopyFailure> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.cancelled {
            return Err(CopyFailure::Cancelled);
        }
        if state.requested || state.epoch != expected_epoch {
            return Ok(None);
        }
        state.claims = state
            .claims
            .checked_add(1)
            .ok_or(CopyFailure::UnsupportedPlan)?;
        Ok(Some(MutationClaim { gate: self.clone() }))
    }
    #[cfg(test)]
    pub(super) fn claim(self: &Arc<Self>) -> Result<MutationClaim, CopyFailure> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut state = self.ready(state)?;
        state.claims = state
            .claims
            .checked_add(1)
            .ok_or(CopyFailure::UnsupportedPlan)?;
        Ok(MutationClaim { gate: self.clone() })
    }
}
pub(super) struct MutationClaim {
    gate: Arc<PauseGate>,
}
impl Drop for MutationClaim {
    fn drop(&mut self) {
        let mut state = self.gate.state.lock().unwrap_or_else(|e| e.into_inner());
        state.claims -= 1;
        self.gate.wake.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    fn wait_for(gate: &PauseGate, wanted: CopyPauseState) {
        let until = Instant::now() + Duration::from_secs(2);
        while gate.state() != wanted {
            assert!(Instant::now() < until);
            std::thread::yield_now();
        }
    }
    #[test]
    fn transfer_pause_acknowledges_only_after_claim_and_cancel_wakes_waiter() {
        let gate = Arc::new(PauseGate::default());
        let claim = gate.claim().unwrap();
        assert!(gate.request_pause());
        assert_eq!(gate.state(), CopyPauseState::PauseRequested);
        let (tx, rx) = mpsc::channel();
        let worker = gate.clone();
        let thread = std::thread::spawn(move || {
            tx.send(worker.checkpoint().is_err()).unwrap();
        });
        assert!(rx.recv_timeout(Duration::from_millis(30)).is_err());
        assert_eq!(gate.state(), CopyPauseState::PauseRequested);
        drop(claim);
        wait_for(&gate, CopyPauseState::Paused);
        gate.cancel();
        assert!(rx.recv_timeout(Duration::from_secs(2)).unwrap());
        thread.join().unwrap();
        assert_eq!(gate.state(), CopyPauseState::Cancelled);
    }
    #[test]
    fn transfer_pause_resume_and_claim_admission() {
        let gate = Arc::new(PauseGate::default());
        assert!(gate.request_pause());
        let (tx, rx) = mpsc::channel();
        let worker = gate.clone();
        let thread = std::thread::spawn(move || {
            let claim = worker.claim().unwrap();
            tx.send(()).unwrap();
            drop(claim);
        });
        wait_for(&gate, CopyPauseState::Paused);
        assert!(rx.try_recv().is_err());
        assert!(gate.resume());
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        thread.join().unwrap();
        assert_eq!(gate.state(), CopyPauseState::Running);
        gate.cancel();
        assert!(gate.claim().is_err());
    }
    #[test]
    fn transfer_pause_epoch_rejects_pre_pause_admission() {
        let gate = Arc::new(PauseGate::default());
        let old = gate.epoch().unwrap().unwrap();
        assert!(gate.request_pause());
        assert!(gate.resume());
        assert!(gate.try_claim(old).unwrap().is_none());
        let current = gate.epoch().unwrap().unwrap();
        assert!(gate.try_claim(current).unwrap().is_some());
    }
}
