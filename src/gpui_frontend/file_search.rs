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

pub(super) struct FileSearchView {
    pub open: bool,
    root: WeakEntity<RootWindow>,
    admission: Arc<RestartAdmission>,
    settings: FileSearchSettings,
    coordinator: SearchCoordinator,
    term: Entity<InputState>,
    directory: Entity<InputState>,
    focus: FocusHandle,
    calculate_focus: FocusHandle,
    explorer_focus: FocusHandle,
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
                |this, _, event: &InputEvent, cx| match event {
                    InputEvent::Change => {
                        this.invalidate("Search changed. Search again.");
                        cx.notify();
                    }
                    InputEvent::PressEnter {
                        secondary: false,
                        shift: false,
                    } => {
                        this.start(cx);
                    }
                    _ => {}
                },
            ));
        }
        Self {
            open: true,
            root,
            admission,
            coordinator: SearchCoordinator::from_settings(settings.clone()),
            settings,
            term,
            directory,
            focus,
            calculate_focus: cx.focus_handle(),
            explorer_focus,
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

    fn invalidate(&mut self, message: &str) {
        self.explorer_authority.store(false, Ordering::Release);
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
        self.set_page(0);
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

    pub(super) fn hash_action_is_focused(&self, window: &Window) -> bool {
        self.calculate_focus.is_focused(window)
    }

    pub(super) fn retire_explorer(&self) {
        self.explorer_authority.store(false, Ordering::Release);
    }

    pub fn shutdown(&mut self) {
        self.invalidate("Search closed.");
        self.escape_waiting_for_release = false;
        self.open = false;
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.shutdown();
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
        if !window.is_window_active()
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
        if self.explorer_pending || !window.is_window_active() || self.admission.is_fenced()
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
        let worker_authority = authority.clone();
        let task = cx.background_executor().spawn(async move {
            let current = || {
                worker_authority.load(Ordering::Acquire)
                    && !admission.is_fenced() && visibility.is_current(revision, true)
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
        if !held.unwrap_or(self.escape_waiting_for_release) {
            if self.running() {
                self.escape_waiting_for_release = true;
                self.invalidate("Search cancelled.");
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

    fn set_page(&mut self, page: usize) {
        self.explorer_authority.store(false, Ordering::Release);
        self.page = page;
        self.selected = None;
        self.results_scroll.set_offset(point(px(0.0), px(0.0)));
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        if !self.open || self.running() {
            return;
        }
        self.start_with_expected_root(None, cx);
    }

    fn start_typed(&mut self, receipt: ScopedReceipt, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.kind = SearchKind::Filename;
        self.term.update(cx, |input, cx| input.set_value(receipt.draft.search_text, window, cx));
        let directory = receipt.draft.root.unwrap_or_default().to_string_lossy().into_owned();
        self.directory.update(cx, |input, cx| input.set_value(directory, window, cx));
        self.term.focus_handle(cx).focus(window, cx);
        self.start_with_expected_root(Some(receipt.canonical_root), cx);
    }

    fn start_with_expected_root(&mut self, expected_root: Option<PathBuf>, cx: &mut Context<Self>) {
        let Some(permit) = self.admission.admit() else {
            self.status = "Application is closing.".into();
            cx.notify();
            return;
        };
        self.invalidate("Validating directory…");
        self.validating = true;
        let generation = self.generation;
        let term = self.term.read(cx).value().to_string();
        let directory = self.directory.read(cx).value().to_string();
        let settings = self.settings.clone();
        let kind = self.kind;
        let task = cx.background_executor().spawn(async move {
            let preferences = settings.ui_preferences.clone();
            let request = if expected_root.is_some() && !checked_root(std::path::Path::new(&directory)) {
                Err(multi_launcher::file_search::request::RequestError::EmptyDirectoryRoots)
            } else { build_request(&RequestInput {
                text: &term,
                kind,
                scope: RootScope::Directory,
                custom_roots: vec![PathBuf::from(directory)],
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
                    self.invalidate("Search cancelled.");
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
                .on_click(cx.listener(move |this, _, _, cx| {
                    if current_row(
                        this.open,
                        this.generation,
                        this.active,
                        generation,
                        active,
                        index,
                        this.visible_rows(),
                    ) {
                        this.explorer_authority.store(false, Ordering::Release);
                        this.selected = Some(index);
                        cx.notify();
                    }
                }));
                div().flex_shrink_0().child(button)
            })
            .collect::<Vec<_>>()
        } else {
            self.content_rows.iter().enumerate().skip(start).take(MOUNTED_ROWS).map(|(index, row)| {
                let key = row.key.clone();
                let path = row.path.clone();
                let label = format!("{}:{}  {}", row.found.line_number,
                    row.found.column.map(|column| column.saturating_add(1)).unwrap_or(1), match_excerpt(&row.found));
                let button = Button::new(SharedString::from(format!("file-search-content-{generation}-{index}")))
                    .accessibility_label(format!("Content match: {label}"))
                    .label(label).tab_stop(true).selected(self.selected == Some(index))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.kind == SearchKind::Content && current_row(this.open, this.generation, this.active,
                            generation, active, index, this.visible_rows())
                            && this.content_rows.get(index).is_some_and(|row| row.key == key && row.path == path) {
                            this.explorer_authority.store(false, Ordering::Release);
                            this.selected = Some(index); cx.notify();
                        }
                    }));
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
                    .on_click(cx.listener(|this, _, _, cx| { if this.kind != SearchKind::Filename {
                        this.invalidate("Mode changed. Search again."); this.kind = SearchKind::Filename; cx.notify();
                    } })))
                .child(Button::new("file-search-content-mode").label("Content").selected(self.kind == SearchKind::Content)
                    .on_click(cx.listener(|this, _, _, cx| { if this.kind != SearchKind::Content {
                        this.invalidate("Mode changed. Search again."); this.kind = SearchKind::Content; cx.notify();
                    } }))))
            .child(Label::new("Directory"))
            .child(Input::new(&self.directory).aria_label("Search directory"))
            .child(Label::new(if self.kind == SearchKind::Filename { "Filename" } else { "Content" }))
            .child(Input::new(&self.term).aria_label(if self.kind == SearchKind::Filename { "Filename search text" } else { "Content search text" }))
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
                            .on_click(cx.listener(|this, _, _, cx| this.start(cx))),
                    )
                    .child(
                        Button::new("file-search-cancel")
                            .tab_stop(true)
                            .label("Cancel")
                            .disabled(!running)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.invalidate("Search cancelled.");
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("file-search-calculate-hash")
                            .tab_stop(true)
                            .track_focus(&self.calculate_focus)
                            .label("Calculate hash")
                            .disabled(hash_receipt.is_none())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(receipt) = hash_receipt.clone() {
                                    this.calculate_selected(receipt, window, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("file-search-open-explorer")
                            .tab_stop(true).track_focus(&self.explorer_focus)
                            .label("Open in Explorer")
                            .disabled(selected_receipt.is_none() || self.explorer_pending)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(receipt) = selected_receipt.clone() {
                                    this.open_selected_in_explorer(receipt, window, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("file-search-close")
                            .tab_stop(true)
                            .label("Close")
                            .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
                    ),
            )
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
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.set_page(this.page.saturating_sub(1));
                                    cx.notify();
                                })),
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
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if (this.page + 1) * MOUNTED_ROWS < this.row_count() {
                                        this.set_page(this.page + 1);
                                        cx.notify();
                                    }
                                })),
                        ),
                )
            })
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
            && multi_launcher::plugins::file_search::scoped_filename_start(query, action).as_ref() == Some(&receipt.draft)
        ).cloned()
    }
}

