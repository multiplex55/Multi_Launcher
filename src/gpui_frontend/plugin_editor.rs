//! Retained Search capability editor for installed providers.
use super::*;
use std::borrow::Cow;
use gpui_kit::KeyUpEvent;
use multi_launcher::settings::{
    PluginMembershipError, PluginMembershipRequest, PluginMembershipSnapshot,
    SearchCapabilitySaveRequest, StatelessPlugin, provider_capability_enabled,
    provider_search_capability_enabled,
};

fn receipt_is_current(
    open: bool,
    pending: bool,
    session: u64,
    request: u64,
    expected_session: u64,
    expected_request: u64,
) -> bool {
    open && pending && session == expected_session && request == expected_request
}

fn editable_membership(name: &str) -> Option<StatelessPlugin> {
    match name {
        "date_arithmetic" => Some(StatelessPlugin::DateArithmetic),
        "base_convert" => Some(StatelessPlugin::BaseConvert),
        "unit_convert" => Some(StatelessPlugin::UnitConvert),
        _ => None,
    }
}

pub(super) fn effective_membership(
    canonical: Option<&HashSet<String>>,
    pending: Option<StatelessPlugin>,
) -> Option<Cow<'_, HashSet<String>>> {
    let Some(pending) = pending else {
        return canonical.map(Cow::Borrowed);
    };
    let mut enabled = canonical.cloned().unwrap_or_else(|| {
        multi_launcher::plugin::builtin_plugin_ids()
            .iter()
            .map(|id| (*id).to_owned())
            .collect()
    });
    enabled.remove(pending.id());
    Some(Cow::Owned(enabled))
}

#[derive(Clone, Copy)]
enum PendingKind { MembershipRead, MembershipSave, SearchSave }

impl PendingKind {
    fn message(self) -> &'static str {
        match self {
            Self::MembershipRead => "Wait for the plugin membership read.",
            Self::MembershipSave => "Wait for the plugin membership save.",
            Self::SearchSave => "Wait for the plugin Search capability save.",
        }
    }
}

pub(super) struct PluginEditorView {
    pub open: bool,
    pub pending: bool,
    pending_kind: Option<PendingKind>,
    root: WeakEntity<RootWindow>,
    filter: Entity<InputState>,
    focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    return_scroll: Option<Point<Pixels>>,
    focus_pending: bool,
    escape_waiting_for_release: bool,
    _focus_subscriptions: Vec<Subscription>,
    session: u64,
    request: u64,
    status: Option<String>,
    membership: Option<Arc<PluginMembershipSnapshot>>,
    membership_lease: Option<Arc<AtomicBool>>,
    membership_blur: Option<Subscription>,
    membership_active: Option<Subscription>,
}

