//! Checked stock Calendar result replacement and generated Jump receipts.
use super::*;
use multi_launcher::commands::{CalendarCommand, QueryPolicy, ResultsPolicy};
use multi_launcher::plugins::calendar::{CheckedCalendarOwner, CheckedCalendarSnapshot};

type Source = (
    Arc<CheckedCalendarOwner>,
    Arc<CheckedCalendarSnapshot>,
    Arc<AtomicU64>,
    u64,
);
fn exact_row(actions: &[Action], expected: &Action, index: usize) -> bool {
    actions.get(index) == Some(expected)
}
fn visible_toasts(
    toasts: &[multi_launcher::commands::ToastPolicy],
    enabled: bool,
    errors: bool,
) -> Vec<(String, bool)> {
    if !enabled {
        return Vec::new();
    }
    toasts
        .iter()
        .filter_map(|toast| match toast {
            multi_launcher::commands::ToastPolicy::Launched(label) => {
                Some((format!("Launched {label}"), false))
            }
            multi_launcher::commands::ToastPolicy::Copied(label) => {
                Some((format!("Copied {label}"), false))
            }
            multi_launcher::commands::ToastPolicy::Info(message)
            | multi_launcher::commands::ToastPolicy::Success(message) => {
                Some((message.clone(), false))
            }
            multi_launcher::commands::ToastPolicy::Error(message) if errors => {
                Some((message.clone(), true))
            }
            _ => None,
        })
        .collect()
}
struct Pending {
    serial: u64,
    lease: Arc<AtomicBool>,
    _focus: Subscription,
    _activation: Subscription,
}
struct Replacement {
    serial: u64,
    query: String,
    actions: Vec<Action>,
    source: Source,
    authority: Arc<AtomicBool>,
    _focus: Subscription,
    _activation: Subscription,
    visibility: RootVisibility,
    visible_revision: u64,
    native: NativeWindow,
}
#[derive(Default)]
pub(super) struct Owner {
    next: u64,
    pending: Option<Pending>,
    replacement: Option<Replacement>,
    marker: Option<(String, u64)>,
    observed_source: Option<(Arc<CheckedCalendarOwner>, Arc<CheckedCalendarSnapshot>)>,
    source_dirty: bool,
}
impl Owner {
    pub fn observe_source(
        &mut self,
        source: Option<(Arc<CheckedCalendarOwner>, Arc<CheckedCalendarSnapshot>)>,
    ) {
        let same = match (&self.observed_source, &source) {
            (None, None) => true,
            (Some((a, x)), Some((b, y))) => Arc::ptr_eq(a, b) && Arc::ptr_eq(x, y),
            _ => false,
        };
        if !same {
            self.observed_source = source;
            self.source_dirty = true;
        }
    }
    pub fn source_dirty(&self) -> bool {
        self.source_dirty
    }
    pub fn source_projected(
        &mut self,
        source: Option<(Arc<CheckedCalendarOwner>, Arc<CheckedCalendarSnapshot>)>,
    ) {
        self.observed_source = source;
        self.source_dirty = false;
    }
    pub fn acknowledge_source(&mut self) {
        self.source_dirty = false;
    }
    pub fn has_replacement(&self) -> bool { self.replacement.is_some() }
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn retire(&mut self) {
        if let Some(pending) = self.pending.take() {
            pending.lease.store(false, Ordering::Release);
        }
        if let Some(replacement) = self.replacement.take() {
            replacement.authority.store(false, Ordering::Release);
        }
        self.marker = None;
    }
    pub fn accept_change(&mut self, query: &str, generation: u64) -> bool {
        if self
            .marker
            .as_ref()
            .is_some_and(|(expected, current)| expected == query && *current == generation)
        {
            self.marker = None;
            true
        } else {
            false
        }
    }
    fn begin(
        &mut self,
        lease: Arc<AtomicBool>,
        focus: Subscription,
        activation: Subscription,
    ) -> u64 {
        self.retire();
        self.next = self.next.wrapping_add(1).max(1);
        self.pending = Some(Pending {
            serial: self.next,
            lease,
            _focus: focus,
            _activation: activation,
        });
        self.next
    }
    fn pending(&self, serial: u64, lease: &Arc<AtomicBool>) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| pending.serial == serial && Arc::ptr_eq(&pending.lease, lease))
    }
    pub fn receipt(
        &self,
        query: &str,
        action: &Action,
        serial: u64,
        index: usize,
    ) -> Option<(Source, Arc<AtomicBool>)> {
        let r = self.replacement.as_ref()?;
        let (owner, snapshot, scope, revision) = &r.source;
        (r.serial == serial
            && r.query == query
            && exact_row(&r.actions, action, index)
            && r.authority.load(Ordering::Acquire)
            && current_foreground_identity() == Some(r.native.identity)
            && json_utility::native_host_current(
                &r.visibility,
                r.visible_revision,
                Some(r.native),
                false,
            )
            && scope.load(Ordering::Acquire) == *revision
            && snapshot.actionable
            && Arc::ptr_eq(&owner.snapshot(), snapshot))
        .then(|| (r.source.clone(), r.authority.clone()))
    }
    pub fn invalidate_source(&mut self) -> bool {
        let stale = self.replacement.as_ref().is_some_and(|r| {
            let (owner, snapshot, scope, revision) = &r.source;
            !r.authority.load(Ordering::Acquire)
                || !json_utility::native_host_current(
                    &r.visibility,
                    r.visible_revision,
                    Some(r.native),
                    false,
                )
                || !snapshot.actionable
                || !Arc::ptr_eq(&owner.snapshot(), snapshot)
                || scope.load(Ordering::Acquire) != *revision
        });
        if stale {
            self.retire();
        }
        stale
    }
}

