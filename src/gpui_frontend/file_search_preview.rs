//! Retained read-only preview for one exact File Search selection.
use super::*;
use multi_launcher::file_search::preview::{preview_file, PreviewLoadOutcome, PreviewLoadStateMachine, PreviewLoadingState, PreviewRequest, PreviewLine, PreviewCoverage};

pub(super) const WAIT: &str = "Wait for the File Search preview action to finish.";
const MAX_LINES: usize = 12;
const MAX_LINE_CHARS: usize = 240;

fn line_segments(line: &PreviewLine, start: usize) -> Vec<(String, bool)> {
    let total = line.text.chars().count();
    let selected = line.match_ranges.first().map(|range| (range.start_column.saturating_sub(1), range.end_column.saturating_sub(1)));
    let start = start.min(total);
    let end = start.saturating_add(MAX_LINE_CHARS).min(total);
    let mut result = Vec::new();
    if start > 0 { result.push(("…".into(), false)); }
    let (match_start, match_end) = selected.unwrap_or((end, end));
    let mut current = String::new();
    let mut highlighted = false;
    for (index, ch) in line.text.chars().enumerate().skip(start).take(end - start) {
        let next = index >= match_start && index < match_end;
        if next != highlighted && !current.is_empty() { result.push((std::mem::take(&mut current), highlighted)); }
        highlighted = next;
        current.push(if ch == '\n' || ch == '\r' { ' ' } else { ch });
    }
    if !current.is_empty() { result.push((current, highlighted)); }
    if end < total { result.push(("…".into(), false)); }
    result
}

fn line_navigation(offset: usize, count: usize, key: &str) -> Option<usize> {
    let last = count.saturating_sub(MAX_LINES);
    Some(match key {
        "up" => offset.saturating_sub(1), "down" => offset.saturating_add(1).min(last),
        "pageup" => offset.saturating_sub(MAX_LINES), "pagedown" => offset.saturating_add(MAX_LINES).min(last),
        "home" => 0, "end" => count.saturating_sub(MAX_LINES), _ => return None,
    })
}

fn inline_load_current(selection: bool, root_visible: bool, fenced: bool) -> bool { selection && root_visible && !fenced }

fn popup_load_current(selection: bool, native: bool, fenced: bool) -> bool { selection && native && !fenced }

fn popup_action_authorized(selection: bool, action: bool, parent: bool, native: bool, foreground: bool, live: bool) -> bool {
    selection && action && parent && native && foreground && live
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewAction { Explorer, CopyPath, CopyLine }
impl PreviewAction {
    fn is_clipboard(self) -> bool { !matches!(self, Self::Explorer) }
    fn payload(self, path: &std::path::Path, line: Option<&str>) -> Option<String> {
        match self { Self::Explorer => None, Self::CopyPath => Some(path.display().to_string()), Self::CopyLine => line.map(str::to_owned) }
    }
}

pub(super) fn copy_if_current(backend: &dyn multi_launcher::clipboard_modify::clipboard::ClipboardBackend,
    payload: &str, current: &dyn Fn() -> bool) -> Result<(), String> {
    if payload.contains('\0') { return Err("Cannot copy text containing a NUL character.".into()); }
    if !current() { return Err("Action was superseded.".into()); }
    #[cfg(windows)]
    let result = backend.write_text_acquired_if(payload, current);
    #[cfg(not(windows))]
    let result = backend.write_text_if(payload, current);
    match result { Ok(true) => Ok(()), Ok(false) => Err("Action was superseded.".into()),
        Err(error) => Err(format!("Could not copy: {error}")) }
}

fn preview_shell_owns_child<T: PartialEq>(child_host: T, shell_host: T) -> bool { child_host == shell_host }

pub(super) struct PreviewPopupShell {
    pub(super) child: Entity<FileSearchPreview>,
    pub(super) native: Option<NativeWindow>,
}
impl Render for PreviewPopupShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().id("file-search-preview-popup").size_full().overflow_y_scroll().when(preview_shell_owns_child(self.child.read(cx).host(), window.window_handle().window_id()),
            |shell| shell.child(self.child.clone()))
    }
}

