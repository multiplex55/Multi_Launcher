use crate::gui::Panel;
use crate::hotkey::Key;
use crate::hotkey::{parse_hotkey, Hotkey};
use crate::settings::defaults::*;
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct QueryResultsLayoutSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_query_results_layout_rows")]
    pub rows: usize,
    #[serde(default = "default_query_results_layout_cols")]
    pub cols: usize,
    #[serde(default = "default_true")]
    pub respect_plugin_capability: bool,
    #[serde(default)]
    pub plugin_opt_out: Vec<String>,
}
impl Default for QueryResultsLayoutSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            rows: default_query_results_layout_rows(),
            cols: default_query_results_layout_cols(),
            respect_plugin_capability: true,
            plugin_opt_out: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NoteGraphSettings {
    #[serde(default = "default_note_graph_max_nodes")]
    pub max_nodes: usize,
    #[serde(default = "default_true")]
    pub show_labels: bool,
    #[serde(default = "default_note_graph_label_zoom_threshold")]
    pub label_zoom_threshold: f32,
    #[serde(default = "default_note_graph_layout_iterations_per_frame")]
    pub layout_iterations_per_frame: usize,
    #[serde(default = "default_note_graph_repulsion_strength")]
    pub repulsion_strength: f32,
    #[serde(default = "default_note_graph_link_distance")]
    pub link_distance: f32,
    #[serde(default = "default_note_graph_local_graph_depth")]
    pub local_graph_depth: usize,
    #[serde(default)]
    pub include_tags: Vec<String>,
    #[serde(default)]
    pub exclude_tags: Vec<String>,
}
impl Default for NoteGraphSettings {
    fn default() -> Self {
        Self {
            max_nodes: default_note_graph_max_nodes(),
            show_labels: true,
            label_zoom_threshold: default_note_graph_label_zoom_threshold(),
            layout_iterations_per_frame: default_note_graph_layout_iterations_per_frame(),
            repulsion_strength: default_note_graph_repulsion_strength(),
            link_distance: default_note_graph_link_distance(),
            local_graph_depth: default_note_graph_local_graph_depth(),
            include_tags: Vec::new(),
            exclude_tags: Vec::new(),
        }
    }
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
    pub widget_inactive_fill: ThemeColor,
    #[serde(default)]
    pub widget_inactive_stroke: ThemeColor,
    #[serde(default)]
    pub widget_hovered_fill: ThemeColor,
    #[serde(default)]
    pub widget_hovered_stroke: ThemeColor,
    #[serde(default)]
    pub widget_active_fill: ThemeColor,
    #[serde(default)]
    pub widget_active_stroke: ThemeColor,
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
            widget_inactive_fill: ThemeColor::rgba(49, 49, 55, 255),
            widget_inactive_stroke: ThemeColor::rgba(90, 90, 102, 255),
            widget_hovered_fill: ThemeColor::rgba(64, 64, 74, 255),
            widget_hovered_stroke: ThemeColor::rgba(133, 133, 152, 255),
            widget_active_fill: ThemeColor::rgba(84, 84, 100, 255),
            widget_active_stroke: ThemeColor::rgba(170, 170, 194, 255),
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
            widget_inactive_fill: ThemeColor::rgba(240, 241, 246, 255),
            widget_inactive_stroke: ThemeColor::rgba(183, 186, 198, 255),
            widget_hovered_fill: ThemeColor::rgba(229, 234, 246, 255),
            widget_hovered_stroke: ThemeColor::rgba(145, 160, 196, 255),
            widget_active_fill: ThemeColor::rgba(206, 220, 246, 255),
            widget_active_stroke: ThemeColor::rgba(103, 130, 184, 255),
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MultiManagerSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_multi_manager_workspaces_path")]
    pub workspaces_path: String,
    #[serde(default = "default_multi_manager_bindings_path")]
    pub bindings_path: String,
    #[serde(default = "default_true")]
    pub auto_save: bool,
    #[serde(default = "default_true")]
    pub save_on_exit: bool,
    #[serde(default)]
    pub developer_debugging: bool,
    #[serde(default)]
    pub show_force_recapture_prompt: bool,
    #[serde(default = "default_multi_manager_hotkey_poll_ms")]
    pub hotkey_poll_ms: u64,
    #[serde(default)]
    pub hide_launcher_before_toggle: bool,
    #[serde(default = "default_true")]
    pub ignore_launcher_window_on_capture: bool,
}

