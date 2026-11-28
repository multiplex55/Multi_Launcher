//! Retained read-only Duplicate Finder over the shared scanner and report.
use super::*;
use multi_launcher::duplicate_finder::{DuplicateFinder, DuplicateFinderReport, DuplicateFinderRow,
    DuplicateFinderSessionError, DuplicateFinderStatus, MAX_ROOTS, MAX_ROOT_PATH_BYTES, encoded_root_path_bytes};
use multi_launcher::plugin::Plugin;

pub(super) const WAIT: &str = "Wait for Duplicate Finder to finish.";

pub(super) fn current_provider(query: &str, action: &Action, enabled: Option<&HashSet<String>>,
    capabilities: Option<&HashMap<String, Vec<String>>>) -> bool {
    enabled.is_none_or(|set| set.contains("duplicate_finder"))
        && multi_launcher::settings::provider_search_capability_enabled(capabilities, "duplicate_finder")
        && action.args.is_none() && matches!(parse_action(action), Ok(Command::Dialog(DialogCommand::DuplicateFinder)))
        && super::search_providers::search_routed(
            [&multi_launcher::plugins::duplicate_finder::DuplicateFinderPlugin as &dyn Plugin], query, enabled, capabilities)
            .into_iter().any(|(name, row)| name == "duplicate_finder" && row == *action)
}

fn add_root(roots: &mut Vec<PathBuf>, path: PathBuf) -> Result<(), &'static str> {
    if path.as_os_str().is_empty() { return Err("Enter or choose a folder before adding it."); }
    if roots.len() >= MAX_ROOTS { return Err("Select no more than 32 folder roots per scan."); }
    let used = roots.iter().map(|root| encoded_root_path_bytes(root)).fold(0usize, usize::saturating_add);
    if used.saturating_add(encoded_root_path_bytes(&path)) > MAX_ROOT_PATH_BYTES { return Err("Selected root paths exceed the 2 MiB selection limit."); }
    if roots.iter().any(|root| root == &path) { return Err("That exact folder path is already selected."); }
    roots.push(path); Ok(())
}
fn receipt_current(mount: &utility_mount::MountReceipt, issued: u64, current: u64, open: bool, host: bool) -> bool {
    mount.is_current() && issued == current && open && host
}

