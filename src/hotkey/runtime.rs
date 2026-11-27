use super::{EventType, Hotkey, Key};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};
use std::thread;
use std::time::Duration;

// Shared signal to open launcher
pub struct HotkeyTrigger {
    pub open: Arc<Mutex<bool>>,
    recording_stop_bridge: Option<Arc<crate::recording::RecordingStopHotkeyBridge>>,
    recording_stop_generation: Option<Arc<Mutex<Option<u64>>>>,
    scoped_route: Option<ScopedHotkeyRoute>,
    pub _key: Key,
    pub _ctrl: bool,
    pub _shift: bool,
    pub _alt: bool,
    pub _alt_gr: bool,
    pub _win: bool,
}

struct ScopedHotkeyRoute {
    capture_stamp: Box<dyn Fn() -> Option<u64> + Send + Sync>,
    pending_stamp: Mutex<Option<Option<u64>>>,
}

pub struct HotkeyListener {
    stop: Arc<AtomicBool>,
    active: Arc<AtomicBool>,
    suppress_first_active_scan: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
    triggers: Vec<Arc<HotkeyTrigger>>,
}

fn update_trigger_cycle(triggered: bool, combo: bool, suppress_new_press: bool) -> (bool, bool) {
    if combo {
        (true, !triggered && !suppress_new_press)
    } else {
        (false, false)
    }
}

impl HotkeyTrigger {
    pub fn new(hotkey: Hotkey) -> Self {
        Self {
            open: Arc::new(Mutex::new(false)),
            recording_stop_bridge: None,
            recording_stop_generation: None,
            scoped_route: None,
            _key: hotkey.key,
            _ctrl: hotkey.ctrl,
            _shift: hotkey.shift,
            _alt: hotkey.alt,
            _alt_gr: hotkey.alt_gr,
            _win: hotkey.win,
        }
    }

    /// Creates a process hotkey route whose press edge captures the current
    /// scope before sending the listener's ordinary wake notification.
    pub fn new_scoped(
        hotkey: Hotkey,
        capture_stamp: impl Fn() -> Option<u64> + Send + Sync + 'static,
    ) -> Self {
        let mut trigger = Self::new(hotkey);
        trigger.scoped_route = Some(ScopedHotkeyRoute {
            capture_stamp: Box::new(capture_stamp),
            pending_stamp: Mutex::new(None),
        });
        trigger
    }

    pub fn new_recording_stop(
        hotkey: Hotkey,
        bridge: Arc<crate::recording::RecordingStopHotkeyBridge>,
    ) -> Self {
        let mut trigger = Self::new(hotkey);
        trigger.recording_stop_bridge = Some(bridge);
        trigger.recording_stop_generation = Some(Arc::new(Mutex::new(None)));
        trigger
    }

    pub fn is_recording_stop(&self) -> bool {
        self.recording_stop_bridge.is_some()
    }

    pub fn hotkey(&self) -> Hotkey {
        Hotkey {
            key: self._key,
            ctrl: self._ctrl,
            shift: self._shift,
            alt: self._alt,
            alt_gr: self._alt_gr,
            win: self._win,
        }
    }

    pub fn take_recording_stop_generation(&self) -> Option<u64> {
        self.recording_stop_generation.as_ref()?.lock().ok()?.take()
    }

    /// Discard a hotkey admission observed while the main owner is fencing
    /// frontend-restart transitions. The press can be made again after the
    /// fence is released; it must not replay after Cancel or denial.
    pub fn clear_pending(&self) {
        if let Ok(mut open) = self.open.lock() {
            *open = false;
        }
        self.clear_recording_stop_pending();
        self.clear_scoped_pending();
    }

    fn clear_recording_stop_pending(&self) {
        if let Some(generation_slot) = self.recording_stop_generation.as_ref()
            && let Ok(mut generation) = generation_slot.lock()
        {
            *generation = None;
        }
    }

