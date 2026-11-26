//! Retained saved Snippets browser backed by the canonical snippet store.
use super::*;
use gpui_kit::base::Selectable;
use multi_launcher::common::persistence::LoadState;
use multi_launcher::plugins::snippets::{self, SnippetEntry};
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
        && matches!(parse_action(action), Ok(Command::Storage(multi_launcher::commands::StorageCommand::SnippetEdit(_))))
}

fn edit_is_current(edit: &EditDraft, generation: u64, rows: &Arc<Vec<SnippetEntry>>, instance: u64) -> bool {
    edit.instance == instance && edit.generation == generation && Arc::ptr_eq(&edit.snapshot, rows)
}
fn edited_alias(original: &str, initial_display: &str, current_display: &str) -> String {
    if initial_display == current_display { original.to_owned() } else { current_display.to_owned() }
}
fn exact_named_index(rows: &[SnippetEntry], expected: &[SnippetEntry], alias: &str) -> Option<usize> {
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
    root: WeakEntity<RootWindow>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    host: AnyWindowHandle,
    path: PathBuf,
    focus: FocusHandle,
    save_focus: FocusHandle,
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
    feedback: Option<String>,
}

impl SnippetsView {
    fn new(window: &mut Window, cx: &mut Context<Self>, root: WeakEntity<RootWindow>, path: PathBuf) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter snippets"));
        let subscription = cx.subscribe(&filter, |_: &mut Self, _: Entity<InputState>, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) { cx.notify(); }
        });
        Self { open: false, root, owner, mount, host: window.window_handle(), path, focus: cx.focus_handle(), save_focus: cx.focus_handle(), filter,
            _filter_sub: subscription, selected_tags: Vec::new(), rows: Arc::new(Vec::new()), generation: 0,
            loading: false, diagnostic: None, selected: None, edit: None, removal: None,
            removal_focus: cx.focus_handle(), removal_cancel_focus: cx.focus_handle(), removal_confirm_focus: cx.focus_handle(), instance: 0, pending: false,
            authority: Arc::new(AtomicU64::new(0)), feedback: None }
    }
    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            if !self.mount.is_current() { self.mount = self.owner.attach(); }
            self.open = true;
            self.host = window.window_handle();
            self.refresh(cx);
        }
        if self.removal.is_some() { self.removal_cancel_focus.focus(window, cx); }
        else if let Some(edit) = &self.edit { edit.alias.focus_handle(cx).focus(window, cx); }
        else { self.filter.focus_handle(cx).focus(window, cx); }
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
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || self.pending || self.removal.is_some() || self.host.window_id() != window.window_handle().window_id() { return; }
        let owns_focus = self.focus.is_focused(window) || self.focus.contains_focused(window, cx);
        self.open = false;
        self.owner.retire();
        self.loading = false;
        self.authority.fetch_add(1, Ordering::AcqRel);
        self.edit = None;
        self.selected = None;
        self.selected_tags.clear();
        self.filter.update(cx, |input, cx| input.set_value("", window, cx));
        let _ = self.root.update(cx, |root, cx| {
            if owns_focus && window.is_window_active() && !window.has_active_dialog(cx)
                && root.native_window.is_some_and(|native| current_foreground_identity() == Some(native.identity)) {
                root.query.focus_handle(cx).focus(window, cx);
            }
            cx.notify();
        });
        cx.notify();
    }
    pub(super) fn busy(&self) -> bool { self.pending || self.removal.is_some() }
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
        if self.host.window_id() != window.window_handle().window_id()
            || self.root.upgrade().is_none_or(|root| root.read(cx).native_window.is_none_or(|native| current_foreground_identity() != Some(native.identity))) { return; }
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
        self.edit = Some(EditDraft { snapshot: self.rows.clone(), generation, kind, instance: self.instance,
            alias: alias.clone(), alias_initial_display, body, body_private: entry.text.clone(), tags: entry.tags.clone(), tag_input, hidden: entry.hide_contents, revealed: !entry.hide_contents });
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
        } else if let Some(body) = edit.body.take() {
            if body.focus_handle(cx).is_focused(window) { self.focus.focus(window, cx); }
            edit.body_private = body.read(cx).value().to_string();
        }
        edit.revealed = revealed;
        cx.notify();
    }

    fn save(&mut self, instance: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || self.loading || self.diagnostic.is_some() || !window.is_window_active()
            || window.has_active_dialog(cx) || !self.mount.is_current() { return; }
        let Some(edit) = &self.edit else { return; };
        if !edit_is_current(edit, self.generation, &self.rows, instance) { return; }
        let alias_display = edit.alias.read(cx).value().to_string();
        let original = match edit.kind {
            DraftKind::Existing(index) => { let Some(entry) = edit.snapshot.get(index) else { return; }; entry.clone() }
            DraftKind::New => new_snippet(),
        };
        let alias = edited_alias(&original.alias, &edit.alias_initial_display, &alias_display);
        if alias.trim().is_empty() { self.feedback = Some("An alias is required.".into()); cx.notify(); return; }
        let tags = match snippets::normalize_snippet_tags(edit.tags.clone()) {
            Ok(tags) => tags,
            Err(message) => { self.feedback = Some(message.into()); cx.notify(); return; }
        };
        let text = edit.body.as_ref().map(|body| body.read(cx).value().to_string()).unwrap_or_else(|| edit.body_private.clone());
        let mut candidate = match snippets::prepare_snippet_draft(&original, &text) {
            Ok(candidate) => candidate,
            Err(_) => { self.feedback = Some("The prompted snippet template is invalid.".into()); cx.notify(); return; }
        };
        candidate.alias = alias.clone();
        candidate.tags = tags;
        candidate.hide_contents = edit.hidden;
        let mut rows = edit.snapshot.as_ref().clone();
        let index = match edit.kind {
            DraftKind::Existing(index) => { rows[index] = candidate; Some(index) }
            DraftKind::New => { rows.push(candidate); None }
        };
        let expected = edit.snapshot.clone();
        self.submit(expected, rows, Some((index, alias)), Mutation::Save(instance), window, cx);
    }

    fn submit(&mut self, expected: Arc<Vec<SnippetEntry>>, rows: Vec<SnippetEntry>,
        alias: Option<(Option<usize>, String)>, mutation: Mutation, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || self.loading || !self.open || !window.is_window_active() || window.has_active_dialog(cx) || !self.mount.is_current() { return; }
        let Some(root) = self.root.upgrade() else { return; };
        let root = root.read(cx);
        let Some(native) = root.native_window else { return; };
        if root.root_window.is_none_or(|host| host.window_id() != window.window_handle().window_id())
            || current_foreground_identity() != Some(native.identity) { return; }
        let Some(permit) = root.restart_admission.admit() else { return; };
        let admission = root.restart_admission.clone();
        let visibility = root.root_visibility.clone();
        let ack_visibility = visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let authority = self.authority.clone();
        let ack_authority = authority.clone();
        let epoch = authority.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        let receipt = self.mount.clone();
        let worker_receipt = receipt.clone();
        let path = self.path.clone();
        let host = self.host;
        if let Some(edit) = &self.edit {
            for input in [&edit.alias, &edit.tag_input] { input.update(cx, |input, cx| input.set_readonly(true, cx)); }
            if let Some(body) = &edit.body { body.update(cx, |input, cx| input.set_readonly(true, cx)); }
        }
        self.focus.focus(window, cx);
        self.pending = true;
        self.feedback = None;
        let task = cx.background_executor().spawn(async move {
            snippets::update_snapshot_checked(path, &expected, rows, alias.as_ref().map(|(index, alias)| (*index, alias.as_str())), || {
                worker_receipt.is_current() && authority.load(Ordering::Acquire) == epoch
                    && !admission.is_fenced()
                    && json_utility::native_host_current(&visibility, revision, Some(native), false)
                    && current_foreground_identity() == Some(native.identity)
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
                if let Some(edit) = &view.edit {
                    for input in [&edit.alias, &edit.tag_input] { input.update(cx, |input, cx| input.set_readonly(false, cx)); }
                    if let Some(body) = &edit.body { body.update(cx, |input, cx| input.set_readonly(false, cx)); }
                }
                if view.removal.is_some() && owns_focus && window.is_window_active()
                    && !window.has_active_dialog(cx) && current_foreground_identity() == Some(native.identity)
                    && json_utility::native_host_current(&ack_visibility, revision, Some(native), false)
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
                if let Some(edit) = &view.edit {
                    for input in [&edit.alias, &edit.tag_input] { input.update(cx, |input, cx| input.set_readonly(false, cx)); }
                    if let Some(body) = &edit.body { body.update(cx, |input, cx| input.set_readonly(false, cx)); }
                }
                view.clear_wait(cx); cx.notify(); }); }
        }).detach();
        cx.notify();
    }
}

