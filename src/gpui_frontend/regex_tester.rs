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
fn standalone_close_key(
    popout: bool,
    child_free: bool,
    focused: bool,
    key: &str,
    modifiers: Modifiers,
) -> bool {
    popout
        && child_free
        && focused
        && ((key == "escape" && modifiers == Modifiers::default())
            || (key == "w"
                && modifiers
                    == (Modifiers {
                        control: true,
                        ..Modifiers::default()
                    })))
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
pub(super) fn advance_due(session: &mut RegexSession, now: Instant) -> (bool, bool, bool) {
    let previous = session.evaluated_revision();
    let replacement = session.tick(now);
    let main_completed =
        session.evaluated_revision().is_some() && session.evaluated_revision() != previous;
    let cases = session.tick_test_cases(now);
    (main_completed, replacement, cases)
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
pub(super) struct RegexMount {
    pub(super) receipt: utility_mount::MountReceipt,
    pub(super) window: AnyWindowHandle,
    pub(super) native: Option<NativeWindow>,
    pub(super) pattern: Entity<InputState>,
    pub(super) text: Entity<TextareaState>,
    pub(super) replacement: Entity<TextareaState>,
    pub(super) preset_name: Entity<InputState>,
    pub(super) preset_confirmation_focus: FocusHandle,
    pub(super) case_owner: utility_mount::MountOwner,
    pub(super) case_receipt: utility_mount::MountReceipt,
    pub(super) cases:
        HashMap<multi_launcher::regex_tester::RegexTestCaseId, super::regex_cases::CaseEditor>,
    focus: FocusHandle,
    scroll: gpui_kit::UniformListScrollHandle,
    _subscriptions: Vec<Subscription>,
}
pub(super) struct RegexWorkspace {
    pub open: bool,
    pub(super) popout: bool,
    pub(super) rehosting: bool,
    pub(super) presentation_error: Option<String>,
    pub(super) geometry_error: Option<String>,
    pub(super) root: WeakEntity<RootWindow>,
    pub(super) admission: Arc<RestartAdmission>,
    pub(super) visibility: RootVisibility,
    pub(super) session: RegexSession,
    pub(super) history: super::regex_history::HistoryLane,
    pub(super) presets: super::regex_presets::PresetLane,
    pub(super) clipboard_pending: bool,
    pub(super) clipboard_epochs: super::regex_clipboard::Epochs,
    pub(super) clipboard_feedback: Option<String>,
    pub(super) selected_capture: usize,
    mount_owner: utility_mount::MountOwner,
    pub(super) mount: RegexMount,
    pub(super) cases_expanded: bool,
    pub(super) cases_error: Option<String>,
    return_focus: Option<FocusHandle>,
    generation: u64,
    pub(super) activation_revision: Arc<AtomicU64>,
    pumping: bool,
}
impl std::ops::Deref for RegexWorkspace {
    type Target = RegexMount;
    fn deref(&self) -> &Self::Target {
        &self.mount
    }
}
impl std::ops::DerefMut for RegexWorkspace {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mount
    }
}
impl RegexMount {
    fn new(
        window: &mut Window,
        cx: &mut Context<RegexWorkspace>,
        owner: &utility_mount::MountOwner,
        host: AnyWindowHandle,
        native: Option<NativeWindow>,
    ) -> Self {
        let receipt = owner.attach();
        let case_owner = utility_mount::MountOwner::default();
        let case_receipt = case_owner.attach();
        let pattern =
            cx.new(|cx| InputState::new(window, cx).placeholder("Enter a Rust regex pattern"));
        let text = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(5, 12)
                .placeholder("Enter test text")
        });
        let replacement = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 5)
                .placeholder("Replacement")
        });
        let preset_name = cx.new(|cx| InputState::new(window, cx).placeholder("Preset name"));
        let preset_receipt = receipt.clone();
        let preset_sub = cx.subscribe(&preset_name, move |view: &mut RegexWorkspace, input: Entity<InputState>, event: &InputEvent, cx| {
            if preset_receipt.is_current() && view.open && !view.rehosting && !view.admission.is_fenced()
                && view.presets.can_edit_name() && matches!(event, InputEvent::Change) {
                view.presets.name = input.read(cx).value().to_string();
                cx.notify();
            }
        });
        let replacement_receipt = receipt.clone();
        let replacement_sub = cx.subscribe_in(
            &replacement,
            window,
            move |view: &mut RegexWorkspace,
                  input: &Entity<TextareaState>,
                  event: &InputEvent,
                  window: &mut Window,
                  cx: &mut Context<RegexWorkspace>| {
                if replacement_receipt.is_current()
                    && view.open
                    && !view.rehosting
                    && !view.admission.is_fenced()
                    && matches!(event, InputEvent::Change)
                {
                    let next = input.read(cx).value().to_string();
                    if next != view.session.draft.replacement {
                        if next.len() > view.session.policy().replacement_bytes
                            && input.focus_handle(cx).is_focused(window)
                        {
                            view.focus.focus(window, cx);
                        }
                        view.session.draft.replacement = next;
                        view.clipboard_epochs.replacement.fetch_add(1, Ordering::AcqRel);
                        view.session.mark_replacement_changed(Instant::now());
                        view.pump(cx);
                        cx.notify();
                    }
                }
            },
        );
        let pattern_receipt = receipt.clone();
        let pattern_sub = cx.subscribe(
            &pattern,
            move |view: &mut RegexWorkspace, input: Entity<InputState>, event: &InputEvent, cx| {
                if pattern_receipt.is_current()
                    && view.open
                    && !view.rehosting
                    && !view.admission.is_fenced()
                    && matches!(event, InputEvent::Change)
                {
                    let next = input.read(cx).value().to_string();
                    if next != view.session.draft.pattern {
                        view.clipboard_epochs.pattern.fetch_add(1, Ordering::AcqRel);
                        view.session.draft.pattern = next;
                        view.changed(true, cx);
                    }
                }
            },
        );
        let text_receipt = receipt.clone();
        let text_sub = cx.subscribe_in(
            &text,
            window,
            move |view: &mut RegexWorkspace,
                  input: &Entity<TextareaState>,
                  event: &InputEvent,
                  window: &mut Window,
                  cx: &mut Context<RegexWorkspace>| {
                if text_receipt.is_current()
                    && view.open
                    && !view.rehosting
                    && !view.admission.is_fenced()
                    && matches!(event, InputEvent::Change)
                {
                    let next = input.read(cx).value().to_string();
                    if next != view.session.draft.test_text {
                        if next.len() > view.session.policy().text_bytes
                            && input.focus_handle(cx).is_focused(window) {
                            view.focus_anchor(window, cx);
                        }
                        view.clipboard_epochs.source.fetch_add(1, Ordering::AcqRel);
                        view.session.draft.test_text = next;
                        view.changed(false, cx);
                    }
                }
            },
        );
        Self {
            receipt,
            window: host,
            native,
            pattern,
            text,
            replacement,
            preset_name,
            preset_confirmation_focus: cx.focus_handle(),
            case_owner,
            case_receipt,
            cases: HashMap::new(),
            focus: cx.focus_handle(),
            scroll: Default::default(),
            _subscriptions: vec![pattern_sub, text_sub, replacement_sub, preset_sub],
        }
    }
}
impl RegexWorkspace {
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        root: WeakEntity<RootWindow>,
        admission: Arc<RestartAdmission>,
        visibility: RootVisibility,
        history_path: Option<std::path::PathBuf>,
        host: AnyWindowHandle,
        native: Option<NativeWindow>,
    ) -> Self {
        let mount_owner = utility_mount::MountOwner::default();
        let mount = RegexMount::new(window, cx, &mount_owner, host, native);
        Self {
            open: false,
            popout: false,
            rehosting: false,
            presentation_error: None,
            geometry_error: None,
            root,
            admission,
            visibility,
            session: RegexSession::default(),
            presets: super::regex_presets::PresetLane::new(history_path.as_ref().map(|path| path.with_file_name("regex_presets.json"))),
            history: super::regex_history::HistoryLane::new(history_path),
            clipboard_pending: false,
            clipboard_epochs: super::regex_clipboard::Epochs::default(),
            clipboard_feedback: None,
            selected_capture: 0,
            mount_owner,
            mount,
            cases_expanded: true,
            cases_error: None,
            return_focus: None,
            generation: 0,
            activation_revision: Arc::new(AtomicU64::new(0)),
            pumping: false,
        }
    }
    pub(super) fn replace_mount(
        &mut self,
        host: AnyWindowHandle,
        native: Option<NativeWindow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mount = RegexMount::new(window, cx, &self.mount_owner, host, native);
        let pattern = self.session.draft.pattern.clone();
        let text = self.session.draft.test_text.clone();
        let replacement = self.session.draft.replacement.clone();
        mount
            .pattern
            .update(cx, |input, cx| input.set_value(pattern, window, cx));
        mount
            .text
            .update(cx, |input, cx| input.set_value(text, window, cx));
        mount
            .replacement
            .update(cx, |input, cx| input.set_value(replacement, window, cx));
        mount.preset_name.update(cx, |input, cx| input.set_value(self.presets.name.clone(), window, cx));
        self.mount = mount;
        super::regex_cases::sync_editors(self, window, cx);
    }
    pub(super) fn mount_receipt(&self) -> utility_mount::MountReceipt {
        self.mount.receipt.clone()
    }
    pub(super) fn is_mounted_in(&self, id: WindowId, popout: bool) -> bool {
        self.open
            && self.popout == popout
            && self.mount.receipt.is_current()
            && self.mount.window.window_id() == id
    }
    pub(super) fn focus_anchor(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
    }
    pub(super) fn contains_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus.is_focused(window) || self.focus.contains_focused(window, cx)
    }
    pub(super) fn focus_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.rehosting {
            self.focus.focus(window, cx);
        } else if self.presets.has_confirmation() {
            self.mount.preset_confirmation_focus.focus(window, cx);
        } else {
            self.pattern.focus_handle(cx).focus(window, cx);
        }
    }
    pub(super) fn finish_popout_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.focus.is_focused(window) {
            self.focus_editor(window, cx);
        }
    }
    pub(super) fn finish_dock_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.focus.is_focused(window)
            && self
                .mount
                .native
                .is_some_and(|native| current_foreground_identity() == Some(native.identity))
        {
            self.focus_editor(window, cx);
        }
    }
    fn request_rehost(&mut self, popout: bool, cx: &mut Context<Self>) {
        if !self.open || self.rehosting || self.clipboard_pending || self.presets.blocks_host() || self.admission.is_fenced() || self.popout == popout {
            return;
        }
        let source = self.mount.receipt.clone();
        let gesture = self.mount.native.map(|source| hash_dock::Gesture {
            source,
            foreground_authorized: native_window_is_valid(source)
                && current_foreground_identity() == Some(source.identity),
            visibility_revision: self.visibility.request_revision.load(Ordering::Acquire),
        });
        self.rehosting = true;
        self.activation_revision.fetch_add(1, Ordering::AcqRel);
        let root = self.root.clone();
        cx.spawn(async move |_, cx| {
            let _ = root.update(cx, |root, cx| {
                root.start_regex_rehost(popout, gesture, source, cx)
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn changed(&mut self, cases_changed: bool, cx: &mut Context<Self>) {
        self.clipboard_epochs.main_changed(cases_changed);
        self.finish_prepared_change(cx);
    }
    pub(super) fn finish_prepared_change(&mut self, cx: &mut Context<Self>) {
        self.session.mark_changed(Instant::now());
        self.pump(cx);
        cx.notify();
    }
    pub(super) fn pump(&mut self, cx: &mut Context<Self>) {
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
                        let (main, replacement, cases) = advance_due(&mut view.session, Instant::now());
                        if main {
                            view.clipboard_epochs.matches.fetch_add(1, Ordering::AcqRel);
                            view.clipboard_epochs.selection.fetch_add(1, Ordering::AcqRel);
                            view.clipboard_epochs.capture.fetch_add(1, Ordering::AcqRel);
                            view.record_accepted_history(cx);
                        }
                        if replacement { view.clipboard_epochs.replacement.fetch_add(1, Ordering::AcqRel); }
                        if cases { view.clipboard_epochs.cases.fetch_add(1, Ordering::AcqRel); }
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
            if !self.mount.receipt.is_current() {
                self.replace_mount(self.mount.window, self.mount.native, window, cx);
            }
            self.generation = self.generation.wrapping_add(1);
            self.open = true;
            self.return_focus = Some(origin);
            self.session.ensure_initial_evaluation(Instant::now());
            self.pump(cx);
        }
        self.focus_editor(window, cx);
        cx.notify();
    }
    pub(super) fn shutdown(&mut self) {
        self.presets.cancel_confirmation();
        self.open = false;
        self.activation_revision.fetch_add(1, Ordering::AcqRel);
        self.mount_owner.retire();
        self.generation = self.generation.wrapping_add(1);
        self.pumping = false;
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.presets.cancel_confirmation() { self.focus_anchor(window, cx); cx.notify(); return; }
        if !self.open || self.rehosting || self.clipboard_pending || self.presets.busy() {
            return;
        }
        let popout = self.popout;
        if popout {
            let _ = self
                .root
                .update(cx, |root, cx| root.drain_regex_native_geometry(cx, true));
        }
        let restore = window.is_window_active() && self.focus.contains_focused(window, cx);
        self.shutdown();
        let _ = self.root.update(cx, |root, cx| {
            panel_stack::retire(&mut root.panel_stack, Panel::RegexTesterDialog);
            if popout {
                if let Some(popup) = root.regex_popup.take() {
                    let retiring = RetiringHashWindow {
                        window: popup.window,
                        native_window: popup.native,
                    };
                    root.regex_retiring.push(retiring);
                    let weak = cx.entity().downgrade();
                    cx.spawn(async move |_, cx| {
                        if remove_and_wait_for_hash_window(cx, retiring).await {
                            let _ = weak.update(cx, |root, _| {
                                root.regex_retiring.retain(|item| {
                                    item.window.window_id() != retiring.window.window_id()
                                })
                            });
                        }
                    })
                    .detach();
                }
            }
        });
        if restore && !popout {
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
        if self.rehosting || self.admission.is_fenced() {
            return;
        }
        if if backwards {
            self.session.previous_match()
        } else {
            self.session.next_match()
        } {
            self.clipboard_epochs.selection.fetch_add(1, Ordering::AcqRel);
            self.clipboard_epochs.capture.fetch_add(1, Ordering::AcqRel);
            self.selected_capture = 0;
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
        let render_receipt = self.mount.receipt.clone();
        let escape_receipt = render_receipt.clone();
        let key_receipt = render_receipt.clone();
        let previous_receipt = render_receipt.clone();
        let next_receipt = render_receipt.clone();
        let close_receipt = render_receipt.clone();
        let rehost_receipt = render_receipt.clone();
        let clear_source_receipt = render_receipt.clone();
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
            let receipt = render_receipt.clone();
            flag_row = flag_row.child(
                Checkbox::new(SharedString::from(format!("regex-flag-{index}")))
                    .checked(selected)
                    .disabled(self.rehosting || self.admission.is_fenced())
                    .accessibility_label(label)
                    .on_change(move |state, _, _, app| {
                        let _ = owner.update(app, |view, cx| {
                            if !receipt.is_current()
                                || !view.open
                                || view.rehosting
                                || view.admission.is_fenced()
                            {
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
                            view.changed(true, cx);
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
                            .child(
                                Label::new(label)
                                    .id(SharedString::from(format!("regex-flag-label-{index}"))),
                            ),
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
        let rows_receipt = render_receipt.clone();
        let rows = gpui_kit::uniform_list("regex-match-rows", count, move |range, _, app| {
            let Some(view) = owner.upgrade() else {
                return Vec::<AnyElement>::new();
            };
            let state = view.read(app);
            if !rows_receipt.is_current() || !state.open || state.session.revision() != revision {
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
                    let receipt = rows_receipt.clone();
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
                                        if receipt.is_current()
                                            && view.open
                                            && !view.admission.is_fenced()
                                            && view.session.revision() == revision
                                            && index < view.session.matches().len()
                                        {
                                            view.session.select_match(index);
                                            view.clipboard_epochs.selection.fetch_add(1, Ordering::AcqRel);
                                            view.clipboard_epochs.capture.fetch_add(1, Ordering::AcqRel);
                                            view.selected_capture = 0;
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
            .capture_action(cx.listener(
                move |view, _: &gpui_kit::base::input::Escape, window, cx| {
                    let contexts = window.context_stack();
                    if escape_receipt.is_current()
                        && view.open
                        && view.focus.contains_focused(window, cx)
                        && !window.has_active_dialog(cx)
                        && !context_stack_has_primary(&contexts, "PopupMenu")
                        && !context_stack_has_primary(&contexts, "Dialog")
                        && !context_stack_has_primary(&contexts, "AlertDialog")
                    {
                        view.close(window, cx);
                        cx.stop_propagation();
                    }
                },
            ))
            .capture_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                let contexts = window.context_stack();
                let child_free = !window.has_active_dialog(cx)
                    && !context_stack_has_primary(&contexts, "PopupMenu")
                    && !context_stack_has_primary(&contexts, "Dialog")
                    && !context_stack_has_primary(&contexts, "AlertDialog");
                if key_receipt.is_current() && view.open && !view.rehosting && child_free
                    && view.contains_focus(window, cx) && event.keystroke.key == "escape"
                    && event.keystroke.modifiers == Modifiers::default() && view.presets.has_confirmation() {
                    view.focus_anchor(window, cx);
                    view.presets.cancel_confirmation();
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                if key_receipt.is_current() && event.is_held && view.open && child_free
                    && standalone_close_key(view.popout, child_free, view.contains_focus(window, cx),
                        &event.keystroke.key, event.keystroke.modifiers) {
                    cx.stop_propagation();
                    return;
                }
                let input_owns_escape = event.keystroke.key == "escape"
                    && (view.pattern.focus_handle(cx).is_focused(window)
                        || view.text.focus_handle(cx).is_focused(window)
                        || view.mount.replacement.focus_handle(cx).is_focused(window)
                        || view.mount.preset_name.focus_handle(cx).is_focused(window)
                        || super::regex_cases::editor_focused(view, window, cx));
                if key_receipt.is_current()
                    && !input_owns_escape
                    && view.open
                    && view.popout
                    && !view.rehosting
                    && !view.admission.is_fenced()
                    && standalone_close_key(
                        view.popout,
                        child_free,
                        view.contains_focus(window, cx),
                        &event.keystroke.key,
                        event.keystroke.modifiers,
                    )
                {
                    view.close(window, cx);
                    cx.stop_propagation();
                    return;
                }
                if key_receipt.is_current()
                    && view.open
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
                    .disabled(self.rehosting || self.admission.is_fenced())
                    .aria_label("Pattern"),
            )
            .child(flag_row)
            .when(
                self.session.draft.test_text.len() <= self.session.policy().text_bytes,
                |element| element.child(
                    Textarea::new(&self.text)
                        .readonly(self.rehosting || self.admission.is_fenced())
                        .aria_label("Test Text"),
                ),
            )
            .when(
                self.session.draft.test_text.len() > self.session.policy().text_bytes,
                |element| element.child(
                    div().flex().flex_col().gap_1()
                        .child(Label::new(format!("Test Text retained ({} bytes), above the evaluation limit.", self.session.draft.test_text.len())))
                        .child(Label::new(preview(&self.session.draft.test_text)).id("regex-test-text-oversize-preview"))
                        .child(Button::new("regex-test-text-clear").label("Clear Test Text")
                            .disabled(self.rehosting || self.admission.is_fenced())
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if clear_source_receipt.is_current() && view.open && !view.rehosting && !view.admission.is_fenced() {
                                    view.session.draft.test_text.clear();
                                    view.focus_anchor(window, cx);
                                    view.clipboard_epochs.source.fetch_add(1, Ordering::AcqRel);
                                    view.mount.text.update(cx, |input, cx| input.set_value("", window, cx));
                                    view.changed(false, cx);
                                }
                            })))),
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
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if previous_receipt.is_current() {
                                    view.navigate(true, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("regex-next")
                            .label("Next")
                            .disabled(count == 0)
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if next_receipt.is_current() {
                                    view.navigate(false, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("regex-rehost")
                            .label(if self.popout { "Dock" } else { "Pop Out" })
                            .disabled(self.rehosting || self.clipboard_pending || self.presets.blocks_host())
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if rehost_receipt.is_current() {
                                    view.request_rehost(!view.popout, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("regex-close")
                            .label("Close")
                            .disabled(self.clipboard_pending || self.presets.busy())
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if close_receipt.is_current() {
                                    view.close(window, cx);
                                }
                            })),
                    ),
            )
            .children(self.history.diagnostic.as_ref().map(|message| {
                div().id("regex-history-warning").role(Role::Alert).child(
                    Label::new(format!("Regex history: {message}"))
                        .id("regex-history-warning-text"),
                )
            }))
            .children(
                self.presentation_error
                    .as_ref()
                    .map(|message| Label::new(message.clone())),
            )
            .children(
                self.geometry_error
                    .as_ref()
                    .map(|message| Label::new(message.clone())),
            )
            .child(rows)
            .children(details.map(|(index, rows)| {
                let match_id = self.session.matches()[index].id;
                let receipt = render_receipt.clone();
                let owner = cx.entity().downgrade();
                let selected = self.selected_capture;
                div()
                    .id("regex-capture-details")
                    .max_h(px(220.0))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .children(rows.into_iter().map(move |(group, text)| {
                        if group == 0 {
                            return Label::new(text).id(SharedString::from(format!(
                                "regex-match-detail-{revision}-{index}"
                            ))).into_any_element();
                        }
                        let owner = owner.clone();
                        let receipt = receipt.clone();
                        Button::new(SharedString::from(format!("regex-capture-{revision}-{index}-{group}")))
                            .label(text)
                            .selected(selected == group - 1)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |view, cx| {
                                    if receipt.is_current() && view.open && !view.rehosting
                                        && !view.admission.is_fenced() && view.session.revision() == revision
                                        && view.session.selected_index().and_then(|selected| view.session.matches().get(selected))
                                            .is_some_and(|item| item.id == match_id && item.captures.get(group - 1).is_some_and(|capture| capture.group_index == group)) {
                                        view.selected_capture = group - 1;
                                        view.clipboard_epochs.capture.fetch_add(1, Ordering::AcqRel);
                                        cx.notify();
                                    }
                                });
                            }).into_any_element()
                    }))
            }))
            .child(super::regex_substitution::render(self, cx))
            .child(super::regex_cases::render(self, cx))
            .child(super::regex_clipboard::render(self, cx))
            .child(super::regex_history::render(self, cx))
            .child(super::regex_presets::render(self, cx))
    }
}
impl RootWindow {
    pub(super) fn scroll_to_regex(&self, cx: &App) {
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
    }
    pub(super) fn open_regex_tester(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced()
            || self
                .regex_view
                .as_ref()
                .is_some_and(|view| view.read(cx).rehosting)
        {
            return;
        }
        if let (Some(popup), Some(view), Some(root_native)) =
            (&self.regex_popup, &self.regex_view, self.native_window)
        {
            let Some(native) = popup.native else {
                return;
            };
            if current_foreground_identity() != Some(root_native.identity) {
                return;
            }
            let Some(permit) = self.restart_admission.admit() else {
                return;
            };
            let popup_window = popup.window;
            let view = view.clone();
            let receipt = view.read(cx).mount_receipt();
            let guard = view.read(cx).activation_revision.clone();
            let revision = guard.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
            let worker = guard.clone();
            cx.spawn(async move |_, cx| {
                let _permit = permit;
                cx.background_executor()
                    .spawn(async move {
                        activate_for_secondary_window(
                            &worker,
                            revision,
                            native,
                            Some(root_native.identity),
                        );
                    })
                    .await;
                if !receipt.is_current()
                    || guard.load(Ordering::Acquire) != revision
                    || current_foreground_identity() != Some(native.identity)
                {
                    return;
                }
                let _ = update_kit_window_content(popup_window, &view, cx, |view, window, cx| {
                    if receipt.is_current() && view.popout && !view.rehosting {
                        view.focus_editor(window, cx);
                    }
                });
            })
            .detach();
            return;
        }
        let Some(root_window) = self.root_window else {
            return;
        };
        let root = cx.entity().downgrade();
        let admission = self.restart_admission.clone();
        let visibility = self.root_visibility.clone();
        #[cfg(windows)]
        let history_path = Some(self.app_data_root.path().join("regex_history.json"));
        #[cfg(not(windows))]
        let history_path = None;
        let native = self.native_window;
        let view = self.regex_view.clone().unwrap_or_else(|| {
            cx.new(|cx| {
                RegexWorkspace::new(
                    window,
                    cx,
                    root,
                    admission,
                    visibility,
                    history_path,
                    root_window,
                    native,
                )
            })
        });
        let origin = self.query.focus_handle(cx);
        let was_open = view.read(cx).open;
        view.update(cx, |view, cx| {
            if !view.open {
                view.popout = false;
                view.replace_mount(root_window, native, window, cx);
            }
            view.reopen(window, cx, origin);
        });
        self.regex_view = Some(view.clone());
        let popout = multi_launcher::settings::UtilityToolKey::parse("regex-tester")
            .ok()
            .map(|key| {
                self.utility_presentations
                    .preference(key)
                    .unwrap_or(RememberedPresentation::Popout)
                    == RememberedPresentation::Popout
            })
            .unwrap_or(true);
        if !was_open && popout {
            view.update(cx, |view, cx| view.request_rehost(true, cx));
        }
        self.scroll_to_regex(cx);
        panel_stack::promote(&mut self.panel_stack, Panel::RegexTesterDialog);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Modifiers, advance_due, pending_timer, preview, provider_is_current, standalone_close_key,
        timer_current,
    };
    use multi_launcher::plugin::Plugin;
    use multi_launcher::plugins::regex_tester::RegexTesterPlugin;
    use multi_launcher::regex_tester::{CaptureValue, EvaluationResult, RegexSession};
    use std::time::{Duration, Instant};
    #[test]
    fn standalone_close_keys_respect_child_and_exact_modifiers() {
        let ctrl = Modifiers {
            control: true,
            ..Modifiers::default()
        };
        assert!(standalone_close_key(
            true,
            true,
            true,
            "escape",
            Modifiers::default()
        ));
        assert!(!standalone_close_key(true, true, true, "escape", ctrl));
        assert!(!standalone_close_key(
            true,
            false,
            true,
            "escape",
            Modifiers::default()
        ));
        assert!(standalone_close_key(true, true, true, "w", ctrl));
        assert!(!standalone_close_key(false, true, true, "w", ctrl));
        assert!(!standalone_close_key(true, false, true, "w", ctrl));
        assert!(!standalone_close_key(true, true, false, "w", ctrl));
        assert!(!standalone_close_key(
            true,
            true,
            true,
            "w",
            Modifiers {
                shift: true,
                ..ctrl
            }
        ));
    }
    #[test]
    fn retained_regex_session_survives_mount_replacement() {
        let owner = super::utility_mount::MountOwner::default();
        let first = owner.attach();
        let mut session = RegexSession::default();
        session.draft.pattern = "(é)".into();
        session.draft.test_text = "é".into();
        let now = Instant::now();
        session.mark_changed(now);
        let revision = session.revision();
        let replacement = owner.attach();
        assert!(!first.is_current());
        assert!(replacement.is_current());
        assert_eq!(session.revision(), revision);
        assert!(session.tick(now + Duration::from_millis(200)));
        assert_eq!(session.matches()[0].text, "é");
        assert_eq!(session.draft.pattern, "(é)");
    }
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
    fn grouped_due_work_keeps_primary_match_and_history_revision_stable() {
        use multi_launcher::regex_tester::{
            RegexTestCaseExpectation, RegexTestCaseStatus, SubstitutionEvaluationResult,
        };
        let now = Instant::now();
        let mut session = RegexSession::default();
        session.draft.pattern = "a".into();
        session.draft.test_text = "aa".into();
        session.mark_changed(now);
        assert!(advance_due(&mut session, now + Duration::from_millis(200)).0);
        session.select_match(1);
        let revision = session.revision();
        session.set_substitution_enabled(true, now);
        session.draft.replacement = "b".into();
        session.mark_replacement_changed(now);
        assert!(!advance_due(&mut session, now + Duration::from_millis(200)).0);
        assert!(
            matches!(session.substitution_result(), Some(SubstitutionEvaluationResult::Success(result)) if result.output == "bb")
        );
        assert_eq!(session.revision(), revision);
        assert_eq!(session.selected_index(), Some(1));
        let id = session
            .draft
            .test_cases
            .add(None, "a".into(), RegexTestCaseExpectation::Match)
            .unwrap();
        session.mark_test_cases_changed(now);
        assert!(!advance_due(&mut session, now + Duration::from_millis(200)).0);
        assert!(
            session
                .test_case_evaluation()
                .unwrap()
                .results
                .iter()
                .any(|result| result.case_id == id && result.status == RegexTestCaseStatus::Pass)
        );
        assert_eq!(session.revision(), revision);
        assert_eq!(session.selected_index(), Some(1));
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
