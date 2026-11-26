//! Retained read-only preview for one exact File Search content match.
use super::*;
use multi_launcher::file_search::preview::{preview_file, PreviewLoadOutcome, PreviewLoadStateMachine, PreviewLoadingState, PreviewRequest, PreviewLine};

pub(super) const WAIT: &str = "Wait for the File Search preview action to finish.";
const MAX_LINES: usize = 7;
const MAX_LINE_CHARS: usize = 240;

fn line_segments(line: &PreviewLine) -> Vec<(String, bool)> {
    let total = line.text.chars().count();
    let selected = line.match_ranges.first().map(|range| (range.start_column.saturating_sub(1), range.end_column.saturating_sub(1)));
    let start = selected.map_or(0, |(start, _)| start.saturating_sub(80)).min(total);
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewAction { Explorer, CopyPath, CopyLine }
impl PreviewAction {
    fn is_clipboard(self) -> bool { !matches!(self, Self::Explorer) }
    fn payload(self, path: &std::path::Path, line: &str) -> Option<String> {
        match self { Self::Explorer => None, Self::CopyPath => Some(path.display().to_string()), Self::CopyLine => Some(line.to_owned()) }
    }
}

fn copy_if_current(backend: &dyn multi_launcher::clipboard_modify::clipboard::ClipboardBackend,
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

pub(super) struct FileSearchPreview {
    parent: WeakEntity<file_search::FileSearchView>,
    root: WeakEntity<RootWindow>,
    receipt: file_search::SelectionReceipt,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    focus: FocusHandle,
    explorer_focus: FocusHandle,
    copy_path_focus: FocusHandle,
    copy_line_focus: FocusHandle,
     pending: bool,
    effect_lease: Option<Arc<AtomicBool>>,
    focus_guard: Option<Subscription>,
    activation_guard: Option<Subscription>,
    feedback: Option<String>,
    pub(super) return_focus: FocusHandle,
    pub(super) return_scroll: Point<Pixels>,
    host: AnyWindowHandle,
    visibility: RootVisibility,
    revision: u64,
    native: NativeWindow,
    admission: Arc<RestartAdmission>,
    state: PreviewLoadStateMachine,
    found: multi_launcher::file_search::model::ContentMatch,
    rendered: Vec<(usize, Vec<(String, bool)>)>,
    match_present: bool,
    scroll: ScrollHandle,
}

impl FileSearchPreview {
    pub(super) fn new(parent: WeakEntity<file_search::FileSearchView>, root: WeakEntity<RootWindow>, receipt: file_search::SelectionReceipt,
        return_focus: FocusHandle, return_scroll: Point<Pixels>, host: AnyWindowHandle, found: multi_launcher::file_search::model::ContentMatch, owner: utility_mount::MountOwner,
        mount: utility_mount::MountReceipt, visibility: RootVisibility, revision: u64, native: NativeWindow,
        admission: Arc<RestartAdmission>, cx: &mut Context<Self>) -> Self {
        Self { parent, root, receipt, owner, mount, focus: cx.focus_handle(), explorer_focus: cx.focus_handle(),
            copy_path_focus: cx.focus_handle(), copy_line_focus: cx.focus_handle(), pending: false,
            effect_lease: None, focus_guard: None, activation_guard: None, feedback: None,
            return_focus, return_scroll, host, visibility, revision, native, admission,
            state: PreviewLoadStateMachine::default(), found, rendered: Vec::new(), match_present: false, scroll: ScrollHandle::new() }
    }
    pub(super) fn focus(&self, window: &mut Window, cx: &mut Context<Self>) { self.focus.focus(window, cx); }
    pub(super) fn owns_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus.is_focused(window) || self.focus.contains_focused(window, cx)
    }
    pub(super) fn host(&self) -> WindowId { self.host.window_id() }
    pub(super) fn receipt(&self) -> &file_search::SelectionReceipt { &self.receipt }
    pub(super) fn scope_current(&self) -> bool {
        self.mount.is_current() && !self.admission.is_fenced()
            && json_utility::native_host_current(&self.visibility, self.revision, Some(self.native), false)
    }
    pub(super) fn retire(&mut self) {
        if let Some(lease) = &self.effect_lease { lease.store(false, Ordering::Release); }
        self.owner.retire(); self.state.clear(); self.rendered.clear();
    }
    fn start_action(&mut self, action: PreviewAction, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || !self.scope_current()
            || self.host.window_id() != window.window_handle().window_id() || !window.is_window_active()
            || window.has_active_dialog(cx) || current_foreground_identity() != Some(self.native.identity) { return; }
        let Some(parent) = self.parent.upgrade() else { return; };
        if !parent.read(cx).preview_is_current(cx.entity_id(), &self.receipt) { return; }
        let Some(root) = self.root.upgrade() else { return; };
        let before = window.focused(cx);
        {
            let root_state = root.read(cx);
            if root_state.root_window.is_none_or(|host| host.window_id() != self.host.window_id())
                || root_state.native_window != Some(self.native)
                || !root_state.external_owner_is_current(before.as_ref(), window, cx) { return; }
        }
        let focus = match action { PreviewAction::Explorer => &self.explorer_focus, PreviewAction::CopyPath => &self.copy_path_focus, PreviewAction::CopyLine => &self.copy_line_focus }.clone();
        focus.focus(window, cx);
        if !focus.is_focused(window) { return; }
        let Some(permit) = self.admission.admit() else { return; };
        let lease = Arc::new(AtomicBool::new(true));
        let admitted = root.update(cx, |root, cx| {
            if !root.external_owner_is_current(Some(&focus), window, cx) || window.has_active_dialog(cx)
                || !window.is_window_active() || current_foreground_identity() != Some(self.native.identity) { return false; }
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
        let payload = action.payload(&self.receipt.path, &self.found.line);
        let path = self.receipt.path.clone();
        let mount = self.mount.clone();
        let worker_mount = mount.clone();
        let visibility = self.visibility.clone();
        let revision = self.revision;
        let native = self.native;
        let admission = self.admission.clone();
        let worker_lease = lease.clone();
        let task = cx.background_executor().spawn(async move {
            let current = || worker_lease.load(Ordering::Acquire) && worker_mount.is_current() && !admission.is_fenced()
                && json_utility::native_host_current(&visibility, revision, Some(native), false)
                && current_foreground_identity() == Some(native.identity);
            match action {
                PreviewAction::Explorer => {
                    #[cfg(windows)]
                    { multi_launcher::file_search::actions::reveal_checked_file_if_current(&path, current).map_err(|error| format!("Could not open in Explorer: {error:#}")) }
                    #[cfg(not(windows))]
                    { let _ = (path, current); Err("Explorer requires Windows.".into()) }
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
                if mount.is_current() && mount.matches(&view.mount) && view.scope_current()
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

    pub(super) fn load(&mut self, request: PreviewRequest, permit: EffectPermit, cx: &mut Context<Self>) {
        let epoch = self.state.begin_request();
        let mount = self.mount.clone();
        let worker_mount = mount.clone();
        let visibility = self.visibility.clone();
        let revision = self.revision;
        let native = self.native;
        let admission = self.admission.clone();
        let found = self.found.clone();
        let task = cx.background_executor().spawn(async move {
            if !worker_mount.is_current() || admission.is_fenced()
                || !json_utility::native_host_current(&visibility, revision, Some(native), false) { return None; }
            let mut preview = preview_file(&request);
            let selected_index = preview.lines.iter().position(|line| line.line_number == found.line_number).unwrap_or(0);
            let match_present = preview.lines.get(selected_index).is_some_and(|line| line.line_number == found.line_number && line.text == found.line);
            if !match_present { for line in &mut preview.lines { line.match_ranges.clear(); } }
            let start = selected_index.saturating_sub(1);
            let end = start.saturating_add(MAX_LINES).min(preview.lines.len());
            let rendered = preview.lines[start..end].iter().map(|line| (line.line_number, line_segments(line))).collect();
            Some((preview, rendered, match_present))
        });
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = task.await;
            let _ = weak.update(cx, |view, cx| {
                if !mount.is_current() || !mount.matches(&view.mount) || !view.scope_current()
                    || view.state.current_request_id != Some(epoch) { return; }
                let Some(parent) = view.parent.upgrade() else { return; };
                if !parent.read(cx).preview_is_current(cx.entity_id(), &view.receipt) { return; }
                let outcome = match result {
                    Some((preview, rendered, match_present)) => {
                        view.rendered = rendered; view.match_present = match_present;
                        PreviewLoadOutcome::Loaded(preview)
                    }
                    None => PreviewLoadOutcome::Failed("Preview was cancelled.".into()),
                };
                if view.state.complete_request(epoch, outcome) { cx.notify(); }
            });
        }).detach();
    }
}

impl Render for FileSearchPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut pane = div().id("file-search-preview").track_focus(&self.focus).flex().flex_col().gap_2().p_3()
            .child(Label::new("Content Preview"))
            .child(Label::new(self.receipt.path.display().to_string()));
        match &self.state.state {
            PreviewLoadingState::Idle | PreviewLoadingState::Loading { .. } => { pane = pane.child(Label::new("Loading preview…")); }
            PreviewLoadingState::Failed { error, .. } => { pane = pane.child(Label::new(format!("Could not load preview: {error}"))); }
            PreviewLoadingState::Loaded { preview, .. } => {
                if let Some(error) = &preview.error { pane = pane.child(Label::new(format!("Could not load preview: {}", error.message))); }
                if let Some(unsupported) = &preview.binary_or_unsupported { pane = pane.child(Label::new(unsupported.reason.clone())); }
                for warning in &preview.warnings { pane = pane.child(Label::new(warning.clone())); }
                if !self.match_present { pane = pane.child(Label::new("The selected match is no longer present in the loaded file.")); }
                let mut lines = div().id("file-search-preview-lines").flex().flex_col().max_h(px(360.0)).overflow_y_scroll().track_scroll(&self.scroll);
                lines = lines.child(Label::new("Selected-match context"));
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
        if let Some(message) = &self.feedback { pane = pane.child(Label::new(message.clone())); }
        let parent = self.parent.clone();
        let owner = cx.entity_id();
        pane.child(div().flex().flex_wrap().gap_2()
            .child(Button::new("file-search-preview-explorer").label("Open in Explorer").tab_stop(true)
                .track_focus(&self.explorer_focus).disabled(self.pending)
                .on_click(cx.listener(|view, _, window, cx| view.start_action(PreviewAction::Explorer, window, cx))))
            .child(Button::new("file-search-preview-copy-path").label("Copy Full Path").tab_stop(true)
                .track_focus(&self.copy_path_focus).disabled(self.pending)
                .on_click(cx.listener(|view, _, window, cx| view.start_action(PreviewAction::CopyPath, window, cx))))
            .child(Button::new("file-search-preview-copy-line").label("Copy Matching Line").tab_stop(true)
                .track_focus(&self.copy_line_focus).disabled(self.pending)
                .on_click(cx.listener(|view, _, window, cx| view.start_action(PreviewAction::CopyLine, window, cx))))
            .child(Button::new("file-search-preview-return").label("Return to Search").tab_stop(true).on_click(cx.listener(move |_, _, window, cx| {
                let parent = parent.clone();
                window.defer(cx, move |window, app| {
                    let _ = parent.update(app, |parent, cx| parent.close_preview(owner, window, cx));
                });
            }))))
    }
}

#[cfg(test)]
mod tests {
    use super::{copy_if_current, line_segments, PreviewAction, MAX_LINE_CHARS};
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
        assert_eq!(PreviewAction::CopyLine.payload(path, &line).as_deref(), Some(line.as_str()));
        assert_eq!(PreviewAction::CopyPath.payload(path, &line), Some(path.display().to_string()));
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
    fn retired_preview_mount_cannot_write_clipboard() {
        let owner = super::super::utility_mount::MountOwner::default();
        let receipt = owner.attach();
        let clipboard = FakeClipboard::default();
        owner.retire();
        assert!(copy_if_current(&clipboard, "private", &|| receipt.is_current()).is_err());
        assert!(clipboard.writes.lock().unwrap().is_empty());
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
        let segments = line_segments(line);
        assert_eq!(segments.iter().filter(|(_, marked)| *marked).map(|(text, _)| text.as_str()).collect::<String>(), "éneedle");
        assert!(segments.iter().map(|(text, _)| text.chars().count()).sum::<usize>() <= MAX_LINE_CHARS + 2);
    }
}
