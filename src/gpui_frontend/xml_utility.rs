//! Retained XML Source workspace and its single engine owner.
use super::*;
use multi_launcher::commands::{XmlToolkitCommand, XmlToolkitCommandHost};
use multi_launcher::xml_toolkit_engine::{XmlFormatMode, XmlNodeDetailKind, XmlNodeId};
use multi_launcher::xml_toolkit_session::{
    XmlDetailIntent, XmlDetailStatus, XmlEngineSession, XmlNodeDetailOutput, XmlNodeDetailRequest,
    XmlNodeSelection, XmlOutputStatus, XmlParseStatus, XmlSchemaRevision, XmlSessionSnapshot,
    XmlSourceRevision, XmlTreeStatus, XmlXPathQueryRequest,
};
use multi_launcher::xml_tree_model::{default_detail_kind, XmlToolkitTreeViewState};
use multi_launcher::xml_xpath_model::{VisibleXPathStatus, XmlToolkitXPathViewState};

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
    Scalar,
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
        || matches!(
            snapshot.schema.status,
            multi_launcher::xml_toolkit_session::XmlSchemaStatus::Pending(_)
        )
        || matches!(snapshot.tree, XmlTreeStatus::Pending)
        || matches!(snapshot.detail, XmlDetailStatus::Pending(_))
        || matches!(
            snapshot.xpath,
            multi_launcher::xml_toolkit_session::XmlXPathStatus::Pending(_)
        )
}
#[derive(Clone, Copy)]
enum XmlPickerKind {
    Schema,
    Open,
}
struct XmlReadReceipt {
    mount: utility_mount::MountReceipt,
    source_epoch: u64,
    source_revision: Option<XmlSourceRevision>,
    owner: NativeWindow,
    visibility_revision: u64,
    root_window: AnyWindowHandle,
    open_focus: Option<FocusHandle>,
}
enum Engine {
    Idle,
    Starting { cancel: bool },
    Active(XmlEngineSession),
    Closing(XmlEngineSession),
}
pub(super) struct XPathBinding {
    pub(super) id: u64,
    pub(super) prefix: Entity<InputState>,
    pub(super) uri: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}
