use super::model::{
    CellId, ClickGesture, InteractionMode, InvocationId, MenuId, SessionId, TriggerScope,
};
use super::session::{NavigationCommand, NavigationModifiers};

pub type Timestamp = u64;
pub type SettingsGeneration = u64;
pub type ContextToken = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputProvenance {
    Physical,
    ExternalInjected,
    SelfInjected,
}

/// Why the invocation route must retire its current logical lifecycle.
///
/// Keeping this typed at the reducer boundary prevents reload, operating-system
/// transitions, and failures from being collapsed into an ordinary dismissal.
/// The reason is carried to the controller so native ownership can be released
/// through the same close path as the corresponding lifecycle transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleCancellation {
    SettingsReload,
    FeatureDisabled,
    HostFailure,
    HookFailure,
    Shutdown,
    Suspend,
    SessionLock,
    DesktopUnavailable,
    SessionReplaced,
    PriorityPreempted,
}

impl LifecycleCancellation {
    /// Whether the hook can still be trusted to observe the matching release.
    pub fn preserves_input_continuity(self) -> bool {
        !matches!(
            self,
            Self::HookFailure
                | Self::Shutdown
                | Self::Suspend
                | Self::SessionLock
                | Self::DesktopUnavailable
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvocationEvent {
    ChordPressed {
        id: InvocationId,
        primary_key: u32,
        at: Timestamp,
        threshold_ms: u64,
        generation: SettingsGeneration,
        context_token: ContextToken,
        menu_id: MenuId,
        interaction: InteractionMode,
        repeat: bool,
    },
    PrimaryReleased {
        id: InvocationId,
        at: Timestamp,
    },
    ModifierChanged {
        id: InvocationId,
        at: Timestamp,
    },
    Deadline {
        id: InvocationId,
        at: Timestamp,
        generation: SettingsGeneration,
    },
    /// A session-scoped close request (for example Escape) that must retire
    /// the current radial lifecycle before any pending physical cycle can be
    /// admitted.  The session identity is optional only while preparation is
    /// still waiting for a native session.
    RadialCloseRequested {
        id: InvocationId,
        session_id: Option<SessionId>,
    },
    RadialSessionOpened {
        id: InvocationId,
        session_id: SessionId,
    },
    /// An externally requested radial has been admitted as the current
    /// lifecycle before its native session exists.  This is the replacement
    /// boundary for command/direct opens: old feedback can no longer install
    /// or retire the newly admitted lifecycle.
    ExternalSessionAdmitted {
        id: InvocationId,
        menu_id: MenuId,
        context_token: ContextToken,
        interaction: InteractionMode,
    },
    /// A radial session opened from a command or a direct/global item-input
    /// route rather than from the shared physical chord.  These sessions are
    /// still part of the same visible lifecycle so a later shared hold or
    /// Escape closes them, but their open feedback must not resurrect a
    /// cancelled physical preparation.
    ExternalSessionOpened {
        id: InvocationId,
        session_id: SessionId,
        menu_id: MenuId,
        context_token: ContextToken,
        interaction: InteractionMode,
    },
    RadialClosedForAction {
        id: InvocationId,
    },
    RadialSessionClosed {
        id: InvocationId,
    },
    DirectToggle {
        id: InvocationId,
        menu_id: MenuId,
        at: Timestamp,
        primary_key: u32,
        provenance: InputProvenance,
    },
    CancelLifecycle {
        reason: LifecycleCancellation,
        primary_still_down: bool,
    },
    ExclusiveToolChanged {
        active: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvocationIntent {
    ScheduleDeadline {
        id: InvocationId,
        at: Timestamp,
        generation: SettingsGeneration,
    },
    ToggleLegacyLauncher {
        id: InvocationId,
    },
    OpenRadial {
        id: InvocationId,
        menu_id: MenuId,
        context_token: ContextToken,
        interaction: InteractionMode,
        trigger_still_down: bool,
    },
    /// Open a radial from a command/direct route.  Unlike a physical hold,
    /// this emits the external lifecycle admission before native preparation.
    OpenExternalRadial {
        id: InvocationId,
        menu_id: MenuId,
        context_token: ContextToken,
        interaction: InteractionMode,
        trigger_still_down: bool,
    },
    ToggleDirectMenu {
        id: InvocationId,
        menu_id: MenuId,
        primary_key: u32,
        provenance: InputProvenance,
        trigger_still_down: bool,
    },
    CloseRadial {
        session_id: Option<SessionId>,
    },
    TriggerReleased {
        id: InvocationId,
    },
    Navigate {
        session_id: SessionId,
        command: NavigationCommand,
        modifiers: NavigationModifiers,
    },
    ActivateItem {
        id: InvocationId,
        menu_id: MenuId,
        cell_id: CellId,
        gesture: ClickGesture,
        scope: TriggerScope,
        source: crate::commands::ActivationSource,
        trigger_still_down: bool,
    },
    HoldCancelledBeforePresentation {
        id: InvocationId,
    },
    CancelRadialLifecycle {
        id: InvocationId,
        reason: LifecycleCancellation,
    },
    CancelDeadline {
        id: InvocationId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DrainReason {
    Dismissed,
    ActionHandoff,
    LifecycleCancelled,
    ExclusiveTool,
}

/// The lifecycle of the radial presentation, independent from the physical
/// key cycle currently being admitted.  An opening lifecycle may exist while
/// GUI preparation is still in flight, and a closing lifecycle remains owned
/// until the native host acknowledges it.  Invocation ids make late feedback
/// harmless when a newer chord has already been admitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RadialLifecycle {
    Closed,
    Opening {
        id: InvocationId,
        menu_id: MenuId,
        context_token: ContextToken,
        interaction: InteractionMode,
        trigger_still_down: bool,
    },
    Active {
        id: InvocationId,
        menu_id: MenuId,
        context_token: ContextToken,
        interaction: InteractionMode,
        session_id: SessionId,
        trigger_still_down: bool,
    },
    Closing {
        id: InvocationId,
        session_id: Option<SessionId>,
    },
}

impl RadialLifecycle {
    pub(crate) fn invocation_id(&self) -> Option<InvocationId> {
        match self {
            Self::Closed => None,
            Self::Opening { id, .. } | Self::Active { id, .. } | Self::Closing { id, .. } => {
                Some(*id)
            }
        }
    }

    pub(crate) fn session_id(&self) -> Option<SessionId> {
        match self {
            Self::Active { session_id, .. } => Some(session_id.clone()),
            Self::Closing { session_id, .. } => session_id.clone(),
            Self::Closed | Self::Opening { .. } => None,
        }
    }

    fn interaction(&self) -> InteractionMode {
        match self {
            Self::Opening { interaction, .. } | Self::Active { interaction, .. } => *interaction,
            Self::Closed | Self::Closing { .. } => InteractionMode::StickyClick,
        }
    }

    fn is_open_or_opening(&self) -> bool {
        !matches!(self, Self::Closed)
    }

    fn matches_close_target(
        &self,
        invocation_id: InvocationId,
        session_id: Option<&SessionId>,
    ) -> bool {
        match self {
            Self::Opening { id, .. } => *id == invocation_id && session_id.is_none(),
            Self::Active {
                id,
                session_id: active_session,
                ..
            }
            | Self::Closing {
                id,
                session_id: Some(active_session),
            } => *id == invocation_id && session_id == Some(active_session),
            Self::Closing {
                id,
                session_id: None,
            } => *id == invocation_id && session_id.is_none(),
            Self::Closed => false,
        }
    }
}

/// The action admitted for a held physical cycle.  It is captured at key
/// admission rather than re-derived at release, so a close-starting cycle
/// cannot reopen after Esc/native close feedback changes the visible state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeldRadialAction {
    Open {
        menu_id: MenuId,
        context_token: ContextToken,
        interaction: InteractionMode,
    },
    Close {
        invocation_id: InvocationId,
        session_id: Option<SessionId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvocationState {
    Idle,
    Pending {
        id: InvocationId,
        primary_key: u32,
        start: Timestamp,
        deadline: Timestamp,
        generation: SettingsGeneration,
        context_token: ContextToken,
        menu_id: MenuId,
        interaction: InteractionMode,
        primary_down: bool,
        provenance: InputProvenance,
        held_action: HeldRadialAction,
    },
    /// A physical cycle that crossed the hold deadline.  The radial lifecycle
    /// is tracked separately by [`InvocationReducer::radial_lifecycle`].
    RadialActive {
        id: InvocationId,
        interaction: InteractionMode,
        session_id: Option<SessionId>,
        trigger_still_down: bool,
        held_action: HeldRadialAction,
    },
    AwaitingOwnedRelease {
        id: InvocationId,
        reason: DrainReason,
    },
    SuppressedByExclusiveTool {
        owned_release: Option<InvocationId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvocationReducer {
    state: InvocationState,
    radial_lifecycle: RadialLifecycle,
}

impl Default for InvocationReducer {
    fn default() -> Self {
        Self {
            state: InvocationState::Idle,
            radial_lifecycle: RadialLifecycle::Closed,
        }
    }
}

impl InvocationReducer {
    pub fn state(&self) -> &InvocationState {
        &self.state
    }

    pub fn radial_lifecycle(&self) -> &RadialLifecycle {
        &self.radial_lifecycle
    }

    pub fn has_radial_lifecycle(&self) -> bool {
        self.radial_lifecycle.is_open_or_opening()
    }

    pub fn reduce(&mut self, event: InvocationEvent) -> Vec<InvocationIntent> {
        self.reduce_with_provenance(event, InputProvenance::Physical)
    }

    /// Reduce an event while recording the provenance of a newly admitted
    /// physical cycle.  The event shape intentionally remains compatible with
    /// the existing reducer callers; the native adapter is the owner of the
    /// provenance boundary and supplies it here.
    pub fn reduce_with_provenance(
        &mut self,
        event: InvocationEvent,
        provenance: InputProvenance,
    ) -> Vec<InvocationIntent> {
        use InvocationEvent as E;
        use InvocationIntent as I;
        match event {
            E::ExclusiveToolChanged { active: true } => {
                if matches!(
                    &self.state,
                    InvocationState::SuppressedByExclusiveTool { .. }
                ) {
                    return vec![];
                }
                let (intents, owned_release) = self.cancel_for_exclusive_tool();
                self.state = InvocationState::SuppressedByExclusiveTool { owned_release };
                intents
            }
            E::ExclusiveToolChanged { active: false }
                if matches!(
                    &self.state,
                    InvocationState::SuppressedByExclusiveTool { .. }
                ) =>
            {
                let InvocationState::SuppressedByExclusiveTool { owned_release } = &self.state
                else {
                    unreachable!()
                };
                let owned_release = *owned_release;
                self.state = owned_release.map_or(InvocationState::Idle, |id| {
                    InvocationState::AwaitingOwnedRelease {
                        id,
                        reason: DrainReason::ExclusiveTool,
                    }
                });
                vec![]
            }
            E::CancelLifecycle {
                reason,
                primary_still_down,
            } => self.cancel_current(reason, primary_still_down),
            E::DirectToggle {
                id,
                menu_id,
                primary_key,
                provenance,
                ..
            } if matches!(
                &self.state,
                InvocationState::Idle | InvocationState::RadialActive { .. }
            ) =>
            {
                vec![I::ToggleDirectMenu {
                    id,
                    menu_id,
                    primary_key,
                    provenance,
                    trigger_still_down: true,
                }]
            }
            E::ChordPressed { repeat: true, .. } => vec![],
            E::ChordPressed {
                id,
                primary_key,
                at,
                threshold_ms,
                generation,
                context_token,
                menu_id,
                interaction,
                repeat: false,
            } if matches!(&self.state, InvocationState::Idle) => {
                let deadline = at.saturating_add(threshold_ms);
                let held_action = match &self.radial_lifecycle {
                    RadialLifecycle::Closed => HeldRadialAction::Open {
                        menu_id: menu_id.clone(),
                        context_token,
                        interaction,
                    },
                    lifecycle => HeldRadialAction::Close {
                        invocation_id: lifecycle
                            .invocation_id()
                            .expect("non-closed radial lifecycle has an invocation id"),
                        session_id: lifecycle.session_id(),
                    },
                };
                self.state = InvocationState::Pending {
                    id,
                    primary_key,
                    start: at,
                    deadline,
                    generation,
                    context_token,
                    menu_id,
                    interaction,
                    primary_down: true,
                    provenance,
                    held_action,
                };
                vec![I::ScheduleDeadline {
                    id,
                    at: deadline,
                    generation,
                }]
            }
            E::ChordPressed { repeat: false, .. } => vec![],
            E::ModifierChanged { .. } => vec![],
            E::Deadline { id, at, generation } => {
                let InvocationState::Pending {
                    id: pending,
                    deadline,
                    generation: pending_generation,
                    primary_down,
                    ..
                } = &self.state
                else {
                    return vec![];
                };
                if *pending != id
                    || *pending_generation != generation
                    || at < *deadline
                    || !*primary_down
                {
                    return vec![];
                }
                self.promote_pending(true)
            }
            E::PrimaryReleased { id, at } => self.release(id, at),
            E::RadialCloseRequested { id, session_id } => {
                if !self
                    .radial_lifecycle
                    .matches_close_target(id, session_id.as_ref())
                {
                    return vec![];
                }
                let mut intents = Vec::new();
                if let InvocationState::Pending {
                    id: pending,
                    primary_down,
                    ..
                } = &self.state
                {
                    let pending = *pending;
                    intents.push(I::CancelDeadline { id: pending });
                    self.state = if *primary_down {
                        InvocationState::AwaitingOwnedRelease {
                            id: pending,
                            reason: DrainReason::LifecycleCancelled,
                        }
                    } else {
                        InvocationState::Idle
                    };
                }
                match self.radial_lifecycle.clone() {
                    RadialLifecycle::Opening { .. } => {
                        self.radial_lifecycle = RadialLifecycle::Closed;
                        intents.push(I::CancelRadialLifecycle {
                            id,
                            reason: LifecycleCancellation::SessionReplaced,
                        });
                    }
                    RadialLifecycle::Active { session_id, .. } => {
                        self.radial_lifecycle = RadialLifecycle::Closing {
                            id,
                            session_id: Some(session_id),
                        };
                    }
                    RadialLifecycle::Closing { .. } | RadialLifecycle::Closed => {}
                }
                intents
            }
            E::ExternalSessionAdmitted {
                id,
                menu_id,
                context_token,
                interaction,
            } => self.admit_external_session(id, menu_id, context_token, interaction),
            E::ExternalSessionOpened {
                id,
                session_id,
                menu_id,
                context_token,
                interaction,
            } if matches!(
                &self.radial_lifecycle,
                RadialLifecycle::Opening {
                    id: opening,
                    ..
                } if *opening == id
            ) =>
            {
                self.radial_lifecycle = RadialLifecycle::Active {
                    id,
                    menu_id,
                    context_token,
                    interaction,
                    session_id,
                    trigger_still_down: false,
                };
                vec![]
            }
            E::ExternalSessionOpened { .. } => vec![],
            E::RadialSessionOpened { id, session_id } => {
                let lifecycle =
                    std::mem::replace(&mut self.radial_lifecycle, RadialLifecycle::Closed);
                self.radial_lifecycle = match lifecycle {
                    RadialLifecycle::Opening {
                        id: active,
                        menu_id,
                        context_token,
                        interaction,
                        trigger_still_down,
                    } if active == id => RadialLifecycle::Active {
                        id,
                        menu_id,
                        context_token,
                        interaction,
                        session_id,
                        trigger_still_down,
                    },
                    other => other,
                };
                vec![]
            }
            E::RadialClosedForAction { id } => {
                if self.radial_lifecycle.invocation_id() == Some(id) {
                    self.radial_lifecycle = RadialLifecycle::Closed;
                }
                if let InvocationState::RadialActive {
                    id: active,
                    trigger_still_down: true,
                    ..
                } = &self.state
                    && *active == id
                {
                    self.state = InvocationState::AwaitingOwnedRelease {
                        id,
                        reason: DrainReason::ActionHandoff,
                    };
                }
                vec![]
            }
            E::RadialSessionClosed { id } => {
                if self.radial_lifecycle.invocation_id() == Some(id) {
                    self.radial_lifecycle = RadialLifecycle::Closed;
                }
                if let InvocationState::RadialActive {
                    id: active,
                    trigger_still_down: true,
                    ..
                } = &self.state
                    && *active == id
                {
                    self.state = InvocationState::AwaitingOwnedRelease {
                        id,
                        reason: DrainReason::Dismissed,
                    };
                }
                vec![]
            }
            _ => vec![],
        }
    }

    fn admit_external_session(
        &mut self,
        id: InvocationId,
        menu_id: MenuId,
        context_token: ContextToken,
        interaction: InteractionMode,
    ) -> Vec<InvocationIntent> {
        use InvocationIntent as I;

        // The controller has already admitted this replacement and owns
        // closing/invalidating the previous native or preparation lease. Do
        // not send a second unscoped close back to it: a newer pending open
        // may already have replaced the old session by the time feedback is
        // delivered.
        let mut intents = Vec::new();

        match self.state.clone() {
            InvocationState::Pending {
                id: pending,
                primary_down,
                ..
            } => {
                intents.push(I::CancelDeadline { id: pending });
                self.state = if primary_down {
                    InvocationState::AwaitingOwnedRelease {
                        id: pending,
                        reason: DrainReason::LifecycleCancelled,
                    }
                } else {
                    InvocationState::Idle
                };
            }
            InvocationState::RadialActive {
                id: cycle,
                trigger_still_down,
                ..
            } => {
                if trigger_still_down {
                    self.state = InvocationState::AwaitingOwnedRelease {
                        id: cycle,
                        reason: DrainReason::LifecycleCancelled,
                    };
                } else {
                    self.state = InvocationState::Idle;
                }
            }
            InvocationState::Idle
            | InvocationState::AwaitingOwnedRelease { .. }
            | InvocationState::SuppressedByExclusiveTool { .. } => {}
        }

        self.radial_lifecycle = RadialLifecycle::Opening {
            id,
            menu_id,
            context_token,
            interaction,
            trigger_still_down: false,
        };
        intents
    }

    fn release(&mut self, id: InvocationId, at: Timestamp) -> Vec<InvocationIntent> {
        use InvocationIntent as I;
        match self.state.clone() {
            InvocationState::Pending {
                id: pending,
                start,
                deadline,
                held_action,
                ..
            } if pending == id => {
                if at < start {
                    self.state = InvocationState::Idle;
                    vec![I::CancelDeadline { id }]
                } else if at < deadline {
                    self.state = InvocationState::Idle;
                    vec![I::CancelDeadline { id }, I::ToggleLegacyLauncher { id }]
                } else if matches!(
                    held_action,
                    HeldRadialAction::Open {
                        interaction: InteractionMode::ReleaseToSelect,
                        ..
                    }
                ) {
                    self.state = InvocationState::Idle;
                    vec![
                        I::CancelDeadline { id },
                        I::HoldCancelledBeforePresentation { id },
                    ]
                } else {
                    // A delayed release carries the physical timestamp.  If
                    // it crossed the deadline, admit the already-captured
                    // hold action exactly once before draining the release.
                    let mut intents = vec![I::CancelDeadline { id }];
                    intents.extend(self.promote_pending(false));
                    intents.extend(self.release_holding(id));
                    intents
                }
            }
            InvocationState::RadialActive {
                id: active,
                trigger_still_down: true,
                ..
            } if active == id => self.release_holding(id),
            InvocationState::AwaitingOwnedRelease { id: owned, reason } if owned == id => {
                let acknowledge = reason == DrainReason::ActionHandoff;
                self.state = InvocationState::Idle;
                acknowledge
                    .then_some(I::TriggerReleased { id })
                    .into_iter()
                    .collect()
            }
            InvocationState::SuppressedByExclusiveTool { owned_release }
                if owned_release == Some(id) =>
            {
                self.state = InvocationState::SuppressedByExclusiveTool {
                    owned_release: None,
                };
                vec![]
            }
            _ => vec![],
        }
    }

    /// Turn a pending physical cycle into its admitted hold action.  This is
    /// shared by the timer deadline and a delayed key-up so timestamp delivery
    /// cannot reclassify a hold as a tap.
    fn promote_pending(&mut self, trigger_still_down: bool) -> Vec<InvocationIntent> {
        use InvocationIntent as I;
        let state = std::mem::replace(&mut self.state, InvocationState::Idle);
        let InvocationState::Pending {
            id, held_action, ..
        } = state
        else {
            self.state = state;
            return vec![];
        };

        let session_id = match &held_action {
            HeldRadialAction::Close { session_id, .. } => session_id.clone(),
            HeldRadialAction::Open { .. } => None,
        };
        let holding_interaction = match &held_action {
            HeldRadialAction::Open { interaction, .. } => *interaction,
            HeldRadialAction::Close { .. } => self.radial_lifecycle.interaction(),
        };
        self.state = InvocationState::RadialActive {
            id,
            interaction: holding_interaction,
            session_id: session_id.clone(),
            trigger_still_down,
            held_action: held_action.clone(),
        };

        match held_action {
            HeldRadialAction::Open {
                menu_id,
                context_token,
                interaction,
            } => {
                self.radial_lifecycle = RadialLifecycle::Opening {
                    id,
                    menu_id: menu_id.clone(),
                    context_token,
                    interaction,
                    trigger_still_down,
                };
                vec![I::OpenRadial {
                    id,
                    menu_id,
                    context_token,
                    interaction,
                    trigger_still_down,
                }]
            }
            HeldRadialAction::Close {
                invocation_id,
                session_id,
            } => {
                if !self
                    .radial_lifecycle
                    .matches_close_target(invocation_id, session_id.as_ref())
                {
                    // Esc, a lifecycle failure, or prior close feedback may
                    // have retired the target while this physical cycle was
                    // still held. The cycle is nevertheless consumed, but it
                    // must not issue a second close or reopen an old session.
                    return vec![];
                }
                match self.radial_lifecycle.clone() {
                    RadialLifecycle::Opening { .. } => {
                        // Preparation has no native session yet. Invalidate
                        // the admitted opening lease itself so a delayed GUI
                        // reply cannot resurrect the menu.
                        self.radial_lifecycle = RadialLifecycle::Closed;
                        vec![I::CancelRadialLifecycle {
                            id: invocation_id,
                            reason: LifecycleCancellation::SessionReplaced,
                        }]
                    }
                    RadialLifecycle::Active { session_id, .. } => {
                        self.radial_lifecycle = RadialLifecycle::Closing {
                            id: invocation_id,
                            session_id: Some(session_id.clone()),
                        };
                        vec![I::CloseRadial {
                            session_id: Some(session_id),
                        }]
                    }
                    RadialLifecycle::Closing { .. } | RadialLifecycle::Closed => vec![],
                }
            }
        }
    }

    fn release_holding(&mut self, id: InvocationId) -> Vec<InvocationIntent> {
        use InvocationIntent as I;
        let InvocationState::RadialActive {
            id: active,
            interaction,
            held_action,
            ..
        } = self.state.clone()
        else {
            return vec![];
        };
        if active != id {
            return vec![];
        }
        self.state = InvocationState::Idle;
        match held_action {
            HeldRadialAction::Close { .. } => vec![],
            HeldRadialAction::Open { .. }
                if interaction == InteractionMode::ReleaseToSelect
                    && matches!(
                        &self.radial_lifecycle,
                        RadialLifecycle::Opening { id: opening, .. } if *opening == id
                    ) =>
            {
                self.radial_lifecycle = RadialLifecycle::Closed;
                vec![I::CancelRadialLifecycle {
                    id,
                    reason: LifecycleCancellation::SessionReplaced,
                }]
            }
            HeldRadialAction::Open { .. }
                if matches!(
                    &self.radial_lifecycle,
                    RadialLifecycle::Closed | RadialLifecycle::Closing { .. }
                ) =>
            {
                vec![]
            }
            HeldRadialAction::Open { .. } => match interaction {
                InteractionMode::StickyClick | InteractionMode::ReleaseToSelect => {
                    self.set_radial_trigger_released(id);
                    vec![I::TriggerReleased { id }]
                }
                InteractionMode::HoldAndClick => self.close_opening_or_active(id),
            },
        }
    }

    fn set_radial_trigger_released(&mut self, id: InvocationId) {
        match &mut self.radial_lifecycle {
            RadialLifecycle::Opening {
                id: active,
                trigger_still_down,
                ..
            }
            | RadialLifecycle::Active {
                id: active,
                trigger_still_down,
                ..
            } if *active == id => *trigger_still_down = false,
            _ => {}
        }
    }

    fn close_opening_or_active(&mut self, id: InvocationId) -> Vec<InvocationIntent> {
        use InvocationIntent as I;
        match self.radial_lifecycle.clone() {
            RadialLifecycle::Opening { id: opening, .. } if opening == id => {
                self.radial_lifecycle = RadialLifecycle::Closed;
                vec![I::CancelRadialLifecycle {
                    id,
                    reason: LifecycleCancellation::SessionReplaced,
                }]
            }
            RadialLifecycle::Active {
                id: active,
                session_id,
                ..
            } if active == id => {
                self.radial_lifecycle = RadialLifecycle::Closing {
                    id,
                    session_id: Some(session_id.clone()),
                };
                vec![I::CloseRadial {
                    session_id: Some(session_id),
                }]
            }
            _ => vec![],
        }
    }

    fn cancel_current(
        &mut self,
        reason: LifecycleCancellation,
        primary_still_down: bool,
    ) -> Vec<InvocationIntent> {
        let mut intents = Vec::new();
        let cycle_id = match &self.state {
            InvocationState::Pending { id, .. }
            | InvocationState::RadialActive { id, .. }
            | InvocationState::AwaitingOwnedRelease { id, .. } => Some(*id),
            InvocationState::SuppressedByExclusiveTool {
                owned_release: Some(id),
            } => Some(*id),
            InvocationState::Idle | InvocationState::SuppressedByExclusiveTool { .. } => None,
        };
        if let InvocationState::Pending { id, .. } = &self.state {
            intents.push(InvocationIntent::CancelDeadline { id: *id });
        }
        if let Some(radial_id) = self.radial_lifecycle.invocation_id() {
            intents.push(InvocationIntent::CancelRadialLifecycle {
                id: radial_id,
                reason,
            });
            self.radial_lifecycle = RadialLifecycle::Closed;
        }
        self.state = if primary_still_down {
            cycle_id.map_or(InvocationState::Idle, |id| {
                InvocationState::AwaitingOwnedRelease {
                    id,
                    reason: DrainReason::LifecycleCancelled,
                }
            })
        } else {
            InvocationState::Idle
        };
        if self.state == InvocationState::Idle {
            // A cancellation after an already-drained cycle must not leave a
            // synthetic radial lifecycle around, but it also has no release to
            // acknowledge.
            if cycle_id.is_none() && intents.is_empty() {
                self.radial_lifecycle = RadialLifecycle::Closed;
            }
        }
        intents
    }

    fn cancel_for_exclusive_tool(&mut self) -> (Vec<InvocationIntent>, Option<InvocationId>) {
        let mut intents = Vec::new();
        let owned_release = match &self.state {
            InvocationState::Pending {
                id, primary_down, ..
            } => {
                intents.push(InvocationIntent::CancelDeadline { id: *id });
                (*primary_down).then_some(*id)
            }
            InvocationState::RadialActive {
                id,
                trigger_still_down,
                ..
            } => (*trigger_still_down).then_some(*id),
            InvocationState::AwaitingOwnedRelease { id, .. } => Some(*id),
            InvocationState::SuppressedByExclusiveTool { owned_release } => *owned_release,
            InvocationState::Idle => None,
        };
        match self.radial_lifecycle.clone() {
            RadialLifecycle::Opening { id, .. } => {
                self.radial_lifecycle = RadialLifecycle::Closed;
                intents.push(InvocationIntent::CancelRadialLifecycle {
                    id,
                    reason: LifecycleCancellation::PriorityPreempted,
                });
            }
            RadialLifecycle::Active { id, session_id, .. } => {
                self.radial_lifecycle = RadialLifecycle::Closing {
                    id,
                    session_id: Some(session_id.clone()),
                };
                intents.push(InvocationIntent::CloseRadial {
                    session_id: Some(session_id),
                });
            }
            RadialLifecycle::Closing { .. } | RadialLifecycle::Closed => {}
        }
        (intents, owned_release)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn press(id: u64, at: u64, mode: InteractionMode) -> InvocationEvent {
        InvocationEvent::ChordPressed {
            id: InvocationId(id),
            primary_key: 35,
            at,
            threshold_ms: 350,
            generation: 7,
            context_token: 9,
            menu_id: MenuId::new("main"),
            interaction: mode,
            repeat: false,
        }
    }
    #[test]
    fn tap_releases_immediately_and_exactly_once() {
        let mut r = InvocationReducer::default();
        r.reduce(press(1, 100, InteractionMode::StickyClick));
        let i = r.reduce(InvocationEvent::PrimaryReleased {
            id: InvocationId(1),
            at: 449,
        });
        assert!(matches!(
            i.as_slice(),
            [
                InvocationIntent::CancelDeadline { .. },
                InvocationIntent::ToggleLegacyLauncher { .. }
            ]
        ));
        assert!(
            r.reduce(InvocationEvent::PrimaryReleased {
                id: InvocationId(1),
                at: 450
            })
            .is_empty()
        );
    }
    #[test]
    fn boundary_is_hold_and_stale_deadline_is_ignored() {
        let mut r = InvocationReducer::default();
        r.reduce(press(2, 100, InteractionMode::StickyClick));
        assert!(
            r.reduce(InvocationEvent::Deadline {
                id: InvocationId(1),
                at: 450,
                generation: 7
            })
            .is_empty()
        );
        assert!(matches!(
            r.reduce(InvocationEvent::Deadline {
                id: InvocationId(2),
                at: 450,
                generation: 7
            })
            .as_slice(),
            [InvocationIntent::OpenRadial { .. }]
        ));
    }
    #[test]
    fn delayed_release_to_select_cancels_before_presentation() {
        let mut r = InvocationReducer::default();
        r.reduce(press(1, 0, InteractionMode::ReleaseToSelect));
        assert!(matches!(
            r.reduce(InvocationEvent::PrimaryReleased {
                id: InvocationId(1),
                at: 900
            })
            .as_slice(),
            [
                InvocationIntent::CancelDeadline { .. },
                InvocationIntent::HoldCancelledBeforePresentation { .. }
            ]
        ));
    }
    #[test]
    fn delayed_sticky_release_still_opens_before_draining_release() {
        let mut r = InvocationReducer::default();
        r.reduce(press(2, 0, InteractionMode::StickyClick));
        assert!(matches!(
            r.reduce(InvocationEvent::PrimaryReleased {
                id: InvocationId(2),
                at: 900
            })
            .as_slice(),
            [
                InvocationIntent::CancelDeadline { .. },
                InvocationIntent::OpenRadial { .. },
                InvocationIntent::TriggerReleased { .. }
            ]
        ));
    }
    #[test]
    fn repeats_and_modifier_changes_do_not_rearm() {
        let mut r = InvocationReducer::default();
        r.reduce(press(1, 0, InteractionMode::StickyClick));
        let mut p = press(1, 5, InteractionMode::StickyClick);
        if let InvocationEvent::ChordPressed { repeat, .. } = &mut p {
            *repeat = true
        };
        assert!(r.reduce(p).is_empty());
        assert!(
            r.reduce(InvocationEvent::ModifierChanged {
                id: InvocationId(1),
                at: 6
            })
            .is_empty()
        );
    }
    #[test]
    fn long_close_press_closes_and_drains_release() {
        let mut r = InvocationReducer::default();
        r.reduce(press(1, 0, InteractionMode::StickyClick));
        r.reduce(InvocationEvent::Deadline {
            id: InvocationId(1),
            at: 350,
            generation: 7,
        });
        r.reduce(InvocationEvent::RadialSessionOpened {
            id: InvocationId(1),
            session_id: SessionId::new("session-1"),
        });
        r.reduce(InvocationEvent::PrimaryReleased {
            id: InvocationId(1),
            at: 360,
        });
        r.reduce(press(2, 400, InteractionMode::StickyClick));
        assert!(matches!(r.state(), InvocationState::Pending { .. }));
        let i = r.reduce(InvocationEvent::Deadline {
            id: InvocationId(2),
            at: 750,
            generation: 7,
        });
        assert!(matches!(
            i.as_slice(),
            [InvocationIntent::CloseRadial {
                session_id: Some(_)
            }]
        ));
        assert!(
            r.reduce(InvocationEvent::PrimaryReleased {
                id: InvocationId(2),
                at: 410
            })
            .is_empty()
        );
        assert!(matches!(r.state(), InvocationState::Idle));
    }
    #[test]
    fn lifecycle_reload_cancels_without_replaying_tap() {
        let mut r = InvocationReducer::default();
        r.reduce(press(1, 0, InteractionMode::StickyClick));
        assert!(matches!(
            r.reduce(InvocationEvent::CancelLifecycle {
                reason: LifecycleCancellation::SettingsReload,
                primary_still_down: true
            })
            .as_slice(),
            [InvocationIntent::CancelDeadline { .. }]
        ));
        assert!(
            r.reduce(InvocationEvent::PrimaryReleased {
                id: InvocationId(1),
                at: 20
            })
            .is_empty()
        );
    }
    #[test]
    fn direct_toggle_bypasses_threshold() {
        let mut r = InvocationReducer::default();
        assert!(matches!(
            r.reduce(InvocationEvent::DirectToggle {
                id: InvocationId(3),
                menu_id: MenuId::new("tools"),
                at: 1,
                primary_key: 0x54,
                provenance: InputProvenance::Physical,
            })
            .as_slice(),
            [InvocationIntent::ToggleDirectMenu { .. }]
        ));
    }

    #[test]
    fn non_monotonic_release_never_replays_a_tap() {
        let mut r = InvocationReducer::default();
        r.reduce(press(1, 100, InteractionMode::StickyClick));
        assert!(matches!(
            r.reduce(InvocationEvent::PrimaryReleased {
                id: InvocationId(1),
                at: 99
            })
            .as_slice(),
            [InvocationIntent::CancelDeadline { .. }]
        ));
    }

    #[test]
    fn exclusive_takeover_retains_and_drains_pending_release() {
        let mut r = InvocationReducer::default();
        r.reduce(press(4, 0, InteractionMode::StickyClick));
        assert!(matches!(
            r.reduce(InvocationEvent::ExclusiveToolChanged { active: true })
                .as_slice(),
            [InvocationIntent::CancelDeadline {
                id: InvocationId(4)
            }]
        ));
        assert!(matches!(
            r.state(),
            InvocationState::SuppressedByExclusiveTool {
                owned_release: Some(InvocationId(4))
            }
        ));
        r.reduce(InvocationEvent::ExclusiveToolChanged { active: false });
        assert!(matches!(
            r.state(),
            InvocationState::AwaitingOwnedRelease {
                id: InvocationId(4),
                reason: DrainReason::ExclusiveTool
            }
        ));
        assert!(
            r.reduce(InvocationEvent::PrimaryReleased {
                id: InvocationId(4),
                at: 10
            })
            .is_empty()
        );
        assert!(matches!(r.state(), InvocationState::Idle));
    }

    #[test]
    fn release_during_exclusive_takeover_is_consumed_before_resume() {
        let mut r = InvocationReducer::default();
        r.reduce(press(5, 0, InteractionMode::StickyClick));
        r.reduce(InvocationEvent::ExclusiveToolChanged { active: true });
        r.reduce(InvocationEvent::PrimaryReleased {
            id: InvocationId(5),
            at: 20,
        });
        assert!(matches!(
            r.state(),
            InvocationState::SuppressedByExclusiveTool {
                owned_release: None
            }
        ));
        r.reduce(InvocationEvent::ExclusiveToolChanged { active: false });
        assert!(matches!(r.state(), InvocationState::Idle));
    }

    #[test]
    fn session_cancellation_clears_released_invocation_state() {
        let mut r = InvocationReducer::default();
        r.reduce(press(6, 0, InteractionMode::ReleaseToSelect));
        r.reduce(InvocationEvent::Deadline {
            id: InvocationId(6),
            at: 350,
            generation: 7,
        });
        r.reduce(InvocationEvent::PrimaryReleased {
            id: InvocationId(6),
            at: 360,
        });
        r.reduce(InvocationEvent::RadialSessionClosed {
            id: InvocationId(6),
        });
        assert!(matches!(r.state(), InvocationState::Idle));
    }

    #[test]
    fn action_handoff_acknowledges_only_the_exact_owned_release() {
        let mut reducer = InvocationReducer::default();
        reducer.reduce(press(20, 0, InteractionMode::StickyClick));
        reducer.reduce(InvocationEvent::Deadline {
            id: InvocationId(20),
            at: 350,
            generation: 7,
        });
        reducer.reduce(InvocationEvent::RadialClosedForAction {
            id: InvocationId(20),
        });
        assert!(
            reducer
                .reduce(InvocationEvent::PrimaryReleased {
                    id: InvocationId(19),
                    at: 400,
                })
                .is_empty()
        );
        assert!(matches!(
            reducer
                .reduce(InvocationEvent::PrimaryReleased {
                    id: InvocationId(20),
                    at: 401,
                })
                .as_slice(),
            [InvocationIntent::TriggerReleased {
                id: InvocationId(20)
            }]
        ));
    }

    #[test]
    fn threshold_boundary_matrix_uses_physical_release_time() {
        for (elapsed, expected_tap) in [
            (0, true),
            (100, true),
            (349, true),
            (350, false),
            (351, false),
        ] {
            let mut reducer = InvocationReducer::default();
            reducer.reduce(press(30, 1_000, InteractionMode::StickyClick));
            let intents = reducer.reduce(InvocationEvent::PrimaryReleased {
                id: InvocationId(30),
                at: 1_000 + elapsed,
            });
            assert_eq!(
                intents
                    .iter()
                    .filter(|intent| matches!(
                        intent,
                        InvocationIntent::ToggleLegacyLauncher { .. }
                    ))
                    .count(),
                usize::from(expected_tap),
                "elapsed={elapsed}"
            );
            assert_eq!(
                intents
                    .iter()
                    .filter(|intent| matches!(intent, InvocationIntent::OpenRadial { .. }))
                    .count(),
                usize::from(!expected_tap),
                "elapsed={elapsed}"
            );
        }
    }

    #[test]
    fn deadline_repeat_duplicate_and_delayed_delivery_are_exact_once() {
        let mut reducer = InvocationReducer::default();
        reducer.reduce(press(31, 10, InteractionMode::StickyClick));
        let mut repeat = press(31, 20, InteractionMode::StickyClick);
        if let InvocationEvent::ChordPressed { repeat, .. } = &mut repeat {
            *repeat = true;
        }
        assert!(reducer.reduce(repeat).is_empty());
        assert!(
            reducer
                .reduce(InvocationEvent::Deadline {
                    id: InvocationId(31),
                    at: 359,
                    generation: 7,
                })
                .is_empty()
        );
        assert!(matches!(
            reducer
                .reduce(InvocationEvent::Deadline {
                    id: InvocationId(31),
                    at: 360,
                    generation: 7,
                })
                .as_slice(),
            [InvocationIntent::OpenRadial { .. }]
        ));
        assert!(
            reducer
                .reduce(InvocationEvent::Deadline {
                    id: InvocationId(31),
                    at: 900,
                    generation: 7,
                })
                .is_empty(),
            "a duplicate delayed deadline cannot open twice"
        );
        assert!(matches!(
            reducer
                .reduce(InvocationEvent::PrimaryReleased {
                    id: InvocationId(31),
                    at: 370,
                })
                .as_slice(),
            [InvocationIntent::TriggerReleased { .. }]
        ));
        assert!(
            reducer
                .reduce(InvocationEvent::PrimaryReleased {
                    id: InvocationId(31),
                    at: 371,
                })
                .is_empty()
        );
    }

    #[test]
    fn every_lifecycle_cancellation_is_typed_stale_safe_and_drains_once() {
        let reasons = [
            LifecycleCancellation::SettingsReload,
            LifecycleCancellation::FeatureDisabled,
            LifecycleCancellation::HostFailure,
            LifecycleCancellation::HookFailure,
            LifecycleCancellation::Shutdown,
            LifecycleCancellation::Suspend,
            LifecycleCancellation::SessionLock,
            LifecycleCancellation::DesktopUnavailable,
            LifecycleCancellation::SessionReplaced,
            LifecycleCancellation::PriorityPreempted,
        ];
        for (index, reason) in reasons.into_iter().enumerate() {
            let id = InvocationId(100 + index as u64);
            let mut reducer = InvocationReducer::default();
            let mut event = press(id.0, 0, InteractionMode::StickyClick);
            if let InvocationEvent::ChordPressed {
                generation,
                context_token,
                ..
            } = &mut event
            {
                *generation = index as u64 + 1;
                *context_token = id.0;
            }
            reducer.reduce(event);
            let intents = reducer.reduce(InvocationEvent::CancelLifecycle {
                reason,
                primary_still_down: true,
            });
            assert!(matches!(
                intents.as_slice(),
                [InvocationIntent::CancelDeadline { id: cancelled }] if *cancelled == id
            ));
            assert!(
                reducer
                    .reduce(InvocationEvent::Deadline {
                        id,
                        at: 10_000,
                        generation: index as u64 + 1,
                    })
                    .is_empty()
            );
            assert!(
                reducer
                    .reduce(InvocationEvent::PrimaryReleased { id, at: 10_001 })
                    .is_empty()
            );
            assert!(matches!(reducer.state(), InvocationState::Idle));
            assert!(
                reducer
                    .reduce(InvocationEvent::PrimaryReleased { id, at: 10_002 })
                    .is_empty()
            );
        }
    }

    #[test]
    fn rapid_completed_invocations_discard_each_others_stale_events() {
        let mut reducer = InvocationReducer::default();
        for (id, start) in [(200, 0), (201, 200)] {
            reducer.reduce(press(id, start, InteractionMode::StickyClick));
            let intents = reducer.reduce(InvocationEvent::PrimaryReleased {
                id: InvocationId(id),
                at: start + 100,
            });
            assert_eq!(
                intents
                    .iter()
                    .filter(|intent| matches!(
                        intent,
                        InvocationIntent::ToggleLegacyLauncher { .. }
                    ))
                    .count(),
                1
            );
        }
        assert!(
            reducer
                .reduce(InvocationEvent::Deadline {
                    id: InvocationId(200),
                    at: 10_000,
                    generation: 7,
                })
                .is_empty()
        );
    }

    #[test]
    fn tap_while_open_toggles_grid_without_touching_radial_lifecycle() {
        let mut reducer = InvocationReducer::default();
        reducer.reduce(press(210, 0, InteractionMode::StickyClick));
        reducer.reduce(InvocationEvent::Deadline {
            id: InvocationId(210),
            at: 350,
            generation: 7,
        });
        let session_id = SessionId::new("session-210");
        reducer.reduce(InvocationEvent::RadialSessionOpened {
            id: InvocationId(210),
            session_id: session_id.clone(),
        });
        reducer.reduce(InvocationEvent::PrimaryReleased {
            id: InvocationId(210),
            at: 351,
        });

        let before = reducer.radial_lifecycle().clone();
        reducer.reduce(press(211, 500, InteractionMode::StickyClick));
        let intents = reducer.reduce(InvocationEvent::PrimaryReleased {
            id: InvocationId(211),
            at: 600,
        });
        assert!(matches!(
            intents.as_slice(),
            [
                InvocationIntent::CancelDeadline {
                    id: InvocationId(211)
                },
                InvocationIntent::ToggleLegacyLauncher {
                    id: InvocationId(211)
                }
            ]
        ));
        assert_eq!(reducer.radial_lifecycle(), &before);
    }

    #[test]
    fn external_admission_replaces_active_and_opening_lifecycles_stale_safe() {
        let mut reducer = InvocationReducer::default();
        let old = InvocationId(260);
        let replacement = InvocationId(261);
        let old_menu = MenuId::new("old");
        let new_menu = MenuId::new("new");
        reducer.reduce(InvocationEvent::ExternalSessionAdmitted {
            id: old,
            menu_id: old_menu.clone(),
            context_token: old.0,
            interaction: InteractionMode::StickyClick,
        });
        reducer.reduce(InvocationEvent::ExternalSessionOpened {
            id: old,
            session_id: SessionId::new("old-session"),
            menu_id: old_menu,
            context_token: old.0,
            interaction: InteractionMode::StickyClick,
        });
        let intents = reducer.reduce(InvocationEvent::ExternalSessionAdmitted {
            id: replacement,
            menu_id: new_menu.clone(),
            context_token: replacement.0,
            interaction: InteractionMode::StickyClick,
        });
        assert!(intents.is_empty());
        assert!(matches!(
            reducer.radial_lifecycle(),
            RadialLifecycle::Opening { id, menu_id: actual, .. }
                if *id == replacement && actual == &new_menu
        ));

        // Both old open feedback and a close from the replaced session are
        // stale; neither may erase the replacement's preparation lease.
        reducer.reduce(InvocationEvent::ExternalSessionOpened {
            id: old,
            session_id: SessionId::new("late-old"),
            menu_id: MenuId::new("old"),
            context_token: old.0,
            interaction: InteractionMode::StickyClick,
        });
        reducer.reduce(InvocationEvent::RadialSessionClosed { id: old });
        assert!(matches!(
            reducer.radial_lifecycle(),
            RadialLifecycle::Opening { id, .. } if *id == replacement
        ));
        let new_session = SessionId::new("new-session");
        reducer.reduce(InvocationEvent::ExternalSessionOpened {
            id: replacement,
            session_id: new_session.clone(),
            menu_id: new_menu,
            context_token: replacement.0,
            interaction: InteractionMode::StickyClick,
        });
        assert!(matches!(
            reducer.radial_lifecycle(),
            RadialLifecycle::Active { id, session_id, .. }
                if *id == replacement && session_id == &new_session
        ));

        // A second replacement while preparation is still pending follows the
        // same admission path and still rejects the first opening's feedback.
        let third = InvocationId(262);
        reducer.reduce(InvocationEvent::ExternalSessionAdmitted {
            id: third,
            menu_id: MenuId::new("third"),
            context_token: third.0,
            interaction: InteractionMode::StickyClick,
        });
        let fourth = InvocationId(263);
        let intents = reducer.reduce(InvocationEvent::ExternalSessionAdmitted {
            id: fourth,
            menu_id: MenuId::new("fourth"),
            context_token: fourth.0,
            interaction: InteractionMode::StickyClick,
        });
        assert!(intents.is_empty());
        reducer.reduce(InvocationEvent::ExternalSessionOpened {
            id: third,
            session_id: SessionId::new("late-third"),
            menu_id: MenuId::new("third"),
            context_token: third.0,
            interaction: InteractionMode::StickyClick,
        });
        assert!(matches!(
            reducer.radial_lifecycle(),
            RadialLifecycle::Opening { id, .. } if *id == fourth
        ));
    }

    #[test]
    fn stale_open_and_close_feedback_cannot_rewrite_new_pending_cycle() {
        let mut reducer = InvocationReducer::default();
        reducer.reduce(press(220, 0, InteractionMode::StickyClick));
        let open = reducer.reduce(InvocationEvent::Deadline {
            id: InvocationId(220),
            at: 350,
            generation: 7,
        });
        assert!(matches!(
            open.as_slice(),
            [InvocationIntent::OpenRadial { .. }]
        ));
        reducer.reduce(InvocationEvent::PrimaryReleased {
            id: InvocationId(220),
            at: 351,
        });

        reducer.reduce(press(221, 500, InteractionMode::StickyClick));
        assert!(matches!(
            reducer.radial_lifecycle(),
            RadialLifecycle::Opening {
                id: InvocationId(220),
                ..
            }
        ));
        reducer.reduce(InvocationEvent::RadialSessionClosed {
            id: InvocationId(220),
        });
        assert!(matches!(reducer.state(), InvocationState::Pending { .. }));
        assert!(matches!(
            reducer.radial_lifecycle(),
            RadialLifecycle::Closed
        ));
        assert!(
            reducer
                .reduce(InvocationEvent::RadialSessionOpened {
                    id: InvocationId(220),
                    session_id: SessionId::new("late"),
                })
                .is_empty()
        );

        let intents = reducer.reduce(InvocationEvent::Deadline {
            id: InvocationId(221),
            at: 850,
            generation: 7,
        });
        assert!(
            intents.is_empty(),
            "the admitted close target was already retired"
        );
        assert!(
            reducer
                .reduce(InvocationEvent::PrimaryReleased {
                    id: InvocationId(221),
                    at: 851,
                })
                .is_empty()
        );
    }

    #[test]
    fn close_during_preparation_cancels_old_open_and_rejects_late_reply() {
        let mut reducer = InvocationReducer::default();
        reducer.reduce(press(230, 0, InteractionMode::StickyClick));
        reducer.reduce(InvocationEvent::Deadline {
            id: InvocationId(230),
            at: 350,
            generation: 7,
        });
        reducer.reduce(InvocationEvent::PrimaryReleased {
            id: InvocationId(230),
            at: 351,
        });
        assert!(matches!(
            reducer.radial_lifecycle(),
            RadialLifecycle::Opening { .. }
        ));

        reducer.reduce(press(231, 500, InteractionMode::StickyClick));
        let intents = reducer.reduce(InvocationEvent::Deadline {
            id: InvocationId(231),
            at: 850,
            generation: 7,
        });
        assert!(matches!(
            intents.as_slice(),
            [InvocationIntent::CancelRadialLifecycle {
                id: InvocationId(230),
                reason: LifecycleCancellation::SessionReplaced
            }]
        ));
        assert!(matches!(
            reducer.radial_lifecycle(),
            RadialLifecycle::Closed
        ));
        reducer.reduce(InvocationEvent::PrimaryReleased {
            id: InvocationId(231),
            at: 851,
        });
        reducer.reduce(InvocationEvent::RadialSessionOpened {
            id: InvocationId(230),
            session_id: SessionId::new("late-230"),
        });
        assert!(matches!(
            reducer.radial_lifecycle(),
            RadialLifecycle::Closed
        ));
    }

    #[test]
    fn close_request_during_opening_cancels_preparation_and_ignores_late_open() {
        let mut reducer = InvocationReducer::default();
        reducer.reduce(press(240, 0, InteractionMode::StickyClick));
        reducer.reduce(InvocationEvent::Deadline {
            id: InvocationId(240),
            at: 350,
            generation: 7,
        });
        let intents = reducer.reduce(InvocationEvent::RadialCloseRequested {
            id: InvocationId(240),
            session_id: None,
        });
        assert!(matches!(
            intents.as_slice(),
            [InvocationIntent::CancelRadialLifecycle {
                id: InvocationId(240),
                reason: LifecycleCancellation::SessionReplaced
            }]
        ));
        reducer.reduce(InvocationEvent::RadialSessionOpened {
            id: InvocationId(240),
            session_id: SessionId::new("late"),
        });
        assert!(matches!(
            reducer.radial_lifecycle(),
            RadialLifecycle::Closed
        ));
    }

    #[test]
    fn close_request_marks_active_session_closing_once() {
        let mut reducer = InvocationReducer::default();
        reducer.reduce(press(241, 0, InteractionMode::StickyClick));
        reducer.reduce(InvocationEvent::Deadline {
            id: InvocationId(241),
            at: 350,
            generation: 7,
        });
        let session_id = SessionId::new("active-241");
        reducer.reduce(InvocationEvent::RadialSessionOpened {
            id: InvocationId(241),
            session_id: session_id.clone(),
        });
        assert!(
            reducer
                .reduce(InvocationEvent::RadialCloseRequested {
                    id: InvocationId(241),
                    session_id: Some(session_id.clone()),
                })
                .is_empty()
        );
        assert!(matches!(
            reducer.radial_lifecycle(),
            RadialLifecycle::Closing {
                id: InvocationId(241),
                session_id: Some(actual)
            } if actual == &session_id
        ));
        assert!(
            reducer
                .reduce(InvocationEvent::RadialCloseRequested {
                    id: InvocationId(241),
                    session_id: Some(session_id),
                })
                .is_empty()
        );
    }

    #[test]
    fn physical_cycle_truth_table_keeps_taps_grid_only_and_holds_radial_only() {
        // Closed surface: a short cycle toggles only the legacy grid.
        let mut closed = InvocationReducer::default();
        closed.reduce(press(250, 0, InteractionMode::StickyClick));
        assert!(matches!(
            closed
                .reduce(InvocationEvent::PrimaryReleased {
                    id: InvocationId(250),
                    at: 1,
                })
                .as_slice(),
            [
                InvocationIntent::CancelDeadline { .. },
                InvocationIntent::ToggleLegacyLauncher { .. }
            ]
        ));
        assert!(matches!(closed.radial_lifecycle(), RadialLifecycle::Closed));

        // Opening surface: the next short cycle must not touch the pending
        // preparation; a held cycle cancels that preparation.
        let mut opening = InvocationReducer::default();
        opening.reduce(press(251, 0, InteractionMode::StickyClick));
        opening.reduce(InvocationEvent::Deadline {
            id: InvocationId(251),
            at: 350,
            generation: 7,
        });
        opening.reduce(InvocationEvent::PrimaryReleased {
            id: InvocationId(251),
            at: 351,
        });
        let before = opening.radial_lifecycle().clone();
        opening.reduce(press(252, 400, InteractionMode::StickyClick));
        assert!(matches!(
            opening
                .reduce(InvocationEvent::PrimaryReleased {
                    id: InvocationId(252),
                    at: 401,
                })
                .as_slice(),
            [
                InvocationIntent::CancelDeadline { .. },
                InvocationIntent::ToggleLegacyLauncher { .. }
            ]
        ));
        assert_eq!(opening.radial_lifecycle(), &before);
        opening.reduce(press(253, 500, InteractionMode::StickyClick));
        assert!(matches!(
            opening
                .reduce(InvocationEvent::Deadline {
                    id: InvocationId(253),
                    at: 850,
                    generation: 7,
                })
                .as_slice(),
            [InvocationIntent::CancelRadialLifecycle { .. }]
        ));

        // Active surface: a held cycle targets the existing session; its
        // short counterpart still toggles only the grid.
        let mut active = InvocationReducer::default();
        active.reduce(press(254, 0, InteractionMode::StickyClick));
        active.reduce(InvocationEvent::Deadline {
            id: InvocationId(254),
            at: 350,
            generation: 7,
        });
        let session = SessionId::new("truth-table-active");
        active.reduce(InvocationEvent::RadialSessionOpened {
            id: InvocationId(254),
            session_id: session.clone(),
        });
        active.reduce(InvocationEvent::PrimaryReleased {
            id: InvocationId(254),
            at: 351,
        });
        active.reduce(press(255, 400, InteractionMode::StickyClick));
        assert!(matches!(
            active
                .reduce(InvocationEvent::PrimaryReleased {
                    id: InvocationId(255),
                    at: 401,
                })
                .as_slice(),
            [
                InvocationIntent::CancelDeadline { .. },
                InvocationIntent::ToggleLegacyLauncher { .. }
            ]
        ));
        active.reduce(press(256, 500, InteractionMode::StickyClick));
        assert!(matches!(
            active
                .reduce(InvocationEvent::Deadline {
                    id: InvocationId(256),
                    at: 850,
                    generation: 7,
                })
                .as_slice(),
            [InvocationIntent::CloseRadial {
                session_id: Some(actual)
            }] if actual == &session
        ));

        // Closing surface: another long cycle is consumed but cannot issue a
        // duplicate close while native acknowledgement is outstanding.
        let mut closing = active;
        closing.reduce(InvocationEvent::PrimaryReleased {
            id: InvocationId(256),
            at: 851,
        });
        closing.reduce(press(257, 900, InteractionMode::StickyClick));
        assert!(
            closing
                .reduce(InvocationEvent::Deadline {
                    id: InvocationId(257),
                    at: 1_250,
                    generation: 7,
                })
                .is_empty()
        );
        assert!(
            closing
                .reduce(InvocationEvent::PrimaryReleased {
                    id: InvocationId(257),
                    at: 1_251,
                })
                .is_empty()
        );
    }
}
