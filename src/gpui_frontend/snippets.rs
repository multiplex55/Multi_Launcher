//! Retained saved Snippets browser backed by the canonical snippet store.
use super::*;
use gpui_kit::base::Selectable;
use multi_launcher::common::persistence::LoadState;
use multi_launcher::plugins::snippets::{self, SnippetEntry};
pub(super) const SAVE_WAIT: &str = "Wait for the Snippets save to finish.";

fn edit_is_current(edit: &EditDraft, generation: u64, rows: &Arc<Vec<SnippetEntry>>, instance: u64) -> bool {
    edit.instance == instance && edit.generation == generation && Arc::ptr_eq(&edit.snapshot, rows)
}
fn edited_alias(original: &str, initial_display: &str, current_display: &str) -> String {
    if initial_display == current_display { original.to_owned() } else { current_display.to_owned() }
}

struct EditDraft {
    snapshot: Arc<Vec<SnippetEntry>>,
    generation: u64,
    index: usize,
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RestartDraft {
    snapshot: Vec<SnippetEntry>,
    index: usize,
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
            loading: false, diagnostic: None, selected: None, edit: None, instance: 0, pending: false,
            authority: Arc::new(AtomicU64::new(0)), feedback: None }
    }
    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            if !self.mount.is_current() { self.mount = self.owner.attach(); }
            self.open = true;
            self.host = window.window_handle();
            self.refresh(cx);
        }
        self.filter.focus_handle(cx).focus(window, cx);
        cx.notify();
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.open || self.loading || self.pending || self.edit.is_some() { return; }
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
        if !self.open || self.pending || self.host.window_id() != window.window_handle().window_id() { return; }
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
    pub(super) fn busy(&self) -> bool { self.pending }
    fn row_visible(&self, generation: u64, index: usize, cx: &App) -> bool {
        self.open && !self.loading && self.diagnostic.is_none() && self.generation == generation
            && self.rows.get(index).is_some_and(|entry| {
                snippets::snippet_matches_list_filter(entry, self.filter.read(cx).value().as_str(), &self.selected_tags)
            })
    }
    pub(super) fn restart_draft(&self, cx: &App) -> Option<RestartDraft> {
        self.edit.as_ref().map(|edit| RestartDraft {
            snapshot: edit.snapshot.as_ref().clone(), index: edit.index,
            alias: edit.alias.read(cx).value().to_string(),
            body: edit.body.as_ref().map(|body| body.read(cx).value().to_string()).unwrap_or_else(|| edit.body_private.clone()), tags: edit.tags.clone(),
            tag_input: edit.tag_input.read(cx).value().to_string(),
            hidden: edit.hidden, revealed: edit.revealed,
        })
    }

    fn begin_edit(&mut self, generation: u64, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.pending || self.diagnostic.is_some() || self.edit.is_some()
            || self.generation != generation || self.selected != Some((generation, index)) { return; }
        let Some(entry) = self.rows.get(index) else { return; };
        let alias = cx.new(|cx| InputState::new(window, cx).placeholder("Alias"));
        let tag_input = cx.new(|cx| InputState::new(window, cx).placeholder("Add tag"));
        let body = (!entry.hide_contents).then(|| cx.new(|cx| TextareaState::new(window, cx).auto_grow(6, 14).placeholder("Snippet text")));
        alias.update(cx, |input, cx| input.set_value(entry.alias.clone(), window, cx));
        let alias_initial_display = alias.read(cx).value().to_string();
        if let Some(body) = &body { body.update(cx, |input, cx| input.set_value(entry.text.clone(), window, cx)); }
        self.instance = self.instance.wrapping_add(1);
        self.edit = Some(EditDraft { snapshot: self.rows.clone(), generation, index, instance: self.instance,
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
        let Some(original) = edit.snapshot.get(edit.index) else { return; };
        let alias = edited_alias(&original.alias, &edit.alias_initial_display, &alias_display);
        if alias.trim().is_empty() { self.feedback = Some("An alias is required.".into()); cx.notify(); return; }
        let tags = match snippets::normalize_snippet_tags(edit.tags.clone()) {
            Ok(tags) => tags,
            Err(message) => { self.feedback = Some(message.into()); cx.notify(); return; }
        };
        let Some(entry) = edit.snapshot.get(edit.index) else { return; };
        let text = edit.body.as_ref().map(|body| body.read(cx).value().to_string()).unwrap_or_else(|| edit.body_private.clone());
        let mut candidate = match snippets::prepare_snippet_draft(entry, &text) {
            Ok(candidate) => candidate,
            Err(_) => { self.feedback = Some("The prompted snippet template is invalid.".into()); cx.notify(); return; }
        };
        candidate.alias = alias.clone();
        candidate.tags = tags;
        candidate.hide_contents = edit.hidden;
        let mut rows = edit.snapshot.as_ref().clone();
        rows[edit.index] = candidate;
        let expected = edit.snapshot.clone();
        let index = edit.index;
        let Some(root) = self.root.upgrade() else { return; };
        let root = root.read(cx);
        let Some(native) = root.native_window else { return; };
        if root.root_window.is_none_or(|host| host.window_id() != window.window_handle().window_id())
            || current_foreground_identity() != Some(native.identity) { return; }
        let Some(permit) = root.restart_admission.admit() else { return; };
        let admission = root.restart_admission.clone();
        let visibility = root.root_visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let authority = self.authority.clone();
        let epoch = authority.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        let receipt = self.mount.clone();
        let worker_receipt = receipt.clone();
        let path = self.path.clone();
        let host = self.host;
        for input in [&edit.alias, &edit.tag_input] { input.update(cx, |input, cx| input.set_readonly(true, cx)); }
        if let Some(body) = &edit.body { body.update(cx, |input, cx| input.set_readonly(true, cx)); }
        self.focus.focus(window, cx);
        self.pending = true;
        self.feedback = None;
        let task = cx.background_executor().spawn(async move {
            snippets::update_snapshot_checked(path, &expected, rows, Some((Some(index), &alias)), || {
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
                if !view.pending { return; }
                view.pending = false;
                let exact = view.open && receipt.is_current() && receipt.matches(&view.mount)
                    && view.edit.as_ref().is_some_and(|edit| edit.instance == instance);
                if !exact { cx.notify(); return; }
                match result {
                    Ok(snippets::CheckedSnapshotUpdate::Saved(rows) | snippets::CheckedSnapshotUpdate::Unchanged(rows)) => {
                        view.rows = Arc::new(rows); view.generation = view.generation.wrapping_add(1);
                        view.selected = None; view.edit = None; view.feedback = Some("Snippet saved.".into());
                        if window.is_window_active() && current_foreground_identity() == Some(native.identity)
                            && view.focus.is_focused(window) { view.filter.focus_handle(cx).focus(window, cx); }
                    }
                    Ok(snippets::CheckedSnapshotUpdate::Rejected(snippets::SnapshotRejection::DuplicateAlias)) => view.feedback = Some("An entry with this exact alias already exists.".into()),
                    Ok(snippets::CheckedSnapshotUpdate::Rejected(_) | snippets::CheckedSnapshotUpdate::DiskChanged) => {
                        view.diagnostic = Some("Snippets changed on disk. Reload before editing.".into());
                        view.feedback = Some("Snippets changed on disk. Draft retained; cancel and reload.".into());
                    }
                    Ok(snippets::CheckedSnapshotUpdate::Superseded) => view.feedback = Some("Save was superseded. Draft retained.".into()),
                    Err(error) => {
                        if matches!(error.downcast_ref::<multi_launcher::common::persistence::PersistenceError>(),
                            Some(multi_launcher::common::persistence::PersistenceError::Read { .. } | multi_launcher::common::persistence::PersistenceError::MalformedJson { .. }))
                        {
                            view.diagnostic = Some("Could not load snippets. Existing entries are shown read-only.".into());
                        }
                        view.feedback = Some("Could not save snippet. Draft retained.".into());
                    }
                }
                if let Some(edit) = &view.edit {
                    for input in [&edit.alias, &edit.tag_input] { input.update(cx, |input, cx| input.set_readonly(false, cx)); }
                    if let Some(body) = &edit.body { body.update(cx, |input, cx| input.set_readonly(false, cx)); }
                }
                let _ = view.root.update(cx, |root, cx| {
                    if root.restart_status.as_deref() == Some(SAVE_WAIT) { root.restart_status = None; cx.notify(); }
                });
                cx.notify();
            });
            if update.is_err() { let _ = view.update(cx, |view, cx| { view.pending = false; view.feedback = Some("Save result could not be delivered.".into());
                if let Some(edit) = &view.edit {
                    for input in [&edit.alias, &edit.tag_input] { input.update(cx, |input, cx| input.set_readonly(false, cx)); }
                    if let Some(body) = &edit.body { body.update(cx, |input, cx| input.set_readonly(false, cx)); }
                }
                let _ = view.root.update(cx, |root, cx| { if root.restart_status.as_deref() == Some(SAVE_WAIT) { root.restart_status = None; cx.notify(); } }); cx.notify(); }); }
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
                    && view.save_focus.is_focused(window)
                { window.prevent_default(); cx.stop_propagation(); }
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
                    if !receipt.is_current() || !receipt.matches(&view.mount) || !view.open { return; }
                    if view.selected_tags.iter().any(|value| value.to_lowercase() == tag.to_lowercase()) { view.selected_tags.retain(|value| value.to_lowercase() != tag.to_lowercase()); }
                    else { view.selected_tags.push(tag.clone()); }
                    cx.notify();
                })));
        }
        panel = panel.child(tag_row);
        if let Some(message) = &self.diagnostic { panel = panel.child(Label::new(message.clone())); }
        if let Some(message) = &self.feedback { panel = panel.child(Label::new(message.clone())); }
        if self.loading { panel = panel.child(Label::new("Loading snippets…")); }
        if let Some(edit) = &self.edit {
            let instance = edit.instance;
            let revealed = edit.revealed;
            let hidden = edit.hidden;
            let edit_mount = mount.clone();
            let reveal_mount = mount.clone();
            let cancel_mount = mount.clone();
            let add_tag_mount = mount.clone();
            panel = panel.child(Label::new("Edit snippet"))
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
                        }))));
            }
            panel = panel.child(list);
        }
        let reload = mount.clone();
        let clear = mount.clone();
        let close = mount;
        panel.child(div().flex().gap_2()
            .child(Button::new("snippet-reload").label("Reload").disabled(self.loading || self.edit.is_some() || self.pending)
                .on_click(cx.listener(move |view, _, _, cx| { if reload.is_current() && reload.matches(&view.mount) { view.refresh(cx); } })))
            .child(Button::new("snippet-clear-filter").label("Clear Filters")
                .on_click(cx.listener(move |view, _, window, cx| {
                    if clear.is_current() && clear.matches(&view.mount) && view.open {
                        view.selected_tags.clear();
                        view.filter.update(cx, |input, cx| input.set_value("", window, cx));
                        cx.notify();
                    }
                })))
            .child(Button::new("snippet-close").label("Close").disabled(self.pending)
                .on_click(cx.listener(move |view, _, window, cx| { if close.is_current() && close.matches(&view.mount) { view.close(window, cx); } }))))
    }
}

impl RootWindow {
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
    #[test]
    fn unchanged_single_line_projection_preserves_legacy_alias() {
        assert_eq!(super::edited_alias("line\r\nbreak", "line break", "line break"), "line\r\nbreak");
        assert_eq!(super::edited_alias("line\r\nbreak", "line break", "new alias"), "new alias");
    }
    #[test]
    fn tag_draft_preserves_comma_inside_one_tag() {
        assert_eq!(multi_launcher::plugins::snippets::normalize_snippet_tags(vec!["alpha,beta".into(), "work".into()]).unwrap(), vec!["alpha,beta", "work"]);
    }
}