pub(super) struct FileSearchPreview {
    parent: WeakEntity<file_search::FileSearchView>,
    root: WeakEntity<RootWindow>,
    receipt: file_search::SelectionReceipt,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    action_owner: utility_mount::MountOwner,
    action_mount: utility_mount::MountReceipt,
    parent_mount: utility_mount::MountReceipt,
    popup: bool,
    focus: FocusHandle,
    document_focus: FocusHandle,
    explorer_focus: FocusHandle,
    copy_path_focus: FocusHandle,
    copy_line_focus: FocusHandle,
     pending: bool,
    effect_lease: Option<Arc<AtomicBool>>,
    focus_guard: Option<Subscription>,
    activation_guard: Option<Subscription>,
    feedback: Option<String>,
    pub(super) return_focus: Option<FocusHandle>,
    pub(super) return_baseline: Option<Option<FocusHandle>>,
    pub(super) return_scroll: Point<Pixels>,
    host: AnyWindowHandle,
    visibility: RootVisibility,
    revision: u64,
    native: Option<NativeWindow>,
    admission: Arc<RestartAdmission>,
    state: PreviewLoadStateMachine,
    found: Option<multi_launcher::file_search::model::ContentMatch>,
    rendered: Vec<(usize, Vec<(String, bool)>)>,
    match_present: bool,
    line_offset: usize,
    column_offset: usize,
    max_column: usize,
    scroll: ScrollHandle,
}

