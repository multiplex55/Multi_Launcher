//! Retained, read-only Calendar day agenda.
use super::*;
use chrono::{Duration as ChronoDuration, Local, NaiveDate, NaiveDateTime, NaiveTime};
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::calendar::{
    CheckedCalendarOwner, CheckedCalendarSnapshot, EventInstance, expand_instances,
};

type OwnerSlot = Arc<Mutex<Option<Result<Arc<CheckedCalendarOwner>, String>>>>;

fn open_stamp_current(lease: &AtomicBool, issued: u64, current: u64) -> bool {
    lease.load(Ordering::Acquire) && issued == current
}

pub(super) fn current_provider(
    query: &str,
    action: &Action,
    enabled: Option<&HashSet<String>>,
    caps: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    enabled.is_none_or(|set| set.contains("calendar"))
        && multi_launcher::settings::provider_search_capability_enabled(caps, "calendar")
        && search_providers::search_routed(
            [&search_providers::CalendarBare as &dyn Plugin],
            query,
            enabled,
            caps,
        )
        .iter()
        .any(|(name, row)| name == "calendar" && row == action)
}

fn same_occurrence(a: &EventInstance, b: &EventInstance) -> bool {
    a.instance_id == b.instance_id
        && a.source_event_id == b.source_event_id
        && a.start == b.start
        && a.end == b.end
        && a.all_day == b.all_day
}
fn day_instances(snapshot: &CheckedCalendarSnapshot, day: NaiveDate) -> Vec<EventInstance> {
    let start = NaiveDateTime::new(day, NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    let Some(end) = start.checked_add_signed(ChronoDuration::days(1)) else {
        return Vec::new();
    };
    expand_instances(&snapshot.events, start, end, 64)
}
fn unique_event<'a>(
    snapshot: &'a CheckedCalendarSnapshot,
    instance: &EventInstance,
) -> Option<&'a multi_launcher::plugins::calendar::CalendarEvent> {
    let mut events = snapshot
        .events
        .iter()
        .filter(|event| event.id == instance.source_event_id);
    let event = events.next()?;
    events.next().is_none().then_some(event)
}

struct Details {
    snapshot: Arc<CheckedCalendarSnapshot>,
    instance: EventInstance,
    day: NaiveDate,
}
struct OpenReceipt {
    query: String,
    action: Action,
    origin: LauncherResultOrigin,
    source: Option<(
        Arc<CheckedCalendarSnapshot>,
        Arc<AtomicU64>,
        u64,
        Arc<AtomicBool>,
    )>,
    now: NaiveDateTime,
    native: NativeWindow,
    revision: u64,
    lease: Arc<AtomicBool>,
}

