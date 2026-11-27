//! Retained, inactive MultiManager workspace authoring.
use super::*;
use multi_launcher::multi_manager::checked;
use multi_launcher::multi_manager::{
    bindings,
    model::{MmWorkspace, new_workspace_id},
    state,
    store::{self, CheckedWorkspaceSnapshot},
};

#[derive(Clone)]
struct PlacementOverlay {
    index: usize,
    revision: u64,
    expected: MmWorkspace,
    reconnect: checked::ReconnectedWorkspace,
}

fn overlay_current(
    overlay: &PlacementOverlay,
    index: usize,
    expected: &MmWorkspace,
    revision: u64,
) -> bool {
    overlay.index == index && overlay.revision == revision && &overlay.expected == expected
}

fn focus_out_revokes(started: bool, launcher_foreground: bool) -> bool {
    !started || launcher_foreground
}

pub(super) const WAIT: &str = "Save or discard the MultiManager workspace draft before closing.";

pub(super) fn current_provider(
    query: &str,
    action: &Action,
    enabled: Option<&HashSet<String>>,
    capabilities: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    enabled.is_none_or(|set| set.contains("MultiManager"))
        && multi_launcher::settings::provider_search_capability_enabled(
            capabilities,
            "MultiManager",
        )
        && action.args.is_none()
        && matches!(
            parse_action(action),
            Ok(Command::MultiManager(
                multi_launcher::commands::MultiManagerCommand::Open
            ))
        )
        && super::search_providers::search_routed(
            [&super::search_providers::MultiManagerBare as &dyn multi_launcher::plugin::Plugin],
            query,
            enabled,
            capabilities,
        )
        .into_iter()
        .any(|(name, row)| name == "MultiManager" && row == *action)
}

fn row_current(
    rows: &[MmWorkspace],
    index: usize,
    expected: &MmWorkspace,
    issued: u64,
    current: u64,
) -> bool {
    issued == current && rows.get(index) == Some(expected)
}

fn attempt_current(
    issued: &utility_mount::MountReceipt,
    current: &utility_mount::MountReceipt,
    attempt: u64,
    current_attempt: u64,
) -> bool {
    issued.is_current() && issued.matches(current) && attempt == current_attempt
}

fn discard_draft(rows: &mut Vec<MmWorkspace>, snapshot: Option<&CheckedWorkspaceSnapshot>) {
    *rows = snapshot.map_or_else(Vec::new, |snapshot| snapshot.workspaces().to_vec());
}

#[derive(Clone)]
enum Review {
    Delete {
        index: usize,
        expected: MmWorkspace,
        revision: u64,
    },
    Reload,
    Close,
}

pub(super) struct MultiManagerView {
    pub(super) open: bool,
    root: WeakEntity<RootWindow>,
    host: AnyWindowHandle,
    native: Option<NativeWindow>,
    visibility: RootVisibility,
    admission: Arc<RestartAdmission>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    focus: FocusHandle,
    review_cancel: FocusHandle,
    workspace_path: PathBuf,
    bindings_path: PathBuf,
    snapshot: Option<CheckedWorkspaceSnapshot>,
    rows: Vec<MmWorkspace>,
    bindings: Option<Vec<bindings::WorkspaceBindingSnapshot>>,
    binding_error: Option<String>,
    revision: u64,
    attempt: u64,
    pending: bool,
    rename: Option<(usize, MmWorkspace, u64)>,
    name: Entity<InputState>,
    _name_sub: Subscription,
    review: Option<Review>,
    review_epoch: u64,
    rename_epoch: u64,
    expanded: HashSet<usize>,
    message: Option<String>,
    focus_guard: Option<Subscription>,
    activation_guard: Option<Subscription>,
    operation_lease: Option<Arc<AtomicBool>>,
    overlay: Option<PlacementOverlay>,
}

impl MultiManagerView {
    pub(super) fn revoke_operation(&mut self) {
        if let Some(lease) = &self.operation_lease {
            lease.store(false, Ordering::Release);
        }
    }

