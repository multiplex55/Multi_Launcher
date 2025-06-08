use rdev::{listen, EventType, Key};
#[cfg(feature = "unstable_grab")]
use rdev::{grab, Event};
use std::sync::{Arc, Mutex};
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
}

impl HotkeyTrigger {
    pub fn new(hotkey: Hotkey) -> Self {
        Self {
            open: Arc::new(Mutex::new(false)),
            key: hotkey.key,
            ctrl: hotkey.ctrl,
            shift: hotkey.shift,
            alt: hotkey.alt,
        }
    }

    pub fn start_listener(&self) {
        let open = self.open.clone();
        let watch = self.key;
        let need_ctrl = self.ctrl;
        let need_shift = self.shift;
        let need_alt = self.alt;
        tracing::debug!("starting hotkey listener for {:?}", watch);
        thread::spawn(move || {
            let mut ctrl_pressed = false;
            let mut alt_pressed = false;
            #[cfg(feature = "unstable_grab")]
            {
                if watch == Key::CapsLock {
                    let mut shift_pressed = false;
                    let mut watch_pressed = false;
                    let mut triggered = false;
                    let callback = move |event: Event| -> Option<Event> {
                        tracing::debug!("grabbed event: {:?}", event.event_type);
                        match event.event_type {
                            EventType::KeyPress(k) => {
                                tracing::debug!("grab key press: {:?}", k);
                                if k == Key::ShiftLeft || k == Key::ShiftRight {
                                    shift_pressed = true;
                                }
                                if k == watch {
                                    watch_pressed = true;
                                }
                                tracing::debug!(
                                    "state: ctrl={}, shift={}, alt={}, watch={}",
                                    ctrl_pressed,
                                    shift_pressed,
                                    alt_pressed,
                                    watch_pressed
                                );
                            }
                            EventType::KeyRelease(k) => {
                                tracing::debug!("grab key release: {:?}", k);
                                if k == Key::ShiftLeft || k == Key::ShiftRight {
                                    shift_pressed = false;
                                }
                                if k == watch {
                                    watch_pressed = false;
                                }
                                tracing::debug!(
                                    "state: ctrl={}, shift={}, alt={}, watch={}",
                                    ctrl_pressed,
                                    shift_pressed,
                                    alt_pressed,
                                    watch_pressed
                                );
                            }
                            _ => {}
                        }

                        let combo = watch_pressed && !shift_pressed;
                        if combo {
                            tracing::debug!(
                                "combo={} state: ctrl={}, shift={}, alt={}, watch={}",
                                combo,
                                ctrl_pressed,
                                shift_pressed,
                                alt_pressed,
                                watch_pressed
                            );
                            if !triggered {
                                triggered = true;
                                tracing::debug!("hotkey match -> open=true");
                                if let Ok(mut flag) = open.lock() {
                                    *flag = true;
                                }
                                return None;
                            }
                        } else {
                            if triggered {
                                tracing::debug!("combo released");
                            }
                            triggered = false;
                        }

                        Some(event)
                    };
                    match grab(callback) {
                        Ok(()) => return,
                        Err(e) => {
                            tracing::error!("Failed to grab events: {:?}. Falling back to listening", e);
                        }
                    }
                }
            }

            loop {
                let mut watch_pressed = false;
                let mut triggered = false;
                let mut ctrl_pressed = false;
                let mut shift_pressed = false;
                let mut alt_pressed = false;
                let open_listener = open.clone();

                let result = listen(move |event| {
                    match event.event_type {
                        EventType::KeyPress(k) => {
                            tracing::debug!("key pressed: {:?}", k);
                            match k {
                                Key::ControlLeft | Key::ControlRight => ctrl_pressed = true,
                                Key::ShiftLeft | Key::ShiftRight => shift_pressed = true,
                                Key::Alt | Key::AltGr => alt_pressed = true,
                                _ => {}
                            }
                            if k == watch {
                                watch_pressed = true;
                            }
                            tracing::debug!(
                                "state: ctrl={}, shift={}, alt={}, watch={}",
                                ctrl_pressed,
                                shift_pressed,
                                alt_pressed,
                                watch_pressed
                            );
                        }
                        EventType::KeyRelease(k) => {
                            tracing::debug!("key released: {:?}", k);
                            match k {
                                Key::ControlLeft | Key::ControlRight => ctrl_pressed = false,
                                Key::ShiftLeft | Key::ShiftRight => shift_pressed = false,
                                Key::Alt | Key::AltGr => alt_pressed = false,
                                _ => {}
                            }
                            if k == watch {
                                watch_pressed = false;
                            }
                            tracing::debug!(
                                "state: ctrl={}, shift={}, alt={}, watch={}",
                                ctrl_pressed,
                                shift_pressed,
                                alt_pressed,
                                watch_pressed
                            );
                        }
                        _ => {}
                    }

                    let combo = watch_pressed
                        && (!need_ctrl || ctrl_pressed)
                        && (!need_shift || shift_pressed)
                        && (!need_alt || alt_pressed);
                    if combo {
                        tracing::debug!(
                            "combo={} state: ctrl={}, shift={}, alt={}, watch={}",
                            combo,
                            ctrl_pressed,
                            shift_pressed,
                            alt_pressed,
                            watch_pressed
                        );
                        if !triggered {
                            triggered = true;
                            tracing::debug!("hotkey match -> open=true");
                            if let Ok(mut flag) = open_listener.lock() {
                                *flag = true;
                            }
                        }
                    } else {
                        if triggered {
                            tracing::debug!("combo released");
                        }
                        triggered = false;
                    }
                });

                match result {
                    Ok(()) => tracing::warn!("Hotkey listener exited unexpectedly. Restarting shortly"),
                    Err(e) => tracing::warn!("Hotkey listener failed: {:?}. Retrying shortly", e),
                }

                thread::sleep(Duration::from_millis(500));
            }
        });
    }

    pub fn take(&self) -> bool {
        let mut open = self.open.lock().unwrap();
        if *open {
            *open = false;
            true
        } else {
            false
        }
    }
}
