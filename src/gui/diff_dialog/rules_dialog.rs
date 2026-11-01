use crate::diff::model::TextViewModel;
use crate::diff::text_compare::{CompiledRules, RegexReplacement, TextComparisonRules};
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, semantic_status, with_button_emphasis,
};
use eframe::egui;

#[derive(Clone)]
pub struct RulesDialogState {
    pub open: bool,
    pub draft: TextComparisonRules,
    pub replacements: String,
    pub unimportant: String,
    pub error: Option<String>,
}
impl RulesDialogState {
    pub fn new(rules: &TextComparisonRules) -> Self {
        Self {
            open: true,
            draft: rules.clone(),
            replacements: rules
                .replacements
                .iter()
                .map(|r| format!("{} => {}", r.pattern, r.replacement))
                .collect::<Vec<_>>()
                .join("\n"),
            unimportant: rules.unimportant_sections.join("\n"),
            error: None,
        }
    }
    fn candidate(&self) -> Result<TextComparisonRules, String> {
        let mut rules = self.draft.clone();
        rules.unimportant_sections = self
            .unimportant
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        rules.replacements = self
            .replacements
            .lines()
            .enumerate()
            .filter(|(_, s)| !s.trim().is_empty())
            .map(|(i, s)| {
                s.split_once("=>")
                    .map(|(pattern, replacement)| RegexReplacement {
                        pattern: pattern.trim().into(),
                        replacement: replacement.trim().into(),
                    })
                    .ok_or_else(|| {
                        format!(
                            "Replacement line {}: expected PATTERN => REPLACEMENT",
                            i + 1
                        )
                    })
            })
            .collect::<Result<_, _>>()?;
        CompiledRules::compile(&rules).map_err(|errors| errors.join("\n"))?;
        Ok(rules)
    }
    pub fn apply(&mut self, model: &mut TextViewModel) -> bool {
        match self.candidate() {
            Ok(rules) => {
                model.set_rules(rules);
                self.error = None;
                self.open = false;
                true
            }
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }
}

pub fn show(ctx: &egui::Context, view: u64, model: &mut TextViewModel) {
    let id = egui::Id::new(("diff-rules", view));
    let mut state = ctx
        .data_mut(|d| d.get_temp::<RulesDialogState>(id))
        .unwrap_or_else(|| RulesDialogState::new(&model.rules));
    if !state.open {
        return;
    }
    let mut open = true;
    egui::Window::new("Comparison rules")
        .id(id.with("window"))
        .collapsible(false)
        .resizable(true)
        .open(&mut open)
        .max_width(ctx.content_rect().width().max(1.0))
        .max_height(ctx.content_rect().height().max(1.0))
        .show(ctx, |ui| {
            let footer_height = action_footer_reserved_height(ui, &["Apply", "Cancel"]);
            let body_height = (ui.available_height() - footer_height).max(0.0);
            egui::ScrollArea::vertical()
                .id_salt("diff_rules_form_body")
                .auto_shrink([false, true])
                .min_scrolled_height(0.0)
                .max_height(body_height)
                .show(ui, |ui| {
                    ui.checkbox(
                        &mut state.draft.ignore_leading_whitespace,
                        "Ignore leading whitespace",
                    );
                    ui.checkbox(
                        &mut state.draft.ignore_trailing_whitespace,
                        "Ignore trailing whitespace",
                    );
                    ui.checkbox(
                        &mut state.draft.ignore_all_whitespace,
                        "Ignore all whitespace",
                    );
                    ui.checkbox(&mut state.draft.ignore_blank_lines, "Ignore blank lines");
                    ui.checkbox(&mut state.draft.case_sensitive, "Case sensitive");
                    ui.checkbox(
                        &mut state.draft.line_ending_equivalence,
                        "Treat line endings as equivalent",
                    );
                    compact_section_heading(
                        ui,
                        "Replacement rules",
                        Some("One PATTERN => REPLACEMENT per line."),
                    );
                    ui.add(
                        egui::TextEdit::multiline(&mut state.replacements)
                            .desired_width(ui.available_width())
                            .desired_rows(4),
                    );
                    compact_section_heading(
                        ui,
                        "Unimportant-section regexes",
                        Some("One regex per line."),
                    );
                    ui.add(
                        egui::TextEdit::multiline(&mut state.unimportant)
                            .desired_width(ui.available_width())
                            .desired_rows(4),
                    );
                    if let Some(error) = &state.error {
                        semantic_status(ui, StatusTone::Error, "Invalid rules", Some(error));
                    }
                });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                let apply_button = ui.button("Apply");
                let apply = with_button_emphasis(ui, apply_button, ButtonEmphasis::Primary);
                if apply.clicked() {
                    state.apply(model);
                }
                if ui.button("Cancel").clicked() {
                    state.open = false;
                }
            });
        });
    state.open &= open;
    ctx.data_mut(|d| d.insert_temp(id, state));
}

