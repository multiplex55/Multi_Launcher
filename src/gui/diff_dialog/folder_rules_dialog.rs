//! Transactional editor for folder comparison rules.
use crate::diff::model::{ContentComparisonMode, FolderRulesDraft};
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, semantic_status, with_button_emphasis,
};
use eframe::egui;

#[derive(Clone, Copy)]
pub(super) enum FormAction {
    Apply,
    Cancel,
}

pub(super) fn render_contents(
    ui: &mut egui::Ui,
    draft: &mut FolderRulesDraft,
) -> Option<FormAction> {
    let mut action = None;
    let footer_height =
        super::rules_dialog::action_footer_reserved_height(ui, &["Apply + Rescan", "Cancel"]);
    let body_height = (ui.available_height() - footer_height).max(0.0);
    let validation = egui::ScrollArea::vertical()
        .id_salt("diff_folder_rules_form_body")
        .auto_shrink([false, true])
        .min_scrolled_height(0.0)
        .max_height(body_height)
        .show(ui, |ui| {
            compact_section_heading(ui, "Comparison", None);
            ui.checkbox(&mut draft.compare_file_size, "Compare file size");
            ui.checkbox(
                &mut draft.compare_modified_timestamps,
                "Compare modified timestamps",
            );
            ui.horizontal_wrapped(|ui| {
                ui.label("Timestamp tolerance (seconds)");
                ui.add(
                    egui::TextEdit::singleline(&mut draft.timestamp_tolerance_seconds)
                        .desired_width(ui.available_width().min(220.0)),
                );
            });
            ui.horizontal_wrapped(|ui| {
                ui.radio_value(
                    &mut draft.content_comparison,
                    ContentComparisonMode::Metadata,
                    "Metadata only",
                );
                ui.radio_value(
                    &mut draft.content_comparison,
                    ContentComparisonMode::OnDemand,
                    "Contents on demand",
                );
                ui.radio_value(
                    &mut draft.content_comparison,
                    ContentComparisonMode::Always,
                    "Always compare contents",
                );
            });
            ui.separator();
            compact_section_heading(ui, "Includes / Excludes", Some("One pattern per line."));
            let two_columns_fit = ui.available_width()
                >= 2.0 * ui.spacing().text_edit_width + ui.spacing().item_spacing.x;
            if two_columns_fit {
                ui.columns(2, |columns| {
                    columns[0].label("Includes");
                    columns[0].add(
                        egui::TextEdit::multiline(&mut draft.include_rules)
                            .desired_width(columns[0].available_width())
                            .desired_rows(4),
                    );
                    columns[1].label("Excludes");
                    columns[1].add(
                        egui::TextEdit::multiline(&mut draft.exclude_rules)
                            .desired_width(columns[1].available_width())
                            .desired_rows(4),
                    );
                });
            } else {
                ui.label("Includes");
                ui.add(
                    egui::TextEdit::multiline(&mut draft.include_rules)
                        .desired_width(ui.available_width())
                        .desired_rows(4),
                );
                ui.label("Excludes");
                ui.add(
                    egui::TextEdit::multiline(&mut draft.exclude_rules)
                        .desired_width(ui.available_width())
                        .desired_rows(4),
                );
            }
            ui.separator();
            compact_section_heading(ui, "Text Rules", None);
            ui.checkbox(
                &mut draft.use_text_compare_rules,
                "Use Text Compare rules for textual contents",
            );
            ui.add_enabled_ui(draft.use_text_compare_rules, |ui| {
                ui.checkbox(&mut draft.text_rules.case_sensitive, "Case sensitive");
                ui.checkbox(
                    &mut draft.text_rules.ignore_all_whitespace,
                    "Ignore all whitespace",
                );
                ui.checkbox(
                    &mut draft.text_rules.ignore_blank_lines,
                    "Ignore blank lines",
                );
                ui.checkbox(
                    &mut draft.text_rules.line_ending_equivalence,
                    "Treat line endings as equivalent",
                );
            });
            let validation = draft.validate();
            if let Err(error) = &validation {
                semantic_status(ui, StatusTone::Error, "Invalid folder rules", Some(error));
            }
            validation
        })
        .inner;
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        let apply_button = ui.add_enabled(validation.is_ok(), egui::Button::new("Apply + Rescan"));
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::new("diff-folder-apply-test"),
                (apply_button.rect, ui.clip_rect(), apply_button.enabled()),
            )
        });
        if with_button_emphasis(ui, apply_button, ButtonEmphasis::Primary).clicked() {
            action = Some(FormAction::Apply);
        }
        let cancel = ui.button("Cancel");
        #[cfg(test)]
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                egui::Id::new("diff-folder-cancel-test"),
                (cancel.rect, ui.clip_rect(), cancel.enabled()),
            )
        });
        if cancel.clicked() {
            action = Some(FormAction::Cancel);
        }
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_restores_pre_dialog_draft() {
        use crate::gui::utility_window_host::HostedUtilityDialog;
        let live = FolderRulesDraft::default();
        let mut editor =
            super::super::child_hosts::Editor::<super::super::child_hosts::Folder>::new(
                1,
                1,
                1,
                live.clone(),
            );
        let checkpoint = editor.capture();
        editor.draft.exclude_rules = "*.tmp".into();
        editor.restore(checkpoint);
        assert_eq!(editor.draft, live);
        assert!(live.exclude_rules.is_empty());
    }
}
