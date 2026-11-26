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
fn effect_epochs_current(source: u64, expected: u64, tree: Option<(u64, u64)>) -> bool {
    source == expected && tree.is_none_or(|(current, expected)| current == expected)
}
fn mounted_effect_current(mount: &super::utility_mount::MountReceipt, source: u64, expected: u64, tree: Option<(u64, u64)>) -> bool {
    mount.is_current() && effect_epochs_current(source, expected, tree)
}

fn apply_paste_result(
    session: &mut JsonEditorState,
    initial_projection: &mut Option<(u64, u64)>,
    checking: &mut Option<u64>,
    initial_current: bool,
    result: Result<String, ClipboardError>,
) {
    if initial_projection.is_some() && !initial_current {
        session.cancel_initial_clipboard();
        *initial_projection = None;
        *checking = None;
    }
    if result.is_ok() {
        *initial_projection = None;
        *checking = None;
    }
    session.finish_reload(result);
}

pub(super) fn native_host_current(
    visibility: &RootVisibility,
    revision: u64,
    native: Option<NativeWindow>,
    popout: bool,
) -> bool {
    #[cfg(windows)]
    {
        native.is_some_and(|window| {
            if popout {
                use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::IsIconic};
                native_window_is_valid(window) && native_window_is_visible(window).unwrap_or(false)
                    && !unsafe { IsIconic(HWND(window.identity.hwnd as *mut _)) }.as_bool()
                    && current_foreground_identity() == Some(window.identity)
            } else { visibility.is_current_visible_revision(revision, window) }
        })
    }
    #[cfg(not(windows))]
    {
        let _ = (native, popout);
        visibility.is_current(revision, true)
    }
}

#[derive(Default)]
pub(super) struct ClipboardLane {
    pub(super) active: bool,
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
    tree_guard: Option<(Arc<AtomicU64>, u64)>,
    mount: super::utility_mount::MountReceipt,
    popout: bool,
}
impl ClipboardBackend for GuardedClipboard {
    fn read_text(&self) -> Result<String, ClipboardError> {
        ArboardClipboardBackend.read_text()
    }
    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        if !mounted_effect_current(&self.mount, self.epoch.load(Ordering::Acquire), self.expected,
                self.tree_guard.as_ref().map(|(epoch, expected)| (epoch.load(Ordering::Acquire), *expected)))
            || self.admission.is_fenced()
            || !native_host_current(&self.visibility, self.visibility_revision, self.native, self.popout)
        {
            return Err(ClipboardError::Permanent(
                "JSON editor changed before copy".into(),
            ));
        }
        ArboardClipboardBackend.write_text(text)
    }
}

pub(super) struct JsonMount {
    receipt: super::utility_mount::MountReceipt,
    root_window: AnyWindowHandle,
    pub(super) native: Option<NativeWindow>,
    input: Entity<TextareaState>,
    pub(super) tree_search: Entity<InputState>,
    pub(super) tree_scroll: gpui_kit::UniformListScrollHandle,
    focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    return_scroll: Option<Point<Pixels>>,
    escape_waiting: bool,
    _subscriptions: Vec<Subscription>,
}

