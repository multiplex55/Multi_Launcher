use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, labeled_form_row, secondary_help,
    semantic_status, with_button_emphasis,
};
use crate::plugins::mouse_gestures::{
    MouseGestureSettings, add_ignore_window_title, apply_runtime_settings,
    collect_visible_window_titles, normalize_ignore_window_titles,
};
use crate::settings::Settings;
use eframe::egui;

struct MouseGestureSettingsUiLayout {
    scroll: egui::containers::scroll_area::ScrollAreaOutput<()>,
    body_clip_rect: egui::Rect,
    reload_button: egui::Response,
    save_button: egui::Response,
    close_button: egui::Response,
}

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
        if !self.open {
            self.needs_reload = true;
        }
        self.open = true;
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

        let value = match serde_json::to_value(&self.settings) {
            Ok(v) => v,
            Err(e) => {
                self.last_error = Some(format!("Failed to serialize settings: {e}"));
                return;
            }
        };

        if let Err(e) = Settings::update(&app.settings_path, |settings| {
            settings
                .plugin_settings
                .insert("mouse_gestures".to_string(), value.clone());
            Ok(())
        }) {
            self.last_error = Some(format!("Failed to save settings: {e}"));
            return;
        }

        // Keep the main Settings dialog in sync (without stomping other edits).
        app.settings_editor
            .set_plugin_setting_value("mouse_gestures", value);

        // Apply runtime settings even if the plugin is disabled.
        apply_runtime_settings(self.settings.clone());

        // If the plugin is currently loaded, ensure it receives the new settings.
        match app.plugins.try_write_plugin("mouse_gestures") {
            Ok(mut plugin) => plugin.apply_settings(
                app.settings_editor
                    .get_plugin_setting_value("mouse_gestures")
                    .unwrap_or(&serde_json::Value::Null),
            ),
            Err("plugin busy") => {
                self.last_error = Some(
                    "Mouse gesture plugin is busy; settings were saved and will apply after reload."
                        .into(),
                );
            }
            Err(_) => {}
        }

        self.dirty = false;
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut crate::gui::LauncherApp) {
        let _ = self.show_ui(ctx, app);
    }

    fn show_ui(
        &mut self,
        ctx: &egui::Context,
        app: &mut crate::gui::LauncherApp,
    ) -> Option<MouseGestureSettingsUiLayout> {
        if !self.open {
            return None;
        }

        if self.needs_reload {
            self.reload(&app.settings_path);
        }

        let mut open = self.open;
        let mut close_requested = false;
        let window = egui::Window::new("Mouse Gesture Settings")
            .open(&mut open)
            .resizable(true)
            .default_size(egui::vec2(520.0, 480.0))
            .min_size(egui::vec2(360.0, 260.0))
            .max_size(egui::vec2(
                (ctx.content_rect().width() - 16.0).max(360.0),
                (ctx.content_rect().height() - 16.0).max(260.0),
            ))
            .show(ctx, |ui| {
                if let Some(err) = self.last_error.as_ref() {
                    semantic_status(ui, StatusTone::Error, "Settings error", Some(err));
                    ui.separator();
                }

                compact_section_heading(
                    ui,
                    "Gesture behavior",
                    Some("Changes apply immediately. Save persists them; Close does not undo them."),
                );
                secondary_help(ui, "Saved summaries show committed settings.");

                let mut body_clip_rect = egui::Rect::NOTHING;
                let body_height = (ui.available_height()
                    - mouse_gesture_settings_footer_reserved_height(ui))
                .max(0.0);
                let scroll = egui::ScrollArea::vertical()
                    .id_salt("mouse_gesture_settings_form")
                    .auto_shrink([false, true])
                    .min_scrolled_height(0.0)
                    .max_height(body_height)
                    .show(ui, |ui| {
                body_clip_rect = ui.clip_rect();
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
                                .range(1.0..=20.0)
                                .speed(0.5),
                        )
                        .changed();
                });

                ui.separator();
                compact_section_heading(ui, "Timing / Performance", None);
                ui.horizontal_wrapped(|ui| {
                    ui.label("Trail refresh interval (ms)");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.settings.trail_interval_ms)
                                .speed(1)
                                .range(4..=250),
                        )
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label("Recognition interval (ms)");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.settings.recognition_interval_ms)
                                .speed(1)
                                .range(4..=500),
                        )
                        .changed();
                });
                ui.small("Lower intervals update more frequently but may use more CPU.");

                ui.separator();

                changed |= ui.checkbox(&mut self.settings.show_hint, "Show hint overlay").changed();
                ui.horizontal_wrapped(|ui| {
                    ui.label("Hint offset (x, y)");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.settings.hint_offset.0)
                                .range(-200.0..=200.0)
                                .speed(1.0),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.settings.hint_offset.1)
                                .range(-200.0..=200.0)
                                .speed(1.0),
                        )
                        .changed();
                });

                ui.horizontal_wrapped(|ui| {
                    ui.label("Wheel cycling");
                    egui::ComboBox::from_id_salt("mg_settings_wheel_cycle_gate")
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

                ui.horizontal_wrapped(|ui| {
                    ui.label("Cancel behavior");
                    egui::ComboBox::from_id_salt("mg_settings_cancel_behavior")
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

                ui.horizontal_wrapped(|ui| {
                    ui.label("No-match behavior");
                    egui::ComboBox::from_id_salt("mg_settings_no_match_behavior")
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
                secondary_help(
                    ui,
                    "Fallback runs when a gesture does not match; default is pass-through right-click.",
                );

                ui.separator();
                compact_section_heading(
                    ui,
                    "Ignore windows (disable gestures)",
                    Some(
                    "Gestures will be ignored when the active window title contains one of these entries.",
                    ),
                );

                let mut remove_index: Option<usize> = None;
                if self.settings.ignore_window_titles.is_empty() {
                    ui.label("No ignored windows.");
                } else {
                    for (index, title) in self.settings.ignore_window_titles.iter().enumerate() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(title);
                            let remove = ui.button("Remove");
                            if with_button_emphasis(ui, remove, ButtonEmphasis::Destructive).clicked() {
                                remove_index = Some(index);
                            }
                        });
                    }
                }
                if let Some(index) = remove_index {
                    self.settings.ignore_window_titles.remove(index);
                    changed = true;
                }

                ui.horizontal_wrapped(|ui| {
                    let response = ui.text_edit_singleline(&mut self.ignore_input);
                    let add_button = ui.button("Add");
                    let mut add_now = with_button_emphasis(
                        ui,
                        add_button,
                        ButtonEmphasis::Primary,
                    )
                    .clicked();
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
                                semantic_status(
                                    ui,
                                    StatusTone::Error,
                                    "Window picker error",
                                    Some(err),
                                );
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
                                                if ui.button("Add").clicked()
                                                    && add_ignore_window_title(
                                                        &mut self.settings.ignore_window_titles,
                                                        &title,
                                                    ) {
                                                        changed = true;
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

                secondary_help(
                    ui,
                    "Tip: You can also open this window by running `mg settings` in the launcher.",
                );

                if changed {
                    self.dirty = true;
                    // Live-apply while editing.
                    apply_runtime_settings(self.settings.clone());
                }
                    });

                ui.separator();

                let (reload_button, save_button, close_button) = ui.horizontal(|ui| {
                    let reload_button = ui.button("Reload");
                    if reload_button.clicked() {
                        self.needs_reload = true;
                    }

                    let save_button = ui.add_enabled(self.dirty, egui::Button::new("Save"));
                    if with_button_emphasis(ui, save_button.clone(), ButtonEmphasis::Primary)
                        .clicked()
                    {
                        self.save(app);
                    }
                    let close_button = ui.button("Close");
                    if close_button.clicked() {
                        close_requested = true;
                    }
                    (reload_button, save_button, close_button)
                }).inner;
                MouseGestureSettingsUiLayout {
                    scroll,
                    body_clip_rect,
                    reload_button,
                    save_button,
                    close_button,
                }
            });

        let pinned = app
            .pinned_panels
            .contains(&crate::gui::Panel::MouseGestureSettingsDialog);
        self.open = (open && !close_requested) || pinned;
        window.and_then(|window| window.inner)
    }
}

