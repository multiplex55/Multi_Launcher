//! Bounded physical-paste admission. The hook owns key cycles; the playback
//! controller owns sessions and explicitly publishes executor readiness.
use crate::hotkey::launcher_invocation::{InputProvenance, KeyEvent, KeyTransition};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak, mpsc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PasteFocus {
    pub hwnd: isize,
    pub process_id: u32,
    pub thread_id: u32,
}

impl PasteFocus {
    pub fn eligible(self, own_process: u32) -> bool {
        self.hwnd != 0
            && self.process_id != 0
            && self.thread_id != 0
            && self.process_id != own_process
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Session {
    owner: u64,
    generation: u64,
}

#[derive(Default)]
struct State {
    observer_live: bool,
    session: Option<Session>,
    invalidated: Option<Session>,
    next_permit: u64,
    in_flight: Option<u64>,
    busy_reported: bool,
    notices: VecDeque<PhysicalPasteNotice>,
    failure: Option<(u64, u64, &'static str)>,
    wake: Option<mpsc::Sender<()>>,
}

#[derive(Default)]
pub struct PasteAdmissionBridge {
    state: Mutex<State>,
    armed: AtomicBool,
    contended: AtomicBool,
    hook_wake: OnceLock<mpsc::Sender<()>>,
}

pub fn physical_paste_bridge() -> &'static Arc<PasteAdmissionBridge> {
    static BRIDGE: OnceLock<Arc<PasteAdmissionBridge>> = OnceLock::new();
    BRIDGE.get_or_init(|| Arc::new(PasteAdmissionBridge::default()))
}

#[derive(Clone, Debug)]
pub struct PhysicalPasteAdmission {
    pub owner: u64,
    pub generation: u64,
    pub focus: PasteFocus,
    permit: Arc<Permit>,
}

#[derive(Debug)]
struct Permit {
    id: u64,
    bridge: Weak<PasteAdmissionBridge>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        if let Some(bridge) = self.bridge.upgrade()
            && let Ok(mut state) = bridge.state.lock()
            && state.in_flight == Some(self.id)
        {
            fail(
                &mut state,
                "Physical paste delivery was lost; restart the sequence.",
            );
            bridge.armed.store(false, Ordering::Release);
        }
    }
}

#[derive(Clone, Debug)]
pub enum PhysicalPasteNotice {
    Paste(PhysicalPasteAdmission),
    Busy {
        owner: u64,
        generation: u64,
    },
    Failed {
        owner: u64,
        generation: u64,
        reason: &'static str,
    },
}

fn fail(state: &mut State, reason: &'static str) {
    if let Some(session) = state.session.take() {
        state.invalidated = Some(session);
        state.failure = Some((session.owner, session.generation, reason));
    }
    state.in_flight = None;
    state.busy_reported = false;
    // Do not drop queued permits while holding their bridge lock.
}

impl PasteAdmissionBridge {
    pub(crate) fn arm(&self, owner: u64, generation: u64) -> Result<(), &'static str> {
        self.publish(owner, Some(generation), true);
        let state = self
            .state
            .lock()
            .map_err(|_| "Physical paste admission is unavailable.")?;
        if !state.observer_live {
            return Err(
                "Physical paste observer is unavailable; restart after input service recovery.",
            );
        }
        if state.session != Some(Session { owner, generation }) {
            return Err("Physical paste session could not be armed; restart the sequence.");
        }
        Ok(())
    }
    pub fn observer_live(&self, live: bool) {
        if let Ok(mut state) = self.state.lock() {
            state.observer_live = live;
            if !live {
                self.armed.store(false, Ordering::Release);
                fail(
                    &mut state,
                    "Physical paste observer stopped; restart the sequence.",
                );
            }
        }
    }
    pub fn install_wake(&self, wake: mpsc::Sender<()>) {
        let _ = self.hook_wake.set(wake.clone());
        if let Ok(mut state) = self.state.lock() {
            state.wake = Some(wake);
        }
    }