    fn clear_scoped_pending(&self) {
        if let Some(route) = self.scoped_route.as_ref()
            && let Ok(mut pending) = route.pending_stamp.lock()
        {
            *pending = None;
        }
    }

    /// Returns the press edge's captured scope. The outer option means there
    /// was a pending edge; an inner `None` means the edge occurred out of scope.
    pub fn take_scoped_stamp(&self) -> Option<Option<u64>> {
        self.scoped_route
            .as_ref()?
            .pending_stamp
            .lock()
            .ok()?
            .take()
    }

    /// Observe an edge while holding the exact lock used to publish it.
    /// The callback must not block or stop a listener.
    pub fn with_publication_guard<T>(&self, callback: impl FnOnce(bool) -> T) -> Result<T, String> {
        if let Some(route) = &self.scoped_route {
            let pending = route.pending_stamp.lock().map_err(|_| "scoped hotkey edge lock poisoned".to_owned())?;
            Ok(callback(pending.is_some()))
        } else {
            let pending = self.open.lock().map_err(|_| "hotkey edge lock poisoned".to_owned())?;
            Ok(callback(*pending))
        }
    }

    fn fire(&self) -> bool {
        if let Some(bridge) = self.recording_stop_bridge.as_ref() {
            let Some(generation) = bridge.capture_stoppable_generation() else {
                return false;
            };
            let Some(generation_slot) = self.recording_stop_generation.as_ref() else {
                return false;
            };
            let Ok(mut pending) = generation_slot.lock() else {
                return false;
            };
            if pending.is_some_and(|previous| previous >= generation) {
                return false;
            }
            *pending = Some(generation);
            true
        } else if let Some(route) = self.scoped_route.as_ref() {
            let stamp = (route.capture_stamp)();
            if let Ok(mut pending) = route.pending_stamp.lock() {
                *pending = Some(stamp);
                true
            } else {
                false
            }
        } else if let Ok(mut open) = self.open.lock() {
            *open = true;
            true
        } else {
            false
        }
    }

    pub fn start_listener(
        triggers: Vec<Arc<HotkeyTrigger>>,
        label: &'static str,
        event_tx: Sender<()>,
    ) -> HotkeyListener {
        // Existing settings may contain a key the polling path never mapped.
        // Keep valid saved bindings alive while warning about each unsupported
        // trigger; proposals use `prepare_listener` directly and fail as a
        // whole before any active listener is replaced.
        let triggers = triggers
            .into_iter()
            .filter(|trigger| {
                if legacy_virtual_key(trigger._key).is_some() {
                    true
                } else {
                    tracing::warn!(
                        key = ?trigger._key,
                        "saved hotkey is unsupported by the legacy polling listener; ignoring it"
                    );
                    false
                }
            })
            .collect();
        match Self::prepare_listener(triggers, label, event_tx) {
            Ok(listener) => {
                listener.activate();
                listener
            }
            Err(error) => {
                tracing::error!(%error, "hotkey listener could not be started");
                HotkeyListener::empty()
            }
        }
    }

