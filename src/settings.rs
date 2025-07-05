use rdev::Key;

use crate::hotkey::{parse_hotkey, Hotkey};
use serde::{Deserialize, Serialize};

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
    /// Position used to hide the window off-screen when not visible.
    /// Defaults to `(2000, 2000)` if missing.
    #[serde(default)]
    pub offscreen_pos: Option<(i32, i32)>,
    /// Last known window size. If absent, a default size is used.
    #[serde(default)]
    pub window_size: Option<(i32, i32)>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: Some("F2".into()),
            quit_hotkey: None,
            index_paths: None,
            plugin_dirs: None,
            debug_logging: false,
            offscreen_pos: Some((2000, 2000)),
            window_size: Some((400, 220)),
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
}
