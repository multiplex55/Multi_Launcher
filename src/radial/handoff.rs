use super::bindings::PreparationGeneration;
use super::context::InvocationContext;
use super::dynamic::FrozenBinding;
use super::model::{AfterActionPolicy, ConfigRevision, InvocationId, QueryRunMode, SessionId};
use super::session::DispatchToken;
use crate::actions::Action;
use crate::commands::Command;
use crate::universal_actions::{UniversalAction, UniversalActionOperation};
use std::sync::mpsc;
use std::sync::{Arc, atomic::AtomicBool};

/// App-owned capacity for blocking provider searches. Cancellation invalidates
/// a result, but does not release the capacity until the provider call exits.
#[derive(Clone, Default)]
pub(crate) struct DeferredProviderSearchCapacity(Arc<AtomicBool>);

impl DeferredProviderSearchCapacity {
    pub(crate) fn is_occupied(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Acquire)
    }

    pub(crate) fn try_acquire(&self) -> Option<DeferredProviderSearchPermit> {
        self.0
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .ok()
            .map(|_| DeferredProviderSearchPermit(Arc::clone(&self.0)))
    }
}

pub(crate) struct DeferredProviderSearchPermit(Arc<AtomicBool>);

impl Drop for DeferredProviderSearchPermit {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionRequirement {
    None,
    /// The authored operation is validly represented, but its execution owner
    /// is intentionally deferred to a later runtime milestone.
    Deferred,
    /// A destructive radial operation requires a launcher confirmation modal.
    /// Close and release the radial session before presenting the modal.
    Confirmation,
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
        | Command::MouseGesture(_)
        | Command::CoordinateTool(crate::commands::CoordinateToolCommand::Pick) => {
            InteractionRequirement::ExclusiveCapture
        }
        Command::CoordinateTool(crate::commands::CoordinateToolCommand::Settings) => {
            InteractionRequirement::LauncherUi
        }
        Command::Launcher(_)
        | Command::Radial(
            crate::commands::RadialCommand::Edit | crate::commands::RadialCommand::Skins,
        )
        | Command::Query(_)
        | Command::Dialog(_)
        | Command::FileSearch(_)
        | Command::Diff(_) => InteractionRequirement::LauncherUi,
        Command::Note(crate::commands::NoteCommand::OpenLink { link })
            if link.starts_with("www.") || link.contains("://") =>
        {
            InteractionRequirement::ExternalInput
        }
        Command::Note(
            crate::commands::NoteCommand::Dialog
            | crate::commands::NoteCommand::GraphDialog { .. }
            | crate::commands::NoteCommand::UnusedAssets
            | crate::commands::NoteCommand::Open { .. }
            | crate::commands::NoteCommand::New { .. }
            | crate::commands::NoteCommand::Tags
            | crate::commands::NoteCommand::OpenLink { .. },
        )
        | Command::ClipboardModify(crate::commands::ClipboardModifyCommand::Open { .. }) => {
            InteractionRequirement::LauncherUi
        }
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
    /// Selected authored/runtime cell, carried through deferred resolution so
    /// trace evidence can bind the native hover to the eventual dispatch.
    pub selected_cell_id: String,
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
    /// Present only for work materialized from an authored deferred binding.
    /// This keeps execution-time revalidation tied to the exact result selected
    /// during the pre-close resolution round trip.
    pub deferred_origin: Option<DeferredDispatchOrigin>,
    pub history_query: String,
    pub context: InvocationContext,
    pub after_action: AfterActionPolicy,
    pub source: crate::commands::ActivationSource,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DeferredDispatchOrigin {
    Query {
        query: String,
        mode: QueryRunMode,
        selected_action: Option<Action>,
        provider_revision: Option<u64>,
        result_catalog_versions: Option<super::dynamic::MutableResultCatalogVersions>,
        explanation: Option<String>,
    },
    ExactCommand {
        command: String,
        args: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedDeferredSelection {
    pub binding: FrozenBinding,
    pub requirement: InteractionRequirement,
    pub origin: DeferredDispatchOrigin,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DeferredResolutionResult {
    Ready(ResolvedDeferredSelection),
    Pending {
        provider_revision: u64,
        wait_for_change: bool,
    },
    Cancelled,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct DeferredResolutionEnvelope {
    pub identity: RadialDispatchIdentity,
    /// Monotonic per-selection attempt number; replies from an older provider
    /// search cannot satisfy a later retry or timeout fallback.
    pub attempt: u64,
    pub binding: FrozenBinding,
    pub history_query: String,
    pub last_provider_revision: Option<u64>,
    pub wait_for_provider_change: bool,
    pub force_open_query: Option<String>,
    /// Set when the controller cancels/supersedes the selection. A provider
    /// worker may still be blocked in plugin code, but it must not publish a
    /// late result after this token is set.
    pub cancellation: Arc<AtomicBool>,
    pub reply: mpsc::Sender<DeferredResolutionReply>,
    pub wake: mpsc::Sender<()>,
}

pub(crate) struct PendingDeferredSelection {
    pub request: RadialDispatchRequest,
    pub attempt: u64,
    pub cancellation: Arc<AtomicBool>,
    pub deadline_ms: u64,
    pub retry_at_ms: Option<u64>,
    pub retries: u8,
    pub last_provider_revision: Option<u64>,
    pub wait_for_provider_change: bool,
    pub force_open_query: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeferredResolutionReply {
    pub identity: RadialDispatchIdentity,
    pub attempt: u64,
    pub result: DeferredResolutionResult,
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
    fn deferred_provider_capacity_stays_claimed_until_worker_releases_it() {
        let capacity = DeferredProviderSearchCapacity::default();
        let permit = capacity.try_acquire().expect("first worker owns slot");
        assert!(capacity.try_acquire().is_none());
        let cancelled = AtomicBool::new(true);
        assert!(cancelled.load(std::sync::atomic::Ordering::Acquire));
        assert!(capacity.try_acquire().is_none());
        drop(permit);
        assert!(capacity.try_acquire().is_some());
    }

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

    #[test]
    fn typed_note_and_clipboard_open_commands_require_their_actual_handoff() {
        use crate::commands::{ClipboardModifyCommand, NoteCommand};

        for command in [
            Command::Note(NoteCommand::Dialog),
            Command::Note(NoteCommand::GraphDialog { args: None }),
            Command::Note(NoteCommand::UnusedAssets),
            Command::Note(NoteCommand::Open {
                slug: "note".into(),
            }),
            Command::Note(NoteCommand::New {
                slug: "new-note".into(),
                template: None,
            }),
            Command::Note(NoteCommand::Tags),
            Command::Note(NoteCommand::OpenLink {
                link: "local-note".into(),
            }),
            Command::ClipboardModify(ClipboardModifyCommand::Open {
                section: crate::clipboard_modify::actions::ClipboardModifySectionPayload::Help,
            }),
        ] {
            assert_eq!(
                command_requirement(&command),
                InteractionRequirement::LauncherUi
            );
        }
        assert_eq!(
            command_requirement(&Command::Note(NoteCommand::OpenLink {
                link: "https://example.com".into(),
            })),
            InteractionRequirement::ExternalInput
        );
        assert_eq!(
            command_requirement(&Command::Note(NoteCommand::Remove {
                slug: "note".into(),
            })),
            InteractionRequirement::None
        );
        assert_eq!(
            command_requirement(&Command::ClipboardModify(ClipboardModifyCommand::Undo {
                raw_argument: None,
            })),
            InteractionRequirement::None
        );
    }

    #[test]
    fn mouse_pick_and_settings_use_their_required_handoffs() {
        assert_eq!(
            command_requirement(&Command::CoordinateTool(
                crate::commands::CoordinateToolCommand::Pick,
            )),
            InteractionRequirement::ExclusiveCapture
        );
        assert_eq!(
            command_requirement(&Command::CoordinateTool(
                crate::commands::CoordinateToolCommand::Cancel,
            )),
            InteractionRequirement::None
        );
        assert_eq!(
            command_requirement(&Command::CoordinateTool(
                crate::commands::CoordinateToolCommand::Settings,
            )),
            InteractionRequirement::LauncherUi
        );
    }

    fn request(generation: u64) -> RadialDispatchRequest {
        RadialDispatchRequest {
            identity: RadialDispatchIdentity {
                session_id: SessionId::new("s1"),
                selected_cell_id: "s1-cell".into(),
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
            deferred_origin: None,
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
