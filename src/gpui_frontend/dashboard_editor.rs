//! Docked editor for the canonical checked Dashboard draft.
use super::*;
use gpui_kit::base::Selectable;
use multi_launcher::dashboard::checked_editor::{DashboardDraft, SaveError, SlotToken};
use multi_launcher::dashboard::widgets::WidgetRegistry;
use multi_launcher::plugin::Plugin;

pub(super) const WAIT: &str = "Save or discard the Dashboard draft before closing.";

#[derive(Clone)]
struct PaintedSlot {
    token: SlotToken,
    widget: String,
    row: i32,
    col: i32,
    row_span: u8,
    col_span: u8,
}

fn adjusted_coordinate(value: i32, delta: i32, minimum: i32) -> i32 {
    value.saturating_add(delta).max(minimum)
}

pub(super) fn current_provider(
    query: &str,
    action: &Action,
    enabled: Option<&HashSet<String>>,
    caps: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    action.action == "dashboard:settings"
        && action.args.is_none()
        && search_providers::search_routed(
            [&multi_launcher::plugins::settings::SettingsPlugin as &dyn Plugin],
            query,
            enabled,
            caps,
        )
        .iter()
        .any(|(name, candidate)| name == "settings" && candidate == action)
}

pub(super) struct Editor {
    pub open: bool,
    root: WeakEntity<RootWindow>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    host: AnyWindowHandle,
    focus: FocusHandle,
    draft: Option<Arc<Mutex<DashboardDraft>>>,
    painted: Option<(
        multi_launcher::dashboard::config::GridConfig,
        Vec<PaintedSlot>,
    )>,
    selected: Option<SlotToken>,
    swap: Option<SlotToken>,
    dirty: bool,
    busy: bool,
    unverified: bool,
    revision: u64,
    status: Option<String>,
    lease: Option<Arc<AtomicBool>>,
    blur: Option<Subscription>,
    activation: Option<Subscription>,
}

