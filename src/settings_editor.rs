use crate::gui::LauncherApp;
use crate::hotkey::parse_hotkey;
use crate::settings::Settings;
use eframe::egui;
use egui_toast::{Toast, ToastKind, ToastOptions};
#[cfg(target_os = "windows")]
use rfd::FileDialog;
use std::sync::Arc;

#[derive(Default)]
pub struct SettingsEditor {
    hotkey: String,
    hotkey_valid: bool,
    last_valid_hotkey: String,
    quit_hotkey_enabled: bool,
    quit_hotkey: String,
    quit_hotkey_valid: bool,
    last_valid_quit_hotkey: String,
    help_hotkey_enabled: bool,
    help_hotkey: String,
    help_hotkey_valid: bool,
    last_valid_help_hotkey: String,
    debug_logging: bool,
    show_toasts: bool,
    toast_duration: f32,
    offscreen_x: i32,
    offscreen_y: i32,
    window_w: i32,
    window_h: i32,
    note_panel_w: f32,
    note_panel_h: f32,
    note_save_on_close: bool,
    query_scale: f32,
    list_scale: f32,
    history_limit: usize,
    clipboard_limit: usize,
    fuzzy_weight: f32,
    usage_weight: f32,
    page_jump: usize,
    follow_mouse: bool,
    static_enabled: bool,
    static_x: i32,
    static_y: i32,
    static_w: i32,
    static_h: i32,
    hide_after_run: bool,
    pub always_on_top: bool,
    timer_refresh: f32,
    disable_timer_updates: bool,
    preserve_command: bool,
    net_refresh: f32,
    net_unit: crate::settings::NetUnit,
    screenshot_dir: String,
    screenshot_save_file: bool,
    plugin_settings: std::collections::HashMap<String, serde_json::Value>,
    plugins_expanded: bool,
    expand_request: Option<bool>,
}

impl SettingsEditor {
    pub fn new(settings: &Settings) -> Self {
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
        Self {
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
            toast_duration: settings.toast_duration,
            offscreen_x: settings.offscreen_pos.unwrap_or((2000, 2000)).0,
            offscreen_y: settings.offscreen_pos.unwrap_or((2000, 2000)).1,
            window_w: settings.window_size.unwrap_or((400, 220)).0,
            window_h: settings.window_size.unwrap_or((400, 220)).1,
            note_panel_w: settings.note_panel_default_size.0,
            note_panel_h: settings.note_panel_default_size.1,
            note_save_on_close: settings.note_save_on_close,
            query_scale: settings.query_scale.unwrap_or(1.0),
            list_scale: settings.list_scale.unwrap_or(1.0),
            history_limit: settings.history_limit,
            clipboard_limit: settings.clipboard_limit,
            fuzzy_weight: settings.fuzzy_weight,
            usage_weight: settings.usage_weight,
            page_jump: settings.page_jump,
            follow_mouse: settings.follow_mouse,
            static_enabled: settings.static_location_enabled,
            static_x: settings.static_pos.unwrap_or((0, 0)).0,
            static_y: settings.static_pos.unwrap_or((0, 0)).1,
            static_w: settings.static_size.unwrap_or((400, 220)).0,
            static_h: settings.static_size.unwrap_or((400, 220)).1,
            hide_after_run: settings.hide_after_run,
            always_on_top: settings.always_on_top,
            timer_refresh: settings.timer_refresh,
            disable_timer_updates: settings.disable_timer_updates,
            preserve_command: settings.preserve_command,
            net_refresh: settings.net_refresh,
            net_unit: settings.net_unit,
            screenshot_dir: settings.screenshot_dir.clone().unwrap_or_default(),
            screenshot_save_file: settings.screenshot_save_file,
            plugin_settings: settings.plugin_settings.clone(),
            plugins_expanded: false,
            expand_request: None,
        }
    }

