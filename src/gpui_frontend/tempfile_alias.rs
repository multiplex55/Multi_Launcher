//! Checked alias editor for an exact temporary-file search row.
use super::*;
use multi_launcher::actions::tempfiles::ListedTempFile;

pub(super) const WAIT: &str = "Save or Cancel the temporary file alias before closing.";

pub(super) fn admitted(
    result: &LauncherSearchResult,
    query: &str,
    path: &str,
    enabled: Option<&HashSet<String>>,
    caps: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    matches!(&result.origin, LauncherResultOrigin::Provider(name) if name == "tempfile")
        && tempfile_search::list_query(query)
        && tempfile_search::enabled(enabled, caps)
        && result.action.action == path
        && result.action.args.is_none()
}

fn blocks_alias_close(open: bool, pending: bool, committed: bool, seed: &str, draft: &str) -> bool {
    open && (pending || (!committed && seed != draft))
}

#[derive(Debug, PartialEq, Eq)]
enum Settlement {
    Saved,
    Failed(String),
}
fn classify_settlement(result: Result<(), String>) -> Settlement {
    match result {
        Ok(()) => Settlement::Saved,
        Err(error) => Settlement::Failed(error),
    }
}

pub(super) struct AliasView {
    pub open: bool,
    pub pending: bool,
    committed: bool,
    root: WeakEntity<RootWindow>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    host: AnyWindowHandle,
    native: NativeWindow,
    visibility: RootVisibility,
    visible_revision: u64,
    generation: u64,
    index: usize,
    query: String,
    action: Action,
    row: ListedTempFile,
    authority: Arc<AtomicBool>,
    lease: Option<Arc<AtomicBool>>,
    gate: Option<Arc<external_launch::EffectGate>>,
    _root_watch: Option<Subscription>,
    focus_guard: Option<Subscription>,
    active_guard: Option<Subscription>,
    focus: FocusHandle,
    input: Entity<InputState>,
    seed: String,
    error: Option<String>,
}
impl AliasView {
    fn new(
        root: WeakEntity<RootWindow>,
        native: NativeWindow,
        visibility: RootVisibility,
        generation: u64,
        index: usize,
        query: String,
        action: Action,
        row: ListedTempFile,
        authority: Arc<AtomicBool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let seed = std::path::Path::new(&action.action)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .trim_start_matches("temp_")
            .to_owned();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Alias"));
        input.update(cx, |input, cx| input.set_value(seed.clone(), window, cx));
        let visible_revision = visibility.request_revision.load(Ordering::Acquire);
        let root_watch = root
            .upgrade()
            .map(|root| cx.observe(&root, |_: &mut Self, _, cx| cx.notify()));
        Self {
            open: true,
            pending: false,
            committed: false,
            root,
            owner,
            mount,
            host: window.window_handle(),
            native,
            visibility,
            visible_revision,
            generation,
            index,
            query,
            action,
            row,
            authority,
            lease: None,
            gate: None,
            _root_watch: root_watch,
            focus_guard: None,
            active_guard: None,
            focus: cx.focus_handle(),
            input,
            seed,
            error: None,
        }
    }
    pub fn restart_draft(&self, cx: &App) -> Option<(String, String)> {
        (self.open && !self.committed)
            .then(|| (self.seed.clone(), self.input.read(cx).value().to_string()))
    }
    pub fn blocks_close(&self, cx: &App) -> bool {
        blocks_alias_close(
            self.open,
            self.pending,
            self.committed,
            &self.seed,
            self.input.read(cx).value().as_ref(),
        )
    }
    pub(super) fn revoke_operation(&mut self) {
        if let Some(gate) = &self.gate {
            gate.revoke_pending();
        }
        if let Some(lease) = &self.lease {
            lease.store(false, Ordering::Release);
        }
    }
    fn source_current(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.open
            && self.mount.is_current()
            && self.authority.load(Ordering::Acquire)
            && self.host.window_id() == window.window_handle().window_id()
            && json_utility::native_host_current(
                &self.visibility,
                self.visible_revision,
                Some(self.native),
                false,
            )
            && self.root.upgrade().is_some_and(|root| {
                let root = root.read(cx);
                !root.restart_admission.is_fenced()
                    && root.query.read(cx).value().as_ref() == self.query.as_str()
                    && root
                        .launcher_results
                        .resolve_result(self.generation, self.index, &self.query)
                        .is_some_and(|result| {
                            admitted(
                                &result,
                                &self.query,
                                &self.action.action,
                                root.enabled_plugins.as_ref(),
                                root.enabled_capabilities.as_ref(),
                            ) && result.action == self.action
                        })
                    && root
                        .tempfile_search
                        .receipt(self.generation, &self.query, &self.action)
                        .is_some()
            })
    }
    fn presentation_current(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.open
            && self.mount.is_current()
            && self.host.window_id() == window.window_handle().window_id()
            && json_utility::native_host_current(
                &self.visibility,
                self.visible_revision,
                Some(self.native),
                false,
            )
            && window.is_window_active()
            && !window.has_active_dialog(cx)
            && self.focus.contains_focused(window, cx)
            && current_foreground_identity() == Some(self.native.identity)
            && self.root.upgrade().is_some_and(|root| {
                let root = root.read(cx);
                !root.restart_admission.is_fenced()
                    && root.native_window == Some(self.native)
                    && root.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            })
    }
    fn current(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.source_current(window, cx) && self.presentation_current(window, cx)
    }
    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open
            || self.pending
            || !self.mount.is_current()
            || self.host.window_id() != window.window_handle().window_id()
        {
            return;
        }
        let focused = self.presentation_current(window, cx);
        self.open = false;
        self.owner.retire();
        if let Some(lease) = self.lease.take() {
            lease.store(false, Ordering::Release);
        }
        let _ = self.root.update(cx, |root, cx| {
            if focused
                && window.is_window_active()
                && !window.has_active_dialog(cx)
                && current_foreground_identity() == Some(self.native.identity)
            {
                root.query.focus_handle(cx).focus(window, cx);
                root.root_scroll
                    .scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
            }
            cx.notify();
        });
        cx.notify();
    }
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || self.committed || !self.current(window, cx) {
            return;
        }
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let Some(permit) = root.read(cx).restart_admission.admit() else {
            return;
        };
        let admission = root.read(cx).restart_admission.clone();
        let Some(focus) = window.focused(cx) else {
            return;
        };
        let alias = self.input.read(cx).value().to_string();
        let lease = Arc::new(AtomicBool::new(true));
        let gate = Arc::new(external_launch::EffectGate::default());
        let blur_gate = gate.clone();
        let blur = lease.clone();
        self.focus_guard = Some(cx.on_blur(&focus, window, move |_, _, _| {
            blur_gate.revoke_pending();
            blur.store(false, Ordering::Release)
        }));
        let inactive = lease.clone();
        let inactive_gate = gate.clone();
        self.active_guard = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() {
                inactive_gate.revoke_pending();
                inactive.store(false, Ordering::Release);
            }
        }));
        self.lease = Some(lease.clone());
        self.gate = Some(gate.clone());
        self.pending = true;
        self.error = None;
        let receipt = self.mount.clone();
        let worker_mount = receipt.clone();
        let authority = self.authority.clone();
        let visibility = self.visibility.clone();
        let revision = self.visible_revision;
        let native = self.native;
        let row = self.row.clone();
        let worker_lease = lease.clone();
        let task = cx.background_executor().spawn(async move {
            let current = || {
                worker_mount.is_current()
                    && authority.load(Ordering::Acquire)
                    && worker_lease.load(Ordering::Acquire)
                    && !admission.is_fenced()
                    && current_foreground_identity() == Some(native.identity)
                    && json_utility::native_host_current(&visibility, revision, Some(native), false)
            };
            multi_launcher::actions::tempfiles::alias_listed_with_effect(
                &row,
                &alias,
                &current,
                || gate.claim(&current),
            )
        });
        let host = self.host;
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = Arc::new(Mutex::new(Some(
                task.await.map_err(|error| format!("{error:#}")),
            )));
            let Some(entity) = weak.upgrade() else {
                return;
            };
            let fallback = receipt.clone();
            let delivered_result = result.clone();
            if update_kit_window_content(host, &entity, cx, |view, window, cx| {
                if !receipt.is_current() || !receipt.matches(&view.mount) || !view.pending {
                    return;
                }
                let Some(result) = delivered_result
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                else {
                    return;
                };
                view.pending = false;
                view.gate = None;
                view.focus_guard = None;
                view.active_guard = None;
                view.lease = None;
                let current = lease.load(Ordering::Acquire) && view.current(window, cx);
                match classify_settlement(result) {
                    Settlement::Saved => {
                        view.committed = true;
                        let original_query = view.query.clone();
                        if current {
                            view.open = false;
                            view.owner.retire();
                        } else {
                            view.error = Some(
                                "Alias saved. Reopen the temporary file list to continue.".into(),
                            );
                        }
                        let _ = view.root.update(cx, |root, cx| {
                            let query = root.query.read(cx).value().to_string();
                            let revision = root
                                .root_visibility
                                .request_revision
                                .load(Ordering::Acquire);
                            let visible = json_utility::native_host_current(
                                &root.root_visibility,
                                revision,
                                root.native_window,
                                false,
                            );
                            if visible
                                && !root.restart_admission.is_fenced()
                                && root.launcher_results.query == query
                                && tempfile_search::supported_query(&query)
                                && tempfile_search::enabled(
                                    root.enabled_plugins.as_ref(),
                                    root.enabled_capabilities.as_ref(),
                                )
                            {
                                root.refresh_launcher_results_with_viewport(
                                    query.clone(),
                                    false,
                                    cx,
                                );
                                if current && query == original_query {
                                    root.query.focus_handle(cx).focus(window, cx);
                                    root.root_scroll
                                        .scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
                                }
                            }
                            cx.notify();
                        });
                    }
                    Settlement::Failed(error) => {
                        view.error = Some(format!("Could not set alias: {error}"));
                    }
                }
                cx.notify();
            })
            .is_err()
            {
                let _ = entity.update(cx, |view, cx| {
                    if fallback.is_current() && fallback.matches(&view.mount) && view.pending {
                        if let Some(result) = result
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .take()
                        {
                            view.pending = false;
                            view.gate = None;
                            view.focus_guard = None;
                            view.active_guard = None;
                            view.lease = None;
                            match classify_settlement(result) {
                                Settlement::Saved => {
                                    view.committed = true;
                                    view.error = Some(
                                        "Alias saved. Reopen the temporary file list to continue."
                                            .into(),
                                    );
                                }
                                Settlement::Failed(error) => {
                                    view.error = Some(format!("Could not set alias: {error}"));
                                }
                            }
                            cx.notify();
                        }
                    }
                });
            }
        })
        .detach();
        cx.notify();
    }
}

