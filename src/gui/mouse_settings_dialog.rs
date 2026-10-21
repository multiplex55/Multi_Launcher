use crate::commands::CoordinateToolCommand;
use crate::coordinate_tool::{
    ClickEffectPreferences, ClickEffectStatus, ClickEffectStyle, CoordinateOffset, CoordinateSpace,
    CoordinateToolPreferences, CrosshairColor, CursorEffectStatus, HaloPreferences, HudDetail,
    ZoomMode, ZoomPreferences,
};
use crate::gui::coordinate_tool::CoordinateToolGui;
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, semantic_status, with_button_emphasis,
};
use eframe::egui;

#[derive(Default)]
pub(crate) struct MouseSettingsDialog {
    pub(crate) open: bool,
    baseline: CoordinateToolPreferences,
    draft: CoordinateToolPreferences,
    last_error: Option<String>,
    #[cfg(test)]
    last_scroll_area_id: Option<egui::Id>,
    #[cfg(test)]
    last_scroll_max_offset_y: f32,
    #[cfg(test)]
    last_cancel_response: Option<egui::Response>,
}

fn register_mouse_settings_target(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    identity: &'static str,
    label: &'static str,
    kind: crate::gui::jump_mode::TargetKind,
    visual_order: &mut u64,
) {
    let _ = crate::gui::jump_mode::ui::register_and_apply_target(
        scope,
        ui,
        response,
        identity,
        label,
        kind,
        visual_order,
    );
}

impl MouseSettingsDialog {
    pub(crate) fn open(&mut self, current: &CoordinateToolPreferences) {
        if !self.open {
            self.reset_from(current);
        } else if !self.is_dirty() {
            self.sync_clean_draft(current);
        }
        self.open = true;
    }

    fn is_dirty(&self) -> bool {
        self.draft != self.baseline
    }

    #[cfg(test)]
    pub(crate) fn test_draft(&self) -> &CoordinateToolPreferences {
        &self.draft
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
    pub(crate) fn test_cancel_response(&self) -> Option<egui::Response> {
        self.last_cancel_response.clone()
    }

    fn reset_from(&mut self, current: &CoordinateToolPreferences) {
        self.sync_clean_draft(current);
        self.last_error = None;
    }

    fn sync_clean_draft(&mut self, current: &CoordinateToolPreferences) {
        self.baseline = current.clone();
        self.draft = current.clone();
    }

    fn reset_halo_draft(&mut self) {
        self.draft.halo = HaloPreferences::default();
    }

    fn reset_zoom_draft(&mut self) {
        self.draft.zoom = ZoomPreferences::default();
    }

    fn reset_click_draft(&mut self) {
        self.draft.click = ClickEffectPreferences::default();
    }

    fn cancel_draft(&mut self, current: &CoordinateToolPreferences) {
        self.reset_from(current);
        self.open = false;
    }

    pub(crate) fn apply_to(&mut self, adapter: &mut CoordinateToolGui) -> Result<(), String> {
        match adapter.apply_draft_preferences(&self.baseline, &self.draft) {
            Ok(()) => {
                let committed = adapter.preferences().clone();
                self.reset_from(&committed);
                Ok(())
            }
            Err(error) => {
                self.last_error = Some(error.clone());
                Err(error)
            }
        }
    }

    pub(crate) fn ui(&mut self, ctx: &egui::Context, adapter: &mut CoordinateToolGui) {
        self.ui_with_jump_mode(ctx, adapter, None, true, true);
    }

    fn show_ui(
        &mut self,
        ctx: &egui::Context,
        adapter: &mut CoordinateToolGui,
    ) -> Option<egui::containers::scroll_area::ScrollAreaOutput<()>> {
        self.show_ui_with_jump_mode(ctx, adapter, None, true, true)
    }

    pub(crate) fn ui_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        adapter: &mut CoordinateToolGui,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        native_ui_owner: bool,
        first_ui_pass: bool,
    ) {
        let _ = self.show_ui_with_jump_mode(ctx, adapter, scope, native_ui_owner, first_ui_pass);
    }

