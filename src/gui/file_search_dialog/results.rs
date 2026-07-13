use eframe::egui::{self, WidgetText};

pub(super) fn non_wrapping_selectable_label(
    ui: &mut egui::Ui,
    selected: bool,
    text: impl Into<WidgetText>,
) -> egui::Response {
    let text = text.into();
    let button_padding = ui.spacing().button_padding;
    let total_extra = button_padding + button_padding;
    let galley = text.into_galley(ui, Some(false), f32::INFINITY, egui::TextStyle::Button);
    let mut desired_size = total_extra + galley.size();
    desired_size.y = desired_size.y.max(ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_at_least(desired_size, egui::Sense::click());

    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, selected, galley.text())
    });

    if ui.is_rect_visible(response.rect) {
        let text_pos = ui
            .layout()
            .align_size_within_rect(galley.size(), rect.shrink2(button_padding))
            .min;
        let visuals = ui.style().interact_selectable(&response, selected);
        if selected || response.hovered() || response.highlighted() || response.has_focus() {
            let rect = rect.expand(visuals.expansion);
            ui.painter().rect(
                rect,
                visuals.rounding,
                visuals.weak_bg_fill,
                visuals.bg_stroke,
            );
        }
        ui.painter().galley(text_pos, galley, visuals.text_color());
    }

    response
}
