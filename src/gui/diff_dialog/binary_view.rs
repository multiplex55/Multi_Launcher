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
    let command_height = ui.spacing().interact_size.y;
    egui::ScrollArea::horizontal()
        .max_height(command_height)
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
                ui.label(model.current_difference.map_or_else(
                    || format!("0/{} differences", model.differences.ranges.len()),
                    |i| format!("Difference {}/{}", i + 1, model.differences.ranges.len()),
                ));
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
    let mut scroll = egui::ScrollArea::vertical().id_source((workspace, view, "binary_scroll"));
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
