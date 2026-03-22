pub use rdev::Key;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    KeyPress(Key),
    KeyRelease(Key),
}

#[derive(Debug, Clone, Copy)]
pub struct Hotkey {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
}

impl Default for Hotkey {
    fn default() -> Self {
        Self {
            key: Key::CapsLock,
            ctrl: false,
            shift: false,
            alt: false,
            win: false,
        }
    }
}

/// Parse a hotkey string like "Ctrl+Shift+Space" into a [`Hotkey`].
pub fn parse_hotkey(s: &str) -> Option<Hotkey> {
    let mut ctrl = false;
    let mut shift = false;
    let mut alt = false;
    let mut win = false;
    let mut key: Option<Key> = None;

    for part in s.split('+') {
        let upper = part.trim().to_ascii_uppercase();
        match upper.as_str() {
            "CTRL" | "CONTROL" => ctrl = true,
            "SHIFT" => shift = true,
            "ALT" => alt = true,
            "WIN" | "SUPER" => win = true,
            "" => {}
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
        win,
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
            Some(13) => Some(Key::F13),
            Some(14) => Some(Key::F14),
            Some(15) => Some(Key::F15),
            Some(16) => Some(Key::F16),
            Some(17) => Some(Key::F17),
            Some(18) => Some(Key::F18),
            Some(19) => Some(Key::F19),
            Some(20) => Some(Key::F20),
            Some(21) => Some(Key::F21),
            Some(22) => Some(Key::F22),
            Some(23) => Some(Key::F23),
            Some(24) => Some(Key::F24),
            _ => None,
        },
        _ if upper.len() == 1 => {
            let c = upper.chars().next()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modifier_hotkey() {
        let hotkey = parse_hotkey("Ctrl + Shift + F12").unwrap();
        assert!(hotkey.ctrl && hotkey.shift);
        assert_eq!(hotkey.key, Key::F12);
    }

    #[test]
    fn rejects_unknown_key() {
        assert!(parse_hotkey("Ctrl+Nope").is_none());
    }
}
