use crate::mouse_gestures::mouse_gesture_service;
use crate::plugins::mouse_gestures::settings::{
    MouseGestureOverlaySettings, MouseGesturePluginSettings,
};
use crate::settings::Settings;
use eframe::egui;
use serde_json::Value;

#[derive(Default)]
pub struct MouseGesturesSettingsDialog {
    pub open: bool,
    loaded: bool,
    settings: MouseGesturePluginSettings,
}

impl MouseGesturesSettingsDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.loaded = false;
    }

    fn load_settings(app: &crate::gui::LauncherApp) -> MouseGesturePluginSettings {
        Settings::load(&app.settings_path)
            .ok()
            .and_then(|settings| {
                settings
                    .plugin_settings
                    .get("mouse_gestures")
                    .and_then(|val| {
                        serde_json::from_value::<MouseGesturePluginSettings>(val.clone()).ok()
                    })
            })
            .unwrap_or_default()
    }

    fn persist_settings(app: &mut crate::gui::LauncherApp, settings: &MouseGesturePluginSettings) {
        if let Ok(mut cfg) = Settings::load(&app.settings_path) {
            cfg.plugin_settings.insert(
                "mouse_gestures".to_string(),
                serde_json::to_value(settings).unwrap_or(Value::Null),
            );
            if let Err(e) = cfg.save(&app.settings_path) {
                app.set_error(format!("Failed to save mouse gesture settings: {e}"));
            }
        } else {
            app.set_error("Failed to load settings for mouse gestures".into());
        }
    }

    fn apply_settings(&self, app: &mut crate::gui::LauncherApp) {
        mouse_gesture_service().update_settings(self.settings.clone());
        Self::persist_settings(app, &self.settings);
    }

    fn overlay_preview_color(value: &str) -> Option<egui::Color32> {
        let raw = value.trim().trim_start_matches('#');
        if raw.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&raw[0..2], 16).ok()?;
        let g = u8::from_str_radix(&raw[2..4], 16).ok()?;
        let b = u8::from_str_radix(&raw[4..6], 16).ok()?;
        Some(egui::Color32::from_rgb(r, g, b))
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut crate::gui::LauncherApp) {
        if !self.open {
            return;
        }
        if !self.loaded {
            self.settings = Self::load_settings(app);
            mouse_gesture_service().update_settings(self.settings.clone());
            self.loaded = true;
        }
        let mut open = self.open;
        egui::Window::new("Mouse Gestures Settings")
            .open(&mut open)
            .show(ctx, |ui| {
                let mut changed = false;
                changed |= ui
                    .checkbox(&mut self.settings.enabled, "Enable mouse gestures")
                    .changed();
                ui.horizontal(|ui| {
                    ui.label("Trigger button");
                    let previous = self.settings.trigger_button.clone();
                    let trigger = previous.to_ascii_lowercase();
                    let mut trigger_label = match trigger.as_str() {
                        "left" => "Left",
                        "middle" => "Middle",
                        _ => "Right",
                    };
                    egui::ComboBox::from_id_source("mg_trigger_button")
                        .selected_text(trigger_label)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(trigger_label == "Left", "Left")
                                .clicked()
                            {
                                self.settings.trigger_button = "left".to_string();
                                trigger_label = "Left";
                            }
                            if ui
                                .selectable_label(trigger_label == "Right", "Right")
                                .clicked()
                            {
                                self.settings.trigger_button = "right".to_string();
                                trigger_label = "Right";
                            }
                            if ui
                                .selectable_label(trigger_label == "Middle", "Middle")
                                .clicked()
                            {
                                self.settings.trigger_button = "middle".to_string();
                                trigger_label = "Middle";
                            }
                        });
                    changed |= self.settings.trigger_button != previous;
                });
                ui.horizontal(|ui| {
                    ui.label("Min track length");
                    changed |= ui
                        .add(egui::DragValue::new(&mut self.settings.min_track_len).speed(1.0))
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label("Max distance");
                    changed |= ui
                        .add(egui::DragValue::new(&mut self.settings.max_distance).speed(0.5))
                        .changed();
                });
                changed |= ui
                    .checkbox(
                        &mut self.settings.passthrough_on_no_match,
                        "Pass through clicks when no gesture matches",
                    )
                    .changed();
                changed |= ui
                    .checkbox(&mut self.settings.sampling_enabled, "Enable sampling")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.settings.smoothing_enabled, "Enable smoothing")
                    .changed();

                ui.separator();
                ui.label("Overlay");
                overlay_settings_ui(ui, &mut self.settings.overlay, &mut changed);

                if changed {
                    self.apply_settings(app);
                }
            });
        self.open = open;
    }
}

fn overlay_settings_ui(
    ui: &mut egui::Ui,
    overlay: &mut MouseGestureOverlaySettings,
    changed: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label("Color");
        let response = ui.text_edit_singleline(&mut overlay.color);
        *changed |= response.changed();
        if let Some(color) = MouseGesturesSettingsDialog::overlay_preview_color(&overlay.color) {
            ui.colored_label(color, "■■");
        }
    });
    ui.horizontal(|ui| {
        ui.label("Thickness");
        *changed |= ui
            .add(egui::DragValue::new(&mut overlay.thickness).speed(0.1))
            .changed();
    });
    ui.horizontal(|ui| {
        ui.label("Fade (ms)");
        *changed |= ui
            .add(egui::DragValue::new(&mut overlay.fade).speed(10.0))
            .changed();
    });
}
