//! Retained profile-backed Todo view.
use super::*;
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::todo::{CheckedTodoOwner, TodoSortMode, sorted_todo_indices};

#[cfg(windows)]
fn current_visible_revision(
    visibility: &RootVisibility,
    revision: u64,
    native: NativeWindow,
) -> bool {
    visibility.is_current_visible_revision(revision, native)
}
#[cfg(not(windows))]
fn current_visible_revision(visibility: &RootVisibility, revision: u64, _: NativeWindow) -> bool {
    visibility.active.load(Ordering::Acquire)
        && visibility.visible.load(Ordering::Acquire)
        && visibility.request_revision.load(Ordering::Acquire) == revision
}

pub(super) fn current_provider(
    query: &str,
    action: &Action,
    enabled: Option<&HashSet<String>>,
    caps: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    enabled.is_none_or(|set| set.contains("todo"))
        && multi_launcher::settings::provider_search_capability_enabled(caps, "todo")
        && search_providers::search_routed(
            [&search_providers::TodoViewBare as &dyn Plugin],
            query,
            enabled,
            caps,
        )
        .iter()
        .any(|(name, row)| name == "todo" && row == action)
}

pub(super) struct TodoView {
    pub open: bool,
    root: WeakEntity<RootWindow>,
    host: AnyWindowHandle,
    owner: Arc<Mutex<Option<Result<Arc<CheckedTodoOwner>, String>>>>,
    mount_owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    focus: FocusHandle,
    filter: Entity<InputState>,
    _filter_subscription: Subscription,
    sort: TodoSortMode,
    busy: bool,
    feedback: Option<String>,
    focus_guard: Option<Subscription>,
    activation_guard: Option<Subscription>,
}