impl Render for SnippetsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        let mut tag_row = div().flex().flex_wrap().gap_2();
        for (index, tag) in tags.into_iter().enumerate() {
            let selected = self.selected_tags.iter().any(|value| value.to_lowercase() == tag.to_lowercase());
            let receipt = mount.clone();
            tag_row = tag_row.child(Button::new(SharedString::from(format!("snippet-filter-tag-{index}")))
                .label(tag.clone()).selected(selected).on_click(cx.listener(move |view, _, _, cx| {
                    if !receipt.is_current() || !receipt.matches(&view.mount) || !view.open || view.removal.is_some() || view.pending { return; }
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
                    .disabled(self.pending)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if !receipt.is_current() || !receipt.matches(&view.mount) || view.pending { return; }
                        if let Some(edit) = view.edit.as_mut().filter(|edit| edit.instance == instance)
                            && edit.tags.get(index) == Some(&tag)
                        { edit.tags.remove(index); cx.notify(); }
                    })));
            }
            panel = panel.child(tag_edit).child(Button::new("snippet-add-tag").label("Add tag").disabled(self.pending)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if !add_tag_mount.is_current() || !add_tag_mount.matches(&view.mount) || view.pending { return; }
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
            panel = panel.child(div().flex().gap_2()
                .child(Button::new("snippet-reveal").label(if revealed { "Hide" } else { "Reveal" }).disabled(self.pending)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if reveal_mount.is_current() && reveal_mount.matches(&view.mount) && !view.pending {
                            view.set_revealed(instance, !revealed, window, cx);
                        }
                    })))
                .child(Button::new("snippet-privacy").label(if hidden { "Hidden in previews" } else { "Visible in previews" }).disabled(self.pending)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if edit_mount.is_current() && edit_mount.matches(&view.mount) && !view.pending {
                            if let Some(edit) = view.edit.as_mut().filter(|edit| edit.instance == instance) { edit.hidden = !hidden; }
                            if !hidden { view.set_revealed(instance, false, window, cx); }
                            cx.notify();
                        }
                    })))
                .child(Button::new("snippet-save").label("Save").track_focus(&self.save_focus).disabled(self.pending || self.diagnostic.is_some())
                    .on_click(cx.listener(move |view, _, window, cx| view.save(instance, window, cx))))
                .child(Button::new("snippet-cancel-edit").label("Cancel").disabled(self.pending)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if cancel_mount.is_current() && cancel_mount.matches(&view.mount) && !view.pending
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
                view.loading || view.pending || view.edit.is_some() || view.removal.is_some()
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
        let focus = window.focused(cx);
        #[cfg(windows)]
        let path = self.app_data_root.path().join(snippets::SNIPPETS_FILE);
        #[cfg(not(windows))]
        let path = PathBuf::from(snippets::SNIPPETS_FILE);
        let task = cx.background_executor().spawn(async move {
            let current = || worker_authority.load(Ordering::Acquire) == epoch && !admission.is_fenced()
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
                        && !window.has_active_dialog(cx) && root.external_owner_is_current(focus.as_ref(), window, cx)
                        && json_utility::native_host_current(&root.root_visibility, revision, Some(native), false)
                        && current_foreground_identity() == Some(native.identity) && search_snapshot_current(&owner, &snapshot);
                    if !valid { return; }
                    let Some((rows, index, path)) = loaded else {
                        root.launcher_search_error = Some("Snippet is unavailable or changed. Refresh and try again.".into()); cx.notify(); return;
                    };
                    let weak = cx.entity().downgrade();
                    let view = root.snippets_view.clone().unwrap_or_else(|| cx.new(|cx| SnippetsView::new(window, cx, weak, path)));
                    let opened = view.update(cx, |view, cx| {
                        if view.loading || view.pending || view.edit.is_some() || view.removal.is_some() { return false; }
                        if !view.mount.is_current() { view.mount = view.owner.attach(); }
                        view.open = true; view.host = window.window_handle(); view.rows = Arc::new(rows);
                        view.generation = view.generation.wrapping_add(1); view.diagnostic = None;
                        view.selected = Some((view.generation, index));
                        view.begin_edit(view.generation, index, window, cx);
                        view.edit.is_some()
                    });
                    if opened {
                        root.snippets_view = Some(view);
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
        #[cfg(windows)]
        let path = self.app_data_root.path().join(snippets::SNIPPETS_FILE);
        #[cfg(not(windows))]
        let path = PathBuf::from(snippets::SNIPPETS_FILE);
        let root = cx.entity().downgrade();
        let view = self.snippets_view.clone().unwrap_or_else(|| cx.new(|cx| SnippetsView::new(window, cx, root, path)));
        view.update(cx, |view, cx| view.open(window, cx));
        self.snippets_view = Some(view);
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
    fn entry(alias: &str, text: &str) -> SnippetEntry {
        SnippetEntry { alias: alias.into(), text: text.into(), hide_contents: false, prompt_for_fields: false, fields: Vec::new(), tags: Vec::new() }
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