    /// Only a successfully started controller with a checked executor may arm.
    /// Preparing/Ready and the transport-only M5-B checkpoint pass `false`.
    pub fn publish(&self, owner: u64, generation: Option<u64>, executor_ready: bool) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        self.apply_contention(&mut state);
        let candidate = generation
            .filter(|_| executor_ready && state.observer_live)
            .map(|generation| Session { owner, generation })
            .filter(|candidate| Some(*candidate) != state.invalidated);
        if state.session.is_some_and(|session| session.owner != owner) {
            return;
        }
        if state.session == candidate {
            return;
        }
        state.session = candidate;
        self.armed.store(candidate.is_some(), Ordering::Release);
        state.in_flight = None;
        state.busy_reported = false;
        let retired = std::mem::take(&mut state.notices);
        drop(state);
        drop(retired);
    }

    pub fn invalidate(&self) {
        self.armed.store(false, Ordering::Release);
        if let Ok(mut state) = self.state.lock() {
            fail(
                &mut state,
                "Physical paste session was invalidated; restart the sequence.",
            );
        }
    }

    pub fn invalidate_owner(&self, owner: u64) {
        if let Ok(mut state) = self.state.lock()
            && state.session.is_some_and(|session| session.owner == owner)
        {
            self.armed.store(false, Ordering::Release);
            fail(&mut state, "The sequence event owner closed.");
        }
    }

    pub fn valid(&self, admission: &PhysicalPasteAdmission) -> bool {
        if self.contended.load(Ordering::Acquire) || !self.armed.load(Ordering::Acquire) {
            return false;
        }
        self.state.lock().is_ok_and(|state| {
            !self.contended.load(Ordering::Acquire)
                && self.armed.load(Ordering::Acquire)
                && state.session
                    == Some(Session {
                        owner: admission.owner,
                        generation: admission.generation,
                    })
                && state.in_flight == Some(admission.permit.id)
        })
    }

    pub fn finish(&self, admission: &PhysicalPasteAdmission) {
        if let Ok(mut state) = self.state.lock()
            && state.in_flight == Some(admission.permit.id)
        {
            state.in_flight = None;
            state.busy_reported = false;
        }
    }

    pub fn delivery_failed(&self) {
        self.armed.store(false, Ordering::Release);
        if let Ok(mut state) = self.state.lock() {
            fail(
                &mut state,
                "Physical paste could not reach its playback owner; restart the sequence.",
            );
        }
    }

    pub fn take_notice(&self) -> Option<PhysicalPasteNotice> {
        let mut state = self.state.lock().ok()?;
        self.apply_contention(&mut state);
        state.notices.pop_front()
    }

    pub fn take_failure(&self) -> Option<(u64, u64, &'static str)> {
        let mut state = self.state.lock().ok()?;
        self.apply_contention(&mut state);
        state.failure.take()
    }

    fn apply_contention(&self, state: &mut State) {
        if self.contended.swap(false, Ordering::AcqRel) {
            self.armed.store(false, Ordering::Release);
            fail(
                state,
                "Physical paste admission was busy; restart the sequence.",
            );
        }
    }

    /// Called only from the hook. Reservation, bounded enqueue and wake happen
    /// before the V cycle is claimed. Active lock contention claims only the
    /// physical cycle and defers one explicit failure, never a stale OS paste.
    fn reserve(self: &Arc<Self>, focus: PasteFocus) -> bool {
        let Ok(mut state) = self.state.try_lock() else {
            if !self.armed.load(Ordering::Acquire) {
                return false;
            }
            // Never let an active physical paste silently become a normal OS
            // paste of the preceding step merely because the owner is busy.
            // One atomic signal and one coalesced wake defer failure handling.
            if !self.contended.swap(true, Ordering::AcqRel)
                && let Some(wake) = self.hook_wake.get()
            {
                let _ = wake.send(());
            }
            return true;
        };
        self.apply_contention(&mut state);
        let Some(session) = state.session else {
            return false;
        };
        if state.notices.len() >= 2 && !(state.in_flight.is_some() && state.busy_reported) {
            self.armed.store(false, Ordering::Release);
            fail(
                &mut state,
                "Physical paste queue is full; restart the sequence.",
            );
            return false;
        }
        if state.in_flight.is_some() {
            if state.busy_reported {
                return true;
            }
            if !state.busy_reported {
                state.notices.push_back(PhysicalPasteNotice::Busy {
                    owner: session.owner,
                    generation: session.generation,
                });
                state.busy_reported = true;
            }
        } else {
            state.next_permit = state.next_permit.wrapping_add(1);
            let id = state.next_permit;
            state.in_flight = Some(id);
            state
                .notices
                .push_back(PhysicalPasteNotice::Paste(PhysicalPasteAdmission {
                    owner: session.owner,
                    generation: session.generation,
                    focus,
                    permit: Arc::new(Permit {
                        id,
                        bridge: Arc::downgrade(self),
                    }),
                }));
        }
        if state
            .wake
            .as_ref()
            .is_none_or(|wake| wake.send(()).is_err())
        {
            self.armed.store(false, Ordering::Release);
            fail(
                &mut state,
                "Physical paste wake failed; restart the sequence.",
            );
            return false;
        }
        true
    }
}

