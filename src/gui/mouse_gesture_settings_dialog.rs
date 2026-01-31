use crate::plugins::mouse_gestures::{apply_runtime_settings, MouseGestureSettings};
use crate::settings::Settings;
use eframe::egui;

/// Standalone settings window for mouse gestures.
///
/// This edits the `mouse_gestures` entry inside `Settings::plugin_settings` and
/// applies changes live.
#[derive(Default)]
pub struct MouseGestureSettingsDialog {
    pub open: bool,
    needs_reload: bool,
    dirty: bool,
    settings: MouseGestureSettings,
    last_error: Option<String>,
}

impl MouseGestureSettingsDialog {
    pub fn open(&mut self) {
        self.open = true;
        self.needs_reload = true;
    }

    fn reload(&mut self, settings_path: &str) {
        self.last_error = None;
        match Settings::load(settings_path) {
            Ok(s) => {
                let cfg = s
                    .plugin_settings
                    .get("mouse_gestures")
                    .and_then(|v| serde_json::from_value::<MouseGestureSettings>(v.clone()).ok())
                    .unwrap_or_default();
                self.settings = cfg;
                self.dirty = false;
                self.needs_reload = false;
            }
            Err(e) => {
                self.last_error = Some(format!("Failed to load settings: {e}"));
                self.settings = MouseGestureSettings::default();
                self.dirty = false;
                self.needs_reload = false;
            }
        }
    }

