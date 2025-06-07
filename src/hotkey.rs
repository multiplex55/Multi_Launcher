use rdev::{listen, EventType, Key};
#[cfg(feature = "unstable_grab")]
use rdev::{grab, Event};
use std::sync::{Arc, Mutex};
use std::thread;

/// Parse a hotkey string like "Ctrl+Shift+Space" and return the final key.
pub fn parse_hotkey(s: &str) -> Option<Key> {
    let key_part = s.split('+').last()?.trim();
    let upper = key_part.to_ascii_uppercase();
    match upper.as_str() {
        "CTRL" | "CONTROL" => Some(Key::ControlLeft),
        "SHIFT" => Some(Key::ShiftLeft),
        "ALT" => Some(Key::Alt),
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
}

impl HotkeyTrigger {
    pub fn new(key: Key) -> Self {
        Self {
            open: Arc::new(Mutex::new(false)),
            key,
        }
    }

    pub fn start_listener(&self) {
        let open = self.open.clone();
        let watch = self.key;
        thread::spawn(move || {
            #[cfg(feature = "unstable_grab")]
            {
                if watch == Key::CapsLock {
                    let mut shift_pressed = false;
                    let callback = move |event: Event| -> Option<Event> {
                        match event.event_type {
                            EventType::KeyPress(k) => {
                                if k == Key::ShiftLeft || k == Key::ShiftRight {
                                    shift_pressed = true;
                                } else if k == watch {
                                    if !shift_pressed {
                                        if let Ok(mut flag) = open.lock() {
                                            *flag = true;
                                        }
                                        return None;
                                    }
                                }
                            }
                            EventType::KeyRelease(k) => {
                                if k == Key::ShiftLeft || k == Key::ShiftRight {
                                    shift_pressed = false;
                                }
                            }
                            _ => {}
                        }
                        Some(event)
                    };
                    if let Err(e) = grab(callback) {
                        eprintln!("Failed to grab events: {:?}", e);
                    }
                    return;
                }
            }

            listen(move |event| {
                if let EventType::KeyPress(k) = event.event_type {
                    if k == watch {
                        if let Ok(mut flag) = open.lock() {
                            *flag = true;
                        }
                    }
                }
            })
            .unwrap();
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
