use super::state::SettingsEditor;
use crate::hotkey::parse_hotkey;
use crate::plugins::screenshot::ScreenshotPluginSettings;
use crate::settings::{QueryResultsLayoutSettings, Settings};

impl SettingsEditor {
    pub fn from_settings(settings: &Settings) -> Self {
        let hotkey = settings.hotkey.clone().unwrap_or_default();
        let hotkey_valid = parse_hotkey(&hotkey).is_some();
        let default_hotkey = Settings::default().hotkey.unwrap_or_else(|| "F2".into());
        let last_valid_hotkey = if hotkey_valid {
            hotkey.clone()
        } else {
            default_hotkey.clone()
        };
        let quit_hotkey = settings.quit_hotkey.clone().unwrap_or_default();
        let quit_hotkey_enabled = settings.quit_hotkey.is_some();
        let quit_hotkey_valid = if quit_hotkey_enabled {
            parse_hotkey(&quit_hotkey).is_some()
        } else {
            true
        };
        let last_valid_quit_hotkey = if quit_hotkey_valid {
            quit_hotkey.clone()
        } else {
            String::new()
        };
        let help_hotkey = settings.help_hotkey.clone().unwrap_or_default();
        let help_hotkey_enabled = settings.help_hotkey.is_some();
        let help_hotkey_valid = if help_hotkey_enabled {
            parse_hotkey(&help_hotkey).is_some()
        } else {
            true
        };
        let last_valid_help_hotkey = if help_hotkey_valid {
            help_hotkey.clone()
        } else {
            String::new()
        };
        let follow_mouse = settings.follow_mouse;
        let (static_enabled, _, _) = Self::normalized_static_settings(
            follow_mouse,
            settings.static_location_enabled,
            settings.static_pos.unwrap_or((0, 0)),
            settings.static_size.unwrap_or((400, 220)),
        );
        let mut s = Self {
            hotkey,
            hotkey_valid,
            last_valid_hotkey,
            quit_hotkey_enabled,
            quit_hotkey,
            quit_hotkey_valid,
            last_valid_quit_hotkey,
            help_hotkey_enabled,
            help_hotkey,
            help_hotkey_valid,
            last_valid_help_hotkey,
            debug_logging: settings.debug_logging,
            show_toasts: settings.enable_toasts,
            show_inline_errors: settings.show_inline_errors,
            show_error_toasts: settings.show_error_toasts,
            toast_duration: settings.toast_duration,
            offscreen_x: settings.offscreen_pos.unwrap_or((2000, 2000)).0,
            offscreen_y: settings.offscreen_pos.unwrap_or((2000, 2000)).1,
            window_w: settings.window_size.unwrap_or((400, 220)).0,
            window_h: settings.window_size.unwrap_or((400, 220)).1,
            note_panel_w: settings.note_panel_default_size.0,
            note_panel_h: settings.note_panel_default_size.1,
            note_save_on_close: settings.note_save_on_close,
            note_always_overwrite: settings.note_always_overwrite,
            note_images_as_links: settings.note_images_as_links,
            note_show_details: settings.note_show_details,
            note_more_limit: settings.note_more_limit,
            query_scale: settings.query_scale.unwrap_or(1.0),
            list_scale: settings.list_scale.unwrap_or(1.0),
            history_limit: settings.history_limit,
            clipboard_limit: settings.clipboard_limit,
            fuzzy_weight: settings.fuzzy_weight,
            usage_weight: settings.usage_weight,
            match_exact: settings.match_exact,
            page_jump: settings.page_jump,
            query_results_layout_enabled: settings.query_results_layout.enabled,
            query_results_layout_rows: settings.query_results_layout.rows.max(1),
            query_results_layout_cols: settings.query_results_layout.cols.max(1),
            query_results_layout_respect_plugin_capability: settings
                .query_results_layout
                .respect_plugin_capability,
            query_results_layout_plugin_opt_out: settings
                .query_results_layout
                .plugin_opt_out
                .join(", "),
            follow_mouse,
            static_enabled,
            static_x: settings.static_pos.unwrap_or((0, 0)).0,
            static_y: settings.static_pos.unwrap_or((0, 0)).1,
            static_w: settings.static_size.unwrap_or((400, 220)).0,
            static_h: settings.static_size.unwrap_or((400, 220)).1,
            hide_after_run: settings.hide_after_run,
            always_on_top: settings.always_on_top,
            timer_refresh: settings.timer_refresh,
            disable_timer_updates: settings.disable_timer_updates,
            preserve_command: settings.preserve_command,
            clear_query_after_run: settings.clear_query_after_run,
            require_confirm_destructive: settings.require_confirm_destructive,
            query_autocomplete: settings.query_autocomplete,
            net_refresh: settings.net_refresh,
            net_unit: settings.net_unit,
            screenshot_dir: settings.screenshot_dir.clone().unwrap_or_default(),
            screenshot_save_file: settings.screenshot_save_file,
            screenshot_auto_save: settings.screenshot_auto_save,
            screenshot_use_editor: settings.screenshot_use_editor,
            reduce_dashboard_work_when_unfocused: settings.reduce_dashboard_work_when_unfocused,
            show_dashboard_diagnostics: settings.show_dashboard_diagnostics,
            dashboard_enabled: settings.dashboard.enabled,
            dashboard_path: settings
                .dashboard
                .config_path
                .clone()
                .unwrap_or_else(|| "dashboard.json".into()),
            dashboard_default_location: settings
                .dashboard
                .default_location
                .clone()
                .unwrap_or_default(),
            dashboard_show_when_empty: settings.dashboard.show_when_query_empty,
            plugin_settings: settings.plugin_settings.clone(),
            plugins_expanded: false,
            expand_request: None,
        };
        s.plugin_settings
            .entry("screenshot".into())
            .or_insert_with(|| {
                serde_json::json!({
                    "screenshot_dir": s.screenshot_dir.clone(),
                    "screenshot_save_file": s.screenshot_save_file,
                    "screenshot_auto_save": s.screenshot_auto_save,
                    "screenshot_use_editor": s.screenshot_use_editor,
                })
            });
        s
    }

