//! Retained docked filename-search pane.
use super::*;
use gpui_kit::{KeyUpEvent, base::Selectable};
use multi_launcher::file_search::{
    coordinator::SearchCoordinator,
    model::{
        ContentFileResult, ContentMatch, FileKind, FileSearchResultKey, FilenameResult, PathIdentity,
        SearchBackend, SearchDiagnostic, SearchEvent, SearchId, SearchKind, SearchResult, SearchStatus,
    },
    request::{RequestInput, RootScope, build_request},
    settings::FileSearchSettings,
};

const EVENT_BATCH: usize = 128;
const MOUNTED_ROWS: usize = 100;

pub(super) fn current_row(
    open: bool,
    current_generation: u64,
    current_search: Option<SearchId>,
    row_generation: u64,
    row_search: Option<SearchId>,
    index: usize,
    visible: std::ops::Range<usize>,
) -> bool {
    open && row_search.is_some()
        && current_search == row_search
        && current_generation == row_generation
        && visible.contains(&index)
}

fn popup_return_allowed(child_current: bool, parent_current: bool, selection_current: bool, parent_focus_unchanged: bool) -> bool {
    child_current && parent_current && selection_current && parent_focus_unchanged
}

fn pending_popup_ready(mount: bool, intent: bool, accepted: bool, active: bool, foreground: bool) -> bool {
    mount && intent && accepted && active && foreground
}

fn mounted_search_row_current(
    mount: &utility_mount::MountReceipt, open: bool, generation: u64, search: Option<SearchId>,
    rendered_generation: u64, rendered_search: Option<SearchId>, index: usize, visible: std::ops::Range<usize>,
) -> bool {
    mount.is_current() && current_row(open, generation, search, rendered_generation, rendered_search, index, visible)
}

