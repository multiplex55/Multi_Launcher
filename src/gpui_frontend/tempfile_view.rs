//! Retained Create Temp File editor; filesystem work belongs to the shared action.
use super::*;
use multi_launcher::commands::StorageCommand;
use multi_launcher::actions::tempfiles::{CreateAndOpenOutcome, PublishedReason};

pub(super) const WAIT: &str = "Wait for Create Temp File to finish.";
static NEXT_TOAST_ID: AtomicU64 = AtomicU64::new(u64::MAX);

pub(super) fn current_provider(query: &str, action: &Action, enabled: Option<&HashSet<String>>, caps: Option<&HashMap<String, Vec<String>>>) -> bool {
    search_providers::tempfile_rows(query, enabled, caps).contains(action)
        && matches!(parse_action(action), Ok(Command::Storage(StorageCommand::TempfileDialog)))
}

#[derive(Clone)]
enum SearchReceipt { Bare(String, u64), List(String, u64) }
fn same_search(issued: Option<&SearchReceipt>, query: &str, generation: u64, list_revision: u64) -> bool {
    match issued {
        Some(SearchReceipt::Bare(raw, issued_generation)) => raw == query && *issued_generation == generation,
        Some(SearchReceipt::List(raw, revision)) => raw == query && *revision == list_revision,
        None => false,
    }
}
fn refresh_current_list(issued: Option<&SearchReceipt>, query: &str, generation: u64, list_revision: u64) -> bool {
    tempfile_search::list_query(query) && match issued {
        Some(SearchReceipt::List(raw, _)) if raw == query => same_search(issued, query, generation, list_revision),
        _ => true,
    }
}

pub(super) struct TempfileView {
    pub open: bool,
    pub pending: bool,
    root: WeakEntity<RootWindow>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    host: AnyWindowHandle,
    focus: FocusHandle,
    alias: Entity<InputState>,
    text: Entity<TextareaState>,
    feedback: Option<String>,
    focus_guard: Option<Subscription>,
    size: multi_launcher::settings::PreferredWindowSize,
}