impl FileSearchPreview {
    pub(super) fn new(parent: WeakEntity<file_search::FileSearchView>, root: WeakEntity<RootWindow>, receipt: file_search::SelectionReceipt,
        return_focus: FocusHandle, return_scroll: Point<Pixels>, host: AnyWindowHandle, found: Option<multi_launcher::file_search::model::ContentMatch>, owner: utility_mount::MountOwner,
        mount: utility_mount::MountReceipt, visibility: RootVisibility, revision: u64, native: Option<NativeWindow>,
        admission: Arc<RestartAdmission>, parent_mount: utility_mount::MountReceipt, popup: bool, cx: &mut Context<Self>) -> Self {
        let action_owner = utility_mount::MountOwner::default();
        let action_mount = action_owner.attach();
        Self { parent, root, receipt, owner, mount, action_owner, action_mount, parent_mount, popup, focus: cx.focus_handle(), document_focus: cx.focus_handle(), explorer_focus: cx.focus_handle(),
            copy_path_focus: cx.focus_handle(), copy_line_focus: cx.focus_handle(), pending: false,
            effect_lease: None, focus_guard: None, activation_guard: None, feedback: None,
            return_baseline: Some(Some(return_focus.clone())), return_focus: Some(return_focus), return_scroll, host, visibility, revision, native, admission,
            state: PreviewLoadStateMachine::default(), found, rendered: Vec::new(), match_present: false, line_offset: 0, column_offset: 0, max_column: 0, scroll: ScrollHandle::new() }
    }
    pub(super) fn inline_promotion_current(&self) -> bool {
        !self.popup && self.mount.is_current() && !self.admission.is_fenced()
            && self.native.is_some_and(|native| json_utility::native_host_current(&self.visibility,
                self.visibility.request_revision.load(Ordering::Acquire), Some(native), false))
    }
    pub(super) fn promote_to_popup(&mut self, host: AnyWindowHandle, native: NativeWindow,
        parent_mount: utility_mount::MountReceipt, return_focus: FocusHandle, cx: &mut Context<Self>) {
        self.host = host; self.native = Some(native); self.popup = true; self.parent_mount = parent_mount;
        self.return_baseline = Some(Some(return_focus.clone())); self.return_focus = Some(return_focus);
        self.focus = cx.focus_handle(); self.document_focus = cx.focus_handle(); self.explorer_focus = cx.focus_handle();
        self.copy_path_focus = cx.focus_handle(); self.copy_line_focus = cx.focus_handle();
        self.action_mount = self.action_owner.attach();
    }
    pub(super) fn focus(&self, window: &mut Window, cx: &mut Context<Self>) { self.document_focus.focus(window, cx); }
    pub(super) fn owns_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus.is_focused(window) || self.focus.contains_focused(window, cx)
    }
    pub(super) fn focus_owner(&self) -> FocusHandle { self.focus.clone() }
    pub(super) fn effect_pending(&self) -> bool { self.pending }
    pub(super) fn load_receipt(&self) -> utility_mount::MountReceipt { self.mount.clone() }
    pub(super) fn suspend_actions(&mut self) {
        if let Some(lease) = &self.effect_lease { lease.store(false, Ordering::Release); }
        self.action_owner.retire();
        self.focus_guard = None; self.activation_guard = None;
    }
    pub(super) fn parent_matches(&self, parent_mount: &utility_mount::MountReceipt) -> bool { self.parent_mount.matches(parent_mount) }
    pub(super) fn bind_parent(&mut self, parent_mount: utility_mount::MountReceipt, return_focus: Option<FocusHandle>) {
        if !self.parent_mount.matches(&parent_mount) {
            self.parent_mount = parent_mount;
            self.return_focus = return_focus;
            self.return_baseline = None;
        }
        if !self.popup { self.revision = self.visibility.request_revision.load(Ordering::Acquire); }
        self.action_mount = self.action_owner.attach();
    }
    pub(super) fn set_return_baseline(&mut self, baseline: Option<FocusHandle>) { self.return_baseline = Some(baseline); }
    pub(super) fn retarget_return(&mut self, focus: FocusHandle) {
        self.return_baseline = Some(Some(focus.clone())); self.return_focus = Some(focus);
    }
    pub(super) fn action_is_current(&self, receipt: &utility_mount::MountReceipt) -> bool {
        receipt.is_current() && receipt.matches(&self.action_mount)
    }
    pub(super) fn action_receipt(&self) -> utility_mount::MountReceipt { self.action_mount.clone() }
    pub(super) fn native(&self) -> Option<NativeWindow> { self.native }
    pub(super) fn native_matches(&self, native: NativeWindow) -> bool { self.native == Some(native) }
    pub(super) fn host(&self) -> WindowId { self.host.window_id() }
    pub(super) fn receipt(&self) -> &file_search::SelectionReceipt { &self.receipt }
    pub(super) fn scope_current(&self) -> bool {
        self.mount.is_current() && self.action_mount.is_current() && self.parent_mount.is_current() && !self.admission.is_fenced()
            && self.native.is_some_and(|native| if self.popup { native_window_is_valid(native) }
               else { json_utility::native_host_current(&self.visibility, self.revision, Some(native), false) })
    }
    pub(super) fn retire(&mut self) {
        if let Some(lease) = &self.effect_lease { lease.store(false, Ordering::Release); }
        self.action_owner.retire(); self.owner.retire(); self.state.clear(); self.rendered.clear();
    }
    fn start_action(&mut self, action: PreviewAction, issuing: &utility_mount::MountReceipt, window: &mut Window, cx: &mut Context<Self>) {
        if !self.action_is_current(issuing) { return; }
        let Some(native) = self.native else { return; };
        if (action == PreviewAction::CopyLine && self.found.is_none()) || self.pending || !self.scope_current()
            || self.host.window_id() != window.window_handle().window_id() || !window.is_window_active()
            || window.has_active_dialog(cx) || current_foreground_identity() != Some(native.identity) { return; }
        let Some(parent) = self.parent.upgrade() else { return; };
        if !parent.read(cx).preview_is_current(cx.entity_id(), &self.receipt) { return; }
        let Some(root) = self.root.upgrade() else { return; };
        let before = window.focused(cx);
        {
            let root_state = root.read(cx);
            if (!self.popup && (root_state.root_window.is_none_or(|host| host.window_id() != self.host.window_id())
                || root_state.native_window != Some(native)))
                || !root_state.external_owner_is_current(before.as_ref(), window, cx) { return; }
        }
        let focus = match action { PreviewAction::Explorer => &self.explorer_focus, PreviewAction::CopyPath => &self.copy_path_focus, PreviewAction::CopyLine => &self.copy_line_focus }.clone();
        focus.focus(window, cx);
        if !focus.is_focused(window) { return; }
        let Some(permit) = self.admission.admit() else { return; };
        let lease = Arc::new(AtomicBool::new(true));
        let admitted = root.update(cx, |root, cx| {
            if !root.external_owner_is_current(Some(&focus), window, cx) || window.has_active_dialog(cx)
                || !window.is_window_active() || current_foreground_identity() != Some(native.identity) { return false; }
            if action.is_clipboard() && (root.clipboard_modify_startup.pending
                || root.clipboard_modify_view.as_ref().is_some_and(|view| view.read(cx).blocks_close())) { return false; }
            if !root.external_launch.begin_auxiliary_effect(lease.clone(), action.is_clipboard()) { return false; }
            if action.is_clipboard() {
                if let Some(modify) = &root.clipboard_modify_view {
                    modify.update(cx, |view, cx| { view.retire_read_for_effect(); cx.notify(); });
                }
            }
            true
        });
        if !admitted { self.feedback = Some("Another clipboard or preview action is still active.".into()); cx.notify(); return; }
        let blur_lease = lease.clone();
        self.focus_guard = Some(cx.on_blur(&focus, window, move |_, _, _| blur_lease.store(false, Ordering::Release)));
        let active_lease = lease.clone();
        self.activation_guard = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() { active_lease.store(false, Ordering::Release); }
        }));
        self.pending = true;
        self.effect_lease = Some(lease.clone());
        self.feedback = None;
        let payload = action.payload(&self.receipt.path, self.found.as_ref().map(|found| found.line.as_str()));
        let path = self.receipt.path.clone();
        let directory = self.receipt.kind == multi_launcher::file_search::model::FileKind::Directory;
        let mount = self.action_mount.clone();
        let worker_mount = mount.clone();
        let selection_mount = self.mount.clone();
        let parent_mount = self.parent_mount.clone();
        let popup = self.popup;
        let visibility = self.visibility.clone();
        let revision = self.revision;
        let admission = self.admission.clone();
        let worker_lease = lease.clone();
        let task = cx.background_executor().spawn(async move {
            let current = || {
                let live = worker_lease.load(Ordering::Acquire) && !admission.is_fenced();
                let foreground = current_foreground_identity() == Some(native.identity);
                if popup { popup_action_authorized(selection_mount.is_current(), worker_mount.is_current(), parent_mount.is_current(),
                    native_window_is_valid(native), foreground, live) }
                else { live && worker_mount.is_current() && selection_mount.is_current() && parent_mount.is_current()
                    && json_utility::native_host_current(&visibility, revision, Some(native), false) && foreground }
            };
            match action {
                PreviewAction::Explorer => {
                    #[cfg(windows)]
                    { (|| -> anyhow::Result<()> {
                        if directory {
                            let target = multi_launcher::file_search::actions::checked_explorer_target(&path, true)?;
                            multi_launcher::actions::exec::launch_association_if_current(target.as_os_str(), current)
                        } else { multi_launcher::file_search::actions::reveal_checked_file_if_current(&path, current) }
                    })().map_err(|error| format!("Could not open in Explorer: {error:#}")) }
                    #[cfg(not(windows))]
                    { let _ = (path, current, directory); Err("Explorer requires Windows.".into()) }
                }
                PreviewAction::CopyPath | PreviewAction::CopyLine => copy_if_current(&multi_launcher::clipboard_modify::ArboardClipboardBackend, payload.as_deref().unwrap_or_default(), &current),
            }
        });
        let root = self.root.clone();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let _permit = permit;
            let result = task.await;
            let _ = root.update(cx, |root, cx| {
                root.external_launch.finish_auxiliary_effect(&lease);
                if action.is_clipboard() {
                    if root.launcher_search_error.as_deref() == Some(clipboard_modify_view::LAUNCHER_EFFECT_WAIT) { root.launcher_search_error = None; }
                    if let Some(view) = &root.clipboard_modify_view { view.update(cx, |view, cx| view.launcher_effect_settled(cx)); }
                }
                if root.restart_status.as_deref() == Some(WAIT) { root.restart_status = None; }
                cx.notify();
            });
            let _ = weak.update(cx, |view, cx| {
                view.pending = false;
                view.focus_guard = None; view.activation_guard = None; view.effect_lease = None;
                if mount.is_current() && mount.matches(&view.action_mount) && view.scope_current()
                    && lease.load(Ordering::Acquire)
                    && view.parent.upgrade().is_some_and(|parent| parent.read(cx).preview_is_current(cx.entity_id(), &view.receipt)) {
                    view.feedback = Some(match result { Ok(()) if action.is_clipboard() => "Copied to clipboard.".into(),
                        Ok(()) => "Opened in Explorer.".into(), Err(message) => message });
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    fn refresh_lines(&mut self, new_page: bool) {
        self.rendered.clear();
        if let PreviewLoadingState::Loaded { preview, .. } = &self.state.state {
            self.line_offset = self.line_offset.min(preview.lines.len().saturating_sub(MAX_LINES));
            if new_page {
                self.max_column = preview.lines.iter().skip(self.line_offset).take(MAX_LINES)
                    .map(|line| line.text.chars().count().saturating_sub(1)).max().unwrap_or(0);
            }
            self.column_offset = self.column_offset.min(self.max_column);
            self.rendered = preview.lines.iter().skip(self.line_offset).take(MAX_LINES)
                .map(|line| (line.line_number, line_segments(line, self.column_offset))).collect();
        }
        self.scroll.set_offset(point(px(0.0), px(0.0)));
    }

    fn navigate(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let PreviewLoadingState::Loaded { preview, .. } = &self.state.state else { return false; };
        if preview.lines.is_empty() { return false; }
        let old_line = self.line_offset;
        let old_column = self.column_offset;
        if let Some(offset) = line_navigation(old_line, preview.lines.len(), key) {
            self.line_offset = offset;
        } else {
            match key {
                "left" => self.column_offset = old_column.saturating_sub(MAX_LINE_CHARS),
                "right" => self.column_offset = old_column.saturating_add(MAX_LINE_CHARS).min(self.max_column),
                _ => return false,
            }
        }
        if old_line != self.line_offset || old_column != self.column_offset {
            self.refresh_lines(old_line != self.line_offset);
            cx.notify();
        }
        true
    }

    pub(super) fn load(&mut self, request: PreviewRequest, permit: EffectPermit, cx: &mut Context<Self>) {
        let epoch = self.state.begin_request();
        let mount = self.mount.clone();
        let worker_mount = mount.clone();
        let admission = self.admission.clone();
        let found = self.found.clone();
        let task = cx.background_executor().spawn(async move {
            if !worker_mount.is_current() || admission.is_fenced() { return None; }
            let mut preview = preview_file(&request);
            let selected_index = found.as_ref().and_then(|found| preview.lines.iter().position(|line| line.line_number == found.line_number)).unwrap_or(0);
            let match_present = found.as_ref().is_some_and(|found| preview.lines.get(selected_index)
                .is_some_and(|line| line.line_number == found.line_number && line.text == found.line));
            if !match_present { for line in &mut preview.lines { line.match_ranges.clear(); } }
            let column = preview.lines.get(selected_index).and_then(|line| line.match_ranges.first())
                .map_or(0, |range| range.start_column.saturating_sub(81));
            Some((preview, selected_index.saturating_sub(1), column, match_present))
        });
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = task.await;
            let _ = weak.update(cx, |view, cx| {
                let presentation_current = if view.popup {
                    popup_load_current(mount.is_current(), view.native.is_some_and(native_window_is_valid), view.admission.is_fenced())
                } else {
                    inline_load_current(mount.is_current(), json_utility::native_host_current(&view.visibility,
                        view.visibility.request_revision.load(Ordering::Acquire), view.native, false), view.admission.is_fenced())
                };
                if !presentation_current || !mount.matches(&view.mount) || view.state.current_request_id != Some(epoch) { return; }
                let Some(parent) = view.parent.upgrade() else { return; };
                if !parent.read(cx).preview_is_current(cx.entity_id(), &view.receipt) { return; }
                let outcome = match result {
                    Some((preview, line_offset, column_offset, match_present)) => {
                        view.line_offset = line_offset; view.column_offset = column_offset; view.match_present = match_present;
                        PreviewLoadOutcome::Loaded(preview)
                    }
                    None => PreviewLoadOutcome::Failed("Preview was cancelled.".into()),
                };
                if view.state.complete_request(epoch, outcome) { view.refresh_lines(true); cx.notify(); }
            });
        }).detach();
    }
}

