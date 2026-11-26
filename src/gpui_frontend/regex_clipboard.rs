//! Explicit protected clipboard actions for the retained Regex workspace.
use super::regex_tester::RegexWorkspace;
use super::*;
use multi_launcher::regex_tester::{CaptureValue, MatchCompleteness, EvaluationResult, SubstitutionEvaluationResult};

#[derive(Clone, Copy, Debug)]
pub(super) enum Action {
    Import,
    Pattern,
    Match,
    Capture,
    All,
    Information,
    Replacement,
    Cases,
    Reference(&'static str),
}

#[derive(Default)]
pub(super) struct Epochs {
    pub source: Arc<AtomicU64>,
    pub pattern: Arc<AtomicU64>,
    pub matches: Arc<AtomicU64>,
    pub selection: Arc<AtomicU64>,
    pub capture: Arc<AtomicU64>,
    pub replacement: Arc<AtomicU64>,
    pub cases: Arc<AtomicU64>,
    pub reference: Arc<AtomicU64>,
}
impl Epochs {
    fn for_action(&self, action: Action) -> Arc<AtomicU64> {
        match action {
            Action::Import => self.source.clone(),
            Action::Pattern => self.pattern.clone(),
            Action::All => self.matches.clone(),
            Action::Match | Action::Information => self.selection.clone(),
            Action::Capture => self.capture.clone(),
            Action::Replacement => self.replacement.clone(),
            Action::Cases => self.cases.clone(),
            Action::Reference(_) => self.reference.clone(),
        }
    }
    pub fn main_changed(&self, cases_changed: bool) {
        for epoch in [&self.matches, &self.selection, &self.capture, &self.replacement] {
            epoch.fetch_add(1, Ordering::AcqRel);
        }
        if cases_changed { self.cases.fetch_add(1, Ordering::AcqRel); }
    }
}

fn clear_wait(view: &mut RegexWorkspace, cx: &mut Context<RegexWorkspace>) {
    let _ = view.root.update(cx, |root, cx| {
        if root.restart_status.as_deref().is_some_and(|status| matches!(status,
            "Wait for the Regex Tester clipboard operation before restarting."
            | "Wait for the Regex Tester clipboard operation before quitting."
            | "Wait for the Regex Tester clipboard operation before closing.")) {
            root.restart_status = None;
            cx.notify();
        }
    });
}

fn available(session: &multi_launcher::regex_tester::RegexSession, capture: usize, action: Action) -> bool {
    let selected = session.selected_index().and_then(|index| session.matches().get(index));
    match action {
        Action::Import | Action::Pattern | Action::Cases | Action::Reference(_) => true,
        Action::Match | Action::Information => selected.is_some(),
        Action::Capture => selected.and_then(|item| item.captures.get(capture))
            .is_some_and(|capture| matches!(&capture.value, CaptureValue::Matched { .. })),
        Action::All => !session.matches().is_empty(),
        Action::Replacement => session.substitution_enabled()
            && matches!(session.substitution_result(), Some(SubstitutionEvaluationResult::Success(_))),
    }
}

fn payload(session: &multi_launcher::regex_tester::RegexSession, capture: usize, action: Action) -> Option<String> {
    let selected = session.selected_index().and_then(|index| session.matches().get(index));
    match action {
        Action::Import => None,
        Action::Reference(text) => Some(text.to_owned()),
        Action::Pattern => Some(session.draft.pattern.clone()),
        Action::Match => selected.map(|item| item.text.clone()),
        Action::Capture => match &selected?.captures.get(capture)?.value {
            CaptureValue::Matched { text, .. } => Some(text.clone()),
            CaptureValue::Unmatched => None,
        },
        Action::All => (!session.matches().is_empty())
            .then(|| multi_launcher::regex_tester::clipboard_format::all_matches(session.matches())),
        Action::Information => selected.map(multi_launcher::regex_tester::clipboard_format::match_information),
        Action::Replacement => match session.substitution_result() {
            Some(SubstitutionEvaluationResult::Success(result)) if session.substitution_enabled() => Some(result.output.clone()),
            _ => None,
        },
        Action::Cases => Some(multi_launcher::regex_tester::clipboard_format::case_summary(
            session.test_case_summary().copied().unwrap_or_default()
        )),
    }
}

fn execute(
    backend: &dyn ClipboardBackend,
    text: Option<&str>,
    current: &dyn Fn() -> bool,
) -> Result<Option<String>, String> {
    if !current() {
        return Err("Clipboard request was superseded.".to_owned());
    }
    match text {
        Some(text) => match backend.write_text_if(text, current) {
            Ok(true) => Ok(None),
            Ok(false) => Err("Clipboard request was superseded.".to_owned()),
            Err(error) => Err(format!("Could not copy: {error}")),
        },
        None => backend.read_text().map(Some)
            .map_err(|error| format!("Could not read clipboard: {error}")),
    }
}

impl RegexWorkspace {
    pub(super) fn clipboard_action(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || self.rehosting || self.clipboard_pending || self.admission.is_fenced()
            || !self.mount.receipt.is_current() || !window.is_window_active()
            || window.window_handle().window_id() != self.mount.window.window_id()
        {
            return;
        }
        let Some(native) = self.mount.native else { return; };
        if current_foreground_identity() != Some(native.identity) { return; }
        let text = payload(&self.session, self.selected_capture, action);
        if !matches!(action, Action::Import) && text.is_none() { return; }
        let Some(permit) = self.admission.admit() else { return; };
        self.clipboard_pending = true;
        self.clipboard_feedback = None;
        let epoch = self.clipboard_epochs.for_action(action);
        let publication_epoch = epoch.clone();
        let expected = epoch.load(Ordering::Acquire);
        let mount = self.mount.receipt.clone();
        let worker_mount = mount.clone();
        let host = self.mount.window;
        let visibility = self.visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let admission = self.admission.clone();
        let popout = self.popout;
        let task = cx.background_executor().spawn(async move {
            let current = || worker_mount.is_current() && epoch.load(Ordering::Acquire) == expected
                && !admission.is_fenced()
                && json_utility::native_host_current(&visibility, revision, Some(native), popout)
                && current_foreground_identity() == Some(native.identity);
            execute(&ArboardClipboardBackend, text.as_deref(), &current)
        });
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let _permit = permit;
            let result = task.await;
            let Some(view) = weak.upgrade() else { return; };
            let update = update_kit_window_content(host, &view, cx, |view, window, cx| {
                view.clipboard_pending = false;
                let current = view.open && mount.is_current()
                    && publication_epoch.load(Ordering::Acquire) == expected
                    && !view.admission.is_fenced()
                    && json_utility::native_host_current(&view.visibility, revision, Some(native), popout)
                    && current_foreground_identity() == Some(native.identity);
                if current {
                    match result {
                        Ok(Some(text)) => {
                            if text.len() > view.session.policy().text_bytes
                                && view.mount.text.focus_handle(cx).is_focused(window) {
                                view.focus_anchor(window, cx);
                            }
                            view.session.draft.test_text = text.clone();
                            view.clipboard_epochs.source.fetch_add(1, Ordering::AcqRel);
                            view.mount.text.update(cx, |input, cx| input.set_value(text, window, cx));
                            view.changed(false, cx);
                            view.clipboard_feedback = Some("Imported clipboard text.".into());
                        }
                        Ok(None) => view.clipboard_feedback = Some("Copied to clipboard.".into()),
                        Err(error) => view.clipboard_feedback = Some(error),
                    }
                }
                clear_wait(view, cx);
                cx.notify();
            });
            if update.is_err() {
                let _ = view.update(cx, |view, cx| {
                    view.clipboard_pending = false;
                    clear_wait(view, cx);
                    cx.notify();
                });
            }
        }).detach();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, Epochs, available, execute, payload};
    use multi_launcher::clipboard_modify::clipboard::{ClipboardBackend, ClipboardError};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct CountingClipboard {
        reads: AtomicUsize,
        writes: AtomicUsize,
    }
    impl ClipboardBackend for CountingClipboard {
        fn read_text(&self) -> Result<String, ClipboardError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok("clipboard sample".into())
        }
        fn write_text(&self, _: &str) -> Result<(), ClipboardError> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    #[test]
    fn explicit_effect_checks_authority_before_touching_clipboard() {
        let clipboard = CountingClipboard::default();
        assert!(execute(&clipboard, None, &|| false).is_err());
        assert!(execute(&clipboard, Some("sample"), &|| false).is_err());
        assert_eq!(clipboard.reads.load(Ordering::SeqCst), 0);
        assert_eq!(clipboard.writes.load(Ordering::SeqCst), 0);
        assert_eq!(execute(&clipboard, None, &|| true).unwrap().as_deref(), Some("clipboard sample"));
        assert_eq!(execute(&clipboard, Some(""), &|| true).unwrap(), None);
        assert_eq!(clipboard.reads.load(Ordering::SeqCst), 1);
        assert_eq!(clipboard.writes.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn reference_copy_uses_exact_syntax_and_independent_authority() {
        use std::sync::atomic::Ordering;
        let epochs = super::Epochs::default();
        let reference = epochs.for_action(super::Action::Reference("\\p{Greek}"));
        let pattern = epochs.for_action(super::Action::Pattern);
        reference.fetch_add(1, Ordering::AcqRel);
        assert_eq!(pattern.load(Ordering::Acquire), 0);
        let session = multi_launcher::regex_tester::RegexSession::default();
        assert_eq!(super::payload(&session, 0, super::Action::Reference("\\p{Greek}")).as_deref(), Some("\\p{Greek}"));
    }

    #[test]
    fn copy_payload_distinguishes_empty_and_unmatched_captures() {
        let mut session = multi_launcher::regex_tester::RegexSession::default();
        session.draft.pattern = "(a)?(b*)".into();
        session.draft.test_text = "".into();
        let now = Instant::now();
        session.mark_changed(now);
        session.tick(now + Duration::from_millis(200));
        assert!(!available(&session, 0, Action::Capture));
        assert!(available(&session, 1, Action::Capture));
        assert_eq!(payload(&session, 1, Action::Capture).as_deref(), Some(""));
    }
    #[test]
    fn unrelated_authority_survives_and_case_settlement_retires_summary() {
        let epochs = Epochs::default();
        let summary = epochs.for_action(Action::Cases);
        let pattern = epochs.for_action(Action::Pattern);
        let expected_summary = summary.load(Ordering::Acquire);
        let expected_pattern = pattern.load(Ordering::Acquire);
        let mut session = multi_launcher::regex_tester::RegexSession::default();
        let now = Instant::now();
        session.draft.test_cases.add(None, "a".into(), multi_launcher::regex_tester::RegexTestCaseExpectation::Match).unwrap();
        session.mark_test_cases_changed(now);
        let (_, _, cases) = super::super::regex_tester::advance_due(&mut session, now + Duration::from_millis(200));
        assert!(cases);
        if cases { epochs.cases.fetch_add(1, Ordering::AcqRel); }
        assert_ne!(summary.load(Ordering::Acquire), expected_summary);
        assert_eq!(pattern.load(Ordering::Acquire), expected_pattern);
    }
}

pub(super) fn render(view: &mut RegexWorkspace, cx: &mut Context<RegexWorkspace>) -> AnyElement {
    let enabled = view.open && !view.rehosting && !view.clipboard_pending && !view.admission.is_fenced();
    let all_label = if matches!(view.session.result(), Some(EvaluationResult::Success { completeness: MatchCompleteness::Truncated { .. }, .. })) {
        "Copy displayed matches"
    } else { "Copy All Matches" };
    let receipt = view.mount.receipt.clone();
    let capture_stamp = view.session.selected_index().and_then(|index| view.session.matches().get(index))
        .and_then(|item| item.captures.get(view.selected_capture)
            .map(|capture| (view.session.revision(), item.id, capture.group_index)));
    let mut buttons = div().flex().flex_wrap().gap_2();
    for (key, label, action) in [
        ("import", "Use Clipboard Text", Action::Import),
        ("pattern", "Copy Pattern", Action::Pattern),
        ("match", "Copy Selected Match", Action::Match),
        ("capture", "Copy Selected Capture", Action::Capture),
        ("all", all_label, Action::All),
        ("info", "Copy Match Information", Action::Information),
        ("replacement", "Copy Replacement Result", Action::Replacement),
    ] {
        let available = available(&view.session, view.selected_capture, action);
        let current = receipt.clone();
        buttons = buttons.child(Button::new(SharedString::from(format!("regex-clipboard-{key}")))
            .label(label).disabled(!enabled || !available)
            .on_click(cx.listener(move |view, _, window, cx| {
                let capture_current = !matches!(action, Action::Capture) || capture_stamp.is_some_and(|(revision, match_id, group)| {
                    view.session.revision() == revision && view.session.selected_index()
                        .and_then(|index| view.session.matches().get(index))
                        .is_some_and(|item| item.id == match_id && item.captures.get(view.selected_capture)
                            .is_some_and(|capture| capture.group_index == group))
                });
                if current.is_current() && capture_current { view.clipboard_action(action, window, cx); }
            })));
    }
    div().id("regex-clipboard").flex().flex_col().gap_2()
        .child(Label::new("Clipboard"))
        .child(buttons)
        .children(view.clipboard_feedback.as_ref().map(|message| Label::new(message.clone()).id("regex-clipboard-feedback")))
        .into_any_element()
}
