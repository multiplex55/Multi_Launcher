use crate::hotkey::Key;

use crate::gui::Panel;
use crate::hotkey::{parse_hotkey, Hotkey};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

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

/// Configuration for writing log output to a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LogFile {
    Flag(bool),
    Path(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DashboardSettings {
    #[serde(default = "default_dashboard_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub config_path: Option<String>,
    #[serde(default)]
    pub default_location: Option<String>,
    #[serde(default = "default_show_dashboard_when_empty")]
    pub show_when_query_empty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    System,
    Dark,
    Light,
    Custom,
}

impl Default for ThemeMode {
    fn default() -> Self {
        Self::System
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeColor {
    #[serde(default)]
    pub r: u8,
    #[serde(default)]
    pub g: u8,
    #[serde(default)]
    pub b: u8,
    #[serde(default = "default_alpha")]
    pub a: u8,
}

impl ThemeColor {
    const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

fn default_alpha() -> u8 {
    255
}

impl Default for ThemeColor {
    fn default() -> Self {
        Self::rgba(0, 0, 0, default_alpha())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorScheme {
    #[serde(default)]
    pub window_fill: ThemeColor,
    #[serde(default)]
    pub panel_fill: ThemeColor,
    #[serde(default)]
    pub text: ThemeColor,
    #[serde(default)]
    pub hyperlink: ThemeColor,
    #[serde(default)]
    pub selection_bg: ThemeColor,
    #[serde(default)]
    pub selection_stroke: ThemeColor,
    #[serde(default)]
    pub warn_accent: ThemeColor,
    #[serde(default)]
    pub error_accent: ThemeColor,
    #[serde(default)]
    pub success_accent: ThemeColor,
}

impl ColorScheme {
    pub fn dark() -> Self {
        Self {
            window_fill: ThemeColor::rgba(24, 24, 27, 255),
            panel_fill: ThemeColor::rgba(31, 31, 35, 255),
            text: ThemeColor::rgba(235, 235, 240, 255),
            hyperlink: ThemeColor::rgba(94, 173, 255, 255),
            selection_bg: ThemeColor::rgba(67, 121, 201, 210),
            selection_stroke: ThemeColor::rgba(170, 204, 255, 255),
            warn_accent: ThemeColor::rgba(255, 192, 92, 255),
            error_accent: ThemeColor::rgba(255, 104, 104, 255),
            success_accent: ThemeColor::rgba(116, 219, 149, 255),
        }
    }

    pub fn light() -> Self {
        Self {
            window_fill: ThemeColor::rgba(245, 246, 250, 255),
            panel_fill: ThemeColor::rgba(255, 255, 255, 255),
            text: ThemeColor::rgba(26, 30, 40, 255),
            hyperlink: ThemeColor::rgba(35, 102, 214, 255),
            selection_bg: ThemeColor::rgba(153, 194, 255, 220),
            selection_stroke: ThemeColor::rgba(48, 96, 170, 255),
            warn_accent: ThemeColor::rgba(219, 131, 0, 255),
            error_accent: ThemeColor::rgba(196, 36, 43, 255),
            success_accent: ThemeColor::rgba(34, 145, 93, 255),
        }
    }
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self::dark()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeSettings {
    #[serde(default)]
    pub mode: ThemeMode,
    #[serde(default)]
    pub named_presets: std::collections::HashMap<String, ColorScheme>,
    #[serde(default = "ThemeSettings::default_custom_scheme")]
    pub custom_scheme: ColorScheme,
}

impl ThemeSettings {
    pub fn default_dark() -> Self {
        Self {
            mode: ThemeMode::System,
            named_presets: std::collections::HashMap::from([
                ("dark".to_string(), ColorScheme::dark()),
                ("light".to_string(), ColorScheme::light()),
            ]),
            custom_scheme: ColorScheme::dark(),
        }
    }

    pub fn default_light() -> Self {
        Self {
            mode: ThemeMode::Light,
            named_presets: std::collections::HashMap::from([
                ("dark".to_string(), ColorScheme::dark()),
                ("light".to_string(), ColorScheme::light()),
            ]),
            custom_scheme: ColorScheme::light(),
        }
    }

    fn default_custom_scheme() -> ColorScheme {
        ColorScheme::dark()
    }
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self::default_dark()
    }
}

impl Default for DashboardSettings {
    fn default() -> Self {
        Self {
            enabled: default_dashboard_enabled(),
            config_path: None,
            default_location: None,
            show_when_query_empty: true,
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
    /// Enable logging to a file. Use `true` for the default `launcher.log` next
    /// to the executable or provide a custom path.
    #[serde(default)]
    pub log_file: Option<LogFile>,
    /// Position used to hide the window off-screen when not visible.
    /// Defaults to `(2000, 2000)` if missing.
    #[serde(default)]
    pub offscreen_pos: Option<(i32, i32)>,
    /// Last known window size. If absent, a default size is used.
    #[serde(default)]
    pub window_size: Option<(i32, i32)>,
    /// Default size for note editor panels.
    #[serde(default = "default_note_panel_size")]
    pub note_panel_default_size: (f32, f32),
    /// When enabled, the note panel saves its contents whenever its window is
    /// closed—whether by pressing `Esc`, clicking the window's close button, or
    /// any other close event. Defaults to `false` when the field is missing in
    /// the settings file.
    #[serde(default = "default_note_save_on_close")]
    pub note_save_on_close: bool,
    /// When true, saving a note overwrites existing files without prompting.
    #[serde(default)]
    pub note_always_overwrite: bool,
    /// When true, images in notes are rendered as links to avoid loading large
    /// textures directly in the preview.
    #[serde(default)]
    pub note_images_as_links: bool,
    /// Number of tags or links shown before an expandable "... (more)" control
    /// appears in the note panel.
    #[serde(default = "default_note_more_limit")]
    pub note_more_limit: usize,
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
    /// Enable autocomplete suggestions while typing a query.
    #[serde(default = "default_query_autocomplete")]
    pub query_autocomplete: bool,
    /// Number of results to move when paging through the action list.
    #[serde(default = "default_page_jump")]
    pub page_jump: usize,
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
    /// Keep the window always on top of other windows.
    #[serde(default = "default_always_on_top")]
    pub always_on_top: bool,
    /// Interval in seconds to refresh the timer list.
    #[serde(default = "default_timer_refresh")]
    pub timer_refresh: f32,
    /// When true, the timer list will not refresh automatically.
    #[serde(default)]
    pub disable_timer_updates: bool,
    /// Keep the command prefix in the query after running an action.
    #[serde(default)]
    pub preserve_command: bool,
    /// Clear the search query after successfully running an action.
    #[serde(default)]
    pub clear_query_after_run: bool,
    /// Require confirmation before destructive actions.
    #[serde(default = "default_true")]
    pub require_confirm_destructive: bool,
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
    /// Automatically save screenshots after editing without prompting.
    #[serde(default = "default_true")]
    pub screenshot_auto_save: bool,
    /// Enable the in-app screenshot editor after capture.
    #[serde(default = "default_true")]
    pub screenshot_use_editor: bool,
    #[serde(default)]
    pub plugin_settings: std::collections::HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub pinned_panels: Vec<Panel>,
    /// Reduce dashboard refresh work when the launcher is not focused.
    #[serde(default = "default_true")]
    pub reduce_dashboard_work_when_unfocused: bool,
    /// Show the dashboard diagnostics widget (developer option).
    #[serde(default)]
    pub show_dashboard_diagnostics: bool,
    #[serde(default)]
    pub dashboard: DashboardSettings,
    #[serde(default)]
    pub theme: ThemeSettings,
}

static SETTINGS_PATH: OnceCell<PathBuf> = OnceCell::new();

pub fn set_settings_path(path: impl Into<PathBuf>) {
    let _ = SETTINGS_PATH.set(path.into());
}

pub fn settings_path() -> PathBuf {
    SETTINGS_PATH
        .get()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("settings.json"))
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

fn default_query_autocomplete() -> bool {
    true
}

fn default_page_jump() -> usize {
    5
}

fn default_true() -> bool {
    true
}

fn default_follow_mouse() -> bool {
    true
}

fn default_always_on_top() -> bool {
    true
}

fn default_timer_refresh() -> f32 {
    1.0
}

fn default_net_refresh() -> f32 {
    1.0
}

fn default_dashboard_enabled() -> bool {
    true
}

fn default_show_dashboard_when_empty() -> bool {
    true
}

fn default_note_panel_size() -> (f32, f32) {
    (420.0, 320.0)
}

fn default_note_save_on_close() -> bool {
    false
}

fn default_note_more_limit() -> usize {
    5
}

fn default_log_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("launcher.log")))
        .unwrap_or_else(|| PathBuf::from("launcher.log"))
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
            log_file: None,
            offscreen_pos: Some((2000, 2000)),
            window_size: Some((400, 220)),
            note_panel_default_size: default_note_panel_size(),
            note_save_on_close: default_note_save_on_close(),
            note_always_overwrite: false,
            note_images_as_links: false,
            note_more_limit: default_note_more_limit(),
            enable_toasts: true,
            toast_duration: default_toast_duration(),
            query_scale: Some(1.0),
            list_scale: Some(1.0),
            fuzzy_weight: default_fuzzy_weight(),
            usage_weight: default_usage_weight(),
            query_autocomplete: default_query_autocomplete(),
            page_jump: default_page_jump(),
            history_limit: default_history_limit(),
            clipboard_limit: default_clipboard_limit(),
            follow_mouse: true,
            static_location_enabled: false,
            static_pos: None,
            static_size: None,
            hide_after_run: false,
            always_on_top: default_always_on_top(),
            timer_refresh: default_timer_refresh(),
            net_refresh: default_net_refresh(),
            net_unit: NetUnit::Auto,
            disable_timer_updates: false,
            preserve_command: false,
            clear_query_after_run: false,
            require_confirm_destructive: true,
            show_examples: false,
            screenshot_dir: Some(
                std::env::current_dir()
                    .unwrap_or_else(|_| std::env::temp_dir())
                    .join("MultiLauncher_Screenshots")
                    .to_string_lossy()
                    .to_string(),
            ),
            screenshot_save_file: true,
            screenshot_auto_save: true,
            screenshot_use_editor: true,
            plugin_settings: std::collections::HashMap::new(),
            pinned_panels: Vec::new(),
            reduce_dashboard_work_when_unfocused: true,
            show_dashboard_diagnostics: false,
            dashboard: DashboardSettings::default(),
            theme: ThemeSettings::default(),
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

    /// Determine the path for file logging if enabled.
    pub fn log_file_path(&self) -> Option<PathBuf> {
        match &self.log_file {
            Some(LogFile::Flag(true)) => Some(default_log_path()),
            Some(LogFile::Path(p)) => Some(PathBuf::from(p)),
            _ => None,
        }
    }
}