impl Render for FileSearchPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut pane = div().id("file-search-preview-document").track_focus(&self.document_focus).flex().flex_col().gap_2()
            .capture_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                if event.keystroke.modifiers == Modifiers::default() && view.document_focus.is_focused(window)
                    && view.navigate(&event.keystroke.key, cx) {
                    window.prevent_default(); cx.stop_propagation();
                }
            }))
            .child(Label::new("Preview"))
            .child(Label::new(self.receipt.path.display().to_string()));
        match &self.state.state {
            PreviewLoadingState::Idle | PreviewLoadingState::Loading { .. } => { pane = pane.child(Label::new("Loading preview…")); }
            PreviewLoadingState::Failed { error, .. } => { pane = pane.child(Label::new(format!("Could not load preview: {error}"))); }
            PreviewLoadingState::Loaded { preview, .. } => {
                if let Some(error) = &preview.error { pane = pane.child(Label::new(format!("Could not load preview: {}", error.message))); }
                if let Some(unsupported) = &preview.binary_or_unsupported { pane = pane.child(Label::new(unsupported.reason.clone())); }
                for warning in &preview.warnings { pane = pane.child(Label::new(warning.clone())); }
                if self.found.is_some() && !self.match_present { pane = pane.child(Label::new("The selected match is no longer present in the loaded file.")); }
                let mut lines = div().id("file-search-preview-lines").flex().flex_col().max_h(px(360.0)).overflow_y_scroll().track_scroll(&self.scroll);
                let coverage = if preview.metadata.is_directory { "Directory" } else if self.found.is_some()
                    && matches!(preview.coverage, PreviewCoverage::Complete | PreviewCoverage::MatchContextOnly) { "Match context" } else { match preview.coverage {
                    PreviewCoverage::Complete => "Complete", PreviewCoverage::BeginningOnly => "Beginning",
                    PreviewCoverage::MatchContextOnly => "Match context", PreviewCoverage::BinaryUnsupported => "Binary",
                    PreviewCoverage::Unsupported => "Unsupported", PreviewCoverage::ReadError => "Read error",
                }};
                pane = pane.child(Label::new(format!("{coverage} · {} bytes{}", preview.metadata.len_bytes,
                    if preview.metadata.readonly { " · read-only" } else { "" })));
                if !preview.lines.is_empty() {
                    lines = lines.child(Label::new(format!("Lines {}–{} · {} loaded · Column {}",
                        self.rendered.first().map_or(0, |row| row.0), self.rendered.last().map_or(0, |row| row.0), preview.lines.len(), self.column_offset + 1)));
                }
                for (number, segments) in &self.rendered {
                    let mut row = div().flex().gap_1().child(Label::new(format!("{number}")));
                    for (text, matched) in segments {
                        row = row.child(div().when(*matched, |segment| segment.bg(rgb(0x8d6b13))).child(text.clone()));
                    }
                    lines = lines.child(row);
                }
                pane = pane.child(lines.vertical_scrollbar(&self.scroll));
            }
        }
        let line_count = match &self.state.state {
            PreviewLoadingState::Loaded { preview, .. } => preview.lines.len(), _ => 0,
        };
        if line_count > 0 {
            pane = pane.child(div().flex().flex_wrap().gap_2()
                .child(Button::new("preview-prev").label("Previous lines").disabled(self.line_offset == 0).on_click(cx.listener(|view, _, _, cx| { view.navigate("pageup", cx); })))
                .child(Button::new("preview-next").label("Next lines").disabled(self.line_offset + MAX_LINES >= line_count).on_click(cx.listener(|view, _, _, cx| { view.navigate("pagedown", cx); })))
                .child(Button::new("preview-left").label("Previous columns").disabled(self.column_offset == 0).on_click(cx.listener(|view, _, _, cx| { view.navigate("left", cx); })))
                .child(Button::new("preview-right").label("Next columns").disabled(self.column_offset.saturating_add(MAX_LINE_CHARS) > self.max_column).on_click(cx.listener(|view, _, _, cx| { view.navigate("right", cx); }))));
        }
        let parent_escape = self.parent.clone(); let escape_receipt = self.action_mount.clone(); let child_id = cx.entity_id();
        let mut pane = div().id("file-search-preview").track_focus(&self.focus).flex().flex_col().gap_2().p_3()
            .capture_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                if view.popup && view.action_is_current(&escape_receipt) && event.keystroke.key == "escape"
                    && event.keystroke.modifiers == Modifiers::default() && view.owns_focus(window, cx) {
                    window.prevent_default(); cx.stop_propagation();
                    if !event.is_held { let parent = parent_escape.clone(); let receipt = escape_receipt.clone();
                        window.defer(cx, move |window, app| { let _ = parent.update(app, |parent, cx|
                            parent.close_preview_if_action(child_id, &receipt, true, window, cx)); }); }
                }
            })).child(pane);
        if let Some(message) = &self.feedback { pane = pane.child(Label::new(message.clone())); }
        let parent = self.parent.clone();
        let owner = cx.entity_id();
        let explorer_receipt = self.action_mount.clone();
        let path_receipt = self.action_mount.clone();
        let line_receipt = self.action_mount.clone();
        let return_receipt = self.action_mount.clone();
        pane.child(div().flex().flex_wrap().gap_2()
            .child(Button::new("file-search-preview-explorer").label("Open in Explorer").tab_stop(true)
                .track_focus(&self.explorer_focus).disabled(self.pending)
                .on_click(cx.listener(move |view, _, window, cx| view.start_action(PreviewAction::Explorer, &explorer_receipt, window, cx))))
            .child(Button::new("file-search-preview-copy-path").label("Copy Full Path").tab_stop(true)
                .track_focus(&self.copy_path_focus).disabled(self.pending)
                .on_click(cx.listener(move |view, _, window, cx| view.start_action(PreviewAction::CopyPath, &path_receipt, window, cx))))
            .when(self.found.is_some(), |row| row.child(Button::new("file-search-preview-copy-line").label("Copy Matching Line").tab_stop(true)
                .track_focus(&self.copy_line_focus).disabled(self.pending)
                .on_click(cx.listener(move |view, _, window, cx| view.start_action(PreviewAction::CopyLine, &line_receipt, window, cx)))))
            .child(Button::new("file-search-preview-return").label("Return to Search").tab_stop(true).on_click(cx.listener(move |_, _, window, cx| {
                let parent = parent.clone();
                let receipt = return_receipt.clone();
                window.defer(cx, move |window, app| {
                    let _ = parent.update(app, |parent, cx| parent.close_preview_if_action(owner, &receipt, false, window, cx));
                });
            }))))
    }
}

