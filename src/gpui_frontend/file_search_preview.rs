//! Retained read-only preview for one exact File Search content match.
use super::*;
use multi_launcher::file_search::preview::{preview_file, PreviewLoadOutcome, PreviewLoadStateMachine, PreviewLoadingState, PreviewRequest, PreviewLine};

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

pub(super) struct FileSearchPreview {
    parent: WeakEntity<file_search::FileSearchView>,
    receipt: file_search::SelectionReceipt,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    focus: FocusHandle,
    pub(super) return_focus: FocusHandle,
    pub(super) return_scroll: Point<Pixels>,
    host: WindowId,
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
    pub(super) fn new(parent: WeakEntity<file_search::FileSearchView>, receipt: file_search::SelectionReceipt,
        return_focus: FocusHandle, return_scroll: Point<Pixels>, host: WindowId, found: multi_launcher::file_search::model::ContentMatch, owner: utility_mount::MountOwner,
        mount: utility_mount::MountReceipt, visibility: RootVisibility, revision: u64, native: NativeWindow,
        admission: Arc<RestartAdmission>, cx: &mut Context<Self>) -> Self {
        Self { parent, receipt, owner, mount, focus: cx.focus_handle(), return_focus, return_scroll, host, visibility, revision, native, admission,
            state: PreviewLoadStateMachine::default(), found, rendered: Vec::new(), match_present: false, scroll: ScrollHandle::new() }
    }
    pub(super) fn focus(&self, window: &mut Window, cx: &mut Context<Self>) { self.focus.focus(window, cx); }
    pub(super) fn owns_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus.is_focused(window) || self.focus.contains_focused(window, cx)
    }
    pub(super) fn host(&self) -> WindowId { self.host }
    pub(super) fn receipt(&self) -> &file_search::SelectionReceipt { &self.receipt }
    pub(super) fn scope_current(&self) -> bool {
        self.mount.is_current() && !self.admission.is_fenced()
            && json_utility::native_host_current(&self.visibility, self.revision, Some(self.native), false)
    }
    pub(super) fn retire(&mut self) { self.owner.retire(); self.state.clear(); self.rendered.clear(); }
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
        let parent = self.parent.clone();
        let owner = cx.entity_id();
        pane.child(Button::new("file-search-preview-return").label("Return to Search").tab_stop(true).on_click(cx.listener(move |_, _, window, cx| {
            let parent = parent.clone();
            window.defer(cx, move |window, app| {
                let _ = parent.update(app, |parent, cx| parent.close_preview(owner, window, cx));
            });
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::{line_segments, MAX_LINE_CHARS};
    use multi_launcher::file_search::{model::ContentMatch, preview::{preview_file, PreviewRequest}};
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
