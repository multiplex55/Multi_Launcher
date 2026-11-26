//! Retained, read-only File Lock Inspector.
use super::*;
use gpui_kit::base::Selectable;
use multi_launcher::file_lock::{FileLockSession, FileLockSessionStatus};
use multi_launcher::plugin::Plugin;
use std::path::PathBuf;

pub(super) const WAIT: &str = "Wait for the File Lock inspection to finish.";

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

fn process_start(filetime: u64) -> String {
    let seconds = (filetime / 10_000_000) as i64 - 11_644_473_600;
    let nanos = ((filetime % 10_000_000) * 100) as u32;
    chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, nanos)
        .map(|time| time.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S %Z").to_string())
        .unwrap_or_else(|| "time unavailable".into())
}

pub(super) struct FileLockView {
    pub(super) open: bool,
    root: WeakEntity<RootWindow>,
    host: AnyWindowHandle,
    native: Option<NativeWindow>,
    visibility: RootVisibility,
    admission: Arc<RestartAdmission>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    session: FileLockSession,
    wake: Arc<HashWakeSignal>,
    selected_file: Option<PathBuf>,
    submitted: Option<(PathBuf, u64)>,
    selected: Option<ProcessKey>,
    picker: bool,
    picker_attempt: u64,
    permit: Option<EffectPermit>,
    focus: FocusHandle,
    choose_focus: FocusHandle,
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
            owner, mount, session, wake, selected_file: None, submitted: None, selected: None, picker: false,
            picker_attempt: 0, permit: None, focus: cx.focus_handle(), choose_focus: cx.focus_handle(), message: None }
    }
    pub(super) fn busy(&self) -> bool { self.picker || self.session.view().is_busy() }
    pub(super) fn owns_focus(&self, window: &Window, cx: &App) -> bool { self.open && self.focus.contains_focused(window, cx) }
    pub(super) fn shutdown(&mut self) {
        self.owner.retire(); self.picker_attempt = self.picker_attempt.wrapping_add(1);
        self.open = false; self.selected = None; self.session.close();
    }
    fn current(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.open && self.mount.is_current() && !self.admission.is_fenced()
            && self.host.window_id() == window.window_handle().window_id()
            && window.is_window_active() && !window.has_active_dialog(cx)
            && self.native.is_some_and(|native| current_foreground_identity() == Some(native.identity)
                && json_utility::native_host_current(&self.visibility, self.visibility.request_revision.load(Ordering::Acquire), Some(native), false))
            && self.root.upgrade().is_some_and(|root| root.read(cx).external_owner_is_current(window.focused(cx).as_ref(), window, cx))
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker { self.message = Some(WAIT.into()); cx.notify(); return; }
        if !self.current(window, cx) { return; }
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
        if !self.owns_focus(window, cx) || !self.current(window, cx) { return false; }
        if !held { self.close(window, cx); } true
    }
    fn choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker || !self.current(window, cx) { return; }
        let Some(native) = checked_active_dialog_owner(window).ok().flatten().filter(|owner| Some(*owner) == self.native) else {
            self.message = Some("Activate the launcher before choosing a file.".into()); cx.notify(); return;
        };
        let revision = self.visibility.request_revision.load(Ordering::Acquire);
        if !json_utility::native_host_current(&self.visibility, revision, Some(native), false) { return; }
        let Some(permit) = self.admission.admit() else { return; };
        self.picker_attempt = self.picker_attempt.wrapping_add(1).max(1);
        let attempt = self.picker_attempt; let mount = self.mount.clone(); let worker_mount = mount.clone();
        let visibility = self.visibility.clone(); let admission = self.admission.clone();
        let picker = rfd::FileDialog::new().set_parent(&*window);
        self.picker = true; self.message = None;
        let task = cx.background_executor().spawn(async move {
            if !worker_mount.is_current() || admission.is_fenced() || current_foreground_identity() != Some(native.identity)
                || !json_utility::native_host_current(&visibility, revision, Some(native), false) { return None; }
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
                    && json_utility::native_host_current(&view.visibility, revision, Some(native), false));
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
        let mut panel = div().id("file-lock-pane").key_context("FileLockPane").track_focus(&self.focus)
            .capture_action(cx.listener(|view, _: &gpui_kit::base::input::Escape, window, cx| {
                if view.owns_focus(window, cx) {
                    let root = view.root.clone();
                    let latched = root.update(cx, |root, _| { let was = root.pane_escape_latched; root.pane_escape_latched = true; was }).unwrap_or(true);
                    if !latched { view.handle_escape(window, cx, false); }
                    cx.stop_propagation();
                }
            })).flex().flex_col().gap_2().w_full().p_3()
            .child(Label::new("File Lock Inspector"))
            .child(Label::new(self.selected_file.as_ref().map_or_else(|| "No file selected".into(), |path| path.display().to_string())))
            .child(div().flex().flex_wrap().gap_2()
                .child(Button::new("file-lock-choose").label("Choose file").tab_stop(true).track_focus(&self.choose_focus).disabled(self.picker)
                    .on_click({ let mount = mount.clone(); cx.listener(move |view, _, window, cx| {
                        if mount.is_current() && mount.matches(&view.mount) && view.picker_attempt == picker_attempt { view.choose(window, cx); }
                    }) }))
                .child(Button::new("file-lock-inspect").label(if submitted && view.status() == FileLockSessionStatus::Complete { "Refresh" } else { "Inspect" })
                    .disabled(self.picker || view.is_busy() || path.is_none())
                    .on_click({ let mount = mount.clone(); cx.listener(move |view, _, window, cx| {
                        if mount.is_current() && mount.matches(&view.mount) && let Some(path) = path.clone() { view.inspect(path, generation, window, cx); }
                    }) }))
                .child(Button::new("file-lock-cancel").label("Cancel").disabled(!view.is_busy())
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
            if inspection.reports_no_processes() { panel = panel.child(Label::new("Windows reported no processes using this file.")); }
            let mut rows = div().id("file-lock-processes").flex().flex_col().gap_1().max_h(px(300.0)).overflow_y_scroll();
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
    pub(super) fn open_file_lock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced() || window.has_active_dialog(cx) || !window.is_window_active()
            || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            || !self.native_window.is_some_and(|native| current_foreground_identity() == Some(native.identity)) { return; }
        if let Some(existing) = &self.file_lock_view {
            if existing.read(cx).open {
                existing.update(cx, |view, cx| { view.choose_focus.focus(window, cx); cx.notify(); });
                self.reveal_file_lock(window, cx); cx.notify(); return;
            }
            if existing.read(cx).busy() { self.launcher_search_error = Some(WAIT.into()); cx.notify(); return; }
        }
        let root = cx.entity().downgrade(); let native = self.native_window;
        let visibility = self.root_visibility.clone(); let admission = self.restart_admission.clone();
        let view = cx.new(|cx| FileLockView::new(window, cx, root, native, visibility, admission));
        view.update(cx, |view, cx| view.choose_focus.focus(window, cx));
        self.file_lock_view = Some(view); self.query_history.reset(); self.reveal_file_lock(window, cx); cx.notify();
    }
    fn reveal_file_lock(&self, window: &Window, cx: &App) {
        let id = window.window_handle().window_id();
        let history = self.clipboard_history.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let modify = self.clipboard_modify_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let snippets = self.snippets_view.as_ref().is_some_and(|view| view.read(cx).open);
        let fill = self.snippet_fill.as_ref().is_some_and(|view| view.read(cx).open);
        let tempfile = self.tempfile_view.as_ref().is_some_and(|view| view.read(cx).open);
        let folder = self.folder_size_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        self.root_scroll.scroll_to_top_of_item(self.clipboard_scroll_item(id, cx)
            + usize::from(history) + usize::from(modify) + usize::from(snippets) + usize::from(fill) + usize::from(tempfile) + usize::from(folder));
    }
}

#[cfg(test)]
mod tests {
    use super::{attempt_current, current_provider, submission_current, ProcessKey};
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
}
