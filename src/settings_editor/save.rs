use super::state::SettingsEditor;
use crate::dashboard::config::DashboardConfig;
use crate::gui::{LauncherApp, UiErrorEvent};
use crate::plugins::note::NotePluginSettings;
use crate::settings::Settings;
use eframe::egui;
use egui_toast::{Toast, ToastKind, ToastOptions};
use std::sync::Arc;

impl SettingsEditor {
    pub(crate) fn save_settings(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if let Err(message) = self.validate_before_save() {
            app.add_error_toast(message);
            return;
        }

        self.sync_from_plugin_settings();
        match Settings::load(&app.settings_path) {
            Ok(current) => {
                let mut new_settings = self.to_settings(&current);
                app.merge_file_search_ui_preferences_into_settings(&mut new_settings);
                if let Err(e) = new_settings.save(&app.settings_path) {
                    app.report_ui_error(UiErrorEvent::new(
                        "settings_editor.save",
                        format!("Failed to save: {e}"),
                    ));
                } else {
                    self.apply_saved_settings(ctx, app, new_settings);
                }
            }
            Err(e) => {
                let msg = format!("Failed to read settings: {e}");
                app.report_error_message("settings_editor.read", msg);
            }
        }
    }

    fn apply_saved_settings(
        &self,
        ctx: &egui::Context,
        app: &mut LauncherApp,
        new_settings: Settings,
    ) {
        app.update_paths(
            new_settings.plugin_dirs.clone(),
            new_settings.index_paths.clone(),
            new_settings.enabled_plugins.clone(),
            new_settings.enabled_capabilities.clone(),
            new_settings.offscreen_pos,
            Some(new_settings.enable_toasts),
            Some(new_settings.show_inline_errors),
            Some(new_settings.show_error_toasts),
            Some(new_settings.toast_duration),
            Some(new_settings.fuzzy_weight),
            Some(new_settings.usage_weight),
            Some(new_settings.match_exact),
            Some(new_settings.follow_mouse),
            Some(new_settings.static_location_enabled),
            new_settings.static_pos,
            new_settings.static_size,
            Some(new_settings.hide_after_run),
            Some(new_settings.clear_query_after_run),
            Some(new_settings.require_confirm_destructive),
            Some(new_settings.timer_refresh),
            Some(new_settings.disable_timer_updates),
            Some(new_settings.preserve_command),
            Some(new_settings.query_autocomplete),
            Some(new_settings.net_refresh),
            Some(new_settings.net_unit),
            new_settings.screenshot_dir.clone(),
            Some(new_settings.screenshot_save_file),
            Some(new_settings.screenshot_use_editor),
            Some(new_settings.screenshot_auto_save),
            Some(new_settings.always_on_top),
            Some(new_settings.page_jump),
            Some(new_settings.note.clone()),
            Some(new_settings.note_panel_default_size),
            Some(new_settings.note_save_on_close),
            Some(new_settings.note_always_overwrite),
            Some(new_settings.note_images_as_links),
            Some(new_settings.note_show_details),
            Some(new_settings.note_more_limit),
            Some(new_settings.show_dashboard_diagnostics),
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            if new_settings.always_on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            },
        ));
        app.hotkey_str = new_settings.hotkey.clone();
        app.quit_hotkey_str = new_settings.quit_hotkey.clone();
        app.help_hotkey_str = new_settings.help_hotkey.clone();
        app.query_scale = new_settings.query_scale.unwrap_or(1.0).min(5.0);
        app.list_scale = new_settings.list_scale.unwrap_or(1.0).min(5.0);
        app.history_limit = new_settings.history_limit;
        app.clipboard_limit = new_settings.clipboard_limit;
        app.page_jump = new_settings.page_jump;
        app.note_show_details = new_settings.note_show_details;
        app.preserve_command = new_settings.preserve_command;
        app.clear_query_after_run = new_settings.clear_query_after_run;
        app.require_confirm_destructive = new_settings.require_confirm_destructive;
        app.query_autocomplete = new_settings.query_autocomplete;
        app.query_results_layout = new_settings.query_results_layout.clone();
        app.recompute_query_results_layout();
        app.net_refresh = new_settings.net_refresh;
        app.net_unit = new_settings.net_unit;
        app.screenshot_dir = new_settings.screenshot_dir.clone();
        app.screenshot_save_file = new_settings.screenshot_save_file;
        app.screenshot_auto_save = new_settings.screenshot_auto_save;
        app.screenshot_use_editor = new_settings.screenshot_use_editor;
        app.reduce_dashboard_work_when_unfocused =
            new_settings.reduce_dashboard_work_when_unfocused;
        app.show_dashboard_diagnostics = new_settings.show_dashboard_diagnostics;
        app.dashboard_enabled = new_settings.dashboard.enabled;
        app.dashboard_show_when_empty = new_settings.dashboard.show_when_query_empty;
        app.dashboard_default_location = new_settings.dashboard.default_location.clone();
        app.dashboard_path = DashboardConfig::path_for(
            new_settings
                .dashboard
                .config_path
                .as_deref()
                .unwrap_or("dashboard.json"),
        )
        .to_string_lossy()
        .to_string();
        app.dashboard.set_path(&app.dashboard_path);
        app.toast_duration = new_settings.toast_duration;
        app.note_more_limit = new_settings.note_more_limit;
        let dirs = new_settings.plugin_dirs.clone().unwrap_or_default();
        let actions_arc = Arc::clone(&app.actions);
        let mut plugin_settings = new_settings.plugin_settings.clone();
        plugin_settings.insert(
            "note".into(),
            crate::plugins::note::note_plugin_settings_with_backlinks(
                plugin_settings.get("note"),
                new_settings.note.backlinks_enabled,
                new_settings.note.aliases_enabled,
                new_settings.note.templates_enabled,
            ),
        );
        app.plugins.reload_from_dirs(
            &dirs,
            app.clipboard_limit,
            app.net_unit,
            false,
            &plugin_settings,
            actions_arc,
        );
        if let Some(val) = new_settings.plugin_settings.get("file_search")
            && let Ok(cfg) = serde_json::from_value::<
                crate::file_search::settings::FileSearchSettings,
            >(val.clone())
        {
            app.apply_file_search_settings(cfg);
        }
        if let Some(val) = new_settings.plugin_settings.get("note")
            && let Ok(cfg) = serde_json::from_value::<NotePluginSettings>(val.clone())
        {
            app.note_external_open = cfg.external_open;
        }
        crate::request_hotkey_restart(new_settings);
        if app.enable_toasts {
            app.add_toast(Toast {
                text: "Settings saved".into(),
                kind: ToastKind::Success,
                options: ToastOptions::default().duration_in_seconds(app.toast_duration as f64),
            });
        }
    }
}
