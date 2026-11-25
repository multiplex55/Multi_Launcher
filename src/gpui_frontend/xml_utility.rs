//! Retained XML Source workspace and its single engine owner.
use super::*;
use multi_launcher::commands::{XmlToolkitCommand, XmlToolkitCommandHost};
use multi_launcher::xml_toolkit_engine::{XmlFormatMode, XmlNodeDetailKind, XmlNodeId};
use multi_launcher::xml_toolkit_session::{
    XmlDetailIntent, XmlDetailStatus, XmlEngineSession, XmlNodeDetailOutput, XmlNodeSelection,
    XmlOutputStatus, XmlParseStatus, XmlSessionSnapshot, XmlSourceRevision, XmlTreeStatus,
};
use multi_launcher::xml_tree_model::{default_detail_kind, XmlToolkitTreeViewState};

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

#[derive(Clone, Copy)]
enum ClipboardIntent {
    Paste,
    Source,
    Output,
    Tree,
}
fn output_receipt_current(current: u64, expected: u64) -> bool {
    current == expected
}
fn pump_needed(snapshot_pending: bool, tree_service_due: bool) -> bool {
    snapshot_pending || tree_service_due
}
fn snapshot_pending(snapshot: &XmlSessionSnapshot) -> bool {
    matches!(snapshot.parse, XmlParseStatus::Pending)
        || matches!(snapshot.output, XmlOutputStatus::Pending(_))
        || matches!(snapshot.tree, XmlTreeStatus::Pending)
        || matches!(snapshot.detail, XmlDetailStatus::Pending(_))
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
    output_focus: FocusHandle,
    tree_focus: FocusHandle,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    epoch: Arc<AtomicU64>,
    output_epoch: Arc<AtomicU64>,
    output_tab: bool,
    pub(super) tree_tab: bool,
    pub(super) tree: XmlToolkitTreeViewState,
    pub(super) tree_scroll: gpui_kit::UniformListScrollHandle,
    tree_epoch: Arc<AtomicU64>,
    tree_service_due: bool,
    tree_copy_visibility: Option<u64>,
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
            output_focus: cx.focus_handle(),
            tree_focus: cx.focus_handle(),
            owner,
            mount,
            epoch: Arc::new(AtomicU64::new(1)),
            output_epoch: Arc::new(AtomicU64::new(1)),
            output_tab: false,
            tree_tab: false,
            tree: XmlToolkitTreeViewState::default(),
            tree_scroll: gpui_kit::UniformListScrollHandle::default(),
            tree_epoch: Arc::new(AtomicU64::new(1)),
            tree_service_due: false,
            tree_copy_visibility: None,
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
        self.tree_epoch.fetch_add(1, Ordering::AcqRel);
        self.tree.revoke_copy_authority();
        self.tree_copy_visibility = None;
        self.tree_service_due = false;
        self.tree_scroll = gpui_kit::UniformListScrollHandle::default();
        self.output_epoch.fetch_add(1, Ordering::AcqRel);
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
    fn service_tick(&mut self, cx: &mut Context<Self>) -> bool {
        let snapshot = match &self.engine {
            Engine::Active(session) => Some(session.snapshot()),
            _ => None,
        };
        if let Some(snapshot) = &snapshot {
            if current_source_revision(self.accepted_revision, snapshot.revision) {
                self.service_tree(snapshot, cx);
            }
        }
        let pending = match &self.engine {
            Engine::Starting { .. } | Engine::Closing(_) => true,
            Engine::Active(_) => snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot_pending(snapshot)),
            Engine::Idle => false,
        };
        pump_needed(pending, self.tree_service_due)
    }
    fn pump(&mut self, cx: &mut Context<Self>) {
        if !self.service_tick(cx) || self.pumping {
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
                    let pending = view.service_tick(cx);
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
        self.tree_epoch.fetch_add(1, Ordering::AcqRel);
        self.tree.revoke_copy_authority();
        self.tree.sync(&XmlSessionSnapshot::default());
        self.tree_copy_visibility = None;
        self.tree_service_due = false;
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
        if self.tree_tab {
            self.tree_focus.focus(window, cx);
        } else if self.output_tab {
            self.output_focus.focus(window, cx);
        } else {
            self.input.focus_handle(cx).focus(window, cx);
        }
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
    pub(super) fn current_snapshot(&self) -> Option<Arc<XmlSessionSnapshot>> {
        let Engine::Active(session) = &self.engine else {
            return None;
        };
        let snapshot = session.snapshot();
        current_source_revision(self.accepted_revision, snapshot.revision).then_some(snapshot)
    }
    pub(super) fn tree_mount(&self) -> utility_mount::MountReceipt {
        self.mount.clone()
    }
    pub(super) fn tree_live(&self) -> bool {
        self.open && self.tree_tab && self.mount.is_current() && !self.admission.is_fenced()
    }
    pub(super) fn tree_toggle(
        &mut self,
        node: XmlNodeId,
        revision: XmlSourceRevision,
        cx: &mut Context<Self>,
    ) {
        if !self.tree_live()
            || self.accepted_revision != Some(revision)
            || !self.tree.visible_contains(node)
        {
            return;
        }
        self.tree_epoch.fetch_add(1, Ordering::AcqRel);
        self.tree.revoke_copy_authority();
        self.tree_copy_visibility = None;
        self.tree.toggle_expansion(node);
        cx.notify();
    }
    pub(super) fn tree_select(
        &mut self,
        node: XmlNodeId,
        revision: XmlSourceRevision,
        cx: &mut Context<Self>,
    ) {
        if !self.tree_live()
            || self.accepted_revision != Some(revision)
            || !self.tree.visible_contains(node)
        {
            return;
        }
        self.tree_epoch.fetch_add(1, Ordering::AcqRel);
        self.tree.revoke_copy_authority();
        self.tree_copy_visibility = None;
        if let Some(selection) = self.tree.select_node(node) {
            let kind = self
                .tree
                .projection()
                .and_then(|p| p.node(node))
                .and_then(|node| default_detail_kind(Some(node.kind)));
            if let Some(kind) = kind {
                self.tree_detail(selection, kind, XmlDetailIntent::Inspect, cx);
            } else {
                cx.notify();
            }
        }
    }
    pub(super) fn tree_detail(
        &mut self,
        selection: XmlNodeSelection,
        kind: XmlNodeDetailKind,
        intent: XmlDetailIntent,
        cx: &mut Context<Self>,
    ) {
        if !self.tree_live() || self.tree.selected() != Some(selection) {
            return;
        }
        self.tree_epoch.fetch_add(1, Ordering::AcqRel);
        self.tree_copy_visibility = if intent == XmlDetailIntent::Copy {
            let revision = self.visibility.request_revision.load(Ordering::Acquire);
            if !json_utility::native_host_current(&self.visibility, revision, self.native, false) {
                return;
            }
            Some(revision)
        } else {
            None
        };
        let Some(request) = self.tree.request_detail(kind, intent) else {
            return;
        };
        self.tree_service_due = true;
        if let Engine::Active(session) = &self.engine {
            if let Err(error) = session.request_detail(request) {
                self.tree_service_due = false;
                self.tree_copy_visibility = None;
                self.tree.revoke_copy_authority();
                self.feedback = Some(format!("Could not inspect XML node: {error:?}"));
            }
        }
        self.pump(cx);
        cx.notify();
    }
    fn service_tree(&mut self, snapshot: &XmlSessionSnapshot, cx: &mut Context<Self>) {
        self.tree.sync(snapshot);
        if !matches!(snapshot.detail, XmlDetailStatus::Pending(_)) {
            self.tree_service_due = false;
        }
        if self.tree_live() && !self.clipboard_pending {
            if let Some(revision) = self.tree_copy_visibility {
                if !json_utility::native_host_current(
                    &self.visibility,
                    revision,
                    self.native,
                    false,
                ) {
                    self.tree_copy_visibility = None;
                    self.tree.revoke_copy_authority();
                } else if let Some(output) = self.tree.take_valid_copy_completion(snapshot) {
                    self.tree_copy_visibility = None;
                    self.clipboard(ClipboardIntent::Tree, Some((output, revision)), cx);
                }
            }
        }
    }
    fn service_current_tree(&mut self, cx: &mut Context<Self>) {
        if let Some(snapshot) = self.current_snapshot() {
            self.service_tree(&snapshot, cx);
        }
    }
    fn format(&mut self, mode: XmlFormatMode, cx: &mut Context<Self>) {
        if !self.open || !self.mount.is_current() || self.admission.is_fenced() {
            return;
        }
        let Some(snapshot) = self.current_snapshot() else {
            return;
        };
        if snapshot.parse != XmlParseStatus::Ready {
            return;
        }
        let Some(revision) = snapshot.revision else {
            return;
        };
        self.output_epoch.fetch_add(1, Ordering::AcqRel);
        if let Engine::Active(session) = &self.engine {
            if let Err(error) = session.format(revision, mode) {
                self.feedback = Some(format!("Could not format XML: {error:?}"));
            } else {
                self.feedback = None;
            }
        }
        self.pump(cx);
        cx.notify();
    }
    fn apply_output(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || !self.mount.is_current() || self.admission.is_fenced() {
            return;
        }
        let Some(snapshot) = self.current_snapshot() else {
            return;
        };
        match multi_launcher::xml_source::output_text_for_apply(&snapshot) {
            Ok(text) => {
                self.epoch.fetch_add(1, Ordering::AcqRel);
                self.source = text.clone();
                self.feedback = None;
                self.input
                    .update(cx, |input, cx| input.set_value(&text, window, cx));
                self.submit(cx);
            }
            Err(error) => {
                self.feedback = Some(error);
                cx.notify();
            }
        }
    }
    fn clipboard(
        &mut self,
        intent: ClipboardIntent,
        tree_output: Option<(Arc<XmlNodeDetailOutput>, u64)>,
        cx: &mut Context<Self>,
    ) {
        if !self.open || self.clipboard_pending {
            return;
        }
        let output = if matches!(intent, ClipboardIntent::Output) {
            let Some(snapshot) = self.current_snapshot() else {
                return;
            };
            let XmlOutputStatus::Ready(output) = &snapshot.output else {
                return;
            };
            if multi_launcher::xml_source::current_output(&snapshot).is_none() {
                return;
            }
            Some(output.clone())
        } else {
            None
        };
        let Some(permit) = self.admission.admit() else {
            return;
        };
        self.clipboard_pending = true;
        let expected = self.epoch.load(Ordering::Acquire);
        let epoch = self.epoch.clone();
        let mount = self.mount.clone();
        let work_mount = mount.clone();
        let visibility = self.visibility.clone();
        let revision = tree_output.as_ref().map_or_else(
            || visibility.request_revision.load(Ordering::Acquire),
            |(_, revision)| *revision,
        );
        let native = self.native;
        let admission = self.admission.clone();
        let source = self.source.clone();
        let tree_epoch = self.tree_epoch.clone();
        let expected_tree = tree_epoch.load(Ordering::Acquire);
        let worker_tree_epoch = tree_epoch.clone();
        let output_epoch = self.output_epoch.clone();
        let expected_output = output_epoch.load(Ordering::Acquire);
        let worker_output_epoch = output_epoch.clone();
        let task = cx.background_executor().spawn(async move {
            let _permit = permit;
            if !matches!(intent, ClipboardIntent::Paste) {
                if !clipboard_receipt_current(
                    &work_mount,
                    epoch.load(Ordering::Acquire),
                    expected,
                    !admission.is_fenced()
                        && json_utility::native_host_current(&visibility, revision, native, false)
                        && (!matches!(intent, ClipboardIntent::Tree)
                            || output_receipt_current(
                                worker_tree_epoch.load(Ordering::Acquire),
                                expected_tree,
                            ))
                        && (!matches!(intent, ClipboardIntent::Output)
                            || output_receipt_current(
                                worker_output_epoch.load(Ordering::Acquire),
                                expected_output,
                            )),
                ) {
                    return Err("Clipboard request was superseded.".to_owned());
                }
                ArboardClipboardBackend
                    .write_text(tree_output.as_ref().map_or_else(
                        || {
                            output
                                .as_ref()
                                .map_or(source.as_str(), |output| output.text.as_str())
                        },
                        |(output, _)| output.value.as_str(),
                    ))
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
                            && (!matches!(intent, ClipboardIntent::Tree)
                                || output_receipt_current(
                                    tree_epoch.load(Ordering::Acquire),
                                    expected_tree,
                                ))
                            && (!matches!(intent, ClipboardIntent::Output)
                                || output_receipt_current(
                                    output_epoch.load(Ordering::Acquire),
                                    expected_output,
                                ))
                            && json_utility::native_host_current(
                                &view.visibility,
                                revision,
                                view.native,
                                false,
                            ),
                    )
                {
                    view.service_current_tree(cx);
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
                    Ok(None) => {
                        view.feedback = Some(
                            if matches!(intent, ClipboardIntent::Output) {
                                "Copied current XML output."
                            } else if matches!(intent, ClipboardIntent::Tree) {
                                "Copied the selected XML value."
                            } else {
                                "XML source copied"
                            }
                            .into(),
                        )
                    }
                    Err(error) => view.feedback = Some(error),
                }
                view.service_current_tree(cx);
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
        let format_mount = self.mount.clone();
        let minify_mount = self.mount.clone();
        let apply_mount = self.mount.clone();
        let output_copy_mount = self.mount.clone();
        let source_tab_mount = self.mount.clone();
        let output_tab_mount = self.mount.clone();
        let tree_tab_mount = self.mount.clone();
        let source_epoch = self.epoch.load(Ordering::Acquire);
        let output_epoch = self.output_epoch.load(Ordering::Acquire);
        let snapshot = self.current_snapshot();
        let can_format = snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.parse == XmlParseStatus::Ready);
        let output = snapshot
            .as_ref()
            .and_then(|snapshot| multi_launcher::xml_source::current_output(snapshot));
        let can_copy = output.is_some() && !self.clipboard_pending;
        let can_apply = output.is_some_and(|output| {
            output.text.len() <= multi_launcher::xml_toolkit_engine::MAX_XML_INPUT_BYTES
        });
        let output_text = match snapshot.as_ref().map(|snapshot| &snapshot.output) {
            Some(XmlOutputStatus::Pending(XmlFormatMode::Pretty)) => {
                "Formatting the current XML…".to_owned()
            }
            Some(XmlOutputStatus::Pending(XmlFormatMode::Minify)) => {
                "Minifying the current XML…".to_owned()
            }
            Some(XmlOutputStatus::Failed(error)) => format!(
                "XML output failed: {}",
                multi_launcher::xml_source::xml_error_message(error)
            ),
            Some(XmlOutputStatus::Ready(output))
                if snapshot.as_ref().is_some_and(|snapshot| {
                    multi_launcher::xml_source::current_output(snapshot).is_some()
                }) =>
            {
                let preview = multi_launcher::xml_source::output_preview(&output.text);
                format!(
                    "{} UTF-8 bytes\n{}{}",
                    output.text.len(),
                    preview,
                    if preview.len() < output.text.len() {
                        "\nOutput preview is clipped. Copy the full output explicitly."
                    } else {
                        ""
                    }
                )
            }
            _ => "Format or minify the current parsed source to create output.".to_owned(),
        };
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
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("xml-tab-source")
                            .label("Source")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if source_tab_mount.is_current()
                                    && view.open
                                    && !view.admission.is_fenced()
                                {
                                    view.tree_epoch.fetch_add(1, Ordering::AcqRel);
                                    view.tree.revoke_copy_authority();
                                    view.tree_copy_visibility = None;
                                    view.tree_tab = false;
                                    view.output_tab = false;
                                    view.input.focus_handle(cx).focus(window, cx);
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Button::new("xml-tab-output")
                            .label("Output")
                            .track_focus(&self.output_focus)
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if output_tab_mount.is_current()
                                    && view.open
                                    && !view.admission.is_fenced()
                                {
                                    view.tree_epoch.fetch_add(1, Ordering::AcqRel);
                                    view.tree.revoke_copy_authority();
                                    view.tree_copy_visibility = None;
                                    view.tree_tab = false;
                                    view.output_tab = true;
                                    view.output_focus.focus(window, cx);
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .child(
                Button::new("xml-tab-tree")
                    .label("Tree")
                    .track_focus(&self.tree_focus)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if tree_tab_mount.is_current() && view.open && !view.admission.is_fenced() {
                            view.tree_tab = true;
                            view.output_tab = false;
                            view.tree_focus.focus(window, cx);
                            view.service_current_tree(cx);
                            cx.notify();
                        }
                    })),
            )
            .children(self.tree_tab.then(|| super::xml_tree::render(self, cx)))
            .when(!self.output_tab && !self.tree_tab, |div| {
                div.child(
                    Textarea::new(&self.input)
                        .aria_label("XML Source")
                        .tab_index(0),
                )
            })
            .when(self.output_tab, |container| {
                container.child(
                    div()
                        .id("xml-output-preview")
                        .max_h(px(240.0))
                        .overflow_y_scroll()
                        .font_family("Consolas")
                        .child(output_text),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("xml-paste")
                            .label("Paste")
                            .disabled(self.clipboard_pending)
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if paste_mount.is_current() {
                                    view.clipboard(ClipboardIntent::Paste, None, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("xml-copy")
                            .label("Copy Source")
                            .disabled(self.clipboard_pending)
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if copy_mount.is_current() {
                                    view.clipboard(ClipboardIntent::Source, None, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("xml-format")
                            .label("Format")
                            .disabled(!can_format)
                            .tab_stop(can_format)
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if format_mount.is_current()
                                    && view.epoch.load(Ordering::Acquire) == source_epoch
                                {
                                    view.format(XmlFormatMode::Pretty, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("xml-minify")
                            .label("Minify")
                            .disabled(!can_format)
                            .tab_stop(can_format)
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if minify_mount.is_current()
                                    && view.epoch.load(Ordering::Acquire) == source_epoch
                                {
                                    view.format(XmlFormatMode::Minify, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("xml-copy-output")
                            .label("Copy Output")
                            .disabled(!can_copy)
                            .tab_stop(can_copy)
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if output_copy_mount.is_current()
                                    && view.output_epoch.load(Ordering::Acquire) == output_epoch
                                {
                                    view.clipboard(ClipboardIntent::Output, None, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("xml-apply-output")
                            .label("Apply Output to Source")
                            .disabled(!can_apply)
                            .tab_stop(can_apply)
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if apply_mount.is_current()
                                    && view.output_epoch.load(Ordering::Acquire) == output_epoch
                                {
                                    view.apply_output(window, cx);
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
                                if view.tree_tab {
                                    view.tree_focus.focus(window, cx);
                                } else if view.output_tab {
                                    view.output_focus.focus(window, cx);
                                } else {
                                    view.input.focus_handle(cx).focus(window, cx);
                                }
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
        clipboard_receipt_current, output_receipt_current, provider_is_current, pump_needed,
        snapshot_pending, startup_disposition, StartupDisposition,
    };
    use multi_launcher::plugin::Plugin;
    use multi_launcher::plugins::xml_toolkit::XmlToolkitPlugin;
    #[test]
    fn pending_and_ready_parse_are_decided_from_the_same_snapshot() {
        use multi_launcher::xml_toolkit_session::{XmlParseStatus, XmlSessionSnapshot};
        let mut snapshot = XmlSessionSnapshot::default();
        snapshot.parse = XmlParseStatus::Pending;
        assert!(snapshot_pending(&snapshot));
        snapshot.parse = XmlParseStatus::Ready;
        assert!(!snapshot_pending(&snapshot));
    }
    #[test]
    fn terminal_detail_between_service_and_pending_check_still_wakes() {
        assert!(pump_needed(false, true));
        assert!(pump_needed(true, false));
        assert!(!pump_needed(false, false));
    }
    #[test]
    fn output_generation_supersession_revokes_old_copy() {
        assert!(output_receipt_current(12, 12));
        assert!(!output_receipt_current(13, 12));
    }
    #[test]
    fn output_preview_is_unicode_bounded_without_mutating_full_payload() {
        let full = "é🦀".repeat(4_001);
        let preview = multi_launcher::xml_source::output_preview(&full);
        assert_eq!(preview.chars().count(), 8_000);
        assert_eq!(full.chars().count(), 8_002);
        assert_eq!(preview, &full[..preview.len()]);
    }
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