impl TodoView {
    fn new(
        root: WeakEntity<RootWindow>,
        owner: Arc<Mutex<Option<Result<Arc<CheckedTodoOwner>, String>>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mount_owner = utility_mount::MountOwner::default();
        let mount = mount_owner.attach();
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter todos or #tag"));
        let subscription = cx.subscribe(&filter, |_: &mut Self, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        Self {
            open: true,
            root,
            host: window.window_handle(),
            owner,
            mount_owner,
            mount,
            focus: cx.focus_handle(),
            filter,
            _filter_subscription: subscription,
            sort: TodoSortMode::Priority,
            busy: false,
            feedback: None,
            focus_guard: None,
            activation_guard: None,
        }
    }
    fn current(&self, window: &Window, cx: &App) -> bool {
        self.open
            && self.mount.is_current()
            && self.host.window_id() == window.window_handle().window_id()
            && self.root.upgrade().is_some_and(|root| {
                let root = root.read(cx);
                !root.restart_admission.is_fenced()
                    && root.action_sheet.is_none()
                    && root.native_window.is_some_and(|native| {
                        current_visible_revision(
                            &root.root_visibility,
                            root.root_visibility
                                .request_revision
                                .load(Ordering::Acquire),
                            native,
                        ) && current_foreground_identity() == Some(native.identity)
                    })
            })
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.current(window, cx) || window.has_active_dialog(cx) {
            return;
        }
        let focused = self.focus.is_focused(window) || self.focus.contains_focused(window, cx);
        self.open = false;
        self.mount_owner.retire();
        self.focus_guard = None;
        self.activation_guard = None;
        let _ = self.root.update(cx, |root, cx| {
            if focused
                && window.is_window_active()
                && root
                    .native_window
                    .is_some_and(|native| current_foreground_identity() == Some(native.identity))
            {
                root.query.focus_handle(cx).focus(window, cx);
            }
            cx.notify();
        });
        cx.notify();
    }
    fn toggle(
        &mut self,
        snapshot: Arc<multi_launcher::plugins::todo::CheckedTodoSnapshot>,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || !self.current(window, cx) || window.has_active_dialog(cx) {
            return;
        }
        let Some(Ok(owner)) = self.owner.lock().ok().and_then(|slot| slot.clone()) else {
            return;
        };
        let Some(receipt) = owner.receipt(&snapshot, &action) else {
            self.feedback = Some("Todo selection changed; refresh the list.".into());
            cx.notify();
            return;
        };
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let root = root.read(cx);
        let Some(native) = root.native_window else {
            return;
        };
        let Some(permit) = root.restart_admission.admit() else {
            return;
        };
        let admission = root.restart_admission.clone();
        let visibility = root.root_visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        self.focus.focus(window, cx);
        let lease = Arc::new(AtomicBool::new(true));
        let blur = lease.clone();
        self.focus_guard = Some(cx.on_focus_out(&self.focus, window, move |_, _, _, _| {
            blur.store(false, Ordering::Release);
        }));
        let active = lease.clone();
        self.activation_guard = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() {
                active.store(false, Ordering::Release);
            }
        }));
        self.busy = true;
        self.feedback = None;
        let mount = self.mount.clone();
        let worker_mount = mount.clone();
        let task = cx.background_executor().spawn(async move {
            let _permit = permit;
            owner.toggle(&receipt, || {
                lease.load(Ordering::Acquire)
                    && worker_mount.is_current()
                    && !admission.is_fenced()
                    && current_visible_revision(&visibility, revision, native)
                    && current_foreground_identity() == Some(native.identity)
            })
        });
        let host = self.host;
        cx.spawn(async move |weak, cx| {
            let outcome = task.await;
            if let Some(view) = weak.upgrade() {
                let _ = update_kit_window_content(host, &view, cx, |view, _, cx| {
                    view.busy = false;
                    view.focus_guard = None;
                    view.activation_guard = None;
                    if mount.is_current() && mount.matches(&view.mount) {
                        if let Err(error) = outcome {
                            view.feedback = Some(error.to_string());
                        }
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        cx.notify();
    }
}

impl Render for TodoView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.owner.lock().ok().and_then(|slot| slot.clone());
        let close = self.mount.clone();
        let sort = self.mount.clone();
        let escape = self.mount.clone();
        let mut panel = div()
            .id("todo-view")
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .capture_action(cx.listener(
                move |view, _: &gpui_kit::base::input::Escape, window, cx| {
                    if escape.is_current()
                        && escape.matches(&view.mount)
                        && view.current(window, cx)
                        && !window.has_active_dialog(cx)
                        && (view.focus.is_focused(window)
                            || view.focus.contains_focused(window, cx))
                    {
                        view.close(window, cx);
                        cx.stop_propagation();
                    }
                },
            ))
            .child(Label::new("Todos"))
            .child(Input::new(&self.filter).aria_label("Filter todos"))
            .child(
                Button::new("todo-sort")
                    .label(match self.sort {
                        TodoSortMode::Priority => "Sort: Priority",
                        TodoSortMode::Name => "Sort: Name",
                    })
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if sort.is_current()
                            && sort.matches(&view.mount)
                            && view.current(window, cx)
                        {
                            view.sort = match view.sort {
                                TodoSortMode::Priority => TodoSortMode::Name,
                                TodoSortMode::Name => TodoSortMode::Priority,
                            };
                            cx.notify();
                        }
                    })),
            )
            .child(
                Button::new("todo-close")
                    .label("Close")
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if close.is_current() && close.matches(&view.mount) {
                            view.close(window, cx);
                        }
                    })),
            );
        let owner = match state {
            Some(Ok(owner)) => owner,
            Some(Err(error)) => {
                return panel.child(Label::new(format!("Could not load Todos: {error}")));
            }
            None => return panel.child(Label::new("Loading Todos…")),
        };
        let snapshot = owner.snapshot();
        let indices =
            sorted_todo_indices(&snapshot.entries, &self.filter.read(cx).value(), self.sort);
        if let Some(status) = &snapshot.status {
            panel = panel.child(Label::new(status.clone()));
        }
        if let Some(status) = &self.feedback {
            panel = panel.child(Label::new(status.clone()));
        }
        let mut rows = div()
            .id("todo-rows")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(360.))
            .overflow_y_scroll();
        for index in indices {
            let entry = &snapshot.entries[index];
            let action = Action {
                label: format!("{} {}", if entry.done { "[x]" } else { "[ ]" }, entry.text),
                desc: "Todo".into(),
                action: format!("todo:done:{index}"),
                args: None,
            };
            let allowed = !self.busy && owner.receipt(&snapshot, &action).is_some();
            let row_snapshot = snapshot.clone();
            let mount = self.mount.clone();
            let weak = cx.entity().downgrade();
            let done = entry.done;
            rows = rows.child(
                div()
                    .id(SharedString::from(format!(
                        "todo-row-{}-{index}",
                        snapshot.generation
                    )))
                    .flex()
                    .gap_2()
                    .child(
                        Checkbox::new(SharedString::from(format!(
                            "todo-toggle-{}-{index}",
                            snapshot.generation
                        )))
                        .accessibility_label(format!("Complete {}", entry.text))
                        .checked(entry.done)
                        .disabled(!allowed)
                        .on_change(move |state, _, window, app| {
                            let _ = weak.update(app, |view, cx| {
                                if (state == CheckboxState::Checked) != done
                                    && mount.is_current()
                                    && mount.matches(&view.mount)
                                {
                                    view.toggle(row_snapshot.clone(), action.clone(), window, cx);
                                }
                            });
                        })
                        .child(CheckboxIndicator::new().checked(done)),
                    )
                    .child(Label::new(format!(
                        "{}  ·  Priority {}  {}",
                        entry.text,
                        entry.priority,
                        entry.tags.join(" ")
                    ))),
            );
        }
        panel.child(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::current_provider;
    use std::collections::HashSet;

    #[test]
    fn view_route_requires_exact_stock_action_and_direct_search() {
        let action = multi_launcher::plugins::todo::todo_view_action();
        assert!(current_provider("ToDo view", &action, None, None));
        assert!(!current_provider("todo list", &action, None, None));
        assert!(!current_provider(
            "todo view",
            &action,
            Some(&HashSet::new()),
            None
        ));
        let mut forged = action.clone();
        forged.label.push('!');
        assert!(!current_provider("todo view", &forged, None, None));
    }
}

