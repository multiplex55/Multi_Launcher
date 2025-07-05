use rdev::{listen, EventType, Key};
#[cfg(feature = "unstable_grab")]
use rdev::{grab, Event};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct Hotkey {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Default for Hotkey {
    fn default() -> Self {
        Self {
            key: Key::CapsLock,
            ctrl: false,
            shift: false,
            alt: false,
        }
    }
}

/// Parse a hotkey string like "Ctrl+Shift+Space" into a [`Hotkey`].
pub fn parse_hotkey(s: &str) -> Option<Hotkey> {
    let mut ctrl = false;
    let mut shift = false;
    let mut alt = false;
    let mut key: Option<Key> = None;

    for part in s.split('+') {
        let upper = part.trim().to_ascii_uppercase();
        match upper.as_str() {
            "CTRL" | "CONTROL" => ctrl = true,
            "SHIFT" => shift = true,
            "ALT" => alt = true,
            "" => {},
            _ => {
                if let Some(k) = parse_key(&upper) {
                    key = Some(k);
                } else {
                    return None;
                }
            }
        }
    }

    key.map(|k| Hotkey {
        key: k,
        ctrl,
        shift,
        alt,
    })
}

fn parse_key(upper: &str) -> Option<Key> {
    match upper {
        "SPACE" => Some(Key::Space),
        "TAB" => Some(Key::Tab),
        "ENTER" | "RETURN" => Some(Key::Return),
        "ESC" | "ESCAPE" => Some(Key::Escape),
        "DELETE" => Some(Key::Delete),
        "BACKSPACE" => Some(Key::Backspace),
        "CAPSLOCK" => Some(Key::CapsLock),
        "HOME" => Some(Key::Home),
        "END" => Some(Key::End),
        "PAGEUP" => Some(Key::PageUp),
        "PAGEDOWN" => Some(Key::PageDown),
        "LEFT" | "LEFTARROW" => Some(Key::LeftArrow),
        "RIGHT" | "RIGHTARROW" => Some(Key::RightArrow),
        "UP" | "UPARROW" => Some(Key::UpArrow),
        "DOWN" | "DOWNARROW" => Some(Key::DownArrow),
        _ if upper.starts_with('F') => match upper[1..].parse::<u8>().ok() {
            Some(1) => Some(Key::F1),
            Some(2) => Some(Key::F2),
            Some(3) => Some(Key::F3),
            Some(4) => Some(Key::F4),
            Some(5) => Some(Key::F5),
            Some(6) => Some(Key::F6),
            Some(7) => Some(Key::F7),
            Some(8) => Some(Key::F8),
            Some(9) => Some(Key::F9),
            Some(10) => Some(Key::F10),
            Some(11) => Some(Key::F11),
            Some(12) => Some(Key::F12),
            _ => None,
        },
        _ if upper.len() == 1 => {
            let c = upper.chars().next().unwrap();
            if c.is_ascii_digit() {
                Some(match c {
                    '0' => Key::Num0,
                    '1' => Key::Num1,
                    '2' => Key::Num2,
                    '3' => Key::Num3,
                    '4' => Key::Num4,
                    '5' => Key::Num5,
                    '6' => Key::Num6,
                    '7' => Key::Num7,
                    '8' => Key::Num8,
                    '9' => Key::Num9,
                    _ => return None,
                })
            } else if c.is_ascii_alphabetic() {
                Some(match c {
                    'A' => Key::KeyA,
                    'B' => Key::KeyB,
                    'C' => Key::KeyC,
                    'D' => Key::KeyD,
                    'E' => Key::KeyE,
                    'F' => Key::KeyF,
                    'G' => Key::KeyG,
                    'H' => Key::KeyH,
                    'I' => Key::KeyI,
                    'J' => Key::KeyJ,
                    'K' => Key::KeyK,
                    'L' => Key::KeyL,
                    'M' => Key::KeyM,
                    'N' => Key::KeyN,
                    'O' => Key::KeyO,
                    'P' => Key::KeyP,
                    'Q' => Key::KeyQ,
                    'R' => Key::KeyR,
                    'S' => Key::KeyS,
                    'T' => Key::KeyT,
                    'U' => Key::KeyU,
                    'V' => Key::KeyV,
                    'W' => Key::KeyW,
                    'X' => Key::KeyX,
                    'Y' => Key::KeyY,
                    'Z' => Key::KeyZ,
                    _ => return None,
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

// Shared signal to open launcher
pub struct HotkeyTrigger {
    pub open: Arc<Mutex<bool>>,
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub mouse_pos: Arc<Mutex<(f64, f64)>>,
}

pub struct HotkeyListener {
    stop: Arc<AtomicBool>,
}

impl HotkeyTrigger {
    pub fn new(hotkey: Hotkey) -> Self {
        Self {
            open: Arc::new(Mutex::new(false)),
            key: hotkey.key,
            ctrl: hotkey.ctrl,
            shift: hotkey.shift,
            alt: hotkey.alt,
            mouse_pos: Arc::new(Mutex::new((0.0, 0.0))),
        }
    }

    pub fn start_listener(triggers: Vec<Arc<HotkeyTrigger>>, label: &'static str) -> HotkeyListener {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = stop_flag.clone();
        let watch_keys: Vec<Key> = triggers.iter().map(|t| t.key).collect();
        let need_ctrl: Vec<bool> = triggers.iter().map(|t| t.ctrl).collect();
        let need_shift: Vec<bool> = triggers.iter().map(|t| t.shift).collect();
        let need_alt: Vec<bool> = triggers.iter().map(|t| t.alt).collect();
        let mouse_positions: Vec<_> = triggers.iter().map(|t| t.mouse_pos.clone()).collect();
        thread::spawn(move || {
            while !stop_clone.load(Ordering::SeqCst) {
                let open_listeners: Vec<_> = triggers.iter().map(|t| t.open.clone()).collect();
                let mut watch_pressed = vec![false; triggers.len()];
                let mut triggered = vec![false; triggers.len()];
                let mut ctrl_pressed = false;
                let mut shift_pressed = false;
                let mut alt_pressed = false;
                let mut last_pos = (0.0f64, 0.0f64);
                let watch_keys = watch_keys.clone();
                let need_ctrl = need_ctrl.clone();
                let need_shift = need_shift.clone();
                let need_alt = need_alt.clone();

                let result = listen(move |event| {
                    match event.event_type {
                        EventType::KeyPress(k) => {
                            match k {
                                Key::ControlLeft | Key::ControlRight => ctrl_pressed = true,
                                Key::ShiftLeft | Key::ShiftRight => shift_pressed = true,
                                Key::Alt | Key::AltGr => alt_pressed = true,
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
                                Key::Alt | Key::AltGr => alt_pressed = false,
                                _ => {}
                            }
                            for (i, wk) in watch_keys.iter().enumerate() {
                                if k == *wk {
                                    watch_pressed[i] = false;
                                }
                            }
                        }
                        EventType::MouseMove { x, y } => {
                            last_pos = (x, y);
                        }
                        _ => {}
                    }

                    for i in 0..watch_keys.len() {
                        let combo = watch_pressed[i]
                            && (!need_ctrl[i] || ctrl_pressed)
                            && (!need_shift[i] || shift_pressed)
                            && (!need_alt[i] || alt_pressed);
                        if combo {
                            if !triggered[i] {
                                triggered[i] = true;
                                if let Ok(mut flag) = open_listeners[i].lock() {
                                    *flag = true;
                                }
                                if let Ok(mut mp) = mouse_positions[i].lock() {
                                    *mp = last_pos;
                                }
                            }
                        } else {
                            triggered[i] = false;
                        }
                    }
                });

                match result {
                    Ok(()) => tracing::warn!(%label, "Hotkey listener exited unexpectedly. Restarting shortly"),
                    Err(e) => tracing::warn!(%label, "Hotkey listener failed: {:?}. Retrying shortly", e),
                }

                thread::sleep(Duration::from_millis(500));
            }
        });

        HotkeyListener { stop: stop_flag }
    }

    pub fn take(&self) -> bool {
        let mut open = self.open.lock().unwrap();
        if *open {
            *open = false;
            tracing::debug!("HotkeyTrigger fired!");
            true
        } else {
            false
        }
    }

    pub fn mouse_pos(&self) -> (f64, f64) {
        *self.mouse_pos.lock().unwrap()
    }

}

impl HotkeyListener {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

pub fn process_test_events(triggers: &[Arc<HotkeyTrigger>], events: &[EventType]) {
    let open_listeners: Vec<_> = triggers.iter().map(|t| t.open.clone()).collect();
    let watch_keys: Vec<Key> = triggers.iter().map(|t| t.key).collect();
    let need_ctrl: Vec<bool> = triggers.iter().map(|t| t.ctrl).collect();
    let need_shift: Vec<bool> = triggers.iter().map(|t| t.shift).collect();
    let need_alt: Vec<bool> = triggers.iter().map(|t| t.alt).collect();
    let mouse_positions: Vec<_> = triggers.iter().map(|t| t.mouse_pos.clone()).collect();

    let mut watch_pressed = vec![false; triggers.len()];
    let mut triggered = vec![false; triggers.len()];
    let mut ctrl_pressed = false;
    let mut shift_pressed = false;
    let mut alt_pressed = false;
    let mut last_pos = (0.0f64, 0.0f64);

    for event in events {
        match *event {
            EventType::KeyPress(k) => {
                match k {
                    Key::ControlLeft | Key::ControlRight => ctrl_pressed = true,
                    Key::ShiftLeft | Key::ShiftRight => shift_pressed = true,
                    Key::Alt | Key::AltGr => alt_pressed = true,
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
                    Key::Alt | Key::AltGr => alt_pressed = false,
                    _ => {}
                }
                for (i, wk) in watch_keys.iter().enumerate() {
                    if k == *wk {
                        watch_pressed[i] = false;
                    }
                }
            }
            EventType::MouseMove { x, y } => {
                last_pos = (x, y);
            }
            _ => {}
        }

        for i in 0..watch_keys.len() {
            let combo = watch_pressed[i]
                && (!need_ctrl[i] || ctrl_pressed)
                && (!need_shift[i] || shift_pressed)
                && (!need_alt[i] || alt_pressed);
            if combo {
                if !triggered[i] {
                    triggered[i] = true;
                    if let Ok(mut flag) = open_listeners[i].lock() {
                        *flag = true;
                    }
                    if let Ok(mut mp) = mouse_positions[i].lock() {
                        *mp = last_pos;
                    }
                }
            } else {
                triggered[i] = false;
            }
        }
    }
}
