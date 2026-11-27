//! Settled-turn GPUI adapter for the stock typed hotkey transaction.
use super::*;
use multi_launcher::hotkey::reconfiguration::{
    AppliedHotkeyReconfiguration, HotkeyReconfigurationError as Error, HotkeyReconfigurationId,
    HotkeyRuntimeGeneration, HotkeyRuntimeMessage, PreparedHotkeyReconfiguration,
};
use multi_launcher::settings::Settings;

fn exact_launcher_leaf(
    prepared: &Option<String>,
    requested: &Option<String>,
    current: &Option<String>,
) -> bool {
    prepared == requested && prepared == current
}

fn launcher_only_candidate(current: &Settings, candidate: &Settings) -> bool {
    current.quit_hotkey == candidate.quit_hotkey
        && current.help_hotkey == candidate.help_hotkey
        && current.recording_stop_hotkey == candidate.recording_stop_hotkey
        && current.paste_sequence.next_hotkey == candidate.paste_sequence.next_hotkey
        && current.paste_sequence.cancel_hotkey == candidate.paste_sequence.cancel_hotkey
        && current
            .enabled_plugins
            .as_ref()
            .is_none_or(|plugins| plugins.contains("paste_sequence"))
            == candidate
                .enabled_plugins
                .as_ref()
                .is_none_or(|plugins| plugins.contains("paste_sequence"))
        && current.plugin_settings.get("screen_draw")
            == candidate.plugin_settings.get("screen_draw")
        && current.experimental_jump_mode_enabled == candidate.experimental_jump_mode_enabled
        && current.jump_mode_activation_chord == candidate.jump_mode_activation_chord
        && serde_json::to_value(&current.radial).ok()
            == serde_json::to_value(&candidate.radial).ok()
}

struct Prepared {
    id: HotkeyReconfigurationId,
    generation: u64,
    base_revision: u64,
    fingerprint: RouteDependencies,
    launcher: Hotkey,
    hotkey_value: Option<String>,
    candidate: Option<invocation::Installed>,
    fallback: Option<invocation::Installed>,
}

#[derive(Clone, Debug, PartialEq)]
struct RouteDependencies {
    stock: crate::HotkeyRouteDependencies,
    jump_enabled: bool,
    jump: JumpModeChord,
}

pub(super) struct Runtime {
    owner: Arc<invocation::Owner>,
    root: multi_launcher::platform::app_data::AppDataRoot,
    settings_path: String,
    visibility: RootVisibility,
    native: NativeWindow,
    admission: Arc<RestartAdmission>,
    wake: Sender<()>,
    prepared: Option<Prepared>,
    applied: Option<HotkeyReconfigurationId>,
    deferred: Option<HotkeyRuntimeMessage>,
}

/// Survives the GPUI task and owns the receiver until Settings workers settle.
pub(super) struct Coordinator {
    receiver: multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationReceiver,
    runtime: Runtime,
    pending: Option<HotkeyRuntimeMessage>,
    claimed: Option<HotkeyRuntimeMessage>,
    queued: std::collections::VecDeque<HotkeyRuntimeMessage>,
}