impl TempfileView {
    fn new(root: WeakEntity<RootWindow>, size: multi_launcher::settings::PreferredWindowSize, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        Self { open: true, pending: false, root, owner, mount, host: window.window_handle(), focus: cx.focus_handle(),
            alias: cx.new(|cx| InputState::new(window, cx).placeholder("Alias")),
            text: cx.new(|cx| TextareaState::new(window, cx).auto_grow(4, 12).placeholder("Text")), feedback: None, focus_guard: None, size }
    }
    pub(super) fn restart_draft(&self, cx: &App) -> Option<(String, String)> {
        if !self.open { return None; }
        let alias = self.alias.read(cx).value().to_string();
        let text = self.text.read(cx).value().to_string();
        (!alias.is_empty() || !text.is_empty()).then_some((alias, text))
    }
    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || self.pending || self.host.window_id() != window.window_handle().window_id() { return; }
        let owns_focus = self.focus.is_focused(window) || self.focus.contains_focused(window, cx);
        self.open = false;
        self.owner.retire();
        let _ = self.root.update(cx, |root, cx| {
            if owns_focus && window.is_window_active() && !window.has_active_dialog(cx)
                && root.native_window.is_some_and(|native| current_foreground_identity() == Some(native.identity)) {
                root.query.focus_handle(cx).focus(window, cx);
            }
            cx.notify();
        });
        cx.notify();
    }
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || self.pending || !self.mount.is_current() || self.host.window_id() != window.window_handle().window_id()
            || !window.is_window_active() || window.has_active_dialog(cx)
            || !(self.focus.is_focused(window) || self.focus.contains_focused(window, cx)) { return; }
        let alias = self.alias.read(cx).value().to_string();
        if alias.trim().is_empty() { self.feedback = Some("Enter an alias.".into()); cx.notify(); return; }
        let Some(focus) = window.focused(cx) else { return; };
        let host = self.host;
        let receipt = self.mount.clone();
        let Some((permit, admission, visibility, revision, native, issued_search)) = self.root.update(cx, |root, cx| {
            let native = root.native_window?;
            if root.root_window.is_none_or(|handle| handle.window_id() != host.window_id())
                || current_foreground_identity() != Some(native.identity)
                || !root.external_owner_is_current(Some(&focus), window, cx) { return None; }
            let permit = root.restart_admission.admit()?;
            let query = root.query.read(cx).value().to_string();
            let issued_search = if tempfile_search::list_query(&query) && tempfile_search::enabled(root.enabled_plugins.as_ref(), root.enabled_capabilities.as_ref()) {
                Some(SearchReceipt::List(query, root.tempfile_search.revision()))
            } else if search_providers::tempfile_rows(&query, root.enabled_plugins.as_ref(), root.enabled_capabilities.as_ref()).iter().any(|action| action.action == "tempfile:dialog") {
                Some(SearchReceipt::Bare(query, root.launcher_results.generation))
            } else { None };
            Some((permit, root.restart_admission.clone(), root.root_visibility.clone(),
                root.root_visibility.request_revision.load(Ordering::Acquire), native, issued_search))
        }).ok().flatten() else { return; };
        let text = self.text.read(cx).value().to_string();
        let lease = Arc::new(AtomicBool::new(true));
        let focus_lease = lease.clone();
        self.focus_guard = Some(cx.on_blur(&focus, window, move |_, _, _| focus_lease.store(false, Ordering::Release)));
        let _ = self.root.update(cx, |root, _| root.external_launch.set_snippet_preview_lease(lease.clone()));
        let worker_lease = lease.clone();
        self.pending = true;
        self.feedback = None;
        self.alias.update(cx, |input, cx| input.set_readonly(true, cx));
        self.text.update(cx, |input, cx| input.set_readonly(true, cx));
        let worker_receipt = receipt.clone();
        let worker_visibility = visibility.clone();
        let task = cx.background_executor().spawn(async move {
            multi_launcher::actions::tempfiles::create_and_open_checked(&alias, &text, || {
                worker_lease.load(Ordering::Acquire) && worker_receipt.is_current() && !admission.is_fenced()
                    && json_utility::native_host_current(&worker_visibility, revision, Some(native), false)
                    && current_foreground_identity() == Some(native.identity)
            })
        });
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let outcome = task.await;
            let Some(entity) = weak.upgrade() else { return; };
            let fallback = receipt.clone();
            let fallback_lease = lease.clone();
            let ack = update_kit_window_content(host, &entity, cx, |view, window, cx| {
                    if !view.open || !receipt.is_current() || !receipt.matches(&view.mount)
                        || view.host.window_id() != window.window_handle().window_id() { return; }
                    view.pending = false;
                    view.focus_guard = None;
                    let _ = view.root.update(cx, |root, _| root.external_launch.clear_snippet_preview_lease(&lease));
                    view.alias.update(cx, |input, cx| input.set_readonly(false, cx));
                    view.text.update(cx, |input, cx| input.set_readonly(false, cx));
                    match outcome {
                        Ok(CreateAndOpenOutcome::Opened(path)) => {
                            let owns_focus = view.focus.is_focused(window) || view.focus.contains_focused(window, cx);
                            view.open = false; view.owner.retire();
                            let _ = view.root.update(cx, |root, cx| {
                                let query = root.query.read(cx).value().to_string();
                                let host_current = root.root_window.is_some_and(|current| current.window_id() == host.window_id())
                                    && root.native_window == Some(native)
                                    && json_utility::native_host_current(&visibility, revision, Some(native), false);
                                let same_bare = same_search(issued_search.as_ref(), &query, root.launcher_results.generation, root.tempfile_search.revision())
                                    && search_providers::tempfile_rows(&query, root.enabled_plugins.as_ref(), root.enabled_capabilities.as_ref()).iter().any(|action| action.action == "tempfile:dialog");
                                let live_list = root.launcher_results.query == query && tempfile_search::enabled(root.enabled_plugins.as_ref(), root.enabled_capabilities.as_ref())
                                    && refresh_current_list(issued_search.as_ref(), &query, root.launcher_results.generation, root.tempfile_search.revision());
                                if host_current && (same_bare || live_list) {
                                    if root.external_launch_preferences.enable_toasts {
                                        let name = path.file_name().unwrap_or_else(|| path.as_os_str()).to_string_lossy();
                                        root.external_notification(NEXT_TOAST_ID.fetch_sub(1, Ordering::Relaxed), 0, format!("Created {name}"), false,
                                            root.external_launch_preferences.toast_duration, window, cx);
                                    }
                                    root.refresh_launcher_results_with_viewport(query, false, cx);
                                }
                                if host_current && owns_focus && !window.has_active_dialog(cx) {
                                    root.query.focus_handle(cx).focus(window, cx);
                                }
                            });
                        }
                        Ok(CreateAndOpenOutcome::Published { path, reason }) => {
                            let detail = match reason { PublishedReason::Superseded => "opening was superseded", PublishedReason::Changed => "the file changed before opening", PublishedReason::OpenFailed => "the associated application could not be opened" };
                            view.feedback = Some(format!("File created at {} but {detail}.", path.display()));
                            let _ = view.root.update(cx, |root, cx| {
                                let query = root.query.read(cx).value().to_string();
                                if root.root_window.is_some_and(|current| current.window_id() == host.window_id())
                                    && root.native_window == Some(native)
                                    && json_utility::native_host_current(&visibility, revision, Some(native), false)
                                    && root.launcher_results.query == query
                                    && refresh_current_list(issued_search.as_ref(), &query, root.launcher_results.generation, root.tempfile_search.revision())
                                    && tempfile_search::enabled(root.enabled_plugins.as_ref(), root.enabled_capabilities.as_ref()) {
                                    root.refresh_launcher_results_with_viewport(query, false, cx);
                                }
                            });
                        }
                        Err(error) => view.feedback = Some(format!("Could not create temp file: {error}")),
                    }
                    let _ = view.root.update(cx, |root, cx| {
                        if root.restart_status.as_deref() == Some(WAIT) { root.restart_status = None; }
                        cx.notify();
                    });
                    cx.notify();
            });
            if ack.is_err() {
                let _ = entity.update(cx, |view, cx| {
                    if view.pending && fallback.is_current() && fallback.matches(&view.mount) {
                        view.pending = false;
                        view.focus_guard = None;
                        view.alias.update(cx, |input, cx| input.set_readonly(false, cx));
                        view.text.update(cx, |input, cx| input.set_readonly(false, cx));
                        view.feedback = Some("Create Temp File finished, but the window is unavailable. Check the temp directory before retrying.".into());
                        let _ = view.root.update(cx, |root, cx| {
                            root.external_launch.clear_snippet_preview_lease(&fallback_lease);
                            if root.restart_status.as_deref() == Some(WAIT) { root.restart_status = None; }
                            cx.notify();
                        });
                        cx.notify();
                    }
                });
            }
        }).detach();
        cx.notify();
    }
}

