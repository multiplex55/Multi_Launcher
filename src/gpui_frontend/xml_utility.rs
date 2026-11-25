//! Retained XML Source workspace and its single engine owner.
use super::*;
use multi_launcher::commands::{XmlToolkitCommand, XmlToolkitCommandHost};
use multi_launcher::xml_toolkit_session::{XmlEngineSession, XmlParseStatus, XmlSourceRevision};

pub(super) fn provider_is_current(
    query: &str,
    action: &Action,
    enabled: Option<&HashSet<String>>,
    caps: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    let plugin = multi_launcher::plugins::xml_toolkit::XmlToolkitPlugin;
    search_providers::search_routed(
        [&plugin as &dyn multi_launcher::plugin::Plugin],
        query,
        enabled,
        caps,
    )
    .iter()
    .any(|(name, row)| name == "xml_toolkit" && row == action)
}
fn clipboard_receipt_current(
    mount: &utility_mount::MountReceipt,
    current: u64,
    expected: u64,
    host_current: bool,
) -> bool {
    mount.is_current() && current == expected && host_current
}
#[derive(Debug, PartialEq, Eq)]
enum StartupDisposition {
    Accept,
    Reap,
    Retry,
    Failure,
    Retire,
}
fn startup_disposition(
    open: bool,
    cancelled: bool,
    fenced: bool,
    success: bool,
) -> StartupDisposition {
    if success {
        if open && !cancelled && !fenced {
            StartupDisposition::Accept
        } else {
            StartupDisposition::Reap
        }
    } else if open && cancelled && !fenced {
        StartupDisposition::Retry
    } else if open && !fenced {
        StartupDisposition::Failure
    } else {
        StartupDisposition::Retire
    }
}
fn current_source_revision(
    accepted: Option<XmlSourceRevision>,
    snapshot: Option<XmlSourceRevision>,
) -> bool {
    accepted.is_some() && accepted == snapshot
}
#[derive(Default)]
struct OpenHost(bool);
impl XmlToolkitCommandHost for OpenHost {
    fn open_xml_toolkit(&mut self) {
        self.0 = true;
    }
}

