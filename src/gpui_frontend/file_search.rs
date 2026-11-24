//! Retained docked filename-search pane.
use super::*;
use gpui_kit::{KeyUpEvent, base::Selectable};
use multi_launcher::file_search::{
    coordinator::SearchCoordinator,
    model::{
        FileKind, FilenameResult, SearchBackend, SearchDiagnostic, SearchEvent, SearchId,
        SearchKind, SearchResult, SearchStatus,
    },
    request::{RequestInput, RootScope, build_request},
    settings::FileSearchSettings,
};

const EVENT_BATCH: usize = 128;
const MOUNTED_ROWS: usize = 100;

#[cfg(windows)]
pub(super) fn intercept_escape(
    root: &WeakEntity<RootWindow>,
    receipt: PhysicalKeydown,
    event: &gpui_kit::KeystrokeEvent,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    if event.keystroke.key != "escape"
        || receipt.virtual_key != 0x1b
        || receipt.modifiers.modified()
        || !context_stack_has_primary(&event.context_stack, "FileSearchPane")
    {
        return false;
    }
    let Some(root) = root.upgrade() else {
        return false;
    };
    let root = root.read(cx);
    if !root.root_visibility.active.load(Ordering::Acquire)
        || !root.root_visibility.visible.load(Ordering::Acquire)
        || !root
            .root_window
            .is_some_and(|handle| handle.window_id() == window.window_handle().window_id())
        || !root.native_window.is_some_and(|native| {
            receipt.hwnd == native.identity.hwnd
                && native_window_is_valid(native)
                && current_foreground_identity() == Some(native.identity)
        })
    {
        return false;
    }
    let view = root.file_search.clone();
    view.is_some_and(|view| {
        view.update(cx, |view, cx| {
            view.handle_escape(window, cx, Some(receipt.held))
        })
    })
}

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
    escape_waiting_for_release: bool,
    generation: u64,
    active: Option<SearchId>,
    validating: bool,
    status: String,
    backend: Option<SearchBackend>,
    results: Vec<FilenameResult>,
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
        let term = cx.new(|cx| InputState::new(window, cx).placeholder("Filename to find"));
        let directory = cx.new(|cx| InputState::new(window, cx).placeholder("Directory to search"));
        let focus = cx.focus_handle();
        let mut subscriptions = vec![
            cx.on_focus_out(&focus, window, |this, _, _, _| {
                this.escape_waiting_for_release = false;
            }),
            cx.observe_window_activation(window, |this, window, _| {
                if !window.is_window_active() {
                    this.escape_waiting_for_release = false;
                }
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
            escape_waiting_for_release: false,
            generation: 0,
            active: None,
            validating: false,
            status: "Enter a directory and filename, then search.".into(),
            backend: None,
            results: Vec::new(),
            selected: None,
            page: 0,
            results_scroll: ScrollHandle::new(),
            truncated: false,
            _subscriptions: subscriptions,
        }
    }

    fn invalidate(&mut self, message: &str) {
        self.generation = self.generation.wrapping_add(1).max(1);
        if self.active.take().is_some() {
            self.coordinator.cancel_and_retire();
            // Close the retired receiver so queued results cannot delay the next search.
            self.coordinator = SearchCoordinator::from_settings(self.settings.clone());
        }
        self.validating = false;
        self.results.clear();
        self.set_page(0);
        self.truncated = false;
        self.backend = None;
        self.status = message.to_owned();
    }

    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            self.status = "Enter a directory and filename, then search.".to_owned();
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
        let row = self.results.get(index)?;
        eligible_file_row(row, &row.path).then(|| SelectionReceipt {
            owner,
            search,
            generation: self.generation,
            index,
            path: row.path.clone(),
        })
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
        if !self.receipt_is_current(receipt, owner) {
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
            || !self.receipt_is_current(&receipt, cx.entity_id())
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

    fn running(&self) -> bool {
        self.validating
            || (self.active.is_some() && self.coordinator.active_status() == SearchStatus::Running)
    }

    fn handle_escape(
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

    fn visible_rows(&self) -> std::ops::Range<usize> {
        let start = self.page.saturating_mul(MOUNTED_ROWS);
        let end = start.saturating_add(MOUNTED_ROWS).min(self.results.len());
        start..end
    }

    fn set_page(&mut self, page: usize) {
        self.page = page;
        self.selected = None;
        self.results_scroll.set_offset(point(px(0.0), px(0.0)));
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        if !self.open || self.running() {
            return;
        }
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
        let task = cx.background_executor().spawn(async move {
            let preferences = settings.ui_preferences.clone();
            let request = build_request(&RequestInput {
                text: &term,
                kind: SearchKind::Filename,
                scope: RootScope::Directory,
                custom_roots: vec![PathBuf::from(directory)],
                case_sensitive: settings.case_sensitive,
                include_hidden: settings.include_hidden_files,
                excluded_directory_names: settings.excluded_directory_names.clone(),
                settings: &settings,
                preferences: &preferences,
            });
            (request, permit)
        });
        let owner = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let (request, permit) = task.await;
            let started = owner
                .update(cx, |this, cx| {
                    if !this.open || this.generation != generation || this.admission.is_fenced() {
                        return false;
                    }
                    this.validating = false;
                    match request {
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
                SearchEvent::Progress { progress, .. } => {
                    self.truncated |= progress.global_truncated;
                    self.status = format!(
                        "Searching… {} files scanned, {} results",
                        progress.files_scanned, progress.results_found
                    );
                }
                SearchEvent::Diagnostic { diagnostic, .. } => {
                    self.truncated |= matches!(
                        &diagnostic,
                        SearchDiagnostic::FilenameResultsTruncated { .. }
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
                    self.status =
                        format!("Completed{backend} · {} results{limit}", self.results.len());
                }
                SearchEvent::Cancelled { .. } => {
                    self.invalidate("Search cancelled.");
                    break;
                }
                SearchEvent::Failed { error, .. } => {
                    self.status = format!("Search failed: {error}")
                }
                _ => {}
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
        let selected_path = selected_receipt
            .as_ref()
            .map(|receipt| receipt.path.display().to_string());
        let rows = self
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
                        this.selected = Some(index);
                        cx.notify();
                    }
                }));
                div().flex_shrink_0().child(button)
            })
            .collect::<Vec<_>>();
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
            .child(Label::new("File Search · Filename"))
            .child(Label::new("Directory"))
            .child(Input::new(&self.directory).aria_label("Search directory"))
            .child(Label::new("Filename"))
            .child(Input::new(&self.term).aria_label("Filename search text"))
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
                            .disabled(selected_receipt.is_none())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(receipt) = selected_receipt.clone() {
                                    this.calculate_selected(receipt, window, cx);
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
            .child(Label::new(self.status.clone()).id("file-search-status"))
            .when_some(selected_path, |pane, path| {
                pane.child(Label::new("Selected file"))
                    .child(Label::new(path).id("file-search-selected-path"))
            })
            .when(self.results.len() > MOUNTED_ROWS, |pane| {
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
                            self.results.len()
                        )))
                        .child(
                            Button::new("file-search-next-page")
                                .tab_stop(true)
                                .label("Next page")
                                .disabled(end >= self.results.len())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if (this.page + 1) * MOUNTED_ROWS < this.results.len() {
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
