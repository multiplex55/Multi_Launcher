use super::*;

const ACTIONS_FILE: &str = "actions.json";
type CatalogAction = multi_launcher::actions::Action;

struct CatalogSnapshot {
    rows: Vec<CatalogAction>,
    verified_folders: Vec<CatalogAction>,
}

impl CatalogSnapshot {
    fn from_rows(rows: Vec<CatalogAction>) -> Self {
        let verified_folders = super::saved_folder::verified_actions(&rows);
        Self { rows, verified_folders }
    }
}


fn catalog_filter(filter: &str) -> String {
    if filter.trim().is_empty() {
        String::new()
    } else {
        filter.to_lowercase()
    }
}

fn catalog_matches(action: &CatalogAction, query: &str) -> bool {
    query.is_empty()
        || action.label.to_lowercase().contains(query)
        || action.desc.to_lowercase().contains(query)
        || action.action.to_lowercase().contains(query)
}

fn indexed_match_ordinals(actions: &[CatalogAction], filter: &str) -> Vec<usize> {
    actions.iter().enumerate()
        .filter_map(|(index, action)| catalog_matches(action, filter).then_some(index))
        .collect()
}

#[derive(Default)]
struct CatalogReads {
    revision: u64,
    pending: Option<u64>,
}

impl CatalogReads {
    fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
        self.pending = None;
    }

    fn begin(&mut self) -> u64 {
        self.invalidate();
        self.pending = Some(self.revision);
        self.revision
    }

    fn finish(&mut self, receipt: u64) -> bool {
        if self.pending != Some(receipt) {
            return false;
        }
        self.pending = None;
        true
    }
}

pub(super) const APP_WRITE_CLOSE_STATUS: &str =
    "Wait for the App Editor save to finish before closing Multi Launcher.";
pub(super) const APP_WRITE_RESTART_STATUS: &str =
    "Wait for the App Editor save to finish before restarting in egui.";
pub(super) const APP_PICKER_STATUS: &str = "Wait for the App Editor file picker to finish.";
pub(super) const APP_DRAFT_RESTART_STATUS: &str =
    "Save or Cancel the App Editor draft before restarting in egui.";

enum AppWriteError {
    Stale,
    Message(String),
}

fn checked_action_error(error: multi_launcher::actions::CheckedActionError) -> AppWriteError {
    match error {
        multi_launcher::actions::CheckedActionError::StaleTarget => AppWriteError::Stale,
        multi_launcher::actions::CheckedActionError::Store(error) => {
            AppWriteError::Message(format!("Could not save app changes: {error:#}"))
        }
    }
}

fn clear_app_diagnostic(status: &mut Option<String>) {
    if matches!(
        status.as_deref(),
        Some(APP_WRITE_CLOSE_STATUS | APP_WRITE_RESTART_STATUS | APP_DRAFT_RESTART_STATUS | APP_PICKER_STATUS)
    ) {
        *status = None;
    }
}

pub(super) struct AppEditorState {
    pub(super) open: bool,
    draft: Option<AddAppDraft>,
    filter: Option<Entity<InputState>>,
    filter_subscription: Option<Subscription>,
    catalog: Arc<[CatalogAction]>,
    catalog_revision: u64,
    catalog_stale: bool,
    catalog_error: Option<String>,
    reads: CatalogReads,
    catalog_list: ListState,
    saved_group_focus: FocusHandle,
    indexed_actions: Arc<Vec<CatalogAction>>,
    indexed_generation: u64,
    indexed_matches: Arc<Vec<usize>>,
    indexed_filter: String,
    status: Option<String>,
    writes: AddAppWrites,
    picker: AddAppWrites,
    browse_focus: FocusHandle,
    picker_lease: Option<Arc<AtomicBool>>,
    picker_guard: Option<Subscription>,
    owner_focus: FocusHandle,
    new_focus: FocusHandle,
    arguments_focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    return_scroll: Option<Point<Pixels>>,
}

#[derive(Default)]
struct AddAppWrites {
    pending: Option<u64>,
    next: u64,
}

impl AddAppWrites {
    fn begin(&mut self) -> Option<u64> {
        if self.pending.is_some() {
            return None;
        }
        self.next = self.next.wrapping_add(1).max(1);
        self.pending = Some(self.next);
        self.pending
    }

    fn finish(&mut self, receipt: u64) -> bool {
        if self.pending != Some(receipt) {
            return false;
        }
        self.pending = None;
        true
    }
}

fn selected_path_text(path: &std::path::Path) -> Option<&str> {
    path.to_str().filter(|value| !value.contains(['\r', '\n']))
}

fn picker_may_apply(open: bool, current_draft: bool, current_host: bool, selected: bool) -> bool {
    open && current_draft && current_host && selected
}

#[derive(Clone)]
struct CatalogSelection {
    revision: u64,
    index: usize,
    original: CatalogAction,
}

impl CatalogSelection {
    fn target(
        &self,
        catalog: &[CatalogAction],
        revision: u64,
        stale: bool,
    ) -> Option<multi_launcher::actions::CustomActionRowTarget> {
        if stale || revision != self.revision || catalog.get(self.index) != Some(&self.original) {
            return None;
        }
        multi_launcher::actions::custom_action_row_target(catalog, self.index)
    }
}

#[derive(Clone)]
enum DraftMode {
    Add,
    Edit(multi_launcher::actions::CustomActionRowTarget),
}

#[derive(Clone)]
enum AppDraftField {
    Single(Entity<InputState>),
    Multiline(Entity<TextareaState>),
}

impl AppDraftField {
    fn new(value: &str, window: &mut Window, cx: &mut Context<RootWindow>) -> Self {
        if value.contains(['\r', '\n']) {
            Self::Multiline(cx.new(|cx| {
                TextareaState::new(window, cx)
                    .default_value(value.to_owned())
                    .auto_grow(1, 3)
                    .submit_on_enter(true)
            }))
        } else {
            Self::Single(cx.new(|cx| InputState::new(window, cx).default_value(value.to_owned())))
        }
    }

    fn entity_id(&self) -> EntityId {
        match self {
            Self::Single(input) => input.entity_id(),
            Self::Multiline(input) => input.entity_id(),
        }
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self {
            Self::Single(input) => input.focus_handle(cx),
            Self::Multiline(input) => input.focus_handle(cx),
        }
    }

    fn value(&self, cx: &App) -> String {
        match self {
            Self::Single(input) => input.read(cx).value().to_string(),
            Self::Multiline(input) => input.read(cx).value().to_string(),
        }
    }

