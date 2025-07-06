use crate::settings::Settings;
use crate::gui::LauncherApp;
use eframe::egui;
use rfd::FileDialog;

#[derive(Default)]
pub struct SettingsEditor {
    hotkey: String,
    quit_hotkey: String,
    index_paths: Vec<String>,
    index_input: String,
    debug_logging: bool,
    offscreen_x: i32,
    offscreen_y: i32,
    window_w: i32,
    window_h: i32,
}

impl SettingsEditor {
    pub fn new(settings: &Settings) -> Self {
        Self {
            hotkey: settings.hotkey.clone().unwrap_or_default(),
            quit_hotkey: settings.quit_hotkey.clone().unwrap_or_default(),
            index_paths: settings.index_paths.clone().unwrap_or_default(),
            index_input: String::new(),
            debug_logging: settings.debug_logging,
            offscreen_x: settings.offscreen_pos.unwrap_or((2000, 2000)).0,
            offscreen_y: settings.offscreen_pos.unwrap_or((2000, 2000)).1,
            window_w: settings.window_size.unwrap_or((400, 220)).0,
            window_h: settings.window_size.unwrap_or((400, 220)).1,
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
            offscreen_pos: Some((self.offscreen_x, self.offscreen_y)),
            window_size: Some((self.window_w, self.window_h)),
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

            ui.horizontal(|ui| {
                ui.label("Off-screen X");
                ui.add(egui::DragValue::new(&mut self.offscreen_x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut self.offscreen_y));
            });

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
                            );
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
