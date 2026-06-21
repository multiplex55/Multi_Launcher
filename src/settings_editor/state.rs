use crate::hotkey::parse_hotkey;
use crate::settings::Settings;
use eframe::egui;

#[derive(Default)]
pub struct SettingsEditor {
    pub(crate) hotkey: String,
    pub(crate) hotkey_valid: bool,
    pub(crate) last_valid_hotkey: String,
    pub(crate) quit_hotkey_enabled: bool,
    pub(crate) quit_hotkey: String,
    pub(crate) quit_hotkey_valid: bool,
    pub(crate) last_valid_quit_hotkey: String,
    pub(crate) help_hotkey_enabled: bool,
    pub(crate) help_hotkey: String,
    pub(crate) help_hotkey_valid: bool,
    pub(crate) last_valid_help_hotkey: String,
    pub(crate) debug_logging: bool,
    pub(crate) show_toasts: bool,
    pub(crate) show_inline_errors: bool,
    pub(crate) show_error_toasts: bool,
    pub(crate) toast_duration: f32,
    pub(crate) offscreen_x: i32,
    pub(crate) offscreen_y: i32,
    pub(crate) window_w: i32,
    pub(crate) window_h: i32,
    pub(crate) note_panel_w: f32,
    pub(crate) note_panel_h: f32,
    pub(crate) note_save_on_close: bool,
    pub(crate) note_always_overwrite: bool,
    pub(crate) note_images_as_links: bool,
    pub(crate) note_show_details: bool,
    pub(crate) note_more_limit: usize,
    pub(crate) note_rich_markdown_enabled: bool,
    pub(crate) note_task_lists_enabled: bool,
    pub(crate) note_interactive_checkboxes_enabled: bool,
    pub(crate) note_collapsible_sections_enabled: bool,
    pub(crate) note_outline_sidebar_enabled: bool,
    pub(crate) note_outline_sidebar_default_open: bool,
    pub(crate) note_split_view_enabled: bool,
    pub(crate) note_default_view_mode: crate::settings::NoteViewMode,
    pub(crate) note_callouts_enabled: bool,
    pub(crate) note_backlinks_enabled: bool,
    pub(crate) note_aliases_enabled: bool,
    pub(crate) note_templates_enabled: bool,
    pub(crate) note_max_outline_depth: usize,
    pub(crate) note_collapsed_sections_persist: bool,
    pub(crate) query_scale: f32,
    pub(crate) list_scale: f32,
    pub(crate) history_limit: usize,
    pub(crate) clipboard_limit: usize,
    pub(crate) fuzzy_weight: f32,
    pub(crate) usage_weight: f32,
    pub(crate) match_exact: bool,
    pub(crate) page_jump: usize,
    pub(crate) query_results_layout_enabled: bool,
    pub(crate) query_results_layout_rows: usize,
    pub(crate) query_results_layout_cols: usize,
    pub(crate) query_results_layout_respect_plugin_capability: bool,
    pub(crate) query_results_layout_plugin_opt_out: String,
    pub(crate) follow_mouse: bool,
    pub(crate) static_enabled: bool,
    pub(crate) static_x: i32,
    pub(crate) static_y: i32,
    pub(crate) static_w: i32,
    pub(crate) static_h: i32,
    pub(crate) hide_after_run: bool,
    pub always_on_top: bool,
    pub(crate) timer_refresh: f32,
    pub(crate) disable_timer_updates: bool,
    pub(crate) preserve_command: bool,
    pub(crate) clear_query_after_run: bool,
    pub(crate) require_confirm_destructive: bool,
    pub(crate) query_autocomplete: bool,
    pub(crate) net_refresh: f32,
    pub(crate) net_unit: crate::settings::NetUnit,
    pub(crate) screenshot_dir: String,
    pub(crate) screenshot_save_file: bool,
    pub(crate) screenshot_auto_save: bool,
    pub(crate) screenshot_use_editor: bool,
    pub(crate) reduce_dashboard_work_when_unfocused: bool,
    pub(crate) show_dashboard_diagnostics: bool,
    pub(crate) dashboard_enabled: bool,
    pub(crate) dashboard_path: String,
    pub(crate) dashboard_default_location: String,
    pub(crate) dashboard_show_when_empty: bool,
    pub(crate) plugin_settings: std::collections::HashMap<String, serde_json::Value>,
    pub(crate) plugins_expanded: bool,
    pub(crate) expand_request: Option<bool>,
}

impl SettingsEditor {
    pub(crate) const SETTINGS_WINDOW_DEFAULT_WIDTH: f32 = 640.0;
    pub(crate) const SETTINGS_WINDOW_MAX_DEFAULT_HEIGHT: f32 = 720.0;
    pub(crate) const SETTINGS_WINDOW_MIN_HEIGHT: f32 = 360.0;
    pub(crate) const SETTINGS_CONTENT_MIN_HEIGHT: f32 = 180.0;
    pub(crate) const SETTINGS_FOOTER_RESERVED_HEIGHT: f32 = 56.0;

    pub(crate) fn normalized_static_settings(
        follow_mouse: bool,
        static_enabled: bool,
        static_pos: (i32, i32),
        static_size: (i32, i32),
    ) -> (bool, Option<(i32, i32)>, Option<(i32, i32)>) {
        if follow_mouse {
            (false, None, None)
        } else {
            (static_enabled, Some(static_pos), Some(static_size))
        }
    }

    pub fn new(settings: &Settings) -> Self {
        Self::from_settings(settings)
    }

    pub fn new_with_plugins(settings: &Settings) -> Self {
        let mut s = Self::new(settings);
        s.sync_from_plugin_settings();
        s
    }

    pub(crate) fn settings_window_default_height(available_height: f32) -> f32 {
        (available_height * 0.5)
            .max(Self::SETTINGS_WINDOW_MIN_HEIGHT)
            .min(Self::SETTINGS_WINDOW_MAX_DEFAULT_HEIGHT)
    }

    pub(crate) fn settings_window_default_size(ctx: &egui::Context) -> [f32; 2] {
        [
            Self::SETTINGS_WINDOW_DEFAULT_WIDTH,
            Self::settings_window_default_height(ctx.available_rect().height()),
        ]
    }

    pub(crate) fn settings_content_height(available_height: f32) -> f32 {
        (available_height - Self::SETTINGS_FOOTER_RESERVED_HEIGHT)
            .max(Self::SETTINGS_CONTENT_MIN_HEIGHT)
    }

    pub(crate) fn validate_before_save(&mut self) -> Result<(), &'static str> {
        if parse_hotkey(&self.hotkey).is_none() {
            self.hotkey = self.last_valid_hotkey.clone();
            self.hotkey_valid = true;
            return Err("Failed to save settings: hotkey is invalid");
        }
        if self.quit_hotkey_enabled && parse_hotkey(&self.quit_hotkey).is_none() {
            self.quit_hotkey = self.last_valid_quit_hotkey.clone();
            self.quit_hotkey_valid = true;
            return Err("Failed to save settings: quit hotkey is invalid");
        }
        if self.help_hotkey_enabled && parse_hotkey(&self.help_hotkey).is_none() {
            self.help_hotkey = self.last_valid_help_hotkey.clone();
            self.help_hotkey_valid = true;
            return Err("Failed to save settings: help hotkey is invalid");
        }

        self.last_valid_hotkey = self.hotkey.clone();
        if self.quit_hotkey_enabled {
            self.last_valid_quit_hotkey = self.quit_hotkey.clone();
        }
        if self.help_hotkey_enabled {
            self.last_valid_help_hotkey = self.help_hotkey.clone();
        }
        Ok(())
    }
}
