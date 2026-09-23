//! Complete, timestamped lifecycle adapter for the launcher chord.
//! The pure adapter is also the synchronous decision core used by the native hook.
use super::{Hotkey, Key};
use crate::radial::acceptance_trace::{
    self, Event, HookDeadlineEdge, HookPriorityOwner, PrimaryTransition,
};
pub use crate::radial::invocation::InputProvenance;
use crate::radial::invocation::{
    ContextToken, InvocationEvent, InvocationIntent, InvocationReducer, LifecycleCancellation,
    RadialLifecycle, SettingsGeneration, Timestamp,
};
use crate::radial::model::{InteractionMode, InvocationId, MenuId, SessionId};
use std::sync::atomic::{AtomicU32, Ordering};

pub const MULTI_LAUNCHER_INJECT_TAG: usize = 0x004D_4C49_4E4A; // "MLINJ"

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ExclusiveOwner {
    ScreenDraw = 1,
    VisualSelection = 2,
    MacroPlayback = 4,
    MacroRecorder = 8,
}

static EXCLUSIVE_OWNERS: AtomicU32 = AtomicU32::new(0);
static EXCLUSIVE_WAKE: std::sync::OnceLock<std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>>> =
    std::sync::OnceLock::new();

pub fn install_exclusive_wake(wake: std::sync::mpsc::Sender<()>) {
    if let Ok(mut slot) = EXCLUSIVE_WAKE.get_or_init(Default::default).lock() {
        *slot = Some(wake);
    }
}

pub fn set_exclusive_owner(owner: ExclusiveOwner, active: bool) {
    let previous = if active {
        EXCLUSIVE_OWNERS.fetch_or(owner as u32, Ordering::AcqRel)
    } else {
        EXCLUSIVE_OWNERS.fetch_and(!(owner as u32), Ordering::AcqRel)
    };
    let current = exclusive_owner_transition(previous, owner, active);
    if current != previous
        && let Ok(slot) = EXCLUSIVE_WAKE.get_or_init(Default::default).lock()
        && let Some(wake) = slot.as_ref()
    {
        let _ = wake.send(());
    }
}

fn exclusive_owner_transition(previous: u32, owner: ExclusiveOwner, active: bool) -> u32 {
    if active {
        previous | owner as u32
    } else {
        previous & !(owner as u32)
    }
}

pub fn exclusive_owners() -> u32 {
    EXCLUSIVE_OWNERS.load(Ordering::Acquire)
}