impl Coordinator {
    pub fn new(
        receiver: multi_launcher::hotkey::reconfiguration::HotkeyReconfigurationReceiver,
        runtime: Runtime,
    ) -> Self {
        Self {
            receiver,
            runtime,
            pending: None,
            claimed: None,
            queued: std::collections::VecDeque::new(),
        }
    }
    pub fn next(&mut self) -> Option<HotkeyRuntimeMessage> {
        if let Some(message) = self.claimed.take() {
            if self.pending.is_none() {
                self.pending = Some(message);
            } else {
                self.queued.push_front(message);
            }
        }
        if let Some(HotkeyRuntimeMessage::Apply(application)) = self.pending.as_ref() {
            let id = application.request_id();
            if let Some(index) = self.queued.iter().position(|message| matches!(message, HotkeyRuntimeMessage::Cancel(cancellation) if cancellation.request_id() == id)) {
                return self.queued.remove(index);
            }
            while let Ok(Some(message)) = self.receiver.try_recv() {
                match message {
                    HotkeyRuntimeMessage::Cancel(cancellation)
                        if cancellation.request_id() == id =>
                    {
                        return Some(HotkeyRuntimeMessage::Cancel(cancellation));
                    }
                    other => self.queued.push_back(other),
                }
            }
        }
        self.pending
            .take()
            .or_else(|| self.queued.pop_front())
            .or_else(|| self.receiver.try_recv().ok().flatten())
    }
    pub fn stage_next(&mut self) -> bool {
        if self.claimed.is_none() {
            self.claimed = self.next();
        }
        self.claimed.is_some()
    }
    pub fn unclaim(&mut self) {
        if let Some(message) = self.claimed.take() {
            self.defer(message);
        }
    }
    pub fn handle_claimed(&mut self) {
        if let Some(message) = self.claimed.take() {
            self.handle(message);
        }
    }
    pub fn defer(&mut self, message: HotkeyRuntimeMessage) {
        if self.pending.is_some() {
            self.queued.push_front(message);
        } else {
            self.pending = Some(message);
        }
    }
    pub fn handle(&mut self, message: HotkeyRuntimeMessage) {
        if let HotkeyRuntimeMessage::Cancel(mut cancellation) = message {
            let id = cancellation.request_id();
            if self.pending.as_ref().is_some_and(|message| matches!(message, HotkeyRuntimeMessage::Apply(application) if application.request_id() == id)) {
                cancellation.defer_terminal();
                if let Some(HotkeyRuntimeMessage::Apply(application)) = self.pending.take() {
                    if self.runtime.prepared.as_ref().is_some_and(|prepared| prepared.id == id) { self.runtime.prepared = None; }
                    let _ = application.complete_restored(Error::TransactionCancelled);
                }
                return;
            }
            self.runtime
                .handle(HotkeyRuntimeMessage::Cancel(cancellation));
            return;
        }
        self.runtime.handle(message);
        if let Some(message) = self.runtime.take_deferred() {
            self.pending = Some(message);
        }
    }
    pub fn wait_and_drain(&mut self, timeout: Duration) {
        let message = self
            .next()
            .or_else(|| self.receiver.recv_timeout(timeout).ok().flatten());
        if let Some(message) = message {
            self.handle(message);
        }
    }
}

impl Runtime {
    pub fn new(
        owner: Arc<invocation::Owner>,
        root: multi_launcher::platform::app_data::AppDataRoot,
        settings_path: String,
        visibility: RootVisibility,
        native: NativeWindow,
        admission: Arc<RestartAdmission>,
        wake: Sender<()>,
    ) -> Self {
        Self {
            owner,
            root,
            settings_path,
            visibility,
            native,
            admission,
            wake,
            prepared: None,
            applied: None,
            deferred: None,
        }
    }

    pub fn take_deferred(&mut self) -> Option<HotkeyRuntimeMessage> {
        self.deferred.take()
    }

    fn listener(
        &self,
        launcher: Hotkey,
        help: Option<Hotkey>,
    ) -> Result<invocation::Installed, Error> {
        let launcher = Arc::new(HotkeyTrigger::new(launcher));
        let visibility = self.visibility.clone();
        let native = self.native;
        let admission = self.admission.clone();
        let quick_help = help.map(|hotkey| {
            Arc::new(HotkeyTrigger::new_scoped(hotkey, move || {
                if admission.is_fenced() {
                    None
                } else {
                    visibility.current_visible_revision(native)
                }
            }))
        });
        let mut triggers = vec![launcher.clone()];
        triggers.extend(quick_help.iter().cloned());
        let listener =
            HotkeyTrigger::prepare_listener(triggers, "gpui-prepared", self.wake.clone())
                .map_err(Error::ListenerPreparation)?;
        Ok(invocation::Installed {
            revision: 0,
            listener,
            launcher,
            quick_help,
        })
    }