impl Editor {
    fn new(root: WeakEntity<RootWindow>, window: &Window, cx: &mut Context<Self>) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        Self {
            open: true,
            root,
            owner,
            mount,
            host: window.window_handle(),
            focus: cx.focus_handle(),
            draft: None,
            painted: None,
            selected: None,
            swap: None,
            dirty: false,
            busy: false,
            unverified: false,
            revision: 0,
            status: None,
            lease: None,
            blur: None,
            activation: None,
        }
    }
    pub(super) fn blocks_close(&self) -> bool {
        self.busy || (self.open && (self.dirty || self.unverified))
    }
    pub(super) fn revoke(&mut self) {
        if let Some(lease) = &self.lease {
            lease.store(false, Ordering::Release);
        }
    }
    fn current(&self, window: &Window, cx: &App) -> bool {
        self.open
            && self.mount.is_current()
            && self.host.window_id() == window.window_handle().window_id()
            && window.is_window_active()
            && self.root.upgrade().is_some_and(|root| {
                let root = root.read(cx);
                !root.restart_admission.is_fenced()
                    && root.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
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
    fn presentation_stamp(&self, cx: &App) -> Option<(NativeWindow, RootVisibility, u64)> {
        let root = self.root.upgrade()?;
        let root = root.read(cx);
        let native = root.native_window?;
        let visibility = root.root_visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        Some((native, visibility, revision))
    }
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.current(window, cx) || window.has_active_dialog(cx) {
            return;
        }
        self.begin_load(cx);
    }

    fn begin_load(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let root = root.read(cx);
        let Some(permit) = root.restart_admission.admit() else {
            return;
        };
        let receiver = match root
            .settings_save_workers
            .begin_dashboard_editor_read(root.dashboard_source.path().to_owned())
        {
            Ok(receiver) => receiver,
            Err(error) => {
                self.status = Some(error);
                cx.notify();
                return;
            }
        };
        self.busy = true;
        let receipt = self.mount.clone();
        let host = self.host;
        let task = cx.background_executor().spawn(async move {
            receiver
                .recv()
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()))
        });
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = task.await;
            if let Some(view) = weak.upgrade() {
                let _ = update_kit_window_content(host, &view, cx, |view, _, cx| {
                    if !receipt.is_current() || !receipt.matches(&view.mount) {
                        return;
                    }
                    view.busy = false;
                    match result {
                        Ok(draft) => {
                            view.revision = view.revision.wrapping_add(1);
                            view.painted = None;
                            view.draft = Some(Arc::new(Mutex::new(draft)));
                            view.selected = None;
                            view.swap = None;
                            view.dirty = false;
                            view.unverified = false;
                            view.status = None;
                            let _ = view.root.update(cx, |root, _| {
                                root.dashboard_source.request_reload();
                                if root.restart_status.as_deref() == Some(WAIT) {
                                    root.restart_status = None;
                                }
                            });
                        }
                        Err(error) => {
                            view.status = Some(format!("Could not load Dashboard: {error}"))
                        }
                    }
                    cx.notify();
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn request_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.current(window, cx) || window.has_active_dialog(cx) {
            return;
        }
        if !self.dirty || self.unverified {
            self.load(window, cx);
            return;
        }
        let weak = cx.entity().downgrade();
        let mount = self.mount.clone();
        let revision = self.revision;
        let Some((native, visibility, visible_revision)) = self.presentation_stamp(cx) else {
            return;
        };
        window.open_alert_dialog(cx, move |alert, _, _| {
            let weak = weak.clone();
            let mount = mount.clone();
            let visibility = visibility.clone();
            alert
                .title(Label::new("Discard Dashboard draft and reload?"))
                .description(Label::new(
                    "The current draft will be replaced only if loading succeeds.",
                ))
                .confirm()
                .close_button(true)
                .keyboard(true)
                .on_ok(move |_, _, app| {
                    weak.update(app, |view, cx| {
                        if !mount.is_current()
                            || !mount.matches(&view.mount)
                            || view.revision != revision
                            || view.busy
                            || current_foreground_identity() != Some(native.identity)
                            || !json_utility::native_host_current(
                                &visibility,
                                visible_revision,
                                Some(native),
                                false,
                            )
                        {
                            return false;
                        }
                        view.begin_load(cx);
                        true
                    })
                    .unwrap_or(false)
                })
        });
    }
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy
            || self.unverified
            || !self.dirty
            || !self.current(window, cx)
            || window.has_active_dialog(cx)
        {
            return;
        }
        let Some(draft) = self.draft.clone() else {
            return;
        };
        let Some(focus) = window.focused(cx) else {
            return;
        };
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let root = root.read(cx);
        let Some(permit) = root.restart_admission.admit() else {
            return;
        };
        let Some(native) = root.native_window else {
            return;
        };
        let admission = root.restart_admission.clone();
        let visibility = root.root_visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let mount = self.mount.clone();
        let worker_mount = mount.clone();
        let lease = Arc::new(AtomicBool::new(true));
        let worker_lease = lease.clone();
        let current = Arc::new(move || {
            worker_mount.is_current()
                && worker_lease.load(Ordering::Acquire)
                && !admission.is_fenced()
                && current_foreground_identity() == Some(native.identity)
                && json_utility::native_host_current(&visibility, revision, Some(native), false)
        });
        let workers = root.settings_save_workers.clone();
        let blur = lease.clone();
        self.blur = Some(cx.on_blur(&focus, window, move |_, _, _| {
            blur.store(false, Ordering::Release)
        }));
        let inactive = lease.clone();
        self.activation = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() {
                inactive.store(false, Ordering::Release);
            }
        }));
        self.lease = Some(lease.clone());
        self.busy = true;
        self.status = None;
        let host = self.host;
        let weak = cx.entity().downgrade();
        let draft_revision = self.revision;
        window.defer(cx, move |window, app| {
            let _ = weak.update(app, |view, cx| {
                if !view.busy
                    || !mount.is_current()
                    || !mount.matches(&view.mount)
                    || view.revision != draft_revision
                    || !view
                        .draft
                        .as_ref()
                        .is_some_and(|live| Arc::ptr_eq(live, &draft))
                    || !lease.load(Ordering::Acquire)
                    || !focus.is_focused(window)
                    || !view.current(window, cx)
                    || window.has_active_dialog(cx)
                    || !current()
                {
                    view.busy = false;
                    view.lease = None;
                    view.blur = None;
                    view.activation = None;
                    view.status = Some("Dashboard save was superseded; try again.".into());
                    cx.notify();
                    return;
                }
                let (receiver, token) = match workers.begin_dashboard_editor_save(draft, current) {
                    Ok(value) => value,
                    Err(error) => {
                        view.busy = false;
                        view.lease = None;
                        view.blur = None;
                        view.activation = None;
                        view.status = Some(error);
                        cx.notify();
                        return;
                    }
                };
                let task = cx
                    .background_executor()
                    .spawn(async move { receiver.recv() });
                cx.spawn(async move |weak, cx| {
                    let _permit = permit;
                    let result = task.await;
                    workers.acknowledge_boolean_preference_save(token.id());
                    if let Some(view) = weak.upgrade() {
                        let _ = update_kit_window_content(host, &view, cx, |view, _, cx| {
                            if !mount.is_current() || !mount.matches(&view.mount) {
                                return;
                            }
                            view.busy = false;
                            view.lease = None;
                            view.blur = None;
                            view.activation = None;
                            match result {
                                Ok(Ok(())) => {
                                    view.dirty = false;
                                    view.status = Some("Dashboard saved.".into());
                                    let _ = view.root.update(cx, |root, _| {
                                        root.dashboard_source.request_reload();
                                        if root.restart_status.as_deref() == Some(WAIT) {
                                            root.restart_status = None;
                                        }
                                    });
                                }
                                Ok(Err(SaveError::CommittedUnverified)) => {
                                    view.unverified = true;
                                    view.status = Some(
                                        "Dashboard may have been saved; reload before editing."
                                            .into(),
                                    );
                                }
                                Ok(Err(error)) => view.status = Some(error.to_string()),
                                Err(error) => {
                                    view.unverified = true;
                                    view.status = Some(format!(
                                        "Dashboard save ended without confirmation: {error}"
                                    ));
                                }
                            }
                            cx.notify();
                        });
                    }
                })
                .detach();
            });
        });
        cx.notify();
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.current(window, cx) || window.has_active_dialog(cx) {
            return;
        }
        if !self.dirty && !self.unverified {
            self.finish_close(window, cx);
            return;
        }
        let weak = cx.entity().downgrade();
        let mount = self.mount.clone();
        let revision = self.revision;
        let Some((native, visibility, visible_revision)) = self.presentation_stamp(cx) else {
            return;
        };
        window.open_alert_dialog(cx, move |alert, _, _| {
            let weak = weak.clone();
            let mount = mount.clone();
            let visibility = visibility.clone();
            alert
                .title(Label::new("Discard Dashboard draft?"))
                .description(Label::new("Unsaved Dashboard changes will be discarded."))
                .confirm()
                .close_button(true)
                .keyboard(true)
                .on_ok(move |_, window, app| {
                    weak.update(app, |view, cx| {
                        if !mount.is_current()
                            || !mount.matches(&view.mount)
                            || view.revision != revision
                            || view.busy
                            || current_foreground_identity() != Some(native.identity)
                            || !json_utility::native_host_current(
                                &visibility,
                                visible_revision,
                                Some(native),
                                false,
                            )
                        {
                            return false;
                        }
                        window.close_dialog(cx);
                        view.finish_close(window, cx);
                        false
                    })
                    .unwrap_or(false)
                })
        });
    }
    fn finish_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self.focus.contains_focused(window, cx);
        self.open = false;
        self.owner.retire();
        self.draft = None;
        self.dirty = false;
        self.unverified = false;
        let _ = self.root.update(cx, |root, _| {
            if root.restart_status.as_deref() == Some(WAIT) {
                root.restart_status = None;
            }
        });
        if focused {
            let _ = self
                .root
                .update(cx, |root, cx| root.query.focus_handle(cx).focus(window, cx));
        }
        cx.notify();
    }

    fn edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut DashboardDraft) -> anyhow::Result<()>,
    ) {
        if self.busy || self.unverified || !self.current(window, cx) || window.has_active_dialog(cx)
        {
            return;
        }
        let Some(draft) = self.draft.clone() else {
            return;
        };
        let result = draft
            .lock()
            .map_err(|_| "Dashboard draft unavailable".to_owned())
            .and_then(|mut draft| update(&mut draft).map_err(|error| error.to_string()));
        match result {
            Ok(()) => {
                self.dirty = true;
                self.revision = self.revision.wrapping_add(1);
                self.status = None;
            }
            Err(error) => self.status = Some(error),
        }
        cx.notify();
    }

    fn remove_review(&mut self, token: SlotToken, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.unverified || !self.current(window, cx) || window.has_active_dialog(cx)
        {
            return;
        }
        let weak = cx.entity().downgrade();
        let mount = self.mount.clone();
        let revision = self.revision;
        let description = self
            .painted
            .as_ref()
            .and_then(|(_, rows)| rows.iter().find(|row| row.token == token))
            .map(|row| {
                format!(
                    "Remove {} at row {}, column {}?",
                    row.widget, row.row, row.col
                )
            })
            .unwrap_or_else(|| "Remove the selected widget?".into());
        let Some((native, visibility, visible_revision)) = self.presentation_stamp(cx) else {
            return;
        };
        window.open_alert_dialog(cx, move |alert, _, _| {
            let weak = weak.clone();
            let mount = mount.clone();
            let visibility = visibility.clone();
            let description = description.clone();
            alert
                .title(Label::new("Remove Dashboard widget?"))
                .description(Label::new(description))
                .confirm()
                .close_button(true)
                .keyboard(true)
                .on_ok(move |_, _window, app| {
                    weak.update(app, |view, cx| {
                        if !mount.is_current()
                            || !mount.matches(&view.mount)
                            || view.revision != revision
                            || view.busy
                            || view.selected != Some(token)
                            || current_foreground_identity() != Some(native.identity)
                            || !json_utility::native_host_current(
                                &visibility,
                                visible_revision,
                                Some(native),
                                false,
                            )
                        {
                            return false;
                        }
                        let Some(draft) = view.draft.clone() else {
                            return false;
                        };
                        let outcome = draft
                            .lock()
                            .map_err(|_| "Dashboard draft unavailable".to_owned())
                            .and_then(|mut draft| {
                                draft.remove(token).map_err(|error| error.to_string())
                            });
                        match outcome {
                            Ok(()) => {
                                view.selected = None;
                                view.swap = None;
                                view.dirty = true;
                                view.revision = view.revision.wrapping_add(1);
                                view.status = None;
                                cx.notify();
                                true
                            }
                            Err(error) => {
                                view.status = Some(error);
                                cx.notify();
                                false
                            }
                        }
                    })
                    .unwrap_or(false)
                })
        });
    }
}