    fn save(&mut self, app: &mut crate::gui::LauncherApp) {
        self.last_error = None;

        let mut settings = match Settings::load(&app.settings_path) {
            Ok(s) => s,
            Err(e) => {
                self.last_error = Some(format!("Failed to load settings: {e}"));
                return;
            }
        };

        let value = match serde_json::to_value(&self.settings) {
            Ok(v) => v,
            Err(e) => {
                self.last_error = Some(format!("Failed to serialize settings: {e}"));
                return;
            }
        };

        settings
            .plugin_settings
            .insert("mouse_gestures".to_string(), value.clone());

        if let Err(e) = settings.save(&app.settings_path) {
            self.last_error = Some(format!("Failed to save settings: {e}"));
            return;
        }

        // Keep the main Settings dialog in sync (without stomping other edits).
        app.settings_editor
            .set_plugin_setting_value("mouse_gestures", value);

        // Apply runtime settings even if the plugin is disabled.
        apply_runtime_settings(self.settings.clone());

        // If the plugin is currently loaded, ensure it receives the new settings.
        for plugin in app.plugins.iter_mut() {
            if plugin.name() == "mouse_gestures" {
                plugin.apply_settings(
                    app.settings_editor
                        .get_plugin_setting_value("mouse_gestures")
                        .unwrap_or(&serde_json::Value::Null),
                );
                break;
            }
        }

        self.dirty = false;
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut crate::gui::LauncherApp) {
        if !self.open {
            return;
        }

        if self.needs_reload {
            self.reload(&app.settings_path);
        }

        let mut open = self.open;
        egui::Window::new("Mouse Gesture Settings")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                if let Some(err) = self.last_error.as_ref() {
                    ui.colored_label(egui::Color32::RED, err);
                    ui.separator();
                }

                ui.label("Configure how mouse gestures are captured and displayed.");
                ui.add_space(6.0);

                let mut changed = false;
                changed |= ui.checkbox(&mut self.settings.enabled, "Enable mouse gestures").changed();
                changed |= ui
                    .checkbox(
                        &mut self.settings.require_button,
                        "Require trigger button to be held",
                    )
                    .changed();

                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Min distance between points (px)");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.settings.min_distance_px)
                                .clamp_range(1.0..=50.0)
                                .speed(0.5),
                        )
                        .changed();
                });

                ui.horizontal(|ui| {
                    ui.label("Max gesture duration (ms)");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.settings.max_duration_ms)
                                .clamp_range(100..=60_000)
                                .speed(50.0),
                        )
                        .changed();
                });

                ui.separator();

                changed |= ui.checkbox(&mut self.settings.show_trail, "Show trail overlay").changed();
                ui.horizontal(|ui| {
                    ui.label("Trail color");
                    let mut color = egui::Color32::from_rgba_unmultiplied(
                        self.settings.trail_color[0],
                        self.settings.trail_color[1],
                        self.settings.trail_color[2],
                        self.settings.trail_color[3],
                    );
                    let resp = ui.color_edit_button_srgba(&mut color);
                    if resp.changed() {
                        self.settings.trail_color = [color.r(), color.g(), color.b(), color.a()];
                        changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Trail width");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.settings.trail_width)
                                .clamp_range(1.0..=20.0)
                                .speed(0.5),
                        )
                        .changed();
                });

                ui.separator();

                changed |= ui.checkbox(&mut self.settings.show_hint, "Show hint overlay").changed();
                ui.horizontal(|ui| {
                    ui.label("Hint offset (x, y)");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.settings.hint_offset.0)
                                .clamp_range(-200.0..=200.0)
                                .speed(1.0),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.settings.hint_offset.1)
                                .clamp_range(-200.0..=200.0)
                                .speed(1.0),
                        )
                        .changed();
                });

                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Cancel behavior");
                    egui::ComboBox::from_id_source("mg_settings_cancel_behavior")
                        .selected_text(cancel_behavior_label(self.settings.cancel_behavior))
                        .show_ui(ui, |ui| {
                            changed |= ui
                                .selectable_value(
                                    &mut self.settings.cancel_behavior,
                                    crate::mouse_gestures::service::CancelBehavior::DoNothing,
                                    "Do nothing",
                                )
                                .changed();
                            changed |= ui
                                .selectable_value(
                                    &mut self.settings.cancel_behavior,
                                    crate::mouse_gestures::service::CancelBehavior::PassThroughClick,
                                    "Pass through right-click",
                                )
                                .changed();
                        });
                });

                ui.horizontal(|ui| {
                    ui.label("No-match behavior");
                    egui::ComboBox::from_id_source("mg_settings_no_match_behavior")
                        .selected_text(no_match_behavior_label(self.settings.no_match_behavior))
                        .show_ui(ui, |ui| {
                            changed |= ui
                                .selectable_value(
                                    &mut self.settings.no_match_behavior,
                                    crate::mouse_gestures::service::NoMatchBehavior::DoNothing,
                                    "Do nothing",
                                )
                                .changed();
                            changed |= ui
                                .selectable_value(
                                    &mut self.settings.no_match_behavior,
                                    crate::mouse_gestures::service::NoMatchBehavior::PassThroughClick,
                                    "Pass through right-click",
                                )
                                .changed();
                            changed |= ui
                                .selectable_value(
                                    &mut self.settings.no_match_behavior,
                                    crate::mouse_gestures::service::NoMatchBehavior::ShowNoMatchHint,
                                    "Show no-match hint",
                                )
                                .changed();
                        });
                });

                if changed {
                    self.dirty = true;
                    // Live-apply while editing.
                    apply_runtime_settings(self.settings.clone());
                }

                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Reload").clicked() {
                        self.needs_reload = true;
                    }
                    if ui
                        .add_enabled(self.dirty, egui::Button::new("Save"))
                        .clicked()
                    {
                        self.save(app);
                    }
                    if ui.button("Close").clicked() {
                        self.open = false;
                    }
                });

                ui.add_space(4.0);
                ui.small("Tip: You can also open this window by running `mg settings` in the launcher.");
            });

        self.open = open;
    }
}

fn cancel_behavior_label(
    value: crate::mouse_gestures::service::CancelBehavior,
) -> &'static str {
    match value {
        crate::mouse_gestures::service::CancelBehavior::DoNothing => "Do nothing",
        crate::mouse_gestures::service::CancelBehavior::PassThroughClick => {
            "Pass through right-click"
        }
    }
}

fn no_match_behavior_label(
    value: crate::mouse_gestures::service::NoMatchBehavior,
) -> &'static str {
    match value {
        crate::mouse_gestures::service::NoMatchBehavior::DoNothing => "Do nothing",
        crate::mouse_gestures::service::NoMatchBehavior::PassThroughClick => {
            "Pass through right-click"
        }
        crate::mouse_gestures::service::NoMatchBehavior::ShowNoMatchHint => "Show no-match hint",
    }
}