    fn show_ui_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        adapter: &mut CoordinateToolGui,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        native_ui_owner: bool,
        first_ui_pass: bool,
    ) -> Option<egui::containers::scroll_area::ScrollAreaOutput<()>> {
        if !self.open {
            return None;
        }
        #[cfg(test)]
        {
            self.last_cancel_response = None;
        }
        // Native effect status arrives asynchronously from the passive worker.
        // Keep the open dialog's local availability labels fresh even when no
        // other application activity would otherwise request an egui frame.
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        if !self.is_dirty() {
            self.sync_clean_draft(adapter.preferences());
        }

        let scope = native_ui_owner.then_some(scope).flatten();
        let draft_before_ui = self.draft.clone();
        let error_before_ui = self.last_error.clone();
        let open_before_ui = self.open;
        let mut authorized_owner_pass = native_ui_owner && first_ui_pass;
        if !native_ui_owner
            && crate::gui::jump_mode::ui::focused_response_layer(ctx)
                == crate::gui::jump_mode::ui::NavigationDialogKind::MouseSettings.layer_id()
            && let Some(focused) = ctx.memory(|memory| memory.focused())
        {
            ctx.memory_mut(|memory| memory.surrender_focus(focused));
        }

        let runtime = adapter.runtime_state();
        let effect_status = adapter.effects_status();
        let mut open = self.open;
        let mut cancel_requested = false;
        #[cfg(test)]
        let mut cancel_response = None;
        let output = egui::Window::new("Mouse Settings")
            .open(&mut open)
            .resizable(true)
            .default_width(420.0)
            .max_width((ctx.content_rect().width() - 16.0).max(240.0))
            .show(ctx, |ui| {
                authorized_owner_pass &= ui.is_visible() && !ui.is_sizing_pass();
                ui.add_enabled_ui(native_ui_owner, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("mouse_settings_form")
                        .max_height(ui.available_height().max(1.0))
                        .show(ui, |ui| {
                            let mut visual_order = 0;
                        compact_section_heading(
                            ui,
                            "Coordinate overlays",
                            Some("Session switches take effect immediately. Appearance changes stay in draft until Apply."),
                        );
                        crate::gui::theme::widgets::secondary_help(
                            ui,
                            "Commands: mouse coords …, mouse click toggle|on|off or settings, mouse crosshair gap N, mouse halo/zoom toggle|on|off, mouse effects off, and mouse help.",
                        );
                        ui.separator();

                        if let Some(error) = &self.last_error {
                            semantic_status(ui, StatusTone::Error, "Settings error", Some(error));
                            ui.separator();
                        }

                        ui.horizontal_wrapped(|ui| {
                            compact_section_heading(ui, "Click Visualization", None);
                            let reset = ui.small_button("Reset click draft");
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &reset,
                                "click/reset",
                                "Reset click draft",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut visual_order,
                            );
                            if reset.clicked() && authorized_owner_pass {
                                self.reset_click_draft();
                            }
                        });
                        let mut click_enabled = runtime.click_enabled();
                        let click_enabled_response = ui
                            .push_id("mouse_settings_click_enabled", |ui| {
                                ui.checkbox(&mut click_enabled, "Enabled for this session")
                            })
                            .inner
                            .on_hover_text(
                                "Starts or stops passive click feedback immediately. Enablement is not saved.",
                            );
                        register_mouse_settings_target(
                            scope,
                            ui,
                            &click_enabled_response,
                            "click/session-enabled",
                            "Click feedback session switch",
                            crate::gui::jump_mode::TargetKind::Checkbox,
                            &mut visual_order,
                        );
                        if click_enabled_response.changed()
                            && authorized_owner_pass
                            && let Err(error) =
                                adapter.execute(&CoordinateToolCommand::SetClickEnabled(click_enabled))
                        {
                            self.last_error = Some(error);
                        }
                        let click_status = adapter.effects_status().click().clone();
                        semantic_status(
                            ui,
                            click_status_tone(&click_status),
                            &click_status_text(&click_status),
                            None,
                        );
                        ui.small(applied_click_summary(adapter.preferences()));
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Style");
                            let popup_scope = scope.map(|scope| {
                                scope.combo_popup_scope("click/style")
                            });
                            let combo = egui::ComboBox::from_id_salt("mouse_settings_click_style")
                                .selected_text(click_style_label(self.draft.click.style))
                                .show_ui(ui, |ui| {
                                    let ripple = ui.selectable_value(
                                        &mut self.draft.click.style,
                                        ClickEffectStyle::Ripple,
                                        "Ripple",
                                    );
                                    register_mouse_settings_target(
                                        popup_scope.as_ref(),
                                        ui,
                                        &ripple,
                                        "style/ripple",
                                        "Ripple",
                                        crate::gui::jump_mode::TargetKind::Selectable,
                                        &mut visual_order,
                                    );
                                    let circle = ui.selectable_value(
                                        &mut self.draft.click.style,
                                        ClickEffectStyle::Circle,
                                        "Circle",
                                    );
                                    register_mouse_settings_target(
                                        popup_scope.as_ref(),
                                        ui,
                                        &circle,
                                        "style/circle",
                                        "Circle",
                                        crate::gui::jump_mode::TargetKind::Selectable,
                                        &mut visual_order,
                                    );
                                    let flash = ui.selectable_value(
                                        &mut self.draft.click.style,
                                        ClickEffectStyle::Flash,
                                        "Flash",
                                    );
                                    register_mouse_settings_target(
                                        popup_scope.as_ref(),
                                        ui,
                                        &flash,
                                        "style/flash",
                                        "Flash",
                                        crate::gui::jump_mode::TargetKind::Selectable,
                                        &mut visual_order,
                                    );
                                });
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &combo.response,
                                "click/style",
                                "Click effect style",
                                crate::gui::jump_mode::TargetKind::ComboBox,
                                &mut visual_order,
                            );
                        });
                        let mut click_rgb = [
                            self.draft.click.color.red,
                            self.draft.click.color.green,
                            self.draft.click.color.blue,
                        ];
                        let click_color_row = crate::gui::theme::widgets::labeled_form_row(
                            ui,
                            "RGB color",
                            |ui| {
                                let color = ui.color_edit_button_srgb(&mut click_rgb);
                                register_mouse_settings_target(
                                    scope,
                                    ui,
                                    &color,
                                    "click/color",
                                    "Click effect color",
                                    crate::gui::jump_mode::TargetKind::Button,
                                    &mut visual_order,
                                );
                                color
                            },
                        );
                        if click_color_row.changed() {
                            self.draft.click.color = CrosshairColor::new(
                                click_rgb[0],
                                click_rgb[1],
                                click_rgb[2],
                            );
                        }
                        let click_size = ui.add(
                            egui::Slider::new(&mut self.draft.click.size_px, 4..=256)
                                .show_value(true)
                                .text("Size (physical pixels)"),
                        );
                        register_mouse_settings_target(
                            scope,
                            ui,
                            &click_size,
                            "click/size",
                            "Click effect size",
                            crate::gui::jump_mode::TargetKind::Slider,
                            &mut visual_order,
                        );
                        let click_opacity = ui.add(
                            egui::Slider::new(&mut self.draft.click.opacity, 0.1..=1.0)
                                .show_value(true)
                                .text("Opacity"),
                        );
                        register_mouse_settings_target(
                            scope,
                            ui,
                            &click_opacity,
                            "click/opacity",
                            "Click effect opacity",
                            crate::gui::jump_mode::TargetKind::Slider,
                            &mut visual_order,
                        );
                        let click_duration = ui.add(
                            egui::Slider::new(&mut self.draft.click.duration_ms, 50..=2_000)
                                .show_value(true)
                                .text("Duration (ms)"),
                        );
                        register_mouse_settings_target(
                            scope,
                            ui,
                            &click_duration,
                            "click/duration",
                            "Click effect duration",
                            crate::gui::jump_mode::TargetKind::Slider,
                            &mut visual_order,
                        );
                        ui.small(
                            "The indicator follows the physical press point and disappears after its saved duration. Appearance changes stay in draft until Apply.",
                        );
                        ui.separator();

                        compact_section_heading(ui, "Crosshair", None);
                        let mut crosshair_enabled = runtime.crosshair_enabled();
                        let crosshair_enabled_response = ui
                            .push_id("mouse_settings_crosshair_enabled", |ui| {
                                ui.checkbox(&mut crosshair_enabled, "Enabled for this session")
                            })
                            .inner
                            ;
                        register_mouse_settings_target(
                            scope,
                            ui,
                            &crosshair_enabled_response,
                            "crosshair/session-enabled",
                            "Crosshair session switch",
                            crate::gui::jump_mode::TargetKind::Checkbox,
                            &mut visual_order,
                        );
                        if crosshair_enabled_response.changed()
                            && authorized_owner_pass
                            && let Err(error) = adapter.execute(
                                &CoordinateToolCommand::SetCrosshairEnabled(crosshair_enabled),
                            )
                        {
                            self.last_error = Some(error);
                        }

                        let mut crosshair_rgb = [
                            self.draft.crosshair.color.red,
                            self.draft.crosshair.color.green,
                            self.draft.crosshair.color.blue,
                        ];
                        let crosshair_color_row = crate::gui::theme::widgets::labeled_form_row(
                            ui,
                            "RGB color",
                            |ui| {
                                let color = ui.color_edit_button_srgb(&mut crosshair_rgb);
                                register_mouse_settings_target(
                                    scope,
                                    ui,
                                    &color,
                                    "crosshair/color",
                                    "Crosshair color",
                                    crate::gui::jump_mode::TargetKind::Button,
                                    &mut visual_order,
                                );
                                color
                            },
                        );
                        if crosshair_color_row.changed() {
                            self.draft.crosshair.color = CrosshairColor::new(
                                crosshair_rgb[0],
                                crosshair_rgb[1],
                                crosshair_rgb[2],
                            );
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Thickness");
                            let thickness = ui.add(
                                egui::DragValue::new(&mut self.draft.crosshair.thickness)
                                    .range(1..=16),
                            );
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &thickness,
                                "crosshair/thickness",
                                "Crosshair thickness",
                                crate::gui::jump_mode::TargetKind::TextEntry,
                                &mut visual_order,
                            );
                            ui.label("Arm length");
                            let arm_length = ui.add(
                                egui::DragValue::new(&mut self.draft.crosshair.arm_length)
                                    .range(2..=256),
                            );
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &arm_length,
                                "crosshair/arm-length",
                                "Crosshair arm length",
                                crate::gui::jump_mode::TargetKind::TextEntry,
                                &mut visual_order,
                            );
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Center gap (physical pixels)");
                            let center_gap = ui.add(
                                egui::DragValue::new(&mut self.draft.crosshair.center_gap)
                                    .range(0..=128),
                            )
                            .on_hover_text(
                                "Per-arm distance from the cursor hotspot to the first visible pixel, including the outline, in physical pixels. Set to 0 to meet at the hotspot.",
                            );
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &center_gap,
                                "crosshair/center-gap",
                                "Crosshair center gap",
                                crate::gui::jump_mode::TargetKind::TextEntry,
                                &mut visual_order,
                            );
                        });
                        let crosshair_opacity = ui.add(
                            egui::Slider::new(&mut self.draft.crosshair.opacity, 0.1..=1.0)
                                .show_value(true)
                                .text("Opacity"),
                        );
                        register_mouse_settings_target(
                            scope,
                            ui,
                            &crosshair_opacity,
                            "crosshair/opacity",
                            "Crosshair opacity",
                            crate::gui::jump_mode::TargetKind::Slider,
                            &mut visual_order,
                        );
                        let guides = ui.checkbox(
                            &mut self.draft.crosshair.virtual_desktop_guides,
                            "Show virtual-desktop guide lines",
                        );
                        register_mouse_settings_target(
                            scope,
                            ui,
                            &guides,
                            "crosshair/virtual-desktop-guides",
                            "Virtual desktop guide lines",
                            crate::gui::jump_mode::TargetKind::Checkbox,
                            &mut visual_order,
                        );
                        let high_contrast = ui.checkbox(
                            &mut self.draft.crosshair.high_contrast_outline,
                            "High-contrast outline",
                        );
                        register_mouse_settings_target(
                            scope,
                            ui,
                            &high_contrast,
                            "crosshair/high-contrast-outline",
                            "Crosshair high-contrast outline",
                            crate::gui::jump_mode::TargetKind::Checkbox,
                            &mut visual_order,
                        );

                        ui.separator();
                        ui.horizontal_wrapped(|ui| {
                            compact_section_heading(ui, "Cursor Halo", None);
                            let reset = ui.small_button("Reset halo draft");
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &reset,
                                "halo/reset",
                                "Reset halo draft",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut visual_order,
                            );
                            if reset.clicked() && authorized_owner_pass {
                                self.reset_halo_draft();
                            }
                        });
                        let mut halo_enabled = runtime.halo_enabled();
                        let halo_enabled_response = ui
                            .push_id("mouse_settings_halo_enabled", |ui| {
                                ui.checkbox(&mut halo_enabled, "Enabled for this session")
                            })
                            .inner
                            .on_hover_text(
                                "Starts or stops the halo immediately for this session. This switch is not saved.",
                            );
                        register_mouse_settings_target(
                            scope,
                            ui,
                            &halo_enabled_response,
                            "halo/session-enabled",
                            "Halo session switch",
                            crate::gui::jump_mode::TargetKind::Checkbox,
                            &mut visual_order,
                        );
                        if halo_enabled_response.changed()
                            && authorized_owner_pass
                            && let Err(error) = adapter.set_halo_enabled(halo_enabled)
                        {
                            self.last_error = Some(error);
                        }
                        let halo_status = effect_status.halo();
                        semantic_status(
                            ui,
                            effect_status_tone(halo_status),
                            &effect_status_text(halo_status),
                            None,
                        );
                        ui.small(applied_halo_summary(adapter.preferences()));
                        let halo_radius = ui.add(
                            egui::Slider::new(&mut self.draft.halo.radius, 8..=256)
                                .show_value(true)
                                .text("Radius (physical pixels)"),
                        )
                        .on_hover_text("Outer halo radius in physical desktop pixels.");
                        register_mouse_settings_target(
                            scope,
                            ui,
                            &halo_radius,
                            "halo/radius",
                            "Halo radius",
                            crate::gui::jump_mode::TargetKind::Slider,
                            &mut visual_order,
                        );
                        let halo_inversion = ui.add(
                            egui::Slider::new(
                                &mut self.draft.halo.inversion_strength,
                                0.0..=1.0,
                            )
                            .show_value(true)
                            .text("Inversion strength"),
                        )
                        .on_hover_text(
                            "0 leaves colors unchanged; 1 applies full inversion. Apply saves this draft value.",
                        );
                        register_mouse_settings_target(
                            scope,
                            ui,
                            &halo_inversion,
                            "halo/inversion-strength",
                            "Halo inversion strength",
                            crate::gui::jump_mode::TargetKind::Slider,
                            &mut visual_order,
                        );
                        let halo_outline_enabled = ui.checkbox(
                            &mut self.draft.halo.outline_enabled,
                            "Show halo outline",
                        );
                        register_mouse_settings_target(
                            scope,
                            ui,
                            &halo_outline_enabled,
                            "halo/outline-enabled",
                            "Halo outline",
                            crate::gui::jump_mode::TargetKind::Checkbox,
                            &mut visual_order,
                        );
                        let mut halo_outline_rgb = [
                            self.draft.halo.outline_color.red,
                            self.draft.halo.outline_color.green,
                            self.draft.halo.outline_color.blue,
                        ];
                        let halo_outline_color_row = crate::gui::theme::widgets::labeled_form_row(
                            ui,
                            "Outline RGB",
                            |ui| {
                                let color = ui.color_edit_button_srgb(&mut halo_outline_rgb);
                                register_mouse_settings_target(
                                    scope,
                                    ui,
                                    &color,
                                    "halo/outline-color",
                                    "Halo outline color",
                                    crate::gui::jump_mode::TargetKind::Button,
                                    &mut visual_order,
                                );
                                color
                            },
                        );
                        if halo_outline_color_row.changed() {
                            self.draft.halo.outline_color = CrosshairColor::new(
                                halo_outline_rgb[0],
                                halo_outline_rgb[1],
                                halo_outline_rgb[2],
                            );
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Thickness (px)");
                            let outline_thickness = ui.add(
                                egui::DragValue::new(&mut self.draft.halo.outline_thickness)
                                    .range(1..=8),
                            );
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &outline_thickness,
                                "halo/outline-thickness",
                                "Halo outline thickness",
                                crate::gui::jump_mode::TargetKind::TextEntry,
                                &mut visual_order,
                            );
                        });

                        ui.separator();
                        ui.horizontal_wrapped(|ui| {
                            compact_section_heading(ui, "Cursor Magnifier", None);
                            let reset = ui.small_button("Reset magnifier draft");
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &reset,
                                "magnifier/reset",
                                "Reset magnifier draft",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut visual_order,
                            );
                            if reset.clicked() && authorized_owner_pass {
                                self.reset_zoom_draft();
                            }
                        });
                        let mut zoom_enabled = runtime.zoom_enabled();
                        let zoom_enabled_response = ui
                            .push_id("mouse_settings_zoom_enabled", |ui| {
                                ui.checkbox(&mut zoom_enabled, "Enabled for this session")
                            })
                            .inner
                            .on_hover_text(
                                "Starts or stops the magnifier immediately for this session. This switch is not saved.",
                            );
                        register_mouse_settings_target(
                            scope,
                            ui,
                            &zoom_enabled_response,
                            "magnifier/session-enabled",
                            "Magnifier session switch",
                            crate::gui::jump_mode::TargetKind::Checkbox,
                            &mut visual_order,
                        );
                        if zoom_enabled_response.changed()
                            && authorized_owner_pass
                            && let Err(error) = adapter.set_zoom_enabled(zoom_enabled)
                        {
                            self.last_error = Some(error);
                        }
                        let zoom_status = effect_status.zoom();
                        semantic_status(
                            ui,
                            effect_status_tone(zoom_status),
                            &effect_status_text(zoom_status),
                            None,
                        );
                        ui.small(applied_zoom_summary(adapter.preferences()));
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Destination mode");
                            let popup_scope = scope.map(|scope| {
                                scope.combo_popup_scope("magnifier/destination-mode")
                            });
                            let combo = egui::ComboBox::from_id_salt("mouse_settings_zoom_mode")
                                .selected_text(zoom_mode_label(self.draft.zoom.mode))
                                .show_ui(ui, |ui| {
                                    let offset = ui.selectable_value(
                                        &mut self.draft.zoom.mode,
                                        ZoomMode::Offset,
                                        "Offset",
                                    );
                                    register_mouse_settings_target(
                                        popup_scope.as_ref(),
                                        ui,
                                        &offset,
                                        "mode/offset",
                                        "Offset",
                                        crate::gui::jump_mode::TargetKind::Selectable,
                                        &mut visual_order,
                                    );
                                    let centered = ui.selectable_value(
                                        &mut self.draft.zoom.mode,
                                        ZoomMode::Centered,
                                        "Centered on cursor",
                                    );
                                    register_mouse_settings_target(
                                        popup_scope.as_ref(),
                                        ui,
                                        &centered,
                                        "mode/centered",
                                        "Centered on cursor",
                                        crate::gui::jump_mode::TargetKind::Selectable,
                                        &mut visual_order,
                                    );
                                });
                            let combo_response = combo.response.on_hover_text(
                                "Choose where the lens is placed; its source remains centered on the cursor.",
                            );
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &combo_response,
                                "magnifier/destination-mode",
                                "Magnifier destination mode",
                                crate::gui::jump_mode::TargetKind::ComboBox,
                                &mut visual_order,
                            );
                        });
                        let zoom_factor = ui.add(
                            egui::Slider::new(&mut self.draft.zoom.zoom_factor, 1.25..=4.0)
                                .show_value(true)
                                .text("Magnification factor"),
                        )
                        .on_hover_text(
                            "Scale applied to the cursor-centered desktop source. Apply saves this draft value.",
                        );
                        register_mouse_settings_target(
                            scope,
                            ui,
                            &zoom_factor,
                            "magnifier/factor",
                            "Magnifier factor",
                            crate::gui::jump_mode::TargetKind::Slider,
                            &mut visual_order,
                        );
                        let zoom_diameter = ui.add(
                            egui::Slider::new(&mut self.draft.zoom.diameter, 64..=480)
                                .show_value(true)
                                .text("Lens diameter (physical pixels)"),
                        );
                        register_mouse_settings_target(
                            scope,
                            ui,
                            &zoom_diameter,
                            "magnifier/diameter",
                            "Magnifier lens diameter",
                            crate::gui::jump_mode::TargetKind::Slider,
                            &mut visual_order,
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Destination X offset (physical px)");
                            let x_offset = ui.add(
                                egui::DragValue::new(&mut self.draft.zoom.destination_offset.x)
                                    .range(-2048..=2048),
                            );
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &x_offset,
                                "magnifier/destination-x",
                                "Magnifier destination X offset",
                                crate::gui::jump_mode::TargetKind::TextEntry,
                                &mut visual_order,
                            );
                            ui.label("Y offset");
                            let y_offset = ui.add(
                                egui::DragValue::new(&mut self.draft.zoom.destination_offset.y)
                                    .range(-2048..=2048),
                            );
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &y_offset,
                                "magnifier/destination-y",
                                "Magnifier destination Y offset",
                                crate::gui::jump_mode::TargetKind::TextEntry,
                                &mut visual_order,
                            );
                        })
                        .response
                        .on_hover_text(
                            "Signed physical-pixel displacement of the lens center from the live cursor. The source remains cursor-centered.",
                        );
                        let zoom_outline_enabled = ui.checkbox(
                            &mut self.draft.zoom.outline_enabled,
                            "Show magnifier outline",
                        );
                        register_mouse_settings_target(
                            scope,
                            ui,
                            &zoom_outline_enabled,
                            "magnifier/outline-enabled",
                            "Magnifier outline",
                            crate::gui::jump_mode::TargetKind::Checkbox,
                            &mut visual_order,
                        );
                        let mut zoom_outline_rgb = [
                            self.draft.zoom.outline_color.red,
                            self.draft.zoom.outline_color.green,
                            self.draft.zoom.outline_color.blue,
                        ];
                        let zoom_outline_color_row = crate::gui::theme::widgets::labeled_form_row(
                            ui,
                            "Outline RGB",
                            |ui| {
                                let color = ui.color_edit_button_srgb(&mut zoom_outline_rgb);
                                register_mouse_settings_target(
                                    scope,
                                    ui,
                                    &color,
                                    "magnifier/outline-color",
                                    "Magnifier outline color",
                                    crate::gui::jump_mode::TargetKind::Button,
                                    &mut visual_order,
                                );
                                color
                            },
                        );
                        if zoom_outline_color_row.changed() {
                            self.draft.zoom.outline_color = CrosshairColor::new(
                                zoom_outline_rgb[0],
                                zoom_outline_rgb[1],
                                zoom_outline_rgb[2],
                            );
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Thickness (px)");
                            let outline_thickness = ui.add(
                                egui::DragValue::new(&mut self.draft.zoom.outline_thickness)
                                    .range(1..=8),
                            );
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &outline_thickness,
                                "magnifier/outline-thickness",
                                "Magnifier outline thickness",
                                crate::gui::jump_mode::TargetKind::TextEntry,
                                &mut visual_order,
                            );
                        });

                        ui.separator();
                        compact_section_heading(ui, "Coordinate Display", None);
                        let mut hud_enabled = runtime.hud_enabled();
                        let hud_enabled_response = ui
                            .push_id("mouse_settings_hud_enabled", |ui| {
                                ui.checkbox(&mut hud_enabled, "Enabled for this session")
                            })
                            .inner
                            ;
                        register_mouse_settings_target(
                            scope,
                            ui,
                            &hud_enabled_response,
                            "hud/session-enabled",
                            "HUD session switch",
                            crate::gui::jump_mode::TargetKind::Checkbox,
                            &mut visual_order,
                        );
                        if hud_enabled_response.changed()
                            && authorized_owner_pass
                            && let Err(error) =
                                adapter.execute(&CoordinateToolCommand::SetHudEnabled(hud_enabled))
                        {
                            self.last_error = Some(error);
                        }

                        ui.horizontal_wrapped(|ui| {
                            ui.label("Coordinate space");
                            let popup_scope = scope.map(|scope| {
                                scope.combo_popup_scope("hud/coordinate-space")
                            });
                            let combo = egui::ComboBox::from_id_salt("mouse_settings_coordinate_space")
                                .selected_text(space_label(self.draft.space))
                                .show_ui(ui, |ui| {
                                    let desktop = ui.selectable_value(
                                        &mut self.draft.space,
                                        CoordinateSpace::Desktop,
                                        "Desktop",
                                    );
                                    register_mouse_settings_target(
                                        popup_scope.as_ref(),
                                        ui,
                                        &desktop,
                                        "space/desktop",
                                        "Desktop",
                                        crate::gui::jump_mode::TargetKind::Selectable,
                                        &mut visual_order,
                                    );
                                    let monitor = ui.selectable_value(
                                        &mut self.draft.space,
                                        CoordinateSpace::Monitor,
                                        "Monitor",
                                    );
                                    register_mouse_settings_target(
                                        popup_scope.as_ref(),
                                        ui,
                                        &monitor,
                                        "space/monitor",
                                        "Monitor",
                                        crate::gui::jump_mode::TargetKind::Selectable,
                                        &mut visual_order,
                                    );
                                    let foreground = ui.selectable_value(
                                        &mut self.draft.space,
                                        CoordinateSpace::ForegroundClient,
                                        "Foreground client",
                                    );
                                    register_mouse_settings_target(
                                        popup_scope.as_ref(),
                                        ui,
                                        &foreground,
                                        "space/foreground-client",
                                        "Foreground client",
                                        crate::gui::jump_mode::TargetKind::Selectable,
                                        &mut visual_order,
                                    );
                                });
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &combo.response,
                                "hud/coordinate-space",
                                "Coordinate space",
                                crate::gui::jump_mode::TargetKind::ComboBox,
                                &mut visual_order,
                            );
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("HUD format");
                            let compact = ui.selectable_value(
                                &mut self.draft.hud_detail,
                                HudDetail::Compact,
                                "Compact",
                            );
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &compact,
                                "hud/detail-compact",
                                "Compact HUD",
                                crate::gui::jump_mode::TargetKind::Selectable,
                                &mut visual_order,
                            );
                            let detailed = ui.selectable_value(
                                &mut self.draft.hud_detail,
                                HudDetail::Detailed,
                                "Detailed",
                            );
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &detailed,
                                "hud/detail-detailed",
                                "Detailed HUD",
                                crate::gui::jump_mode::TargetKind::Selectable,
                                &mut visual_order,
                            );
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Cursor offset (physical pixels)");
                            let x_offset = ui.add(
                                egui::DragValue::new(&mut self.draft.cursor_offset.x)
                                    .range(-512..=512),
                            );
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &x_offset,
                                "hud/cursor-offset-x",
                                "HUD cursor offset X",
                                crate::gui::jump_mode::TargetKind::TextEntry,
                                &mut visual_order,
                            );
                            let y_offset = ui.add(
                                egui::DragValue::new(&mut self.draft.cursor_offset.y)
                                    .range(-512..=512),
                            );
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &y_offset,
                                "hud/cursor-offset-y",
                                "HUD cursor offset Y",
                                crate::gui::jump_mode::TargetKind::TextEntry,
                                &mut visual_order,
                            );
                        });

                        ui.separator();
                        ui.horizontal(|ui| {
                            let apply_button =
                                ui.add_enabled(self.is_dirty(), egui::Button::new("Apply"));
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &apply_button,
                                "form/apply",
                                "Apply appearance changes",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut visual_order,
                            );
                            if with_button_emphasis(ui, apply_button, ButtonEmphasis::Primary)
                                .clicked()
                                && authorized_owner_pass
                                && let Err(error) = self.apply_to(adapter)
                            {
                                self.last_error = Some(error);
                            }
                            let cancel = ui.button("Cancel");
                            #[cfg(test)]
                            {
                                cancel_response = Some(cancel.clone());
                            }
                            register_mouse_settings_target(
                                scope,
                                ui,
                                &cancel,
                                "form/cancel",
                                "Cancel appearance changes",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut visual_order,
                            );
                            if cancel.clicked() && authorized_owner_pass {
                                cancel_requested = true;
                            }
                        });
                        })
                })
                .inner
            });
        if authorized_owner_pass {
            self.open = open;
            if cancel_requested {
                self.cancel_draft(adapter.preferences());
            }
        } else {
            self.draft = draft_before_ui;
            self.last_error = error_before_ui;
            self.open = open_before_ui;
        }
        if !self.open
            && let Some(scope) = scope
        {
            scope.discard_registered_targets();
        }
        let scroll_output = output.and_then(|window| window.inner);
        #[cfg(test)]
        {
            self.last_cancel_response = cancel_response;
        }
        #[cfg(test)]
        if let Some(output) = &scroll_output {
            self.last_scroll_area_id = Some(output.id);
            self.last_scroll_max_offset_y =
                (output.content_size.y - output.inner_rect.height()).max(0.0);
        }
        scroll_output
    }
}

