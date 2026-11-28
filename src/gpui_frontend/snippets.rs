//! Retained saved Snippets browser backed by the canonical snippet store.
use super::*;
use gpui_kit::base::Selectable;
use multi_launcher::common::persistence::LoadState;
use multi_launcher::plugins::snippets::{self, SnippetEntry};
mod host;
pub(super) const SAVE_WAIT: &str = "Finish or cancel the Snippets review, or wait for its save.";

#[derive(Default)]
pub(super) struct SearchState {
    pub pending: bool,
    pub wake: Arc<HashWakeSignal>,
    pub projection: Option<(String, String, snippets::SnippetSearchSnapshot)>,
    pub published: Option<(String, String, u64, snippets::SnippetSearchSnapshot)>,
    pub request_epoch: Arc<AtomicU64>,
}

pub(super) fn search_snapshot_current(owner: &snippets::SnippetSearchOwner, expected: &snippets::SnippetSearchSnapshot) -> bool {
    let current = owner.snapshot();
    current.healthy && current.generation == expected.generation && Arc::ptr_eq(&current.entries, &expected.entries)
}
pub(super) fn named_row_is_current(
    _query: &str, action: &Action, snapshot: &snippets::SnippetSearchSnapshot,
    owner: Option<&snippets::SnippetSearchOwner>, enabled: Option<&HashSet<String>>,
    caps: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    enabled.is_none_or(|enabled| enabled.contains("snippets"))
        && multi_launcher::settings::provider_search_capability_enabled(caps, "snippets")
        && owner.is_some_and(|owner| search_snapshot_current(owner, snapshot))
        && matches!(parse_action(action), Ok(Command::Storage(multi_launcher::commands::StorageCommand::SnippetEdit(_) | multi_launcher::commands::StorageCommand::SnippetRun(_))))
}

fn edit_is_current(edit: &EditDraft, generation: u64, rows: &Arc<Vec<SnippetEntry>>, instance: u64) -> bool {
    edit.instance == instance && edit.generation == generation && Arc::ptr_eq(&edit.snapshot, rows)
}
fn preview_return_is_current(issued: &utility_mount::MountReceipt, current: &utility_mount::MountReceipt,
    active: bool, open: bool, issued_instance: u64, current_instance: Option<u64>, same_host: bool) -> bool {
    issued.is_current() && issued.matches(current) && active && open && current_instance == Some(issued_instance) && same_host
}
fn preview_child_is_current<T: PartialEq>(installed: Option<T>, returning: T) -> bool {
    installed == Some(returning)
}
fn edited_alias(original: &str, initial_display: &str, current_display: &str) -> String {
    if initial_display == current_display { original.to_owned() } else { current_display.to_owned() }
}
fn rehost_allowed(open: bool, rehosting: bool, same_mode: bool, busy: bool) -> bool {
    open && !rehosting && !same_mode && !busy
}
pub(super) fn exact_named_index(rows: &[SnippetEntry], expected: &[SnippetEntry], alias: &str) -> Option<usize> {
    if rows != expected { return None; }
    let mut matches = rows.iter().enumerate().filter(|(_, row)| row.alias == alias);
    let (index, _) = matches.next()?;
    matches.next().is_none().then_some(index)
}

struct EditDraft {
    snapshot: Arc<Vec<SnippetEntry>>,
    generation: u64,
    kind: DraftKind,
    instance: u64,
    alias: Entity<InputState>,
    alias_initial_display: String,
    body: Option<Entity<TextareaState>>,
    body_private: String,
    tags: Vec<String>,
    tag_input: Entity<InputState>,
    hidden: bool,
    revealed: bool,
    prompt_for_fields: bool,
    fields: Vec<AuthorField>,
    body_sub: Option<Subscription>,
}

struct AuthorField {
    definition: snippets::SnippetFieldDefinition,
    label: Option<Entity<InputState>>,
    label_initial_display: String,
    default_value: Option<Entity<TextareaState>>,
}

