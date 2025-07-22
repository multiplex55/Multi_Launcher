use crate::hotkey::Key;

use crate::hotkey::{parse_hotkey, Hotkey};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetUnit {
    Auto,
    B,
    Kb,
    Mb,
}

impl Default for NetUnit {
    fn default() -> Self {
        NetUnit::Auto
    }
}

impl std::fmt::Display for NetUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetUnit::Auto => write!(f, "Auto"),
            NetUnit::B => write!(f, "B/s"),
            NetUnit::Kb => write!(f, "kB/s"),
            NetUnit::Mb => write!(f, "MB/s"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub hotkey: Option<String>,
    pub quit_hotkey: Option<String>,
    /// Hotkey to show the quick help overlay. If `None`, the overlay is disabled.
    pub help_hotkey: Option<String>,
    pub index_paths: Option<Vec<String>>,
    pub plugin_dirs: Option<Vec<String>>,
    /// Set of plugin names which should be enabled. If `None`, all loaded
    /// plugins are enabled.
    pub enabled_plugins: Option<HashSet<String>>,
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
    /// Duration of toast notifications in seconds.
    #[serde(default = "default_toast_duration")]
    pub toast_duration: f32,
    /// Remember whether the help window shows example queries.
    #[serde(default)]
    pub show_examples: bool,
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
    #[serde(default = "default_clipboard_limit")]
    pub clipboard_limit: usize,
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
    /// Hide the main window automatically after successfully launching an action.
    #[serde(default)]
    pub hide_after_run: bool,
    /// Interval in seconds to refresh the timer list.
    #[serde(default = "default_timer_refresh")]
    pub timer_refresh: f32,
    /// When true, the timer list will not refresh automatically.
    #[serde(default)]
    pub disable_timer_updates: bool,
    /// Keep the command prefix in the query after running an action.
    #[serde(default)]
    pub preserve_command: bool,
    #[serde(default = "default_net_refresh")]
    pub net_refresh: f32,
    #[serde(default)]
    pub net_unit: NetUnit,
    /// Directory used for saving screenshots. If `None`, a platform default is
    /// used.
    pub screenshot_dir: Option<String>,
    /// When capturing screenshots to the clipboard, also save them to disk.
    #[serde(default)]
    pub screenshot_save_file: bool,
    #[serde(default)]
    pub plugin_settings: std::collections::HashMap<String, serde_json::Value>,
}

fn default_toasts() -> bool {
    true
}

fn default_toast_duration() -> f32 {
    3.0
}

fn default_scale() -> Option<f32> {
    Some(1.0)
}

fn default_history_limit() -> usize {
    100
}

fn default_clipboard_limit() -> usize {
    20
}

fn default_fuzzy_weight() -> f32 {
    1.0
}

fn default_usage_weight() -> f32 {
    1.0
}

fn default_follow_mouse() -> bool {
    true
}

fn default_timer_refresh() -> f32 {
    1.0
}

fn default_net_refresh() -> f32 {
    1.0
}

fn default_launcher_hotkey() -> Option<String> {
    if std::env::var("ML_DEFAULT_HOTKEY_NONE").is_ok() {
        None
    } else {
        Some("F2".into())
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: default_launcher_hotkey(),
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
            toast_duration: default_toast_duration(),
            query_scale: Some(1.0),
            list_scale: Some(1.0),
            fuzzy_weight: default_fuzzy_weight(),
            usage_weight: default_usage_weight(),
            history_limit: default_history_limit(),
            clipboard_limit: default_clipboard_limit(),
            follow_mouse: true,
            static_location_enabled: false,
            static_pos: None,
            static_size: None,
            hide_after_run: false,
            timer_refresh: default_timer_refresh(),
            net_refresh: default_net_refresh(),
            net_unit: NetUnit::Auto,
            disable_timer_updates: false,
            preserve_command: false,
            show_examples: false,
            screenshot_dir: Some(
                std::env::current_dir()
                    .unwrap_or_else(|_| std::env::temp_dir())
                    .join("MultiLauncher_Screenshots")
                    .to_string_lossy()
                    .to_string(),
            ),
            screenshot_save_file: true,
            plugin_settings: std::collections::HashMap::new(),
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
            win: false,
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