    /// Starts a dormant listener after validating every trigger mapping.
    /// Call [`HotkeyListener::activate`] only when the owning route is ready
    /// to replace its current listener.
    pub fn prepare_listener(
        triggers: Vec<Arc<HotkeyTrigger>>,
        label: &'static str,
        event_tx: Sender<()>,
    ) -> Result<HotkeyListener, String> {
        if triggers.is_empty() {
            return Ok(HotkeyListener {
                stop: Arc::new(AtomicBool::new(true)),
                active: Arc::new(AtomicBool::new(false)),
                suppress_first_active_scan: Arc::new(AtomicBool::new(false)),
                join: None,
                triggers,
            });
        }
        use windows::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
        };
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_RCONTROL, VK_RMENU,
            VK_RSHIFT, VK_RWIN,
        };

        fn is_down(vk: i32) -> bool {
            unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
        }

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = stop_flag.clone();
        let active_flag = Arc::new(AtomicBool::new(false));
        let active_clone = active_flag.clone();
        let suppress_first_active_scan = Arc::new(AtomicBool::new(false));
        let suppress_scan = Arc::clone(&suppress_first_active_scan);
        let vk_keys = triggers
            .iter()
            .map(|trigger| {
                legacy_virtual_key(trigger._key).ok_or_else(|| {
                    format!(
                        "primary key {:?} is unsupported by the legacy polling listener",
                        trigger._key
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let watch_keys: Vec<Key> = triggers.iter().map(|t| t._key).collect();
        let need_ctrl: Vec<bool> = triggers.iter().map(|t| t._ctrl).collect();
        let need_shift: Vec<bool> = triggers.iter().map(|t| t._shift).collect();
        let need_alt: Vec<bool> = triggers.iter().map(|t| t._alt).collect();
        let need_alt_gr: Vec<bool> = triggers.iter().map(|t| t._alt_gr).collect();
        let need_win: Vec<bool> = triggers.iter().map(|t| t._win).collect();
        let triggers_for_listener: Vec<_> = triggers.iter().map(Arc::clone).collect();
        let trigger_count = triggers.len();
        let join = thread::Builder::new()
            .name(format!("hotkey-{label}"))
            .spawn(move || {
                unsafe {
                    let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
                }
                let mut triggered = vec![false; trigger_count];
                while !stop_clone.load(Ordering::SeqCst) {
                    if !active_clone.load(Ordering::Acquire) {
                        thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    let ctrl_pressed =
                        is_down(VK_LCONTROL.0 as i32) || is_down(VK_RCONTROL.0 as i32);
                    let shift_pressed = is_down(VK_LSHIFT.0 as i32) || is_down(VK_RSHIFT.0 as i32);
                    let alt_pressed = is_down(VK_LMENU.0 as i32) || is_down(VK_RMENU.0 as i32);
                    let alt_gr_pressed = is_down(VK_RMENU.0 as i32);
                    let win_pressed = is_down(VK_LWIN.0 as i32) || is_down(VK_RWIN.0 as i32);
                    let suppress_new_press = suppress_scan.swap(false, Ordering::AcqRel);

                    for i in 0..vk_keys.len() {
                        let key_down = is_down(vk_keys[i]);
                        let combo = if watch_keys[i] == Key::CapsLock
                            && !need_ctrl[i]
                            && !need_shift[i]
                            && !need_alt[i]
                            && !need_alt_gr[i]
                            && !need_win[i]
                        {
                            key_down
                                && !ctrl_pressed
                                && !shift_pressed
                                && !alt_pressed
                                && !win_pressed
                        } else {
                            key_down
                                && (!need_ctrl[i] || ctrl_pressed)
                                && (!need_shift[i] || shift_pressed)
                                && (!need_alt[i] || alt_pressed)
                                && (!need_alt_gr[i] || alt_gr_pressed)
                                && (!need_win[i] || win_pressed)
                        };
                        let (next_triggered, fire) =
                            update_trigger_cycle(triggered[i], combo, suppress_new_press);
                        triggered[i] = next_triggered;
                        if fire && triggers_for_listener[i].fire() {
                            let _ = event_tx.send(());
                        }
                    }
                    thread::sleep(Duration::from_millis(20));
                }
            })
            .map_err(|error| format!("failed to start hotkey listener thread: {error}"))?;

        Ok(HotkeyListener {
            stop: stop_flag,
            active: active_flag,
            suppress_first_active_scan,
            join: Some(join),
            triggers,
        })
    }

    pub fn take(&self) -> bool {
        let mut open = match self.open.lock() {
            Ok(g) => g,
            Err(e) => {
                tracing::error!("failed to lock hotkey trigger: {e}");
                return false;
            }
        };
        if *open {
            *open = false;
            tracing::debug!("HotkeyTrigger fired!");
            true
        } else {
            false
        }
    }
}

impl HotkeyListener {
    fn empty() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(true)),
            active: Arc::new(AtomicBool::new(false)),
            suppress_first_active_scan: Arc::new(AtomicBool::new(false)),
            join: None,
            triggers: Vec::new(),
        }
    }

    pub fn activate(&self) {
        for trigger in &self.triggers {
            trigger.clear_recording_stop_pending();
            trigger.clear_scoped_pending();
        }
        self.suppress_first_active_scan
            .store(true, Ordering::Release);
        self.active.store(true, Ordering::Release);
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire) && !self.stop.load(Ordering::Acquire)
    }

    pub fn triggers(&self) -> &[Arc<HotkeyTrigger>] {
        &self.triggers
    }

    pub fn stop_checked(&mut self) -> Result<(), String> {
        self.stop.store(true, Ordering::SeqCst);
        let join_result = self.join.take().map_or(Ok(()), |join| {
            join.join()
                .map_err(|_| "hotkey listener thread panicked during shutdown".to_owned())
        });
        for trigger in &self.triggers {
            trigger.clear_recording_stop_pending();
            trigger.clear_scoped_pending();
        }
        join_result
    }

    pub fn stop(&mut self) {
        if let Err(error) = self.stop_checked() {
            tracing::warn!(%error, "hotkey listener did not shut down cleanly");
        }
    }
}

