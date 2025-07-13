#[cfg(target_os = "windows")]
use crate::workspace::is_valid_key_combo;
#[cfg(target_os = "windows")]
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_CONTROL, MOD_ALT,
    MOD_SHIFT, MOD_WIN,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Hotkey {
    pub key_sequence: String,
    #[cfg(target_os = "windows")]
    #[serde(skip)]
    pub id: Option<i32>,
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.key_sequence)
    }
}

impl Hotkey {
    #[cfg(target_os = "windows")]
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

    #[cfg(target_os = "windows")]
    pub fn register(&mut self, app: &crate::gui::LauncherApp, id: i32) -> bool {
        let mut modifiers: u32 = 0;
        let mut vk_code: Option<u32> = None;

        for part in self.key_sequence.split('+') {
            match part.to_lowercase().as_str() {
                "ctrl" => modifiers |= MOD_CONTROL.0,
                "alt" => modifiers |= MOD_ALT.0,
                "shift" => modifiers |= MOD_SHIFT.0,
                "win" => modifiers |= MOD_WIN.0,
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


    #[cfg(target_os = "windows")]
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