    fn render(&self, label: &'static str, pending: bool) -> AnyElement {
        match self {
            Self::Single(input) => Input::new(input)
                .disabled(pending)
                .aria_label(label)
                .into_any_element(),
            Self::Multiline(input) => Textarea::new(input)
                .aria_label(label)
                .tab_index(0)
                .readonly(pending)
                .into_any_element(),
        }
    }
}

struct AddAppDraft {
    mode: DraftMode,
    baseline: AddAppSnapshot,
    label: AppDraftField,
    description: AppDraftField,
    path: AppDraftField,
    args: AppDraftField,
    show_args: bool,
    scroll: ScrollHandle,
    _focus_subscriptions: Vec<Subscription>,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct AddAppSnapshot {
    pub(super) label: String,
    pub(super) description: String,
    pub(super) path: String,
    pub(super) show_args: bool,
    pub(super) args: String,
}

impl AppEditorState {
    pub(super) fn new(cx: &mut Context<RootWindow>) -> Self {
        let saved_group_focus = cx.focus_handle();
        let catalog_list = ListState::new(1, ListAlignment::Top, px(800.0));
        catalog_list.splice_focusable(0..1, [Some(saved_group_focus.clone())]);
        Self {
            open: false,
            draft: None,
            filter: None,
            filter_subscription: None,
            catalog: Arc::from([]),
            catalog_revision: 0,
            catalog_stale: false,
            catalog_error: None,
            reads: CatalogReads::default(),
            catalog_list,
            saved_group_focus,
            indexed_actions: Arc::new(Vec::new()),
            indexed_generation: 0,
            indexed_matches: Arc::new(Vec::new()),
            indexed_filter: String::new(),
            status: None,
            writes: AddAppWrites::default(),
            picker: AddAppWrites::default(),
            browse_focus: cx.focus_handle(),
            picker_lease: None,
            picker_guard: None,
            owner_focus: cx.focus_handle(),
            new_focus: cx.focus_handle(),
            arguments_focus: cx.focus_handle(),
            return_focus: None,
            return_scroll: None,
        }
    }

    pub(super) fn restart_blocker(&self, cx: &App) -> Option<&'static str> {
        if self.picker.pending.is_some() { return Some(APP_PICKER_STATUS); }
        if self.write_pending() {
            return Some(APP_WRITE_RESTART_STATUS);
        }
        let draft = self.draft.as_ref()?;
        (self.snapshot(cx).as_ref() != Some(&draft.baseline)).then_some(APP_DRAFT_RESTART_STATUS)
    }

    pub(super) fn set_index(&mut self, generation: u64, actions: Arc<Vec<CatalogAction>>) {
        let old_count = self.catalog_list.item_count();
        self.indexed_generation = generation;
        self.indexed_actions = actions;
        self.indexed_matches = Arc::new(indexed_match_ordinals(&self.indexed_actions, &self.indexed_filter));
        self.catalog_list.splice(1..old_count, self.indexed_matches.len());
    }

    fn set_indexed_filter(&mut self, filter: &str) {
        let filter = catalog_filter(filter);
        if self.indexed_filter != filter {
            self.indexed_filter = filter;
            self.rebuild_indexed_matches();
        }
    }

    fn reset_catalog_list(&self) {
        self.catalog_list.reset(1 + self.indexed_matches.len());
        self.catalog_list.splice_focusable(0..1, [Some(self.saved_group_focus.clone())]);
    }

    fn rebuild_indexed_matches(&mut self) {
        self.indexed_matches = Arc::new(indexed_match_ordinals(&self.indexed_actions, &self.indexed_filter));
        self.reset_catalog_list();
    }

    fn publish_catalog(&mut self, rows: Vec<CatalogAction>) {
        if self.catalog_stale {
            self.status = None;
        }
        self.catalog_revision = self.catalog_revision.wrapping_add(1).max(1);
        self.catalog = rows.into();
        self.catalog_list.remeasure_items(0..1);
        self.catalog_stale = false;
        self.catalog_error = None;
    }

    fn target(
        &self,
        selection: &CatalogSelection,
    ) -> Option<multi_launcher::actions::CustomActionRowTarget> {
        selection.target(&self.catalog, self.catalog_revision, self.catalog_stale)
    }

    fn is_current_draft(&self, id: EntityId) -> bool {
        self.open
            && !self.write_pending()
            && self
                .draft
                .as_ref()
                .is_some_and(|draft| draft.label.entity_id() == id)
    }

    pub(super) fn drop_draft_id(&self) -> Option<EntityId> {
        self.draft.as_ref().map(|draft| draft.label.entity_id())
    }

    pub(super) fn external_drop_safe(&self) -> bool { !self.write_pending() }
    pub(super) fn accepts_external_drop(&self) -> bool {
        self.draft.is_none() && self.external_drop_safe()
    }

    pub(super) fn write_pending(&self) -> bool {
        self.writes.pending.is_some() || self.picker.pending.is_some()
    }

    pub(super) fn owns_focus_for_escape(&self, window: &Window, cx: &App) -> bool {
        self.owner_focus.contains_focused(window, cx)
    }
    pub(super) fn close_wait_status(&self) -> &'static str {
        if self.picker.pending.is_some() { APP_PICKER_STATUS } else { APP_WRITE_CLOSE_STATUS }
    }

    pub(super) fn revoke_picker_prelaunch(&mut self) {
        if let Some(lease) = &self.picker_lease { lease.store(false, Ordering::Release); }
    }

    fn begin_write(&mut self) -> Option<u64> {
        let receipt = self.writes.begin()?;
        self.reads.invalidate();
        self.status = Some("Saving app changes...".to_owned());
        Some(receipt)
    }

    fn finish_write(
        &mut self,
        receipt: u64,
        result: Result<CatalogSnapshot, AppWriteError>,
    ) -> bool {
        if !self.writes.finish(receipt) {
            return false;
        }
        match result {
            Ok(snapshot) => {
                self.reads.invalidate();
                self.publish_catalog(snapshot.rows);
                self.draft = None;
                self.status = None;
            }
            Err(AppWriteError::Stale) => {
                self.catalog_stale = true;
                self.status =
                    Some("App selection is stale. Cancel or refresh and select it again.".into());
            }
            Err(AppWriteError::Message(error)) => self.status = Some(error),
        }
        true
    }

    pub(super) fn snapshot(&self, cx: &App) -> Option<AddAppSnapshot> {
        let draft = self.draft.as_ref()?;
        Some(AddAppSnapshot {
            label: draft.label.value(cx),
            description: draft.description.value(cx),
            path: draft.path.value(cx),
            show_args: draft.show_args,
            args: draft.args.value(cx),
        })
    }
}