pub(super) fn eligible_file_row(row: &FilenameResult, expected: &std::path::Path) -> bool {
    row.kind == FileKind::File && row.path == expected
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SelectionReceipt {
    pub owner: EntityId,
    pub search: SearchId,
    pub generation: u64,
    pub index: usize,
    pub path: PathBuf,
    pub kind: FileKind,
    pub content_key: Option<FileSearchResultKey>,
}

#[derive(Clone, Debug)]
struct ContentRow {
    path: PathBuf,
    key: FileSearchResultKey,
    found: ContentMatch,
    heading: String,
    first_in_file: bool,
}

fn match_excerpt(found: &ContentMatch) -> String {
    let line = &found.line;
    let mut split = found.byte_start.min(line.len());
    while !line.is_char_boundary(split) { split -= 1; }
    let left_reversed: String = line[..split].chars().rev().take(60).collect();
    let left: String = left_reversed.chars().rev().collect();
    let right: String = line[split..].chars().take(120).collect();
    let mut excerpt = String::new();
    if left.len() < split { excerpt.push('…'); }
    excerpt.extend(left.chars().map(|ch| if ch == '\n' || ch == '\r' { ' ' } else { ch }));
    excerpt.extend(right.chars().map(|ch| if ch == '\n' || ch == '\r' { ' ' } else { ch }));
    if right.len() < line.len() - split { excerpt.push('…'); }
    excerpt
}

fn append_content_rows(rows: &mut Vec<ContentRow>, occurrences: &mut HashMap<(PathIdentity, usize, usize, usize), usize>, file: ContentFileResult) {
    let identity = PathIdentity::from_path(&file.path);
    let heading = format!("{} — {} · {} matches{}", file.file_name, file.path.display(), file.total_matches,
        if file.truncated { " (per-file limit reached)" } else { "" });
    for (index, found) in file.matches.into_iter().enumerate() {
        let count = occurrences.entry((identity.clone(), found.line_number, found.byte_start, found.byte_end)).or_default();
        let occurrence = *count;
        *count = count.saturating_add(1);
        let key = FileSearchResultKey::Content { path: identity.clone(), line_number: found.line_number,
            byte_start: found.byte_start, byte_end: found.byte_end, occurrence };
        rows.push(ContentRow { path: file.path.clone(), key, found, heading: heading.clone(), first_in_file: index == 0 });
    }
}

struct SearchSource {
    term: String,
    directory: PathBuf,
    kind: SearchKind,
    expected_root: Option<PathBuf>,
}

fn typed_source(draft: multi_launcher::file_search::query::SearchRequestDraft, canonical_root: PathBuf) -> Option<SearchSource> {
    Some(SearchSource { term: draft.search_text, directory: draft.root?, kind: draft.kind, expected_root: Some(canonical_root) })
}

pub(super) struct FileSearchView {
    pub open: bool,
    pub(super) root: WeakEntity<RootWindow>,
    pub(super) admission: Arc<RestartAdmission>,
    settings: FileSearchSettings,
    pub(super) popout: bool,
    pub(super) rehosting: bool,
    pub(super) presentation_error: Option<String>,
    pub(super) geometry_error: Option<String>,
    pub(super) activation_revision: Arc<AtomicU64>,
    pending_popup_typed: Option<(ScopedReceipt, utility_mount::MountReceipt, Arc<AtomicBool>, Arc<AtomicBool>)>,
    mount_owner: utility_mount::MountOwner,
    mount_receipt: utility_mount::MountReceipt,
    host: AnyWindowHandle,
    native: Option<NativeWindow>,
    pub(super) outer_scroll: ScrollHandle,
    coordinator: SearchCoordinator,
    term: Entity<InputState>,
    directory: Entity<InputState>,
    focus: FocusHandle,
    calculate_focus: FocusHandle,
    explorer_focus: FocusHandle,
    preview_focus: FocusHandle,
    content_focus: HashMap<FileSearchResultKey, FocusHandle>,
    preview: Option<Entity<file_search_preview::FileSearchPreview>>,
    preview_popup: Option<AnyWindowHandle>,
    preview_geometry: Option<utility_host::ShellGeometry>,
    preview_retiring: Vec<RetiringHashWindow>,
    preview_owner: utility_mount::MountOwner,
     explorer_authority: Arc<AtomicBool>,
    explorer_pending: bool,
    escape_waiting_for_release: bool,
    generation: u64,
    active: Option<SearchId>,
    validating: bool,
    status: String,
    backend: Option<SearchBackend>,
    results: Vec<FilenameResult>,
    content_rows: Vec<ContentRow>,
    content_files: usize,
    content_total_matches: usize,
    content_occurrences: HashMap<(PathIdentity, usize, usize, usize), usize>,
    kind: SearchKind,
    selected: Option<usize>,
    page: usize,
    results_scroll: ScrollHandle,
    truncated: bool,
    _subscriptions: Vec<Subscription>,
}

impl FileSearchView {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        root: WeakEntity<RootWindow>,
        admission: Arc<RestartAdmission>,
        settings: FileSearchSettings,
        native: Option<NativeWindow>,
    ) -> Self {
        let term = cx.new(|cx| InputState::new(window, cx).placeholder("Search text"));
        let directory = cx.new(|cx| InputState::new(window, cx).placeholder("Directory to search"));
        let focus = cx.focus_handle();
        let explorer_focus = cx.focus_handle();
        let mut subscriptions = vec![
            cx.on_focus_out(&focus, window, |this, _, _, _| {
                this.escape_waiting_for_release = false;
            }),
            cx.observe_window_activation(window, |this, window, _| {
                if !window.is_window_active() {
                    this.escape_waiting_for_release = false;
                    this.explorer_authority.store(false, Ordering::Release);
                }
            }),
            cx.on_blur(&explorer_focus, window, |this, _, _| {
                this.explorer_authority.store(false, Ordering::Release);
            }),
        ];
        for input in [term.clone(), directory.clone()] {
            subscriptions.push(cx.subscribe(
                &input,
                |this, _, event: &InputEvent, cx| { if this.rehosting { return; } match event {
                    InputEvent::Change => {
                        this.invalidate("Search changed. Search again.", cx);
                        cx.notify();
                    }
                    InputEvent::PressEnter {
                        secondary: false,
                        shift: false,
                    } => {
                        this.start(cx);
                    }
                    _ => {}
                } },
            ));
        }
        let mount_owner = utility_mount::MountOwner::default();
        let mount_receipt = mount_owner.attach();
        let host = window.window_handle();
        Self {
            open: true,
            popout: false, rehosting: false, presentation_error: None, geometry_error: None,
            activation_revision: Arc::new(AtomicU64::new(0)), pending_popup_typed: None, mount_owner, mount_receipt, host, native,
            outer_scroll: ScrollHandle::new(),
            root,
            admission,
            coordinator: SearchCoordinator::from_settings(settings.clone()),
            settings,
            term,
            directory,
            focus,
            calculate_focus: cx.focus_handle(),
            explorer_focus,
            preview_focus: cx.focus_handle(),
            content_focus: HashMap::new(),
            preview: None,
            preview_popup: None,
            preview_geometry: None,
            preview_retiring: Vec::new(),
            preview_owner: utility_mount::MountOwner::default(),
             explorer_authority: Arc::new(AtomicBool::new(false)),
            explorer_pending: false,
            escape_waiting_for_release: false,
            generation: 0,
            active: None,
            validating: false,
            status: "Enter a directory and filename, then search.".into(),
            backend: None,
            results: Vec::new(),
            content_rows: Vec::new(),
            content_files: 0,
            content_total_matches: 0,
            content_occurrences: HashMap::new(),
            kind: SearchKind::Filename,
            selected: None,
            page: 0,
            results_scroll: ScrollHandle::new(),
            truncated: false,
            _subscriptions: subscriptions,
        }
    }

    pub(super) fn mount_receipt(&self) -> utility_mount::MountReceipt { self.mount_receipt.clone() }
    pub(super) fn is_mounted_in(&self, id: WindowId, popout: bool) -> bool {
        self.open && self.popout == popout && self.mount_receipt.is_current() && self.host.window_id() == id
    }
    pub(super) fn replace_mount(&mut self, host: AnyWindowHandle, native: Option<NativeWindow>, window: &mut Window, cx: &mut Context<Self>) {
        let term_value = self.term.read(cx).value().to_string();
        let directory_value = self.directory.read(cx).value().to_string();
        let term = cx.new(|cx| InputState::new(window, cx).placeholder("Search text"));
        let directory = cx.new(|cx| InputState::new(window, cx).placeholder("Directory to search"));
        term.update(cx, |input, cx| input.set_value(term_value, window, cx));
        directory.update(cx, |input, cx| input.set_value(directory_value, window, cx));
        if self.rehosting {
            term.update(cx, |input, cx| input.set_readonly(true, cx));
            directory.update(cx, |input, cx| input.set_readonly(true, cx));
        }
        self.retire_preview(cx);
        self.retire_explorer();
        self.pending_popup_typed = None;
        self.mount_receipt = self.mount_owner.attach();
        self.host = host;
        self.native = native;
        self.focus = cx.focus_handle();
        self.calculate_focus = cx.focus_handle();
        self.explorer_focus = cx.focus_handle();
        self.preview_focus = cx.focus_handle();
        self.content_focus.clear();
        self.term = term;
        self.directory = directory;
        let receipt = self.mount_receipt.clone();
        let mut subscriptions = Vec::new();
        let focus_receipt = receipt.clone();
        subscriptions.push(cx.on_focus_out(&self.focus, window, move |this, _, _, _| {
            if focus_receipt.is_current() { this.escape_waiting_for_release = false; }
        }));
        let activation_receipt = receipt.clone();
        subscriptions.push(cx.observe_window_activation(window, move |this, window, _| {
            if activation_receipt.is_current() && !window.is_window_active() {
                this.escape_waiting_for_release = false; this.retire_explorer();
            }
        }));
        let blur_receipt = receipt.clone();
        subscriptions.push(cx.on_blur(&self.explorer_focus, window, move |this, _, _| {
            if blur_receipt.is_current() { this.retire_explorer(); }
        }));
        for input in [self.term.clone(), self.directory.clone()] {
            let receipt = receipt.clone();
            subscriptions.push(cx.subscribe(&input, move |this, _, event: &InputEvent, cx| {
                if !receipt.is_current() || this.rehosting { return; }
                match event {
                    InputEvent::Change => { this.invalidate("Search changed. Search again.", cx); cx.notify(); }
                    InputEvent::PressEnter { secondary: false, shift: false } => this.start(cx),
                    _ => {}
                }
            }));
        }
        self._subscriptions = subscriptions;
    }
    pub(super) fn focus_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mount_receipt.is_current() && self.host.window_id() == window.window_handle().window_id() {
            self.term.focus_handle(cx).focus(window, cx);
        }
    }

    pub(super) fn wake_pending_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !window.is_window_active() || !self.is_mounted_in(window.window_handle().window_id(), true) || self.rehosting { return; }
        let Some((_, mount, intent, accepted)) = self.pending_popup_typed.as_ref() else { return; };
        if !mount.is_current() || !intent.load(Ordering::Acquire) { self.pending_popup_typed = None; return; }
        if !pending_popup_ready(mount.is_current(), intent.load(Ordering::Acquire), accepted.load(Ordering::Acquire),
            window.is_window_active(), self.native.is_some_and(|native| current_foreground_identity() == Some(native.identity))) { return; }
        let Some((typed, mount, intent, _)) = self.pending_popup_typed.take() else { return; };
        let root = self.root.clone();
        let view = cx.entity().downgrade();
        window.defer(cx, move |window, app| {
            let _ = root.update(app, |root, cx| {
                if !intent.load(Ordering::Acquire) || !mount.is_current() || !root.file_search_typed_receipt_current(&typed, cx) { return; }
                let _ = view.update(cx, |view, cx| {
                    if mount.is_current() && view.is_mounted_in(window.window_handle().window_id(), true) && window.is_window_active()
                        && view.native.is_some_and(|native| current_foreground_identity() == Some(native.identity)) {
                        view.start_typed(typed, window, cx);
                    }
                });
            });
        });
    }
    pub(super) fn host_native(&self) -> Option<NativeWindow> { self.native }
    pub(super) fn host_focus_is_exact(&self, window: &Window, cx: &App) -> bool {
        self.term.focus_handle(cx).is_focused(window)
    }
    pub(super) fn set_host_rehosting(&mut self, value: bool, cx: &mut Context<Self>) {
        self.rehosting = value;
        self.term.update(cx, |input, cx| input.set_readonly(value, cx));
        self.directory.update(cx, |input, cx| input.set_readonly(value, cx));
    }
    pub(super) fn host_effect_pending(&self, cx: &App) -> bool {
        self.explorer_pending || self.preview_retiring.iter().any(|item| item.native_window.is_none())
            || self.preview.as_ref().is_some_and(|preview| preview.read(cx).effect_pending())
    }
    pub(super) fn request_rehost(&mut self, popout: bool, cx: &mut Context<Self>) {
        if !self.open || self.rehosting || self.explorer_pending || self.admission.is_fenced() || self.popout == popout || !self.mount_receipt.is_current() { return; }
        let Some(root) = self.root.upgrade() else { return; };
        if self.preview.is_some() { self.status = "Close Preview before changing windows.".into(); cx.notify(); return; }
        if root.read(cx).external_launch.auxiliary_effect_pending() { return; }
        let visibility_revision = root.read(cx).root_visibility.request_revision.load(Ordering::Acquire);
        let gesture = self.native.map(|source| hash_dock::Gesture {
            source, foreground_authorized: native_window_is_valid(source) && current_foreground_identity() == Some(source.identity),
            visibility_revision,
        });
        self.retire_explorer();
        self.set_host_rehosting(true, cx);
        self.activation_revision.fetch_add(1, Ordering::AcqRel);
        let source = self.mount_receipt.clone();
        let root = self.root.clone();
        cx.spawn(async move |_, cx| { let _ = root.update(cx, |root, cx| root.start_file_search_rehost(popout, gesture, source, cx)); }).detach();
        cx.notify();
    }

    fn invalidate(&mut self, message: &str, cx: &mut Context<Self>) {
        self.explorer_authority.store(false, Ordering::Release);
        self.retire_preview(cx);
        self.content_focus.clear();
        self.generation = self.generation.wrapping_add(1).max(1);
        if self.active.take().is_some() {
            self.coordinator.cancel_and_retire();
            // Close the retired receiver so queued results cannot delay the next search.
            self.coordinator = SearchCoordinator::from_settings(self.settings.clone());
        }
        self.validating = false;
        self.results.clear();
        self.content_rows.clear();
        self.content_files = 0;
        self.content_total_matches = 0;
        self.content_occurrences.clear();
        self.set_page(0, cx);
        self.truncated = false;
        self.backend = None;
        self.status = message.to_owned();
    }

    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            self.status = match self.kind {
                SearchKind::Filename => "Enter a directory and filename, then search.",
                SearchKind::Content => "Enter a directory and content, then search.",
            }.to_owned();
        }
        self.open = true;
        self.term.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    pub(super) fn owns_focus(&self, window: &Window, cx: &App) -> bool {
        self.open && self.focus.contains_focused(window, cx)
    }

    pub(super) fn hash_action_focus(&self, _: &App) -> FocusHandle { self.calculate_focus.clone() }
    pub(super) fn hash_action_is_focused(&self, window: &Window) -> bool {
        self.calculate_focus.is_focused(window)
    }

    pub(super) fn retire_explorer(&self) {
        self.explorer_authority.store(false, Ordering::Release);
    }

    fn track_retiring_preview(&mut self, popup: AnyWindowHandle, native: Option<NativeWindow>, cx: &mut Context<Self>) -> Option<Arc<HashWakeSignal>> {
        let id = popup.window_id();
        if self.preview_retiring.iter().any(|item| item.window.window_id() == id) { return None; }
        self.preview_retiring.push(RetiringHashWindow { window: popup, native_window: native });
        let weak = cx.entity().downgrade();
        if native.is_none() {
            let signal = Arc::new(HashWakeSignal::default());
            let closed_signal = signal.clone();
            let subscription = cx.on_window_closed(move |app, closed| {
                if closed == id {
                    let _ = weak.update(app, |view, _| view.preview_retiring.retain(|item| item.window.window_id() != id));
                    closed_signal.notify();
                }
            });
            let waiter = signal.clone();
            cx.spawn(async move |_, _| { let _subscription = subscription; waiter.wait().await; }).detach();
            Some(signal)
        } else {
            cx.spawn(async move |_, cx| {
                if wait_for_hash_window_retirement(cx, native).await {
                    let _ = weak.update(cx, |view, _| view.preview_retiring.retain(|item| item.window.window_id() != id));
                }
            }).detach();
            None
        }
    }
    fn remove_retiring_preview(&mut self, popup: AnyWindowHandle, signal: Option<Arc<HashWakeSignal>>, cx: &mut Context<Self>) {
        if popup.update(cx, |_, window, _| window.remove_window()).is_err() {
            self.preview_retiring.retain(|item| item.window.window_id() != popup.window_id());
            if let Some(signal) = signal { signal.notify(); }
        }
    }
    pub(super) fn set_preview_geometry_error(&mut self, message: Option<String>, cx: &mut Context<Self>) {
        if let Some(message) = message { self.status = format!("Preview geometry: {message}"); }
        else if self.status.starts_with("Preview geometry:") { self.status = "Preview geometry saved.".into(); }
        cx.notify();
    }
    pub(super) fn take_preview_retiring(&mut self) -> Vec<RetiringHashWindow> { std::mem::take(&mut self.preview_retiring) }
    pub(super) fn preview_windows_for_restart(&mut self, cx: &App) -> Result<Vec<(AnyWindowHandle, NativeWindow)>, String> {
        self.preview_retiring.retain(|item| item.native_window.is_none_or(native_window_is_valid));
        if !self.preview_retiring.is_empty() { return Err("A File Search Preview window has not retired.".into()); }
        match (self.preview_popup, self.preview.as_ref()) {
            (Some(host), Some(child)) => child.read(cx).native().map(|native| vec![(host, native)])
                .ok_or_else(|| "File Search Preview has no checked native handle.".to_owned()),
            _ => Ok(Vec::new()),
        }
    }
    pub(super) fn retire_preview(&mut self, cx: &mut Context<Self>) {
        let sample = self.take_preview_geometry(true);
        if let Some(sample) = sample { let root = self.root.clone(); cx.spawn(async move |_, cx| { let _ = root.update(cx, |root, cx| root.record_file_preview_geometry(sample, cx)); }).detach(); }
        self.preview_owner.retire();
        let native = self.preview.as_ref().and_then(|child| child.read(cx).native());
        if let Some(child) = self.preview.take() { child.update(cx, |child, _| child.retire()); cx.notify(); }
        if let Some(popup) = self.preview_popup.take() {
            let signal = self.track_retiring_preview(popup, native, cx);
            self.remove_retiring_preview(popup, signal, cx);
        }
    }

    pub(super) fn take_preview_geometry(&mut self, detach: bool) -> Option<(UtilityNativeGeometrySample, Option<PreferredWindowSize>)> {
        let sample = self.preview_geometry.as_ref().and_then(utility_host::ShellGeometry::take);
        if detach { self.preview_geometry = None; }
        sample
    }

    fn select_row(&mut self, index: usize, cx: &mut Context<Self>) {
        self.explorer_authority.store(false, Ordering::Release);
        if self.selected != Some(index) { self.retire_preview(cx); }
        self.selected = Some(index);
        cx.notify();
    }

    pub fn shutdown(&mut self, cx: &mut Context<Self>) {
        self.invalidate("Search closed.", cx);
        self.escape_waiting_for_release = false;
        self.open = false;
        self.mount_owner.retire();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.rehosting || self.explorer_pending || !self.is_mounted_in(window.window_handle().window_id(), self.popout)
            || self.root.upgrade().is_some_and(|root| root.read(cx).external_launch.auxiliary_effect_pending()) { return; }
        if self.popout {
            let _ = self.root.update(cx, |root, cx| root.drain_file_search_geometry(cx, true));
            self.shutdown(cx); window.remove_window(); return;
        }
        self.shutdown(cx);
        let _ = self.root.update(cx, |root, cx| {
            root.root_scroll
                .scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
            root.query.focus_handle(cx).focus(window, cx);
            cx.notify();
        });
        cx.notify();
    }

    fn selected_receipt(&self, owner: EntityId) -> Option<SelectionReceipt> {
        let index = self.selected?;
        let search = self.active?;
        if !current_row(
            self.open,
            self.generation,
            self.active,
            self.generation,
            Some(search),
            index,
            self.visible_rows(),
        ) {
            return None;
        }
        match self.kind {
            SearchKind::Filename => {
                let row = self.results.get(index)?;
                matches!(row.kind, FileKind::File | FileKind::Directory).then(|| SelectionReceipt {
                    owner, search, generation: self.generation, index, path: row.path.clone(), kind: row.kind, content_key: None,
                })
            }
            SearchKind::Content => {
                let row = self.content_rows.get(index)?;
                Some(SelectionReceipt { owner, search, generation: self.generation, index,
                    path: row.path.clone(), kind: FileKind::File, content_key: Some(row.key.clone()) })
            }
        }
    }

    pub(super) fn preview_is_current(&self, child: EntityId, receipt: &SelectionReceipt) -> bool {
        self.preview.as_ref().is_some_and(|preview| preview.entity_id() == child)
            && self.receipt_is_current(receipt, receipt.owner)
    }

    fn open_preview(&mut self, receipt: SelectionReceipt, window: &mut Window, cx: &mut Context<Self>) {
        if self.rehosting || !matches!(receipt.kind, FileKind::File | FileKind::Directory) || !self.receipt_is_current(&receipt, cx.entity_id())
            || !window.is_window_active() || window.has_active_dialog(cx) || !self.owns_focus(window, cx) { return; }
        let Some(return_focus) = window.focused(cx) else { return; };
        if self.popout { self.open_popup_preview(receipt, return_focus, window, cx); return; }
        let Some(root) = self.root.upgrade() else { return; };
        let root_state = root.read(cx);
        let host = window.window_handle();
        let Some(native) = root_state.native_window else { return; };
        if root_state.root_window.is_none_or(|current| current.window_id() != host.window_id())
            || !root_state.external_owner_is_current(Some(&return_focus), window, cx)
            || current_foreground_identity() != Some(native.identity) { return; }
        if let Some(preview) = self.preview.as_ref().cloned() {
            let child = preview.read(cx);
            if child.receipt() != &receipt || child.host() != host.window_id() || !child.scope_current() { return; }
            preview.update(cx, |child, cx| child.focus(window, cx));
            root.update(cx, |root, cx| { root.root_scroll.scroll_to_top_of_item(root.file_search_scroll_item()); cx.notify(); });
            cx.notify();
            return;
        }
        let found = if receipt.content_key.is_some() {
            let Some(row) = self.content_rows.get(receipt.index).filter(|row| Some(&row.key) == receipt.content_key.as_ref() && row.path == receipt.path) else { return; };
            Some(row.found.clone())
        } else { None };
        let Some(permit) = self.admission.admit() else { return; };
        let return_scroll = root_state.root_scroll.offset();
        let visibility = root_state.root_visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        if !json_utility::native_host_current(&visibility, revision, Some(native), false) { return; }
        let request = multi_launcher::file_search::preview::PreviewRequest::from_search_result(
            &receipt.path, found.as_ref(), self.settings.max_full_preview_file_size_bytes);
        let parent = cx.entity().downgrade();
        let root_weak = self.root.clone();
        let owner = self.preview_owner.clone();
        let mount = owner.attach();
        let preview = cx.new(|cx| file_search_preview::FileSearchPreview::new(parent, root_weak, receipt, return_focus, return_scroll,
            host, found, owner, mount, visibility, revision, Some(native), self.admission.clone(), self.mount_receipt.clone(), false, cx));
        self.preview = Some(preview.clone());
        preview.update(cx, |preview, cx| { preview.focus(window, cx); preview.load(request, permit, cx); });
        root.update(cx, |root, cx| {
            root.root_scroll.scroll_to_top_of_item(root.file_search_scroll_item());
            cx.notify();
        });
        cx.notify();
    }

    fn open_popup_preview(&mut self, receipt: SelectionReceipt, return_focus: FocusHandle, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_mounted_in(window.window_handle().window_id(), true) || self.rehosting { return; }
        let Some(parent_native) = self.native else { return; };
        if !native_window_is_valid(parent_native) || current_foreground_identity() != Some(parent_native.identity) { return; }
        let Some(root) = self.root.upgrade() else { return; };
        if !root.read(cx).external_owner_is_current(Some(&return_focus), window, cx) { return; }
        if let (Some(popup), Some(child)) = (self.preview_popup, self.preview.clone()) {
            if child.read(cx).receipt() != &receipt || !child.read(cx).scope_current() { return; }
            let Some(native) = child.read(cx).native() else { return; };
            let parent_mount = self.mount_receipt.clone();
            let action_mount = child.read(cx).action_receipt();
            let admission = self.admission.clone();
            let revision = self.activation_revision.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
            let guard = self.activation_revision.clone(); let worker = guard.clone();
            cx.spawn(async move |_, cx| {
                let active = cx.background_executor().spawn(async move {
                    activate_for_secondary_window_if(&worker, revision, native, Some(parent_native.identity), &||
                        parent_mount.is_current() && action_mount.is_current() && !admission.is_fenced())
                }).await;
                if active && guard.load(Ordering::Acquire) == revision && current_foreground_identity() == Some(native.identity) {
                    let _ = update_kit_window_content(popup, &child, cx, |child, window, cx| {
                        if child.scope_current() { child.focus(window, cx); }
                    });
                }
            }).detach();
            return;
        }
        if self.preview.is_some() { return; }
        let found = if receipt.content_key.is_some() {
            let Some(row) = self.content_rows.get(receipt.index).filter(|row| Some(&row.key) == receipt.content_key.as_ref() && row.path == receipt.path) else { return; };
            Some(row.found.clone())
        } else { None };
        let Some(permit) = self.admission.admit() else { return; };
        let request = multi_launcher::file_search::preview::PreviewRequest::from_search_result(&receipt.path, found.as_ref(), self.settings.max_full_preview_file_size_bytes);
        let visibility = root.read(cx).root_visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let parent = cx.entity().downgrade();
        let close_parent = parent.clone();
        let root_weak = self.root.clone();
        let owner = self.preview_owner.clone();
        let mount = owner.attach();
        let parent_mount = self.mount_receipt.clone();
        let admission = self.admission.clone();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(860.0), px(620.0)), cx)),
            window_min_size: Some(size(px(320.0), px(240.0))),
            titlebar: Some(TitlebarOptions { title: Some("File Search Preview".into()), ..Default::default() }),
            show: false, focus: false,
            ..Default::default()
        };
        let opened = gpui_kit::open_window(options, cx, move |child_window, app| {
            let native = checked_native_window(child_window).ok().flatten();
            let host = child_window.window_handle();
            child_window.on_window_should_close(app, move |window, app| {
                close_parent.update(app, |parent, cx| {
                    if parent.root.upgrade().is_some_and(|root| root.read(cx).external_launch.auxiliary_effect_pending()) { return false; }
                    let Some(child) = parent.preview.as_ref().filter(|child| child.read(cx).host() == window.window_handle().window_id()).cloned() else { return true; };
                    let (receipt, focus, native, owns) = { let child = child.read(cx);
                        (child.receipt().clone(), child.return_focus.clone(), child.native(), child.owns_focus(window, cx)) };
                    parent.return_from_popup_preview(receipt, focus, native, owns, window, cx);
                    let sample = parent.take_preview_geometry(true);
                    if let Some(sample) = sample { let root = parent.root.clone(); cx.spawn(async move |_, cx| { let _ = root.update(cx, |root, cx| root.record_file_preview_geometry(sample, cx)); }).detach(); }
                    let native = child.read(cx).native();
                    if let Some(popup) = parent.preview_popup.take() { let _ = parent.track_retiring_preview(popup, native, cx); }
                    child.update(cx, |view, _| view.retire());
                    parent.preview = None; cx.notify(); true
                }).unwrap_or(true)
            });
            app.new(|cx| file_search_preview::FileSearchPreview::new(parent, root_weak, receipt, return_focus, point(px(0.0), px(0.0)),
                host, found, owner, mount, visibility, revision, native, admission, parent_mount, true, cx))
        });
        let Ok((popup, child)) = opened else { self.preview_owner.retire(); self.status = "Could not open Preview window.".into(); cx.notify(); return; };
        if child.read(cx).native().is_none() || child.read(cx).native_matches(parent_native) {
            let candidate_native = child.read(cx).native();
            let signal = self.track_retiring_preview(popup, candidate_native, cx);
            self.remove_retiring_preview(popup, signal, cx);
            self.preview_owner.retire(); self.status = "Could not check Preview window.".into(); cx.notify(); return;
        }
        self.preview = Some(child.clone()); self.preview_popup = Some(popup);
        let Some(child_native) = child.read(cx).native() else { return; };
        let action_receipt = child.read(cx).action_receipt();
        if let Ok(key) = UtilityToolKey::parse("file-preview") {
            let geometry = {
                let state = root.read(cx);
                let mut store = state.utility_size_store.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                store.track_key(key); store.effective_native_geometry(key)
            };
            if let Some(geometry) = geometry {
                if let Some(target) = checked_geometry_target(child_native) {
                    if let Err(error) = restore_native_window_geometry(target, Some(geometry), (320, 240), 1.0) {
                        self.status = format!("Could not restore Preview geometry: {error}");
                    }
                }
            }
            let state = root.read(cx);
            match utility_host::ShellGeometry::attach(child_native, key, action_receipt, state.hash_geometry_sequence.clone(), state.hash_wake_signal.clone()) {
                Ok(geometry) => self.preview_geometry = Some(geometry),
                Err(error) => self.status = format!("Could not observe Preview geometry: {error}"),
            }
        }
        if !self.mount_receipt.is_current() || !self.receipt_is_current(child.read(cx).receipt(), cx.entity_id())
            || !window.is_window_active() || current_foreground_identity() != Some(parent_native.identity)
            || !native_window_is_valid(child_native) || set_native_window_visible(child_native, true).is_err() {
            self.retire_preview(cx); self.status = "Could not show Preview window.".into(); cx.notify(); return;
        }
        if self.mount_receipt.is_current() && current_foreground_identity() == Some(parent_native.identity) {
            let _ = activate_native_window(child_native);
        }
        let _ = update_kit_window_content(popup, &child, cx, |child, window, cx| { child.focus(window, cx); child.load(request, permit, cx); });
        cx.notify();
    }

    fn return_from_popup_preview(&self, receipt: SelectionReceipt, focus: FocusHandle, child_native: Option<NativeWindow>,
        owns: bool, window: &Window, cx: &mut Context<Self>) {
        let host = self.host; let native = self.native; let mount = self.mount_receipt.clone();
        let parent_focus_unchanged = host.update(cx, |_, parent_window, app| {
            parent_window.focused(app).is_none_or(|current| current == focus)
        }).unwrap_or(false);
        let child_current = owns && window.is_window_active()
            && child_native.is_some_and(|native| native_window_is_valid(native) && current_foreground_identity() == Some(native.identity));
        let parent_current = mount.is_current() && self.is_mounted_in(host.window_id(), true)
            && native.is_some_and(native_window_is_valid) && !self.admission.is_fenced();
        if !popup_return_allowed(child_current, parent_current, self.receipt_is_current(&receipt, cx.entity_id()), parent_focus_unchanged) { return; }
        let Some(native) = native else { return; };
        if activate_native_window(native).is_err() { return; }
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let Some(view) = weak.upgrade() else { return; };
            let _ = update_kit_window_content(host, &view, cx, |view, window, cx| {
                if mount.is_current() && view.is_mounted_in(window.window_handle().window_id(), true)
                    && view.receipt_is_current(&receipt, cx.entity_id()) && window.is_window_active()
                    && current_foreground_identity() == Some(native.identity)
                    && window.focused(cx).is_none_or(|current| current == focus) { focus.focus(window, cx); }
            });
        }).detach();
    }

    pub(super) fn close_preview(&mut self, child: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(preview) = self.preview.as_ref().filter(|preview| preview.entity_id() == child).cloned() else { return; };
        if self.preview_popup.is_some_and(|popup| popup.window_id() == window.window_handle().window_id()) {
            if self.root.upgrade().is_some_and(|root| root.read(cx).external_launch.auxiliary_effect_pending()) { return; }
            let (receipt, focus, native, owns) = { let child = preview.read(cx);
                (child.receipt().clone(), child.return_focus.clone(), child.native(), child.owns_focus(window, cx)) };
            self.return_from_popup_preview(receipt, focus, native, owns, window, cx);
            let sample = self.take_preview_geometry(true);
            if let Some(sample) = sample { let root = self.root.clone(); cx.spawn(async move |_, cx| { let _ = root.update(cx, |root, cx| root.record_file_preview_geometry(sample, cx)); }).detach(); }
            let native = preview.read(cx).native();
            if let Some(popup) = self.preview_popup.take() { let _ = self.track_retiring_preview(popup, native, cx); }
            preview.update(cx, |view, _| view.retire());
            self.escape_waiting_for_release = true;
            self.preview = None; window.remove_window(); cx.notify(); return;
        }
        let view = preview.read(cx);
        let owns_focus = view.owns_focus(window, cx);
        let same_host = view.host() == window.window_handle().window_id() && view.scope_current();
        let current = self.receipt_is_current(view.receipt(), cx.entity_id());
        let return_focus = view.return_focus.clone();
        let return_scroll = view.return_scroll;
        preview.update(cx, |view, _| view.retire());
        self.preview = None;
        if owns_focus && same_host && current && window.is_window_active() && !window.has_active_dialog(cx) {
            let valid = self.root.upgrade().is_some_and(|root| {
                let root = root.read(cx);
                root.root_window.is_some_and(|host| host.window_id() == window.window_handle().window_id())
                    && root.native_window.is_some_and(|native| current_foreground_identity() == Some(native.identity))
                    && !root.restart_admission.is_fenced()
            });
            if valid {
                let _ = self.root.update(cx, |root, cx| { root.root_scroll.set_offset(return_scroll); cx.notify(); });
                return_focus.focus(window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn receipt_is_current(&self, receipt: &SelectionReceipt, owner: EntityId) -> bool {
        self.selected_receipt(owner).as_ref() == Some(receipt)
    }

    pub(super) fn restore_receipt_focus(
        &self,
        receipt: &SelectionReceipt,
        owner: EntityId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if receipt.kind != FileKind::File || receipt.content_key.is_some() || !self.receipt_is_current(receipt, owner) {
            return false;
        }
        self.calculate_focus.focus(window, cx);
        true
    }

    pub(super) fn clear_hash_error(&mut self, cx: &mut Context<Self>) {
        if self.status.starts_with("Could not open Hash Generator:") {
            self.status = "Hash Generator opened.".into(); cx.notify();
        }
    }

    pub(super) fn report_hash_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.status = format!("Could not open Hash Generator: {error}");
        cx.notify();
    }

    fn calculate_selected(
        &mut self,
        receipt: SelectionReceipt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.rehosting || !self.is_mounted_in(window.window_handle().window_id(), self.popout) || !window.is_window_active()
            || self.admission.is_fenced()
            || receipt.kind != FileKind::File || receipt.content_key.is_some() || !self.receipt_is_current(&receipt, cx.entity_id())
            || !self.results.get(receipt.index).is_some_and(|row| eligible_file_row(row, &receipt.path))
        {
            return;
        }
        self.calculate_focus.focus(window, cx);
        let root = self.root.clone();
        let source = cx.entity().downgrade();
        window.defer(cx, move |window, app| {
            let _ = root.update(app, |root, cx| {
                root.open_hash_from_file_search(source, receipt, window, cx)
            });
        });
    }

    fn open_selected_in_explorer(&mut self, receipt: SelectionReceipt, window: &mut Window, cx: &mut Context<Self>) {
        if self.explorer_pending || self.rehosting || !self.is_mounted_in(window.window_handle().window_id(), self.popout) || !window.is_window_active() || self.admission.is_fenced()
            || !self.receipt_is_current(&receipt, cx.entity_id()) { return; }
        self.explorer_focus.focus(window, cx);
        let root = self.root.clone();
        let source = cx.entity().downgrade();
        window.defer(cx, move |window, app| {
            let _ = root.update(app, |root, cx| root.open_file_search_explorer(source, receipt, window, cx));
        });
    }

    fn begin_explorer(&mut self, receipt: SelectionReceipt, visibility: RootVisibility, revision: u64,
        native: Option<NativeWindow>, permit: EffectPermit, cx: &mut Context<Self>) {
        if self.explorer_pending { return; }
        self.explorer_pending = true;
        let authority = Arc::new(AtomicBool::new(true));
        self.explorer_authority = authority.clone();
        let admission = self.admission.clone();
        let mount = self.mount_receipt.clone();
        let popout = self.popout;
        let worker_authority = authority.clone();
        let task = cx.background_executor().spawn(async move {
            let current = || {
                worker_authority.load(Ordering::Acquire)
                    && !admission.is_fenced() && mount.is_current() && (popout || visibility.is_current(revision, true))
                    && native.is_some_and(|native| native_window_is_valid(native)
                        && current_foreground_identity() == Some(native.identity))
            };
            #[cfg(windows)]
            let result = (|| -> anyhow::Result<()> {
                let directory = receipt.kind == FileKind::Directory;
                anyhow::ensure!(directory || receipt.kind == FileKind::File, "Unsupported result type");
                if directory {
                    let target = multi_launcher::file_search::actions::checked_explorer_target(&receipt.path, true)?;
                    multi_launcher::actions::exec::launch_association_if_current(target.as_os_str(), current)
                } else {
                    multi_launcher::file_search::actions::reveal_checked_file_if_current(&receipt.path, current)
                }
            })();
            #[cfg(not(windows))]
            let result: anyhow::Result<()> = { let _ = current; Err(anyhow::anyhow!("Explorer requires Windows")) };
            drop(permit);
            result
        });
        let owner = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = owner.update(cx, |this, cx| {
                this.explorer_pending = false;
                if authority.load(Ordering::Acquire) {
                    this.status = match result { Ok(()) => "Opened in Explorer.".into(), Err(error) => format!("Could not open in Explorer: {error:#}") };
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    fn running(&self) -> bool {
        self.validating
            || (self.active.is_some() && self.coordinator.active_status() == SearchStatus::Running)
    }

    pub(super) fn handle_escape(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        held: Option<bool>,
    ) -> bool {
        if !self.owns_focus(window, cx) || self.admission.is_fenced() {
            return false;
        }
        if self.escape_waiting_for_release || held == Some(true) { return true; }
        if let Some(preview) = self.preview.as_ref().filter(|preview| preview.read(cx).owns_focus(window, cx)) {
            let child = preview.entity_id();
            self.escape_waiting_for_release = true;
            self.close_preview(child, window, cx);
            return true;
        }
        if !held.unwrap_or(self.escape_waiting_for_release) {
            if self.running() {
                self.escape_waiting_for_release = true;
                self.invalidate("Search cancelled.", cx);
                cx.notify();
            } else {
                self.close(window, cx);
            }
        }
        true
    }

    fn row_count(&self) -> usize {
        match self.kind { SearchKind::Filename => self.results.len(), SearchKind::Content => self.content_rows.len() }
    }

    fn visible_rows(&self) -> std::ops::Range<usize> {
        let start = self.page.saturating_mul(MOUNTED_ROWS);
        let end = start.saturating_add(MOUNTED_ROWS).min(self.row_count());
        start..end
    }

    fn set_page(&mut self, page: usize, cx: &mut Context<Self>) {
        self.explorer_authority.store(false, Ordering::Release);
        self.page = page;
        self.retire_preview(cx);
        self.content_focus.clear();
        self.selected = None;
        self.results_scroll.set_offset(point(px(0.0), px(0.0)));
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        if !self.open || self.rehosting || self.running() {
            return;
        }
        let source = SearchSource { term: self.term.read(cx).value().to_string(),
            directory: PathBuf::from(self.directory.read(cx).value().as_str()), kind: self.kind, expected_root: None };
        self.start_source(source, cx);
    }

    fn start_typed(&mut self, receipt: ScopedReceipt, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = typed_source(receipt.draft, receipt.canonical_root) else {
            self.status = "The search directory is unavailable; enter the query again.".into(); cx.notify(); return;
        };
        self.open = true;
        self.kind = source.kind;
        self.term.update(cx, |input, cx| input.set_value(source.term.clone(), window, cx));
        self.directory.update(cx, |input, cx| input.set_value(source.directory.to_string_lossy().into_owned(), window, cx));
        self.term.focus_handle(cx).focus(window, cx);
        self.start_source(source, cx);
    }

    fn start_source(&mut self, source: SearchSource, cx: &mut Context<Self>) {
        let Some(permit) = self.admission.admit() else {
            self.status = "Application is closing.".into();
            cx.notify();
            return;
        };
        self.invalidate("Validating directory…", cx);
        self.validating = true;
        let generation = self.generation;
        let SearchSource { term, directory, kind, expected_root } = source;
        let settings = self.settings.clone();
        let task = cx.background_executor().spawn(async move {
            let preferences = settings.ui_preferences.clone();
            let request = if expected_root.is_some() && !checked_root(&directory) {
                Err(multi_launcher::file_search::request::RequestError::EmptyDirectoryRoots)
            } else { build_request(&RequestInput {
                text: &term,
                kind,
                scope: RootScope::Directory,
                custom_roots: vec![directory],
                case_sensitive: settings.case_sensitive,
                include_hidden: settings.include_hidden_files,
                excluded_directory_names: settings.excluded_directory_names.clone(),
                settings: &settings,
                preferences: &preferences,
            }) };
            (request, permit, expected_root)
        });
        let owner = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let (request, permit, expected_root) = task.await;
            let started = owner
                .update(cx, |this, cx| {
                    if !this.open || this.generation != generation || this.admission.is_fenced() {
                        return false;
                    }
                    this.validating = false;
                    match request {
                        Ok(request) if !validated_root_matches(expected_root.as_deref(), &request.scope) => {
                            this.status = "The search directory changed; enter the query again.".into();
                            cx.notify();
                            false
                        }
                        Ok(request) => {
                            let id = this
                                .coordinator
                                .start_search_with_retirement_guard(request, permit);
                            this.active = Some(id);
                            this.status = "Searching…".into();
                            cx.notify();
                            true
                        }
                        Err(error) => {
                            this.status = error.user_message().to_owned();
                            cx.notify();
                            false
                        }
                    }
                })
                .unwrap_or(false);
            if !started {
                return;
            }
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(35))
                    .await;
                match owner.update(cx, |this, cx| this.poll(generation, cx)) {
                    Ok(true) => {}
                    _ => break,
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn poll(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        if !self.open || self.generation != generation || self.active.is_none() {
            return false;
        }
        let events = self.coordinator.drain_current_events_bounded(EVENT_BATCH);
        let changed = !events.is_empty();
        for event in events {
            match event {
                SearchEvent::Started { backend, .. } => {
                    self.backend = Some(backend);
                    self.status = format!("Searching with {backend:?}…")
                }
                SearchEvent::BackendFallback { to, reason, .. } => {
                    self.backend = Some(to);
                    self.status = format!("Using {to:?}: {reason}")
                }
                SearchEvent::Result {
                    result: SearchResult::Filename(row),
                    ..
                } => self.results.push(row),
                SearchEvent::Result { result: SearchResult::ContentFile(file), .. } => {
                    self.truncated |= file.truncated;
                    self.content_files = self.content_files.saturating_add(1);
                    self.content_total_matches = self.content_total_matches.saturating_add(file.total_matches);
                    append_content_rows(&mut self.content_rows, &mut self.content_occurrences, file);
                }
                SearchEvent::Progress { progress, .. } => {
                    self.truncated |= progress.global_truncated;
                    let noun = if self.kind == SearchKind::Content { "matching files" } else { "results" };
                    self.status = format!("Searching… {} files scanned, {} {noun}", progress.files_scanned, progress.results_found);
                }
                SearchEvent::Diagnostic { diagnostic, .. } => {
                    self.truncated |= matches!(
                        &diagnostic,
                        SearchDiagnostic::FilenameResultsTruncated { .. }
                            | SearchDiagnostic::GlobalMatchedFilesTruncated { .. }
                            | SearchDiagnostic::PerFileContentTruncated { .. }
                    );
                    self.status = format!("Search: {diagnostic:?}");
                }
                SearchEvent::Completed { .. } => {
                    let backend = self
                        .backend
                        .map(|backend| format!(" · {backend:?}"))
                        .unwrap_or_default();
                    let limit = if self.truncated {
                        " (limit reached)"
                    } else {
                        ""
                    };
                    self.status = match self.kind {
                        SearchKind::Filename => format!("Completed{backend} · {} results{limit}", self.row_count()),
                        SearchKind::Content => format!("Completed{backend} · {} matching files · {} of {} matches shown{limit}",
                            self.content_files, self.content_rows.len(), self.content_total_matches),
                    };
                }
                SearchEvent::Cancelled { .. } => {
                    self.invalidate("Search cancelled.", cx);
                    break;
                }
                SearchEvent::Failed { error, .. } => {
                    self.status = format!("Search failed: {error}")
                }
            }
        }
        if changed {
            cx.notify();
        }
        self.coordinator.active_status() == SearchStatus::Running
    }
}

impl Drop for FileSearchView {
    fn drop(&mut self) {
        self.coordinator.cancel_and_retire();
    }
}

impl Render for FileSearchView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let running = self.running();
        let visible = self.visible_rows();
        let start = visible.start;
        let end = visible.end;
        let generation = self.generation;
        let active = self.active;
        let selected_receipt = self.selected_receipt(cx.entity_id());
        if self.kind == SearchKind::Content {
            for row in self.content_rows.iter().skip(start).take(MOUNTED_ROWS) {
                self.content_focus.entry(row.key.clone()).or_insert_with(|| cx.focus_handle());
            }
        }
        let preview_receipt = selected_receipt.clone().filter(|receipt| matches!(receipt.kind, FileKind::File | FileKind::Directory));
        let hash_receipt = selected_receipt.clone().filter(|receipt| receipt.kind == FileKind::File && receipt.content_key.is_none());
        let selected_path = selected_receipt
            .as_ref()
            .map(|receipt| receipt.path.display().to_string());
        let rows = if self.kind == SearchKind::Filename {
            self
            .results
            .iter()
            .enumerate()
            .skip(start)
            .take(MOUNTED_ROWS)
            .map(|(index, row)| {
                let label = format!("{}  —  {}", row.file_name, row.path.display());
                let button = Button::new(SharedString::from(format!(
                    "file-search-result-{generation}-{index}"
                )))
                .accessibility_label(format!("File search result: {label}"))
                .label(label)
                .tab_stop(true)
                .selected(self.selected == Some(index))
                .on_click({ let mount = self.mount_receipt.clone(); cx.listener(move |this, _, window, cx| { if !mount.is_current() || !this.is_mounted_in(window.window_handle().window_id(), this.popout) || this.rehosting { return; }
                    if mounted_search_row_current(
                        &mount, this.open,
                        this.generation,
                        this.active,
                        generation,
                        active,
                        index,
                        this.visible_rows(),
                    ) {
                        this.select_row(index, cx);
                    }
                }) });
                div().flex_shrink_0().child(button)
            })
            .collect::<Vec<_>>()
        } else {
            self.content_rows.iter().enumerate().skip(start).take(MOUNTED_ROWS).map(|(index, row)| {
                let key = row.key.clone();
                let row_focus = self.content_focus.get(&key).cloned();
                let path = row.path.clone();
                let label = format!("{}:{}  {}", row.found.line_number,
                    row.found.column.map(|column| column.saturating_add(1)).unwrap_or(1), match_excerpt(&row.found));
                let button = Button::new(SharedString::from(format!("file-search-content-{generation}-{index}")))
                    .accessibility_label(format!("Content match: {label}"))
                    .label(label).tab_stop(true).selected(self.selected == Some(index))
                    .when_some(row_focus, |button, focus| button.track_focus(&focus))
                    .on_click({ let mount = self.mount_receipt.clone(); cx.listener(move |this, _, window, cx| { if !mount.is_current() || !this.is_mounted_in(window.window_handle().window_id(), this.popout) || this.rehosting { return; }
                        if this.kind == SearchKind::Content && mounted_search_row_current(&mount, this.open, this.generation, this.active,
                            generation, active, index, this.visible_rows())
                            && this.content_rows.get(index).is_some_and(|row| row.key == key && row.path == path) {
                            this.select_row(index, cx);
                        }
                    }) });
                let heading = (row.first_in_file || index == start).then(|| row.heading.clone());
                div().flex_shrink_0().flex().flex_col().when_some(heading, |row, heading| row.child(Label::new(heading))).child(button)
            }).collect::<Vec<_>>()
        };
        div()
            .id("file-search-pane")
            .key_context("FileSearchPane")
            .track_focus(&self.focus)
            .capture_action(
                cx.listener(|this, _: &gpui_kit::base::input::Escape, window, cx| {
                    if this.handle_escape(window, cx, None) {
                        cx.stop_propagation();
                    }
                }),
            )
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "enter" && event.keystroke.modifiers == Modifiers::default()
                    && !event.is_held && !this.rehosting {
                    let focused = this.visible_rows().find(|index| this.content_rows.get(*index).is_some_and(|row|
                        this.content_focus.get(&row.key).is_some_and(|focus| focus.is_focused(window))));
                    if this.kind == SearchKind::Content {
                        if let Some(index) = focused {
                            this.select_row(index, cx);
                            if let Some(receipt) = this.selected_receipt(cx.entity_id()) {
                                window.prevent_default(); cx.stop_propagation();
                                this.open_preview(receipt, window, cx); return;
                            }
                        }
                    }
                }
                if event.keystroke.key != "escape"
                    || event.keystroke.modifiers != Modifiers::default()
                    || !this.owns_focus(window, cx)
                {
                    return;
                }
                if this.handle_escape(window, cx, event.is_held.then_some(true)) {
                    cx.stop_propagation();
                }
            }))
            .capture_key_up(cx.listener(|this, event: &KeyUpEvent, _, _| {
                if event.keystroke.key == "escape" {
                    this.escape_waiting_for_release = false;
                }
            }))
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .p_3()
            .child(Label::new("File Search"))
            .child(div().flex().gap_2()
                .child(Button::new("file-search-filename-mode").label("Filename").selected(self.kind == SearchKind::Filename)
                    .on_click({ let mount = self.mount_receipt.clone(); cx.listener(move |this, _, window, cx| { if !mount.is_current() || !this.is_mounted_in(window.window_handle().window_id(), this.popout) || this.rehosting { return; } if this.kind != SearchKind::Filename {
                        this.invalidate("Mode changed. Search again.", cx); this.kind = SearchKind::Filename; cx.notify();
                    } }) }))
                .child(Button::new("file-search-content-mode").label("Content").selected(self.kind == SearchKind::Content)
                    .on_click({ let mount = self.mount_receipt.clone(); cx.listener(move |this, _, window, cx| { if !mount.is_current() || !this.is_mounted_in(window.window_handle().window_id(), this.popout) || this.rehosting { return; } if this.kind != SearchKind::Content {
                        this.invalidate("Mode changed. Search again.", cx); this.kind = SearchKind::Content; cx.notify();
                    } }) })))
            .child(Label::new("Directory"))
            .child(Input::new(&self.directory).readonly(self.rehosting).aria_label("Search directory"))
            .child(Label::new(if self.kind == SearchKind::Filename { "Filename" } else { "Content" }))
            .child(Input::new(&self.term).readonly(self.rehosting).aria_label(if self.kind == SearchKind::Filename { "Filename search text" } else { "Content search text" }))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("file-search-start")
                            .tab_stop(true)
                            .label("Search")
                            .disabled(running)
                            .on_click({ let mount = self.mount_receipt.clone(); cx.listener(move |this, _, window, cx| { if !mount.is_current() || !this.is_mounted_in(window.window_handle().window_id(), this.popout) || this.rehosting { return; } if this.is_mounted_in(window.window_handle().window_id(), this.popout) && !this.rehosting { this.start(cx); } }) }),
                    )
                    .child(
                        Button::new("file-search-cancel")
                            .tab_stop(true)
                            .label("Cancel")
                            .disabled(!running)
                            .on_click({ let mount = self.mount_receipt.clone(); cx.listener(move |this, _, window, cx| { if !mount.is_current() || !this.is_mounted_in(window.window_handle().window_id(), this.popout) || this.rehosting { return; }
                                this.invalidate("Search cancelled.", cx);
                                cx.notify();
                            }) }),
                    )
                    .child(
                        Button::new("file-search-calculate-hash")
                            .tab_stop(true)
                            .track_focus(&self.calculate_focus)
                            .label("Calculate hash")
                            .disabled(self.rehosting || hash_receipt.is_none())
                            .on_click({ let mount = self.mount_receipt.clone(); cx.listener(move |this, _, window, cx| { if !mount.is_current() || !this.is_mounted_in(window.window_handle().window_id(), this.popout) || this.rehosting { return; }
                                if let Some(receipt) = hash_receipt.clone() {
                                    this.calculate_selected(receipt, window, cx);
                                }
                            }) }),
                    )
                    .child(
                        Button::new("file-search-preview-selected")
                            .tab_stop(true).track_focus(&self.preview_focus)
                            .label("Preview").disabled(self.rehosting || preview_receipt.is_none())
                            .on_click({ let mount = self.mount_receipt.clone(); cx.listener(move |this, _, window, cx| { if !mount.is_current() || !this.is_mounted_in(window.window_handle().window_id(), this.popout) || this.rehosting { return; }
                                if let Some(receipt) = preview_receipt.clone() { this.open_preview(receipt, window, cx); }
                            }) }),
                    )
                    .child(
                        Button::new("file-search-open-explorer")
                            .tab_stop(true).track_focus(&self.explorer_focus)
                            .label("Open in Explorer")
                            .disabled(selected_receipt.is_none() || self.explorer_pending)
                            .on_click({ let mount = self.mount_receipt.clone(); cx.listener(move |this, _, window, cx| { if !mount.is_current() || !this.is_mounted_in(window.window_handle().window_id(), this.popout) || this.rehosting { return; }
                                if let Some(receipt) = selected_receipt.clone() {
                                    this.open_selected_in_explorer(receipt, window, cx);
                                }
                            }) }),
                    )
                    .child(Button::new("file-search-rehost").label(if self.popout { "Dock" } else { "Pop Out" })
                        .disabled(self.rehosting || self.explorer_pending)
                        .on_click({ let mount = self.mount_receipt.clone(); let target = !self.popout;
                            cx.listener(move |this, _, window, cx| {
                                if mount.is_current() && this.is_mounted_in(window.window_handle().window_id(), !target) { this.request_rehost(target, cx); }
                            }) }))
                    .child(
                        Button::new("file-search-close")
                            .tab_stop(true)
                            .label("Close")
                            .on_click({ let mount = self.mount_receipt.clone(); cx.listener(move |this, _, window, cx| { if mount.is_current() { this.close(window, cx); } }) }),
                    ),
            )
            .when_some(self.presentation_error.clone(), |pane, error| pane.child(Label::new(error)))
            .when_some(self.geometry_error.clone(), |pane, error| pane.child(Label::new(error)))
            .child(Label::new(if self.explorer_pending { "Opening in Explorer…".to_owned() } else { self.status.clone() }).id("file-search-status"))
            .when_some(selected_path, |pane, path| {
                pane.child(Label::new("Selected result"))
                    .child(Label::new(path).id("file-search-selected-path"))
            })
            .when(self.row_count() > MOUNTED_ROWS, |pane| {
                pane.child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("file-search-previous-page")
                                .tab_stop(true)
                                .label("Previous page")
                                .disabled(self.page == 0)
                                .on_click({ let mount = self.mount_receipt.clone(); cx.listener(move |this, _, window, cx| { if !mount.is_current() || !this.is_mounted_in(window.window_handle().window_id(), this.popout) || this.rehosting { return; }
                                    this.set_page(this.page.saturating_sub(1), cx);
                                    cx.notify();
                                }) }),
                        )
                        .child(Label::new(format!(
                            "Results {}–{} of {}",
                            start + 1,
                            end,
                            self.row_count()
                        )))
                        .child(
                            Button::new("file-search-next-page")
                                .tab_stop(true)
                                .label("Next page")
                                .disabled(end >= self.row_count())
                                .on_click({ let mount = self.mount_receipt.clone(); cx.listener(move |this, _, window, cx| { if !mount.is_current() || !this.is_mounted_in(window.window_handle().window_id(), this.popout) || this.rehosting { return; }
                                    if (this.page + 1) * MOUNTED_ROWS < this.row_count() {
                                        this.set_page(this.page + 1, cx);
                                        cx.notify();
                                    }
                                }) }),
                        ),
                )
            })
            .when_some(self.preview.clone().filter(|_| self.preview_popup.is_none()), |pane, preview| pane.child(preview))
            .child(
                div()
                    .id("file-search-results")
                    .flex()
                    .flex_col()
                    .max_h(px(300.0))
                    .overflow_y_scroll()
                    .track_scroll(&self.results_scroll)
                    .children(rows)
                    .vertical_scrollbar(&self.results_scroll),
            )
    }
}

