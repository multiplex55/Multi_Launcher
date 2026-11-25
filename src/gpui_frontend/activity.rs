use super::*;
use multi_launcher::activity::{ActivityCancelResult, ActivityCenter, ActivityKey, ActivityLifecycle, ActivityOperation, ActivityOwner, ActivityRow};
use gpui_kit::KeyUpEvent;

const REFRESH: Duration = Duration::from_millis(250);

fn refresh_is_current(open: bool, active: bool, visible: bool, in_viewport: bool, expected: u64, actual: u64) -> bool {
    open && active && visible && in_viewport && expected == actual
}

fn eligible_cancel(row: &ActivityRow, key: ActivityKey, current_hash: Option<u64>) -> bool {
    row.key == key && key.owner() == ActivityOwner::Hash && row.operation == ActivityOperation::CalculateHash
        && current_hash == Some(key.incarnation().get())
        && row.can_cancel && !row.lifecycle.is_terminal() && row.lifecycle != ActivityLifecycle::Cancelling
}

fn row_element_ids(operation_id: u64) -> (String, String) {
    (format!("activity-row-{operation_id}"), format!("activity-row-label-{operation_id}"))
}

fn cancel_exact_hash(center: &ActivityCenter, rendered: &[ActivityRow], key: ActivityKey, current_hash: Option<u64>) -> ActivityCancelResult {
    if !rendered.iter().any(|row| eligible_cancel(row, key, current_hash))
        || !center.snapshot().rows.iter().any(|row| eligible_cancel(row, key, current_hash)) {
        return ActivityCancelResult::Stale;
    }
    center.request_cancel(key)
}

pub(super) fn provider_is_current(query: &str, action: &Action, enabled: Option<&HashSet<String>>,
    caps: Option<&HashMap<String, Vec<String>>>) -> bool {
    let plugin = multi_launcher::plugins::activity::ActivityPlugin;
    super::search_providers::search_routed(
        [&plugin as &dyn multi_launcher::plugin::Plugin], query, enabled, caps,
    ).iter().any(|(name, row)| name == "activity" && row == action)
        && matches!(parse_action(action), Ok(Command::Dialog(DialogCommand::Activity)))
}

pub(super) struct ActivityView {
    pub(super) open: bool,
    root: WeakEntity<RootWindow>,
    center: ActivityCenter,
    hash_session: Arc<Mutex<HashSession>>,
    visibility: RootVisibility,
    focus: FocusHandle,
    close_focus: FocusHandle,
    return_focus: Option<FocusHandle>,
    return_scroll: Option<Point<Pixels>>,
    scroll: ScrollHandle,
    root_scroll: ScrollHandle,
    viewport_visible: bool,
    measured_visible: Rc<Cell<bool>>,
    rows: Vec<ActivityRow>,
    cancel_status: Option<&'static str>,
    refresh_epoch: u64,
    escape_waiting: bool,
    _subscriptions: Vec<Subscription>,
}

