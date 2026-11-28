//! Retained Actions and Feedback preferences backed by checked scoped Settings writes.
use super::*;
use multi_launcher::settings::{
    ScopedSetting as Field, ScopedSettingChange as Change, ScopedSettingValue as Value,
    ScopedSettingsAcknowledgement, ScopedSettingsError, Settings, SettingsBooleanSaveToken,
};

pub(super) const WAIT: &str = "Wait for Actions and Feedback settings to finish.";

#[derive(Clone, Debug, PartialEq)]
struct Values {
    preserve: bool,
    clear: bool,
    hide: bool,
    clipboard_hide: bool,
    toasts: bool,
    duration: f32,
    inline: bool,
    errors: bool,
}
impl Values {
    fn from_settings(settings: &Settings) -> Self {
        Self {
            preserve: settings.preserve_command,
            clear: settings.clear_query_after_run,
            hide: settings.hide_after_run,
            clipboard_hide: matches!(Field::ClipboardModifyHideAfterApply.value(settings), Value::Bool(true)),
            toasts: settings.enable_toasts,
            duration: settings.toast_duration,
            inline: settings.show_inline_errors,
            errors: settings.show_error_toasts,
        }
    }
    fn changes(&self, desired: &Self) -> Vec<Change> {
        [
            (Field::PreserveCommand, Value::Bool(self.preserve), Value::Bool(desired.preserve)),
            (
                Field::ClearQueryAfterRun,
                Value::Bool(self.clear),
                Value::Bool(desired.clear),
            ),
            (
                Field::HideAfterRun,
                Value::Bool(self.hide),
                Value::Bool(desired.hide),
            ),
            (
                Field::ClipboardModifyHideAfterApply,
                Value::Bool(self.clipboard_hide),
                Value::Bool(desired.clipboard_hide),
            ),
            (
                Field::EnableToasts,
                Value::Bool(self.toasts),
                Value::Bool(desired.toasts),
            ),
            (
                Field::ToastDuration,
                Value::Float(self.duration),
                Value::Float(desired.duration),
            ),
            (
                Field::ShowInlineErrors,
                Value::Bool(self.inline),
                Value::Bool(desired.inline),
            ),
            (
                Field::ShowErrorToasts,
                Value::Bool(self.errors),
                Value::Bool(desired.errors),
            ),
        ]
        .into_iter()
        .filter(|(_, baseline, desired)| baseline != desired)
        .map(|(field, baseline, desired)| Change {
            field,
            baseline,
            desired,
        })
        .collect()
    }
}