#[derive(Clone)]
pub(super) struct ScopedReceipt {
    query: String,
    action: Action,
    generation: u64,
    draft: multi_launcher::file_search::query::SearchRequestDraft,
    canonical_root: PathBuf,
}

struct ProbeRequest {
    query: String,
    generation: u64,
    revision: u64,
    other: Vec<(String, Action)>,
    enabled: Option<HashSet<String>>,
    caps: Option<HashMap<String, Vec<String>>>,
}

#[derive(Default)]
pub(super) struct ProbeOwner {
    active: bool,
    pending: Option<ProbeRequest>,
    receipt: Option<ScopedReceipt>,
    revision: u64,
}

impl ProbeOwner {
    pub(super) fn retire(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.pending = None;
        self.receipt = None;
    }

    pub(super) fn receipt(&self, generation: u64, query: &str, action: &Action,
        plugins: Option<&HashSet<String>>, caps: Option<&HashMap<String, Vec<String>>>,
        fenced: bool) -> Option<ScopedReceipt> {
        self.receipt.as_ref().filter(|receipt| !fenced && enabled(plugins, caps)
            && receipt.generation == generation && receipt.query == query && receipt.action == *action
            && multi_launcher::plugins::file_search::scoped_search_start(query, action).as_ref() == Some(&receipt.draft)
        ).cloned()
    }
}