impl AuthorField {
    fn current(&self, cx: &App) -> snippets::SnippetFieldDefinition {
        let mut definition = self.definition.clone();
        if let Some(label) = &self.label {
            definition.label = edited_alias(&definition.label, &self.label_initial_display, label.read(cx).value().as_str());
        }
        if let Some(default) = &self.default_value { definition.default_value = default.read(cx).value().to_string(); }
        definition
    }
    fn detach(&mut self, cx: &App) {
        self.definition = self.current(cx);
        self.label = None;
        self.default_value = None;
    }
    fn attach(&mut self, window: &mut Window, cx: &mut Context<SnippetsView>) {
        if self.label.is_some() { return; }
        let label = cx.new(|cx| InputState::new(window, cx).placeholder("Field label"));
        label.update(cx, |input, cx| input.set_value(self.definition.label.clone(), window, cx));
        self.label_initial_display = label.read(cx).value().to_string();
        let default = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 4).placeholder("Default value"));
        default.update(cx, |input, cx| input.set_value(self.definition.default_value.clone(), window, cx));
        self.label = Some(label);
        self.default_value = Some(default);
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DraftKind { New, Existing(usize) }
#[derive(Clone)]
struct RemovalReview {
    snapshot: Arc<Vec<SnippetEntry>>,
    generation: u64,
    index: usize,
    entry: SnippetEntry,
    instance: u64,
}
struct NamedOpen {
    rows: Vec<SnippetEntry>,
    index: usize,
    snapshot: snippets::SnippetSearchSnapshot,
    owner: Arc<snippets::SnippetSearchOwner>,
    query: String,
    generation: u64,
    epoch: u64,
    focus: FocusHandle,
    lease: Arc<AtomicBool>,
    _focus_guard: Subscription,
}
struct ColdOpen {
    focus: FocusHandle,
    lease: Arc<AtomicBool>,
    query: String,
    generation: u64,
    epoch: u64,
    _focus_guard: Subscription,
}
#[derive(Clone, Copy)]
enum Mutation { Save(u64), Remove(u64) }
fn new_snippet() -> SnippetEntry {
    SnippetEntry { alias: String::new(), text: String::new(), hide_contents: false,
        prompt_for_fields: false, fields: Vec::new(), tags: Vec::new() }
}
fn removal_candidate(review: &RemovalReview, rows: &Arc<Vec<SnippetEntry>>, generation: u64, instance: u64) -> Option<Vec<SnippetEntry>> {
    if review.instance != instance || review.generation != generation || !Arc::ptr_eq(&review.snapshot, rows)
        || review.snapshot.get(review.index) != Some(&review.entry) { return None; }
    let mut candidate = review.snapshot.as_ref().clone();
    candidate.remove(review.index);
    Some(candidate)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RestartDraft {
    snapshot: Vec<SnippetEntry>,
    kind: DraftKind,
    alias: String,
    body: String,
    tags: Vec<String>,
    tag_input: String,
    hidden: bool,
    revealed: bool,
    prompt_for_fields: bool,
    fields: Vec<snippets::SnippetFieldDefinition>,
}

pub(super) fn current_provider(
    query: &str,
    action: &Action,
    enabled: Option<&HashSet<String>>,
    caps: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    search_providers::snippets_rows(query, enabled, caps)
        .iter().any(|row| row == action)
        && matches!(parse_action(action), Ok(Command::Storage(multi_launcher::commands::StorageCommand::SnippetDialog)))
}

pub(super) struct SnippetsView {
    pub open: bool,
    pub(super) root: WeakEntity<RootWindow>,
    owner: utility_mount::MountOwner,
    pub(super) mount: utility_mount::MountReceipt,
    pub(super) host: AnyWindowHandle,
    pub(super) native: Option<NativeWindow>,
    pub(super) popout: bool,
    pub(super) rehosting: bool,
    pub(super) admission: Arc<RestartAdmission>,
    pub(super) presentation_error: Option<String>,
    pub(super) geometry_error: Option<String>,
    cold_popout: bool,
    pending_named: Option<NamedOpen>,
    cold_origin: Option<ColdOpen>,
    path: PathBuf,
    focus: FocusHandle,
    save_focus: FocusHandle,
    preview_focus: FocusHandle,
    filter: Entity<InputState>,
    _filter_sub: Subscription,
    selected_tags: Vec<String>,
    rows: Arc<Vec<SnippetEntry>>,
    generation: u64,
    loading: bool,
    diagnostic: Option<String>,
    selected: Option<(u64, usize)>,
    edit: Option<EditDraft>,
    removal: Option<RemovalReview>,
    removal_focus: FocusHandle,
    removal_cancel_focus: FocusHandle,
    removal_confirm_focus: FocusHandle,
    instance: u64,
    pending: bool,
    authority: Arc<AtomicU64>,
    activation: Arc<AtomicU64>,
    activation_source_lease: Option<Arc<AtomicBool>>,
    feedback: Option<String>,
    preview_pending: bool,
    preview_child: Option<Entity<super::snippet_fill::SnippetFill>>,
    preview_scroll_offset: Option<Point<Pixels>>,
    pub(super) outer_scroll: ScrollHandle,
    preview_focus_guard: Option<Subscription>,
    preview_activation_guard: Option<Subscription>,
}

impl SnippetsView {
    pub(super) fn has_pending_named(&self) -> bool { self.pending_named.is_some() }
    pub(super) fn has_cold_origin(&self) -> bool { self.cold_origin.is_some() }
    pub(super) fn revoke_source(&mut self) {
        if let Some(lease) = &self.activation_source_lease { lease.store(false, Ordering::Release); }
        if let Some(named) = &self.pending_named { named.lease.store(false, Ordering::Release); }
        if let Some(cold) = &self.cold_origin { cold.lease.store(false, Ordering::Release); }
    }
    pub(super) fn named_intent_current(&self, root: &RootWindow, cx: &App) -> bool {
        let named_current = self.pending_named.as_ref().is_none_or(|named| {
            root.snippets_search.request_epoch.load(Ordering::Acquire) == named.epoch
                && root.launcher_results.generation == named.generation && root.query.read(cx).value().as_str() == named.query
                && root.enabled_plugins.as_ref().is_none_or(|enabled| enabled.contains("snippets"))
                && multi_launcher::settings::provider_search_capability_enabled(root.enabled_capabilities.as_ref(), "snippets")
                && search_snapshot_current(&named.owner, &named.snapshot)
        });
        named_current && self.cold_origin.as_ref().is_none_or(|cold| {
            root.snippets_search.request_epoch.load(Ordering::Acquire) == cold.epoch
                && root.launcher_results.generation == cold.generation && root.query.read(cx).value().as_str() == cold.query
        })
    }
    pub(super) fn named_source_current(&self, root: &RootWindow, window: &Window, cx: &App, modal: bool) -> bool {
        let source = self.pending_named.as_ref().map(|named| (&named.focus, &named.lease))
            .or_else(|| self.cold_origin.as_ref().map(|cold| (&cold.focus, &cold.lease)));
        source.is_none_or(|(focus, lease)| lease.load(Ordering::Acquire)
            && !modal && window.is_window_active()
            && root.external_owner_is_current(Some(focus), window, cx)
            && root.native_window.is_some_and(|native| current_foreground_identity() == Some(native.identity)))
            && self.named_intent_current(root, cx)
    }
    pub(super) fn mount_receipt(&self) -> utility_mount::MountReceipt { self.mount.clone() }
    pub(super) fn is_mounted_in(&self, id: WindowId, popout: bool) -> bool {
        self.open && self.popout == popout && self.mount.is_current() && self.host.window_id() == id
    }
    pub(super) fn shutdown_host(&mut self, cx: &mut Context<Self>) {
        if let Some(child) = self.preview_child.take() { child.update(cx, |child, cx| child.retire(cx)); }
        self.open = false;
        self.owner.retire();
        self.authority.fetch_add(1, Ordering::AcqRel);
        self.loading = false;
        self.edit = None;
        self.removal = None;
        self.selected = None;
        self.selected_tags.clear();
        self.preview_pending = false;
        self.preview_child = None;
        self.preview_scroll_offset = None;
        self.pending_named = None;
        self.cold_origin = None;
    }
    pub(super) fn contains_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus.is_focused(window) || self.focus.contains_focused(window, cx)
    }
    pub(super) fn focus_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.rehosting { self.focus.focus(window, cx); }
        else if let Some(child) = &self.preview_child {
            child.update(cx, |child, cx| {
                if !child.contains_focus(window, cx) { child.focus_initial(window, cx); }
            });
        }
        else if let Some(edit) = &self.edit { edit.alias.focus_handle(cx).focus(window, cx); }
        else { self.filter.focus_handle(cx).focus(window, cx); }
    }
    pub(super) fn after_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.is_window_active() && host::HostAuthority::capture(self, window, cx).is_some() {
            self.cold_origin = None;
            if let Some(named) = self.pending_named.take() { self.install_named(named, window, cx); }
            else { self.focus_host(window, cx); }
        }
    }
    pub(super) fn set_rehosting(&mut self, value: bool, cx: &mut Context<Self>) {
        self.rehosting = value;
        if !value && !self.popout { self.cold_origin = None; }
        if !value && !self.popout && self.pending_named.take().is_some() {
            self.feedback = Some("Could not open the snippet in its window. Try again.".into());
        }
        self.filter.update(cx, |input, cx| input.set_readonly(value, cx));
        self.set_editor_readonly(value, cx);
    }
    fn install_named(&mut self, named: NamedOpen, window: &mut Window, cx: &mut Context<Self>) {
        let valid = self.root.upgrade().is_some_and(|root| {
            let root = root.read(cx);
            root.snippets_search.request_epoch.load(Ordering::Acquire) == named.epoch
                && root.launcher_results.generation == named.generation && root.query.read(cx).value().as_str() == named.query
                && root.enabled_plugins.as_ref().is_none_or(|enabled| enabled.contains("snippets"))
                && multi_launcher::settings::provider_search_capability_enabled(root.enabled_capabilities.as_ref(), "snippets")
                && search_snapshot_current(&named.owner, &named.snapshot)
        });
        if !valid || self.loading || self.pending || self.edit.is_some() || self.removal.is_some() || self.preview_pending || self.preview_child.is_some() { return; }
        self.rows = Arc::new(named.rows);
        self.generation = self.generation.wrapping_add(1);
        self.diagnostic = None;
        self.selected = Some((self.generation, named.index));
        self.begin_edit(self.generation, named.index, window, cx);
    }
    pub(super) fn replace_mount(&mut self, host: AnyWindowHandle, native: Option<NativeWindow>, window: &mut Window, cx: &mut Context<Self>) {
        let filter = self.filter.read(cx).value().to_string();
        self.mount = self.owner.attach();
        self.host = host;
        self.native = native;
        self.outer_scroll = ScrollHandle::new();
        self.focus = cx.focus_handle();
        self.save_focus = cx.focus_handle();
        self.preview_focus = cx.focus_handle();
        self.removal_focus = cx.focus_handle();
        self.removal_cancel_focus = cx.focus_handle();
        self.removal_confirm_focus = cx.focus_handle();
        self.filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter snippets"));
        self.filter.update(cx, |input, cx| input.set_value(filter, window, cx));
        self._filter_sub = cx.subscribe(&self.filter, |_: &mut Self, _: Entity<InputState>, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) { cx.notify(); }
        });
        if let Some(edit) = &mut self.edit {
            let alias_value = edit.alias.read(cx).value().to_string();
            let tag_value = edit.tag_input.read(cx).value().to_string();
            let body_value = edit.body.as_ref().map(|body| body.read(cx).value().to_string());
            for field in &mut edit.fields { field.detach(cx); }
            edit.alias = cx.new(|cx| InputState::new(window, cx).placeholder("Alias"));
            edit.alias.update(cx, |input, cx| input.set_value(alias_value, window, cx));
            edit.tag_input = cx.new(|cx| InputState::new(window, cx).placeholder("Add tag"));
            edit.tag_input.update(cx, |input, cx| input.set_value(tag_value, window, cx));
            edit.body = body_value.map(|value| {
                let body = cx.new(|cx| TextareaState::new(window, cx).auto_grow(6, 14).placeholder("Snippet text"));
                body.update(cx, |input, cx| input.set_value(value, window, cx));
                body
            });
            edit.body_sub = edit.body.as_ref().map(|body| cx.subscribe(body, |_: &mut Self, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) { cx.notify(); }
            }));
            if edit.revealed && edit.prompt_for_fields { for field in &mut edit.fields { field.attach(window, cx); } }
        }
        cx.notify();
    }
    fn new(window: &mut Window, cx: &mut Context<Self>, root: WeakEntity<RootWindow>, native: Option<NativeWindow>, admission: Arc<RestartAdmission>, path: PathBuf) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter snippets"));
        let subscription = cx.subscribe(&filter, |_: &mut Self, _: Entity<InputState>, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) { cx.notify(); }
        });
        Self { open: false, root, owner, mount, host: window.window_handle(), native, popout: false, rehosting: false, admission, cold_popout: false, pending_named: None, cold_origin: None, presentation_error: None, geometry_error: None, path, focus: cx.focus_handle(), save_focus: cx.focus_handle(), preview_focus: cx.focus_handle(), filter,
            _filter_sub: subscription, selected_tags: Vec::new(), rows: Arc::new(Vec::new()), generation: 0,
            loading: false, diagnostic: None, selected: None, edit: None, removal: None,
            removal_focus: cx.focus_handle(), removal_cancel_focus: cx.focus_handle(), removal_confirm_focus: cx.focus_handle(), instance: 0, pending: false,
            authority: Arc::new(AtomicU64::new(0)), activation: Arc::new(AtomicU64::new(0)), activation_source_lease: None, feedback: None, preview_pending: false, preview_child: None, preview_scroll_offset: None, outer_scroll: ScrollHandle::new(), preview_focus_guard: None, preview_activation_guard: None }
    }
    fn open(&mut self, native: Option<NativeWindow>, focus_on_open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview_pending || self.preview_child.is_some() { return; }
        if !self.open {
            if !self.mount.is_current() { self.mount = self.owner.attach(); }
            self.open = true;
            self.host = window.window_handle();
            self.popout = false;
            self.native = native;
            self.refresh(cx);
        }
        if focus_on_open {
            if self.removal.is_some() { self.removal_cancel_focus.focus(window, cx); }
            else if let Some(edit) = &self.edit { edit.alias.focus_handle(cx).focus(window, cx); }
            else { self.filter.focus_handle(cx).focus(window, cx); }
        }
        cx.notify();
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.open || self.loading || self.pending || self.edit.is_some() || self.removal.is_some() { return; }
        self.loading = true;
        let path = self.path.clone();
        let receipt = self.mount.clone();
        let task = cx.background_executor().spawn(async move {
            match snippets::load_snippets_for_reload(path) {
                Ok(LoadState::Missing | LoadState::Empty) => Ok(Vec::new()),
                Ok(LoadState::Loaded(rows)) => Ok(rows),
                Err(_) => Err("Could not load snippets. Existing entries are shown read-only."),
            }
        });
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            let _ = weak.update(cx, |view, cx| {
                if !view.open || !receipt.is_current() || !receipt.matches(&view.mount) { return; }
                view.loading = false;
                match result {
                    Ok(rows) => { view.rows = Arc::new(rows); view.generation = view.generation.wrapping_add(1); view.selected = None; view.diagnostic = None; }
                    Err(message) => view.diagnostic = Some(message.into()),
                }
                if std::mem::take(&mut view.cold_popout) {
                    let host = view.host;
                    let weak = cx.entity().downgrade();
                    cx.spawn(async move |_, cx| {
                        let _ = host.update(cx, |_, window, app| { let _ = weak.update(app, |view, cx| view.request_rehost(true, window, cx)); });
                    }).detach();
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || self.pending || self.preview_pending || self.removal.is_some() || self.host.window_id() != window.window_handle().window_id() { return; }
        if let Some(child) = self.preview_child.take() { child.update(cx, |child, cx| child.retire(cx)); }
        let owns_focus = self.focus.is_focused(window) || self.focus.contains_focused(window, cx);
        let popout = self.popout;
        if popout { let _ = self.root.update(cx, |root, cx| root.drain_snippets_geometry(cx, true)); }
        self.open = false;
        self.owner.retire();
        self.loading = false;
        self.authority.fetch_add(1, Ordering::AcqRel);
        self.edit = None;
        self.selected = None;
        self.selected_tags.clear();
        self.filter.update(cx, |input, cx| input.set_value("", window, cx));
        if popout { window.remove_window(); }
        let _ = self.root.update(cx, |root, cx| {
            if owns_focus && window.is_window_active() && !window.has_active_dialog(cx)
                && root.native_window.is_some_and(|native| current_foreground_identity() == Some(native.identity)) {
                root.query.focus_handle(cx).focus(window, cx);
            }
            cx.notify();
        });
        cx.notify();
    }
    fn request_rehost(&mut self, popout: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !rehost_allowed(self.open, self.rehosting, self.popout == popout, self.busy() || self.preview_child.is_some())
            || window.has_active_dialog(cx) || !window.is_window_active() { return; }
        let Some(authority) = host::HostAuthority::capture(self, window, cx) else { return; };
        if self.filter.update(cx, |input, cx| input.marked_text_range(window, cx).is_some()) { return; }
        if let Some(edit) = &self.edit {
            if edit.alias.update(cx, |input, cx| input.marked_text_range(window, cx).is_some())
                || edit.tag_input.update(cx, |input, cx| input.marked_text_range(window, cx).is_some())
                || edit.body.as_ref().is_some_and(|body| body.update(cx, |input, cx| input.marked_text_range(window, cx).is_some()))
                || edit.fields.iter().any(|field| field.label.as_ref().is_some_and(|input| input.update(cx, |input, cx| input.marked_text_range(window, cx).is_some()))
                    || field.default_value.as_ref().is_some_and(|input| input.update(cx, |input, cx| input.marked_text_range(window, cx).is_some()))) { return; }
        }
        let Some(root) = self.root.upgrade() else { return; };
        let revision = root.read(cx).root_visibility.request_revision.load(Ordering::Acquire);
        let gesture = Some(hash_dock::Gesture { source: authority.native, foreground_authorized: true, visibility_revision: revision });
        let receipt = self.mount.clone();
        self.set_rehosting(true, cx);
        if self.pending_named.is_none() && self.cold_origin.is_none() { self.focus.focus(window, cx); }
        let root = self.root.clone();
        cx.spawn(async move |_, cx| { let _ = root.update(cx, |root, cx| root.start_snippets_rehost(popout, gesture, receipt, cx)); }).detach();
        cx.notify();
    }
    pub(super) fn busy(&self) -> bool { self.rehosting || self.loading || self.pending || self.preview_pending || self.removal.is_some() }
    pub(super) fn preview_values(&self, cx: &App) -> Option<Vec<String>> {
        self.preview_child.as_ref().and_then(|child| child.read(cx).restart_draft(cx))
    }
    pub(super) fn blocks_snippet_run(&self) -> bool { self.pending || self.edit.is_some() || self.removal.is_some() }
    fn clear_wait(&self, cx: &mut Context<Self>) {
        if self.busy() { return; }
        let _ = self.root.update(cx, |root, cx| {
            if root.restart_status.as_deref() == Some(SAVE_WAIT) {
                root.restart_status = None;
                cx.notify();
            }
        });
    }
    fn row_visible(&self, generation: u64, index: usize, cx: &App) -> bool {
        self.open && !self.loading && !self.pending && self.removal.is_none() && self.diagnostic.is_none() && self.generation == generation
            && self.rows.get(index).is_some_and(|entry| {
                snippets::snippet_matches_list_filter(entry, self.filter.read(cx).value().as_str(), &self.selected_tags)
            })
    }
    fn request_removal(&mut self, generation: u64, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.row_visible(generation, index, cx) || self.edit.is_some() || window.has_active_dialog(cx) || !window.is_window_active() { return; }
        if host::HostAuthority::capture(self, window, cx).is_none() { return; }
        let Some(entry) = self.rows.get(index).cloned() else { return; };
        self.instance = self.instance.wrapping_add(1);
        self.removal = Some(RemovalReview { snapshot: self.rows.clone(), generation, index, entry, instance: self.instance });
        self.filter.update(cx, |input, cx| input.set_readonly(true, cx));
        self.removal_cancel_focus.focus(window, cx);
        cx.notify();
    }
    fn confirm_removal(&mut self, instance: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || self.diagnostic.is_some() || window.has_active_dialog(cx) || !window.is_window_active() { return; }
        let Some(review) = &self.removal else { return; };
        let Some(candidate) = removal_candidate(review, &self.rows, self.generation, instance) else { return; };
        self.submit(review.snapshot.clone(), candidate, None, Mutation::Remove(instance), window, cx);
    }
    pub(super) fn restart_draft(&self, cx: &App) -> Option<RestartDraft> {
        self.edit.as_ref().map(|edit| RestartDraft {
            snapshot: edit.snapshot.as_ref().clone(), kind: edit.kind,
            alias: edit.alias.read(cx).value().to_string(),
            body: edit.body.as_ref().map(|body| body.read(cx).value().to_string()).unwrap_or_else(|| edit.body_private.clone()), tags: edit.tags.clone(),
            tag_input: edit.tag_input.read(cx).value().to_string(),
            hidden: edit.hidden, revealed: edit.revealed,
            prompt_for_fields: edit.prompt_for_fields, fields: edit.fields.iter().map(|field| field.current(cx)).collect(),
        })
    }

    fn begin_edit(&mut self, generation: u64, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.pending || self.diagnostic.is_some() || self.edit.is_some() || self.removal.is_some()
            || self.generation != generation || self.selected != Some((generation, index)) { return; }
        let Some(entry) = self.rows.get(index).cloned() else { return; };
        self.begin_draft(DraftKind::Existing(index), entry, window, cx);
    }
    fn begin_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || self.loading || self.pending || self.diagnostic.is_some() || self.edit.is_some() || self.removal.is_some() { return; }
        self.begin_draft(DraftKind::New, new_snippet(), window, cx);
    }
    fn begin_draft(&mut self, kind: DraftKind, entry: SnippetEntry, window: &mut Window, cx: &mut Context<Self>) {
        let generation = self.generation;
        let alias = cx.new(|cx| InputState::new(window, cx).placeholder("Alias"));
        let tag_input = cx.new(|cx| InputState::new(window, cx).placeholder("Add tag"));
        let body = (!entry.hide_contents).then(|| cx.new(|cx| TextareaState::new(window, cx).auto_grow(6, 14).placeholder("Snippet text")));
        alias.update(cx, |input, cx| input.set_value(entry.alias.clone(), window, cx));
        let alias_initial_display = alias.read(cx).value().to_string();
        if let Some(body) = &body { body.update(cx, |input, cx| input.set_value(entry.text.clone(), window, cx)); }
        self.instance = self.instance.wrapping_add(1);
        let body_sub = body.as_ref().map(|body| cx.subscribe(body, |_: &mut Self, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) { cx.notify(); }
        }));
        let fields = entry.fields.iter().cloned().map(|definition| AuthorField { definition, label: None,
            label_initial_display: String::new(), default_value: None }).collect();
        self.edit = Some(EditDraft { snapshot: self.rows.clone(), generation, kind, instance: self.instance,
            alias: alias.clone(), alias_initial_display, body, body_private: entry.text.clone(), tags: entry.tags.clone(), tag_input, hidden: entry.hide_contents, revealed: !entry.hide_contents,
            prompt_for_fields: entry.prompt_for_fields, fields, body_sub });
        self.feedback = None;
        alias.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn set_revealed(&mut self, instance: u64, revealed: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.edit.as_mut().filter(|edit| edit.instance == instance) else { return; };
        if revealed == edit.revealed { return; }
        if revealed {
            let body = cx.new(|cx| TextareaState::new(window, cx).auto_grow(6, 14).placeholder("Snippet text"));
            body.update(cx, |input, cx| input.set_value(edit.body_private.clone(), window, cx));
            edit.body = Some(body);
            edit.body_sub = edit.body.as_ref().map(|body| cx.subscribe(body, |_: &mut Self, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) { cx.notify(); }
            }));
        } else if let Some(body) = edit.body.take() {
            if body.focus_handle(cx).is_focused(window) { self.focus.focus(window, cx); }
            edit.body_private = body.read(cx).value().to_string();
            edit.body_sub = None;
            for field in &mut edit.fields { field.detach(cx); }
        }
        edit.revealed = revealed;
        cx.notify();
    }

    fn discover_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Vec<String> {
        if self.pending || self.preview_pending || self.preview_child.is_some() { return Vec::new(); }
        let Some(edit) = self.edit.as_mut() else { return Vec::new(); };
        if !edit.revealed || !edit.prompt_for_fields { return Vec::new(); }
        let text = edit.body.as_ref().map(|body| body.read(cx).value().to_string()).unwrap_or_else(|| edit.body_private.clone());
        let definitions: Vec<_> = edit.fields.iter().map(|field| field.current(cx)).collect();
        let Ok(discovery) = multi_launcher::snippet_prompt::discover_editor_fields(&text, &definitions) else { return Vec::new(); };
        for definition in discovery.fields {
            if !edit.fields.iter().any(|field| field.definition.name == definition.name) {
                edit.fields.push(AuthorField { definition, label: None, label_initial_display: String::new(), default_value: None });
            }
        }
        for field in &mut edit.fields {
            if discovery.keys.contains(&field.definition.name) { field.attach(window, cx); }
        }
        discovery.keys
    }

    fn editor_candidate(&self, instance: u64, cx: &App) -> Result<(Arc<Vec<SnippetEntry>>, Vec<SnippetEntry>, Option<usize>, String), String> {
        let edit = self.edit.as_ref().ok_or("The editor is no longer available.")?;
        if !edit_is_current(edit, self.generation, &self.rows, instance) { return Err("The saved list changed.".into()); }
        let original = match edit.kind {
            DraftKind::Existing(index) => edit.snapshot.get(index).cloned().ok_or("The edited snippet no longer exists.")?,
            DraftKind::New => new_snippet(),
        };
        let alias = edited_alias(&original.alias, &edit.alias_initial_display, edit.alias.read(cx).value().as_str());
        if alias.trim().is_empty() { return Err("An alias is required.".into()); }
        let tags = snippets::normalize_snippet_tags(edit.tags.clone()).map_err(str::to_owned)?;
        let text = edit.body.as_ref().map(|body| body.read(cx).value().to_string()).unwrap_or_else(|| edit.body_private.clone());
        let mut draft = original;
        draft.prompt_for_fields = edit.prompt_for_fields;
        draft.fields = edit.fields.iter().map(|field| field.current(cx)).collect();
        let mut candidate = snippets::prepare_snippet_draft(&draft, &text).map_err(|_| "The prompted snippet template is invalid.".to_owned())?;
        candidate.alias = alias.clone();
        candidate.tags = tags;
        candidate.hide_contents = edit.hidden;
        let mut rows = edit.snapshot.as_ref().clone();
        let index = match edit.kind {
            DraftKind::Existing(index) => { rows[index] = candidate; Some(index) }
            DraftKind::New => { rows.push(candidate); None }
        };
        snippets::snapshot_edit_decision(&edit.snapshot, &edit.snapshot, &rows, Some((index, &alias)))
            .map_err(|_| "An entry with this exact alias already exists.".to_owned())?;
        Ok((edit.snapshot.clone(), rows, index, alias))
    }

    fn set_editor_readonly(&mut self, readonly: bool, cx: &mut Context<Self>) {
        if let Some(edit) = &self.edit {
            for input in [&edit.alias, &edit.tag_input] { input.update(cx, |input, cx| input.set_readonly(readonly, cx)); }
            if let Some(body) = &edit.body { body.update(cx, |input, cx| input.set_readonly(readonly, cx)); }
            for field in &edit.fields {
                if let Some(label) = &field.label { label.update(cx, |input, cx| input.set_readonly(readonly, cx)); }
                if let Some(default) = &field.default_value { default.update(cx, |input, cx| input.set_readonly(readonly, cx)); }
            }
        }
    }

    pub(super) fn return_from_preview(&mut self, issued: &utility_mount::MountReceipt, instance: u64, host: WindowId, child: EntityId,
        focus: Option<FocusHandle>, owns: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !preview_return_is_current(issued, &self.mount, self.preview_child.is_some(), self.open, instance,
            self.edit.as_ref().map(|edit| edit.instance), self.host.window_id() == host && window.window_handle().window_id() == host)
            || !preview_child_is_current(self.preview_child.as_ref().map(|current| current.entity_id()), child) { return; }
        self.preview_child = None;
        self.set_editor_readonly(false, cx);
        if owns && window.is_window_active() && !window.has_active_dialog(cx)
            && host::HostAuthority::capture(self, window, cx).is_some() {
            let _ = focus;
            self.preview_focus.focus(window, cx);
            if let Some(offset) = self.preview_scroll_offset.take() {
                let weak = cx.entity().downgrade();
                let mount = self.mount.clone();
                window.defer(cx, move |window, app| {
                    let _ = weak.update(app, |view, cx| {
                        if mount.is_current() && mount.matches(&view.mount) && view.preview_child.is_none()
                            && window.is_window_active() && view.preview_focus.is_focused(window)
                            && host::HostAuthority::capture(view, window, cx).is_some() {
                            if view.popout { view.outer_scroll.set_offset(offset); }
                            else { let _ = view.root.update(cx, |root, _| root.root_scroll.set_offset(offset)); }
                        }
                    });
                });
            }
        } else {
            self.preview_scroll_offset = None;
        }
        cx.notify();
    }

    fn start_preview(&mut self, instance: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || self.preview_pending || self.preview_child.is_some() || self.loading || self.diagnostic.is_some()
            || !self.open || !self.mount.is_current() || !window.is_window_active() || window.has_active_dialog(cx)
            || !(self.focus.is_focused(window) || self.focus.contains_focused(window, cx) || self.preview_focus.is_focused(window))
            || !self.edit.as_ref().is_some_and(|edit| edit.instance == instance && edit.revealed) { return; }
        let (_, rows, index, _) = match self.editor_candidate(instance, cx) {
            Ok(candidate) => candidate,
            Err(error) => { self.feedback = Some(error); cx.notify(); return; }
        };
        let index = index.unwrap_or(rows.len() - 1);
        let definition = match multi_launcher::snippet_prompt::prepare(&rows[index]) {
            Ok(multi_launcher::snippet_prompt::PreparedInvocation::Automatic(definition)
                | multi_launcher::snippet_prompt::PreparedInvocation::Prompted(definition)) => definition,
            _ => { self.feedback = Some("Add a prompt field or automatic value to preview.".into()); cx.notify(); return; }
        };
        let Some(mounted) = host::HostAuthority::capture(self, window, cx) else { return; };
        let Some(root) = self.root.upgrade() else { return; };
        let root_state = root.read(cx);
        let native = mounted.native;
        let host = mounted.host;
        let focus = window.focused(cx);
        if (!self.popout && !root_state.external_owner_is_current(focus.as_ref(), window, cx))
            || root_state.snippet_fill.as_ref().is_some_and(|view| view.read(cx).open) { return; }
        let Some(permit) = root_state.restart_admission.admit() else { return; };
        let admission = root_state.restart_admission.clone();
        let visibility = root_state.root_visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let origin = visibility.snippet_origin.lock().ok().and_then(|origin| origin.as_ref().filter(|(issued, _)| *issued == revision).map(|(_, origin)| origin.clone()));
        let timestamp = chrono::Local::now().fixed_offset();
        let receipt = self.mount.clone();
        let worker_receipt = receipt.clone();
        let Some(origin_focus) = focus.as_ref() else { return; };
        let lease = Arc::new(AtomicBool::new(true));
        let focus_lease = lease.clone();
        self.preview_focus_guard = Some(cx.on_blur(origin_focus, window, move |_, _, _| focus_lease.store(false, Ordering::Release)));
        if !self.popout { root.update(cx, |root, _| root.external_launch.set_snippet_preview_lease(lease.clone())); }
        let active_lease = lease.clone();
        self.preview_activation_guard = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() { active_lease.store(false, Ordering::Release); }
        }));
        let worker_lease = lease.clone();
        let worker_host = mounted.clone();
        let ack_host = mounted.clone();
        self.preview_pending = true;
        self.set_editor_readonly(true, cx);
        self.feedback = None;
        let task = cx.background_executor().spawn(async move {
            let current: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || worker_lease.load(Ordering::Acquire) && worker_receipt.is_current() && !admission.is_fenced()
                && worker_host.current());
            if !current() { return None; }
            let context = definition.has_system_variables().then(|| multi_launcher::snippet_prompt::capture_protected_context_guarded(
                timestamp, definition.uses_clipboard(), definition.uses_window_title(), current.clone(), || origin)).flatten();
            if !current() { return None; }
            definition.begin(context).ok()
        });
        cx.spawn(async move |parent, cx| {
            let _permit = permit;
            let session = task.await;
            let view_entity = parent.upgrade()?;
            let fallback_receipt = receipt.clone();
            let ack_lease = lease.clone();
            let result = update_kit_window_content(host, &view_entity, cx, |view, window, cx| {
                if !view.preview_pending || !receipt.is_current() || !receipt.matches(&view.mount)
                    || !view.open || !view.edit.as_ref().is_some_and(|edit| edit.instance == instance) { return; }
                view.preview_pending = false;
                view.preview_focus_guard = None;
                view.preview_activation_guard = None;
                let Some(root) = view.root.upgrade() else { return; };
                root.update(cx, |root, _| root.external_launch.clear_snippet_preview_lease(&ack_lease));
                let valid = ack_lease.load(Ordering::Acquire) && window.is_window_active() && !window.has_active_dialog(cx)
                    && current_foreground_identity() == Some(native.identity)
                    && ack_host.current_for(view, window, cx)
                    && (view.popout || root.read(cx).external_owner_is_current(focus.as_ref(), window, cx))
                    && focus.as_ref() == window.focused(cx).as_ref()
                    && !root.read(cx).restart_admission.is_fenced();
                if !valid { view.set_editor_readonly(false, cx); view.clear_wait(cx); cx.notify(); return; }
                let Some(session) = session else { view.set_editor_readonly(false, cx); view.clear_wait(cx); view.feedback = Some("Could not prepare the preview.".into()); cx.notify(); return; };
                if root.read(cx).snippet_fill.as_ref().is_some_and(|child| child.read(cx).open) {
                    view.set_editor_readonly(false, cx); view.clear_wait(cx);
                    view.feedback = Some("Close the active Snippet Fill before previewing.".into()); cx.notify(); return;
                }
                let parent = cx.entity().downgrade();
                let root_weak = view.root.clone();
                view.preview_scroll_offset = Some(if view.popout { view.outer_scroll.offset() } else { root.read(cx).root_scroll.offset() });
                let child = cx.new(|cx| super::snippet_fill::SnippetFill::preview(session, parent, receipt.clone(), instance,
                    host.window_id(), root_weak, focus, window, cx));
                child.update(cx, |child, cx| child.focus_initial(window, cx));
                view.preview_child = Some(child);
                if view.popout { view.outer_scroll.set_offset(point(px(0.0), px(0.0))); }
                else {
                    root.update(cx, |root, cx| {
                        let id = window.window_handle().window_id();
                        let history = root.clipboard_history.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
                        let modify = root.clipboard_modify_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
                        root.root_scroll.scroll_to_top_of_item(root.clipboard_scroll_item(id, cx) + usize::from(history) + usize::from(modify));
                    });
                }
                view.clear_wait(cx);
                cx.notify();
            });
            if result.is_err() {
                let _ = view_entity.update(cx, |view, cx| {
                    if view.preview_pending && fallback_receipt.is_current() && fallback_receipt.matches(&view.mount)
                        && view.edit.as_ref().is_some_and(|edit| edit.instance == instance) {
                        view.preview_pending = false;
                        view.preview_focus_guard = None;
                        view.preview_activation_guard = None;
                        view.set_editor_readonly(false, cx);
                        if let Some(root) = view.root.upgrade() { root.update(cx, |root, _| root.external_launch.clear_snippet_preview_lease(&lease)); }
                        view.clear_wait(cx);
                        view.feedback = Some("Could not open the preview.".into());
                        cx.notify();
                    }
                });
            }
            Some(())
        }).detach();
        cx.notify();
    }

    fn save(&mut self, instance: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || self.preview_pending || self.preview_child.is_some() || self.loading || self.diagnostic.is_some() || !window.is_window_active()
            || window.has_active_dialog(cx) || !self.mount.is_current() { return; }
        let (expected, rows, index, alias) = match self.editor_candidate(instance, cx) {
            Ok(candidate) => candidate,
            Err(error) => { self.feedback = Some(error); cx.notify(); return; }
        };
        self.submit(expected, rows, Some((index, alias)), Mutation::Save(instance), window, cx);
    }

    fn submit(&mut self, expected: Arc<Vec<SnippetEntry>>, rows: Vec<SnippetEntry>,
        alias: Option<(Option<usize>, String)>, mutation: Mutation, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || self.loading || !self.open || !window.is_window_active() || window.has_active_dialog(cx) || !self.mount.is_current() { return; }
        let Some(mounted) = host::HostAuthority::capture(self, window, cx) else { return; };
        let Some(root) = self.root.upgrade() else { return; };
        let root = root.read(cx);
        let native = mounted.native;
        let Some(permit) = root.restart_admission.admit() else { return; };
        let admission = root.restart_admission.clone();
        let worker_host = mounted.clone();
        let ack_host = mounted.clone();
        let authority = self.authority.clone();
        let ack_authority = authority.clone();
        let epoch = authority.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        let receipt = self.mount.clone();
        let worker_receipt = receipt.clone();
        let path = self.path.clone();
        let host = self.host;
        self.set_editor_readonly(true, cx);
        self.focus.focus(window, cx);
        self.pending = true;
        self.feedback = None;
        let task = cx.background_executor().spawn(async move {
            snippets::update_snapshot_checked(path, &expected, rows, alias.as_ref().map(|(index, alias)| (*index, alias.as_str())), || {
                worker_receipt.is_current() && authority.load(Ordering::Acquire) == epoch
                    && !admission.is_fenced() && worker_host.current()
            })
        });
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = task.await;
            let Some(view) = weak.upgrade() else { return; };
            let update = update_kit_window_content(host, &view, cx, |view, window, cx| {
                let exact = view.pending && ack_authority.load(Ordering::Acquire) == epoch
                    && view.open && receipt.is_current() && receipt.matches(&view.mount)
                    && match mutation {
                        Mutation::Save(instance) => view.edit.as_ref().is_some_and(|edit| edit.instance == instance),
                        Mutation::Remove(instance) => view.removal.as_ref().is_some_and(|review| review.instance == instance),
                    };
                if !exact { cx.notify(); return; }
                let owns_focus = view.focus.is_focused(window) || view.focus.contains_focused(window, cx);
                view.pending = false;
                match result {
                    Ok(snippets::CheckedSnapshotUpdate::Saved(rows) | snippets::CheckedSnapshotUpdate::Unchanged(rows)) => {
                        view.rows = Arc::new(rows); view.generation = view.generation.wrapping_add(1);
                        view.selected = None; view.edit = None; view.removal = None;
                        view.filter.update(cx, |input, cx| input.set_readonly(false, cx));
                        view.feedback = Some(match mutation { Mutation::Save(_) => "Snippet saved.", Mutation::Remove(_) => "Snippet removed." }.into());
                        if window.is_window_active() && current_foreground_identity() == Some(native.identity)
                            && view.focus.is_focused(window) { view.filter.focus_handle(cx).focus(window, cx); }
                    }
                    Ok(snippets::CheckedSnapshotUpdate::Rejected(snippets::SnapshotRejection::DuplicateAlias)) => view.feedback = Some("An entry with this exact alias already exists.".into()),
                    Ok(snippets::CheckedSnapshotUpdate::Rejected(_) | snippets::CheckedSnapshotUpdate::DiskChanged) => {
                        view.diagnostic = Some("Snippets changed on disk. Reload before editing.".into());
                        if matches!(mutation, Mutation::Remove(_)) {
                            view.removal = None;
                            view.filter.update(cx, |input, cx| input.set_readonly(false, cx));
                        }
                        view.feedback = Some("Snippets changed on disk. Reload before editing.".into());
                    }
                    Ok(snippets::CheckedSnapshotUpdate::Superseded) => view.feedback = Some("Save was superseded. Draft retained.".into()),
                    Err(error) => {
                        if matches!(error.downcast_ref::<multi_launcher::common::persistence::PersistenceError>(),
                            Some(multi_launcher::common::persistence::PersistenceError::Read { .. } | multi_launcher::common::persistence::PersistenceError::MalformedJson { .. }))
                        {
                            view.diagnostic = Some("Could not load snippets. Existing entries are shown read-only.".into());
                            if matches!(mutation, Mutation::Remove(_)) {
                                view.removal = None;
                                view.filter.update(cx, |input, cx| input.set_readonly(false, cx));
                            }
                        }
                        view.feedback = Some("Could not save snippet. Draft retained.".into());
                    }
                }
                view.set_editor_readonly(false, cx);
                if view.removal.is_some() && owns_focus && window.is_window_active()
                    && !window.has_active_dialog(cx) && current_foreground_identity() == Some(native.identity)
                    && ack_host.current_for(view, window, cx)
                {
                    view.removal_cancel_focus.focus(window, cx);
                }
                view.clear_wait(cx);
                cx.notify();
            });
            if update.is_err() { let _ = view.update(cx, |view, cx| {
                let exact = view.pending && ack_authority.load(Ordering::Acquire) == epoch
                    && view.open && receipt.is_current() && receipt.matches(&view.mount)
                    && match mutation {
                        Mutation::Save(instance) => view.edit.as_ref().is_some_and(|edit| edit.instance == instance),
                        Mutation::Remove(instance) => view.removal.as_ref().is_some_and(|review| review.instance == instance),
                    };
                if !exact { return; }
                view.pending = false; view.feedback = Some("Save result could not be delivered.".into());
                view.set_editor_readonly(false, cx);
                view.clear_wait(cx); cx.notify(); }); }
        }).detach();
        cx.notify();
    }
}