impl ActivityView {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>, root: WeakEntity<RootWindow>,
        center: ActivityCenter, hash_session: Arc<Mutex<HashSession>>, visibility: RootVisibility, root_scroll: ScrollHandle) -> Self {
        let focus = cx.focus_handle();
        let subscriptions = vec![
            cx.on_focus_out(&focus, window, |this, _, _, _| this.escape_waiting = false),
            cx.observe_window_activation(window, |this, window, _| {
                if !window.is_window_active() { this.escape_waiting = false; }
            }),
        ];
        Self { open: false, root, center, hash_session, visibility, focus, close_focus: cx.focus_handle(),
            return_focus: None, return_scroll: None, scroll: ScrollHandle::new(), root_scroll,
            viewport_visible: false, measured_visible: Rc::new(Cell::new(false)), rows: Vec::new(), cancel_status: None,
            refresh_epoch: 0, escape_waiting: false, _subscriptions: subscriptions }
    }

    pub(super) fn show(&mut self, origin: Option<FocusHandle>, scroll: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            self.open = true;
            self.return_focus = origin;
            self.return_scroll = Some(scroll);
            self.cancel_status = None;
            self.scroll.set_offset(point(px(0.0), px(0.0)));
        }
        self.close_focus.focus(window, cx);
        self.resume(cx);
    }

    pub(super) fn resume(&mut self, cx: &mut Context<Self>) {
        self.refresh_epoch = self.refresh_epoch.wrapping_add(1).max(1);
        let epoch = self.refresh_epoch;
        if !self.refresh(epoch, cx) { return; }
        let owner = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            loop {
                cx.background_executor().timer(REFRESH).await;
                match owner.update(cx, |view, cx| view.refresh(epoch, cx)) {
                    Ok(true) => {}
                    _ => break,
                }
            }
        }).detach();
    }

    fn refresh(&mut self, epoch: u64, cx: &mut Context<Self>) -> bool {
        if !refresh_is_current(self.open, self.visibility.active.load(Ordering::Acquire),
            self.visibility.visible.load(Ordering::Acquire), self.viewport_visible, epoch, self.refresh_epoch) {
            return false;
        }
        self.rows = self.center.snapshot().rows.into_iter()
            .filter(|row| row.key.owner() == ActivityOwner::Hash).collect();
        cx.notify();
        true
    }

    fn viewport_changed(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.viewport_visible == visible { return; }
        self.viewport_visible = visible;
        if visible { self.resume(cx); }
        else { self.refresh_epoch = self.refresh_epoch.wrapping_add(1); }
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open { return; }
        let owned = window.is_window_active() && self.focus.contains_focused(window, cx);
        self.open = false;
        self.viewport_visible = false;
        self.measured_visible.set(false);
        self.refresh_epoch = self.refresh_epoch.wrapping_add(1);
        self.rows.clear();
        self.cancel_status = None;
        let origin = self.return_focus.take();
        let offset = self.return_scroll.take();
        let _ = self.root.update(cx, |root, cx| {
            panel_stack::retire(&mut root.panel_stack, Panel::ActivityDialog);
            if owned && !root.restart_admission.is_fenced() {
                if let Some(origin) = origin.filter(|focus| root.focus_handle.contains(focus, window)) {
                    if let Some(offset) = offset { root.root_scroll.set_offset(offset); }
                    origin.focus(window, cx);
                } else {
                    root.query.focus_handle(cx).focus(window, cx);
                }
            }
            cx.notify();
        });
        cx.notify();
    }

    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>, held: bool) -> bool {
        let contexts = window.context_stack();
        if !self.open || !self.focus.contains_focused(window, cx) || window.has_active_dialog(cx)
            || context_stack_has_primary(&contexts, "PopupMenu") || context_stack_has_primary(&contexts, "Dialog")
            || context_stack_has_primary(&contexts, "AlertDialog") || context_stack_has_primary(&contexts, "List") {
            return false;
        }
        if !held && !self.escape_waiting { self.escape_waiting = true; self.close(window, cx); }
        true
    }

    fn cancel(&mut self, key: ActivityKey, expected_view: EntityId, epoch: u64, window: &mut Window, cx: &mut Context<Self>) {
        if expected_view != cx.entity_id() || !refresh_is_current(self.open,
            self.visibility.active.load(Ordering::Acquire), self.visibility.visible.load(Ordering::Acquire),
            self.viewport_visible && self.measured_visible.get(), epoch, self.refresh_epoch)
            || !window.is_window_active() || window.has_active_dialog(cx) { return; }
        let contexts = window.context_stack();
        if context_stack_has_primary(&contexts, "PopupMenu") || context_stack_has_primary(&contexts, "Dialog")
            || context_stack_has_primary(&contexts, "AlertDialog") || context_stack_has_primary(&contexts, "List") { return; }
        let permit = self.root.update(cx, |root, cx| {
            if !root.activity_view.as_ref().is_some_and(|view| view.entity_id() == expected_view)
                || !root.root_window.is_some_and(|handle| handle.window_id() == window.window_handle().window_id())
                || root.favorites_toolbar_visibility_revision(window).is_none()
                || !root.restart_admission.is_ready() || root.action_sheet.is_some()
                || root.context_favorite_popup.is_some() || window.has_active_dialog(cx) { return None; }
            root.restart_admission.admit()
        }).ok().flatten();
        let Some(_permit) = permit else { return; };
        let current_hash = self.hash_session.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
            .current_request_id().map(|id| id.get());
        let result = cancel_exact_hash(&self.center, &self.rows, key, current_hash);
        self.cancel_status = Some(match result {
            ActivityCancelResult::Requested => "Cancellation requested.",
            ActivityCancelResult::HandlerFailed => "Could not request cancellation.",
            ActivityCancelResult::Unsupported | ActivityCancelResult::Stale | ActivityCancelResult::Terminal => "This operation is no longer cancellable.",
        });
        self.refresh(epoch, cx);
    }
}

