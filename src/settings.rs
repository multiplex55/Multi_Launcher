use crate::hotkey::Key;

use crate::hotkey::{parse_hotkey, Hotkey};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub hotkey: Option<String>,
    pub quit_hotkey: Option<String>,
    /// Hotkey to show the quick help overlay. If `None`, the overlay is disabled.
    pub help_hotkey: Option<String>,
    pub index_paths: Option<Vec<String>>,
    pub plugin_dirs: Option<Vec<String>>,
    /// List of plugin names which should be enabled. If `None`, all loaded
    /// plugins are enabled.
    pub enabled_plugins: Option<Vec<String>>,
    /// Map of plugin capability identifiers enabled per plugin.
    pub enabled_capabilities: Option<std::collections::HashMap<String, Vec<String>>>,
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
    /// Enable toast notifications in the UI.
    #[serde(default = "default_toasts")]
    pub enable_toasts: bool,
    /// Scale factor for the search box. Defaults to `1.0`.
    #[serde(default = "default_scale")]
    pub query_scale: Option<f32>,
    /// Scale factor for the action list. Defaults to `1.0`.
    #[serde(default = "default_scale")]
    pub list_scale: Option<f32>,
    /// Weight of the fuzzy match score when ranking results.
    #[serde(default = "default_fuzzy_weight")]
    pub fuzzy_weight: f32,
    /// Weight of the usage count when ranking results.
    #[serde(default = "default_usage_weight")]
    pub usage_weight: f32,
    /// Maximum number of entries kept in the history list.
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,
    /// When true the window spawns at the mouse cursor each time it becomes
    /// visible.
    #[serde(default = "default_follow_mouse")]
    pub follow_mouse: bool,
    /// Enable positioning and sizing the window at a fixed location rather than
    /// following the cursor.
    #[serde(default)]
    pub static_location_enabled: bool,
    /// Position of the window when `static_location_enabled` is true.
    #[serde(default)]
    pub static_pos: Option<(i32, i32)>,
    /// Size of the window when `static_location_enabled` is true.
    #[serde(default)]
    pub static_size: Option<(i32, i32)>,
}

fn default_toasts() -> bool { true }

fn default_scale() -> Option<f32> { Some(1.0) }

fn default_history_limit() -> usize { 100 }

fn default_fuzzy_weight() -> f32 { 1.0 }

fn default_usage_weight() -> f32 { 1.0 }

fn default_follow_mouse() -> bool { true }

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: Some("F2".into()),
            quit_hotkey: None,
            help_hotkey: Some("F1".into()),
            index_paths: None,
            plugin_dirs: None,
            enabled_plugins: None,
            enabled_capabilities: None,
            debug_logging: false,
            offscreen_pos: Some((2000, 2000)),
            window_size: Some((400, 220)),
            enable_toasts: true,
            query_scale: Some(1.0),
            list_scale: Some(1.0),
            fuzzy_weight: default_fuzzy_weight(),
            usage_weight: default_usage_weight(),
            history_limit: default_history_limit(),
            follow_mouse: true,
            static_location_enabled: false,
            static_pos: None,
            static_size: None,
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

    /// Parse the help overlay hotkey if configured.
    pub fn help_hotkey(&self) -> Option<Hotkey> {
        if let Some(hotkey) = &self.help_hotkey {
            match parse_hotkey(hotkey) {
                Some(k) => return Some(k),
                None => {
                    tracing::warn!(
                        "provided help_hotkey string '{}' is invalid; ignoring",
                        hotkey
                    );
                }
            }
        }
        None
    }
}