pub(super) struct DuplicateFinderView {
    pub(super) open: bool,
    root: WeakEntity<RootWindow>,
    host: AnyWindowHandle,
    native: Option<NativeWindow>,
    visibility: RootVisibility,
    admission: Arc<RestartAdmission>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    session: DuplicateFinder,
    wake: Arc<HashWakeSignal>,
    permit: Option<EffectPermit>,
    editor: Entity<InputState>,
    focus: FocusHandle,
    roots: Vec<PathBuf>,
    roots_revision: u64,
    picker: bool,
    picker_attempt: u64,
    reopening: bool,
    report: Option<Arc<DuplicateFinderReport>>,
    scroll: gpui_kit::UniformListScrollHandle,
    message: Option<String>,
}
impl DuplicateFinderView {
    fn new(window: &mut Window, cx: &mut Context<Self>, root: WeakEntity<RootWindow>, native: Option<NativeWindow>,
        visibility: RootVisibility, admission: Arc<RestartAdmission>, activity: multi_launcher::activity::ActivityCenter) -> Self {
        let wake = Arc::new(HashWakeSignal::default()); let signal = wake.clone();
        let session = DuplicateFinder::new(move || signal.notify(), activity);
        let owner = utility_mount::MountOwner::default(); let mount = owner.attach();
        let weak = cx.entity().downgrade(); let receiver = wake.clone();
        cx.spawn(async move |_, cx| {
            while receiver.wait().await {
                if weak.update(cx, |view, cx| {
                    if view.open && view.reopening {
                        match view.session.reopen() {
                            Ok(()) => { view.reopening = false; view.message = None; view.report = None; }
                            Err(DuplicateFinderSessionError::WorkerRetiring) => {}
                            Err(error) => { view.reopening = false; view.message = Some(format!("Could not reopen Duplicate Finder: {error}")); }
                        }
                    }
                    if !view.session.view().busy() { view.permit = None; }
                    if !view.busy() {
                        let root = view.root.clone();
                        cx.spawn(async move |_, cx| { let _ = root.update(cx, |root, cx| {
                            if root.duplicate_finder_view.as_ref().is_some_and(|view| view.read(cx).busy()) { return; }
                            if root.launcher_search_error.as_deref() == Some(WAIT) { root.launcher_search_error = None; }
                            if root.restart_status.as_deref() == Some(WAIT) { root.restart_status = None; }
                            cx.notify();
                        }); }).detach();
                    }
                    cx.notify();
                }).is_err() { break; }
            }
        }).detach();
        Self { open: true, root, host: window.window_handle(), native, visibility, admission, owner, mount,
            session, wake, permit: None, editor: cx.new(|cx| InputState::new(window, cx).placeholder("Folder path")),
            focus: cx.focus_handle(), roots: Vec::new(), roots_revision: 0, picker: false, picker_attempt: 0, reopening: false,
            report: None, scroll: gpui_kit::UniformListScrollHandle::default(), message: None }
    }
    pub(super) fn busy(&self) -> bool { self.picker || self.reopening || self.session.view().busy() }
    pub(super) fn owns_focus(&self, window: &Window, cx: &App) -> bool { self.open && self.focus.contains_focused(window, cx) }
    pub(super) fn shutdown(&mut self) {
        self.owner.retire(); self.picker_attempt = self.picker_attempt.wrapping_add(1);
        self.open = false; self.reopening = false; self.report = None; self.session.close();
    }
    fn current(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.open && self.mount.is_current() && !self.admission.is_fenced()
            && self.host.window_id() == window.window_handle().window_id() && window.is_window_active() && !window.has_active_dialog(cx)
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
            let valid = root.duplicate_finder_view.as_ref().is_some_and(|view| view.entity_id() == owner && !view.read(cx).open)
                && root.root_window.is_some_and(|host| host.window_id() == window.window_handle().window_id())
                && native.is_some_and(|native| root.native_window == Some(native) && current_foreground_identity() == Some(native.identity)
                    && json_utility::native_host_current(&root.root_visibility, revision, Some(native), false))
                && window.is_window_active() && !window.has_active_dialog(cx) && root.action_sheet.is_none()
                && root.context_favorite_popup.is_none() && !context_stack_has_primary(&window.context_stack(), "PopupMenu")
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
    fn add(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match add_root(&mut self.roots, path) {
            Ok(()) => { self.roots_revision = self.roots_revision.wrapping_add(1); self.message = None;
                self.editor.update(cx, |input, cx| input.set_value(String::new(), window, cx)); }
            Err(error) => self.message = Some(error.into()),
        }
        cx.notify();
    }
    fn choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker || !self.current(window, cx) { return; }
        let Some(native) = checked_active_dialog_owner(window).ok().flatten().filter(|owner| Some(*owner) == self.native) else {
            self.message = Some("Activate the launcher before choosing a folder.".into()); cx.notify(); return;
        };
        let revision = self.visibility.request_revision.load(Ordering::Acquire);
        if !json_utility::native_host_current(&self.visibility, revision, Some(native), false) { return; }
        let Some(permit) = self.admission.admit() else { return; };
        self.picker_attempt = self.picker_attempt.wrapping_add(1).max(1); let attempt = self.picker_attempt;
        let roots_revision = self.roots_revision; let editor = self.editor.read(cx).value().to_string();
        let mount = self.mount.clone(); let worker_mount = mount.clone();
        let visibility = self.visibility.clone(); let admission = self.admission.clone();
        let picker = rfd::FileDialog::new().set_parent(&*window);
        self.picker = true; self.message = None;
        let task = cx.background_executor().spawn(async move {
            if !worker_mount.is_current() || admission.is_fenced() || current_foreground_identity() != Some(native.identity)
                || !json_utility::native_host_current(&visibility, revision, Some(native), false) { return None; }
            Some(picker.pick_folder())
        });
        let weak = cx.entity().downgrade(); let host = self.host;
        cx.spawn(async move |_, cx| {
            let _permit = permit; let selected = task.await.flatten();
            let Some(view) = weak.upgrade() else { return; };
            let _ = update_kit_window_content(host, &view, cx, |view, window, cx| {
                if view.picker_attempt != attempt { return; }
                view.picker = false;
                if view.message.as_deref() == Some(WAIT) { view.message = None; }
                let current = receipt_current(&mount, attempt, view.picker_attempt, view.open && !view.admission.is_fenced(),
                    view.host.window_id() == window.window_handle().window_id() && view.native == Some(native)
                    && json_utility::native_host_current(&view.visibility, revision, Some(native), false))
                    && view.roots_revision == roots_revision && view.editor.read(cx).value().as_str() == editor;
                if current && let Some(path) = selected { view.add(path, window, cx); }
                if !view.busy() {
                    let root = view.root.clone();
                    cx.spawn(async move |_, cx| { let _ = root.update(cx, |root, cx| {
                        if root.duplicate_finder_view.as_ref().is_some_and(|view| view.read(cx).busy()) { return; }
                            if root.launcher_search_error.as_deref() == Some(WAIT) { root.launcher_search_error = None; }
                        if root.restart_status.as_deref() == Some(WAIT) { root.restart_status = None; }
                        cx.notify();
                    }); }).detach();
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
}
impl Drop for DuplicateFinderView {
    fn drop(&mut self) { self.session.close(); self.wake.close(); }
}
impl Render for DuplicateFinderView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.session.view();
        if self.report.as_ref().is_none_or(|report| !state.matches_report(report)) {
            self.report = Some(Arc::new(state.report()));
        }
        let report = self.report.as_ref().unwrap().clone();
        let generation = state.generation(); let revision = self.roots_revision;
        let draft = self.editor.read(cx).value().to_string(); let mount = self.mount.clone();
        let picker_attempt = self.picker_attempt;
        let shortcut_mount = mount.clone();
        let mut panel = div().id("duplicate-finder-pane").key_context("DuplicateFinderPane").track_focus(&self.focus)
            .capture_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                if !window.default_prevented() && shortcut_mount.is_current() && shortcut_mount.matches(&view.mount)
                    && event.keystroke.key == "w" && event.keystroke.modifiers == (Modifiers { control: true, ..Modifiers::default() })
                    && view.owns_focus(window, cx) && view.current(window, cx) {
                    if !event.is_held { view.close(window, cx); }
                    window.prevent_default(); cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|view, _: &gpui_kit::base::input::Escape, window, cx| {
                if view.owns_focus(window, cx) {
                    let root = view.root.clone();
                    let latched = root.update(cx, |root, _| { let was = root.pane_escape_latched; root.pane_escape_latched = true; was }).unwrap_or(true);
                    if !latched { view.handle_escape(window, cx, false); }
                    cx.stop_propagation();
                }
            })).flex().flex_col().gap_2().w_full().p_3()
            .child(Label::new("Duplicate Finder"))
            .child(Label::new("Choose one or more folders. Duplicate discovery runs only after Start."))
            .child(Input::new(&self.editor).aria_label("Folder path").disabled(self.picker))
            .child(div().flex().flex_wrap().gap_2()
                .child(Button::new("dff-choose").label("Choose folder").disabled(self.picker)
                    .on_click({ let mount = mount.clone(); cx.listener(move |view, _, window, cx| {
                        if mount.is_current() && mount.matches(&view.mount) && view.picker_attempt == picker_attempt { view.choose(window, cx); }
                    }) }))
                .child(Button::new("dff-add").label("Add root").disabled(self.picker)
                    .on_click({ let mount = mount.clone(); cx.listener(move |view, _, window, cx| {
                        if mount.is_current() && mount.matches(&view.mount) && view.roots_revision == revision && !view.picker
                            && view.editor.read(cx).value().as_str() == draft && view.current(window, cx) {
                            view.add(PathBuf::from(draft.trim()), window, cx);
                        }
                    }) }))
                .child(Button::new("dff-start").label("Start scan").disabled(self.picker || self.reopening || state.closed() || state.busy() || self.roots.is_empty())
                    .on_click({ let mount = mount.clone(); let roots = self.roots.clone(); cx.listener(move |view, _, window, cx| {
                        if !mount.is_current() || !mount.matches(&view.mount) || view.picker || view.reopening || view.session.view().closed() || view.roots_revision != revision
                            || view.roots != roots || view.session.view().generation() != generation || view.session.view().busy() || !view.current(window, cx) { return; }
                        let Some(permit) = view.admission.admit() else { return; };
                        match view.session.start(roots.clone()) {
                            Ok(_) => { view.permit = Some(permit); view.message = None; }
                            Err(error) => view.message = Some(format!("Could not start scan: {error}")),
                        }
                        cx.notify();
                    }) }))
                .child(Button::new("dff-cancel").label("Cancel scan").disabled(!state.busy())
                    .on_click({ let mount = mount.clone(); cx.listener(move |view, _, window, cx| {
                        if mount.is_current() && mount.matches(&view.mount) && view.session.view().generation() == generation
                            && view.current(window, cx) { let _ = view.session.cancel(); cx.notify(); }
                    }) }))
                .child(Button::new("dff-close").label("Close").on_click({ let mount = mount.clone(); cx.listener(move |view, _, window, cx| {
                    if mount.is_current() && mount.matches(&view.mount) { view.close(window, cx); }
                }) })));
        if let Some(message) = &self.message { panel = panel.child(Label::new(message.clone())); }
        if self.roots.is_empty() { panel = panel.child(Label::new("No roots selected.")); }
        let mut root_list = div().id("dff-roots").flex().flex_col().max_h(px(120.0)).overflow_y_scroll();
        for (index, path) in self.roots.iter().enumerate() {
            let path = path.clone(); let issued = mount.clone();
            root_list = root_list.child(div().flex().gap_2().child(Label::new(path.display().to_string()).id(SharedString::from(format!("dff-root-label-{index}"))))
                .child(Button::new(SharedString::from(format!("dff-remove-{index}"))).label("Remove")
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if issued.is_current() && issued.matches(&view.mount) && view.roots_revision == revision
                            && view.roots.get(index) == Some(&path) && !view.picker && view.current(window, cx) {
                            view.roots.remove(index); view.roots_revision = view.roots_revision.wrapping_add(1); view.message = None; cx.notify();
                        }
                    }))));
        }
        panel = panel.child(root_list);
        let status = if self.reopening { "Waiting for the previous scan to retire…" } else { match state.status() {
            DuplicateFinderStatus::Idle => "Ready. Select folders, then start a scan.",
            DuplicateFinderStatus::Scanning => "Scanning…",
            DuplicateFinderStatus::Complete => "Scan complete.",
            DuplicateFinderStatus::Partial => "Partial results; some entries could not be confirmed.",
            DuplicateFinderStatus::Cancelled => "Scan cancelled.",
            DuplicateFinderStatus::Failed => "Scan failed.",
            DuplicateFinderStatus::Closed => "Duplicate Finder closed.",
        }};
        panel = panel.child(Label::new(status));
        if let Some(progress) = state.progress() {
            panel = panel.child(Label::new(format!("{} · roots {}/{} · {} directories · {} files",
                progress.stage, progress.roots_completed, progress.roots_total, progress.directories_scanned, progress.files_seen)));
            if let (Some(done), Some(total), Some(bytes)) = (progress.files_completed, progress.files_total, progress.bytes_processed) {
                panel = panel.child(Label::new(format!("Content verification · {done}/{total} files · {bytes} bytes read")));
            }
        }
        let totals = report.totals(); let counts = report.counts();
        panel = panel.child(Label::new(format!("{} verified groups · {} distinct files · {} logical duplicate bytes (not recoverable disk space) · {} identity-unverified matches",
            totals.verified_groups, totals.verified_files, totals.logical_duplicate_bytes, totals.identity_unverified_matches)))
            .child(Label::new(format!("{} files · {} directories scanned · {} inaccessible · {} excluded",
                counts.files_seen, counts.directories_scanned, counts.inaccessible, counts.reparse_excluded + counts.cloud_excluded)));
        let mut diagnostics = div().id("dff-diagnostics").flex().flex_col().max_h(px(120.0)).overflow_y_scroll();
        for (index, (path, status)) in report.roots().enumerate() {
            diagnostics = diagnostics.child(Label::new(format!("{} · {status}", path.display())).id(SharedString::from(format!("dff-report-root-{index}"))));
        }
        for (index, (path, message)) in report.diagnostics().enumerate() {
            diagnostics = diagnostics.child(Label::new(path.map_or_else(|| message.to_owned(), |path| format!("{}: {message}", path.display())))
                .id(SharedString::from(format!("dff-diagnostic-{index}"))));
        }
        if report.diagnostics_truncated() { diagnostics = diagnostics.child(Label::new("Additional diagnostics were omitted.").id("dff-diagnostics-truncated")); }
        panel = panel.child(diagnostics);
        if report.truncated() { panel = panel.child(Label::new("The visible result list reached its display limit; scan totals remain unchanged.")); }
        let row_report = report.clone();
        let list = gpui_kit::uniform_list("dff-results", report.len(), move |range, _, _| {
            range.filter_map(|index| {
                let text = match row_report.row(index)? {
                    DuplicateFinderRow::VerifiedGroup { bytes, files, duplicate_bytes } => format!("Verified duplicate group · {bytes} bytes each · {files} distinct files · {duplicate_bytes} logical duplicate bytes"),
                    DuplicateFinderRow::VerifiedFile { path, bytes } => format!("{} · {bytes} bytes", path.display()),
                    DuplicateFinderRow::UnverifiedGroup { bytes } => format!("Content match with unverified identity · {bytes} bytes each · not included in verified totals"),
                    DuplicateFinderRow::UnverifiedKnownFile { path, bytes } => format!("{} · {bytes} bytes · identity-unverified match", path.display()),
                    DuplicateFinderRow::UnverifiedPath { path, bytes } => format!("{} · {bytes} bytes · identity unavailable", path.display()),
                };
                Some(div().h(px(30.0)).w_full().child(Label::new(text).id(SharedString::from(format!("dff-result-label-{index}"))).whitespace_nowrap().flex_shrink_0()).overflow_x_scrollbar().id(SharedString::from(format!("dff-result-{index}"))).into_any_element())
            }).collect::<Vec<_>>()
        }).track_scroll(&self.scroll).h(px(280.0)).w_full();
        panel.child(list)
    }
}