#[cfg(test)]
mod tests {
    use super::{copy_if_current, line_segments, line_navigation, PreviewAction, MAX_LINE_CHARS, MAX_LINES};
    use multi_launcher::file_search::{model::ContentMatch, preview::{preview_file, PreviewRequest}};
    #[derive(Default)]
    struct FakeClipboard { writes: std::sync::Mutex<Vec<String>>, revoke_during_acquire: bool,
        ordinary_calls: std::sync::atomic::AtomicUsize, acquired_calls: std::sync::atomic::AtomicUsize }
    impl multi_launcher::clipboard_modify::clipboard::ClipboardBackend for FakeClipboard {
        fn read_text(&self) -> Result<String, multi_launcher::clipboard_modify::clipboard::ClipboardError> { Ok(String::new()) }
        fn write_text(&self, text: &str) -> Result<(), multi_launcher::clipboard_modify::clipboard::ClipboardError> {
            self.writes.lock().unwrap().push(text.to_owned()); Ok(())
        }
        fn write_text_if(&self, text: &str, current: &dyn Fn() -> bool) -> Result<bool, multi_launcher::clipboard_modify::clipboard::ClipboardError> {
            self.ordinary_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.revoke_during_acquire || !current() { return Ok(false); }
            self.write_text(text)?; Ok(true)
        }
        fn write_text_acquired_if(&self, text: &str, current: &dyn Fn() -> bool) -> Result<bool, multi_launcher::clipboard_modify::clipboard::ClipboardError> {
            self.acquired_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.revoke_during_acquire || !current() { return Ok(false); }
            self.write_text(text)?; Ok(true)
        }
    }