pub(super) fn enabled(plugins: Option<&HashSet<String>>, caps: Option<&HashMap<String, Vec<String>>>) -> bool {
    plugins.is_none_or(|plugins| plugins.contains("file_search"))
        && multi_launcher::settings::provider_search_capability_enabled(caps, "file_search")
}

pub(super) fn typed_query(query: &str) -> bool {
    let (filtered, _) = multi_launcher::common::query::split_action_filters(query);
    multi_launcher::file_search::query::explicit_search_root_candidate(&filtered)
        .is_some_and(|path| local_absolute(&path))
}

fn local_absolute(path: &std::path::Path) -> bool {
    #[cfg(windows)]
    { multi_launcher::actions::exec::ordinary_local_path_is_admissible(path) }
    #[cfg(not(windows))]
    { path.is_absolute() && !path.to_string_lossy().starts_with(r"\\") }
}

fn checked_root(path: &std::path::Path) -> bool {
    #[cfg(windows)]
    { multi_launcher::actions::exec::checked_local_directory(path).is_ok() }
    #[cfg(not(windows))]
    { local_absolute(path) && path.is_dir() }
}

fn validated_root_matches(expected: Option<&std::path::Path>, scope: &multi_launcher::file_search::model::SearchScope) -> bool {
    expected.is_none_or(|expected| matches!(scope, multi_launcher::file_search::model::SearchScope::Roots { roots } if roots.len() == 1 && roots[0] == expected))
}