impl RootWindow {
    fn app_editor_scroll_item(&self) -> usize {
        self.quick_help_scroll_item()
            + usize::from(self.quick_help.open)
            + usize::from(self.result_favorites.is_open())
    }

    /// The menu route supplies its live origin before dismissing the menu.
    pub(super) fn open_app_editor(
        &mut self,
        origin: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.external_drop.revoke();
        self.open_app_editor_with_focus(origin, true, window, cx);
    }

    fn open_app_editor_with_focus(&mut self, origin: Option<FocusHandle>, focus: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced() {
            return;
        }
        if !self.app_editor.open {
            self.app_editor.return_focus =
                origin.filter(|focus| self.focus_handle.contains(focus, window));
            self.app_editor.return_scroll = Some(self.root_scroll.offset());
            self.app_editor.open = true;
            if self.app_editor.filter.is_none() {
                let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter apps"));
                let subscription = cx.subscribe(&filter, |root, source, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change)
                        && root
                            .app_editor
                            .filter
                            .as_ref()
                            .is_some_and(|filter| *filter == source)
                    {
                        let value = source.read(cx).value().to_string();
                        root.app_editor.set_indexed_filter(&value);
                        cx.notify();
                    }
                });
                self.app_editor.filter = Some(filter);
                self.app_editor.filter_subscription = Some(subscription);
            }
            self.refresh_app_catalog(window, cx);
        }
        self.root_scroll
            .scroll_to_top_of_item(self.app_editor_scroll_item());
        if focus {
            if let Some(draft) = &self.app_editor.draft {
                draft.label.focus_handle(cx).focus(window, cx);
            } else {
                self.app_editor.new_focus.focus(window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn open_dropped_app_draft(&mut self, path: &str, focus: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.app_editor.accepts_external_drop() || path.contains(['\r', '\n']) { return; }
        self.open_app_editor_with_focus(None, false, window, cx);
        self.open_app_draft_with_focus(DraftMode::Add, None, focus, window, cx);
        if let Some(draft) = &self.app_editor.draft {
            if let AppDraftField::Single(input) = &draft.path {
                input.update(cx, |input, cx| input.set_value(path.to_owned(), window, cx));
            }
        }
        cx.notify();
    }

    fn publish_app_editor_search_actions(&mut self, verified_folders: Vec<CatalogAction>, window: &mut Window, cx: &mut Context<Self>) {
        let owns_focus = self.app_editor.open
            && window.is_window_active()
            && self.app_editor.owner_focus.contains_focused(window, cx);
        if self.publish_saved_search_actions(self.app_editor.catalog.to_vec(), verified_folders, cx) && owns_focus {
            self.root_scroll
                .scroll_to_top_of_item(self.app_editor_scroll_item());
        }
    }

    fn refresh_app_catalog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.app_editor.open || self.app_editor.write_pending() {
            return;
        }
        let receipt = self.app_editor.reads.begin();
        let task = cx.background_executor().spawn(async move {
            multi_launcher::actions::load_actions(ACTIONS_FILE)
                .map(CatalogSnapshot::from_rows)
                .map_err(|error| format!("Could not load app catalog: {error:#}"))
        });
        let root = cx.entity().downgrade();
        let window_handle = window.window_handle();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = window_handle.update(cx, |_, window, app| {
                let _ = root.update(app, |root, cx| {
                    if root.app_editor.open && root.app_editor.reads.finish(receipt) {
                        match result {
                            Ok(snapshot) => {
                                root.app_editor.publish_catalog(snapshot.rows);
                                root.publish_app_editor_search_actions(snapshot.verified_folders, window, cx);
                            }
                            Err(error) => root.app_editor.catalog_error = Some(error),
                        }
                        cx.notify();
                    }
                });
            });
        })
        .detach();
    }

    fn new_app_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_app_draft(DraftMode::Add, None, window, cx);
    }

    fn edit_app(
        &mut self,
        selection: CatalogSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.app_editor.open
            || self.app_editor.write_pending()
            || self.app_editor.draft.is_some()
        {
            return;
        }
        let Some(target) = self.app_editor.target(&selection) else {
            self.app_editor.status =
                Some("App selection is stale. Refresh and select it again.".into());
            cx.notify();
            return;
        };
        self.open_app_draft(
            DraftMode::Edit(target),
            Some(selection.original),
            window,
            cx,
        );
    }

    fn open_app_draft(
        &mut self,
        mode: DraftMode,
        original: Option<CatalogAction>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.external_drop.revoke();
        self.open_app_draft_with_focus(mode, original, true, window, cx);
    }

    fn open_app_draft_with_focus(&mut self, mode: DraftMode, original: Option<CatalogAction>, focus: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.app_editor.open
            || self.restart_admission.is_fenced()
            || self.app_editor.write_pending()
            || self.app_editor.draft.is_some()
        {
            return;
        }
        let baseline = original
            .map(|row| AddAppSnapshot {
                label: row.label,
                description: row.desc,
                path: row.action,
                show_args: row.args.is_some(),
                args: row.args.unwrap_or_default(),
            })
            .unwrap_or(AddAppSnapshot {
                label: String::new(),
                description: String::new(),
                path: String::new(),
                show_args: false,
                args: String::new(),
            });
        let label = AppDraftField::new(&baseline.label, window, cx);
        let description = AppDraftField::new(&baseline.description, window, cx);
        let path = AppDraftField::new(&baseline.path, window, cx);
        let args = AppDraftField::new(&baseline.args, window, cx);
        let scroll = ScrollHandle::new();
        let draft_id = label.entity_id();
        let handles = [
            label.focus_handle(cx),
            description.focus_handle(cx),
            path.focus_handle(cx),
            self.app_editor.arguments_focus.clone(),
            args.focus_handle(cx),
        ];
        let mut subscriptions: Vec<Subscription> = handles
            .iter()
            .enumerate()
            .map(|(index, handle)| {
                cx.on_focus(handle, window, move |root, window, cx| {
                    if window.is_window_active() && root.app_editor.is_current_draft(draft_id) {
                        root.root_scroll
                            .scroll_to_top_of_item(root.app_editor_scroll_item());
                        if let Some(draft) = &root.app_editor.draft {
                            draft.scroll.scroll_to_item(index);
                            cx.notify();
                        }
                    }
                })
            })
            .collect();
        subscriptions.push(cx.on_focus(&self.app_editor.browse_focus, window, move |root, window, cx| {
            if window.is_window_active() && root.app_editor.is_current_draft(draft_id) {
                root.root_scroll.scroll_to_top_of_item(root.app_editor_scroll_item());
                if let Some(draft) = &root.app_editor.draft { draft.scroll.scroll_to_item(2); }
                cx.notify();
            }
        }));
        let mut last_viewport = window.viewport_size();
        subscriptions.push(cx.observe_window_bounds(window, move |root, window, cx| {
            let viewport = window.viewport_size();
            if viewport == last_viewport {
                return;
            }
            last_viewport = viewport;
            if !window.is_window_active()
                || !root.app_editor.is_current_draft(draft_id)
                || !root.app_editor.owner_focus.contains_focused(window, cx)
            {
                return;
            }
            root.root_scroll
                .scroll_to_top_of_item(root.app_editor_scroll_item());
            if root.app_editor.browse_focus.is_focused(window) {
                if let Some(draft) = &root.app_editor.draft { draft.scroll.scroll_to_item(2); }
            } else if let Some(index) = handles.iter().position(|handle| handle.is_focused(window)) {
                if let Some(draft) = &root.app_editor.draft {
                    draft.scroll.scroll_to_item(index);
                }
            }
            cx.notify();
        }));
        self.app_editor.draft = Some(AddAppDraft {
            mode,
            show_args: baseline.show_args,
            baseline,
            label,
            description,
            path,
            args,
            scroll,
            _focus_subscriptions: subscriptions,
        });
        if focus && let Some(draft) = &self.app_editor.draft {
            draft.label.focus_handle(cx).focus(window, cx);
        }
        self.app_editor.status = None;
        cx.notify();
    }

    fn browse_app_path(&mut self, draft_id: EntityId, path_id: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        if !self.app_editor.is_current_draft(draft_id) || self.restart_admission.is_fenced()
            || !self.app_editor.draft.as_ref().is_some_and(|draft| draft.path.entity_id() == path_id)
            || !window.is_window_active()
            || window.has_active_dialog(cx) || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            || self.root_window.is_none_or(|host| host.window_id() != window.window_handle().window_id()) { return; }
        let Ok(Some(native)) = checked_active_dialog_owner(window) else { return; };
        let revision = self.root_visibility.request_revision.load(Ordering::Acquire);
        if self.native_window != Some(native) || current_foreground_identity() != Some(native.identity)
            || !json_utility::native_host_current(&self.root_visibility, revision, Some(native), false) { return; }
        self.app_editor.browse_focus.focus(window, cx);
        if !self.app_editor.browse_focus.is_focused(window) { return; }
        let Some(permit) = self.restart_admission.admit() else { return; };
        let Some(attempt) = self.app_editor.picker.begin() else { return; };
        let lease = Arc::new(AtomicBool::new(true));
        let blur_lease = lease.clone();
        self.app_editor.picker_guard = Some(cx.on_blur(&self.app_editor.browse_focus, window, move |_, _, _| {
            blur_lease.store(false, Ordering::Release);
        }));
        self.app_editor.picker_lease = Some(lease.clone());
        let picker = rfd::FileDialog::new().set_parent(&*window);
        let visibility = self.root_visibility.clone(); let admission = self.restart_admission.clone();
        let task = cx.background_executor().spawn(async move {
            if !lease.load(Ordering::Acquire) || admission.is_fenced() || current_foreground_identity() != Some(native.identity)
                || !json_utility::native_host_current(&visibility, revision, Some(native), false) { return None; }
            picker.pick_file()
        });
        let root = cx.entity().downgrade(); let host = window.window_handle();
        cx.spawn(async move |_, cx| {
            let selected = task.await;
            let mut result = Some(selected);
            let updated = host.update(cx, |_, window, app| { let _ = root.update(app, |root, cx| {
                if !root.app_editor.picker.finish(attempt) { return; }
                root.app_editor.picker_lease = None; root.app_editor.picker_guard = None;
                #[cfg(windows)]
                if result.as_ref().is_some_and(Option::is_none) { root.pane_escape_latched = true; }
                let current_draft = root.app_editor.draft.as_ref().is_some_and(|draft| draft.label.entity_id() == draft_id && draft.path.entity_id() == path_id);
                let current_host = !root.restart_admission.is_fenced()
                    && root.root_window.is_some_and(|live| live.window_id() == host.window_id())
                    && root.native_window == Some(native) && current_foreground_identity() == Some(native.identity)
                    && json_utility::native_host_current(&root.root_visibility, revision, Some(native), false)
                    && !window.has_active_dialog(cx)
                    && root.app_editor.browse_focus.is_focused(window)
                    && root.external_owner_is_current(window.focused(cx).as_ref(), window, cx);
                if picker_may_apply(root.app_editor.open, current_draft, current_host, result.as_ref().is_some_and(Option::is_some))
                    && let Some(path) = result.take().flatten() {
                    if let Some(value) = selected_path_text(&path) {
                        if let Some(draft) = &root.app_editor.draft {
                            match &draft.path {
                                AppDraftField::Single(input) => input.update(cx, |input, cx| input.set_value(value.to_owned(), window, cx)),
                                AppDraftField::Multiline(input) => input.update(cx, |input, cx| input.set_value(value.to_owned(), window, cx)),
                            }
                            root.app_editor.status = None;
                        }
                    } else { root.app_editor.status = Some("The selected path cannot be represented in this editor.".into()); }
                }
                if root.restart_status.as_deref() == Some(APP_PICKER_STATUS) { root.restart_status = None; }
                cx.notify();
            }); });
            if updated.is_err() { let _ = root.update(cx, |root, cx| {
                if root.app_editor.picker.finish(attempt) {
                    root.app_editor.picker_lease = None; root.app_editor.picker_guard = None;
                    if root.restart_status.as_deref() == Some(APP_PICKER_STATUS) { root.restart_status = None; }
                    cx.notify();
                }
            }); }
            drop(permit);
        }).detach();
        cx.notify();
    }

    fn submit_add_app(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.app_editor.write_pending()
            || (self.app_editor.catalog_stale
                && matches!(
                    self.app_editor.draft.as_ref().map(|draft| &draft.mode),
                    Some(DraftMode::Edit(_))
                ))
        {
            return;
        }
        let Some(snapshot) = self.app_editor.snapshot(cx) else {
            return;
        };
        let Some(mode) = self
            .app_editor
            .draft
            .as_ref()
            .map(|draft| draft.mode.clone())
        else {
            return;
        };
        let is_add = matches!(&mode, DraftMode::Add);
        self.run_app_write(
            is_add,
            move || match mode {
                DraftMode::Add => multi_launcher::actions::AddAction::new(
                    &snapshot.label,
                    &snapshot.description,
                    &snapshot.path,
                    snapshot.show_args,
                    &snapshot.args,
                )
                .map_err(|error| AppWriteError::Message(error.to_owned()))
                .and_then(|action| {
                    action.commit(ACTIONS_FILE).map_err(|error| {
                        AppWriteError::Message(format!("Could not save app: {error:#}"))
                    })
                }),
                DraftMode::Edit(target) => multi_launcher::actions::EditAction::new(
                    target,
                    &snapshot.label,
                    &snapshot.description,
                    &snapshot.path,
                    snapshot.show_args,
                    &snapshot.args,
                )
                .map_err(|error| AppWriteError::Message(error.to_owned()))
                .and_then(|action| action.commit(ACTIONS_FILE).map_err(checked_action_error)),
            },
            window,
            cx,
        );
    }

    fn remove_app(
        &mut self,
        selection: CatalogSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.app_editor.open
            || self.app_editor.write_pending()
            || self.app_editor.draft.is_some()
        {
            return;
        }
        let Some(target) = self.app_editor.target(&selection) else {
            self.app_editor.status =
                Some("App selection is stale. Refresh and select it again.".into());
            cx.notify();
            return;
        };
        self.run_app_write(
            false,
            move || {
                multi_launcher::actions::RemoveAction::new(target)
                    .commit(ACTIONS_FILE)
                    .map_err(checked_action_error)
            },
            window,
            cx,
        );
    }

    fn run_app_write(
        &mut self,
        return_to_new: bool,
        write: impl FnOnce() -> Result<Vec<CatalogAction>, AppWriteError> + Send + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(operation) = self.restart_admission.admit() else {
            return;
        };
        let Some(receipt) = self.app_editor.begin_write() else {
            return;
        };
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            write().map(CatalogSnapshot::from_rows)
        });
        let root = cx.entity().downgrade();
        let window_handle = window.window_handle();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = window_handle.update(cx, |_, window, app| {
                let _ = root.update(app, |root, cx| {
                    let restore = result.is_ok()
                        && root.app_editor.open
                        && window.is_window_active()
                        && root.app_editor.owner_focus.contains_focused(window, cx)
                        && !root.restart_admission.is_fenced();
                    let failed = result.is_err();
                    let verified_folders = result.as_ref().ok().map(|snapshot| snapshot.verified_folders.clone());
                    if root.app_editor.finish_write(receipt, result) {
                        if let Some(verified_folders) = verified_folders {
                            root.publish_app_editor_search_actions(verified_folders, window, cx);
                        }
                        if failed && root.app_editor.open {
                            if let Some(draft) = &root.app_editor.draft {
                                draft.scroll.scroll_to_item(5);
                            }
                        }
                        clear_app_diagnostic(&mut root.restart_status);
                        if restore {
                            if return_to_new {
                                root.app_editor.new_focus.focus(window, cx);
                            } else if let Some(filter) = &root.app_editor.filter {
                                filter.focus_handle(cx).focus(window, cx);
                            }
                        }
                        cx.notify();
                    }
                });
            });
            drop(operation);
        })
        .detach();
    }

    fn cancel_add_app(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.app_editor.write_pending() {
            return;
        }
        let Some(draft) = self.app_editor.draft.take() else {
            return;
        };
        self.app_editor.status = if self.app_editor.catalog_stale {
            Some("App selection is stale. Refresh and select it again.".into())
        } else {
            None
        };
        clear_app_diagnostic(&mut self.restart_status);
        if self.app_editor.owner_focus.contains_focused(window, cx)
            && !self.restart_admission.is_fenced()
        {
            if matches!(draft.mode, DraftMode::Add) {
                self.app_editor.new_focus.focus(window, cx);
            } else if let Some(filter) = &self.app_editor.filter {
                filter.focus_handle(cx).focus(window, cx);
            }
        }
        cx.notify();
    }

    fn close_app_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.app_editor.open
            || self.app_editor.write_pending()
            || self.app_editor.draft.is_some()
        {
            return;
        }
        let restore = self.app_editor.owner_focus.contains_focused(window, cx)
            && !self.restart_admission.is_fenced();
        self.app_editor.open = false;
        self.app_editor.reads.invalidate();
        self.app_editor.draft = None;
        if !self.app_editor.catalog_stale {
            self.app_editor.status = None;
        }
        let origin = self.app_editor.return_focus.take();
        let offset = self.app_editor.return_scroll.take();
        if restore {
            if let Some(origin) = origin.filter(|focus| self.focus_handle.contains(focus, window)) {
                if let Some(offset) = offset {
                    self.root_scroll.set_offset(offset);
                }
                origin.focus(window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn app_editor_escape(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.app_editor.open
            || !quick_help_escape_capture_should_close(
                &window.context_stack(),
                self.app_editor.owner_focus.contains_focused(window, cx),
            )
        {
            return false;
        }
        if self.app_editor.draft.is_some() {
            self.cancel_add_app(window, cx);
        } else {
            self.close_app_editor(window, cx);
        }
        true
    }
}

#[derive(IntoElement)]
pub(super) struct AppEditorPanel {
    root: WeakEntity<RootWindow>,
    width: f32,
    viewport_height: f32,
    metrics: multi_launcher::gui::theme::ThemeMetrics,
    palette: multi_launcher::gui::theme::SemanticPalette,
    owner_focus: FocusHandle,
    new_focus: FocusHandle,
    arguments_focus: FocusHandle,
    browse_focus: FocusHandle,
    draft: Option<AddAppPanelDraft>,
    filter: Option<Entity<InputState>>,
    catalog: Arc<[CatalogAction]>,
    catalog_revision: u64,
    catalog_error: Option<String>,
    catalog_loading: bool,
    catalog_stale: bool,
    catalog_list: ListState,
    saved_group_focus: FocusHandle,
    indexed_actions: Arc<Vec<CatalogAction>>,
    indexed_generation: u64,
    indexed_matches: Arc<Vec<usize>>,
    status: Option<String>,
    pending: bool,
}

struct AddAppPanelDraft {
    label: AppDraftField,
    description: AppDraftField,
    path: AppDraftField,
    args: AppDraftField,
    show_args: bool,
    editing: bool,
    scroll: ScrollHandle,
}

impl AppEditorPanel {
    pub(super) fn new(
        root: WeakEntity<RootWindow>,
        state: &AppEditorState,
        width: f32,
        viewport_height: f32,
        metrics: multi_launcher::gui::theme::ThemeMetrics,
        palette: multi_launcher::gui::theme::SemanticPalette,
    ) -> Self {
        Self {
            root,
            width,
            viewport_height,
            metrics,
            palette,
            owner_focus: state.owner_focus.clone(),
            new_focus: state.new_focus.clone(),
            arguments_focus: state.arguments_focus.clone(),
            browse_focus: state.browse_focus.clone(),
            status: state.status.clone(),
            filter: state.filter.clone(),
            catalog: state.catalog.clone(),
            catalog_revision: state.catalog_revision,
            catalog_error: state.catalog_error.clone(),
            catalog_loading: state.reads.pending.is_some(),
            catalog_stale: state.catalog_stale,
            catalog_list: state.catalog_list.clone(),
            saved_group_focus: state.saved_group_focus.clone(),
            indexed_actions: state.indexed_actions.clone(),
            indexed_generation: state.indexed_generation,
            indexed_matches: state.indexed_matches.clone(),
            pending: state.write_pending(),
            draft: state.draft.as_ref().map(|draft| AddAppPanelDraft {
                label: draft.label.clone(),
                description: draft.description.clone(),
                path: draft.path.clone(),
                args: draft.args.clone(),
                show_args: draft.show_args,
                editing: matches!(&draft.mode, DraftMode::Edit(_)),
                scroll: draft.scroll.clone(),
            }),
        }
    }
}

fn render_saved_catalog_group(
    catalog: &[CatalogAction],
    filter: &str,
    revision: u64,
    root: &WeakEntity<RootWindow>,
    focus: &FocusHandle,
    pending: bool,
    stale: bool,
    loading: bool,
    error: Option<&str>,
    metrics: multi_launcher::gui::theme::ThemeMetrics,
) -> AnyElement {
    let mut group = div().id("app-catalog-saved-group").track_focus(focus)
        .role(Role::Group).aria_label("Saved apps")
        .flex().flex_col().gap_y(px(metrics.item_spacing_y)).w_full().min_w(px(0.0)).min_h(px(1.0));
    if loading { group = group.child(Label::new("Loading apps…")); }
    if let Some(error) = error { group = group.child(Label::new(error.to_owned())); }
    for (index, row) in catalog.iter().enumerate().filter(|(_, row)| catalog_matches(row, filter)) {
        let selection = CatalogSelection { revision, index, original: row.clone() };
        let edit_selection = selection.clone();
        let edit_root = root.clone();
        let remove_root = root.clone();
        let mut item = div().id(("app-catalog-row", index)).flex().flex_col().w_full().min_w(px(0.0))
            .child(Label::new(row.label.clone()))
            .child(Label::new(row.desc.clone()))
            .child(Label::new(row.action.clone()));
        if let Some(args) = &row.args {
            item = item.child(Label::new("Arguments")).child(Label::new(args.clone()));
        }
        group = group.child(item.child(
            div().flex().flex_row().gap_x(px(metrics.item_spacing_x))
                .child(Button::new(("app-catalog-edit", index)).label("Edit")
                    .disabled(pending || stale)
                    .on_click(move |_, window, app| {
                        let selection = edit_selection.clone();
                        let _ = edit_root.update(app, |root, cx| root.edit_app(selection, window, cx));
                    }))
                .child(Button::new(("app-catalog-remove", index)).label("Remove")
                    .disabled(pending || stale)
                    .on_click(move |_, window, app| {
                        let selection = selection.clone();
                        let _ = remove_root.update(app, |root, cx| root.remove_app(selection, window, cx));
                    })),
        ));
    }
    group.into_any_element()
}

fn render_indexed_catalog_row(
    action: &CatalogAction,
    generation: u64,
    ordinal: usize,
    metrics: multi_launcher::gui::theme::ThemeMetrics,
) -> AnyElement {
    let id = format!("app-indexed-row-{generation}-{ordinal}");
    let mut row = div().id(id).flex().flex_col().w_full().min_w(px(0.0))
        .pb(px(metrics.item_spacing_y))
        .child(Label::new(format!("Label: {}", action.label)).w_full().min_w(px(0.0)).whitespace_normal())
        .child(Label::new(format!("Description: {}", action.desc)).w_full().min_w(px(0.0)).whitespace_normal())
        .child(Label::new(format!("Path: {}", action.action)).w_full().min_w(px(0.0)).whitespace_normal());
    if let Some(args) = &action.args {
        row = row.child(Label::new(format!("Args: {args}")).w_full().min_w(px(0.0)).whitespace_normal());
    }
    row.into_any_element()
}

impl RenderOnce for AppEditorPanel {
    fn render(self, window: &mut Window, app: &mut App) -> impl IntoElement {
        let metrics = self.metrics;
        let mut panel = div()
            .id("app-editor")
            .track_focus(&self.owner_focus)
            .key_context("AppEditor")
            .flex()
            .flex_col()
            .gap_y(px(metrics.item_spacing_y))
            .w(px(self.width))
            .max_w_full()
            .p(px(metrics.panel_padding))
            .bg(appearance::to_hsla(self.palette.panel_surface))
            .rounded(px(metrics.corner_radius))
            .text_color(appearance::to_hsla(self.palette.primary_text))
            .text_size(px(metrics.body_text_size))
            .line_height(px(metrics.line_height))
            .child(Label::new("App Editor").text_size(px(metrics.heading_text_size)));
        if let Some(draft) = self.draft {
            let draft_id = draft.label.entity_id();
            let checkbox_root = self.root.clone();
            let args_state = draft.args.clone();
            let arguments_focus = self.arguments_focus.clone();
            let checkbox = Checkbox::new("add-app-arguments")
                .checked(draft.show_args)
                .disabled(self.pending)
                .track_focus(&self.arguments_focus)
                .accessibility_label("Add arguments")
                .on_change(move |state, _, window, app| {
                    let _ = checkbox_root.update(app, |root, cx| {
                        if !root.app_editor.is_current_draft(draft_id) {
                            return;
                        }
                        if let Some(draft) = root.app_editor.draft.as_mut() {
                            let enabled = state == CheckboxState::Checked;
                            if !enabled && args_state.focus_handle(cx).is_focused(window) {
                                arguments_focus.focus(window, cx);
                            }
                            draft.show_args = enabled;
                            cx.notify();
                        }
                    });
                })
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_x(px(metrics.item_spacing_x))
                        .child(
                            CheckboxIndicator::new()
                                .checked(draft.show_args)
                                .size(px(18.0))
                                .rounded(px(3.0))
                                .border_1()
                                .border_color(appearance::to_hsla(self.palette.control_border))
                                .when(draft.show_args, |indicator| {
                                    indicator.bg(appearance::to_hsla(self.palette.accent))
                                })
                                .items_center()
                                .justify_center()
                                .child(if draft.show_args { "✓" } else { "" }),
                        )
                        .child(Label::new("Add arguments")),
                );
            let cancel_root = self.root.clone();
            let submit_root = self.root.clone();
            let browse_root = self.root.clone();
            let path_id = draft.path.entity_id();
            let tab_root = self.root.clone();
            panel = panel.capture_key_down(move |event: &KeyDownEvent, window, app| {
                if event.keystroke.key != "tab" || !window.is_window_active() {
                    return;
                }
                let _ = tab_root.update(app, |root, cx| {
                    if root.app_editor.is_current_draft(draft_id)
                        && root.app_editor.owner_focus.contains_focused(window, cx)
                    {
                        root.root_scroll
                            .scroll_to_top_of_item(root.app_editor_scroll_item());
                        cx.notify();
                    }
                });
            });
            let reserved_height = 2.0 * metrics.window_padding
                + 2.0 * metrics.panel_padding
                + metrics.heading_text_size.max(metrics.line_height)
                + metrics.line_height
                + metrics.row_min_height
                + 2.0 * metrics.button_padding_y
                + 4.0 * metrics.item_spacing_y;
            let body_height = (window.viewport_size().height.as_f32() - reserved_height).max(40.0);
            let body = div()
                .id("add-app-body")
                .flex()
                .flex_col()
                .gap_y(px(metrics.item_spacing_y))
                .max_h(px(body_height))
                .flex_shrink_0()
                .overflow_y_scroll()
                .track_scroll(&draft.scroll)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .child(Label::new("Label"))
                        .child(draft.label.render("Label", self.pending)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .child(Label::new("Description"))
                        .child(draft.description.render("Description", self.pending)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .child(Label::new("Path"))
                        .child(draft.path.render("Path", self.pending))
                        .child(Button::new("add-app-browse").label("Browse…").track_focus(&self.browse_focus)
                            .disabled(self.pending).on_click(move |_, window, app| {
                                let _ = browse_root.update(app, |root, cx| root.browse_app_path(draft_id, path_id, window, cx));
                            })),
                )
                .child(div().flex_shrink_0().child(checkbox))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .when(draft.show_args, |row| {
                            row.child(Label::new("Args"))
                                .child(draft.args.render("Args", self.pending))
                        }),
                )
                .child(div().flex_shrink_0().children(self.status.map(Label::new)));
            panel = panel
                .child(Label::new(if draft.editing {
                    "Edit App"
                } else {
                    "Add App"
                }))
                .child(body)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_x(px(metrics.item_spacing_x))
                        .child(
                            Button::new("add-app-submit")
                                .label(if draft.editing { "Save" } else { "Add" })
                                .disabled(self.pending || (draft.editing && self.catalog_stale))
                                .on_click(move |_, window, app| {
                                    let _ = submit_root.update(app, |root, cx| {
                                        if root.app_editor.is_current_draft(draft_id) {
                                            root.submit_add_app(window, cx);
                                        }
                                    });
                                }),
                        )
                        .child(
                            Button::new("add-app-cancel")
                                .label("Cancel")
                                .disabled(self.pending)
                                .on_click(move |_, window, app| {
                                    let _ = cancel_root.update(app, |root, cx| {
                                        if root.app_editor.is_current_draft(draft_id) {
                                            root.cancel_add_app(window, cx);
                                        }
                                    });
                                }),
                        ),
                );
        } else {
            let new_root = self.root.clone();
            let close_root = self.root.clone();
            let refresh_root = self.root.clone();
            let filter_text = self
                .filter
                .as_ref()
                .map(|filter| catalog_filter(filter.read(app).value().as_str()))
                .unwrap_or_default();
            let catalog = self.catalog.clone();
            let indexed_actions = self.indexed_actions.clone();
            let indexed_matches = self.indexed_matches.clone();
            let indexed_generation = self.indexed_generation;
            let revision = self.catalog_revision;
            let row_root = self.root.clone();
            let saved_group_focus = self.saved_group_focus.clone();
            let pending = self.pending;
            let stale = self.catalog_stale;
            let loading = self.catalog_loading;
            let catalog_error = self.catalog_error.clone();
            let catalog_list = list(self.catalog_list.clone(), move |slot, _, _| {
                if slot == 0 {
                    return render_saved_catalog_group(
                        &catalog, &filter_text, revision, &row_root, &saved_group_focus, pending, stale,
                        loading, catalog_error.as_deref(), metrics,
                    );
                }
                let Some(&ordinal) = indexed_matches.get(slot - 1) else { return div().into_any_element(); };
                let Some(action) = indexed_actions.get(ordinal) else { return div().into_any_element(); };
                render_indexed_catalog_row(action, indexed_generation, ordinal, metrics)
            });
            let viewport_height = if self.viewport_height > 0.0 {
                self.viewport_height
            } else {
                window.viewport_size().height.as_f32()
            };
            let reserved_height = 2.0 * metrics.panel_padding
                + metrics.heading_text_size.max(metrics.line_height)
                + 4.0 * metrics.row_min_height
                + 7.0 * metrics.item_spacing_y
                + if self.status.is_some() { metrics.line_height + metrics.item_spacing_y } else { 0.0 };
            let body_height = (viewport_height - reserved_height).max(40.0);
            panel = panel
                .children(
                    self.filter
                        .as_ref()
                        .map(|filter| Input::new(filter).aria_label("Filter apps")),
                )
                .child(
                    div()
                        .id("app-catalog-list")
                        .w_full()
                        .h(px(body_height))
                        .flex_shrink_0()
                        .on_scroll(|_, _, cx| cx.stop_propagation())
                        .child(catalog_list.size_full()),
                )
                .children(self.status.map(Label::new))
                .child(
                    Button::new("app-editor-refresh")
                        .label("Refresh")
                        .disabled(self.pending)
                        .on_click(move |_, window, app| {
                            let _ = refresh_root
                                .update(app, |root, cx| root.refresh_app_catalog(window, cx));
                        }),
                )
                .child(
                    Button::new("app-editor-new")
                        .label("New App")
                        .disabled(self.pending)
                        .track_focus(&self.new_focus)
                        .on_click(move |_, window, app| {
                            let _ = new_root.update(app, |root, cx| root.new_app_draft(window, cx));
                        }),
                )
                .child(
                    Button::new("app-editor-close")
                        .label("Close")
                        .disabled(self.pending)
                        .on_click(move |_, window, app| {
                            let _ = close_root
                                .update(app, |root, cx| root.close_app_editor(window, cx));
                        }),
                );
        }
        panel
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AddAppWrites, CatalogAction, CatalogReads, CatalogSelection, catalog_filter,
        catalog_matches,
    };

    #[test]
    fn add_app_write_accepts_only_live_receipt_once() {
        let mut writes = AddAppWrites::default();
        let first = writes.begin().unwrap();
        assert!(writes.begin().is_none());
        assert!(!writes.finish(first.wrapping_add(1)));
        assert_eq!(writes.pending, Some(first));
        assert!(writes.finish(first));
        assert!(!writes.finish(first));
        let second = writes.begin().unwrap();
        assert_ne!(first, second);
        assert!(!writes.finish(first));
        assert_eq!(writes.pending, Some(second));
        assert!(writes.finish(second));
    }

    #[test]
    fn catalog_filter_matches_egui_literal_rules() {
        let row = CatalogAction {
            label: "My App".into(),
            desc: "A Description".into(),
            action: "C:/Tools/Run.exe".into(),
            args: Some("secret args".into()),
        };
        let matches = |filter| catalog_matches(&row, &catalog_filter(filter));
        assert!(matches("  "));
        assert!(matches("APP"));
        assert!(matches("description"));
        assert!(matches("TOOLS"));
        assert!(!matches(" app "));
        assert!(!matches("secret"));
    }

    #[test]
    fn browse_path_preserves_unicode_and_rejects_unrepresentable_values() {
        use std::path::Path;
        assert_eq!(super::selected_path_text(Path::new("C:\\音楽\\café.exe")), Some("C:\\音楽\\café.exe"));
        assert_eq!(super::selected_path_text(Path::new("bad\npath")), None);
        #[cfg(windows)] {
            use std::os::windows::ffi::OsStringExt;
            let invalid = std::ffi::OsString::from_wide(&[0xd800]);
            assert_eq!(super::selected_path_text(Path::new(&invalid)), None);
        }
    }

    #[test]
    fn browse_receipt_rejects_cancel_stale_draft_and_hidden_host() {
        assert!(super::picker_may_apply(true, true, true, true));
        assert!(!super::picker_may_apply(true, true, true, false));
        assert!(!super::picker_may_apply(true, false, true, true));
        assert!(!super::picker_may_apply(true, true, false, true));
        assert!(!super::picker_may_apply(false, true, true, true));
        let mut receipts = super::AddAppWrites::default();
        let attempt = receipts.begin().unwrap();
        assert!(receipts.begin().is_none());
        assert!(receipts.finish(attempt));
        assert!(!receipts.finish(attempt));
    }

    #[test]
    fn catalog_saved_remeasurement_and_index_splice_preserve_scroll_anchor() {
        let list = gpui_kit::ListState::new(1, gpui_kit::ListAlignment::Top, gpui_kit::px(800.0));
        let anchor = gpui_kit::ListOffset { item_ix: 0, offset_in_item: gpui_kit::px(35.0) };
        list.scroll_to(anchor);
        list.remeasure_items(0..1);
        list.splice(1..1, 10);
        assert_eq!(list.logical_scroll_top(), anchor);
        assert_eq!(list.item_count(), 11);
    }

    #[test]
    fn indexed_filter_preserves_original_ordinals_and_excludes_args() {
        let rows = vec![
            CatalogAction { label: "Notes".into(), desc: "first".into(), action: "C:/one.md".into(), args: Some("secret".into()) },
            CatalogAction { label: "Notes".into(), desc: "second".into(), action: "C:/two.txt".into(), args: None },
            CatalogAction { label: "Other".into(), desc: "notes".into(), action: "C:/three.txt".into(), args: None },
        ];
        assert_eq!(super::indexed_match_ordinals(&rows, &catalog_filter("  ")), [0, 1, 2]);
        assert_eq!(super::indexed_match_ordinals(&rows, &catalog_filter("NOTES")), [0, 1, 2]);
        assert_eq!(super::indexed_match_ordinals(&rows, &catalog_filter("TWO.TXT")), [1]);
        assert!(super::indexed_match_ordinals(&rows, &catalog_filter(" notes ")).is_empty());
        assert!(super::indexed_match_ordinals(&rows, &catalog_filter("secret")).is_empty());
    }

    #[test]
    fn selection_requires_published_revision_and_preserves_duplicate_identity() {
        let row = CatalogAction {
            label: "same".into(),
            desc: "desc".into(),
            action: "path".into(),
            args: Some(" exact ".into()),
        };
        let rows = vec![row.clone(), row.clone()];
        let selection = CatalogSelection {
            revision: 7,
            index: 1,
            original: row,
        };
        assert!(selection.target(&rows, 8, false).is_none());
        assert!(selection.target(&rows, 7, true).is_none());
        let target = selection.target(&rows, 7, false).unwrap();
        let mut changed = rows.clone();
        changed.remove(0);
        assert!(
            multi_launcher::actions::RemoveAction::new(target.clone())
                .apply_to(&mut changed)
                .is_err()
        );
        let mut unchanged = rows;
        multi_launcher::actions::RemoveAction::new(target)
            .apply_to(&mut unchanged)
            .unwrap();
        assert_eq!(unchanged.len(), 1);
    }

    #[test]
    fn catalog_reads_reject_superseded_and_retired_receipts() {
        let mut reads = CatalogReads::default();
        let first = reads.begin();
        let second = reads.begin();
        assert!(!reads.finish(first));
        assert!(reads.finish(second));
        assert!(!reads.finish(second));
        let before_close_or_commit = reads.begin();
        reads.invalidate();
        let reopened = reads.begin();
        assert!(!reads.finish(before_close_or_commit));
        assert!(reads.finish(reopened));
    }
}