enum Engine {
    Idle,
    Starting { cancel: bool },
    Active(XmlEngineSession),
    Closing(XmlEngineSession),
}
pub(super) struct XmlWorkspace {
    pub(super) open: bool,
    source: String,
    input: Entity<TextareaState>,
    focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    epoch: Arc<AtomicU64>,
    engine: Engine,
    accepted_revision: Option<XmlSourceRevision>,
    pumping: bool,
    shutdown_error: Option<String>,
    clipboard_pending: bool,
    feedback: Option<String>,
    root: WeakEntity<RootWindow>,
    root_window: AnyWindowHandle,
    native: Option<NativeWindow>,
    visibility: RootVisibility,
    admission: Arc<RestartAdmission>,
    activity: multi_launcher::activity::ActivityCenter,
    _subscription: Subscription,
}
impl XmlWorkspace {
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        root: WeakEntity<RootWindow>,
        root_window: AnyWindowHandle,
        native: Option<NativeWindow>,
        visibility: RootVisibility,
        admission: Arc<RestartAdmission>,
        activity: multi_launcher::activity::ActivityCenter,
    ) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(5, 12)
                .placeholder("Enter XML source")
        });
        let subscription = cx.subscribe(
            &input,
            |this: &mut Self,
             input: Entity<TextareaState>,
             event: &InputEvent,
             cx: &mut Context<Self>| {
                if !this.open
                    || !this.mount.is_current()
                    || !matches!(event, InputEvent::Change)
                    || this.admission.is_fenced()
                {
                    return;
                }
                let value = input.read(cx).value().to_string();
                if this.source != value {
                    this.epoch.fetch_add(1, Ordering::AcqRel);
                    this.source = value;
                    this.feedback = None;
                    this.submit(cx);
                }
            },
        );
        Self {
            open: false,
            source: String::new(),
            input,
            focus: cx.focus_handle(),
            return_focus: None,
            owner,
            mount,
            epoch: Arc::new(AtomicU64::new(1)),
            engine: Engine::Idle,
            accepted_revision: None,
            pumping: false,
            shutdown_error: None,
            clipboard_pending: false,
            feedback: None,
            root,
            root_window,
            native,
            visibility,
            admission,
            activity,
            _subscription: subscription,
        }
    }
    fn start(&mut self, cx: &mut Context<Self>) {
        if !self.open || !matches!(self.engine, Engine::Idle) || self.admission.is_fenced() {
            return;
        }
        self.engine = Engine::Starting { cancel: false };
        let activity = self.activity.clone();
        let task = cx
            .background_executor()
            .spawn(async move { XmlEngineSession::start_with_activity(activity) });
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = weak.update(cx, |view, cx| {
                let cancelled = matches!(view.engine, Engine::Starting { cancel: true });
                let disposition = startup_disposition(
                    view.open,
                    cancelled,
                    view.admission.is_fenced(),
                    result.is_ok(),
                );
                match result {
                    Ok(session) if disposition == StartupDisposition::Accept => {
                        if view
                            .shutdown_error
                            .as_ref()
                            .is_some_and(|error| view.feedback.as_ref() == Some(error))
                        {
                            view.feedback = None;
                        }
                        view.shutdown_error = None;
                        view.engine = Engine::Active(session);
                        view.submit(cx);
                    }
                    Ok(session) => {
                        session.request_shutdown();
                        view.engine = Engine::Closing(session);
                    }
                    Err(error) => {
                        view.engine = Engine::Idle;
                        if disposition == StartupDisposition::Retry {
                            view.start(cx);
                        } else if disposition == StartupDisposition::Failure {
                            view.feedback =
                                Some(format!("Could not start secure XML session: {error:?}"));
                        }
                    }
                }
                view.pump(cx);
                cx.notify();
            });
        })
        .detach();
    }
    fn submit(&mut self, cx: &mut Context<Self>) {
        self.accepted_revision = None;
        if let Engine::Active(session) = &self.engine {
            match session.replace_source(self.source.clone()) {
                Ok(revision) => self.accepted_revision = Some(revision),
                Err(error) => {
                    self.feedback = Some(format!("Could not update XML source: {error:?}"))
                }
            }
        }
        self.pump(cx);
        cx.notify();
    }
    fn pump(&mut self, cx: &mut Context<Self>) {
        let pending = match &self.engine {
            Engine::Starting { .. } | Engine::Closing(_) => true,
            Engine::Active(session) => matches!(session.snapshot().parse, XmlParseStatus::Pending),
            Engine::Idle => false,
        };
        if !pending || self.pumping {
            return;
        }
        self.pumping = true;
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(40))
                .await;
            let keep = weak
                .update(cx, |view, cx| {
                    let done = if let Engine::Closing(session) = &view.engine {
                        match session.try_reap() {
                            Ok(done) => done,
                            Err(error) => {
                                let message = format!("XML worker shutdown failed: {error:?}");
                                view.shutdown_error = Some(message.clone());
                                view.feedback = Some(message);
                                true
                            }
                        }
                    } else {
                        false
                    };
                    if done {
                        view.engine = Engine::Idle;
                        if view.open && view.shutdown_error.is_none() {
                            view.start(cx);
                        }
                    }
                    let pending = match &view.engine {
                        Engine::Starting { .. } | Engine::Closing(_) => true,
                        Engine::Active(session) => {
                            matches!(session.snapshot().parse, XmlParseStatus::Pending)
                        }
                        Engine::Idle => false,
                    };
                    if !pending {
                        view.pumping = false;
                    }
                    cx.notify();
                    pending
                })
                .unwrap_or(false);
            if !keep {
                break;
            }
        })
        .detach();
    }
    pub(super) fn shutdown(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self.owner.retire();
        self.epoch.fetch_add(1, Ordering::AcqRel);
        let state = std::mem::replace(&mut self.engine, Engine::Idle);
        self.engine = match state {
            Engine::Starting { .. } => Engine::Starting { cancel: true },
            Engine::Active(session) => {
                session.request_shutdown();
                Engine::Closing(session)
            }
            other => other,
        };
        self.pump(cx);
    }
    pub(super) fn reaped(&self) -> bool {
        matches!(self.engine, Engine::Idle)
    }
    pub(super) fn shutdown_failure(&self) -> Option<String> {
        self.shutdown_error.clone()
    }
    pub(super) fn draft(&self) -> Option<String> {
        (!self.source.is_empty()).then(|| self.source.clone())
    }
    fn reopen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            self.mount = self.owner.attach();
            self.open = true;
            self.start(cx);
        }
        if self.open && matches!(self.engine, Engine::Idle) {
            self.start(cx);
        }
        self.input.focus_handle(cx).focus(window, cx);
        cx.notify();
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let restore = window.is_window_active() && self.focus.contains_focused(window, cx);
        self.shutdown(cx);
        let _ = self.root.update(cx, |root, cx| {
            panel_stack::retire(&mut root.panel_stack, Panel::XmlToolkitDialog);
            cx.notify();
        });
        if restore {
            if let Some(focus) = self.return_focus.take() {
                focus.focus(window, cx);
            }
        }
        cx.notify();
    }
    fn clipboard(&mut self, copy: bool, cx: &mut Context<Self>) {
        if !self.open || self.clipboard_pending {
            return;
        }
        let Some(permit) = self.admission.admit() else {
            return;
        };
        self.clipboard_pending = true;
        let expected = self.epoch.load(Ordering::Acquire);
        let epoch = self.epoch.clone();
        let mount = self.mount.clone();
        let work_mount = mount.clone();
        let visibility = self.visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let native = self.native;
        let admission = self.admission.clone();
        let source = self.source.clone();
        let task = cx.background_executor().spawn(async move {
            let _permit = permit;
            if copy {
                if !clipboard_receipt_current(
                    &work_mount,
                    epoch.load(Ordering::Acquire),
                    expected,
                    !admission.is_fenced()
                        && json_utility::native_host_current(&visibility, revision, native, false),
                ) {
                    return Err("Clipboard request was superseded.".to_owned());
                }
                ArboardClipboardBackend
                    .write_text(&source)
                    .map(|_| None)
                    .map_err(|error| format!("Could not copy XML: {error}"))
            } else {
                ArboardClipboardBackend
                    .read_text()
                    .map_err(|error| format!("Could not read clipboard: {error}"))
                    .and_then(|text| {
                        multi_launcher::xml_source::validate_clipboard_source(&text)?;
                        Ok(Some(text))
                    })
            }
        });
        let weak = cx.entity().downgrade();
        let root = self.root_window;
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let Some(view) = weak.upgrade() else {
                return;
            };
            let _ = update_kit_window_content(root, &view, cx, |view, window, cx| {
                view.clipboard_pending = false;
                if !view.open
                    || !clipboard_receipt_current(
                        &mount,
                        view.epoch.load(Ordering::Acquire),
                        expected,
                        !view.admission.is_fenced()
                            && json_utility::native_host_current(
                                &view.visibility,
                                revision,
                                view.native,
                                false,
                            ),
                    )
                {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(Some(text)) => {
                        view.epoch.fetch_add(1, Ordering::AcqRel);
                        view.source = text.clone();
                        view.feedback = None;
                        view.input
                            .update(cx, |input, cx| input.set_value(&text, window, cx));
                        view.submit(cx);
                    }
                    Ok(None) => view.feedback = Some("XML source copied".into()),
                    Err(error) => view.feedback = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
impl Render for XmlWorkspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let escape_mount = self.mount.clone();
        let paste_mount = self.mount.clone();
        let copy_mount = self.mount.clone();
        let clear_mount = self.mount.clone();
        let close_mount = self.mount.clone();
        let status = match &self.engine {
            Engine::Starting { .. } => "Starting XML session…".to_owned(),
            Engine::Closing(_) => "Stopping XML session…".to_owned(),
            Engine::Idle => "XML session unavailable".to_owned(),
            Engine::Active(session)
                if !current_source_revision(
                    self.accepted_revision,
                    session.snapshot().revision,
                ) =>
            {
                "XML source could not be submitted".into()
            }
            Engine::Active(session) => match &session.snapshot().parse {
                XmlParseStatus::Empty => "Enter XML source".into(),
                XmlParseStatus::Pending => "Parsing XML…".into(),
                XmlParseStatus::Ready => "XML parsed".into(),
                XmlParseStatus::Failed(error) => format!(
                    "XML parse error: {}",
                    multi_launcher::xml_source::xml_error_message(error)
                ),
            },
        };
        div()
            .id("xml-workspace")
            .track_focus(&self.focus)
            .capture_action(cx.listener(
                move |view, _: &gpui_kit::base::input::Escape, window, cx| {
                    let contexts = window.context_stack();
                    if escape_mount.is_current()
                        && view.open
                        && !view.admission.is_fenced()
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
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(Label::new("XML Toolkit"))
            .child(
                Textarea::new(&self.input)
                    .aria_label("XML Source")
                    .tab_index(0),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("xml-paste")
                            .label("Paste")
                            .disabled(self.clipboard_pending)
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if paste_mount.is_current() {
                                    view.clipboard(false, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("xml-copy")
                            .label("Copy Source")
                            .disabled(self.clipboard_pending)
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if copy_mount.is_current() {
                                    view.clipboard(true, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("xml-clear")
                            .label("Clear")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if !clear_mount.is_current()
                                    || !view.open
                                    || view.admission.is_fenced()
                                {
                                    return;
                                }
                                view.epoch.fetch_add(1, Ordering::AcqRel);
                                view.source.clear();
                                view.feedback = None;
                                view.input
                                    .update(cx, |input, cx| input.set_value("", window, cx));
                                view.submit(cx);
                                view.input.focus_handle(cx).focus(window, cx);
                            })),
                    )
                    .child(
                        Button::new("xml-close")
                            .label("Close")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if close_mount.is_current()
                                    && view.open
                                    && !view.admission.is_fenced()
                                {
                                    view.close(window, cx);
                                }
                            })),
                    ),
            )
            .child(div().id("xml-status").role(Role::Status).child(status))
            .children(self.feedback.as_ref().map(|text| Label::new(text.clone())))
    }
}
impl RootWindow {
    pub(super) fn open_xml_toolkit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut host = OpenHost::default();
        multi_launcher::commands::handlers::handle_xml_toolkit(&mut host, XmlToolkitCommand::Open);
        if !host.0 || self.restart_admission.is_fenced() {
            return;
        }
        let Some(root_window) = self.root_window else {
            return;
        };
        let root = cx.entity().downgrade();
        let view = self.xml_view.clone().unwrap_or_else(|| {
            cx.new(|cx| {
                XmlWorkspace::new(
                    window,
                    cx,
                    root,
                    root_window,
                    self.native_window,
                    self.root_visibility.clone(),
                    self.restart_admission.clone(),
                    self.activity_center.clone(),
                )
            })
        });
        let origin = self.query.focus_handle(cx);
        view.update(cx, |view, cx| {
            if !view.open {
                view.return_focus = Some(origin);
            }
            view.reopen(window, cx);
        });
        self.xml_view = Some(view);
        panel_stack::promote(&mut self.panel_stack, Panel::XmlToolkitDialog);
        let index =
            self.file_search_scroll_item()
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
                + usize::from(self.json_view.as_ref().is_some_and(|view| {
                    view.read(cx).is_mounted_in(root_window.window_id(), false)
                }));
        self.root_scroll.scroll_to_top_of_item(index);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::super::utility_mount::MountOwner;
    use super::{
        clipboard_receipt_current, provider_is_current, startup_disposition, StartupDisposition,
    };
    use multi_launcher::plugin::Plugin;
    use multi_launcher::plugins::xml_toolkit::XmlToolkitPlugin;
    #[test]
    fn cancelled_startup_reaps_success_retries_failure_and_terminal_never_reopens() {
        assert_eq!(
            startup_disposition(true, true, false, true),
            StartupDisposition::Reap
        );
        assert_eq!(
            startup_disposition(true, true, false, false),
            StartupDisposition::Retry
        );
        assert_eq!(
            startup_disposition(false, true, true, true),
            StartupDisposition::Reap
        );
        assert_eq!(
            startup_disposition(false, true, true, false),
            StartupDisposition::Retire
        );
        assert_eq!(
            startup_disposition(true, false, false, true),
            StartupDisposition::Accept
        );
    }
    #[test]
    fn clipboard_receipt_rejects_edits_hidden_host_and_replaced_mount() {
        let owner = MountOwner::default();
        let mount = owner.attach();
        assert!(clipboard_receipt_current(&mount, 4, 4, true));
        assert!(!clipboard_receipt_current(&mount, 5, 4, true));
        assert!(!clipboard_receipt_current(&mount, 4, 4, false));
        owner.attach();
        assert!(!clipboard_receipt_current(&mount, 4, 4, true));
    }
    #[test]
    fn stock_xml_search_receipt_respects_membership() {
        let plugin = XmlToolkitPlugin;
        let action = plugin.search("xml").remove(0);
        assert!(provider_is_current("xml", &action, None, None));
        assert!(!provider_is_current("xml other", &action, None, None));
        let disabled = std::collections::HashSet::new();
        assert!(!provider_is_current("xml", &action, Some(&disabled), None));
    }
}
