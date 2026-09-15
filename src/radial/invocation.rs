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
    RadialSessionOpened {
        id: InvocationId,
        session_id: SessionId,
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
    },
    RadialActive {
        id: InvocationId,
        interaction: InteractionMode,
        session_id: Option<SessionId>,
        trigger_still_down: bool,
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
}

impl Default for InvocationReducer {
    fn default() -> Self {
        Self {
            state: InvocationState::Idle,
        }
    }
}

impl InvocationReducer {
    pub fn state(&self) -> &InvocationState {
        &self.state
    }

    pub fn reduce(&mut self, event: InvocationEvent) -> Vec<InvocationIntent> {
        use InvocationEvent as E;
        use InvocationIntent as I;
        match event {
            E::ExclusiveToolChanged { active: true } => {
                let (intents, owned_release) = self.cancel_for_exclusive_tool();
                self.state = InvocationState::SuppressedByExclusiveTool { owned_release };
                intents
            }
            E::ExclusiveToolChanged { active: false }
                if matches!(
                    self.state,
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
                self.state,
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
            } => match &self.state {
                InvocationState::Idle => {
                    let deadline = at.saturating_add(threshold_ms);
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
                    };
                    vec![I::ScheduleDeadline {
                        id,
                        at: deadline,
                        generation,
                    }]
                }
                InvocationState::RadialActive { session_id, .. } => {
                    let session_id = session_id.clone();
                    self.state = InvocationState::AwaitingOwnedRelease {
                        id,
                        reason: DrainReason::Dismissed,
                    };
                    vec![I::CloseRadial { session_id }]
                }
                _ => vec![],
            },
            E::ModifierChanged { .. } => vec![],
            E::Deadline { id, at, generation } => {
                let InvocationState::Pending {
                    id: pending,
                    deadline,
                    generation: pending_generation,
                    context_token,
                    menu_id,
                    interaction,
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
                let (context_token, menu_id, interaction) =
                    (*context_token, menu_id.clone(), *interaction);
                self.state = InvocationState::RadialActive {
                    id,
                    interaction,
                    session_id: None,
                    trigger_still_down: true,
                };
                vec![I::OpenRadial {
                    id,
                    menu_id,
                    context_token,
                    interaction,
                    trigger_still_down: true,
                }]
            }
            E::PrimaryReleased { id, at } => self.release(id, at),
            E::RadialSessionOpened { id, session_id } => {
                if let InvocationState::RadialActive {
                    id: active,
                    session_id: slot,
                    ..
                } = &mut self.state
                {
                    if *active == id {
                        *slot = Some(session_id);
                    }
                }
                vec![]
            }
            E::RadialClosedForAction { id } => {
                match &self.state {
                    InvocationState::RadialActive {
                        id: active,
                        trigger_still_down: true,
                        ..
                    } if *active == id => {
                        self.state = InvocationState::AwaitingOwnedRelease {
                            id,
                            reason: DrainReason::ActionHandoff,
                        }
                    }
                    InvocationState::RadialActive { id: active, .. } if *active == id => {
                        self.state = InvocationState::Idle
                    }
                    _ => {}
                }
                vec![]
            }
            E::RadialSessionClosed { id } => {
                match &self.state {
                    InvocationState::RadialActive {
                        id: active,
                        trigger_still_down: true,
                        ..
                    } if *active == id => {
                        self.state = InvocationState::AwaitingOwnedRelease {
                            id,
                            reason: DrainReason::Dismissed,
                        };
                    }
                    InvocationState::RadialActive { id: active, .. } if *active == id => {
                        self.state = InvocationState::Idle;
                    }
                    _ => {}
                }
                vec![]
            }
            _ => vec![],
        }
    }