impl Render for TempfileView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let save = self.mount.clone();
        let cancel = self.mount.clone();
        let keys = self.mount.clone();
        let mut panel = div().id("tempfile-form").track_focus(&self.focus).w(px(self.size.width.max(200.0))).max_w_full().flex().flex_col().gap_2().p_3()
            .capture_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                if !keys.is_current() || !keys.matches(&view.mount) || !view.open || view.pending
                    || !(view.focus.is_focused(window) || view.focus.contains_focused(window, cx)) { return; }
                if event.keystroke.key == "escape" && event.keystroke.modifiers == Modifiers::default() {
                    window.prevent_default(); cx.stop_propagation();
                    if !event.is_held { let _ = view.root.update(cx, |root, _| root.pane_escape_latched = true); view.cancel(window, cx); }
                }
            }))
            .child(Label::new("Create Temp File"))
            .child(Label::new("Alias"))
            .child(Input::new(&self.alias).readonly(self.pending).aria_label("Alias"))
            .child(Label::new("Text"))
            .child(div().id("tempfile-text-scroll").max_h(px((self.size.height.max(150.0) - 150.0).max(40.0))).overflow_y_scroll().child(Textarea::new(&self.text).readonly(self.pending).aria_label("Text")));
        if let Some(message) = &self.feedback { panel = panel.child(Label::new(message.clone())); }
        panel.child(div().flex().gap_2()
            .child(Button::new("tempfile-save").label("Save").disabled(self.pending).on_click(cx.listener(move |view, _, window, cx| {
                if save.is_current() && save.matches(&view.mount) { view.save(window, cx); }
            })))
            .child(Button::new("tempfile-cancel").label("Cancel").disabled(self.pending).on_click(cx.listener(move |view, _, window, cx| {
                if cancel.is_current() && cancel.matches(&view.mount) { view.cancel(window, cx); }
            }))))
    }
}

