//! Retained Folder Size scanner backed by the shared session.
mod report;
use super::*;
use multi_launcher::batch_rename::ExplicitDirectoryRoot;
use multi_launcher::folder_size::{FolderSizeSession, FolderSizeSessionStatus};
use multi_launcher::plugin::Plugin;

pub(super) const WAIT: &str = "Wait for the Folder Size operation to finish.";

pub(super) fn current_provider(query: &str, action: &Action, enabled: Option<&HashSet<String>>,
    capabilities: Option<&HashMap<String, Vec<String>>>) -> bool {
    enabled.is_none_or(|set| set.contains("folder_size"))
        && multi_launcher::settings::provider_search_capability_enabled(capabilities, "folder_size")
        && action.args.is_none() && matches!(parse_action(action), Ok(Command::Dialog(DialogCommand::FolderSize)))
        && super::search_providers::search_routed(
            [&multi_launcher::plugins::folder_size::FolderSizePlugin as &dyn Plugin], query, enabled, capabilities)
            .into_iter().any(|(name, row)| name == "folder_size" && row == *action)
}

fn picker_current(mount: &utility_mount::MountReceipt, attempt: u64, current: u64, open: bool, host: bool) -> bool {
    mount.is_current() && attempt == current && open && host
}

#[derive(Clone, PartialEq, Eq)]
struct ScanDraft { root: ExplicitDirectoryRoot, recursive: bool }
impl ScanDraft {
    fn freeze(root: Option<&ExplicitDirectoryRoot>, recursive: bool) -> Option<Self> {
        Some(Self { root: root?.clone(), recursive })
    }
}

fn scan_receipt_current(issued: &ScanDraft, current: Option<&ScanDraft>, issued_generation: u64, generation: u64) -> bool {
    current == Some(issued) && issued_generation == generation
}

pub(super) struct FolderSizeView {
    pub(super) open: bool,
    root: WeakEntity<RootWindow>,
    host: AnyWindowHandle,
    native: Option<NativeWindow>,
    visibility: RootVisibility,
    admission: Arc<RestartAdmission>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    session: FolderSizeSession,
    wake: Arc<HashWakeSignal>,
    root_choice: Option<ExplicitDirectoryRoot>,
    report_root: Option<ExplicitDirectoryRoot>,
    report_ui: report::ReportUi,
    report_recursive: bool,
    recursive: bool,
    picker: bool,
    picker_attempt: u64,
    scan_permit: Option<EffectPermit>,
    focus: FocusHandle,
    choose_focus: FocusHandle,
    message: Option<String>,
}