impl Render for SnippetsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.rehosting { return div().id("snippets-rehosting").track_focus(&self.focus).child(Label::new("Moving Snippets…")); }
        let discovered = self.discover_fields(window, cx);
        let mount = self.mount.clone();
        let filter = self.filter.read(cx).value().to_string();
        let key_mount = mount.clone();
        let mut tags = Vec::<String>::new();
        for tag in self.rows.iter().flat_map(|entry| entry.tags.iter()).chain(self.selected_tags.iter()) {
            if !tags.iter().any(|known| known.to_lowercase() == tag.to_lowercase()) { tags.push(tag.clone()); }
        }
        tags.sort_by_key(|tag| tag.to_lowercase());
        let mut panel = div().id("snippets-browser").track_focus(&self.focus)
            .capture_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                if key_mount.is_current() && key_mount.matches(&view.mount)
                    && event.is_held && event.keystroke.modifiers == Modifiers::default()
                    && matches!(event.keystroke.key.as_str(), "enter" | "space")
                    && (view.save_focus.is_focused(window) || view.removal_cancel_focus.is_focused(window)
                        || view.removal_confirm_focus.is_focused(window) || view.removal_focus.is_focused(window))
                { window.prevent_default(); cx.stop_propagation(); }
                if key_mount.is_current() && key_mount.matches(&view.mount) && view.removal.is_some()
                    && (view.removal_focus.is_focused(window) || view.removal_focus.contains_focused(window, cx))
                    && event.keystroke.modifiers == Modifiers::default() && event.keystroke.key == "escape"
                {
                    window.prevent_default(); cx.stop_propagation();
                    if !event.is_held && !view.pending {
                        view.removal = None;
                        view.filter.update(cx, |input, cx| input.set_readonly(false, cx));
                        view.filter.focus_handle(cx).focus(window, cx);
                        view.clear_wait(cx);
                        cx.notify();
                    }
                }
            }))
            .w_full().flex().flex_col().gap_2().p_3()
            .child(Label::new("Snippets"))
            .child(Input::new(&self.filter).aria_label("Filter snippets"));
        let rehost = mount.clone();
        panel = panel.child(Button::new("snippets-rehost").label(if self.popout { "Dock" } else { "Pop Out" })
            .disabled(self.rehosting || self.busy())
            .on_click(cx.listener(move |view, _, window, cx| {
                if rehost.is_current() && rehost.matches(&view.mount) { view.request_rehost(!view.popout, window, cx); }
            })));
        if let Some(error) = &self.presentation_error { panel = panel.child(Label::new(error.clone())); }
        if let Some(error) = &self.geometry_error { panel = panel.child(Label::new(error.clone())); }
        if let Some(child) = &self.preview_child {
            return div().id("snippets-preview-parent").child(child.clone());
        }
        let mut tag_row = div().flex().flex_wrap().gap_2();
        for (index, tag) in tags.into_iter().enumerate() {
            let selected = self.selected_tags.iter().any(|value| value.to_lowercase() == tag.to_lowercase());
            let receipt = mount.clone();
            tag_row = tag_row.child(Button::new(SharedString::from(format!("snippet-filter-tag-{index}")))
                .label(tag.clone()).selected(selected).on_click(cx.listener(move |view, _, _, cx| {
                    if !receipt.is_current() || !receipt.matches(&view.mount) || !view.open || view.removal.is_some() || view.pending || view.preview_pending || view.preview_child.is_some() { return; }
                    if view.selected_tags.iter().any(|value| value.to_lowercase() == tag.to_lowercase()) { view.selected_tags.retain(|value| value.to_lowercase() != tag.to_lowercase()); }
                    else { view.selected_tags.push(tag.clone()); }
                    cx.notify();
                })));
        }
        panel = panel.child(tag_row);
        if let Some(message) = &self.diagnostic { panel = panel.child(Label::new(message.clone())); }
        if let Some(message) = &self.feedback { panel = panel.child(Label::new(message.clone())); }
        if self.loading { panel = panel.child(Label::new("Loading snippets…")); }
        if let Some(review) = &self.removal {
            let instance = review.instance;
            let confirm = mount.clone();
            let cancel = mount.clone();
            panel = panel.child(div().track_focus(&self.removal_focus).flex().flex_col().gap_2()
                .child(Label::new(format!("Remove snippet {}?", review.entry.alias)))
                .child(Label::new(snippets::snippet_preview_text(&review.entry)))
                .child(div().flex().gap_2()
                    .child(Button::new("snippet-remove-confirm").label("Remove").track_focus(&self.removal_confirm_focus).disabled(self.pending)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if confirm.is_current() && confirm.matches(&view.mount) { view.confirm_removal(instance, window, cx); }
                        })))
                    .child(Button::new("snippet-remove-cancel").label("Cancel").track_focus(&self.removal_cancel_focus).disabled(self.pending)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if cancel.is_current() && cancel.matches(&view.mount) && !view.pending
                                && view.removal.as_ref().is_some_and(|review| review.instance == instance)
                                && !window.has_active_dialog(cx) {
                                view.removal = None;
                                view.filter.update(cx, |input, cx| input.set_readonly(false, cx));
                                view.filter.focus_handle(cx).focus(window, cx);
                                view.clear_wait(cx);
                                cx.notify();
                            }
                        })))));
        } else if let Some(edit) = &self.edit {
            let instance = edit.instance;
            let revealed = edit.revealed;
            let hidden = edit.hidden;
            let edit_mount = mount.clone();
            let reveal_mount = mount.clone();
            let cancel_mount = mount.clone();
            let add_tag_mount = mount.clone();
            panel = panel.child(Label::new(if edit.kind == DraftKind::New { "Add snippet" } else { "Edit snippet" }))
                .child(Input::new(&edit.alias).aria_label("Snippet alias"))
                .child(Input::new(&edit.tag_input).aria_label("Add snippet tag"));
            let mut tag_edit = div().flex().flex_wrap().gap_2();
            for (index, tag) in edit.tags.iter().enumerate() {
                let receipt = mount.clone();
                let tag = tag.clone();
                tag_edit = tag_edit.child(Button::new(SharedString::from(format!("snippet-remove-tag-{instance}-{index}")))
                    .label(format!("Remove {tag}"))
                    .disabled(self.pending || self.preview_pending)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if !receipt.is_current() || !receipt.matches(&view.mount) || view.pending || view.preview_pending || view.preview_child.is_some() { return; }
                        if let Some(edit) = view.edit.as_mut().filter(|edit| edit.instance == instance)
                            && edit.tags.get(index) == Some(&tag)
                        { edit.tags.remove(index); cx.notify(); }
                    })));
            }
            panel = panel.child(tag_edit).child(Button::new("snippet-add-tag").label("Add tag").disabled(self.pending || self.preview_pending)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if !add_tag_mount.is_current() || !add_tag_mount.matches(&view.mount) || view.pending || view.preview_pending || view.preview_child.is_some() { return; }
                    let Some(edit) = view.edit.as_mut().filter(|edit| edit.instance == instance) else { return; };
                    let value = edit.tag_input.read(cx).value().to_string();
                    let mut tags = edit.tags.clone();
                    tags.push(value);
                    match snippets::normalize_snippet_tags(tags) {
                        Ok(tags) => { edit.tags = tags; edit.tag_input.update(cx, |input, cx| input.set_value("", window, cx)); view.feedback = None; }
                        Err(message) => view.feedback = Some(message.into()),
                    }
                    cx.notify();
                })));
            if let Some(body) = &edit.body { panel = panel.child(Textarea::new(body).aria_label("Snippet text")); }
            else { panel = panel.child(Label::new("Contents hidden")); }
            if revealed {
                let toggle_mount = mount.clone();
                let prompted = edit.prompt_for_fields;
                panel = panel.child(Button::new("snippet-prompt-fields")
                    .label(if prompted { "Prompt for Fields: On" } else { "Prompt for Fields: Off" })
                    .disabled(self.pending || self.preview_pending)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if toggle_mount.is_current() && toggle_mount.matches(&view.mount) && !view.pending && !view.preview_pending && !view.preview_child.is_some() {
                            if let Some(edit) = view.edit.as_mut().filter(|edit| edit.instance == instance) {
                                edit.prompt_for_fields = !edit.prompt_for_fields;
                                cx.notify();
                            }
                        }
                    })));
                if prompted {
                    for (order, key) in discovered.iter().enumerate() {
                        let Some(field) = edit.fields.iter().find(|field| &field.definition.name == key) else { continue; };
                        let (Some(label), Some(default)) = (&field.label, &field.default_value) else { continue; };
                        let required = field.definition.required;
                        let kind = field.definition.input_kind;
                        let required_key = key.clone();
                        let kind_key = key.clone();
                        let required_mount = mount.clone();
                        let kind_mount = mount.clone();
                        panel = panel.child(div().id(SharedString::from(format!("snippet-field-{instance}-{order}"))).flex().flex_col().gap_1()
                            .child(Label::new(format!("Field {key}")))
                            .child(Input::new(label).aria_label(format!("Label for {key}")))
                            .child(Textarea::new(default).aria_label(format!("Default for {key}")))
                            .child(div().flex().gap_2()
                                .child(Button::new(SharedString::from(format!("snippet-field-required-{instance}-{order}")))
                                    .label(if required { "Required" } else { "Optional" }).disabled(self.pending || self.preview_pending)
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        if !required_mount.is_current() || !required_mount.matches(&view.mount) || view.pending || view.preview_pending || view.preview_child.is_some() { return; }
                                        if let Some(field) = view.edit.as_mut().filter(|edit| edit.instance == instance)
                                            .and_then(|edit| edit.fields.iter_mut().find(|field| field.definition.name == required_key)) {
                                            field.definition.required = !field.definition.required; cx.notify();
                                        }
                                    })))
                                .child(Button::new(SharedString::from(format!("snippet-field-kind-{instance}-{order}")))
                                    .label(if kind == snippets::SnippetInputKind::SingleLine { "Single line" } else { "Multiline" }).disabled(self.pending || self.preview_pending)
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        if !kind_mount.is_current() || !kind_mount.matches(&view.mount) || view.pending || view.preview_pending || view.preview_child.is_some() { return; }
                                        if let Some(field) = view.edit.as_mut().filter(|edit| edit.instance == instance)
                                            .and_then(|edit| edit.fields.iter_mut().find(|field| field.definition.name == kind_key)) {
                                            field.definition.input_kind = if field.definition.input_kind == snippets::SnippetInputKind::SingleLine { snippets::SnippetInputKind::Multiline } else { snippets::SnippetInputKind::SingleLine }; cx.notify();
                                        }
                                    })))));
                    }
                }
            }
            panel = panel.child(div().flex().gap_2()
                .child(Button::new("snippet-reveal").label(if revealed { "Hide" } else { "Reveal" }).disabled(self.pending || self.preview_pending)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if reveal_mount.is_current() && reveal_mount.matches(&view.mount) && !view.pending && !view.preview_pending && !view.preview_child.is_some() {
                            view.set_revealed(instance, !revealed, window, cx);
                        }
                    })))
                .child(Button::new("snippet-privacy").label(if hidden { "Hidden in previews" } else { "Visible in previews" }).disabled(self.pending || self.preview_pending)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if edit_mount.is_current() && edit_mount.matches(&view.mount) && !view.pending && !view.preview_pending && !view.preview_child.is_some() {
                            if let Some(edit) = view.edit.as_mut().filter(|edit| edit.instance == instance) { edit.hidden = !hidden; }
                            if !hidden { view.set_revealed(instance, false, window, cx); }
                            cx.notify();
                        }
                    })))
                .child(Button::new("snippet-save").label("Save").track_focus(&self.save_focus).disabled(self.pending || self.preview_pending || self.diagnostic.is_some())
                    .on_click(cx.listener(move |view, _, window, cx| view.save(instance, window, cx))))
                .child(Button::new("snippet-preview").label("Test / Preview").track_focus(&self.preview_focus).disabled(self.rehosting || self.pending || self.preview_pending || !revealed || self.diagnostic.is_some())
                    .on_click(cx.listener(move |view, _, window, cx| view.start_preview(instance, window, cx))))
                .child(Button::new("snippet-cancel-edit").label("Cancel").disabled(self.pending || self.preview_pending)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if cancel_mount.is_current() && cancel_mount.matches(&view.mount) && !view.pending && !view.preview_pending && !view.preview_child.is_some()
                            && view.edit.as_ref().is_some_and(|edit| edit.instance == instance) {
                            view.edit = None; view.feedback = None;
                            if view.diagnostic.is_some() { view.refresh(cx); }
                            view.filter.focus_handle(cx).focus(window, cx); cx.notify();
                        }
                    }))));
        } else {
            let mut list = div().id("snippet-list-scroll").overflow_y_scroll().h(px(240.)).flex().flex_col();
            for (index, entry) in self.rows.iter().enumerate() {
                if !snippets::snippet_matches_list_filter(entry, &filter, &self.selected_tags) { continue; }
                let generation = self.generation;
                let select = mount.clone();
                let edit_mount = mount.clone();
                let remove_mount = mount.clone();
                let selected = self.selected == Some((generation, index));
                list = list.child(div().id(SharedString::from(format!("snippet-row-{generation}-{index}"))).flex().gap_2()
                    .child(Button::new(SharedString::from(format!("snippet-select-{generation}-{index}")))
                        .label(format!("{}: {}", entry.alias, snippets::snippet_preview_text(entry)))
                        .selected(selected).disabled(self.loading || self.diagnostic.is_some())
                        .on_click(cx.listener(move |view, _, _, cx| {
                            if select.is_current() && select.matches(&view.mount) && view.row_visible(generation, index, cx) {
                                view.selected = Some((generation, index)); cx.notify();
                            }
                        })))
                    .child(Button::new(SharedString::from(format!("snippet-edit-{generation}-{index}"))).label("Edit")
                        .disabled(self.loading || self.diagnostic.is_some())
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if edit_mount.is_current() && edit_mount.matches(&view.mount) && view.row_visible(generation, index, cx) {
                                view.selected = Some((generation, index)); view.begin_edit(generation, index, window, cx);
                            }
                        })))
                    .child(Button::new(SharedString::from(format!("snippet-remove-{generation}-{index}"))).label("Remove…")
                        .disabled(self.loading || self.diagnostic.is_some())
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if remove_mount.is_current() && remove_mount.matches(&view.mount) {
                                view.request_removal(generation, index, window, cx);
                            }
                        }))));
            }
            let add = mount.clone();
            panel = panel.child(list).child(Button::new("snippet-add").label("Add Snippet")
                .disabled(self.loading || self.pending || self.diagnostic.is_some())
                .on_click(cx.listener(move |view, _, window, cx| {
                    if add.is_current() && add.matches(&view.mount) && !window.has_active_dialog(cx) && window.is_window_active() {
                        view.begin_new(window, cx);
                    }
                })));
        }
        let reload = mount.clone();
        let clear = mount.clone();
        let close = mount;
        panel.child(div().flex().gap_2()
            .child(Button::new("snippet-reload").label("Reload").disabled(self.loading || self.edit.is_some() || self.removal.is_some() || self.pending)
                .on_click(cx.listener(move |view, _, _, cx| { if reload.is_current() && reload.matches(&view.mount) { view.refresh(cx); } })))
            .child(Button::new("snippet-clear-filter").label("Clear Filters").disabled(self.removal.is_some() || self.pending)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if clear.is_current() && clear.matches(&view.mount) && view.open && view.removal.is_none() && !view.pending {
                        view.selected_tags.clear();
                        view.filter.update(cx, |input, cx| input.set_value("", window, cx));
                        cx.notify();
                    }
                })))
            .child(Button::new("snippet-close").label("Close").disabled(self.pending || self.removal.is_some())
                .on_click(cx.listener(move |view, _, window, cx| { if close.is_current() && close.matches(&view.mount) { view.close(window, cx); } }))))
    }
}