impl RootWindow {
    pub(super) fn open_duplicate_finder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced() || window.has_active_dialog(cx) || !window.is_window_active()
            || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            || !self.native_window.is_some_and(|native| current_foreground_identity() == Some(native.identity)) { return; }
        let view = if let Some(existing) = &self.duplicate_finder_view {
            let existing = existing.clone();
            if !existing.read(cx).open {
                let reopened = existing.update(cx, |view, _| view.session.reopen());
                if let Err(error) = &reopened && !matches!(error, DuplicateFinderSessionError::WorkerRetiring) {
                    self.launcher_search_error = Some(format!("Could not reopen Duplicate Finder: {error}")); cx.notify(); return;
                }
                existing.update(cx, |view, _| {
                    view.open = true; view.mount = view.owner.attach();
                    view.reopening = matches!(reopened, Err(DuplicateFinderSessionError::WorkerRetiring));
                });
            }
            existing
        } else {
            let root = cx.entity().downgrade(); let native = self.native_window; let visibility = self.root_visibility.clone();
            let admission = self.restart_admission.clone(); let activity = self.activity_center.clone();
            let view = cx.new(|cx| DuplicateFinderView::new(window, cx, root, native, visibility, admission, activity));
            self.duplicate_finder_view = Some(view.clone()); view
        };
        view.update(cx, |view, cx| view.editor.focus_handle(cx).focus(window, cx));
        self.query_history.reset(); self.reveal_duplicate_finder(window, cx); cx.notify();
    }
    fn reveal_duplicate_finder(&self, window: &Window, cx: &App) {
        let id = window.window_handle().window_id();
        let history = self.clipboard_history.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let modify = self.clipboard_modify_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let snippets = self.snippets_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let fill = self.snippet_fill.as_ref().is_some_and(|view| view.read(cx).open);
        let tempfile = self.tempfile_view.as_ref().is_some_and(|view| view.read(cx).open);
        let folder = self.folder_size_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let lock = self.file_lock_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        self.root_scroll.scroll_to_top_of_item(self.clipboard_scroll_item(id, cx)
            + usize::from(history) + usize::from(modify) + usize::from(snippets) + usize::from(fill)
            + usize::from(tempfile) + usize::from(folder) + usize::from(lock));
    }
}

