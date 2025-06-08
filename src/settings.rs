use rdev::Key;

use crate::hotkey::{parse_hotkey, Hotkey};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub hotkey: Option<String>,
    pub index_paths: Option<Vec<String>>,
    pub plugin_dirs: Option<Vec<String>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: Some("CapsLock".into()),
            index_paths: None,
            plugin_dirs: None,
        }
    }
}

impl Settings {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        if content.is_empty() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_str(&content)?)
    }

    pub fn hotkey(&self) -> Hotkey {
        if let Some(hotkey) = &self.hotkey {
            if let Some(k) = parse_hotkey(hotkey) {
                return k;
            }
        }
        Hotkey {
            key: Key::CapsLock,
            ctrl: false,
            shift: false,
            alt: false,
        }
    }
}
