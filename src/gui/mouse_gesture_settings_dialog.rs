use crate::plugins::mouse_gestures::{
    add_ignore_window_title, apply_runtime_settings, collect_visible_window_titles,
    normalize_ignore_window_titles, MouseGestureSettings,
};
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
    ignore_input: String,
    window_picker_open: bool,
    window_picker_titles: Vec<String>,
    window_picker_error: Option<String>,
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
                    .checkbox(&mut self.settings.debug_logging, "Enable debug logging")
                    .changed();

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

                ui.horizontal(|ui| {
                    ui.label("Wheel cycling");
                    egui::ComboBox::from_id_source("mg_settings_wheel_cycle_gate")
                        .selected_text(wheel_cycle_gate_label(self.settings.wheel_cycle_gate))
                        .show_ui(ui, |ui| {
                            changed |= ui
                                .selectable_value(
                                    &mut self.settings.wheel_cycle_gate,
                                    crate::mouse_gestures::service::WheelCycleGate::Deadzone,
                                    "After deadzone",
                                )
                                .changed();
                            changed |= ui
                                .selectable_value(
                                    &mut self.settings.wheel_cycle_gate,
                                    crate::mouse_gestures::service::WheelCycleGate::Shift,
                                    "Shift + wheel",
                                )
                                .changed();
                        });
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
                ui.small("Fallback runs when a gesture does not match; default is pass-through right-click.");

                ui.separator();
                ui.heading("Ignore windows (disable gestures)");
                ui.small(
                    "Gestures will be ignored when the active window title contains one of these entries.",
                );

                let mut remove_index: Option<usize> = None;
                if self.settings.ignore_window_titles.is_empty() {
                    ui.label("No ignored windows.");
                } else {
                    for (index, title) in self.settings.ignore_window_titles.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(title);
                            if ui.button("Remove").clicked() {
                                remove_index = Some(index);
                            }
                        });
                    }
                }
                if let Some(index) = remove_index {
                    self.settings.ignore_window_titles.remove(index);
                    changed = true;
                }

                ui.horizontal(|ui| {
                    let response = ui.text_edit_singleline(&mut self.ignore_input);
                    let mut add_now = ui.button("Add").clicked();
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        add_now = true;
                    }
                    if add_now {
                        if add_ignore_window_title(
                            &mut self.settings.ignore_window_titles,
                            &self.ignore_input,
                        ) {
                            changed = true;
                        }
                        self.ignore_input.clear();
                    }
                });

                let window_button = ui.add_enabled(cfg!(windows), egui::Button::new("Select window..."));
                if !cfg!(windows) {
                    window_button.on_hover_text("Window picker is only available on Windows.");
                } else if window_button.clicked() {
                    self.window_picker_error = None;
                    match collect_visible_window_titles() {
                        Ok(titles) => {
                            self.window_picker_titles = titles;
                            self.window_picker_open = true;
                        }
                        Err(err) => {
                            self.window_picker_error = Some(format!(
                                "Failed to enumerate windows: {err}"
                            ));
                            self.window_picker_titles.clear();
                            self.window_picker_open = true;
                        }
                    }
                }

                if self.window_picker_open {
                    let mut open_picker = self.window_picker_open;
                    egui::Window::new("Select window to ignore")
                        .open(&mut open_picker)
                        .resizable(true)
                        .show(ctx, |ui| {
                            if let Some(err) = &self.window_picker_error {
                                ui.colored_label(egui::Color32::RED, err);
                                ui.separator();
                            }

                            if self.window_picker_titles.is_empty() {
                                ui.label("No windows found.");
                            } else {
                                egui::ScrollArea::vertical()
                                    .max_height(220.0)
                                    .show(ui, |ui| {
                                        for title in self.window_picker_titles.clone() {
                                            ui.horizontal(|ui| {
                                                ui.label(&title);
                                                if ui.button("Add").clicked() {
                                                    if add_ignore_window_title(
                                                        &mut self.settings.ignore_window_titles,
                                                        &title,
                                                    ) {
                                                        changed = true;
                                                    }
                                                }
                                            });
                                        }
                                    });
                            }
                        });
                    self.window_picker_open = open_picker;
                }

                if normalize_ignore_window_titles(&mut self.settings.ignore_window_titles) {
                    changed = true;
                }

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

fn cancel_behavior_label(value: crate::mouse_gestures::service::CancelBehavior) -> &'static str {
    match value {
        crate::mouse_gestures::service::CancelBehavior::DoNothing => "Do nothing",
        crate::mouse_gestures::service::CancelBehavior::PassThroughClick => {
            "Pass through right-click"
        }
    }
}

fn no_match_behavior_label(value: crate::mouse_gestures::service::NoMatchBehavior) -> &'static str {
    match value {
        crate::mouse_gestures::service::NoMatchBehavior::DoNothing => "Do nothing",
        crate::mouse_gestures::service::NoMatchBehavior::PassThroughClick => {
            "Pass through right-click"
        }
        crate::mouse_gestures::service::NoMatchBehavior::ShowNoMatchHint => "Show no-match hint",
    }
}

fn wheel_cycle_gate_label(value: crate::mouse_gestures::service::WheelCycleGate) -> &'static str {
    match value {
        crate::mouse_gestures::service::WheelCycleGate::Deadzone => "After deadzone",
        crate::mouse_gestures::service::WheelCycleGate::Shift => "Shift + wheel",
    }
}