/// Physical state is separate from the launcher's injected-input policy.
/// A contaminated or held startup baseline must drain before fresh admission.
pub(crate) struct PhysicalPasteReducer {
    down: [bool; 9],
    injected_modifiers: [bool; 8],
    known: bool,
    owned_v: bool,
}

const KEYS: [u32; 9] = [0xA2, 0xA3, 0xA0, 0xA1, 0xA4, 0xA5, 0x5B, 0x5C, 0x56];

impl PhysicalPasteReducer {
    pub(crate) fn new(down: [bool; 9]) -> Self {
        Self {
            injected_modifiers: [false; 8],
            known: !down.iter().any(|down| *down),
            down,
            owned_v: false,
        }
    }

    pub(crate) fn has_owned_cycle(&self) -> bool {
        self.owned_v
    }

    pub(crate) fn invalidate(&mut self, continuity: bool) {
        self.known = false;
        if !continuity {
            self.owned_v = false;
        }
    }

    pub(crate) fn process(
        &mut self,
        event: KeyEvent,
        focus: Option<PasteFocus>,
        own_process: u32,
        eligible_owner: bool,
        bridge: &Arc<PasteAdmissionBridge>,
    ) -> bool {
        let Some(index) = KEYS.iter().position(|key| *key == event.vk) else {
            return false;
        };
        if event.provenance != InputProvenance::Physical {
            // Injected modifier transitions cannot establish a physical chord.
            if index < 8 && event.provenance == InputProvenance::ExternalInjected {
                self.injected_modifiers[index] = event.transition != KeyTransition::Up;
                self.known = false;
            }
            return false;
        }
        let was_down = self.down[index];
        self.down[index] = event.transition != KeyTransition::Up;
        if index == 8 && self.owned_v {
            if event.transition == KeyTransition::Up {
                self.owned_v = false;
            }
            if self.clean() {
                self.known = true;
            }
            return true;
        }
        let fresh = event.transition == KeyTransition::Down && !was_down;
        let chord = (self.down[0] || self.down[1]) && !self.down[2..8].iter().any(|down| *down);
        let consume = self.known
            && index == 8
            && fresh
            && chord
            && eligible_owner
            && focus
                .filter(|focus| focus.eligible(own_process))
                .is_some_and(|focus| bridge.reserve(focus));
        if consume {
            self.owned_v = true;
        }
        if self.clean() {
            self.known = true;
        }
        consume
    }

    fn clean(&self) -> bool {
        !self.down.iter().any(|down| *down) && !self.injected_modifiers.iter().any(|down| *down)
    }
}

#[cfg(all(windows, not(test)))]
pub(crate) fn physical_baseline() -> [bool; 9] {
    KEYS.map(|key| unsafe {
        windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(key as i32) < 0
    })
}