    pub(crate) fn sync_from_plugin_settings(&mut self) {
        if let Some(val) = self.plugin_settings.get("clipboard") {
            if let Ok(cfg) = serde_json::from_value::<
                crate::plugins::clipboard::ClipboardPluginSettings,
            >(val.clone())
            {
                self.clipboard_limit = cfg.max_entries;
            }
        }
        if let Some(val) = self.plugin_settings.get("network") {
            if let Ok(cfg) = serde_json::from_value::<crate::plugins::network::NetworkPluginSettings>(
                val.clone(),
            ) {
                self.net_refresh = cfg.refresh_rate;
                self.net_unit = cfg.unit;
            }
        }
        if let Some(val) = self.plugin_settings.get("history") {
            if let Ok(cfg) = serde_json::from_value::<crate::plugins::history::HistoryPluginSettings>(
                val.clone(),
            ) {
                self.history_limit = cfg.max_entries;
            }
        }
        if let Some(val) = self.plugin_settings.get("screenshot") {
            if let Ok(cfg) = serde_json::from_value::<ScreenshotPluginSettings>(val.clone()) {
                self.screenshot_dir = cfg.screenshot_dir;
                self.screenshot_save_file = cfg.screenshot_save_file;
                self.screenshot_auto_save = cfg.screenshot_auto_save;
                self.screenshot_use_editor = cfg.screenshot_use_editor;
            }
        }
    }

    pub fn get_plugin_setting_value(&self, name: &str) -> Option<&serde_json::Value> {
        self.plugin_settings.get(name)
    }

    pub fn set_plugin_setting_value(&mut self, name: &str, value: serde_json::Value) {
        self.plugin_settings.insert(name.to_string(), value);
        self.sync_from_plugin_settings();
    }

