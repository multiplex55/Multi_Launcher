//! Retained launcher-hotkey draft backed by the checked Settings worker.
use super::*;
use multi_launcher::settings::{LauncherHotkeySnapshot, Settings};
use multi_launcher::{LauncherHotkeySaveEvent, LauncherHotkeySaveTicket};

pub(super) const WAIT: &str = "Wait for the launcher hotkey save to finish.";
const RECOVERY: &str = "Cancel Settings to discard the draft, then close Multi Launcher and manually relaunch the desired frontend.";

fn drain_events(
    mut next: impl FnMut() -> Result<LauncherHotkeySaveEvent, std::sync::mpsc::TryRecvError>,
) -> (Vec<LauncherHotkeySaveEvent>, bool) {
    let mut events = Vec::new();
    loop {
        match next() {
            Ok(event) => {
                let terminal = matches!(event, LauncherHotkeySaveEvent::Completed { .. });
                events.push(event);
                if terminal {
                    return (events, false);
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => return (events, false),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return (events, true),
        }
    }
}

fn reconciliation_visible(
    visibility: &RootVisibility,
    revision: u64,
    native: NativeWindow,
) -> bool {
    #[cfg(windows)]
    {
        visibility.is_current_visible_revision(revision, native)
            && current_foreground_identity() == Some(native.identity)
    }
    #[cfg(not(windows))]
    {
        let _ = (visibility, revision, native);
        false
    }
}

#[derive(Clone)]
pub(super) struct RecoveryProof {
    baseline: LauncherHotkeySnapshot,
    generation: u64,
    request_id: u64,
}

pub(super) struct SettingsHotkey {
    root: WeakEntity<RootWindow>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    host: AnyWindowHandle,
    input: Entity<InputState>,
    _subscription: Subscription,
    baseline: Option<LauncherHotkeySnapshot>,
    ticket: Option<LauncherHotkeySaveTicket>,
    wake: Arc<HashWakeSignal>,
    reading: bool,
    reconciliation: bool,
    reconciliation_owner: Arc<AtomicBool>,
    proof: Arc<Mutex<Option<RecoveryProof>>>,
    status: Option<String>,
}

impl SettingsHotkey {
    fn new(
        root: WeakEntity<RootWindow>,
        reconciliation_owner: Arc<AtomicBool>,
        proof: Arc<Mutex<Option<RecoveryProof>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("F2"));
        let subscription = cx.subscribe(&input, |_: &mut Self, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let reconciliation = reconciliation_owner.load(Ordering::Acquire);
        Self {
            root,
            owner,
            mount,
            host: window.window_handle(),
            input,
            _subscription: subscription,
            baseline: None,
            ticket: None,
            wake: Arc::new(HashWakeSignal::default()),
            reading: false,
            reconciliation,
            reconciliation_owner,
            proof,
            status: reconciliation.then(|| format!("The previous hotkey save requires reconciliation. Check the saved and active routes, or {RECOVERY}")),
        }
    }

    pub fn busy(&self) -> bool {
        self.reading || self.ticket.is_some()
    }
    pub fn dirty(&self, cx: &App) -> bool {
        self.baseline.as_ref().is_some_and(|baseline| {
            self.input.read(cx).value().as_str() != baseline.value.as_deref().unwrap_or("")
        })
    }
    pub fn restart_draft(&self, cx: &App) -> Option<(Option<String>, String)> {
        self.baseline.as_ref().map(|baseline| {
            (
                baseline.value.clone(),
                self.input.read(cx).value().to_string(),
            )
        })
    }
    pub fn can_focus(&self) -> bool {
        self.mount.is_current() && !self.busy() && self.baseline.is_some()
    }
    pub fn focus_control(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.can_focus() {
            return false;
        }
        self.input.focus_handle(cx).focus(window, cx);
        true
    }
    pub fn retire(&mut self) {
        self.owner.retire();
        self.wake.close();
    }
    pub fn cancel_pending(&self) {
        if let Some(ticket) = &self.ticket {
            ticket.request_cancel();
        }
    }

    fn current(&self, window: &Window, cx: &App) -> bool {
        self.mount.is_current()
            && self.host.window_id() == window.window_handle().window_id()
            && self.root.upgrade().is_some_and(|root| {
                let root = root.read(cx);
                root.settings_editor.open
                    && !root.restart_admission.is_fenced()
                    && root.action_sheet.is_none()
                    && root.settings_hotkey.is_some()
                    && root.root_visibility.active.load(Ordering::Acquire)
                    && root.root_visibility.visible.load(Ordering::Acquire)
                    && root.native_window.is_some_and(|native| {
                        current_foreground_identity() == Some(native.identity)
                    })
            })
    }

    fn read(&mut self, cx: &mut Context<Self>) {
        if self.busy() {
            return;
        }
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let root = root.read(cx);
        let Some(permit) = root.restart_admission.admit() else {
            return;
        };
        let path = root.settings_path.clone();
        let host = self.host;
        let receipt = self.mount.clone();
        self.reading = true;
        self.input
            .update(cx, |input, cx| input.set_readonly(true, cx));
        let task = cx.background_executor().spawn(async move {
            Settings::launcher_hotkey_snapshot(path).map_err(|error| error.to_string())
        });
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = task.await;
            if let Some(view) = weak.upgrade() {
                let _ = update_kit_window_content(host, &view, cx, |view, window, cx| {
                    if !receipt.is_current() || !receipt.matches(&view.mount) {
                        return;
                    }
                    view.reading = false;
                    view.input
                        .update(cx, |input, cx| input.set_readonly(view.reconciliation, cx));
                    match result {
                        Ok(baseline) => {
                            let text = baseline.value.clone().unwrap_or_default();
                            view.baseline = Some(baseline);
                            view.input
                                .update(cx, |input, cx| input.set_value(text, window, cx));
                        }
                        Err(error) => {
                            view.status = Some(format!("Could not read launcher hotkey: {error}"))
                        }
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy()
            || self.reconciliation
            || !self.current(window, cx)
            || window.has_active_dialog(cx)
        {
            return;
        }
        let Some(baseline) = self.baseline.clone() else {
            return;
        };
        let text = self.input.read(cx).value().trim().to_owned();
        let Some(parsed) = parse_hotkey(&text) else {
            self.status = Some("Enter a valid launcher hotkey.".into());
            cx.notify();
            return;
        };
        if legacy_virtual_key(parsed.key).is_none() {
            self.status = Some("That key is not supported by the launcher listener.".into());
            cx.notify();
            return;
        }
        if baseline.value.as_deref() == Some(text.as_str()) {
            self.input.update(cx, |input, cx| {
                input.set_value(baseline.value.clone().unwrap_or_default(), window, cx)
            });
            self.status = Some("Launcher hotkey is unchanged.".into());
            let _ = self.root.update(cx, |root, cx| {
                let others_clean = root
                    .settings_search
                    .as_ref()
                    .is_none_or(|view| !view.read(cx).restart_draft(cx).dirty())
                    && root
                        .settings_actions
                        .as_ref()
                        .is_none_or(|view| !view.read(cx).restart_draft(cx).dirty())
                    && root.command_help.pending.is_none();
                clear_stale_settings_restart_status(&mut root.restart_status, others_clean);
            });
            cx.notify();
            return;
        }
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let root = root.read(cx);
        let Some(turn) = root.invocation.turn() else {
            self.status = Some("Launcher listener is unavailable.".into());
            cx.notify();
            return;
        };
        if turn.hotkey() != parse_hotkey(baseline.value.as_deref().unwrap_or("F2")) {
            self.status = Some(format!(
                "Saved and active launcher hotkeys differ. {RECOVERY}"
            ));
            cx.notify();
            return;
        }
        let prior_generation = turn.generation();
        let Some(permit) = root.restart_admission.admit() else {
            return;
        };
        let wake = self.wake.clone();
        let ticket = match root.settings_save_workers.begin_launcher_hotkey_save(
            root.settings_path.clone(),
            baseline.clone(),
            Some(text),
            Arc::new(move || wake.notify()),
        ) {
            Ok(ticket) => ticket,
            Err(error) => {
                self.status = Some(error);
                cx.notify();
                return;
            }
        };
        if let Ok(mut proof) = self.proof.lock() {
            *proof = Some(RecoveryProof {
                baseline,
                generation: prior_generation,
                request_id: ticket.id(),
            });
        }
        self.ticket = Some(ticket);
        self.input
            .update(cx, |input, cx| input.set_readonly(true, cx));
        self.status = Some("Saving launcher hotkey…".into());
        let signal = self.wake.clone();
        let receipt = self.mount.clone();
        let host = self.host;
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            while signal.wait().await {
                let Some(view) = weak.upgrade() else { break; };
                let done = update_kit_window_content(host, &view, cx, |view, window, cx| {
                    if !receipt.is_current() || !receipt.matches(&view.mount) { return true; }
                    let Some(ticket) = view.ticket.as_ref() else { return true; };
                    let (events, disconnected) = drain_events(|| ticket.try_recv());
                    for event in events { match event {
                        LauncherHotkeySaveEvent::ReconciliationRequired(message) => { view.reconciliation = true; view.reconciliation_owner.store(true, Ordering::Release); view.status = Some(message); cx.notify(); }
                        LauncherHotkeySaveEvent::Completed { result, reconciliation_required } => {
                            let id = view.ticket.take().map(|ticket| ticket.id());
                            view.reconciliation = reconciliation_required;
                            view.reconciliation_owner.store(reconciliation_required, Ordering::Release);
                            view.input.update(cx, |input, cx| input.set_readonly(reconciliation_required, cx));
                            match result {
                                Ok(ack) => {
                                    let active = view.root.update(cx, |root, cx| {
                                        let active = root.invocation.turn().is_some_and(|turn| turn.current() && turn.generation() == ack.applied.generation.0);
                                        if active {
                                            let mut lines = root.quick_help_hotkeys.to_vec();
                                            if let Some(line) = lines.first_mut() { line.chord = ack.snapshot.value.clone().unwrap_or_else(|| "F2".into()); }
                                            root.quick_help_hotkeys = lines.into(); cx.notify();
                                            let others_clean = root.settings_search.as_ref().is_none_or(|view| !view.read(cx).restart_draft(cx).dirty()) && root.settings_actions.as_ref().is_none_or(|view| !view.read(cx).restart_draft(cx).dirty()) && root.command_help.pending.is_none();
                                            clear_stale_settings_restart_status(&mut root.restart_status, others_clean);
                                        }
                                        if let Some(id) = id { root.settings_save_workers.acknowledge_boolean_preference_save(id); }
                                        active
                                    }).unwrap_or(false);
                                    if active { let text = ack.snapshot.value.clone().unwrap_or_default(); view.input.update(cx, |input, cx| input.set_value(text, window, cx)); view.baseline = Some(ack.snapshot); if let Ok(mut proof) = view.proof.lock() { *proof = None; } view.status = Some("Launcher hotkey saved and active.".into()); }
                                    else { view.reconciliation = true; view.reconciliation_owner.store(true, Ordering::Release); view.input.update(cx, |input, cx| input.set_readonly(true, cx)); view.status = Some(format!("The saved hotkey could not be confirmed active. {RECOVERY}")); }
                                }
                                Err(error) => {
                                    if !view.reconciliation { if let Ok(mut proof) = view.proof.lock() { *proof = None; } if let Some(id) = id { let _ = view.root.update(cx, |root, _| root.settings_save_workers.acknowledge_boolean_preference_save(id)); } }
                                    view.status = Some(if view.reconciliation { format!("{error} {RECOVERY}") } else { error });
                                }
                            }
                            cx.notify(); return true;
                        }
                    } }
                    if disconnected { view.ticket.take(); view.reconciliation = true; view.reconciliation_owner.store(true, Ordering::Release); view.input.update(cx, |input, cx| input.set_readonly(true, cx)); view.status = Some(format!("Hotkey save ended without a terminal receipt. Check the saved and active routes, or {RECOVERY}")); cx.notify(); return true; }
                    false
                }).unwrap_or(true);
                if done { break; }
            }
        }).detach();
        cx.notify();
    }

    fn reconcile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy()
            || !self.reconciliation
            || !self.current(window, cx)
            || window.has_active_dialog(cx)
        {
            return;
        }
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let root = root.read(cx);
        let Some(permit) = root.restart_admission.admit() else {
            return;
        };
        let path = root.settings_path.clone();
        let owner = root.invocation.clone();
        let Some(proof) = self.proof.lock().ok().and_then(|proof| proof.clone()) else {
            self.status = Some(format!(
                "The prior route cannot be independently proven. {RECOVERY}"
            ));
            cx.notify();
            return;
        };
        let Some(native) = root.native_window else {
            return;
        };
        let visibility = root.root_visibility.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let workers = root.settings_save_workers.clone();
        let receipt = self.mount.clone();
        let effect_receipt = receipt.clone();
        let host = self.host;
        self.reading = true;
        self.input
            .update(cx, |input, cx| input.set_readonly(true, cx));
        let effect_proof = proof.clone();
        let task = cx.background_executor().spawn(async move {
            Settings::with_exact_launcher_hotkey_snapshot(path, &effect_proof.baseline, || {
                owner
                    .with_generation(effect_proof.generation, |active| {
                        if !effect_receipt.is_current()
                            || !reconciliation_visible(&visibility, revision, native)
                            || parse_hotkey(effect_proof.baseline.value.as_deref().unwrap_or("F2"))
                                != Some(active)
                        {
                            return false;
                        }
                        workers.acknowledge_boolean_preference_save(effect_proof.request_id);
                        true
                    })
                    .filter(|matched| *matched)
                    .map(|_| ())
                    .ok_or_else(|| {
                        "The active launcher generation or foreground changed.".to_owned()
                    })
            })
        });
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = task.await;
            if let Some(view) = weak.upgrade() {
                let _ = update_kit_window_content(host, &view, cx, |view, _, cx| {
                    if !receipt.is_current() || !receipt.matches(&view.mount) { return; }
                    view.reading = false;
                    view.status = Some(match result {
                        Ok(()) => {
                            view.reconciliation = false; view.reconciliation_owner.store(false, Ordering::Release);
                            view.input.update(cx, |input, cx| input.set_readonly(false, cx));
                            view.baseline = Some(proof.baseline);
                            if let Ok(mut saved) = view.proof.lock() { *saved = None; }
                            "The saved and active prior launcher hotkey are confirmed. You can retry the edit.".into()
                        }
                        Err(error) => format!("Hotkey state remains unresolved ({error}). {RECOVERY}"),
                    });
                    cx.notify();
                });
            }
        }).detach();
        cx.notify();
    }
}

impl Render for SettingsHotkey {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.busy();
        let mount = self.mount.clone();
        div()
            .id("settings-launcher-hotkey")
            .flex()
            .flex_col()
            .gap_2()
            .child(Label::new("Launcher hotkey"))
            .child(
                Input::new(&self.input)
                    .readonly(busy || self.reconciliation || self.baseline.is_none())
                    .aria_label("Launcher hotkey"),
            )
            .child(
                Button::new("settings-launcher-hotkey-save")
                    .label("Save launcher hotkey")
                    .disabled(busy || self.reconciliation || self.baseline.is_none())
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if mount.is_current() && mount.matches(&view.mount) {
                            view.save(window, cx);
                        }
                    })),
            )
            .when(self.ticket.is_some(), |panel| {
                panel.child(
                    Button::new("settings-launcher-hotkey-cancel-save")
                        .label("Cancel hotkey save")
                        .on_click(cx.listener(|view, _, window, cx| {
                            if view.current(window, cx)
                                && let Some(ticket) = &view.ticket
                            {
                                view.status = Some(
                                    if ticket.request_cancel() {
                                        "Cancelling launcher hotkey save…"
                                    } else {
                                        "The launcher hotkey commit is already settling."
                                    }
                                    .into(),
                                );
                                cx.notify();
                            }
                        })),
                )
            })
            .when(self.reconciliation, |panel| {
                panel.child(
                    Button::new("settings-launcher-hotkey-reconcile")
                        .label("Check saved and active hotkey")
                        .disabled(busy)
                        .on_click(cx.listener(|view, _, window, cx| view.reconcile(window, cx))),
                )
            })
            .children(
                self.status
                    .clone()
                    .map(|status| Label::new(status).id("settings-launcher-hotkey-status")),
            )
    }
}

