use crate::clipboard_modify::clipboard::ClipboardBackend;
use crate::gui::theme::widgets::{StatusTone, secondary_help, semantic_status};
use crate::regex_tester::{
    MAX_TEST_CASES, RegexSession, RegexTestCase, RegexTestCaseCollectionError,
    RegexTestCaseExpectation, RegexTestCaseId, RegexTestCaseStatus, RegexTestCaseSummary,
};
use eframe::egui;
use std::time::Instant;

#[derive(Clone, Copy)]
enum CaseAction {
    Add,
    Duplicate(RegexTestCaseId),
    Remove(RegexTestCaseId),
    Move(RegexTestCaseId, usize),
}

struct PresentedStatus {
    tone: StatusTone,
    label: String,
    details: Option<String>,
}

fn summary_text(summary: RegexTestCaseSummary) -> String {
    let noun = if summary.total == 1 { "case" } else { "cases" };
    format!(
        "{} {noun} · {} passed · {} failed · {} errors · {} pending",
        summary.total,
        summary.passed,
        summary.failed,
        summary.pattern_errors + summary.unevaluated,
        summary.pending
    )
}

fn status_for(
    case: &RegexTestCase,
    status: Option<&RegexTestCaseStatus>,
) -> PresentedStatus {
    match status {
        None | Some(RegexTestCaseStatus::Pending) => PresentedStatus {
            tone: StatusTone::Info,
            label: "Pending".to_owned(),
            details: None,
        },
        Some(RegexTestCaseStatus::Pass) => PresentedStatus {
            tone: StatusTone::Success,
            label: "Pass".to_owned(),
            details: None,
        },
        Some(RegexTestCaseStatus::Fail) => PresentedStatus {
            tone: StatusTone::Error,
            label: "Fail".to_owned(),
            details: Some(match case.expected {
                RegexTestCaseExpectation::Match => {
                    "Expected a substring match, but none was found.".to_owned()
                }
                RegexTestCaseExpectation::NoMatch => {
                    "Expected no match, but the pattern matched part of the input.".to_owned()
                }
            }),
        },
        Some(RegexTestCaseStatus::PatternError(error)) => PresentedStatus {
            tone: StatusTone::Error,
            label: "Error · invalid pattern".to_owned(),
            details: Some(error.message.clone()),
        },
        Some(RegexTestCaseStatus::Unevaluated(reason)) => PresentedStatus {
            tone: StatusTone::Error,
            label: "Error · not evaluated".to_owned(),
            details: Some(super::presentation::suspension(reason)),
        },
    }
}

fn copy_summary(
    clipboard: &dyn ClipboardBackend,
    summary: RegexTestCaseSummary,
) -> Result<&'static str, String> {
    super::copy_text(
        clipboard,
        &summary_text(summary),
        "Copied test case summary",
    )
}

fn label_widget_id(id: RegexTestCaseId) -> egui::Id {
    egui::Id::new(("regex_test_case_label", id.value()))
}

fn input_widget_id(id: RegexTestCaseId) -> egui::Id {
    egui::Id::new(("regex_test_case_input", id.value()))
}

fn label_value(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_owned())
}

fn collection_error(error: RegexTestCaseCollectionError) -> String {
    match error {
        RegexTestCaseCollectionError::LimitReached => {
            format!("The test case limit is {MAX_TEST_CASES} cases.")
        }
        RegexTestCaseCollectionError::IdentifierSpaceExhausted => {
            "No more test case identifiers are available in this session.".to_owned()
        }
        RegexTestCaseCollectionError::DuplicateIdentifier => {
            "The case list contains a duplicate identifier.".to_owned()
        }
        RegexTestCaseCollectionError::MissingCase => {
            "The selected case no longer exists.".to_owned()
        }
    }
}

