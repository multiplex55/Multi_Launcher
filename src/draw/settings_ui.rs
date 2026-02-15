use crate::draw::save::validate_fixed_save_folder_display;
use crate::draw::settings::{
    CanvasBackgroundMode, DrawColor, DrawSettings, DrawTool, ToolbarPosition,
};
use eframe::egui;
use rfd::FileDialog;

pub struct DrawSettingsFormResult {
    pub changed: bool,
    pub toolbar_hotkey_error: Option<String>,
    pub fixed_save_folder_error: Option<String>,
}

#[cfg(test)]
fn select_canvas_background_mode(
    current: &mut CanvasBackgroundMode,
    next: CanvasBackgroundMode,
) -> bool {
    if *current == next {
        false
    } else {
        *current = next;
        true
    }
}

pub fn render_draw_settings_form(
    ui: &mut egui::Ui,
    settings: &mut DrawSettings,
    id_source: &str,
) -> DrawSettingsFormResult {
    let mut changed = false;

    changed |= ui
        .checkbox(&mut settings.enable_pressure, "Enable pressure sensitivity")
        .changed();
    changed |= ui
        .checkbox(
            &mut settings.toolbar_collapsed,
            "Start with toolbar collapsed",
        )
        .changed();

    ui.horizontal(|ui| {
        ui.label("Toolbar position");
        egui::ComboBox::from_id_source(format!("{id_source}_toolbar_position"))
            .selected_text(format!("{:?}", settings.toolbar_position))
            .show_ui(ui, |ui| {
                changed |= ui
                    .selectable_value(&mut settings.toolbar_position, ToolbarPosition::Top, "Top")
                    .changed();
                changed |= ui
                    .selectable_value(
                        &mut settings.toolbar_position,
                        ToolbarPosition::Bottom,
                        "Bottom",
                    )
                    .changed();
                changed |= ui
                    .selectable_value(
                        &mut settings.toolbar_position,
                        ToolbarPosition::Left,
                        "Left",
                    )
                    .changed();
                changed |= ui
                    .selectable_value(
                        &mut settings.toolbar_position,
                        ToolbarPosition::Right,
                        "Right",
                    )
                    .changed();
            });
    });

    let mut toolbar_hotkey_error = None;
    ui.horizontal(|ui| {
        ui.label("Toolbar toggle hotkey");
        changed |= ui
            .text_edit_singleline(&mut settings.toolbar_toggle_hotkey)
            .changed();
    });
    if let Err(error) = settings.parse_toolbar_toggle_hotkey() {
        toolbar_hotkey_error = Some(error);
        ui.colored_label(
            egui::Color32::RED,
            "Invalid hotkey format (example: Ctrl+Shift+D)",
        );
    } else {
        ui.colored_label(egui::Color32::GREEN, "Hotkey is valid.");
    }

    ui.horizontal(|ui| {
        ui.label("Last tool");
        egui::ComboBox::from_id_source(format!("{id_source}_last_tool"))
            .selected_text(format!("{:?}", settings.last_tool))
            .show_ui(ui, |ui| {
                changed |= ui
                    .selectable_value(&mut settings.last_tool, DrawTool::Pen, "Pen")
                    .changed();
                changed |= ui
                    .selectable_value(&mut settings.last_tool, DrawTool::Line, "Line")
                    .changed();
                changed |= ui
                    .selectable_value(&mut settings.last_tool, DrawTool::Rect, "Rectangle")
                    .changed();
                changed |= ui
                    .selectable_value(&mut settings.last_tool, DrawTool::Ellipse, "Ellipse")
                    .changed();
                changed |= ui
                    .selectable_value(&mut settings.last_tool, DrawTool::Eraser, "Eraser")
                    .changed();
            });
    });

    ui.horizontal(|ui| {
        ui.label("Last width");
        changed |= ui
            .add(egui::DragValue::new(&mut settings.last_width).clamp_range(1..=128))
            .changed();
    });

    ui.horizontal(|ui| {
        ui.label("Exit timeout (seconds)");
        changed |= ui
            .add(egui::DragValue::new(&mut settings.exit_timeout_seconds).clamp_range(5..=3600))
            .changed();
    });

    ui.horizontal(|ui| {
        ui.label("Render target Hz");
        changed |= ui
            .add(egui::DragValue::new(&mut settings.render_target_hz).clamp_range(30..=240))
            .changed();
        ui.label("Fallback Hz");
        changed |= ui
            .add(egui::DragValue::new(&mut settings.render_fallback_hz).clamp_range(30..=120))
            .changed();
    });
    changed |= ui
        .checkbox(
            &mut settings.drop_intermediate_move_points_on_lag,
            "Enable stricter move sampling when render falls behind",
        )
        .changed();

    ui.horizontal(|ui| {
        ui.label("Move samples / frame");
        changed |= ui
            .add(
                egui::DragValue::new(&mut settings.sampling.move_samples_per_frame)
                    .clamp_range(1..=32),
            )
            .changed();
        ui.label("Lag");
        changed |= ui
            .add(
                egui::DragValue::new(&mut settings.sampling.lag_move_samples_per_frame)
                    .clamp_range(1..=32),
            )
            .changed();
        ui.label("Target Hz");
        changed |= ui
            .add(
                egui::DragValue::new(&mut settings.sampling.move_samples_target_hz)
                    .clamp_range(30..=240),
            )
            .changed();
    });
    ui.horizontal(|ui| {
        ui.label("Move max gap px");
        changed |= ui
            .add(
                egui::DragValue::new(&mut settings.sampling.move_sample_min_gap_px)
                    .clamp_range(1..=128),
            )
            .changed();
        ui.label("Stroke width multiplier");
        changed |= ui
            .add(
                egui::DragValue::new(
                    &mut settings
                        .sampling
                        .move_sample_max_gap_stroke_width_multiplier,
                )
                .clamp_range(1..=16),
            )
            .changed();
    });

    changed |= ui
        .checkbox(
            &mut settings.offer_save_without_desktop,
            "Offer save without desktop capture",
        )
        .changed();

    let mut fixed_save_folder_error = None;
    ui.horizontal(|ui| {
        ui.label("Fixed save folder");
        changed |= ui
            .add(
                egui::TextEdit::singleline(&mut settings.fixed_save_folder_display)
                    .desired_width(320.0),
            )
            .changed();
        if ui.button("Browse…").clicked() {
            if let Some(path) = FileDialog::new().pick_folder() {
                let selected = path.to_string_lossy().to_string();
                if settings.fixed_save_folder_display != selected {
                    settings.fixed_save_folder_display = selected;
                    changed = true;
                }
            }
        }
    });
    if let Err(error) = validate_fixed_save_folder_display(&settings.fixed_save_folder_display) {
        fixed_save_folder_error = Some(error.to_string());
        ui.colored_label(egui::Color32::RED, error.to_string());
    }

    ui.separator();
    ui.label("Colors");

    fn edit_color(ui: &mut egui::Ui, label: &str, color: &mut DrawColor) -> bool {
        let mut color_changed = false;
        ui.horizontal(|ui| {
            ui.label(label);
            let mut rgba = color.to_rgba_array();
            color_changed = ui.color_edit_button_srgba_unmultiplied(&mut rgba).changed();
            if color_changed {
                *color = DrawColor::from_rgba_array(rgba);
            }
        });
        color_changed
    }

    changed |= edit_color(ui, "Last color", &mut settings.last_color);
    changed |= edit_color(ui, "Default outline", &mut settings.default_outline_color);
    changed |= ui
        .checkbox(&mut settings.default_fill_enabled, "Default fill enabled")
        .changed();
    changed |= edit_color(ui, "Default fill", &mut settings.default_fill_color);
    ui.separator();
    ui.label("Live drawing background");
    changed |= ui
        .radio_value(
            &mut settings.canvas_background_mode,
            CanvasBackgroundMode::Transparent,
            "Draw on desktop (transparent overlay)",
        )
        .changed();

    let mut blank_color = settings.canvas_solid_background_color;
    changed |= ui
        .radio_value(
            &mut settings.canvas_background_mode,
            CanvasBackgroundMode::Solid,
            "Draw on blank canvas (solid background)",
        )
        .changed();

    if matches!(settings.canvas_background_mode, CanvasBackgroundMode::Solid) {
        changed |= edit_color(ui, "Live blank color", &mut blank_color);
        settings.canvas_solid_background_color = blank_color;
    }

    ui.separator();
    changed |= edit_color(
        ui,
        "Export blank background",
        &mut settings.export_blank_background_color,
    );

    ui.label("Quick colors");
    for (index, color) in settings.quick_colors.iter_mut().enumerate() {
        changed |= edit_color(ui, &format!("Quick #{index}"), color);
    }

    DrawSettingsFormResult {
        changed,
        toolbar_hotkey_error,
        fixed_save_folder_error,
    }
}

#[cfg(test)]
mod tests {
    use super::select_canvas_background_mode;
    use crate::draw::settings::CanvasBackgroundMode;

    #[test]
    fn blank_canvas_mode_can_be_selected_from_transparent_state() {
        let mut mode = CanvasBackgroundMode::Transparent;
        let changed = select_canvas_background_mode(&mut mode, CanvasBackgroundMode::Solid);

        assert!(changed);
        assert_eq!(mode, CanvasBackgroundMode::Solid);
    }
}