    fn reservations(&self, settings: &Settings) -> Result<(Hotkey, RouteDependencies), Error> {
        use multi_launcher::radial::store::{CandidateState, RadialStore, StoreError};
        let store =
            RadialStore::new(&self.root).map_err(|error| Error::Validation(error.to_string()))?;
        let radial = if settings.radial.enabled {
            match store.reload() {
                Ok(document) => document,
                Err(StoreError::Candidate(CandidateState::Missing | CandidateState::Empty)) => {
                    store
                        .snapshot()
                        .map_err(|error| Error::Validation(error.to_string()))?
                }
                Err(error) => return Err(Error::Validation(error.to_string())),
            }
        } else {
            store
                .snapshot()
                .map_err(|error| Error::Validation(error.to_string()))?
        };
        let macros = multi_launcher::mkmacro::read_hotkey_reservation_document(self.root.path())
            .map_err(|error| Error::Validation(error.to_string()))?;
        let bindings = if settings
            .enabled_plugins
            .as_ref()
            .is_some_and(|plugins| !plugins.contains("paste_sequence"))
        {
            Vec::new()
        } else {
            multi_launcher::paste_sequence::SequenceStore::new(
                self.root
                    .path()
                    .join(multi_launcher::paste_sequence::PASTE_SEQUENCES_FILE),
            )
            .load()
            .map_err(|error| Error::Validation(error.to_string()))?
            .hotkey_bindings()
        };
        let (routes, sequences, _) = crate::resolve_hotkey_route_reservations(
            settings,
            &radial,
            macros.as_ref(),
            Some(&bindings),
        )?;
        if legacy_virtual_key(routes.launcher.key).is_none() {
            return Err(Error::Validation(
                "launcher key is unsupported by the GPUI polling listener".into(),
            ));
        }
        let fingerprint = RouteDependencies {
            stock: crate::HotkeyRouteDependencies::from_routes(settings, &routes, &sequences),
            jump_enabled: settings.experimental_jump_mode_enabled,
            jump: settings.jump_mode_activation_chord,
        };
        Ok((routes.launcher, fingerprint))
    }