pub(super) struct CalendarView {
    pub open: bool,
    root: WeakEntity<RootWindow>,
    host: AnyWindowHandle,
    owner: OwnerSlot,
    mount_owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    focus: FocusHandle,
    details_focus: FocusHandle,
    day: NaiveDate,
    details: Option<Details>,
    pending_open: Option<OpenReceipt>,
    open_query: Option<String>,
    open_action: Option<Action>,
    open_omni: bool,
    lease: Option<Arc<AtomicBool>>,
    busy: bool,
    feedback: Option<String>,
    focus_guard: Option<Subscription>,
    activation_guard: Option<Subscription>,
    details_lease: Option<Arc<AtomicBool>>,
    details_focus_guard: Option<Subscription>,
    details_activation_guard: Option<Subscription>,
}
impl CalendarView {
    fn new(
        root: WeakEntity<RootWindow>,
        owner: OwnerSlot,
        now: NaiveDateTime,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mount_owner = utility_mount::MountOwner::default();
        let mount = mount_owner.attach();
        Self {
            open: true,
            root,
            host: window.window_handle(),
            owner,
            mount_owner,
            mount,
            focus: cx.focus_handle(),
            details_focus: cx.focus_handle(),
            day: now.date(),
            details: None,
            pending_open: None,
            open_query: None,
            open_action: None,
            open_omni: false,
            lease: None,
            busy: false,
            feedback: None,
            focus_guard: None,
            activation_guard: None,
            details_lease: None,
            details_focus_guard: None,
            details_activation_guard: None,
        }
    }
    pub(super) fn revoke_open(&mut self) {
        if let Some(receipt) = self.pending_open.take() {
            receipt.lease.store(false, Ordering::Release);
        }
        self.open_query = None;
        self.open_action = None;
        self.open_omni = false;
        if let Some(lease) = &self.lease {
            lease.store(false, Ordering::Release);
        }
    }
    fn arm_open(&mut self, receipt: OpenReceipt, window: &mut Window, cx: &mut Context<Self>) {
        self.revoke_open();
        self.open_query = Some(receipt.query.clone());
        self.open_action = Some(receipt.action.clone());
        self.open_omni = matches!(&receipt.origin, LauncherResultOrigin::Omni(_));
        let lease = receipt.lease.clone();
        self.lease = Some(lease.clone());
        let blur = lease.clone();
        self.focus_guard = Some(cx.on_focus_out(&self.focus, window, move |_, _, _, _| {
            blur.store(false, Ordering::Release);
        }));
        self.activation_guard = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() {
                lease.store(false, Ordering::Release);
            }
        }));
        self.pending_open = Some(receipt);
    }
    fn revoke_details(&mut self) {
        if let Some(lease) = self.details_lease.take() {
            lease.store(false, Ordering::Release);
        }
        self.details_focus_guard = None;
        self.details_activation_guard = None;
    }
    fn reveal(&self, cx: &mut Context<Self>) {
        let id = self.host.window_id();
        let _ = self.root.update(cx, |root, cx| {
            let index = root.todo_scroll_item(id, cx)
                + usize::from(
                    root.todo_view
                        .as_ref()
                        .is_some_and(|view| view.read(cx).open),
                );
            root.root_scroll.scroll_to_top_of_item(index);
        });
    }
    pub(super) fn search_refreshed(
        &mut self,
        query: &str,
        enabled: Option<&HashSet<String>>,
        caps: Option<&HashMap<String, Vec<String>>>,
    ) {
        if self
            .open_query
            .as_deref()
            .is_some_and(|issued| issued != query)
            || (!self.open_omni
                && self
                    .open_action
                    .as_ref()
                    .is_some_and(|action| !current_provider(query, action, enabled, caps)))
            || (self.open_omni
                && (enabled.is_some_and(|set| !set.contains("omni_search"))
                    || !multi_launcher::settings::provider_search_capability_enabled(
                        caps,
                        "omni_search",
                    )))
        {
            self.revoke_open();
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
                        current_foreground_identity() == Some(native.identity)
                            && json_utility::native_host_current(
                                &root.root_visibility,
                                root.root_visibility
                                    .request_revision
                                    .load(Ordering::Acquire),
                                Some(native),
                                false,
                            )
                    })
            })
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.current(window, cx) || window.has_active_dialog(cx) {
            return;
        }
        self.revoke_details();
        if self.details.take().is_some() {
            self.reveal(cx);
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }
        let focused = self.focus.is_focused(window) || self.focus.contains_focused(window, cx);
        self.revoke_open();
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
    fn try_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.pending_open.is_none() {
            return;
        }
        let live = self.pending_open.as_ref().is_some_and(|receipt| {
            self.root.upgrade().is_some_and(|root| {
                open_stamp_current(
                    &receipt.lease,
                    receipt.revision,
                    root.read(cx)
                        .root_visibility
                        .request_revision
                        .load(Ordering::Acquire),
                )
            }) && current_foreground_identity() == Some(receipt.native.identity)
                && self.root.upgrade().is_some_and(|root| {
                    json_utility::native_host_current(
                        &root.read(cx).root_visibility,
                        receipt.revision,
                        Some(receipt.native),
                        false,
                    )
                })
        });
        if !live
            || !self.current(window, cx)
            || window.has_active_dialog(cx)
            || !(self.focus.is_focused(window) || self.focus.contains_focused(window, cx))
        {
            self.revoke_open();
            return;
        }
        let state = self.owner.lock().ok().and_then(|slot| slot.clone());
        let owner = match state {
            None => return,
            Some(Err(error)) => {
                self.revoke_open();
                self.feedback = Some(format!("Could not load Calendar: {error}"));
                cx.notify();
                return;
            }
            Some(Ok(owner)) => owner,
        };
        let Some(receipt) = self.pending_open.take() else {
            return;
        };
        if !receipt.lease.load(Ordering::Acquire)
            || !self.current(window, cx)
            || window.has_active_dialog(cx)
            || !(self.focus.is_focused(window) || self.focus.contains_focused(window, cx))
        {
            return;
        }
        let Some(root_entity) = self.root.upgrade() else {
            return;
        };
        let root = root_entity.read(cx);
        if root.query.read(cx).value().as_str() != receipt.query
            || root.launcher_results.query != receipt.query
            || !root
                .launcher_results
                .actions
                .iter()
                .any(|result| result.action == receipt.action && result.origin == receipt.origin)
            || match &receipt.origin {
                LauncherResultOrigin::Provider(name) if name == "calendar" => !current_provider(
                    &receipt.query,
                    &receipt.action,
                    root.enabled_plugins.as_ref(),
                    root.enabled_capabilities.as_ref(),
                ),
                LauncherResultOrigin::Omni(
                    multi_launcher::plugins::omni_search::OmniSource::Calendar,
                ) => {
                    root.enabled_plugins
                        .as_ref()
                        .is_some_and(|enabled| !enabled.contains("omni_search"))
                        || !multi_launcher::settings::provider_search_capability_enabled(
                            root.enabled_capabilities.as_ref(),
                            "omni_search",
                        )
                        || !root
                            .search_catalog
                            .providers
                            .omni
                            .settings()
                            .include_calendar
                        || root
                            .search_catalog
                            .providers
                            .omni
                            .projection(&receipt.query)
                            .and_then(|p| p.calendar_for(&receipt.action))
                            .is_none_or(|(_, snapshot, scope, revision)| {
                                receipt.source.as_ref().is_none_or(
                                    |(expected, old_scope, old_revision, catalog)| {
                                        !Arc::ptr_eq(&snapshot, expected)
                                            || !Arc::ptr_eq(&scope, old_scope)
                                            || revision != *old_revision
                                            || !catalog.load(Ordering::Acquire)
                                    },
                                )
                            })
                }
                _ => true,
            }
        {
            return;
        }
        let Some(native) = root
            .native_window
            .filter(|native| *native == receipt.native)
        else {
            return;
        };
        if !json_utility::native_host_current(
            &root.root_visibility,
            receipt.revision,
            Some(native),
            false,
        ) {
            return;
        }
        let Some(permit) = root.restart_admission.admit() else {
            return;
        };
        let visibility = root.root_visibility.clone();
        let revision = receipt.revision;
        let completion_visibility = visibility.clone();
        let admission = root.restart_admission.clone();
        let snapshot = owner.snapshot();
        if receipt
            .source
            .as_ref()
            .is_some_and(|(expected, scope, revision, catalog)| {
                !Arc::ptr_eq(expected, &snapshot)
                    || scope.load(Ordering::Acquire) != *revision
                    || !catalog.load(Ordering::Acquire)
            })
        {
            return;
        }
        let lease = receipt.lease.clone();
        let completion_lease = lease.clone();
        self.busy = true;
        self.feedback = None;
        let mount = self.mount.clone();
        let worker_mount = mount.clone();
        let task = cx.background_executor().spawn(async move {
            let _permit = permit;
            owner.write_state_if_current(&snapshot, receipt.now.date(), receipt.now, || {
                lease.load(Ordering::Acquire)
                    && receipt
                        .source
                        .as_ref()
                        .is_none_or(|(_, scope, revision, catalog)| {
                            scope.load(Ordering::Acquire) == *revision
                                && catalog.load(Ordering::Acquire)
                        })
                    && worker_mount.is_current()
                    && !admission.is_fenced()
                    && current_foreground_identity() == Some(native.identity)
                    && json_utility::native_host_current(&visibility, revision, Some(native), false)
            })
        });
        let host = self.host;
        cx.spawn(async move |weak, cx| {
            let result = task.await;
            if let Some(view) = weak.upgrade() {
                let _ = update_kit_window_content(host, &view, cx, |view, window, cx| {
                    view.busy = false;
                    if view
                        .lease
                        .as_ref()
                        .is_some_and(|lease| Arc::ptr_eq(lease, &completion_lease))
                    {
                        view.lease = None;
                        view.focus_guard = None;
                        view.activation_guard = None;
                    }
                    if mount.is_current() && mount.matches(&view.mount) {
                        if let Err(error) = result
                            && view.current(window, cx)
                            && completion_lease.load(Ordering::Acquire)
                            && json_utility::native_host_current(
                                &completion_visibility,
                                revision,
                                Some(native),
                                false,
                            )
                        {
                            view.feedback = Some(format!("Calendar state error: {error}"));
                        }
                        cx.notify();
                    }
                    view.try_open(window, cx);
                });
            }
        })
        .detach();
        cx.notify();
    }
    fn select(
        &mut self,
        snapshot: Arc<CheckedCalendarSnapshot>,
        day: NaiveDate,
        instance: EventInstance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.current(window, cx)
            || window.has_active_dialog(cx)
            || self.day != day
            || !snapshot.actionable
        {
            return;
        }
        let Some(Ok(owner)) = self.owner.lock().ok().and_then(|slot| slot.clone()) else {
            return;
        };
        if !Arc::ptr_eq(&owner.snapshot(), &snapshot)
            || unique_event(&snapshot, &instance).is_none()
            || !day_instances(&snapshot, day)
                .iter()
                .any(|candidate| same_occurrence(candidate, &instance))
        {
            return;
        }
        let Some(root_entity) = self.root.upgrade() else {
            return;
        };
        let root = root_entity.read(cx);
        let Some(native) = root.native_window else {
            return;
        };
        let Some(permit) = root.restart_admission.admit() else {
            return;
        };
        let admission = root.restart_admission.clone();
        let visibility = root.root_visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        self.revoke_details();
        self.focus.focus(window, cx);
        let lease = Arc::new(AtomicBool::new(true));
        self.details_lease = Some(lease.clone());
        let blur = lease.clone();
        self.details_focus_guard = Some(cx.on_focus_out(&self.focus, window, move |_, _, _, _| {
            blur.store(false, Ordering::Release);
        }));
        let active = lease.clone();
        self.details_activation_guard =
            Some(cx.observe_window_activation(window, move |_, window, _| {
                if !window.is_window_active() {
                    active.store(false, Ordering::Release);
                }
            }));
        let mount = self.mount.clone();
        let worker_mount = mount.clone();
        let worker_snapshot = snapshot.clone();
        let worker_lease = lease.clone();
        let task = cx.background_executor().spawn(async move {
            let _permit = permit;
            owner.snapshot_is_current(&worker_snapshot)
                && worker_lease.load(Ordering::Acquire)
                && worker_mount.is_current()
                && !admission.is_fenced()
                && current_foreground_identity() == Some(native.identity)
                && json_utility::native_host_current(&visibility, revision, Some(native), false)
        });
        let host = self.host;
        cx.spawn(async move |weak, cx| {
            let valid = task.await;
            if let Some(view) = weak.upgrade() {
                let _ = update_kit_window_content(host, &view, cx, |view, window, cx| {
                    let exact = view
                        .details_lease
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(current, &lease));
                    if !exact {
                        return;
                    }
                    let current = view.owner.lock().ok().and_then(|slot| {
                        slot.as_ref()
                            .and_then(|result| result.as_ref().ok())
                            .map(|owner| owner.snapshot())
                    });
                    if valid
                        && lease.load(Ordering::Acquire)
                        && mount.is_current()
                        && mount.matches(&view.mount)
                        && view.day == day
                        && view.current(window, cx)
                        && !window.has_active_dialog(cx)
                        && current
                            .as_ref()
                            .is_some_and(|live| live.actionable && Arc::ptr_eq(live, &snapshot))
                    {
                        view.details = Some(Details {
                            snapshot,
                            instance,
                            day,
                        });
                        view.reveal(cx);
                        view.details_focus.focus(window, cx);
                    }
                    view.revoke_details();
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }
}

impl Render for CalendarView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let close = self.mount.clone();
        let escape = self.mount.clone();
        let mut panel = div()
            .id("calendar-view")
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
            .child(Label::new("Calendar"));
        let state = self.owner.lock().ok().and_then(|slot| slot.clone());
        if let (Some(details), Some(Ok(owner))) = (&self.details, &state) {
            let snapshot = owner.snapshot();
            if details.day == self.day
                && snapshot.actionable
                && Arc::ptr_eq(&snapshot, &details.snapshot)
                && let Some(event) = unique_event(&snapshot, &details.instance)
            {
                let close = self.mount.clone();
                let body = div()
                    .id("calendar-details-body")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .max_h(px(160.))
                    .overflow_y_scroll()
                    .child(Label::new(event.title.clone()))
                    .child(Label::new(format!(
                        "{} – {}",
                        details.instance.start.format("%Y-%m-%d %H:%M"),
                        details.instance.end.format("%Y-%m-%d %H:%M")
                    )))
                    .children(event.notes.as_ref().map(|notes| Label::new(notes.clone())))
                    .child(Label::new(format!("Tags: {}", event.tags.join(", "))));
                return panel.child(
                    div()
                        .id("calendar-details")
                        .track_focus(&self.details_focus)
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            Button::new("calendar-details-close")
                                .label("Close details")
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if close.is_current()
                                        && close.matches(&view.mount)
                                        && view.current(window, cx)
                                        && !window.has_active_dialog(cx)
                                    {
                                        view.revoke_details();
                                        view.details = None;
                                        view.reveal(cx);
                                        view.focus.focus(window, cx);
                                        cx.notify();
                                    }
                                })),
                        )
                        .child(body),
                );
            }
        }
        let mut nav = div().flex().gap_2();
        for (id, label, offset) in [
            ("calendar-previous", "Previous day", -1_i64),
            ("calendar-today", "Today", 0),
            ("calendar-next", "Next day", 1),
        ] {
            let mount = self.mount.clone();
            nav = nav.child(Button::new(id).label(label).on_click(cx.listener(
                move |view, _, window, cx| {
                    if mount.is_current()
                        && mount.matches(&view.mount)
                        && view.current(window, cx)
                        && !window.has_active_dialog(cx)
                    {
                        let next = if offset == 0 {
                            Some(Local::now().naive_local().date())
                        } else {
                            view.day.checked_add_signed(ChronoDuration::days(offset))
                        };
                        if let Some(day) = next {
                            view.revoke_details();
                            view.day = day;
                            view.details = None;
                            cx.notify();
                        }
                    }
                },
            )));
        }
        panel = panel
            .child(nav)
            .child(Label::new(format!("{}", self.day.format("%A, %B %-d, %Y"))))
            .child(
                Button::new("calendar-close")
                    .label("Close")
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if close.is_current() && close.matches(&view.mount) {
                            view.close(window, cx);
                        }
                    })),
            );
        if let Some(error) = &self.feedback {
            panel = panel.child(Label::new(error.clone()));
        }
        let owner = match state {
            None => return panel.child(Label::new("Loading Calendar…")),
            Some(Err(error)) => {
                return panel.child(Label::new(format!("Could not load Calendar: {error}")));
            }
            Some(Ok(owner)) => owner,
        };
        let snapshot = owner.snapshot();
        if let Some(status) = &snapshot.status {
            panel = panel.child(Label::new(status.clone()));
        }
        let mut agenda = div()
            .id("calendar-agenda")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(220.))
            .overflow_y_scroll()
            .child(Label::new("Agenda"));
        let instances = day_instances(&snapshot, self.day);
        if instances.is_empty() {
            agenda = agenda.child(Label::new("No events"));
        }
        for (index, instance) in instances.into_iter().enumerate() {
            let title = snapshot
                .events
                .iter()
                .rev()
                .find(|event| event.id == instance.source_event_id)
                .map(|event| event.title.as_str())
                .unwrap_or("Event");
            let time = if instance.all_day {
                "All day".to_owned()
            } else {
                format!(
                    "{} - {}",
                    instance.start.format("%H:%M"),
                    instance.end.format("%H:%M")
                )
            };
            let label = format!("{title} ({time})");
            let allowed = snapshot.actionable && unique_event(&snapshot, &instance).is_some();
            let mount = self.mount.clone();
            let source = snapshot.clone();
            let day = self.day;
            agenda = agenda.child(
                Button::new(SharedString::from(format!(
                    "calendar-occurrence-{}-{index}",
                    snapshot.revision
                )))
                .label(label)
                .disabled(!allowed)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if mount.is_current() && mount.matches(&view.mount) {
                        view.select(source.clone(), day, instance.clone(), window, cx);
                    }
                })),
            );
        }
        panel = panel.child(agenda);
        panel
    }
}

