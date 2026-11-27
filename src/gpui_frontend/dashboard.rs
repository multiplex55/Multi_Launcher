//! Profile-scoped, read-only Dashboard source. Rendering is owned separately.
use super::*;
use multi_launcher::dashboard::config::DashboardConfig;
use multi_launcher::dashboard::widgets::QuickToolsWidget;
use multi_launcher::dashboard::{
    ActiveTimersConfig, DashboardLayout, QuickToolsConfig, WidgetAction, WidgetRegistry,
};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread::JoinHandle;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(super) enum SlotIdentity {
    Id {
        id: String,
        widget: String,
        occurrence: usize,
    },
    Position {
        widget: String,
        row: usize,
        col: usize,
        occurrence: usize,
    },
}

#[derive(Clone, Debug)]
pub(super) enum Widget {
    Timers(ActiveTimersConfig),
    QuickTools {
        config: QuickToolsConfig,
        actions: Vec<QuickTool>,
    },
}

#[derive(Clone, Debug)]
pub(super) struct QuickTool {
    pub index: usize,
    pub occurrence: usize,
    pub action: WidgetAction,
}

#[derive(Clone, Debug)]
pub(super) struct Slot {
    pub identity: SlotIdentity,
    pub widget: Widget,
    pub row: usize,
    pub col: usize,
    pub col_span: usize,
}

#[derive(Clone, Debug)]
pub(super) struct Snapshot {
    pub generation: u64,
    pub layout: DashboardLayout,
    pub slots: Vec<Slot>,
}

#[derive(Default)]
pub(super) struct UiState {
    outer: ScrollHandle,
    slots: HashMap<SlotIdentity, ScrollHandle>,
    focuses: HashMap<(SlotIdentity, usize, usize), ToolFocus>,
}

struct ToolFocus {
    handle: FocusHandle,
    _subscription: Subscription,
    stamp: (u64, u64, u64, usize, usize),
}

fn body_visible(enabled: bool, show_empty: bool, query: &str, has_slots: bool) -> bool {
    enabled && show_empty && query.trim().is_empty() && has_slots
}

fn current_slot(snapshot: &Snapshot, generation: u64, identity: &SlotIdentity) -> bool {
    snapshot.generation == generation
        && snapshot.slots.iter().any(|slot| &slot.identity == identity)
}

fn manual_outcome(action: &Action) -> Option<CommandOutcome> {
    let invocation = parse_command(action.clone(), None, ActivationSource::Dashboard).ok()?;
    let Command::Query(command @ QueryCommand::Set { .. }) = invocation.command else {
        return None;
    };
    let outcome = handle_query(&command, ActivationSource::Dashboard);
    query_set_outcome_text(&outcome)?;
    Some(outcome)
}

fn matching_tool(
    snapshot: &Snapshot,
    generation: u64,
    identity: &SlotIdentity,
    index: usize,
    occurrence: usize,
    action: &Action,
) -> bool {
    current_slot(snapshot, generation, identity)
        && snapshot
            .slots
            .iter()
            .find(|slot| &slot.identity == identity)
            .is_some_and(|slot| match &slot.widget {
                Widget::QuickTools { actions, .. } => actions.iter().any(|tool| {
                    tool.index == index
                        && tool.occurrence == occurrence
                        && &tool.action.action == action
                }),
                _ => false,
            })
}

fn running_rows(
    rows: &[multi_launcher::plugins::timer_session::TimerRow],
    count: usize,
) -> Vec<multi_launcher::plugins::timer_session::TimerRow> {
    let mut running: Vec<_> = rows.iter().filter(|row| !row.paused).cloned().collect();
    running.sort_by_key(|row| row.remaining);
    running.truncate(count);
    running
}

fn format_remaining(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs >= 3600 {
        format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
    } else {
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }
}