impl FolderSizeView {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>, root: WeakEntity<RootWindow>,
        native: Option<NativeWindow>, visibility: RootVisibility, admission: Arc<RestartAdmission>,
        activity: multi_launcher::activity::ActivityCenter) -> Self {
        let wake = Arc::new(HashWakeSignal::default());
        let signal = wake.clone();
        let session = FolderSizeSession::new_with_activity_center(move || signal.notify(), activity);
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let weak = cx.entity().downgrade();
        let receiver = wake.clone();
        cx.spawn(async move |_, cx| {
            while receiver.wait().await {
                if weak.update(cx, |view, cx| {
                    view.sync_report();
                    if !view.session.view().is_busy() { view.scan_permit = None; }
                    cx.notify();
                }).is_err() { break; }
            }
        }).detach();
        Self { open: true, root, host: window.window_handle(), native, visibility, admission, owner, mount,
            session, wake, root_choice: None, report_root: None, report_ui: report::ReportUi::default(), report_recursive: true, recursive: true, picker: false, picker_attempt: 0,
            scan_permit: None, focus: cx.focus_handle(), choose_focus: cx.focus_handle(), message: None }
    }
    pub(super) fn busy(&self) -> bool { self.picker || self.session.view().is_busy() }
    pub(super) fn owns_focus(&self, window: &Window, cx: &App) -> bool { self.open && self.focus.contains_focused(window, cx) }
    pub(super) fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true; self.choose_focus.focus(window, cx); cx.notify();
    }
    pub(super) fn shutdown(&mut self) { self.owner.retire(); self.picker_attempt = self.picker_attempt.wrapping_add(1); self.open = false; self.session.close(); }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker { self.message = Some(WAIT.into()); cx.notify(); return; }
        if !self.open || !self.mount.is_current() || self.host.window_id() != window.window_handle().window_id() { return; }
        let owns = self.owns_focus(window, cx); let retired_focus = self.focus.clone();
        let root = self.root.clone(); let owner = cx.entity_id(); let native = self.native;
        let revision = self.visibility.request_revision.load(Ordering::Acquire);
        self.shutdown();
        if owns { window.defer(cx, move |window, app| { let _ = root.update(app, |root, cx| {
            let current = root.folder_size_view.as_ref().is_some_and(|view| view.entity_id() == owner && !view.read(cx).open)
                && root.root_window.is_some_and(|host| host.window_id() == window.window_handle().window_id())
                && native.is_some_and(|native| root.native_window == Some(native)
                    && current_foreground_identity() == Some(native.identity)
                    && json_utility::native_host_current(&root.root_visibility, revision, Some(native), false))
                && window.is_window_active() && !window.has_active_dialog(cx)
                && root.action_sheet.is_none() && root.context_favorite_popup.is_none()
                && !context_stack_has_primary(&window.context_stack(), "PopupMenu")
                && window.focused(cx).is_none_or(|focus| retired_focus.contains(&focus, window));
            if current { root.query.focus_handle(cx).focus(window, cx); root.root_scroll.scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM); }
            cx.notify();
        }); }); }
        cx.notify();
    }
    pub(super) fn handle_escape(&mut self, window: &mut Window, cx: &mut Context<Self>, held: bool) -> bool {
        if !self.owns_focus(window, cx) || !self.gesture_current(window, cx) { return false; }
        if !held { self.close(window, cx); }
        true
    }
    fn gesture_current(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.open && self.mount.is_current() && !self.admission.is_fenced() && self.host.window_id() == window.window_handle().window_id()
            && window.is_window_active() && !window.has_active_dialog(cx)
            && self.native.is_some_and(|native| current_foreground_identity() == Some(native.identity)
                && json_utility::native_host_current(&self.visibility, self.visibility.request_revision.load(Ordering::Acquire), Some(native), false))
            && self.root.upgrade().is_some_and(|root| root.read(cx).external_owner_is_current(window.focused(cx).as_ref(), window, cx))
    }

    fn choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker || !self.gesture_current(window, cx) { return; }
        let Some(owner) = checked_active_dialog_owner(window).ok().flatten().filter(|owner| Some(*owner) == self.native) else {
            self.message = Some("Activate the launcher before choosing a folder.".into()); cx.notify(); return;
        };
        let revision = self.visibility.request_revision.load(Ordering::Acquire);
        if !json_utility::native_host_current(&self.visibility, revision, Some(owner), false) { return; }
        let Some(permit) = self.admission.admit() else { return; };
        self.picker_attempt = self.picker_attempt.wrapping_add(1).max(1);
        let attempt = self.picker_attempt; let mount = self.mount.clone(); let worker_mount = mount.clone();
        let visibility = self.visibility.clone(); let admission = self.admission.clone();
        self.picker = true; self.message = None;
        let picker = rfd::FileDialog::new().set_parent(&*window);
        let task = cx.background_executor().spawn(async move {
            if !worker_mount.is_current() || admission.is_fenced() || current_foreground_identity() != Some(owner.identity)
                || !json_utility::native_host_current(&visibility, revision, Some(owner), false) { return None; }
            Some(picker.pick_folder())
        });
        let weak = cx.entity().downgrade(); let host = self.host;
        cx.spawn(async move |_, cx| {
            let _permit = permit; let selected = task.await.flatten();
            let Some(view) = weak.upgrade() else { return; };
            let _ = update_kit_window_content(host, &view, cx, |view, window, cx| {
                if view.picker_attempt != attempt { return; }
                view.picker = false;
                let current = picker_current(&mount, attempt, view.picker_attempt, view.open && !view.admission.is_fenced(),
                    view.host.window_id() == window.window_handle().window_id() && view.native == Some(owner)
                    && json_utility::native_host_current(&view.visibility, revision, Some(owner), false));
                if current { if let Some(path) = selected {
                    match ExplicitDirectoryRoot::new(path) {
                        Ok(root) => {
                            if view.root_choice.as_ref().map(|old| old.as_path()) != Some(root.as_path()) {
                                let _ = view.session.cancel(); view.report_root = None; view.sync_report();
                            }
                            view.root_choice = Some(root); view.message = None;
                        }
                        Err(error) => view.message = Some(format!("Cannot use this folder: {error:?}")),
                    }
                }}
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
    fn scan(&mut self, draft: ScanDraft, generation: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker || !self.gesture_current(window, cx)
            || !scan_receipt_current(&draft, ScanDraft::freeze(self.root_choice.as_ref(), self.recursive).as_ref(),
                generation, self.session.view().generation()) { return; }
        let permit = if self.scan_permit.is_none() { self.admission.admit() } else { None };
        if self.scan_permit.is_none() && permit.is_none() { return; }
        self.report_root = Some(draft.root.clone()); self.report_recursive = draft.recursive;
        match self.session.refresh(draft.root, draft.recursive) {
            Ok(_) => { if self.scan_permit.is_none() { self.scan_permit = permit; } self.message = None; }
            Err(error) => { self.report_root = None; self.message = Some(format!("Could not start scan: {error}")); }
        }
        self.sync_report();
        cx.notify();
    }
    fn sync_report(&mut self) {
        self.report_ui.report.sync(&self.session.view(), self.report_root.as_ref(), self.root_choice.as_ref());
        self.report_ui.report.sync_visible_rows();
    }
}
impl Drop for FolderSizeView { fn drop(&mut self) { self.session.close(); self.wake.close(); } }
impl Render for FolderSizeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_report();
        let snapshot = self.session.view();
        let show_report = self.root_choice.as_ref().is_some_and(|root| self.report_root.as_ref() == Some(root));
        let mut panel = div().id("folder-size-pane").key_context("FolderSizePane").track_focus(&self.focus)
            .capture_action(cx.listener(|view, _: &gpui_kit::base::input::Escape, window, cx| {
                if view.owns_focus(window, cx) {
                    let root = view.root.clone();
                    let latched = root.update(cx, |root, _| { let was = root.pane_escape_latched; root.pane_escape_latched = true; was }).unwrap_or(true);
                    if !latched { view.handle_escape(window, cx, false); }
                    cx.stop_propagation();
                }
            })).flex().flex_col().gap_2().w_full().p_3()
            .child(Label::new("Folder Size"))
            .child(Label::new(self.root_choice.as_ref().map_or_else(|| "No folder selected".into(), |root| root.as_path().display().to_string())));
        let weak = cx.entity().downgrade(); let mount = self.mount.clone();
        let recursive = self.recursive; let generation = snapshot.generation(); let draft = ScanDraft::freeze(self.root_choice.as_ref(), recursive);
        let picker_attempt = self.picker_attempt;
        panel = panel.child(Checkbox::new("folder-size-recursive").accessibility_label("Include subfolders").checked(self.recursive)
            .disabled(self.picker).on_change(move |state, _, window, app| {
                let _ = weak.update(app, |view, cx| { if mount.is_current() && mount.matches(&view.mount) && !view.picker && view.recursive == recursive
                    && view.session.view().generation() == generation && view.gesture_current(window, cx) { view.recursive = state == CheckboxState::Checked; cx.notify(); } });
            }).child(div().flex().gap_2().child(CheckboxIndicator::new().checked(self.recursive)).child(Label::new("Include subfolders"))));
        panel = panel.child(div().flex().flex_wrap().gap_2()
            .child(Button::new("folder-size-choose").label("Choose folder").track_focus(&self.choose_focus).tab_stop(true).disabled(self.picker)
                .on_click({ let mount = self.mount.clone(); cx.listener(move |view, _, window, cx| {
                    if mount.is_current() && mount.matches(&view.mount) && view.picker_attempt == picker_attempt { view.choose(window, cx); }
                }) }))
            .child(Button::new("folder-size-scan").label(if snapshot.is_busy() { "Refresh" } else { "Scan" }).disabled(self.root_choice.is_none() || self.picker)
                .on_click({ let mount = self.mount.clone(); cx.listener(move |view, _, window, cx| { if mount.is_current() && mount.matches(&view.mount) { if let Some(draft) = draft.clone() { view.scan(draft, generation, window, cx); } } }) }))
            .child(Button::new("folder-size-cancel").label("Cancel scan").disabled(!snapshot.is_busy())
                .on_click({ let mount = self.mount.clone(); cx.listener(move |view, _, window, cx| { if mount.is_current() && mount.matches(&view.mount) && view.session.view().generation() == generation && view.gesture_current(window, cx) { let _ = view.session.cancel(); cx.notify(); } }) }))
            .child(Button::new("folder-size-close").label("Close").on_click({ let mount = self.mount.clone(); cx.listener(move |view, _, window, cx| {
                if mount.is_current() && mount.matches(&view.mount) && view.gesture_current(window, cx) { view.close(window, cx); }
            }) })));
        if let Some(message) = &self.message { panel = panel.child(Label::new(message.clone())); }
        if show_report {
            panel = panel.child(Label::new(format!("{:?} · {}", snapshot.status(),
                if self.report_recursive { "Including subfolders" } else { "Top folder only" })));
            if let Some(progress) = snapshot.progress().filter(|_| snapshot.result().is_none() || snapshot.status() == FolderSizeSessionStatus::Cancelled) {
                panel = panel.child(Label::new(format!("{} bytes · {} files · {} directories scanned · {} inaccessible · {} excluded",
                    progress.total_logical_bytes(), progress.files_seen(), progress.directories_scanned(), progress.inaccessible_entries(),
                    progress.reparse_entries_excluded() + progress.cloud_placeholders_excluded())));
            }
            if let Some(result) = snapshot.result().filter(|_| snapshot.result_is_current()) {
                let summary = result.summary();
                panel = panel.child(Label::new(format!("{} bytes · {} files · {} directories scanned · {} inaccessible · {} excluded · {} issues",
                    result.total_logical_bytes(), summary.files_seen(), summary.directories_scanned(), summary.inaccessible_entries(),
                    summary.reparse_entries_excluded() + summary.cloud_placeholders_excluded(), summary.issue_count())));
            }
            if let Some(error) = snapshot.error() { panel = panel.child(Label::new(format!("Scan failed: {error}"))); }
            panel = panel.child(report::render(self, window, cx));
        } else if snapshot.status() == FolderSizeSessionStatus::Idle { panel = panel.child(Label::new("Choose a folder, then select Scan.")); }
        panel
    }
}
impl RootWindow {
    pub(super) fn open_folder_size(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced() || window.has_active_dialog(cx) || !window.is_window_active()
            || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            || !self.native_window.is_some_and(|native| current_foreground_identity() == Some(native.identity)) { return; }
        if let Some(existing) = &self.folder_size_view {
            if existing.read(cx).open { existing.update(cx, |view, cx| view.show(window, cx)); self.reveal_folder_size(window, cx); cx.notify(); return; }
            if existing.read(cx).busy() { self.launcher_search_error = Some(WAIT.into()); cx.notify(); return; }
        }
        let root = cx.entity().downgrade(); let native = self.native_window;
        let visibility = self.root_visibility.clone(); let admission = self.restart_admission.clone();
        let activity = self.activity_center.clone();
        let view = cx.new(|cx| FolderSizeView::new(window, cx, root, native, visibility, admission, activity));
        view.update(cx, |view, cx| view.show(window, cx));
        self.folder_size_view = Some(view); self.query_history.reset(); self.reveal_folder_size(window, cx); cx.notify();
    }
    fn reveal_folder_size(&self, window: &Window, cx: &App) {
        let id = window.window_handle().window_id();
        let history = self.clipboard_history.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let modify = self.clipboard_modify_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let snippets = self.snippets_view.as_ref().is_some_and(|view| view.read(cx).open);
        let fill = self.snippet_fill.as_ref().is_some_and(|view| view.read(cx).open);
        let tempfile = self.tempfile_view.as_ref().is_some_and(|view| view.read(cx).open);
        self.root_scroll.scroll_to_top_of_item(self.clipboard_scroll_item(id, cx)
            + usize::from(history) + usize::from(modify) + usize::from(snippets) + usize::from(fill) + usize::from(tempfile));
    }
}

#[cfg(test)]
mod tests {
    use super::{current_provider, picker_current, ScanDraft};
    use multi_launcher::plugin::Plugin;
    #[test]
    fn exact_installed_folder_size_admission_and_forgery() {
        let plugin = multi_launcher::plugins::folder_size::FolderSizePlugin;
        let row = plugin.search("FolderSize").pop().unwrap();
        assert!(current_provider("FolderSize", &row, None, None));
        assert!(!current_provider("foldersize extra", &row, None, None));
        assert!(current_provider("kind:foldersize foldersize", &row, None, None));
        let mut forged = row.clone(); forged.args = Some("forged".into());
        assert!(!current_provider("foldersize", &forged, None, None));
        assert!(!current_provider("foldersize", &row, Some(&std::collections::HashSet::new()), None));
    }
    #[test]
    fn selected_root_and_recursive_flag_freeze_and_picker_receipts_retire() {
        let owner = super::super::utility_mount::MountOwner::default();
        let mount = owner.attach();
        assert!(picker_current(&mount, 4, 4, true, true));
        assert!(!picker_current(&mount, 4, 5, true, true));
        let root = multi_launcher::batch_rename::ExplicitDirectoryRoot::new(std::env::temp_dir()).unwrap();
        let session = multi_launcher::folder_size::FolderSizeSession::new(|| {});
        assert!(!session.view().is_busy());
        assert!(ScanDraft::freeze(None, true).is_none());
        let draft = ScanDraft::freeze(Some(&root), true).unwrap();
        assert!(!session.view().is_busy(), "selecting or freezing a root does not scan");
        assert_eq!(draft.root.as_path(), root.as_path()); assert!(draft.recursive);
        assert!(super::scan_receipt_current(&draft, Some(&draft), 1, 1));
        assert!(!super::scan_receipt_current(&draft, Some(&draft), 1, 2));
        let changed = ScanDraft::freeze(Some(&root), false).unwrap();
        assert!(!super::scan_receipt_current(&draft, Some(&changed), 1, 1));
        owner.retire(); assert!(!picker_current(&mount, 4, 4, true, true));
    }
}