struct ScopedStockRow(Action);
impl multi_launcher::plugin::Plugin for ScopedStockRow {
    fn name(&self) -> &str { "file_search" }
    fn description(&self) -> &str { "Open local filename/content search" }
    fn capabilities(&self) -> &[&str] { &["search"] }
    fn query_prefixes(&self) -> &[&str] { &["fs"] }
    fn search(&self, _: &str) -> Vec<Action> { vec![self.0.clone()] }
}

impl RootWindow {
    pub(super) fn record_file_preview_geometry(&mut self, (sample, factory): (UtilityNativeGeometrySample, Option<PreferredWindowSize>), cx: &mut Context<Self>) {
        let Ok(key) = UtilityToolKey::parse("file-preview") else { return; };
        let result = {
            let mut store = self.utility_size_store.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let factory = store.effective_native_geometry(key).is_none().then_some(factory).flatten();
            store.observe_native_geometry(key, sample, factory)
        };
        match result {
            Ok(Some(_)) => self.start_utility_size_ack_poll(cx),
            Err(error) => {
                if let Some(view) = self.file_search.clone() { view.update(cx, |view, cx| view.set_preview_geometry_error(Some(error.to_string()), cx)); }
            },
            _ => {}
        }
    }
    pub(super) fn drain_file_preview_geometry(&mut self, cx: &mut Context<Self>, detach: bool) {
        let sample = self.file_search.clone().and_then(|view| view.update(cx, |view, _| view.take_preview_geometry(detach)));
        if let Some(sample) = sample { self.record_file_preview_geometry(sample, cx); }
    }

