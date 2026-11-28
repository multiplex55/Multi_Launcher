//! Retained Calculation Pad result-format settings.
use super::*;
use multi_launcher::settings::{
    CalculationPadDisplayPreferences, CalculationPadFormatError, CalculationPadFormatSnapshot,
    CalculationPadResultFormat,
};

pub(super) struct FormatOwner {
    pub baseline: Option<CalculationPadFormatSnapshot>,
    pub draft: Option<CalculationPadResultFormat>,
    pub busy: bool,
    pub uncertain: bool,
    pub conflicted: bool,
    pub revision: u64,
    pub status: Option<String>,
    pub lease: Option<Arc<AtomicBool>>,
    pub blur: Option<Subscription>,
    pub activation: Option<Subscription>,
    pub focus: [FocusHandle; 10],
}
impl FormatOwner {
    pub fn new(cx: &mut Context<CalculationPadView>) -> Self {
        Self {
            baseline: None,
            draft: None,
            busy: false,
            uncertain: false,
            conflicted: false,
            revision: 1,
            status: None,
            lease: None,
            blur: None,
            activation: None,
            focus: std::array::from_fn(|_| cx.focus_handle()),
        }
    }
}
impl FormatOwner {
    pub fn dirty(&self) -> bool {
        self.baseline
            .as_ref()
            .zip(self.draft)
            .is_some_and(|(snapshot, draft)| {
                CalculationPadResultFormat::from(snapshot.display) != draft
            })
    }
    pub fn display(&self) -> CalculationPadDisplayPreferences {
        self.baseline
            .as_ref()
            .map_or_else(CalculationPadDisplayPreferences::default, |snapshot| {
                snapshot.display
            })
    }
    pub fn revoke(&self) {
        if let Some(lease) = &self.lease {
            lease.store(false, Ordering::Release);
        }
    }
    fn settle(&mut self) {
        self.busy = false;
        self.lease = None;
        self.blur = None;
        self.activation = None;
    }
}
impl CalculationPadView {
    pub(super) fn load_format(&mut self, cx: &mut Context<Self>) {
        if self.format.busy
            || self.format.dirty()
            || self.format.uncertain
            || self.pending_copy
            || !self.open
            || self
                .session
                .as_ref()
                .is_none_or(|session| session.pending_transition().is_some())
        {
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
                .begin_calculation_pad_format_read(root.settings_path.clone())
            {
                Ok(rx) => (rx, permit),
                Err(error) => {
                    self.format.status = Some(error);
                    cx.notify();
                    return;
                }
            }
        };
        self.format.busy = true;
        self.format.status = Some("Reading result format…".into());
        let mount = self.mount.clone();
        let task = cx
            .background_executor()
            .spawn(async move { receiver.recv() });
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = task.await;
            let _ = weak.update(cx, |view, cx| {
                if !mount.is_current() || !mount.matches(&view.mount) || !view.format.busy {
                    return;
                }
                view.format.settle();
                match result {
                    Ok(Ok(snapshot)) => {
                        view.format.draft = Some(snapshot.display.into());
                        view.format.baseline = Some(snapshot);
                        view.format.conflicted = false;
                        view.format.revision = view.format.revision.wrapping_add(1).max(1);
                        view.format.status = None;
                    }
                    Ok(Err(error)) => view.format.status = Some(error.to_string()),
                    Err(error) => view.format.status = Some(error.to_string()),
                }
                view.settle_format_wait(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn settle_format_wait(&self, cx: &mut Context<Self>) {
        if self.blocks_close() {
            return;
        }
        let root = self.root.clone();
        cx.spawn(async move |_, cx| {
            let _ = root.update(cx, |root, cx| {
                if root.restart_status.as_deref() == Some(WAIT) {
                    root.restart_status = None;
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn save_format(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.format.busy
            || self.format.uncertain
            || self.format.conflicted
            || self.pending_copy
            || self
                .session
                .as_ref()
                .is_none_or(|s| s.pending_transition().is_some())
            || !self.current(window, cx)
        {
            return;
        }
        let Some(baseline) = self.format.baseline.clone() else {
            return;
        };
        let Some(draft) = self.format.draft else {
            return;
        };
        let Some(native) = self.native else {
            return;
        };
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let (permit, workers, path) = {
            let root = root.read(cx);
            let Some(permit) = root.restart_admission.admit() else {
                return;
            };
            (
                permit,
                root.settings_save_workers.clone(),
                root.settings_path.clone(),
            )
        };
        self.focus.focus(window, cx);
        let lease = Arc::new(AtomicBool::new(true));
        let flag = lease.clone();
        self.format.blur = Some(cx.on_focus_out(&self.focus, window, move |_, _, _, _| {
            flag.store(false, Ordering::Release)
        }));
        let flag = lease.clone();
        self.format.activation = Some(cx.observe_window_activation(window, move |_, window, _| {
            if !window.is_window_active() {
                flag.store(false, Ordering::Release);
            }
        }));
        self.format.lease = Some(lease.clone());
        self.format.busy = true;
        let mount = self.mount.clone();
        let revision = self.format.revision;
        let visibility = self.visibility.clone();
        let visible = visibility.request_revision.load(Ordering::Acquire);
        let admission = self.admission.clone();
        let focus = self.focus.clone();
        let weak = cx.entity().downgrade();
        window.defer(cx, move |window, app| {
            let _ = weak.update(app, |view, cx| {
                if !mount.is_current() || !mount.matches(&view.mount) {
                    return;
                }
                if !view.format.busy
                    || view.format.revision != revision
                    || view.format.baseline.as_ref() != Some(&baseline)
                    || view.format.draft != Some(draft)
                    || !lease.load(Ordering::Acquire)
                    || !focus.is_focused(window)
                    || !view.current(window, cx)
                    || view.visibility.request_revision.load(Ordering::Acquire) != visible
                {
                    view.format.settle();
                    view.format.status = Some("Format save was superseded; try again.".into());
                    view.settle_format_wait(cx);
                    cx.notify();
                    return;
                }
                let worker_mount = mount.clone();
                let worker_lease = lease.clone();
                let worker_visibility = visibility.clone();
                let current: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
                    worker_mount.is_current()
                        && worker_lease.load(Ordering::Acquire)
                        && !admission.is_fenced()
                        && current_foreground_identity() == Some(native.identity)
                        && json_utility::native_host_current(
                            &worker_visibility,
                            visible,
                            Some(native),
                            false,
                        )
                });
                let (receiver, token) = match workers.begin_calculation_pad_format_save(
                    path,
                    baseline.clone(),
                    draft,
                    current,
                ) {
                    Ok(v) => v,
                    Err(error) => {
                        view.format.settle();
                        view.format.status = Some(error);
                        view.settle_format_wait(cx);
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
                    let _ = weak.update(cx, |view, cx| {
                        if !mount.is_current() || !mount.matches(&view.mount) || !view.format.busy {
                            return;
                        }
                        if view.format.revision != revision {
                            view.format.settle(); view.format.uncertain = true;
                            view.format.revision = view.format.revision.wrapping_add(1).max(1);
                            view.format.status = Some("Format acknowledgement is uncertain. Close Multi Launcher and manually relaunch; reopening this Pad does not reconcile it.".into());
                            view.settle_format_wait(cx); cx.notify(); return;
                        }
                        view.format.settle();
                        match result {
                            Ok(Ok(ack))
                                if CalculationPadResultFormat::from(ack.snapshot.display)
                                    == draft =>
                            {
                                view.format.baseline = Some(ack.snapshot);
                                view.format.draft = Some(draft);
                                view.format.revision = view.format.revision.wrapping_add(1).max(1);
                                view.format.status = Some("Result format applied.".into());
                            }
                            Ok(Err(CalculationPadFormatError::CommittedUnverified(error))) => {
                                view.format.uncertain = true;
                                view.format.revision = view.format.revision.wrapping_add(1).max(1);
                                view.format.status = Some(format!(
                                    "Format may have been saved: {error}. Close Multi Launcher and manually relaunch; reopening this Pad does not reconcile it."
                                ));
                            }
                            Ok(Err(CalculationPadFormatError::Conflict)) => { view.format.conflicted = true; view.format.status = Some("Format settings changed. Discard the draft, then reload before applying.".into()); }
                            Ok(Err(error)) => view.format.status = Some(error.to_string()),
                            _ => {
                                view.format.uncertain = true;
                                view.format.revision = view.format.revision.wrapping_add(1).max(1);
                                view.format.status = Some(
                                    "Format acknowledgement is uncertain. Close Multi Launcher and manually relaunch; reopening this Pad does not reconcile it."
                                        .into(),
                                );
                            }
                        }
                        view.settle_format_wait(cx);
                        cx.notify();
                    });
                })
                .detach();
            });
        });
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{format_action_current, format_receipt_current};
    use multi_launcher::calculation_pad::CalculationPadSession;

    #[test]
    fn formatting_receipt_rejects_changed_basis_and_format_revision() {
        let dir = tempfile::tempdir().unwrap();
        let root = multi_launcher::platform::app_data::AppDataRoot::from_settings_path(
            dir.path().join("settings.json"),
        )
        .unwrap();
        let mut session = CalculationPadSession::new(&root);
        let basis = session.evaluation_basis();
        assert!(format_receipt_current(3, 3, basis, basis));
        assert!(!format_receipt_current(3, 4, basis, basis));
        session.replace_source("x = 1").unwrap();
        assert!(!format_receipt_current(
            3,
            3,
            basis,
            session.evaluation_basis()
        ));
    }

    #[test]
    fn stale_discard_and_reload_are_rejected_during_apply_or_transition() {
        assert!(format_action_current(4, 4, false, false, false, true));
        assert!(!format_action_current(4, 4, true, false, false, true));
        assert!(!format_action_current(4, 4, false, false, false, false));
        assert!(!format_action_current(4, 5, false, false, false, true));
        assert!(!format_action_current(4, 4, false, true, false, true));
    }
}

#[derive(Clone, Copy)]
enum Change {
    Number(multi_launcher::settings::CalculationPadNumberMode),
    Exact(multi_launcher::settings::CalculationPadExactMode),
    Precision(i8),
    Group,
    Percent,
}
fn format_receipt_current(
    issued: u64,
    current: u64,
    issued_basis: Option<EvaluationBasis>,
    current_basis: Option<EvaluationBasis>,
) -> bool {
    issued == current && issued_basis == current_basis
}
fn format_action_current(
    issued: u64,
    current: u64,
    busy: bool,
    uncertain: bool,
    pending_copy: bool,
    session_ready: bool,
) -> bool {
    issued == current && !busy && !uncertain && !pending_copy && session_ready
}
impl CalculationPadView {
    fn change_format(
        &mut self,
        change: Change,
        revision: u64,
        basis: Option<EvaluationBasis>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.format.conflicted
            || !format_action_current(
                revision,
                self.format.revision,
                self.format.busy,
                self.format.uncertain,
                self.pending_copy,
                self.session
                    .as_ref()
                    .is_some_and(|session| session.pending_transition().is_none()),
            )
            || !self.current(window, cx)
            || !format_receipt_current(
                revision,
                self.format.revision,
                basis,
                self.session.as_ref().and_then(|s| s.evaluation_basis()),
            )
        {
            return;
        }
        let evaluation = self.session.as_mut().and_then(|s| s.shared_evaluation());
        let values = || {
            evaluation
                .iter()
                .flat_map(|evaluation| evaluation.lines.iter())
                .filter_map(|row| {
                    if let LineOutcome::Value(value) = &row.outcome {
                        Some(value)
                    } else {
                        None
                    }
                })
        };
        let precision = values().any(calculation_pad::result_format::supports_precision);
        let percent = values().any(calculation_pad::result_format::supports_percent);
        let Some(mut draft) = self.format.draft else {
            return;
        };
        use multi_launcher::settings::CalculationPadNumberMode as Number;
        match change {
            Change::Number(value) if value == Number::Auto || precision => {
                draft.number_mode = value
            }
            Change::Exact(value) => draft.exact_mode = value,
            Change::Precision(delta) if precision && draft.number_mode != Number::Auto => {
                draft.decimal_places =
                    (i16::from(draft.decimal_places) + i16::from(delta)).clamp(0, 15) as u8
            }
            Change::Group => draft.group_digits = !draft.group_digits,
            Change::Percent if percent => draft.percent = !draft.percent,
            _ => return,
        }
        if self.format.draft != Some(draft) {
            self.format.draft = Some(draft);
            self.format.revision = self.format.revision.wrapping_add(1).max(1);
            self.format.status = None;
            cx.notify();
        }
    }
    pub(super) fn format_panel(
        &self,
        basis: Option<EvaluationBasis>,
        precision: bool,
        percent: bool,
        document_pending: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use multi_launcher::settings::{
            CalculationPadExactMode as Exact, CalculationPadNumberMode as Number,
        };
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(Label::new("Number format"));
        if let Some(draft) = self.format.draft {
            let revision = self.format.revision;
            let disabled =
                self.format.busy || self.format.uncertain || document_pending || self.pending_copy;
            let mut buttons = div().flex().flex_wrap().gap_1();
            for (index, (label, change, selected, enabled)) in [
                (
                    "Auto",
                    Change::Number(Number::Auto),
                    draft.number_mode == Number::Auto,
                    true,
                ),
                (
                    "Fixed decimal",
                    Change::Number(Number::Fixed),
                    draft.number_mode == Number::Fixed,
                    precision,
                ),
                (
                    "Scientific",
                    Change::Number(Number::Scientific),
                    draft.number_mode == Number::Scientific,
                    precision,
                ),
                (
                    "Exact",
                    Change::Exact(Exact::Exact),
                    draft.exact_mode == Exact::Exact,
                    true,
                ),
                (
                    "Decimal",
                    Change::Exact(Exact::Decimal),
                    draft.exact_mode == Exact::Decimal,
                    true,
                ),
                (
                    "Mixed number",
                    Change::Exact(Exact::MixedNumber),
                    draft.exact_mode == Exact::MixedNumber,
                    true,
                ),
                (
                    "−",
                    Change::Precision(-1),
                    false,
                    precision && draft.number_mode != Number::Auto && draft.decimal_places > 0,
                ),
                (
                    "+",
                    Change::Precision(1),
                    false,
                    precision && draft.number_mode != Number::Auto && draft.decimal_places < 15,
                ),
                ("Group digits", Change::Group, draft.group_digits, true),
                (
                    "Percent for scalars",
                    Change::Percent,
                    draft.percent,
                    percent,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let text = if selected {
                    format!("✓ {label}")
                } else {
                    label.to_owned()
                };
                buttons = buttons.child(
                    Button::new(SharedString::from(format!(
                        "pad-format-{label}-{revision}-{:?}",
                        basis
                    )))
                    .track_focus(&self.format.focus[index])
                    .tab_stop(true)
                    .label(text)
                    .disabled(disabled || self.format.conflicted || !enabled)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.change_format(change, revision, basis, window, cx)
                    })),
                );
            }
            panel = panel.child(buttons).child(Label::new(format!(
                "Decimal places: {}",
                draft.decimal_places
            )));
            let mount = self.mount.clone();
            let cancel_mount = self.mount.clone();
            panel = panel.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new(SharedString::from(format!(
                            "pad-format-apply-{revision}-{:?}",
                            basis
                        )))
                        .label("Apply format")
                        .disabled(disabled || self.format.conflicted || !self.format.dirty())
                        .on_click(cx.listener(
                            move |view, _, window, cx| {
                                if mount.is_current()
                                    && mount.matches(&view.mount)
                                    && !view.format.conflicted
                                    && format_action_current(
                                        revision,
                                        view.format.revision,
                                        view.format.busy,
                                        view.format.uncertain,
                                        view.pending_copy,
                                        view.session.as_ref().is_some_and(|session| {
                                            session.pending_transition().is_none()
                                        }),
                                    )
                                    && view
                                        .session
                                        .as_ref()
                                        .and_then(|session| session.evaluation_basis())
                                        == basis
                                {
                                    view.save_format(window, cx);
                                }
                            },
                        )),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "pad-format-discard-{revision}-{:?}",
                            basis
                        )))
                        .label("Discard format changes")
                        .disabled(disabled || !self.format.dirty())
                        .on_click(cx.listener(
                            move |view, _, window, cx| {
                                if cancel_mount.is_current()
                                    && cancel_mount.matches(&view.mount)
                                    && format_action_current(
                                        revision,
                                        view.format.revision,
                                        view.format.busy,
                                        view.format.uncertain,
                                        view.pending_copy,
                                        view.session.as_ref().is_some_and(|session| {
                                            session.pending_transition().is_none()
                                        }),
                                    )
                                    && view
                                        .session
                                        .as_ref()
                                        .and_then(|session| session.evaluation_basis())
                                        == basis
                                    && view.current(window, cx)
                                {
                                    if let Some(snapshot) = &view.format.baseline {
                                        view.format.draft = Some(snapshot.display.into());
                                        view.format.revision =
                                            view.format.revision.wrapping_add(1).max(1);
                                        view.settle_format_wait(cx);
                                        cx.notify();
                                    }
                                }
                            },
                        )),
                    ),
            );
        }
        if let Some(status) = &self.format.status {
            panel = panel.child(Label::new(status.clone()));
        }
        if !self.format.uncertain {
            let revision = self.format.revision;
            let mount = self.mount.clone();
            panel = panel.child(
                Button::new(SharedString::from(format!("pad-format-reload-{revision}")))
                    .label("Reload format")
                    .disabled(
                        self.format.busy
                            || self.format.dirty()
                            || document_pending
                            || self.pending_copy,
                    )
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if mount.is_current()
                            && mount.matches(&view.mount)
                            && format_action_current(
                                revision,
                                view.format.revision,
                                view.format.busy,
                                view.format.uncertain,
                                view.pending_copy,
                                view.session
                                    .as_ref()
                                    .is_some_and(|session| session.pending_transition().is_none()),
                            )
                            && !view.format.dirty()
                            && view.current(window, cx)
                        {
                            view.load_format(cx);
                        }
                    })),
            );
        }
        panel.into_any_element()
    }
}