impl RootWindow {
    pub(super) fn start_todo_wake(&mut self, cx: &mut Context<Self>) {
        let wake = self.todo_wake.clone();
        cx.spawn(async move |root, cx| {
            while wake.wait().await {
                if root
                    .update(cx, |root, cx| {
                        if let Some(view) = &root.todo_view {
                            view.update(cx, |_, cx| cx.notify());
                        }
                        let query = root.query.read(cx).value().to_string();
                        if omni_search::is_omni_query(&query)
                            && root
                                .enabled_plugins
                                .as_ref()
                                .is_none_or(|enabled| enabled.contains("omni_search"))
                            && multi_launcher::settings::provider_search_capability_enabled(
                                root.enabled_capabilities.as_ref(),
                                "omni_search",
                            )
                            && root.search_catalog.providers.omni.settings().include_todos
                            && !root.external_launch.defer_notes_wake_for_omni()
                        {
                            root.refresh_launcher_results_with_viewport(query, false, cx);
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }
    pub(super) fn open_todo_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced() || window.has_active_dialog(cx) {
            return;
        }
        if let Some(view) = &self.todo_view {
            view.update(cx, |view, cx| {
                view.mount = view.mount_owner.attach();
                view.open = true;
                view.filter.focus_handle(cx).focus(window, cx);
                cx.notify();
            });
        } else {
            let root = cx.entity().downgrade();
            let owner = self.todo_owner.clone();
            let view = cx.new(|cx| TodoView::new(root, owner, window, cx));
            view.read(cx).filter.focus_handle(cx).focus(window, cx);
            self.todo_view = Some(view);
        }
        let id = window.window_handle().window_id();
        let index = self.clipboard_scroll_item(id, cx)
            + usize::from(
                self.clipboard_history
                    .as_ref()
                    .is_some_and(|v| v.read(cx).is_mounted_in(id, false)),
            )
            + usize::from(
                self.clipboard_modify_view
                    .as_ref()
                    .is_some_and(|v| v.read(cx).is_mounted_in(id, false)),
            )
            + usize::from(self.snippets_view.as_ref().is_some_and(|v| v.read(cx).open))
            + usize::from(self.snippet_fill.as_ref().is_some_and(|v| v.read(cx).open))
            + usize::from(self.tempfile_view.as_ref().is_some_and(|v| v.read(cx).open))
            + usize::from(
                self.folder_size_view
                    .as_ref()
                    .is_some_and(|v| v.read(cx).is_mounted_in(id, false)),
            )
            + usize::from(
                self.file_lock_view
                    .as_ref()
                    .is_some_and(|v| v.read(cx).is_mounted_in(id, false)),
            )
            + usize::from(
                self.duplicate_finder_view
                    .as_ref()
                    .is_some_and(|v| v.read(cx).open),
            )
            + usize::from(
                self.quick_notes_view
                    .as_ref()
                    .is_some_and(|v| v.read(cx).open),
            )
            + usize::from(
                self.note_editor_view
                    .as_ref()
                    .is_some_and(|v| v.read(cx).open),
            )
            + usize::from(
                self.multi_manager_view
                    .as_ref()
                    .is_some_and(|v| v.read(cx).open),
            )
            + usize::from(
                self.calculation_pad_view
                    .as_ref()
                    .is_some_and(|v| v.read(cx).open),
            )
            + usize::from(self.timer_view.as_ref().is_some_and(|v| v.read(cx).open));
        self.root_scroll.scroll_to_top_of_item(index);
        cx.notify();
    }
}
