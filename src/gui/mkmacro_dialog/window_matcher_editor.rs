//! Shared editor for the criteria used to identify a window.
use crate::mkmacro::MkWindowMatcher;
use eframe::egui;

/// Describes the two independent actions produced by the matcher editor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct MatcherEditorOutcome {
    pub(crate) changed: bool,
    pub(crate) pick_window: bool,
}

/// Render and edit all of the persisted window-matcher criteria.
pub(crate) fn matcher_ui(ui: &mut egui::Ui, matcher: &mut MkWindowMatcher) -> MatcherEditorOutcome {
    let mut changed = false;
    changed |= optional_field(ui, "Executable", &mut matcher.process);

    let title_changed = optional_field(ui, "Title contains", &mut matcher.title);
    let regex_changed = optional_field(ui, "Title regex", &mut matcher.title_regex);
    changed |= title_changed;
    changed |= regex_changed;

    changed |= optional_field(ui, "Class", &mut matcher.class);

    // Window matching gives title_regex precedence over title. Keep an
    // unchanged legacy matcher lossless, but make edits to either title mode
    // select that mode explicitly so the editor and picker cannot diverge.
    if title_changed && matcher.title.is_some() {
        matcher.title_regex = None;
    } else if regex_changed && matcher.title_regex.is_some() {
        matcher.title = None;
    }
    if matcher.title.is_some() && matcher.title_regex.is_some() {
        ui.small("Title regex takes precedence over Title contains.");
    }

    MatcherEditorOutcome {
        changed,
        pick_window: ui.button("Choose Window…").clicked(),
    }
}

fn optional_field(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) -> bool {
    let before = value.clone();
    let text = value.get_or_insert_with(String::new);
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(text);
    });
    normalize_optional(value);
    *value != before
}

/// Convert the empty value used by an egui text field back to the model's
/// optional representation.
fn normalize_optional(value: &mut Option<String>) -> bool {
    if value.as_ref().is_some_and(String::is_empty) {
        *value = None;
        true
    } else {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MatcherField {
    Process,
    Title,
    TitleRegex,
    Class,
}

/// Apply one field update using the same normalization and title-mode rules
/// as the widget editor. This keeps the non-visual behavior unit-testable.
fn apply_field_edit(
    matcher: &mut MkWindowMatcher,
    field: MatcherField,
    value: Option<String>,
) -> bool {
    let before = matcher.clone();
    let value = value.filter(|value| !value.is_empty());
    match field {
        MatcherField::Process => matcher.process = value,
        MatcherField::Title => matcher.title = value,
        MatcherField::TitleRegex => matcher.title_regex = value,
        MatcherField::Class => matcher.class = value,
    }

    let field_changed = *matcher != before;
    if field_changed {
        match field {
            MatcherField::Title if matcher.title.is_some() => matcher.title_regex = None,
            MatcherField::TitleRegex if matcher.title_regex.is_some() => matcher.title = None,
            _ => {}
        }
    }
    *matcher != before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_edited_fields_normalize_to_none() {
        let mut value = Some(String::new());
        assert!(normalize_optional(&mut value));
        assert_eq!(value, None);

        let mut matcher = MkWindowMatcher {
            title: Some("Title".into()),
            ..Default::default()
        };
        assert!(apply_field_edit(
            &mut matcher,
            MatcherField::Title,
            Some(String::new())
        ));
        assert_eq!(matcher.title, None);
    }

    #[test]
    fn unchanged_values_survive_an_update_cycle() {
        let original = MkWindowMatcher {
            process: Some("editor.exe".into()),
            title: Some("Document".into()),
            title_regex: Some("^Document".into()),
            class: Some("Editor".into()),
        };
        let mut matcher = original.clone();

        let mut outcome = MatcherEditorOutcome::default();
        let _ = egui::Context::default().run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                outcome = matcher_ui(ui, &mut matcher);
            });
        });
        assert!(!outcome.changed);
        assert!(!outcome.pick_window);
        assert_eq!(matcher, original);

        for (field, value) in [
            (MatcherField::Process, matcher.process.clone()),
            (MatcherField::Title, matcher.title.clone()),
            (MatcherField::TitleRegex, matcher.title_regex.clone()),
            (MatcherField::Class, matcher.class.clone()),
        ] {
            assert!(!apply_field_edit(&mut matcher, field, value));
        }
        assert_eq!(matcher, original);
    }

    #[test]
    fn editing_each_matcher_criterion_reports_changed() {
        for field in [
            MatcherField::Process,
            MatcherField::Title,
            MatcherField::TitleRegex,
            MatcherField::Class,
        ] {
            let mut matcher = MkWindowMatcher::default();
            assert!(apply_field_edit(&mut matcher, field, Some("edited".into())));
        }
    }

    #[test]
    fn title_modes_follow_picker_precedence_when_edited() {
        let mut matcher = MkWindowMatcher {
            title: Some("old title".into()),
            title_regex: Some("old regex".into()),
            ..Default::default()
        };
        assert!(apply_field_edit(
            &mut matcher,
            MatcherField::Title,
            Some("new title".into())
        ));
        assert_eq!(matcher.title_regex, None);

        matcher.title_regex = Some("old regex".into());
        assert!(apply_field_edit(
            &mut matcher,
            MatcherField::TitleRegex,
            Some("new regex".into())
        ));
        assert_eq!(matcher.title, None);
    }

    #[test]
    fn action_editor_and_macro_properties_reexport_the_same_editor() {
        let action_editor: fn(&mut egui::Ui, &mut MkWindowMatcher) -> MatcherEditorOutcome =
            super::super::action_editor::matcher_ui;
        let macro_properties: fn(&mut egui::Ui, &mut MkWindowMatcher) -> MatcherEditorOutcome =
            super::super::macro_properties::matcher_ui;

        assert_eq!(action_editor as usize, macro_properties as usize);
    }
}