    fn file_search_typed_receipt_current(&self, typed: &ScopedReceipt, cx: &App) -> bool {
        self.query.read(cx).value().as_str() == typed.query && self.launcher_results.query == typed.query
            && self.launcher_results.generation == typed.generation
            && self.file_search_probe.receipt(typed.generation, &typed.query, &typed.action,
                self.enabled_plugins.as_ref(), self.enabled_capabilities.as_ref(), self.restart_admission.is_fenced()).is_some()
    }

    pub(super) fn file_search_prefers_popout(&self) -> bool {
        multi_launcher::settings::UtilityToolKey::parse("file-search").ok().is_some_and(|key|
            self.utility_presentations.preference(key).unwrap_or(multi_launcher::settings::RememberedPresentation::Docked)
                == multi_launcher::settings::RememberedPresentation::Popout)
    }

    pub(super) fn reveal_file_search_popup(&mut self, typed: Option<ScopedReceipt>, cx: &mut Context<Self>) -> bool {
        let (Some(popup), Some(view), Some(root_native)) = (&self.file_search_popup, &self.file_search, self.native_window) else { return false; };
        let Some(native) = popup.native else { return true; };
        if current_foreground_identity() != Some(root_native.identity) || view.read(cx).rehosting { return true; }
        let Some(permit) = self.restart_admission.admit() else { return true; };
        let popup_window = popup.window;
        let view = view.clone();
        let receipt = view.read(cx).mount_receipt();
        let worker_receipt = receipt.clone();
        let visibility = self.root_visibility.clone();
        let visibility_revision = visibility.request_revision.load(Ordering::Acquire);
        let admission = self.restart_admission.clone();
        self.file_search_popup_intent.store(false, Ordering::Release);
        let intent = Arc::new(AtomicBool::new(true));
        self.file_search_popup_intent = intent.clone();
        let worker_intent = intent.clone();
        let accepted = Arc::new(AtomicBool::new(false));
        let root = cx.entity().downgrade();
        if let Some(typed) = typed.as_ref() {
            view.update(cx, |view, _| view.pending_popup_typed = Some((typed.clone(), receipt.clone(), intent.clone(), accepted.clone())));
        }
        let guard = view.read(cx).activation_revision.clone();
        let revision = guard.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        let worker = guard.clone();
        cx.spawn(async move |_, cx| {
            let _permit = permit;
            let activated = cx.background_executor().spawn(async move {
                activate_for_secondary_window_if(&worker, revision, native, Some(root_native.identity), &||
                    worker_intent.load(Ordering::Acquire) && worker_receipt.is_current() && !admission.is_fenced()
                        && json_utility::native_host_current(&visibility, visibility_revision, Some(root_native), false))
            }).await;
            if !activated {
                intent.store(false, Ordering::Release);
                let _ = view.update(cx, |view, _| {
                    if view.pending_popup_typed.as_ref().is_some_and(|(_, _, pending, _)| Arc::ptr_eq(pending, &intent)) { view.pending_popup_typed = None; }
                });
                return;
            }
            accepted.store(true, Ordering::Release);
            if !intent.load(Ordering::Acquire) || !receipt.is_current() || guard.load(Ordering::Acquire) != revision { return; }
            if current_foreground_identity() != Some(native.identity) { return; }
            let current = root.update(cx, |root, cx| {
                root.file_search_popup.as_ref().is_some_and(|popup| popup.window.window_id() == popup_window.window_id())
                    && root.file_search.as_ref().is_some_and(|current| current.entity_id() == view.entity_id())
                    && !root.restart_admission.is_fenced()
                    && typed.as_ref().is_none_or(|typed| root.file_search_typed_receipt_current(typed, cx))
            }).unwrap_or(false);
            if !current || !intent.load(Ordering::Acquire) { return; }
            let _ = update_kit_window_content(popup_window, &view, cx, |view, window, cx| {
                if view.is_mounted_in(window.window_handle().window_id(), true) && !view.rehosting {
                    if typed.is_some() { view.wake_pending_popup(window, cx); } else { view.focus_host(window, cx); }
                }
            });
        }).detach();
        true
    }

