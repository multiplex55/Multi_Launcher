//! Retained JSON Text workspace and checked clipboard receipts.
use super::*;
use multi_launcher::commands::{JsonUtilityCommand, JsonUtilityCommandHost, JsonUtilityIntent};
use multi_launcher::json_utility_session::{JsonEditorState, Transform};

pub(super) fn provider_is_current(
    query: &str,
    action: &Action,
    enabled: Option<&HashSet<String>>,
    caps: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    let plugin = multi_launcher::plugins::json_utility::JsonUtilityPlugin;
    search_providers::search_routed(
        [&plugin as &dyn multi_launcher::plugin::Plugin],
        query,
        enabled,
        caps,
    )
    .iter()
    .any(|(name, row)| name == "json_utility" && row == action)
}

#[derive(Default)]
struct OpenHost(Option<JsonUtilityIntent>);
impl JsonUtilityCommandHost for OpenHost {
    fn open_json_utility(&mut self, intent: JsonUtilityIntent) {
        self.0 = Some(intent);
    }
}

fn receipt_current(open: bool, epoch: u64, expected: u64, active: bool) -> bool {
    open && epoch == expected && active
}

fn apply_paste_result(
    session: &mut JsonEditorState,
    initial_projection: &mut Option<(u64, u64)>,
    checking: &mut Option<u64>,
    initial_current: bool,
    result: Result<String, ClipboardError>,
) {
    if initial_projection.is_some() && !initial_current {
        session.begin_initialize();
        *initial_projection = None;
        *checking = None;
    }
    if result.is_ok() {
        *initial_projection = None;
        *checking = None;
    }
    session.finish_reload(result);
}

fn native_root_current(
    visibility: &RootVisibility,
    revision: u64,
    native: Option<NativeWindow>,
) -> bool {
    #[cfg(windows)]
    {
        native.is_some_and(|window| visibility.is_current_visible_revision(revision, window))
    }
    #[cfg(not(windows))]
    {
        let _ = native;
        visibility.is_current(revision, true)
    }
}

#[derive(Default)]
struct ClipboardLane {
    active: bool,
    latest_initial: Option<(u64, u64)>,
}
impl ClipboardLane {
    fn queue_initial(&mut self, epoch: u64, revision: u64) {
        self.latest_initial = Some((epoch, revision));
    }
    fn invalidate(&mut self) {
        self.latest_initial = None;
    }
    fn take_ready(&mut self) -> Option<(u64, u64)> {
        if self.active {
            None
        } else {
            self.latest_initial.take()
        }
    }
}

struct GuardedClipboard {
    epoch: Arc<AtomicU64>,
    expected: u64,
    visibility: RootVisibility,
    visibility_revision: u64,
    native: Option<NativeWindow>,
    admission: Arc<RestartAdmission>,
}
impl ClipboardBackend for GuardedClipboard {
    fn read_text(&self) -> Result<String, ClipboardError> {
        ArboardClipboardBackend.read_text()
    }
    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        if self.epoch.load(Ordering::Acquire) != self.expected
            || self.admission.is_fenced()
            || !native_root_current(&self.visibility, self.visibility_revision, self.native)
        {
            return Err(ClipboardError::Permanent(
                "JSON editor changed before copy".into(),
            ));
        }
        ArboardClipboardBackend.write_text(text)
    }
}