pub(super) struct XmlWorkspace {
    pub(super) open: bool,
    source: String,
    input: Entity<TextareaState>,
    focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    output_focus: FocusHandle,
    tree_focus: FocusHandle,
    xpath_focus: FocusHandle,
    schema_focus: FocusHandle,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    epoch: Arc<AtomicU64>,
    output_epoch: Arc<AtomicU64>,
    output_tab: bool,
    pub(super) tree_tab: bool,
    pub(super) xpath_tab: bool,
    pub(super) schema_tab: bool,
    schema_picker: bool,
    file_read_pending: bool,
    next_file_request: u64,
    schema_path_draft: Option<std::path::PathBuf>,
    pub(super) xpath: XmlToolkitXPathViewState,
    pub(super) xpath_expression: Entity<InputState>,
    pub(super) xpath_bindings: Vec<XPathBinding>,
    pub(super) xpath_namespaces_open: bool,
    pub(super) xpath_scroll: gpui_kit::UniformListScrollHandle,
    xpath_epoch: Arc<AtomicU64>,
    pub(super) tree: XmlToolkitTreeViewState,
    pub(super) tree_scroll: gpui_kit::UniformListScrollHandle,
    tree_epoch: Arc<AtomicU64>,
    tree_service_due: bool,
    tree_copy_visibility: Option<u64>,
    tree_copy_request: Option<XmlNodeDetailRequest>,
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
    _subscriptions: Vec<Subscription>,
    xpath_subscription: Option<Subscription>,
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
                    || this.schema_picker
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
        let xpath_expression =
            cx.new(|cx| InputState::new(window, cx).placeholder("//item[@id='example']"));
        let xpath_subscription = cx.subscribe(
            &xpath_expression,
            |view: &mut Self,
             input: Entity<InputState>,
             event: &InputEvent,
             cx: &mut Context<Self>| {
                if !view.xpath_live() || !matches!(event, InputEvent::Change) {
                    return;
                }
                let value = input.read(cx).value().to_string();
                view.xpath_epoch.fetch_add(1, Ordering::AcqRel);
                view.tree_epoch.fetch_add(1, Ordering::AcqRel);
                if view.xpath.edit_expression(value, &mut view.tree) {
                    view.invalidate_xpath(cx);
                    view.sync_xpath_editors(cx);
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
            xpath_focus: cx.focus_handle(),
            schema_focus: cx.focus_handle(),
            owner,
            mount,
            epoch: Arc::new(AtomicU64::new(1)),
            output_epoch: Arc::new(AtomicU64::new(1)),
            output_tab: false,
            tree_tab: false,
            xpath_tab: false,
            schema_tab: false,
            schema_picker: false,
            file_read_pending: false,
            next_file_request: 0,
            schema_path_draft: None,
            xpath: XmlToolkitXPathViewState::default(),
            xpath_expression,
            xpath_bindings: Vec::new(),
            xpath_namespaces_open: false,
            xpath_scroll: gpui_kit::UniformListScrollHandle::default(),
            xpath_epoch: Arc::new(AtomicU64::new(1)),
            tree: XmlToolkitTreeViewState::default(),
            tree_scroll: gpui_kit::UniformListScrollHandle::default(),
            tree_epoch: Arc::new(AtomicU64::new(1)),
            tree_service_due: false,
            tree_copy_visibility: None,
            tree_copy_request: None,
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
            _subscriptions: vec![subscription],
            xpath_subscription: Some(xpath_subscription),
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
                        if let (Some(path), Engine::Active(session)) =
                            (view.schema_path_draft.clone(), &view.engine)
                        {
                            if let Err(error) = session.select_schema_path(path) {
                                view.feedback =
                                    Some(format!("Could not restore selected XSD: {error:?}"));
                            }
                        }
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
        self.xpath_epoch.fetch_add(1, Ordering::AcqRel);
        self.tree.revoke_copy_authority();
        self.tree_copy_visibility = None;
        self.tree_copy_request = None;
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
        self.xpath.sync(&XmlSessionSnapshot::default());
        self.xpath_subscription.take();
        self.xpath_bindings.clear();
        self.xpath_epoch.fetch_add(1, Ordering::AcqRel);
        self.tree_copy_visibility = None;
        self.tree_copy_request = None;
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
    pub(super) fn schema_picker_pending(&self) -> bool {
        self.schema_picker
    }
    pub(super) fn file_effect_pending(&self) -> bool {
        super::xml_schema::file_effect_pending(self.schema_picker, self.file_read_pending)
    }
    pub(super) fn schema_path_draft(&self) -> Option<&std::path::Path> {
        self.schema_path_draft.as_deref()
    }
    pub(super) fn draft(&self) -> Option<String> {
        (!self.source.is_empty()).then(|| self.source.clone())
    }
    fn reopen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            self.mount = self.owner.attach();
            self.mount_xpath_editors(window, cx);
            self.open = true;
            self.start(cx);
        }
        if self.open && matches!(self.engine, Engine::Idle) {
            self.start(cx);
        }
        if self.schema_tab {
            self.schema_focus.focus(window, cx);
        } else if self.xpath_tab {
            self.xpath_focus.focus(window, cx);
        } else if self.tree_tab {
            self.tree_focus.focus(window, cx);
        } else if self.output_tab {
            self.output_focus.focus(window, cx);
        } else {
            self.input.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if super::xml_schema::pane_close_blocked(self.schema_picker) {
            return;
        }
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
        self.open
            && !self.schema_picker
            && self.tree_tab
            && self.mount.is_current()
            && !self.admission.is_fenced()
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
        self.tree_copy_request = None;
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
        self.tree_copy_request = None;
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
        self.tree_copy_request = (intent == XmlDetailIntent::Copy).then_some(request);
        self.tree_service_due = true;
        if let Engine::Active(session) = &self.engine {
            if let Err(error) = session.request_detail(request) {
                self.tree_service_due = false;
                self.tree_copy_visibility = None;
                self.tree_copy_request = None;
                self.tree.revoke_copy_authority();
                self.feedback = Some(format!("Could not inspect XML node: {error:?}"));
            }
        }
        self.pump(cx);
        cx.notify();
    }
    fn service_tree(&mut self, snapshot: &XmlSessionSnapshot, cx: &mut Context<Self>) {
        self.tree.sync(snapshot);
        self.xpath.sync(snapshot);
        if !matches!(snapshot.detail, XmlDetailStatus::Pending(_)) {
            self.tree_service_due = false;
        }
        if (self.tree_live() || self.xpath_live()) && !self.clipboard_pending {
            if let Some(revision) = self.tree_copy_visibility {
                if !json_utility::native_host_current(
                    &self.visibility,
                    revision,
                    self.native,
                    false,
                ) {
                    self.tree_copy_visibility = None;
                    self.tree_copy_request = None;
                    self.tree.revoke_copy_authority();
                } else if let Some(output) = self.tree.take_valid_copy_completion(snapshot) {
                    self.tree_copy_visibility = None;
                    self.tree_copy_request = None;
                    self.clipboard(ClipboardIntent::Tree, Some((output, revision)), None, cx);
                }
            }
        }
    }
    fn service_current_tree(&mut self, cx: &mut Context<Self>) {
        if let Some(snapshot) = self.current_snapshot() {
            self.service_tree(&snapshot, cx);
        }
    }
    pub(super) fn clipboard_available(&self) -> bool {
        !self.clipboard_pending
    }
    pub(super) fn xpath_live(&self) -> bool {
        self.open
            && !self.schema_picker
            && self.xpath_tab
            && self.mount.is_current()
            && !self.admission.is_fenced()
    }
    fn invalidate_xpath(&mut self, cx: &mut Context<Self>) {
        self.tree_copy_visibility = None;
        self.tree_copy_request = None;
        if let (Engine::Active(session), Some(revision)) = (&self.engine, self.accepted_revision) {
            if let Err(error) = session.invalidate_xpath(revision) {
                self.feedback = Some(format!("Could not invalidate XPath: {error:?}"));
            }
        }
        self.xpath_scroll = gpui_kit::UniformListScrollHandle::default();
        cx.notify();
    }
    fn mount_xpath_editors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receipt = self.mount.clone();
        let expression =
            cx.new(|cx| InputState::new(window, cx).placeholder("//item[@id='example']"));
        expression.update(cx, |input, cx| {
            input.set_value(self.xpath.expression().to_owned(), window, cx)
        });
        let subscription = cx.subscribe(
            &expression,
            move |view: &mut Self,
                  input: Entity<InputState>,
                  event: &InputEvent,
                  cx: &mut Context<Self>| {
                if !receipt.is_current()
                    || !view.xpath_live()
                    || !matches!(event, InputEvent::Change)
                {
                    return;
                }
                let value = input.read(cx).value().to_string();
                view.xpath_epoch.fetch_add(1, Ordering::AcqRel);
                view.tree_epoch.fetch_add(1, Ordering::AcqRel);
                if view.xpath.edit_expression(value, &mut view.tree) {
                    view.invalidate_xpath(cx);
                    view.sync_xpath_editors(cx);
                }
            },
        );
        self.xpath_expression = expression;
        self.xpath_subscription = Some(subscription);
        self.xpath_bindings.clear();
        let rows: Vec<_> = self
            .xpath
            .namespace_rows()
            .map(|(id, prefix, uri)| (id, prefix.to_owned(), uri.to_owned()))
            .collect();
        for (id, prefix, uri) in rows {
            self.mount_xpath_binding(id, prefix, uri, window, cx);
        }
    }
    fn mount_xpath_binding(
        &mut self,
        id: u64,
        prefix_value: String,
        uri_value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prefix = cx.new(|cx| InputState::new(window, cx).placeholder("Prefix"));
        let uri = cx.new(|cx| InputState::new(window, cx).placeholder("Namespace URI"));
        prefix.update(cx, |input, cx| input.set_value(prefix_value, window, cx));
        uri.update(cx, |input, cx| input.set_value(uri_value, window, cx));
        let a_receipt = self.mount.clone();
        let b_receipt = self.mount.clone();
        let a = cx.subscribe(
            &prefix,
            move |view: &mut Self, _, event: &InputEvent, cx| {
                if a_receipt.is_current() && matches!(event, InputEvent::Change) {
                    view.xpath_namespace_changed(id, cx);
                }
            },
        );
        let b = cx.subscribe(&uri, move |view: &mut Self, _, event: &InputEvent, cx| {
            if b_receipt.is_current() && matches!(event, InputEvent::Change) {
                view.xpath_namespace_changed(id, cx);
            }
        });
        self.xpath_bindings.push(XPathBinding {
            id,
            prefix,
            uri,
            _subscriptions: vec![a, b],
        });
    }
    fn sync_xpath_editors(&mut self, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        let root = self.root_window;
        let receipt = self.mount.clone();
        cx.spawn(async move |_, cx| {
            let Some(entity) = weak.upgrade() else {
                return;
            };
            let _ = update_kit_window_content(root, &entity, cx, |view, window, cx| {
                if !view.open || !receipt.is_current() {
                    return;
                }
                let expression = view.xpath.expression().to_owned();
                if view.xpath_expression.read(cx).value() != expression {
                    view.xpath_expression
                        .update(cx, |input, cx| input.set_value(expression, window, cx));
                }
                let rows: Vec<_> = view
                    .xpath
                    .namespace_rows()
                    .map(|(id, prefix, uri)| (id, prefix.to_owned(), uri.to_owned()))
                    .collect();
                for (id, prefix, uri) in rows {
                    if let Some(row) = view.xpath_bindings.iter().find(|row| row.id == id) {
                        if row.prefix.read(cx).value() != prefix {
                            row.prefix
                                .update(cx, |input, cx| input.set_value(prefix, window, cx));
                        }
                        if row.uri.read(cx).value() != uri {
                            row.uri
                                .update(cx, |input, cx| input.set_value(uri, window, cx));
                        }
                    }
                }
            });
        })
        .detach();
    }
    fn xpath_namespace_changed(&mut self, id: u64, cx: &mut Context<Self>) {
        if !self.xpath_live() {
            return;
        }
        let Some(row) = self.xpath_bindings.iter().find(|row| row.id == id) else {
            return;
        };
        let prefix = row.prefix.read(cx).value().to_string();
        let uri = row.uri.read(cx).value().to_string();
        self.xpath_epoch.fetch_add(1, Ordering::AcqRel);
        self.tree_epoch.fetch_add(1, Ordering::AcqRel);
        if self.xpath.edit_namespace(id, prefix, uri, &mut self.tree) {
            self.invalidate_xpath(cx);
            self.sync_xpath_editors(cx);
        }
    }
    pub(super) fn xpath_add_namespace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.xpath_live() {
            return;
        }
        self.xpath_epoch.fetch_add(1, Ordering::AcqRel);
        self.tree_epoch.fetch_add(1, Ordering::AcqRel);
        let Some(id) = self.xpath.add_namespace(&mut self.tree) else {
            return;
        };
        self.mount_xpath_binding(id, String::new(), String::new(), window, cx);
        self.invalidate_xpath(cx);
        self.sync_xpath_editors(cx);
    }
    pub(super) fn xpath_remove_namespace(&mut self, id: u64, cx: &mut Context<Self>) {
        if !self.xpath_live() {
            return;
        }
        self.xpath_epoch.fetch_add(1, Ordering::AcqRel);
        self.tree_epoch.fetch_add(1, Ordering::AcqRel);
        if self.xpath.remove_namespace(id, &mut self.tree) {
            self.xpath_bindings.retain(|row| row.id != id);
            self.invalidate_xpath(cx);
        }
    }
    pub(super) fn xpath_run(&mut self, cx: &mut Context<Self>) {
        if !self.xpath_live() {
            return;
        }
        let Some(snapshot) = self.current_snapshot() else {
            return;
        };
        self.xpath_epoch.fetch_add(1, Ordering::AcqRel);
        self.tree_epoch.fetch_add(1, Ordering::AcqRel);
        self.tree_copy_visibility = None;
        self.tree_copy_request = None;
        self.xpath.invalidate_local_query(&mut self.tree);
        self.invalidate_xpath(cx);
        match self.xpath.run(&snapshot, &mut self.tree) {
            Ok(request) => {
                self.feedback = None;
                if let Engine::Active(session) = &self.engine {
                    if let Err(error) = session.request_xpath(request) {
                        self.feedback = Some(format!("Could not run XPath: {error:?}"));
                    }
                }
            }
            Err(error) => self.feedback = Some(error),
        }
        self.pump(cx);
        cx.notify();
    }
    pub(super) fn xpath_navigate(&mut self, index: usize, generation: u64, cx: &mut Context<Self>) {
        if !self.xpath_live() || self.xpath.generation() != generation {
            return;
        }
        let Some(snapshot) = self.current_snapshot() else {
            return;
        };
        self.tree_epoch.fetch_add(1, Ordering::AcqRel);
        self.tree_copy_visibility = None;
        self.tree_copy_request = None;
        if let Some(intents) = self.xpath.navigate(index, &snapshot, &mut self.tree) {
            if let (Some(detail), Engine::Active(session)) = (intents.detail, &self.engine) {
                self.tree_service_due = true;
                if let Err(error) = session.request_detail(detail) {
                    self.tree_service_due = false;
                    self.feedback = Some(format!("Could not inspect XPath result: {error:?}"));
                }
            }
        }
        if self.xpath.selected_result() == Some(index) {
            self.xpath_scroll
                .scroll_to_item(index, gpui_kit::ScrollStrategy::Nearest);
        }
        self.pump(cx);
        cx.notify();
    }
    pub(super) fn xpath_copy_node(
        &mut self,
        index: usize,
        kind: XmlNodeDetailKind,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        if !self.xpath_live() || self.xpath.generation() != generation {
            return;
        }
        let Some(snapshot) = self.current_snapshot() else {
            return;
        };
        let visibility = self.visibility.request_revision.load(Ordering::Acquire);
        if !json_utility::native_host_current(&self.visibility, visibility, self.native, false) {
            return;
        }
        self.tree_epoch.fetch_add(1, Ordering::AcqRel);
        let Some(intents) = self
            .xpath
            .request_result_copy(index, kind, &snapshot, &mut self.tree)
        else {
            return;
        };
        self.tree_copy_visibility = Some(visibility);
        self.tree_service_due = true;
        if let (Some(detail), Engine::Active(session)) = (intents.detail, &self.engine) {
            self.tree_copy_request = Some(detail);
            if let Err(error) = session.request_detail(detail) {
                self.tree_service_due = false;
                self.tree_copy_visibility = None;
                self.tree_copy_request = None;
                self.tree.revoke_copy_authority();
                self.feedback = Some(format!("Could not copy XPath result: {error:?}"));
            }
        }
        self.pump(cx);
        cx.notify();
    }
    pub(super) fn xpath_copy_status(&self, snapshot: &XmlSessionSnapshot) -> Option<String> {
        let request = self.tree_copy_request?;
        let visibility = self.tree_copy_visibility?;
        if !self.xpath_live()
            || !json_utility::native_host_current(&self.visibility, visibility, self.native, false)
        {
            return None;
        }
        let VisibleXPathStatus::Nodes(nodes) = self.xpath.visible_status(snapshot) else {
            return None;
        };
        let selected_node = self
            .xpath
            .selected_result()
            .and_then(|index| nodes.get(index))?;
        let selection = self.tree.selected()?;
        if selected_node.id != request.node
            || selection.node != request.node
            || selection.request != request.selection_request
        {
            return None;
        }
        match &snapshot.detail {
            XmlDetailStatus::Pending(current) if *current == request => {
                Some("Preparing complete value for explicit copy…".into())
            }
            XmlDetailStatus::Failed {
                request: current,
                error,
            } if *current == request => Some(format!(
                "Could not copy XPath result: {}",
                multi_launcher::xml_source::xml_error_message(error)
            )),
            _ => None,
        }
    }
    pub(super) fn xpath_copy_scalar(&mut self, generation: u64, cx: &mut Context<Self>) {
        if !self.xpath_live() || self.xpath.generation() != generation || self.clipboard_pending {
            return;
        }
        let Some(snapshot) = self.current_snapshot() else {
            return;
        };
        if self.xpath.authorize_scalar_copy(&snapshot).is_none() {
            return;
        }
        if let Some(text) = self.xpath.take_valid_scalar_copy_completion(&snapshot) {
            self.clipboard(ClipboardIntent::Scalar, None, Some(text), cx);
        }
    }
    pub(super) fn xpath_reveal(
        &mut self,
        generation: u64,
        index: usize,
        node: XmlNodeId,
        request: &XmlXPathQueryRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.xpath_live() || self.xpath.generation() != generation {
            return;
        }
        let Some(snapshot) = self.current_snapshot() else {
            return;
        };
        if !self
            .xpath
            .current_selected_node(&snapshot, index, node, request)
        {
            return;
        }
        self.xpath_epoch.fetch_add(1, Ordering::AcqRel);
        self.tree_epoch.fetch_add(1, Ordering::AcqRel);
        self.tree_copy_visibility = None;
        self.tree_copy_request = None;
        let Some(intents) = self.xpath.navigate(index, &snapshot, &mut self.tree) else {
            cx.notify();
            return;
        };
        if let (Some(detail), Engine::Active(session)) = (intents.detail, &self.engine) {
            self.tree_service_due = true;
            if let Err(error) = session.request_detail(detail) {
                self.tree_service_due = false;
                self.feedback = Some(format!("Could not inspect XPath result: {error:?}"));
            }
        }
        self.xpath_tab = false;
        self.tree_tab = true;
        if let Some(index) = self.tree.take_pending_scroll_row() {
            self.tree_scroll
                .scroll_to_item(index, gpui_kit::ScrollStrategy::Nearest);
        }
        self.tree_focus.focus(window, cx);
        self.pump(cx);
        cx.notify();
    }
    pub(super) fn schema_live(&self) -> bool {
        self.open
            && self.schema_tab
            && !self.file_effect_pending()
            && self.mount.is_current()
            && !self.admission.is_fenced()
    }
    pub(super) fn schema_choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.schema_live() {
            return;
        }
        self.begin_xml_picker(XmlPickerKind::Schema, window, cx);
    }
    fn open_xml_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_xml_picker(XmlPickerKind::Open, window, cx);
    }
    fn begin_xml_picker(
        &mut self,
        kind: XmlPickerKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open
            || !self.mount.is_current()
            || self.admission.is_fenced()
            || self.file_effect_pending()
            || self.clipboard_pending
            || window.window_handle().window_id() != self.root_window.window_id()
        {
            return;
        }
        let contexts = window.context_stack();
        if window.has_active_dialog(cx)
            || context_stack_has_primary(&contexts, "PopupMenu")
            || context_stack_has_primary(&contexts, "Dialog")
            || context_stack_has_primary(&contexts, "AlertDialog")
        {
            return;
        }
        let Some(snapshot) = self.current_snapshot() else {
            return;
        };
        let owner = match checked_active_dialog_owner(window) {
            Ok(Some(owner)) if Some(owner) == self.native => owner,
            _ => {
                self.feedback =
                    Some("Activate the XML Utility window before choosing a file.".into());
                cx.notify();
                return;
            }
        };
        let visibility_revision = self.visibility.request_revision.load(Ordering::Acquire);
        if !json_utility::native_host_current(
            &self.visibility,
            visibility_revision,
            Some(owner),
            false,
        ) {
            return;
        }
        let Some(permit) = self.admission.admit() else {
            return;
        };
        let mount = self.mount.clone();
        let source_epoch = self.epoch.load(Ordering::Acquire);
        let source_revision = snapshot.revision;
        let schema_revision = snapshot.schema.revision;
        let root_window = self.root_window;
        if matches!(kind, XmlPickerKind::Open) && !self.focus.contains_focused(window, cx) {
            self.focus.focus(window, cx);
        }
        self.schema_picker = true;
        let picker = rfd::FileDialog::new().set_parent(&*window);
        let picker = match kind {
            XmlPickerKind::Schema => picker.add_filter("XML Schema", &["xsd"]),
            XmlPickerKind::Open => picker.add_filter("XML document", &["xml", "xsl", "xslt"]),
        };
        let worker_mount = mount.clone();
        let worker_epoch = self.epoch.clone();
        let worker_visibility = self.visibility.clone();
        let worker_admission = self.admission.clone();
        let task = cx.background_executor().spawn(async move {
            if !super::xml_schema::picker_launch_current(
                &worker_mount,
                worker_epoch.load(Ordering::Acquire),
                source_epoch,
                !worker_admission.is_fenced(),
                current_foreground_identity() == Some(owner.identity)
                    && native_window_is_valid(owner)
                    && json_utility::native_host_current(
                        &worker_visibility,
                        visibility_revision,
                        Some(owner),
                        false,
                    ),
            ) {
                return None;
            }
            Some(picker.pick_file())
        });
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let mut permit = Some(permit);
            let result = task.await.flatten();
            let Some(view) = weak.upgrade() else {
                return;
            };
            let update = update_kit_window_content(root_window, &view, cx, |view, window, cx| {
                view.schema_picker = false;
                let snapshot = view.current_snapshot();
                let current = super::xml_schema::picker_receipt_current(
                    &mount,
                    view.open
                        && (!matches!(kind, XmlPickerKind::Schema) || view.schema_tab)
                        && !view.admission.is_fenced(),
                    view.epoch.load(Ordering::Acquire),
                    source_epoch,
                    snapshot.as_ref().is_some_and(|snapshot| {
                        snapshot.revision == source_revision
                            && (!matches!(kind, XmlPickerKind::Schema)
                                || snapshot.schema.revision == schema_revision)
                    }),
                    view.native == Some(owner)
                        && window.window_handle().window_id() == root_window.window_id()
                        && json_utility::native_host_current(
                            &view.visibility,
                            visibility_revision,
                            Some(owner),
                            false,
                        ),
                );
                if !current {
                    cx.notify();
                    return;
                }
                if let Some(path) = result {
                    if let Err(error) = multi_launcher::xml_source::validate_local_path(&path) {
                        view.feedback = Some(error);
                    } else {
                        match kind {
                            XmlPickerKind::Schema => {
                                if let Engine::Active(session) = &view.engine {
                                    match session.select_schema_path(path.clone()) {
                                        Ok(_) => {
                                            view.schema_path_draft = Some(path);
                                            view.feedback = None;
                                        }
                                        Err(error) => {
                                            view.feedback =
                                                Some(format!("Could not select XSD: {error:?}"))
                                        }
                                    }
                                }
                            }
                            XmlPickerKind::Open => {
                                if let Some(permit) = permit.take() {
                                    let receipt = XmlReadReceipt {
                                        mount: mount.clone(),
                                        source_epoch,
                                        source_revision,
                                        owner,
                                        visibility_revision,
                                        root_window,
                                        open_focus: window
                                            .focused(cx)
                                            .filter(|focus| view.focus.contains(focus, window)),
                                    };
                                    view.start_xml_load(path, receipt, permit, cx);
                                }
                            }
                        }
                    }
                }
                cx.notify();
            });
            if update.is_err() {
                let _ = view.update(cx, |view, cx| {
                    view.schema_picker = false;
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }
    fn start_xml_load(
        &mut self,
        path: std::path::PathBuf,
        receipt: XmlReadReceipt,
        permit: EffectPermit,
        cx: &mut Context<Self>,
    ) {
        if self.file_read_pending {
            return;
        }
        let Some(next) = self.next_file_request.checked_add(1) else {
            self.feedback = Some("XML file request identifiers are exhausted.".into());
            return;
        };
        self.next_file_request = next;
        let activity = self.activity.start(
            multi_launcher::activity::ActivityOwner::Xml,
            multi_launcher::activity::ActivitySlot::Primary,
            multi_launcher::activity::ActivityIncarnation::new(next),
            multi_launcher::activity::ActivityOperation::LoadXmlFile,
        );
        self.file_read_pending = true;
        self.feedback = Some("Loading selected XML file…".into());
        let task = cx.background_executor().spawn(async move {
            multi_launcher::xml_file_load::run_xml_load(&path, activity.as_ref())
        });
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let _permit = permit;
            let result = task.await;
            let Some(view) = weak.upgrade() else {
                return;
            };
            let update =
                update_kit_window_content(receipt.root_window, &view, cx, |view, window, cx| {
                    view.file_read_pending = false;
                    if view.feedback.as_deref() == Some("Loading selected XML file…") {
                        view.feedback = None;
                    }
                    let snapshot = view.current_snapshot();
                    let current = super::xml_schema::picker_receipt_current(
                        &receipt.mount,
                        view.open && !view.admission.is_fenced(),
                        view.epoch.load(Ordering::Acquire),
                        receipt.source_epoch,
                        snapshot
                            .as_ref()
                            .is_some_and(|snapshot| snapshot.revision == receipt.source_revision),
                        view.native == Some(receipt.owner)
                            && window.window_handle().window_id()
                                == receipt.root_window.window_id()
                            && json_utility::native_host_current(
                                &view.visibility,
                                receipt.visibility_revision,
                                Some(receipt.owner),
                                false,
                            ),
                    );
                    if !current {
                        cx.notify();
                        return;
                    }
                    match result {
                        Ok(source) => {
                            let focus_current = receipt.open_focus.as_ref().is_some_and(|focus| {
                                focus.is_focused(window) && view.focus.contains(focus, window)
                            });
                            let root_foreground = window.is_window_active()
                                && current_foreground_identity() == Some(receipt.owner.identity);
                            let may_focus = super::xml_schema::loaded_source_may_focus(
                                focus_current,
                                root_foreground,
                            );
                            view.epoch.fetch_add(1, Ordering::AcqRel);
                            view.xpath_epoch.fetch_add(1, Ordering::AcqRel);
                            view.source = source.clone();
                            view.feedback = Some("Loaded XML file.".into());
                            view.schema_tab = false;
                            view.xpath_tab = false;
                            view.tree_tab = false;
                            view.output_tab = false;
                            view.input
                                .update(cx, |input, cx| input.set_value(&source, window, cx));
                            view.submit(cx);
                            if may_focus {
                                view.input.focus_handle(cx).focus(window, cx);
                            }
                        }
                        Err(error) => view.feedback = Some(error),
                    }
                    cx.notify();
                });
            if update.is_err() {
                let _ = view.update(cx, |view, cx| {
                    view.file_read_pending = false;
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }
    pub(super) fn schema_clear(
        &mut self,
        source: XmlSourceRevision,
        schema: XmlSchemaRevision,
        cx: &mut Context<Self>,
    ) {
        if !self.schema_live() {
            return;
        }
        let Some(snapshot) = self.current_snapshot() else {
            return;
        };
        if snapshot.revision != Some(source)
            || snapshot.schema.revision != schema
            || snapshot.schema.selected_path.is_none()
        {
            return;
        }
        if let Engine::Active(session) = &self.engine {
            match session.clear_schema_path() {
                Ok(_) => {
                    self.schema_path_draft = None;
                    self.feedback = None;
                }
                Err(error) => self.feedback = Some(format!("Could not clear XSD: {error:?}")),
            }
        }
        cx.notify();
    }
    pub(super) fn schema_validate(
        &mut self,
        source: XmlSourceRevision,
        schema: XmlSchemaRevision,
        cx: &mut Context<Self>,
    ) {
        if !self.schema_live() {
            return;
        }
        let Some(snapshot) = self.current_snapshot() else {
            return;
        };
        if snapshot.revision != Some(source)
            || snapshot.schema.revision != schema
            || snapshot.schema.selected_path.is_none()
            || snapshot.parse != XmlParseStatus::Ready
            || !matches!(snapshot.tree, XmlTreeStatus::Ready(_))
        {
            return;
        }
        if let Engine::Active(session) = &self.engine {
            match session.validate_selected_schema() {
                Ok(_) => self.feedback = None,
                Err(error) => self.feedback = Some(format!("Could not validate XML: {error:?}")),
            }
        }
        self.pump(cx);
        cx.notify();
    }
    fn format(&mut self, mode: XmlFormatMode, cx: &mut Context<Self>) {
        if !self.open
            || self.schema_picker
            || !self.mount.is_current()
            || self.admission.is_fenced()
        {
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
        if !self.open
            || self.schema_picker
            || !self.mount.is_current()
            || self.admission.is_fenced()
        {
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
        scalar: Option<Arc<str>>,
        cx: &mut Context<Self>,
    ) {
        if !self.open || self.schema_picker || self.clipboard_pending {
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
        if matches!(intent, ClipboardIntent::Paste) && self.file_read_pending {
            self.epoch.fetch_add(1, Ordering::AcqRel);
        }
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
        let xpath_epoch = self.xpath_epoch.clone();
        let expected_xpath = xpath_epoch.load(Ordering::Acquire);
        let worker_xpath_epoch = xpath_epoch.clone();
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
                        && (!matches!(intent, ClipboardIntent::Scalar)
                            || output_receipt_current(
                                worker_xpath_epoch.load(Ordering::Acquire),
                                expected_xpath,
                            ))
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
                    .write_text(scalar.as_deref().unwrap_or_else(|| {
                        tree_output.as_ref().map_or_else(
                            || {
                                output
                                    .as_ref()
                                    .map_or(source.as_str(), |output| output.text.as_str())
                            },
                            |(output, _)| output.value.as_str(),
                        )
                    }))
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
                            && (!matches!(intent, ClipboardIntent::Scalar)
                                || output_receipt_current(
                                    xpath_epoch.load(Ordering::Acquire),
                                    expected_xpath,
                                ))
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
                            } else if matches!(intent, ClipboardIntent::Scalar) {
                                "Copied current XPath scalar text."
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
        let open_mount = self.mount.clone();
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
        let xpath_tab_mount = self.mount.clone();
        let schema_tab_mount = self.mount.clone();
        let source_epoch = self.epoch.load(Ordering::Acquire);
        let output_epoch = self.output_epoch.load(Ordering::Acquire);
        let snapshot = self.current_snapshot();
        let can_open_file = self.open
            && !self.file_effect_pending()
            && !self.clipboard_pending
            && !self.admission.is_fenced()
            && snapshot.is_some();
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
                                    && !view.schema_picker
                                {
                                    view.xpath_epoch.fetch_add(1, Ordering::AcqRel);
                                    view.schema_tab = false;
                                    view.xpath_tab = false;
                                    view.tree_epoch.fetch_add(1, Ordering::AcqRel);
                                    view.tree.revoke_copy_authority();
                                    view.tree_copy_visibility = None;
                                    view.tree_copy_request = None;
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
                                    && !view.schema_picker
                                {
                                    view.xpath_epoch.fetch_add(1, Ordering::AcqRel);
                                    view.schema_tab = false;
                                    view.xpath_tab = false;
                                    view.tree_epoch.fetch_add(1, Ordering::AcqRel);
                                    view.tree.revoke_copy_authority();
                                    view.tree_copy_visibility = None;
                                    view.tree_copy_request = None;
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
                        if tree_tab_mount.is_current()
                            && view.open
                            && !view.schema_picker
                            && !view.admission.is_fenced()
                        {
                            view.xpath_epoch.fetch_add(1, Ordering::AcqRel);
                            view.tree_epoch.fetch_add(1, Ordering::AcqRel);
                            view.tree.revoke_copy_authority();
                            view.tree_copy_visibility = None;
                            view.tree_copy_request = None;
                            view.schema_tab = false;
                            view.xpath_tab = false;
                            view.tree_tab = true;
                            view.output_tab = false;
                            view.tree_focus.focus(window, cx);
                            view.service_current_tree(cx);
                            cx.notify();
                        }
                    })),
            )
            .child(
                Button::new("xml-tab-xpath")
                    .label("XPath")
                    .track_focus(&self.xpath_focus)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if xpath_tab_mount.is_current()
                            && view.open
                            && !view.schema_picker
                            && !view.admission.is_fenced()
                        {
                            view.tree_epoch.fetch_add(1, Ordering::AcqRel);
                            view.tree.revoke_copy_authority();
                            view.tree_copy_visibility = None;
                            view.tree_copy_request = None;
                            view.schema_tab = false;
                            view.xpath_tab = true;
                            view.tree_tab = false;
                            view.output_tab = false;
                            view.xpath_expression.focus_handle(cx).focus(window, cx);
                            view.service_current_tree(cx);
                            cx.notify();
                        }
                    })),
            )
            .child(
                Button::new("xml-tab-validation")
                    .label("Validation")
                    .track_focus(&self.schema_focus)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if schema_tab_mount.is_current()
                            && view.open
                            && !view.schema_picker
                            && !view.admission.is_fenced()
                        {
                            view.xpath_epoch.fetch_add(1, Ordering::AcqRel);
                            view.tree_epoch.fetch_add(1, Ordering::AcqRel);
                            view.tree.revoke_copy_authority();
                            view.tree_copy_visibility = None;
                            view.tree_copy_request = None;
                            view.schema_tab = true;
                            view.xpath_tab = false;
                            view.tree_tab = false;
                            view.output_tab = false;
                            view.schema_focus.focus(window, cx);
                            cx.notify();
                        }
                    })),
            )
            .children(self.schema_tab.then(|| super::xml_schema::render(self, cx)))
            .children(self.xpath_tab.then(|| super::xml_xpath::render(self, cx)))
            .children(self.tree_tab.then(|| super::xml_tree::render(self, cx)))
            .when(
                !self.output_tab && !self.tree_tab && !self.xpath_tab && !self.schema_tab,
                |div| {
                    div.child(
                        Textarea::new(&self.input)
                            .aria_label("XML Source")
                            .tab_index(0),
                    )
                },
            )
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
                        Button::new("xml-open-file")
                            .label("Open XML…")
                            .disabled(!can_open_file)
                            .tab_stop(can_open_file)
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if open_mount.is_current() {
                                    view.open_xml_file(window, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("xml-paste")
                            .label("Paste")
                            .disabled(self.clipboard_pending)
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if paste_mount.is_current() {
                                    view.clipboard(ClipboardIntent::Paste, None, None, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("xml-copy")
                            .label("Copy Source")
                            .disabled(self.clipboard_pending)
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if copy_mount.is_current() {
                                    view.clipboard(ClipboardIntent::Source, None, None, cx);
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
                                    view.clipboard(ClipboardIntent::Output, None, None, cx);
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
                                    || view.schema_picker
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
                                if view.schema_tab {
                                    view.schema_focus.focus(window, cx);
                                } else if view.xpath_tab {
                                    view.xpath_expression.focus_handle(cx).focus(window, cx);
                                } else if view.tree_tab {
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
                            .disabled(self.schema_picker)
                            .tab_stop(!self.schema_picker)
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
    fn adding_namespace_reapplies_aggregate_byte_limit() {
        use multi_launcher::xml_toolkit_engine::MAX_XPATH_NAMESPACE_BYTES;
        use multi_launcher::xml_tree_model::XmlToolkitTreeViewState;
        use multi_launcher::xml_xpath_model::XmlToolkitXPathViewState;
        let mut tree = XmlToolkitTreeViewState::default();
        let mut xpath = XmlToolkitXPathViewState::default();
        let first = xpath.add_namespace(&mut tree).unwrap();
        assert!(xpath.edit_namespace(
            first,
            "p".into(),
            "x".repeat(MAX_XPATH_NAMESPACE_BYTES),
            &mut tree
        ));
        let before = xpath.namespace_rows().next().unwrap().2.len();
        xpath.add_namespace(&mut tree).unwrap();
        let rows: Vec<_> = xpath.namespace_rows().collect();
        assert_eq!(rows[0].2.len(), before - 10);
        assert!(
            rows.iter()
                .map(|(_, prefix, uri)| prefix.len() + uri.len() + 10)
                .sum::<usize>()
                <= MAX_XPATH_NAMESPACE_BYTES
        );
    }
    #[test]
    fn xpath_query_edits_revoke_the_production_generation() {
        use multi_launcher::xml_tree_model::XmlToolkitTreeViewState;
        use multi_launcher::xml_xpath_model::XmlToolkitXPathViewState;
        let mut tree = XmlToolkitTreeViewState::default();
        let mut xpath = XmlToolkitXPathViewState::default();
        let original = xpath.generation();
        assert!(xpath.edit_expression("//item".into(), &mut tree));
        assert_ne!(xpath.generation(), original);
        let expression_generation = xpath.generation();
        let row = xpath.add_namespace(&mut tree).unwrap();
        assert_ne!(xpath.generation(), expression_generation);
        let namespace_generation = xpath.generation();
        assert!(xpath.edit_namespace(row, "p".into(), "urn:items".into(), &mut tree));
        assert_ne!(xpath.generation(), namespace_generation);
    }
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