    fn move_windows(
        &mut self,
        index: usize,
        expected: &MmWorkspace,
        revision: u64,
        destination: checked::Destination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending
            || self.dirty()
            || !self.current(window, cx)
            || !row_current(&self.rows, index, expected, revision, self.revision)
        {
            return;
        }
        let Some(overlay) = self
            .overlay
            .clone()
            .filter(|overlay| overlay_current(overlay, index, expected, revision))
        else {
            return;
        };
        let Some(snapshot) = self.snapshot.clone() else {
            return;
        };
        let Some(native) = self.native else {
            return;
        };
        let Some(permit) = self.admission.admit() else {
            return;
        };
        self.focus.focus(window, cx);
        self.pending = true;
        self.attempt = self.attempt.wrapping_add(1);
        let attempt = self.attempt;
        let lease = Arc::new(AtomicBool::new(true));
        self.operation_lease = Some(lease.clone());
        let started = Arc::new(AtomicBool::new(false));
        let blur = lease.clone();
        let blur_started = started.clone();
        self.focus_guard = Some(cx.on_focus_out(&self.focus, window, move |_, _, _, _| {
            if focus_out_revokes(
                blur_started.load(Ordering::Acquire),
                current_foreground_identity() == Some(native.identity),
            ) {
                blur.store(false, Ordering::Release);
            }
        }));
        let active = lease.clone();
        let active_started = started.clone();
        self.activation_guard = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() && !active_started.load(Ordering::Acquire) {
                active.store(false, Ordering::Release);
            }
        }));
        let receipt = self.mount.clone();
        let worker_receipt = receipt.clone();
        let visibility = self.visibility.clone();
        let host_revision = visibility.request_revision.load(Ordering::Acquire);
        let admission = self.admission.clone();
        let issuer = multi_launcher::window_catalog::WindowNativeIdentity {
            hwnd: native.identity.hwnd,
            pid: native.identity.process_id,
            tid: native.identity.thread_id,
        };
        let current: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
            worker_receipt.is_current()
                && !admission.is_fenced()
                && lease.load(Ordering::Acquire)
                && store::checked_workspace_source_is_current(&snapshot)
                && worker_receipt.is_current()
                && !admission.is_fenced()
                && lease.load(Ordering::Acquire)
                && json_utility::native_host_current(
                    &visibility,
                    host_revision,
                    Some(native),
                    false,
                )
        });
        let task = cx.background_executor().spawn(async move {
            let mut backend = checked::NativePlacement {
                effect_started: started,
            };
            let report = checked::place_checked(
                &overlay.reconnect,
                issuer,
                destination,
                current,
                &mut backend,
            );
            (permit, report)
        });
        cx.spawn(async move |weak, cx| {
            let (permit, report) = task.await;
            let _ = weak.update(cx, |view, cx| {
                if !attempt_current(&receipt, &view.mount, attempt, view.attempt) {
                    return;
                }
                view.pending = false;
                view.operation_lease = None;
                view.focus_guard = None;
                view.activation_guard = None;
                if report.error.is_some() {
                    view.overlay = None;
                }
                view.message = Some(match report.error {
                    Some(error) => format!(
                        "Moved {} window(s), skipped {}: {error}",
                        report.moved, report.skipped
                    ),
                    None => format!(
                        "Moved {} window(s), skipped {}.",
                        report.moved, report.skipped
                    ),
                });
                view.settle_wait(cx);
                cx.notify();
            });
            drop(permit);
        })
        .detach();
        cx.notify();
    }
    fn reconnect(
        &mut self,
        index: usize,
        expected: &MmWorkspace,
        revision: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending
            || self.dirty()
            || !self.current(window, cx)
            || !row_current(&self.rows, index, expected, revision, self.revision)
        {
            return;
        }
        let Some(snapshot) = self.snapshot.clone() else {
            return;
        };
        let Some(native) = self.native else {
            return;
        };
        let Some(permit) = self.admission.admit() else {
            return;
        };
        self.focus.focus(window, cx);
        self.overlay = None;
        self.pending = true;
        self.attempt = self.attempt.wrapping_add(1);
        let attempt = self.attempt;
        let lease = Arc::new(AtomicBool::new(true));
        self.operation_lease = Some(lease.clone());
        let blur = lease.clone();
        self.focus_guard = Some(cx.on_focus_out(&self.focus, window, move |_, _, _, _| {
            blur.store(false, Ordering::Release)
        }));
        let active = lease.clone();
        self.activation_guard = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() {
                active.store(false, Ordering::Release);
            }
        }));
        let receipt = self.mount.clone();
        let worker_receipt = receipt.clone();
        let visibility = self.visibility.clone();
        let host_revision = visibility.request_revision.load(Ordering::Acquire);
        let admission = self.admission.clone();
        let rows = self.rows.clone();
        let snapshot_worker = snapshot.clone();
        let issued_lease = lease.clone();
        let expected = expected.clone();
        let host = self.host;
        let task = cx.background_executor().spawn(async move {
            let result = checked::reconnect_checked(
                &rows,
                index,
                || {
                    worker_receipt.is_current()
                        && !admission.is_fenced()
                        && lease.load(Ordering::Acquire)
                        && store::checked_workspace_source_is_current(&snapshot_worker)
                        && worker_receipt.is_current()
                        && !admission.is_fenced()
                        && lease.load(Ordering::Acquire)
                        && json_utility::native_host_current(
                            &visibility,
                            host_revision,
                            Some(native),
                            false,
                        )
                        && current_foreground_identity() == Some(native.identity)
                },
                &checked::NativeReconnect,
            );
            (permit, result)
        });
        cx.spawn(async move |weak, cx| {
            let (permit, result) = task.await;
            let delivered = host.update(cx, |_, window, cx| {
                let _ = weak.update(cx, |view, cx| {
                    if !attempt_current(&receipt, &view.mount, attempt, view.attempt) {
                        return;
                    }
                    view.pending = false;
                    view.operation_lease = None;
                    view.focus_guard = None;
                    view.activation_guard = None;
                    if !row_current(&view.rows, index, &expected, revision, view.revision)
                        || view.dirty()
                        || !issued_lease.load(Ordering::Acquire)
                        || !json_utility::native_host_current(
                            &view.visibility,
                            host_revision,
                            Some(native),
                            false,
                        )
                        || !view.current(window, cx)
                    {
                        view.settle_wait(cx);
                        cx.notify();
                        return;
                    }
                    match result {
                        Ok(reconnect) => {
                            let count = reconnect.identities.len();
                            view.overlay = Some(PlacementOverlay {
                                index,
                                revision,
                                expected,
                                reconnect,
                            });
                            view.message = Some(format!("Reconnected {count} window(s)."));
                        }
                        Err(error) => {
                            view.message = Some(format!("Could not reconnect: {error:#}"))
                        }
                    }
                    view.settle_wait(cx);
                    cx.notify();
                });
            });
            if delivered.is_err() {
                let _ = weak.update(cx, |view, cx| {
                    if attempt_current(&receipt, &view.mount, attempt, view.attempt) {
                        view.pending = false;
                        view.operation_lease = None;
                        view.focus_guard = None;
                        view.activation_guard = None;
                        view.message = Some(
                            "Reconnect finished after the launcher became unavailable.".into(),
                        );
                        view.settle_wait(cx);
                        cx.notify();
                    }
                });
            }
            drop(permit);
        })
        .detach();
        cx.notify();
    }
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        root: WeakEntity<RootWindow>,
        native: Option<NativeWindow>,
        visibility: RootVisibility,
        admission: Arc<RestartAdmission>,
        settings: &multi_launcher::settings::MultiManagerSettings,
        settings_path: &std::path::Path,
    ) -> Self {
        let base = settings_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let workspace_path = state::resolve_relative_to(base, &settings.workspaces_path);
        let bindings_path = state::resolve_relative_to(base, &settings.bindings_path);
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Workspace name"));
        let name_sub = cx.subscribe(&name, |_: &mut Self, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let mut view = Self {
            open: true,
            root,
            host: window.window_handle(),
            native,
            visibility,
            admission,
            owner,
            mount,
            focus: cx.focus_handle(),
            review_cancel: cx.focus_handle(),
            workspace_path,
            bindings_path,
            snapshot: None,
            rows: Vec::new(),
            bindings: None,
            binding_error: None,
            revision: 0,
            attempt: 0,
            pending: false,
            rename: None,
            name,
            _name_sub: name_sub,
            review: None,
            review_epoch: 0,
            rename_epoch: 0,
            expanded: HashSet::new(),
            message: None,
            focus_guard: None,
            activation_guard: None,
            operation_lease: None,
            overlay: None,
        };
        view.reload(cx);
        view
    }
    fn dirty(&self) -> bool {
        self.snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.workspaces() != self.rows.as_slice())
    }
    pub(super) fn blocks_close(&self) -> bool {
        self.open
            && self.mount.is_current()
            && (self.pending || self.dirty() || self.rename.is_some() || self.review.is_some())
    }
    fn settle_wait(&self, cx: &mut Context<Self>) {
        let root = self.root.clone();
        let entity = cx.entity_id();
        cx.spawn(async move |_, cx| {
            let _ =
                root.update(cx, |root, cx| {
                    if root.multi_manager_view.as_ref().is_some_and(|view| {
                        view.entity_id() == entity && view.read(cx).blocks_close()
                    }) {
                        return;
                    }
                    if root.restart_status.as_deref() == Some(WAIT) {
                        root.restart_status = None;
                    }
                    if root.launcher_search_error.as_deref() == Some(WAIT) {
                        root.launcher_search_error = None;
                    }
                    cx.notify();
                });
        })
        .detach();
    }
    fn current(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.open
            && self.mount.is_current()
            && !self.admission.is_fenced()
            && self.host.window_id() == window.window_handle().window_id()
            && window.is_window_active()
            && !window.has_active_dialog(cx)
            && self.native.is_some_and(|native| {
                current_foreground_identity() == Some(native.identity)
                    && json_utility::native_host_current(
                        &self.visibility,
                        self.visibility.request_revision.load(Ordering::Acquire),
                        Some(native),
                        false,
                    )
            })
            && self.root.upgrade().is_some_and(|root| {
                root.read(cx)
                    .external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            })
    }
    fn reload(&mut self, cx: &mut Context<Self>) {
        if self.pending || !self.open {
            return;
        }
        self.overlay = None;
        self.pending = true;
        self.attempt = self.attempt.wrapping_add(1);
        let attempt = self.attempt;
        let receipt = self.mount.clone();
        let worker_receipt = receipt.clone();
        let path = self.workspace_path.clone();
        let binding_path = self.bindings_path.clone();
        let admission = self.admission.clone();
        let task = cx.background_executor().spawn(async move {
            let workspace = store::load_checked_workspaces(&path, || {
                worker_receipt.is_current() && !admission.is_fenced()
            });
            let bindings = bindings::load_bindings_if_exists(&binding_path)
                .map_err(|error| format!("{error:#}"));
            (workspace, bindings)
        });
        cx.spawn(async move |weak, cx| {
            let (workspace, bindings) = task.await;
            let _ = weak.update(cx, |view, cx| {
                if !attempt_current(&receipt, &view.mount, attempt, view.attempt) {
                    return;
                }
                view.pending = false;
                match workspace {
                    Ok(snapshot) => {
                        view.rows = snapshot.workspaces().to_vec();
                        view.snapshot = Some(snapshot);
                        view.revision = view.revision.wrapping_add(1);
                        view.rename = None;
                        view.review = None;
                        view.expanded.clear();
                        view.message = None;
                    }
                    Err(error) => {
                        view.message = Some(format!("Could not load workspaces: {error:#}"))
                    }
                }
                match bindings {
                    Ok(rows) => {
                        view.bindings = rows;
                        view.binding_error = None;
                    }
                    Err(error) => {
                        view.binding_error =
                            Some(format!("Could not load window bindings: {error}"))
                    }
                }
                view.settle_wait(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending
            || self.rename.is_some()
            || self.review.is_some()
            || !self.dirty()
            || !self.current(window, cx)
        {
            return;
        }
        let Some(snapshot) = self.snapshot.clone() else {
            return;
        };
        let Some(permit) = self.admission.admit() else {
            return;
        };
        self.overlay = None;
        self.focus.focus(window, cx);
        self.pending = true;
        self.attempt = self.attempt.wrapping_add(1);
        let attempt = self.attempt;
        let lease = Arc::new(AtomicBool::new(true));
        self.operation_lease = Some(lease.clone());
        let blur = lease.clone();
        self.focus_guard = Some(cx.on_focus_out(&self.focus, window, move |_, _, _, _| {
            blur.store(false, Ordering::Release)
        }));
        let active = lease.clone();
        self.activation_guard = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() {
                active.store(false, Ordering::Release);
            }
        }));
        let receipt = self.mount.clone();
        let worker_receipt = receipt.clone();
        let revision = self.revision;
        let visibility = self.visibility.clone();
        let native = self.native;
        let host_revision = visibility.request_revision.load(Ordering::Acquire);
        let admission = self.admission.clone();
        let rows = self.rows.clone();
        let task = cx.background_executor().spawn(async move {
            let result = store::save_checked_workspaces(&snapshot, &rows, || {
                worker_receipt.is_current()
                    && !admission.is_fenced()
                    && lease.load(Ordering::Acquire)
                    && json_utility::native_host_current(&visibility, host_revision, native, false)
                    && native.is_some_and(|native| {
                        current_foreground_identity() == Some(native.identity)
                    })
            });
            (permit, result)
        });
        cx.spawn(async move |weak, cx| {
            let (permit, result) = task.await;
            let _ = weak.update(cx, |view, cx| {
                if !attempt_current(&receipt, &view.mount, attempt, view.attempt)
                    || view.revision != revision
                {
                    return;
                }
                view.pending = false;
                view.operation_lease = None;
                view.focus_guard = None;
                view.activation_guard = None;
                match result {
                    Ok(snapshot) => {
                        view.rows = snapshot.workspaces().to_vec();
                        view.snapshot = Some(snapshot);
                        view.message = Some("Workspaces saved.".into());
                    }
                    Err(error) => {
                        view.message = Some(format!("Could not save workspaces: {error:#}"))
                    }
                }
                view.settle_wait(cx);
                cx.notify();
            });
            drop(permit);
        })
        .detach();
        cx.notify();
    }
    fn retire(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let owns = self.focus.contains_focused(window, cx);
        let focus = self.focus.clone();
        let root = self.root.clone();
        let entity = cx.entity_id();
        let native = self.native;
        let revision = self.visibility.request_revision.load(Ordering::Acquire);
        self.open = false;
        self.overlay = None;
        self.owner.retire();
        self.attempt = self.attempt.wrapping_add(1);
        self.pending = false;
        discard_draft(&mut self.rows, self.snapshot.as_ref());
        self.revision = self.revision.wrapping_add(1);
        self.expanded.clear();
        self.name
            .update(cx, |name, cx| name.set_value(String::new(), window, cx));
        self.rename = None;
        self.review = None;
        self.review_epoch = self.review_epoch.wrapping_add(1);
        self.rename_epoch = self.rename_epoch.wrapping_add(1);
        self.settle_wait(cx);
        if owns {
            window.defer(cx, move |window, app| {
                let _ =
                    root.update(app, |root, cx| {
                        let valid =
                            root.multi_manager_view.as_ref().is_some_and(|view| {
                                view.entity_id() == entity && !view.read(cx).open
                            }) && root.root_window.is_some_and(|host| {
                                host.window_id() == window.window_handle().window_id()
                            }) && native.is_some_and(|native| {
                                root.native_window == Some(native)
                                    && current_foreground_identity() == Some(native.identity)
                                    && json_utility::native_host_current(
                                        &root.root_visibility,
                                        revision,
                                        Some(native),
                                        false,
                                    )
                            }) && window.is_window_active()
                                && !window.has_active_dialog(cx)
                                && root.action_sheet.is_none()
                                && root.context_favorite_popup.is_none()
                                && !context_stack_has_primary(&window.context_stack(), "PopupMenu")
                                && window
                                    .focused(cx)
                                    .is_none_or(|current| focus.contains(&current, window));
                        if valid {
                            root.query.focus_handle(cx).focus(window, cx);
                            root.root_scroll
                                .scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
                        }
                        cx.notify();
                    });
            });
        }
        cx.notify();
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.current(window, cx)
            || self.pending
            || self.review.is_some()
            || self.rename.is_some()
        {
            return;
        }
        if self.dirty() || self.rename.is_some() {
            self.review_epoch = self.review_epoch.wrapping_add(1);
            self.review = Some(Review::Close);
            self.review_cancel.focus(window, cx);
            cx.notify();
        } else {
            self.retire(window, cx);
        }
    }
    pub(super) fn handle_escape(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        held: bool,
    ) -> bool {
        if !self.focus.contains_focused(window, cx) || !self.current(window, cx) {
            return false;
        }
        if !held && !self.pending {
            if self.review.take().is_some() {
                self.review_epoch = self.review_epoch.wrapping_add(1);
                self.settle_wait(cx);
                self.focus.focus(window, cx);
                cx.notify();
            } else if self.rename.take().is_some() {
                self.rename_epoch = self.rename_epoch.wrapping_add(1);
                self.settle_wait(cx);
                self.focus.focus(window, cx);
                cx.notify();
            } else {
                self.close(window, cx);
            }
        }
        true
    }
}

impl Drop for MultiManagerView {
    fn drop(&mut self) {
        self.owner.retire();
    }
}

#[derive(Clone, Copy)]
enum RowAction {
    Rename,
    Toggle,
    Up,
    Down,
    Delete,
    Expand,
    Reconnect,
    Home,
    Target,
}

impl MultiManagerView {
    fn row_action(
        &mut self,
        action: RowAction,
        index: usize,
        expected: &MmWorkspace,
        revision: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending
            || self.review.is_some()
            || self.rename.is_some()
            || !self.current(window, cx)
            || !row_current(&self.rows, index, expected, revision, self.revision)
        {
            return;
        }
        match action {
            RowAction::Rename => {
                self.overlay = None;
                self.rename_epoch = self.rename_epoch.wrapping_add(1);
                self.name.update(cx, |name, cx| {
                    name.set_value(expected.name.clone(), window, cx)
                });
                self.rename = Some((index, expected.clone(), revision));
                self.name.focus_handle(cx).focus(window, cx);
            }
            RowAction::Toggle => {
                self.overlay = None;
                self.rows[index].disabled = !self.rows[index].disabled;
                self.revision = self.revision.wrapping_add(1);
            }
            RowAction::Up if index > 0 => {
                self.overlay = None;
                self.rows.swap(index, index - 1);
                self.revision = self.revision.wrapping_add(1);
                self.expanded.clear();
            }
            RowAction::Down if index + 1 < self.rows.len() => {
                self.overlay = None;
                self.rows.swap(index, index + 1);
                self.revision = self.revision.wrapping_add(1);
                self.expanded.clear();
            }
            RowAction::Delete => {
                self.review_epoch = self.review_epoch.wrapping_add(1);
                self.review = Some(Review::Delete {
                    index,
                    expected: expected.clone(),
                    revision,
                });
                self.review_cancel.focus(window, cx);
            }
            RowAction::Expand => {
                if !self.expanded.remove(&index) {
                    self.expanded.insert(index);
                }
            }
            RowAction::Reconnect => self.reconnect(index, expected, revision, window, cx),
            RowAction::Home => self.move_windows(
                index,
                expected,
                revision,
                checked::Destination::Home,
                window,
                cx,
            ),
            RowAction::Target => self.move_windows(
                index,
                expected,
                revision,
                checked::Destination::Target,
                window,
                cx,
            ),
            _ => {}
        }
        cx.notify();
    }
}

impl Render for MultiManagerView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div()
            .id("multi-manager-pane")
            .key_context("MultiManagerPane")
            .track_focus(&self.focus)
            .capture_action(
                cx.listener(|view, _: &gpui_kit::base::input::Escape, window, cx| {
                    if view.focus.contains_focused(window, cx) {
                        let root = view.root.clone();
                        let latched = root
                            .update(cx, |root, _| {
                                let was = root.pane_escape_latched;
                                root.pane_escape_latched = true;
                                was
                            })
                            .unwrap_or(true);
                        if !latched {
                            view.handle_escape(window, cx, false);
                        }
                        cx.stop_propagation();
                    }
                }),
            )
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .p_3()
            .child(Label::new("MultiManager"));
        if let Some(message) = &self.message {
            panel = panel.child(Label::new(message.clone()));
        }
        if let Some(error) = &self.binding_error {
            panel = panel.child(Label::new(error.clone()));
        }
        if self.pending {
            panel = panel.child(Label::new("Working…"));
        }
        if let Some(review) = self.review.clone() {
            let review_epoch = self.review_epoch;
            let label = match &review {
                Review::Delete { expected, .. } => format!("Delete workspace ‘{}’?", expected.name),
                Review::Reload => "Discard the draft and reload?".into(),
                Review::Close => "Discard the draft and close?".into(),
            };
            let confirm_label = if matches!(&review, Review::Delete { .. }) {
                "Delete"
            } else {
                "Discard"
            };
            panel = panel.child(Label::new(label)).child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("mm-review-cancel")
                            .label("Cancel")
                            .track_focus(&self.review_cancel)
                            .disabled(self.pending)
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if view.review_epoch == review_epoch
                                    && view.review.is_some()
                                    && view.current(window, cx)
                                    && !view.pending
                                {
                                    view.review = None;
                                    view.review_epoch = view.review_epoch.wrapping_add(1);
                                    view.settle_wait(cx);
                                    view.focus.focus(window, cx);
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Button::new("mm-review-confirm")
                            .label(confirm_label)
                            .disabled(self.pending)
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if view.review_epoch != review_epoch
                                    || !view.current(window, cx)
                                    || view.pending
                                {
                                    return;
                                }
                                match (&review, &view.review) {
                                    (
                                        Review::Delete {
                                            index,
                                            expected,
                                            revision,
                                        },
                                        Some(Review::Delete {
                                            index: current,
                                            expected: current_note,
                                            revision: current_rev,
                                        }),
                                    ) if index == current
                                        && expected == current_note
                                        && revision == current_rev
                                        && row_current(
                                            &view.rows,
                                            *index,
                                            expected,
                                            *revision,
                                            view.revision,
                                        ) =>
                                    {
                                        view.rows.remove(*index);
                                        view.overlay = None;
                                        view.revision = view.revision.wrapping_add(1);
                                        view.expanded.clear();
                                        view.review = None;
                                        if view.focus.contains_focused(window, cx) {
                                            view.focus.focus(window, cx);
                                        }
                                    }
                                    (Review::Reload, Some(Review::Reload)) => {
                                        view.review = None;
                                        view.review_epoch = view.review_epoch.wrapping_add(1);
                                        if view.focus.contains_focused(window, cx) {
                                            view.focus.focus(window, cx);
                                        }
                                        view.reload(cx);
                                    }
                                    (Review::Close, Some(Review::Close)) => view.retire(window, cx),
                                    _ => {}
                                }
                                cx.notify();
                            })),
                    ),
            );
            return panel;
        }
        if let Some((index, expected, revision)) = self.rename.clone() {
            let rename_epoch = self.rename_epoch;
            panel = panel
                .child(Label::new(format!("Rename {}", expected.name)))
                .child(Input::new(&self.name).aria_label("Workspace name"))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("mm-rename-apply")
                                .label("Apply")
                                .disabled(self.pending)
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if view.pending
                                        || view.rename_epoch != rename_epoch
                                        || !view.current(window, cx)
                                        || !row_current(
                                            &view.rows,
                                            index,
                                            &expected,
                                            revision,
                                            view.revision,
                                        )
                                        || !view.rename.as_ref().is_some_and(|(i, row, rev)| {
                                            *i == index && row == &expected && *rev == revision
                                        })
                                    {
                                        return;
                                    }
                                    let name = view.name.read(cx).value().trim().to_owned();
                                    if name.is_empty() {
                                        view.message = Some("Enter a workspace name.".into());
                                    } else {
                                        view.rows[index].name = name;
                                        view.revision = view.revision.wrapping_add(1);
                                        view.rename = None;
                                        view.rename_epoch = view.rename_epoch.wrapping_add(1);
                                        view.message = None;
                                        view.focus.focus(window, cx);
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("mm-rename-cancel")
                                .label("Cancel")
                                .disabled(self.pending)
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if view.rename_epoch == rename_epoch
                                        && view.rename.is_some()
                                        && view.current(window, cx)
                                        && !view.pending
                                    {
                                        view.rename = None;
                                        view.rename_epoch = view.rename_epoch.wrapping_add(1);
                                        view.settle_wait(cx);
                                        view.focus.focus(window, cx);
                                        cx.notify();
                                    }
                                })),
                        ),
                );
            return panel;
        }
        let issued_revision = self.revision;
        let mount = self.mount.clone();
        panel = panel.child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(
                    Button::new("mm-add")
                        .label("Add workspace")
                        .disabled(self.pending || self.snapshot.is_none())
                        .on_click({
                            let mount = mount.clone();
                            cx.listener(move |view, _, window, cx| {
                                if mount.is_current()
                                    && mount.matches(&view.mount)
                                    && view.revision == issued_revision
                                    && !view.pending
                                    && view.review.is_none()
                                    && view.rename.is_none()
                                    && view.snapshot.is_some()
                                    && view.current(window, cx)
                                {
                                    view.rows.push(MmWorkspace {
                                        id: new_workspace_id(),
                                        name: "New Workspace".into(),
                                        ..Default::default()
                                    });
                                    view.overlay = None;
                                    view.revision = view.revision.wrapping_add(1);
                                    cx.notify();
                                }
                            })
                        }),
                )
                .child(
                    Button::new("mm-save")
                        .label("Save")
                        .disabled(self.pending || !self.dirty())
                        .on_click({
                            let mount = mount.clone();
                            cx.listener(move |view, _, window, cx| {
                                if mount.is_current()
                                    && mount.matches(&view.mount)
                                    && view.revision == issued_revision
                                {
                                    view.save(window, cx);
                                }
                            })
                        }),
                )
                .child(
                    Button::new("mm-reload")
                        .label("Reload")
                        .disabled(self.pending)
                        .on_click({
                            let mount = mount.clone();
                            cx.listener(move |view, _, window, cx| {
                                if !mount.is_current()
                                    || !mount.matches(&view.mount)
                                    || view.revision != issued_revision
                                    || view.pending
                                    || view.review.is_some()
                                    || view.rename.is_some()
                                    || !view.current(window, cx)
                                {
                                    return;
                                }
                                if view.dirty() {
                                    view.review_epoch = view.review_epoch.wrapping_add(1);
                                    view.review = Some(Review::Reload);
                                    view.review_cancel.focus(window, cx);
                                    cx.notify();
                                } else {
                                    view.reload(cx);
                                }
                            })
                        }),
                )
                .child(
                    Button::new("mm-close")
                        .label("Close")
                        .disabled(self.pending)
                        .on_click({
                            let mount = mount.clone();
                            cx.listener(move |view, _, window, cx| {
                                if mount.is_current()
                                    && mount.matches(&view.mount)
                                    && view.revision == issued_revision
                                {
                                    view.close(window, cx);
                                }
                            })
                        }),
                ),
        );
        if self.rows.is_empty() && !self.pending {
            panel = panel.child(Label::new("No workspaces."));
        }
        let mut cards = div()
            .id("mm-workspaces")
            .flex()
            .flex_col()
            .gap_2()
            .max_h(px(480.0))
            .overflow_y_scroll();
        for (index, row) in self.rows.iter().enumerate() {
            let mut card = div()
                .id(SharedString::from(format!("mm-card-{index}")))
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .child(Label::new(format!(
                    "{}{} · {} windows",
                    row.name,
                    if row.disabled { " (disabled)" } else { "" },
                    row.windows.len()
                )));
            let mut controls = div().flex().flex_wrap().gap_1();
            for (label, action) in [
                ("Details", RowAction::Expand),
                ("Reconnect", RowAction::Reconnect),
                ("Move to Home", RowAction::Home),
                ("Move to Target", RowAction::Target),
                ("Rename", RowAction::Rename),
                (
                    if row.disabled { "Enable" } else { "Disable" },
                    RowAction::Toggle,
                ),
                ("Up", RowAction::Up),
                ("Down", RowAction::Down),
                ("Delete", RowAction::Delete),
            ] {
                let expected = row.clone();
                let receipt = mount.clone();
                let overlay_ready = self.overlay.as_ref().is_some_and(|overlay| {
                    overlay.index == index
                        && overlay.revision == issued_revision
                        && overlay.expected == *row
                        && !overlay.reconnect.identities.is_empty()
                });
                let unavailable = self.pending
                    || match action {
                        RowAction::Reconnect => {
                            self.dirty() || self.snapshot.is_none() || row.disabled || !row.valid
                        }
                        RowAction::Home | RowAction::Target => self.dirty() || !overlay_ready,
                        _ => false,
                    };
                controls = controls.child(
                    Button::new(SharedString::from(format!("mm-{index}-{label}")))
                        .label(label)
                        .disabled(unavailable)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if receipt.is_current() && receipt.matches(&view.mount) {
                                view.row_action(
                                    action,
                                    index,
                                    &expected,
                                    issued_revision,
                                    window,
                                    cx,
                                );
                            }
                        })),
                );
            }
            card = card.child(controls);
            if let Some(overlay) = self.overlay.as_ref().filter(|overlay| {
                overlay.index == index
                    && overlay.revision == self.revision
                    && overlay.expected == *row
            }) {
                card = card.child(Label::new(format!(
                    "{} of {} windows reconnected",
                    overlay.reconnect.identities.len(),
                    row.windows.len()
                )));
            }
            if self.expanded.contains(&index) {
                card = card.child(Label::new(format!(
                    "Workspace ID: {} · Home: {:?} · Target: {:?}",
                    row.id, row.home_rect, row.target_rect
                )));
                if let Some(hotkey) = &row.hotkey {
                    card = card.child(Label::new(format!(
                        "Hotkey: {}",
                        hotkey.sequence().unwrap_or_default()
                    )));
                }
                for (window_index, entry) in row.windows.iter().enumerate() {
                    card = card.child(Label::new(format!(
                        "{}: {} · {} · home {:?} · target {:?}",
                        window_index + 1,
                        entry.display_label(),
                        entry.executable,
                        entry.home_rect,
                        entry.target_rect
                    )));
                }
                if let Some(bindings) = &self.bindings {
                    for binding in bindings.iter().filter(|binding| {
                        multi_launcher::multi_manager::bindings::workspace_index_for_snapshot(
                            &self.rows, binding,
                        ) == Some(index)
                    }) {
                        card = card.child(Label::new(format!(
                            "Saved binding snapshot: {} window(s)",
                            binding.windows.len()
                        )));
                        for saved in &binding.windows {
                            card = card.child(Label::new(format!(
                                "{} · HWND {} · {}",
                                saved.alias.as_deref().unwrap_or(&saved.captured_title),
                                saved.hwnd,
                                saved.executable.as_deref().unwrap_or("unknown executable")
                            )));
                        }
                    }
                }
            }
            cards = cards.child(card);
        }
        panel.child(cards)
    }
}