    pub fn handle(&mut self, message: HotkeyRuntimeMessage) {
        match message {
            HotkeyRuntimeMessage::Prepare(request) => {
                let id = request.request_id();
                let result = (|| {
                    if self.admission.is_fenced() || self.prepared.is_some() {
                        return Err(Error::InvalidPhase);
                    }
                    if request.sequence_hotkey_bindings().is_some() {
                        return Err(Error::Validation(
                            "only the launcher shortcut can be changed".into(),
                        ));
                    }
                    let active = self
                        .owner
                        .active_routes()
                        .ok_or(Error::RuntimeUnavailable)?;
                    let current = Settings::read_current(&self.settings_path)
                        .map_err(|error| Error::Validation(error.to_string()))?;
                    let proposed = request.settings().clone();
                    if !launcher_only_candidate(&current, &proposed) {
                        return Err(Error::Validation(
                            "only the launcher shortcut can be changed".into(),
                        ));
                    }
                    if current.hotkey() != active.1 {
                        return Err(Error::StaleGeneration);
                    }
                    let (launcher, fingerprint) = self.reservations(&proposed)?;
                    let proposed_help = admit_quick_help_shortcut(
                        proposed.help_hotkey.as_deref(),
                        launcher,
                        proposed.experimental_jump_mode_enabled,
                        proposed.jump_mode_activation_chord,
                    )
                    .shortcut
                    .map(|shortcut| shortcut.hotkey);
                    if active.2.is_some() && proposed_help != active.2 {
                        return Err(Error::Validation(
                            "launcher shortcut conflicts with the current Quick Help route".into(),
                        ));
                    }
                    let generation = active.0.checked_add(1).ok_or_else(|| {
                        Error::RuntimeTransition("hotkey generation exhausted".into())
                    })?;
                    let candidate = self.listener(launcher, active.2)?;
                    let fallback = self.listener(active.1, active.2)?;
                    Ok(Prepared {
                        id,
                        generation,
                        base_revision: active.0,
                        fingerprint,
                        launcher,
                        hotkey_value: proposed.hotkey.clone(),
                        candidate: Some(candidate),
                        fallback: Some(fallback),
                    })
                })();
                match result {
                    Ok(prepared) => {
                        let acknowledgement = PreparedHotkeyReconfiguration {
                            request_id: id,
                            generation: HotkeyRuntimeGeneration(prepared.generation),
                            shared_invocation: false,
                        };
                        self.prepared = Some(prepared);
                        if request.complete(Ok(acknowledgement)).is_err() {
                            self.prepared = None;
                        }
                    }
                    Err(error) => {
                        let _ = request.complete(Err(error));
                    }
                }
            }
            HotkeyRuntimeMessage::Apply(request) => {
                let Some(mut prepared) = self.prepared.take() else {
                    let _ = request.complete_restored(Error::StaleGeneration);
                    return;
                };
                if request.request_id() != prepared.id
                    || request.generation().0 != prepared.generation
                {
                    self.prepared = Some(prepared);
                    let _ = request.complete_restored(Error::StaleGeneration);
                    return;
                }
                let mut owner_error: Option<(bool, String)> = None;
                let claim = Settings::with_current(&self.settings_path, |current, recheck| {
                    if self.admission.is_fenced() {
                        return Err("hotkey transaction cancelled".into());
                    }
                    if !exact_launcher_leaf(
                        &prepared.hotkey_value,
                        &request.settings().hotkey,
                        &current.hotkey,
                    ) || current.hotkey() != prepared.launcher
                        || !launcher_only_candidate(current, request.settings())
                    {
                        return Err("hotkey settings changed before Apply".into());
                    }
                    let (launcher, fingerprint) = self
                        .reservations(current)
                        .map_err(|error| error.to_string())?;
                    if launcher != prepared.launcher || fingerprint != prepared.fingerprint {
                        return Err("hotkey reservations changed before Apply".into());
                    }
                    let latest = recheck()?;
                    if !exact_launcher_leaf(
                        &prepared.hotkey_value,
                        &request.settings().hotkey,
                        &latest.hotkey,
                    ) || latest.hotkey() != prepared.launcher
                        || !launcher_only_candidate(&latest, current)
                    {
                        return Err("hotkey settings changed before replacement".into());
                    }
                    match self.owner.replace(
                        prepared.base_revision,
                        &mut prepared.candidate,
                        &mut prepared.fallback,
                        |claim| {
                            self.admission
                                .claim_if_unfenced(|| {
                                    self.visibility.claim_without_visibility_request(claim)
                                })
                                .flatten()
                        },
                    ) {
                        Ok(result) => Ok(result),
                        Err(error) => {
                            owner_error = Some(error);
                            Err("hotkey listener transition failed".into())
                        }
                    }
                });
                match claim {
                    Ok(None) => {
                        self.prepared = Some(prepared);
                        self.deferred = Some(HotkeyRuntimeMessage::Apply(request));
                    }
                    Ok(Some(generation)) if generation == prepared.generation => {
                        self.applied = Some(prepared.id);
                        let _ = request.complete_applied(AppliedHotkeyReconfiguration {
                            request_id: prepared.id,
                            generation: HotkeyRuntimeGeneration(generation),
                        });
                    }
                    Ok(Some(_)) => {
                        let _ = request.complete_restoration_unavailable(Error::RuntimeTransition(
                            "unexpected active generation".into(),
                        ));
                    }
                    Err(error) => match owner_error {
                        Some((false, detail)) => {
                            let _ = request
                                .complete_restoration_unavailable(Error::RuntimeTransition(detail));
                        }
                        Some((true, detail)) => {
                            let _ = request.complete_restored(Error::RuntimeTransition(detail));
                        }
                        None => {
                            let _ = request.complete_restored(Error::RuntimeTransition(error));
                        }
                    },
                }
            }
            HotkeyRuntimeMessage::Cancel(mut request) => {
                if self.applied == Some(request.request_id()) {
                    request.defer_terminal();
                } else {
                    if self
                        .prepared
                        .as_ref()
                        .is_some_and(|prepared| prepared.id == request.request_id())
                    {
                        self.prepared = None;
                    }
                    let _ = request.complete_restored(Error::TransactionCancelled);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{exact_launcher_leaf, launcher_only_candidate};
    use multi_launcher::settings::Settings;

    #[test]
    fn launcher_candidate_rejects_other_route_changes() {
        let mut baseline = Settings::default();
        baseline.paste_sequence.next_hotkey = None;
        baseline.paste_sequence.cancel_hotkey = None;
        let mut candidate = baseline.clone();
        candidate.hotkey = Some("Ctrl+F8".into());
        assert!(launcher_only_candidate(&baseline, &candidate));
        candidate.clear_query_after_run = !baseline.clear_query_after_run;
        assert!(launcher_only_candidate(&baseline, &candidate));
        candidate.help_hotkey = Some("F9".into());
        assert!(!launcher_only_candidate(&baseline, &candidate));
    }

    #[test]
    fn apply_requires_exact_prepared_launcher_scalar() {
        let prepared = Some("F2".to_owned());
        assert!(exact_launcher_leaf(&prepared, &prepared, &prepared));
        assert!(!exact_launcher_leaf(
            &prepared,
            &Some("f2".into()),
            &prepared
        ));
        assert!(!exact_launcher_leaf(&prepared, &prepared, &None));
    }
}