impl PluginEditorView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>, root: WeakEntity<RootWindow>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter plugins"));
        cx.subscribe(&filter, |_, _, _: &InputEvent, cx| cx.notify())
            .detach();
        let focus = cx.focus_handle();
        let subscriptions = vec![
            cx.on_focus_out(&focus, window, |this, _, _, _| {
                this.escape_waiting_for_release = false
            }),
            cx.observe_window_activation(window, |this, window, _| {
                if !window.is_window_active() {
                    this.escape_waiting_for_release = false;
                }
            }),
        ];
        Self {
            open: false,
            pending: false,
            pending_kind: None,
            root,
            filter,
            focus,
            return_focus: None,
            return_scroll: None,
            focus_pending: false,
            escape_waiting_for_release: false,
            _focus_subscriptions: subscriptions,
            session: 0,
            request: 0,
            status: None,
            membership: None,
            membership_lease: None,
            membership_blur: None,
            membership_active: None,
        }
    }
    pub fn show(
        &mut self,
        return_focus: Option<FocusHandle>,
        return_scroll: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open {
            self.session = self.session.wrapping_add(1).max(1);
            self.open = true;
            self.membership = None;
            self.return_focus = return_focus;
            self.return_scroll = Some(return_scroll);
        }
        self.focus_pending = true;
        let _ = window;
        cx.notify();
    }
    pub fn session(&self) -> u64 {
        self.session
    }
    pub fn focus_filter(&mut self, session: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.open && self.focus_pending && self.session == session {
            self.focus_pending = false;
            self.filter.focus_handle(cx).focus(window, cx);
        }
    }
    pub fn owns_focus(&self, window: &Window, cx: &App) -> bool {
        self.open && self.focus.contains_focused(window, cx)
    }

    pub(super) fn pending_message(&self) -> &'static str {
        self.pending_kind.map(PendingKind::message).unwrap_or("Wait for the plugin operation.")
    }

    pub(super) fn revoke_membership(&mut self) {
        if let Some(lease) = &self.membership_lease {
            lease.store(false, Ordering::Release);
        }
    }

    pub(super) fn load_membership(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || self.pending {
            return;
        }
        let root = self.root.clone();
        let owner_id = cx.entity_id();
        let session = self.session;
        let start = root.update(cx, |root, _| {
            if !root.plugin_membership_startup_safe || root.plugin_membership_uncertain { return Err("Plugin membership requires a clean startup without plugin directories; close and relaunch to reconcile.".to_owned()); }
            if !root.plugin_editor.as_ref().is_some_and(|view| view.entity_id() == owner_id) { return Err("Plugin Editor is no longer current.".to_owned()); }
            if !root.root_window.is_some_and(|host| host.window_id() == window.window_handle().window_id()) || !root.root_visibility.active.load(Ordering::Acquire) || root.restart_admission.is_fenced() { return Err("Plugin Editor window is no longer current.".into()); }
            let permit = root.restart_admission.admit().ok_or_else(|| "Application is closing.".to_owned())?;
            let receiver = root.settings_save_workers.begin_plugin_membership_read(root.settings_path.clone())?;
            Ok((receiver, permit))
        });
        let (receiver, permit) = match start {
            Ok(Ok(start)) => start,
            Ok(Err(error)) => {
                self.status = Some(error);
                cx.notify();
                return;
            }
            Err(_) => return,
        };
        self.pending = true;
        self.pending_kind = Some(PendingKind::MembershipRead);
        self.membership = None;
        self.status = Some("Reading plugin membership…".into());
        let weak = cx.entity().downgrade();
        let task = cx
            .background_executor()
            .spawn(async move { receiver.recv() });
        cx.spawn(async move |_, cx| {
            let _permit = permit; let result = task.await;
            let _ = root.update(cx, |root, cx| {
                let Some(view) = weak.upgrade() else { return; };
                if !root.plugin_editor.as_ref().is_some_and(|mounted| mounted.entity_id() == view.entity_id()) { return; }
                view.update(cx, |view, cx| {
                    if !view.open || !view.pending || view.session != session { return; }
                    view.pending = false;
                    view.pending_kind = None;
                    match result {
                        Ok(Ok(snapshot)) if snapshot.enabled == root.enabled_plugins && snapshot.plugin_dirs.as_ref().is_none_or(Vec::is_empty) => { view.membership = Some(Arc::new(snapshot)); view.status = None; }
                        Ok(Ok(_)) => view.status = Some("Plugin membership differs from the running launcher or plugin directories are configured; close and relaunch.".into()),
                        Ok(Err(error)) => view.status = Some(format!("Could not read plugin membership: {error}")),
                        Err(error) => view.status = Some(format!("Plugin membership reader ended: {error}")),
                    }
                    cx.notify();
                });
                if root.restart_status.as_deref() == Some("Wait for the plugin membership read.") { root.restart_status = None; }
            });
        }).detach();
        cx.notify();
    }

    fn toggle_membership(
        &mut self,
        plugin: StatelessPlugin,
        enabled: bool,
        reviewed: bool,
        session: u64,
        rendered_request: u64,
        rendered_filter: String,
        rendered_member: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open
            || self.pending
            || self.session != session
            || self.request != rendered_request
            || self.filter.read(cx).value().to_lowercase() != rendered_filter
            || !self.owns_focus(window, cx)
            || !window.is_window_active()
            || window.has_active_dialog(cx)
        {
            return;
        }
        let Some(baseline) = self.membership.clone() else {
            return;
        };
        if baseline
            .enabled
            .as_ref()
            .is_none_or(|set| set.contains(plugin.id()))
            != rendered_member
        {
            return;
        }
        let request = PluginMembershipRequest {
            plugin,
            enabled,
            reviewed_all_to_explicit: reviewed,
        };
        let desired = match baseline.propose(request) {
            Ok(desired) => desired,
            Err(PluginMembershipError::ReviewRequired) => {
                let weak = cx.entity().downgrade();
                let expected = baseline.clone();
                let Some(root) = self.root.upgrade() else {
                    return;
                };
                let root = root.read(cx);
                let Some(native) = root.native_window else {
                    return;
                };
                let visibility = root.root_visibility.clone();
                let visible_revision = visibility.request_revision.load(Ordering::Acquire);
                window.open_alert_dialog(cx, move |alert, _, _| { let weak = weak.clone(); let expected = expected.clone();
                    let visibility = visibility.clone();
                    let rendered_filter = rendered_filter.clone();
                    alert.title(Label::new("Use an explicit plugin list?"))
                        .description(Label::new(format!("All current built-in plugins will remain enabled except {}. Future installed plugins require explicit enabling. Plugin directories must be empty.", plugin.id())))
                        .confirm().close_button(true).keyboard(true)
                        .on_ok(move |_, window, app| weak.update(app, |view, cx| {
                            if !view.open || view.pending || view.session != session || view.request != rendered_request || view.filter.read(cx).value().to_lowercase() != rendered_filter || view.membership.as_ref() != Some(&expected)
                                || current_foreground_identity() != Some(native.identity)
                                || !json_utility::native_host_current(&visibility, visible_revision, Some(native), false) { return false; }
                            window.close_dialog(cx);
                            view.toggle_membership(plugin, enabled, true, session, rendered_request, rendered_filter.clone(), rendered_member, window, cx);
                            false
                        }).unwrap_or(false))
                });
                return;
            }
            Err(error) => {
                self.status = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let Some(focus) = window.focused(cx) else {
            return;
        };
        let lease = Arc::new(AtomicBool::new(true));
        let blur = lease.clone();
        self.membership_blur = Some(cx.on_blur(&focus, window, move |_, _, _| {
            blur.store(false, Ordering::Release)
        }));
        let inactive = lease.clone();
        self.membership_active = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() {
                inactive.store(false, Ordering::Release);
            }
        }));
        self.membership_lease = Some(lease.clone());
        self.pending = true;
        self.pending_kind = Some(PendingKind::MembershipSave);
        self.request = self.request.wrapping_add(1).max(1);
        let request_id = self.request;
        self.status = Some(format!("Saving {} Enabled…", plugin.id()));
        let root = self.root.clone();
        let owner_id = cx.entity_id();
        let start = root.update(cx, |root, cx| {
            if !root.plugin_membership_startup_safe
                || root.plugin_membership_uncertain
                || root.plugin_membership_pending.is_some()
                || root.external_launch.is_pending()
                || root.enabled_plugins != baseline.enabled
                || !root
                    .plugin_editor
                    .as_ref()
                    .is_some_and(|view| view.entity_id() == owner_id)
                || !root
                    .root_window
                    .is_some_and(|host| host.window_id() == window.window_handle().window_id())
                || !root.external_owner_is_current(Some(&focus), window, cx)
                || root.restart_admission.is_fenced()
            {
                return Err("Plugin membership changed or another action is pending.".to_owned());
            }
            let native = root
                .native_window
                .ok_or_else(|| "Launcher window is unavailable.".to_owned())?;
            let visibility = root.root_visibility.clone();
            let revision = visibility.request_revision.load(Ordering::Acquire);
            if current_foreground_identity() != Some(native.identity)
                || !json_utility::native_host_current(&visibility, revision, Some(native), false)
            {
                return Err("Launcher window is no longer current.".to_owned());
            }
            let permit = root
                .restart_admission
                .admit()
                .ok_or_else(|| "Application is closing.".to_owned())?;
            let admission = root.restart_admission.clone();
            let check_lease = lease.clone();
            let worker_visibility = visibility.clone();
            let current: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
                check_lease.load(Ordering::Acquire)
                    && !admission.is_fenced()
                    && current_foreground_identity() == Some(native.identity)
                    && json_utility::native_host_current(
                        &worker_visibility,
                        revision,
                        Some(native),
                        false,
                    )
            });
            root.plugin_membership_pending = Some(plugin);
            root.renew_plugin_membership_projection(cx);
            Ok((
                permit,
                root.settings_save_workers.clone(),
                root.settings_path.clone(),
                current,
                native,
                visibility,
                revision,
            ))
        });
        let (permit, workers, settings_path, current, native, visibility, visible_revision) =
            match start {
                Ok(Ok(start)) => start,
                Ok(Err(error)) => {
                    self.pending = false;
                    self.pending_kind = None;
                    self.membership_lease = None;
                    self.membership_blur = None;
                    self.membership_active = None;
                    self.status = Some(error);
                    cx.notify();
                    return;
                }
                Err(_) => {
                    self.pending = false;
                    self.pending_kind = None;
                    self.membership_lease = None;
                    self.membership_blur = None;
                    self.membership_active = None;
                    return;
                }
            };
        let weak = cx.entity().downgrade();
        window.defer(cx, move |window, app| {
            let _ = weak.update(app, |view, cx| {
                let root_current = root.upgrade().is_some_and(|root| {
                    let root = root.read(cx);
                    root.plugin_editor.as_ref().is_some_and(|mounted| mounted.entity_id() == owner_id)
                        && root.plugin_membership_pending == Some(plugin) && !root.plugin_membership_uncertain
                        && root.enabled_plugins == baseline.enabled && root.settings_path == settings_path
                        && !root.external_launch.is_pending() && !root.restart_admission.is_fenced()
                        && root.root_window.is_some_and(|host| host.window_id() == window.window_handle().window_id())
                        && root.native_window == Some(native) && root.external_owner_is_current(Some(&focus), window, cx)
                });
                if !receipt_is_current(view.open, view.pending, view.session, view.request, session, request_id)
                    || view.filter.read(cx).value().to_lowercase() != rendered_filter
                    || view.membership.as_ref() != Some(&baseline) || !lease.load(Ordering::Acquire)
                    || !focus.is_focused(window) || !view.owns_focus(window, cx) || !window.is_window_active()
                    || window.has_active_dialog(cx) || !root_current
                    || current_foreground_identity() != Some(native.identity)
                    || !json_utility::native_host_current(&visibility, visible_revision, Some(native), false) || !current() {
                    view.pending = false; view.pending_kind = None; view.membership_lease = None; view.membership_blur = None; view.membership_active = None;
                    view.status = Some("Plugin membership save was superseded; try again.".into());
                    let _ = root.update(cx, |root, cx| { if root.plugin_membership_pending == Some(plugin) { root.plugin_membership_pending = None; root.renew_plugin_membership_projection(cx); } if root.restart_status.as_deref() == Some(PendingKind::MembershipSave.message()) { root.restart_status = None; } });
                    cx.notify(); return;
                }
                let (receiver, token) = match workers.begin_plugin_membership_save(settings_path.clone(), (*baseline).clone(), request, current) {
                    Ok(value) => value,
                    Err(error) => { view.pending = false; view.pending_kind = None; view.membership_lease = None; view.membership_blur = None; view.membership_active = None; view.status = Some(error); let _ = root.update(cx, |root, cx| { if root.plugin_membership_pending == Some(plugin) { root.plugin_membership_pending = None; root.renew_plugin_membership_projection(cx); } if root.restart_status.as_deref() == Some(PendingKind::MembershipSave.message()) { root.restart_status = None; } }); cx.notify(); return; }
                };
                let task = cx.background_executor().spawn(async move { receiver.recv() });
                cx.spawn(async move |weak, cx| {
                    let _permit = permit;
                    let result = task.await;
                    workers.acknowledge_boolean_preference_save(token.id());
                    let _ = root.update(cx, |root, cx| {
                        let Some(view) = weak.upgrade() else { return; };
                        if !root.plugin_editor.as_ref().is_some_and(|mounted| mounted.entity_id() == view.entity_id())
                            || root.plugin_membership_pending != Some(plugin) || root.enabled_plugins != baseline.enabled
                            || root.settings_path != settings_path { return; }
                        let disposition = view.update(cx, |view, cx| {
                            if !receipt_is_current(view.open, view.pending, view.session, view.request, session, request_id)
                                || view.membership.as_ref() != Some(&baseline) { return None; }
                            view.pending = false; view.pending_kind = None; view.membership_lease = None; view.membership_blur = None; view.membership_active = None;
                            let value = match result {
                                Ok(Ok(ack)) if ack.snapshot.enabled.as_ref() == Some(&desired) && ack.snapshot.plugin_dirs == baseline.plugin_dirs => { view.membership = Some(Arc::new(ack.snapshot.clone())); view.status = Some(format!("Saved {} Enabled.", plugin.id())); Some(Ok(ack.snapshot.enabled)) }
                                Ok(Err(PluginMembershipError::CommittedUnverified(error))) => { view.membership = None; view.status = Some(format!("Membership may have changed: {error}. Close and manually relaunch.")); Some(Err(true)) }
                                Ok(Ok(_)) | Err(_) => { view.membership = None; view.status = Some("Membership acknowledgement is uncertain. Close and manually relaunch.".into()); Some(Err(true)) }
                                Ok(Err(error)) => { view.status = Some(format!("Could not save membership: {error}")); Some(Err(false)) }
                            }; cx.notify(); value
                        });
                        if let Some(disposition) = disposition {
                            if root.restart_status.as_deref() == Some(PendingKind::MembershipSave.message()) { root.restart_status = None; }
                            match disposition { Ok(enabled) => { root.enabled_plugins = enabled; root.plugin_membership_pending = None; }, Err(true) => root.plugin_membership_uncertain = true, Err(false) => root.plugin_membership_pending = None }
                            root.renew_plugin_membership_projection(cx); cx.notify();
                        }
                    });
                }).detach();
            });
        });
        cx.notify();
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending {
            self.status = Some(self.pending_message().into());
            cx.notify();
            return;
        }
        let owned_focus = self.owns_focus(window, cx);
        self.open = false;
        self.focus_pending = false;
        self.session = self.session.wrapping_add(1).max(1);
        let return_focus = self.return_focus.take();
        let return_scroll = self.return_scroll.take();
        let _ = self.root.update(cx, |root, cx| {
            if owned_focus {
                if let Some(focus) =
                    return_focus.filter(|focus| root.focus_handle.contains(focus, window))
                {
                    if let Some(offset) = return_scroll {
                        root.root_scroll.set_offset(offset);
                    }
                    focus.focus(window, cx);
                } else {
                    root.root_scroll
                        .scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
                    root.query.focus_handle(cx).focus(window, cx);
                }
            }
            cx.notify();
        });
        cx.notify();
    }
    pub(super) fn handle_escape(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        held: Option<bool>,
    ) -> bool {
        let contexts = window.context_stack();
        if !self.owns_focus(window, cx)
            || window.has_active_dialog(cx)
            || context_stack_has_primary(&contexts, "List")
            || context_stack_has_primary(&contexts, "Dialog")
            || context_stack_has_primary(&contexts, "PopupMenu")
            || context_stack_has_primary(&contexts, "AlertDialog")
        {
            return false;
        }
        if !held.unwrap_or(self.escape_waiting_for_release) {
            self.escape_waiting_for_release = true;
            self.close(window, cx);
        }
        true
    }

    fn save(
        &mut self,
        name: String,
        baseline: bool,
        desired: bool,
        rendered_session: u64,
        rendered_filter: &str,
        rendered_member: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open
            || self.pending
            || baseline == desired
            || self.session != rendered_session
            || self.filter.read(cx).value().to_lowercase() != rendered_filter
        {
            return;
        }
        let root = self.root.clone();
        let owner_id = cx.entity_id();
        let start = root.update(cx, |root, root_cx| {
            if !root
                .plugin_editor
                .as_ref()
                .is_some_and(|view| view.entity_id() == owner_id)
                || !root.root_visibility.active.load(Ordering::Acquire)
                || !root.root_visibility.visible.load(Ordering::Acquire)
                || !root
                    .root_window
                    .is_some_and(|handle| handle.window_id() == window.window_handle().window_id())
                || !window.is_window_active()
                || window.has_active_dialog(root_cx)
                || root.restart_admission.is_fenced()
                || root.action_sheet.is_some()
                || root.context_favorite_popup.is_some()
                || root.file_menu_open
                || root.settings_menu_open
                || root.help_menu_open
            {
                return Err("Plugin Editor is no longer the active window.".to_owned());
            }
            #[cfg(windows)]
            if !root.native_window.is_some_and(|native| {
                native_window_is_valid(native)
                    && current_foreground_identity() == Some(native.identity)
            }) {
                return Err("Plugin Editor is no longer the active window.".to_owned());
            }
            let mut providers = root
                .search_catalog
                .providers
                .plugins()
                .filter(|provider| provider.name() == name);
            let Some(provider) = providers.next() else {
                return Err("Plugin catalog changed. Try again.".to_owned());
            };
            if providers.next().is_some()
                || !provider.capabilities().contains(&"search")
                || !(provider.name().to_lowercase().contains(rendered_filter)
                    || provider
                        .description()
                        .to_lowercase()
                        .contains(rendered_filter))
                || root
                    .enabled_plugins
                    .as_ref()
                    .is_none_or(|plugins| plugins.contains(&name))
                    != rendered_member
                || provider_search_capability_enabled(root.enabled_capabilities.as_ref(), &name)
                    != baseline
            {
                return Err("Plugin selection changed. Try again.".to_owned());
            }
            let permit = root
                .restart_admission
                .admit()
                .ok_or_else(|| "Application is closing.".to_owned())?;
            let request = SearchCapabilitySaveRequest {
                provider: name.clone(),
                inherited_enabled_siblings: provider
                    .capabilities()
                    .iter()
                    .copied()
                    .filter(|capability| {
                        *capability != "search"
                            && provider_capability_enabled(None, &name, capability)
                    })
                    .map(str::to_owned)
                    .collect(),
                expected_baseline: baseline,
                desired,
            };
            let receiver = root
                .settings_save_workers
                .begin_search_capability_save(root.settings_path.clone(), request)?;
            Ok::<_, String>((
                receiver,
                permit,
                root.settings_save_workers.clone(),
                root.settings_path.clone(),
            ))
        });
        let (receiver, permit, workers, settings_path) = match start {
            Ok(Ok(start)) => start,
            Ok(Err(error)) => {
                self.status = Some(error);
                cx.notify();
                return;
            }
            Err(_) => return,
        };
        self.pending = true;
        self.pending_kind = Some(PendingKind::SearchSave);
        self.status = Some(format!("Saving {name} Search…"));
        self.request = self.request.wrapping_add(1).max(1);
        let (session, request) = (self.session, self.request);
        let owner = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let _permit = permit;
            let result = loop {
                match receiver.try_recv() {
                    Ok(result) => break result,
                    Err(TryRecvError::Disconnected) => {
                        break Err(multi_launcher::settings::SearchCapabilityError::Store(
                            "Settings writer ended without an acknowledgement.".to_owned(),
                        ));
                    }
                    Err(TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(20))
                            .await
                    }
                }
            };
            let refresh = if matches!(
                &result,
                Err(multi_launcher::settings::SearchCapabilityError::Conflict { .. })
            ) {
                match workers.begin_search_capability_read(settings_path, name.clone()) {
                    Ok(receiver) => loop {
                        match receiver.try_recv() {
                            Ok(result) => break result.ok(),
                            Err(TryRecvError::Disconnected) => break None,
                            Err(TryRecvError::Empty) => {
                                cx.background_executor()
                                    .timer(Duration::from_millis(20))
                                    .await
                            }
                        }
                    },
                    Err(_) => None,
                }
            } else {
                None
            };
            let _ = root.update(cx, |root, cx| {
                let Some(view) = owner.upgrade() else {
                    return;
                };
                if !root
                    .plugin_editor
                    .as_ref()
                    .is_some_and(|mounted| mounted.entity_id() == view.entity_id())
                {
                    return;
                }
                let committed = view.update(cx, |view, cx| {
                    if !receipt_is_current(
                        view.open,
                        view.pending,
                        view.session,
                        view.request,
                        session,
                        request,
                    ) {
                        return None;
                    }
                    view.pending = false;
                    view.pending_kind = None;
                    match result {
                        Ok(ack)
                            if ack.snapshot.search_enabled == desired
                                && provider_search_capability_enabled(
                                    ack.snapshot.capabilities.as_ref(),
                                    &name,
                                ) == desired =>
                        {
                            view.status = Some(format!("Saved {name} Search."));
                            cx.notify();
                            Some(ack.snapshot.capabilities)
                        }
                        Ok(_) => {
                            view.status = Some(
                                "Settings acknowledgement did not match the requested value."
                                    .into(),
                            );
                            cx.notify();
                            None
                        }
                        Err(error) => {
                            view.status = Some(format!("Could not save {name} Search: {error}"));
                            cx.notify();
                            refresh.map(|snapshot| snapshot.capabilities)
                        }
                    }
                });
                if root.restart_status.as_deref()
                    == Some("Wait for the plugin Search capability save.")
                    || root.restart_status.as_deref()
                        == Some("A plugin Search capability save is still pending.")
                {
                    root.restart_status = None;
                }
                if let Some(map) = committed {
                    root.enabled_capabilities = map;
                    if !multi_launcher::settings::provider_search_capability_enabled(
                        root.enabled_capabilities.as_ref(),
                        "MultiManager",
                    ) {
                        root.cancel_multi_manager_capture(cx);
                    }
                    let query = root.query.read(cx).value().to_string();
                    root.refresh_launcher_results_with_viewport(query, false, cx);
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for PluginEditorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.filter.read(cx).value().to_lowercase();
        let session = self.session;
        let rendered_request = self.request;
        let rows = self
            .root
            .upgrade()
            .map(|root| {
                let root = root.read(cx);
                root.search_catalog
                    .providers
                    .plugins()
                    .filter(|provider| {
                        provider.capabilities().contains(&"search")
                            && (provider.name().to_lowercase().contains(&query)
                                || provider.description().to_lowercase().contains(&query))
                    })
                    .map(|provider| {
                        let name = provider.name();
                        let enabled = provider_search_capability_enabled(
                            root.enabled_capabilities.as_ref(),
                            name,
                        );
                        let member = root
                            .enabled_plugins
                            .as_ref()
                            .is_none_or(|plugins| plugins.contains(name));
                        (name.to_owned(), enabled, member)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let palette = appearance::current(cx).palette;
        let items = rows
            .into_iter()
            .map(|(name, enabled, member)| {
                let target = name.clone();
                let rendered_filter = query.clone();
                let owner = cx.entity().downgrade();
                let id = SharedString::from(format!("plugin-editor-search-{name}"));
                let search = Checkbox::new(id)
                    .checked(enabled)
                    .disabled(self.pending)
                    .accessibility_label(format!("{name} Search"))
                    .on_change(move |state, _, window, app| {
                        let _ = owner.update(app, |view, cx| {
                            view.save(
                                target.clone(),
                                enabled,
                                state == CheckboxState::Checked,
                                session,
                                &rendered_filter,
                                member,
                                window,
                                cx,
                            )
                        });
                    })
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                CheckboxIndicator::new()
                                    .checked(enabled)
                                    .size(px(18.0))
                                    .border_1()
                                    .child(if enabled { "✓" } else { "" }),
                            )
                            .child(Label::new(format!(
                                "{name} Search · {}",
                                if member {
                                    "enabled plugin"
                                } else {
                                    "plugin disabled"
                                }
                            ))),
                    );
                let plugin = editable_membership(&name);
                let membership = self.membership.clone();
                let membership_control = plugin.map(|plugin| {
                    let target = cx.entity().downgrade();
                    let expected = membership.clone();
                    let rendered_filter = query.clone();
                    let checked = membership.as_ref().map_or(member, |snapshot| {
                        snapshot
                            .enabled
                            .as_ref()
                            .is_none_or(|set| set.contains(plugin.id()))
                    });
                    Checkbox::new(SharedString::from(format!(
                        "plugin-editor-enabled-{}",
                        plugin.id()
                    )))
                    .checked(checked)
                    .disabled(self.pending || membership.is_none())
                    .accessibility_label(format!("{} Enabled", plugin.id()))
                    .on_change(move |state, _, window, app| {
                        let _ = target.update(app, |view, cx| {
                            if view.membership == expected {
                                view.toggle_membership(
                                    plugin,
                                    state == CheckboxState::Checked,
                                    false,
                                    session,
                                    rendered_request,
                                    rendered_filter.clone(),
                                    member,
                                    window,
                                    cx,
                                );
                            }
                        });
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                CheckboxIndicator::new()
                                    .checked(checked)
                                    .size(px(18.0))
                                    .rounded(px(3.0))
                                    .border_1()
                                    .border_color(appearance::to_hsla(palette.control_border))
                                    .when(checked, |indicator| {
                                        indicator
                                            .bg(appearance::to_hsla(palette.accent))
                                            .text_color(appearance::to_hsla(palette.primary_text))
                                    })
                                    .items_center()
                                    .justify_center()
                                    .child(if checked { "✓" } else { "" }),
                            )
                            .child(Label::new("Enabled")),
                    )
                });
                div()
                    .id(SharedString::from(format!("plugin-editor-row-{name}")))
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .when_some(membership_control, |row, control| row.child(control))
                    .child(search)
            })
            .collect::<Vec<_>>();
        div()
            .id("plugin-editor-pane")
            .key_context("PluginEditorPane")
            .role(Role::Group)
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .p_3()
            .capture_action(
                cx.listener(|this, _: &gpui_kit::base::input::Escape, window, cx| {
                    if this.handle_escape(window, cx, None) {
                        cx.stop_propagation();
                    }
                }),
            )
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape"
                    && event.keystroke.modifiers == Modifiers::default()
                    && this.handle_escape(window, cx, Some(event.is_held))
                {
                    cx.stop_propagation();
                }
            }))
            .capture_key_up(cx.listener(|this, event: &KeyUpEvent, _, _| {
                if event.keystroke.key == "escape" {
                    this.escape_waiting_for_release = false;
                }
            }))
            .child(Label::new("Plugin Editor · Search capabilities"))
            .child(Input::new(&self.filter).aria_label("Filter plugins"))
            .child(
                Button::new("plugin-editor-reload-membership")
                    .label("Reload membership")
                    .disabled(self.pending)
                    .on_click(cx.listener(|view, _, window, cx| {
                        if view.owns_focus(window, cx)
                            && window.is_window_active()
                            && !window.has_active_dialog(cx)
                        {
                            view.load_membership(window, cx);
                        }
                    })),
            )
            .child(
                Button::new("plugin-editor-close")
                    .tab_stop(true)
                    .label("Close")
                    .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            )
            .when_some(self.status.clone(), |pane, status| {
                pane.child(Label::new(status))
            })
            .child(
                div()
                    .id("plugin-editor-rows")
                    .flex()
                    .flex_col()
                    .max_h(px(300.0))
                    .overflow_y_scroll()
                    .children(items),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{PendingKind, StatelessPlugin, editable_membership, effective_membership, receipt_is_current};
    use std::{borrow::Cow, collections::HashSet};
    #[test]
    fn pending_messages_follow_the_actual_operation() {
        assert_eq!(PendingKind::MembershipRead.message(), "Wait for the plugin membership read.");
        assert_eq!(PendingKind::MembershipSave.message(), "Wait for the plugin membership save.");
        assert_eq!(PendingKind::SearchSave.message(), "Wait for the plugin Search capability save.");
    }
    #[test]
    fn save_receipt_requires_current_open_session_and_request() {
        assert!(receipt_is_current(true, true, 4, 8, 4, 8));
        assert!(!receipt_is_current(false, true, 4, 8, 4, 8));
        assert!(!receipt_is_current(true, false, 4, 8, 4, 8));
        assert!(!receipt_is_current(true, true, 5, 8, 4, 8));
        assert!(!receipt_is_current(true, true, 4, 9, 4, 8));
    }

    #[test]
    fn membership_edit_admits_only_the_three_installed_stateless_ids() {
        for name in ["date_arithmetic", "base_convert", "unit_convert"] {
            assert_eq!(editable_membership(name).unwrap().id(), name);
        }
        assert!(editable_membership("timer").is_none());
        assert!(editable_membership("Date_Arithmetic").is_none());
    }

    #[test]
    fn disabled_membership_does_not_remove_installed_editor_rows() {
        let providers = super::super::search_providers::InstalledLauncherProviders::test_without_persisted_providers();
        let disabled = std::collections::HashSet::<String>::new();
        for name in ["date_arithmetic", "base_convert", "unit_convert"] {
            assert!(!disabled.contains(name));
            assert!(providers.plugins().any(|provider| provider.name() == name));
            assert!(editable_membership(name).is_some());
        }
    }

    #[test]
    fn pending_membership_omits_provider_and_cache_but_keeps_conversion_sibling() {
        let providers = super::super::search_providers::InstalledLauncherProviders::test_without_persisted_providers();
        let canonical = HashSet::from([
            "base_convert".to_owned(),
            "unit_convert".to_owned(),
            "unknown".to_owned(),
        ]);
        let effective =
            effective_membership(Some(&canonical), Some(StatelessPlugin::BaseConvert)).unwrap();
        assert!(
            effective.contains("unknown")
                && effective.contains("unit_convert")
                && !effective.contains("base_convert")
        );
        let query = "conv 1010 bin to hex";
        let before = super::super::search_providers::search_routed(
            providers.plugins(),
            query,
            Some(&canonical),
            None,
        );
        let fenced = super::super::search_providers::search_routed(
            providers.plugins(),
            query,
            Some(effective.as_ref()),
            None,
        );
        assert!(before.iter().any(|(name, _)| name == "base_convert"));
        assert!(!fenced.iter().any(|(name, _)| name == "base_convert"));
        let commands = providers.cached_commands(Some(effective.as_ref()));
        assert!(commands.iter().any(|action| action.desc == "Unit convert"));
        assert!(!commands.iter().any(|action| action.desc == "Base Convert"));
        assert!(matches!(effective_membership(Some(&canonical), None), Some(Cow::Borrowed(value)) if value == &canonical));
        let implicit = effective_membership(None, Some(StatelessPlugin::DateArithmetic)).unwrap();
        assert!(!implicit.contains("date_arithmetic") && implicit.contains("base_convert"));
    }
}
