use crate::draw::settings::{
    DrawColor, DrawSettings, DrawTool, LiveBackgroundMode, ToolbarPosition,
};
use eframe::egui;

pub struct DrawSettingsFormResult {
    pub changed: bool,
    pub toolbar_hotkey_error: Option<String>,
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
            "Drop intermediate move points when render falls behind",
        )
        .changed();

    changed |= ui
        .checkbox(
            &mut settings.offer_save_without_desktop,
            "Offer save without desktop capture",
        )
        .changed();

    ui.horizontal(|ui| {
        ui.label("Fixed save folder");
        ui.add_enabled(
            false,
            egui::TextEdit::singleline(&mut settings.fixed_save_folder_display),
        );
    });

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
    let is_transparent = matches!(
        settings.live_background_mode,
        LiveBackgroundMode::Transparent
    );
    if ui
        .radio(is_transparent, "Draw on desktop (transparent overlay)")
        .changed()
        && !is_transparent
    {
        settings.live_background_mode = LiveBackgroundMode::Transparent;
        changed = true;
    }

    let mut blank_color = match settings.live_background_mode {
        LiveBackgroundMode::Blank { color } => color,
        LiveBackgroundMode::Transparent => DrawColor::rgba(15, 18, 24, 255),
    };
    let is_blank = matches!(
        settings.live_background_mode,
        LiveBackgroundMode::Blank { .. }
    );
    if ui
        .radio(is_blank, "Draw on blank canvas (solid background)")
        .changed()
        && !is_blank
    {
        settings.live_background_mode = LiveBackgroundMode::Blank { color: blank_color };
        changed = true;
    }

    if matches!(
        settings.live_background_mode,
        LiveBackgroundMode::Blank { .. }
    ) {
        changed |= edit_color(ui, "Live blank color", &mut blank_color);
        settings.live_background_mode = LiveBackgroundMode::Blank { color: blank_color };
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
    }
}