impl RootWindow {
    pub(super) fn open_activity(&mut self, origin: Option<FocusHandle>, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced() { return; }
        let view = if let Some(view) = &self.activity_view { view.clone() } else {
            let root = cx.entity().downgrade();
            let center = self.activity_center.clone();
            let hash_session = self.hash_session.clone();
            let visibility = self.root_visibility.clone();
            let root_scroll = self.root_scroll.clone();
            let view = cx.new(|cx| ActivityView::new(window, cx, root, center, hash_session, visibility, root_scroll));
            self.activity_view = Some(view.clone());
            view
        };
        let origin = origin.filter(|focus| self.focus_handle.contains(focus, window));
        let offset = self.root_scroll.offset();
        view.update(cx, |view, cx| view.show(origin, offset, window, cx));
        panel_stack::promote(&mut self.panel_stack, Panel::ActivityDialog);
        let index = self.file_search_scroll_item()
            + usize::from(self.file_search.as_ref().is_some_and(|view| view.read(cx).open))
            + usize::from(self.downloads_triage_open(cx))
            + usize::from(self.plugin_editor.as_ref().is_some_and(|view| view.read(cx).open));
        self.root_scroll.scroll_to_top_of_item(index);
        cx.notify();
    }
}

impl Render for ActivityView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root_scroll = self.root_scroll.clone();
        let measured = self.measured_visible.clone();
        let owner = cx.entity().downgrade();
        let view_id = cx.entity_id();
        let epoch = self.refresh_epoch;
        let current_hash = self.hash_session.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
            .current_request_id().map(|id| id.get());
        let rows = self.rows.iter().map(|row| {
            let key = row.key;
            let (row_id, label_id) = row_element_ids(key.operation_id().get());
            let progress = row.progress.filter(|progress| progress.unit == multi_launcher::activity::ActivityProgressUnit::Bytes)
                .map(|progress| match progress.total {
                    Some(total) => format!(" · {} / {} bytes", progress.completed, total),
                    None => format!(" · {} bytes", progress.completed),
                }).unwrap_or_default();
            div().id(SharedString::from(row_id)).flex().flex_row().flex_wrap().items_center().gap_2().w_full()
                .child(Label::new(format!("{} · {}{}", row.operation.label(), row.lifecycle.label(), progress)).id(SharedString::from(label_id)))
                .when(eligible_cancel(row, key, current_hash), |line| line.child(
                    Button::new(SharedString::from(format!("activity-cancel-{}", key.operation_id().get())))
                        .label("Cancel current operation").tab_stop(true)
                        .on_click(cx.listener(move |this, _, window, cx| this.cancel(key, view_id, epoch, window, cx)))
                ))
        }).collect::<Vec<_>>();
        div().id("activity-pane").relative().key_context("ActivityPane").track_focus(&self.focus)
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && event.keystroke.modifiers == Modifiers::default()
                    && this.escape(window, cx, event.is_held) { cx.stop_propagation(); }
            }))
            .capture_key_up(cx.listener(|this, event: &KeyUpEvent, _, _| {
                if event.keystroke.key == "escape" { this.escape_waiting = false; }
            }))
            .capture_action(cx.listener(|this, _: &gpui_kit::base::input::Escape, window, cx| {
                if this.escape(window, cx, false) { cx.stop_propagation(); }
            }))
            .flex().flex_col().gap_2().w_full().p_3()
            .child(Label::new("Activity"))
            .child(Button::new("activity-close").label("Close").tab_stop(true).track_focus(&self.close_focus)
                .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))))
            .children(self.cancel_status.map(|status| Label::new(status).id("activity-cancel-status")))
            .child(div().id("activity-rows").flex().flex_col().gap_2().max_h(px(300.0))
                .overflow_y_scroll().track_scroll(&self.scroll)
                .children(rows).when(self.rows.is_empty(), |body| body.child(Label::new("No activity."))))
            .child(canvas(move |bounds, window, app| {
                let visible = bounds.intersects(&root_scroll.bounds());
                if measured.replace(visible) != visible {
                    let owner = owner.clone();
                    window.defer(app, move |_, app| {
                        let _ = owner.update(app, |view, cx| view.viewport_changed(visible, cx));
                    });
                }
            }, |_, _, _, _| {}).absolute().size_full())
    }
}