pub(super) struct JsonView {
    pub(super) open: bool,
    root: WeakEntity<RootWindow>,
    root_window: AnyWindowHandle,
    admission: Arc<RestartAdmission>,
    visibility: RootVisibility,
    native: Option<NativeWindow>,
    input: Entity<TextareaState>,
    focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    return_scroll: Option<Point<Pixels>>,
    session: JsonEditorState,
    intent: JsonUtilityIntent,
    epoch: Arc<AtomicU64>,
    wake: Arc<HashWakeSignal>,
    lane: ClipboardLane,
    initial_projection: Option<(u64, u64)>,
    checking: Option<u64>,
    escape_waiting: bool,
    _subscriptions: Vec<Subscription>,
}
impl JsonView {
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        root: WeakEntity<RootWindow>,
        root_window: AnyWindowHandle,
        admission: Arc<RestartAdmission>,
        visibility: RootVisibility,
        native: Option<NativeWindow>,
        intent: JsonUtilityIntent,
        return_focus: Option<FocusHandle>,
        return_scroll: Point<Pixels>,
    ) -> Self {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(5, 12)
                .placeholder("Enter JSON")
        });
        let focus = cx.focus_handle();
        let subscription = cx.subscribe(
            &input,
            |this: &mut Self,
             input: Entity<TextareaState>,
             event: &InputEvent,
             cx: &mut Context<Self>| {
                if !matches!(event, InputEvent::Change) || !this.open || this.admission.is_fenced()
                {
                    return;
                }
                let value = input.read(cx).value().to_string();
                if value != this.session.source() {
                    this.epoch.fetch_add(1, Ordering::AcqRel);
                    this.lane.invalidate();
                    this.initial_projection = None;
                    this.checking = None;
                    this.session.set_source(value);
                    this.service(cx);
                    cx.notify();
                }
            },
        );
        let focus_subscription =
            cx.on_focus_out(&focus, window, |this, _, _, _| this.escape_waiting = false);
        let mut session = JsonEditorState::default();
        session.begin_initialize();
        Self {
            open: false,
            root,
            root_window,
            admission,
            visibility,
            native,
            input,
            focus,
            return_focus,
            return_scroll: Some(return_scroll),
            session,
            intent,
            epoch: Arc::new(AtomicU64::new(1)),
            wake: Arc::new(HashWakeSignal::default()),
            lane: ClipboardLane::default(),
            initial_projection: None,
            checking: None,
            escape_waiting: false,
            _subscriptions: vec![subscription, focus_subscription],
        }
    }
    fn service(&mut self, cx: &mut Context<Self>) {
        if self.initial_projection.is_some_and(|(epoch, revision)| {
            !receipt_current(
                self.open,
                self.epoch.load(Ordering::Acquire),
                epoch,
                !self.admission.is_fenced()
                    && native_root_current(&self.visibility, revision, self.native),
            )
        }) {
            self.initial_projection = None;
            self.checking = None;
            self.session.begin_initialize();
        }
        let wake = self.wake.clone();
        self.session.service_projection(|| {
            let wake = wake.clone();
            Arc::new(move || wake.notify())
        });
        if !self.session.loading_clipboard() && self.initial_projection.take().is_some() {
            self.checking = None;
        }
        cx.notify();
    }
    fn reopen(
        &mut self,
        intent: JsonUtilityIntent,
        origin: Option<FocusHandle>,
        offset: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
        let first_open = !self.open;
        if first_open {
            self.wake.close();
            self.session.close();
            self.session = JsonEditorState::default();
            self.wake = Arc::new(HashWakeSignal::default());
        }
        self.session.begin_initialize();
        self.open = true;
        self.intent = intent;
        self.return_focus = origin;
        self.return_scroll = Some(offset);
        self.initial_projection = None;
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        if first_open {
            self.start_wake(cx);
        }
        let epoch = self.epoch.load(Ordering::Acquire);
        let revision = self.visibility.request_revision.load(Ordering::Acquire);
        self.checking = Some(epoch);
        self.lane.queue_initial(epoch, revision);
        self.drain_initial(cx);
        self.input.focus_handle(cx).focus(window, cx);
        cx.notify();
    }
    fn drain_initial(&mut self, cx: &mut Context<Self>) {
        let Some((epoch, revision)) = self.lane.take_ready() else {
            return;
        };
        if receipt_current(
            self.open,
            self.epoch.load(Ordering::Acquire),
            epoch,
            !self.admission.is_fenced()
                && native_root_current(&self.visibility, revision, self.native),
        ) {
            self.read_clipboard(true, cx);
        } else {
            self.checking = None;
        }
    }
    fn sync_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.session.source().to_owned();
        if self.input.read(cx).value().as_str() != text {
            self.input
                .update(cx, |input, cx| input.set_value(text, window, cx));
        }
    }
    fn start_wake(&self, cx: &mut Context<Self>) {
        let wake = self.wake.clone();
        let owner = cx.entity().downgrade();
        let root_window = self.root_window;
        cx.spawn(async move |_, cx| {
            while wake.wait().await {
                let Some(view) = owner.upgrade() else {
                    break;
                };
                let Ok(current) =
                    update_kit_window_content(root_window, &view, cx, |view, window, cx| {
                        if !view.open {
                            return false;
                        }
                        view.service(cx);
                        view.sync_input(window, cx);
                        true
                    })
                else {
                    break;
                };
                if !current {
                    break;
                }
            }
        })
        .detach();
    }
    fn read_clipboard(&mut self, initial: bool, cx: &mut Context<Self>) {
        if !self.open || self.lane.active {
            return;
        }
        let Some(permit) = self.admission.admit() else {
            if initial {
                self.checking = None;
            }
            return;
        };
        self.lane.active = true;
        let expected = self.epoch.load(Ordering::Acquire);
        let visibility_revision = self.visibility.request_revision.load(Ordering::Acquire);
        let owner = cx.entity().downgrade();
        let root_window = self.root_window;
        let task = cx.background_executor().spawn(async move {
            let _permit = permit;
            ArboardClipboardBackend.read_text()
        });
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let Some(view) = owner.upgrade() else {
                return;
            };
            let _ = update_kit_window_content(root_window, &view, cx, |view, window, cx| {
                view.lane.active = false;
                if !receipt_current(
                    view.open,
                    view.epoch.load(Ordering::Acquire),
                    expected,
                    !view.admission.is_fenced()
                        && native_root_current(&view.visibility, visibility_revision, view.native),
                ) {
                    if view.checking == Some(expected) {
                        view.checking = None;
                    }
                    view.drain_initial(cx);
                    cx.notify();
                    return;
                }
                if initial {
                    view.session.finish_initialize(result);
                    view.initial_projection = Some((expected, visibility_revision));
                } else {
                    let initial_current =
                        view.initial_projection.is_none_or(|(epoch, revision)| {
                            receipt_current(
                                view.open,
                                view.epoch.load(Ordering::Acquire),
                                epoch,
                                !view.admission.is_fenced()
                                    && native_root_current(&view.visibility, revision, view.native),
                            )
                        });
                    apply_paste_result(
                        &mut view.session,
                        &mut view.initial_projection,
                        &mut view.checking,
                        initial_current,
                        result,
                    );
                }
                view.service(cx);
                view.sync_input(window, cx);
                view.drain_initial(cx);
            });
        })
        .detach();
        cx.notify();
    }
    fn copy(&mut self, cx: &mut Context<Self>) {
        if !self.open || self.lane.active || self.checking.is_some() {
            return;
        }
        let Some(permit) = self.admission.admit() else {
            return;
        };
        self.lane.active = true;
        let expected = self.epoch.load(Ordering::Acquire);
        let visibility_revision = self.visibility.request_revision.load(Ordering::Acquire);
        let epoch = self.epoch.clone();
        let visibility = self.visibility.clone();
        let native = self.native;
        let admission = self.admission.clone();
        let text = self.session.source().to_owned();
        let owner = cx.entity().downgrade();
        let root_window = self.root_window;
        let task = cx.background_executor().spawn(async move {
            let _permit = permit;
            let mut session = JsonEditorState::default();
            session.set_source(text);
            session.copy(&GuardedClipboard {
                epoch,
                expected,
                visibility,
                visibility_revision,
                native,
                admission,
            });
            session
        });
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let Some(view) = owner.upgrade() else {
                return;
            };
            let _ = update_kit_window_content(root_window, &view, cx, |view, _, cx| {
                view.lane.active = false;
                if receipt_current(
                    view.open,
                    view.epoch.load(Ordering::Acquire),
                    expected,
                    !view.admission.is_fenced()
                        && native_root_current(&view.visibility, visibility_revision, view.native),
                ) {
                    view.session.accept_copy_feedback(&result);
                }
                view.drain_initial(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn transform(&mut self, transform: Transform, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || self.checking.is_some() {
            return;
        }
        let Some(_permit) = self.admission.admit() else {
            return;
        };
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.lane.invalidate();
        self.initial_projection = None;
        self.checking = None;
        self.session.transform(transform);
        self.sync_input(window, cx);
        self.service(cx);
    }
    fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(_permit) = self.admission.admit() else {
            return;
        };
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.lane.invalidate();
        self.initial_projection = None;
        self.checking = None;
        self.session.clear();
        self.sync_input(window, cx);
        self.service(cx);
        self.input.focus_handle(cx).focus(window, cx);
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        let owned = window.is_window_active() && self.focus.contains_focused(window, cx);
        self.open = false;
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.lane.invalidate();
        self.initial_projection = None;
        self.checking = None;
        self.wake.close();
        self.session.close();
        self.session = JsonEditorState::default();
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        let origin = self.return_focus.take();
        let offset = self.return_scroll.take();
        let _ = self.root.update(cx, |root, cx| {
            panel_stack::retire(&mut root.panel_stack, Panel::JsonUtilityDialog);
            if owned && !root.restart_admission.is_fenced() {
                if let Some(focus) =
                    origin.filter(|focus| root.focus_handle.contains(focus, window))
                {
                    if let Some(offset) = offset {
                        root.root_scroll.set_offset(offset);
                    }
                    focus.focus(window, cx);
                } else {
                    root.query.focus_handle(cx).focus(window, cx);
                }
            }
            cx.notify();
        });
        cx.notify();
    }
    pub(super) fn draft(&self) -> Option<String> {
        self.open.then(|| self.session.source().to_owned())
    }
    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>, held: bool) -> bool {
        let contexts = window.context_stack();
        if !self.open
            || !self.focus.contains_focused(window, cx)
            || window.has_active_dialog(cx)
            || context_stack_has_primary(&contexts, "PopupMenu")
            || context_stack_has_primary(&contexts, "Dialog")
            || context_stack_has_primary(&contexts, "AlertDialog")
        {
            return false;
        }
        if !held && !self.escape_waiting {
            self.escape_waiting = true;
            self.close(window, cx);
        }
        true
    }
}
impl Drop for JsonView {
    fn drop(&mut self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.wake.close();
    }
}
impl Render for JsonView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let first = self.intent == JsonUtilityIntent::Minify;
        let checking = self.checking.is_some();
        let actions = if first {
            [("Minify", Transform::Minify), ("Format", Transform::Format)]
        } else {
            [("Format", Transform::Format), ("Minify", Transform::Minify)]
        };
        let buttons = actions
            .into_iter()
            .map(|(name, action)| {
                Button::new(SharedString::from(format!("json-{name}")))
                    .label(name)
                    .tab_stop(!checking)
                    .disabled(checking)
                    .on_click(
                        cx.listener(move |view, _, window, cx| view.transform(action, window, cx)),
                    )
            })
            .collect::<Vec<_>>();
        let error = self
            .session
            .parse_error()
            .map(ToString::to_string)
            .or_else(|| self.session.worker_error().map(str::to_owned));
        let feedback = self.session.feedback().map(|result| match result {
            Ok(message) | Err(message) => message.clone(),
        });
        div()
            .id("json-utility-pane")
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .track_focus(&self.focus)
            .capture_action(
                cx.listener(|view, _: &gpui_kit::base::input::Escape, window, cx| {
                    if view.escape(window, cx, false) {
                        cx.stop_propagation();
                    }
                }),
            )
            .capture_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape"
                    && event.keystroke.modifiers == Modifiers::default()
                    && view.escape(window, cx, event.is_held)
                {
                    cx.stop_propagation();
                }
            }))
            .capture_key_up(cx.listener(|view, event: &gpui_kit::KeyUpEvent, _, _| {
                if event.keystroke.key == "escape" {
                    view.escape_waiting = false;
                }
            }))
            .child(Label::new("JSON Utility"))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(buttons)
                    .child(
                        Button::new("json-paste")
                            .label("Paste")
                            .tab_stop(!self.lane.active)
                            .disabled(self.lane.active)
                            .on_click(cx.listener(|view, _, _, cx| view.read_clipboard(false, cx))),
                    )
                    .child(
                        Button::new("json-clear")
                            .label("Clear")
                            .tab_stop(true)
                            .on_click(cx.listener(|view, _, window, cx| view.clear(window, cx))),
                    )
                    .child(
                        Button::new("json-copy")
                            .label("Copy Result")
                            .tab_stop(!self.lane.active && !checking)
                            .disabled(self.lane.active || checking)
                            .on_click(cx.listener(|view, _, _, cx| view.copy(cx))),
                    )
                    .child(
                        Button::new("json-close")
                            .label("Close")
                            .tab_stop(true)
                            .on_click(cx.listener(|view, _, window, cx| view.close(window, cx))),
                    ),
            )
            .child(
                Textarea::new(&self.input)
                    .aria_label("JSON text")
                    .tab_index(0),
            )
            .children(self.checking.is_some().then(|| {
                div()
                    .id("json-checking-clipboard")
                    .role(Role::Status)
                    .aria_label("Checking clipboard JSON…")
                    .child("Checking clipboard JSON…")
            }))
            .children(error.map(|message| {
                div()
                    .id("json-error")
                    .role(Role::Alert)
                    .aria_label(message.clone())
                    .child(message)
            }))
            .children(feedback.map(|message| {
                div()
                    .id("json-feedback")
                    .role(Role::Status)
                    .aria_label(message.clone())
                    .child(message)
            }))
    }
}
impl RootWindow {
    pub(super) fn open_json_utility(
        &mut self,
        intent: JsonUtilityIntent,
        origin: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut host = OpenHost::default();
        multi_launcher::commands::handlers::handle_json_utility(
            &mut host,
            &JsonUtilityCommand::Open { intent },
        );
        let Some(intent) = host.0 else {
            return;
        };
        if self.restart_admission.is_fenced() {
            return;
        }
        let Some(root_window) = self.root_window else {
            return;
        };
        let root = cx.entity().downgrade();
        let admission = self.restart_admission.clone();
        let visibility = self.root_visibility.clone();
        let origin = origin.filter(|focus| self.focus_handle.contains(focus, window));
        let offset = self.root_scroll.offset();
        let native = self.native_window;
        let view = if let Some(view) = &self.json_view {
            view.clone()
        } else {
            cx.new(|cx| {
                JsonView::new(
                    window,
                    cx,
                    root,
                    root_window,
                    admission,
                    visibility,
                    native,
                    intent,
                    origin.clone(),
                    offset,
                )
            })
        };
        view.update(cx, |view, cx| {
            view.reopen(intent, origin, offset, window, cx)
        });
        self.json_view = Some(view);
        panel_stack::promote(&mut self.panel_stack, Panel::JsonUtilityDialog);
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
            );
        self.root_scroll.scroll_to_top_of_item(index);
        cx.notify();
    }
}
#[cfg(test)]
mod tests {
    use super::{
        ClipboardLane, OpenHost, apply_paste_result, provider_is_current, receipt_current,
    };
    use multi_launcher::{
        commands::{Command, JsonUtilityCommand, JsonUtilityIntent, parse_action},
        plugin::Plugin,
        plugins::json_utility::JsonUtilityPlugin,
    };
    #[test]
    fn json_route_and_clipboard_receipts_are_exact() {
        let plugin = JsonUtilityPlugin;
        for query in ["json", "json format", "json pretty", "json minify"] {
            let action = plugin.search(query).pop().unwrap();
            assert!(provider_is_current(query, &action, None, None));
            assert!(!provider_is_current("json extra", &action, None, None));
            assert!(!provider_is_current(
                query,
                &action,
                Some(&std::collections::HashSet::new()),
                None
            ));
            let disabled =
                std::collections::HashMap::from([("json_utility".to_owned(), Vec::new())]);
            assert!(!provider_is_current(query, &action, None, Some(&disabled)));
            assert!(matches!(
                super::super::parse_installed_search_invocation(
                    action.clone(),
                    multi_launcher::commands::ActivationSource::Enter
                )
                .unwrap()
                .route,
                super::super::InstalledSearchRoute::JsonUtility(_)
            ));
            let Command::JsonUtility(command) = parse_action(&action).unwrap() else {
                panic!("expected JSON command")
            };
            let JsonUtilityCommand::Open { intent } = command;
            let mut host = OpenHost::default();
            multi_launcher::commands::handlers::handle_json_utility(&mut host, &command);
            assert_eq!(host.0, Some(intent));
        }
        assert_eq!(
            plugin.search("json pretty")[0].action,
            "json_utility:format"
        );
        let mut host = OpenHost::default();
        multi_launcher::commands::handlers::handle_json_utility(
            &mut host,
            &JsonUtilityCommand::Open {
                intent: JsonUtilityIntent::Minify,
            },
        );
        assert_eq!(host.0, Some(JsonUtilityIntent::Minify));
        assert!(receipt_current(true, 4, 4, true));
        assert!(!receipt_current(true, 5, 4, true));
        assert!(!receipt_current(false, 4, 4, true));
        assert!(!receipt_current(true, 4, 4, false));
    }