    fn open_file_search_explorer(&mut self, source: WeakEntity<FileSearchView>, receipt: SelectionReceipt,
        window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = source.upgrade() else { return; };
        let source = view.read(cx);
        let contexts = window.context_stack();
        let revision = self.root_visibility.request_revision.load(Ordering::Acquire);
        if !self.file_search.as_ref().is_some_and(|mounted| mounted.entity_id() == receipt.owner)
            || view.entity_id() != receipt.owner
            || !source.is_mounted_in(window.window_handle().window_id(), source.popout) || source.rehosting
            || (!source.popout && !self.root_visibility.is_current(revision, true)) || !window.is_window_active()
            || self.action_sheet.is_some() || self.context_favorite_popup.is_some()
            || context_stack_has_primary(&contexts, "PopupMenu")
            || context_stack_has_primary(&contexts, "AlertDialog")
            || context_stack_has_primary(&contexts, "Dialog")
            || !source.explorer_focus.is_focused(window) || source.explorer_pending
            || !source.receipt_is_current(&receipt, view.entity_id()) { return; }
        if !source.native.is_some_and(|native| native_window_is_valid(native)
            && current_foreground_identity() == Some(native.identity)) { return; }
        let Some(permit) = self.restart_admission.admit() else { return; };
        let visibility = self.root_visibility.clone();
        let native = source.native;
        view.update(cx, |view, cx| view.begin_explorer(receipt, visibility, revision, native, permit, cx));
    }

    pub(super) fn start_file_search_probe(&mut self, query: String, other: Vec<(String, Action)>, scroll: bool, cx: &mut Context<Self>) {
        self.publish_launcher_provider_results(query.clone(), other.clone(), scroll);
        self.file_search_probe.pending = Some(ProbeRequest {
            query, generation: self.launcher_results.generation, revision: self.file_search_probe.revision, other,
            enabled: self.enabled_plugins.clone(), caps: self.enabled_capabilities.clone(),
        });
        self.launch_file_search_probe(cx);
    }

    fn launch_file_search_probe(&mut self, cx: &mut Context<Self>) {
        if self.file_search_probe.active { return; }
        if self.restart_admission.is_fenced() { self.file_search_probe.retire(); return; }
        let Some(request) = self.file_search_probe.pending.take() else { return; };
        self.file_search_probe.active = true;
        let query = request.query.clone();
        let plugins = request.enabled.clone();
        let caps = request.caps.clone();
        let task = cx.background_executor().spawn(async move {
            let (filtered, _) = multi_launcher::common::query::split_action_filters(&query);
            let root = multi_launcher::file_search::query::explicit_search_root_candidate(&filtered)?;
            if !local_absolute(&root) || !checked_root(&root) { return None; }
            let multi_launcher::file_search::query::FileSearchCommand::StartSearch(draft) =
                multi_launcher::file_search::query::parse_file_search_query_with_root(&filtered, &root)? else { return None; };
            let canonical_root = multi_launcher::file_search::request::resolve_valid_roots([root]).into_iter().next()?;
            let action = multi_launcher::plugins::file_search::start_action(draft.clone());
            let plugin = ScopedStockRow(action.clone());
            let rows = search_providers::search_routed(
                [&plugin as &dyn multi_launcher::plugin::Plugin], &query,
                plugins.as_ref(), caps.as_ref(),
            );
            rows.into_iter().any(|(name, row)| name == "file_search" && row == action)
                .then_some((action, draft, canonical_root))
        });
        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = root.update(cx, |root, cx| {
                root.file_search_probe.active = false;
                let current = root.file_search_probe.revision == request.revision
                    && root.launcher_results.generation == request.generation
                    && root.launcher_results.query == request.query
                    && root.query.read(cx).value().as_str() == request.query
                    && root.root_visibility.active.load(Ordering::Acquire)
                    && root.root_visibility.visible.load(Ordering::Acquire)
                    && !root.restart_admission.is_fenced()
                    && enabled(root.enabled_plugins.as_ref(), root.enabled_capabilities.as_ref());
                if current {
                    if let Some((action, draft, canonical_root)) = result {
                        let mut rows = request.other;
                        let names: Vec<_> = root.search_catalog.providers.plugins().map(|p| p.name().to_owned()).collect();
                        let position = names.iter().position(|name| name == "file_search").unwrap_or(names.len());
                        let index = rows.iter().position(|(name, _)| names.iter().position(|n| n == name).is_some_and(|i| i > position)).unwrap_or(rows.len());
                        rows.insert(index, ("file_search".into(), action.clone()));
                        root.publish_launcher_provider_results(request.query.clone(), rows, false);
                        root.file_search_probe.receipt = Some(ScopedReceipt {
                            query: request.query, action, generation: root.launcher_results.generation, draft, canonical_root,
                        });
                        cx.notify();
                    }
                }
                root.launch_file_search_probe(cx);
            });
        }).detach();
    }

    pub(super) fn open_typed_file_search(&mut self, receipt: ScopedReceipt, window: &mut Window, cx: &mut Context<Self>) {
        let Some(_permit) = self.restart_admission.admit() else { return; };
        if self.file_search_popup.is_some() { self.reveal_file_search_popup(Some(receipt), cx); return; }
        if self.file_search.as_ref().is_some_and(|view| view.read(cx).rehosting) { return; }
        let was_open = self.file_search.as_ref().is_some_and(|view| view.read(cx).open);
        let view = if let Some(view) = &self.file_search { view.clone() } else {
            let root = cx.entity().downgrade();
            let admission = Arc::clone(&self.restart_admission);
            let settings = self.file_search_settings.clone();
            let native = self.native_window;
            let view = cx.new(|cx| FileSearchView::new(window, cx, root, admission, settings, native));
            self.file_search = Some(view.clone());
            view
        };
        view.update(cx, |view, cx| {
            if !view.open { view.popout = false; view.replace_mount(window.window_handle(), self.native_window, window, cx); }
            view.start_typed(receipt, window, cx);
        });
        if !was_open && self.file_search_prefers_popout() {
            let weak = view.downgrade(); cx.spawn(async move |_, cx| { let _ = weak.update(cx, |view, cx| view.request_rehost(true, cx)); }).detach();
        }
        self.root_scroll.scroll_to_top_of_item(self.file_search_scroll_item());
        self.query_history.reset();
        cx.notify();
    }
}