impl Drop for AliasView {
    fn drop(&mut self) {
        self.revoke_operation();
        self.owner.retire();
    }
}

impl Render for AliasView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let stale = !self.committed && !self.source_current(window, cx);
        let mount = self.mount.clone();
        let save = mount.clone();
        let cancel = mount.clone();
        div()
            .id("tempfile-alias")
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .gap_2()
            .w(px(480.0))
            .max_w_full()
            .p_3()
            .capture_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                if mount.is_current()
                    && mount.matches(&view.mount)
                    && event.keystroke.key == "escape"
                    && event.keystroke.modifiers == Modifiers::default()
                    && view.focus.contains_focused(window, cx)
                    && !window.has_active_dialog(cx)
                    && !context_stack_has_primary(&window.context_stack(), "PopupMenu")
                    && !view.pending
                {
                    window.prevent_default();
                    cx.stop_propagation();
                    if !event.is_held {
                        let _ = view
                            .root
                            .update(cx, |root, _| root.pane_escape_latched = true);
                        view.cancel(window, cx);
                    }
                }
            }))
            .child(Label::new("Set Tempfile Alias"))
            .child(Label::new("File path"))
            .child(
                div()
                    .id("tempfile-alias-path")
                    .max_h(px(72.0))
                    .overflow_y_scroll()
                    .child(Label::new(self.action.action.clone())),
            )
            .child(Label::new("Alias"))
            .child(
                Input::new(&self.input)
                    .aria_label("Alias")
                    .readonly(self.pending || self.committed),
            )
            .children(self.error.clone().map(Label::new))
            .children(stale.then(|| {
                Label::new("This result is no longer current. Reopen the temporary file list.")
            }))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("tempfile-alias-save")
                            .label("Save")
                            .disabled(self.pending || self.committed || stale)
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if save.is_current() && save.matches(&view.mount) {
                                    view.save(window, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("tempfile-alias-cancel")
                            .label("Cancel")
                            .disabled(self.pending)
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if cancel.is_current() && cancel.matches(&view.mount) {
                                    view.cancel(window, cx);
                                }
                            })),
                    ),
            )
    }
}

