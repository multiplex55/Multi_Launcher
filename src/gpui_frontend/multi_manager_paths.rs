//! Retained checked MultiManager source-path editor.
use super::*;
use multi_launcher::settings::{
    MultiManagerPaths, MultiManagerPathsError, MultiManagerPathsSnapshot,
};

pub(super) const WAIT: &str = "Save or Cancel MultiManager paths before closing.";

pub(super) struct PathsView {
    pub open: bool,
    pending: bool,
    uncertain: bool,
    root: WeakEntity<RootWindow>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    host: AnyWindowHandle,
    native: NativeWindow,
    visibility: RootVisibility,
    focus: FocusHandle,
    workspace: Entity<InputState>,
    bindings: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
    baseline: Option<MultiManagerPathsSnapshot>,
    status: Option<String>,
    attempt: u64,
    lease: Option<Arc<AtomicBool>>,
    blur: Option<Subscription>,
    active: Option<Subscription>,
}
impl PathsView {
    fn new(
        root: WeakEntity<RootWindow>,
        native: NativeWindow,
        visibility: RootVisibility,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let workspace = cx.new(|cx| InputState::new(window, cx).placeholder("Workspaces path"));
        let bindings = cx.new(|cx| InputState::new(window, cx).placeholder("Bindings path"));
        let subscriptions = [&workspace, &bindings]
            .into_iter()
            .map(|input| {
                cx.subscribe(input, |_: &mut Self, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                })
            })
            .collect();
        Self {
            open: true,
            pending: false,
            uncertain: false,
            root,
            owner,
            mount,
            host: window.window_handle(),
            native,
            visibility,
            focus: cx.focus_handle(),
            workspace,
            bindings,
            _subscriptions: subscriptions,
            baseline: None,
            status: None,
            attempt: 0,
            lease: None,
            blur: None,
            active: None,
        }
    }
    fn draft(&self, cx: &App) -> MultiManagerPaths {
        MultiManagerPaths {
            workspaces: self.workspace.read(cx).value().to_string(),
            bindings: self.bindings.read(cx).value().to_string(),
        }
    }
    pub(super) fn blocks_close(&self, cx: &App) -> bool {
        self.open
            && (self.pending
                || (!self.uncertain
                    && self
                        .baseline
                        .as_ref()
                        .is_some_and(|v| v.paths != self.draft(cx))))
    }
    pub(super) fn retire_for_workspace(&mut self, cx: &App) -> bool {
        if !self.open {
            return true;
        }
        if self.uncertain || self.pending || self.blocks_close(cx) {
            return false;
        }
        self.open = false;
        self.owner.retire();
        true
    }
    pub(super) fn revoke(&mut self) {
        if let Some(lease) = &self.lease {
            lease.store(false, Ordering::Release);
        }
    }
    fn current(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.open
            && self.mount.is_current()
            && self.host.window_id() == window.window_handle().window_id()
            && window.is_window_active()
            && !window.has_active_dialog(cx)
            && self.focus.contains_focused(window, cx)
            && current_foreground_identity() == Some(self.native.identity)
            && json_utility::native_host_current(
                &self.visibility,
                self.visibility.request_revision.load(Ordering::Acquire),
                Some(self.native),
                false,
            )
            && self.root.upgrade().is_some_and(|root| {
                let root = root.read(cx);
                !root.restart_admission.is_fenced()
                    && root.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            })
    }
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || self.pending {
            return;
        }
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let (receiver, permit) = {
            let root = root.read(cx);
            let Some(permit) = root.restart_admission.admit() else {
                return;
            };
            match root
                .settings_save_workers
                .begin_multi_manager_paths_read(root.settings_path.clone())
            {
                Ok(rx) => (rx, permit),
                Err(error) => {
                    self.status = Some(error);
                    cx.notify();
                    return;
                }
            }
        };
        self.pending = true;
        self.status = Some("Reading MultiManager paths…".into());
        let mount = self.mount.clone();
        let host = self.host;
        let task = cx
            .background_executor()
            .spawn(async move { receiver.recv() });
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = task.await;
            let Some(view) = weak.upgrade() else {
                return;
            };
            let fallback = mount.clone();
            let delivered = update_kit_window_content(host, &view, cx, |view, window, cx| {
                if !mount.is_current() || !mount.matches(&view.mount) || !view.pending {
                    return;
                }
                view.pending = false;
                match result {
                    Ok(Ok(snapshot)) => {
                        view.workspace.update(cx, |input, cx| {
                            input.set_value(snapshot.paths.workspaces.clone(), window, cx)
                        });
                        view.bindings.update(cx, |input, cx| {
                            input.set_value(snapshot.paths.bindings.clone(), window, cx)
                        });
                        view.baseline = Some(snapshot);
                        view.status = None;
                    }
                    Ok(Err(error)) => view.status = Some(error.to_string()),
                    Err(error) => view.status = Some(error.to_string()),
                }
                view.clear_wait(cx);
                cx.notify();
            });
            if delivered.is_err() {
                let _ = view.update(cx, |view, cx| {
                    if fallback.is_current() && fallback.matches(&view.mount) && view.pending {
                        view.pending = false;
                        view.status =
                            Some("Could not deliver the paths read; reopen the editor.".into());
                        view.clear_wait(cx);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        let _ = window;
        cx.notify();
    }
    fn clear_wait(&self, cx: &mut Context<Self>) {
        let root = self.root.clone();
        cx.spawn(async move |_, cx| {
            let _ = root.update(cx, |root, cx| {
                if matches!(
                    root.restart_status.as_deref(),
                    Some(WAIT) | Some(multi_manager::WAIT)
                ) {
                    root.restart_status = None;
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || !self.current(window, cx) {
            return;
        }
        self.open = false;
        self.owner.retire();
        self.clear_wait(cx);
        let root = self.root.clone();
        window.defer(cx, move |window, cx| {
            let _ = root.update(cx, |root, cx| {
                if window.is_window_active()
                    && current_foreground_identity() == root.native_window.map(|v| v.identity)
                {
                    root.query.focus_handle(cx).focus(window, cx);
                    root.root_scroll
                        .scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
                }
                cx.notify();
            });
        });
        cx.notify();
    }
    pub(super) fn handle_escape(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        held: bool,
    ) -> bool {
        if !self.current(window, cx) {
            return false;
        }
        if !held {
            self.cancel(window, cx);
        }
        true
    }
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending || self.uncertain || !self.current(window, cx) {
            return;
        }
        let Some(baseline) = self.baseline.clone() else {
            return;
        };
        let desired = self.draft(cx);
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let Some(focus) = window.focused(cx) else {
            return;
        };
        let (permit, workers, path, admission) = {
            let root = root.read(cx);
            if root
                .multi_manager_view
                .as_ref()
                .is_some_and(|view| view.read(cx).open || view.read(cx).blocks_close())
            {
                self.status =
                    Some("Close the MultiManager workspace before changing paths.".into());
                cx.notify();
                return;
            }
            let Some(permit) = root.restart_admission.admit() else {
                return;
            };
            (
                permit,
                root.settings_save_workers.clone(),
                root.settings_path.clone(),
                root.restart_admission.clone(),
            )
        };
        let lease = Arc::new(AtomicBool::new(true));
        let flag = lease.clone();
        self.blur = Some(cx.on_blur(&focus, window, move |_, _, _| {
            flag.store(false, Ordering::Release)
        }));
        let flag = lease.clone();
        self.active = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() {
                flag.store(false, Ordering::Release);
            }
        }));
        self.lease = Some(lease.clone());
        self.pending = true;
        self.attempt = self.attempt.wrapping_add(1);
        let attempt = self.attempt;
        let mount = self.mount.clone();
        let native = self.native;
        let visibility = self.visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let root = self.root.clone();
        let host = self.host;
        let weak = cx.entity().downgrade();
        window.defer(cx, move |window, cx| { let _ = weak.update(cx, |view, cx| {
            let allowed = view.pending && view.attempt == attempt && mount.is_current() && mount.matches(&view.mount) && view.baseline.as_ref() == Some(&baseline) && view.draft(cx) == desired && lease.load(Ordering::Acquire) && focus.is_focused(window) && view.current(window, cx) && view.visibility.request_revision.load(Ordering::Acquire) == revision;
            if !allowed { view.pending = false; view.lease = None; view.blur = None; view.active = None; view.status = Some("Save was superseded; try again.".into()); view.clear_wait(cx); cx.notify(); return; }
            let worker_mount = mount.clone(); let worker_lease = lease.clone(); let worker_visibility = visibility.clone();
            let current: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || worker_mount.is_current() && worker_lease.load(Ordering::Acquire) && !admission.is_fenced() && current_foreground_identity() == Some(native.identity) && json_utility::native_host_current(&worker_visibility, revision, Some(native), false));
            let (receiver, token) = match workers.begin_multi_manager_paths_save(path.clone(), baseline.clone(), desired.clone(), current) { Ok(v) => v, Err(error) => { view.pending = false; view.lease = None; view.blur = None; view.active = None; view.status = Some(error); view.clear_wait(cx); cx.notify(); return; } };
            let task = cx.background_executor().spawn(async move { receiver.recv() });
            cx.spawn(async move |weak, cx| { let _permit = permit; let result = task.await; workers.acknowledge_boolean_preference_save(token.id()); let Some(entity) = weak.upgrade() else { return; };
                let fallback = mount.clone();
                let delivered = update_kit_window_content(host, &entity, cx, |view, _, cx| {
                    if !mount.is_current() || !mount.matches(&view.mount) || !view.pending || view.attempt != attempt { return; }
                    view.pending = false; view.lease = None; view.blur = None; view.active = None;
                    match result {
                        Ok(Ok(ack)) if ack.snapshot.paths == desired => {
                            let adopted = root.update(cx, |root, cx| {
                                if root.settings_path != path || !root.multi_manager_paths.as_ref().is_some_and(|v| v.entity_id() == entity.entity_id()) || root.multi_manager_view.as_ref().is_some_and(|v| v.read(cx).open || v.read(cx).blocks_close()) { return false; }
                                root.search_catalog.multi_manager_settings.workspaces_path = desired.workspaces.clone(); root.search_catalog.multi_manager_settings.bindings_path = desired.bindings.clone(); root.multi_manager_view = None; cx.notify(); true
                            }).unwrap_or(false);
                            if adopted { view.baseline = Some(ack.snapshot); view.status = Some("MultiManager paths saved.".into()); } else { view.uncertain = true; view.status = Some("Paths saved; close and relaunch to apply them.".into()); }
                        }
                        Ok(Err(MultiManagerPathsError::CommittedUnverified(error))) => { view.uncertain = true; view.status = Some(format!("Paths may have been saved: {error}. Cancel and relaunch.")); }
                        Ok(Err(error)) => view.status = Some(error.to_string()),
                        Ok(Ok(_)) | Err(_) => { view.uncertain = true; view.status = Some("Save acknowledgement is uncertain. Cancel and relaunch.".into()); }
                    }
                    if view.uncertain { let _ = root.update(cx, |root, _| root.multi_manager_paths_uncertain = true); }
                    view.clear_wait(cx); cx.notify();
                });
                if delivered.is_err() { let _ = entity.update(cx, |view, cx| { if fallback.is_current() && fallback.matches(&view.mount) && view.pending && view.attempt == attempt { view.pending = false; view.uncertain = true; view.lease = None; view.blur = None; view.active = None; view.status = Some("Save settled after window delivery failed. Cancel and relaunch to reconcile.".into()); let _ = view.root.update(cx, |root, _| root.multi_manager_paths_uncertain = true); view.clear_wait(cx); cx.notify(); } }); }
            }).detach();
        }); });
        cx.notify();
    }
}
impl Render for PathsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let save = self.mount.clone();
        let cancel = self.mount.clone();
        div()
            .id("multi-manager-paths")
            .key_context("MultiManagerPathsPane")
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .p_3()
            .capture_action(
                cx.listener(|view, _: &gpui_kit::base::input::Escape, window, cx| {
                    if view.current(window, cx) {
                        view.cancel(window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .child(Label::new("MultiManager source paths"))
            .child(Label::new("Workspaces"))
            .child(
                Input::new(&self.workspace)
                    .aria_label("Workspaces path")
                    .readonly(self.pending || self.uncertain || self.baseline.is_none()),
            )
            .child(Label::new("Bindings"))
            .child(
                Input::new(&self.bindings)
                    .aria_label("Bindings path")
                    .readonly(self.pending || self.uncertain || self.baseline.is_none()),
            )
            .children(self.status.clone().map(Label::new))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("mm-paths-save")
                            .label("Save")
                            .disabled(self.pending || self.uncertain || self.baseline.is_none())
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if save.is_current() && save.matches(&view.mount) {
                                    view.save(window, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("mm-paths-cancel")
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
    pub(super) fn open_multi_manager_paths(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced()
            || self.multi_manager_paths_uncertain
            || window.has_active_dialog(cx)
            || !window.is_window_active()
            || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            || self
                .multi_manager_view
                .as_ref()
                .is_some_and(|v| v.read(cx).open || v.read(cx).blocks_close())
        {
            self.launcher_search_error = Some("Close the MultiManager workspace before changing paths; relaunch if reconciliation is required.".into());
            cx.notify();
            return;
        }
        let Some(native) = self
            .native_window
            .filter(|n| current_foreground_identity() == Some(n.identity))
        else {
            return;
        };
        if let Some(view) = &self.multi_manager_paths {
            if view.read(cx).open {
                view.update(cx, |view, cx| {
                    view.workspace.focus_handle(cx).focus(window, cx)
                });
                self.root_scroll.scroll_to_top_of_item(
                    self.multi_manager_scroll_item(window.window_handle().window_id(), cx),
                );
                return;
            }
        }
        let root = cx.entity().downgrade();
        let visibility = self.root_visibility.clone();
        let view = cx.new(|cx| PathsView::new(root, native, visibility, window, cx));
        self.multi_manager_paths = Some(view.clone());
        window.defer(cx, move |window, cx| {
            view.update(cx, |view, cx| {
                view.workspace.focus_handle(cx).focus(window, cx);
                view.load(window, cx);
            });
        });
        self.root_scroll.scroll_to_top_of_item(
            self.multi_manager_scroll_item(window.window_handle().window_id(), cx),
        );
        cx.notify();
    }
}
