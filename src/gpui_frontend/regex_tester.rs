//! Retained Regex Tester editor backed by the shared matching session.
use super::*;
use gpui_kit::base::Selectable;
use multi_launcher::regex_tester::{
    CaptureValue, EvaluationResult, MatchCompleteness, RegexSession,
};
use std::time::Instant;

pub(super) fn provider_is_current(
    query: &str,
    action: &Action,
    enabled: Option<&HashSet<String>>,
    caps: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    let plugin = multi_launcher::plugins::regex_tester::RegexTesterPlugin;
    search_providers::search_routed(
        [&plugin as &dyn multi_launcher::plugin::Plugin],
        query,
        enabled,
        caps,
    )
    .iter()
    .any(|(name, row)| name == "regex_tester" && row == action)
}
fn timer_current(open: bool, current: u64, expected: u64) -> bool {
    open && current == expected
}
fn pending_timer(
    session: &RegexSession,
    pumping: &mut bool,
    now: Instant,
) -> Option<std::time::Duration> {
    let delay = session.pending_delay(now);
    if delay.is_none() {
        *pumping = false;
    }
    delay
}
fn preview(text: &str) -> String {
    let mut chars = text.chars();
    let mut output: String = chars.by_ref().take(160).collect();
    if chars.next().is_some() {
        output.push('…');
    }
    if output.is_empty() {
        "(empty)".into()
    } else {
        output
    }
}
pub(super) struct RegexWorkspace {
    pub open: bool,
    root: WeakEntity<RootWindow>,
    admission: Arc<RestartAdmission>,
    session: RegexSession,
    pattern: Entity<InputState>,
    text: Entity<TextareaState>,
    focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    _subscriptions: Vec<Subscription>,
    generation: u64,
    pumping: bool,
    scroll: gpui_kit::UniformListScrollHandle,
}
impl RegexWorkspace {
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        root: WeakEntity<RootWindow>,
        admission: Arc<RestartAdmission>,
    ) -> Self {
        let pattern =
            cx.new(|cx| InputState::new(window, cx).placeholder("Enter a Rust regex pattern"));
        let text = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(5, 12)
                .placeholder("Enter test text")
        });
        let pattern_sub = cx.subscribe(
            &pattern,
            |view: &mut Self, input: Entity<InputState>, event: &InputEvent, cx| {
                if view.open && !view.admission.is_fenced() && matches!(event, InputEvent::Change) {
                    let next = input.read(cx).value().to_string();
                    if next != view.session.draft.pattern {
                        view.session.draft.pattern = next;
                        view.changed(cx);
                    }
                }
            },
        );
        let text_sub = cx.subscribe(
            &text,
            |view: &mut Self, input: Entity<TextareaState>, event: &InputEvent, cx| {
                if view.open && !view.admission.is_fenced() && matches!(event, InputEvent::Change) {
                    let next = input.read(cx).value().to_string();
                    if next != view.session.draft.test_text {
                        view.session.draft.test_text = next;
                        view.changed(cx);
                    }
                }
            },
        );
        Self {
            open: false,
            root,
            admission,
            session: RegexSession::default(),
            pattern,
            text,
            focus: cx.focus_handle(),
            return_focus: None,
            _subscriptions: vec![pattern_sub, text_sub],
            generation: 0,
            pumping: false,
            scroll: Default::default(),
        }
    }
    fn changed(&mut self, cx: &mut Context<Self>) {
        self.session.mark_changed(Instant::now());
        self.pump(cx);
        cx.notify();
    }
    fn pump(&mut self, cx: &mut Context<Self>) {
        if self.pumping || !self.open || self.session.pending_delay(Instant::now()).is_none() {
            return;
        }
        self.pumping = true;
        let generation = self.generation;
        let owner = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            loop {
                let delay = match owner.update(cx, |view, _| {
                    if !timer_current(view.open, view.generation, generation) {
                        None
                    } else {
                        pending_timer(&view.session, &mut view.pumping, Instant::now())
                    }
                }) {
                    Ok(Some(delay)) => delay,
                    _ => break,
                };
                cx.background_executor().timer(delay).await;
                let more = owner
                    .update(cx, |view, cx| {
                        if !timer_current(view.open, view.generation, generation) {
                            return false;
                        }
                        view.session.tick(Instant::now());
                        let more = view.session.pending_delay(Instant::now()).is_some();
                        if !more {
                            view.pumping = false;
                        }
                        cx.notify();
                        more
                    })
                    .unwrap_or(false);
                if !more {
                    break;
                }
            }
        })
        .detach();
    }
    fn reopen(&mut self, window: &mut Window, cx: &mut Context<Self>, origin: FocusHandle) {
        if !self.open {
            self.generation = self.generation.wrapping_add(1);
            self.open = true;
            self.return_focus = Some(origin);
            self.session.ensure_initial_evaluation(Instant::now());
            self.pump(cx);
        }
        self.pattern.focus_handle(cx).focus(window, cx);
        cx.notify();
    }
    pub(super) fn shutdown(&mut self) {
        self.open = false;
        self.generation = self.generation.wrapping_add(1);
        self.pumping = false;
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        let restore = window.is_window_active() && self.focus.contains_focused(window, cx);
        self.shutdown();
        let _ = self.root.update(cx, |root, _| {
            panel_stack::retire(&mut root.panel_stack, Panel::RegexTesterDialog)
        });
        if restore {
            if let Some(focus) = self.return_focus.take() {
                focus.focus(window, cx);
            }
        }
        cx.notify();
    }
    pub(super) fn draft(&self) -> Option<multi_launcher::regex_tester::RegexDraft> {
        (self.session.draft != multi_launcher::regex_tester::RegexDraft::default())
            .then(|| self.session.draft.clone())
    }
    fn navigate(&mut self, backwards: bool, cx: &mut Context<Self>) {
        if self.admission.is_fenced() {
            return;
        }
        if if backwards {
            self.session.previous_match()
        } else {
            self.session.next_match()
        } {
            if let Some(index) = self.session.selected_index() {
                self.scroll
                    .scroll_to_item(index, gpui_kit::ScrollStrategy::Nearest);
            }
            cx.notify();
        }
    }
}
impl Render for RegexWorkspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let flags = self.session.draft.flags;
        let mut flag_row = div().flex().flex_wrap().gap_2();
        for (index, label, selected) in [
            (0, "i · case insensitive", flags.case_insensitive),
            (1, "m · multi-line", flags.multi_line),
            (2, "s · dot matches newline", flags.dot_matches_new_line),
            (3, "u · Unicode", flags.unicode),
            (4, "x · ignore whitespace", flags.ignore_whitespace),
        ] {
            let owner = cx.entity().downgrade();
            flag_row = flag_row.child(
                Checkbox::new(SharedString::from(format!("regex-flag-{index}")))
                    .checked(selected)
                    .disabled(self.admission.is_fenced())
                    .accessibility_label(label)
                    .on_change(move |state, _, _, app| {
                        let _ = owner.update(app, |view, cx| {
                            if !view.open || view.admission.is_fenced() {
                                return;
                            }
                            let value = state == CheckboxState::Checked;
                            match index {
                                0 => view.session.draft.flags.case_insensitive = value,
                                1 => view.session.draft.flags.multi_line = value,
                                2 => view.session.draft.flags.dot_matches_new_line = value,
                                3 => view.session.draft.flags.unicode = value,
                                _ => view.session.draft.flags.ignore_whitespace = value,
                            }
                            view.changed(cx);
                        });
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                CheckboxIndicator::new()
                                    .checked(selected)
                                    .size(px(16.0))
                                    .border_1()
                                    .child(if selected { "✓" } else { "" }),
                            )
                            .child(Label::new(label)),
                    ),
            );
        }
        let status = match self.session.result() {
            None => "Evaluating…".to_owned(),
            Some(EvaluationResult::InvalidPattern(error)) => {
                format!("Invalid pattern: {}", error.message)
            }
            Some(EvaluationResult::Suspended(reason)) => {
                format!("Evaluation suspended: {reason:?}")
            }
            Some(EvaluationResult::Success {
                matches,
                completeness,
            }) => match completeness {
                MatchCompleteness::Complete => format!("{} matches", matches.len()),
                MatchCompleteness::Truncated { reason, at_least } => format!(
                    "{} displayed matches; truncated ({reason:?}, at least {at_least})",
                    matches.len()
                ),
            },
        };
        let revision = self.session.revision();
        let count = self.session.matches().len();
        let owner = cx.entity().downgrade();
        let rows = gpui_kit::uniform_list("regex-match-rows", count, move |range, _, app| {
            let Some(view) = owner.upgrade() else {
                return Vec::<AnyElement>::new();
            };
            let state = view.read(app);
            if !state.open || state.session.revision() != revision {
                return Vec::new();
            }
            range
                .filter_map(|index| {
                    let item = state.session.matches().get(index)?;
                    let label = format!(
                        "{} · {}:{} · {}",
                        index + 1,
                        item.location.line,
                        item.location.column,
                        preview(&item.text)
                    );
                    let target = owner.clone();
                    Some(
                        div()
                            .id(SharedString::from(format!(
                                "regex-match-{revision}-{index}"
                            )))
                            .h(px(32.0))
                            .child(
                                Button::new(SharedString::from(format!(
                                    "regex-select-{revision}-{index}"
                                )))
                                .small()
                                .selected(state.session.selected_index() == Some(index))
                                .label(label)
                                .on_click(move |_, _, app| {
                                    let _ = target.update(app, |view, cx| {
                                        if view.open
                                            && !view.admission.is_fenced()
                                            && view.session.revision() == revision
                                            && index < view.session.matches().len()
                                        {
                                            view.session.select_match(index);
                                            cx.notify();
                                        }
                                    });
                                }),
                            )
                            .into_any_element(),
                    )
                })
                .collect()
        })
        .track_scroll(&self.scroll)
        .h(px(192.0));
        let selected_match = self
            .session
            .selected_index()
            .and_then(|index| self.session.matches().get(index).map(|item| (index, item)));
        let details = selected_match.map(|(index, item)| {
            let mut rows = vec![(
                0usize,
                format!(
                    "Match {} · bytes {}..{}: {}",
                    index + 1,
                    item.span.start_byte(),
                    item.span.end_byte(),
                    preview(&item.text)
                ),
            )];
            rows.extend(item.captures.iter().map(|capture| {
                let value = match &capture.value {
                    CaptureValue::Unmatched => "(unmatched)".to_owned(),
                    CaptureValue::Matched { text, span } => format!(
                        "bytes {}..{}: {}",
                        span.start_byte(),
                        span.end_byte(),
                        preview(text)
                    ),
                };
                (
                    capture.group_index,
                    format!(
                        "Capture {}{}: {value}",
                        capture.group_index,
                        capture
                            .name
                            .as_ref()
                            .map(|name| format!(" ({name})"))
                            .unwrap_or_default()
                    ),
                )
            }));
            (index, rows)
        });
        div()
            .id("regex-workspace")
            .track_focus(&self.focus)
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .capture_action(
                cx.listener(|view, _: &gpui_kit::base::input::Escape, window, cx| {
                    let contexts = window.context_stack();
                    if view.open
                        && view.focus.contains_focused(window, cx)
                        && !window.has_active_dialog(cx)
                        && !context_stack_has_primary(&contexts, "PopupMenu")
                        && !context_stack_has_primary(&contexts, "Dialog")
                        && !context_stack_has_primary(&contexts, "AlertDialog")
                    {
                        view.close(window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .capture_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                let contexts = window.context_stack();
                if view.open
                    && !view.admission.is_fenced()
                    && !context_stack_has_primary(&contexts, "PopupMenu")
                    && !context_stack_has_primary(&contexts, "Dialog")
                    && !context_stack_has_primary(&contexts, "AlertDialog")
                    && event.keystroke.key == "f3"
                    && !event.keystroke.modifiers.control
                    && !event.keystroke.modifiers.alt
                    && !event.keystroke.modifiers.platform
                    && view.focus.contains_focused(window, cx)
                    && !window.has_active_dialog(cx)
                {
                    view.navigate(event.keystroke.modifiers.shift, cx);
                    cx.stop_propagation();
                }
            }))
            .child(Label::new("Regex Tester"))
            .child(
                Input::new(&self.pattern)
                    .disabled(self.admission.is_fenced())
                    .aria_label("Pattern"),
            )
            .child(flag_row)
            .child(
                Textarea::new(&self.text)
                    .readonly(self.admission.is_fenced())
                    .aria_label("Test Text"),
            )
            .child(div().id("regex-status").role(Role::Status).child(status))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("regex-previous")
                            .label("Previous")
                            .disabled(count == 0)
                            .on_click(cx.listener(|view, _, _, cx| view.navigate(true, cx))),
                    )
                    .child(
                        Button::new("regex-next")
                            .label("Next")
                            .disabled(count == 0)
                            .on_click(cx.listener(|view, _, _, cx| view.navigate(false, cx))),
                    )
                    .child(
                        Button::new("regex-close")
                            .label("Close")
                            .on_click(cx.listener(|view, _, window, cx| view.close(window, cx))),
                    ),
            )
            .child(rows)
            .children(details.map(|(index, rows)| {
                div()
                    .id("regex-capture-details")
                    .max_h(px(220.0))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .children(rows.into_iter().map(move |(group, text)| {
                        Label::new(text).id(SharedString::from(format!(
                            "regex-capture-{revision}-{index}-{group}"
                        )))
                    }))
            }))
    }
}
impl RootWindow {
    pub(super) fn open_regex_tester(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced() {
            return;
        }
        let root = cx.entity().downgrade();
        let admission = self.restart_admission.clone();
        let view = self
            .regex_view
            .clone()
            .unwrap_or_else(|| cx.new(|cx| RegexWorkspace::new(window, cx, root, admission)));
        let origin = self.query.focus_handle(cx);
        view.update(cx, |view, cx| view.reopen(window, cx, origin));
        self.regex_view = Some(view);
        let index = self.file_search_scroll_item()
            + usize::from(
                self.file_search
                    .as_ref()
                    .is_some_and(|view| view.read(cx).open),
            )
            + usize::from(self.downloads_triage_open(cx))
            + usize::from(
                self.plugin_editor
                    .as_ref()
                    .is_some_and(|view| view.read(cx).open),
            )
            + usize::from(
                self.activity_view
                    .as_ref()
                    .is_some_and(|view| view.read(cx).open),
            )
            + usize::from(self.root_window.is_some_and(|root| {
                self.json_view
                    .as_ref()
                    .is_some_and(|view| view.read(cx).is_mounted_in(root.window_id(), false))
            }))
            + usize::from(self.root_window.is_some_and(|root| {
                self.xml_view
                    .as_ref()
                    .is_some_and(|view| view.read(cx).is_mounted_in(root.window_id(), false))
            }));
        self.root_scroll.scroll_to_top_of_item(index);
        panel_stack::promote(&mut self.panel_stack, Panel::RegexTesterDialog);
        cx.notify();
    }
}
#[cfg(test)]
mod tests {
    use super::{pending_timer, preview, provider_is_current, timer_current};
    use multi_launcher::plugin::Plugin;
    use multi_launcher::plugins::regex_tester::RegexTesterPlugin;
    use multi_launcher::regex_tester::{CaptureValue, EvaluationResult, RegexSession};
    use std::time::{Duration, Instant};
    #[test]
    fn exact_regex_provider_respects_query_action_and_capabilities() {
        let plugin = RegexTesterPlugin;
        let action = plugin.search("ReGeX").pop().unwrap();
        assert!(provider_is_current("ReGeX", &action, None, None));
        assert!(!provider_is_current("regex extra", &action, None, None));
        assert!(!provider_is_current(
            "regex",
            &action,
            Some(&std::collections::HashSet::new()),
            None
        ));
        let disabled = std::collections::HashMap::from([("regex_tester".to_owned(), Vec::new())]);
        assert!(!provider_is_current(
            "regex",
            &action,
            None,
            Some(&disabled)
        ));
    }
    #[test]
    fn shared_session_preserves_capture_kind_and_navigation_across_debounce() {
        let mut session = RegexSession::default();
        session.draft.pattern = "(a)?(b*)".into();
        session.draft.test_text = "b".into();
        let now = Instant::now();
        session.mark_changed(now);
        assert!(session.result().is_none());
        assert!(!session.tick(now));
        assert!(session.tick(now + Duration::from_millis(200)));
        assert!(matches!(
            session.result(),
            Some(EvaluationResult::Success { .. })
        ));
        let captures = &session.matches()[0].captures;
        assert!(matches!(&captures[0].value, CaptureValue::Unmatched));
        assert!(matches!(&captures[1].value, CaptureValue::Matched { text, .. } if text == "b"));
        let original = session.draft.test_text.clone();
        assert!(session.next_match());
        assert!(session.previous_match());
        assert_eq!(session.draft.test_text, original);
        assert!(timer_current(true, 7, 7));
        assert!(!timer_current(false, 7, 7));
        assert!(!timer_current(true, 8, 7));
    }
    #[test]
    fn oversized_then_valid_releases_timer_and_evaluates_new_source() {
        let now = Instant::now();
        let mut session = RegexSession::default();
        session.draft.pattern = "a".into();
        session.mark_changed(now);
        let mut pumping = true;
        session.draft.test_text = "x".repeat(64 * 1024 + 1);
        session.mark_changed(now);
        assert!(matches!(
            session.result(),
            Some(EvaluationResult::Suspended(_))
        ));
        assert!(pending_timer(&session, &mut pumping, now).is_none());
        assert!(!pumping);
        session.draft.test_text = "a".into();
        session.mark_changed(now);
        assert!(session.pending_delay(now).is_some());
        assert!(session.tick(now + Duration::from_millis(200)));
        assert!(matches!(
            session.result(),
            Some(EvaluationResult::Success { .. })
        ));
    }
    #[test]
    fn exact_route_is_typed_regex_command() {
        let action = RegexTesterPlugin.search("regex").pop().unwrap();
        let route = super::super::parse_installed_search_invocation(
            action,
            multi_launcher::commands::ActivationSource::Enter,
        )
        .unwrap();
        assert_eq!(route.route, super::super::InstalledSearchRoute::RegexTester);
    }
    #[test]
    fn preview_distinguishes_empty_and_clips_at_unicode_boundaries() {
        assert_eq!(preview(""), "(empty)");
        assert_eq!(preview(&"é".repeat(161)), format!("{}…", "é".repeat(160)));
    }
}
