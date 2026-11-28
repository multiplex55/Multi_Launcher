//! Retained, read-only File Lock Inspector.
mod actions;
use super::*;
use gpui_kit::base::Selectable;
use multi_launcher::file_lock::{FileLockSession, FileLockSessionStatus};
use multi_launcher::plugin::Plugin;
use std::path::PathBuf;

pub(super) const WAIT: &str = "Wait for the File Lock inspection to finish.";

#[derive(Clone)]
pub(super) struct ColdOrigin {
    pub(super) host: AnyWindowHandle,
    pub(super) native: NativeWindow,
    pub(super) focus: FocusHandle,
    pub(super) revision: u64,
}

pub(super) fn current_provider(query: &str, action: &Action, enabled: Option<&HashSet<String>>,
    capabilities: Option<&HashMap<String, Vec<String>>>) -> bool {
    enabled.is_none_or(|set| set.contains("file_lock"))
        && multi_launcher::settings::provider_search_capability_enabled(capabilities, "file_lock")
        && action.args.is_none()
        && matches!(parse_action(action), Ok(Command::Dialog(DialogCommand::FileLock { path: None })))
        && super::search_providers::search_routed(
            [&multi_launcher::plugins::file_lock::FileLockPlugin as &dyn Plugin], query, enabled, capabilities)
            .into_iter().any(|(name, row)| name == "file_lock" && row == *action)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProcessKey { generation: u64, pid: u32, created: u64 }

fn attempt_current(mount: &utility_mount::MountReceipt, issued: u64, current: u64, open: bool, host: bool) -> bool {
    mount.is_current() && issued == current && open && host
}
fn submission_current(submitted: Option<&(PathBuf, u64)>, selected: Option<&PathBuf>, generation: u64) -> bool {
    submitted.is_some_and(|(path, issued)| selected == Some(path) && *issued == generation)
}
fn prefill_changes_file(current: Option<&PathBuf>, incoming: &PathBuf) -> bool { current != Some(incoming) }
fn apply_file_prefill(selected: &mut Option<PathBuf>, submitted: &mut Option<(PathBuf, u64)>,
    process: &mut Option<ProcessKey>, incoming: PathBuf, cancel: impl FnOnce()) -> bool {
    if !prefill_changes_file(selected.as_ref(), &incoming) { return false; }
    cancel();
    *selected = Some(incoming); *submitted = None; *process = None;
    true
}
fn inherited_prefers_popout(source_popout: bool, fresh: bool, remembered: bool) -> bool {
    source_popout || (fresh && remembered)
}
fn rehost_allowed(open: bool, rehosting: bool, picker: bool, fenced: bool, popout: bool, target: bool,
    mount: &utility_mount::MountReceipt) -> bool {
    open && !rehosting && !picker && !fenced && popout != target && mount.is_current()
}
fn key_may_close(action_pending: bool, held: bool) -> bool { !action_pending && !held }

fn process_start(filetime: u64) -> String {
    let seconds = (filetime / 10_000_000) as i64 - 11_644_473_600;
    let nanos = ((filetime % 10_000_000) * 100) as u32;
    chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, nanos)
        .map(|time| time.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S %Z").to_string())
        .unwrap_or_else(|| "time unavailable".into())
}

pub(super) struct FileLockView {
    pub(super) open: bool,
    pub(super) root: WeakEntity<RootWindow>,
    host: AnyWindowHandle,
    pub(super) native: Option<NativeWindow>,
    visibility: RootVisibility,
    pub(super) admission: Arc<RestartAdmission>,
    owner: utility_mount::MountOwner,
    pub(super) popout: bool,
    pub(super) rehosting: bool,
    pub(super) staging: bool,
    pub(super) presentation_error: Option<String>,
    pub(super) geometry_error: Option<String>,
    pub(super) mount: utility_mount::MountReceipt,
    session: FileLockSession,
    wake: Arc<HashWakeSignal>,
    selected_file: Option<PathBuf>,
    submitted: Option<(PathBuf, u64)>,
    selected: Option<ProcessKey>,
    picker: bool,
    picker_attempt: u64,
    permit: Option<EffectPermit>,
    action_lease: Option<Arc<AtomicBool>>,
    action_pending: bool,
    action_blur: Option<Subscription>,
    action_activation: Option<Subscription>,
    action_escape_waiting: bool,
    copy_focus: FocusHandle,
    reveal_focus: FocusHandle,
    focus: FocusHandle,
    pub(super) choose_focus: FocusHandle,
    rows_scroll: ScrollHandle,
    message: Option<String>,
}

impl FileLockView {
    fn new(window: &mut Window, cx: &mut Context<Self>, root: WeakEntity<RootWindow>,
        native: Option<NativeWindow>, visibility: RootVisibility, admission: Arc<RestartAdmission>) -> Self {
        let wake = Arc::new(HashWakeSignal::default());
        let worker_wake = wake.clone();
        let session = FileLockSession::new(move || worker_wake.notify());
        let owner = utility_mount::MountOwner::default(); let mount = owner.attach();
        let receiver = wake.clone(); let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            while receiver.wait().await {
                if weak.update(cx, |view, cx| {
                    if !view.session.view().is_busy() {
                        view.permit = None;
                        let root = view.root.clone();
                        cx.spawn(async move |_, cx| { let _ = root.update(cx, |root, cx| {
                            if root.launcher_search_error.as_deref() == Some(WAIT) { root.launcher_search_error = None; }
                            if root.restart_status.as_deref() == Some(WAIT) { root.restart_status = None; }
                            cx.notify();
                        }); }).detach();
                    }
                    cx.notify();
                }).is_err() { break; }
            }
        }).detach();
        Self { open: true, root, host: window.window_handle(), native, visibility, admission,
            owner, mount, popout: false, rehosting: false, staging: false, presentation_error: None, geometry_error: None, session, wake, selected_file: None, submitted: None, selected: None, picker: false,
            picker_attempt: 0, permit: None, action_lease: None, action_pending: false, action_blur: None, action_activation: None, action_escape_waiting: false,
            copy_focus: cx.focus_handle(), reveal_focus: cx.focus_handle(),
            focus: cx.focus_handle(), choose_focus: cx.focus_handle(), rows_scroll: ScrollHandle::new(), message: None }
    }
    pub(super) fn host_close_blocked(&self) -> bool { self.picker || self.rehosting || self.action_pending }
    pub(super) fn is_mounted_in(&self, id: WindowId, popout: bool) -> bool { self.open && (!self.staging || popout) && self.popout == popout && self.mount.is_current() && self.host.window_id() == id }
    pub(super) fn replace_mount(&mut self, host: AnyWindowHandle, native: Option<NativeWindow>, cx: &mut Context<Self>) {
        self.mount = self.owner.attach(); self.host = host; self.native = native;
        self.focus = cx.focus_handle(); self.choose_focus = cx.focus_handle();
        self.copy_focus = cx.focus_handle(); self.reveal_focus = cx.focus_handle();
    }
    pub(super) fn focus_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_mounted_in(window.window_handle().window_id(), self.popout) { self.choose_focus.focus(window, cx); }
    }
    pub(super) fn prefill(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let session = &self.session;
        apply_file_prefill(&mut self.selected_file, &mut self.submitted, &mut self.selected, path, || { let _ = session.cancel(); });
        self.message = None; cx.notify();
    }
    fn request_rehost(&mut self, target: bool, cx: &mut Context<Self>) {
        if self.action_pending || !rehost_allowed(self.open, self.rehosting, self.picker, self.admission.is_fenced(), self.popout, target, &self.mount) { return; }
        let Some(root) = self.root.upgrade() else { return; };
        let revision = root.read(cx).root_visibility.request_revision.load(Ordering::Acquire);
        let gesture = self.native.map(|source| hash_dock::Gesture { source, foreground_authorized: native_window_is_valid(source) && current_foreground_identity() == Some(source.identity), visibility_revision: revision });
        self.rehosting = true; let mount = self.mount.clone(); let root = self.root.clone();
        cx.spawn(async move |_, cx| { let _ = root.update(cx, |root, cx| root.start_file_lock_rehost(target, gesture, mount, cx)); }).detach(); cx.notify();
    }
    fn request_cold_rehost(&mut self, origin: ColdOrigin, cx: &mut Context<Self>) {
        if self.action_pending || !rehost_allowed(self.open, self.rehosting, self.picker, self.admission.is_fenced(), self.popout, true, &self.mount) { return; }
        self.rehosting = true; let mount = self.mount.clone(); let root = self.root.clone();
        cx.spawn(async move |_, cx| { let _ = root.update(cx, |root, cx| root.start_file_lock_cold_rehost(mount, origin, cx)); }).detach();
        cx.notify();
    }
    pub(super) fn busy(&self) -> bool { self.picker || self.action_pending || self.session.view().is_busy() }
    pub(super) fn owns_focus(&self, window: &Window, cx: &App) -> bool { self.open && self.focus.contains_focused(window, cx) }
    pub(super) fn shutdown(&mut self) {
        if let Some(lease) = &self.action_lease { lease.store(false, Ordering::Release); }
        self.owner.retire(); self.picker_attempt = self.picker_attempt.wrapping_add(1);
        self.open = false; self.rehosting = false; self.staging = false; self.selected = None; self.session.close();
    }
    fn input_owner_current(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.open && !self.rehosting && self.mount.is_current() && !self.admission.is_fenced()
            && self.host.window_id() == window.window_handle().window_id()
            && window.is_window_active() && !window.has_active_dialog(cx)
            && self.native.is_some_and(|native| current_foreground_identity() == Some(native.identity)
                && json_utility::native_host_current(&self.visibility, self.visibility.request_revision.load(Ordering::Acquire), Some(native), self.popout))
            && (self.popout || self.root.upgrade().is_some_and(|root| root.read(cx).external_owner_is_current(window.focused(cx).as_ref(), window, cx)))
    }
    fn current(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        !self.action_pending && self.input_owner_current(window, cx)
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker || self.action_pending { self.message = Some(WAIT.into()); cx.notify(); return; }
        if !self.current(window, cx) { return; }
        if self.popout { let _ = self.root.update(cx, |root, cx| root.drain_file_lock_geometry(cx, true)); self.shutdown(); window.remove_window(); return; }
        let owns = self.owns_focus(window, cx); let focus = self.focus.clone();
        let root = self.root.clone(); let owner = cx.entity_id(); let native = self.native;
        let revision = self.visibility.request_revision.load(Ordering::Acquire);
        self.shutdown();
        if owns { window.defer(cx, move |window, app| { let _ = root.update(app, |root, cx| {
            let valid = root.file_lock_view.as_ref().is_some_and(|view| view.entity_id() == owner && !view.read(cx).open)
                && root.root_window.is_some_and(|host| host.window_id() == window.window_handle().window_id())
                && native.is_some_and(|native| root.native_window == Some(native)
                    && current_foreground_identity() == Some(native.identity)
                    && json_utility::native_host_current(&root.root_visibility, revision, Some(native), false))
                && window.is_window_active() && !window.has_active_dialog(cx)
                && root.action_sheet.is_none() && root.context_favorite_popup.is_none()
                && !context_stack_has_primary(&window.context_stack(), "PopupMenu")
                && window.focused(cx).is_none_or(|current| focus.contains(&current, window));
            if valid { root.query.focus_handle(cx).focus(window, cx); root.root_scroll.scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM); }
            cx.notify();
        }); }); }
        cx.notify();
    }
    pub(super) fn handle_escape(&mut self, window: &mut Window, cx: &mut Context<Self>, held: bool) -> bool {
        if !self.owns_focus(window, cx) || !self.input_owner_current(window, cx) { return false; }
        if !held { self.action_escape_waiting = self.action_pending; }
        if key_may_close(self.action_pending, held) { self.close(window, cx); } true
    }
    fn choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker || !self.current(window, cx) { return; }
        let Some(native) = checked_active_dialog_owner(window).ok().flatten().filter(|owner| Some(*owner) == self.native) else {
            self.message = Some("Activate the launcher before choosing a file.".into()); cx.notify(); return;
        };
        let revision = self.visibility.request_revision.load(Ordering::Acquire);
        if !json_utility::native_host_current(&self.visibility, revision, Some(native), self.popout) { return; }
        let popup = self.popout;
        let Some(permit) = self.admission.admit() else { return; };
        self.picker_attempt = self.picker_attempt.wrapping_add(1).max(1);
        let attempt = self.picker_attempt; let mount = self.mount.clone(); let worker_mount = mount.clone();
        let visibility = self.visibility.clone(); let admission = self.admission.clone();
        let picker = rfd::FileDialog::new().set_parent(&*window);
        self.picker = true; self.message = None;
        let task = cx.background_executor().spawn(async move {
            if !worker_mount.is_current() || admission.is_fenced() || current_foreground_identity() != Some(native.identity)
                || !json_utility::native_host_current(&visibility, revision, Some(native), popup) { return None; }
            Some(picker.pick_file())
        });
        let weak = cx.entity().downgrade(); let host = self.host;
        cx.spawn(async move |_, cx| {
            let _permit = permit; let selected = task.await.flatten();
            let Some(view) = weak.upgrade() else { return; };
            let _ = update_kit_window_content(host, &view, cx, |view, window, cx| {
                if view.picker_attempt != attempt { return; }
                view.picker = false;
                let current = attempt_current(&mount, attempt, view.picker_attempt, view.open && !view.admission.is_fenced(),
                    view.host.window_id() == window.window_handle().window_id() && view.native == Some(native)
                    && view.popout == popup && json_utility::native_host_current(&view.visibility, revision, Some(native), popup));
                if current && let Some(path) = selected {
                    match multi_launcher::file_lock::validate_file_path(&path) {
                        Ok(()) => {
                            if view.selected_file.as_ref() != Some(&path) {
                                let _ = view.session.cancel(); view.selected = None; view.submitted = None;
                            }
                            view.selected_file = Some(path); view.message = None;
                        }
                        Err(error) => view.message = Some(format!("Cannot use this file: {error}")),
                    }
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
    fn inspect(&mut self, path: PathBuf, generation: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker || !self.current(window, cx) || self.selected_file.as_ref() != Some(&path)
            || self.session.view().generation() != generation || self.session.view().is_busy() { return; }
        if let Err(error) = multi_launcher::file_lock::validate_file_path(&path) {
            self.message = Some(format!("Cannot use this file: {error}")); cx.notify(); return;
        }
        let Some(permit) = self.admission.admit() else { return; };
        match self.session.refresh(path.clone()) {
            Ok(generation) => { self.submitted = Some((path, generation)); self.permit = Some(permit); self.selected = None; self.message = None; }
            Err(error) => self.message = Some(format!("Could not start inspection: {error}")),
        }
        cx.notify();
    }
}
impl Drop for FileLockView {
    fn drop(&mut self) { self.session.close(); self.wake.close(); }
}

impl Render for FileLockView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.session.view(); let generation = view.generation();
        let submitted = submission_current(self.submitted.as_ref(), self.selected_file.as_ref(), generation);
        let mount = self.mount.clone(); let picker_attempt = self.picker_attempt;
        let path = self.selected_file.clone();
        let target = !self.popout; let rehost_mount = self.mount.clone();
        let shortcut_mount = self.mount.clone();
        let mut panel = div().id("file-lock-pane").key_context("FileLockPane").track_focus(&self.focus)
            .capture_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                if !window.default_prevented() && shortcut_mount.is_current() && shortcut_mount.matches(&view.mount)
                    && event.keystroke.key == "w" && event.keystroke.modifiers == (Modifiers { control: true, ..Modifiers::default() })
                    && view.owns_focus(window, cx) && view.input_owner_current(window, cx) {
                    if key_may_close(view.action_pending, event.is_held) { view.close(window, cx); }
                    window.prevent_default(); cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|view, _: &gpui_kit::base::input::Escape, window, cx| {
                if view.owns_focus(window, cx) && view.popout {
                    if !view.action_escape_waiting { view.handle_escape(window, cx, false); }
                    cx.stop_propagation();
                } else if view.owns_focus(window, cx) {
                    let root = view.root.clone();
                    let latched = root.update(cx, |root, _| { let was = root.pane_escape_latched; root.pane_escape_latched = true; was }).unwrap_or(true);
                    if !latched { view.handle_escape(window, cx, false); }
                    cx.stop_propagation();
                }
            }))
            .capture_key_up(cx.listener(|view, event: &gpui_kit::KeyUpEvent, _, _| {
                if event.keystroke.key == "escape" { view.action_escape_waiting = false; }
            })).flex().flex_col().gap_2().w_full().p_3()
            .child(Label::new("File Lock Inspector"))
            .child(Button::new("file-lock-rehost").label(if self.popout { "Dock" } else { "Pop Out" }).disabled(self.picker || self.rehosting || self.action_pending)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if rehost_mount.is_current() && rehost_mount.matches(&view.mount) && view.current(window, cx) { view.request_rehost(target, cx); }
                })))
            .child(Label::new(self.selected_file.as_ref().map_or_else(|| "No file selected".into(), |path| path.display().to_string())))
            .child(div().flex().flex_wrap().gap_2()
                .child(Button::new("file-lock-choose").label("Choose file").tab_stop(true).track_focus(&self.choose_focus).disabled(self.picker || self.action_pending)
                    .on_click({ let mount = mount.clone(); cx.listener(move |view, _, window, cx| {
                        if mount.is_current() && mount.matches(&view.mount) && view.picker_attempt == picker_attempt { view.choose(window, cx); }
                    }) }))
                .child(Button::new("file-lock-inspect").label(if submitted && view.status() == FileLockSessionStatus::Complete { "Refresh" } else { "Inspect" })
                    .disabled(self.picker || self.action_pending || view.is_busy() || path.is_none())
                    .on_click({ let mount = mount.clone(); cx.listener(move |view, _, window, cx| {
                        if mount.is_current() && mount.matches(&view.mount) && let Some(path) = path.clone() { view.inspect(path, generation, window, cx); }
                    }) }))
                .child(Button::new("file-lock-cancel").label("Cancel").disabled(!view.is_busy() || self.action_pending)
                    .on_click({ let mount = mount.clone(); cx.listener(move |view, _, window, cx| {
                        if mount.is_current() && mount.matches(&view.mount) && view.session.view().generation() == generation && view.current(window, cx) {
                            if view.session.cancel().unwrap_or(false) {
                                let next_generation = view.session.view().generation();
                                if let Some((path, issued)) = view.submitted.as_mut() {
                                    if view.selected_file.as_ref() == Some(path) && *issued == generation { *issued = next_generation; }
                                }
                            }
                            view.selected = None; cx.notify();
                        }
                    }) }))
                .child(Button::new("file-lock-close").label("Close").on_click({ let mount = mount.clone(); cx.listener(move |view, _, window, cx| {
                    if mount.is_current() && mount.matches(&view.mount) { view.close(window, cx); }
                }) })));
        if let Some(error) = self.presentation_error.as_ref().or(self.geometry_error.as_ref()) { panel = panel.child(Label::new(error.clone())); }
        if let Some(message) = &self.message { panel = panel.child(Label::new(message.clone())); }
        panel = panel.child(Label::new(if !submitted {
            if self.selected_file.is_some() { "Select Inspect to check this file.".to_owned() }
            else { "Choose a file, then select Inspect.".to_owned() }
        } else { match view.status() {
            FileLockSessionStatus::Idle => "Choose a file, then select Inspect.".to_owned(),
            FileLockSessionStatus::Inspecting | FileLockSessionStatus::RefreshQueued => "Inspecting file users…".to_owned(),
            FileLockSessionStatus::Cancelled => "Inspection cancelled.".to_owned(),
            FileLockSessionStatus::Failed => view.inspection_error().map(ToString::to_string)
                .or_else(|| view.worker_error().map(ToString::to_string)).unwrap_or_else(|| "Inspection failed.".into()),
            FileLockSessionStatus::Complete => "Inspection complete.".to_owned(),
            FileLockSessionStatus::Closed => "Inspector closed.".to_owned(),
        }}));
        if let Some(inspection) = view.inspection().filter(|inspection| submitted && view.inspection_is_current()
            && self.selected_file.as_ref() == Some(&inspection.path)) {
            panel = panel.child(actions::toolbar(self, inspection.clone(), generation, cx));
            if inspection.reports_no_processes() { panel = panel.child(Label::new("Windows reported no processes using this file.")); }
            let mut rows = div().id("file-lock-processes").flex().flex_col().gap_1().max_h(px(300.0)).overflow_y_scroll().track_scroll(&self.rows_scroll);
            for process in inspection.processes.iter().take(multi_launcher::file_lock::MAX_REPORTED_FILE_USERS) {
                let key = ProcessKey { generation, pid: process.process_id, created: process.creation_filetime };
                let label = if !process.application_name.is_empty() { process.application_name.clone() }
                    else if !process.service_name.is_empty() { process.service_name.clone() } else { "Process details unavailable".into() };
                let executable = process.executable_path.as_ref().map_or_else(|| "Executable provenance unavailable".into(), |path| path.display().to_string());
                let text = format!("PID {} · {} · Started {} · FILETIME {} · {}", key.pid, label, process_start(key.created), key.created, executable);
                let issued = mount.clone();
                rows = rows.child(Button::new(SharedString::from(format!("file-lock-process-{}-{}", key.pid, key.created)))
                    .label(text).selected(self.selected == Some(key)).on_click(cx.listener(move |view, _, window, cx| {
                        if issued.is_current() && issued.matches(&view.mount) && view.current(window, cx) {
                            let snapshot = view.session.view();
                            if submission_current(view.submitted.as_ref(), view.selected_file.as_ref(), key.generation)
                                && snapshot.generation() == key.generation && snapshot.inspection_is_current()
                                && snapshot.inspection().is_some_and(|result| view.selected_file.as_ref() == Some(&result.path))
                                && snapshot.inspection().is_some_and(|result| result.processes.iter().any(|process|
                                    process.process_id == key.pid && process.creation_filetime == key.created)) {
                                view.selected = Some(key); cx.notify();
                            }
                        }
                    })));
            }
            panel = panel.child(rows);
        }
        panel
    }
}