impl Default for MultiManagerSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            workspaces_path: default_multi_manager_workspaces_path(),
            bindings_path: default_multi_manager_bindings_path(),
            auto_save: true,
            save_on_exit: true,
            developer_debugging: false,
            show_force_recapture_prompt: false,
            hotkey_poll_ms: default_multi_manager_hotkey_poll_ms(),
            hide_launcher_before_toggle: false,
            ignore_launcher_window_on_capture: true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub hotkey: Option<String>,
    pub quit_hotkey: Option<String>,
    pub help_hotkey: Option<String>,
    pub index_paths: Option<Vec<String>>,
    pub max_indexed_items: Option<usize>,
    pub plugin_dirs: Option<Vec<String>>,
    pub enabled_plugins: Option<HashSet<String>>,
    pub enabled_capabilities: Option<std::collections::HashMap<String, Vec<String>>>,
    #[serde(default)]
    pub debug_logging: bool,
    #[serde(default)]
    pub log_file: Option<LogFile>,
    #[serde(default)]
    pub offscreen_pos: Option<(i32, i32)>,
    #[serde(default)]
    pub window_size: Option<(i32, i32)>,
    #[serde(default = "default_note_panel_size")]
    pub note_panel_default_size: (f32, f32),
    #[serde(default = "default_note_save_on_close")]
    pub note_save_on_close: bool,
    #[serde(default)]
    pub note_always_overwrite: bool,
    #[serde(default)]
    pub note_images_as_links: bool,
    #[serde(default = "default_note_show_details")]
    pub note_show_details: bool,
    #[serde(default = "default_note_more_limit")]
    pub note_more_limit: usize,
    #[serde(default = "default_toasts")]
    pub enable_toasts: bool,
    #[serde(default = "default_true")]
    pub show_inline_errors: bool,
    #[serde(default = "default_true")]
    pub show_error_toasts: bool,
    #[serde(default = "default_toast_duration")]
    pub toast_duration: f32,
    #[serde(default)]
    pub show_examples: bool,
    #[serde(default = "default_scale")]
    pub query_scale: Option<f32>,
    #[serde(default = "default_scale")]
    pub list_scale: Option<f32>,
    #[serde(default = "default_fuzzy_weight")]
    pub fuzzy_weight: f32,
    #[serde(default = "default_usage_weight")]
    pub usage_weight: f32,
    #[serde(default)]
    pub match_exact: bool,
    #[serde(default = "default_query_autocomplete")]
    pub query_autocomplete: bool,
    #[serde(default = "default_page_jump")]
    pub page_jump: usize,
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,
    #[serde(default = "default_clipboard_limit")]
    pub clipboard_limit: usize,
    #[serde(default = "default_follow_mouse")]
    pub follow_mouse: bool,
    #[serde(default)]
    pub static_location_enabled: bool,
    #[serde(default)]
    pub static_pos: Option<(i32, i32)>,
    #[serde(default)]
    pub static_size: Option<(i32, i32)>,
    #[serde(default)]
    pub hide_after_run: bool,
    #[serde(default = "default_always_on_top")]
    pub always_on_top: bool,
    #[serde(default = "default_timer_refresh")]
    pub timer_refresh: f32,
    #[serde(default)]
    pub disable_timer_updates: bool,
    #[serde(default)]
    pub preserve_command: bool,
    #[serde(default)]
    pub clear_query_after_run: bool,
    #[serde(default = "default_true")]
    pub require_confirm_destructive: bool,
    #[serde(default = "default_net_refresh")]
    pub net_refresh: f32,
    #[serde(default)]
    pub net_unit: NetUnit,
    pub screenshot_dir: Option<String>,
    #[serde(default)]
    pub screenshot_save_file: bool,
    #[serde(default = "default_true")]
    pub screenshot_auto_save: bool,
    #[serde(default = "default_true")]
    pub screenshot_use_editor: bool,
    #[serde(default)]
    pub plugin_settings: std::collections::HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub pinned_panels: Vec<Panel>,
    #[serde(default = "default_true")]
    pub reduce_dashboard_work_when_unfocused: bool,
    #[serde(default)]
    pub show_dashboard_diagnostics: bool,
    #[serde(default)]
    pub dashboard: DashboardSettings,
    #[serde(default)]
    pub theme: ThemeSettings,
    #[serde(default)]
    pub note_graph: NoteGraphSettings,
    #[serde(default)]
    pub query_results_layout: QueryResultsLayoutSettings,
    #[serde(default)]
    pub multi_manager: MultiManagerSettings,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: default_launcher_hotkey(),
            quit_hotkey: None,
            help_hotkey: Some("F1".into()),
            index_paths: None,
            max_indexed_items: None,
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
            note_show_details: default_note_show_details(),
            note_more_limit: default_note_more_limit(),
            enable_toasts: true,
            show_inline_errors: true,
            show_error_toasts: true,
            toast_duration: default_toast_duration(),
            query_scale: Some(1.0),
            list_scale: Some(1.0),
            fuzzy_weight: default_fuzzy_weight(),
            usage_weight: default_usage_weight(),
            match_exact: false,
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
            note_graph: NoteGraphSettings::default(),
            query_results_layout: QueryResultsLayoutSettings::default(),
            multi_manager: MultiManagerSettings::default(),
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
                None => tracing::warn!(
                    "provided hotkey string '{}' is invalid; using default F2",
                    hotkey
                ),
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
                None => tracing::warn!(
                    "provided quit_hotkey string '{}' is invalid; ignoring",
                    hotkey
                ),
            }
        }
        None
    }
    pub fn help_hotkey(&self) -> Option<Hotkey> {
        if let Some(hotkey) = &self.help_hotkey {
            match parse_hotkey(hotkey) {
                Some(k) => return Some(k),
                None => tracing::warn!(
                    "provided help_hotkey string '{}' is invalid; ignoring",
                    hotkey
                ),
            }
        }
        None
    }
    pub fn log_file_path(&self) -> Option<PathBuf> {
        match &self.log_file {
            Some(LogFile::Flag(true)) => Some(default_log_path()),
            Some(LogFile::Path(p)) => Some(PathBuf::from(p)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MultiManagerSettings, QueryResultsLayoutSettings, Settings};

    #[test]
    fn multi_manager_defaults_are_backward_compatible() {
        let parsed: Settings = serde_json::from_str("{}").expect("settings should deserialize");
        assert_eq!(parsed.multi_manager, MultiManagerSettings::default());
    }

    #[test]
    fn query_results_layout_defaults_are_backward_compatible() {
        let parsed: Settings = serde_json::from_str("{}").expect("settings should deserialize");
        assert_eq!(
            parsed.query_results_layout,
            QueryResultsLayoutSettings::default()
        );
    }

    #[test]
    fn query_results_layout_round_trip_serialization() {
        let mut settings = Settings::default();
        settings.query_results_layout.enabled = true;
        settings.query_results_layout.rows = 4;
        settings.query_results_layout.cols = 5;
        settings.query_results_layout.respect_plugin_capability = false;
        settings.query_results_layout.plugin_opt_out = vec!["note".into(), "todo".into()];
        let json = serde_json::to_string(&settings).expect("serialize settings");
        let restored: Settings = serde_json::from_str(&json).expect("deserialize settings");
        assert_eq!(restored.query_results_layout, settings.query_results_layout);
    }

    #[test]
    fn settings_snapshot_backcompat_defaults() {
        let parsed: Settings = serde_json::from_str("{}").expect("settings should deserialize");
        let snapshot = serde_json::json!({
            "note_show_details": parsed.note_show_details,
            "show_inline_errors": parsed.show_inline_errors,
            "show_error_toasts": parsed.show_error_toasts,
            "multi_manager": {
                "enabled": parsed.multi_manager.enabled,
                "workspaces_path": parsed.multi_manager.workspaces_path,
                "bindings_path": parsed.multi_manager.bindings_path,
                "auto_save": parsed.multi_manager.auto_save,
                "save_on_exit": parsed.multi_manager.save_on_exit,
                "developer_debugging": parsed.multi_manager.developer_debugging,
                "show_force_recapture_prompt": parsed.multi_manager.show_force_recapture_prompt,
                "hotkey_poll_ms": parsed.multi_manager.hotkey_poll_ms,
                "hide_launcher_before_toggle": parsed.multi_manager.hide_launcher_before_toggle,
                "ignore_launcher_window_on_capture": parsed.multi_manager.ignore_launcher_window_on_capture,
            },
            "query_results_layout": {
                "enabled": parsed.query_results_layout.enabled,
                "rows": parsed.query_results_layout.rows,
                "cols": parsed.query_results_layout.cols,
                "respect_plugin_capability": parsed.query_results_layout.respect_plugin_capability,
                "plugin_opt_out": parsed.query_results_layout.plugin_opt_out,
            }
        });
        assert_eq!(
            serde_json::to_string_pretty(&snapshot).unwrap(),
            "{\n  \"multi_manager\": {\n    \"auto_save\": true,\n    \"bindings_path\": \"multi_manager_bindings.json\",\n    \"developer_debugging\": false,\n    \"enabled\": true,\n    \"hide_launcher_before_toggle\": false,\n    \"hotkey_poll_ms\": 50,\n    \"ignore_launcher_window_on_capture\": true,\n    \"save_on_exit\": true,\n    \"show_force_recapture_prompt\": false,\n    \"workspaces_path\": \"multi_manager_workspaces.json\"\n  },\n  \"note_show_details\": false,\n  \"query_results_layout\": {\n    \"cols\": 2,\n    \"enabled\": false,\n    \"plugin_opt_out\": [],\n    \"respect_plugin_capability\": true,\n    \"rows\": 3\n  },\n  \"show_error_toasts\": true,\n  \"show_inline_errors\": true\n}"
        );
    }
}