pub(super) fn enabled(plugins: Option<&HashSet<String>>, caps: Option<&HashMap<String, Vec<String>>>) -> bool {
    plugins.is_none_or(|plugins| plugins.contains("file_search"))
        && multi_launcher::settings::provider_search_capability_enabled(caps, "file_search")
}

pub(super) fn typed_query(query: &str) -> bool {
    let (filtered, _) = multi_launcher::common::query::split_action_filters(query);
    multi_launcher::file_search::query::explicit_filename_root_candidate(&filtered)
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
    fn open_file_search_explorer(&mut self, source: WeakEntity<FileSearchView>, receipt: SelectionReceipt,
        window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = source.upgrade() else { return; };
        let source = view.read(cx);
        let contexts = window.context_stack();
        let revision = self.root_visibility.request_revision.load(Ordering::Acquire);
        if !self.file_search.as_ref().is_some_and(|mounted| mounted.entity_id() == receipt.owner)
            || view.entity_id() != receipt.owner
            || !self.root_window.is_some_and(|root| root.window_id() == window.window_handle().window_id())
            || !self.root_visibility.is_current(revision, true) || !window.is_window_active()
            || self.action_sheet.is_some() || self.context_favorite_popup.is_some()
            || context_stack_has_primary(&contexts, "PopupMenu")
            || context_stack_has_primary(&contexts, "AlertDialog")
            || context_stack_has_primary(&contexts, "Dialog")
            || !source.explorer_focus.is_focused(window) || source.explorer_pending
            || !source.receipt_is_current(&receipt, view.entity_id()) { return; }
        if !self.native_window.is_some_and(|native| native_window_is_valid(native)
            && current_foreground_identity() == Some(native.identity)) { return; }
        let Some(permit) = self.restart_admission.admit() else { return; };
        let visibility = self.root_visibility.clone();
        let native = self.native_window;
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
            let root = multi_launcher::file_search::query::explicit_filename_root_candidate(&filtered)?;
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
        let view = if let Some(view) = &self.file_search { view.clone() } else {
            let root = cx.entity().downgrade();
            let admission = Arc::clone(&self.restart_admission);
            let settings = self.file_search_settings.clone();
            let view = cx.new(|cx| FileSearchView::new(window, cx, root, admission, settings));
            self.file_search = Some(view.clone());
            view
        };
        view.update(cx, |view, cx| view.start_typed(receipt, window, cx));
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