    pub fn to_settings(&self, current: &Settings) -> Settings {
        let (static_location_enabled, static_pos, static_size) = Self::normalized_static_settings(
            self.follow_mouse,
            self.static_enabled,
            (self.static_x, self.static_y),
            (self.static_w, self.static_h),
        );
        Settings {
            hotkey: if self.hotkey.trim().is_empty() {
                None
            } else {
                Some(self.hotkey.clone())
            },
            quit_hotkey: if !self.quit_hotkey_enabled || self.quit_hotkey.trim().is_empty() {
                None
            } else {
                Some(self.quit_hotkey.clone())
            },
            help_hotkey: if !self.help_hotkey_enabled || self.help_hotkey.trim().is_empty() {
                None
            } else {
                Some(self.help_hotkey.clone())
            },
            index_paths: current.index_paths.clone(),
            max_indexed_items: current.max_indexed_items,
            plugin_dirs: current.plugin_dirs.clone(),
            enabled_plugins: current.enabled_plugins.clone(),
            enabled_capabilities: current.enabled_capabilities.clone(),
            debug_logging: self.debug_logging,
            log_file: current.log_file.clone(),
            enable_toasts: self.show_toasts,
            show_inline_errors: self.show_inline_errors,
            show_error_toasts: self.show_error_toasts,
            toast_duration: self.toast_duration,
            offscreen_pos: Some((self.offscreen_x, self.offscreen_y)),
            window_size: Some((self.window_w, self.window_h)),
            note_panel_default_size: (self.note_panel_w, self.note_panel_h),
            note_save_on_close: self.note_save_on_close,
            note_always_overwrite: self.note_always_overwrite,
            note_images_as_links: self.note_images_as_links,
            note_show_details: self.note_show_details,
            note_more_limit: self.note_more_limit,
            query_scale: Some(self.query_scale),
            list_scale: Some(self.list_scale),
            history_limit: self.history_limit,
            clipboard_limit: self.clipboard_limit,
            fuzzy_weight: self.fuzzy_weight,
            usage_weight: self.usage_weight,
            match_exact: self.match_exact,
            page_jump: self.page_jump,
            query_results_layout: QueryResultsLayoutSettings {
                enabled: self.query_results_layout_enabled,
                rows: self.query_results_layout_rows.max(1),
                cols: self.query_results_layout_cols.max(1),
                respect_plugin_capability: self.query_results_layout_respect_plugin_capability,
                plugin_opt_out: self
                    .query_results_layout_plugin_opt_out
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
                    .collect(),
            },
            follow_mouse: self.follow_mouse,
            static_location_enabled,
            static_pos,
            static_size,
            hide_after_run: self.hide_after_run,
            always_on_top: self.always_on_top,
            timer_refresh: self.timer_refresh,
            disable_timer_updates: self.disable_timer_updates,
            preserve_command: self.preserve_command,
            clear_query_after_run: self.clear_query_after_run,
            require_confirm_destructive: self.require_confirm_destructive,
            query_autocomplete: self.query_autocomplete,
            net_refresh: self.net_refresh,
            net_unit: self.net_unit,
            screenshot_dir: if self.screenshot_dir.trim().is_empty() {
                None
            } else {
                Some(self.screenshot_dir.clone())
            },
            screenshot_save_file: self.screenshot_save_file,
            screenshot_auto_save: self.screenshot_auto_save,
            screenshot_use_editor: self.screenshot_use_editor,
            plugin_settings: self.plugin_settings.clone(),
            show_examples: current.show_examples,
            pinned_panels: current.pinned_panels.clone(),
            reduce_dashboard_work_when_unfocused: self.reduce_dashboard_work_when_unfocused,
            show_dashboard_diagnostics: self.show_dashboard_diagnostics,
            dashboard: crate::settings::DashboardSettings {
                enabled: self.dashboard_enabled,
                config_path: if self.dashboard_path.trim().is_empty() {
                    None
                } else {
                    Some(self.dashboard_path.clone())
                },
                default_location: if self.dashboard_default_location.trim().is_empty() {
                    None
                } else {
                    Some(self.dashboard_default_location.clone())
                },
                show_when_query_empty: self.dashboard_show_when_empty,
            },
            theme: current.theme.clone(),
            note_graph: current.note_graph.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SettingsEditor;
    use crate::plugins::note::NotePluginSettings;
    use crate::plugins::screenshot::ScreenshotPluginSettings;
    use crate::settings::Settings;

    #[test]
    fn query_results_layout_round_trip_editor_conversion() {
        let mut initial = Settings::default();
        initial.query_results_layout.enabled = true;
        initial.query_results_layout.rows = 6;
        initial.query_results_layout.cols = 4;
        initial.query_results_layout.respect_plugin_capability = false;
        initial.query_results_layout.plugin_opt_out = vec!["note".into(), "todo".into()];

        let editor = SettingsEditor::new(&initial);
        let saved = editor.to_settings(&initial);
        assert_eq!(saved.query_results_layout, initial.query_results_layout);
    }

    #[test]
    fn query_results_layout_clamps_rows_and_cols_to_one() {
        let initial = Settings::default();
        let mut editor = SettingsEditor::new(&initial);
        editor.query_results_layout_enabled = true;
        editor.query_results_layout_rows = 0;
        editor.query_results_layout_cols = 0;

        let saved = editor.to_settings(&initial);
        assert_eq!(saved.query_results_layout.rows, 1);
        assert_eq!(saved.query_results_layout.cols, 1);
    }

    #[test]
    fn error_visibility_round_trip_editor_conversion() {
        let mut initial = Settings::default();
        initial.show_inline_errors = false;
        initial.show_error_toasts = false;

        let editor = SettingsEditor::new(&initial);
        let saved = editor.to_settings(&initial);
        assert!(!saved.show_inline_errors);
        assert!(!saved.show_error_toasts);
    }

    #[test]
    fn screenshot_plugin_settings_round_trip_syncs_editor_state() {
        let initial = Settings::default();
        let mut editor = SettingsEditor::new(&initial);
        let value = serde_json::to_value(ScreenshotPluginSettings {
            screenshot_dir: "shots".into(),
            screenshot_save_file: false,
            screenshot_auto_save: true,
            screenshot_use_editor: true,
        })
        .unwrap();

        editor.set_plugin_setting_value("screenshot", value.clone());

        assert_eq!(editor.get_plugin_setting_value("screenshot"), Some(&value));
        assert_eq!(editor.screenshot_dir, "shots");
        assert!(!editor.screenshot_save_file);
        assert!(editor.screenshot_auto_save);
        assert!(editor.screenshot_use_editor);
    }

    #[test]
    fn note_plugin_settings_survive_round_trip() {
        let initial = Settings::default();
        let mut editor = SettingsEditor::new(&initial);
        let note_value = serde_json::to_value(NotePluginSettings::default()).unwrap();
        editor.set_plugin_setting_value("note", note_value.clone());

        let saved = editor.to_settings(&initial);

        assert_eq!(saved.plugin_settings.get("note"), Some(&note_value));
    }
}
