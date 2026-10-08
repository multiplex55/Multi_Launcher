use crate::commands::CoordinateToolCommand;
use crate::coordinate_tool::{
    CoordinateOffset, CoordinateSpace, CoordinateToolPreferences, CrosshairColor, HudDetail,
};
use crate::gui::coordinate_tool::CoordinateToolGui;
use eframe::egui;

#[derive(Default)]
pub(crate) struct MouseSettingsDialog {
    pub(crate) open: bool,
    baseline: CoordinateToolPreferences,
    draft: CoordinateToolPreferences,
    last_error: Option<String>,
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

    fn reset_from(&mut self, current: &CoordinateToolPreferences) {
        self.sync_clean_draft(current);
        self.last_error = None;
    }

    fn sync_clean_draft(&mut self, current: &CoordinateToolPreferences) {
        self.baseline = current.clone();
        self.draft = current.clone();
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
        let _ = self.show_ui(ctx, adapter);
    }

    fn show_ui(
        &mut self,
        ctx: &egui::Context,
        adapter: &mut CoordinateToolGui,
    ) -> Option<egui::containers::scroll_area::ScrollAreaOutput<()>> {
        if !self.open {
            return None;
        }
        if !self.is_dirty() {
            self.sync_clean_draft(adapter.preferences());
        }

        let runtime = adapter.runtime_state();
        let mut open = self.open;
        let output = egui::Window::new("Mouse Settings")
            .open(&mut open)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_source("mouse_settings_form")
                    .max_height(ui.available_height().max(1.0))
                    .show(ui, |ui| {
                ui.label("Configure the coordinate display and crosshair.");
                ui.small("Enabled states are temporary. Apply saves display options and updates active overlays.");
                ui.small("Commands: mouse coords …, mouse crosshair …, and mouse help.");
                ui.separator();

                if let Some(error) = &self.last_error {
                    ui.colored_label(egui::Color32::RED, error);
                    ui.separator();
                }

                ui.heading("Crosshair");
                let mut crosshair_enabled = runtime.crosshair_enabled();
                if ui
                    .checkbox(&mut crosshair_enabled, "Enabled for this session")
                    .changed()
                    && let Err(error) = adapter.execute(
                        &CoordinateToolCommand::SetCrosshairEnabled(crosshair_enabled),
                    )
                {
                    self.last_error = Some(error);
                }

                ui.horizontal(|ui| {
                    ui.label("RGB color");
                    let mut rgb = [
                        self.draft.crosshair.color.red,
                        self.draft.crosshair.color.green,
                        self.draft.crosshair.color.blue,
                    ];
                    if ui.color_edit_button_srgb(&mut rgb).changed() {
                        self.draft.crosshair.color = CrosshairColor::new(rgb[0], rgb[1], rgb[2]);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Thickness");
                    ui.add(
                        egui::DragValue::new(&mut self.draft.crosshair.thickness)
                            .clamp_range(1..=16),
                    );
                    ui.label("Arm length");
                    ui.add(
                        egui::DragValue::new(&mut self.draft.crosshair.arm_length)
                            .clamp_range(2..=256),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Opacity");
                    ui.add(
                        egui::Slider::new(&mut self.draft.crosshair.opacity, 0.1..=1.0)
                            .show_value(true),
                    );
                });
                ui.checkbox(
                    &mut self.draft.crosshair.virtual_desktop_guides,
                    "Show virtual-desktop guide lines",
                );
                ui.checkbox(
                    &mut self.draft.crosshair.high_contrast_outline,
                    "High-contrast outline",
                );

                ui.separator();
                ui.heading("Coordinate Display");
                let mut hud_enabled = runtime.hud_enabled();
                if ui
                    .checkbox(&mut hud_enabled, "Enabled for this session")
                    .changed()
                    && let Err(error) =
                        adapter.execute(&CoordinateToolCommand::SetHudEnabled(hud_enabled))
                {
                    self.last_error = Some(error);
                }

                ui.horizontal(|ui| {
                    ui.label("Coordinate space");
                    egui::ComboBox::from_id_source("mouse_settings_coordinate_space")
                        .selected_text(space_label(self.draft.space))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.draft.space,
                                CoordinateSpace::Desktop,
                                "Desktop",
                            );
                            ui.selectable_value(
                                &mut self.draft.space,
                                CoordinateSpace::Monitor,
                                "Monitor",
                            );
                            ui.selectable_value(
                                &mut self.draft.space,
                                CoordinateSpace::ForegroundClient,
                                "Foreground client",
                            );
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("HUD format");
                    ui.selectable_value(&mut self.draft.hud_detail, HudDetail::Compact, "Compact");
                    ui.selectable_value(
                        &mut self.draft.hud_detail,
                        HudDetail::Detailed,
                        "Detailed",
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Cursor offset (physical pixels)");
                    ui.add(
                        egui::DragValue::new(&mut self.draft.cursor_offset.x)
                            .clamp_range(-512..=512),
                    );
                    ui.add(
                        egui::DragValue::new(&mut self.draft.cursor_offset.y)
                            .clamp_range(-512..=512),
                    );
                });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.is_dirty(), egui::Button::new("Apply"))
                        .clicked()
                        && let Err(error) = self.apply_to(adapter)
                    {
                        self.last_error = Some(error);
                    }
                });
                    })
            });
        self.open = open;
        output.and_then(|window| window.inner)
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
    use super::*;

    #[test]
    fn repeated_open_preserves_dirty_draft_and_a_new_open_refreshes_from_adapter_preferences() {
        let mut dialog = MouseSettingsDialog::default();
        let initial = CoordinateToolPreferences::default();
        dialog.open(&initial);
        dialog.draft.cursor_offset = CoordinateOffset::new(-42, 77);

        let current = CoordinateToolPreferences {
            space: CoordinateSpace::Monitor,
            ..initial.clone()
        };
        dialog.open(&current);
        assert!(dialog.open);
        assert!(dialog.is_dirty());
        assert_eq!(dialog.draft.cursor_offset, CoordinateOffset::new(-42, 77));
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
        let draft = dialog.draft.clone();

        assert!(dialog.apply_to(&mut adapter).is_err());
        assert_eq!(dialog.draft, draft);
        assert!(dialog.is_dirty());
        assert!(dialog.last_error.is_some());
        assert_eq!(adapter.preferences(), &CoordinateToolPreferences::default());
        assert!(!adapter.is_running());
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
        let _ = ctx.run(input, |ctx| {
            scroll = dialog.show_ui(ctx, &mut adapter);
        });

        let scroll = scroll.expect("open settings window should render its form");
        assert!(
            scroll.content_size.y > scroll.inner_rect.height(),
            "the complete settings form should extend beyond the compact viewport"
        );
        assert!(scroll.inner_rect.height() <= 220.0);
    }
}
