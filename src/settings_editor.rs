use crate::settings::Settings;
use crate::gui::LauncherApp;
use eframe::egui;
use rfd::FileDialog;

#[derive(Default)]
pub struct SettingsEditor {
    hotkey: String,
    quit_hotkey: String,
    index_paths: Vec<String>,
    plugin_dirs: Vec<String>,
    index_input: String,
    plugin_input: String,
    debug_logging: bool,
    hidden_x: String,
    hidden_y: String,
}

impl SettingsEditor {
    pub fn new(settings: &Settings) -> Self {
        Self {
            hotkey: settings.hotkey.clone().unwrap_or_default(),
            quit_hotkey: settings.quit_hotkey.clone().unwrap_or_default(),
            index_paths: settings.index_paths.clone().unwrap_or_default(),
            plugin_dirs: settings.plugin_dirs.clone().unwrap_or_default(),
            index_input: String::new(),
            plugin_input: String::new(),
            debug_logging: settings.debug_logging,
            hidden_x: settings.hidden_x.to_string(),
            hidden_y: settings.hidden_y.to_string(),
        }
    }

    fn to_settings(&self) -> Settings {
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
            plugin_dirs: if self.plugin_dirs.is_empty() {
                None
            } else {
                Some(self.plugin_dirs.clone())
            },
            debug_logging: self.debug_logging,
            hidden_x: self.hidden_x.parse().unwrap_or(2000.0),
            hidden_y: self.hidden_y.parse().unwrap_or(2000.0),
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
                ui.label("Hidden X");
                ui.text_edit_singleline(&mut self.hidden_x);
            });
            ui.horizontal(|ui| {
                ui.label("Hidden Y");
                ui.text_edit_singleline(&mut self.hidden_y);
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

            ui.separator();
            ui.label("Plugin directories:");
            let mut remove_p: Option<usize> = None;
            for (idx, path) in self.plugin_dirs.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(path);
                    if ui.button("Remove").clicked() {
                        remove_p = Some(idx);
                    }
                });
            }
            if let Some(i) = remove_p {
                self.plugin_dirs.remove(i);
            }
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.plugin_input);
                if ui.button("Browse").clicked() {
                    if let Some(dir) = FileDialog::new().pick_folder() {
                        self.plugin_input = dir.display().to_string();
                    }
                }
                if ui.button("Add").clicked() {
                    if !self.plugin_input.is_empty() {
                        self.plugin_dirs.push(self.plugin_input.clone());
                        self.plugin_input.clear();
                    }
                }
            });

            if ui.button("Save").clicked() {
                let new_settings = self.to_settings();
                if let Err(e) = new_settings.save(&app.settings_path) {
                    app.error = Some(format!("Failed to save: {e}"));
                } else {
                    app.update_paths(
                        new_settings.plugin_dirs.clone(),
                        new_settings.index_paths.clone(),
                    );
                    app.hidden_position = new_settings.hidden_position();
                    crate::request_hotkey_restart(new_settings);
                }
            }
        });
        app.show_settings = open;
    }
}