impl RootWindow {
    pub(super) fn start_calendar_query(
        &mut self,
        action: Action,
        command: CalendarCommand,
        owner: Arc<CheckedCalendarOwner>,
        snapshot: Arc<CheckedCalendarSnapshot>,
        scope: Arc<AtomicU64>,
        revision: u64,
        catalog: Arc<AtomicBool>,
        query: String,
        native: NativeWindow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.restart_admission.is_fenced()
            || window.has_active_dialog(cx)
            || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
        {
            return;
        }
        let Some(focus) = window.focused(cx) else {
            return;
        };
        let Some(permit) = self.restart_admission.admit() else {
            return;
        };
        let lease = Arc::new(AtomicBool::new(true));
        let blur = lease.clone();
        let focus_guard = cx.on_blur(&focus, window, move |root, _, _| {
            blur.store(false, Ordering::Release);
            root.calendar_wake.notify();
        });
        let inactive = lease.clone();
        let activation = cx.observe_window_activation(window, move |root, window, _| {
            if !window.is_window_active() {
                inactive.store(false, Ordering::Release);
                root.calendar_wake.notify();
            }
        });
        let serial = self
            .calendar_results
            .begin(lease.clone(), focus_guard, activation);
        let generation = self.launcher_results.generation;
        let visibility = self.root_visibility.clone();
        let receipt_visibility = visibility.clone();
        let visible_revision = visibility.request_revision.load(Ordering::Acquire);
        let admission = self.restart_admission.clone();
        let source = (owner.clone(), snapshot.clone(), scope.clone(), revision);
        let worker_lease = lease.clone();
        let worker_catalog = catalog.clone();
        let task = cx.background_executor().spawn(async move {
            let _permit = permit;
            let current = || {
                worker_lease.load(Ordering::Acquire)
                    && worker_catalog.load(Ordering::Acquire)
                    && scope.load(Ordering::Acquire) == revision
                    && !admission.is_fenced()
                    && current_foreground_identity() == Some(native.identity)
                    && json_utility::native_host_current(
                        &visibility,
                        visible_revision,
                        Some(native),
                        false,
                    )
            };
            if !current() || !owner.snapshot_is_current(&snapshot) {
                return None;
            }
            let outcome = match command {
                CalendarCommand::Search { input } => {
                    multi_launcher::commands::handlers::calendar_search_outcome(
                        &snapshot.events,
                        &input,
                        true,
                    )
                }
                CalendarCommand::Upcoming => {
                    multi_launcher::commands::handlers::calendar_upcoming_outcome(
                        &snapshot.events,
                        chrono::Local::now().naive_local(),
                    )
                }
                _ => return None,
            };
            if !owner.snapshot_is_current(&snapshot) || !current() {
                return None;
            }
            Some(outcome)
        });
        let root = cx.entity().downgrade();
        let host = window.window_handle();
        cx.spawn(async move |_, cx| {
            let outcome = task.await;
            let _ = host.update(cx, |_, window, app| {
                let _ = root.update(app, |root, cx| {
                    if !root.calendar_results.pending(serial, &lease) {
                        root.calendar_wake.notify();
                        return;
                    }
                    let Some(pending) = root.calendar_results.pending.take() else {
                        return;
                    };
                    root.calendar_wake.notify();
                    if !lease.load(Ordering::Acquire) {
                        return;
                    }
                    let Some(outcome) = outcome else {
                        return;
                    };
                    if root.restart_admission.is_fenced()
                        || window.has_active_dialog(cx)
                        || !window.is_window_active()
                        || root.launcher_results.generation != generation
                        || root.query.read(cx).value().as_str() != query
                        || !root.external_owner_is_current(Some(&focus), window, cx)
                        || current_foreground_identity() != Some(native.identity)
                        || !json_utility::native_host_current(
                            &root.root_visibility,
                            visible_revision,
                            Some(native),
                            false,
                        )
                        || !catalog.load(Ordering::Acquire)
                        || source.2.load(Ordering::Acquire) != source.3
                        || !Arc::ptr_eq(&source.0.snapshot(), &source.1)
                        || root
                            .enabled_plugins
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
                            .projection(&query)
                            .and_then(|projection| projection.calendar_for(&action))
                            .is_none_or(|(owner, snapshot, scope, revision)| {
                                !Arc::ptr_eq(&owner, &source.0)
                                    || !Arc::ptr_eq(&snapshot, &source.1)
                                    || !Arc::ptr_eq(&scope, &source.2)
                                    || revision != source.3
                            })
                    {
                        return;
                    }
                    let feedback = visible_toasts(
                        &outcome.toasts,
                        root.external_launch_preferences.enable_toasts,
                        root.external_launch_preferences.show_error_toasts,
                    );
                    let (next, actions) = match (outcome.query, outcome.results) {
                        (QueryPolicy::Set(next), ResultsPolicy::Replace(actions)) => {
                            (next, actions)
                        }
                        _ => {
                            if let Some(message) =
                                outcome.toasts.into_iter().find_map(|toast| match toast {
                                    multi_launcher::commands::ToastPolicy::Error(message) => {
                                        Some(message)
                                    }
                                    _ => None,
                                })
                            {
                                root.launcher_search_error = Some(message);
                            }
                            if outcome.focus {
                                root.query.focus_handle(cx).focus(window, cx);
                            }
                            cx.notify();
                            return;
                        }
                    };
                    let authority = lease.clone();
                    root.launcher_search_error = None;
                    let rows = actions
                        .iter()
                        .enumerate()
                        .map(|(index, action)| LauncherSearchResult {
                            action: action.clone(),
                            origin: LauncherResultOrigin::CalendarJump { serial, index },
                            display_label: None,
                            hide_description: false,
                        })
                        .collect();
                    let layout = root
                        .search_catalog
                        .resolve_result_layout(&next, root.enabled_plugins.as_ref());
                    root.launcher_results.replace(
                        next.clone(),
                        rows,
                        layout,
                        root.search_catalog.page_jump,
                    );
                    root.calendar_results.replacement = Some(Replacement {
                        serial,
                        query: next.clone(),
                        actions,
                        source,
                        authority,
                        _focus: pending._focus,
                        _activation: pending._activation,
                        visibility: receipt_visibility,
                        visible_revision,
                        native,
                    });
                    root.calendar_results.marker =
                        Some((next.clone(), root.launcher_results.generation));
                    root.fresh_search_enter = None;
                    root.launcher_results_scroll = ScrollHandle::new();
                    root.query_history.reset();
                    root.search_completion.clear_for_query_replacement();
                    root.query
                        .update(cx, |input, cx| input.replace_all(next, window, cx));
                    root.query.focus_handle(cx).focus(window, cx);
                    root.root_scroll
                        .scroll_to_top_of_item(ROOT_SCROLL_SEARCH_ITEM);
                    let duration = root.external_launch_preferences.toast_duration;
                    for (index, (message, error)) in feedback.into_iter().enumerate() {
                        root.external_notification(
                            serial | (1_u64 << 63),
                            index,
                            message,
                            error,
                            duration,
                            window,
                            cx,
                        );
                    }
                    cx.notify();
                });
            });
        })
        .detach();
    }