fn click_status_text(status: &ClickEffectStatus) -> String {
    match status {
        ClickEffectStatus::Disabled => "Disabled".into(),
        ClickEffectStatus::Idle => "Requested · idle".into(),
        ClickEffectStatus::Animating => "Animating".into(),
        ClickEffectStatus::Unavailable(reason) => {
            format!("Unavailable: {}", concise_status_reason(reason))
        }
    }
}

fn click_status_tone(status: &ClickEffectStatus) -> StatusTone {
    match status {
        ClickEffectStatus::Disabled | ClickEffectStatus::Idle => StatusTone::Info,
        ClickEffectStatus::Animating => StatusTone::Success,
        ClickEffectStatus::Unavailable(_) => StatusTone::Error,
    }
}

fn click_style_label(style: ClickEffectStyle) -> &'static str {
    match style {
        ClickEffectStyle::Ripple => "Ripple",
        ClickEffectStyle::Circle => "Circle",
        ClickEffectStyle::Flash => "Flash",
    }
}

fn applied_click_summary(preferences: &CoordinateToolPreferences) -> String {
    let click = preferences.click;
    format!(
        "Saved click appearance: {} · {} px · {:.0}% opacity · {} ms",
        click_style_label(click.style),
        click.size_px,
        click.opacity * 100.0,
        click.duration_ms
    )
}

