//! Transactional editor for folder comparison rules.
use crate::diff::model::{ContentComparisonMode, FolderCompareState};
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
        .show(ctx, |ui| {
            ui.heading("Comparison");
            ui.checkbox(
                &mut state.draft_rules.compare_file_size,
                "Compare file size",
            );
            ui.checkbox(
                &mut state.draft_rules.compare_modified_timestamps,
                "Compare modified timestamps",
            );
            ui.horizontal(|ui| {
                ui.label("Timestamp tolerance (seconds)");
                ui.text_edit_singleline(&mut state.draft_rules.timestamp_tolerance_seconds);
            });
            ui.horizontal(|ui| {
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
            ui.heading("Includes / Excludes");
            ui.columns(2, |columns| {
                columns[0].label("Includes (one pattern per line)");
                columns[0].add(
                    egui::TextEdit::multiline(&mut state.draft_rules.include_rules).desired_rows(4),
                );
                columns[1].label("Excludes (one pattern per line)");
                columns[1].add(
                    egui::TextEdit::multiline(&mut state.draft_rules.exclude_rules).desired_rows(4),
                );
            });
            ui.separator();
            ui.heading("Text Rules");
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
                ui.colored_label(egui::Color32::RED, error);
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(validation.is_ok(), egui::Button::new("Apply + Rescan"))
                    .clicked()
                {
                    if state.apply_draft().is_ok() {
                        state.folder_rules_cancel_snapshot = None;
                        state.folder_rules_open = false;
                        applied = true;
                    }
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