fn resolve_duration(text: &str, baseline: f32, toasts: bool) -> Result<f32, &'static str> {
    if text == baseline.to_string() {
        return Ok(baseline);
    }
    match text.trim().parse::<f32>() {
        Ok(value) if value.is_finite() && (0.1..=5.0).contains(&value) => Ok(value),
        _ if !toasts => Ok(baseline),
        _ => Err("Toast duration must be a number from 0.1 to 5.0 seconds."),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RestartDraft {
    baseline: Option<Values>,
    draft: Option<Values>,
    duration_text: String,
    busy: bool,
}
impl RestartDraft {
    pub(super) fn dirty(&self) -> bool {
        self.baseline != self.draft
            || self
                .baseline
                .as_ref()
                .is_some_and(|base| self.duration_text != base.duration.to_string())
    }
}

pub(super) struct ActionsSettings {
    root: WeakEntity<RootWindow>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    checkbox_focus: HashMap<Field, FocusHandle>,
    scroll: ScrollHandle,
    baseline: Option<Values>,
    draft: Option<Values>,
    duration: Entity<InputState>,
    _duration_sub: Subscription,
    reading: bool,
    pending: Option<(SettingsBooleanSaveToken, Vec<Change>)>,
    status: Option<String>,
    clipboard_status: Option<String>,
}
impl ActionsSettings {
    pub(super) fn new(
        root: WeakEntity<RootWindow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let duration = cx.new(|cx| InputState::new(window, cx).placeholder("0.1–5.0 seconds"));
        let sub = cx.subscribe(&duration, |_: &mut Self, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let checkbox_focus = [
            Field::PreserveCommand,
            Field::ClearQueryAfterRun,
            Field::HideAfterRun,
            Field::ClipboardModifyHideAfterApply,
            Field::EnableToasts,
            Field::ShowInlineErrors,
            Field::ShowErrorToasts,
        ]
        .into_iter()
        .map(|field| (field, cx.focus_handle()))
        .collect();
        Self {
            root,
            owner,
            mount,
            checkbox_focus,
            scroll: ScrollHandle::new(),
            baseline: None,
            draft: None,
            duration,
            _duration_sub: sub,
            reading: false,
            pending: None,
            status: None,
            clipboard_status: None,
        }
    }
    pub(super) fn can_focus(&self, target: settings_navigation::Target) -> bool {
        use settings_navigation::Target::*;
        if self.busy() || !self.mount.is_current() {
            return false;
        }
        let Some(draft) = &self.draft else {
            return false;
        };
        match target {
            PreserveCommand | ClearQuery | HideWindow | ClipboardHideAfterApply | Toasts | Inline | ErrorToasts => true,
            Duration => draft.toasts,
            _ => false,
        }
    }
    pub(super) fn focus_control(
        &mut self,
        target: settings_navigation::Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use settings_navigation::Target::*;
        if !self.can_focus(target) {
            return false;
        }
        let (index, focus) = match target {
            PreserveCommand => (1, self.checkbox_focus.get(&Field::PreserveCommand).cloned()),
            ClearQuery => (
                2,
                self.checkbox_focus.get(&Field::ClearQueryAfterRun).cloned(),
            ),
            HideWindow => (3, self.checkbox_focus.get(&Field::HideAfterRun).cloned()),
            ClipboardHideAfterApply => (4, self.checkbox_focus.get(&Field::ClipboardModifyHideAfterApply).cloned()),
            Toasts => (5, self.checkbox_focus.get(&Field::EnableToasts).cloned()),
            Inline => (
                6,
                self.checkbox_focus.get(&Field::ShowInlineErrors).cloned(),
            ),
            ErrorToasts => (7, self.checkbox_focus.get(&Field::ShowErrorToasts).cloned()),
            Duration => (9, Some(self.duration.focus_handle(cx))),
            _ => return false,
        };
        let Some(focus) = focus else {
            return false;
        };
        self.scroll.scroll_to_top_of_item(index);
        focus.focus(window, cx);
        cx.notify();
        true
    }

    pub(super) fn busy(&self) -> bool {
        self.reading || self.pending.is_some()
    }
    pub(super) fn restart_draft(&self, cx: &App) -> RestartDraft {
        RestartDraft {
            baseline: self.baseline.clone(),
            draft: self.draft.clone(),
            duration_text: self.duration.read(cx).value().to_string(),
            busy: self.busy(),
        }
    }
    pub(super) fn retire(&mut self) {
        self.owner.retire();
    }
    fn clear_wait(&self, cx: &mut Context<Self>) {
        let _ = self.root.update(cx, |root, cx| {
            if root.restart_status.as_deref() == Some(WAIT) {
                root.restart_status = None;
            }
            if root.settings_editor.status.as_deref() == Some(WAIT) {
                root.settings_editor.status = None;
            }
            cx.notify();
        });
    }
    pub(super) fn read(
        &mut self,
        path: String,
        workers: SettingsSaveWorkerRegistry,
        permit: EffectPermit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reading = true;
        self.status = None;
        let receiver = match workers.begin_scoped_preferences_read(path) {
            Ok(receiver) => receiver,
            Err(error) => {
                self.reading = false;
                self.status = Some(error);
                cx.notify();
                return;
            }
        };
        let host = window.window_handle();
        let mount = self.mount.clone();
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = loop {
                match receiver.try_recv() {
                    Ok(result) => break result,
                    Err(TryRecvError::Disconnected) => {
                        break Err("Settings reader ended without a receipt.".into());
                    }
                    Err(TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(20))
                            .await
                    }
                }
            };
            let Some(entity) = weak.upgrade() else {
                return;
            };
            let updated = update_kit_window_content(host, &entity, cx, |view, window, cx| {
                if !mount.is_current() || !mount.matches(&view.mount) || !view.reading {
                    return;
                }
                let installed = view.root.upgrade().is_some_and(|root| {
                    let root = root.read(cx);
                    root.settings_editor.open
                        && root
                            .settings_actions
                            .as_ref()
                            .is_some_and(|current| current.entity_id() == cx.entity_id())
                        && root
                            .root_window
                            .is_some_and(|current| current.window_id() == host.window_id())
                });
                if !installed {
                    return;
                }
                view.reading = false;
                match result {
                    Ok(settings) => {
                        let values = Values::from_settings(&settings);
                        view.duration.update(cx, |input, cx| {
                            input.set_value(values.duration.to_string(), window, cx)
                        });
                        view.baseline = Some(values.clone());
                        view.draft = Some(values);
                        view.status = None;
                    }
                    Err(error) => {
                        view.status = Some(format!(
                            "Could not read Actions and Feedback settings: {error}"
                        ))
                    }
                }
                view.clear_wait(cx);
                cx.notify();
            });
            if updated.is_err() {
                let _ = entity.update(cx, |view, cx| {
                    if mount.is_current() {
                        view.reading = false;
                        view.status = Some("Settings window is unavailable.".into());
                        view.clear_wait(cx);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        cx.notify();
    }
    fn open_clipboard_configuration(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy() || !self.mount.is_current() || !window.is_window_active() || window.has_active_dialog(cx) { return; }
        let Some(root) = self.root.upgrade() else { return; };
        let state = root.read(cx);
        if !state.settings_editor.open || state.restart_admission.is_fenced() || state.clipboard_modify_startup.pending
            || state.clipboard_modify_startup.refresh_pending
            || !state.settings_actions.as_ref().is_some_and(|view| view.entity_id() == cx.entity_id())
            || state.root_window.is_none_or(|host| host.window_id() != window.window_handle().window_id())
            || !state.external_owner_is_current(window.focused(cx).as_ref(), window, cx) { return; }
        let Some(native) = state.native_window.filter(|native| current_foreground_identity() == Some(native.identity)) else { return; };
        let Some(path) = state.clipboard_modify_startup.runtime.as_ref().map(|runtime| runtime.store.path.clone()) else { return; };
        let Some(permit) = state.restart_admission.admit() else { return; };
        let Some(focus) = window.focused(cx) else { return; };
        let visibility = state.root_visibility.clone(); let admission = state.restart_admission.clone();
        let host_revision = visibility.request_revision.load(Ordering::Acquire);
        let mount = self.mount.clone(); let worker_mount = mount.clone();
        let lease = Arc::new(AtomicBool::new(true)); let revoked = lease.clone();
        let guard = cx.on_blur(&focus, window, move |_, _, _| { revoked.store(false, Ordering::Release); });
        if !root.update(cx, |root, cx| {
            if root.clipboard_modify_startup.refresh_pending || !root.external_launch.begin_auxiliary_effect(lease.clone(), false) { return false; }
            root.clipboard_modify_startup.refresh_pending = true; cx.notify(); true
        }) { return; }
        self.clipboard_status = None;
        let worker_lease = lease.clone(); let presentation_visibility = visibility.clone();
        let task = cx.background_executor().spawn(async move {
            clipboard_modify_startup::open_configuration_with(&path, &|| worker_mount.is_current()
                && worker_lease.load(Ordering::Acquire) && !admission.is_fenced()
                && current_foreground_identity() == Some(native.identity)
                && json_utility::native_host_current(&visibility, host_revision, Some(native), false),
                |path, current| multi_launcher::actions::exec::launch_association_if_current(path, current))
        });
        let host = window.window_handle(); let weak_root = root.downgrade();
        cx.spawn(async move |weak, cx| {
            let result = task.await; let _permit = permit; let _guard = guard;
            let _ = weak_root.update(cx, |root, cx| {
                root.external_launch.finish_auxiliary_effect(&lease); root.clipboard_modify_startup.refresh_pending = false;
                if root.restart_status.as_deref() == Some(clipboard_modify_view::BLOCK_WAIT) { root.restart_status = None; }
                if root.launcher_search_error.as_deref() == Some(clipboard_modify_view::BLOCK_WAIT) || root.launcher_search_error.as_deref() == Some(clipboard_modify_view::LAUNCHER_EFFECT_WAIT) { root.launcher_search_error = None; }
                cx.notify();
            });
            let _ = host.update(cx, |_, window, cx| {
                let active = lease.load(Ordering::Acquire) && focus.is_focused(window) && window.is_window_active()
                    && !window.has_active_dialog(cx) && current_foreground_identity() == Some(native.identity)
                    && json_utility::native_host_current(&presentation_visibility, host_revision, Some(native), false);
                let _ = weak.update(cx, |view, cx| {
                    if !active || !mount.is_current() || !mount.matches(&view.mount) || !view.root.upgrade().is_some_and(|root| {
                        let root = root.read(cx); root.settings_editor.open && root.settings_actions.as_ref().is_some_and(|current| current.entity_id() == cx.entity_id())
                            && root.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
                    }) { return; }
                    view.clipboard_status = Some(if result.is_ok() { "Opened configuration." } else { "Configuration could not be opened." }.into()); cx.notify();
                });
            });
        }).detach();
        cx.notify();
    }
    fn reload_clipboard_configuration(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy() || !self.mount.is_current() || !window.is_window_active() || window.has_active_dialog(cx) { return; }
        let Some(root) = self.root.upgrade() else { return; };
        let state = root.read(cx);
        if !state.settings_editor.open || state.restart_admission.is_fenced() || state.clipboard_modify_startup.pending
            || state.clipboard_modify_startup.refresh_pending || state.external_launch.clipboard_effect_pending()
            || state.clipboard_modify_view.as_ref().is_some_and(|view| view.read(cx).configuration_reload_blocked())
            || !state.settings_actions.as_ref().is_some_and(|view| view.entity_id() == cx.entity_id())
            || state.root_window.is_none_or(|host| host.window_id() != window.window_handle().window_id())
            || !state.external_owner_is_current(window.focused(cx).as_ref(), window, cx) { return; }
        let Some(native) = state.native_window.filter(|native| current_foreground_identity() == Some(native.identity)) else { return; };
        let Some(runtime) = state.clipboard_modify_startup.runtime.clone() else { return; };
        let Some(permit) = state.restart_admission.admit() else { return; };
        let Some(focus) = window.focused(cx) else { return; };
        let visibility = state.root_visibility.clone(); let admission = state.restart_admission.clone();
        let revision = visibility.request_revision.load(Ordering::Acquire);
        let mount = self.mount.clone(); let worker_mount = mount.clone();
        let lease = Arc::new(AtomicBool::new(true)); let revoked = lease.clone();
        let guard = cx.on_blur(&focus, window, move |_, _, _| { revoked.store(false, Ordering::Release); });
        if !root.update(cx, |root, cx| {
            if root.clipboard_modify_startup.refresh_pending || !root.external_launch.begin_auxiliary_effect(lease.clone(), true) { return false; }
            root.clipboard_modify_startup.refresh_pending = true;
            cx.notify(); true
        }) { return; }
        self.clipboard_status = None;
        let presentation_visibility = visibility.clone();
        let worker_lease = lease.clone();
        let task = cx.background_executor().spawn(async move {
            runtime.store.refresh_checked(&|| worker_mount.is_current() && worker_lease.load(Ordering::Acquire)
                && !admission.is_fenced() && current_foreground_identity() == Some(native.identity)
                && json_utility::native_host_current(&visibility, revision, Some(native), false))
        });
        let host = window.window_handle(); let weak_root = root.downgrade();
        cx.spawn(async move |weak, cx| {
            let result = task.await; let _permit = permit; let _guard = guard;
            let status = match &result {
                Ok(multi_launcher::clipboard_modify::store::CheckedCatalogRead::Loaded(_)) => "Configuration reloaded.",
                Ok(multi_launcher::clipboard_modify::store::CheckedCatalogRead::Changed) => "Configuration changed during reload; try again.",
                Ok(multi_launcher::clipboard_modify::store::CheckedCatalogRead::Superseded) => "Reload cancelled.",
                Err(multi_launcher::clipboard_modify::store::CheckedCatalogError::InvalidCurrent) => "Configuration is invalid; previous settings retained.",
                Err(_) => "Configuration unavailable; previous settings retained.",
            };
            let presentation_current = host.update(cx, |_, window, cx| {
                let active = lease.load(Ordering::Acquire) && window.is_window_active() && !window.has_active_dialog(cx)
                    && current_foreground_identity() == Some(native.identity)
                    && json_utility::native_host_current(&presentation_visibility, revision, Some(native), false);
                weak.upgrade().is_some_and(|view| {
                    let entity_id = view.entity_id();
                    let view = view.read(cx);
                    mount.is_current() && mount.matches(&view.mount) && focus.is_focused(window)
                        && active
                        && view.root.upgrade().is_some_and(|root| {
                            let root = root.read(cx);
                            root.settings_editor.open && root.settings_actions.as_ref().is_some_and(|current| current.entity_id() == entity_id)
                                && root.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
                        })
                })
            }).unwrap_or(false);
            let mut target = None;
            let _ = weak_root.update(cx, |root, cx| {
                if let Ok(multi_launcher::clipboard_modify::store::CheckedCatalogRead::Loaded(catalog)) = &result {
                    if root.clipboard_modify_startup.runtime.as_ref().is_some_and(|runtime| clipboard_modify_startup::catalog_is_current(runtime, catalog)) {
                        root.clipboard_modify_startup.diagnostic = None;
                        let query = root.query.read(cx).value().to_string();
                        if clipboard_modify_startup::is_clipboard_modify_search(&query) { root.refresh_launcher_results_with_viewport(query, false, cx); }
                        target = root.clipboard_modify_view.clone();
                    }
                }
                cx.notify();
            });
            if let (Some(view), Ok(multi_launcher::clipboard_modify::store::CheckedCatalogRead::Loaded(catalog))) = (target, result) {
                let _ = view.update(cx, |view, cx| view.adopt_configuration_reload(catalog, cx));
            }
            let _ = weak_root.update(cx, |root, cx| {
                root.external_launch.finish_auxiliary_effect(&lease);
                root.clipboard_modify_startup.refresh_pending = false;
                if root.restart_status.as_deref() == Some(clipboard_modify_view::BLOCK_WAIT) { root.restart_status = None; }
                if root.launcher_search_error.as_deref() == Some(clipboard_modify_view::BLOCK_WAIT) || root.launcher_search_error.as_deref() == Some(clipboard_modify_view::LAUNCHER_EFFECT_WAIT) { root.launcher_search_error = None; }
                cx.notify();
            });
            if presentation_current { let _ = host.update(cx, |_, _, cx| { let _ = weak.update(cx, |view, cx| { view.clipboard_status = Some(status.to_owned()); cx.notify(); }); }); }
        }).detach();
        cx.notify();
    }
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy() || !self.mount.is_current() {
            return;
        }
        let (Some(baseline), Some(mut desired)) = (self.baseline.clone(), self.draft.clone())
        else {
            return;
        };
        let text = self.duration.read(cx).value().to_string();
        desired.duration = match resolve_duration(&text, baseline.duration, desired.toasts) {
            Ok(duration) => duration,
            Err(message) => {
                self.status = Some(message.into());
                cx.notify();
                return;
            }
        };
        let changes = baseline.changes(&desired);
        if changes.is_empty() {
            self.duration.update(cx, |input, cx| {
                input.set_value(baseline.duration.to_string(), window, cx)
            });
            self.status = Some("No changes to save.".into());
            cx.notify();
            return;
        }
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let has_active_dialog = window.has_active_dialog(cx);
        let state = root.read(cx);
        if !state.settings_editor.open
            || state.restart_admission.is_fenced()
            || !window.is_window_active()
            || has_active_dialog
            || !state
                .settings_actions
                .as_ref()
                .is_some_and(|view| view.entity_id() == cx.entity_id())
            || state
                .root_window
                .is_none_or(|host| host.window_id() != window.window_handle().window_id())
            || !state.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            || state
                .native_window
                .is_none_or(|native| current_foreground_identity() != Some(native.identity))
        {
            return;
        }
        let Some(permit) = state.restart_admission.admit() else {
            return;
        };
        let workers = state.settings_save_workers.clone();
        let path = state.settings_path.clone();
        let (receiver, token) = match workers.begin_scoped_preferences_save(path, changes.clone()) {
            Ok(pending) => pending,
            Err(error) => {
                self.status = Some(error);
                cx.notify();
                return;
            }
        };
        self.pending = Some((token.clone(), changes.clone()));
        self.status = None;
        self.duration
            .update(cx, |input, cx| input.set_readonly(true, cx));
        let host = window.window_handle();
        let mount = self.mount.clone();
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = loop {
                match receiver.try_recv() {
                    Ok(result) => break result,
                    Err(TryRecvError::Disconnected) => {
                        break Err(ScopedSettingsError::Store(
                            "Settings writer ended without an acknowledgement.".into(),
                        ));
                    }
                    Err(TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(20))
                            .await
                    }
                }
            };
            let Some(entity) = weak.upgrade() else {
                return;
            };
            let fallback = mount.clone();
            let updated = update_kit_window_content(host, &entity, cx, |view, window, cx| {
                if !mount.is_current()
                    || !mount.matches(&view.mount)
                    || view
                        .pending
                        .as_ref()
                        .is_none_or(|(pending, _)| pending.id() != token.id())
                {
                    return;
                }
                let installed = view.root.upgrade().is_some_and(|root| {
                    let root = root.read(cx);
                    root.settings_editor.open
                        && root
                            .settings_actions
                            .as_ref()
                            .is_some_and(|current| current.entity_id() == cx.entity_id())
                        && root
                            .root_window
                            .is_some_and(|current| current.window_id() == host.window_id())
                        && !root.restart_admission.is_fenced()
                });
                if !installed {
                    return;
                }
                workers.acknowledge_boolean_preference_save(token.id());
                view.pending = None;
                view.duration
                    .update(cx, |input, cx| input.set_readonly(false, cx));
                match result {
                    Ok(ack) if ack_matches(&changes, &ack) => {
                        view.baseline = Some(desired.clone());
                        view.draft = Some(desired.clone());
                        view.duration.update(cx, |input, cx| {
                            input.set_value(desired.duration.to_string(), window, cx)
                        });
                        let _ = view.root.update(cx, |root, cx| {
                            apply_if_acknowledged(
                                &mut root.external_launch_preferences,
                                &changes,
                                &ack,
                            );
                            if root.restart_status.as_deref() == Some(WAIT) {
                                root.restart_status = None;
                            }
                            cx.notify();
                        });
                        view.status = Some("Actions and Feedback settings saved.".into());
                    }
                    Ok(_) => {
                        view.status = Some(
                            "Settings acknowledgement did not match the requested values.".into(),
                        )
                    }
                    Err(ScopedSettingsError::CommittedUnverified(error)) => {
                        view.baseline = None;
                        view.draft = None;
                        view.status = Some(format!("Settings were written but could not be verified: {error}. Close and reopen Settings to read the file; relaunch to reconcile live behavior."));
                    }
                    Err(error) => {
                        view.status = Some(format!("Could not save Actions and Feedback: {error}"))
                    }
                }
                view.clear_wait(cx);
                cx.notify();
            });
            if updated.is_err() {
                let _ = entity.update(cx, |view, cx| {
                    if fallback.is_current()
                        && view
                            .pending
                            .as_ref()
                            .is_some_and(|(pending, _)| pending.id() == token.id())
                    {
                        workers.acknowledge_boolean_preference_save(token.id());
                        view.pending = None;
                        view.duration
                            .update(cx, |input, cx| input.set_readonly(false, cx));
                        view.status = Some(
                            "Settings window is unavailable; reopen Settings to reload.".into(),
                        );
                        view.clear_wait(cx);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        cx.notify();
    }
}

pub(super) fn ack_matches(changes: &[Change], ack: &ScopedSettingsAcknowledgement) -> bool {
    ack.values.len() == changes.len()
        && changes.iter().all(|change| {
            ack.values
                .iter()
                .any(|(field, value)| *field == change.field && *value == change.desired)
        })
}
fn apply_if_acknowledged(
    settings: &mut external_launch::Settings,
    changes: &[Change],
    ack: &ScopedSettingsAcknowledgement,
) -> bool {
    if !ack_matches(changes, ack) {
        return false;
    }
    apply_acknowledged(settings, changes);
    true
}

fn apply_acknowledged(settings: &mut external_launch::Settings, changes: &[Change]) {
    for change in changes {
        match (&change.field, &change.desired) {
            (Field::PreserveCommand, Value::Bool(value)) => settings.preserve_command = *value,
            (Field::ClearQueryAfterRun, Value::Bool(value)) => {
                settings.clear_query_after_run = *value
            }
            (Field::HideAfterRun, Value::Bool(value)) => settings.hide_after_run = *value,
            (Field::ClipboardModifyHideAfterApply, Value::Bool(value)) => settings.clipboard_modify_hide_after_apply = *value,
            (Field::EnableToasts, Value::Bool(value)) => settings.enable_toasts = *value,
            (Field::ToastDuration, Value::Float(value)) => settings.toast_duration = *value,
            (Field::ShowInlineErrors, Value::Bool(value)) => settings.show_inline_errors = *value,
            (Field::ShowErrorToasts, Value::Bool(value)) => settings.show_error_toasts = *value,
            _ => {}
        }
    }
}

impl Render for ActionsSettings {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.busy();
        let scroll = self.scroll.clone();
        let mut panel = div()
            .id("settings-actions-feedback")
            .flex()
            .flex_col()
            .gap_2()
            .max_h(px(350.0))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_scroll(move |event, _, cx| {
                let pixels = event.delta.pixel_delta(px(20.0));
                let delta = if pixels.y.is_zero() {
                    pixels.x
                } else {
                    pixels.y
                }
                .as_f32();
                let offset = scroll.offset();
                let maximum = scroll.max_offset().y.as_f32();
                let previous = (offset.y.as_f32() - delta).clamp(-maximum, 0.0);
                scroll.set_offset(point(offset.x, px(offset.y.as_f32().clamp(-maximum, 0.0))));
                if inner_results_consumed_scroll(previous, maximum, delta) {
                    cx.stop_propagation();
                }
            })
            .child(Label::new("Actions and Feedback"));
        if let Some(draft) = &self.draft {
            for (field, label, checked) in [
                (Field::PreserveCommand, "Preserve command after run", draft.preserve),
                (
                    Field::ClearQueryAfterRun,
                    "Clear query after run",
                    draft.clear,
                ),
                (
                    Field::HideAfterRun,
                    "Hide window after running action",
                    draft.hide,
                ),
                (
                    Field::ClipboardModifyHideAfterApply,
                    "Hide Launcher after applying template or pipeline",
                    draft.clipboard_hide,
                ),
                (
                    Field::EnableToasts,
                    "Enable toast notifications",
                    draft.toasts,
                ),
                (Field::ShowInlineErrors, "Show inline errors", draft.inline),
                (Field::ShowErrorToasts, "Show error toasts", draft.errors),
            ] {
                let weak = cx.entity().downgrade();
                let mount = self.mount.clone();
                let Some(focus) = self.checkbox_focus.get(&field).cloned() else {
                    continue;
                };
                let palette = appearance::current(cx).palette;
                let focused = focus.is_focused(window);
                panel = panel.child(
                    Checkbox::new(SharedString::from(format!("actions-setting-{field:?}")))
                        .checked(checked)
                        .track_focus(&focus)
                        .disabled(busy)
                        .accessibility_label(label)
                        .on_change(move |state, _, _, app| {
                            let _ = weak.update(app, |view, cx| {
                                if view.busy() || !mount.is_current() || !mount.matches(&view.mount)
                                {
                                    return;
                                }
                                if let Some(draft) = &mut view.draft {
                                    let value = state == CheckboxState::Checked;
                                    match field {
                                        Field::PreserveCommand => draft.preserve = value,
                                        Field::ClearQueryAfterRun => draft.clear = value,
                                        Field::HideAfterRun => draft.hide = value,
                                        Field::ClipboardModifyHideAfterApply => draft.clipboard_hide = value,
                                        Field::EnableToasts => draft.toasts = value,
                                        Field::ShowInlineErrors => draft.inline = value,
                                        Field::ShowErrorToasts => draft.errors = value,
                                        _ => {}
                                    }
                                }
                                view.status = None;
                                cx.notify();
                            });
                        })
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(CheckboxIndicator::new().checked(checked)
                                    .when(matches!(field, Field::PreserveCommand | Field::ClipboardModifyHideAfterApply), |indicator| {
                                        indicator.size(px(18.0)).rounded(px(3.0)).border_1()
                                            .border_color(appearance::to_hsla(if focused { palette.focus } else { palette.control_border }))
                                            .when(checked, |indicator| indicator.bg(appearance::to_hsla(palette.accent)).text_color(appearance::to_hsla(palette.primary_text)))
                                            .items_center().justify_center().child(if checked { "✓" } else { "" })
                                    }))
                                .child(Label::new(label)),
                        ),
                );
            }
            if draft.toasts {
                panel = panel
                    .child(Label::new("Toast duration (seconds)"))
                    .child(Input::new(&self.duration).aria_label("Toast duration in seconds"));
            }
            panel = panel.child(
                Button::new("settings-actions-save")
                    .label(if self.pending.is_some() {
                        "Saving…"
                    } else {
                        "Save Actions and Feedback"
                    })
                    .disabled(busy)
                    .on_click(cx.listener(|view, _, window, cx| view.save(window, cx))),
            );
                    let (loading, status) = self.root.upgrade().map(|root| {
                        let root = root.read(cx);
                        (root.clipboard_modify_startup.refresh_pending, self.clipboard_status.clone().or_else(|| root.clipboard_modify_startup.diagnostic.clone()))
                    }).unwrap_or((false, None));
                    panel = panel.child(Button::new("clipboard-modify-config-reload").label("Reload configuration").disabled(busy || loading)
                        .on_click(cx.listener(|view, _, window, cx| view.reload_clipboard_configuration(window, cx))));
                    panel = panel.child(Button::new("clipboard-modify-config-open").label("Open clipboard_modifiers.json").disabled(busy || loading)
                        .on_click(cx.listener(|view, _, window, cx| view.open_clipboard_configuration(window, cx))));
                    if let Some(status) = status { panel = panel.child(Label::new(status)); }
        } else if self.reading {
            panel = panel.child(Label::new("Reading Actions and Feedback…"));
        }
        if let Some(status) = &self.status {
            panel = panel.child(Label::new(status.clone()).id("settings-actions-status"));
        }
        panel
    }
}
impl RootWindow {
    pub(super) fn open_actions_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_actions.is_some() {
            return;
        }
        let Some(permit) = self.restart_admission.admit() else {
            return;
        };
        let root = cx.entity().downgrade();
        let view = cx.new(|cx| ActionsSettings::new(root, window, cx));
        self.settings_actions = Some(view.clone());
        let path = self.settings_path.clone();
        let workers = self.settings_save_workers.clone();
        view.update(cx, |view, cx| view.read(path, workers, permit, window, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::{Values, ack_matches, apply_if_acknowledged, resolve_duration};
    use multi_launcher::settings::{
        ScopedSetting as Field, ScopedSettingChange as Change, ScopedSettingValue as Value,
        ScopedSettingsAcknowledgement,
    };

    #[test]
    fn live_actions_change_only_after_exact_acknowledgement() {
        let mut settings = super::external_launch::Settings {
            preserve_command: false,
            clear_query_after_run: false,
            hide_after_run: false,
            history_limit: 10,
            enable_toasts: true,
            show_error_toasts: true,
            show_inline_errors: true,
            toast_duration: 2.0,
            clipboard_modify_hide_after_apply: true,
        };
        let changes = vec![Change {
            field: Field::HideAfterRun,
            baseline: Value::Bool(false),
            desired: Value::Bool(true),
        }];
        let wrong = ScopedSettingsAcknowledgement {
            values: vec![(Field::HideAfterRun, Value::Bool(false))],
            persisted: true,
        };
        assert!(!ack_matches(&changes, &wrong));
        assert!(!apply_if_acknowledged(&mut settings, &changes, &wrong));
        assert!(!settings.hide_after_run);
        let right = ScopedSettingsAcknowledgement {
            values: vec![(Field::HideAfterRun, Value::Bool(true))],
            persisted: true,
        };
        assert!(ack_matches(&changes, &right));
        assert!(apply_if_acknowledged(&mut settings, &changes, &right));
        assert!(settings.hide_after_run);
        assert!(settings.enable_toasts);
    }

    #[test]
    fn preserve_command_requires_exact_ack_before_live_adoption() {
        let mut settings = super::external_launch::Settings {
            preserve_command: false, clear_query_after_run: false, hide_after_run: false,
            history_limit: 10, enable_toasts: true, show_error_toasts: true,
            show_inline_errors: true, toast_duration: 2.0, clipboard_modify_hide_after_apply: true,
        };
        let changes = vec![Change { field: Field::PreserveCommand, baseline: Value::Bool(false), desired: Value::Bool(true) }];
        let wrong = ScopedSettingsAcknowledgement { values: vec![(Field::PreserveCommand, Value::Bool(false))], persisted: true };
        assert!(!apply_if_acknowledged(&mut settings, &changes, &wrong)); assert!(!settings.preserve_command);
        let right = ScopedSettingsAcknowledgement { values: vec![(Field::PreserveCommand, Value::Bool(true))], persisted: true };
        assert!(apply_if_acknowledged(&mut settings, &changes, &right)); assert!(settings.preserve_command);
        let disable = vec![Change { field: Field::PreserveCommand, baseline: Value::Bool(true), desired: Value::Bool(false) }];
        let ack = ScopedSettingsAcknowledgement { values: vec![(Field::PreserveCommand, Value::Bool(false))], persisted: true };
        assert!(apply_if_acknowledged(&mut settings, &disable, &ack)); assert!(!settings.preserve_command);
    }

    #[test]
    fn clipboard_visibility_requires_exact_ack_before_live_adoption() {
        let mut settings = super::external_launch::Settings {
            preserve_command: false, clear_query_after_run: false, hide_after_run: false,
            history_limit: 10, enable_toasts: true, show_error_toasts: true,
            show_inline_errors: true, toast_duration: 2.0, clipboard_modify_hide_after_apply: true,
        };
        let changes = vec![Change { field: Field::ClipboardModifyHideAfterApply, baseline: Value::Bool(true), desired: Value::Bool(false) }];
        let wrong = ScopedSettingsAcknowledgement { values: vec![(Field::ClipboardModifyHideAfterApply, Value::Bool(true))], persisted: true };
        assert!(!apply_if_acknowledged(&mut settings, &changes, &wrong));
        assert!(settings.clipboard_modify_hide_after_apply);
        let right = ScopedSettingsAcknowledgement { values: vec![(Field::ClipboardModifyHideAfterApply, Value::Bool(false))], persisted: true };
        assert!(apply_if_acknowledged(&mut settings, &changes, &right));
        assert!(!settings.clipboard_modify_hide_after_apply);
    }

    #[test]
    fn hidden_invalid_duration_preserves_baseline_but_valid_edit_survives() {
        assert_eq!(resolve_duration("invalid", 2.0, false), Ok(2.0));
        assert_eq!(resolve_duration("3.25", 2.0, false), Ok(3.25));
        assert!(resolve_duration("invalid", 2.0, true).is_err());
        assert_eq!(resolve_duration("7", 7.0, true), Ok(7.0));
        assert_eq!(resolve_duration("0.10", 0.1, true), Ok(0.1));
    }

    #[test]
    fn unchanged_actions_produce_no_patch() {
        let settings = multi_launcher::settings::Settings::default();
        let baseline = Values::from_settings(&settings);
        assert!(baseline.changes(&baseline).is_empty());
    }
}