    #[test]
    fn preview_copy_keeps_full_unicode_payload_and_checks_late_authority() {
        let line = format!("é{}終", "λ".repeat(512));
        let path = std::path::Path::new("owned/é.txt");
        assert_eq!(PreviewAction::CopyLine.payload(path, Some(&line)).as_deref(), Some(line.as_str()));
        assert_eq!(PreviewAction::CopyPath.payload(path, Some(&line)), Some(path.display().to_string()));
        assert_eq!(PreviewAction::CopyLine.payload(path, None), None);
        let revoked = FakeClipboard { revoke_during_acquire: true, ..Default::default() };
        assert!(copy_if_current(&revoked, &line, &|| true).is_err());
        assert!(revoked.writes.lock().unwrap().is_empty());
        let allowed = FakeClipboard::default();
        assert!(copy_if_current(&allowed, &line, &|| false).is_err());
        assert!(copy_if_current(&allowed, "before\0after", &|| true).is_err());
        assert!(allowed.writes.lock().unwrap().is_empty());
        assert!(copy_if_current(&allowed, &line, &|| true).is_ok());
        assert_eq!(allowed.writes.lock().unwrap().as_slice(), &[line]);
        #[cfg(windows)]
        {
            assert_eq!(allowed.acquired_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
            assert_eq!(allowed.ordinary_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        }
        #[cfg(not(windows))]
        assert_eq!(allowed.ordinary_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn popup_shell_never_projects_child_before_host_commit() {
        assert!(!super::preview_shell_owns_child(1, 2));
        assert!(super::preview_shell_owns_child(2, 2));
        assert!(!super::preview_shell_owns_child(3, 2));
    }

    #[test]
    fn popup_action_requires_live_selection_parent_child_and_foreground() {
        use super::popup_action_authorized as current;
        assert!(current(true, true, true, true, true, true));
        assert!(!current(false, true, true, true, true, true));
        assert!(!current(true, false, true, true, true, true));
        assert!(!current(true, true, false, true, true, true));
        assert!(!current(true, true, true, false, true, true));
        assert!(!current(true, true, true, true, false, true));
        assert!(!current(true, true, true, true, true, false));
    }

    #[test]
    fn inline_load_keeps_its_root_scope_across_parent_action_rotation() {
        let selection_owner = super::super::utility_mount::MountOwner::default();
        let selection = selection_owner.attach();
        let parent_owner = super::super::utility_mount::MountOwner::default();
        let original_parent = parent_owner.attach();
        let replacement_parent = parent_owner.attach();
        assert!(!original_parent.is_current());
        assert!(replacement_parent.is_current());
        assert!(super::inline_load_current(selection.is_current(), true, false));
        assert!(!super::inline_load_current(selection.is_current(), false, false));
        selection_owner.retire();
        assert!(!super::inline_load_current(selection.is_current(), true, false));
    }

    #[test]
    fn popup_load_survives_action_rotation_but_not_selection_retirement() {
        let selection_owner = super::super::utility_mount::MountOwner::default();
        let selection = selection_owner.attach();
        let action_owner = super::super::utility_mount::MountOwner::default();
        let old = action_owner.attach();
        action_owner.retire();
        assert!(!old.is_current());
        assert!(super::popup_load_current(selection.is_current(), true, false));
        let rebound = action_owner.attach();
        assert!(rebound.is_current());
        assert!(!old.matches(&rebound));
        selection_owner.retire();
        assert!(!super::popup_load_current(selection.is_current(), true, false));
        assert!(!super::popup_load_current(true, false, false));
        assert!(!super::popup_load_current(true, true, true));
    }

    #[test]
    fn replacing_action_mount_keeps_selection_and_load_owner_current() {
        let selection_owner = super::super::utility_mount::MountOwner::default();
        let selection = selection_owner.attach();
        let action_owner = super::super::utility_mount::MountOwner::default();
        let old = action_owner.attach();
        let replacement = action_owner.attach();
        assert!(selection.is_current());
        assert!(!old.is_current());
        assert!(replacement.is_current());
        selection_owner.retire();
        assert!(!selection.is_current());
    }

    #[test]
    fn retired_preview_mount_cannot_write_clipboard() {
        let owner = super::super::utility_mount::MountOwner::default();
        let receipt = owner.attach();
        let clipboard = FakeClipboard::default();
        owner.retire();
        assert!(copy_if_current(&clipboard, "private", &|| receipt.is_current()).is_err());
        assert!(clipboard.writes.lock().unwrap().is_empty());
    }

    #[test]
    fn preview_navigation_reaches_beginning_and_late_lines() {
        assert_eq!(line_navigation(0, 100, "pagedown"), Some(MAX_LINES));
        assert_eq!(line_navigation(0, 100, "end"), Some(100 - MAX_LINES));
        assert_eq!(line_navigation(99, 100, "down"), Some(100 - MAX_LINES));
        assert_eq!(line_navigation(99, 100, "home"), Some(0));
        assert_eq!(line_navigation(0, 0, "up"), Some(0));
        assert_eq!(line_navigation(0, 100, "escape"), None);
    }

    #[test]
    fn bounded_unicode_line_marks_the_exact_visible_match() {
        let text = format!("{}éneedle{}", "界".repeat(500), "λ".repeat(500));
        let found = ContentMatch::new(3, text.clone(), 1500, 1508);
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("unicode.txt");
        std::fs::write(&path, format!("first\nsecond\n{text}\n")).unwrap();
        let request = PreviewRequest::from_search_result(&path, Some(&found), 1024 * 1024);
        let preview = preview_file(&request);
        let line = preview.lines.iter().find(|line| line.line_number == 3).unwrap();
        let segments = line_segments(line, 420);
        assert_eq!(segments.iter().filter(|(_, marked)| *marked).map(|(text, _)| text.as_str()).collect::<String>(), "éneedle");
        assert!(segments.iter().map(|(text, _)| text.chars().count()).sum::<usize>() <= MAX_LINE_CHARS + 2);
    }
}