    pub(super) fn activate_calendar_jump(
        &mut self,
        result: &LauncherSearchResult,
        query: String,
        source: ActivationSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let LauncherResultOrigin::CalendarJump { serial, index } = &result.origin else {
            return;
        };
        if self.restart_admission.is_fenced()
            || window.has_active_dialog(cx)
            || !self.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
        {
            return;
        }
        let Some(((owner, snapshot, scope, revision), authority)) =
            self.calendar_results
                .receipt(&query, &result.action, *serial, *index)
        else {
            return;
        };
        let Ok(Command::Calendar(CalendarCommand::Jump { reference })) =
            parse_action(&result.action)
        else {
            return;
        };
        let Ok(day) = chrono::NaiveDate::parse_from_str(&reference, "%Y-%m-%d") else {
            return;
        };
        if day.format("%Y-%m-%d").to_string() != reference {
            return;
        }
        let Some(native) = self
            .native_window
            .filter(|native| current_foreground_identity() == Some(native.identity))
        else {
            return;
        };
        let guard = external_launch::OmniGuard {
            catalog: authority,
            lease: Arc::new(AtomicBool::new(true)),
            effect_gate: Arc::new(external_launch::EffectGate::default()),
            visibility: self.root_visibility.clone(),
            revision: self
                .root_visibility
                .request_revision
                .load(Ordering::Acquire),
            native,
            admission: self.restart_admission.clone(),
        };
        self.start_calendar_jump(
            result.action.clone(),
            owner,
            snapshot,
            scope,
            revision,
            guard,
            day,
            query,
            source,
            window,
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{Owner, exact_row, visible_toasts};
    use multi_launcher::actions::Action;
    #[test]
    fn replacement_change_marker_is_exact_and_one_shot() {
        let mut owner = Owner::default();
        owner.marker = Some(("cal upcoming".into(), 9));
        assert!(!owner.accept_change("cal upcoming", 8));
        assert!(!owner.accept_change("cal find x", 9));
        assert!(owner.accept_change("cal upcoming", 9));
        assert!(!owner.accept_change("cal upcoming", 9));
    }
    #[test]
    fn generated_row_requires_exact_ordinal_and_payload() {
        let action = Action {
            label: "Meeting".into(),
            desc: "Calendar".into(),
            action: "calendar:jump:2026-09-06".into(),
            args: None,
        };
        let mut changed = action.clone();
        changed.label.push('!');
        assert!(exact_row(std::slice::from_ref(&action), &action, 0));
        assert!(!exact_row(std::slice::from_ref(&action), &action, 1));
        assert!(!exact_row(std::slice::from_ref(&action), &changed, 0));
    }
    #[test]
    fn accepted_empty_search_keeps_stock_info_and_toast_preference() {
        let toasts = vec![multi_launcher::commands::ToastPolicy::Info(
            "Found 0 events".into(),
        )];
        assert_eq!(
            visible_toasts(&toasts, true, false),
            vec![("Found 0 events".into(), false)]
        );
        assert!(visible_toasts(&toasts, false, true).is_empty());
    }
    #[test]
    fn settled_worker_wake_does_not_dirty_an_already_projected_source() {
        let temp = tempfile::tempdir().unwrap();
        let root = multi_launcher::platform::app_data::AppDataRoot::from_settings_path(
            temp.path().join("settings.json"),
        )
        .unwrap();
        let calendar = multi_launcher::plugins::calendar::CheckedCalendarOwner::start(
            &root,
            std::sync::Arc::new(|| {}),
        )
        .unwrap();
        let first = calendar.snapshot();
        let mut owner = Owner::default();
        owner.observe_source(Some((calendar.clone(), first.clone())));
        assert!(owner.source_dirty());
        owner.source_projected(Some((calendar.clone(), first.clone())));
        owner.observe_source(Some((calendar.clone(), first)));
        assert!(!owner.source_dirty());
        std::fs::create_dir(temp.path().join("calendar")).unwrap();
        std::fs::write(temp.path().join("calendar/events.json"), b"[]").unwrap();
        calendar.refresh().unwrap();
        let next = calendar.snapshot();
        owner.observe_source(Some((calendar.clone(), next.clone())));
        assert!(owner.source_dirty());
        owner.source_projected(Some((calendar.clone(), next.clone())));
        owner.observe_source(Some((calendar.clone(), next)));
        assert!(!owner.source_dirty());
        calendar.shutdown();
    }
}