#[cfg(test)]
mod typed_tests {
    use super::{ProbeOwner, ProbeRequest, ScopedReceipt, validated_root_matches};
    use std::{collections::HashSet, path::PathBuf};
    use multi_launcher::file_search::{model::{SearchKind, SearchScope}, query::SearchRequestDraft};

    #[test]
    fn remount_and_rollback_preserve_search_identity_but_retire_old_rows() {
        use super::{mounted_search_row_current, utility_mount::MountOwner};
        use multi_launcher::file_search::model::SearchId;
        let owner = MountOwner::default();
        let dock = owner.attach();
        let search = Some(SearchId(41));
        assert!(mounted_search_row_current(&dock, true, 7, search, 7, search, 2, 0..10));
        let candidate = owner.attach();
        assert!(!mounted_search_row_current(&dock, true, 7, search, 7, search, 2, 0..10));
        assert!(mounted_search_row_current(&candidate, true, 7, search, 7, search, 2, 0..10));
        let rollback = owner.attach();
        assert!(!mounted_search_row_current(&candidate, true, 7, search, 7, search, 2, 0..10));
        assert!(mounted_search_row_current(&rollback, true, 7, search, 7, search, 2, 0..10));
        assert!(!mounted_search_row_current(&rollback, true, 7, search, 7, Some(SearchId(42)), 2, 0..10));
    }

    #[test]
    fn popup_return_requires_exact_current_child_parent_selection_and_focus() {
        use super::popup_return_allowed as allowed;
        assert!(allowed(true, true, true, true));
        assert!(!allowed(false, true, true, true));
        assert!(!allowed(true, false, true, true));
        assert!(!allowed(true, true, false, true));
        assert!(!allowed(true, true, true, false));
    }

    #[test]
    fn popup_delivery_waits_for_accepted_activation_and_current_mount() {
        use super::pending_popup_ready;
        assert!(!super::super::secondary_activation_request_allowed(true, false, true));
        assert!(super::super::secondary_activation_request_allowed(false, true, true));
        assert!(!super::super::secondary_activation_request_allowed(false, true, false));
        assert!(!pending_popup_ready(true, true, false, true, true));
        assert!(pending_popup_ready(true, true, true, true, true));
        assert!(!pending_popup_ready(false, true, true, true, true));
        assert!(!pending_popup_ready(true, false, true, true, true));
        assert!(!pending_popup_ready(true, true, true, false, true));
        assert!(!pending_popup_ready(true, true, true, true, false));
    }

    #[test]
    fn content_excerpt_is_bounded_utf8_and_keeps_match_near_center() {
        let line = format!("{}éneedle{}", "λ".repeat(1000), "界".repeat(1000));
        let found = multi_launcher::file_search::model::ContentMatch::new(1, line, 2001, 2008);
        let excerpt = super::match_excerpt(&found);
        assert!(excerpt.chars().count() <= 182);
        assert!(excerpt.contains("éneedle"));
        assert!(excerpt.starts_with('…') && excerpt.ends_with('…'));
    }

    #[test]
    fn content_rows_keep_exact_file_match_and_duplicate_occurrence() {
        use super::{append_content_rows, ContentRow};
        use multi_launcher::file_search::model::{ContentFileResult, ContentMatch, FileSearchResultKey};
        let found = ContentMatch::new(7, "café needle".into(), 6, 12);
        let file = ContentFileResult { path: PathBuf::from("folder/é.txt"), file_name: "é.txt".into(),
            modified: None, filename_relevance: None, arrival_index: 0, total_matches: 2,
            matches: vec![found.clone(), found.clone()], truncated: false };
        let mut rows: Vec<ContentRow> = Vec::new();
        let mut occurrences = std::collections::HashMap::new();
        append_content_rows(&mut rows, &mut occurrences, file.clone());
        append_content_rows(&mut rows, &mut occurrences, file);
        assert_eq!(rows.len(), 4);
        for (index, row) in rows.iter().enumerate() {
            assert_eq!(row.path, PathBuf::from("folder/é.txt"));
            assert_eq!(row.found, found);
            assert!(matches!(&row.key, FileSearchResultKey::Content { occurrence, .. } if *occurrence == index));
        }
        assert!(rows[0].first_in_file && !rows[1].first_in_file && rows[2].first_in_file);
    }

    #[test]
    fn typed_receipt_allows_repeat_and_rejects_stale_authority() {
        let query = r#"fs file "Café Marker" "C:\Owned Corpus""#;
        let draft = SearchRequestDraft { kind: SearchKind::Filename, root: Some(PathBuf::from(r"C:\Owned Corpus")), search_text: "Café Marker".into() };
        let action = multi_launcher::plugins::file_search::start_action(draft.clone());
        let mut owner = ProbeOwner::default();
        owner.receipt = Some(ScopedReceipt { query: query.into(), action: action.clone(), generation: 7, draft, canonical_root: PathBuf::from("canonical") });
        assert!(owner.receipt(6, query, &action, None, None, false).is_none());
        let mut forged = action.clone();
        forged.desc.push('!');
        assert!(owner.receipt(7, query, &forged, None, None, false).is_none());
        assert!(owner.receipt(7, query, &action, Some(&HashSet::new()), None, false).is_none());
        assert!(owner.receipt(7, query, &action, None, None, true).is_none());
        assert!(owner.receipt(7, query, &action, None, None, false).is_some());
        assert!(owner.receipt(7, query, &action, None, None, false).is_some());
        owner.retire();
        assert!(owner.receipt(7, query, &action, None, None, false).is_none());
    }

    #[test]
    fn typed_content_receipts_require_exact_alias_action_and_live_scope() {
        use std::collections::HashMap;
        let draft = SearchRequestDraft { kind: SearchKind::Content,
            root: Some(PathBuf::from(r"C:\Owned Corpus")), search_text: "café needle".into() };
        let action = multi_launcher::plugins::file_search::start_action(draft.clone());
        for alias in ["content", "contents", "text"] {
            let query = format!(r#"fs {alias} "café needle" "C:\Owned Corpus""#);
            let mut owner = ProbeOwner::default();
            owner.receipt = Some(ScopedReceipt { query: query.clone(), action: action.clone(), generation: 7,
                draft: draft.clone(), canonical_root: PathBuf::from("canonical") });
            assert!(owner.receipt(7, &query, &action, None, None, false).is_some());
            assert!(owner.receipt(7, &query, &action, None, None, false).is_some());
            assert!(owner.receipt(8, &query, &action, None, None, false).is_none());
            assert!(owner.receipt(7, &query, &action, None, None, true).is_none());
            assert!(owner.receipt(7, &query, &action, Some(&HashSet::new()), None, false).is_none());
            let caps = HashMap::from([("file_search".to_owned(), Vec::new())]);
            assert!(owner.receipt(7, &query, &action, None, Some(&caps), false).is_none());
            let mut forged = action.clone(); forged.desc.push('!');
            assert!(owner.receipt(7, &query, &forged, None, None, false).is_none());
            let wrong_query = query.replace("café needle", "other");
            assert!(owner.receipt(7, &wrong_query, &action, None, None, false).is_none());
            let wrong_root = query.replace("Owned Corpus", "Other Corpus");
            assert!(owner.receipt(7, &wrong_root, &action, None, None, false).is_none());
            let filename = multi_launcher::plugins::file_search::start_action(SearchRequestDraft { kind: SearchKind::Filename, ..draft.clone() });
            assert!(owner.receipt(7, &query, &filename, None, None, false).is_none());
            owner.retire();
            assert!(owner.receipt(7, &query, &action, None, None, false).is_none());
        }
    }

    #[test]
    fn typed_source_keeps_original_values_and_requires_a_root() {
        let root = PathBuf::from("owned/é directory");
        let draft = SearchRequestDraft { kind: SearchKind::Content, root: Some(root.clone()), search_text: "café needle".into() };
        let source = super::typed_source(draft.clone(), PathBuf::from("canonical")).unwrap();
        assert_eq!(source.kind, SearchKind::Content);
        assert_eq!(source.term, "café needle");
        assert_eq!(source.directory, root);
        assert_eq!(source.expected_root, Some(PathBuf::from("canonical")));
        assert!(super::typed_source(SearchRequestDraft { root: None, ..draft }, PathBuf::from("canonical")).is_none());
    }

    #[test]
    fn typed_content_explicit_roots_share_filename_admission_and_recheck() {
        let root = std::env::current_dir().unwrap();
        for mode in ["content", "contents", "text"] {
            let query = format!("fs {mode} \"café needle\" \"{}\"", root.display());
            assert!(super::typed_query(&query));
        }
        assert!(!super::typed_query("fs content needle"));
        assert!(validated_root_matches(Some(&root), &SearchScope::Roots { roots: vec![root.clone()] }));
        assert!(!validated_root_matches(Some(&root), &SearchScope::Roots { roots: vec![root.join("replacement")] }));
    }

    #[test]
    fn typed_root_must_match_validated_canonical_identity() {
        let expected = std::path::Path::new("one");
        assert!(validated_root_matches(Some(expected), &SearchScope::Roots { roots: vec![expected.into()] }));
        assert!(!validated_root_matches(Some(expected), &SearchScope::Roots { roots: vec![PathBuf::from("replacement")] }));
        assert!(!validated_root_matches(Some(expected), &SearchScope::Roots { roots: vec![] }));
    }

    #[test]
    fn typed_probe_retirement_invalidates_receipt_and_pending_generation() {
        let mut owner = ProbeOwner::default();
        owner.pending = Some(ProbeRequest {
            query: "fs file needle /root".into(), generation: 1, revision: owner.revision,
            other: Vec::new(), enabled: None, caps: None,
        });
        let previous = owner.revision;
        owner.retire();
        assert_ne!(owner.revision, previous);
        assert!(owner.receipt.is_none());
        assert!(owner.pending.is_none());
    }
}
