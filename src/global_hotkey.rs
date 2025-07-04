use crate::workspace::is_valid_key_combo;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::fmt;
use windows::Win32::UI::Input::KeyboardAndMouse::RegisterHotKey;
use windows::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey;
use windows::Win32::UI::Input::KeyboardAndMouse::HOT_KEY_MODIFIERS;

#[derive(Clone, Serialize, Deserialize)]
pub struct Hotkey {
    pub key_sequence: String,
    #[serde(skip)]
    pub id: Option<i32>,
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.key_sequence)
    }
}

impl Hotkey {
    pub fn new(key_sequence: &str) -> Result<Self, String> {
        if is_valid_key_combo(key_sequence) {
            Ok(Self {
                key_sequence: key_sequence.to_string(),
                id: None,
            })
        } else {
            Err(format!("Invalid hotkey: '{}'", key_sequence))
        }
    }

    pub fn register(&mut self, app: &crate::gui::LauncherApp, id: i32) -> bool {
        let mut modifiers: u32 = 0;
        let mut vk_code: Option<u32> = None;

        for part in self.key_sequence.split('+') {
            match part.to_lowercase().as_str() {
                "ctrl" => modifiers |= windows::Win32::UI::Input::KeyboardAndMouse::MOD_CONTROL.0,
                "alt" => modifiers |= windows::Win32::UI::Input::KeyboardAndMouse::MOD_ALT.0,
                "shift" => modifiers |= windows::Win32::UI::Input::KeyboardAndMouse::MOD_SHIFT.0,
                "win" => modifiers |= windows::Win32::UI::Input::KeyboardAndMouse::MOD_WIN.0,
                _ => vk_code = crate::window_manager::virtual_key_from_string(part),
            }
        }

        if let Some(vk) = vk_code {
            unsafe {
                if RegisterHotKey(None, id, HOT_KEY_MODIFIERS(modifiers), vk).is_ok() {
                    self.id = Some(id);
                    let mut registered_hotkeys = app.registered_hotkeys.lock().unwrap();
                    registered_hotkeys.insert(self.key_sequence.clone(), id as usize);
                    info!("Registered hotkey '{}' with ID {}.", self.key_sequence, id);
                    return true;
                } else {
                    error!("Failed to register hotkey: '{}'.", self.key_sequence);
                }
            }
        } else {
            warn!("Invalid key sequence for hotkey '{}'.", self.key_sequence);
        }

        false
    }

    pub fn unregister(&self, app: &crate::gui::LauncherApp) -> bool {
        if let Some(id) = self.id {
            unsafe {
                if UnregisterHotKey(None, id).is_ok() {
                    let mut registered_hotkeys = app.registered_hotkeys.lock().unwrap();
                    registered_hotkeys.remove(&self.key_sequence);
                    info!("Unregistered hotkey '{}'.", self.key_sequence);
                    return true;
                } else {
                    warn!("Failed to unregister hotkey '{}'.", self.key_sequence);
                }
            }
        }
        false
    }
}