/// Virtual-key mapping used by the legacy polling listener. Keep this shared
/// with route validation so unsupported chords are rejected before replacement.
pub fn legacy_virtual_key(key: Key) -> Option<i32> {
    use rdev::Key::*;
    Some(match key {
        F1 => 0x70,
        F2 => 0x71,
        F3 => 0x72,
        F4 => 0x73,
        F5 => 0x74,
        F6 => 0x75,
        F7 => 0x76,
        F8 => 0x77,
        F9 => 0x78,
        F10 => 0x79,
        F11 => 0x7A,
        F12 => 0x7B,
        KeyA => 0x41,
        KeyB => 0x42,
        KeyC => 0x43,
        KeyD => 0x44,
        KeyE => 0x45,
        KeyF => 0x46,
        KeyG => 0x47,
        KeyH => 0x48,
        KeyI => 0x49,
        KeyJ => 0x4A,
        KeyK => 0x4B,
        KeyL => 0x4C,
        KeyM => 0x4D,
        KeyN => 0x4E,
        KeyO => 0x4F,
        KeyP => 0x50,
        KeyQ => 0x51,
        KeyR => 0x52,
        KeyS => 0x53,
        KeyT => 0x54,
        KeyU => 0x55,
        KeyV => 0x56,
        KeyW => 0x57,
        KeyX => 0x58,
        KeyY => 0x59,
        KeyZ => 0x5A,
        Num0 => 0x30,
        Num1 => 0x31,
        Num2 => 0x32,
        Num3 => 0x33,
        Num4 => 0x34,
        Num5 => 0x35,
        Num6 => 0x36,
        Num7 => 0x37,
        Num8 => 0x38,
        Num9 => 0x39,
        Escape => 0x1B,
        Space => 0x20,
        Return => 0x0D,
        Tab => 0x09,
        Backspace => 0x08,
        Delete => 0x2E,
        Home => 0x24,
        End => 0x23,
        PageUp => 0x21,
        PageDown => 0x22,
        LeftArrow => 0x25,
        RightArrow => 0x27,
        UpArrow => 0x26,
        DownArrow => 0x28,
        CapsLock => 0x14,
        _ => return None,
    })
}

impl Drop for HotkeyListener {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn process_test_events(triggers: &[Arc<HotkeyTrigger>], events: &[EventType]) {
    let watch_keys: Vec<Key> = triggers.iter().map(|t| t._key).collect();
    let need_ctrl: Vec<bool> = triggers.iter().map(|t| t._ctrl).collect();
    let need_shift: Vec<bool> = triggers.iter().map(|t| t._shift).collect();
    let need_alt: Vec<bool> = triggers.iter().map(|t| t._alt).collect();
    let need_alt_gr: Vec<bool> = triggers.iter().map(|t| t._alt_gr).collect();
    let need_win: Vec<bool> = triggers.iter().map(|t| t._win).collect();