fn project(layout: &DashboardLayout) -> Vec<Slot> {
    let mut occurrences = HashMap::new();
    layout
        .slots
        .iter()
        .filter_map(|slot| {
            let widget = match slot.widget.as_str() {
                "timers" => Widget::Timers(
                    serde_json::from_value(slot.settings.clone()).unwrap_or_default(),
                ),
                "quick_tools" => {
                    let config: QuickToolsConfig =
                        serde_json::from_value(slot.settings.clone()).unwrap_or_default();
                    let mut duplicates = HashMap::new();
                    let actions: Vec<QuickTool> = config
                        .queries
                        .iter()
                        .enumerate()
                        .filter(|(_, entry)| !entry.auto_submit)
                        .filter_map(|(index, entry)| {
                            let action = QuickToolsWidget::action_for(entry)?;
                            let occurrence = duplicates
                                .entry(action.action.action.clone())
                                .or_insert(0usize);
                            let tool = QuickTool {
                                index,
                                occurrence: *occurrence,
                                action,
                            };
                            *occurrence += 1;
                            Some(tool)
                        })
                        .collect();
                    if actions.is_empty() {
                        return None;
                    }
                    Widget::QuickTools { config, actions }
                }
                _ => return None,
            };
            let key = (
                slot.id.clone(),
                slot.widget.clone(),
                if slot.id.is_some() { 0 } else { slot.row },
                if slot.id.is_some() { 0 } else { slot.col },
            );
            let occurrence = occurrences.entry(key).or_insert(0usize);
            let identity = if let Some(id) = &slot.id {
                SlotIdentity::Id {
                    id: id.clone(),
                    widget: slot.widget.clone(),
                    occurrence: *occurrence,
                }
            } else {
                SlotIdentity::Position {
                    widget: slot.widget.clone(),
                    row: slot.row,
                    col: slot.col,
                    occurrence: *occurrence,
                }
            };
            *occurrence += 1;
            Some(Slot {
                identity,
                widget,
                row: slot.row,
                col: slot.col,
                col_span: slot.col_span,
            })
        })
        .collect()
}

