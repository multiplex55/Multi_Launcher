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
    source: OverlaySource,
}

#[derive(Clone)]
enum OverlaySource {
    Reconnect(checked::ReconnectedWorkspace),
    Catalog(Arc<checked::ReconnectCatalogPlan>),
    Startup(Arc<checked::ReconnectCatalogPlan>, Arc<super::multi_manager_auto::Owner>),
    Restore(Arc<checked::RestorePlan>),
}
impl PlacementOverlay {
    fn bound_count(&self) -> usize {
        match &self.source {
            OverlaySource::Reconnect(value) => value.identities.len(),
            OverlaySource::Catalog(plan) | OverlaySource::Startup(plan, _) => plan.rows().iter().find(|row| row.index == self.index).map_or(0, |row| row.identities.len()),
            OverlaySource::Restore(plan) => plan
                .rows()
                .iter()
                .find(|row| row.index() == self.index)
                .map_or(0, |row| row.bound_count()),
        }
    }
}

fn overlay_current(
    overlay: &PlacementOverlay,
    index: usize,
    expected: &MmWorkspace,
    revision: u64,
) -> bool {
    overlay.index == index && overlay.revision == revision && &overlay.expected == expected
        && match &overlay.source { OverlaySource::Startup(_, owner) => owner.is_current(), _ => true }
}

fn select_overlay<'a>(
    overlays: &'a [PlacementOverlay],
    index: usize,
    expected: &MmWorkspace,
    revision: u64,
) -> Option<&'a PlacementOverlay> {
    overlays.iter().rev().find(|overlay| {
        overlay_current(overlay, index, expected, revision) && overlay.bound_count() != 0
    })
}

fn replace_restore_entries<T>(
    entries: &mut Vec<T>,
    replacements: impl IntoIterator<Item = T>,
    is_restore: impl Fn(&T) -> bool,
) {
    entries.retain(|entry| !is_restore(entry));
    entries.extend(replacements);
}

fn restore_busy(pending: bool, loading: bool, has_restore: bool) -> bool {
    pending && (!loading || has_restore)
}

fn overlay_has_live_identity(
    overlays: &[PlacementOverlay],
    rows: &[MmWorkspace],
    revision: u64,
    identity: multi_launcher::window_catalog::WindowNativeIdentity,
    is_live: impl Fn(multi_launcher::window_catalog::WindowNativeIdentity) -> bool,
) -> bool {
    overlays.iter().any(|overlay| {
        rows.get(overlay.index)
            .is_some_and(|row| overlay_current(overlay, overlay.index, row, revision))
            && match &overlay.source {
                OverlaySource::Reconnect(value) => value
                    .identities
                    .values()
                    .any(|item| item.native == identity && is_live(item.native)),
                OverlaySource::Catalog(plan) | OverlaySource::Startup(plan, _) => plan.rows().iter().find(|row| row.index == overlay.index).is_some_and(|row| row.identities.values().any(|item| item.native == identity && is_live(identity))),
                OverlaySource::Restore(plan) => plan
                    .rows()
                    .iter()
                    .find(|row| row.index() == overlay.index)
                    .is_some_and(|row| row.has_identity(identity) && is_live(identity)),
            }
    })
}

fn focus_out_revokes(started: bool, launcher_foreground: bool) -> bool {
    !started || launcher_foreground
}

pub(super) const WAIT: &str = "Save or discard the MultiManager workspace or paths draft before closing.";

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
                    | multi_launcher::commands::MultiManagerCommand::RestoreBindings
                    | multi_launcher::commands::MultiManagerCommand::Settings
                    | multi_launcher::commands::MultiManagerCommand::Reconnect
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WorkspaceOperationKind { RestoreBindings, CatalogReconnect }

pub(super) fn workspace_operation(action: &Action) -> Option<WorkspaceOperationKind> {
    if action.args.is_some() { return None; }
    match parse_action(action).ok()? {
        Command::MultiManager(multi_launcher::commands::MultiManagerCommand::RestoreBindings) => Some(WorkspaceOperationKind::RestoreBindings),
        Command::MultiManager(multi_launcher::commands::MultiManagerCommand::Reconnect) => Some(WorkspaceOperationKind::CatalogReconnect),
        _ => None,
    }
}

struct RestoreIntent {
    kind: WorkspaceOperationKind,
    query: String,
    action: Action,
    generation: u64,
    index: usize,
    visibility_revision: u64,
    native: NativeWindow,
    mount: utility_mount::MountReceipt,
    lease: Arc<AtomicBool>,
    permit: Option<EffectPermit>,
    validated: bool,
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
    loading: bool,
    restore_intent: Option<RestoreIntent>,
    restore_lease: Option<Arc<AtomicBool>>,
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
    capture_slot: Option<multi_launcher::multi_manager::capture::CaptureCancel>,
    capture_identities: Vec<multi_launcher::window_catalog::WindowNativeIdentity>,
    ignore_launcher_capture: bool,
    overlays: Vec<PlacementOverlay>,
    auto: Arc<super::multi_manager_auto::Owner>,
}

impl MultiManagerView {
    fn retire_auto_reconnect(&mut self) {
        self.auto.retire();
        self.overlays.retain(|overlay| !matches!(&overlay.source, OverlaySource::Startup(_, _)));
    }
    pub(super) fn revoke_operation(&mut self) {
        self.revoke_restore();
        if let Some(lease) = &self.operation_lease {
            lease.store(false, Ordering::Release);
        }
        if let Some(cancel) = &self.capture_slot {
            cancel.cancel();
        }
    }

    pub(super) fn cancel_capture(&mut self) {
        if self.capture_slot.is_some() {
            self.revoke_operation();
        }
    }