    pub fn new_with_plugins(settings: &Settings) -> Self {
        let mut s = Self::new(settings);
        s.sync_from_plugin_settings();
        s
    }

    fn sync_from_plugin_settings(&mut self) {
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
    }

    pub fn to_settings(&self, current: &Settings) -> Settings {
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
            plugin_dirs: current.plugin_dirs.clone(),
            enabled_plugins: current.enabled_plugins.clone(),
            enabled_capabilities: current.enabled_capabilities.clone(),
            debug_logging: self.debug_logging,
            log_file: current.log_file.clone(),
            enable_toasts: self.show_toasts,
            toast_duration: self.toast_duration,
            offscreen_pos: Some((self.offscreen_x, self.offscreen_y)),
            window_size: Some((self.window_w, self.window_h)),
            note_panel_default_size: (self.note_panel_w, self.note_panel_h),
            note_save_on_close: self.note_save_on_close,
            query_scale: Some(self.query_scale),
            list_scale: Some(self.list_scale),
            history_limit: self.history_limit,
            clipboard_limit: self.clipboard_limit,
            fuzzy_weight: self.fuzzy_weight,
            usage_weight: self.usage_weight,
            page_jump: self.page_jump,
            follow_mouse: self.follow_mouse,
            static_location_enabled: self.static_enabled,
            static_pos: Some((self.static_x, self.static_y)),
            static_size: Some((self.static_w, self.static_h)),
            hide_after_run: self.hide_after_run,
            always_on_top: self.always_on_top,
            timer_refresh: self.timer_refresh,
            disable_timer_updates: self.disable_timer_updates,
            preserve_command: self.preserve_command,
            net_refresh: self.net_refresh,
            net_unit: self.net_unit,
            screenshot_dir: if self.screenshot_dir.trim().is_empty() {
                None
            } else {
                Some(self.screenshot_dir.clone())
            },
            screenshot_save_file: self.screenshot_save_file,
            plugin_settings: self.plugin_settings.clone(),
            show_examples: current.show_examples,
            pinned_panels: current.pinned_panels.clone(),
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        let mut open = app.show_settings;
        egui::Window::new("Settings")
            .open(&mut open)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Launcher hotkey");
                            let resp = ui.text_edit_singleline(&mut self.hotkey);
                            if resp.changed() {
                                self.hotkey_valid = parse_hotkey(&self.hotkey).is_some();
                                if self.hotkey_valid {
                                    self.last_valid_hotkey = self.hotkey.clone();
                                }
                            }
                            let color = if self.hotkey_valid {
                                egui::Color32::GREEN
                            } else {
                                egui::Color32::RED
                            };
                            ui.add(egui::Label::new(egui::RichText::new("●").color(color)));
                        });
                        ui.checkbox(&mut self.quit_hotkey_enabled, "Enable quit hotkey");
                        if self.quit_hotkey_enabled {
                            ui.horizontal(|ui| {
                                ui.label("Quit hotkey");
                                let resp = ui.text_edit_singleline(&mut self.quit_hotkey);
                                if resp.changed() {
                                    self.quit_hotkey_valid =
                                        parse_hotkey(&self.quit_hotkey).is_some();
                                    if self.quit_hotkey_valid {
                                        self.last_valid_quit_hotkey = self.quit_hotkey.clone();
                                    }
                                }
                                let color = if self.quit_hotkey_valid {
                                    egui::Color32::GREEN
                                } else {
                                    egui::Color32::RED
                                };
                                ui.add(egui::Label::new(egui::RichText::new("●").color(color)));
                            });
                        }

                        ui.checkbox(&mut self.help_hotkey_enabled, "Enable help hotkey");
                        if self.help_hotkey_enabled {
                            ui.horizontal(|ui| {
                                ui.label("Help hotkey");
                                let resp = ui.text_edit_singleline(&mut self.help_hotkey);
                                if resp.changed() {
                                    self.help_hotkey_valid =
                                        parse_hotkey(&self.help_hotkey).is_some();
                                    if self.help_hotkey_valid {
                                        self.last_valid_help_hotkey = self.help_hotkey.clone();
                                    }
                                }
                                let color = if self.help_hotkey_valid {
                                    egui::Color32::GREEN
                                } else {
                                    egui::Color32::RED
                                };
                                ui.add(egui::Label::new(egui::RichText::new("●").color(color)));
                            });
                        }

                        ui.horizontal(|ui| {
                            egui::ComboBox::from_label("Debug logging")
                                .selected_text(if self.debug_logging {
                                    "Enabled"
                                } else {
                                    "Disabled"
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.debug_logging, false, "Disabled");
                                    ui.selectable_value(&mut self.debug_logging, true, "Enabled");
                                });
                        });

                        ui.checkbox(&mut self.show_toasts, "Enable toast notifications");
                        if self.show_toasts {
                            ui.horizontal(|ui| {
                                ui.label("Toast duration (s)");
                                ui.add(
                                    egui::Slider::new(&mut self.toast_duration, 0.1..=5.0).text(""),
                                );
                            });
                        }
                        ui.checkbox(&mut self.hide_after_run, "Hide window after running action");
                        ui.checkbox(&mut self.always_on_top, "Always on top");
                        ui.checkbox(&mut self.preserve_command, "Preserve command after run");
                        ui.checkbox(
                            &mut self.disable_timer_updates,
                            "Disable timer auto refresh",
                        );
                        ui.horizontal(|ui| {
                            ui.label("Timer refresh rate (s)");
                            ui.add_enabled_ui(!self.disable_timer_updates, |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut self.timer_refresh)
                                        .clamp_range(0.1..=60.0)
                                        .speed(0.1),
                                );
                            });
                        });

                        ui.horizontal(|ui| {
                            ui.label("Query scale");
                            ui.add(egui::Slider::new(&mut self.query_scale, 0.5..=5.0).text(""));
                        });
                        ui.horizontal(|ui| {
                            ui.label("List scale");
                            ui.add(egui::Slider::new(&mut self.list_scale, 0.5..=5.0).text(""));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Fuzzy weight");
                            ui.add(egui::Slider::new(&mut self.fuzzy_weight, 0.0..=5.0).text(""));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Usage weight");
                            ui.add(egui::Slider::new(&mut self.usage_weight, 0.0..=5.0).text(""));
                        });

                        ui.horizontal(|ui| {
                            ui.label("Page jump");
                            ui.add(
                                egui::DragValue::new(&mut self.page_jump)
                                    .clamp_range(1..=100)
                                    .speed(1),
                            );
                        });

                        ui.horizontal(|ui| {
                            ui.label("Off-screen X");
                            ui.add(egui::DragValue::new(&mut self.offscreen_x));
                            ui.label("Y");
                            ui.add(egui::DragValue::new(&mut self.offscreen_y));
                        });

                        ui.horizontal(|ui| {
                            ui.label("Note panel W");
                            ui.add(egui::DragValue::new(&mut self.note_panel_w));
                            ui.label("H");
                            ui.add(egui::DragValue::new(&mut self.note_panel_h));
                        });

                        ui.checkbox(&mut self.note_save_on_close, "Save note on close (Esc)");

                        ui.checkbox(&mut self.follow_mouse, "Follow mouse");
                        ui.add_enabled_ui(!self.follow_mouse, |ui| {
                            ui.checkbox(&mut self.static_enabled, "Use static position");
                        });
                        if self.static_enabled {
                            ui.horizontal(|ui| {
                                ui.label("X");
                                ui.add(egui::DragValue::new(&mut self.static_x));
                                ui.label("Y");
                                ui.add(egui::DragValue::new(&mut self.static_y));
                                ui.label("W");
                                ui.add(egui::DragValue::new(&mut self.static_w));
                                ui.label("H");
                                ui.add(egui::DragValue::new(&mut self.static_h));
                                if ui.button("Snapshot").clicked() {
                                    self.static_x = app.window_pos.0;
                                    self.static_y = app.window_pos.1;
                                    self.static_w = app.window_size.0;
                                    self.static_h = app.window_size.1;
                                }
                            });
                        }

                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Screenshot dir");
                            ui.text_edit_singleline(&mut self.screenshot_dir);
                            if ui.button("Browse").clicked() {
                                #[cfg(target_os = "windows")]
                                if let Some(dir) = FileDialog::new().pick_folder() {
                                    self.screenshot_dir = dir.display().to_string();
                                }
                            }
                        });
                        ui.checkbox(
                            &mut self.screenshot_save_file,
                            "Save file when copying screenshot",
                        );

                        ui.separator();
                        if ui
                            .button(if self.plugins_expanded {
                                "Collapse plugin sections"
                            } else {
                                "Expand plugin sections"
                            })
                            .clicked()
                        {
                            self.plugins_expanded = !self.plugins_expanded;
                            self.expand_request = Some(self.plugins_expanded);
                        }
                        let enabled_list = app.enabled_plugins_list();
                        for plugin in app.plugins.iter_mut() {
                            let name = plugin.name().to_string();
                            let enabled = match &enabled_list {
                                Some(list) => list.contains(&name),
                                None => true,
                            };
                            if !enabled {
                                continue;
                            }

                            let has_settings = plugin.default_settings().is_some()
                                || self.plugin_settings.contains_key(&name);
                            if !has_settings {
                                continue;
                            }

                            let entry =
                                self.plugin_settings.entry(name.clone()).or_insert_with(|| {
                                    plugin.default_settings().unwrap_or(serde_json::Value::Null)
                                });
                            let id = ui.make_persistent_id(format!("plugin_{name}"));
                            let mut state =
                                egui::collapsing_header::CollapsingState::load_with_default_open(
                                    ui.ctx(),
                                    id,
                                    false,
                                );
                            if let Some(open) = self.expand_request {
                                state.set_open(open);
                            }
                            state
                                .show_header(ui, |ui| {
                                    ui.label(format!("{name} settings"));
                                })
                                .body(|ui| {
                                    plugin.settings_ui(ui, entry);
                                });
                        }
                        self.expand_request = None;

                        if ui.button("Save").clicked() {
                            if parse_hotkey(&self.hotkey).is_none() {
                                self.hotkey = self.last_valid_hotkey.clone();
                                self.hotkey_valid = true;
                                if app.enable_toasts {
                                    app.add_toast(Toast {
                                        text: "Failed to save settings: hotkey is invalid".into(),
                                        kind: ToastKind::Error,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(app.toast_duration as f64),
                                    });
                                }
                            } else if self.quit_hotkey_enabled
                                && parse_hotkey(&self.quit_hotkey).is_none()
                            {
                                self.quit_hotkey = self.last_valid_quit_hotkey.clone();
                                self.quit_hotkey_valid = true;
                                if app.enable_toasts {
                                    app.add_toast(Toast {
                                        text: "Failed to save settings: quit hotkey is invalid"
                                            .into(),
                                        kind: ToastKind::Error,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(app.toast_duration as f64),
                                    });
                                }
                            } else if self.help_hotkey_enabled
                                && parse_hotkey(&self.help_hotkey).is_none()
                            {
                                self.help_hotkey = self.last_valid_help_hotkey.clone();
                                self.help_hotkey_valid = true;
                                if app.enable_toasts {
                                    app.add_toast(Toast {
                                        text: "Failed to save settings: help hotkey is invalid"
                                            .into(),
                                        kind: ToastKind::Error,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(app.toast_duration as f64),
                                    });
                                }
                            } else {
                                self.last_valid_hotkey = self.hotkey.clone();
                                if self.quit_hotkey_enabled {
                                    self.last_valid_quit_hotkey = self.quit_hotkey.clone();
                                }
                                if self.help_hotkey_enabled {
                                    self.last_valid_help_hotkey = self.help_hotkey.clone();
                                }
                                self.sync_from_plugin_settings();
                                match Settings::load(&app.settings_path) {
                                    Ok(current) => {
                                        let new_settings = self.to_settings(&current);
                                        if let Err(e) = new_settings.save(&app.settings_path) {
                                            app.set_error(format!("Failed to save: {e}"));
                                        } else {
                                            app.update_paths(
                                                new_settings.plugin_dirs.clone(),
                                                new_settings.index_paths.clone(),
                                                new_settings.enabled_plugins.clone(),
                                                new_settings.enabled_capabilities.clone(),
                                                new_settings.offscreen_pos,
                                                Some(new_settings.enable_toasts),
                                                Some(new_settings.toast_duration),
                                                Some(new_settings.fuzzy_weight),
                                                Some(new_settings.usage_weight),
                                                Some(new_settings.follow_mouse),
                                                Some(new_settings.static_location_enabled),
                                                new_settings.static_pos,
                                                new_settings.static_size,
                                                Some(new_settings.hide_after_run),
                                                Some(new_settings.timer_refresh),
                                                Some(new_settings.disable_timer_updates),
                                                Some(new_settings.preserve_command),
                                                Some(new_settings.net_refresh),
                                                Some(new_settings.net_unit),
                                                new_settings.screenshot_dir.clone(),
                                                Some(new_settings.screenshot_save_file),
                                                Some(new_settings.always_on_top),
                                                Some(new_settings.page_jump),
                                                Some(new_settings.note_panel_default_size),
                                                Some(new_settings.note_save_on_close),
                                            );
                                            ctx.send_viewport_cmd(
                                                egui::ViewportCommand::WindowLevel(
                                                    if new_settings.always_on_top {
                                                        egui::WindowLevel::AlwaysOnTop
                                                    } else {
                                                        egui::WindowLevel::Normal
                                                    },
                                                ),
                                            );
                                            app.hotkey_str = new_settings.hotkey.clone();
                                            app.quit_hotkey_str = new_settings.quit_hotkey.clone();
                                            app.help_hotkey_str = new_settings.help_hotkey.clone();
                                            app.query_scale =
                                                new_settings.query_scale.unwrap_or(1.0).min(5.0);
                                            app.list_scale =
                                                new_settings.list_scale.unwrap_or(1.0).min(5.0);
                                            app.history_limit = new_settings.history_limit;
                                            app.clipboard_limit = new_settings.clipboard_limit;
                                            app.page_jump = new_settings.page_jump;
                                            app.preserve_command = new_settings.preserve_command;
                                            app.net_refresh = new_settings.net_refresh;
                                            app.net_unit = new_settings.net_unit;
                                            app.screenshot_dir =
                                                new_settings.screenshot_dir.clone();
                                            app.screenshot_save_file =
                                                new_settings.screenshot_save_file;
                                            app.toast_duration = new_settings.toast_duration;
                                            let dirs = new_settings
                                                .plugin_dirs
                                                .clone()
                                                .unwrap_or_default();
                                            let actions_arc = Arc::clone(&app.actions);
                                            app.plugins.reload_from_dirs(
                                                &dirs,
                                                app.clipboard_limit,
                                                app.net_unit,
                                                false,
                                                &new_settings.plugin_settings,
                                                actions_arc,
                                            );
                                            crate::request_hotkey_restart(new_settings);
                                            if app.enable_toasts {
                                                app.add_toast(Toast {
                                                    text: "Settings saved".into(),
                                                    kind: ToastKind::Success,
                                                    options: ToastOptions::default()
                                                        .duration_in_seconds(
                                                            app.toast_duration as f64,
                                                        ),
                                                });
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        app.set_error(format!("Failed to read settings: {e}"))
                                    }
                                }
                            }
                        }
                    });
            });
        app.show_settings = open;
    }
}