    #[test]
    fn clipboard_lane_coalesces_reopens_and_discards_after_edit_or_close() {
        let mut lane = ClipboardLane::default();
        lane.active = true;
        lane.queue_initial(2, 8);
        lane.queue_initial(3, 8);
        assert_eq!(lane.take_ready(), None);
        lane.active = false;
        assert_eq!(lane.take_ready(), Some((3, 8)));
        lane.queue_initial(4, 8);
        lane.invalidate();
        assert_eq!(lane.take_ready(), None);
        assert!(!receipt_current(true, 5, 4, true));
        assert!(!receipt_current(false, 4, 4, true));
        assert!(!receipt_current(true, 4, 4, false));
    }

    #[test]
    fn paste_result_retires_only_stale_or_successfully_replaced_initial_projection() {
        use multi_launcher::clipboard_modify::clipboard::ClipboardError;
        use multi_launcher::json_utility_session::JsonEditorState;

        let mut session = JsonEditorState::default();
        session.begin_initialize();
        session.finish_initialize(Ok("{\"old\":true}".into()));
        let mut initial = Some((1, 2));
        let mut checking = Some(1);
        apply_paste_result(
            &mut session,
            &mut initial,
            &mut checking,
            false,
            Ok("new text".into()),
        );
        assert_eq!(session.source(), "new text");
        assert_eq!(initial, None);
        assert_eq!(checking, None);

        session.begin_initialize();
        session.finish_initialize(Ok("{}".into()));
        initial = Some((3, 4));
        checking = Some(3);
        apply_paste_result(
            &mut session,
            &mut initial,
            &mut checking,
            false,
            Err(ClipboardError::NonText),
        );
        assert!(
            matches!(session.feedback(), Some(Err(message)) if message.contains("Could not read clipboard"))
        );
        assert_eq!(initial, None);
        assert_eq!(checking, None);

        session.finish_initialize(Ok("{}".into()));
        initial = Some((5, 6));
        checking = Some(5);
        apply_paste_result(
            &mut session,
            &mut initial,
            &mut checking,
            true,
            Err(ClipboardError::NonText),
        );
        assert_eq!(initial, Some((5, 6)));
        assert_eq!(checking, Some(5));
        assert!(session.loading_clipboard());
    }
}
