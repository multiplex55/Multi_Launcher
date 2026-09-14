use super::bindings::PreparationGeneration;
use super::context::InvocationContext;
use super::dynamic::FrozenBinding;
use super::model::{AfterActionPolicy, ConfigRevision, InvocationId, SessionId};
use super::session::DispatchToken;
use crate::commands::Command;
use crate::universal_actions::{UniversalAction, UniversalActionOperation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionRequirement {
    None,
    LauncherUi,
    ExternalInput,
    ExclusiveCapture,
}

pub fn interaction_requirement(action: &UniversalAction) -> InteractionRequirement {
    match &action.operation {
        UniversalActionOperation::UiIntent(intent) => ui_intent_requirement(intent),
        UniversalActionOperation::Command { command, .. } => command_requirement(command),
        UniversalActionOperation::InvokePrimary(action) => crate::commands::parse_action(action)
            .map(|command| command_requirement(&command))
            .unwrap_or(InteractionRequirement::ExternalInput),
    }
}

/// Static requirement knowledge shared by configuration validation and runtime
/// preparation. `None` means the concrete resolved operation is required.
pub(crate) fn action_id_requirement(
    action_id: &crate::universal_actions::ActionId,
) -> Option<InteractionRequirement> {
    use crate::universal_actions::action_ids as ids;
    if action_id == &ids::MKMACRO_RUN {
        Some(InteractionRequirement::ExclusiveCapture)
    } else if [
        &ids::CUSTOM_ACTION_EDIT,
        &ids::FOLDER_SET_ALIAS,
        &ids::BOOKMARK_SET_ALIAS,
        &ids::SNIPPET_EDIT,
        &ids::TEMPFILE_SET_ALIAS,
        &ids::NOTE_EDIT,
        &ids::CLIPBOARD_EDIT,
        &ids::TODO_EDIT,
        &ids::MKMACRO_EDIT,
    ]
    .contains(&action_id)
    {
        Some(InteractionRequirement::LauncherUi)
    } else if [
        &ids::NOTE_OPEN_NOTEPAD,
        &ids::NOTE_OPEN_NEOVIM,
        &ids::CLIPBOARD_COPY,
    ]
    .contains(&action_id)
    {
        Some(InteractionRequirement::ExternalInput)
    } else {
        None
    }
}

pub(crate) fn command_requirement(command: &Command) -> InteractionRequirement {
    match command {
        Command::Crop(_)
        | Command::ScreenDraw(_)
        | Command::Macro(_)
        | Command::Screenshot(_)
        | Command::MouseGesture(_) => InteractionRequirement::ExclusiveCapture,
        Command::Launcher(_)
        | Command::Query(_)
        | Command::Dialog(_)
        | Command::FileSearch(_)
        | Command::Diff(_) => InteractionRequirement::LauncherUi,
        Command::Clipboard(crate::commands::ClipboardCommand::SetText { .. })
        | Command::External(_) => InteractionRequirement::ExternalInput,
        _ => InteractionRequirement::None,
    }
}

fn ui_intent_requirement(
    intent: &crate::universal_actions::UniversalUiIntent,
) -> InteractionRequirement {
    use crate::universal_actions::UniversalUiIntent as I;
    match intent {
        I::EditCustomAction { .. }
        | I::OpenFolderAlias { .. }
        | I::OpenBookmarkAlias { .. }
        | I::EditSnippet { .. }
        | I::OpenTempfileAlias { .. }
        | I::EditNote { .. }
        | I::EditClipboardEntry { .. }
        | I::EditTodo { .. }
        | I::AddFavorite { .. }
        | I::OpenMkMacro { .. } => InteractionRequirement::LauncherUi,
        I::OpenNoteExternal { .. } => InteractionRequirement::ExternalInput,
        I::RemoveClipboardEntry { .. }
        | I::PinResult { .. }
        | I::UnpinResult { .. }
        | I::ReplacePin { .. }
        | I::RecomputePins
        | I::CopyStopwatchTime { .. } => InteractionRequirement::None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RadialDispatchIdentity {
    pub session_id: SessionId,
    pub invocation_id: InvocationId,
    pub session_generation: u64,
    pub token: DispatchToken,
    pub config_revision: ConfigRevision,
    pub preparation_generation: PreparationGeneration,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadialDispatchRequest {
    pub identity: RadialDispatchIdentity,
    pub binding: FrozenBinding,
    pub requirement: InteractionRequirement,
    pub history_query: String,
    pub context: InvocationContext,
    pub after_action: AfterActionPolicy,
    pub source: crate::commands::ActivationSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchPhase {
    Selected,
    AwaitingClose,
    AwaitingRelease,
    Ready,
    Dispatched,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DispatchEvent {
    Begin,
    Closed {
        session_id: SessionId,
        reason: super::native::CloseReason,
    },
    InvocationReleased {
        invocation_id: InvocationId,
    },
    Tick {
        now_ms: u64,
    },
    Cancel,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DispatchIntent {
    CloseRadial { session_id: SessionId },
    AwaitInvocationRelease { invocation_id: InvocationId },
    Dispatch(RadialDispatchRequest),
    Cancelled { reason: &'static str },
}

pub struct PendingRadialDispatch {
    request: RadialDispatchRequest,
    phase: DispatchPhase,
    close_required: bool,
    release_required: bool,
    close_acknowledged: bool,
    release_acknowledged: bool,
    deadline_ms: u64,
}

impl PendingRadialDispatch {
    pub fn new(
        request: RadialDispatchRequest,
        close_required: bool,
        release_required: bool,
        now_ms: u64,
        timeout_ms: u64,
    ) -> Result<Self, String> {
        let deadline_ms = now_ms
            .checked_add(timeout_ms)
            .ok_or_else(|| "radial dispatch timeout overflow".to_string())?;
        Ok(Self {
            request,
            phase: DispatchPhase::Selected,
            close_required,
            release_required,
            close_acknowledged: !close_required,
            release_acknowledged: !release_required,
            deadline_ms,
        })
    }

    pub fn phase(&self) -> DispatchPhase {
        self.phase
    }

    pub fn identity(&self) -> &RadialDispatchIdentity {
        &self.request.identity
    }

    pub fn reduce(&mut self, event: DispatchEvent) -> Vec<DispatchIntent> {
        if matches!(
            self.phase,
            DispatchPhase::Dispatched | DispatchPhase::Cancelled
        ) {
            return Vec::new();
        }
        match event {
            DispatchEvent::Cancel => {
                self.phase = DispatchPhase::Cancelled;
                return vec![DispatchIntent::Cancelled {
                    reason: "cancelled",
                }];
            }
            DispatchEvent::Tick { now_ms } if now_ms >= self.deadline_ms => {
                self.phase = DispatchPhase::Cancelled;
                return vec![DispatchIntent::Cancelled {
                    reason: "timed out",
                }];
            }
            DispatchEvent::Closed { session_id, reason }
                if session_id == self.request.identity.session_id
                    && reason == super::native::CloseReason::ActionHandoff =>
            {
                self.close_acknowledged = true;
            }
            DispatchEvent::InvocationReleased { invocation_id }
                if invocation_id == self.request.identity.invocation_id =>
            {
                self.release_acknowledged = true;
            }
            DispatchEvent::Begin
            | DispatchEvent::Tick { .. }
            | DispatchEvent::Closed { .. }
            | DispatchEvent::InvocationReleased { .. } => {}
        }
        if !self.close_acknowledged {
            if self.phase == DispatchPhase::Selected {
                self.phase = DispatchPhase::AwaitingClose;
                return vec![DispatchIntent::CloseRadial {
                    session_id: self.request.identity.session_id.clone(),
                }];
            }
            return Vec::new();
        }
        if !self.release_acknowledged {
            if self.phase != DispatchPhase::AwaitingRelease {
                self.phase = DispatchPhase::AwaitingRelease;
                return vec![DispatchIntent::AwaitInvocationRelease {
                    invocation_id: self.request.identity.invocation_id,
                }];
            }
            return Vec::new();
        }
        self.phase = DispatchPhase::Ready;
        let request = self.request.clone();
        self.phase = DispatchPhase::Dispatched;
        vec![DispatchIntent::Dispatch(request)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::model::ActionBinding;
    use crate::universal_actions::{ActionId, PersistedUniversalActionRef};

    #[test]
    fn static_action_requirements_include_external_note_editors() {
        assert_eq!(
            action_id_requirement(&crate::universal_actions::action_ids::NOTE_OPEN_NOTEPAD),
            Some(InteractionRequirement::ExternalInput)
        );
        assert_eq!(
            action_id_requirement(&crate::universal_actions::action_ids::NOTE_OPEN_NEOVIM),
            Some(InteractionRequirement::ExternalInput)
        );
        assert_eq!(
            action_id_requirement(&crate::universal_actions::action_ids::MKMACRO_RUN),
            Some(InteractionRequirement::ExclusiveCapture)
        );
    }

    fn request(generation: u64) -> RadialDispatchRequest {
        RadialDispatchRequest {
            identity: RadialDispatchIdentity {
                session_id: SessionId::new("s1"),
                invocation_id: InvocationId(2),
                session_generation: generation,
                token: DispatchToken {
                    session_generation: generation,
                    ordinal: 1,
                },
                config_revision: ConfigRevision(1),
                preparation_generation: PreparationGeneration(3),
            },
            binding: FrozenBinding::Stable(ActionBinding::Persisted {
                action: PersistedUniversalActionRef {
                    target: None,
                    action_id: ActionId::new("test"),
                },
            }),
            requirement: InteractionRequirement::ExclusiveCapture,
            history_query: String::new(),
            context: InvocationContext::empty(5),
            after_action: AfterActionPolicy::CloseTree,
            source: crate::commands::ActivationSource::Click,
        }
    }

    #[test]
    fn exact_close_then_release_dispatches_once() {
        let mut pending = PendingRadialDispatch::new(request(4), true, true, 10, 100).unwrap();
        assert!(matches!(
            pending.reduce(DispatchEvent::Begin).as_slice(),
            [DispatchIntent::CloseRadial { .. }]
        ));
        assert!(
            pending
                .reduce(DispatchEvent::Closed {
                    session_id: SessionId::new("stale"),
                    reason: super::super::native::CloseReason::ActionHandoff,
                })
                .is_empty()
        );
        assert!(matches!(
            pending
                .reduce(DispatchEvent::Closed {
                    session_id: SessionId::new("s1"),
                    reason: super::super::native::CloseReason::ActionHandoff,
                })
                .as_slice(),
            [DispatchIntent::AwaitInvocationRelease { .. }]
        ));
        assert!(
            pending
                .reduce(DispatchEvent::InvocationReleased {
                    invocation_id: InvocationId(99)
                })
                .is_empty()
        );
        assert!(matches!(
            pending
                .reduce(DispatchEvent::InvocationReleased {
                    invocation_id: InvocationId(2)
                })
                .as_slice(),
            [DispatchIntent::Dispatch(_)]
        ));
        assert!(pending.reduce(DispatchEvent::Begin).is_empty());
    }

    #[test]
    fn dismissal_close_cannot_unlock_action_handoff() {
        let mut pending = PendingRadialDispatch::new(request(4), true, false, 10, 100).unwrap();
        pending.reduce(DispatchEvent::Begin);
        assert!(
            pending
                .reduce(DispatchEvent::Closed {
                    session_id: SessionId::new("s1"),
                    reason: super::super::native::CloseReason::Dismissed,
                })
                .is_empty()
        );
        assert_eq!(pending.phase(), DispatchPhase::AwaitingClose);
    }

    #[test]
    fn cancel_timeout_and_overflow_are_fail_closed() {
        let mut cancelled = PendingRadialDispatch::new(request(1), false, false, 0, 10).unwrap();
        assert!(matches!(
            cancelled.reduce(DispatchEvent::Cancel).as_slice(),
            [DispatchIntent::Cancelled { .. }]
        ));
        assert!(cancelled.reduce(DispatchEvent::Begin).is_empty());
        let mut timed = PendingRadialDispatch::new(request(2), true, true, 5, 5).unwrap();
        assert!(matches!(
            timed.reduce(DispatchEvent::Tick { now_ms: 10 }).as_slice(),
            [DispatchIntent::Cancelled {
                reason: "timed out"
            }]
        ));
        assert!(PendingRadialDispatch::new(request(3), false, false, u64::MAX, 1).is_err());
    }
}
