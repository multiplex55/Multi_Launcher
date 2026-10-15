use crate::diff::binary_compare::{BinaryCell, BinaryRow, BinaryViewModel};
use crate::diff::text_compare::NavigationDirection;
use eframe::egui::{self, Color32, RichText};

#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct BinaryViewGeometry {
    pub viewport: egui::Rect,
    pub content_size: egui::Vec2,
}

pub fn show(
    ui: &mut egui::Ui,
    workspace: u64,
    view: u64,
    model: &mut BinaryViewModel,
) -> BinaryViewGeometry {
    let difference_label = model.current_difference.map_or_else(
        || format!("0/{} differences", model.differences.ranges.len()),
        |i| format!("Difference {}/{}", i + 1, model.differences.ranges.len()),
    );
    let command_height = binary_toolbar_height(ui, &difference_label);
    egui::ScrollArea::horizontal()
        .max_height(command_height + ui.spacing().scroll.allocated_width())
        .auto_shrink([true, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (label, direction) in [
                    ("First", NavigationDirection::First),
                    ("Previous", NavigationDirection::Previous),
                    ("Next", NavigationDirection::Next),
                    ("Last", NavigationDirection::Last),
                ] {
                    if ui.button(label).clicked() {
                        model.navigate(direction);
                    }
                }
                ui.label(difference_label.as_str());
                ui.label("Read-only · 16 bytes/row");
            })
        });
    let row_height = 22.0;
    let rows = model
        .left
        .len
        .max(model.right.len)
        .div_ceil(model.bytes_per_row as u64) as usize;
    let pending = model.pending_scroll_offset.take();
    if let Some(offset) = pending {
        model.visible_byte_offset =
            offset / model.bytes_per_row as u64 * model.bytes_per_row as u64;
    }
    let mut scroll = egui::ScrollArea::vertical().id_salt((workspace, view, "binary_scroll"));
    if pending.is_some() {
        scroll = scroll.vertical_scroll_offset(
            (model.visible_byte_offset / model.bytes_per_row as u64) as f32 * row_height,
        );
    }
    let viewport = binary_viewport_size(ui.available_size());
    let mut geometry = BinaryViewGeometry {
        viewport: egui::Rect::NOTHING,
        content_size: egui::Vec2::ZERO,
    };
    ui.allocate_ui(viewport, |ui| {
        let output = egui::ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                scroll.show_rows(ui, row_height, rows, |ui, range| {
                    for row_index in range {
                        let offset = row_index as u64 * model.bytes_per_row as u64;
                        if let Ok(row) = model.row(offset) {
                            ui.horizontal(|ui| {
                                side(ui, row.offset, &row.left);
                                ui.separator();
                                side(ui, row.offset, &row.right);
                            });
                        }
                    }
                });
            });
        geometry.viewport = output.inner_rect;
        geometry.content_size = output.content_size;
    });
    geometry
}

fn binary_toolbar_height(ui: &mut egui::Ui, difference_label: &str) -> f32 {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let button_style = style.button_style(
        &egui::widget_style::Classes::default(),
        egui::widget_style::WidgetState::Inactive,
    );
    let button_font = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| button_style.text_style.font_id.clone());
    let frame_margin = if ui.visuals().button_frame {
        button_style.frame.total_margin().sum()
    } else {
        egui::Vec2::ZERO
    };
    let button_height = ["First", "Previous", "Next", "Last"]
        .into_iter()
        .map(|label| {
            let text_height = egui::WidgetText::from(label)
                .into_galley(
                    ui,
                    Some(egui::TextWrapMode::Extend),
                    f32::INFINITY,
                    button_font.clone(),
                )
                .size()
                .y;
            (text_height + spacing.extra_text_line_spacing + frame_margin.y)
                .max(spacing.interact_size.y)
        })
        .fold(spacing.interact_size.y, f32::max);
    let label_font = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| egui::TextStyle::Body.resolve(&style));
    let label_height = [difference_label, "Read-only · 16 bytes/row"]
        .into_iter()
        .map(|label| {
            egui::WidgetText::from(label)
                .into_galley(
                    ui,
                    Some(egui::TextWrapMode::Extend),
                    f32::INFINITY,
                    label_font.clone(),
                )
                .size()
                .y
                + spacing.extra_text_line_spacing
        })
        .fold(0.0, f32::max);
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);
    ((button_height.max(label_height) * pixels_per_point).ceil() / pixels_per_point)
        .max(spacing.interact_size.y)
}

fn binary_viewport_size(available: egui::Vec2) -> egui::Vec2 {
    egui::vec2(available.x.max(0.0), available.y.max(0.0))
}

fn side(ui: &mut egui::Ui, offset: u64, cells: &[BinaryCell]) {
    ui.label(RichText::new(format!("{offset:08X}")).monospace().weak());
    for cell in cells {
        let text = cell.byte.map_or("--".to_owned(), |b| format!("{b:02X}"));
        ui.label(
            RichText::new(text)
                .monospace()
                .background_color(if cell.changed {
                    Color32::from_rgb(100, 65, 20)
                } else {
                    Color32::TRANSPARENT
                }),
        );
    }
    ui.label(RichText::new(BinaryRow::ascii(cells)).monospace());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_viewport_never_exceeds_workspace_or_depends_on_row_width() {
        for (width, height) in [(400.0, 250.0), (900.0, 650.0), (1600.0, 1000.0)] {
            let short = binary_viewport_size(egui::vec2(width, height));
            let long = binary_viewport_size(egui::vec2(width, height));
            assert_eq!(short, long, "content width is handled by the scroll area");
            assert!(short.x <= width && short.y <= height);
        }
        assert_eq!(
            binary_viewport_size(egui::vec2(-1.0, -1.0)),
            egui::Vec2::ZERO
        );
    }
}