fn effect_status_text(status: &CursorEffectStatus) -> String {
    match status {
        CursorEffectStatus::Disabled => "Disabled".into(),
        CursorEffectStatus::Prepared => "Prepared (hidden until presentation is ready)".into(),
        CursorEffectStatus::Active => "Active".into(),
        CursorEffectStatus::Fallback(reason) => format!(
            "Fallback: non-inverting outline — {}",
            concise_status_reason(reason)
        ),
        CursorEffectStatus::Paused => "Paused: waiting for a live cursor sample".into(),
        CursorEffectStatus::GeometryPaused(reason) => {
            format!("Paused: {}", concise_status_reason(reason))
        }
        CursorEffectStatus::Unavailable(reason) => {
            format!("Unavailable: {}", concise_status_reason(reason))
        }
    }
}

fn effect_status_tone(status: &CursorEffectStatus) -> StatusTone {
    match status {
        CursorEffectStatus::Disabled | CursorEffectStatus::Prepared => StatusTone::Info,
        CursorEffectStatus::Active => StatusTone::Success,
        CursorEffectStatus::Fallback(_)
        | CursorEffectStatus::Paused
        | CursorEffectStatus::GeometryPaused(_) => StatusTone::Warning,
        CursorEffectStatus::Unavailable(_) => StatusTone::Error,
    }
}