/// Title-free hook snapshot. Full protected-control validation belongs to the
/// asynchronous playback boundary, which rechecks this exact identity.
#[cfg(all(windows, not(test)))]
pub(crate) fn foreground_focus() -> Option<PasteFocus> {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    let hwnd = unsafe { GetForegroundWindow() };
    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    Some(PasteFocus {
        hwnd: hwnd.0 as isize,
        process_id,
        thread_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOCUS: PasteFocus = PasteFocus {
        hwnd: 42,
        process_id: 9,
        thread_id: 11,
    };

    fn fixture() -> (
        Arc<PasteAdmissionBridge>,
        PhysicalPasteReducer,
        mpsc::Receiver<()>,
    ) {
        let bridge = Arc::new(PasteAdmissionBridge::default());
        let (tx, rx) = mpsc::channel();
        bridge.install_wake(tx);
        bridge.observer_live(true);
        bridge.publish(4, Some(7), true);
        (bridge, PhysicalPasteReducer::new([false; 9]), rx)
    }

    fn event(
        reducer: &mut PhysicalPasteReducer,
        bridge: &Arc<PasteAdmissionBridge>,
        vk: u32,
        transition: KeyTransition,
        provenance: InputProvenance,
    ) -> bool {
        reducer.process(
            KeyEvent {
                vk,
                transition,
                provenance,
                at: 0,
            },
            Some(FOCUS),
            1,
            true,
            bridge,
        )
    }

    fn physical(
        reducer: &mut PhysicalPasteReducer,
        bridge: &Arc<PasteAdmissionBridge>,
        vk: u32,
        transition: KeyTransition,
    ) -> bool {
        event(reducer, bridge, vk, transition, InputProvenance::Physical)
    }

    fn take_paste(bridge: &PasteAdmissionBridge) -> PhysicalPasteAdmission {
        match bridge.take_notice().unwrap() {
            PhysicalPasteNotice::Paste(admission) => admission,
            other => panic!("expected paste, got {other:?}"),
        }
    }

    #[test]
    fn physical_paste_fresh_cycles_are_bounded_and_owned_until_release() {
        let (bridge, mut reducer, _wake) = fixture();
        assert!(!physical(&mut reducer, &bridge, 0xA2, KeyTransition::Down));
        assert!(physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
        let first = take_paste(&bridge);
        assert_eq!((first.owner, first.generation, first.focus), (4, 7, FOCUS));
        assert!(bridge.valid(&first));
        assert!(physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
        assert!(physical(&mut reducer, &bridge, 0x56, KeyTransition::Repeat));
        assert!(bridge.take_notice().is_none());
        assert!(physical(&mut reducer, &bridge, 0x56, KeyTransition::Up));
        for _ in 0..4 {
            assert!(physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
            assert!(physical(&mut reducer, &bridge, 0x56, KeyTransition::Up));
        }
        assert!(matches!(
            bridge.take_notice(),
            Some(PhysicalPasteNotice::Busy { generation: 7, .. })
        ));
        assert!(
            bridge.take_notice().is_none(),
            "busy presses coalesce rather than queue steps"
        );
        bridge.finish(&first);
        assert!(physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
        let second = take_paste(&bridge);
        assert_ne!(first.permit.id, second.permit.id);
        bridge.finish(&second);
    }

    #[test]
    fn physical_paste_injected_shift_altgr_and_unrelated_input_pass_through() {
        for modifier in [0xA0, 0xA1, 0xA4, 0xA5, 0x5B, 0x5C] {
            let (bridge, mut reducer, _wake) = fixture();
            physical(&mut reducer, &bridge, 0xA2, KeyTransition::Down);
            physical(&mut reducer, &bridge, modifier, KeyTransition::Down);
            assert!(!physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
            assert!(!physical(&mut reducer, &bridge, 0x56, KeyTransition::Up));
            assert!(bridge.take_notice().is_none());
        }
        let (bridge, mut reducer, _wake) = fixture();
        for provenance in [
            InputProvenance::SelfInjected,
            InputProvenance::ExternalInjected,
        ] {
            assert!(!event(
                &mut reducer,
                &bridge,
                0xA2,
                KeyTransition::Down,
                provenance
            ));
            assert!(!event(
                &mut reducer,
                &bridge,
                0x56,
                KeyTransition::Down,
                provenance
            ));
            assert!(!event(
                &mut reducer,
                &bridge,
                0x56,
                KeyTransition::Up,
                provenance
            ));
            assert!(!event(
                &mut reducer,
                &bridge,
                0xA2,
                KeyTransition::Up,
                provenance
            ));
        }
        assert!(!physical(&mut reducer, &bridge, 0x43, KeyTransition::Down));
        assert!(!physical(&mut reducer, &bridge, 0x58, KeyTransition::Down));
        assert!(bridge.take_notice().is_none());
    }

    #[test]
    fn physical_paste_unknown_baseline_and_contamination_require_safe_boundary() {
        let (bridge, _, _wake) = fixture();
        let mut baseline = [false; 9];
        baseline[0] = true;
        let mut reducer = PhysicalPasteReducer::new(baseline);
        assert!(!physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
        physical(&mut reducer, &bridge, 0x56, KeyTransition::Up);
        physical(&mut reducer, &bridge, 0xA2, KeyTransition::Up);
        physical(&mut reducer, &bridge, 0xA3, KeyTransition::Down);
        event(
            &mut reducer,
            &bridge,
            0xA5,
            KeyTransition::Down,
            InputProvenance::ExternalInjected,
        );
        assert!(!physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
        physical(&mut reducer, &bridge, 0x56, KeyTransition::Up);
        physical(&mut reducer, &bridge, 0xA3, KeyTransition::Up);
        physical(&mut reducer, &bridge, 0xA3, KeyTransition::Down);
        assert!(!physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
        physical(&mut reducer, &bridge, 0x56, KeyTransition::Up);
        physical(&mut reducer, &bridge, 0xA3, KeyTransition::Up);
        event(
            &mut reducer,
            &bridge,
            0xA5,
            KeyTransition::Up,
            InputProvenance::ExternalInjected,
        );
        // A complete physical release after the injected modifier is released
        // establishes the next safe baseline.
        physical(&mut reducer, &bridge, 0xA3, KeyTransition::Up);
        physical(&mut reducer, &bridge, 0xA3, KeyTransition::Down);
        assert!(physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
        bridge.finish(&take_paste(&bridge));
    }

    #[test]
    fn physical_paste_generation_invalidation_preserves_owned_up_and_rejects_stale() {
        let (bridge, mut reducer, _wake) = fixture();
        physical(&mut reducer, &bridge, 0xA2, KeyTransition::Down);
        assert!(physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
        let old = take_paste(&bridge);
        bridge.invalidate();
        reducer.invalidate(true);
        bridge.publish(4, Some(7), true);
        assert!(!bridge.valid(&old));
        bridge.publish(4, Some(8), true);
        assert!(!event(
            &mut reducer,
            &bridge,
            0x56,
            KeyTransition::Up,
            InputProvenance::SelfInjected
        ));
        assert!(reducer.has_owned_cycle());
        assert!(physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
        assert!(physical(&mut reducer, &bridge, 0x56, KeyTransition::Up));
        assert!(!reducer.has_owned_cycle());
        physical(&mut reducer, &bridge, 0xA2, KeyTransition::Up);
        physical(&mut reducer, &bridge, 0xA2, KeyTransition::Down);
        assert!(physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
        let new = take_paste(&bridge);
        assert_eq!(new.generation, 8);
        drop(old);
        assert!(
            bridge.valid(&new),
            "old permit drop cannot invalidate replacement session"
        );
        bridge.finish(&new);
    }

    #[test]
    fn physical_paste_focus_owner_readiness_and_observer_admission_fail_closed() {
        let (bridge, mut reducer, _wake) = fixture();
        physical(&mut reducer, &bridge, 0xA2, KeyTransition::Down);
        for focus in [
            None,
            Some(PasteFocus {
                process_id: 1,
                ..FOCUS
            }),
            Some(PasteFocus {
                thread_id: 0,
                ..FOCUS
            }),
            Some(PasteFocus { hwnd: 0, ..FOCUS }),
        ] {
            assert!(!reducer.process(
                KeyEvent {
                    vk: 0x56,
                    transition: KeyTransition::Down,
                    provenance: InputProvenance::Physical,
                    at: 0
                },
                focus,
                1,
                true,
                &bridge
            ));
            physical(&mut reducer, &bridge, 0x56, KeyTransition::Up);
        }
        assert!(!reducer.process(
            KeyEvent {
                vk: 0x56,
                transition: KeyTransition::Down,
                provenance: InputProvenance::Physical,
                at: 0
            },
            Some(FOCUS),
            1,
            false,
            &bridge
        ));
        physical(&mut reducer, &bridge, 0x56, KeyTransition::Up);
        bridge.publish(4, Some(8), false);
        assert!(!physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
        physical(&mut reducer, &bridge, 0x56, KeyTransition::Up);
        bridge.observer_live(false);
        bridge.publish(4, Some(9), true);
        assert!(!physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
        assert!(bridge.take_notice().is_none());
    }

    #[test]
    fn physical_paste_delivery_loss_owner_drop_and_failed_wake_disarm() {
        let (bridge, mut reducer, wake) = fixture();
        physical(&mut reducer, &bridge, 0xA2, KeyTransition::Down);
        assert!(physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
        drop(take_paste(&bridge));
        assert!(bridge.take_failure().unwrap().2.contains("lost"));
        physical(&mut reducer, &bridge, 0x56, KeyTransition::Up);
        bridge.publish(4, Some(8), true);
        assert!(physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
        let admission = take_paste(&bridge);
        bridge.invalidate_owner(5);
        assert!(bridge.valid(&admission));
        bridge.invalidate_owner(4);
        assert!(!bridge.valid(&admission));
        physical(&mut reducer, &bridge, 0x56, KeyTransition::Up);
        bridge.publish(4, Some(9), true);
        drop(wake);
        assert!(!physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
        assert!(!reducer.has_owned_cycle());
        assert!(bridge.take_failure().unwrap().2.contains("wake"));
    }

    #[test]
    fn physical_paste_lock_contention_consumes_and_coalesces_failure_without_blocking() {
        let (bridge, mut reducer, wake) = fixture();
        physical(&mut reducer, &bridge, 0xA2, KeyTransition::Down);
        let guard = bridge.state.lock().unwrap();
        for _ in 0..5 {
            assert!(physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
            assert!(physical(&mut reducer, &bridge, 0x56, KeyTransition::Up));
        }
        assert_eq!(
            wake.try_iter().count(),
            1,
            "contended presses produce one bounded wake"
        );
        drop(guard);
        assert!(bridge.take_notice().is_none());
        assert!(bridge.take_failure().unwrap().2.contains("busy"));
        bridge.publish(4, Some(7), true);
        assert!(
            !physical(&mut reducer, &bridge, 0x56, KeyTransition::Down),
            "failed generation cannot rearm"
        );

        let (bridge, mut reducer, _wake) = fixture();
        physical(&mut reducer, &bridge, 0xA2, KeyTransition::Down);
        physical(&mut reducer, &bridge, 0x56, KeyTransition::Down);
        let admission = take_paste(&bridge);
        physical(&mut reducer, &bridge, 0x56, KeyTransition::Up);
        let guard = bridge.state.lock().unwrap();
        let checking = Arc::clone(&bridge);
        let checked_admission = admission.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let check = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            checking.valid(&checked_admission)
        });
        ready_rx.recv().unwrap();
        assert!(physical(&mut reducer, &bridge, 0x56, KeyTransition::Down));
        drop(guard);
        assert!(
            !check.join().unwrap(),
            "waiting validation must observe the contention fence"
        );
    }
}
