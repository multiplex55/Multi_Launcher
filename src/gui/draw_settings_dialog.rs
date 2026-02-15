use crate::draw::service::runtime;
use crate::draw::settings::DrawSettings;
use crate::draw::settings_store;
use crate::draw::settings_ui::render_draw_settings_form;
use crate::settings::Settings;
use eframe::egui;

#[derive(Default)]
pub struct DrawSettingsDialog {
    pub open: bool,
    settings: DrawSettings,
    dirty: bool,
    last_error: Option<String>,
    hotkey_validation_error: Option<String>,
}

impl DrawSettingsDialog {
    pub fn open(&mut self, settings_path: &str) {
        self.open = true;
        self.reload(settings_path);
    }

    fn reload(&mut self, settings_path: &str) {
        self.last_error = None;
        self.hotkey_validation_error = None;
        match settings_store::load(settings_path) {
            Ok(settings) => {
                self.settings = settings;
                self.dirty = false;
            }
            Err(e) => {
                self.settings = DrawSettings::default();
                self.dirty = false;
                self.last_error = Some(format!("Failed to load settings: {e}"));
            }
        }
    }

    fn persist(&mut self, app: &mut crate::gui::LauncherApp) {
        self.last_error = None;
        self.settings.sanitize_for_first_pass_transparency();

        if !self.settings.toolbar_hotkey_valid() {
            self.hotkey_validation_error =
                Some("Invalid hotkey format (example: Ctrl+Shift+D).".to_string());
            self.last_error =
                Some("Cannot save draw settings until toolbar hotkey is valid.".to_string());
            return;
        }
        self.hotkey_validation_error = None;

        if let Err(e) = settings_store::save(&self.settings) {
            self.last_error = Some(format!("Failed to save draw settings: {e}"));
            return;
        }

        let mut settings = match Settings::load(&app.settings_path) {
            Ok(settings) => settings,
            Err(e) => {
                self.last_error = Some(format!("Failed to load settings: {e}"));
                return;
            }
        };

        let value = match serde_json::to_value(&self.settings) {
            Ok(value) => value,
            Err(e) => {
                self.last_error = Some(format!("Failed to serialize draw settings: {e}"));
                return;
            }
        };

        settings
            .plugin_settings
            .insert("draw".to_string(), value.clone());

        if let Err(e) = settings.save(&app.settings_path) {
            self.last_error = Some(format!("Failed to save settings: {e}"));
            return;
        }

        app.settings_editor
            .set_plugin_setting_value("draw", value.clone());
        runtime().apply_settings(self.settings.clone());

        for plugin in app.plugins.iter_mut() {
            if plugin.name() == "draw" {
                plugin.apply_settings(&value);
                break;
            }
        }

        self.dirty = false;
    }

    fn reset(&mut self, app: &mut crate::gui::LauncherApp) {
        self.settings = DrawSettings::default();
        self.persist(app);
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut crate::gui::LauncherApp) {
        if !self.open {
            return;
        }

        let mut open = self.open;
        egui::Window::new("Draw Settings")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                if let Some(err) = self.last_error.as_ref() {
                    ui.colored_label(egui::Color32::RED, err);
                    ui.separator();
                }

                let form_result = render_draw_settings_form(ui, &mut self.settings, "draw_dialog");
                self.dirty |= form_result.changed;
                self.hotkey_validation_error = form_result
                    .toolbar_hotkey_error
                    .or(form_result.fixed_save_folder_error);

                if let Some(err) = self.hotkey_validation_error.as_ref() {
                    ui.colored_label(egui::Color32::RED, err);
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.dirty && self.hotkey_validation_error.is_none(),
                            egui::Button::new("Save"),
                        )
                        .clicked()
                    {
                        self.persist(app);
                    }

                    if ui.button("Reset").clicked() {
                        self.reset(app);
                    }
                });
            });

        self.open = open;
    }

    #[cfg(test)]
    pub fn settings_for_test(&self) -> &DrawSettings {
        &self.settings
    }

    #[cfg(test)]
    pub fn set_settings_for_test(&mut self, settings: DrawSettings) {
        self.settings = settings;
        self.dirty = true;
    }

    #[cfg(test)]
    pub fn save_for_test(&mut self, app: &mut crate::gui::LauncherApp) {
        self.persist(app);
    }

    #[cfg(test)]
    pub fn reset_for_test(&mut self, app: &mut crate::gui::LauncherApp) {
        self.reset(app);
    }
}

#[cfg(test)]
impl DrawSettingsDialog {
    pub fn hotkey_validation_error_for_test(&self) -> Option<&str> {
        self.hotkey_validation_error.as_deref()
    }
}