#[cfg(test)]
mod tests {
    use super::{add_root, current_provider, receipt_current};
    use multi_launcher::plugin::Plugin;
    #[test]
    fn root_draft_limits_and_current_picker_receipt() {
        let mut roots = Vec::new();
        assert!(add_root(&mut roots, "owned".into()).is_ok());
        assert!(add_root(&mut roots, "owned".into()).is_err());
        assert_eq!(roots.len(), 1);
        let owner = super::super::utility_mount::MountOwner::default(); let mount = owner.attach();
        assert!(receipt_current(&mount, 2, 2, true, true));
        assert!(!receipt_current(&mount, 2, 3, true, true));
        owner.retire(); assert!(!receipt_current(&mount, 2, 2, true, true));
    }
    #[test]
    fn exact_provider_rejects_forged_or_disabled_actions() {
        let row = multi_launcher::plugins::duplicate_finder::DuplicateFinderPlugin.search("dff").pop().unwrap();
        assert!(current_provider("DFF", &row, None, None));
        assert!(!current_provider("dff extra", &row, None, None));
        let mut forged = row.clone(); forged.args = Some("path".into());
        assert!(!current_provider("dff", &forged, None, None));
        assert!(!current_provider("dff", &row, Some(&std::collections::HashSet::new()), None));
    }
}