    fn capture_available(&self, cx: &App) -> bool {
        self.root.upgrade().is_some_and(|root| {
            let root = root.read(cx);
            root.enabled_plugins
                .as_ref()
                .is_none_or(|enabled| enabled.contains("MultiManager"))
                && multi_launcher::settings::provider_search_capability_enabled(
                    root.enabled_capabilities.as_ref(),
                    "MultiManager",
                )
        })
    }

    fn capture_window(
        &mut self,
        index: usize,
        expected: &MmWorkspace,
        revision: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending
            || !self.capture_available(cx)
            || !self.current(window, cx)
            || !row_current(&self.rows, index, expected, revision, self.revision)
            || expected.id.is_empty()
            || expected.disabled
            || !expected.valid
            || self.rows.iter().filter(|row| row.id == expected.id).count() != 1
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
        self.pending = true;
        self.attempt = self.attempt.wrapping_add(1);
        let attempt = self.attempt;
        let lease = Arc::new(AtomicBool::new(true));
        self.operation_lease = Some(lease.clone());
        let cheap_mount = self.mount.clone();
        let cheap_visibility = self.visibility.clone();
        let cheap_revision = cheap_visibility.request_revision.load(Ordering::Acquire);
        let cheap_admission = self.admission.clone();
        let cheap_lease = lease.clone();
        let cancel = multi_launcher::multi_manager::capture::CaptureCancel::with_authority(
            Arc::new(move || {
                cheap_mount.is_current()
                    && !cheap_admission.is_fenced()
                    && cheap_lease.load(Ordering::Acquire)
                    && json_utility::native_host_current(
                        &cheap_visibility,
                        cheap_revision,
                        Some(native),
                        false,
                    )
            }),
        );
        self.capture_slot = Some(cancel.clone());
        let local = lease.clone();
        let local_cancel = cancel.clone();
        self.focus_guard = Some(cx.on_focus_out(&self.focus, window, move |_, _, _, _| {
            if current_foreground_identity() == Some(native.identity) {
                local.store(false, Ordering::Release);
                local_cancel.cancel();
            }
        }));
        let receipt = self.mount.clone();
        let worker_receipt = receipt.clone();
        let host_revision = self.visibility.request_revision.load(Ordering::Acquire);
        let visibility = self.visibility.clone();
        let admission = self.admission.clone();
        let ignore_launcher = self.ignore_launcher_capture;
        let expected = expected.clone();
        let issued_lease = lease.clone();
        self.message = Some("Preparing capture…".into());
        let readiness = Arc::new(std::sync::atomic::AtomicU8::new(0));
        let signal = Arc::new(HashWakeSignal::default());
        let ready_receipt = receipt.clone();
        let ready_lease = issued_lease.clone();
        let ready_state = readiness.clone();
        let ready_signal = signal.clone();
        cx.spawn(async move |weak, cx| {
            while ready_signal.wait().await {
                let state = ready_state.load(Ordering::Acquire);
                if state == 2 { break; }
                if weak.update(cx, |view, cx| {
                    if attempt_current(&ready_receipt, &view.mount, attempt, view.attempt)
                        && view.capture_slot.is_some() && ready_lease.load(Ordering::Acquire) {
                        view.message = Some(if state == 1 {
                            "Focus a window and press Enter to capture it. Press Escape or Cancel to stop."
                        } else { "Preparing capture…" }.into());
                        cx.notify();
                    }
                }).is_err() { break; }
            }
        }).detach();
        let task = cx.background_executor().spawn(async move {
            let result = (|| {
                let current = || {
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
                };
                for _ in 0..3 {
                    if !current() {
                        return Err("Capture was superseded.".to_string());
                    }
                    let session =
                        multi_launcher::multi_manager::capture::start_capture_session_with_wake(
                            Arc::new(|| {}), cancel.clone(), multi_launcher::multi_manager::capture::CaptureMode::HookRequired,
                        )
                        .map_err(|error| match error {
                            multi_launcher::multi_manager::capture::CaptureStartError::Busy => "Another capture session is already active.",
                            multi_launcher::multi_manager::capture::CaptureStartError::Cancelled => "Capture cancelled.",
                            multi_launcher::multi_manager::capture::CaptureStartError::NativeUnavailable => "Capture keyboard hook is unavailable.",
                        }.to_string())?;
                    if !lease.load(Ordering::Acquire) {
                        session.cancel();
                    }
                    readiness.store(1, Ordering::Release);
                    signal.notify();
                    let event = session.wait();
                    readiness.store(0, Ordering::Release);
                    signal.notify();
                    if !current() {
                        return Err("Capture was superseded.".to_string());
                    }
                    let Some(event) = event else {
                        return Err("Capture cancelled.".to_string());
                    };
                    if event.action != multi_launcher::multi_manager::win::CaptureKeyAction::Confirm
                    {
                        return Err("Capture cancelled.".to_string());
                    }
                    if let (Some(window), Some(identity)) = (event.captured, event.identity) {
                        if (!ignore_launcher || identity.pid != std::process::id())
                            && multi_launcher::window_catalog::native_window_identity(window.hwnd)
                                .as_ref()
                                == Some(&identity)
                            && current()
                        {
                            return Ok((window, identity));
                        }
                    }
                }
                Err("No eligible window was captured. Try again.".to_string())
            })();
            readiness.store(2, Ordering::Release);
            signal.notify();
            (permit, result)
        });
        cx.spawn(async move |weak, cx| {
            let (permit, result) = task.await;
            let _ = weak.update(cx, |view, cx| {
                if !attempt_current(&receipt, &view.mount, attempt, view.attempt) { return; }
                view.pending = false;
                view.operation_lease = None;
                view.capture_slot = None;
                view.focus_guard = None;
                if !issued_lease.load(Ordering::Acquire) || view.admission.is_fenced()
                    || !row_current(&view.rows, index, &expected, revision, view.revision)
                    || !json_utility::native_host_current(&view.visibility, host_revision, Some(native), false) {
                    view.message = Some("Capture cancelled or superseded.".into());
                    view.settle_wait(cx); cx.notify(); return;
                }
                match result {
                    Ok((captured, identity)) if multi_launcher::window_catalog::native_window_identity(captured.hwnd).as_ref() == Some(&identity) => {
                        let mut shadow = view.rows.clone();
                        for row in &mut shadow { for window in &mut row.windows { window.hwnd = 0; window.binding_verified = false; } }
                        let duplicate = view.capture_identities.iter().any(|item| view.rows.iter().any(|row| row.windows.iter().any(|window| window.hwnd == item.hwnd)) && *item == identity && multi_launcher::window_catalog::native_window_identity(item.hwnd).as_ref() == Some(item))
                            || overlay_has_live_identity(&view.overlays, &view.rows, view.revision, identity, |native| multi_launcher::window_catalog::native_window_identity(native.hwnd) == Some(native));
                        let action = multi_launcher::multi_manager::model::PendingCaptureAction::CaptureOneWindow { workspace_id: expected.id.clone() };
                        if !duplicate && multi_launcher::multi_manager::apply_capture::apply_capture_to_workspaces(&mut shadow, &action, captured) == multi_launcher::multi_manager::apply_capture::ApplyCaptureResult::Applied {
                            if let Some(window) = shadow[index].windows.pop() {
                                view.rows[index].windows.push(window);
                                view.capture_identities.push(identity);
                                view.overlays.clear();
                                view.revision = view.revision.wrapping_add(1);
                                view.message = Some("Window captured. Save to keep the workspace change.".into());
                            }
                        } else { view.message = Some("That window is already captured.".into()); }
                    }
                    Ok(_) => view.message = Some("The captured window changed before it could be added.".into()),
                    Err(error) => view.message = Some(error),
                }
                view.settle_wait(cx); cx.notify();
            });
            drop(permit);
        }).detach();
        cx.notify();
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
        let Some(overlay) = select_overlay(&self.overlays, index, expected, revision).cloned()
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
        let startup_owner = match &overlay.source { OverlaySource::Startup(_, owner) => Some(owner.clone()), _ => None };
        let current: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
            worker_receipt.is_current()
                && startup_owner.as_ref().is_none_or(|owner| owner.is_current())
                && !admission.is_fenced()
                && lease.load(Ordering::Acquire)
                && store::checked_workspace_source_is_current(&snapshot)
                && worker_receipt.is_current()
                && startup_owner.as_ref().is_none_or(|owner| owner.is_current())
                && !admission.is_fenced()
                && lease.load(Ordering::Acquire)
                && json_utility::native_host_current(
                    &visibility,
                    host_revision,
                    Some(native),
                    false,
                )
        });
        let restore_placement = matches!(&overlay.source, OverlaySource::Restore(_));
        let task = cx.background_executor().spawn(async move {
            let mut backend = checked::NativePlacement {
                effect_started: started,
            };
            let report = match overlay.source {
                OverlaySource::Reconnect(ref reconnect) => {
                    checked::place_checked(reconnect, issuer, destination, current, &mut backend)
                }
                OverlaySource::Catalog(plan) | OverlaySource::Startup(plan, _) => checked::place_catalog_checked(plan, overlay.index, issuer, destination, current, &mut backend),
                OverlaySource::Restore(plan) => checked::place_restored_checked(
                    plan,
                    overlay.index,
                    issuer,
                    destination,
                    current,
                    &mut backend,
                ),
            };
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
                if report.error.is_some() && restore_placement {
                    view.overlays.retain(|item| {
                        item.index != index || !matches!(&item.source, OverlaySource::Restore(_))
                    });
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
        self.retire_auto_reconnect();
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
                            view.overlays.retain(|item| {
                                item.index != index
                                    || !matches!(&item.source, OverlaySource::Reconnect(_))
                            });
                            view.overlays.push(PlacementOverlay {
                                index,
                                revision,
                                expected,
                                source: OverlaySource::Reconnect(reconnect),
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
        auto: Arc<super::multi_manager_auto::Owner>,
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
            loading: false,
            restore_intent: None,
            restore_lease: None,
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
            capture_slot: None,
            capture_identities: Vec::new(),
            ignore_launcher_capture: settings.ignore_launcher_window_on_capture,
            overlays: Vec::new(),
            auto,
        };
        view.reload(cx);
        view
    }
    fn dirty(&self) -> bool {
        self.snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.workspaces() != self.rows.as_slice())
    }
    fn adopt_auto_reconnect(&mut self, cx: &mut Context<Self>) {
        if !self.open || !self.mount.is_current() || self.admission.is_fenced()
            || self.pending || self.dirty() || self.rename.is_some() || self.review.is_some()
        {
            return;
        }
        let Some(plan) = self.auto.plan() else {
            if let Some(error) = self.auto.error() {
                self.message = Some(error);
                cx.notify();
            }
            return;
        };
        let Some(snapshot) = &self.snapshot else { return; };
        if !plan.matches_raw_source(snapshot) { return; }
        let rows = plan.rows().iter().filter_map(|row| {
            self.rows.get(row.index).cloned().map(|expected| PlacementOverlay {
                index: row.index,
                revision: self.revision,
                expected,
                source: OverlaySource::Startup(plan.clone(), self.auto.clone()),
            })
        }).collect::<Vec<_>>();
        replace_restore_entries(&mut self.overlays, rows, |old| {
            matches!(&old.source, OverlaySource::Startup(_, _))
        });
        if plan.outcome == checked::ReconnectCatalogOutcome::NoMatch {
            self.message = Some("Automatic reconnect found no matching windows.".into());
        }
        cx.notify();
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
    pub(super) fn revoke_restore(&mut self) {
        if let Some(lease) = self.restore_lease.take() {
            lease.store(false, Ordering::Release);
        }
        if let Some(intent) = self.restore_intent.take() {
            intent.lease.store(false, Ordering::Release);
            self.operation_lease = None;
            self.focus_guard = None;
            self.activation_guard = None;
        }
    }

    fn restore_origin_current(
        &self,
        intent: &RestoreIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        intent.lease.load(Ordering::Acquire)
            && intent.mount.is_current() && intent.mount.matches(&self.mount)
            && self.visibility.request_revision.load(Ordering::Acquire) == intent.visibility_revision
            && self.native == Some(intent.native)
            && self.focus.contains_focused(window, cx)
            && self.current(window, cx)
            && self.root.upgrade().is_some_and(|root| {
                let root = root.read(cx);
                root.query.read(cx).value().as_ref() == intent.query.as_str()
                    && root.launcher_results.resolve_result(intent.generation, intent.index, &intent.query).is_some_and(|result| {
                        matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "MultiManager")
                            && result.action == intent.action
                            && current_provider(&intent.query, &result.action, root.enabled_plugins.as_ref(), root.enabled_capabilities.as_ref())
                    })
            })
    }

    fn queue_restore(
        &mut self,
        query: String,
        action: Action,
        generation: u64,
        index: usize,
        visibility_revision: u64,
        native: NativeWindow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(kind) = workspace_operation(&action) else { return; };
        if self.restore_intent.is_some()
            || self.rename.is_some()
            || self.review.is_some()
            || self.capture_slot.is_some()
            || self.dirty()
            || restore_busy(self.pending, self.loading, self.restore_lease.is_some())
        {
            return;
        }
        let Some(permit) = self.admission.admit() else {
            return;
        };
        self.retire_auto_reconnect();
        let lease = Arc::new(AtomicBool::new(true));
        let blur = lease.clone();
        self.focus_guard = Some(cx.on_focus_out(&self.focus, window, move |_, _, _, _| {
            blur.store(false, Ordering::Release);
        }));
        let active = lease.clone();
        self.activation_guard = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() {
                active.store(false, Ordering::Release);
            }
        }));
        let intent = RestoreIntent {
            kind,
            query,
            action,
            generation,
            index,
            visibility_revision,
            native,
            mount: self.mount.clone(),
            lease,
            permit: Some(permit),
            validated: false,
        };
        self.restore_lease = Some(intent.lease.clone());
        self.operation_lease = Some(intent.lease.clone());
        self.restore_intent = Some(intent);
    }

    fn validate_restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let valid = self
            .restore_intent
            .as_ref()
            .is_some_and(|intent| self.restore_origin_current(intent, window, cx));
        if !valid {
            self.revoke_restore();
            self.operation_lease = None;
            self.focus_guard = None;
            self.activation_guard = None;
            return;
        }
        if let Some(intent) = self.restore_intent.as_mut() {
            intent.validated = true;
        }
        if !self.pending {
            self.start_restore(window, cx);
        }
    }

    fn start_restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(intent) = self.restore_intent.take() else {
            return;
        };
        if self.pending
            || self.rename.is_some()
            || self.review.is_some()
            || self.dirty()
            || !self.restore_origin_current(&intent, window, cx)
        {
            self.focus_guard = None;
            self.activation_guard = None;
            self.operation_lease = None;
            self.restore_lease = None;
            return;
        }
        let Some(snapshot) = self.snapshot.clone() else {
            self.focus_guard = None;
            self.activation_guard = None;
            self.operation_lease = None;
            self.restore_lease = None;
            self.message = Some("Could not restore bindings without a loaded workspace.".into());
            cx.notify();
            return;
        };
        self.retire_auto_reconnect();
        if intent.kind == WorkspaceOperationKind::CatalogReconnect {
            self.start_catalog(Some(intent), window, cx);
            return;
        }
        self.pending = true;
        self.attempt = self.attempt.wrapping_add(1);
        let attempt = self.attempt;
        let revision = self.revision;
        let path = self.bindings_path.clone();
        let visibility = self.visibility.clone();
        let admission = self.admission.clone();
        let worker_mount = intent.mount.clone();
        let worker_lease = intent.lease.clone();
        let native = intent.native;
        let visibility_revision = intent.visibility_revision;
        let host = self.host;
        let task = cx.background_executor().spawn(async move {
            checked::restore_checked(
                &snapshot,
                &path,
                || {
                    worker_mount.is_current()
                        && !admission.is_fenced()
                        && worker_lease.load(Ordering::Acquire)
                        && json_utility::native_host_current(
                            &visibility,
                            visibility_revision,
                            Some(native),
                            false,
                        )
                        && current_foreground_identity() == Some(native.identity)
                },
                &checked::NativeRestore,
            )
        });
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            let delivered = host.update(cx, |_, window, cx| {
                let _ = weak.update(cx, |view, cx| {
                    if !attempt_current(&intent.mount, &view.mount, attempt, view.attempt) {
                        return;
                    }
                    view.pending = false;
                    let current = view.revision == revision
                        && view.restore_origin_current(&intent, window, cx);
                    view.focus_guard = None;
                    view.activation_guard = None;
                    view.operation_lease = None;
                    view.restore_lease = None;
                    if current {
                        match result {
                            Ok(plan) => {
                                let plan = Arc::new(plan);
                                let replacements = plan
                                    .rows()
                                    .iter()
                                    .filter_map(|row| {
                                        let index = row.index();
                                        view.rows.get(index).cloned().map(|expected| {
                                            PlacementOverlay {
                                                index,
                                                revision,
                                                expected,
                                                source: OverlaySource::Restore(plan.clone()),
                                            }
                                        })
                                    })
                                    .collect::<Vec<_>>();
                                replace_restore_entries(&mut view.overlays, replacements, |old| {
                                    matches!(&old.source, OverlaySource::Restore(_))
                                });
                                view.message = Some(format!(
                                    "Restored {} window(s), skipped {}.",
                                    plan.bound, plan.skipped
                                ));
                            }
                            Err(error) => {
                                view.message =
                                    Some(format!("Could not restore bindings: {error:#}"))
                            }
                        }
                    }
                    view.settle_wait(cx);
                    cx.notify();
                });
            });
            if delivered.is_err() {
                let _ = weak.update(cx, |view, cx| {
                    if attempt_current(&intent.mount, &view.mount, attempt, view.attempt) {
                        view.pending = false;
                        view.operation_lease = None;
                        view.restore_lease = None;
                        view.focus_guard = None;
                        view.activation_guard = None;
                        view.message =
                            Some("Restore finished after the launcher became unavailable.".into());
                        view.settle_wait(cx);
                        cx.notify();
                    }
                });
            }
            drop(intent.permit);
        })
        .detach();
        cx.notify();
    }
    fn start_catalog(&mut self, mut intent: Option<RestoreIntent>, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || self.dirty() || self.rename.is_some() || self.review.is_some() || self.capture_slot.is_some() || !self.current(window, cx) { return; }
        let Some(snapshot) = self.snapshot.clone() else { self.message = Some("Load workspaces before reconnecting.".into()); cx.notify(); return; };
        let Some(native) = self.native else { return; };
        let (permit, lease, receipt, visibility_revision) = if let Some(origin) = intent.as_mut() {
            let Some(permit) = origin.permit.take() else { return; };
            (permit, origin.lease.clone(), origin.mount.clone(), origin.visibility_revision)
        } else {
            let Some(permit) = self.admission.admit() else { return; };
            self.focus.focus(window, cx);
            let lease = Arc::new(AtomicBool::new(true));
            let blur = lease.clone();
            self.focus_guard = Some(cx.on_focus_out(&self.focus, window, move |_, _, _, _| blur.store(false, Ordering::Release)));
            let active = lease.clone();
            self.activation_guard = Some(cx.observe_window_activation(window, move |_, window, _| { if !window.is_window_active() { active.store(false, Ordering::Release); } }));
            (permit, lease, self.mount.clone(), self.visibility.request_revision.load(Ordering::Acquire))
        };
        self.retire_auto_reconnect();
        self.operation_lease = Some(lease.clone()); self.pending = true;
        self.attempt = self.attempt.wrapping_add(1); let attempt = self.attempt; let revision = self.revision;
        let worker_mount = receipt.clone(); let worker_lease = lease.clone(); let visibility = self.visibility.clone(); let admission = self.admission.clone(); let host = self.host;
        let focus = self.focus.clone(); let weak = cx.entity().downgrade();
        window.defer(cx, move |window, app| { let _ = weak.update(app, |view, cx| {
        if !attempt_current(&receipt, &view.mount, attempt, view.attempt) { return; }
        let same_source = view.snapshot.as_ref().is_some_and(|current| current.path() == snapshot.path() && current.source() == snapshot.source() && current.workspaces() == snapshot.workspaces());
        let allowed = attempt_current(&receipt, &view.mount, attempt, view.attempt) && view.pending && !view.dirty() && view.revision == revision && same_source
            && lease.load(Ordering::Acquire) && view.current(window, cx) && view.visibility.request_revision.load(Ordering::Acquire) == visibility_revision
            && (intent.is_some() || focus.is_focused(window)) && intent.as_ref().is_none_or(|origin| view.restore_origin_current(origin, window, cx));
        if !allowed { view.pending = false; view.operation_lease = None; view.restore_lease = None; view.focus_guard = None; view.activation_guard = None; view.message = Some("Reconnect was superseded; try again.".into()); view.settle_wait(cx); cx.notify(); return; }
        let task = cx.background_executor().spawn(async move {
            checked::reconnect_catalog_checked(&snapshot, || worker_mount.is_current() && worker_lease.load(Ordering::Acquire) && !admission.is_fenced() && current_foreground_identity() == Some(native.identity) && json_utility::native_host_current(&visibility, visibility_revision, Some(native), false), &checked::NativeReconnect)
        });
        cx.spawn(async move |weak, cx| {
            let _permit = permit; let result = task.await;
            let delivered = host.update(cx, |_, window, cx| { let _ = weak.update(cx, |view, cx| {
                if !attempt_current(&receipt, &view.mount, attempt, view.attempt) { return; }
                view.pending = false; view.operation_lease = None; view.restore_lease = None; view.focus_guard = None; view.activation_guard = None;
                let current = view.revision == revision && !view.dirty() && lease.load(Ordering::Acquire) && view.current(window, cx)
                    && view.visibility.request_revision.load(Ordering::Acquire) == visibility_revision
                    && intent.as_ref().is_none_or(|origin| view.restore_origin_current(origin, window, cx));
                if current { match result {
                    Ok(plan) if view.snapshot.as_ref().is_some_and(|snapshot| plan.matches_snapshot(snapshot)) => {
                        let plan = Arc::new(plan);
                        let rows = plan.rows().iter().filter_map(|row| view.rows.get(row.index).cloned().map(|expected| PlacementOverlay {
                            index: row.index, revision, expected, source: OverlaySource::Catalog(plan.clone()),
                        })).collect::<Vec<_>>();
                        replace_restore_entries(&mut view.overlays, rows, |old| matches!(&old.source, OverlaySource::Reconnect(_) | OverlaySource::Catalog(_)));
                        let state = match plan.outcome { checked::ReconnectCatalogOutcome::Complete => "Reconnect complete", checked::ReconnectCatalogOutcome::Partial => "Reconnect partially complete", checked::ReconnectCatalogOutcome::NoMatch => "No windows matched" };
                        view.message = Some(format!("{state}: {} matched, {} missing, {} ambiguous, {} metadata mismatch, {} workspaces skipped.", plan.summary.reconnected + plan.summary.already_valid, plan.summary.missing, plan.summary.ambiguous, plan.summary.metadata_mismatch, plan.skipped));
                    }
                    Ok(_) => view.message = Some("Workspaces changed; reload before reconnecting.".into()),
                    Err(error) => view.message = Some(format!("Could not reconnect: {error:#}")),
                } }
                view.settle_wait(cx); cx.notify();
            }); });
            if delivered.is_err() { let _ = weak.update(cx, |view, cx| { if attempt_current(&receipt, &view.mount, attempt, view.attempt) { view.pending = false; view.operation_lease = None; view.restore_lease = None; view.focus_guard = None; view.activation_guard = None; view.message = Some("Reconnect finished after the launcher became unavailable.".into()); view.settle_wait(cx); cx.notify(); } }); }
        }).detach(); cx.notify();
        }); }); cx.notify();
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        if self.pending || !self.open {
            return;
        }
        self.overlays.clear();
        self.pending = true;
        self.loading = true;
        self.attempt = self.attempt.wrapping_add(1);
        let attempt = self.attempt;
        let receipt = self.mount.clone();
        let worker_receipt = receipt.clone();
        let path = self.workspace_path.clone();
        let binding_path = self.bindings_path.clone();
        let admission = self.admission.clone();
        let task = cx.background_executor().spawn(async move {
            let (workspace, bindings) =
                checked::load_editor_sources_checked(&path, &binding_path, || {
                    worker_receipt.is_current() && !admission.is_fenced()
                });
            (workspace, bindings.map_err(|error| format!("{error:#}")))
        });
        let host = self.host;
        cx.spawn(async move |weak, cx| {
            let (workspace, bindings) = task.await;
            let delivered = host.update(cx, |_, window, cx| {
                let _ = weak.update(cx, |view, cx| {
                    if !attempt_current(&receipt, &view.mount, attempt, view.attempt) {
                        return;
                    }
                    view.pending = false;
                    view.loading = false;
                    let loaded = workspace.is_ok();
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
                    if loaded && view.restore_intent.is_none() { view.adopt_auto_reconnect(cx); }
                    if !loaded {
                        view.revoke_restore();
                    }
                    if view
                        .restore_intent
                        .as_ref()
                        .is_some_and(|intent| intent.validated)
                    {
                        view.start_restore(window, cx);
                    }
                    view.settle_wait(cx);
                    cx.notify();
                });
            });
            if delivered.is_err() {
                let _ = weak.update(cx, |view, cx| {
                    if attempt_current(&receipt, &view.mount, attempt, view.attempt) {
                        view.pending = false;
                        view.loading = false;
                        view.revoke_restore();
                        view.message = Some(
                            "Workspace load finished after the launcher became unavailable.".into(),
                        );
                        view.settle_wait(cx);
                        cx.notify();
                    }
                });
            }
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
        self.overlays.clear();
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
        self.revoke_operation();
        self.open = false;
        self.overlays.clear();
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
        self.revoke_operation();
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
    Capture,
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
        if matches!(action, RowAction::Rename | RowAction::Toggle | RowAction::Delete | RowAction::Capture)
            || matches!(action, RowAction::Up) && index > 0
            || matches!(action, RowAction::Down) && index + 1 < self.rows.len()
        {
            self.retire_auto_reconnect();
        }
        match action {
            RowAction::Rename => {
                self.overlays.clear();
                self.rename_epoch = self.rename_epoch.wrapping_add(1);
                self.name.update(cx, |name, cx| {
                    name.set_value(expected.name.clone(), window, cx)
                });
                self.rename = Some((index, expected.clone(), revision));
                self.name.focus_handle(cx).focus(window, cx);
            }
            RowAction::Toggle => {
                self.overlays.clear();
                self.rows[index].disabled = !self.rows[index].disabled;
                self.revision = self.revision.wrapping_add(1);
            }
            RowAction::Up if index > 0 => {
                self.overlays.clear();
                self.rows.swap(index, index - 1);
                self.revision = self.revision.wrapping_add(1);
                self.expanded.clear();
            }
            RowAction::Down if index + 1 < self.rows.len() => {
                self.overlays.clear();
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
            RowAction::Capture => self.capture_window(index, expected, revision, window, cx),
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
                                        view.overlays.clear();
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
                                    view.retire_auto_reconnect();
                                    view.rows.push(MmWorkspace {
                                        id: new_workspace_id(),
                                        name: "New Workspace".into(),
                                        ..Default::default()
                                    });
                                    view.overlays.clear();
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
                                    view.retire_auto_reconnect();
                                    view.reload(cx);
                                }
                            })
                        }),
                )
                .child(
                    Button::new("mm-reconnect-windows")
                        .label("Reconnect Windows")
                        .disabled(self.pending || self.dirty() || self.snapshot.is_none())
                        .on_click({
                            let mount = mount.clone();
                            cx.listener(move |view, _, window, cx| {
                                if mount.is_current() && mount.matches(&view.mount) && view.revision == issued_revision {
                                    view.start_catalog(None, window, cx);
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
        if self.capture_slot.is_some() {
            let capture_mount = self.mount.clone();
            let capture_attempt = self.attempt;
            panel = panel.child(
                Button::new("mm-capture-cancel")
                    .label("Cancel Capture")
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if view.capture_slot.is_some()
                            && attempt_current(
                                &capture_mount,
                                &view.mount,
                                capture_attempt,
                                view.attempt,
                            )
                        {
                            view.cancel_capture();
                            view.message = Some("Cancelling capture…".into());
                            cx.notify();
                        }
                    })),
            );
        }
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
                ("Capture Active Window", RowAction::Capture),
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
                let overlay_ready =
                    select_overlay(&self.overlays, index, row, issued_revision).is_some();
                let unavailable = self.pending
                    || match action {
                        RowAction::Capture => {
                            !self.capture_available(cx)
                                || self.snapshot.is_none()
                                || row.disabled
                                || !row.valid
                                || row.id.is_empty()
                                || self.rows.iter().filter(|other| other.id == row.id).count() != 1
                        }
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
            if let Some(overlay) = self.overlays.iter().rev().find(|overlay| {
                overlay_current(overlay, index, row, self.revision)
            }) {
                card = card.child(Label::new(format!(
                    "{} of {} windows {}",
                    overlay.bound_count(),
                    row.windows.len(),
                    match &overlay.source {
                        OverlaySource::Reconnect(_) => "reconnected",
                        OverlaySource::Catalog(_) | OverlaySource::Startup(_, _) => "reconnected",
                        OverlaySource::Restore(_) => "restored",
                    }
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
    pub(super) fn start_auto_multi_manager(&mut self, cx: &mut Context<Self>) {
        let settings = &self.search_catalog.multi_manager_settings;
        let base = self.search_catalog.multi_manager_settings_path.parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let path = state::resolve_relative_to(base, &settings.workspaces_path);
        self.multi_manager_auto.start(
            settings.auto_reconnect_on_load, path, self.restart_admission.clone(),
        );
        let wake = self.multi_manager_auto.wake.clone();
        cx.spawn(async move |root, cx| {
            while wake.wait().await {
                if root.update(cx, |root, cx| {
                    if let Some(view) = &root.multi_manager_view {
                        view.update(cx, |view, cx| view.adopt_auto_reconnect(cx));
                    }
                }).is_err() {
                    break;
                }
            }
        }).detach();
    }
    pub(super) fn restore_multi_manager(
        &mut self,
        query: String,
        action: Action,
        generation: u64,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(native) = self
            .native_window
            .filter(|native| current_foreground_identity() == Some(native.identity))
        else {
            return;
        };
        let revision = self
            .root_visibility
            .request_revision
            .load(Ordering::Acquire);
        if self.restart_admission.is_fenced()
            || window.has_active_dialog(cx)
            || !window.is_window_active()
            || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
        {
            return;
        }
        if !self.retire_clean_multi_manager_paths(cx) { return; }
        if let Some(view) = &self.multi_manager_view {
            let view_state = view.read(cx);
            if restore_busy(
                view_state.pending,
                view_state.loading,
                view_state.restore_lease.is_some(),
            ) {
                return;
            }
            view.update(cx, |view, _| view.revoke_restore());
        }
        self.open_multi_manager(window, cx);
        let Some(view) = self.multi_manager_view.clone() else {
            return;
        };
        view.update(cx, |view, cx| {
            view.queue_restore(
                query, action, generation, index, revision, native, window, cx,
            )
        });
        window.defer(cx, move |window, cx| {
            view.update(cx, |view, cx| view.validate_restore(window, cx));
        });
    }

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
        if !self.retire_clean_multi_manager_paths(cx) { return; }
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
            let auto = self.multi_manager_auto.clone();
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
                    auto,
                )
            });
            view.update(cx, |view, cx| view.focus.focus(window, cx));
            self.multi_manager_view = Some(view.clone());
            view
        };
        let _ = view;
        let index = self.multi_manager_scroll_item(window.window_handle().window_id(), cx);
        self.root_scroll.scroll_to_top_of_item(index);
        cx.notify();
    }

    pub(super) fn multi_manager_scroll_item(&self, id: WindowId, cx: &App) -> usize {
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
                + usize::from(editor)
    }

    fn retire_clean_multi_manager_paths(&mut self, cx: &mut Context<Self>) -> bool {
        let ready = !self.multi_manager_paths_uncertain && self.multi_manager_paths.as_ref().is_none_or(|view| view.update(cx, |view, cx| view.retire_for_workspace(cx)));
        if !ready {
            self.launcher_search_error = Some("Save or Cancel MultiManager paths before opening a workspace; relaunch if reconciliation is required.".into());
            cx.notify();
        }
        ready
    }
}

#[cfg(test)]
mod tests {
    use super::{
        OverlaySource, PlacementOverlay, attempt_current, current_provider, discard_draft,
        focus_out_revokes, overlay_current, overlay_has_live_identity, replace_restore_entries,
        restore_busy, row_current, select_overlay,
    };
    use multi_launcher::multi_manager::model::MmWorkspace;
    use multi_launcher::multi_manager::{checked, store};
    use multi_launcher::plugin::Plugin;
    use std::collections::HashSet;
    use std::sync::Arc;
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
    fn restore_provider_requires_exact_stock_action_and_enabled_search() {
        let action =
            multi_launcher::multi_manager::commands::search_mm_commands("mm restore bindings")
                .into_iter()
                .find(|row| row.action == "mm:restore-bindings")
                .unwrap();
        assert!(current_provider("mm restore bindings", &action, None, None));
        assert!(!current_provider("mm restore", &action, None, None));
        assert!(!current_provider(
            "mm restore bindings",
            &action,
            Some(&HashSet::new()),
            None
        ));
        let mut forged = action;
        forged.label.push('!');
        assert!(!current_provider(
            "mm restore bindings",
            &forged,
            None,
            None
        ));
    }

    #[test]
    fn settings_provider_requires_exact_stock_action() {
        let action = multi_launcher::multi_manager::commands::search_mm_commands("mm settings").into_iter().find(|row| row.action == "mm:settings").unwrap();
        assert!(current_provider("mm settings", &action, None, None));
        assert!(!current_provider("mm", &action, None, None));
        assert!(!current_provider("mm settings", &action, Some(&HashSet::new()), None));
    }
    #[test]
    fn reconnect_provider_requires_exact_stock_action() {
        let action = multi_launcher::multi_manager::commands::search_mm_commands("mm reconnect").into_iter().find(|row| row.action == "mm:reconnect").unwrap();
        assert!(current_provider("mm reconnect", &action, None, None));
        assert!(!current_provider("mm", &action, None, None));
        assert!(!current_provider("mm reconnect", &action, Some(&HashSet::new()), None));
        assert_eq!(super::workspace_operation(&action), Some(super::WorkspaceOperationKind::CatalogReconnect));
        let restore = multi_launcher::multi_manager::commands::search_mm_commands("mm restore bindings").into_iter().find(|row| row.action == "mm:restore-bindings").unwrap();
        assert_eq!(super::workspace_operation(&restore), Some(super::WorkspaceOperationKind::RestoreBindings));
        let mut forged = action; forged.args = Some("other".into());
        assert_eq!(super::workspace_operation(&forged), None);
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
            source: OverlaySource::Reconnect(
                multi_launcher::multi_manager::checked::ReconnectedWorkspace {
                    workspace: row.clone(),
                    identities: Default::default(),
                    summary: Default::default(),
                },
            ),
        };
        assert!(overlay_current(&overlay, 2, &row, 4));
        assert!(!overlay_current(&overlay, 1, &row, 4));
        assert!(!overlay_current(&overlay, 2, &row, 5));
        let mut changed = row;
        changed.name.push('!');
        assert!(!overlay_current(&overlay, 2, &changed, 4));
    }

    #[test]
    fn startup_overlay_loses_authority_without_retiring_manual_overlay() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspaces.json");
        std::fs::write(&path, br#"[{"id":"one","name":"Saved"}]"#).unwrap();
        let snapshot = store::load_checked_workspaces(&path, || true).unwrap();
        let plan = Arc::new(checked::reconnect_catalog_checked(&snapshot, || true, &checked::NativeReconnect).unwrap());
        let row = snapshot.workspaces()[0].clone();
        let owner = Arc::new(super::super::multi_manager_auto::Owner::default());
        let startup = PlacementOverlay { index: 0, revision: 1, expected: row.clone(), source: OverlaySource::Startup(plan.clone(), owner.clone()) };
        let manual = PlacementOverlay { index: 0, revision: 1, expected: row.clone(), source: OverlaySource::Catalog(plan) };
        assert!(overlay_current(&startup, 0, &row, 1));
        owner.retire();
        assert!(!overlay_current(&startup, 0, &row, 1));
        assert!(overlay_current(&manual, 0, &row, 1));
    }

    #[test]
    fn restore_replaces_only_prior_restore_entries_even_when_empty_or_partial() {
        #[derive(Debug, PartialEq)]
        enum Entry {
            Reconnect(u8),
            Restore(u8),
        }
        let mut entries = vec![Entry::Reconnect(1), Entry::Restore(1), Entry::Restore(2)];
        replace_restore_entries(&mut entries, vec![Entry::Restore(3)], |entry| {
            matches!(entry, Entry::Restore(_))
        });
        assert_eq!(entries, vec![Entry::Reconnect(1), Entry::Restore(3)]);
        replace_restore_entries(&mut entries, Vec::new(), |entry| {
            matches!(entry, Entry::Restore(_))
        });
        assert_eq!(entries, vec![Entry::Reconnect(1)]);
    }

    #[test]
    fn restore_busy_preserves_admitted_work_and_allows_first_reopen_load() {
        assert!(!restore_busy(true, true, false));
        assert!(restore_busy(true, true, true));
        assert!(restore_busy(true, false, false));
        assert!(!restore_busy(false, false, false));
    }

    #[test]
    fn capture_duplicate_uses_live_overlay_from_another_workspace() {
        use multi_launcher::window_catalog::{
            WindowDescriptor, WindowNativeIdentity, WindowSwitchIdentity, WindowTargetIdentity,
        };
        let first = MmWorkspace {
            id: "first".into(),
            ..Default::default()
        };
        let second = MmWorkspace {
            id: "second".into(),
            ..Default::default()
        };
        let descriptor = WindowDescriptor {
            title: "Target".into(),
            hwnd: 42,
            pid: 9,
            executable: None,
            process_path: None,
            class_name: None,
        };
        let native = WindowNativeIdentity {
            hwnd: 42,
            pid: 9,
            tid: 3,
        };
        let identity = WindowSwitchIdentity {
            native,
            descriptor: WindowTargetIdentity::from_descriptor(&descriptor),
        };
        let overlay = PlacementOverlay {
            index: 0,
            revision: 4,
            expected: first.clone(),
            source: OverlaySource::Reconnect(
                multi_launcher::multi_manager::checked::ReconnectedWorkspace {
                    workspace: first.clone(),
                    identities: [(0, identity)].into_iter().collect(),
                    summary: Default::default(),
                },
            ),
        };
        let mut empty = overlay.clone();
        if let OverlaySource::Reconnect(ref mut reconnect) = empty.source {
            reconnect.identities.clear();
        }
        let choices = vec![overlay.clone(), empty];
        assert!(std::ptr::eq(
            select_overlay(&choices, 0, &first, 4).unwrap(),
            &choices[0]
        ));
        assert!(select_overlay(&choices[1..], 0, &first, 4).is_none());
        let rows = vec![first, second];
        assert!(overlay_has_live_identity(
            std::slice::from_ref(&overlay),
            &rows,
            4,
            native,
            |_| true
        ));
        assert!(!overlay_has_live_identity(
            std::slice::from_ref(&overlay),
            &rows,
            5,
            native,
            |_| true
        ));
        assert!(!overlay_has_live_identity(
            std::slice::from_ref(&overlay),
            &rows,
            4,
            native,
            |_| false
        ));
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