impl RootWindow {
    fn activate_snippets_popup(&mut self, named: Option<NamedOpen>, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(popup), Some(view), Some(root_native)) = (&self.snippets_popup, self.snippets_view.clone(), self.native_window) else { return; };
        let Some(native) = popup.native else { return; };
        if current_foreground_identity() != Some(root_native.identity) || view.read(cx).rehosting { return; }
        if named.as_ref().is_some_and(|named| !named.lease.load(Ordering::Acquire)
            || window.has_active_dialog(cx) || !self.external_owner_is_current(Some(&named.focus), window, cx)) { return; }
        let bare = if named.is_none() {
            let Some(focus) = window.focused(cx) else { return; };
            if window.has_active_dialog(cx) || !self.external_owner_is_current(Some(&focus), window, cx) { return; }
            let lease = Arc::new(AtomicBool::new(true));
            let blur = lease.clone();
            let guard = cx.on_blur(&focus, window, move |_, _, _| blur.store(false, Ordering::Release));
            Some(ColdOpen { focus, lease, query: self.query.read(cx).value().to_string(),
                generation: self.launcher_results.generation, epoch: self.snippets_search.request_epoch.load(Ordering::Acquire), _focus_guard: guard })
        } else { None };
        let Some(permit) = self.restart_admission.admit() else { return; };
        let popup_window = popup.window;
        let mount = view.read(cx).mount.clone();
        let worker_mount = mount.clone();
        let visibility = self.root_visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let admission = self.restart_admission.clone();
        let activation = view.read(cx).activation.clone();
        let named_guard = named.as_ref().map(|named| (named.snapshot.clone(), named.owner.clone(), named.epoch, named.lease.clone()));
        let bare_worker = bare.as_ref().map(|bare| (bare.lease.clone(), bare.epoch));
        let source_lease = named.as_ref().map(|named| named.lease.clone()).or_else(|| bare.as_ref().map(|bare| bare.lease.clone()));
        view.update(cx, |view, _| view.activation_source_lease = source_lease);
        let named_epoch = self.snippets_search.request_epoch.clone();
        let attempt = activation.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        let worker = activation.clone();
        cx.spawn(async move |_, cx| {
            let _permit = permit;
            let activated = cx.background_executor().spawn(async move {
                activate_for_secondary_window_if(&worker, attempt, native, Some(root_native.identity), &|| {
                    worker_mount.is_current() && !admission.is_fenced()
                        && json_utility::native_host_current(&visibility, revision, Some(root_native), false)
                        && named_guard.as_ref().is_none_or(|(snapshot, owner, epoch, lease)| lease.load(Ordering::Acquire) && named_epoch.load(Ordering::Acquire) == *epoch && search_snapshot_current(owner, snapshot))
                        && bare_worker.as_ref().is_none_or(|(lease, epoch)| lease.load(Ordering::Acquire) && named_epoch.load(Ordering::Acquire) == *epoch)
                })
            }).await;
            if !activated || !mount.is_current() || activation.load(Ordering::Acquire) != attempt || current_foreground_identity() != Some(native.identity) { return; }
            let _ = update_kit_window_content(popup_window, &view, cx, |view, window, cx| {
                if !view.is_mounted_in(window.window_handle().window_id(), true) || view.rehosting || !window.is_window_active()
                    || host::HostAuthority::capture(view, window, cx).is_none() { return; }
                if let Some(bare) = &bare {
                    let valid = view.root.upgrade().is_some_and(|root| { let root = root.read(cx);
                        root.snippets_search.request_epoch.load(Ordering::Acquire) == bare.epoch
                            && root.launcher_results.generation == bare.generation && root.query.read(cx).value().as_str() == bare.query });
                    if !valid { return; }
                }
                if let Some(named) = named { view.install_named(named, window, cx); }
                else { view.focus_host(window, cx); }
                cx.notify();
            });
        }).detach();
    }
    pub(super) fn start_snippets_search(&mut self, cx: &mut Context<Self>) {
        if self.snippets_search.pending || self.search_catalog.providers.snippets_owner().is_some() { return; }
        let Some(permit) = self.restart_admission.admit() else { return; };
        self.snippets_search.pending = true;
        #[cfg(windows)]
        let path = self.app_data_root.path().join(snippets::SNIPPETS_FILE);
        #[cfg(not(windows))]
        let path = PathBuf::from(snippets::SNIPPETS_FILE);
        let wake = self.snippets_search.wake.clone();
        let notify = wake.clone();
        let task = cx.background_executor().spawn(async move {
            snippets::SnippetSearchOwner::open(path, Arc::new(move || notify.notify()))
        });
        cx.spawn(async move |weak, cx| {
            let owner = task.await;
            let _ = weak.update(cx, |root, cx| {
                if !root.snippets_search.pending { return; }
                root.snippets_search.pending = false;
                root.search_catalog.providers.install_snippets_owner(owner);
                let query = root.query.read(cx).value().to_string();
                let (effective, _) = multi_launcher::common::query::split_action_filters(&query);
                if effective.split_whitespace().next().is_some_and(|head| head.eq_ignore_ascii_case("cs")) {
                    root.refresh_launcher_results_with_viewport(query, false, cx);
                }
                cx.notify();
            });
            drop(permit);
            while wake.wait().await {
                if weak.update(cx, |root, cx| {
                    let query = root.query.read(cx).value().to_string();
                    let (effective, _) = multi_launcher::common::query::split_action_filters(&query);
                    if effective.split_whitespace().next().is_some_and(|head| head.eq_ignore_ascii_case("cs")) {
                        root.refresh_launcher_results_with_viewport(query, false, cx);
                        cx.notify();
                    }
                }).is_err() { break; }
            }
        }).detach();
    }

    pub(super) fn start_named_snippet(&mut self, alias: String, query: String, generation: u64,
        snapshot: snippets::SnippetSearchSnapshot, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) || !window.is_window_active()
            || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            || self.snippets_view.as_ref().is_some_and(|view| {
                let view = view.read(cx);
                view.loading || view.pending || view.rehosting || view.preview_pending || view.preview_child.is_some() || view.edit.is_some() || view.removal.is_some()
            }) { return; }
        let Some(owner) = self.search_catalog.providers.snippets_owner() else { return; };
        if !search_snapshot_current(&owner, &snapshot) { return; }
        let Some(host) = self.root_window else { return; };
        let Some(native) = self.native_window else { return; };
        if host.window_id() != window.window_handle().window_id() || current_foreground_identity() != Some(native.identity) { return; }
        let Some(permit) = self.restart_admission.admit() else { return; };
        let admission = self.restart_admission.clone();
        let visibility = self.root_visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let epoch = self.snippets_search.request_epoch.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        let authority = self.snippets_search.request_epoch.clone();
        let worker_authority = authority.clone();
        let worker_owner = owner.clone();
        let worker_snapshot = snapshot.clone();
        let worker_alias = alias.clone();
        let Some(focus) = window.focused(cx) else { return; };
        let source_lease = Arc::new(AtomicBool::new(true));
        let worker_lease = source_lease.clone();
        let blur_lease = source_lease.clone();
        let focus_guard = cx.on_blur(&focus, window, move |_, _, _| blur_lease.store(false, Ordering::Release));
        #[cfg(windows)]
        let path = self.app_data_root.path().join(snippets::SNIPPETS_FILE);
        #[cfg(not(windows))]
        let path = PathBuf::from(snippets::SNIPPETS_FILE);
        let task = cx.background_executor().spawn(async move {
            let current = || worker_lease.load(Ordering::Acquire) && worker_authority.load(Ordering::Acquire) == epoch && !admission.is_fenced()
                && search_snapshot_current(&worker_owner, &worker_snapshot)
                && json_utility::native_host_current(&visibility, revision, Some(native), false)
                && current_foreground_identity() == Some(native.identity);
            if !current() { return None; }
            let rows = match snippets::load_snippets_for_reload(&path) {
                Ok(LoadState::Loaded(rows)) => rows,
                _ => return None,
            };
            if !current() { return None; }
            let index = exact_named_index(&rows, &worker_snapshot.entries, &worker_alias)?;
            Some((rows, index, path))
        });
        let root = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let _permit = permit;
            let loaded = task.await;
            let _ = host.update(cx, |_, window, app| {
                let _ = root.update(app, |root, cx| {
                    let valid = authority.load(Ordering::Acquire) == epoch
                        && !root.restart_admission.is_fenced()
                        && root.enabled_plugins.as_ref().is_none_or(|enabled| enabled.contains("snippets"))
                        && multi_launcher::settings::provider_search_capability_enabled(root.enabled_capabilities.as_ref(), "snippets")
                        && root.launcher_results.generation == generation
                        && root.query.read(cx).value().as_str() == query
                        && root.root_window.is_some_and(|current| current.window_id() == host.window_id())
                        && root.native_window == Some(native) && window.is_window_active()
                        && !window.has_active_dialog(cx) && root.external_owner_is_current(Some(&focus), window, cx)
                        && source_lease.load(Ordering::Acquire)
                        && json_utility::native_host_current(&root.root_visibility, revision, Some(native), false)
                        && current_foreground_identity() == Some(native.identity) && search_snapshot_current(&owner, &snapshot);
                    if !valid { return; }
                    let Some((rows, index, path)) = loaded else {
                        root.launcher_search_error = Some("Snippet is unavailable or changed. Refresh and try again.".into()); cx.notify(); return;
                    };
                    if root.snippets_popup.is_some() {
                        root.activate_snippets_popup(Some(NamedOpen { rows, index, snapshot: snapshot.clone(), owner: owner.clone(), query: query.clone(), generation, epoch, focus, lease: source_lease, _focus_guard: focus_guard }), window, cx);
                        return;
                    }
                    let weak = cx.entity().downgrade();
                    let admission = root.restart_admission.clone();
                    let view = root.snippets_view.as_ref().filter(|view| view.read(cx).open).cloned().unwrap_or_else(|| cx.new(|cx| SnippetsView::new(window, cx, weak, Some(native), admission, path)));
                    let prefers_popout = multi_launcher::settings::UtilityToolKey::parse("snippets").ok().is_some_and(|key|
                        root.utility_presentations.preference(key) == Some(multi_launcher::settings::RememberedPresentation::Popout));
                    let opened = view.update(cx, |view, cx| {
                        if view.loading || view.pending || view.edit.is_some() || view.removal.is_some() { return false; }
                        if !view.mount.is_current() { view.mount = view.owner.attach(); }
                        view.open = true; view.host = window.window_handle(); view.native = Some(native); view.popout = false;
                        if prefers_popout {
                            view.pending_named = Some(NamedOpen { rows, index, snapshot: snapshot.clone(), owner: owner.clone(), query: query.clone(), generation, epoch, focus, lease: source_lease, _focus_guard: focus_guard });
                            return true;
                        }
                        view.rows = Arc::new(rows);
                        view.generation = view.generation.wrapping_add(1); view.diagnostic = None;
                        view.selected = Some((view.generation, index));
                        view.begin_edit(view.generation, index, window, cx);
                        view.edit.is_some()
                    });
                    if opened {
                        root.snippets_view = Some(view.clone());
                        if prefers_popout {
                            let weak = view.downgrade();
                            window.defer(cx, move |window, app| { let _ = weak.update(app, |view, cx| {
                                view.request_rehost(true, window, cx);
                                if !view.rehosting && view.pending_named.take().is_some() {
                                    view.feedback = Some("Could not open the snippet in its window. Try again.".into());
                                    cx.notify();
                                }
                            }); });
                        }
                        let id = window.window_handle().window_id();
                        let history = root.clipboard_history.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
                        let modify = root.clipboard_modify_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
                        root.root_scroll.scroll_to_top_of_item(root.clipboard_scroll_item(id, cx) + usize::from(history) + usize::from(modify));
                        cx.notify();
                    }
                });
            });
        }).detach();
    }

    pub(super) fn open_snippets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced() { return; }
        if self.snippets_popup.is_some() { self.activate_snippets_popup(None, window, cx); return; }
        #[cfg(windows)]
        let path = self.app_data_root.path().join(snippets::SNIPPETS_FILE);
        #[cfg(not(windows))]
        let path = PathBuf::from(snippets::SNIPPETS_FILE);
        let root = cx.entity().downgrade();
        let native = self.native_window;
        let admission = self.restart_admission.clone();
        let view = self.snippets_view.as_ref().filter(|view| view.read(cx).open).cloned().unwrap_or_else(|| cx.new(|cx| SnippetsView::new(window, cx, root, native, admission, path)));
        let was_open = view.read(cx).open;
        let prefers_popout = !was_open && multi_launcher::settings::UtilityToolKey::parse("snippets").ok().is_some_and(|key|
            self.utility_presentations.preference(key) == Some(multi_launcher::settings::RememberedPresentation::Popout));
        view.update(cx, |view, cx| view.open(native, !prefers_popout, window, cx));
        self.snippets_view = Some(view);
        if !was_open {
            if prefers_popout {
                if let (Some(view), Some(focus)) = (&self.snippets_view, window.focused(cx)) {
                    let lease = Arc::new(AtomicBool::new(true));
                    let blur = lease.clone();
                    let guard = cx.on_blur(&focus, window, move |_, _, _| blur.store(false, Ordering::Release));
                    let query = self.query.read(cx).value().to_string();
                    let generation = self.launcher_results.generation;
                    let epoch = self.snippets_search.request_epoch.load(Ordering::Acquire);
                    view.update(cx, |view, _| { view.cold_popout = true; view.cold_origin = Some(ColdOpen { focus, lease, query, generation, epoch, _focus_guard: guard }); });
                }
            }
        }
        let host = window.window_handle().window_id();
        let history = self.clipboard_history.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(host, false));
        let modify = self.clipboard_modify_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(host, false));
        self.root_scroll.scroll_to_top_of_item(self.clipboard_scroll_item(host, cx) + usize::from(history) + usize::from(modify));
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use multi_launcher::plugins::snippets::SnippetEntry;
    #[test]
    fn rehost_allows_idle_draft_and_rejects_active_operations() {
        assert!(super::rehost_allowed(true, false, false, false));
        assert!(!super::rehost_allowed(true, false, false, true));
        assert!(!super::rehost_allowed(true, true, false, false));
        assert!(!super::rehost_allowed(true, false, true, false));
    }
    fn entry(alias: &str, text: &str) -> SnippetEntry {
        SnippetEntry { alias: alias.into(), text: text.into(), hide_contents: false, prompt_for_fields: false, fields: Vec::new(), tags: Vec::new() }
    }
    #[test]
    fn preview_return_rejects_replaced_parent_and_editor() {
        let owner = super::utility_mount::MountOwner::default();
        let first = owner.attach();
        assert!(super::preview_return_is_current(&first, &first, true, true, 7, Some(7), true));
        assert!(!super::preview_return_is_current(&first, &first, true, true, 7, Some(8), true));
        assert!(!super::preview_return_is_current(&first, &first, true, true, 7, Some(7), false));
        let replacement = owner.attach();
        assert!(!super::preview_return_is_current(&first, &replacement, true, true, 7, Some(7), true));
    }
    #[test]
    fn preview_return_requires_the_installed_child_identity() {
        assert!(super::preview_child_is_current(Some(3), 3));
        assert!(!super::preview_child_is_current(Some(4), 3));
        assert!(!super::preview_child_is_current(None, 3));
    }
    #[test]
    fn unchanged_single_line_projection_preserves_legacy_alias() {
        assert_eq!(super::edited_alias("line\r\nbreak", "line break", "line break"), "line\r\nbreak");
        assert_eq!(super::edited_alias("line\r\nbreak", "line break", "new alias"), "new alias");
    }
    #[test]
    fn tag_draft_preserves_comma_inside_one_tag() {
        assert_eq!(multi_launcher::plugins::snippets::normalize_snippet_tags(vec!["alpha,beta".into(), "work".into()]).unwrap(), vec!["alpha,beta", "work"]);
    }
    #[test]
    fn removal_receipt_targets_exact_duplicate_occurrence_and_expires() {
        let rows = Arc::new(vec![entry("same", "first"), entry("same", "second"), entry("third", "last")]);
        let review = super::RemovalReview { snapshot: rows.clone(), generation: 3, index: 1, entry: rows[1].clone(), instance: 7 };
        let candidate = super::removal_candidate(&review, &rows, 3, 7).unwrap();
        assert_eq!(candidate, vec![rows[0].clone(), rows[2].clone()]);
        assert!(super::removal_candidate(&review, &rows, 3, 8).is_none());
        assert!(super::removal_candidate(&review, &rows, 4, 7).is_none());
        assert!(super::removal_candidate(&review, &Arc::new(rows.as_ref().clone()), 3, 7).is_none());
    }
    #[test]
    fn new_snippet_is_blank_and_duplicate_add_is_rejected() {
        let mut draft = super::new_snippet();
        assert!(draft.alias.is_empty() && draft.text.is_empty() && draft.tags.is_empty());
        assert!(!draft.hide_contents && !draft.prompt_for_fields && draft.fields.is_empty());
        let rows = vec![entry("Exact", "original")];
        draft.alias = "Exact".into();
        let mut candidate = rows.clone();
        candidate.push(draft);
        assert_eq!(multi_launcher::plugins::snippets::snapshot_edit_decision(&rows, &rows, &candidate, Some((None, "Exact"))), Err(multi_launcher::plugins::snippets::SnapshotRejection::DuplicateAlias));
        let mut changed = rows.clone(); changed.push(entry("new", "external"));
        assert_eq!(multi_launcher::plugins::snippets::snapshot_edit_decision(&changed, &rows, &candidate, Some((None, "Exact"))), Err(multi_launcher::plugins::snippets::SnapshotRejection::StaleSnapshot));
    }
    #[test]
    fn named_open_requires_unique_exact_alias_and_complete_snapshot() {
        let rows = vec![entry("Case", "one"), entry("other", "two")];
        assert_eq!(super::exact_named_index(&rows, &rows, "Case"), Some(0));
        assert_eq!(super::exact_named_index(&rows, &rows, "case"), None);
        let mut changed = rows.clone(); changed[1].text = "external".into();
        assert_eq!(super::exact_named_index(&changed, &rows, "Case"), None);
        let mut duplicate = rows.clone(); duplicate.push(entry("Case", "two"));
        assert_eq!(super::exact_named_index(&duplicate, &duplicate, "Case"), None);
    }
}