#[cfg(test)]
mod tests {
    use super::{cancel_exact_hash, eligible_cancel, provider_is_current, refresh_is_current, row_element_ids};
    use multi_launcher::{commands::{Command, DialogCommand, parse_action}, plugin::Plugin, plugins::activity::ActivityPlugin};
    use std::collections::{HashMap, HashSet};

    #[test]
    fn stock_activity_route_is_exact_and_search_capability_gated() {
        let action = ActivityPlugin.search("activity").remove(0);
        assert!(matches!(parse_action(&action), Ok(Command::Dialog(DialogCommand::Activity))));
        assert!(provider_is_current("activity", &action, None, None));
        assert!(!provider_is_current("activity more", &action, None, None));
        let disabled = HashMap::from([("activity".to_owned(), Vec::new())]);
        assert!(!provider_is_current("activity", &action, None, Some(&disabled)));
        assert!(!provider_is_current("activity", &action, Some(&HashSet::new()), None));
        let mut forged = action.clone(); forged.args = Some("private".into());
        assert!(!provider_is_current("activity", &forged, None, None));
    }

    #[test]
    fn refresh_requires_live_visible_exact_mount() {
        assert!(refresh_is_current(true, true, true, true, 4, 4));
        assert!(!refresh_is_current(false, true, true, true, 4, 4));
        assert!(!refresh_is_current(true, false, true, true, 4, 4));
        assert!(!refresh_is_current(true, true, false, true, 4, 4));
        assert!(!refresh_is_current(true, true, true, false, 4, 4));
        assert!(!refresh_is_current(true, true, true, true, 4, 5));
    }

    #[test]
    fn activity_row_and_label_ids_are_stable_and_distinct_per_operation() {
        let first = row_element_ids(1);
        let second = row_element_ids(2);
        assert_eq!(first, row_element_ids(1));
        assert_ne!(first.0, first.1);
        assert_ne!(first.0, second.0);
        assert_ne!(first.1, second.1);
        assert_ne!(first.0, second.1);
    }

    #[test]
    fn cancel_targets_only_current_hash_key_and_rechecks_shared_lifecycle() {
        use multi_launcher::activity::{ActivityCancelResult, ActivityCenter, ActivityIncarnation, ActivityOperation, ActivityOutcome, ActivityOwner, ActivityRetirement, ActivitySlot};
        use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
        let center = ActivityCenter::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let old = center.start(ActivityOwner::Hash, ActivitySlot::Primary, ActivityIncarnation::new(21), ActivityOperation::CalculateHash).unwrap();
        let old_calls = calls.clone();
        old.set_cancel_handler(move || { old_calls.fetch_add(1, Ordering::SeqCst); });
        let old_rows = center.snapshot().rows;
        let current = center.start(ActivityOwner::Hash, ActivitySlot::Primary, ActivityIncarnation::new(22), ActivityOperation::CalculateHash).unwrap();
        let current_calls = calls.clone();
        current.set_cancel_handler(move || { current_calls.fetch_add(1, Ordering::SeqCst); });
        assert_eq!(cancel_exact_hash(&center, &old_rows, old.key(), Some(22)), ActivityCancelResult::Stale);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let rows = center.snapshot().rows;
        assert_eq!(cancel_exact_hash(&center, &rows, current.key(), Some(22)), ActivityCancelResult::Requested);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(cancel_exact_hash(&center, &rows, current.key(), Some(22)), ActivityCancelResult::Stale);
        let cancelling = center.snapshot().rows;
        assert!(!eligible_cancel(cancelling.iter().find(|row| row.key == current.key()).unwrap(), current.key(), Some(22)));
        current.finish(ActivityOutcome::Cancelled, None, ActivityRetirement::Confirmed);
        assert_eq!(cancel_exact_hash(&center, &rows, current.key(), Some(22)), ActivityCancelResult::Stale);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