    let mut watch_pressed = vec![false; triggers.len()];
    let mut triggered = vec![false; triggers.len()];
    let mut ctrl_pressed = false;
    let mut shift_pressed = false;
    let mut alt_pressed = false;
    let mut alt_gr_pressed = false;
    let mut win_pressed = false;

    for event in events {
        match *event {
            EventType::KeyPress(k) => {
                match k {
                    Key::ControlLeft | Key::ControlRight => ctrl_pressed = true,
                    Key::ShiftLeft | Key::ShiftRight => shift_pressed = true,
                    Key::Alt => alt_pressed = true,
                    Key::AltGr => {
                        alt_pressed = true;
                        alt_gr_pressed = true;
                    }
                    Key::MetaLeft | Key::MetaRight => win_pressed = true,
                    _ => {}
                }
                for (i, wk) in watch_keys.iter().enumerate() {
                    if k == *wk {
                        watch_pressed[i] = true;
                    }
                }
            }
            EventType::KeyRelease(k) => {
                match k {
                    Key::ControlLeft | Key::ControlRight => ctrl_pressed = false,
                    Key::ShiftLeft | Key::ShiftRight => shift_pressed = false,
                    Key::Alt => alt_pressed = false,
                    Key::AltGr => {
                        alt_pressed = false;
                        alt_gr_pressed = false;
                    }
                    Key::MetaLeft | Key::MetaRight => win_pressed = false,
                    _ => {}
                }
                for (i, wk) in watch_keys.iter().enumerate() {
                    if k == *wk {
                        watch_pressed[i] = false;
                    }
                }
            }
        }

        for i in 0..watch_keys.len() {
            let combo = if watch_keys[i] == Key::CapsLock
                && !need_ctrl[i]
                && !need_shift[i]
                && !need_alt[i]
                && !need_alt_gr[i]
                && !need_win[i]
            {
                watch_pressed[i] && !ctrl_pressed && !shift_pressed && !alt_pressed && !win_pressed
            } else {
                watch_pressed[i]
                    && (!need_ctrl[i] || ctrl_pressed)
                    && (!need_shift[i] || shift_pressed)
                    && (!need_alt[i] || alt_pressed)
                    && (!need_alt_gr[i] || alt_gr_pressed)
                    && (!need_win[i] || win_pressed)
            };
            if combo {
                if !triggered[i] {
                    triggered[i] = true;
                    let _ = triggers[i].fire();
                }
            } else {
                triggered[i] = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_guard_observes_exact_scoped_and_unscoped_edges_without_consuming() {
        let hotkey = crate::hotkey::parse_hotkey("F2").unwrap();
        let launcher = HotkeyTrigger::new(hotkey);
        let help = HotkeyTrigger::new_scoped(hotkey, || Some(17));
        assert!(!launcher.with_publication_guard(|pending| pending).unwrap());
        assert!(!help.with_publication_guard(|pending| pending).unwrap());
        assert!(launcher.fire());
        assert!(help.fire());
        assert!(launcher.with_publication_guard(|pending| pending).unwrap());
        assert!(help.with_publication_guard(|pending| pending).unwrap());
        assert!(launcher.take());
        assert_eq!(help.take_scoped_stamp(), Some(Some(17)));
    }

    #[test]
    fn reactivated_listener_suppresses_an_already_held_chord_until_release() {
        let (triggered, fired) = update_trigger_cycle(false, true, true);
        assert!(triggered);
        assert!(!fired);

        let (triggered, fired) = update_trigger_cycle(triggered, true, false);
        assert!(triggered);
        assert!(!fired);

        let (triggered, fired) = update_trigger_cycle(triggered, false, false);
        assert!(!triggered);
        assert!(!fired);

        let (triggered, fired) = update_trigger_cycle(triggered, true, false);
        assert!(triggered);
        assert!(fired);
    }

    #[test]
    fn scoped_hotkey_press_retains_revision_and_explicitly_consumes_out_of_scope_edges() {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

        let in_scope = Arc::new(AtomicBool::new(true));
        let revision = Arc::new(AtomicU64::new(41));
        let capture_scope = Arc::clone(&in_scope);
        let capture_revision = Arc::clone(&revision);
        let trigger = Arc::new(HotkeyTrigger::new_scoped(
            crate::hotkey::parse_hotkey("F8").unwrap(),
            move || {
                capture_scope
                    .load(Ordering::Acquire)
                    .then(|| capture_revision.load(Ordering::Acquire))
            },
        ));

        let press_and_release = || {
            process_test_events(
                &[Arc::clone(&trigger)],
                &[EventType::KeyPress(Key::F8), EventType::KeyRelease(Key::F8)],
            );
        };

        press_and_release();
        revision.store(42, Ordering::Release);
        assert_eq!(trigger.take_scoped_stamp(), Some(Some(41)));

        press_and_release();
        // A newer edge replaces the unconsumed stamp in the same pending slot.
        revision.store(43, Ordering::Release);
        press_and_release();
        assert_eq!(trigger.take_scoped_stamp(), Some(Some(43)));
        assert_eq!(trigger.take_scoped_stamp(), None);

        in_scope.store(true, Ordering::Release);
        press_and_release();
        in_scope.store(false, Ordering::Release);
        assert!(trigger.fire());
        // An out-of-scope press replaces an older unconsumed visible intent.
        assert_eq!(trigger.take_scoped_stamp(), Some(None));
        assert_eq!(trigger.take_scoped_stamp(), None);
    }

    #[test]
    fn scoped_hotkey_keeps_press_edge_held_fence_and_clears_pending_scope_on_lifecycle() {
        let trigger = Arc::new(HotkeyTrigger::new_scoped(
            crate::hotkey::parse_hotkey("F8").unwrap(),
            || Some(9),
        ));
        process_test_events(
            &[Arc::clone(&trigger)],
            &[
                EventType::KeyPress(Key::F8),
                EventType::KeyPress(Key::F8),
                EventType::KeyPress(Key::F8),
            ],
        );
        assert_eq!(trigger.take_scoped_stamp(), Some(Some(9)));
        assert_eq!(trigger.take_scoped_stamp(), None);

        process_test_events(
            &[Arc::clone(&trigger)],
            &[EventType::KeyRelease(Key::F8), EventType::KeyPress(Key::F8)],
        );
        trigger.clear_pending();
        assert_eq!(trigger.take_scoped_stamp(), None);

        process_test_events(
            &[Arc::clone(&trigger)],
            &[EventType::KeyRelease(Key::F8), EventType::KeyPress(Key::F8)],
        );
        let listener = HotkeyListener {
            stop: Arc::new(AtomicBool::new(false)),
            active: Arc::new(AtomicBool::new(false)),
            suppress_first_active_scan: Arc::new(AtomicBool::new(false)),
            join: None,
            triggers: vec![Arc::clone(&trigger)],
        };
        listener.activate();
        assert_eq!(trigger.take_scoped_stamp(), None);
    }

    #[test]
    fn ordinary_hotkey_trigger_keeps_its_boolean_pending_behavior() {
        let trigger = Arc::new(HotkeyTrigger::new(
            crate::hotkey::parse_hotkey("F8").unwrap(),
        ));
        process_test_events(
            &[Arc::clone(&trigger)],
            &[EventType::KeyPress(Key::F8), EventType::KeyRelease(Key::F8)],
        );
        assert!(trigger.take());
        assert!(!trigger.take());
        assert_eq!(trigger.take_scoped_stamp(), None);
    }

    #[test]
    fn checked_stop_reports_listener_thread_panic() {
        let mut listener = HotkeyListener {
            stop: Arc::new(AtomicBool::new(false)),
            active: Arc::new(AtomicBool::new(false)),
            suppress_first_active_scan: Arc::new(AtomicBool::new(false)),
            join: Some(thread::spawn(|| panic!("listener test panic"))),
            triggers: Vec::new(),
        };

        assert!(listener.stop_checked().is_err());
    }

    #[test]
    fn legacy_start_keeps_supported_saved_bindings_when_one_key_is_unsupported() {
        let launcher = Arc::new(HotkeyTrigger::new(
            crate::hotkey::parse_hotkey("F2").unwrap(),
        ));
        let unsupported_quit = Arc::new(HotkeyTrigger::new(
            crate::hotkey::parse_hotkey("F13").unwrap(),
        ));
        let (events, event_rx) = std::sync::mpsc::channel();
        drop(event_rx);

        assert!(
            HotkeyTrigger::prepare_listener(
                vec![Arc::clone(&launcher), Arc::clone(&unsupported_quit)],
                "strict-test",
                events.clone(),
            )
            .is_err()
        );

        let mut listener =
            HotkeyTrigger::start_listener(vec![launcher, unsupported_quit], "compat-test", events);
        assert!(listener.is_active());
        assert_eq!(listener.triggers().len(), 1);
        assert_eq!(listener.triggers()[0]._key, Key::F2);
        listener.stop();
    }

    #[test]
    fn legacy_stop_edge_freezes_generation_in_single_payload_slot() {
        use std::sync::atomic::{AtomicU64, Ordering};

        let projection = Arc::new(AtomicU64::new(0));
        let bridge = Arc::new(crate::recording::RecordingStopHotkeyBridge::default());
        bridge.attach_projection_for_test(&projection);
        let trigger = Arc::new(HotkeyTrigger::new_recording_stop(
            crate::hotkey::parse_hotkey("Ctrl+F10").unwrap(),
            Arc::clone(&bridge),
        ));

        projection.store(27, Ordering::Release);
        process_test_events(
            &[Arc::clone(&trigger)],
            &[
                EventType::KeyPress(Key::ControlLeft),
                EventType::KeyPress(Key::F10),
            ],
        );
        projection.store(28, Ordering::Release);
        assert_eq!(trigger.take_recording_stop_generation(), Some(27));
    }

    #[test]
    fn legacy_stop_slot_replaces_stale_generation_and_candidate_clear_is_isolated() {
        use std::sync::atomic::{AtomicU64, Ordering};

        let projection = Arc::new(AtomicU64::new(0));
        let bridge = Arc::new(crate::recording::RecordingStopHotkeyBridge::default());
        bridge.attach_projection_for_test(&projection);
        let installed = Arc::new(HotkeyTrigger::new_recording_stop(
            crate::hotkey::parse_hotkey("Ctrl+F10").unwrap(),
            Arc::clone(&bridge),
        ));
        let edge = || {
            process_test_events(
                &[Arc::clone(&installed)],
                &[
                    EventType::KeyPress(Key::ControlLeft),
                    EventType::KeyPress(Key::F10),
                ],
            );
        };

        projection.store(31, Ordering::Release);
        edge();
        projection.store(32, Ordering::Release);
        edge();
        assert_eq!(installed.take_recording_stop_generation(), Some(32));

        projection.store(33, Ordering::Release);
        edge();
        let candidate = Arc::new(HotkeyTrigger::new_recording_stop(
            crate::hotkey::parse_hotkey("Ctrl+F10").unwrap(),
            bridge,
        ));
        process_test_events(
            &[Arc::clone(&candidate)],
            &[
                EventType::KeyPress(Key::ControlLeft),
                EventType::KeyPress(Key::F10),
            ],
        );
        assert_eq!(
            *candidate
                .recording_stop_generation
                .as_ref()
                .unwrap()
                .lock()
                .unwrap(),
            Some(33)
        );
        candidate.clear_recording_stop_pending();
        assert_eq!(installed.take_recording_stop_generation(), Some(33));
    }
}