pub fn classify_provenance(injected: bool, extra_info: usize) -> InputProvenance {
    let owned = extra_info == MULTI_LAUNCHER_INJECT_TAG
        || extra_info == crate::mkmacro::input::MKMACRO_EXTRA_INFO
        || extra_info == crate::mouse_gestures::service::MG_INJECT_TAG;
    if injected && owned {
        InputProvenance::SelfInjected
    } else if injected {
        InputProvenance::ExternalInjected
    } else {
        InputProvenance::Physical
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyTransition {
    Down,
    Repeat,
    Up,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub vk: u32,
    pub transition: KeyTransition,
    pub at: Timestamp,
    pub provenance: InputProvenance,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriorityOwner {
    Launcher,
    ScreenDrawRecovery,
    ExclusiveTool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelatedAction {
    Quit,
    Help,
    ScreenDrawLaunch,
    ScreenDrawEmergency,
    DirectMenu { trigger_id: String, menu_id: MenuId },
}
#[derive(Clone, Debug)]
pub struct RelatedBinding {
    pub hotkey: Hotkey,
    pub action: RelatedAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DirectCycleOwnership {
    id: InvocationId,
    primary_key: u32,
    provenance: InputProvenance,
}
impl DirectCycleOwnership {
    fn matches_release(self, event: KeyEvent) -> bool {
        event.transition == KeyTransition::Up
            && event.vk == self.primary_key
            && event.provenance == self.provenance
    }
}

#[derive(Clone, Debug)]
pub struct InvocationConfig {
    pub launcher_enabled: bool,
    pub hotkey: Hotkey,
    pub threshold_ms: u64,
    pub generation: SettingsGeneration,
    pub context_token: ContextToken,
    pub menu_id: MenuId,
    pub interaction: InteractionMode,
    pub accept_external_injected: bool,
    pub item_inputs: Vec<crate::radial::item_input::ItemInputBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterOutcome {
    pub consume: bool,
    pub recovery: bool,
    pub intents: Vec<InvocationIntent>,
}
impl AdapterOutcome {
    fn pass() -> Self {
        Self {
            consume: false,
            recovery: false,
            intents: vec![],
        }
    }
}

#[derive(Default, Clone, Copy, Debug)]
struct Modifiers {
    ctrl_left: bool,
    ctrl_right: bool,
    shift_left: bool,
    shift_right: bool,
    alt_left: bool,
    alt_right: bool,
    win_left: bool,
    win_right: bool,
}
impl Modifiers {
    fn update(&mut self, vk: u32, down: bool) -> bool {
        let slot = match vk {
            0xA2 => &mut self.ctrl_left,
            0xA3 => &mut self.ctrl_right,
            0xA0 => &mut self.shift_left,
            0xA1 => &mut self.shift_right,
            0xA4 => &mut self.alt_left,
            0xA5 => &mut self.alt_right,
            0x5B => &mut self.win_left,
            0x5C => &mut self.win_right,
            _ => return false,
        };
        *slot = down;
        true
    }
    fn matches(self, h: &Hotkey) -> bool {
        let any = self.ctrl_left
            || self.ctrl_right
            || self.shift_left
            || self.shift_right
            || self.alt_left
            || self.alt_right
            || self.win_left
            || self.win_right;
        if h.key == Key::CapsLock && !h.ctrl && !h.shift && !h.alt && !h.alt_gr && !h.win {
            return !any;
        }
        (!h.ctrl || self.ctrl_left || self.ctrl_right)
            && (!h.shift || self.shift_left || self.shift_right)
            && (!h.alt || self.alt_left || self.alt_right)
            && (!h.alt_gr || self.alt_right)
            && (!h.win || self.win_left || self.win_right)
    }
}

pub struct LauncherInvocationAdapter {
    config: InvocationConfig,
    reducer: InvocationReducer,
    modifiers: Modifiers,
    next_id: u64,
    owned: Option<InvocationId>,
    owned_primary: Option<u32>,
    owned_provenance: Option<InputProvenance>,
    owned_primary_down_suppressed: bool,
    candidate_primary_down: Option<InputProvenance>,
    recovery_primary: Option<RecoveryOwnership>,
    exclusive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RecoveryOwnership {
    primary: u32,
    provenance: InputProvenance,
    down_suppressed: bool,
}
impl LauncherInvocationAdapter {
    fn validate_config(config: &InvocationConfig) -> Result<(), String> {
        if config.launcher_enabled {
            vk_from_key(config.hotkey.key).ok_or_else(|| {
                "configured launcher primary key is unsupported by native adapter".to_string()
            })?;
        }
        Ok(())
    }

    pub fn new(config: InvocationConfig) -> Result<Self, String> {
        Self::validate_config(&config)?;
        Ok(Self {
            config,
            reducer: InvocationReducer::default(),
            modifiers: Modifiers::default(),
            next_id: 1,
            owned: None,
            owned_primary: None,
            owned_provenance: None,
            owned_primary_down_suppressed: false,
            candidate_primary_down: None,
            recovery_primary: None,
            exclusive: false,
        })
    }
    pub fn config(&self) -> &InvocationConfig {
        &self.config
    }
    pub fn radial_lifecycle(&self) -> &RadialLifecycle {
        self.reducer.radial_lifecycle()
    }
    pub fn feedback(&mut self, event: InvocationEvent) -> Vec<InvocationIntent> {
        self.reducer.reduce(event)
    }
    fn navigation_modifiers(&self) -> crate::radial::session::NavigationModifiers {
        crate::radial::session::NavigationModifiers {
            control: self.modifiers.ctrl_left || self.modifiers.ctrl_right,
            shift: self.modifiers.shift_left || self.modifiers.shift_right,
            alt: self.modifiers.alt_left || self.modifiers.alt_right,
            alt_gr: self.modifiers.alt_right && self.modifiers.ctrl_left,
        }
    }
    fn has_owned_cycle(&self) -> bool {
        self.owned.is_some() || self.recovery_primary.is_some()
    }
    fn is_exclusive(&self) -> bool {
        self.exclusive
    }
    pub fn reload(
        &mut self,
        config: InvocationConfig,
        reason: LifecycleCancellation,
    ) -> Result<Vec<InvocationIntent>, String> {
        Self::validate_config(&config)?;
        let intents = self.cancel_lifecycle(reason);
        self.config = config;
        Ok(intents)
    }
    pub fn set_exclusive(&mut self, active: bool) -> Vec<InvocationIntent> {
        self.exclusive = active;
        self.reducer
            .reduce(InvocationEvent::ExclusiveToolChanged { active })
    }
    pub fn cancel_lifecycle(&mut self, reason: LifecycleCancellation) -> Vec<InvocationIntent> {
        self.candidate_primary_down = None;
        let drain_release = reason.preserves_input_continuity() && self.owned.is_some();
        let intents = self.reducer.reduce(InvocationEvent::CancelLifecycle {
            reason,
            primary_still_down: drain_release,
        });
        if !reason.preserves_input_continuity() {
            self.owned = None;
            self.owned_primary = None;
            self.owned_provenance = None;
            self.owned_primary_down_suppressed = false;
            self.recovery_primary = None;
            self.modifiers = Modifiers::default();
        }
        intents
    }
    pub fn dismiss_active(&mut self) -> Vec<InvocationIntent> {
        if matches!(
            self.reducer.state(),
            crate::radial::invocation::InvocationState::RadialActive { .. }
        ) {
            self.reducer.reduce(InvocationEvent::CancelLifecycle {
                reason: LifecycleCancellation::SessionReplaced,
                primary_still_down: self.owned.is_some(),
            })
        } else {
            Vec::new()
        }
    }
    /// Retire the reducer's current radial lifecycle for a session-scoped
    /// close request.  This is separate from the native Escape ownership so a
    /// pending preparation can be cancelled even though it has no session id,
    /// and a pending physical close cannot later issue a duplicate close.
    pub fn request_radial_close(&mut self) -> Vec<InvocationIntent> {
        let Some(id) = self.reducer.radial_lifecycle().invocation_id() else {
            return Vec::new();
        };
        let session_id = self.reducer.radial_lifecycle().session_id();
        let was_active = matches!(
            self.reducer.radial_lifecycle(),
            RadialLifecycle::Active { .. }
        );
        let mut intents = self
            .reducer
            .reduce(InvocationEvent::RadialCloseRequested { id, session_id });
        if was_active {
            if let Some(session_id) = self.reducer.radial_lifecycle().session_id() {
                intents.push(InvocationIntent::CloseRadial {
                    session_id: Some(session_id),
                });
            }
        }
        intents
    }
    fn preempt(&mut self) -> Vec<InvocationIntent> {
        self.reducer.reduce(InvocationEvent::CancelLifecycle {
            reason: LifecycleCancellation::PriorityPreempted,
            primary_still_down: self.owned.is_some(),
        })
    }
    pub fn deadline(
        &mut self,
        id: InvocationId,
        at: Timestamp,
        generation: SettingsGeneration,
    ) -> Vec<InvocationIntent> {
        self.reducer
            .reduce(InvocationEvent::Deadline { id, at, generation })
    }
    pub fn process(&mut self, event: KeyEvent, owner: PriorityOwner) -> AdapterOutcome {
        if event.provenance == InputProvenance::SelfInjected
            || (event.provenance == InputProvenance::ExternalInjected
                && !self.config.accept_external_injected)
        {
            return AdapterOutcome::pass();
        }
        if let Some(ownership) = self
            .recovery_primary
            .filter(|ownership| ownership.primary == event.vk)
        {
            if event.provenance != ownership.provenance {
                return AdapterOutcome::pass();
            }
            if event.transition == KeyTransition::Up {
                self.recovery_primary = None;
                self.candidate_primary_down = None;
            }
            return AdapterOutcome {
                consume: ownership.down_suppressed,
                recovery: false,
                intents: Vec::new(),
            };
        }
        let configured_primary = self
            .owned_primary
            .or_else(|| vk_from_key(self.config.hotkey.key));
        if configured_primary == Some(event.vk)
            && let Some(transition) = acceptance_trace_primary_transition(event.transition)
        {
            acceptance_trace::emit(Event::InvocationPrimary {
                transition,
                provenance: event.provenance,
                modifiers_match: self.modifiers.matches(&self.config.hotkey),
                invocation_id: self.owned.map_or(self.next_id, |id| id.0),
                generation: self.config.generation,
            });
        }
        if self.owned.is_some() && self.owned_primary == Some(event.vk) {
            return if self.owned_provenance == Some(event.provenance) {
                self.process_owned_primary(event)
            } else {
                AdapterOutcome::pass()
            };
        }
        if !self.config.launcher_enabled {
            return AdapterOutcome::pass();
        }
        let primary = vk_from_key(self.config.hotkey.key).expect("validated adapter key");
        if event.vk == primary {
            self.candidate_primary_down = match event.transition {
                KeyTransition::Up => None,
                KeyTransition::Down | KeyTransition::Repeat => Some(event.provenance),
            };
        }
        let down = event.transition != KeyTransition::Up;
        if self.modifiers.update(event.vk, down) {
            if let Some(id) = self.owned {
                let intents = self
                    .reducer
                    .reduce(InvocationEvent::ModifierChanged { id, at: event.at });
                return AdapterOutcome {
                    consume: false,
                    recovery: false,
                    intents,
                };
            }
            if let Some(provenance) = self.candidate_primary_down
                && self.modifiers.matches(&self.config.hotkey)
            {
                let mut outcome = self.claim(event.at, primary, provenance, owner, false);
                // The modifier down was already delivered to the foreground app.
                outcome.consume = false;
                return outcome;
            }
            return AdapterOutcome::pass();
        }
        if event.vk != primary {
            return AdapterOutcome::pass();
        }
        match event.transition {
            KeyTransition::Up => AdapterOutcome::pass(),
            KeyTransition::Down | KeyTransition::Repeat => {
                if !self.modifiers.matches(&self.config.hotkey) {
                    return AdapterOutcome::pass();
                }
                self.claim(event.at, event.vk, event.provenance, owner, true)
            }
        }
    }

    fn claim(
        &mut self,
        at: Timestamp,
        primary: u32,
        provenance: InputProvenance,
        owner: PriorityOwner,
        primary_down_suppressed: bool,
    ) -> AdapterOutcome {
        if owner == PriorityOwner::ScreenDrawRecovery {
            self.recovery_primary = Some(RecoveryOwnership {
                primary,
                provenance,
                down_suppressed: primary_down_suppressed,
            });
            return AdapterOutcome {
                consume: primary_down_suppressed,
                recovery: true,
                intents: vec![],
            };
        }
        if owner == PriorityOwner::ExclusiveTool || self.exclusive {
            return AdapterOutcome::pass();
        }
        let id = InvocationId(self.next_id);
        self.next_id = self.next_id.checked_add(1).unwrap_or(1);
        self.owned = Some(id);
        self.owned_primary = Some(primary);
        self.owned_provenance = Some(provenance);
        self.owned_primary_down_suppressed = primary_down_suppressed;
        AdapterOutcome {
            consume: true,
            recovery: false,
            intents: self.reducer.reduce_with_provenance(
                InvocationEvent::ChordPressed {
                    id,
                    primary_key: primary,
                    at,
                    threshold_ms: self.config.threshold_ms,
                    generation: self.config.generation,
                    context_token: id.0,
                    menu_id: self.config.menu_id.clone(),
                    interaction: self.config.interaction,
                    repeat: false,
                },
                provenance,
            ),
        }
    }

    fn process_owned_primary(&mut self, event: KeyEvent) -> AdapterOutcome {
        let id = self.owned.expect("owned primary has invocation");
        if self.owned_provenance != Some(event.provenance) {
            return AdapterOutcome::pass();
        }
        if event.transition == KeyTransition::Up {
            let consume = self.owned_primary_down_suppressed;
            self.owned = None;
            self.owned_primary = None;
            self.owned_provenance = None;
            self.owned_primary_down_suppressed = false;
            self.candidate_primary_down = None;
            let intents = self
                .reducer
                .reduce(InvocationEvent::PrimaryReleased { id, at: event.at });
            if intents.iter().any(|intent| {
                matches!(
                    intent,
                    crate::radial::invocation::InvocationIntent::ToggleLegacyLauncher { .. }
                )
            }) {
                acceptance_trace::emit(Event::ShortTap {
                    invocation_id: id.0,
                    terminal: true,
                });
            }
            return AdapterOutcome {
                consume,
                recovery: false,
                intents,
            };
        }
        AdapterOutcome {
            consume: true,
            recovery: false,
            intents: self.reducer.reduce_with_provenance(
                InvocationEvent::ChordPressed {
                    id,
                    primary_key: event.vk,
                    at: event.at,
                    threshold_ms: self.config.threshold_ms,
                    generation: self.config.generation,
                    context_token: id.0,
                    menu_id: self.config.menu_id.clone(),
                    interaction: self.config.interaction,
                    repeat: true,
                },
                event.provenance,
            ),
        }
    }
}

fn acceptance_trace_primary_transition(transition: KeyTransition) -> Option<PrimaryTransition> {
    match transition {
        KeyTransition::Down => Some(PrimaryTransition::Press),
        KeyTransition::Up => Some(PrimaryTransition::Release),
        KeyTransition::Repeat => None,
    }
}

pub fn vk_from_key(key: Key) -> Option<u32> {
    use Key::*;
    Some(match key {
        F1 => 0x70,
        F2 => 0x71,
        F3 => 0x72,
        F4 => 0x73,
        F5 => 0x74,
        F6 => 0x75,
        F7 => 0x76,
        F8 => 0x77,
        F9 => 0x78,
        F10 => 0x79,
        F11 => 0x7A,
        F12 => 0x7B,
        F13 => 0x7C,
        F14 => 0x7D,
        F15 => 0x7E,
        F16 => 0x7F,
        F17 => 0x80,
        F18 => 0x81,
        F19 => 0x82,
        F20 => 0x83,
        F21 => 0x84,
        F22 => 0x85,
        F23 => 0x86,
        F24 => 0x87,
        KeyA => 0x41,
        KeyB => 0x42,
        KeyC => 0x43,
        KeyD => 0x44,
        KeyE => 0x45,
        KeyF => 0x46,
        KeyG => 0x47,
        KeyH => 0x48,
        KeyI => 0x49,
        KeyJ => 0x4A,
        KeyK => 0x4B,
        KeyL => 0x4C,
        KeyM => 0x4D,
        KeyN => 0x4E,
        KeyO => 0x4F,
        KeyP => 0x50,
        KeyQ => 0x51,
        KeyR => 0x52,
        KeyS => 0x53,
        KeyT => 0x54,
        KeyU => 0x55,
        KeyV => 0x56,
        KeyW => 0x57,
        KeyX => 0x58,
        KeyY => 0x59,
        KeyZ => 0x5A,
        Num0 => 0x30,
        Num1 => 0x31,
        Num2 => 0x32,
        Num3 => 0x33,
        Num4 => 0x34,
        Num5 => 0x35,
        Num6 => 0x36,
        Num7 => 0x37,
        Num8 => 0x38,
        Num9 => 0x39,
        Escape => 0x1B,
        Space => 0x20,
        Return => 0x0D,
        Tab => 0x09,
        Backspace => 0x08,
        Delete => 0x2E,
        Home => 0x24,
        End => 0x23,
        PageUp => 0x21,
        PageDown => 0x22,
        LeftArrow => 0x25,
        RightArrow => 0x27,
        UpArrow => 0x26,
        DownArrow => 0x28,
        CapsLock => 0x14,
        _ => return None,
    })
}

fn related_binding_eligible(down: &std::collections::HashSet<u32>, hotkey: &Hotkey) -> bool {
    let Some(primary) = vk_from_key(hotkey.key) else {
        return false;
    };
    if !down.contains(&primary) {
        return false;
    }
    let ctrl = down.contains(&0xA2) || down.contains(&0xA3);
    let shift = down.contains(&0xA0) || down.contains(&0xA1);
    let alt = down.contains(&0xA4) || down.contains(&0xA5);
    let win = down.contains(&0x5B) || down.contains(&0x5C);
    if hotkey.key == Key::CapsLock
        && !hotkey.ctrl
        && !hotkey.shift
        && !hotkey.alt
        && !hotkey.alt_gr
        && !hotkey.win
    {
        return !ctrl && !shift && !alt && !win;
    }
    (!hotkey.ctrl || ctrl)
        && (!hotkey.shift || shift)
        && (!hotkey.alt || alt)
        && (!hotkey.alt_gr || down.contains(&0xA5))
        && (!hotkey.win || win)
}

fn related_priority(action: &RelatedAction) -> u8 {
    match action {
        RelatedAction::ScreenDrawEmergency => 0,
        RelatedAction::Quit => 1,
        RelatedAction::ScreenDrawLaunch => 2,
        RelatedAction::Help => 3,
        RelatedAction::DirectMenu { .. } => 4,
    }
}

fn related_allowed(owner: PriorityOwner, action: &RelatedAction) -> bool {
    owner == PriorityOwner::Launcher
        || matches!(
            action,
            RelatedAction::Quit | RelatedAction::ScreenDrawEmergency
        )
}

fn command_generation_is_valid(sequence: u64, invalid_through: u64) -> bool {
    sequence > invalid_through
}

const POWER_BROADCAST_MESSAGE: u32 = 0x0218;
const SESSION_CHANGE_MESSAGE: u32 = 0x02B1;
const POWER_SUSPEND_EVENT: usize = 4;
const SESSION_LOCK_EVENT: usize = 7;

fn lifecycle_cancellation_for_message(
    message: u32,
    parameter: usize,
) -> Option<LifecycleCancellation> {
    match (message, parameter) {
        (POWER_BROADCAST_MESSAGE, POWER_SUSPEND_EVENT) => Some(LifecycleCancellation::Suspend),
        (SESSION_CHANGE_MESSAGE, SESSION_LOCK_EVENT) => Some(LifecycleCancellation::SessionLock),
        _ => None,
    }
}

#[derive(Default)]
struct EscapeOwnership {
    active_session: Option<(InvocationId, crate::radial::model::SessionId)>,
    owned_provenance: Option<InputProvenance>,
    owned_vk: Option<u32>,
}

impl EscapeOwnership {
    fn active_invocation(&self) -> Option<InvocationId> {
        self.active_session.as_ref().map(|(id, _)| *id)
    }

    fn cancel_session(&mut self) {
        // Retain an already claimed navigation/Escape key until its matching
        // release, but stop accepting any new session-scoped input.
        self.active_session = None;
    }

    fn abandon_owned_cycle(&mut self) {
        self.owned_provenance = None;
        self.owned_vk = None;
    }

    fn claim_without_session(&mut self, event: KeyEvent, accept_external_injected: bool) -> bool {
        if self.owned_provenance.is_some()
            || event.transition != KeyTransition::Down
            || event.provenance == InputProvenance::SelfInjected
            || (event.provenance == InputProvenance::ExternalInjected && !accept_external_injected)
        {
            return false;
        }
        self.owned_provenance = Some(event.provenance);
        self.owned_vk = Some(event.vk);
        true
    }

    fn feedback_correlated(&mut self, event: &InvocationEvent, lifecycle: &RadialLifecycle) {
        let current_id = lifecycle.invocation_id();
        match event {
            InvocationEvent::ExternalSessionAdmitted { id, .. } if current_id == Some(*id) => {
                // A replacement invalidates any native Escape lease from the
                // previous lifecycle while the new external session is still
                // preparing.  The matching Opened event installs ownership
                // again with the new session id.
                self.active_session = None;
            }
            InvocationEvent::RadialSessionOpened { id, session_id }
            | InvocationEvent::ExternalSessionOpened { id, session_id, .. }
                if matches!(
                    lifecycle,
                    RadialLifecycle::Active {
                        id: current,
                        session_id: current_session,
                        ..
                    } if current == id && current_session == session_id
                ) =>
            {
                self.active_session = Some((*id, session_id.clone()));
            }
            InvocationEvent::RadialSessionOpened { id, .. }
            | InvocationEvent::ExternalSessionOpened { id, .. }
            | InvocationEvent::ExternalSessionAdmitted { id, .. }
            | InvocationEvent::RadialSessionClosed { id }
            | InvocationEvent::RadialClosedForAction { id }
                if current_id != Some(*id)
                    && self
                        .active_session
                        .as_ref()
                        .is_some_and(|(active, _)| active == id) =>
            {
                // A stale lifecycle event may retire only its own Escape
                // ownership; it cannot install or clear ownership for the
                // newer lifecycle represented by the reducer.
                self.active_session = None;
            }
            InvocationEvent::RadialSessionClosed { id }
            | InvocationEvent::RadialClosedForAction { id }
                if current_id.is_none()
                    && self
                        .active_session
                        .as_ref()
                        .is_some_and(|(active, _)| active == id) =>
            {
                self.active_session = None;
            }
            _ => {}
        }
    }

    fn process_with_navigation(
        &mut self,
        event: KeyEvent,
        accept_external_injected: bool,
        modifiers: crate::radial::session::NavigationModifiers,
        allow_navigation: bool,
    ) -> Option<AdapterOutcome> {
        let navigation = match event.vk {
            0x25 | 0x26 => Some(crate::radial::session::NavigationCommand::Previous),
            0x27 | 0x28 => Some(crate::radial::session::NavigationCommand::Next),
            0x0D => Some(crate::radial::session::NavigationCommand::ActivatePrimary),
            0x08 => Some(crate::radial::session::NavigationCommand::Back),
            0x21 => Some(crate::radial::session::NavigationCommand::PreviousPage),
            0x22 => Some(crate::radial::session::NavigationCommand::NextPage),
            _ => None,
        };
        if event.vk != 0x1B && navigation.is_none() {
            return None;
        }
        if let Some(owned) = self.owned_provenance {
            if event.provenance != owned || self.owned_vk != Some(event.vk) {
                return Some(AdapterOutcome::pass());
            }
            if event.transition == KeyTransition::Up {
                self.owned_provenance = None;
                self.owned_vk = None;
            }
            return Some(AdapterOutcome {
                consume: true,
                recovery: false,
                intents: Vec::new(),
            });
        }
        if navigation.is_some() && !allow_navigation {
            return None;
        }
        if event.provenance == InputProvenance::SelfInjected
            || (event.provenance == InputProvenance::ExternalInjected && !accept_external_injected)
            || event.transition != KeyTransition::Down
        {
            return Some(AdapterOutcome::pass());
        }
        let (_, session_id) = self.active_session.clone()?;
        self.owned_provenance = Some(event.provenance);
        self.owned_vk = Some(event.vk);
        let intents = if let Some(command) = navigation {
            vec![InvocationIntent::Navigate {
                session_id,
                command,
                modifiers,
            }]
        } else {
            vec![InvocationIntent::CloseRadial {
                session_id: Some(session_id),
            }]
        };
        Some(AdapterOutcome {
            consume: true,
            recovery: false,
            intents,
        })
    }

    #[cfg(test)]
    fn process(
        &mut self,
        event: KeyEvent,
        accept_external_injected: bool,
        modifiers: crate::radial::session::NavigationModifiers,
    ) -> Option<AdapterOutcome> {
        self.process_with_navigation(event, accept_external_injected, modifiers, true)
    }
}

/// Apply the hook's Escape ownership boundary to one key transition.  Keeping
/// this routing seam outside the Windows callback lets tests exercise the same
/// opening/no-session path that native input uses, including release draining.
fn route_escape_event(
    escape: &mut EscapeOwnership,
    adapter: &mut LauncherInvocationAdapter,
    event: KeyEvent,
    accept_external_injected: bool,
    modifiers: crate::radial::session::NavigationModifiers,
    allow_navigation: bool,
) -> Option<AdapterOutcome> {
    let accepted_escape = event.vk == 0x1B
        && event.transition == KeyTransition::Down
        && event.provenance != InputProvenance::SelfInjected
        && (event.provenance != InputProvenance::ExternalInjected || accept_external_injected);
    let mut out = escape.process_with_navigation(
        event,
        accept_external_injected,
        modifiers,
        allow_navigation,
    );
    if accepted_escape {
        let lifecycle_intents = adapter.request_radial_close();
        if !lifecycle_intents.is_empty() {
            let mut routed = out.take().unwrap_or_else(AdapterOutcome::pass);
            if !routed.consume {
                routed.consume = escape.claim_without_session(event, accept_external_injected);
            }
            for intent in lifecycle_intents {
                if !routed.intents.contains(&intent) {
                    routed.intents.push(intent);
                }
            }
            out = Some(routed);
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct ServiceNotice {
    pub recovery: bool,
    pub intents: Vec<InvocationIntent>,
    pub error: Option<String>,
    pub action: Option<RelatedAction>,
    pub cancellation: Option<LifecycleCancellation>,
}

/// Completion of an input-route transition. Success is published only after
/// the hook applied the new admission policy and any previously claimed key
/// cycle has reached its matching release.
pub struct RouteHandoff {
    completion: std::sync::mpsc::Receiver<Result<(), String>>,
}

impl RouteHandoff {
    pub fn try_complete(&self) -> Option<Result<(), String>> {
        match self.completion.try_recv() {
            Ok(result) => Some(result),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(
                "launcher route handoff channel closed before acknowledgement".into(),
            )),
        }
    }
}

#[derive(Default)]
struct StartupCancellation {
    cancelled: std::sync::atomic::AtomicBool,
    worker_thread_id: std::sync::atomic::AtomicU32,
}

impl StartupCancellation {
    fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Release);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Acquire)
    }
}

fn cancellable_startup_install<T, E>(
    control: &StartupCancellation,
    install: impl FnOnce() -> Result<T, E>,
    cleanup: impl FnOnce(T),
) -> Result<Option<T>, E> {
    if control.is_cancelled() {
        return Ok(None);
    }
    let installed = install()?;
    if control.is_cancelled() {
        cleanup(installed);
        Ok(None)
    } else {
        Ok(Some(installed))
    }
}

#[derive(Default)]
struct PendingRouteHandoffs {
    acknowledgements: Vec<std::sync::mpsc::SyncSender<Result<(), String>>>,
}

impl PendingRouteHandoffs {
    fn applied(
        &mut self,
        owned_cycle: bool,
        acknowledgement: std::sync::mpsc::SyncSender<Result<(), String>>,
    ) {
        if owned_cycle {
            self.acknowledgements.push(acknowledgement);
        } else {
            let _ = acknowledgement.send(Ok(()));
        }
    }

    fn acknowledge_if_drained(&mut self, owned_cycle: bool) -> bool {
        if owned_cycle || self.acknowledgements.is_empty() {
            return false;
        }
        for acknowledgement in self.acknowledgements.drain(..) {
            let _ = acknowledgement.send(Ok(()));
        }
        true
    }
}

/// Owner of the one low-level launcher route. Ordinary hotkeys remain on the
/// legacy listener; this service exists only while shared tap/hold is enabled.
pub struct LauncherInvocationService {
    notices: std::sync::mpsc::Receiver<ServiceNotice>,
    #[cfg(all(windows, not(test)))]
    commands: std::sync::mpsc::Sender<(u64, ServiceCommand)>,
    #[cfg(all(windows, not(test)))]
    thread_id: u32,
    #[cfg(all(windows, not(test)))]
    join: Option<std::thread::JoinHandle<()>>,
    #[cfg(all(windows, not(test)))]
    reap_permit: Option<crate::thread_reaper::ReapPermit>,
    #[cfg(all(windows, not(test)))]
    stopped: std::sync::mpsc::Receiver<()>,
    #[cfg(all(windows, not(test)))]
    command_sequence: std::sync::atomic::AtomicU64,
    #[cfg(all(windows, not(test)))]
    invalid_commands: std::sync::Arc<std::sync::atomic::AtomicU64>,
    #[cfg(all(windows, not(test)))]
    lifecycle: std::sync::Arc<std::sync::atomic::AtomicU8>,
}

#[cfg(all(windows, not(test)))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum ServiceLifecycle {
    Running = 0,
    Stopping = 1,
    Stopped = 2,
}

#[cfg(all(windows, not(test)))]
impl ServiceLifecycle {
    fn load(value: &std::sync::atomic::AtomicU8) -> Self {
        match value.load(std::sync::atomic::Ordering::Acquire) {
            0 => Self::Running,
            1 => Self::Stopping,
            _ => Self::Stopped,
        }
    }
}

#[cfg(all(windows, not(test)))]
enum ServiceCommand {
    Feedback(InvocationEvent),
    Reload(
        InvocationConfig,
        Vec<RelatedBinding>,
        LifecycleCancellation,
        std::sync::mpsc::SyncSender<Result<(), String>>,
    ),
    Cancel(LifecycleCancellation),
    Exclusive(bool),
    ActiveMenu(Option<(SessionId, MenuId)>),
    Shutdown,
}

impl LauncherInvocationService {
    pub fn start(
        config: InvocationConfig,
        related: Vec<RelatedBinding>,
        owner: std::sync::Arc<dyn Fn() -> PriorityOwner + Send + Sync>,
        wake: std::sync::mpsc::Sender<()>,
    ) -> Result<Self, String> {
        #[cfg(all(windows, not(test)))]
        {
            return native_service::start(config, related, owner, wake);
        }
        #[cfg(any(not(windows), test))]
        {
            let _ = (config, related, owner, wake);
            Err("shared launcher invocation requires the Windows native backend".into())
        }
    }
    pub fn try_recv(&self) -> Option<ServiceNotice> {
        self.notices.try_recv().ok()
    }
    pub fn feedback(&self, event: InvocationEvent) -> Result<(), String> {
        #[cfg(all(windows, not(test)))]
        {
            return self.send_command(ServiceCommand::Feedback(event));
        }
        #[cfg(any(not(windows), test))]
        {
            let _ = event;
            Ok(())
        }
    }
    pub fn begin_route_handoff(
        &self,
        config: InvocationConfig,
        related: Vec<RelatedBinding>,
        reason: LifecycleCancellation,
    ) -> Result<RouteHandoff, String> {
        let (acknowledge, completion) = std::sync::mpsc::sync_channel(1);
        #[cfg(all(windows, not(test)))]
        {
            self.send_command(ServiceCommand::Reload(config, related, reason, acknowledge))?;
            return Ok(RouteHandoff { completion });
        }
        #[cfg(any(not(windows), test))]
        {
            let _ = (config, related, reason);
            let _ = acknowledge.send(Ok(()));
            Ok(RouteHandoff { completion })
        }
    }
    pub fn cancel_lifecycle(&self, reason: LifecycleCancellation) -> Result<(), String> {
        #[cfg(all(windows, not(test)))]
        {
            return self.send_command(ServiceCommand::Cancel(reason));
        }
        #[cfg(any(not(windows), test))]
        {
            let _ = reason;
            Ok(())
        }
    }
    pub fn set_exclusive(&self, active: bool) -> Result<(), String> {
        #[cfg(all(windows, not(test)))]
        {
            return self.send_command(ServiceCommand::Exclusive(active));
        }
        #[cfg(any(not(windows), test))]
        {
            let _ = active;
            Ok(())
        }
    }
    pub fn set_active_menu(&self, active: Option<(SessionId, MenuId)>) -> Result<(), String> {
        #[cfg(all(windows, not(test)))]
        {
            return self.send_command(ServiceCommand::ActiveMenu(active));
        }
        #[cfg(any(not(windows), test))]
        {
            let _ = active;
            Ok(())
        }
    }
    #[cfg(all(windows, not(test)))]
    fn send_command(&self, command: ServiceCommand) -> Result<(), String> {
        if ServiceLifecycle::load(&self.lifecycle) != ServiceLifecycle::Running {
            return Err("launcher invocation service is stopping".into());
        }
        self.send_command_internal(command)
    }
    #[cfg(all(windows, not(test)))]
    fn send_command_internal(&self, command: ServiceCommand) -> Result<(), String> {
        let generation = self
            .command_sequence
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            + 1;
        self.commands
            .send((generation, command))
            .map_err(|_| "launcher invocation command channel closed".to_string())?;
        let result = unsafe {
            windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                self.thread_id,
                native_service::WM_SERVICE_COMMAND,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(0),
            )
        }
        .map_err(|e| format!("failed to wake launcher invocation service: {e}"));
        if result.is_err() {
            self.invalid_commands
                .fetch_max(generation, std::sync::atomic::Ordering::AcqRel);
        }
        result
    }
    pub fn stop(&mut self) {
        #[cfg(all(windows, not(test)))]
        {
            if self.join.is_none() {
                return;
            }
            if ServiceLifecycle::load(&self.lifecycle) == ServiceLifecycle::Running {
                self.lifecycle.store(
                    ServiceLifecycle::Stopping as u8,
                    std::sync::atomic::Ordering::Release,
                );
                let _ = self.send_command_internal(ServiceCommand::Shutdown);
            }
            let stopped = self
                .stopped
                .recv_timeout(std::time::Duration::from_millis(1500))
                .is_ok();
            if let Some(j) = self.join.take() {
                if stopped || j.is_finished() {
                    let _ = j.join();
                    self.reap_permit.take();
                } else {
                    if let Err(error) = self
                        .reap_permit
                        .take()
                        .expect("live invocation service owns a reaper permit")
                        .reap(j)
                    {
                        eprintln!("launcher invocation cleanup degraded: {error}");
                    }
                }
            }
        }
    }
}
impl Drop for LauncherInvocationService {
    fn drop(&mut self) {
        self.stop()
    }
}

#[cfg(all(windows, not(test)))]
mod native_service {
    use super::*;
    use crate::radial::acceptance_trace::HookDesktop;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock, mpsc};
    use std::time::Instant;
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::RemoteDesktop::{
        NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
    };
    use windows::Win32::System::StationsAndDesktops::{
        GetThreadDesktop, GetUserObjectInformationW, UOI_NAME,
    };
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::{PCWSTR, w};
    pub(super) const WM_SERVICE_COMMAND: u32 = WM_APP + 0x53;
    pub(super) const WM_HOOK_PUMP_PROBE: u32 = WM_APP + 0x54;
    const LIFECYCLE_WINDOW_CLASS: PCWSTR = w!("MultiLauncherInvocationLifecycle");

    unsafe extern "system" fn lifecycle_window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if let Some(reason) = lifecycle_cancellation_for_message(message, wparam.0) {
            publish_lifecycle_cancellation(reason);
            return LRESULT(1);
        }
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    unsafe extern "system" fn desktop_switch_callback(
        _: HWINEVENTHOOK,
        _: u32,
        _: HWND,
        _: i32,
        _: i32,
        _: u32,
        _: u32,
    ) {
        publish_lifecycle_cancellation(LifecycleCancellation::DesktopUnavailable);
    }

    struct LifecycleWindow(HWND);

    impl Drop for LifecycleWindow {
        fn drop(&mut self) {
            let _ = unsafe { DestroyWindow(self.0) };
        }
    }

    struct SessionNotification(HWND);

    impl Drop for SessionNotification {
        fn drop(&mut self) {
            let _ = unsafe { WTSUnRegisterSessionNotification(self.0) };
        }
    }

    struct DesktopNotification(HWINEVENTHOOK);

    impl Drop for DesktopNotification {
        fn drop(&mut self) {
            let _ = unsafe { UnhookWinEvent(self.0) };
        }
    }

    struct KeyboardHook(windows::Win32::UI::WindowsAndMessaging::HHOOK);

    impl Drop for KeyboardHook {
        fn drop(&mut self) {
            let _ = unsafe { UnhookWindowsHookEx(self.0) };
        }
    }

    struct LifecycleSignals {
        // Field order is teardown order: stop callbacks, unregister the session
        // subscription, then retire the window that receives lifecycle messages.
        _desktop: DesktopNotification,
        _session: SessionNotification,
        _window: LifecycleWindow,
    }

    struct ServiceTimer(usize);

    impl ServiceTimer {
        fn cancel(mut self, expected_live: bool, context: &'static str) {
            kill_timer(self.0, expected_live, context);
            self.0 = 0;
        }
    }

    impl Drop for ServiceTimer {
        fn drop(&mut self) {
            if self.0 != 0 {
                kill_timer(self.0, true, "service timer guard drop");
                self.0 = 0;
            }
        }
    }

    struct HookCallbackTiming {
        started: Instant,
        primary: bool,
        down: bool,
        injected: bool,
    }

    impl HookCallbackTiming {
        fn new(transition: KeyTransition, injected: bool) -> Self {
            Self {
                started: Instant::now(),
                primary: false,
                down: transition != KeyTransition::Up,
                injected,
            }
        }
    }

    impl Drop for HookCallbackTiming {
        fn drop(&mut self) {
            acceptance_trace::emit(Event::HookCallback {
                primary: self.primary,
                down: self.down,
                injected: self.injected,
                elapsed_us: self.started.elapsed().as_micros().min(u64::MAX as u128) as u64,
            });
        }
    }

    impl LifecycleSignals {
        fn install() -> Result<Self, String> {
            static CLASS_REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();
            let module =
                unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None) }
                    .map_err(|error| format!("lifecycle module handle unavailable: {error}"))?;
            let instance: HINSTANCE = module.into();
            CLASS_REGISTERED
                .get_or_init(|| {
                    let class = WNDCLASSW {
                        hInstance: instance,
                        lpszClassName: LIFECYCLE_WINDOW_CLASS,
                        lpfnWndProc: Some(lifecycle_window_proc),
                        ..Default::default()
                    };
                    if unsafe { RegisterClassW(&class) } == 0 {
                        Err(format!(
                            "failed to register invocation lifecycle window: {}",
                            windows::core::Error::from_win32()
                        ))
                    } else {
                        Ok(())
                    }
                })
                .clone()?;
            let hwnd = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    LIFECYCLE_WINDOW_CLASS,
                    w!(""),
                    WINDOW_STYLE::default(),
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    instance,
                    None,
                )
            }
            .map_err(|error| format!("failed to create invocation lifecycle window: {error}"))?;
            let window = LifecycleWindow(hwnd);
            if let Err(error) =
                unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) }
            {
                return Err(format!(
                    "failed to register invocation session notifications: {error}"
                ));
            }
            let session = SessionNotification(hwnd);
            let desktop_hook = unsafe {
                SetWinEventHook(
                    EVENT_SYSTEM_DESKTOPSWITCH,
                    EVENT_SYSTEM_DESKTOPSWITCH,
                    None,
                    Some(desktop_switch_callback),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                )
            };
            if desktop_hook.0.is_null() {
                return Err(format!(
                    "failed to register invocation desktop notifications: {}",
                    windows::core::Error::from_win32()
                ));
            }
            Ok(Self {
                _desktop: DesktopNotification(desktop_hook),
                _session: session,
                _window: window,
            })
        }
    }
    struct State {
        adapter: LauncherInvocationAdapter,
        owner: Arc<dyn Fn() -> PriorityOwner + Send + Sync>,
        notices: mpsc::Sender<ServiceNotice>,
        wake: mpsc::Sender<()>,
        epoch: Instant,
        primary_down: bool,
        timers: HashMap<InvocationId, ServiceTimer>,
        related: Vec<(RelatedBinding, bool)>,
        down: std::collections::HashSet<u32>,
        commands: mpsc::Receiver<(u64, ServiceCommand)>,
        invalid_commands: Arc<std::sync::atomic::AtomicU64>,
        shutdown_requested: bool,
        escape: EscapeOwnership,
        pending_handoffs: PendingRouteHandoffs,
        next_direct_id: u64,
        direct_owned: Option<DirectCycleOwnership>,
        item_owned: Option<DirectCycleOwnership>,
        item_primary_suppressed: bool,
        item_inputs: Vec<crate::radial::item_input::ItemInputBinding>,
        item_recognizer: crate::radial::item_input::ItemInputRecognizer,
        active_frame: Option<(SessionId, MenuId)>,
        session_epoch: u64,
    }
    static STATE: OnceLock<Mutex<Option<State>>> = OnceLock::new();
    fn publish_lifecycle_cancellation(reason: LifecycleCancellation) {
        let Some(lock) = STATE.get() else { return };
        let Ok(mut guard) = lock.lock() else { return };
        let Some(state) = guard.as_mut() else { return };
        let intents = cancel_lifecycle(state, reason);
        let _ = state.notices.send(ServiceNotice {
            recovery: false,
            intents,
            error: None,
            action: None,
            cancellation: Some(reason),
        });
        acknowledge_handoffs_if_drained(state);
        let _ = state.wake.send(());
    }
    fn publish(state: &State, out: AdapterOutcome) {
        if out.recovery || !out.intents.is_empty() {
            let _ = state.notices.send(ServiceNotice {
                recovery: out.recovery,
                intents: out.intents,
                error: None,
                action: None,
                cancellation: None,
            });
            let _ = state.wake.send(());
        }
    }
    fn has_owned_input(state: &State) -> bool {
        state.adapter.has_owned_cycle()
            || state.escape.owned_provenance.is_some()
            || state.direct_owned.is_some()
            || state.item_owned.is_some()
    }
    fn cancel_lifecycle(state: &mut State, reason: LifecycleCancellation) -> Vec<InvocationIntent> {
        let mut intents = state.adapter.cancel_lifecycle(reason);
        for id in [
            state.escape.active_invocation(),
            state.direct_owned.map(|owned| owned.id),
            state.item_owned.map(|owned| owned.id),
        ]
        .into_iter()
        .flatten()
        {
            if !intents.iter().any(|intent| {
                matches!(intent, InvocationIntent::CancelRadialLifecycle { id: current, .. } if *current == id)
            }) {
                intents.push(InvocationIntent::CancelRadialLifecycle { id, reason });
            }
        }
        state.escape.cancel_session();
        if !reason.preserves_input_continuity() {
            state.escape.abandon_owned_cycle();
            state.direct_owned = None;
            state.item_owned = None;
            state.item_primary_suppressed = false;
            state.primary_down = false;
        }
        state.active_frame = None;
        state.session_epoch = state.session_epoch.wrapping_add(1);
        state.item_recognizer.clear();
        state.down.clear();
        for (_, latched) in &mut state.related {
            *latched = false;
        }
        let _ = update_timers(state, &intents);
        // A reducer owns at most one deadline, but drain defensively so an old
        // generation can never survive a lifecycle boundary.
        for (_, timer) in state.timers.drain() {
            timer.cancel(true, "lifecycle cancellation");
        }
        intents
    }
    unsafe extern "system" fn hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        if code < 0 {
            return unsafe { CallNextHookEx(None, code, w, l) };
        };
        let data = unsafe { &*(l.0 as *const KBDLLHOOKSTRUCT) };
        let transition = match w.0 as u32 {
            WM_KEYUP | WM_SYSKEYUP => KeyTransition::Up,
            WM_KEYDOWN | WM_SYSKEYDOWN => KeyTransition::Down,
            _ => return unsafe { CallNextHookEx(None, code, w, l) },
        };
        let injected = data.flags.contains(LLKHF_INJECTED);
        let mut callback_timing = HookCallbackTiming::new(transition, injected);
        if data.vkCode == 0x7A || data.vkCode == 0x87 {
            acceptance_trace::emit(Event::HookObserved {
                vk: data.vkCode,
                down: transition == KeyTransition::Down,
                injected: data.flags.contains(LLKHF_INJECTED),
            });
        }
        let Some(lock) = STATE.get() else {
            return unsafe { CallNextHookEx(None, code, w, l) };
        };
        let Ok(mut guard) = lock.lock() else {
            return unsafe { CallNextHookEx(None, code, w, l) };
        };
        let Some(state) = guard.as_mut() else {
            return unsafe { CallNextHookEx(None, code, w, l) };
        };
        let primary = vk_from_key(state.adapter.config().hotkey.key);
        callback_timing.primary = Some(data.vkCode) == primary;
        let transition = if Some(data.vkCode) == primary
            && transition == KeyTransition::Down
            && state.primary_down
        {
            KeyTransition::Repeat
        } else {
            transition
        };
        if Some(data.vkCode) == primary {
            state.primary_down = transition != KeyTransition::Up
        }
        let provenance = classify_provenance(injected, data.dwExtraInfo);
        if Some(data.vkCode) == primary
            && let Some(transition) = acceptance_trace_primary_transition(transition)
        {
            acceptance_trace::emit(Event::HookPrimary {
                transition,
                provenance,
                foreground_owner: acceptance_trace::foreground_owner(),
            });
        }
        let at = state.epoch.elapsed().as_millis() as u64;
        if state.item_owned.is_some() {
            state.item_recognizer.observe_owned_cycle_event(KeyEvent {
                vk: data.vkCode,
                transition,
                at,
                provenance,
            });
        }
        if state.item_owned.is_some_and(|owned| {
            owned.primary_key == data.vkCode
                && owned.provenance == provenance
                && transition != KeyTransition::Up
        }) {
            return if state.item_primary_suppressed {
                LRESULT(1)
            } else {
                unsafe { CallNextHookEx(None, code, w, l) }
            };
        }
        if transition == KeyTransition::Up
            && state.direct_owned.is_some_and(|owned| {
                owned.matches_release(KeyEvent {
                    vk: data.vkCode,
                    transition,
                    at,
                    provenance,
                })
            })
        {
            let id = state
                .direct_owned
                .take()
                .expect("matched direct ownership")
                .id;
            let _ = state.notices.send(ServiceNotice {
                recovery: false,
                intents: vec![InvocationIntent::TriggerReleased { id }],
                error: None,
                action: None,
                cancellation: None,
            });
            let _ = state.wake.send(());
        }
        if transition == KeyTransition::Up
            && state.item_owned.is_some_and(|owned| {
                owned.matches_release(KeyEvent {
                    vk: data.vkCode,
                    transition,
                    at,
                    provenance,
                })
            })
        {
            let id = state.item_owned.take().expect("matched item ownership").id;
            let consume = std::mem::take(&mut state.item_primary_suppressed);
            let _ = state.notices.send(ServiceNotice {
                recovery: false,
                intents: vec![InvocationIntent::TriggerReleased { id }],
                error: None,
                action: None,
                cancellation: None,
            });
            let _ = state.wake.send(());
            acknowledge_handoffs_if_drained(state);
            if state.shutdown_requested && !has_owned_input(state) {
                unsafe { PostQuitMessage(0) };
            }
            return if consume {
                LRESULT(1)
            } else {
                unsafe { CallNextHookEx(None, code, w, l) }
            };
        }
        let key_event = KeyEvent {
            vk: data.vkCode,
            transition,
            at,
            provenance,
        };
        let accept_external_injected = state.adapter.config().accept_external_injected;
        let allow_navigation = state.active_frame.is_some();
        let navigation_modifiers = state.adapter.navigation_modifiers();
        if let Some(out) = route_escape_event(
            &mut state.escape,
            &mut state.adapter,
            key_event,
            accept_external_injected,
            navigation_modifiers,
            allow_navigation,
        ) {
            let consume = out.consume;
            publish(state, out);
            if consume {
                return LRESULT(1);
            }
            return unsafe { CallNextHookEx(None, code, w, l) };
        }
        if state.shutdown_requested {
            let out = state.adapter.process(
                KeyEvent {
                    vk: data.vkCode,
                    transition,
                    at,
                    provenance,
                },
                PriorityOwner::ExclusiveTool,
            );
            let consume = out.consume;
            publish(state, out);
            acknowledge_handoffs_if_drained(state);
            if !has_owned_input(state) {
                unsafe { PostQuitMessage(0) };
            }
            return if consume {
                LRESULT(1)
            } else {
                unsafe { CallNextHookEx(None, code, w, l) }
            };
        }
        let current_owner = (state.owner)();
        let mut related_preempted = false;
        if provenance != InputProvenance::SelfInjected
            && (provenance != InputProvenance::ExternalInjected
                || state.adapter.config().accept_external_injected)
        {
            if transition == KeyTransition::Up {
                state.down.remove(&data.vkCode);
            } else {
                state.down.insert(data.vkCode);
            }
            let mut selected: Option<(u8, RelatedAction, u32)> = None;
            for (binding, latched) in &mut state.related {
                let eligible = related_binding_eligible(&state.down, &binding.hotkey);
                if eligible && !*latched {
                    let priority = related_priority(&binding.action);
                    if selected.as_ref().is_none_or(|(p, _, _)| priority < *p) {
                        if let Some(primary) = vk_from_key(binding.hotkey.key) {
                            selected = Some((priority, binding.action.clone(), primary));
                        }
                    }
                }
                *latched = eligible;
            }
            if let Some((_, action, related_primary)) =
                selected.filter(|(_, action, _)| related_allowed(current_owner, action))
            {
                let mut intents = state.adapter.preempt();
                let mut published_action = Some(action.clone());
                if let RelatedAction::DirectMenu { menu_id, .. } = &action {
                    let id = InvocationId(state.next_direct_id);
                    state.next_direct_id = state.next_direct_id.checked_add(1).unwrap_or(1_000_000);
                    state.direct_owned = Some(DirectCycleOwnership {
                        id,
                        primary_key: related_primary,
                        provenance,
                    });
                    intents.push(InvocationIntent::ToggleDirectMenu {
                        id,
                        menu_id: menu_id.clone(),
                        primary_key: related_primary,
                        provenance,
                        trigger_still_down: true,
                    });
                    published_action = None;
                }
                let _ = update_timers(state, &intents);
                let _ = state.notices.send(ServiceNotice {
                    recovery: false,
                    intents,
                    error: None,
                    action: published_action,
                    cancellation: None,
                });
                let _ = state.wake.send(());
                related_preempted = true;
            }
        }
        let owner = if related_preempted {
            PriorityOwner::ExclusiveTool
        } else {
            current_owner
        };
        state.item_recognizer.transition(
            unsafe { GetForegroundWindow() }.0 as isize,
            state.session_epoch,
            owner != PriorityOwner::Launcher,
        );
        if !related_preempted
            && owner == PriorityOwner::Launcher
            && state.item_owned.is_none()
            && let Some(matched) = state.item_recognizer.process(
                &state.item_inputs,
                state.active_frame.as_ref().map(|(_, menu)| menu),
                KeyEvent {
                    vk: data.vkCode,
                    transition,
                    at,
                    provenance,
                },
            )
        {
            let id = InvocationId(state.next_direct_id);
            state.next_direct_id = state.next_direct_id.checked_add(1).unwrap_or(1_000_000);
            let trigger_still_down = matched.primary_key.is_some();
            if let Some(primary_key) = matched.primary_key {
                state.item_owned = Some(DirectCycleOwnership {
                    id,
                    primary_key,
                    provenance: matched.provenance,
                });
                state.item_primary_suppressed = matched.consume_current;
            }
            let source = match matched.binding.trigger {
                crate::radial::item_input::ItemInputTrigger::Shortcut(_) => {
                    crate::commands::ActivationSource::RadialShortcut
                }
                crate::radial::item_input::ItemInputTrigger::Hotstring { .. } => {
                    crate::commands::ActivationSource::RadialHotstring
                }
            };
            let consume = matched.consume_current;
            let _ = state.notices.send(ServiceNotice {
                recovery: false,
                intents: vec![InvocationIntent::ActivateItem {
                    id,
                    menu_id: matched.binding.menu_id,
                    cell_id: matched.binding.cell_id,
                    gesture: matched.binding.gesture,
                    scope: matched.binding.scope,
                    source,
                    trigger_still_down,
                }],
                error: None,
                action: None,
                cancellation: None,
            });
            let _ = state.wake.send(());
            if consume {
                return LRESULT(1);
            }
        }
        let out = state.adapter.process(
            KeyEvent {
                vk: data.vkCode,
                transition,
                at,
                provenance,
            },
            owner,
        );
        if callback_timing.primary
            && let Some(primary_transition) = acceptance_trace_primary_transition(transition)
        {
            let deadline_scheduled = out
                .intents
                .iter()
                .any(|intent| matches!(intent, InvocationIntent::ScheduleDeadline { .. }));
            let radial_intent = out.intents.iter().any(|intent| {
                matches!(
                    intent,
                    InvocationIntent::OpenRadial { .. } | InvocationIntent::CloseRadial { .. }
                )
            });
            acceptance_trace::emit(Event::HookAdmission {
                transition: primary_transition,
                provenance,
                owner: match current_owner {
                    PriorityOwner::Launcher => HookPriorityOwner::Launcher,
                    PriorityOwner::ScreenDrawRecovery => HookPriorityOwner::ScreenDrawRecovery,
                    PriorityOwner::ExclusiveTool => HookPriorityOwner::ExclusiveTool,
                },
                global_exclusive_owners: super::exclusive_owners(),
                adapter_exclusive: state.adapter.is_exclusive(),
                recovery: out.recovery,
                deadline_scheduled,
                radial_intent,
            });
        }
        let timer_error = update_timers(state, &out.intents);
        if let Some(error) = timer_error {
            state.shutdown_requested = true;
            let cancel = cancel_lifecycle(state, LifecycleCancellation::HookFailure);
            let _ = state.notices.send(ServiceNotice {
                recovery: false,
                intents: cancel,
                error: Some(error),
                action: None,
                cancellation: Some(LifecycleCancellation::HookFailure),
            });
            let _ = state.wake.send(());
            unsafe { PostQuitMessage(0) };
        }
        let consume = out.consume;
        publish(state, out);
        acknowledge_handoffs_if_drained(state);
        if state.shutdown_requested && !has_owned_input(state) {
            unsafe { PostQuitMessage(0) };
        }
        if consume {
            LRESULT(1)
        } else {
            unsafe { CallNextHookEx(None, code, w, l) }
        }
    }
    fn update_timers(state: &mut State, intents: &[InvocationIntent]) -> Option<String> {
        for intent in intents {
            match *intent {
                InvocationIntent::ScheduleDeadline { id, at, .. } => {
                    if let Some(timer) = state.timers.remove(&id) {
                        timer.cancel(true, "deadline replacement");
                    }
                    let delay = at
                        .saturating_sub(state.epoch.elapsed().as_millis() as u64)
                        .clamp(1, u32::MAX as u64) as u32;
                    let timer = unsafe { SetTimer(None, 0, delay, None) };
                    if timer == 0 {
                        return Some(format!("failed to schedule radial deadline for {}", id.0));
                    }
                    state.timers.insert(id, ServiceTimer(timer));
                    acceptance_trace::emit(Event::HookDeadline {
                        edge: HookDeadlineEdge::Scheduled,
                        invocation_id: id.0,
                        timer_id: timer as u64,
                        delay_ms: delay as u64,
                        radial_intent: false,
                        global_exclusive_owners: super::exclusive_owners(),
                    });
                }
                InvocationIntent::CancelDeadline { id } => {
                    if let Some(timer) = state.timers.remove(&id) {
                        let timer_id = timer.0 as u64;
                        timer.cancel(true, "deadline cancellation");
                        acceptance_trace::emit(Event::HookDeadline {
                            edge: HookDeadlineEdge::Cancelled,
                            invocation_id: id.0,
                            timer_id,
                            delay_ms: 0,
                            radial_intent: false,
                            global_exclusive_owners: super::exclusive_owners(),
                        });
                    }
                }
                _ => {}
            }
        }
        None
    }
    fn kill_timer(timer: usize, expected_live: bool, context: &'static str) {
        if let Err(error) = unsafe { KillTimer(None, timer) } {
            if expected_live {
                tracing::warn!(timer, %error, context, "failed to cancel launcher timer");
            } else {
                tracing::debug!(timer, %error, context, "launcher timer was already absent");
            }
        }
    }
    fn acknowledge_handoffs_if_drained(state: &mut State) {
        if has_owned_input(state) {
            return;
        }
        if state
            .pending_handoffs
            .acknowledge_if_drained(has_owned_input(state))
        {
            let _ = state.wake.send(());
        }
    }
    pub fn start(
        config: InvocationConfig,
        related: Vec<RelatedBinding>,
        owner: Arc<dyn Fn() -> PriorityOwner + Send + Sync>,
        wake: mpsc::Sender<()>,
    ) -> Result<LauncherInvocationService, String> {
        let (tx, notices) = mpsc::channel();
        let (commands, command_rx) = mpsc::channel();
        let invalid_commands = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let worker_invalid_commands = Arc::clone(&invalid_commands);
        let (setup_tx, setup_rx) = mpsc::sync_channel(1);
        let (stopped_tx, stopped) = mpsc::sync_channel(1);
        let lifecycle = Arc::new(std::sync::atomic::AtomicU8::new(
            ServiceLifecycle::Running as u8,
        ));
        let worker_lifecycle = Arc::clone(&lifecycle);
        let startup = Arc::new(StartupCancellation::default());
        let worker_startup = Arc::clone(&startup);
        let reap_permit = crate::thread_reaper::reserve()
            .map_err(|error| format!("failed to reserve invocation worker: {error}"))?;
        let completion_notifier = reap_permit.completion_notifier();
        let join = std::thread::Builder::new()
            .name("launcher-invocation-hook".into())
            .spawn(move || {
                let _completion_notifier = completion_notifier;
                struct StoppedOnDrop {
                    stopped: Option<mpsc::SyncSender<()>>,
                    lifecycle: Arc<std::sync::atomic::AtomicU8>,
                }
                impl Drop for StoppedOnDrop {
                    fn drop(&mut self) {
                        self.lifecycle.store(
                            ServiceLifecycle::Stopped as u8,
                            std::sync::atomic::Ordering::Release,
                        );
                        if let Some(stopped) = self.stopped.take() {
                            let _ = stopped.send(());
                        }
                    }
                }
                let _stopped = StoppedOnDrop {
                    stopped: Some(stopped_tx),
                    lifecycle: worker_lifecycle,
                };
                let id = unsafe { GetCurrentThreadId() };
                worker_startup
                    .worker_thread_id
                    .store(id, std::sync::atomic::Ordering::Release);
                let mut queue_probe = MSG::default();
                let _ = unsafe { PeekMessageW(&mut queue_probe, None, 0, 0, PM_NOREMOVE) };
                if worker_startup.is_cancelled() {
                    return;
                }
                let lock = STATE.get_or_init(|| Mutex::new(None));
                let primary_vk = {
                    let Ok(mut slot) = lock.lock() else {
                        let _ = setup_tx.send(Err("launcher hook state poisoned".into()));
                        return;
                    };
                    if slot.is_some() {
                        let _ = setup_tx.send(Err("launcher hook already active".into()));
                        return;
                    }
                    let item_inputs = config.item_inputs.clone();
                    let primary_vk = vk_from_key(config.hotkey.key).unwrap_or_default();
                    let adapter = match LauncherInvocationAdapter::new(config) {
                        Ok(a) => a,
                        Err(e) => {
                            let _ = setup_tx.send(Err(e));
                            return;
                        }
                    };
                    *slot = Some(State {
                        adapter,
                        owner,
                        notices: tx,
                        wake,
                        epoch: Instant::now(),
                        primary_down: false,
                        timers: HashMap::new(),
                        related: related.into_iter().map(|v| (v, false)).collect(),
                        down: std::collections::HashSet::new(),
                        commands: command_rx,
                        invalid_commands: worker_invalid_commands,
                        shutdown_requested: false,
                        escape: EscapeOwnership::default(),
                        pending_handoffs: PendingRouteHandoffs::default(),
                        next_direct_id: 1_000_000,
                        direct_owned: None,
                        item_owned: None,
                        item_primary_suppressed: false,
                        item_inputs,
                        item_recognizer: Default::default(),
                        active_frame: None,
                        session_epoch: 0,
                    });
                    primary_vk
                };
                if worker_startup.is_cancelled() {
                    if let Ok(mut slot) = lock.lock() {
                        *slot = None;
                    }
                    return;
                }
                let lifecycle_signals = match LifecycleSignals::install() {
                    Ok(signals) => signals,
                    Err(error) => {
                        if let Ok(mut slot) = lock.lock() {
                            *slot = None;
                        }
                        let _ = setup_tx.send(Err(error));
                        return;
                    }
                };
                let hook_module = match unsafe { GetModuleHandleW(None) } {
                    Ok(module) => module,
                    Err(error) => {
                        if let Ok(mut slot) = lock.lock() {
                            *slot = None;
                        }
                        let _ = setup_tx
                            .send(Err(format!("launcher hook module lookup failed: {error}")));
                        return;
                    }
                };
                let hook = match cancellable_startup_install(
                    &worker_startup,
                    || unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), hook_module, 0) },
                    |hook| {
                        let _ = unsafe { UnhookWindowsHookEx(hook) };
                    },
                ) {
                    Ok(Some(h)) => h,
                    Ok(None) => {
                        if let Ok(mut s) = lock.lock() {
                            *s = None
                        }
                        return;
                    }
                    Err(e) => {
                        if let Ok(mut s) = lock.lock() {
                            *s = None
                        }
                        let _ = setup_tx.send(Err(format!("launcher hook install failed: {e}")));
                        return;
                    }
                };
                let _hook = KeyboardHook(hook);
                acceptance_trace::emit(Event::HookServiceReady {
                    thread_id: id,
                    desktop: hook_thread_desktop(),
                    primary_vk,
                });
                let _ = setup_tx.send(Ok(id));
                let mut msg = MSG::default();
                let message_result = loop {
                    let result = unsafe { GetMessageW(&mut msg, None, 0, 0) }.0;
                    if result <= 0 {
                        break result;
                    }
                    if msg.message == WM_HOOK_PUMP_PROBE {
                        acceptance_trace::emit(Event::HookPumpProbe {
                            probe_id: msg.wParam.0 as u64,
                        });
                    } else if msg.message == WM_SERVICE_COMMAND {
                        if let Ok(mut s) = lock.lock()
                            && let Some(state) = s.as_mut()
                        {
                            while let Ok((generation, command)) = state.commands.try_recv() {
                                if !command_generation_is_valid(
                                    generation,
                                    state
                                        .invalid_commands
                                        .load(std::sync::atomic::Ordering::Acquire),
                                ) {
                                    continue;
                                }
                                match command {
                                    ServiceCommand::Feedback(event) => {
                                        let intents = state.adapter.feedback(event.clone());
                                        state.escape.feedback_correlated(
                                            &event,
                                            state.adapter.radial_lifecycle(),
                                        );
                                        let _ = update_timers(state, &intents);
                                        let _ = state.notices.send(ServiceNotice {
                                            recovery: false,
                                            intents,
                                            error: None,
                                            action: None,
                                            cancellation: None,
                                        });
                                        let _ = state.wake.send(());
                                    }
                                    ServiceCommand::Reload(
                                        config,
                                        related,
                                        reason,
                                        acknowledgement,
                                    ) => {
                                        let item_inputs = config.item_inputs.clone();
                                        match LauncherInvocationAdapter::validate_config(&config) {
                                            Ok(()) => {
                                                let intents = cancel_lifecycle(state, reason);
                                                state.adapter.config = config;
                                                state.related = related
                                                    .into_iter()
                                                    .map(|binding| (binding, false))
                                                    .collect();
                                                state.down.clear();
                                                state.item_inputs = item_inputs;
                                                state.item_recognizer.clear();
                                                let _ = update_timers(state, &intents);
                                                let _ = state.notices.send(ServiceNotice {
                                                    recovery: false,
                                                    intents,
                                                    error: None,
                                                    action: None,
                                                    cancellation: Some(reason),
                                                });
                                                let _ = state.wake.send(());
                                                state.pending_handoffs.applied(
                                                    has_owned_input(state),
                                                    acknowledgement,
                                                );
                                            }
                                            Err(error) => {
                                                let _ = acknowledgement.send(Err(error.clone()));
                                                let _ = state.notices.send(ServiceNotice {
                                                    recovery: false,
                                                    intents: vec![],
                                                    error: Some(error),
                                                    action: None,
                                                    cancellation: None,
                                                });
                                                let _ = state.wake.send(());
                                            }
                                        }
                                    }
                                    ServiceCommand::Cancel(reason) => {
                                        let intents = cancel_lifecycle(state, reason);
                                        let _ = state.notices.send(ServiceNotice {
                                            recovery: false,
                                            intents,
                                            error: None,
                                            action: None,
                                            cancellation: Some(reason),
                                        });
                                        let _ = state.wake.send(());
                                        acknowledge_handoffs_if_drained(state);
                                    }
                                    ServiceCommand::Exclusive(active) => {
                                        if active {
                                            state.item_recognizer.clear();
                                        }
                                        let intents = state.adapter.set_exclusive(active);
                                        let _ = update_timers(state, &intents);
                                        let _ = state.notices.send(ServiceNotice {
                                            recovery: false,
                                            intents,
                                            error: None,
                                            action: None,
                                            cancellation: None,
                                        });
                                        let _ = state.wake.send(());
                                    }
                                    ServiceCommand::ActiveMenu(active) => {
                                        if state.active_frame != active {
                                            state.active_frame = active;
                                            state.session_epoch =
                                                state.session_epoch.wrapping_add(1);
                                            state.item_recognizer.clear();
                                        }
                                    }
                                    ServiceCommand::Shutdown => {
                                        state.shutdown_requested = true;
                                        let intents = cancel_lifecycle(
                                            state,
                                            LifecycleCancellation::Shutdown,
                                        );
                                        let _ = state.notices.send(ServiceNotice {
                                            recovery: false,
                                            intents,
                                            error: None,
                                            action: None,
                                            cancellation: Some(LifecycleCancellation::Shutdown),
                                        });
                                        let _ = state.wake.send(());
                                        if !has_owned_input(state) {
                                            unsafe { PostQuitMessage(0) }
                                        }
                                    }
                                }
                            }
                        }
                    } else if msg.message == WM_TIMER {
                        if let Ok(mut s) = lock.lock() {
                            if let Some(state) = s.as_mut() {
                                let timer = msg.wParam.0;
                                if let Some(id) = state
                                    .timers
                                    .iter()
                                    .find_map(|(id, actual)| (actual.0 == timer).then_some(*id))
                                {
                                    if let Some(timer) = state.timers.remove(&id) {
                                        timer.cancel(false, "one-shot timer fire");
                                    }
                                    let at = state.epoch.elapsed().as_millis() as u64;
                                    let generation = state.adapter.config().generation;
                                    let intents = state.adapter.deadline(id, at, generation);
                                    acceptance_trace::emit(Event::HookDeadline {
                                        edge: HookDeadlineEdge::Fired,
                                        invocation_id: id.0,
                                        timer_id: timer as u64,
                                        delay_ms: 0,
                                        radial_intent: intents.iter().any(|intent| {
                                            matches!(
                                                intent,
                                                InvocationIntent::OpenRadial { .. }
                                                    | InvocationIntent::CloseRadial { .. }
                                            )
                                        }),
                                        global_exclusive_owners: super::exclusive_owners(),
                                    });
                                    let _ = state.notices.send(ServiceNotice {
                                        recovery: false,
                                        intents,
                                        error: None,
                                        action: None,
                                        cancellation: None,
                                    });
                                    let _ = state.wake.send(());
                                }
                            }
                        }
                    } else if let Some(reason) =
                        lifecycle_cancellation_for_message(msg.message, msg.wParam.0)
                    {
                        publish_lifecycle_cancellation(reason);
                    } else {
                        let _ = unsafe { TranslateMessage(&msg) };
                        unsafe { DispatchMessageW(&msg) };
                    }
                };
                let (shutdown_requested, primary_down, owned_input, pending_deadlines) = lock
                    .lock()
                    .ok()
                    .and_then(|slot| {
                        slot.as_ref().map(|state| {
                            (
                                state.shutdown_requested,
                                state.primary_down,
                                has_owned_input(state),
                                state.timers.len(),
                            )
                        })
                    })
                    .unwrap_or_default();
                acceptance_trace::emit(Event::HookServiceExit {
                    message_result,
                    shutdown_requested,
                    primary_down,
                    owned_input,
                    pending_deadlines,
                });
                let unexpected_exit = lock
                    .lock()
                    .ok()
                    .and_then(|slot| slot.as_ref().map(|state| !state.shutdown_requested))
                    .unwrap_or(false);
                if unexpected_exit {
                    publish_lifecycle_cancellation(LifecycleCancellation::HookFailure);
                }
                drop(_hook);
                drop(lifecycle_signals);
                if let Ok(mut s) = lock.lock() {
                    if let Some(state) = s.as_mut() {
                        for (_, timer) in state.timers.drain() {
                            timer.cancel(true, "service teardown");
                        }
                    }
                    *s = None
                }
            })
            .map_err(|e| e.to_string())?;
        match setup_rx.recv_timeout(std::time::Duration::from_secs(2)) {
            Ok(Ok(thread_id)) => Ok(LauncherInvocationService {
                notices,
                commands,
                thread_id,
                join: Some(join),
                reap_permit: Some(reap_permit),
                stopped,
                command_sequence: std::sync::atomic::AtomicU64::new(0),
                invalid_commands,
                lifecycle,
            }),
            Ok(Err(e)) => {
                let _ = join.join();
                Err(e)
            }
            Err(_) => {
                startup.cancel();
                let thread_id = startup
                    .worker_thread_id
                    .load(std::sync::atomic::Ordering::Acquire);
                if thread_id != 0 {
                    let _ = unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
                }
                let cleaned = stopped
                    .recv_timeout(std::time::Duration::from_millis(1500))
                    .is_ok();
                let reaper_error = reap_permit.reap(join).err();
                let message = if cleaned {
                    "launcher hook readiness timed out and was cancelled".to_string()
                } else {
                    "launcher hook readiness timed out; cancellation is still draining".to_string()
                };
                Err(reaper_error.map_or(message.clone(), |error| format!("{message}; {error}")))
            }
        }
    }

    fn hook_thread_desktop() -> HookDesktop {
        let thread_id = unsafe { GetCurrentThreadId() };
        let Ok(desktop) = (unsafe { GetThreadDesktop(thread_id) }) else {
            return HookDesktop::Unknown;
        };
        let mut name = [0u16; 64];
        let mut needed = 0u32;
        if unsafe {
            GetUserObjectInformationW(
                windows::Win32::Foundation::HANDLE(desktop.0),
                UOI_NAME,
                Some(name.as_mut_ptr().cast()),
                std::mem::size_of_val(&name) as u32,
                Some(&mut needed),
            )
        }
        .is_err()
        {
            return HookDesktop::Unknown;
        }
        let length = name
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(name.len());
        if String::from_utf16_lossy(&name[..length]).eq_ignore_ascii_case("Default") {
            HookDesktop::Default
        } else {
            HookDesktop::Other
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cfg() -> InvocationConfig {
        InvocationConfig {
            launcher_enabled: true,
            hotkey: super::super::parse_hotkey("Shift+Alt+Win+End").unwrap(),
            threshold_ms: 350,
            generation: 4,
            context_token: 9,
            menu_id: MenuId::new("starter"),
            interaction: InteractionMode::StickyClick,
            accept_external_injected: true,
            item_inputs: Vec::new(),
        }
    }
    fn e(vk: u32, t: KeyTransition, at: u64) -> KeyEvent {
        KeyEvent {
            vk,
            transition: t,
            at,
            provenance: InputProvenance::Physical,
        }
    }

    fn external_e(vk: u32, t: KeyTransition, at: u64) -> KeyEvent {
        KeyEvent {
            vk,
            transition: t,
            at,
            provenance: InputProvenance::ExternalInjected,
        }
    }

    #[test]
    fn acceptance_trace_omits_primary_repeat_edges() {
        assert_eq!(
            acceptance_trace_primary_transition(KeyTransition::Down),
            Some(PrimaryTransition::Press)
        );
        assert_eq!(
            acceptance_trace_primary_transition(KeyTransition::Up),
            Some(PrimaryTransition::Release)
        );
        assert_eq!(
            acceptance_trace_primary_transition(KeyTransition::Repeat),
            None
        );
    }

    fn active_lifecycle(id: InvocationId, session_id: SessionId) -> RadialLifecycle {
        RadialLifecycle::Active {
            id,
            menu_id: MenuId::new("starter"),
            context_token: id.0,
            interaction: InteractionMode::StickyClick,
            session_id,
            trigger_still_down: false,
        }
    }
    #[test]
    fn complex_chord_owns_primary_down_repeat_and_release_not_modifiers() {
        let mut a = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            assert!(
                !a.process(e(vk, KeyTransition::Down, 1), PriorityOwner::Launcher)
                    .consume
            )
        }
        let d = a.process(e(0x23, KeyTransition::Down, 2), PriorityOwner::Launcher);
        assert!(
            d.consume
                && matches!(
                    d.intents.as_slice(),
                    [InvocationIntent::ScheduleDeadline { .. }]
                )
        );
        assert!(
            a.process(e(0x23, KeyTransition::Repeat, 3), PriorityOwner::Launcher)
                .consume
        );
        assert!(
            !a.process(e(0xA4, KeyTransition::Up, 4), PriorityOwner::Launcher)
                .consume
        );
        assert!(
            a.process(e(0x23, KeyTransition::Up, 5), PriorityOwner::Launcher)
                .consume
        )
    }
    #[test]
    fn tap_and_hold_are_timestamped_once() {
        let mut a = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            a.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let down = a.process(e(0x23, KeyTransition::Down, 10), PriorityOwner::Launcher);
        let id = match down.intents[0] {
            InvocationIntent::ScheduleDeadline { id, .. } => id,
            _ => panic!(),
        };
        assert!(matches!(
            a.process(e(0x23, KeyTransition::Up, 359), PriorityOwner::Launcher)
                .intents
                .as_slice(),
            [
                InvocationIntent::CancelDeadline { .. },
                InvocationIntent::ToggleLegacyLauncher { .. }
            ]
        ));
        let d = a.process(e(0x23, KeyTransition::Down, 500), PriorityOwner::Launcher);
        let id2 = match d.intents[0] {
            InvocationIntent::ScheduleDeadline { id, .. } => id,
            _ => panic!(),
        };
        assert_ne!(id, id2);
        assert!(matches!(
            a.deadline(id2, 850, 4).as_slice(),
            [InvocationIntent::OpenRadial { .. }]
        ));
    }

    #[test]
    fn exact_shift_alt_win_end_chord_uses_fake_time_for_tap_and_hold() {
        for provenance in [InputProvenance::Physical, InputProvenance::ExternalInjected] {
            let mut adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
            let event = |vk, transition, at| KeyEvent {
                vk,
                transition,
                at,
                provenance,
            };
            let modifiers = [0xA0, 0xA4, 0x5B]; // Shift, Alt, Windows.

            for (index, vk) in modifiers.iter().copied().enumerate() {
                assert!(
                    !adapter
                        .process(
                            event(vk, KeyTransition::Down, index as u64),
                            PriorityOwner::Launcher,
                        )
                        .consume
                );
            }
            let down_at = modifiers.len() as u64;
            let tap_down = adapter.process(
                event(0x23, KeyTransition::Down, down_at), // End.
                PriorityOwner::Launcher,
            );
            let tap_id = match tap_down.intents.as_slice() {
                [InvocationIntent::ScheduleDeadline { id, .. }] => *id,
                other => panic!("unexpected exact-chord tap intents: {other:?}"),
            };
            let repeat = adapter.process(
                event(0x23, KeyTransition::Repeat, down_at + 1),
                PriorityOwner::Launcher,
            );
            assert!(repeat.consume && repeat.intents.is_empty());

            let threshold = adapter.config().threshold_ms;
            let tap_release_at = down_at + threshold - 1;
            let tap_release = adapter.process(
                event(0x23, KeyTransition::Up, tap_release_at),
                PriorityOwner::Launcher,
            );
            assert!(matches!(
                tap_release.intents.as_slice(),
                [InvocationIntent::CancelDeadline { id: cancelled }, InvocationIntent::ToggleLegacyLauncher { id: toggled }]
                    if *cancelled == tap_id && *toggled == tap_id
            ));
            for (index, vk) in modifiers.iter().copied().rev().enumerate() {
                let release = adapter.process(
                    event(vk, KeyTransition::Up, tap_release_at + index as u64 + 1),
                    PriorityOwner::Launcher,
                );
                assert!(!release.consume && release.intents.is_empty());
            }
            assert!(!adapter.has_owned_cycle());

            // A full release immediately admits a new exact chord cycle. At
            // the configured deadline, the held End key produces one radial
            // open and its later release cannot turn into a grid tap.
            let hold_start = tap_release_at + 10;
            for (index, vk) in modifiers.iter().copied().enumerate() {
                adapter.process(
                    event(vk, KeyTransition::Down, hold_start + index as u64),
                    PriorityOwner::Launcher,
                );
            }
            let hold_down_at = hold_start + modifiers.len() as u64;
            let hold_down = adapter.process(
                event(0x23, KeyTransition::Down, hold_down_at),
                PriorityOwner::Launcher,
            );
            let hold_id = match hold_down.intents.as_slice() {
                [InvocationIntent::ScheduleDeadline { id, .. }] => *id,
                other => panic!("unexpected exact-chord hold intents: {other:?}"),
            };
            let hold = adapter.deadline(hold_id, hold_down_at + threshold, 4);
            assert!(matches!(
                hold.as_slice(),
                [InvocationIntent::OpenRadial { id, .. }] if *id == hold_id
            ));
            adapter.feedback(InvocationEvent::RadialSessionOpened {
                id: hold_id,
                session_id: SessionId::new("acceptance-session"),
            });
            let hold_release = adapter.process(
                event(0x23, KeyTransition::Up, hold_down_at + threshold + 1),
                PriorityOwner::Launcher,
            );
            assert!(matches!(
                hold_release.intents.as_slice(),
                [InvocationIntent::TriggerReleased { id }] if *id == hold_id
            ));
            assert!(
                !hold_release
                    .intents
                    .iter()
                    .any(|intent| matches!(intent, InvocationIntent::ToggleLegacyLauncher { .. }))
            );
            for (index, vk) in modifiers.iter().copied().rev().enumerate() {
                adapter.process(
                    event(
                        vk,
                        KeyTransition::Up,
                        hold_down_at + threshold + index as u64 + 2,
                    ),
                    PriorityOwner::Launcher,
                );
            }
            assert!(!adapter.has_owned_cycle());
        }
    }
    #[test]
    fn delayed_release_to_select_cancels_without_opening_a_late_surface() {
        let mut config = cfg();
        config.interaction = InteractionMode::ReleaseToSelect;
        let mut adapter = LauncherInvocationAdapter::new(config).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            adapter.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let down = adapter.process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher);
        let id = match down.intents.as_slice() {
            [InvocationIntent::ScheduleDeadline { id, .. }] => *id,
            _ => unreachable!(),
        };
        let release = adapter.process(e(0x23, KeyTransition::Up, 900), PriorityOwner::Launcher);
        assert!(release.consume);
        assert!(matches!(
            release.intents.as_slice(),
            [
                InvocationIntent::CancelDeadline { id: cancelled },
                InvocationIntent::HoldCancelledBeforePresentation { id: cancelled_hold }
            ] if *cancelled == id && *cancelled_hold == id
        ));
        assert!(matches!(
            adapter.radial_lifecycle(),
            RadialLifecycle::Closed
        ));
    }
    #[test]
    fn consecutive_shared_holds_receive_unique_correlated_context_tokens() {
        let mut adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            adapter.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let first = adapter.process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher);
        let first_id = match first.intents[0] {
            InvocationIntent::ScheduleDeadline { id, .. } => id,
            _ => panic!(),
        };
        let first_token = match adapter.deadline(first_id, 351, 4).as_slice() {
            [InvocationIntent::OpenRadial { context_token, .. }] => *context_token,
            _ => panic!(),
        };
        let first_session = SessionId::new("first-session");
        adapter.feedback(InvocationEvent::RadialSessionOpened {
            id: first_id,
            session_id: first_session.clone(),
        });
        adapter.process(e(0x23, KeyTransition::Up, 400), PriorityOwner::Launcher);
        assert!(matches!(
            adapter.request_radial_close().as_slice(),
            [InvocationIntent::CloseRadial {
                session_id: Some(actual)
            }] if actual == &first_session
        ));
        adapter.feedback(InvocationEvent::RadialSessionClosed { id: first_id });
        let second = adapter.process(e(0x23, KeyTransition::Down, 500), PriorityOwner::Launcher);
        let second_id = match second.intents[0] {
            InvocationIntent::ScheduleDeadline { id, .. } => id,
            _ => panic!(),
        };
        let second_token = match adapter.deadline(second_id, 850, 4).as_slice() {
            [InvocationIntent::OpenRadial { context_token, .. }] => *context_token,
            _ => panic!(),
        };
        assert_eq!(first_token, first_id.0);
        assert_eq!(second_token, second_id.0);
        assert_ne!(first_token, second_token);
    }
    #[test]
    fn reload_invalidates_old_generation_and_drains_release() {
        let mut a = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            a.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let d = a.process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher);
        let id = match d.intents[0] {
            InvocationIntent::ScheduleDeadline { id, .. } => id,
            _ => panic!(),
        };
        let mut c = cfg();
        c.generation = 5;
        assert!(matches!(
            a.reload(c, LifecycleCancellation::SettingsReload)
                .unwrap()
                .as_slice(),
            [InvocationIntent::CancelDeadline { .. }]
        ));
        assert!(a.deadline(id, 351, 4).is_empty());
        assert!(
            a.process(e(0x23, KeyTransition::Up, 4), PriorityOwner::Launcher)
                .consume
        )
    }
    #[test]
    fn disabling_admission_on_reload_retains_the_consumed_release() {
        let mut adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            adapter.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        assert!(
            adapter
                .process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher)
                .consume
        );
        let mut disabled = cfg();
        disabled.launcher_enabled = false;
        disabled.generation += 1;
        adapter
            .reload(disabled, LifecycleCancellation::FeatureDisabled)
            .unwrap();
        assert!(
            adapter
                .process(e(0x23, KeyTransition::Up, 3), PriorityOwner::Launcher)
                .consume
        );
        assert!(
            !adapter
                .process(e(0x23, KeyTransition::Down, 4), PriorityOwner::Launcher)
                .consume
        );
    }
    #[test]
    fn direct_only_mode_never_claims_the_legacy_launcher_chord() {
        let mut config = cfg();
        config.launcher_enabled = false;
        let mut adapter = LauncherInvocationAdapter::new(config).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            assert!(
                !adapter
                    .process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher)
                    .consume
            );
        }
        assert!(
            !adapter
                .process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher)
                .consume
        );
        assert!(
            !adapter
                .process(e(0x23, KeyTransition::Up, 2), PriorityOwner::Launcher)
                .consume
        );
    }
    #[test]
    fn provenance_and_screen_draw_priority_are_explicit() {
        let mut a = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            a.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let mut own = e(0x23, KeyTransition::Down, 1);
        own.provenance = InputProvenance::SelfInjected;
        assert!(!a.process(own, PriorityOwner::Launcher).consume);
        let recovery = a.process(
            e(0x23, KeyTransition::Down, 2),
            PriorityOwner::ScreenDrawRecovery,
        );
        assert!(recovery.recovery && recovery.consume && recovery.intents.is_empty());
        let mut external = e(0x23, KeyTransition::Down, 3);
        external.provenance = InputProvenance::ExternalInjected;
        assert!(
            !a.process(external, PriorityOwner::Launcher).consume,
            "a different provenance cannot enter the physical recovery cycle"
        )
    }
    #[test]
    fn screen_draw_recovery_is_one_shot_and_drains_each_accepted_cycle() {
        let mut adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            adapter.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let first = adapter.process(
            e(0x23, KeyTransition::Down, 1),
            PriorityOwner::ScreenDrawRecovery,
        );
        assert!(first.recovery && first.consume);
        let mut external_up = e(0x23, KeyTransition::Up, 2);
        external_up.provenance = InputProvenance::ExternalInjected;
        let unrelated_up = adapter.process(external_up, PriorityOwner::ScreenDrawRecovery);
        assert!(!unrelated_up.recovery && !unrelated_up.consume);
        let mut external_repeat = e(0x23, KeyTransition::Repeat, 3);
        external_repeat.provenance = InputProvenance::ExternalInjected;
        let unrelated_repeat = adapter.process(external_repeat, PriorityOwner::ScreenDrawRecovery);
        assert!(!unrelated_repeat.recovery && !unrelated_repeat.consume);
        let repeat = adapter.process(
            e(0x23, KeyTransition::Repeat, 4),
            PriorityOwner::ScreenDrawRecovery,
        );
        assert!(!repeat.recovery && repeat.consume);
        let release = adapter.process(
            e(0x23, KeyTransition::Up, 5),
            PriorityOwner::ScreenDrawRecovery,
        );
        assert!(!release.recovery && release.consume);
        let second = adapter.process(
            e(0x23, KeyTransition::Down, 6),
            PriorityOwner::ScreenDrawRecovery,
        );
        assert!(second.recovery && second.consume);

        let mut self_injected = e(0x23, KeyTransition::Down, 7);
        self_injected.provenance = InputProvenance::SelfInjected;
        let mut provenance_adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            provenance_adapter.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let ignored = provenance_adapter.process(self_injected, PriorityOwner::ScreenDrawRecovery);
        assert!(!ignored.recovery && !ignored.consume);

        let mut rejected_config = cfg();
        rejected_config.accept_external_injected = false;
        let mut rejected_external = LauncherInvocationAdapter::new(rejected_config).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            rejected_external.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let mut external = e(0x23, KeyTransition::Down, 8);
        external.provenance = InputProvenance::ExternalInjected;
        let ignored = rejected_external.process(external, PriorityOwner::ScreenDrawRecovery);
        assert!(!ignored.recovery && !ignored.consume);

        let mut externally_owned = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            externally_owned.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let mut external_down = e(0x23, KeyTransition::Down, 9);
        external_down.provenance = InputProvenance::ExternalInjected;
        assert!(
            externally_owned
                .process(external_down, PriorityOwner::ScreenDrawRecovery)
                .recovery
        );
        for transition in [KeyTransition::Repeat, KeyTransition::Up] {
            let physical = externally_owned
                .process(e(0x23, transition, 10), PriorityOwner::ScreenDrawRecovery);
            assert!(!physical.recovery && !physical.consume);
        }
        let mut external_up = e(0x23, KeyTransition::Up, 11);
        external_up.provenance = InputProvenance::ExternalInjected;
        assert!(
            externally_owned
                .process(external_up, PriorityOwner::ScreenDrawRecovery)
                .consume
        );
    }
    #[test]
    fn route_handoff_waits_for_old_release_and_new_admission_is_unambiguous() {
        let mut adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            adapter.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        assert!(
            adapter
                .process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher)
                .consume
        );
        let mut direct_only = cfg();
        direct_only.launcher_enabled = false;
        adapter
            .reload(direct_only, LifecycleCancellation::SettingsReload)
            .unwrap();
        assert!(
            adapter.has_owned_cycle(),
            "legacy route must still be deferred"
        );
        assert!(
            adapter
                .process(e(0x23, KeyTransition::Up, 3), PriorityOwner::Launcher)
                .consume
        );
        assert!(
            !adapter.has_owned_cycle(),
            "handoff can now be acknowledged"
        );
        assert!(
            !adapter
                .process(e(0x23, KeyTransition::Down, 4), PriorityOwner::Launcher)
                .consume,
            "a fresh chord belongs only to the newly admitted legacy route"
        );

        adapter
            .reload(cfg(), LifecycleCancellation::SettingsReload)
            .unwrap();
        assert!(!adapter.has_owned_cycle());
        assert!(
            adapter
                .process(e(0x23, KeyTransition::Down, 6), PriorityOwner::Launcher)
                .consume,
            "after legacy retirement a fresh chord belongs only to the hook"
        );
    }
    #[test]
    fn route_handoff_ack_is_withheld_until_claimed_cycle_drains() {
        let mut pending = PendingRouteHandoffs::default();
        let (acknowledge, completion) = std::sync::mpsc::sync_channel(1);
        pending.applied(true, acknowledge);
        assert!(matches!(
            completion.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        assert!(!pending.acknowledge_if_drained(true));
        assert!(pending.acknowledge_if_drained(false));
        assert!(matches!(completion.try_recv(), Ok(Ok(()))));
    }
    #[test]
    fn delayed_start_cancellation_clears_ownership_and_allows_restart() {
        use std::sync::atomic::AtomicBool;
        let active = std::sync::Arc::new(AtomicBool::new(false));
        let cancellation = std::sync::Arc::new(StartupCancellation::default());
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
        let (resume_tx, resume_rx) = std::sync::mpsc::sync_channel(1);
        let install_active = std::sync::Arc::clone(&active);
        let cleanup_active = std::sync::Arc::clone(&active);
        let worker_cancel = std::sync::Arc::clone(&cancellation);
        let delayed = std::thread::spawn(move || {
            cancellable_startup_install(
                &worker_cancel,
                || {
                    entered_tx.send(()).unwrap();
                    resume_rx.recv().unwrap();
                    install_active.store(true, std::sync::atomic::Ordering::Release);
                    Ok::<_, ()>(())
                },
                |_| cleanup_active.store(false, std::sync::atomic::Ordering::Release),
            )
            .unwrap()
        });
        entered_rx.recv().unwrap();
        cancellation.cancel();
        resume_tx.send(()).unwrap();
        assert!(delayed.join().unwrap().is_none());
        assert!(!active.load(std::sync::atomic::Ordering::Acquire));

        let restart = StartupCancellation::default();
        let installed = cancellable_startup_install(
            &restart,
            || {
                active.store(true, std::sync::atomic::Ordering::Release);
                Ok::<_, ()>(())
            },
            |_| active.store(false, std::sync::atomic::Ordering::Release),
        )
        .unwrap();
        assert!(installed.is_some());
        assert!(active.load(std::sync::atomic::Ordering::Acquire));
        active.store(false, std::sync::atomic::Ordering::Release);
    }
    #[test]
    fn modifier_repeat_does_not_leave_a_phantom_modifier_after_release() {
        let mut a = LauncherInvocationAdapter::new(cfg()).unwrap();
        a.process(e(0xA0, KeyTransition::Down, 0), PriorityOwner::Launcher);
        a.process(e(0xA0, KeyTransition::Repeat, 1), PriorityOwner::Launcher);
        a.process(e(0xA0, KeyTransition::Up, 2), PriorityOwner::Launcher);
        for vk in [0xA4, 0x5B] {
            a.process(e(vk, KeyTransition::Down, 3), PriorityOwner::Launcher);
        }
        assert!(
            !a.process(e(0x23, KeyTransition::Down, 4), PriorityOwner::Launcher)
                .consume
        );
    }
    #[test]
    fn exclusive_tool_cancels_pending_deadline() {
        let mut a = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            a.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        a.process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher);
        assert!(matches!(
            a.set_exclusive(true).as_slice(),
            [InvocationIntent::CancelDeadline { .. }]
        ));
    }
    #[test]
    fn primary_first_becomes_eligible_on_last_modifier_without_consuming_it() {
        let mut a = LauncherInvocationAdapter::new(cfg()).unwrap();
        assert!(
            !a.process(e(0x23, KeyTransition::Down, 0), PriorityOwner::Launcher)
                .consume
        );
        a.process(e(0xA0, KeyTransition::Down, 1), PriorityOwner::Launcher);
        a.process(e(0xA4, KeyTransition::Down, 2), PriorityOwner::Launcher);
        let outcome = a.process(e(0x5B, KeyTransition::Down, 3), PriorityOwner::Launcher);
        assert!(!outcome.consume);
        assert!(matches!(
            outcome.intents.as_slice(),
            [InvocationIntent::ScheduleDeadline { .. }]
        ));
        assert!(
            !a.process(e(0x23, KeyTransition::Up, 4), PriorityOwner::Launcher)
                .consume,
            "a primary down delivered before the modifiers requires a delivered up"
        );
    }
    #[test]
    fn bare_caps_lock_is_exact_and_altgr_is_right_alt_only() {
        let caps = super::super::parse_hotkey("CapsLock").unwrap();
        let mut down = std::collections::HashSet::from([0x14]);
        assert!(related_binding_eligible(&down, &caps));
        down.insert(0xA0);
        assert!(!related_binding_eligible(&down, &caps));
        let altgr = super::super::parse_hotkey("AltGr+End").unwrap();
        let mut right = std::collections::HashSet::from([0xA5, 0x23]);
        assert!(related_binding_eligible(&right, &altgr));
        right.remove(&0xA5);
        right.insert(0xA4);
        assert!(!related_binding_eligible(&right, &altgr));
    }
    #[test]
    fn every_owned_injection_tag_is_self_and_unknown_injection_is_external() {
        assert_eq!(
            classify_provenance(true, MULTI_LAUNCHER_INJECT_TAG),
            InputProvenance::SelfInjected
        );
        assert_eq!(
            classify_provenance(true, crate::mkmacro::input::MKMACRO_EXTRA_INFO),
            InputProvenance::SelfInjected
        );
        assert_eq!(
            classify_provenance(true, crate::mouse_gestures::service::MG_INJECT_TAG),
            InputProvenance::SelfInjected
        );
        assert_eq!(
            classify_provenance(true, 0xCAFE),
            InputProvenance::ExternalInjected
        );
    }
    #[test]
    fn failed_wake_invalidates_that_command_but_not_a_later_generation() {
        assert!(!command_generation_is_valid(8, 8));
        assert!(!command_generation_is_valid(7, 8));
        assert!(command_generation_is_valid(9, 8));
    }
    #[test]
    fn native_lifecycle_messages_map_without_touching_the_desktop() {
        assert_eq!(
            lifecycle_cancellation_for_message(POWER_BROADCAST_MESSAGE, POWER_SUSPEND_EVENT),
            Some(LifecycleCancellation::Suspend)
        );
        assert_eq!(
            lifecycle_cancellation_for_message(SESSION_CHANGE_MESSAGE, SESSION_LOCK_EVENT),
            Some(LifecycleCancellation::SessionLock)
        );
        assert_eq!(
            lifecycle_cancellation_for_message(POWER_BROADCAST_MESSAGE, 7),
            None
        );
        assert_eq!(lifecycle_cancellation_for_message(0, 0), None);
    }
    #[test]
    fn exclusive_owner_events_preserve_independent_lifecycles() {
        let owners = exclusive_owner_transition(0, ExclusiveOwner::MacroPlayback, true);
        let owners = exclusive_owner_transition(owners, ExclusiveOwner::VisualSelection, true);
        let owners = exclusive_owner_transition(owners, ExclusiveOwner::MacroPlayback, false);
        assert_eq!(owners, ExclusiveOwner::VisualSelection as u32);
    }
    #[test]
    fn higher_priority_action_cancels_pending_and_owned_release_is_drained_once() {
        let mut adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            adapter.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        adapter.process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher);
        assert!(matches!(
            adapter.preempt().as_slice(),
            [InvocationIntent::CancelDeadline { .. }]
        ));
        assert!(
            adapter
                .process(e(0x23, KeyTransition::Up, 2), PriorityOwner::ExclusiveTool)
                .consume
        );
        assert!(
            !adapter
                .process(e(0x23, KeyTransition::Up, 3), PriorityOwner::ExclusiveTool)
                .consume
        );
    }
    #[test]
    fn escape_only_dismisses_an_active_radial() {
        let mut adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
        assert!(adapter.dismiss_active().is_empty());
        for vk in [0xA0, 0xA4, 0x5B] {
            adapter.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let down = adapter.process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher);
        let id = match down.intents[0] {
            InvocationIntent::ScheduleDeadline { id, .. } => id,
            _ => unreachable!(),
        };
        adapter.deadline(id, 351, 4);
        assert!(matches!(
            adapter.dismiss_active().as_slice(),
            [InvocationIntent::CancelRadialLifecycle { .. }]
        ));
    }
    #[test]
    fn escape_provenance_and_down_repeat_up_are_balanced() {
        let mut escape = EscapeOwnership::default();
        let session_id = SessionId::new("active");
        escape.feedback_correlated(
            &InvocationEvent::RadialSessionOpened {
                id: InvocationId(7),
                session_id: session_id.clone(),
            },
            &active_lifecycle(InvocationId(7), session_id.clone()),
        );
        let mut own = e(0x1B, KeyTransition::Down, 1);
        own.provenance = InputProvenance::SelfInjected;
        assert!(
            !escape
                .process(own, true, Default::default())
                .unwrap()
                .consume
        );

        let down = escape
            .process(e(0x1B, KeyTransition::Down, 2), true, Default::default())
            .unwrap();
        assert!(down.consume);
        assert!(matches!(
            down.intents.as_slice(),
            [InvocationIntent::CloseRadial {
                session_id: Some(_)
            }]
        ));
        assert!(
            escape
                .process(e(0x1B, KeyTransition::Repeat, 3), true, Default::default())
                .unwrap()
                .consume
        );
        escape.feedback_correlated(
            &InvocationEvent::RadialSessionClosed {
                id: InvocationId(7),
            },
            &RadialLifecycle::Closed,
        );
        assert!(
            escape
                .process(e(0x1B, KeyTransition::Up, 4), true, Default::default())
                .unwrap()
                .consume
        );
        assert!(
            !escape
                .process(e(0x1B, KeyTransition::Up, 5), true, Default::default())
                .unwrap()
                .consume
        );
    }

    #[test]
    fn owned_navigation_key_is_correlated_and_balanced() {
        let mut ownership = EscapeOwnership::default();
        let session_id = crate::radial::model::SessionId::new("active");
        ownership.feedback_correlated(
            &InvocationEvent::RadialSessionOpened {
                id: InvocationId(8),
                session_id: session_id.clone(),
            },
            &active_lifecycle(InvocationId(8), session_id.clone()),
        );
        let down = ownership
            .process(
                e(0x27, KeyTransition::Down, 1),
                true,
                crate::radial::session::NavigationModifiers {
                    control: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(down.consume);
        assert!(matches!(
            down.intents.as_slice(),
            [InvocationIntent::Navigate {
                session_id: actual,
                command: crate::radial::session::NavigationCommand::Next,
                modifiers: crate::radial::session::NavigationModifiers { control: true, .. },
            }] if *actual == session_id
        ));
        assert!(
            ownership
                .process(e(0x27, KeyTransition::Repeat, 2), true, Default::default())
                .unwrap()
                .consume
        );
        assert!(
            ownership
                .process(e(0x27, KeyTransition::Up, 3), true, Default::default())
                .unwrap()
                .consume
        );
    }
    #[test]
    fn related_action_priorities_are_authoritative_and_direct_ids_are_retained() {
        assert!(
            related_priority(&RelatedAction::ScreenDrawEmergency)
                < related_priority(&RelatedAction::Quit)
        );
        assert!(
            related_priority(&RelatedAction::Quit)
                < related_priority(&RelatedAction::ScreenDrawLaunch)
        );
        let action = RelatedAction::DirectMenu {
            trigger_id: "tools-trigger".into(),
            menu_id: MenuId::new("tools"),
        };
        assert!(matches!(
            action,
            RelatedAction::DirectMenu { ref trigger_id, ref menu_id }
                if trigger_id == "tools-trigger" && menu_id.as_str() == "tools"
        ));
        assert!(!related_allowed(PriorityOwner::ExclusiveTool, &action));
        assert!(related_allowed(
            PriorityOwner::ExclusiveTool,
            &RelatedAction::Quit
        ));
    }

    #[test]
    fn direct_release_acknowledgement_requires_matching_key_and_provenance() {
        let owned = DirectCycleOwnership {
            id: InvocationId(41),
            primary_key: 0x54,
            provenance: InputProvenance::Physical,
        };
        let event = |vk, provenance| KeyEvent {
            vk,
            transition: KeyTransition::Up,
            at: 9,
            provenance,
        };
        assert!(!owned.matches_release(event(0x54, InputProvenance::ExternalInjected)));
        assert!(!owned.matches_release(event(0x55, InputProvenance::Physical)));
        assert!(owned.matches_release(event(0x54, InputProvenance::Physical)));
    }

    #[test]
    fn left_and_right_modifier_matrix_and_altgr_are_tracked_independently() {
        for modifiers in [
            [0xA0, 0xA4, 0x5B],
            [0xA1, 0xA4, 0x5C],
            [0xA0, 0xA5, 0x5C],
            [0xA1, 0xA5, 0x5B],
        ] {
            let mut adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
            for vk in modifiers {
                let outcome =
                    adapter.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
                assert!(
                    !outcome.consume,
                    "external modifier down must remain unclaimed"
                );
            }
            let down = adapter.process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher);
            assert!(down.consume);
            for vk in modifiers {
                assert!(
                    !adapter
                        .process(e(vk, KeyTransition::Up, 2), PriorityOwner::Launcher)
                        .consume,
                    "delivered modifier cycles cannot be stranded"
                );
            }
            assert!(
                adapter
                    .process(e(0x23, KeyTransition::Up, 3), PriorityOwner::Launcher)
                    .consume
            );
        }

        let mut altgr_config = cfg();
        altgr_config.hotkey = super::super::parse_hotkey("AltGr+End").unwrap();
        for (alt, accepted) in [(0xA4, false), (0xA5, true)] {
            let mut adapter = LauncherInvocationAdapter::new(altgr_config.clone()).unwrap();
            adapter.process(e(alt, KeyTransition::Down, 0), PriorityOwner::Launcher);
            assert_eq!(
                adapter
                    .process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher)
                    .consume,
                accepted
            );
        }
    }

    #[test]
    fn invocation_chord_modifiers_never_become_action_modifier_intents() {
        let mut adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            assert!(
                adapter
                    .process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher)
                    .intents
                    .is_empty()
            );
        }
        let pressed = adapter.process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher);
        let id = match pressed.intents[0] {
            InvocationIntent::ScheduleDeadline { id, .. } => id,
            _ => unreachable!(),
        };
        assert!(matches!(
            adapter.deadline(id, 351, 4).as_slice(),
            [InvocationIntent::OpenRadial { .. }]
        ));
        for vk in [0xA0, 0xA4, 0x5B] {
            let released = adapter.process(e(vk, KeyTransition::Up, 352), PriorityOwner::Launcher);
            assert!(!released.consume && released.intents.is_empty());
        }
    }

    #[test]
    fn dismiss_press_and_release_are_one_drained_cycle_without_legacy_toggle() {
        let mut adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            adapter.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let opened = adapter.process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher);
        let open_id = match opened.intents[0] {
            InvocationIntent::ScheduleDeadline { id, .. } => id,
            _ => unreachable!(),
        };
        adapter.deadline(open_id, 351, 4);
        adapter.feedback(InvocationEvent::RadialSessionOpened {
            id: open_id,
            session_id: SessionId::new("dismiss-session"),
        });
        adapter.process(e(0x23, KeyTransition::Up, 352), PriorityOwner::Launcher);

        let dismiss = adapter.process(e(0x23, KeyTransition::Down, 400), PriorityOwner::Launcher);
        assert!(dismiss.consume);
        assert!(matches!(
            dismiss.intents.as_slice(),
            [InvocationIntent::ScheduleDeadline { .. }]
        ));
        let dismiss_id = match dismiss.intents.as_slice() {
            [InvocationIntent::ScheduleDeadline { id, .. }] => *id,
            _ => unreachable!(),
        };
        let close = adapter.deadline(dismiss_id, 751, 4);
        assert!(matches!(
            close.as_slice(),
            [InvocationIntent::CloseRadial {
                session_id: Some(_)
            }]
        ));
        let release = adapter.process(e(0x23, KeyTransition::Up, 752), PriorityOwner::Launcher);
        assert!(release.consume);
        assert!(release.intents.is_empty());
        assert!(!adapter.has_owned_cycle());
        assert!(
            !adapter
                .process(e(0x23, KeyTransition::Up, 402), PriorityOwner::Launcher)
                .consume
        );
    }

    #[test]
    fn post_action_trigger_release_acknowledges_once_and_never_reopens() {
        let mut adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            adapter.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let pressed = adapter.process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher);
        let id = match pressed.intents[0] {
            InvocationIntent::ScheduleDeadline { id, .. } => id,
            _ => unreachable!(),
        };
        adapter.deadline(id, 351, 4);
        adapter.feedback(InvocationEvent::RadialClosedForAction { id });
        let released = adapter.process(e(0x23, KeyTransition::Up, 352), PriorityOwner::Launcher);
        assert!(released.consume);
        assert!(matches!(
            released.intents.as_slice(),
            [InvocationIntent::TriggerReleased { id: actual }] if *actual == id
        ));
        let duplicate = adapter.process(e(0x23, KeyTransition::Up, 353), PriorityOwner::Launcher);
        assert!(!duplicate.consume && duplicate.intents.is_empty());
    }

    #[test]
    fn supported_external_injection_invokes_but_owned_injection_never_recurses() {
        let mut external = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            external.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let mut external_down = e(0x23, KeyTransition::Down, 1);
        external_down.provenance = InputProvenance::ExternalInjected;
        let accepted = external.process(external_down, PriorityOwner::Launcher);
        assert!(accepted.consume);
        assert!(matches!(
            accepted.intents.as_slice(),
            [InvocationIntent::ScheduleDeadline { .. }]
        ));

        let mut owned = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            owned.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let mut owned_down = e(0x23, KeyTransition::Down, 1);
        owned_down.provenance = InputProvenance::SelfInjected;
        let rejected = owned.process(owned_down, PriorityOwner::Launcher);
        assert!(!rejected.consume && rejected.intents.is_empty());
    }

    #[test]
    fn owned_primary_release_requires_matching_provenance() {
        let mut adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            adapter.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let down = adapter.process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher);
        assert!(down.consume);

        let mut mismatched_up = e(0x23, KeyTransition::Up, 2);
        mismatched_up.provenance = InputProvenance::ExternalInjected;
        let ignored = adapter.process(mismatched_up, PriorityOwner::Launcher);
        assert!(!ignored.consume && ignored.intents.is_empty());

        let release = adapter.process(e(0x23, KeyTransition::Up, 3), PriorityOwner::Launcher);
        assert!(release.consume);
        assert!(!adapter.has_owned_cycle());
    }

    #[test]
    fn escape_request_cancels_opening_without_allowing_a_late_open() {
        let mut adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            adapter.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let down = adapter.process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher);
        let id = match down.intents[0] {
            InvocationIntent::ScheduleDeadline { id, .. } => id,
            _ => unreachable!(),
        };
        assert!(matches!(
            adapter.deadline(id, 351, 4).as_slice(),
            [InvocationIntent::OpenRadial { .. }]
        ));
        assert!(matches!(
            adapter.request_radial_close().as_slice(),
            [InvocationIntent::CancelRadialLifecycle {
                id: actual,
                reason: LifecycleCancellation::SessionReplaced
            }] if *actual == id
        ));
        adapter.process(e(0x23, KeyTransition::Up, 352), PriorityOwner::Launcher);
        assert!(adapter.request_radial_close().is_empty());
    }

    #[test]
    fn escape_request_closes_the_current_session_once() {
        let mut adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
        for vk in [0xA0, 0xA4, 0x5B] {
            adapter.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
        }
        let down = adapter.process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher);
        let id = match down.intents[0] {
            InvocationIntent::ScheduleDeadline { id, .. } => id,
            _ => unreachable!(),
        };
        adapter.deadline(id, 351, 4);
        let session_id = SessionId::new("escape-active");
        adapter.feedback(InvocationEvent::RadialSessionOpened {
            id,
            session_id: session_id.clone(),
        });
        let intents = adapter.request_radial_close();
        assert!(matches!(
            intents.as_slice(),
            [InvocationIntent::CloseRadial {
                session_id: Some(actual)
            }] if actual == &session_id
        ));
        assert!(adapter.request_radial_close().is_empty());
    }

    #[test]
    fn external_session_feedback_is_owned_by_shared_close_and_escape_routes() {
        let mut adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
        let external_id = InvocationId(1 << 63);
        let session_id = SessionId::new("external-session");
        assert!(
            adapter
                .feedback(InvocationEvent::ExternalSessionAdmitted {
                    id: external_id,
                    menu_id: MenuId::new("starter"),
                    context_token: external_id.0,
                    interaction: InteractionMode::StickyClick,
                })
                .is_empty()
        );
        assert!(
            adapter
                .feedback(InvocationEvent::ExternalSessionOpened {
                    id: external_id,
                    session_id: session_id.clone(),
                    menu_id: MenuId::new("starter"),
                    context_token: external_id.0,
                    interaction: InteractionMode::StickyClick,
                })
                .is_empty()
        );
        assert!(matches!(
            adapter.radial_lifecycle(),
            RadialLifecycle::Active {
                id,
                session_id: actual,
                ..
            } if *id == external_id && actual == &session_id
        ));

        for vk in [0xA0, 0xA4, 0x5B] {
            adapter.process(e(vk, KeyTransition::Down, 1), PriorityOwner::Launcher);
        }
        let shared = adapter.process(e(0x23, KeyTransition::Down, 2), PriorityOwner::Launcher);
        assert!(shared.consume);
        let shared_id = match shared.intents.as_slice() {
            [InvocationIntent::ScheduleDeadline { id, .. }] => *id,
            _ => unreachable!(),
        };
        assert!(matches!(
            adapter.deadline(shared_id, 352, 4).as_slice(),
            [InvocationIntent::CloseRadial {
                session_id: Some(actual)
            }] if actual == &session_id
        ));
        adapter.process(e(0x23, KeyTransition::Up, 3), PriorityOwner::Launcher);
        adapter.feedback(InvocationEvent::RadialSessionClosed { id: external_id });

        let second_external_id = InvocationId((1 << 63) + 1);
        let second_session_id = SessionId::new("external-session-2");
        adapter.feedback(InvocationEvent::ExternalSessionAdmitted {
            id: second_external_id,
            menu_id: MenuId::new("starter"),
            context_token: second_external_id.0,
            interaction: InteractionMode::StickyClick,
        });
        adapter.feedback(InvocationEvent::ExternalSessionOpened {
            id: second_external_id,
            session_id: second_session_id.clone(),
            menu_id: MenuId::new("starter"),
            context_token: second_external_id.0,
            interaction: InteractionMode::StickyClick,
        });
        let mut escape = EscapeOwnership::default();
        let down = route_escape_event(
            &mut escape,
            &mut adapter,
            KeyEvent {
                vk: 0x1B,
                transition: KeyTransition::Down,
                at: 4,
                provenance: InputProvenance::Physical,
            },
            true,
            Default::default(),
            false,
        )
        .expect("active external session owns Escape");
        assert!(down.consume);
        assert!(matches!(
            down.intents.as_slice(),
            [InvocationIntent::CloseRadial {
                session_id: Some(actual)
            }] if actual == &second_session_id
        ));
        let up = route_escape_event(
            &mut escape,
            &mut adapter,
            KeyEvent {
                vk: 0x1B,
                transition: KeyTransition::Up,
                at: 5,
                provenance: InputProvenance::Physical,
            },
            true,
            Default::default(),
            false,
        )
        .expect("Escape release drains ownership");
        assert!(up.consume && up.intents.is_empty());
    }

    #[test]
    fn external_replacement_retargets_shared_hold_and_escape_to_new_session() {
        let mut adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
        let old_id = InvocationId(8_100);
        let old_session = SessionId::new("replacement-old");
        adapter.feedback(InvocationEvent::ExternalSessionAdmitted {
            id: old_id,
            menu_id: MenuId::new("starter"),
            context_token: old_id.0,
            interaction: InteractionMode::StickyClick,
        });
        adapter.feedback(InvocationEvent::ExternalSessionOpened {
            id: old_id,
            session_id: old_session,
            menu_id: MenuId::new("starter"),
            context_token: old_id.0,
            interaction: InteractionMode::StickyClick,
        });

        let new_id = InvocationId(8_101);
        let new_session = SessionId::new("replacement-new");
        adapter.feedback(InvocationEvent::ExternalSessionAdmitted {
            id: new_id,
            menu_id: MenuId::new("replacement"),
            context_token: new_id.0,
            interaction: InteractionMode::StickyClick,
        });
        assert!(
            matches!(adapter.radial_lifecycle(), RadialLifecycle::Opening { id, .. } if *id == new_id)
        );
        adapter.feedback(InvocationEvent::ExternalSessionOpened {
            id: new_id,
            session_id: new_session.clone(),
            menu_id: MenuId::new("replacement"),
            context_token: new_id.0,
            interaction: InteractionMode::StickyClick,
        });

        // A late close from the replaced session cannot clear the new Escape
        // lease. Exercise the same correlated feedback boundary as the hook.
        let mut escape = EscapeOwnership::default();
        escape.feedback_correlated(
            &InvocationEvent::ExternalSessionOpened {
                id: new_id,
                session_id: new_session.clone(),
                menu_id: MenuId::new("replacement"),
                context_token: new_id.0,
                interaction: InteractionMode::StickyClick,
            },
            adapter.radial_lifecycle(),
        );
        escape.feedback_correlated(
            &InvocationEvent::RadialSessionClosed { id: old_id },
            adapter.radial_lifecycle(),
        );
        let down = route_escape_event(
            &mut escape,
            &mut adapter,
            KeyEvent {
                vk: 0x1B,
                transition: KeyTransition::Down,
                at: 20,
                provenance: InputProvenance::Physical,
            },
            true,
            Default::default(),
            false,
        )
        .expect("replacement lifecycle owns Escape");
        assert!(matches!(
            down.intents.as_slice(),
            [InvocationIntent::CloseRadial {
                session_id: Some(actual)
            }] if actual == &new_session
        ));
        let up = route_escape_event(
            &mut escape,
            &mut adapter,
            KeyEvent {
                vk: 0x1B,
                transition: KeyTransition::Up,
                at: 21,
                provenance: InputProvenance::Physical,
            },
            true,
            Default::default(),
            false,
        )
        .expect("matching Escape release is consumed");
        assert!(up.consume);
    }

    #[test]
    fn shared_hold_after_external_replacement_closes_only_new_session() {
        let mut adapter = LauncherInvocationAdapter::new(cfg()).unwrap();
        let old_id = InvocationId(8_200);
        adapter.feedback(InvocationEvent::ExternalSessionAdmitted {
            id: old_id,
            menu_id: MenuId::new("starter"),
            context_token: old_id.0,
            interaction: InteractionMode::StickyClick,
        });
        adapter.feedback(InvocationEvent::ExternalSessionOpened {
            id: old_id,
            session_id: SessionId::new("old-replaced-session"),
            menu_id: MenuId::new("starter"),
            context_token: old_id.0,
            interaction: InteractionMode::StickyClick,
        });

        let new_id = InvocationId(8_201);
        let new_session = SessionId::new("new-replaced-session");
        adapter.feedback(InvocationEvent::ExternalSessionAdmitted {
            id: new_id,
            menu_id: MenuId::new("replacement"),
            context_token: new_id.0,
            interaction: InteractionMode::StickyClick,
        });
        adapter.feedback(InvocationEvent::ExternalSessionOpened {
            id: new_id,
            session_id: new_session.clone(),
            menu_id: MenuId::new("replacement"),
            context_token: new_id.0,
            interaction: InteractionMode::StickyClick,
        });

        for vk in [0xA0, 0xA4, 0x5B] {
            adapter.process(e(vk, KeyTransition::Down, 1), PriorityOwner::Launcher);
        }
        let down = adapter.process(e(0x23, KeyTransition::Down, 2), PriorityOwner::Launcher);
        let cycle = match down.intents.as_slice() {
            [InvocationIntent::ScheduleDeadline { id, .. }] => *id,
            _ => unreachable!(),
        };
        assert!(matches!(
            adapter.deadline(cycle, 352, 4).as_slice(),
            [InvocationIntent::CloseRadial {
                session_id: Some(actual)
            }] if actual == &new_session
        ));
        assert!(
            adapter
                .process(e(0x23, KeyTransition::Up, 353), PriorityOwner::Launcher)
                .consume
        );
    }

    #[test]
    fn lifecycle_matrix_cancels_pending_and_active_without_legacy_fallback() {
        let reasons = [
            LifecycleCancellation::SettingsReload,
            LifecycleCancellation::FeatureDisabled,
            LifecycleCancellation::HostFailure,
            LifecycleCancellation::HookFailure,
            LifecycleCancellation::Shutdown,
            LifecycleCancellation::Suspend,
            LifecycleCancellation::SessionLock,
            LifecycleCancellation::DesktopUnavailable,
        ];
        for reason in reasons {
            let mut pending = LauncherInvocationAdapter::new(cfg()).unwrap();
            for vk in [0xA0, 0xA4, 0x5B] {
                pending.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
            }
            let start = pending.process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher);
            let id = match start.intents[0] {
                InvocationIntent::ScheduleDeadline { id, .. } => id,
                _ => unreachable!(),
            };
            let cancellation = pending.cancel_lifecycle(reason);
            assert!(matches!(
                cancellation.as_slice(),
                [InvocationIntent::CancelDeadline { id: cancelled }] if *cancelled == id
            ));
            assert!(
                !cancellation
                    .iter()
                    .any(|intent| matches!(intent, InvocationIntent::ToggleLegacyLauncher { .. }))
            );
            assert!(pending.deadline(id, 351, 4).is_empty());
            assert_eq!(
                pending
                    .process(e(0x23, KeyTransition::Up, 352), PriorityOwner::Launcher)
                    .consume,
                reason.preserves_input_continuity()
            );
            assert!(!pending.has_owned_cycle());
            if !reason.preserves_input_continuity() {
                assert!(
                    !pending
                        .process(e(0x23, KeyTransition::Down, 353), PriorityOwner::Launcher)
                        .consume,
                    "lost input continuity must not retain phantom modifiers"
                );
            }

            let mut active = LauncherInvocationAdapter::new(cfg()).unwrap();
            for vk in [0xA0, 0xA4, 0x5B] {
                active.process(e(vk, KeyTransition::Down, 0), PriorityOwner::Launcher);
            }
            let start = active.process(e(0x23, KeyTransition::Down, 1), PriorityOwner::Launcher);
            let active_id = match start.intents[0] {
                InvocationIntent::ScheduleDeadline { id, .. } => id,
                _ => unreachable!(),
            };
            active.deadline(active_id, 351, 4);
            assert!(active.cancel_lifecycle(reason).iter().any(|intent| matches!(
                intent,
                InvocationIntent::CancelRadialLifecycle { reason: actual, .. } if *actual == reason
            )));
            let release = active.process(e(0x23, KeyTransition::Up, 352), PriorityOwner::Launcher);
            assert_eq!(release.consume, reason.preserves_input_continuity());
            assert!(release.intents.is_empty());
            assert!(!active.has_owned_cycle());
        }
    }
}