impl RootWindow {
    pub(super) fn start_calendar_wake(&mut self, cx: &mut Context<Self>) {
        let wake = self.calendar_wake.clone();
        cx.spawn(async move |root, cx| {
            while wake.wait().await {
                let view = root
                    .update(cx, |root, cx| {
                        let source = root.calendar_owner.lock().ok().and_then(|slot| {
                            slot.as_ref()
                                .and_then(|result| result.as_ref().ok())
                                .map(|owner| (owner.clone(), owner.snapshot()))
                        });
                        root.calendar_results.observe_source(source);
                        if root.calendar_results.invalidate_source() {
                            let query = root.query.read(cx).value().to_string();
                            let layout = root
                                .search_catalog
                                .resolve_result_layout(&query, root.enabled_plugins.as_ref());
                            root.launcher_results.replace(
                                query,
                                Vec::new(),
                                layout,
                                root.search_catalog.page_jump,
                            );
                            root.fresh_search_enter = None;
                            root.launcher_results_scroll = ScrollHandle::new();
                        }
                        let query = root.query.read(cx).value().to_string();
                        let relevant = omni_search::is_omni_query(&query)
                            && root
                                .enabled_plugins
                                .as_ref()
                                .is_none_or(|enabled| enabled.contains("omni_search"))
                            && multi_launcher::settings::provider_search_capability_enabled(
                                root.enabled_capabilities.as_ref(),
                                "omni_search",
                            )
                            && root
                                .search_catalog
                                .providers
                                .omni
                                .settings()
                                .include_calendar;
                        if relevant
                            && root.calendar_results.source_dirty()
                            && !root.calendar_results.is_pending()
                            && !root.external_launch.defer_notes_wake_for_omni()
                        {
                            root.refresh_launcher_results_with_viewport(query, false, cx);
                        } else if !relevant {
                            root.calendar_results.acknowledge_source();
                        }
                        root.calendar_view
                            .as_ref()
                            .map(|view| (view.clone(), view.read(cx).host))
                    })
                    .ok()
                    .flatten();
                if let Some((view, host)) = view {
                    let _ = update_kit_window_content(host, &view, cx, |view, window, cx| {
                        if let Some(details) = &view.details {
                            let current = view.owner.lock().ok().and_then(|slot| {
                                slot.as_ref()
                                    .and_then(|result| result.as_ref().ok())
                                    .map(|owner| owner.snapshot())
                            });
                            if current.as_ref().is_none_or(|snapshot| {
                                !snapshot.actionable || !Arc::ptr_eq(snapshot, &details.snapshot)
                            }) {
                                let focused = view.details_focus.is_focused(window)
                                    || view.details_focus.contains_focused(window, cx);
                                view.revoke_details();
                                view.details = None;
                                if focused
                                    && view.current(window, cx)
                                    && !window.has_active_dialog(cx)
                                {
                                    view.reveal(cx);
                                    view.focus.focus(window, cx);
                                }
                            }
                        }
                        view.try_open(window, cx);
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }
    pub(super) fn open_calendar_view(
        &mut self,
        query: String,
        action: Action,
        origin: LauncherResultOrigin,
        source: Option<(
            Arc<CheckedCalendarSnapshot>,
            Arc<AtomicU64>,
            u64,
            Arc<AtomicBool>,
        )>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.restart_admission.is_fenced()
            || window.has_active_dialog(cx)
            || !window.is_window_active()
        {
            return;
        }
        let Some(native) = self
            .native_window
            .filter(|native| current_foreground_identity() == Some(native.identity))
        else {
            return;
        };
        let revision = self
            .root_visibility
            .request_revision
            .load(Ordering::Acquire);
        if !json_utility::native_host_current(&self.root_visibility, revision, Some(native), false)
        {
            return;
        }
        let now = Local::now().naive_local();
        let receipt = OpenReceipt {
            query,
            action,
            origin,
            source,
            now,
            native,
            revision,
            lease: Arc::new(AtomicBool::new(true)),
        };
        let view = if let Some(view) = &self.calendar_view {
            view.update(cx, |view, cx| {
                view.revoke_open();
                view.revoke_details();
                view.mount = view.mount_owner.attach();
                view.open = true;
                view.day = now.date();
                view.details = None;
                view.feedback = None;
                cx.notify();
            });
            view.clone()
        } else {
            let root = cx.entity().downgrade();
            let owner = self.calendar_owner.clone();
            let view = cx.new(|cx| CalendarView::new(root, owner, now, window, cx));
            self.calendar_view = Some(view.clone());
            view
        };
        let focus = view.read(cx).focus.clone();
        focus.focus(window, cx);
        view.update(cx, |view, cx| view.arm_open(receipt, window, cx));
        let id = window.window_handle().window_id();
        let index = self.todo_scroll_item(id, cx)
            + usize::from(
                self.todo_view
                    .as_ref()
                    .is_some_and(|view| view.read(cx).open),
            );
        self.root_scroll.scroll_to_top_of_item(index);
        let weak = view.downgrade();
        window.defer(cx, move |window, app| {
            let _ = weak.update(app, |view, cx| view.try_open(window, cx));
        });
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        current_provider, day_instances, open_stamp_current, same_occurrence, unique_event,
    };
    use multi_launcher::plugin::Plugin;
    use std::collections::HashSet;
    use std::sync::Arc;

    #[test]
    fn delayed_open_requires_original_revision_and_unrevoked_lease() {
        let lease = std::sync::atomic::AtomicBool::new(true);
        assert!(open_stamp_current(&lease, 7, 7));
        assert!(!open_stamp_current(&lease, 7, 8));
        lease.store(false, std::sync::atomic::Ordering::Release);
        assert!(!open_stamp_current(&lease, 7, 7));
    }

    #[test]
    fn bare_calendar_route_requires_exact_stock_action_and_search_capability() {
        let action = multi_launcher::plugins::calendar::CalendarPlugin
            .search("cal")
            .pop()
            .unwrap();
        assert!(current_provider("CaL", &action, None, None));
        assert!(!current_provider("cal upcoming", &action, None, None));
        assert!(!current_provider(
            "cal",
            &action,
            Some(&HashSet::new()),
            None
        ));
        let mut changed = action.clone();
        changed.label.push('!');
        assert!(!current_provider("cal", &changed, None, None));
    }

    #[test]
    fn details_bind_exact_occurrence_snapshot_and_unique_event() {
        let temp = tempfile::tempdir().unwrap();
        let root = multi_launcher::platform::app_data::AppDataRoot::from_settings_path(
            temp.path().join("settings.json"),
        )
        .unwrap();
        std::fs::create_dir(temp.path().join("calendar")).unwrap();
        let path = temp.path().join("calendar/events.json");
        std::fs::write(&path, br#"[{"id":"a","title":"Meeting","start":"2026-09-06T09:30:00","created_at":"2026-09-01T00:00:00","recurrence":{"frequency":"Daily","end":{"AfterCount":{"count":3}}}}]"#).unwrap();
        let owner =
            multi_launcher::plugins::calendar::CheckedCalendarOwner::start(&root, Arc::new(|| {}))
                .unwrap();
        let snapshot = owner.snapshot();
        let day = chrono::NaiveDate::from_ymd_opt(2026, 9, 7).unwrap();
        let instances = day_instances(&snapshot, day);
        assert_eq!(instances.len(), 1);
        assert!(same_occurrence(&instances[0], &instances[0].clone()));
        assert_eq!(
            unique_event(&snapshot, &instances[0]).unwrap().title,
            "Meeting"
        );
        assert!(owner.snapshot_is_current(&snapshot));
        std::fs::write(&path, b"[]").unwrap();
        assert!(!owner.snapshot_is_current(&snapshot));
        owner.shutdown();
    }
}