impl Render for Editor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div()
            .id("dashboard-editor")
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .child(Label::new("Customize Dashboard"));
        if let Some(snapshot) = self.draft.as_ref().and_then(|draft| {
            draft.try_lock().ok().map(|draft| {
                (
                    draft.grid().clone(),
                    draft
                        .slots()
                        .iter()
                        .map(|item| PaintedSlot {
                            token: item.token,
                            widget: item.slot.widget.clone(),
                            row: item.slot.row,
                            col: item.slot.col,
                            row_span: item.slot.row_span,
                            col_span: item.slot.col_span,
                        })
                        .collect(),
                )
            })
        }) {
            self.painted = Some(snapshot);
        }
        let snapshot = self.painted.clone();
        let disabled = self.busy || self.unverified;
        if let Some((grid, slots)) = snapshot {
            panel = panel.child(Label::new(format!(
                "Grid: {} rows × {} columns",
                grid.rows, grid.cols
            )));
            let mut controls = div().flex().flex_wrap().gap_2();
            for (id, label, rows, delta) in [
                ("rows-less", "Rows −", true, -1i16),
                ("rows-more", "Rows +", true, 1),
                ("cols-less", "Columns −", false, -1),
                ("cols-more", "Columns +", false, 1),
            ] {
                let receipt = self.mount.clone();
                let epoch = self.revision;
                controls =
                    controls.child(Button::new(id).label(label).disabled(disabled).on_click(
                        cx.listener(move |view, _, window, cx| {
                            if !receipt.is_current()
                                || !receipt.matches(&view.mount)
                                || view.revision != epoch
                            {
                                return;
                            }
                            view.edit(window, cx, |draft| {
                                let mut grid = draft.grid().clone();
                                let old = if rows { grid.rows } else { grid.cols };
                                let value = (i16::from(old) + delta).clamp(1, 255) as u8;
                                if rows {
                                    grid.rows = value;
                                } else {
                                    grid.cols = value;
                                }
                                draft.set_grid(grid);
                                Ok(())
                            });
                        }),
                    ));
            }
            panel = panel.child(controls);
            let mut list = div()
                .id("dashboard-editor-slots")
                .flex()
                .flex_col()
                .gap_2()
                .max_h(px(220.0))
                .overflow_y_scroll();
            for slot in &slots {
                let token = slot.token;
                let receipt = self.mount.clone();
                let epoch = self.revision;
                list = list.child(
                    Button::new(SharedString::from(format!(
                        "dashboard-editor-slot-{token:?}"
                    )))
                    .label(format!(
                        "{} · ({}, {}) · {}×{}",
                        slot.widget, slot.row, slot.col, slot.row_span, slot.col_span
                    ))
                    .disabled(disabled)
                    .selected(self.selected == Some(token))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if receipt.is_current()
                            && receipt.matches(&view.mount)
                            && view.revision == epoch
                            && !view.busy
                            && view.current(window, cx)
                            && !window.has_active_dialog(cx)
                        {
                            view.selected = Some(token);
                            cx.notify();
                        }
                    })),
                );
            }
            panel = panel.child(list);
            if let Some(item) = slots.iter().find(|slot| Some(slot.token) == self.selected) {
                let token = item.token;
                let mut placement = div().flex().flex_wrap().gap_2();
                for (id, label, field, delta) in [
                    ("up", "Up", 0, -1i32),
                    ("down", "Down", 0, 1),
                    ("left", "Left", 1, -1),
                    ("right", "Right", 1, 1),
                    ("shorter", "Height −", 2, -1),
                    ("taller", "Height +", 2, 1),
                    ("narrower", "Width −", 3, -1),
                    ("wider", "Width +", 3, 1),
                ] {
                    let receipt = self.mount.clone();
                    let epoch = self.revision;
                    placement =
                        placement.child(Button::new(id).label(label).disabled(disabled).on_click(
                            cx.listener(move |view, _, window, cx| {
                                if !receipt.is_current()
                                    || !receipt.matches(&view.mount)
                                    || view.revision != epoch
                                    || view.selected != Some(token)
                                {
                                    return;
                                }
                                view.edit(window, cx, |draft| {
                                    let slot = draft
                                        .slots()
                                        .iter()
                                        .find(|row| row.token == token)
                                        .ok_or_else(|| anyhow::anyhow!("Dashboard slot changed"))?
                                        .slot
                                        .clone();
                                    let mut values = [
                                        slot.row,
                                        slot.col,
                                        i32::from(slot.row_span.max(1)),
                                        i32::from(slot.col_span.max(1)),
                                    ];
                                    values[field] = adjusted_coordinate(
                                        values[field],
                                        delta,
                                        if field < 2 { 0 } else { 1 },
                                    );
                                    draft.set_placement(
                                        token,
                                        values[0],
                                        values[1],
                                        values[2].min(255) as u8,
                                        values[3].min(255) as u8,
                                    )
                                });
                            }),
                        ));
                }
                panel = panel.child(placement);
                let receipt = self.mount.clone();
                let epoch = self.revision;
                panel = panel.child(
                    Button::new("dashboard-editor-remove")
                        .label("Remove…")
                        .disabled(disabled)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if receipt.is_current()
                                && receipt.matches(&view.mount)
                                && view.revision == epoch
                                && view.selected == Some(token)
                            {
                                view.remove_review(token, window, cx);
                            }
                        })),
                );
                let receipt = self.mount.clone();
                let epoch = self.revision;
                panel = panel.child(
                    Button::new("dashboard-editor-swap")
                        .label(if self.swap.is_some() {
                            "Swap with selected"
                        } else {
                            "Choose for swap"
                        })
                        .disabled(disabled)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if !receipt.is_current()
                                || !receipt.matches(&view.mount)
                                || view.revision != epoch
                                || view.selected != Some(token)
                                || !view.current(window, cx)
                            {
                                return;
                            }
                            if let Some(first) = view.swap.take() {
                                if first != token {
                                    view.edit(window, cx, |draft| draft.swap(first, token));
                                }
                            } else {
                                view.swap = Some(token);
                                cx.notify();
                            }
                        })),
                );
            }
            let mut add = div()
                .id("dashboard-editor-add")
                .flex()
                .flex_col()
                .gap_2()
                .max_h(px(150.0))
                .overflow_y_scroll()
                .child(Label::new("Add widget"));
            for name in WidgetRegistry::with_defaults().names() {
                let label = name.clone();
                let receipt = self.mount.clone();
                let epoch = self.revision;
                add = add.child(
                    Button::new(SharedString::from(format!("dashboard-add-{name}")))
                        .label(label)
                        .disabled(disabled)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if !receipt.is_current()
                                || !receipt.matches(&view.mount)
                                || view.revision != epoch
                            {
                                return;
                            }
                            view.edit(window, cx, |draft| {
                                let grid = draft.grid();
                                let pos = (0..i32::from(grid.rows))
                                    .flat_map(|row| {
                                        (0..i32::from(grid.cols)).map(move |col| (row, col))
                                    })
                                    .find(|(row, col)| {
                                        !draft.slots().iter().any(|item| {
                                            i64::from(*row) >= i64::from(item.slot.row)
                                                && i64::from(*col) >= i64::from(item.slot.col)
                                                && i64::from(*row)
                                                    < i64::from(item.slot.row)
                                                        + i64::from(item.slot.row_span.max(1))
                                                && i64::from(*col)
                                                    < i64::from(item.slot.col)
                                                        + i64::from(item.slot.col_span.max(1))
                                        })
                                    })
                                    .ok_or_else(|| anyhow::anyhow!("No free Dashboard cell"))?;
                                draft.add(&name, pos.0, pos.1, &WidgetRegistry::with_defaults())?;
                                Ok(())
                            });
                        })),
                );
            }
            panel = panel.child(add);
        }
        if let Some(status) = &self.status {
            panel = panel.child(Label::new(status.clone()).id("dashboard-editor-status"));
        }
        panel.child(
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("dashboard-editor-save")
                        .label("Save")
                        .disabled(disabled || !self.dirty)
                        .on_click(cx.listener(|view, _, window, cx| view.save(window, cx))),
                )
                .child(
                    Button::new("dashboard-editor-reload")
                        .label("Reload")
                        .disabled(self.busy)
                        .on_click(
                            cx.listener(|view, _, window, cx| view.request_reload(window, cx)),
                        ),
                )
                .child(
                    Button::new("dashboard-editor-close")
                        .label("Cancel / Close")
                        .disabled(self.busy)
                        .on_click(cx.listener(|view, _, window, cx| view.close(window, cx))),
                ),
        )
    }
}

