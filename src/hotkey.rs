#[cfg(target_os = "windows")]
pub use rdev::Key;

#[cfg(not(target_os = "windows"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Space,
    Tab,
    Return,
    Escape,
    Delete,
    Backspace,
    CapsLock,
    Home,
    End,
    PageUp,
    PageDown,
    LeftArrow,
    RightArrow,
    UpArrow,
    DownArrow,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
    ControlLeft,
    ControlRight,
    ShiftLeft,
    ShiftRight,
    MetaLeft,
    MetaRight,
    Alt,
    AltGr,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    KeyPress(Key),
    KeyRelease(Key),
}
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
    Arc, Mutex,
};
#[cfg(target_os = "windows")]
use std::thread;
#[cfg(target_os = "windows")]
use std::time::Duration;

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
            let Some(c) = upper.chars().next() else {
                return None;
            };
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
    pub _key: Key,
    pub _ctrl: bool,
    pub _shift: bool,
    pub _alt: bool,
    pub _win: bool,
}

pub struct HotkeyListener {
    stop: Arc<AtomicBool>,
}

impl HotkeyTrigger {
    pub fn new(hotkey: Hotkey) -> Self {
        Self {
            open: Arc::new(Mutex::new(false)),
            _key: hotkey.key,
            _ctrl: hotkey.ctrl,
            _shift: hotkey.shift,
            _alt: hotkey.alt,
            _win: hotkey.win,
        }
    }

    #[cfg(target_os = "windows")]
    pub fn start_listener(
        triggers: Vec<Arc<HotkeyTrigger>>,
        _label: &'static str,
        event_tx: Sender<()>,
    ) -> HotkeyListener {
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

        fn vk_from_key(key: Key) -> Option<i32> {
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

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = stop_flag.clone();
        let vk_keys: Vec<_> = triggers.iter().map(|t| vk_from_key(t._key)).collect();
        let watch_keys: Vec<Key> = triggers.iter().map(|t| t._key).collect();
        let need_ctrl: Vec<bool> = triggers.iter().map(|t| t._ctrl).collect();
        let need_shift: Vec<bool> = triggers.iter().map(|t| t._shift).collect();
        let need_alt: Vec<bool> = triggers.iter().map(|t| t._alt).collect();
        let need_win: Vec<bool> = triggers.iter().map(|t| t._win).collect();
        thread::spawn(move || {
            unsafe {
                let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
            }
            let open_listeners: Vec<_> = triggers.iter().map(|t| t.open.clone()).collect();
            let mut triggered = vec![false; triggers.len()];
            while !stop_clone.load(Ordering::SeqCst) {
                let ctrl_pressed = is_down(VK_LCONTROL.0 as i32) || is_down(VK_RCONTROL.0 as i32);
                let shift_pressed = is_down(VK_LSHIFT.0 as i32) || is_down(VK_RSHIFT.0 as i32);
                let alt_pressed = is_down(VK_LMENU.0 as i32) || is_down(VK_RMENU.0 as i32);
                let win_pressed = is_down(VK_LWIN.0 as i32) || is_down(VK_RWIN.0 as i32);

                for i in 0..vk_keys.len() {
                    if let Some(vk) = vk_keys[i] {
                        let key_down = is_down(vk);
                        let combo = if watch_keys[i] == Key::CapsLock
                            && !need_ctrl[i]
                            && !need_shift[i]
                            && !need_alt[i]
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
                                && (!need_win[i] || win_pressed)
                        };
                        if combo {
                            if !triggered[i] {
                                triggered[i] = true;
                                if let Ok(mut flag) = open_listeners[i].lock() {
                                    *flag = true;
                                }
                                let _ = event_tx.send(());
                            }
                        } else {
                            triggered[i] = false;
                        }
                    }
                }
                thread::sleep(Duration::from_millis(20));
            }
        });

        HotkeyListener { stop: stop_flag }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn start_listener(
        _triggers: Vec<Arc<HotkeyTrigger>>,
        _label: &'static str,
        _event_tx: Sender<()>,
    ) -> HotkeyListener {
        HotkeyListener {
            stop: Arc::new(AtomicBool::new(false)),
        }
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
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn process_test_events(triggers: &[Arc<HotkeyTrigger>], events: &[EventType]) {
    let open_listeners: Vec<_> = triggers.iter().map(|t| t.open.clone()).collect();
    let watch_keys: Vec<Key> = triggers.iter().map(|t| t._key).collect();
    let need_ctrl: Vec<bool> = triggers.iter().map(|t| t._ctrl).collect();
    let need_shift: Vec<bool> = triggers.iter().map(|t| t._shift).collect();
    let need_alt: Vec<bool> = triggers.iter().map(|t| t._alt).collect();
    let need_win: Vec<bool> = triggers.iter().map(|t| t._win).collect();

    let mut watch_pressed = vec![false; triggers.len()];
    let mut triggered = vec![false; triggers.len()];
    let mut ctrl_pressed = false;
    let mut shift_pressed = false;
    let mut alt_pressed = false;
    let mut win_pressed = false;

    for event in events {
        match *event {
            EventType::KeyPress(k) => {
                match k {
                    Key::ControlLeft | Key::ControlRight => ctrl_pressed = true,
                    Key::ShiftLeft | Key::ShiftRight => shift_pressed = true,
                    Key::Alt | Key::AltGr => alt_pressed = true,
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
                    Key::Alt | Key::AltGr => alt_pressed = false,
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
                && !need_win[i]
            {
                watch_pressed[i] && !ctrl_pressed && !shift_pressed && !alt_pressed && !win_pressed
            } else {
                watch_pressed[i]
                    && (!need_ctrl[i] || ctrl_pressed)
                    && (!need_shift[i] || shift_pressed)
                    && (!need_alt[i] || alt_pressed)
                    && (!need_win[i] || win_pressed)
            };
            if combo {
                if !triggered[i] {
                    triggered[i] = true;
                    if let Ok(mut flag) = open_listeners[i].lock() {
                        *flag = true;
                    }
                }
            } else {
                triggered[i] = false;
            }
        }
    }
}