fn apply_action(session: &mut RegexSession, action: CaseAction) -> Result<(), String> {
    let cases = &mut session.draft.test_cases;
    match action {
        CaseAction::Add => cases
            .add(None, String::new(), RegexTestCaseExpectation::Match)
            .map(|_| ())
            .map_err(collection_error),
        CaseAction::Duplicate(id) => cases
            .duplicate(id)
            .map(|_| ())
            .map_err(collection_error),
        CaseAction::Remove(id) => cases
            .remove(id)
            .map(|_| ())
            .ok_or_else(|| collection_error(RegexTestCaseCollectionError::MissingCase)),
        CaseAction::Move(id, index) => cases
            .move_to(id, index)
            .then_some(())
            .ok_or_else(|| collection_error(RegexTestCaseCollectionError::MissingCase)),
    }
}

/// Edit and run authored samples without invalidating the main document's
/// accepted matches, selection, explanation or substitution result.
pub(super) fn show(
    ui: &mut egui::Ui,
    session: &mut RegexSession,
    expanded: &mut bool,
    clipboard: &dyn ClipboardBackend,
    copy_feedback: &mut Option<Result<&'static str, String>>,
    process_actions: bool,
) -> CaseToolbarGeometry {
    let mut changed = false;
    let mut next_action = None;
    let mut copy_requested = false;
    let mut add_rect = egui::Rect::NOTHING;
    let mut run_rect = egui::Rect::NOTHING;
    let mut copy_rect = egui::Rect::NOTHING;

    ui.horizontal_wrapped(|ui| {
        let disclosure = if *expanded { "▾ Test Cases" } else { "▸ Test Cases" };
        if ui.small_button(disclosure).clicked() && process_actions {
            *expanded = !*expanded;
        }
        let at_limit = session.draft.test_cases.len() >= session.test_case_policy().maximum_cases;
        let add = ui
            .add_enabled(!at_limit, egui::Button::new("+ Add"))
            .on_hover_text(if at_limit {
                format!("The limit is {} cases.", session.test_case_policy().maximum_cases)
            } else {
                "Add a new test case".to_owned()
            });
        add_rect = add.rect;
        if add.clicked() && process_actions {
            next_action = Some(CaseAction::Add);
        }
        let has_cases = !session.draft.test_cases.is_empty();
        let summary = session.test_case_summary().copied().unwrap_or_default();
        let run_label = if session.test_case_revision() == 0 || summary.pending > 0 {
            "Run"
        } else {
            "Rerun"
        };
        let run = ui
            .add_enabled(has_cases, egui::Button::new(run_label))
            .on_hover_text("Run all test cases with the current pattern and flags");
        run_rect = run.rect;
        if run.clicked() && process_actions {
            changed = true;
        }
        let copy = ui
            .button("Copy summary")
            .on_hover_text("Copy counts only; case labels and inputs are not included");
        copy_rect = copy.rect;
        if copy.clicked() && process_actions {
            copy_requested = true;
        }
    });

    if *expanded {
        egui::ScrollArea::vertical()
            .id_salt("regex_test_cases_content")
            .auto_shrink([false, false])
            .max_height(ui.available_height().max(1.0))
            .show(ui, |ui| {
                if let Some(feedback) = copy_feedback.as_ref() {
                    match feedback {
                        Ok(message) => {
                            semantic_status(ui, StatusTone::Success, message, None);
                        }
                        Err(message) => {
                            semantic_status(ui, StatusTone::Error, message, None);
                        }
                    }
                }
                let summary = session.test_case_summary().copied().unwrap_or_default();
                secondary_help(ui, &summary_text(summary));
                secondary_help(
                    ui,
                    "Match means any substring. Use ^…$ anchors to require a whole-input match.",
                );
                if let Some(crate::regex_tester::EvaluationResult::InvalidPattern(error)) =
                    session.result()
                {
                    semantic_status(
                        ui,
                        StatusTone::Error,
                        "Pattern error",
                        Some(&error.message),
                    );
                }
                if session.draft.test_cases.is_empty() {
                    secondary_help(
                        ui,
                        "No test cases yet. Add one to check whether the pattern matches a sample input.",
                    );
                } else {
                    let evaluation = session.test_case_evaluation().cloned();
                    let case_count = session.draft.test_cases.len();
                    for index in 0..case_count {
                        let case_id = session.draft.test_cases.as_slice()[index].id;
                        let Some(case) = session.draft.test_cases.get_mut(case_id) else {
                            continue;
                        };
                        let case_status = evaluation
                            .as_ref()
                            .and_then(|result| {
                                result.results.iter().find(|item| item.case_id == case_id)
                            })
                            .map(|result| result.status.clone());
                        let presented = status_for(case, case_status.as_ref());
                        let mut label = case.label.clone().unwrap_or_default();
                        let row_id = egui::Id::new(("regex_test_case_row", case_id.value()));
                        ui.push_id(row_id, |ui| {
                            ui.group(|ui| {
                                let label_response = ui.add(
                                    egui::TextEdit::singleline(&mut label)
                                        .id(label_widget_id(case_id))
                                        .desired_width(ui.available_width())
                                        .hint_text("Optional case label"),
                                );
                                if label_response.changed() {
                                    if process_actions {
                                        case.label = label_value(&label);
                                        changed = true;
                                    }
                                }

                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(index > 0, egui::Button::new("↑").small())
                                        .on_hover_text("Move this case up")
                                        .clicked()
                                        && process_actions
                                    {
                                        next_action = Some(CaseAction::Move(case_id, index - 1));
                                    }
                                    if ui
                                        .add_enabled(
                                            index + 1 < case_count,
                                            egui::Button::new("↓").small(),
                                        )
                                        .on_hover_text("Move this case down")
                                        .clicked()
                                        && process_actions
                                    {
                                        next_action = Some(CaseAction::Move(case_id, index + 1));
                                    }
                                    if ui
                                        .small_button("Duplicate")
                                        .on_hover_text("Duplicate this case with a new stable identity")
                                        .clicked()
                                        && process_actions
                                    {
                                        next_action = Some(CaseAction::Duplicate(case_id));
                                    }
                                    if ui
                                        .small_button("Remove")
                                        .on_hover_text("Remove this case")
                                        .clicked()
                                        && process_actions
                                    {
                                        next_action = Some(CaseAction::Remove(case_id));
                                    }
                                });

                                ui.horizontal(|ui| {
                                    ui.label("Expected:");
                                    let mut expected = case.expected;
                                    let expectation_changed = ui
                                        .selectable_value(
                                            &mut expected,
                                            RegexTestCaseExpectation::Match,
                                            "Match",
                                        )
                                        .changed()
                                        || ui
                                        .selectable_value(
                                            &mut expected,
                                            RegexTestCaseExpectation::NoMatch,
                                            "No match",
                                        )
                                        .changed();
                                    if expectation_changed && process_actions {
                                        case.expected = expected;
                                        changed = true;
                                    }
                                });

                                let mut input_value = case.input.clone();
                                let input = ui.add_sized(
                                    egui::vec2(ui.available_width(), 64.0),
                                    egui::TextEdit::multiline(&mut input_value)
                                        .id(input_widget_id(case_id))
                                        .desired_rows(3)
                                        .desired_width(f32::INFINITY)
                                        .hint_text("Sample input; Enter adds a line"),
                                );
                                if input.changed() && process_actions {
                                    case.input = input_value;
                                    changed = true;
                                }

                                semantic_status(
                                    ui,
                                    presented.tone,
                                    &presented.label,
                                    presented.details.as_deref(),
                                );
                            });
                        });
                        ui.add_space(4.0);
                    }
                }
            });
    }

    if let Some(action) = next_action {
        match apply_action(session, action) {
            Ok(()) => changed = true,
            Err(message) => {
                *copy_feedback = Some(Err(message));
            }
        }
    }
    if changed {
        session.mark_test_cases_changed(Instant::now());
        *copy_feedback = None;
        ui.ctx().request_repaint();
    } else if copy_requested {
        let summary = session.test_case_summary().copied().unwrap_or_default();
        *copy_feedback = Some(copy_summary(clipboard, summary));
    }
    CaseToolbarGeometry {
        add_rect,
        run_rect,
        copy_rect,
    }
}