impl RootWindow {
    pub(super) fn open_dashboard_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced()
            || window.has_active_dialog(cx)
            || !window.is_window_active()
        {
            return;
        }
        if let Some(view) = self
            .dashboard_editor
            .clone()
            .filter(|view| view.read(cx).open)
        {
            view.update(cx, |view, cx| view.focus.focus(window, cx));
            self.root_scroll.scroll_to_bottom();
            return;
        }
        let root = cx.entity().downgrade();
        let view = cx.new(|cx| Editor::new(root, window, cx));
        self.dashboard_editor = Some(view.clone());
        view.update(cx, |view, cx| view.focus.focus(window, cx));
        let weak = view.downgrade();
        window.defer(cx, move |window, cx| {
            let _ = weak.update(cx, |view, cx| view.load(window, cx));
        });
        self.root_scroll.scroll_to_bottom();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{adjusted_coordinate, current_provider};
    use multi_launcher::plugin::Plugin;
    use multi_launcher::plugins::settings::SettingsPlugin;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn placement_adjustment_saturates_at_extreme_values() {
        assert_eq!(adjusted_coordinate(i32::MAX, 1, 0), i32::MAX);
        assert_eq!(adjusted_coordinate(i32::MIN, -1, 0), 0);
        assert_eq!(adjusted_coordinate(1, -1, 1), 1);
    }

    #[test]
    fn dashboard_editor_route_requires_exact_stock_provider_and_capability() {
        let row = SettingsPlugin.search("dashboard").pop().unwrap();
        let enabled = HashSet::from(["settings".to_owned()]);
        assert!(current_provider("dashboard", &row, Some(&enabled), None));
        assert!(!current_provider("settings", &row, Some(&enabled), None));
        assert!(!current_provider(
            "dashboard",
            &row,
            Some(&HashSet::new()),
            None
        ));
        assert!(!current_provider(
            "dashboard",
            &row,
            Some(&enabled),
            Some(&HashMap::from([("settings".into(), vec![])]))
        ));
        let mut forged = row.clone();
        forged.label.push('!');
        assert!(!current_provider(
            "dashboard",
            &forged,
            Some(&enabled),
            None
        ));
    }
}
