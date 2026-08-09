use crate::diff::model::TextViewModel;
use crate::diff::text_compare::{CompiledRules, RegexReplacement, TextComparisonRules};
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
        .show(ctx, |ui| {
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
            ui.label("Replacement rules (PATTERN => REPLACEMENT)");
            ui.text_edit_multiline(&mut state.replacements);
            ui.label("Unimportant-section regexes (one per line)");
            ui.text_edit_multiline(&mut state.unimportant);
            if let Some(error) = &state.error {
                ui.colored_label(egui::Color32::RED, error);
            }
            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
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
            kind: crate::diff::model::FileComparisonKind::Text,
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