#[derive(Clone, Copy)]
pub(super) struct CaseToolbarGeometry {
    pub add_rect: egui::Rect,
    pub run_rect: egui::Rect,
    pub copy_rect: egui::Rect,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_modify::clipboard::ClipboardError;
    use crate::regex_tester::{
        RegexFlags, RegexTestCaseStatus, RegexTestCases,
        session::EVALUATION_DEBOUNCE,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    #[derive(Default)]
    struct CaptureClipboard {
        text: Mutex<Option<String>>,
        reads: AtomicUsize,
        writes: AtomicUsize,
    }

    impl ClipboardBackend for CaptureClipboard {
        fn read_text(&self) -> Result<String, ClipboardError> {
            self.reads.fetch_add(1, Ordering::Relaxed);
            Ok(self.text.lock().unwrap().clone().unwrap_or_default())
        }

        fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
            self.writes.fetch_add(1, Ordering::Relaxed);
            *self.text.lock().unwrap() = Some(text.to_owned());
            Ok(())
        }
    }

    fn evaluated_session() -> RegexSession {
        let mut session = RegexSession::default();
        session.draft.pattern = "a".into();
        session.draft.test_text = "a ba".into();
        session.draft.replacement = "x".into();
        session.set_substitution_enabled(true, Instant::now());
        let now = Instant::now();
        session.mark_changed(now);
        assert!(session.tick(now + EVALUATION_DEBOUNCE));
        session.select_match(1);
        session
    }