pub(super) fn action_footer_reserved_height(ui: &egui::Ui, labels: &[&str]) -> f32 {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let native_button_style = style.button_style(
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
        .unwrap_or_else(|| native_button_style.text_style.font_id.clone());
    let frame_margin = if ui.visuals().button_frame {
        native_button_style.frame.total_margin().sum()
    } else {
        egui::Vec2::ZERO
    };
    let available_width = ui.available_width().max(1.0);
    let text_width = (available_width - frame_margin.x).max(1.0);
    let wrap_mode = ui.wrap_mode();
    let mut row_width = 0.0;
    let mut row_height = 0.0_f32;
    let mut actions_height = 0.0;
    for label in labels {
        let text_size = egui::WidgetText::from(*label)
            .into_galley(ui, Some(wrap_mode), text_width, button_font.clone())
            .size();
        let button_width = (text_size.x + frame_margin.x)
            .max(spacing.interact_size.x)
            .min(available_width);
        let button_height = (text_size.y + spacing.extra_text_line_spacing + frame_margin.y)
            .max(spacing.interact_size.y);
        if row_width > 0.0 && row_width + spacing.item_spacing.x + button_width > available_width {
            actions_height += row_height + spacing.item_spacing.y;
            row_width = button_width;
            row_height = button_height;
        } else {
            if row_width > 0.0 {
                row_width += spacing.item_spacing.x;
            }
            row_width += button_width;
            row_height = row_height.max(button_height);
        }
    }
    let separator_spacing = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    let height = actions_height
        + row_height.max(spacing.interact_size.y)
        + 2.0 * spacing.item_spacing.y
        + separator_spacing;
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);
    (height * pixels_per_point).ceil() / pixels_per_point
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_candidate_before_apply() {
        let mut state = RulesDialogState::new(&TextComparisonRules::default());
        state.unimportant = "[".into();
        assert!(state.candidate().is_err());
        state.unimportant = "ok".into();
        assert!(state.candidate().is_ok());
    }
    #[test]
    fn apply_mutates_and_schedules_exactly_once_only_when_valid() {
        let dir = tempfile::tempdir().unwrap();
        let left = dir.path().join("left");
        let right = dir.path().join("right");
        std::fs::write(&left, "a").unwrap();
        std::fs::write(&right, "b").unwrap();
        let state = crate::diff::model::TextCompareState {
            left: Some(left),
            right: Some(right),
            relative_path: None,
            origin: crate::diff::model::TextCompareOrigin::FileBacked,
        };
        let mut model = TextViewModel::load(&state, &Default::default()).unwrap();
        model.recalculate_at = None;
        let revision = model.rules.revision;
        let mut dialog = RulesDialogState::new(&model.rules);
        dialog.unimportant = "[".into();
        assert!(!dialog.apply(&mut model));
        assert_eq!(model.rules.revision, revision);
        assert!(model.recalculate_at.is_none());
        assert!(dialog.open);
        dialog.unimportant = "x".into();
        assert!(dialog.apply(&mut model));
        assert_eq!(model.rules.revision, revision + 1);
        let scheduled = model.recalculate_at;
        dialog.open = true;
        assert!(dialog.apply(&mut model));
        assert_eq!(model.rules.revision, revision + 1);
        assert_eq!(model.recalculate_at, scheduled);
    }
}
