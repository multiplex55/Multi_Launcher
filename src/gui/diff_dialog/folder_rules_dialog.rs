//! Transactional editor for folder comparison rules.
use crate::diff::model::{ContentComparisonMode, FolderCompareState};
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, semantic_status, with_button_emphasis,
};
use eframe::egui;

pub(super) fn open(state: &mut FolderCompareState) {
    if !state.folder_rules_open {
        state.folder_rules_cancel_snapshot = Some(state.draft_rules.clone());
        state.folder_rules_open = true;
    }
}

pub(super) fn cancel(state: &mut FolderCompareState) {
    if let Some(snapshot) = state.folder_rules_cancel_snapshot.take() {
        state.draft_rules = snapshot;
    }
    state.folder_rules_open = false;
}

/// Returns true only after the complete draft has validated and been applied.
pub(super) fn show(ctx: &egui::Context, state: &mut FolderCompareState) -> bool {
    if !state.folder_rules_open {
        return false;
    }
    let mut applied = false;
    let mut open_window = true;
    egui::Window::new("Folder Rules")
        .collapsible(false)
        .resizable(true)
        .open(&mut open_window)
        .max_width(ctx.content_rect().width().max(1.0))
        .max_height(ctx.content_rect().height().max(1.0))
        .show(ctx, |ui| {
            let footer_height = super::rules_dialog::action_footer_reserved_height(
                ui,
                &["Apply + Rescan", "Cancel"],
            );
            let body_height = (ui.available_height() - footer_height).max(0.0);
            let validation = egui::ScrollArea::vertical()
                .id_salt("diff_folder_rules_form_body")
                .auto_shrink([false, true])
                .min_scrolled_height(0.0)
                .max_height(body_height)
                .show(ui, |ui| {
                    compact_section_heading(ui, "Comparison", None);
                    ui.checkbox(
                        &mut state.draft_rules.compare_file_size,
                        "Compare file size",
                    );
                    ui.checkbox(
                        &mut state.draft_rules.compare_modified_timestamps,
                        "Compare modified timestamps",
                    );
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Timestamp tolerance (seconds)");
                        ui.add(
                            egui::TextEdit::singleline(
                                &mut state.draft_rules.timestamp_tolerance_seconds,
                            )
                            .desired_width(ui.available_width().min(220.0)),
                        );
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.radio_value(
                            &mut state.draft_rules.content_comparison,
                            ContentComparisonMode::Metadata,
                            "Metadata only",
                        );
                        ui.radio_value(
                            &mut state.draft_rules.content_comparison,
                            ContentComparisonMode::OnDemand,
                            "Contents on demand",
                        );
                        ui.radio_value(
                            &mut state.draft_rules.content_comparison,
                            ContentComparisonMode::Always,
                            "Always compare contents",
                        );
                    });
                    ui.separator();
                    compact_section_heading(
                        ui,
                        "Includes / Excludes",
                        Some("One pattern per line."),
                    );
                    let two_columns_fit = ui.available_width()
                        >= 2.0 * ui.spacing().text_edit_width + ui.spacing().item_spacing.x;
                    if two_columns_fit {
                        ui.columns(2, |columns| {
                            columns[0].label("Includes");
                            columns[0].add(
                                egui::TextEdit::multiline(&mut state.draft_rules.include_rules)
                                    .desired_width(columns[0].available_width())
                                    .desired_rows(4),
                            );
                            columns[1].label("Excludes");
                            columns[1].add(
                                egui::TextEdit::multiline(&mut state.draft_rules.exclude_rules)
                                    .desired_width(columns[1].available_width())
                                    .desired_rows(4),
                            );
                        });
                    } else {
                        ui.label("Includes");
                        ui.add(
                            egui::TextEdit::multiline(&mut state.draft_rules.include_rules)
                                .desired_width(ui.available_width())
                                .desired_rows(4),
                        );
                        ui.label("Excludes");
                        ui.add(
                            egui::TextEdit::multiline(&mut state.draft_rules.exclude_rules)
                                .desired_width(ui.available_width())
                                .desired_rows(4),
                        );
                    }
                    ui.separator();
                    compact_section_heading(ui, "Text Rules", None);
                    ui.checkbox(
                        &mut state.draft_rules.use_text_compare_rules,
                        "Use Text Compare rules for textual contents",
                    );
                    ui.add_enabled_ui(state.draft_rules.use_text_compare_rules, |ui| {
                        ui.checkbox(
                            &mut state.draft_rules.text_rules.case_sensitive,
                            "Case sensitive",
                        );
                        ui.checkbox(
                            &mut state.draft_rules.text_rules.ignore_all_whitespace,
                            "Ignore all whitespace",
                        );
                        ui.checkbox(
                            &mut state.draft_rules.text_rules.ignore_blank_lines,
                            "Ignore blank lines",
                        );
                        ui.checkbox(
                            &mut state.draft_rules.text_rules.line_ending_equivalence,
                            "Treat line endings as equivalent",
                        );
                    });
                    let validation = state.validate_draft();
                    if let Err(error) = &validation {
                        semantic_status(ui, StatusTone::Error, "Invalid folder rules", Some(error));
                    }
                    validation
                })
                .inner;
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                let apply_button =
                    ui.add_enabled(validation.is_ok(), egui::Button::new("Apply + Rescan"));
                if with_button_emphasis(ui, apply_button, ButtonEmphasis::Primary).clicked()
                    && state.apply_draft().is_ok()
                {
                    state.folder_rules_cancel_snapshot = None;
                    state.folder_rules_open = false;
                    applied = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel(state);
                }
            });
        });
    if !open_window && state.folder_rules_open {
        cancel(state);
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_restores_pre_dialog_draft() {
        let mut state = FolderCompareState::default();
        open(&mut state);
        state.draft_rules.exclude_rules = "*.tmp".into();
        cancel(&mut state);
        assert!(state.draft_rules.exclude_rules.is_empty());
        assert_eq!(state.applied_scan_rules, Default::default());
    }
}