    fn render_panel(
        context: &egui::Context,
        raw_input: egui::RawInput,
        session: &mut RegexSession,
        expanded: &mut bool,
        clipboard: &dyn ClipboardBackend,
        feedback: &mut Option<Result<&'static str, String>>,
        process_actions: bool,
    ) -> CaseToolbarGeometry {
        let mut geometry = None;
        context
            .run_ui(raw_input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    geometry = Some(show(
                        ui,
                        session,
                        expanded,
                        clipboard,
                        feedback,
                        process_actions,
                    ));
                });
            })
            .drop_without_applying_deltas();
        geometry.unwrap()
    }

    fn click_input(screen: egui::Rect, position: egui::Pos2) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(screen),
            events: vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn case_edits_and_reruns_preserve_main_results_selection_and_substitution() {
        let mut session = evaluated_session();
        session
            .draft
            .test_cases
            .add(
                Some("private label".into()),
                "private sample".into(),
                RegexTestCaseExpectation::Match,
            )
            .unwrap();
        session.mark_test_cases_changed(Instant::now());
        let main_revision = session.revision();
        let evaluated_revision = session.evaluated_revision();
        let selected = session.selected_index();
        let matches = session.matches().as_ptr();
        let substitution = session.substitution_result().cloned();
        let (_, before_text_view) = session.text_edit_parts();
        let source = before_text_view.source.unwrap().as_ptr();
        let explanation = session.explanation().unwrap() as *const _;
        let case_id = session.draft.test_cases.as_slice()[0].id;

        session.draft.test_cases.get_mut(case_id).unwrap().input = "changed sample".into();
        session.mark_test_cases_changed(Instant::now());
        assert!(session.tick_test_cases(Instant::now() + EVALUATION_DEBOUNCE));
        session.mark_test_cases_changed(Instant::now());

        let after_source = {
            let (_, text_view) = session.text_edit_parts();
            text_view.source.unwrap().as_ptr()
        };
        assert_eq!(session.revision(), main_revision);
        assert_eq!(session.evaluated_revision(), evaluated_revision);
        assert_eq!(session.selected_index(), selected);
        assert_eq!(session.matches().as_ptr(), matches);
        assert_eq!(after_source, source);
        assert_eq!(session.substitution_result(), substitution.as_ref());
        assert_eq!(session.explanation().unwrap() as *const _, explanation);
        assert_eq!(session.draft.test_cases.as_slice()[0].id, case_id);
    }

    #[test]
    fn flag_edit_invalidates_case_results_and_clears_main_matches() {
        let mut session = evaluated_session();
        session
            .draft
            .test_cases
            .add(None, "A".into(), RegexTestCaseExpectation::Match)
            .unwrap();
        session.mark_test_cases_changed(Instant::now());
        let now = Instant::now();
        assert!(session.tick_test_cases(now + EVALUATION_DEBOUNCE));
        assert_eq!(session.test_case_summary().unwrap().failed, 1);
        let main_revision = session.revision();

        session.draft.flags = RegexFlags {
            case_insensitive: true,
            ..session.draft.flags
        };
        session.mark_changed(now + EVALUATION_DEBOUNCE);
        assert_eq!(session.test_case_summary().unwrap().pending, 1);
        assert_eq!(session.revision(), main_revision + 1);
        assert!(session.result().is_none());
        assert!(session.matches().is_empty());
        assert!(session.tick_test_cases(now + 2 * EVALUATION_DEBOUNCE));
        assert_eq!(session.test_case_summary().unwrap().passed, 1);
    }

    #[test]
    fn result_statuses_explain_failures_patterns_and_limits_without_false_passes() {
        let mut cases = RegexTestCases::default();
        let id = cases
            .add(
                Some("whole input".into()),
                "prefix cat suffix".into(),
                RegexTestCaseExpectation::Match,
            )
            .unwrap();
        let case = cases.get(id).unwrap();
        let fail = status_for(case, Some(&RegexTestCaseStatus::Fail));
        assert_eq!(fail.label, "Fail");
        assert!(fail.details.unwrap().contains("substring"));

        let invalid = status_for(
            case,
            Some(&RegexTestCaseStatus::PatternError(
                crate::regex_tester::RegexValidationError {
                    message: "unclosed group".into(),
                },
            )),
        );
        assert_eq!(invalid.label, "Error · invalid pattern");
        assert_eq!(invalid.details.as_deref(), Some("unclosed group"));

        let limited = status_for(
            case,
            Some(&RegexTestCaseStatus::Unevaluated(
                crate::regex_tester::EvaluationSuspension {
                    limit: crate::regex_tester::EvaluationLimit::AggregateTestCaseTextBytes,
                    maximum: 10,
                    observed: 11,
                },
            )),
        );
        assert_eq!(limited.label, "Error · not evaluated");
        assert!(limited.details.unwrap().contains("combined test input size"));
    }

    #[test]
    fn copied_summary_omits_labels_and_sample_inputs() {
        let clipboard = CaptureClipboard::default();
        let mut cases = RegexTestCases::default();
        cases
            .add(
                Some("sensitive label".into()),
                "private sample body".into(),
                RegexTestCaseExpectation::Match,
            )
            .unwrap();
        let summary = RegexTestCaseSummary {
            total: 1,
            pending: 1,
            ..Default::default()
        };
        let feedback = copy_summary(&clipboard, summary).unwrap();
        assert_eq!(feedback, "Copied test case summary");
        let copied = clipboard.text.lock().unwrap().clone().unwrap();
        assert_eq!(copied, "1 case · 0 passed · 0 failed · 0 errors · 1 pending");
        assert_eq!(clipboard.reads.load(Ordering::Relaxed), 0);
        assert_eq!(clipboard.writes.load(Ordering::Relaxed), 1);
        assert!(!copied.contains("sensitive label"));
        assert!(!copied.contains("private sample body"));
    }

    #[test]
    fn test_case_add_duplicate_reorder_and_remove_keep_case_ids_widget_identity_inputs() {
        let mut session = RegexSession::default();
        let first = session
            .draft
            .test_cases
            .add(
                Some("first".into()),
                "line 1\nline 2".into(),
                RegexTestCaseExpectation::NoMatch,
            )
            .unwrap();
        let second = session
            .draft
            .test_cases
            .add(None, "second".into(), RegexTestCaseExpectation::Match)
            .unwrap();
        let copy = session.draft.test_cases.duplicate(first).unwrap();
        assert_ne!(first, copy);
        assert_eq!(session.draft.test_cases.get(copy).unwrap().input, "line 1\nline 2");
        assert!(session.draft.test_cases.move_to(second, 0));
        assert_eq!(session.draft.test_cases.as_slice()[0].id, second);
        assert_eq!(session.draft.test_cases.get(first).unwrap().input, "line 1\nline 2");
        assert_eq!(session.draft.test_cases.get(first).unwrap().label.as_deref(), Some("first"));
        assert!(session.draft.test_cases.remove(copy).is_some());
        assert!(session.draft.test_cases.get(first).is_some());
    }

    #[test]
    fn empty_case_editor_shows_help_and_small_viewport_controls_remain_reachable() {
        let context = egui::Context::default();
        let mut session = RegexSession::default();
        let clipboard = CaptureClipboard::default();
        let mut expanded = true;
        let mut feedback = None;
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(360.0, 96.0));
        let raw_input = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        let mut geometry = None;
        context
            .run_ui(raw_input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.set_max_height(80.0);
                    geometry = Some(show(
                        ui,
                        &mut session,
                        &mut expanded,
                        &clipboard,
                        &mut feedback,
                        true,
                    ));
                });
            })
            .drop_without_applying_deltas();
        let geometry = geometry.unwrap();
        assert!(geometry.add_rect.intersects(screen));
        assert!(geometry.run_rect.intersects(screen));
        assert!(geometry.copy_rect.intersects(screen));
        assert!(session.draft.test_cases.is_empty());

        let position = geometry.add_rect.center();
        let click = egui::RawInput {
            screen_rect: Some(screen),
            events: vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        context
            .run_ui(click, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.set_max_height(80.0);
                    show(
                        ui,
                        &mut session,
                        &mut expanded,
                        &clipboard,
                        &mut feedback,
                        true,
                    );
                });
            })
            .drop_without_applying_deltas();
        assert_eq!(session.draft.test_cases.len(), 1);

        let added_revision = session.test_case_revision();
        let main_revision = session.revision();
        let mut next_geometry = None;
        context
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.set_max_height(80.0);
                        next_geometry = Some(show(
                            ui,
                            &mut session,
                            &mut expanded,
                            &clipboard,
                            &mut feedback,
                            true,
                        ));
                    });
                },
            )
            .drop_without_applying_deltas();
        let run_position = next_geometry.unwrap().run_rect.center();
        let run_click = egui::RawInput {
            screen_rect: Some(screen),
            events: vec![
                egui::Event::PointerMoved(run_position),
                egui::Event::PointerButton {
                    pos: run_position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: run_position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        context
            .run_ui(run_click, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.set_max_height(80.0);
                    show(
                        ui,
                        &mut session,
                        &mut expanded,
                        &clipboard,
                        &mut feedback,
                        true,
                    );
                });
            })
            .drop_without_applying_deltas();
        assert_eq!(session.test_case_revision(), added_revision + 1);
        assert_eq!(session.revision(), main_revision);
    }

    #[test]
    fn discarded_viewport_pass_does_not_apply_case_actions_or_copy() {
        let context = egui::Context::default();
        let mut session = RegexSession::default();
        let clipboard = CaptureClipboard::default();
        let mut expanded = true;
        let mut feedback = None;
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        let mut geometry = render_panel(
            &context,
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            &mut session,
            &mut expanded,
            &clipboard,
            &mut feedback,
            true,
        );

        geometry = render_panel(
            &context,
            click_input(screen, geometry.add_rect.center()),
            &mut session,
            &mut expanded,
            &clipboard,
            &mut feedback,
            false,
        );
        assert!(session.draft.test_cases.is_empty());
        assert_eq!(session.test_case_revision(), 0);

        geometry = render_panel(
            &context,
            click_input(screen, geometry.add_rect.center()),
            &mut session,
            &mut expanded,
            &clipboard,
            &mut feedback,
            true,
        );
        assert_eq!(session.draft.test_cases.len(), 1);
        assert_eq!(session.test_case_revision(), 1);

        let copy_position = geometry.copy_rect.center();
        render_panel(
            &context,
            click_input(screen, copy_position),
            &mut session,
            &mut expanded,
            &clipboard,
            &mut feedback,
            false,
        );
        assert_eq!(clipboard.writes.load(Ordering::Relaxed), 0);

        render_panel(
            &context,
            click_input(screen, copy_position),
            &mut session,
            &mut expanded,
            &clipboard,
            &mut feedback,
            true,
        );
        assert_eq!(clipboard.writes.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn case_text_widget_ids_follow_stable_case_ids_across_reordering() {
        let mut cases = RegexTestCases::default();
        let first = cases
            .add(None, "first\nline".into(), RegexTestCaseExpectation::Match)
            .unwrap();
        let second = cases
            .add(None, "second".into(), RegexTestCaseExpectation::NoMatch)
            .unwrap();
        let first_label_id = label_widget_id(first);
        let first_input_id = input_widget_id(first);
        assert!(cases.move_to(second, 0));
        assert_eq!(label_widget_id(first), first_label_id);
        assert_eq!(input_widget_id(first), first_input_id);
        assert_ne!(input_widget_id(first), input_widget_id(second));
    }

    #[test]
    fn optional_label_edit_preserves_multiword_and_trailing_space_input() {
        assert_eq!(label_value("my case"), Some("my case".to_owned()));
        assert_eq!(label_value("my case "), Some("my case ".to_owned()));
        assert_eq!(label_value(""), None);
    }

    #[test]
    fn typing_a_multiword_label_across_frames_preserves_its_separator() {
        let context = egui::Context::default();
        let mut session = RegexSession::default();
        let case_id = session
            .draft
            .test_cases
            .add(None, String::new(), RegexTestCaseExpectation::Match)
            .unwrap();
        let clipboard = CaptureClipboard::default();
        let mut expanded = true;
        let mut feedback = None;
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));

        render_panel(
            &context,
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            &mut session,
            &mut expanded,
            &clipboard,
            &mut feedback,
            true,
        );
        context
            .memory_mut(|memory| memory.request_focus(label_widget_id(case_id)));

        render_panel(
            &context,
            egui::RawInput {
                screen_rect: Some(screen),
                events: vec![egui::Event::Text("my ".to_owned())],
                ..Default::default()
            },
            &mut session,
            &mut expanded,
            &clipboard,
            &mut feedback,
            true,
        );
        assert_eq!(
            session.draft.test_cases.get(case_id).unwrap().label.as_deref(),
            Some("my ")
        );

        render_panel(
            &context,
            egui::RawInput {
                screen_rect: Some(screen),
                events: vec![egui::Event::Text("case".to_owned())],
                ..Default::default()
            },
            &mut session,
            &mut expanded,
            &clipboard,
            &mut feedback,
            true,
        );
        assert_eq!(
            session.draft.test_cases.get(case_id).unwrap().label.as_deref(),
            Some("my case")
        );
    }

    #[test]
    fn passive_case_panel_render_does_not_read_or_write_clipboard() {
        let context = egui::Context::default();
        let clipboard = CaptureClipboard::default();
        let mut session = RegexSession::default();
        session
            .draft
            .test_cases
            .add(
                Some("private label".into()),
                "private sample".into(),
                RegexTestCaseExpectation::Match,
            )
            .unwrap();
        let mut expanded = true;
        let mut feedback = None;
        context
            .run_ui(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show(
                        ui,
                        &mut session,
                        &mut expanded,
                        &clipboard,
                        &mut feedback,
                        true,
                    );
                });
            })
            .drop_without_applying_deltas();
        assert_eq!(clipboard.reads.load(Ordering::Relaxed), 0);
        assert_eq!(clipboard.writes.load(Ordering::Relaxed), 0);
    }
}
