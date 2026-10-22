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

const WINDOW_PICKER_TITLE: &str = "Select window to ignore";

fn register_mouse_gesture_settings_target(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    identity: impl AsRef<str>,
    label: &'static str,
    kind: crate::gui::jump_mode::TargetKind,
    visual_order: &mut u64,
) {
    let identity = identity.as_ref();
    let _ = crate::gui::jump_mode::ui::register_and_apply_target(
        scope,
        ui,
        response,
        identity,
        label,
        kind,
        visual_order,
    );
    if let Some(control_id) = mouse_gesture_catalog_control_id(identity) {
        crate::settings_editor::render::observe_scoped_settings_navigation_response(
            ui, response, control_id, kind,
        );
    }
}

fn mouse_gesture_catalog_control_id(identity: &str) -> Option<&'static str> {
    Some(match identity {
        "enabled" => "mouse_gestures.enabled",
        "debug-logging" => "mouse_gestures.debug_logging",
        "show-trail" => "mouse_gestures.show_trail",
        "trail-color" => "mouse_gestures.trail_color",
        "trail-width" => "mouse_gestures.trail_width",
        "trail-refresh-interval" => "mouse_gestures.trail_interval",
        "recognition-interval" => "mouse_gestures.recognition_interval",
        "show-hint" => "mouse_gestures.show_hint",
        "hint-offset/x" => "mouse_gestures.hint_offset_x",
        "hint-offset/y" => "mouse_gestures.hint_offset_y",
        "wheel-cycle-gate" => "mouse_gestures.wheel_cycle_gate",
        "cancel-behavior" => "mouse_gestures.cancel_behavior",
        "no-match-behavior" => "mouse_gestures.no_match_behavior",
        "ignore-input" | "mouse_gesture_settings_ignore_row" => {
            "mouse_gestures.ignore_window_titles"
        }
        _ => return None,
    })
}

pub(crate) fn window_picker_layer_id() -> egui::LayerId {
    egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new(Some(WINDOW_PICKER_TITLE)),
    )
}

struct MouseGestureSettingsUiLayout {
    scroll: egui::containers::scroll_area::ScrollAreaOutput<()>,
    body_clip_rect: egui::Rect,
    #[cfg(test)]
    enable_checkbox: Option<egui::Response>,
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
    ignore_row_revision: u64,
    next_ignore_row_token: u64,
    ignore_row_tokens: Vec<u64>,
    #[cfg(test)]
    last_scroll_area_id: Option<egui::Id>,
    #[cfg(test)]
    last_scroll_max_offset_y: f32,
    #[cfg(test)]
    last_enable_checkbox_response: Option<egui::Response>,
}

impl MouseGestureSettingsDialog {
    pub(crate) fn window_picker_is_open(&self) -> bool {
        self.window_picker_open
    }

    fn renew_ignore_row_tokens(&mut self) {
        self.ignore_row_revision = self.ignore_row_revision.wrapping_add(1).max(1);
        self.ignore_row_tokens.clear();
        self.ignore_row_tokens
            .reserve(self.settings.ignore_window_titles.len());
        for _ in &self.settings.ignore_window_titles {
            self.next_ignore_row_token = self.next_ignore_row_token.wrapping_add(1).max(1);
            self.ignore_row_tokens.push(self.next_ignore_row_token);
        }
    }

    fn ensure_ignore_row_tokens(&mut self) {
        if self.ignore_row_tokens.len() != self.settings.ignore_window_titles.len() {
            self.renew_ignore_row_tokens();
        }
    }

    #[cfg(test)]
    pub(crate) fn test_settings(&self) -> &MouseGestureSettings {
        &self.settings
    }

    #[cfg(test)]
    pub(crate) fn test_dirty(&self) -> bool {
        self.dirty
    }

    #[cfg(test)]
    pub(crate) fn test_seed_picker(&mut self) {
        self.window_picker_open = true;
        self.window_picker_titles.clear();
        self.window_picker_error = None;
    }

    #[cfg(test)]
    pub(crate) fn test_picker_layer_id() -> egui::LayerId {
        window_picker_layer_id()
    }

    #[cfg(test)]
    pub(crate) fn test_scroll_area_id(&self) -> Option<egui::Id> {
        self.last_scroll_area_id
    }

    #[cfg(test)]
    pub(crate) fn test_scroll_max_offset_y(&self) -> f32 {
        self.last_scroll_max_offset_y
    }