impl RootWindow {
    pub(super) fn open_multi_manager(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced()
            || window.has_active_dialog(cx)
            || !window.is_window_active()
            || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            || !self
                .native_window
                .is_some_and(|native| current_foreground_identity() == Some(native.identity))
        {
            return;
        }
        let view = if let Some(view) = &self.multi_manager_view {
            let view = view.clone();
            view.update(cx, |view, cx| {
                if !view.open {
                    view.open = true;
                    view.mount = view.owner.attach();
                    view.reload(cx);
                }
                if view.review.is_some() {
                    view.review_cancel.focus(window, cx);
                } else if view.rename.is_some() {
                    view.name.focus_handle(cx).focus(window, cx);
                } else {
                    view.focus.focus(window, cx);
                }
            });
            view
        } else {
            let root = cx.entity().downgrade();
            let native = self.native_window;
            let visibility = self.root_visibility.clone();
            let admission = self.restart_admission.clone();
            let settings = self.search_catalog.multi_manager_settings.clone();
            let settings_path = self.search_catalog.multi_manager_settings_path.clone();
            let view = cx.new(|cx| {
                MultiManagerView::new(
                    window,
                    cx,
                    root,
                    native,
                    visibility,
                    admission,
                    &settings,
                    &settings_path,
                )
            });
            view.update(cx, |view, cx| view.focus.focus(window, cx));
            self.multi_manager_view = Some(view.clone());
            view
        };
        let id = window.window_handle().window_id();
        let history = self
            .clipboard_history
            .as_ref()
            .is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let modify = self
            .clipboard_modify_view
            .as_ref()
            .is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let snippets = self
            .snippets_view
            .as_ref()
            .is_some_and(|view| view.read(cx).open);
        let fill = self
            .snippet_fill
            .as_ref()
            .is_some_and(|view| view.read(cx).open);
        let tempfile = self
            .tempfile_view
            .as_ref()
            .is_some_and(|view| view.read(cx).open);
        let folder = self
            .folder_size_view
            .as_ref()
            .is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let lock = self
            .file_lock_view
            .as_ref()
            .is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let duplicates = self
            .duplicate_finder_view
            .as_ref()
            .is_some_and(|view| view.read(cx).open);
        let notes = self
            .quick_notes_view
            .as_ref()
            .is_some_and(|view| view.read(cx).open);
        let editor = self
            .note_editor_view
            .as_ref()
            .is_some_and(|view| view.read(cx).open);
        let _ = view;
        self.root_scroll.scroll_to_top_of_item(
            self.clipboard_scroll_item(id, cx)
                + usize::from(history)
                + usize::from(modify)
                + usize::from(snippets)
                + usize::from(fill)
                + usize::from(tempfile)
                + usize::from(folder)
                + usize::from(lock)
                + usize::from(duplicates)
                + usize::from(notes)
                + usize::from(editor),
        );
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PlacementOverlay, attempt_current, current_provider, discard_draft, focus_out_revokes,
        overlay_current, row_current,
    };
    use multi_launcher::multi_manager::model::MmWorkspace;
    use multi_launcher::plugin::Plugin;
    use std::collections::HashSet;
    #[test]
    fn stock_bare_provider_rejects_forged_disabled_and_typed_actions() {
        let stock = multi_launcher::plugins::multi_manager::MultiManagerPlugin.search("mm");
        let action = stock
            .into_iter()
            .find(|row| row.action == "mm:open")
            .unwrap();
        assert!(current_provider("MM", &action, None, None));
        assert!(!current_provider("mm settings", &action, None, None));
        assert!(!current_provider(
            "mm",
            &action,
            Some(&HashSet::new()),
            None
        ));
        let mut forged = action;
        forged.args = Some("other".into());
        assert!(!current_provider("mm", &forged, None, None));
    }
    #[test]
    fn duplicate_id_rows_require_exact_ordinal_revision_and_record() {
        let a = MmWorkspace {
            id: "same".into(),
            name: "A".into(),
            ..Default::default()
        };
        let b = MmWorkspace {
            id: "same".into(),
            name: "B".into(),
            ..Default::default()
        };
        let rows = vec![a.clone(), b.clone()];
        assert!(row_current(&rows, 0, &a, 2, 2));
        assert!(!row_current(&rows, 1, &a, 2, 2));
        assert!(!row_current(&rows, 0, &a, 2, 3));
        assert!(!row_current(&[b, a.clone()], 0, &a, 2, 2));
    }
    #[test]
    fn stale_acknowledgement_cannot_apply_to_new_mount_or_attempt() {
        let owner = super::super::utility_mount::MountOwner::default();
        let issued = owner.attach();
        assert!(attempt_current(&issued, &issued, 3, 3));
        assert!(!attempt_current(&issued, &issued, 3, 4));
        owner.retire();
        let current = owner.attach();
        assert!(!attempt_current(&issued, &current, 3, 3));
    }

