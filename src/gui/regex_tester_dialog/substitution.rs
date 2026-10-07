//! Bounded presentation of the session's complete local replacement result.

use crate::regex_tester::{RegexSession, SubstitutionEvaluationResult};
use eframe::egui;
use std::time::Instant;

pub(super) const RESULT_PREVIEW_BYTES: usize = 16 * 1024;

pub(super) fn preview(output: &str) -> &str {
    super::utf8_prefix(output, RESULT_PREVIEW_BYTES)
}

/// Returns true for actual replacement/mode changes, never for result rendering.
pub(super) fn show(ui: &mut egui::Ui, session: &mut RegexSession) -> bool {
    let mut changed = false;
    egui::ScrollArea::vertical()
        .id_source("regex_substitution_scroll")
        .auto_shrink([false, false])
        .max_height(ui.available_height().max(1.0))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width());
            ui.style_mut().wrap = Some(true);
            let mut enabled = session.substitution_enabled();
            if ui
                .checkbox(&mut enabled, "Enable substitution preview")
                .changed()
            {
                session.set_substitution_enabled(enabled, Instant::now());
                changed = true;
            }
            ui.label("Replacement");
            ui.weak("Rust replacement syntax: $1, ${name}, and $$ for a literal dollar.");
            if session.draft.replacement.len() > session.policy().replacement_bytes {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!(
                        "Replacement retained ({} bytes); exceeds the {} byte interactive limit.",
                        session.draft.replacement.len(),
                        session.policy().replacement_bytes
                    ),
                );
                ui.label(
                    egui::RichText::new(super::utf8_prefix(&session.draft.replacement, 128))
                        .monospace(),
                );
                if ui.button("Clear replacement").clicked() {
                    session.draft.replacement.clear();
                    session.mark_replacement_changed(Instant::now());
                    changed = true;
                }
            } else if ui
                .add(
                    egui::TextEdit::multiline(&mut session.draft.replacement)
                        .id_source("regex_substitution_replacement")
                        .font(egui::TextStyle::Monospace)
                        .desired_width(ui.available_width())
                        .desired_rows(3),
                )
                .changed()
            {
                session.mark_replacement_changed(Instant::now());
                changed = true;
            }
            ui.separator();
            ui.strong("Replacement result");
            if !session.substitution_enabled() {
                ui.weak(
                    "Enable substitution to preview changes. Input remains editable and unchanged.",
                );
                return;
            }
            match session.substitution_result() {
                Some(SubstitutionEvaluationResult::Success(result)) => {
                    let text = preview(&result.output);
                    ui.weak(format!(
                        "{} replacements · {} output bytes",
                        result.replacements_made,
                        result.output.len()
                    ));
                    if text.len() < result.output.len() {
                        ui.weak(format!(
                            "Showing the first {} bytes; the complete result is retained.",
                            text.len()
                        ));
                    }
                    let mut borrowed = text;
                    egui::ScrollArea::both()
                        .id_source("regex_substitution_result_scroll")
                        .max_height(200.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut borrowed)
                                    .id_source("regex_substitution_result")
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(ui.available_width())
                                    .desired_rows(5),
                            );
                        });
                }
                Some(SubstitutionEvaluationResult::InvalidPattern(error)) => {
                    ui.colored_label(ui.visuals().error_fg_color, &error.message);
                }
                Some(SubstitutionEvaluationResult::Suspended(reason)) => {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!(
                            "Replacement preview suspended: {:?} limit {} (observed at least {}).",
                            reason.limit, reason.maximum, reason.observed
                        ),
                    );
                }
                None => {
                    ui.weak("Replacement preview pending…");
                }
            }
        });
    changed
}