impl RootWindow {
    pub(super) fn open_tempfile_alias(
        &mut self,
        generation: u64,
        index: usize,
        query: String,
        action: Action,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(result) = self
            .launcher_results
            .resolve_result(generation, index, &query)
        else {
            return;
        };
        if !admitted(
            &result,
            &query,
            &path,
            self.enabled_plugins.as_ref(),
            self.enabled_capabilities.as_ref(),
        ) || result.action != action
            || self.restart_admission.is_fenced()
        {
            return;
        }
        let Some((row, authority)) = self.tempfile_search.receipt(generation, &query, &action)
        else {
            return;
        };
        let Some(native) = self
            .native_window
            .filter(|native| current_foreground_identity() == Some(native.identity))
        else {
            return;
        };
        if self
            .tempfile_alias
            .as_ref()
            .is_some_and(|view| view.read(cx).blocks_close(cx))
        {
            return;
        }
        let root = cx.entity().downgrade();
        let visibility = self.root_visibility.clone();
        let view = cx.new(|cx| {
            AliasView::new(
                root, native, visibility, generation, index, query, action, row, authority, window,
                cx,
            )
        });
        view.update(cx, |view, cx| {
            view.input.focus_handle(cx).focus(window, cx);
        });
        self.tempfile_alias = Some(view);
        self.root_scroll.scroll_to_bottom();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Action, LauncherResultOrigin, LauncherSearchResult, Settlement, admitted,
        blocks_alias_close, classify_settlement,
    };
    use std::collections::{HashMap, HashSet};

