//! Retained Omni Search category preferences.
use super::*;
use multi_launcher::plugins::omni_search::OmniSearchSettings;
use multi_launcher::settings::{OmniPreferencesSnapshot, SettingsBooleanSaveToken};

fn acknowledgement_matches(
    expected: &OmniSearchSettings,
    actual: &OmniPreferencesSnapshot,
) -> bool {
    &actual.values == expected
}

const RECONCILE: &str = "Omni Search save could not be verified. Cancel Settings, close Multi Launcher, then manually relaunch from the canonical Settings file.";

#[derive(Debug, PartialEq, Eq)]
enum SaveFailure {
    Ordinary(String),
    CommittedUnverified,
}

fn classify_save_error(error: anyhow::Error) -> SaveFailure {
    if error
        .downcast_ref::<multi_launcher::settings::OmniCommittedUnverified>()
        .is_some()
    {
        SaveFailure::CommittedUnverified
    } else {
        SaveFailure::Ordinary(error.to_string())
    }
}
fn clear_omni_wait(status: &mut Option<String>) {
    if status.as_deref() == Some(WAIT) {
        *status = None;
    }
}

pub(super) const WAIT: &str = "Wait for Omni Search settings to finish.";
pub(super) struct SettingsOmni {
    root: WeakEntity<RootWindow>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    baseline: Option<OmniPreferencesSnapshot>,
    draft: Option<OmniSearchSettings>,
    reading: bool,
    reconciliation: bool,
    pending: Option<SettingsBooleanSaveToken>,
    status: Option<String>,
    focus: Vec<FocusHandle>,
}
impl SettingsOmni {
    pub fn new(root: WeakEntity<RootWindow>, reconciliation: bool, cx: &mut Context<Self>) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        Self {
            root,
            owner,
            mount,
            baseline: None,
            draft: None,
            reading: false,
            reconciliation,
            pending: None,
            status: None,
            focus: (0..6).map(|_| cx.focus_handle()).collect(),
        }
    }
    pub fn busy(&self) -> bool {
        self.reading || self.pending.is_some()
    }
    fn input_enabled(&self) -> bool {
        !self.busy() && !self.reconciliation
    }
    pub fn dirty(&self) -> bool {
        self.baseline
            .as_ref()
            .zip(self.draft.as_ref())
            .is_some_and(|(base, draft)| &base.values != draft)
    }
    pub fn restart_draft(&self) -> Option<(OmniSearchSettings, OmniSearchSettings)> {
        self.baseline
            .as_ref()
            .zip(self.draft.as_ref())
            .map(|(baseline, draft)| (baseline.values.clone(), draft.clone()))
    }
    pub fn retire(&mut self) {
        self.owner.retire();
    }
    pub fn can_focus(&self) -> bool {
        self.mount.is_current() && self.input_enabled() && self.draft.is_some()
    }
    pub fn focus_control(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.can_focus() {
            return false;
        }
        self.focus[0].focus(window, cx);
        true
    }
    fn clear_wait(&self, cx: &mut Context<Self>) {
        let _ = self.root.update(cx, |root, cx| {
            clear_omni_wait(&mut root.restart_status);
            clear_omni_wait(&mut root.settings_editor.status);
            cx.notify();
        });
    }
    pub fn read(
        &mut self,
        path: String,
        workers: SettingsSaveWorkerRegistry,
        permit: EffectPermit,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = match workers.begin_omni_preferences_read(path.clone()) {
            Ok(receiver) => receiver,
            Err(error) => {
                self.status = Some(error);
                return;
            }
        };
        self.reading = true;
        let mount = self.mount.clone();
        let host_id = _window.window_handle().window_id();
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = loop {
                match receiver.try_recv() {
                    Ok(result) => break result,
                    Err(TryRecvError::Disconnected) => {
                        break Err("Omni Settings reader ended without a receipt.".into());
                    }
                    Err(TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(20))
                            .await
                    }
                }
            };
            let _ = weak.update(cx, |view, cx| {
                if !mount.is_current() || !mount.matches(&view.mount) || !view.reading {
                    return;
                }
                let valid = view.root.upgrade().is_some_and(|root| {
                    let root = root.read(cx);
                    root.settings_editor.open
                        && root
                            .root_window
                            .is_some_and(|host| host.window_id() == host_id)
                        && root.settings_path == path
                        && root
                            .settings_omni
                            .as_ref()
                            .is_some_and(|current| current.entity_id() == cx.entity_id())
                });
                if !valid {
                    return;
                }
                view.reading = false;
                match result {
                    Ok(snapshot) => {
                        view.draft = Some(snapshot.values.clone());
                        view.baseline = Some(snapshot);
                        view.status = view.reconciliation.then(|| RECONCILE.to_owned());
                    }
                    Err(error) => {
                        view.status = Some(format!("Could not read Omni Search settings: {error}"))
                    }
                }
                view.clear_wait(cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy() || self.reconciliation || !self.mount.is_current() {
            return;
        }
        let (Some(baseline), Some(desired)) = (self.baseline.clone(), self.draft.clone()) else {
            return;
        };
        if baseline.values == desired {
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
            || !state.external_owner_is_current(window.focused(cx).as_ref(), window, cx)
            || state
                .native_window
                .is_none_or(|native| current_foreground_identity() != Some(native.identity))
            || state
                .settings_omni
                .as_ref()
                .is_none_or(|view| view.entity_id() != cx.entity_id())
        {
            return;
        }
        let Some(permit) = state.restart_admission.admit() else {
            return;
        };
        let workers = state.settings_save_workers.clone();
        let path = state.settings_path.clone();
        let (receiver, token) =
            match workers.begin_omni_preferences_save(path.clone(), baseline, desired.clone()) {
                Ok(value) => value,
                Err(error) => {
                    self.status = Some(error);
                    cx.notify();
                    return;
                }
            };
        self.pending = Some(token.clone());
        self.status = None;
        let mount = self.mount.clone();
        let host_id = window.window_handle().window_id();
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = loop {
                match receiver.try_recv() {
                    Ok(result) => break result.map_err(classify_save_error),
                    Err(TryRecvError::Disconnected) => {
                        break Err(SaveFailure::CommittedUnverified);
                    }
                    Err(TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(20))
                            .await
                    }
                }
            };
            let _ = weak.update(cx, |view, cx| {
                if !mount.is_current()
                    || !mount.matches(&view.mount)
                    || view
                        .pending
                        .as_ref()
                        .is_none_or(|pending| pending.id() != token.id())
                {
                    return;
                }
                view.pending = None;
                let root = view.root.upgrade();
                let valid = root.as_ref().is_some_and(|root| {
                    let state = root.read(cx);
                    state.settings_editor.open
                        && !state.restart_admission.is_fenced()
                        && state
                            .root_window
                            .is_some_and(|host| host.window_id() == host_id)
                        && state
                            .enabled_plugins
                            .as_ref()
                            .is_none_or(|enabled| enabled.contains("omni_search"))
                        && state.settings_path == path
                        && state
                            .settings_omni
                            .as_ref()
                            .is_some_and(|current| current.entity_id() == cx.entity_id())
                });
                if valid {
                    workers.acknowledge_boolean_preference_save(token.id());
                    match result {
                        Ok(ack) if acknowledgement_matches(&desired, &ack.snapshot) => {
                            view.baseline = Some(ack.snapshot);
                            view.draft = Some(desired.clone());
                            view.status = Some("Omni Search settings saved.".into());
                            if let Some(root) = root {
                                root.update(cx, |root, cx| root.apply_omni_categories(desired, cx));
                            }
                        }
                        Ok(_) | Err(SaveFailure::CommittedUnverified) => {
                            view.reconciliation = true;
                            view.status = Some(RECONCILE.into());
                            if let Some(root) = root {
                                root.update(cx, |root, _| root.omni_reconciliation = true);
                            }
                        }
                        Err(SaveFailure::Ordinary(error)) => {
                            view.status =
                                Some(format!("Could not save Omni Search settings: {error}"));
                        }
                    }
                }
                if valid {
                    view.clear_wait(cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for SettingsOmni {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::current(cx).palette;
        let mut panel = div()
            .id("settings-omni")
            .flex()
            .flex_col()
            .gap_2()
            .child(Label::new("Omni Search"));
        if let Some(values) = self.draft.as_ref() {
            let checked = [
                values.include_apps,
                values.include_notes,
                values.include_todos,
                values.include_calendar,
                values.include_folders,
                values.include_bookmarks,
            ];
            for (index, label) in [
                "Search apps",
                "Search notes",
                "Search todos",
                "Search calendar events",
                "Search folders",
                "Search bookmarks",
            ]
            .into_iter()
            .enumerate()
            {
                let mount = self.mount.clone();
                let weak = cx.entity().downgrade();
                let focused = self.focus[index].is_focused(window);
                panel = panel.child(
                    Checkbox::new(SharedString::from(format!("omni-category-{index}")))
                        .checked(checked[index])
                        .disabled(!self.input_enabled())
                        .accessibility_label(label)
                        .track_focus(&self.focus[index])
                        .on_change(move |state, _, _, app| {
                            let _ = weak.update(app, |view, cx| {
                                if !mount.is_current()
                                    || !mount.matches(&view.mount)
                                    || !view.input_enabled()
                                {
                                    return;
                                }
                                if let Some(values) = &mut view.draft {
                                    let value = state == CheckboxState::Checked;
                                    match index {
                                        0 => values.include_apps = value,
                                        1 => values.include_notes = value,
                                        2 => values.include_todos = value,
                                        3 => values.include_calendar = value,
                                        4 => values.include_folders = value,
                                        _ => values.include_bookmarks = value,
                                    }
                                }
                                cx.notify();
                            });
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    CheckboxIndicator::new()
                                        .checked(checked[index])
                                        .size(px(18.0))
                                        .rounded(px(3.0))
                                        .border_1()
                                        .border_color(appearance::to_hsla(if focused {
                                            palette.focus
                                        } else {
                                            palette.control_border
                                        }))
                                        .when(checked[index], |indicator| {
                                            indicator
                                                .bg(appearance::to_hsla(palette.accent))
                                                .text_color(appearance::to_hsla(
                                                    palette.primary_text,
                                                ))
                                        })
                                        .items_center()
                                        .justify_center()
                                        .child(if checked[index] { "✓" } else { "" }),
                                )
                                .child(Label::new(label)),
                        ),
                );
            }
            panel = panel.child(
                Button::new("settings-omni-save")
                    .label("Save Omni Search")
                    .disabled(!self.input_enabled())
                    .on_click(cx.listener(|view, _, window, cx| view.save(window, cx))),
            );
        }
        if let Some(status) = &self.status {
            panel = panel.child(Label::new(status.clone()));
        }
        panel
    }
}

impl RootWindow {
    pub(super) fn open_omni_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_omni.is_some()
            || self
                .enabled_plugins
                .as_ref()
                .is_some_and(|enabled| !enabled.contains("omni_search"))
        {
            return;
        }
        let Some(permit) = self.restart_admission.admit() else {
            return;
        };
        let root = cx.entity().downgrade();
        let reconciliation = self.omni_reconciliation;
        let view = cx.new(|cx| SettingsOmni::new(root, reconciliation, cx));
        self.settings_omni = Some(view.clone());
        let path = self.settings_path.clone();
        let workers = self.settings_save_workers.clone();
        view.update(cx, |view, cx| view.read(path, workers, permit, window, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::{SaveFailure, WAIT, acknowledgement_matches, classify_save_error, clear_omni_wait};
    use multi_launcher::settings::Settings;
    #[test]
    fn terminal_error_classification_and_wait_clear_are_scoped() {
        let committed = anyhow::Error::new(multi_launcher::settings::OmniCommittedUnverified(
            "readback".into(),
        ));
        assert_eq!(
            classify_save_error(committed),
            SaveFailure::CommittedUnverified
        );
        assert_eq!(
            classify_save_error(anyhow::anyhow!("conflict")),
            SaveFailure::Ordinary("conflict".into())
        );
        let mut exact = Some(WAIT.to_owned());
        clear_omni_wait(&mut exact);
        assert!(exact.is_none());
        let mut unrelated = Some("other warning".to_owned());
        clear_omni_wait(&mut unrelated);
        assert_eq!(unrelated.as_deref(), Some("other warning"));
    }

    #[test]
    fn committed_receipt_matches_all_six_and_cancel_leaves_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let baseline = Settings::omni_preferences_snapshot(&path).unwrap();
        let mut draft = baseline.values.clone();
        draft.include_notes = false;
        assert!(!acknowledgement_matches(&draft, &baseline));
        assert!(!path.exists());
        let ack = Settings::save_omni_preferences_checked(&path, &baseline, &draft).unwrap();
        assert!(acknowledgement_matches(&draft, &ack.snapshot));
        draft.include_calendar = false;
        assert!(!acknowledgement_matches(&draft, &ack.snapshot));
    }
}