impl RootWindow {
    pub(super) fn open_settings_hotkey(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_hotkey.is_some() {
            return;
        }
        let root = cx.entity().downgrade();
        let reconciliation = self.hotkey_reconciliation.clone();
        let proof = self.hotkey_reconciliation_proof.clone();
        let view = cx.new(|cx| SettingsHotkey::new(root, reconciliation, proof, window, cx));
        self.settings_hotkey = Some(view.clone());
        window.defer(cx, move |_, app| view.update(app, |view, cx| view.read(cx)));
    }
}

#[cfg(test)]
mod tests {
    use super::drain_events;
    use multi_launcher::LauncherHotkeySaveEvent;

    #[test]
    fn coalesced_wake_drains_pending_and_terminal_events() {
        let (sender, receiver) = std::sync::mpsc::channel();
        sender
            .send(LauncherHotkeySaveEvent::ReconciliationRequired(
                "waiting".into(),
            ))
            .unwrap();
        sender
            .send(LauncherHotkeySaveEvent::Completed {
                result: Err("finished".into()),
                reconciliation_required: false,
            })
            .unwrap();
        let (events, disconnected) = drain_events(|| receiver.try_recv());
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            LauncherHotkeySaveEvent::ReconciliationRequired(_)
        ));
        assert!(matches!(
            &events[1],
            LauncherHotkeySaveEvent::Completed { .. }
        ));
        assert!(!disconnected);
        drop(sender);
        assert!(drain_events(|| receiver.try_recv()).1);
    }
}