    #[test]
    fn alias_settlement_keeps_failed_drafts_and_committed_is_not_dirty() {
        assert_eq!(
            classify_settlement(Err("collision".into())),
            Settlement::Failed("collision".into())
        );
        assert_eq!(classify_settlement(Ok(())), Settlement::Saved);
        assert!(blocks_alias_close(true, true, false, "old", "new"));
        assert!(blocks_alias_close(true, false, false, "old", "new"));
        assert!(!blocks_alias_close(true, false, true, "old", "new"));
        assert!(!blocks_alias_close(true, false, false, "old", "old"));
    }

    #[test]
    fn alias_admission_rejects_saved_forged_and_disabled_rows() {
        let action = Action {
            label: "Café.txt".into(),
            desc: "Tempfile".into(),
            action: "C:\\owned\\Café.txt".into(),
            args: None,
        };
        let mut row = LauncherSearchResult {
            action: action.clone(),
            origin: LauncherResultOrigin::Provider("tempfile".into()),
            display_label: None,
            hide_description: false,
        };
        assert!(admitted(&row, "tmp list café", &action.action, None, None));
        assert!(!admitted(&row, "tmp rm café", &action.action, None, None));
        assert!(!admitted(
            &row,
            "tmp list café",
            "C:\\other.txt",
            None,
            None
        ));
        assert!(!admitted(
            &row,
            "tmp list café",
            &action.action,
            Some(&HashSet::new()),
            None
        ));
        let caps = HashMap::from([("tempfile".to_owned(), Vec::new())]);
        assert!(!admitted(
            &row,
            "tmp list café",
            &action.action,
            None,
            Some(&caps)
        ));
        row.origin = LauncherResultOrigin::Saved;
        assert!(!admitted(&row, "tmp list café", &action.action, None, None));
        row.origin = LauncherResultOrigin::Provider("tempfile".into());
        row.action.args = Some("unexpected".into());
        assert!(!admitted(&row, "tmp list café", &action.action, None, None));
    }

    #[test]
    fn stock_registry_exposes_set_alias_for_list_row() {
        use multi_launcher::universal_actions::{
            ActionResolutionContext, ActionSurface, ActionTargetResolver,
            ActionTargetResolverContext, UniversalActionOperation, UniversalActionRegistry,
            UniversalUiIntent,
        };
        let action = Action {
            label: "Café.txt".into(),
            desc: "Tempfile".into(),
            action: "C:\\owned\\Café.txt".into(),
            args: None,
        };
        let folders = HashMap::new();
        let bookmarks = HashMap::new();
        let target = ActionTargetResolver.resolve(
            &action,
            &ActionTargetResolverContext::new(&folders, &bookmarks, &[]),
        );
        let context = ActionResolutionContext::new(ActionSurface::ActionSheet, "tmp list café");
        assert!(UniversalActionRegistry.resolve(&target, &context).iter().any(|choice| choice.is_available() && matches!(&choice.operation, UniversalActionOperation::UiIntent(UniversalUiIntent::OpenTempfileAlias { path }) if path == &action.action)));
    }
}