impl RootWindow {
    pub(super) fn open_tempfile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced() || window.has_active_dialog(cx) || !window.is_window_active() { return; }
        if let Some(view) = self.tempfile_view.as_ref().filter(|view| view.read(cx).open) {
            if !view.read(cx).pending { view.read(cx).alias.focus_handle(cx).focus(window, cx); }
            self.reveal_tempfile(window, cx);
            cx.notify();
            return;
        }
        let key = UtilityToolKey::parse("create-tempfile").ok();
        let size = key.map(|key| {
            let mut store = self.utility_size_store.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            store.track_key(key);
            match store.preference(key) {
                multi_launcher::settings::UtilitySizePreference::Preferred(size) => size,
                _ => multi_launcher::settings::PreferredWindowSize { width: 360.0, height: 240.0 },
            }
        }).unwrap_or(multi_launcher::settings::PreferredWindowSize { width: 360.0, height: 240.0 });
        let root = cx.entity().downgrade();
        let view = cx.new(|cx| TempfileView::new(root, size, window, cx));
        view.read(cx).alias.focus_handle(cx).focus(window, cx);
        self.tempfile_view = Some(view);
        self.reveal_tempfile(window, cx);
        cx.notify();
    }

    fn reveal_tempfile(&self, window: &Window, cx: &App) {
        let id = window.window_handle().window_id();
        let history = self.clipboard_history.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let modify = self.clipboard_modify_view.as_ref().is_some_and(|view| view.read(cx).is_mounted_in(id, false));
        let snippets = self.snippets_view.as_ref().is_some_and(|view| view.read(cx).open);
        let fill = self.snippet_fill.as_ref().is_some_and(|view| view.read(cx).open);
        self.root_scroll.scroll_to_top_of_item(self.clipboard_scroll_item(id, cx)
            + usize::from(history) + usize::from(modify) + usize::from(snippets) + usize::from(fill));
    }
}

#[cfg(test)]
mod tests {
    use super::{same_search, refresh_current_list, SearchReceipt};
    #[test]
    fn success_receipt_never_refreshes_a_later_query_or_generation() {
        let issued = SearchReceipt::Bare("tmp".to_owned(), 12);
        assert!(same_search(Some(&issued), "tmp", 12, 2));
        assert!(!same_search(Some(&issued), "calc 1+1", 12, 2));
        assert!(!same_search(Some(&issued), "tmp", 13, 2));
        assert!(!same_search(None, "tmp", 12, 2));
        let list = SearchReceipt::List("tmp list café".to_owned(), 7);
        assert!(same_search(Some(&list), "tmp list café", 99, 7));
        assert!(!same_search(Some(&list), "tmp list café", 99, 8));
        assert!(refresh_current_list(Some(&list), "tmp list other", 99, 8));
        assert!(!refresh_current_list(Some(&list), "calc 1+1", 99, 8));
    }
}