    #[test]
    fn placement_focus_lease_rejects_local_ownership_but_allows_external_transfer() {
        assert!(focus_out_revokes(false, true));
        assert!(focus_out_revokes(false, false));
        assert!(focus_out_revokes(true, true));
        assert!(!focus_out_revokes(true, false));
    }

    #[test]
    fn placement_overlay_is_bound_to_exact_workspace_and_revision() {
        let row = MmWorkspace {
            id: "same".into(),
            name: "Workspace".into(),
            ..Default::default()
        };
        let overlay = PlacementOverlay {
            index: 2,
            revision: 4,
            expected: row.clone(),
            reconnect: multi_launcher::multi_manager::checked::ReconnectedWorkspace {
                workspace: row.clone(),
                identities: Default::default(),
                summary: Default::default(),
            },
        };
        assert!(overlay_current(&overlay, 2, &row, 4));
        assert!(!overlay_current(&overlay, 1, &row, 4));
        assert!(!overlay_current(&overlay, 2, &row, 5));
        let mut changed = row;
        changed.name.push('!');
        assert!(!overlay_current(&overlay, 2, &changed, 4));
    }

    #[test]
    fn terminal_discard_restores_last_committed_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        std::fs::write(&path, br#"[{"id":"one","name":"Saved"}]"#).unwrap();
        let snapshot =
            multi_launcher::multi_manager::store::load_checked_workspaces(&path, || true).unwrap();
        let mut rows = snapshot.workspaces().to_vec();
        rows[0].name = "Discarded".into();
        discard_draft(&mut rows, Some(&snapshot));
        assert_eq!(rows[0].name, "Saved");
        discard_draft(&mut rows, None);
        assert!(rows.is_empty());
    }
}
