use rdev::Key;
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

    pub fn hotkey_key(&self) -> Key {
        match self.hotkey.as_deref() {
            Some("CapsLock") | None => Key::CapsLock,
            Some("F2") => Key::F2,
            Some("F1") => Key::F1,
            _ => Key::CapsLock,
        }
    }
}
