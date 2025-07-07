use crate::settings::Settings;
use crate::gui::LauncherApp;
use eframe::egui;
#[cfg(target_os = "windows")]
use rfd::FileDialog;

#[derive(Default)]
pub struct SettingsEditor {
    hotkey: String,
    quit_hotkey: String,
    index_paths: Vec<String>,
    index_input: String,
    debug_logging: bool,
    show_toasts: bool,
    offscreen_x: i32,
    offscreen_y: i32,
    window_w: i32,
    window_h: i32,
    query_scale: f32,
    list_scale: f32,
    history_limit: usize,
    fuzzy_weight: f32,
    usage_weight: f32,
    follow_mouse: bool,
    static_enabled: bool,
    static_x: i32,
    static_y: i32,
    static_w: i32,
    static_h: i32,
}

impl SettingsEditor {
    pub fn new(settings: &Settings) -> Self {
        Self {
            hotkey: settings.hotkey.clone().unwrap_or_default(),
            quit_hotkey: settings.quit_hotkey.clone().unwrap_or_default(),
            index_paths: settings.index_paths.clone().unwrap_or_default(),
            index_input: String::new(),
            debug_logging: settings.debug_logging,
            show_toasts: settings.enable_toasts,
            offscreen_x: settings.offscreen_pos.unwrap_or((2000, 2000)).0,
            offscreen_y: settings.offscreen_pos.unwrap_or((2000, 2000)).1,
            window_w: settings.window_size.unwrap_or((400, 220)).0,
            window_h: settings.window_size.unwrap_or((400, 220)).1,
            query_scale: settings.query_scale.unwrap_or(1.0),
            list_scale: settings.list_scale.unwrap_or(1.0),
            history_limit: settings.history_limit,
            fuzzy_weight: settings.fuzzy_weight,
            usage_weight: settings.usage_weight,
            follow_mouse: settings.follow_mouse,
            static_enabled: settings.static_location_enabled,
            static_x: settings.static_pos.unwrap_or((0, 0)).0,
            static_y: settings.static_pos.unwrap_or((0, 0)).1,
            static_w: settings.static_size.unwrap_or((400, 220)).0,
            static_h: settings.static_size.unwrap_or((400, 220)).1,
        }
    }

    fn to_settings(&self, current: &Settings) -> Settings {
        Settings {
            hotkey: if self.hotkey.trim().is_empty() {
                None
            } else {
                Some(self.hotkey.clone())
            },
            quit_hotkey: if self.quit_hotkey.trim().is_empty() {
                None
            } else {
                Some(self.quit_hotkey.clone())
            },
            index_paths: if self.index_paths.is_empty() {
                None
            } else {
                Some(self.index_paths.clone())
            },
            plugin_dirs: current.plugin_dirs.clone(),
            enabled_plugins: current.enabled_plugins.clone(),
            enabled_capabilities: current.enabled_capabilities.clone(),
            debug_logging: self.debug_logging,
            enable_toasts: self.show_toasts,
            offscreen_pos: Some((self.offscreen_x, self.offscreen_y)),
            window_size: Some((self.window_w, self.window_h)),
            query_scale: Some(self.query_scale),
            list_scale: Some(self.list_scale),
            history_limit: self.history_limit,
            fuzzy_weight: self.fuzzy_weight,
            usage_weight: self.usage_weight,
            follow_mouse: self.follow_mouse,
            static_location_enabled: self.static_enabled,
            static_pos: Some((self.static_x, self.static_y)),
            static_size: Some((self.static_w, self.static_h)),
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        let mut open = app.show_settings;
        egui::Window::new("Settings")
            .open(&mut open)
            .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Launcher hotkey");
                ui.text_edit_singleline(&mut self.hotkey);
            });
            ui.horizontal(|ui| {
                ui.label("Quit hotkey");
                ui.text_edit_singleline(&mut self.quit_hotkey);
            });

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
                ui.label("History limit");
                ui.add(egui::Slider::new(&mut self.history_limit, 10..=500).text(""));
            });

            ui.horizontal(|ui| {
                ui.label("Off-screen X");
                ui.add(egui::DragValue::new(&mut self.offscreen_x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut self.offscreen_y));
            });

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
            ui.label("Index paths:");
            let mut remove: Option<usize> = None;
            for (idx, path) in self.index_paths.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(path);
                    if ui.button("Remove").clicked() {
                        remove = Some(idx);
                    }
                });
            }
            if let Some(i) = remove {
                self.index_paths.remove(i);
            }
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.index_input);
                if ui.button("Browse").clicked() {
                    #[cfg(target_os = "windows")]
                    if let Some(dir) = FileDialog::new().pick_folder() {
                        self.index_input = dir.display().to_string();
                    }
                }
                if ui.button("Add").clicked() {
                    if !self.index_input.is_empty() {
                        self.index_paths.push(self.index_input.clone());
                        self.index_input.clear();
                    }
                }
            });

            if ui.button("Save").clicked() {
                match Settings::load(&app.settings_path) {
                    Ok(current) => {
                        let new_settings = self.to_settings(&current);
                        if let Err(e) = new_settings.save(&app.settings_path) {
                            app.error = Some(format!("Failed to save: {e}"));
                        } else {
                            app.update_paths(
                                new_settings.plugin_dirs.clone(),
                                new_settings.index_paths.clone(),
                                new_settings.enabled_plugins.clone(),
                                new_settings.enabled_capabilities.clone(),
                                new_settings.offscreen_pos,
                                Some(new_settings.enable_toasts),
                                Some(new_settings.fuzzy_weight),
                                Some(new_settings.usage_weight),
                                Some(new_settings.follow_mouse),
                                Some(new_settings.static_location_enabled),
                                new_settings.static_pos,
                                new_settings.static_size,
                            );
                            app.query_scale = new_settings.query_scale.unwrap_or(1.0).min(5.0);
                            app.list_scale = new_settings.list_scale.unwrap_or(1.0).min(5.0);
                            app.history_limit = new_settings.history_limit;
                            crate::request_hotkey_restart(new_settings);
                        }
                    }
                    Err(e) => app.error = Some(format!("Failed to read settings: {e}")),
                }
            }
        });
        app.show_settings = open;
    }
}