impl RootWindow {
    pub(super) fn open_file_lock_from_file_search(&mut self, source: WeakEntity<super::file_search::FileSearchView>,
        receipt: super::file_search::SelectionReceipt, window: &mut Window, cx: &mut Context<Self>) {
        use super::file_search_handoff::{FileSearchOrigin, HandoffTarget};
        let Some(source) = source.upgrade() else { return; };
        let dialog = window.has_active_dialog(cx);
        let contexts = window.context_stack();
        let state = source.read(cx); let popout = state.is_mounted_in(window.window_handle().window_id(), true);
        if self.restart_admission.is_fenced() || self.action_sheet.is_some() || self.context_favorite_popup.is_some()
            || dialog || context_stack_has_primary(&contexts, "PopupMenu") || context_stack_has_primary(&contexts, "Dialog")
            || context_stack_has_primary(&contexts, "AlertDialog") || !window.is_window_active() || state.rehosting
            || (!popout && !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx))
            || !self.file_search.as_ref().is_some_and(|view| view.entity_id() == receipt.owner)
            || source.entity_id() != receipt.owner || !state.is_mounted_in(window.window_handle().window_id(), popout)
            || !state.lock_action_is_focused(window) || !state.eligible_lock_receipt(&receipt, source.entity_id()) { return; }
        let Some(native) = state.host_native() else { return; };
        if !native_window_is_valid(native) || current_foreground_identity() != Some(native.identity) { return; }
        if !popout && !json_utility::native_host_current(&self.root_visibility,
            self.root_visibility.request_revision.load(Ordering::Acquire), Some(native), false) { return; }
        if let Err(error) = multi_launcher::file_lock::validate_file_path(&receipt.path) {
            source.update(cx, |view, cx| view.report_file_lock_error(&error.to_string(), cx)); return;
        }
        let fresh = self.file_lock_view.as_ref().is_none_or(|view| !view.read(cx).open);
        if fresh && self.file_lock_retiring.iter().any(|window| window.native_window.is_none_or(native_window_is_valid)) {
            source.update(cx, |view, cx| view.report_file_lock_error("The previous inspector window is closing.", cx)); return;
        }
        if self.file_lock_view.as_ref().is_some_and(|view| view.read(cx).host_close_blocked() || (fresh && view.read(cx).busy())) {
            source.update(cx, |view, cx| view.report_file_lock_error("The inspector is busy.", cx)); return;
        }
        let origin = FileSearchOrigin { target: HandoffTarget::FileLock(receipt.path.clone()), receipt, mount: state.mount_receipt(),
            host: window.window_handle(), native, focus: state.lock_action_focus(), popout,
            root_revision: self.root_visibility.request_revision.load(Ordering::Acquire), fresh };
        let Some(permit) = self.restart_admission.admit() else { return; };
        if fresh && self.file_lock_popup.is_some() {
            source.update(cx, |view, cx| view.report_file_lock_error("The previous inspector window is closing.", cx)); return;
        }
        if self.file_lock_popup.as_ref().is_some_and(|popup| popup.native.is_none_or(|target| !checked_hash_popup_visible(target))) {
            source.update(cx, |view, cx| view.report_file_lock_error("Restore the inspector window and retry.", cx)); return;
        }
        let view = if fresh {
            let retained = self.file_lock_view.as_ref().and_then(|view| view.read(cx).selected_file.clone());
            let root = cx.entity().downgrade(); let visibility = self.root_visibility.clone(); let admission = self.restart_admission.clone();
            let view = cx.new(|cx| FileLockView::new(window, cx, root, Some(native), visibility, admission));
            view.update(cx, |view, _| view.selected_file = retained);
            self.file_lock_view = Some(view.clone()); view
        } else { self.file_lock_view.as_ref().unwrap().clone() };
        if self.file_lock_popup.is_some() { self.activate_inherited_file_lock_popup(origin, permit, cx); return; }
        let remembered = multi_launcher::settings::UtilityToolKey::parse("file-lock").ok().is_some_and(|key|
            self.utility_presentations.preference(key).unwrap_or(multi_launcher::settings::RememberedPresentation::Docked)
                == multi_launcher::settings::RememberedPresentation::Popout);
        if inherited_prefers_popout(popout, fresh, remembered) {
            let mount = view.update(cx, |view, _| { view.rehosting = true; view.staging = fresh; view.mount.clone() });
            self.start_inherited_file_lock_rehost(mount, origin, permit, cx);
        } else if let HandoffTarget::FileLock(path) = &origin.target {
            view.update(cx, |view, cx| { view.prefill(path.clone(), cx); view.focus_host(window, cx); });
            source.update(cx, |view, cx| view.clear_file_lock_error(&origin.receipt, cx));
            self.reveal_file_lock(window, cx); cx.notify();
        }
    }
    pub(super) fn open_file_lock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced() || window.has_active_dialog(cx) || !window.is_window_active()
            || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            || !self.native_window.is_some_and(|native| current_foreground_identity() == Some(native.identity)) { return; }
        if self.file_lock_popup.is_some() { self.reveal_file_lock_popup(window, cx); return; }
        if self.file_lock_retiring.iter().any(|window| window.native_window.is_none_or(native_window_is_valid)) { self.launcher_search_error = Some(WAIT.into()); cx.notify(); return; }
        if let Some(existing) = &self.file_lock_view {
            if existing.read(cx).rehosting { return; }
            if existing.read(cx).open {
                existing.update(cx, |view, cx| { view.choose_focus.focus(window, cx); cx.notify(); });
                self.reveal_file_lock(window, cx); cx.notify(); return;
            }
            if existing.read(cx).busy() { self.launcher_search_error = Some(WAIT.into()); cx.notify(); return; }
        }
        let retained_path = self.file_lock_view.as_ref().and_then(|view| view.read(cx).selected_file.clone());
        let root = cx.entity().downgrade(); let native = self.native_window;
        let visibility = self.root_visibility.clone(); let admission = self.restart_admission.clone();
        let view = cx.new(|cx| FileLockView::new(window, cx, root, native, visibility, admission));
        view.update(cx, |view, cx| { view.selected_file = retained_path; view.choose_focus.focus(window, cx); });
        self.file_lock_view = Some(view.clone()); self.query_history.reset(); self.reveal_file_lock(window, cx);
        let prefers_popup = multi_launcher::settings::UtilityToolKey::parse("file-lock").ok().is_some_and(|key|
            self.utility_presentations.preference(key).unwrap_or(multi_launcher::settings::RememberedPresentation::Docked)
                == multi_launcher::settings::RememberedPresentation::Popout);
        if prefers_popup && let Some(native) = native {
            let origin = ColdOrigin { host: window.window_handle(), native, focus: view.read(cx).choose_focus.clone(),
                revision: self.root_visibility.request_revision.load(Ordering::Acquire) };
            let weak = view.downgrade();
            cx.spawn(async move |_, cx| { let _ = weak.update(cx, |view, cx| view.request_cold_rehost(origin, cx)); }).detach();
        }
        cx.notify();
    }
    fn file_lock_reveal_origin_current(&self, focus: &FocusHandle, host: AnyWindowHandle, require_active: bool, cx: &mut Context<Self>) -> bool {
        if self.root_window.is_none_or(|window| window.window_id() != host.window_id()) || self.restart_admission.is_fenced() { return false; }
        host.update(cx, |_, window, app| (!require_active || window.is_window_active()) && !window.has_active_dialog(app)
            && window.focused(app).as_ref() == Some(focus)
            && self.external_owner_is_current(Some(focus), window, app)).unwrap_or(false)
    }
    pub(super) fn file_lock_cold_origin_current(&self, origin: &ColdOrigin, cx: &mut Context<Self>) -> bool {
        self.native_window == Some(origin.native)
            && current_foreground_identity() == Some(origin.native.identity)
            && json_utility::native_host_current(&self.root_visibility, origin.revision, Some(origin.native), false)
            && self.file_lock_reveal_origin_current(&origin.focus, origin.host, true, cx)
    }
    pub(super) fn file_lock_cold_origin_retained(&self, origin: &ColdOrigin, cx: &mut Context<Self>) -> bool {
        self.native_window == Some(origin.native)
            && json_utility::native_host_current(&self.root_visibility, origin.revision, Some(origin.native), false)
            && self.file_lock_reveal_origin_current(&origin.focus, origin.host, false, cx)
    }
    fn reveal_file_lock_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(popup), Some(view), Some(source)) = (&self.file_lock_popup, &self.file_lock_view, self.native_window) else { return; };
        let Some(native) = popup.native else { return; };
        if view.read(cx).rehosting || !view.read(cx).is_mounted_in(popup.window.window_id(), true)
            || !checked_hash_popup_visible(native) || current_foreground_identity() != Some(source.identity) { return; }
        let Some(permit) = self.restart_admission.admit() else { return; };
        let Some(focus) = window.focused(cx) else { return; }; let origin_host = window.window_handle();
        let host = popup.window; let view = view.clone(); let mount = view.read(cx).mount.clone();
        let worker_mount = mount.clone(); let visibility = self.root_visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire); let admission = self.restart_admission.clone();
        let root = cx.entity().downgrade(); let guard = Arc::new(AtomicU64::new(1));
        cx.spawn(async move |_, cx| {
            let _permit = permit;
            let current = root.update(cx, |root, cx| root.file_lock_reveal_origin_current(&focus, origin_host, true, cx)).unwrap_or(false);
            if !current || current_foreground_identity() != Some(source.identity) { return; }
            let accepted = activate_for_secondary_window_if(&guard, 1, native, Some(source.identity), &||
                worker_mount.is_current() && !admission.is_fenced() && json_utility::native_host_current(&visibility, revision, Some(source), false));
            if !accepted || !checked_hash_popup_visible(native) || current_foreground_identity() != Some(native.identity)
                || !mount.is_current() || !json_utility::native_host_current(&visibility, revision, Some(source), false) { return; }
            let current = root.update(cx, |root, cx| root.file_lock_popup.as_ref().is_some_and(|popup| popup.window.window_id() == host.window_id() && popup.native == Some(native))
                && root.file_lock_view.as_ref().is_some_and(|v| v.entity_id() == view.entity_id()) && !root.restart_admission.is_fenced()
                && view.read(cx).is_mounted_in(host.window_id(), true) && root.file_lock_reveal_origin_current(&focus, origin_host, false, cx)).unwrap_or(false);
            if current { let _ = update_kit_window_content(host, &view, cx, |view, window, cx| {
                if mount.is_current() && view.is_mounted_in(window.window_handle().window_id(), true)
                    && current_foreground_identity() == Some(native.identity) && !window.has_active_dialog(cx) { view.focus_host(window, cx); cx.notify(); }
            }); }
        }).detach();
    }
    pub(super) fn reveal_file_lock(&self, window: &Window, cx: &App) {
        let id = window.window_handle().window_id();
        let history = self.clipboard_history.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let modify = self.clipboard_modify_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let snippets = self.snippets_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let fill = self.snippet_fill.as_ref().is_some_and(|view| view.read(cx).open);
        let tempfile = self.tempfile_view.as_ref().is_some_and(|view| view.read(cx).open);
        let folder = self.folder_size_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        self.root_scroll.scroll_to_top_of_item(self.clipboard_scroll_item(id, cx)
            + usize::from(history) + usize::from(modify) + usize::from(snippets) + usize::from(fill) + usize::from(tempfile) + usize::from(folder));
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_file_prefill, attempt_current, current_provider, inherited_prefers_popout, key_may_close, prefill_changes_file, rehost_allowed, submission_current, ProcessKey};
    use multi_launcher::plugin::Plugin;
    #[test]
    fn exact_provider_and_stale_picker_receipts() {
        let row = multi_launcher::plugins::file_lock::FileLockPlugin.search("FileLock").pop().unwrap();
        assert!(current_provider("FileLock", &row, None, None));
        assert!(current_provider("kind:filelock filelock", &row, None, None));
        assert!(!current_provider("filelock extra", &row, None, None));
        let mut forged = row.clone(); forged.args = Some("file".into());
        assert!(!current_provider("filelock", &forged, None, None));
        let owner = super::super::utility_mount::MountOwner::default(); let mount = owner.attach();
        assert!(attempt_current(&mount, 1, 1, true, true));
        assert!(!attempt_current(&mount, 1, 2, true, true));
        owner.retire(); assert!(!attempt_current(&mount, 1, 1, true, true));
    }
    #[test]
    fn process_identity_requires_generation_and_creation_time() {
        let row = ProcessKey { generation: 3, pid: 40, created: 55 };
        assert_ne!(row, ProcessKey { generation: 4, ..row });
        assert_ne!(row, ProcessKey { created: 56, ..row });
    }
    #[test]
    fn changed_file_and_new_generation_cannot_reuse_old_status() {
        let first = std::env::temp_dir().join("first");
        let second = std::env::temp_dir().join("second");
        let submitted = (first.clone(), 7);
        assert!(submission_current(Some(&submitted), Some(&first), 7));
        assert!(!submission_current(Some(&submitted), Some(&second), 7));
        assert!(!submission_current(Some(&submitted), Some(&first), 8));
        assert!(!submission_current(None, Some(&first), 7));
    }
    #[test]
    fn picker_blocks_host_transfer_and_old_mount_is_retired() {
        let owner = super::super::utility_mount::MountOwner::default();
        let old = owner.attach();
        assert!(rehost_allowed(true, false, false, false, false, true, &old));
        assert!(!rehost_allowed(true, false, true, false, false, true, &old));
        let replacement = owner.attach();
        assert!(!rehost_allowed(true, false, false, false, false, true, &old));
        assert!(rehost_allowed(true, false, false, false, true, false, &replacement));
    }
    #[test]
    fn file_prefill_preserves_same_path_and_host_policy() {
        let first = std::env::temp_dir().join("first");
        let next = std::env::temp_dir().join("next");
        assert!(!prefill_changes_file(Some(&first), &first));
        assert!(prefill_changes_file(Some(&first), &next));
        assert!(prefill_changes_file(None, &first));
        assert!(inherited_prefers_popout(true, false, false));
        assert!(inherited_prefers_popout(false, true, true));
        assert!(!inherited_prefers_popout(false, false, true));
    }
    #[test]
    fn accepted_prefill_changes_only_different_file_and_keeps_worker_permit() {
        use std::{cell::Cell, sync::Arc};
        let admission = Arc::new(super::super::RestartAdmission::default());
        let permit = admission.admit().unwrap();
        let first = std::env::temp_dir().join("first");
        let next = std::env::temp_dir().join("next");
        let key = ProcessKey { generation: 4, pid: 5, created: 6 };
        let mut selected = Some(first.clone()); let mut submitted = Some((first.clone(), 4)); let mut process = Some(key);
        let cancels = Cell::new(0);
        assert!(!apply_file_prefill(&mut selected, &mut submitted, &mut process, first.clone(), || cancels.set(cancels.get() + 1)));
        assert_eq!(submitted, Some((first, 4))); assert_eq!(process, Some(key)); assert_eq!(cancels.get(), 0);
        assert!(apply_file_prefill(&mut selected, &mut submitted, &mut process, next.clone(), || cancels.set(cancels.get() + 1)));
        assert_eq!(selected, Some(next)); assert!(submitted.is_none()); assert!(process.is_none()); assert_eq!(cancels.get(), 1);
        assert_eq!(admission.pending_effects(), 1);
        drop(permit);
        assert_eq!(admission.pending_effects(), 0);
    }
    #[test]
    fn close_keys_are_owned_but_cannot_close_during_an_effect_or_repeat() {
        assert!(!key_may_close(true, false));
        assert!(!key_may_close(true, true));
        assert!(!key_may_close(false, true));
        assert!(key_may_close(false, false));
    }
}