fn mouse_gesture_settings_footer_reserved_height(ui: &mut egui::Ui) -> f32 {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let button_font = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| egui::TextStyle::Body.resolve(&style));
    let button_frame = style
        .button_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Inactive,
        )
        .frame;
    let button_height = (ui.fonts_mut(|fonts| fonts.row_height(&button_font))
        + spacing.extra_text_line_spacing
        + button_frame.total_margin().sum().y)
        .max(spacing.interact_size.y);
    let separator_height = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    button_height + 2.0 * spacing.item_spacing.y + separator_height
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::{LauncherApp, Panel};
    use crate::plugin::PluginManager;
    use std::sync::{Arc, atomic::AtomicBool};

    fn test_app(ctx: &egui::Context, root: &std::path::Path) -> LauncherApp {
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test(),
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            Settings::default(),
            None,
            None,
            Some(std::collections::HashSet::new()),
            Some(std::collections::HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn render_frame(
        ctx: &egui::Context,
        dialog: &mut MouseGestureSettingsDialog,
        app: &mut LauncherApp,
        screen_rect: egui::Rect,
        time: f64,
        events: Vec<egui::Event>,
    ) -> Option<MouseGestureSettingsUiLayout> {
        let mut layout = None;
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen_rect),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| layout = dialog.show_ui(ctx, app),
        );
        output.drop_without_applying_deltas();
        layout
    }

    fn click_close(
        ctx: &egui::Context,
        dialog: &mut MouseGestureSettingsDialog,
        app: &mut LauncherApp,
        screen_rect: egui::Rect,
        point: egui::Pos2,
        time: f64,
    ) -> Option<MouseGestureSettingsUiLayout> {
        let _ = render_frame(
            ctx,
            dialog,
            app,
            screen_rect,
            time,
            vec![egui::Event::PointerMoved(point)],
        );
        let _ = render_frame(
            ctx,
            dialog,
            app,
            screen_rect,
            time + 0.01,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        render_frame(
            ctx,
            dialog,
            app,
            screen_rect,
            time + 0.02,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        )
    }

    #[test]
    fn constrained_settings_scroll_keeps_footer_reachable_and_close_preserves_live_settings() {
        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx, directory.path());
        let mut dialog = MouseGestureSettingsDialog::default();
        dialog.open();
        dialog.needs_reload = false;
        dialog.settings.ignore_window_titles = (0..18)
            .map(|index| format!("Synthetic window title {index} with a long description"))
            .collect();
        normalize_ignore_window_titles(&mut dialog.settings.ignore_window_titles);
        dialog.dirty = true;

        let settings_before = dialog.settings.clone();
        let dirty_before = dialog.dirty;
        let editor_value_before = app
            .settings_editor
            .get_plugin_setting_value("mouse_gestures")
            .cloned();

        // Pinned panels call open() on every application update. Reopening an
        // already-open settings window must retain its live draft.
        dialog.open();
        assert!(!dialog.needs_reload);
        assert_eq!(dialog.settings, settings_before);
        assert_eq!(dialog.dirty, dirty_before);

        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 360.0));
        let mut layout = None;
        for frame in 0..3 {
            layout = render_frame(
                &ctx,
                &mut dialog,
                &mut app,
                screen_rect,
                frame as f64 * 0.016,
                Vec::new(),
            );
        }
        let layout = layout.expect("the open settings window should render");
        let scroll_id = layout.scroll.id;
        assert!(layout.scroll.inner_rect.is_positive());
        assert!(layout.scroll.content_size.y > layout.scroll.inner_rect.height());
        assert!(layout.body_clip_rect.intersects(layout.scroll.inner_rect));
        let max_scroll_y =
            (layout.scroll.content_size.y - layout.scroll.inner_rect.height()).max(0.0);
        assert!(layout.scroll.state.offset.y.is_finite());
        assert!(layout.scroll.state.offset.y >= 0.0);
        assert!(layout.scroll.state.offset.y <= max_scroll_y + 0.5);
        assert!(layout.reload_button.enabled());
        assert!(layout.save_button.enabled());
        assert!(layout.close_button.enabled());
        assert!(screen_rect.contains_rect(layout.reload_button.rect));
        assert!(screen_rect.contains_rect(layout.save_button.rect));
        assert!(screen_rect.contains_rect(layout.close_button.rect));
        assert!(layout.close_button.rect.top() >= layout.scroll.inner_rect.bottom() - 1.0);

        app.pinned_panels.push(Panel::MouseGestureSettingsDialog);
        let pinned_close = click_close(
            &ctx,
            &mut dialog,
            &mut app,
            screen_rect,
            layout.close_button.rect.center(),
            0.1,
        )
        .expect("pinned Close attempt should leave the window rendered");
        assert!(pinned_close.close_button.clicked());
        assert!(dialog.open, "a pinned window refuses its close request");
        assert!(!dialog.needs_reload);
        assert_eq!(pinned_close.scroll.id, scroll_id);

        app.pinned_panels
            .retain(|panel| *panel != Panel::MouseGestureSettingsDialog);
        let closed = click_close(
            &ctx,
            &mut dialog,
            &mut app,
            screen_rect,
            pinned_close.close_button.rect.center(),
            0.2,
        )
        .expect("the close action should be reported on its release frame");
        assert!(closed.close_button.clicked());
        assert!(!dialog.open, "an unpinned Close request closes the window");
        assert_eq!(dialog.settings, settings_before);
        assert_eq!(dialog.dirty, dirty_before);
        assert!(!dialog.needs_reload);
        assert_eq!(
            app.settings_editor
                .get_plugin_setting_value("mouse_gestures")
                .cloned(),
            editor_value_before
        );
    }
}