    #[cfg(test)]
    pub(crate) fn test_enable_checkbox_response(&self) -> Option<&egui::Response> {
        self.last_enable_checkbox_response.as_ref()
    }

    pub fn open(&mut self) {
        if !self.open {
            self.needs_reload = true;
            self.window_picker_open = false;
            self.window_picker_titles.clear();
            self.window_picker_error = None;
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
        self.renew_ignore_row_tokens();
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
        self.ui_with_jump_mode(ctx, app, None, None, true, true, true);
    }

    pub(crate) fn ui_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        app: &mut crate::gui::LauncherApp,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        dialog_instance: Option<u64>,
        native_ui_owner: bool,
        native_picker_owner: bool,
        first_ui_pass: bool,
    ) {
        let _ = self.show_ui_with_jump_mode(
            ctx,
            app,
            scope,
            dialog_instance,
            native_ui_owner,
            native_picker_owner,
            first_ui_pass,
        );
    }

    fn show_ui(
        &mut self,
        ctx: &egui::Context,
        app: &mut crate::gui::LauncherApp,
    ) -> Option<MouseGestureSettingsUiLayout> {
        self.show_ui_with_jump_mode(ctx, app, None, None, true, true, true)
    }

    fn show_ui_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        app: &mut crate::gui::LauncherApp,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        dialog_instance: Option<u64>,
        native_ui_owner: bool,
        native_picker_owner: bool,
        first_ui_pass: bool,
    ) -> Option<MouseGestureSettingsUiLayout> {
        if !self.open {
            return None;
        }

        let scope = if native_ui_owner && !self.window_picker_open {
            scope
        } else {
            None
        };
        if !native_ui_owner
            && crate::gui::jump_mode::ui::focused_response_layer(ctx)
                == crate::gui::jump_mode::ui::NavigationDialogKind::MouseGestureSettings.layer_id()
            && let Some(focused) = ctx.memory(|memory| memory.focused())
        {
            ctx.memory_mut(|memory| memory.surrender_focus(focused));
        }

        // UI widgets still render and collect on every pass, but only an exact
        // visible owner pass may commit direct-bound form edits. A candidate
        // owner can still be on egui's hidden/sizing pass, so that state is
        // snapshotted inside the Window callback before any controls render.
        let mut main_state_before_pass = if native_ui_owner && first_ui_pass {
            None
        } else {
            Some((self.settings.clone(), self.ignore_input.clone(), self.dirty))
        };
        let mut picker_state_before_pass: Option<(MouseGestureSettings, bool)> = None;

        let mut open = self.open;
        let mut close_requested = false;
        let mut authorized_main_pass = false;
        let mut authorized_picker_pass = false;
        let mut picker_changed = false;
        let mut ignore_rows_changed = false;
        let mut visual_order = 0;
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
                authorized_main_pass = native_ui_owner
                    && first_ui_pass
                    && ui.is_visible()
                    && !ui.is_sizing_pass();
                if !authorized_main_pass && main_state_before_pass.is_none() {
                    main_state_before_pass = Some((
                        self.settings.clone(),
                        self.ignore_input.clone(),
                        self.dirty,
                    ));
                }
                if authorized_main_pass && self.needs_reload {
                    self.reload(&app.settings_path);
                }
                if authorized_main_pass {
                    self.ensure_ignore_row_tokens();
                }
                ui.add_enabled_ui(native_ui_owner && !self.window_picker_open, |ui| {
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
                #[cfg(test)]
                let mut enable_checkbox = None;
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
                let enabled = ui.checkbox(&mut self.settings.enabled, "Enable mouse gestures");
                #[cfg(test)]
                {
                    enable_checkbox = Some(enabled.clone());
                }
                register_mouse_gesture_settings_target(
                    scope, ui, &enabled, "enabled", "Enable mouse gestures",
                    crate::gui::jump_mode::TargetKind::Checkbox, &mut visual_order,
                );
                changed |= enabled.changed();
                let debug_logging = ui
                    .checkbox(&mut self.settings.debug_logging, "Enable debug logging");
                register_mouse_gesture_settings_target(
                    scope, ui, &debug_logging, "debug-logging", "Enable debug logging",
                    crate::gui::jump_mode::TargetKind::Checkbox, &mut visual_order,
                );
                changed |= debug_logging.changed();

                ui.separator();
                let show_trail = ui.checkbox(&mut self.settings.show_trail, "Show trail overlay");
                register_mouse_gesture_settings_target(
                    scope, ui, &show_trail, "show-trail", "Show trail overlay",
                    crate::gui::jump_mode::TargetKind::Checkbox, &mut visual_order,
                );
                changed |= show_trail.changed();
                ui.horizontal(|ui| {
                    ui.label("Trail color");
                    let mut color = egui::Color32::from_rgba_unmultiplied(
                        self.settings.trail_color[0],
                        self.settings.trail_color[1],
                        self.settings.trail_color[2],
                        self.settings.trail_color[3],
                    );
                    let resp = ui.color_edit_button_srgba(&mut color);
                    register_mouse_gesture_settings_target(
                        scope, ui, &resp, "trail-color", "Trail overlay color",
                        crate::gui::jump_mode::TargetKind::Button, &mut visual_order,
                    );
                    if resp.changed() {
                        self.settings.trail_color = [color.r(), color.g(), color.b(), color.a()];
                        changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Trail width");
                    let response = ui
                        .add(
                            egui::DragValue::new(&mut self.settings.trail_width)
                                .range(1.0..=20.0)
                                .speed(0.5),
                        );
                    register_mouse_gesture_settings_target(
                        scope, ui, &response, "trail-width", "Trail width",
                        crate::gui::jump_mode::TargetKind::TextEntry, &mut visual_order,
                    );
                    changed |= response.changed();
                });

                ui.separator();
                compact_section_heading(ui, "Timing / Performance", None);
                ui.horizontal_wrapped(|ui| {
                    ui.label("Trail refresh interval (ms)");
                    let response = ui
                        .add(
                            egui::DragValue::new(&mut self.settings.trail_interval_ms)
                                .speed(1)
                                .range(4..=250),
                        );
                    register_mouse_gesture_settings_target(
                        scope, ui, &response, "trail-refresh-interval", "Trail refresh interval",
                        crate::gui::jump_mode::TargetKind::TextEntry, &mut visual_order,
                    );
                    changed |= response.changed();
                });
                ui.horizontal(|ui| {
                    ui.label("Recognition interval (ms)");
                    let response = ui
                        .add(
                            egui::DragValue::new(&mut self.settings.recognition_interval_ms)
                                .speed(1)
                                .range(4..=500),
                        );
                    register_mouse_gesture_settings_target(
                        scope, ui, &response, "recognition-interval", "Recognition interval",
                        crate::gui::jump_mode::TargetKind::TextEntry, &mut visual_order,
                    );
                    changed |= response.changed();
                });
                ui.small("Lower intervals update more frequently but may use more CPU.");

                ui.separator();

                let show_hint = ui.checkbox(&mut self.settings.show_hint, "Show hint overlay");
                register_mouse_gesture_settings_target(
                    scope, ui, &show_hint, "show-hint", "Show hint overlay",
                    crate::gui::jump_mode::TargetKind::Checkbox, &mut visual_order,
                );
                changed |= show_hint.changed();
                ui.horizontal_wrapped(|ui| {
                    ui.label("Hint offset (x, y)");
                    let x = ui
                        .add(
                            egui::DragValue::new(&mut self.settings.hint_offset.0)
                                .range(-200.0..=200.0)
                                .speed(1.0),
                        );
                    register_mouse_gesture_settings_target(
                        scope, ui, &x, "hint-offset/x", "Hint overlay horizontal offset",
                        crate::gui::jump_mode::TargetKind::TextEntry, &mut visual_order,
                    );
                    changed |= x.changed();
                    let y = ui
                        .add(
                            egui::DragValue::new(&mut self.settings.hint_offset.1)
                                .range(-200.0..=200.0)
                                .speed(1.0),
                        );
                    register_mouse_gesture_settings_target(
                        scope, ui, &y, "hint-offset/y", "Hint overlay vertical offset",
                        crate::gui::jump_mode::TargetKind::TextEntry, &mut visual_order,
                    );
                    changed |= y.changed();
                });

                ui.horizontal_wrapped(|ui| {
                    ui.label("Wheel cycling");
                    let identity = "wheel-cycle-gate";
                    let popup_scope = scope.map(|scope| scope.combo_popup_scope(identity));
                    let combo = egui::ComboBox::from_id_salt("mg_settings_wheel_cycle_gate")
                        .selected_text(wheel_cycle_gate_label(self.settings.wheel_cycle_gate))
                        .show_ui(ui, |ui| {
                            let deadzone = ui.selectable_value(
                                    &mut self.settings.wheel_cycle_gate,
                                    crate::mouse_gestures::service::WheelCycleGate::Deadzone,
                                    "After deadzone",
                                );
                            register_mouse_gesture_settings_target(
                                popup_scope.as_ref(), ui, &deadzone, "option/deadzone",
                                "After deadzone", crate::gui::jump_mode::TargetKind::Selectable,
                                &mut visual_order,
                            );
                            changed |= deadzone.changed();
                            let shift = ui.selectable_value(
                                    &mut self.settings.wheel_cycle_gate,
                                    crate::mouse_gestures::service::WheelCycleGate::Shift,
                                    "Shift + wheel",
                                );
                            register_mouse_gesture_settings_target(
                                popup_scope.as_ref(), ui, &shift, "option/shift",
                                "Shift + wheel", crate::gui::jump_mode::TargetKind::Selectable,
                                &mut visual_order,
                            );
                            changed |= shift.changed();
                        });
                    register_mouse_gesture_settings_target(
                        scope, ui, &combo.response, identity, "Wheel cycling",
                        crate::gui::jump_mode::TargetKind::ComboBox, &mut visual_order,
                    );
                });

                ui.separator();

                ui.horizontal_wrapped(|ui| {
                    ui.label("Cancel behavior");
                    let identity = "cancel-behavior";
                    let popup_scope = scope.map(|scope| scope.combo_popup_scope(identity));
                    let combo = egui::ComboBox::from_id_salt("mg_settings_cancel_behavior")
                        .selected_text(cancel_behavior_label(self.settings.cancel_behavior))
                        .show_ui(ui, |ui| {
                            let do_nothing = ui.selectable_value(
                                    &mut self.settings.cancel_behavior,
                                    crate::mouse_gestures::service::CancelBehavior::DoNothing,
                                    "Do nothing",
                                );
                            register_mouse_gesture_settings_target(
                                popup_scope.as_ref(), ui, &do_nothing, "option/do-nothing",
                                "Do nothing", crate::gui::jump_mode::TargetKind::Selectable,
                                &mut visual_order,
                            );
                            changed |= do_nothing.changed();
                            let pass_through = ui.selectable_value(
                                    &mut self.settings.cancel_behavior,
                                    crate::mouse_gestures::service::CancelBehavior::PassThroughClick,
                                    "Pass through right-click",
                                );
                            register_mouse_gesture_settings_target(
                                popup_scope.as_ref(), ui, &pass_through,
                                "option/pass-through-click", "Pass through right-click",
                                crate::gui::jump_mode::TargetKind::Selectable, &mut visual_order,
                            );
                            changed |= pass_through.changed();
                        });
                    register_mouse_gesture_settings_target(
                        scope, ui, &combo.response, identity, "Cancel behavior",
                        crate::gui::jump_mode::TargetKind::ComboBox, &mut visual_order,
                    );
                });

                ui.horizontal_wrapped(|ui| {
                    ui.label("No-match behavior");
                    let identity = "no-match-behavior";
                    let popup_scope = scope.map(|scope| scope.combo_popup_scope(identity));
                    let combo = egui::ComboBox::from_id_salt("mg_settings_no_match_behavior")
                        .selected_text(no_match_behavior_label(self.settings.no_match_behavior))
                        .show_ui(ui, |ui| {
                            let do_nothing = ui.selectable_value(
                                    &mut self.settings.no_match_behavior,
                                    crate::mouse_gestures::service::NoMatchBehavior::DoNothing,
                                    "Do nothing",
                                );
                            register_mouse_gesture_settings_target(
                                popup_scope.as_ref(), ui, &do_nothing, "option/do-nothing",
                                "Do nothing", crate::gui::jump_mode::TargetKind::Selectable,
                                &mut visual_order,
                            );
                            changed |= do_nothing.changed();
                            let pass_through = ui.selectable_value(
                                    &mut self.settings.no_match_behavior,
                                    crate::mouse_gestures::service::NoMatchBehavior::PassThroughClick,
                                    "Pass through right-click",
                                );
                            register_mouse_gesture_settings_target(
                                popup_scope.as_ref(), ui, &pass_through,
                                "option/pass-through-click", "Pass through right-click",
                                crate::gui::jump_mode::TargetKind::Selectable, &mut visual_order,
                            );
                            changed |= pass_through.changed();
                            let show_hint = ui.selectable_value(
                                    &mut self.settings.no_match_behavior,
                                    crate::mouse_gestures::service::NoMatchBehavior::ShowNoMatchHint,
                                    "Show no-match hint",
                                );
                            register_mouse_gesture_settings_target(
                                popup_scope.as_ref(), ui, &show_hint, "option/show-no-match-hint",
                                "Show no-match hint", crate::gui::jump_mode::TargetKind::Selectable,
                                &mut visual_order,
                            );
                            changed |= show_hint.changed();
                        });
                    register_mouse_gesture_settings_target(
                        scope, ui, &combo.response, identity, "No-match behavior",
                        crate::gui::jump_mode::TargetKind::ComboBox, &mut visual_order,
                    );
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
                let row_scope = scope.map(|scope| {
                    scope.child_namespace(format!("ignore-rows/{}", self.ignore_row_revision))
                });
                if self.settings.ignore_window_titles.is_empty() {
                    ui.label("No ignored windows.");
                } else {
                    for (index, (title, token)) in self
                        .settings
                        .ignore_window_titles
                        .iter()
                        .zip(&self.ignore_row_tokens)
                        .enumerate()
                    {
                        ui.scope_builder(
                            egui::UiBuilder::new().id(egui::Id::new((
                                "mouse_gesture_settings_ignore_row",
                                dialog_instance.unwrap_or(0),
                                self.ignore_row_revision,
                                *token,
                            ))),
                            |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(title);
                                    let remove = ui.button("Remove");
                                    register_mouse_gesture_settings_target(
                                        row_scope.as_ref(),
                                        ui,
                                        &remove,
                                        format!("remove/{token}"),
                                        "Remove ignored window",
                                        crate::gui::jump_mode::TargetKind::Button,
                                        &mut visual_order,
                                    );
                                    if remove.clicked() && authorized_main_pass {
                                        remove_index = Some(index);
                                    }
                                });
                            },
                        );
                    }
                }
                if let Some(index) = remove_index {
                    self.settings.ignore_window_titles.remove(index);
                    changed = true;
                    ignore_rows_changed = true;
                }

                ui.horizontal_wrapped(|ui| {
                    let response = ui.text_edit_singleline(&mut self.ignore_input);
                    register_mouse_gesture_settings_target(
                        scope, ui, &response, "ignore-input", "Ignored window title",
                        crate::gui::jump_mode::TargetKind::TextEntry, &mut visual_order,
                    );
                    let add_button = ui.button("Add");
                    register_mouse_gesture_settings_target(
                        scope, ui, &add_button, "ignore-add", "Add ignored window",
                        crate::gui::jump_mode::TargetKind::Button, &mut visual_order,
                    );
                    let mut add_now = with_button_emphasis(
                        ui,
                        add_button,
                        ButtonEmphasis::Primary,
                    )
                    .clicked();
                    if authorized_main_pass
                        && response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        add_now = true;
                    }
                    if add_now && authorized_main_pass {
                        if add_ignore_window_title(
                            &mut self.settings.ignore_window_titles,
                            &self.ignore_input,
                        ) {
                            changed = true;
                            ignore_rows_changed = true;
                        }
                        self.ignore_input.clear();
                    }
                });

                let window_button = ui.add_enabled(cfg!(windows), egui::Button::new("Select window..."));
                register_mouse_gesture_settings_target(
                    scope, ui, &window_button, "select-window", "Select window",
                    crate::gui::jump_mode::TargetKind::Button, &mut visual_order,
                );
                if !cfg!(windows) {
                    window_button.on_hover_text("Window picker is only available on Windows.");
                } else if window_button.clicked() && authorized_main_pass {
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
                    if self.window_picker_open
                        && let Some(scope) = scope
                    {
                        // The child is a native-only picker. Do not leave the
                        // parent's earlier targets actionable for this pass.
                        scope.discard_registered_targets();
                    }
                }

                // When the parent form is not the authorized native owner,
                // restore its direct-bound widget state before the nested
                // picker gets a chance to apply its own authorized choice.
                if let Some((settings, ignore_input, dirty)) = main_state_before_pass.take() {
                    self.settings = settings;
                    self.ignore_input = ignore_input;
                    self.dirty = dirty;
                }

                if self.window_picker_open {
                    let mut open_picker = self.window_picker_open;
                    egui::Window::new(WINDOW_PICKER_TITLE)
                        .open(&mut open_picker)
                        .resizable(true)
                        .show(ctx, |ui| {
                            authorized_picker_pass = native_picker_owner
                                && first_ui_pass
                                && ui.is_visible()
                                && !ui.is_sizing_pass();
                            if !authorized_picker_pass {
                                picker_state_before_pass = Some((
                                    self.settings.clone(),
                                    self.dirty,
                                ));
                            }
                            ui.add_enabled_ui(authorized_picker_pass, |ui| {
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
                                let titles = &self.window_picker_titles;
                                egui::ScrollArea::vertical()
                                    .max_height(220.0)
                                    .show(ui, |ui| {
                                        for title in titles {
                                            ui.horizontal(|ui| {
                                                ui.label(title);
                                                if ui.button("Add").clicked()
                                                    && authorized_picker_pass
                                                    && add_ignore_window_title(
                                                        &mut self.settings.ignore_window_titles,
                                                        title,
                                                    ) {
                                                        picker_changed = true;
                                                        ignore_rows_changed = true;
                                                    }
                                            });
                                        }
                                    });
                            }
                            });
                        });
                    if authorized_picker_pass {
                        self.window_picker_open = open_picker;
                    }
                }

                if authorized_main_pass
                    && normalize_ignore_window_titles(&mut self.settings.ignore_window_titles)
                {
                    changed = true;
                    ignore_rows_changed = true;
                }

                secondary_help(
                    ui,
                    "Tip: You can also open this window by running `mg settings` in the launcher.",
                );

                if changed && authorized_main_pass {
                    self.dirty = true;
                    // Live-apply while editing.
                    apply_runtime_settings(self.settings.clone());
                }
                if picker_changed && authorized_picker_pass {
                    self.dirty = true;
                    apply_runtime_settings(self.settings.clone());
                }
                if ignore_rows_changed && (authorized_main_pass || authorized_picker_pass) {
                    if let Some(row_scope) = &row_scope {
                        row_scope.discard_registered_namespace();
                    }
                    self.renew_ignore_row_tokens();
                }
                    });

                ui.separator();

                let footer_scope = if self.window_picker_open { None } else { scope };
                let (reload_button, save_button, close_button) = ui.horizontal(|ui| {
                    let reload_button = ui.button("Reload");
                    register_mouse_gesture_settings_target(
                        footer_scope, ui, &reload_button, "reload", "Reload mouse gesture settings",
                        crate::gui::jump_mode::TargetKind::Button, &mut visual_order,
                    );
                    if reload_button.clicked() && authorized_main_pass {
                        self.needs_reload = true;
                    }

                    let save_button = ui.add_enabled(self.dirty, egui::Button::new("Save"));
                    register_mouse_gesture_settings_target(
                        footer_scope, ui, &save_button, "save", "Save mouse gesture settings",
                        crate::gui::jump_mode::TargetKind::Button, &mut visual_order,
                    );
                    if with_button_emphasis(ui, save_button.clone(), ButtonEmphasis::Primary)
                        .clicked()
                        && authorized_main_pass
                    {
                        self.save(app);
                    }
                    let close_button = ui.button("Close");
                    register_mouse_gesture_settings_target(
                        footer_scope, ui, &close_button, "close", "Close mouse gesture settings",
                        crate::gui::jump_mode::TargetKind::Button, &mut visual_order,
                    );
                    if close_button.clicked() && authorized_main_pass {
                        close_requested = true;
                    }
                    (reload_button, save_button, close_button)
                }).inner;
                MouseGestureSettingsUiLayout {
                    scroll,
                    body_clip_rect,
                    #[cfg(test)]
                    enable_checkbox,
                    reload_button,
                    save_button,
                    close_button,
                }
                }).inner
            });

        let pinned = app
            .pinned_panels
            .contains(&crate::gui::Panel::MouseGestureSettingsDialog);
        if native_ui_owner && first_ui_pass {
            self.open = (open && !close_requested) || pinned;
        }
        if authorized_picker_pass {
            self.window_picker_open = self.window_picker_open && self.open;
        }
        if !self.open {
            self.window_picker_open = false;
            self.window_picker_titles.clear();
        }
        if let Some((settings, dirty)) = picker_state_before_pass {
            self.settings = settings;
            self.dirty = dirty;
        }
        let layout = window.and_then(|window| window.inner);
        #[cfg(test)]
        if let Some(layout) = &layout {
            self.last_scroll_area_id = Some(layout.scroll.id);
            self.last_scroll_max_offset_y =
                (layout.scroll.content_size.y - layout.scroll.inner_rect.height()).max(0.0);
            self.last_enable_checkbox_response = layout.enable_checkbox.clone();
        }
        layout
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
        let mut settings = Settings::default();
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test(),
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            settings,
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