fn concise_status_reason(reason: &str) -> String {
    let mut chars = reason.chars();
    let concise: String = chars.by_ref().take(96).collect();
    if chars.next().is_some() {
        format!("{concise}…")
    } else {
        concise
    }
}

#[cfg(test)]
pub(crate) fn test_coordinate_tool_adapter(
    path: String,
    preferences: CoordinateToolPreferences,
) -> CoordinateToolGui {
    tests::fake_adapter(path, preferences)
}

fn applied_halo_summary(preferences: &CoordinateToolPreferences) -> String {
    format!(
        "Saved halo appearance: {:.0}% inversion · {} px radius",
        preferences.halo.inversion_strength * 100.0,
        preferences.halo.radius
    )
}

fn applied_zoom_summary(preferences: &CoordinateToolPreferences) -> String {
    format!(
        "Saved magnifier appearance: {:.2}× · {} px lens",
        preferences.zoom.zoom_factor, preferences.zoom.diameter
    )
}

fn zoom_mode_label(mode: ZoomMode) -> &'static str {
    match mode {
        ZoomMode::Offset => "Offset",
        ZoomMode::Centered => "Centered",
    }
}

fn space_label(space: CoordinateSpace) -> &'static str {
    match space {
        CoordinateSpace::Desktop => "Desktop",
        CoordinateSpace::Monitor => "Monitor",
        CoordinateSpace::ForegroundClient => "Foreground client",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::coordinate_tool::{
        ClickObservationLease, ClickObservationProvider, CoordinateRenderFrame,
        CoordinateRuntimeFactory, CoordinateSample, CoordinateSampler, CoordinateSurfaceBackend,
        PhysicalPoint,
    };

    struct TestSampler;

    impl CoordinateSampler for TestSampler {
        fn sample(&mut self) -> Result<CoordinateSample, String> {
            Ok(CoordinateSample::new(
                PhysicalPoint::new(10, 20),
                None,
                None,
                None,
            ))
        }
    }

    struct TestBackend;

    impl CoordinateSurfaceBackend for TestBackend {
        fn render(&mut self, _frame: &CoordinateRenderFrame) -> Result<(), String> {
            Ok(())
        }

        fn shutdown(&mut self) -> Result<(), String> {
            Ok(())
        }
    }

    struct TestFactory;

    impl CoordinateRuntimeFactory for TestFactory {
        fn create_sampler(&self) -> Result<Box<dyn CoordinateSampler>, String> {
            Ok(Box::new(TestSampler))
        }

        fn create_backend(&self) -> Result<Box<dyn CoordinateSurfaceBackend>, String> {
            Ok(Box::new(TestBackend))
        }
    }

    struct TestClickLease;

    impl ClickObservationLease for TestClickLease {}

    struct TestClickObservation;

    impl ClickObservationProvider for TestClickObservation {
        fn acquire(
            &self,
            _generation: u64,
            _sink: Arc<dyn crate::coordinate_tool::ClickPressSink>,
        ) -> Result<Box<dyn ClickObservationLease>, String> {
            Ok(Box::new(TestClickLease))
        }
    }

    struct TestClipboard;

    impl crate::gui::coordinate_tool::CoordinateClipboardWriter for TestClipboard {
        fn set_text(&mut self, _text: &str) -> anyhow::Result<()> {
            Ok(())
        }
    }

    pub(super) fn fake_adapter(
        path: String,
        preferences: CoordinateToolPreferences,
    ) -> CoordinateToolGui {
        CoordinateToolGui::with_backends_and_click_observation(
            path,
            preferences,
            Arc::new(TestFactory),
            Box::new(TestClipboard),
            Arc::new(TestClickObservation),
        )
    }

    #[test]
    fn repeated_open_preserves_dirty_draft_and_a_new_open_refreshes_from_adapter_preferences() {
        let mut dialog = MouseSettingsDialog::default();
        let initial = CoordinateToolPreferences::default();
        dialog.open(&initial);
        dialog.draft.cursor_offset = CoordinateOffset::new(-42, 77);
        dialog.draft.halo.radius = 88;
        dialog.draft.zoom.zoom_factor = 3.25;

        let current = CoordinateToolPreferences {
            space: CoordinateSpace::Monitor,
            halo: HaloPreferences {
                radius: 120,
                ..initial.halo
            },
            zoom: ZoomPreferences {
                zoom_factor: 1.5,
                ..initial.zoom
            },
            ..initial.clone()
        };
        dialog.open(&current);
        assert!(dialog.open);
        assert!(dialog.is_dirty());
        assert_eq!(dialog.draft.cursor_offset, CoordinateOffset::new(-42, 77));
        assert_eq!(dialog.draft.halo.radius, 88);
        assert_eq!(dialog.draft.zoom.zoom_factor, 3.25);
        assert_eq!(dialog.draft.space, CoordinateSpace::Desktop);

        dialog.open = false;
        dialog.open(&current);
        assert!(!dialog.is_dirty());
        assert_eq!(dialog.draft, current);
    }

    #[test]
    fn failed_apply_keeps_the_draft_and_error_visible() {
        let directory = tempfile::tempdir().unwrap();
        let mut adapter = CoordinateToolGui::new(
            directory.path().to_string_lossy().into_owned(),
            CoordinateToolPreferences::default(),
        );
        let mut dialog = MouseSettingsDialog::default();
        dialog.open(adapter.preferences());
        dialog.draft.space = CoordinateSpace::ForegroundClient;
        dialog.draft.crosshair.center_gap = 0;
        dialog.draft.halo.radius = 80;
        dialog.draft.halo.inversion_strength = 0.8;
        dialog.draft.zoom.diameter = 240;
        dialog.draft.zoom.destination_offset.x = -400;
        dialog.draft.click.style = ClickEffectStyle::Flash;
        dialog.draft.click.duration_ms = 900;
        let draft = dialog.draft.clone();

        assert!(dialog.apply_to(&mut adapter).is_err());
        assert_eq!(dialog.draft, draft);
        assert!(dialog.is_dirty());
        assert!(dialog.last_error.is_some());
        assert_eq!(adapter.preferences(), &CoordinateToolPreferences::default());
        assert!(!adapter.is_running());
    }

    #[test]
    fn successful_appearance_apply_saves_and_reopens_cleanly() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("settings.json")
            .to_string_lossy()
            .into_owned();
        let mut adapter =
            CoordinateToolGui::new(path.clone(), CoordinateToolPreferences::default());
        let mut dialog = MouseSettingsDialog::default();
        dialog.open(adapter.preferences());
        dialog.draft.crosshair.center_gap = 128;
        dialog.draft.halo.radius = 96;
        dialog.draft.halo.inversion_strength = 0.65;
        dialog.draft.halo.outline_enabled = true;
        dialog.draft.halo.outline_color = CrosshairColor::new(12, 34, 56);
        dialog.draft.halo.outline_thickness = 4;
        dialog.draft.zoom.mode = ZoomMode::Centered;
        dialog.draft.zoom.zoom_factor = 3.5;
        dialog.draft.zoom.diameter = 320;
        dialog.draft.zoom.destination_offset = CoordinateOffset::new(-120, 220);
        dialog.draft.zoom.outline_enabled = false;
        dialog.draft.zoom.outline_color = CrosshairColor::new(200, 100, 40);
        dialog.draft.zoom.outline_thickness = 6;
        dialog.draft.click.style = ClickEffectStyle::Circle;
        dialog.draft.click.color = CrosshairColor::new(25, 120, 230);
        dialog.draft.click.size_px = 92;
        dialog.draft.click.opacity = 0.55;
        dialog.draft.click.duration_ms = 1_100;
        assert!(dialog.is_dirty());

        dialog.apply_to(&mut adapter).unwrap();
        assert_eq!(adapter.preferences().crosshair.center_gap, 128);
        assert_eq!(adapter.preferences().halo, dialog.draft.halo);
        assert_eq!(adapter.preferences().zoom, dialog.draft.zoom);
        assert_eq!(adapter.preferences().click, dialog.draft.click);
        assert_eq!(dialog.baseline.crosshair.center_gap, 128);
        assert_eq!(dialog.draft.crosshair.center_gap, 128);
        assert!(!dialog.is_dirty());
        assert_eq!(
            crate::settings::Settings::load(&path)
                .unwrap()
                .coordinate_tool
                .crosshair
                .center_gap,
            128
        );
        let saved = crate::settings::Settings::load(&path)
            .unwrap()
            .coordinate_tool;
        assert_eq!(saved.halo, dialog.draft.halo);
        assert_eq!(saved.zoom, dialog.draft.zoom);
        assert_eq!(saved.click, dialog.draft.click);

        dialog.open = false;
        dialog.open(adapter.preferences());
        assert_eq!(dialog.draft.crosshair.center_gap, 128);
        assert!(!dialog.is_dirty());
    }

    #[test]
    fn section_resets_change_only_the_draft_and_require_apply() {
        let initial = CoordinateToolPreferences {
            halo: HaloPreferences {
                radius: 100,
                ..HaloPreferences::default()
            },
            zoom: ZoomPreferences {
                diameter: 300,
                ..ZoomPreferences::default()
            },
            ..CoordinateToolPreferences::default()
        };
        let mut dialog = MouseSettingsDialog::default();
        dialog.open(&initial);
        dialog.draft.crosshair.center_gap = 24;
        dialog.draft.halo.radius = 140;
        dialog.draft.zoom.zoom_factor = 3.0;
        dialog.draft.click.size_px = 96;

        dialog.reset_halo_draft();
        assert_eq!(dialog.draft.halo, HaloPreferences::default());
        assert_eq!(dialog.draft.zoom.zoom_factor, 3.0);
        assert_eq!(dialog.draft.crosshair.center_gap, 24);
        assert!(dialog.is_dirty());

        dialog.reset_zoom_draft();
        assert_eq!(dialog.draft.zoom, ZoomPreferences::default());
        assert_eq!(dialog.draft.click.size_px, 96);
        dialog.reset_click_draft();
        assert_eq!(dialog.draft.click, ClickEffectPreferences::default());
        assert_eq!(dialog.draft.halo, HaloPreferences::default());
        assert_eq!(dialog.baseline, initial);
        assert!(dialog.is_dirty());
    }

    #[test]
    fn cancel_discards_click_draft_and_reopen_starts_clean() {
        let initial = CoordinateToolPreferences::default();
        let current = CoordinateToolPreferences {
            click: ClickEffectPreferences {
                style: ClickEffectStyle::Circle,
                size_px: 72,
                ..Default::default()
            },
            ..initial.clone()
        };
        let mut dialog = MouseSettingsDialog::default();
        dialog.open(&initial);
        dialog.draft.click.style = ClickEffectStyle::Flash;
        dialog.draft.click.size_px = 120;
        dialog.last_error = Some("injected save failure".into());
        assert!(dialog.is_dirty());

        dialog.cancel_draft(&current);
        assert!(!dialog.open);
        assert_eq!(dialog.draft.click, current.click);
        assert!(!dialog.is_dirty());
        assert!(dialog.last_error.is_none());

        dialog.open(&current);
        assert_eq!(dialog.draft, current);
        assert!(!dialog.is_dirty());
    }

    #[test]
    fn runtime_switches_are_immediate_and_do_not_save_or_change_the_draft() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("settings.json")
            .to_string_lossy()
            .into_owned();
        let initial = CoordinateToolPreferences::default();
        crate::settings::Settings::update(&path, |settings| {
            settings.coordinate_tool = initial.clone();
            Ok(())
        })
        .unwrap();
        let mut adapter = fake_adapter(path.clone(), initial.clone());
        let mut dialog = MouseSettingsDialog::default();
        dialog.open(&initial);
        dialog.draft.halo.radius = 128;
        dialog.draft.zoom.zoom_factor = 3.0;
        dialog.draft.click.duration_ms = 950;
        let draft = dialog.draft.clone();

        adapter.set_halo_enabled(true).unwrap();
        adapter.set_zoom_enabled(true).unwrap();
        adapter
            .execute(&CoordinateToolCommand::SetClickEnabled(true))
            .unwrap();

        let runtime = adapter.runtime_state();
        assert!(runtime.halo_enabled());
        assert!(runtime.zoom_enabled());
        assert!(runtime.click_enabled());
        assert_eq!(adapter.preferences(), &initial);
        assert_eq!(
            crate::settings::Settings::load(&path)
                .unwrap()
                .coordinate_tool,
            initial
        );
        assert_eq!(dialog.draft, draft);
        assert!(dialog.is_dirty());
        adapter.shutdown().unwrap();
    }

    #[test]
    fn effect_status_and_saved_summaries_are_truthful_and_use_committed_values() {
        assert_eq!(
            effect_status_text(&CursorEffectStatus::Disabled),
            "Disabled"
        );
        assert!(effect_status_text(&CursorEffectStatus::Prepared).contains("Prepared"));
        assert_eq!(effect_status_text(&CursorEffectStatus::Active), "Active");
        assert!(
            effect_status_text(&CursorEffectStatus::Fallback("API failed".into()))
                .contains("non-inverting outline")
        );
        assert!(effect_status_text(&CursorEffectStatus::Paused).contains("live cursor sample"));
        assert!(
            effect_status_text(&CursorEffectStatus::GeometryPaused("no coverage".into()))
                .contains("no coverage")
        );
        assert!(
            effect_status_text(&CursorEffectStatus::Unavailable("host failed".into()))
                .contains("host failed")
        );
        assert_eq!(click_status_text(&ClickEffectStatus::Disabled), "Disabled");
        assert_eq!(
            click_status_text(&ClickEffectStatus::Idle),
            "Requested · idle"
        );
        assert_eq!(
            click_status_text(&ClickEffectStatus::Animating),
            "Animating"
        );
        assert!(
            click_status_text(&ClickEffectStatus::Unavailable("surface failed".into()))
                .contains("surface failed")
        );

        let committed = CoordinateToolPreferences::default();
        let mut draft = committed.clone();
        draft.halo.radius = 200;
        draft.halo.inversion_strength = 0.9;
        draft.zoom.zoom_factor = 4.0;
        draft.zoom.diameter = 400;
        draft.click.style = ClickEffectStyle::Flash;
        draft.click.size_px = 96;
        draft.click.opacity = 0.6;
        draft.click.duration_ms = 700;
        assert!(
            applied_halo_summary(&committed)
                .contains("Saved halo appearance: 40% inversion · 60 px")
        );
        assert!(
            applied_zoom_summary(&committed).contains("Saved magnifier appearance: 2.00× · 160 px")
        );
        assert!(applied_halo_summary(&draft).contains("90% inversion · 200 px"));
        assert!(applied_zoom_summary(&draft).contains("4.00× · 400 px"));
        assert_eq!(click_style_label(draft.click.style), "Flash");
        assert!(
            applied_click_summary(&committed).contains("Saved click appearance: Ripple · 32 px")
        );
        assert!(applied_click_summary(&draft).contains("Flash · 96 px · 60% opacity · 700 ms"));
    }

    #[test]
    fn zoom_mode_control_labels_both_destination_modes() {
        assert_eq!(zoom_mode_label(ZoomMode::Offset), "Offset");
        assert_eq!(zoom_mode_label(ZoomMode::Centered), "Centered");
    }

    #[test]
    fn compact_viewport_places_the_full_form_in_a_scrollable_area() {
        let directory = tempfile::tempdir().unwrap();
        let mut adapter = CoordinateToolGui::new(
            directory
                .path()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
            CoordinateToolPreferences::default(),
        );
        let mut dialog = MouseSettingsDialog::default();
        dialog.open(adapter.preferences());
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 220.0),
            )),
            ..Default::default()
        };
        let mut scroll = None;
        ctx.run_ui(input, |ctx| {
            scroll = dialog.show_ui(ctx, &mut adapter);
        })
        .drop_without_applying_deltas();

        let scroll = scroll.expect("open settings window should render its form");
        assert!(
            scroll.content_size.y > scroll.inner_rect.height(),
            "the complete settings form should extend beyond the compact viewport"
        );
        assert!(
            scroll.content_size.x <= scroll.inner_rect.width() + 1.0,
            "the complete settings form should not overflow horizontally in a compact viewport"
        );
        assert!(scroll.inner_rect.height() <= 220.0);
    }
}