pub(super) struct JsonWorkspace {
    pub(super) open: bool,
    pub(super) popout: bool,
    pub(super) rehosting: bool,
    pub(super) presentation_error: Option<String>,
    pub(super) geometry_error: Option<String>,
    open_popout_after_load: bool,
    activation_revision: Arc<AtomicU64>,
    pub(super) root: WeakEntity<RootWindow>,
    pub(super) admission: Arc<RestartAdmission>,
    pub(super) visibility: RootVisibility,
    mount_owner: super::utility_mount::MountOwner,
    mount: JsonMount,
    pub(super) tree_mode: bool,
    pub(super) session: JsonEditorState,
    intent: JsonUtilityIntent,
    pub(super) epoch: Arc<AtomicU64>,
    pub(super) tree_epoch: Arc<AtomicU64>,
    wake: Arc<HashWakeSignal>,
    pub(super) lane: ClipboardLane,
    initial_projection: Option<(u64, u64)>,
    checking: Option<u64>,
}
impl std::ops::Deref for JsonWorkspace {
    type Target = JsonMount;
    fn deref(&self) -> &Self::Target { &self.mount }
}
impl std::ops::DerefMut for JsonWorkspace {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.mount }
}
impl JsonMount {
    fn new(window: &mut Window, cx: &mut Context<JsonWorkspace>, owner: &super::utility_mount::MountOwner,
        root_window: AnyWindowHandle, native: Option<NativeWindow>, return_focus: Option<FocusHandle>, return_scroll: Point<Pixels>) -> Self {
        let receipt = owner.attach();
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(5, 12)
                .placeholder("Enter JSON")
        });
        let focus = cx.focus_handle();
        let tree_search = cx.new(|cx| InputState::new(window, cx).placeholder("Keys or values"));
        let search_receipt = receipt.clone();
        let search_subscription = cx.subscribe(&tree_search, move |this: &mut JsonWorkspace, input: Entity<InputState>, event: &InputEvent, cx: &mut Context<JsonWorkspace>| {
            if !search_receipt.is_current() || !matches!(event, InputEvent::Change) || !this.open || this.rehosting || !this.tree_mode || this.admission.is_fenced() { return; }
            this.tree_epoch.fetch_add(1, Ordering::AcqRel);
            let fields = this.session.tree_search_fields();
            this.session.tree_set_search(input.read(cx).value().to_string(), fields);
            this.service(cx);
        });
        let input_receipt = receipt.clone();
        let subscription = cx.subscribe(
            &input,
            move |this: &mut JsonWorkspace,
             input: Entity<TextareaState>,
             event: &InputEvent,
             cx: &mut Context<JsonWorkspace>| {
                if !input_receipt.is_current() || !matches!(event, InputEvent::Change) || !this.open || this.rehosting || this.admission.is_fenced()
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
        let focus_receipt = receipt.clone();
        let focus_subscription = cx.on_focus_out(&focus, window, move |this: &mut JsonWorkspace, _, _, _| { if focus_receipt.is_current() { this.escape_waiting = false; } });
        Self { receipt, root_window, native, input, tree_search, tree_scroll: gpui_kit::UniformListScrollHandle::default(),
            focus, return_focus, return_scroll: Some(return_scroll), escape_waiting: false,
            _subscriptions: vec![subscription, focus_subscription, search_subscription] }
    }
}
impl JsonWorkspace {
    pub(super) fn mount_receipt(&self) -> super::utility_mount::MountReceipt { self.mount.receipt.clone() }
    pub(super) fn is_mounted_in(&self, id: WindowId, popout: bool) -> bool {
        self.open && self.popout == popout && self.mount.receipt.is_current() && self.root_window.window_id() == id
    }
    pub(super) fn replace_mount(&mut self, root_window: AnyWindowHandle, native: Option<NativeWindow>, origin: Option<FocusHandle>, offset: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let mount = JsonMount::new(window, cx, &self.mount_owner, root_window, native, origin, offset);
        let text = self.session.source().to_owned();
        let query = self.session.tree_search_query().to_owned();
        mount.input.update(cx, |input, cx| input.set_value(text, window, cx));
        mount.tree_search.update(cx, |input, cx| input.set_value(query, window, cx));
        self.mount = mount;
    }
    pub(super) fn focus_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tree_mode { self.tree_search.focus_handle(cx).focus(window, cx); }
        else { self.input.focus_handle(cx).focus(window, cx); }
    }
    pub(super) fn can_rehost(&self) -> bool {
        self.open && !self.rehosting && !self.lane.active && self.checking.is_none() && !self.admission.is_fenced()
    }
    fn request_rehost(&mut self, popout: bool, cx: &mut Context<Self>) {
        if !self.can_rehost() || self.popout == popout { return; }
        let source = self.mount_receipt();
        let gesture = self.native.map(|source| hash_dock::Gesture {
                source,
                foreground_authorized: native_window_is_valid(source) && current_foreground_identity() == Some(source.identity),
                visibility_revision: self.visibility.request_revision.load(Ordering::Acquire),
            });
        self.rehosting = true;
        self.activation_revision.fetch_add(1, Ordering::AcqRel);
        let root = self.root.clone();
        cx.spawn(async move |_, cx| { let _ = root.update(cx, |root, cx| root.start_json_rehost(popout, gesture, source, cx)); }).detach();
        cx.notify();
    }
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
        let mount_owner = super::utility_mount::MountOwner::default();
        let mount = JsonMount::new(window, cx, &mount_owner, root_window, native, return_focus, return_scroll);
        let mut session = JsonEditorState::default();
        session.begin_initialize();
        Self { open: false, popout: false, rehosting: false, presentation_error: None, geometry_error: None, open_popout_after_load: false, activation_revision: Arc::new(AtomicU64::new(0)), root, admission, visibility, mount_owner, mount, tree_mode: false, session, intent,
            epoch: Arc::new(AtomicU64::new(1)), tree_epoch: Arc::new(AtomicU64::new(1)),
            wake: Arc::new(HashWakeSignal::default()), lane: ClipboardLane::default(),
            initial_projection: None, checking: None }
    }
    pub(super) fn service(&mut self, cx: &mut Context<Self>) {
        let before_tree = self.session.tree_stamp();
        if self.initial_projection.is_some_and(|(epoch, revision)| {
            !receipt_current(
                self.open,
                self.epoch.load(Ordering::Acquire),
                epoch,
                !self.admission.is_fenced()
                    && native_host_current(&self.visibility, revision, self.native, self.popout),
            )
        }) {
            self.tree_epoch.fetch_add(1, Ordering::AcqRel);
            self.initial_projection = None;
            self.checking = None;
            self.session.cancel_initial_clipboard();
        }
        if self.session.tree_search_status().2 {
            self.tree_epoch.fetch_add(1, Ordering::AcqRel);
        }
        let wake = self.wake.clone();
        self.session.service_projection(|| {
            let wake = wake.clone();
            Arc::new(move || wake.notify())
        });
        if before_tree != self.session.tree_stamp() {
            self.tree_epoch.fetch_add(1, Ordering::AcqRel);
        }
        if !self.session.loading_clipboard() && self.initial_projection.take().is_some() {
            self.checking = None;
        }
        if self.open_popout_after_load && self.checking.is_none() && !self.lane.active {
            self.open_popout_after_load = false;
            self.request_rehost(true, cx);
        }
        if let Some(index) = self.session.tree_take_reveal() {
            if self.mount.receipt.is_current() {
                self.tree_scroll.scroll_to_item(index, gpui_kit::ScrollStrategy::Nearest);
            }
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
            if !self.mount.receipt.is_current() {
                let root_window = self.root_window;
                let native = self.native;
                self.replace_mount(root_window, native, origin.clone(), offset, window, cx);
            }
            self.wake.close();
            self.session.close();
            self.session = JsonEditorState::default();
            self.wake = Arc::new(HashWakeSignal::default());
        }
        self.session.begin_initialize();
        self.open = true;
        self.intent = intent;
        self.tree_scroll = gpui_kit::UniformListScrollHandle::default();
        self.tree_search.update(cx, |input, cx| input.set_value("", window, cx));
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
        if self.tree_mode { self.tree_search.focus_handle(cx).focus(window, cx); }
        else { self.input.focus_handle(cx).focus(window, cx); }
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
                && native_host_current(&self.visibility, revision, self.native, self.popout),
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
        cx.spawn(async move |_, cx| {
            while wake.wait().await {
                let Some(view) = owner.upgrade() else {
                    break;
                };
                let root_window = cx.update(|app| view.read(app).root_window);
                let Ok(current) =
                    update_kit_window_content(root_window, &view, cx, |view, window, cx| {
                        if !view.open {
                            return false;
                        }
                        view.service(cx);
                        if view.mount.receipt.is_current() { view.sync_input(window, cx); }
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
        if !self.open || self.rehosting || self.lane.active {
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
        let mount = self.mount.receipt.clone();
        let task = cx.background_executor().spawn(async move {
            let _permit = permit;
            ArboardClipboardBackend.read_text()
        });
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let Some(view) = owner.upgrade() else {
                return;
            };
            let root_window = cx.update(|app| view.read(app).root_window);
            let _ = update_kit_window_content(root_window, &view, cx, |view, window, cx| {
                view.lane.active = false;
                if !receipt_current(
                    view.open,
                    view.epoch.load(Ordering::Acquire),
                    expected,
                    mount.is_current() && !view.admission.is_fenced()
                        && native_host_current(&view.visibility, visibility_revision, view.native, view.popout),
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
                                    && native_host_current(&view.visibility, revision, view.native, view.popout),
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
        if !self.open || self.rehosting || self.lane.active || self.checking.is_some() {
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
        let popout = self.popout;
        let admission = self.admission.clone();
        let mount = self.mount.receipt.clone();
        let worker_mount = mount.clone();
        let text = self.session.source().to_owned();
        let owner = cx.entity().downgrade();
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
                tree_guard: None,
                mount: worker_mount,
                popout,
            });
            session
        });
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let Some(view) = owner.upgrade() else {
                return;
            };
            let root_window = cx.update(|app| view.read(app).root_window);
            let _ = update_kit_window_content(root_window, &view, cx, |view, _, cx| {
                view.lane.active = false;
                if receipt_current(
                    view.open,
                    view.epoch.load(Ordering::Acquire),
                    expected,
                    mount.is_current() && !view.admission.is_fenced()
                        && native_host_current(&view.visibility, visibility_revision, view.native, view.popout),
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
    pub(super) fn copy_tree(&mut self, stamp: multi_launcher::json_utility_session::TreeStamp, kind: multi_launcher::json_utility_session::TreeCopyKind, cx: &mut Context<Self>) {
        if !self.open || self.rehosting || !self.tree_mode || self.lane.active || self.checking.is_some() { return; }
        let Some(request) = self.session.tree_copy_request(stamp, kind) else { return; };
        let Some(permit) = self.admission.admit() else { return; };
        self.lane.active = true;
        let expected = self.epoch.load(Ordering::Acquire);
        let expected_tree = self.tree_epoch.load(Ordering::Acquire);
        let visibility_revision = self.visibility.request_revision.load(Ordering::Acquire);
        let mount = self.mount.receipt.clone();
        let backend = GuardedClipboard { epoch: self.epoch.clone(), expected, visibility: self.visibility.clone(), visibility_revision, native: self.native, admission: self.admission.clone(), tree_guard: Some((self.tree_epoch.clone(), expected_tree)), mount: mount.clone(), popout: self.popout };
        let owner = cx.entity().downgrade();
        let task = cx.background_executor().spawn(async move { let _permit = permit; let result = request.execute(&backend); (request, result) });
        cx.spawn(async move |_, cx| {
            let (request, result) = task.await;
            let Some(view) = owner.upgrade() else { return; };
            let root_window = cx.update(|app| view.read(app).root_window);
            let _ = update_kit_window_content(root_window, &view, cx, |view, _, cx| {
                view.lane.active = false;
                if receipt_current(view.open && view.tree_mode, view.epoch.load(Ordering::Acquire), expected,
                    effect_epochs_current(view.epoch.load(Ordering::Acquire), expected, Some((view.tree_epoch.load(Ordering::Acquire), expected_tree)))
                    && mount.is_current() && !view.admission.is_fenced() && native_host_current(&view.visibility, visibility_revision, view.native, view.popout)) {
                    view.session.finish_tree_copy(&request, result);
                }
                view.drain_initial(cx); cx.notify();
            });
        }).detach();
        cx.notify();
    }
    fn transform(&mut self, transform: Transform, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || self.rehosting || self.checking.is_some() {
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
        if self.rehosting { return; }
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
        if self.tree_mode { self.tree_search.focus_handle(cx).focus(window, cx); }
        else { self.input.focus_handle(cx).focus(window, cx); }
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || self.rehosting {
            return;
        }
        let owned = window.is_window_active() && self.focus.contains_focused(window, cx);
        let popout = self.popout;
        let origin = self.return_focus.take();
        let offset = self.return_scroll.take();
        if popout { let _ = self.root.update(cx, |root, cx| root.drain_json_native_geometry(cx, true)); }
        self.shutdown();
        self.input.update(cx, |input, cx| input.set_value("", window, cx));
        self.tree_search.update(cx, |input, cx| input.set_value("", window, cx));
        self.tree_scroll = gpui_kit::UniformListScrollHandle::default();
        let _ = self.root.update(cx, |root, cx| {
            panel_stack::retire(&mut root.panel_stack, Panel::JsonUtilityDialog);
            if popout {
                root.drain_json_native_geometry(cx, true);
                if let Some(popup) = root.json_popup.take() {
                    let retiring = RetiringHashWindow { window: popup.window, native_window: popup.native };
                    root.json_retiring.push(retiring);
                    let root_weak = cx.entity().downgrade();
                    cx.spawn(async move |_, cx| {
                        let retired = remove_and_wait_for_hash_window(cx, retiring).await;
                        if retired { let _ = root_weak.update(cx, |root, _| root.json_retiring.retain(|item| item.window.window_id() != retiring.window.window_id())); }
                    }).detach();
                }
            } else if owned && !root.restart_admission.is_fenced() {
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
    pub(super) fn shutdown(&mut self) {
        self.open = false;
        self.activation_revision.fetch_add(1, Ordering::AcqRel);
        self.mount_owner.retire();
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.lane.invalidate();
        self.initial_projection = None;
        self.checking = None;
        self.wake.close();
        self.session.close();
        self.session = JsonEditorState::default();
        self.mount._subscriptions.clear();
        self.return_focus = None;
        self.return_scroll = None;
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
impl Drop for JsonWorkspace {
    fn drop(&mut self) {
        self.mount_owner.retire();
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.wake.close();
    }
}
impl Render for JsonWorkspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mount = self.mount_receipt();
        let escape_action = mount.clone();
        let escape_key = mount.clone();
        let escape_release = mount.clone();
        let paste_mount = mount.clone();
        let clear_mount = mount.clone();
        let copy_mount = mount.clone();
        let close_mount = mount.clone();
        let text_mount = mount.clone();
        let tree_mount = mount.clone();
        let rehost_mount = mount.clone();
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
                let mount = mount.clone();
                Button::new(SharedString::from(format!("json-{name}")))
                    .label(name)
                    .tab_stop(!checking)
                    .disabled(checking)
                    .on_click(
                        cx.listener(move |view, _, window, cx| { if mount.is_current() { view.transform(action, window, cx); } }),
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
        let tree_panel = self.tree_mode.then(|| super::json_tree::render(self, cx));
        div()
            .id("json-utility-pane")
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .track_focus(&self.focus)
            .capture_action(
                cx.listener(move |view, _: &gpui_kit::base::input::Escape, window, cx| {
                    if escape_action.is_current() && view.escape(window, cx, false) {
                        cx.stop_propagation();
                    }
                }),
            )
            .capture_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                if escape_key.is_current() && event.keystroke.key == "escape"
                    && event.keystroke.modifiers == Modifiers::default()
                    && view.escape(window, cx, event.is_held)
                {
                    cx.stop_propagation();
                }
            }))
            .capture_key_up(cx.listener(move |view, event: &gpui_kit::KeyUpEvent, _, _| {
                if escape_release.is_current() && event.keystroke.key == "escape" {
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
                    .child(Button::new("json-rehost").label(if self.popout { "Dock" } else { "Pop Out" })
                        .tab_stop(self.can_rehost()).disabled(!self.can_rehost())
                        .on_click(cx.listener(move |view, _, _, cx| { if rehost_mount.is_current() { view.request_rehost(!view.popout, cx); } })))
                    .child(
                        Button::new("json-paste")
                            .label("Paste")
                            .tab_stop(!self.lane.active)
                            .disabled(self.lane.active)
                            .on_click(cx.listener(move |view, _, _, cx| { if paste_mount.is_current() { view.read_clipboard(false, cx); } })),
                    )
                    .child(
                        Button::new("json-clear")
                            .label("Clear")
                            .tab_stop(true)
                            .on_click(cx.listener(move |view, _, window, cx| { if clear_mount.is_current() { view.clear(window, cx); } })),
                    )
                    .child(
                        Button::new("json-copy")
                            .label("Copy Result")
                            .tab_stop(!self.lane.active && !checking)
                            .disabled(self.lane.active || checking)
                            .on_click(cx.listener(move |view, _, _, cx| { if copy_mount.is_current() { view.copy(cx); } })),
                    )
                    .child(
                        Button::new("json-close")
                            .label("Close")
                            .tab_stop(true)
                            .on_click(cx.listener(move |view, _, window, cx| { if close_mount.is_current() { view.close(window, cx); } })),
                    ),
            )
            .child(div().flex().gap_2()
                .child(Button::new("json-view-text").label("Text").tab_stop(true).on_click(cx.listener(move |view, _, _, cx| { if !text_mount.is_current() { return; } if view.tree_mode { view.tree_epoch.fetch_add(1, Ordering::AcqRel); view.tree_mode = false; } cx.notify(); })))
                .child(Button::new("json-view-tree").label("Tree").tab_stop(true).on_click(cx.listener(move |view, _, _, cx| { if !tree_mount.is_current() { return; } if !view.tree_mode { view.tree_epoch.fetch_add(1, Ordering::AcqRel); view.tree_mode = true; } cx.notify(); }))))
            .when(!self.tree_mode, |this| this.child(Textarea::new(&self.input).aria_label("JSON text").tab_index(0).readonly(self.rehosting)))
            .children(tree_panel)
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
            .children(self.presentation_error.as_ref().map(|message| Label::new(message.clone())))
            .children(self.geometry_error.as_ref().map(|message| Label::new(message.clone())))
    }
}
impl RootWindow {
    pub(super) fn scroll_to_json(&self, cx: &App) {
        let index = self.file_search_scroll_item()
            + usize::from(self.file_search.as_ref().is_some_and(|view| view.read(cx).open))
            + usize::from(self.downloads_triage_open(cx))
            + usize::from(self.plugin_editor.as_ref().is_some_and(|view| view.read(cx).open))
            + usize::from(self.activity_view.as_ref().is_some_and(|view| view.read(cx).open));
        self.root_scroll.scroll_to_top_of_item(index);
    }
    pub(super) fn retire_json_workspace(&mut self, cx: &mut Context<Self>) {
        self.drain_json_native_geometry(cx, true);
        if let Some(view) = self.json_view.take() { view.update(cx, |view, _| view.shutdown()); }
    }
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
        if self.json_view.as_ref().is_some_and(|view| view.read(cx).rehosting) { return; }
        if let (Some(popup), Some(view), Some(root_native)) = (&self.json_popup, &self.json_view, self.native_window) {
            let Some(native) = popup.native else { return; };
            if current_foreground_identity() != Some(root_native.identity) || view.read(cx).rehosting { return; }
            let Some(permit) = self.restart_admission.admit() else { return; };
            let popup_window = popup.window;
            let view = view.clone();
            let receipt = view.read(cx).mount_receipt();
            let revision_guard = view.read(cx).activation_revision.clone();
            let revision = revision_guard.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
            let worker_guard = revision_guard.clone();
            cx.spawn(async move |_, cx| {
                let _permit = permit;
                cx.background_executor().spawn(async move {
                    activate_for_secondary_window(&worker_guard, revision, native, Some(root_native.identity));
                }).await;
                if !receipt.is_current() || revision_guard.load(Ordering::Acquire) != revision
                    || current_foreground_identity() != Some(native.identity) { return; }
                let _ = update_kit_window_content(popup_window, &view, cx, |view, window, cx| {
                    if receipt.is_current() && view.popout && !view.rehosting {
                        view.reopen(intent, None, point(px(0.0), px(0.0)), window, cx);
                    }
                });
            }).detach();
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
                JsonWorkspace::new(
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
        let should_popout = multi_launcher::settings::UtilityToolKey::parse("json-utility").ok()
            .is_some_and(|key| self.utility_presentations.preference(key) == Some(RememberedPresentation::Popout));
        view.update(cx, |view, cx| {
            view.reopen(intent, origin, offset, window, cx);
            view.open_popout_after_load = should_popout;
        });
        self.json_view = Some(view);
        panel_stack::promote(&mut self.panel_stack, Panel::JsonUtilityDialog);
        self.scroll_to_json(cx);
        cx.notify();
    }
}
#[cfg(test)]
mod tests {
    use super::{
        ClipboardLane, OpenHost, apply_paste_result, effect_epochs_current, mounted_effect_current, provider_is_current, receipt_current,
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
    fn tree_changes_do_not_invalidate_text_clipboard_but_revoke_tree_copy() {
        assert!(effect_epochs_current(4, 4, None));
        assert!(!effect_epochs_current(4, 4, Some((8, 7))));
        assert!(effect_epochs_current(4, 4, Some((8, 8))));
        assert!(!effect_epochs_current(5, 4, None));
    }

    #[test]
    fn mount_receipt_invalidation_does_not_change_session_or_lane() {
        use super::super::utility_mount::MountOwner;
        use multi_launcher::json_utility_session::JsonEditorState;
        let owner = MountOwner::default();
        let old = owner.attach();
        let mut session = JsonEditorState::default();
        session.set_source("{\"draft\":1}".into());
        let mut lane = ClipboardLane::default();
        lane.active = true;
        lane.queue_initial(4, 8);
        lane.queue_initial(5, 8);
        let current = owner.attach();
        assert!(!old.is_current());
        assert!(current.is_current());
        assert!(!mounted_effect_current(&old, 1, 1, None));
        assert!(mounted_effect_current(&current, 1, 1, None));
        assert_eq!(session.source(), "{\"draft\":1}");
        assert_eq!(lane.take_ready(), None);
        lane.active = false;
        assert_eq!(lane.take_ready(), Some((5, 8)));
        owner.retire();
        assert!(!current.is_current());
        assert!(!mounted_effect_current(&current, 1, 1, None));
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
        session.tree_set_search("needle".into(), multi_launcher::json_transform::JsonSearchFields::Keys);
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
        assert_eq!(session.tree_search_query(), "needle");
        assert_eq!(session.tree_search_fields(), multi_launcher::json_transform::JsonSearchFields::Keys);

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