    fn release(&mut self, id: InvocationId, at: Timestamp) -> Vec<InvocationIntent> {
        use InvocationIntent as I;
        match &self.state {
            InvocationState::Pending {
                id: pending,
                start,
                deadline,
                menu_id,
                context_token,
                interaction,
                ..
            } if *pending == id => {
                let (start, deadline, menu_id, context_token, interaction) = (
                    *start,
                    *deadline,
                    menu_id.clone(),
                    *context_token,
                    *interaction,
                );
                self.state = InvocationState::Idle;
                if at < start {
                    vec![I::CancelDeadline { id }]
                } else if at < deadline {
                    vec![I::CancelDeadline { id }, I::ToggleLegacyLauncher { id }]
                } else if interaction == InteractionMode::StickyClick {
                    self.state = InvocationState::RadialActive {
                        id,
                        interaction,
                        session_id: None,
                        trigger_still_down: false,
                    };
                    vec![
                        I::CancelDeadline { id },
                        I::OpenRadial {
                            id,
                            menu_id,
                            context_token,
                            interaction,
                            trigger_still_down: false,
                        },
                    ]
                } else {
                    vec![
                        I::CancelDeadline { id },
                        I::HoldCancelledBeforePresentation { id },
                    ]
                }
            }
            InvocationState::RadialActive {
                id: active,
                interaction,
                session_id,
                trigger_still_down: true,
                ..
            } if *active == id => {
                let interaction = *interaction;
                let session_id = session_id.clone();
                match interaction {
                    InteractionMode::StickyClick => {
                        if let InvocationState::RadialActive {
                            trigger_still_down, ..
                        } = &mut self.state
                        {
                            *trigger_still_down = false;
                        }
                        vec![I::TriggerReleased { id }]
                    }
                    InteractionMode::HoldAndClick => {
                        self.state = InvocationState::Idle;
                        vec![I::CloseRadial { session_id }]
                    }
                    InteractionMode::ReleaseToSelect => {
                        if let InvocationState::RadialActive {
                            trigger_still_down, ..
                        } = &mut self.state
                        {
                            *trigger_still_down = false;
                        }
                        vec![I::TriggerReleased { id }]
                    }
                }
            }
            InvocationState::AwaitingOwnedRelease { id: owned, reason } if *owned == id => {
                let acknowledge = *reason == DrainReason::ActionHandoff;
                self.state = InvocationState::Idle;
                acknowledge
                    .then_some(I::TriggerReleased { id })
                    .into_iter()
                    .collect()
            }
            InvocationState::SuppressedByExclusiveTool { owned_release }
                if *owned_release == Some(id) =>
            {
                self.state = InvocationState::SuppressedByExclusiveTool {
                    owned_release: None,
                };
                vec![]
            }
            _ => vec![],
        }
    }

    fn cancel_current(
        &mut self,
        reason: LifecycleCancellation,
        primary_still_down: bool,
    ) -> Vec<InvocationIntent> {
        let (id, deadline, radial_lifecycle) = match &self.state {
            InvocationState::Pending { id, .. } => (Some(*id), true, true),
            InvocationState::RadialActive { id, .. } => (Some(*id), false, true),
            InvocationState::AwaitingOwnedRelease { id, .. } => (Some(*id), false, false),
            InvocationState::SuppressedByExclusiveTool {
                owned_release: Some(id),
            } => (Some(*id), false, false),
            _ => (None, false, false),
        };
        let mut intents = Vec::new();
        if let Some(id) = id {
            if deadline {
                intents.push(InvocationIntent::CancelDeadline { id });
            }
            if radial_lifecycle {
                intents.push(InvocationIntent::CancelRadialLifecycle { id, reason });
            }
            self.state = if primary_still_down {
                InvocationState::AwaitingOwnedRelease {
                    id,
                    reason: DrainReason::LifecycleCancelled,
                }
            } else {
                InvocationState::Idle
            };
        } else {
            self.state = InvocationState::Idle;
        }
        intents
    }

    fn cancel_for_exclusive_tool(&self) -> (Vec<InvocationIntent>, Option<InvocationId>) {
        match &self.state {
            InvocationState::Pending {
                id, primary_down, ..
            } => (
                vec![InvocationIntent::CancelDeadline { id: *id }],
                if *primary_down { Some(*id) } else { None },
            ),
            InvocationState::RadialActive {
                id,
                session_id,
                trigger_still_down,
                ..
            } => (
                vec![InvocationIntent::CloseRadial {
                    session_id: session_id.clone(),
                }],
                if *trigger_still_down { Some(*id) } else { None },
            ),
            InvocationState::AwaitingOwnedRelease { id, .. } => (vec![], Some(*id)),
            InvocationState::SuppressedByExclusiveTool { owned_release } => {
                (vec![], *owned_release)
            }
            InvocationState::Idle => (vec![], None),
        }
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
    fn delayed_release_is_never_reclassified_as_tap() {
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
    fn dismiss_press_closes_and_drains_release() {
        let mut r = InvocationReducer::default();
        r.reduce(press(1, 0, InteractionMode::StickyClick));
        r.reduce(InvocationEvent::Deadline {
            id: InvocationId(1),
            at: 350,
            generation: 7,
        });
        let i = r.reduce(press(2, 400, InteractionMode::StickyClick));
        assert!(matches!(
            i.as_slice(),
            [InvocationIntent::CloseRadial { .. }]
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
            [
                InvocationIntent::CancelDeadline { .. },
                InvocationIntent::CancelRadialLifecycle { .. }
            ]
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
                [
                    InvocationIntent::CancelDeadline { id: cancelled },
                    InvocationIntent::CancelRadialLifecycle {
                        id: closed,
                        reason: actual,
                    }
                ] if *cancelled == id && *closed == id && *actual == reason
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
}