pub(super) struct Source {
    pub settings: multi_launcher::settings::DashboardSettings,
    state: Arc<Mutex<Option<Arc<Snapshot>>>>,
    active: Arc<AtomicBool>,
    wake: Arc<HashWakeSignal>,
    sender: SyncSender<()>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Source {
    pub fn start(
        settings: multi_launcher::settings::DashboardSettings,
        wake: Arc<HashWakeSignal>,
    ) -> Self {
        let path =
            DashboardConfig::path_for(settings.config_path.as_deref().unwrap_or("dashboard.json"));
        let state = Arc::new(Mutex::new(None::<Arc<Snapshot>>));
        let active = Arc::new(AtomicBool::new(true));
        let (sender, receiver) = sync_channel::<()>(1);
        let worker = {
            let path = path.clone();
            let state = state.clone();
            let active = active.clone();
            let wake = wake.clone();
            let sender = sender.clone();
            std::thread::spawn(move || {
                let callback_active = active.clone();
                let watcher = multi_launcher::common::json_watch::watch_json(&path, move || {
                    if callback_active.load(Ordering::Acquire) {
                        let _ = sender.try_send(());
                    }
                });
                let registry = WidgetRegistry::with_defaults();
                let mut layout = DashboardLayout::initial(&path, &registry);
                if let Err(error) = &watcher {
                    layout
                        .warnings
                        .push(format!("Dashboard watch failed: {error}"));
                }
                let mut generation = 1u64;
                loop {
                    if !active.load(Ordering::Acquire) {
                        break;
                    }
                    let snapshot = Snapshot {
                        generation,
                        slots: project(&layout),
                        layout: layout.clone(),
                    };
                    let mut published = state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if !active.load(Ordering::Acquire) {
                        break;
                    }
                    *published = Some(Arc::new(snapshot));
                    drop(published);
                    wake.notify();
                    if receiver.recv().is_err() || !active.load(Ordering::Acquire) {
                        break;
                    }
                    let next = match DashboardLayout::reload(&path, &registry) {
                        Ok(next) => next,
                        Err(error) => {
                            let mut retained = layout.clone();
                            retained.warnings = vec![error.to_string()];
                            retained
                        }
                    };
                    if next != layout {
                        generation = generation.wrapping_add(1);
                        layout = next;
                    }
                }
                drop(watcher);
            })
        };
        Self {
            settings,
            state,
            active,
            wake,
            sender,
            worker: Mutex::new(Some(worker)),
        }
    }

    pub fn snapshot(&self) -> Option<Arc<Snapshot>> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn claim(&self, predicate: impl FnOnce(&Snapshot) -> bool) -> bool {
        let snapshot = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.active.load(Ordering::Acquire) && snapshot.as_deref().is_some_and(predicate)
    }

    pub fn shutdown(&self) {
        {
            let _published = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            self.active.store(false, Ordering::Release);
        }
        let _ = self.sender.try_send(());
        if let Some(worker) = self
            .worker
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = worker.join();
        }
        self.wake.close();
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl RootWindow {
    pub(super) fn dashboard_body_owned(&self, cx: &App) -> bool {
        body_visible(
            self.dashboard_source.settings.enabled,
            self.dashboard_source.settings.show_when_query_empty,
            self.query.read(cx).value().as_str(),
            self.dashboard_source
                .snapshot()
                .is_some_and(|snapshot| !snapshot.slots.is_empty()),
        )
    }

    fn dashboard_action_current(
        &self,
        generation: u64,
        identity: &SlotIdentity,
        epoch: u64,
        revision: u64,
        window: &Window,
        cx: &App,
    ) -> bool {
        self.dashboard_body_owned(cx)
            && !self.restart_admission.is_fenced()
            && self.dashboard_epoch == epoch
            && self
                .root_window
                .is_some_and(|host| host.window_id() == window.window_handle().window_id())
            && window.is_window_active()
            && self.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            && self.native_window.is_some_and(|native| {
                current_foreground_identity() == Some(native.identity)
                    && json_utility::native_host_current(
                        &self.root_visibility,
                        revision,
                        Some(native),
                        false,
                    )
            })
            && self
                .dashboard_source
                .snapshot()
                .is_some_and(|snapshot| current_slot(&snapshot, generation, identity))
    }

    fn dashboard_tick(&mut self, cx: &mut Context<Self>) {
        if self.dashboard_ticking {
            return;
        }
        self.dashboard_ticking = true;
        cx.spawn(async move |root, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let keep = root
                    .update(cx, |root, cx| {
                        let keep = root.root_visibility.visible.load(Ordering::Acquire)
                            && root.dashboard_body_owned(cx)
                            && root.dashboard_source.snapshot().is_some_and(|snapshot| {
                                snapshot
                                    .slots
                                    .iter()
                                    .any(|slot| matches!(&slot.widget, Widget::Timers(_)))
                            })
                            && root.dashboard_timer_rows.iter().any(|row| !row.paused);
                        if !keep {
                            root.dashboard_ticking = false;
                        }
                        cx.notify();
                        keep
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn render_dashboard(
        &mut self,
        width: f32,
        spacing: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let snapshot = self.dashboard_source.snapshot()?;
        if !body_visible(
            self.dashboard_source.settings.enabled,
            self.dashboard_source.settings.show_when_query_empty,
            self.query.read(cx).value().as_str(),
            !snapshot.slots.is_empty(),
        ) {
            return None;
        }
        let has_timers = snapshot
            .slots
            .iter()
            .any(|slot| matches!(&slot.widget, Widget::Timers(_)));
        if has_timers
            && let Some(rows) = self
                .timer_session
                .as_ref()
                .and_then(|session| session.try_rows())
        {
            self.dashboard_timer_rows = rows;
        }
        let running = running_rows(&self.dashboard_timer_rows, usize::MAX);
        if has_timers && !running.is_empty() && self.root_visibility.visible.load(Ordering::Acquire)
        {
            self.dashboard_tick(cx);
        }
        let columns = usize::from(snapshot.layout.config.grid.cols.max(1));
        let epoch = self.dashboard_epoch;
        let revision = self
            .root_visibility
            .request_revision
            .load(Ordering::Acquire);
        let outer = self.dashboard_ui.outer.clone();
        let mut live_slots = HashSet::new();
        let mut live_focuses = HashSet::new();
        let mut body = div()
            .id("dashboard-body")
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .track_scroll(&outer);
        for (index, warning) in snapshot.layout.warnings.iter().enumerate() {
            body = body.child(
                Label::new(warning.clone())
                    .id(SharedString::from(format!("dashboard-warning-{index}"))),
            );
        }
        let max_row = snapshot
            .slots
            .iter()
            .map(|slot| slot.row)
            .max()
            .unwrap_or(0);
        for row_index in 0..=max_row {
            let mut row = div()
                .id(SharedString::from(format!("dashboard-row-{row_index}")))
                .flex()
                .gap_2()
                .w_full();
            let mut entries: Vec<_> = snapshot
                .slots
                .iter()
                .enumerate()
                .filter(|(_, slot)| slot.row == row_index)
                .collect();
            entries.sort_by_key(|(_, slot)| slot.col);
            let mut cursor = 0;
            for (slot_index, slot) in entries {
                if slot.col > cursor {
                    row = row.child(
                        div()
                            .flex_grow((slot.col - cursor) as f32)
                            .flex_basis(px(0.0)),
                    );
                }
                cursor = slot.col.saturating_add(slot.col_span);
                let mut card = div()
                    .id(SharedString::from(format!(
                        "dashboard-slot-{}-{slot_index}",
                        snapshot.generation
                    )))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .flex_grow(slot.col_span as f32)
                    .flex_basis(px(0.0))
                    .min_w(px(0.0));
                match &slot.widget {
                    Widget::Timers(config) => {
                        let identity = slot.identity.clone();
                        let generation = snapshot.generation;
                        card = card.child(Label::new("Active Timers"));
                        card = card.child(
                            Button::new(SharedString::from(format!(
                                "dashboard-new-timer-{slot_index}"
                            )))
                            .label("New Timer")
                            .on_click(cx.listener(
                                move |root, _, window, cx| {
                                    if window.has_active_dialog(cx)
                                        || !root.dashboard_action_current(
                                            generation, &identity, epoch, revision, window, cx,
                                        )
                                    {
                                        return;
                                    }
                                    if let Some(_permit) = root.restart_admission.admit()
                                        && root.dashboard_source.claim(|snapshot| {
                                            current_slot(snapshot, generation, &identity)
                                                && snapshot.slots.iter().any(|slot| {
                                                    slot.identity == identity
                                                        && matches!(&slot.widget, Widget::Timers(_))
                                                })
                                        })
                                    {
                                        root.open_timer(false, window, cx);
                                    }
                                },
                            )),
                        );
                        if running.is_empty() {
                            card = card.child(Label::new("No running timers"));
                        }
                        for timer in running.iter().take(config.count) {
                            let time = format_remaining(timer.remaining);
                            card = card.child(
                                Label::new(format!("{} – {} left", timer.label, time)).id(
                                    SharedString::from(format!(
                                        "dashboard-timer-{slot_index}-{}",
                                        timer.id
                                    )),
                                ),
                            );
                        }
                    }
                    Widget::QuickTools { config, actions } => {
                        live_slots.insert(slot.identity.clone());
                        let scroll = self
                            .dashboard_ui
                            .slots
                            .entry(slot.identity.clone())
                            .or_default()
                            .clone();
                        card = card.child(Label::new("Quick Tools"));
                        let card_width =
                            (width * slot.col_span as f32 / columns as f32 - spacing).max(0.0);
                        let per_row = multi_launcher::dashboard::widgets::effective_columns(
                            config.preferred_columns,
                            card_width,
                            spacing,
                        );
                        let mut tools = div()
                            .id(SharedString::from(format!("dashboard-tools-{slot_index}")))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .overflow_y_scroll()
                            .track_scroll(&scroll)
                            .max_h(px(config.visible_rows.max(1) as f32 * 36.0));
                        for (tool_row_index, chunk) in actions.chunks(per_row).enumerate() {
                            let mut tool_row = div().flex().gap_2().w_full();
                            for tool in chunk {
                                let identity = slot.identity.clone();
                                let generation = snapshot.generation;
                                let index = tool.index;
                                let occurrence = tool.occurrence;
                                let focus_key = (identity.clone(), index, occurrence);
                                live_focuses.insert(focus_key.clone());
                                let stamp =
                                    (generation, epoch, revision, row_index, tool_row_index);
                                let existing = self.dashboard_ui.focuses.remove(&focus_key);
                                let focus = existing
                                    .as_ref()
                                    .map(|entry| entry.handle.clone())
                                    .unwrap_or_else(|| cx.focus_handle());
                                let entry = if let Some(entry) =
                                    existing.filter(|entry| entry.stamp == stamp)
                                {
                                    entry
                                } else {
                                    let focus_identity = identity.clone();
                                    let inner = scroll.clone();
                                    let outer = outer.clone();
                                    let outer_index = snapshot.layout.warnings.len() + row_index;
                                    let subscription =
                                        cx.on_focus(&focus, window, move |root, window, cx| {
                                            if root.dashboard_action_current(
                                                generation,
                                                &focus_identity,
                                                epoch,
                                                revision,
                                                window,
                                                cx,
                                            ) {
                                                inner.scroll_to_item(tool_row_index);
                                                outer.scroll_to_item(outer_index);
                                                cx.notify();
                                            }
                                        });
                                    ToolFocus {
                                        handle: focus.clone(),
                                        _subscription: subscription,
                                        stamp,
                                    }
                                };
                                self.dashboard_ui.focuses.insert(focus_key, entry);
                                let action = tool.action.action.clone();
                                let label = action.label.clone();
                                tool_row = tool_row.child(
                                    Button::new(SharedString::from(format!(
                                        "dashboard-tool-{slot_index}-{index}-{occurrence}"
                                    )))
                                    .label(label.clone())
                                    .accessibility_label(label.clone())
                                    .tooltip(label)
                                    .track_focus(&focus)
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .on_click(cx.listener(move |root, _, window, cx| {
                                        if window.has_active_dialog(cx)
                                            || !root.dashboard_action_current(
                                                generation, &identity, epoch, revision, window, cx,
                                            )
                                        {
                                            return;
                                        }
                                        let Some(outcome) = manual_outcome(&action) else {
                                            return;
                                        };
                                        if let Some(permit) = root.restart_admission.admit()
                                            && root.dashboard_source.claim(|snapshot| {
                                                matching_tool(
                                                    snapshot, generation, &identity, index,
                                                    occurrence, &action,
                                                )
                                            })
                                        {
                                            let _ = root.apply_query_set_outcome(
                                                outcome, permit, window, cx,
                                            );
                                        }
                                    })),
                                );
                            }
                            tools = tools.child(tool_row);
                        }
                        card = card.child(tools);
                    }
                }
                row = row.child(card);
            }
            if cursor < columns {
                row = row.child(
                    div()
                        .flex_grow((columns - cursor) as f32)
                        .flex_basis(px(0.0)),
                );
            }
            body = body.child(row);
        }
        self.dashboard_ui
            .slots
            .retain(|key, _| live_slots.contains(key));
        self.dashboard_ui
            .focuses
            .retain(|key, _| live_focuses.contains(key));
        Some(body.into_any_element())
    }

    pub(super) fn start_dashboard_wake(
        &mut self,
        wake: Arc<HashWakeSignal>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |root, cx| {
            while wake.wait().await {
                if root.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::super::wake::HashWakeSignal;
    use super::{
        SlotIdentity, Snapshot, Source, Widget, body_visible, current_slot, format_remaining,
        manual_outcome, matching_tool, project, running_rows,
    };
    use multi_launcher::dashboard::{DashboardLayout, WidgetRegistry};
    use std::sync::Arc;

    #[test]
    fn projection_preserves_stock_manual_tools_and_slot_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dashboard.json");
        std::fs::write(&path, br#"{"slots":[{"id":"tools","widget":"quick_tools","row":0,"col":0,"settings":{"queries":["  sys  ","sys",{"query":"danger","auto_submit":true}," ","net"],"preferred_columns":2,"visible_rows":3}},{"widget":"active_timers","row":1,"col":0}]}"#).unwrap();
        let layout = DashboardLayout::load(&path, &WidgetRegistry::with_defaults()).unwrap();
        let slots = project(&layout);
        assert_eq!(slots.len(), 2);
        assert_eq!(
            slots[0].identity,
            SlotIdentity::Id {
                id: "tools".into(),
                widget: "quick_tools".into(),
                occurrence: 0
            }
        );
        let Widget::QuickTools { config, actions } = &slots[0].widget else {
            panic!("expected tools");
        };
        assert_eq!((config.preferred_columns, config.visible_rows), (2, 3));
        assert_eq!(
            actions
                .iter()
                .map(|tool| (
                    tool.index,
                    tool.occurrence,
                    tool.action.action.action.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![
                (0, 0, "query:sys"),
                (1, 1, "query:sys"),
                (4, 0, "query:net")
            ]
        );
        assert!(
            matches!(slots[1].identity, SlotIdentity::Position { ref widget, row: 1, col: 0, occurrence: 0 } if widget == "timers")
        );
        assert!(matches!(&slots[1].widget, Widget::Timers(_)));
        let identity = slots[0].identity.clone();
        let snapshot = Snapshot {
            generation: 4,
            layout,
            slots,
        };
        assert!(current_slot(&snapshot, 4, &identity));
        assert!(!current_slot(&snapshot, 3, &identity));
        assert!(body_visible(true, true, " \t", true));
        assert!(!body_visible(true, true, "o", true));
        assert!(!body_visible(false, true, "", true));
        assert!(!body_visible(true, false, "", true));
        assert!(!body_visible(true, true, "", false));
        let Widget::QuickTools { actions, .. } = &snapshot.slots[0].widget else {
            unreachable!()
        };
        let first = &actions[0];
        let outcome = manual_outcome(&first.action.action).unwrap();
        assert_eq!(super::query_set_outcome_text(&outcome), Some("sys"));
        assert!(matching_tool(
            &snapshot,
            4,
            &identity,
            first.index,
            first.occurrence,
            &first.action.action
        ));
        assert!(!matching_tool(
            &snapshot,
            3,
            &identity,
            first.index,
            first.occurrence,
            &first.action.action
        ));
        assert!(!matching_tool(
            &snapshot,
            4,
            &identity,
            first.index,
            1,
            &first.action.action
        ));
        let auto = multi_launcher::dashboard::QuickToolEntry {
            query: "danger".into(),
            auto_submit: true,
        };
        assert!(
            manual_outcome(
                &multi_launcher::dashboard::widgets::QuickToolsWidget::action_for(&auto)
                    .unwrap()
                    .action
            )
            .is_none()
        );
        let mut empty = snapshot.layout.clone();
        empty.slots.retain(|slot| slot.widget == "quick_tools");
        empty.slots[0].settings =
            serde_json::json!({"queries":[" ", {"query":"danger","auto_submit":true}]});
        assert!(project(&empty).is_empty());
        assert!(!body_visible(true, true, "", !project(&empty).is_empty()));
    }

    #[test]
    fn timer_projection_filters_sorts_caps_and_formats() {
        use multi_launcher::plugins::timer_session::{TimerKind, TimerRow};
        let row = |id, seconds, paused| TimerRow {
            id,
            generation: 1,
            label: id.to_string(),
            remaining: std::time::Duration::from_secs(seconds),
            kind: TimerKind::Countdown,
            paused,
        };
        let rows = vec![row(1u64, 3701, false), row(2, 20, true), row(3, 65, false)];
        let selected = running_rows(&rows, 1);
        assert_eq!(
            selected.iter().map(|row| row.id).collect::<Vec<_>>(),
            vec![3]
        );
        assert_eq!(format_remaining(selected[0].remaining), "01:05");
        assert_eq!(format_remaining(rows[0].remaining), "01:01:41");
    }

    #[test]
    fn source_shutdown_joins_worker_without_creating_layout() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dashboard.json");
        let mut settings = multi_launcher::settings::DashboardSettings::default();
        settings.config_path = Some(path.to_string_lossy().into_owned());
        let source = Source::start(settings, Arc::new(HashWakeSignal::default()));
        source.shutdown();
        assert!(!source.active.load(std::sync::atomic::Ordering::Acquire));
        assert!(source.worker.lock().unwrap().is_none());
        assert!(!path.exists());
    }

    #[test]
    fn source_reload_retains_bad_layout_and_publishes_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dashboard.json");
        std::fs::write(&path, br#"{"slots":[{"widget":"timers","row":0,"col":0}]}"#).unwrap();
        let mut settings = multi_launcher::settings::DashboardSettings::default();
        settings.config_path = Some(path.to_string_lossy().into_owned());
        let source = Source::start(settings, Arc::new(HashWakeSignal::default()));
        let wait = |predicate: &dyn Fn(&super::Snapshot) -> bool| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                if let Some(snapshot) = source.snapshot() {
                    if predicate(&snapshot) {
                        break snapshot;
                    }
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "Dashboard worker did not publish"
                );
                std::thread::yield_now();
            }
        };
        let first = wait(&|snapshot| !snapshot.slots.is_empty());
        std::fs::write(&path, b"{bad").unwrap();
        // Explicit wake exercises the same coalesced worker path without depending on OS delivery timing.
        let _ = source.sender.try_send(());
        let invalid = wait(&|snapshot| {
            snapshot.generation != first.generation && !snapshot.layout.warnings.is_empty()
        });
        assert_eq!(invalid.slots[0].identity, first.slots[0].identity);
        std::fs::write(
            &path,
            br#"{"slots":[{"widget":"quick_tools","row":0,"col":0}]}"#,
        )
        .unwrap();
        let _ = source.sender.try_send(());
        let recovered = wait(&|snapshot| {
            snapshot.generation != invalid.generation
                && matches!(&snapshot.slots[0].widget, Widget::QuickTools { .. })
        });
        assert!(recovered.layout.warnings.is_empty());
        source.shutdown();
    }
}
