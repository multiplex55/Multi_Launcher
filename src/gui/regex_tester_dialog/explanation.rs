//! Read-only presentation of cached domain syntax descriptions.

use crate::regex_tester::ExplanationResult;
use eframe::egui;

pub(super) fn show(ui: &mut egui::Ui, result: &ExplanationResult) {
    egui::ScrollArea::vertical()
        .id_source("regex_explanation_scroll")
        .auto_shrink([false, false])
        .max_height(ui.available_height().max(1.0))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width());
            ui.style_mut().wrap = Some(true);
            match result {
                ExplanationResult::InvalidPattern(error) => {
                    ui.colored_label(ui.visuals().error_fg_color, &error.message);
                }
                ExplanationResult::Success { explanations } => {
                    if explanations.is_empty() {
                        ui.weak("The empty pattern has no syntax tokens.");
                    }
                    for (index, entry) in explanations.iter().enumerate() {
                        ui.push_id(("regex_explanation_token", index), |ui| {
                            ui.strong(entry.label);
                            ui.label(egui::RichText::new(&entry.token).monospace());
                            ui.weak(format!(
                                "Original pattern UTF-8 bytes {}..{} (end exclusive)",
                                entry.span.start_byte(),
                                entry.span.end_byte()
                            ));
                            ui.label(&entry.description);
                            ui.separator();
                        });
                    }
                }
            }
        });
}
