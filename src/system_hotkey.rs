#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
    MOD_CONTROL, MOD_ALT, MOD_SHIFT, MOD_WIN,
};
use serde::{Serialize, Deserialize};
use log::{info, warn, error};

#[derive(Clone, Serialize, Deserialize)]
pub struct SystemHotkey {
    pub key_sequence: String,
    #[cfg(target_os = "windows")]
    pub id: Option<i32>,
}

impl SystemHotkey {
    pub fn new(seq: &str) -> Result<Self, String> {
        // Basic validation using existing parser
        if crate::hotkey::parse_hotkey(seq).is_some() {
            Ok(Self { key_sequence: seq.to_string(), #[cfg(target_os = "windows")] id: None })
        } else {
            Err(format!("Invalid hotkey: '{seq}'"))
        }
    }

    #[cfg(target_os = "windows")]
    fn parse_sequence(&self) -> Option<(u32, u32)> {
        let mut mods: u32 = 0;
        let mut key: Option<u32> = None;
        for part in self.key_sequence.split('+') {
            match part.to_lowercase().as_str() {
                "ctrl" => mods |= MOD_CONTROL.0,
                "alt" => mods |= MOD_ALT.0,
                "shift" => mods |= MOD_SHIFT.0,
                "win" => mods |= MOD_WIN.0,
                other => {
                    key = crate::window_manager::virtual_key_from_string(other);
                }
            }
        }
        key.map(|k| (mods, k))
    }

    #[cfg(target_os = "windows")]
    pub fn register(&mut self, id: i32) -> bool {
        if let Some((mods, key)) = self.parse_sequence() {
            unsafe {
                if RegisterHotKey(None, id, HOT_KEY_MODIFIERS(mods), key).is_ok() {
                    self.id = Some(id);
                    info!("Registered hotkey '{}' with ID {}", self.key_sequence, id);
                    return true;
                }
            }
            error!("Failed to register hotkey '{}'", self.key_sequence);
        } else {
            warn!("Invalid key sequence '{}', cannot register", self.key_sequence);
        }
        false
    }

    #[cfg(target_os = "windows")]
    pub fn unregister(&self) -> bool {
        if let Some(id) = self.id {
            unsafe {
                if UnregisterHotKey(None, id).is_ok() {
                    info!("Unregistered hotkey '{}'", self.key_sequence);
                    return true;
                }
            }
            warn!("Failed to unregister hotkey '{}'", self.key_sequence);
        }
        false
    }

    #[cfg(not(target_os = "windows"))]
    pub fn register(&mut self, _id: i32) -> bool {
        // On non-Windows just pretend success
        info!("Registering hotkey '{}' (noop)", self.key_sequence);
        true
    }

    #[cfg(not(target_os = "windows"))]
    pub fn unregister(&self) -> bool {
        info!("Unregistering hotkey '{}' (noop)", self.key_sequence);
        true
    }
}
