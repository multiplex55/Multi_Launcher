//! Window-local case editors backed by the retained RegexSession.
use super::regex_tester::RegexWorkspace;
use super::*;
use gpui_kit::component::Sizable as _;
use multi_launcher::regex_tester::{
    RegexTestCase, RegexTestCaseCollectionError, RegexTestCaseExpectation, RegexTestCaseId,
    RegexTestCaseStatus, RegexTestCases,
};
use std::time::Instant;

pub(super) struct CaseEditor {
    label: Entity<InputState>,
    input: Entity<TextareaState>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}
impl CaseEditor {
    fn new(
        case: &RegexTestCase,
        receipt: utility_mount::MountReceipt,
        window: &mut Window,
        cx: &mut Context<RegexWorkspace>,
    ) -> Self {
        let id = case.id;
        let label = cx.new(|cx| InputState::new(window, cx).placeholder("Optional case label"));
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 4)
                .placeholder("Case input")
        });
        label.update(cx, |input, cx| {
            input.set_value(case.label.clone().unwrap_or_default(), window, cx)
        });
        input.update(cx, |input, cx| {
            input.set_value(case.input.clone(), window, cx)
        });
        let label_receipt = receipt.clone();
        let label_sub = cx.subscribe(
            &label,
            move |view: &mut RegexWorkspace, input: Entity<InputState>, event: &InputEvent, cx| {
                if !case_live(view, &label_receipt, id) || !matches!(event, InputEvent::Change) {
                    return;
                }
                let value = input.read(cx).value().to_string();
                let value = (!value.is_empty()).then_some(value);
                if let Some(case) = view.session.draft.test_cases.get_mut(id) {
                    if case.label != value {
                        case.label = value;
                        case_changed(view, cx);
                    }
                }
            },
        );
        let input_sub = cx.subscribe(
            &input,
            move |view: &mut RegexWorkspace,
                  input: Entity<TextareaState>,
                  event: &InputEvent,
                  cx| {
                if !case_live(view, &receipt, id) || !matches!(event, InputEvent::Change) {
                    return;
                }
                let value = input.read(cx).value().to_string();
                if let Some(case) = view.session.draft.test_cases.get_mut(id) {
                    if case.input != value {
                        case.input = value;
                        case_changed(view, cx);
                    }
                }
            },
        );
        Self {
            label,
            input,
            focus: cx.focus_handle(),
            _subscriptions: vec![label_sub, input_sub],
        }
    }
}
fn case_live(
    view: &RegexWorkspace,
    receipt: &utility_mount::MountReceipt,
    id: RegexTestCaseId,
) -> bool {
    receipt.is_current()
        && view.open
        && !view.rehosting
        && !view.admission.is_fenced()
        && view.session.draft.test_cases.get(id).is_some()
}
fn case_changed(view: &mut RegexWorkspace, cx: &mut Context<RegexWorkspace>) {
    view.session.mark_test_cases_changed(Instant::now());
    view.cases_error = None;
    view.pump(cx);
    cx.notify();
}
pub(super) fn editor_focused(view: &RegexWorkspace, window: &Window, cx: &App) -> bool {
    view.mount.cases.values().any(|editor| {
        editor.label.focus_handle(cx).is_focused(window)
            || editor.input.focus_handle(cx).is_focused(window)
    })
}
pub(super) fn sync_editors(
    view: &mut RegexWorkspace,
    window: &mut Window,
    cx: &mut Context<RegexWorkspace>,
) {
    let ids: HashSet<_> = view
        .session
        .draft
        .test_cases
        .iter()
        .map(|case| case.id)
        .collect();
    view.mount.cases.retain(|id, _| ids.contains(id));
    let missing: Vec<_> = view
        .session
        .draft
        .test_cases
        .iter()
        .filter(|case| !view.mount.cases.contains_key(&case.id))
        .cloned()
        .collect();
    for case in missing {
        let editor = CaseEditor::new(&case, view.mount.receipt.clone(), window, cx);
        view.mount.cases.insert(case.id, editor);
    }
}
fn status_text(status: Option<&RegexTestCaseStatus>) -> String {
    match status {
        None | Some(RegexTestCaseStatus::Pending) => "Pending".into(),
        Some(RegexTestCaseStatus::Pass) => "Pass".into(),
        Some(RegexTestCaseStatus::Fail) => "Fail".into(),
        Some(RegexTestCaseStatus::PatternError(error)) => {
            format!("Invalid pattern: {}", error.message)
        }
        Some(RegexTestCaseStatus::Unevaluated(reason)) => format!("Not evaluated: {reason:?}"),
    }
}
#[derive(Clone, Copy)]
enum CaseAction {
    Duplicate,
    Remove,
    Up,
    Down,
    Toggle,
}
impl CaseAction {
    fn token(self) -> &'static str {
        match self {
            Self::Duplicate => "duplicate",
            Self::Remove => "remove",
            Self::Up => "up",
            Self::Down => "down",
            Self::Toggle => "expectation",
        }
    }
}
fn case_error(error: RegexTestCaseCollectionError) -> &'static str {
    match error {
        RegexTestCaseCollectionError::LimitReached => "The test case limit is 100 cases.",
        RegexTestCaseCollectionError::IdentifierSpaceExhausted => {
            "No more test case identifiers are available in this session."
        }
        RegexTestCaseCollectionError::DuplicateIdentifier => {
            "The case list contains a duplicate identifier."
        }
        RegexTestCaseCollectionError::MissingCase => "The selected case no longer exists.",
    }
}
fn apply_action(
    cases: &mut RegexTestCases,
    id: RegexTestCaseId,
    action: CaseAction,
) -> Result<bool, RegexTestCaseCollectionError> {
    let index = cases
        .as_slice()
        .iter()
        .position(|case| case.id == id)
        .ok_or(RegexTestCaseCollectionError::MissingCase)?;
    match action {
        CaseAction::Duplicate => cases.duplicate(id).map(|_| true),
        CaseAction::Remove => Ok(cases.remove(id).is_some()),
        CaseAction::Up | CaseAction::Down => {
            let target = if matches!(action, CaseAction::Up) {
                index.saturating_sub(1)
            } else {
                (index + 1).min(cases.len().saturating_sub(1))
            };
            if target == index {
                return Ok(false);
            }
            Ok(cases.move_to(id, target))
        }
        CaseAction::Toggle => {
            let case = cases
                .get_mut(id)
                .ok_or(RegexTestCaseCollectionError::MissingCase)?;
            case.expected = match case.expected {
                RegexTestCaseExpectation::Match => RegexTestCaseExpectation::NoMatch,
                RegexTestCaseExpectation::NoMatch => RegexTestCaseExpectation::Match,
            };
            Ok(true)
        }
    }
}
fn apply(
    view: &mut RegexWorkspace,
    receipt: &utility_mount::MountReceipt,
    id: RegexTestCaseId,
    action: CaseAction,
    window: &mut Window,
    cx: &mut Context<RegexWorkspace>,
) {
    if !case_live(view, receipt, id) {
        return;
    }
    let removing_focused = matches!(action, CaseAction::Remove)
        && view.mount.cases.get(&id).is_some_and(|editor| {
            editor.focus.is_focused(window) || editor.focus.contains_focused(window, cx)
        });
    match apply_action(&mut view.session.draft.test_cases, id, action) {
        Ok(true) => {
            if removing_focused {
                view.focus_anchor(window, cx);
            }
            sync_editors(view, window, cx);
            case_changed(view, cx);
        }
        Ok(false) => {}
        Err(error) => {
            view.cases_error = Some(case_error(error).into());
            cx.notify();
        }
    }
}
pub(super) fn render(view: &mut RegexWorkspace, cx: &mut Context<RegexWorkspace>) -> AnyElement {
    let receipt = view.mount.receipt.clone();
    let enabled = view.open && !view.rehosting && !view.admission.is_fenced();
    let expanded = view.cases_expanded;
    let at_limit =
        view.session.draft.test_cases.len() >= view.session.test_case_policy().maximum_cases;
    let has_cases = !view.session.draft.test_cases.is_empty();
    let summary = view
        .session
        .test_case_summary()
        .copied()
        .unwrap_or_default();
    let toggle_receipt = receipt.clone();
    let add_receipt = receipt.clone();
    let run_receipt = receipt.clone();
    let mut panel = div()
        .id("regex-test-cases")
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(
                    Button::new("regex-cases-disclosure")
                        .label(if expanded {
                            "▾ Test Cases"
                        } else {
                            "▸ Test Cases"
                        })
                        .disabled(!enabled)
                        .on_click(cx.listener(move |view, _, _, cx| {
                            if toggle_receipt.is_current()
                                && view.open
                                && !view.rehosting
                                && !view.admission.is_fenced()
                            {
                                view.cases_expanded = !view.cases_expanded;
                                cx.notify();
                            }
                        })),
                )
                .child(
                    Button::new("regex-cases-add")
                        .label("Add")
                        .disabled(!enabled || at_limit)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if !add_receipt.is_current()
                                || !view.open
                                || view.rehosting
                                || view.admission.is_fenced()
                            {
                                return;
                            }
                            match view.session.draft.test_cases.add(
                                None,
                                String::new(),
                                RegexTestCaseExpectation::Match,
                            ) {
                                Ok(_) => {
                                    sync_editors(view, window, cx);
                                    case_changed(view, cx);
                                }
                                Err(error) => {
                                    view.cases_error = Some(case_error(error).into());
                                    cx.notify();
                                }
                            }
                        })),
                )
                .child(
                    Button::new("regex-cases-run")
                        .label(
                            if view.session.test_case_revision() == 0 || summary.pending > 0 {
                                "Run"
                            } else {
                                "Rerun"
                            },
                        )
                        .disabled(!enabled || !has_cases)
                        .on_click(cx.listener(move |view, _, _, cx| {
                            if run_receipt.is_current()
                                && view.open
                                && !view.rehosting
                                && !view.admission.is_fenced()
                                && !view.session.draft.test_cases.is_empty()
                            {
                                case_changed(view, cx);
                            }
                        })),
                ),
        );
    if !expanded {
        return panel.into_any_element();
    }
    let cases = view.session.draft.test_cases.as_slice();
    let results = view.session.test_case_evaluation();
    panel = panel
        .child(
            Label::new(format!(
                "{} cases · {} passed · {} failed · {} errors · {} pending",
                summary.total,
                summary.passed,
                summary.failed,
                summary.pattern_errors + summary.unevaluated,
                summary.pending
            ))
            .id("regex-cases-summary"),
        )
        .child(Label::new(
            "Match means any substring. Use ^…$ for a whole-input match.",
        ))
        .children(
            view.cases_error
                .as_ref()
                .map(|error| Label::new(error.clone()).id("regex-cases-error")),
        );
    let mut body = div()
        .id("regex-cases-scroll")
        .max_h(px(420.0))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap_2();
    for (index, case) in cases.iter().enumerate() {
        let Some(editor) = view.mount.cases.get(&case.id) else {
            continue;
        };
        let status = results
            .and_then(|result| result.results.iter().find(|item| item.case_id == case.id))
            .map(|item| &item.status);
        let id = case.id;
        let mut controls = div().flex().flex_wrap().gap_1();
        for (action, label, disabled) in [
            (CaseAction::Duplicate, "Duplicate", at_limit),
            (CaseAction::Remove, "Remove", false),
            (CaseAction::Up, "Move Up", index == 0),
            (CaseAction::Down, "Move Down", index + 1 == cases.len()),
            (
                CaseAction::Toggle,
                if case.expected == RegexTestCaseExpectation::Match {
                    "Expected: Match"
                } else {
                    "Expected: No match"
                },
                false,
            ),
        ] {
            let captured = receipt.clone();
            controls = controls.child(
                Button::new(SharedString::from(format!(
                    "regex-case-{}-{}",
                    id.value(),
                    action.token()
                )))
                .label(label)
                .small()
                .disabled(!enabled || disabled)
                .on_click(cx.listener(move |view, _, window, cx| {
                    apply(view, &captured, id, action, window, cx)
                })),
            );
        }
        body = body.child(
            div()
                .id(SharedString::from(format!("regex-case-{}", id.value())))
                .track_focus(&editor.focus)
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    Input::new(&editor.label)
                        .disabled(!enabled)
                        .aria_label("Test case label"),
                )
                .child(
                    Textarea::new(&editor.input)
                        .readonly(!enabled)
                        .aria_label("Test case input"),
                )
                .child(
                    Label::new(status_text(status)).id(SharedString::from(format!(
                        "regex-case-status-{}",
                        id.value()
                    ))),
                )
                .child(controls),
        );
    }
    panel.child(body).into_any_element()
}
#[cfg(test)]
mod tests {
    use super::{CaseAction, apply_action, status_text};
    use multi_launcher::regex_tester::{
        RegexTestCaseExpectation, RegexTestCaseStatus, RegexTestCases,
    };
    #[test]
    fn case_actions_preserve_exact_ids_and_reject_removed_case() {
        assert_eq!(CaseAction::Toggle.token(), "expectation");
        let mut cases = RegexTestCases::default();
        let first = cases
            .add(None, "a".into(), RegexTestCaseExpectation::Match)
            .unwrap();
        assert!(apply_action(&mut cases, first, CaseAction::Duplicate).unwrap());
        let second = cases.as_slice()[1].id;
        assert_ne!(first, second);
        assert!(apply_action(&mut cases, second, CaseAction::Up).unwrap());
        assert_eq!(cases.as_slice()[0].id, second);
        assert!(apply_action(&mut cases, first, CaseAction::Toggle).unwrap());
        assert_eq!(
            cases.get(first).unwrap().expected,
            RegexTestCaseExpectation::NoMatch
        );
        assert!(apply_action(&mut cases, first, CaseAction::Remove).unwrap());
        assert!(apply_action(&mut cases, first, CaseAction::Duplicate).is_err());
        assert_eq!(cases.as_slice()[0].id, second);
    }
    #[test]
    fn case_status_does_not_confuse_pending_and_failure() {
        assert_eq!(status_text(None), "Pending");
        assert_eq!(status_text(Some(&RegexTestCaseStatus::Pass)), "Pass");
        assert_eq!(status_text(Some(&RegexTestCaseStatus::Fail)), "Fail");
    }
}
