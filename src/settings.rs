use rdev::Key;

use crate::hotkey::{parse_hotkey, Hotkey};
use serde::{Deserialize, Serialize};

fn default_hidden_coord() -> f32 {
    2000.0
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub hotkey: Option<String>,
    pub quit_hotkey: Option<String>,
    pub index_paths: Option<Vec<String>>,
    pub plugin_dirs: Option<Vec<String>>,
    /// When enabled the application initialises the logger at debug level.
    /// Defaults to `false` when the field is missing in the settings file.
    #[serde(default)]
    pub debug_logging: bool,
    #[serde(default = "default_hidden_coord")]
    pub hidden_x: f32,
    #[serde(default = "default_hidden_coord")]
    pub hidden_y: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: Some("F2".into()),
            quit_hotkey: None,
            index_paths: None,
            plugin_dirs: None,
            debug_logging: false,
            hidden_x: default_hidden_coord(),
            hidden_y: default_hidden_coord(),
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

    pub fn save(&self, path: &str) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn hotkey(&self) -> Hotkey {
        if let Some(hotkey) = &self.hotkey {
            match parse_hotkey(hotkey) {
                Some(k) => return k,
                None => {
                    tracing::warn!(
                        "provided hotkey string '{}' is invalid; using default F2",
                        hotkey
                    );
                }
            }
        }
        Hotkey {
            key: Key::F2,
            ctrl: false,
            shift: false,
            alt: false,
        }
    }

    pub fn quit_hotkey(&self) -> Option<Hotkey> {
        if let Some(hotkey) = &self.quit_hotkey {
            match parse_hotkey(hotkey) {
                Some(k) => return Some(k),
                None => {
                    tracing::warn!(
                        "provided quit_hotkey string '{}' is invalid; ignoring",
                        hotkey
                    );
                }
            }
        }
        None
    }

    pub fn hidden_position(&self) -> (f32, f32) {
        (self.hidden_x, self.hidden_y)
    }
}
