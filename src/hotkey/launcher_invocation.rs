//! Complete, timestamped lifecycle adapter for the launcher chord.
//! The pure adapter is also the synchronous decision core used by the native hook.
use super::{Hotkey, Key};
use crate::radial::invocation::{
    ContextToken, InvocationEvent, InvocationIntent, InvocationReducer, SettingsGeneration,
    Timestamp,
};
use crate::radial::model::{InteractionMode, InvocationId, MenuId};
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
pub enum InputProvenance {
    Physical,
    ExternalInjected,
    SelfInjected,
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
    pub fn new(config: InvocationConfig) -> Result<Self, String> {
        if config.launcher_enabled {
            vk_from_key(config.hotkey.key).ok_or_else(|| {
                "configured launcher primary key is unsupported by native adapter".to_string()
            })?;
        }
        Ok(Self {
            config,
            reducer: InvocationReducer::default(),
            modifiers: Modifiers::default(),
            next_id: 1,
            owned: None,
            owned_primary: None,
            owned_primary_down_suppressed: false,
            candidate_primary_down: None,
            recovery_primary: None,
            exclusive: false,
        })
    }
    pub fn config(&self) -> &InvocationConfig {
        &self.config
    }
    fn has_owned_cycle(&self) -> bool {
        self.owned.is_some() || self.recovery_primary.is_some()
    }
    pub fn reload(
        &mut self,
        config: InvocationConfig,
        at: Timestamp,
    ) -> Result<Vec<InvocationIntent>, String> {
        if config.launcher_enabled {
            vk_from_key(config.hotkey.key).ok_or_else(|| {
                "configured launcher primary key is unsupported by native adapter".to_string()
            })?;
        }
        let intents = self.reducer.reduce(InvocationEvent::CancelLifecycle {
            primary_still_down: self.owned.is_some(),
        });
        self.config = config;
        let _ = at;
        Ok(intents)
    }
    pub fn set_exclusive(&mut self, active: bool) -> Vec<InvocationIntent> {
        self.exclusive = active;
        self.reducer
            .reduce(InvocationEvent::ExclusiveToolChanged { active })
    }
    pub fn dismiss_active(&mut self) -> Vec<InvocationIntent> {
        if matches!(
            self.reducer.state(),
            crate::radial::invocation::InvocationState::RadialActive { .. }
        ) {
            self.reducer.reduce(InvocationEvent::CancelLifecycle {
                primary_still_down: self.owned.is_some(),
            })
        } else {
            Vec::new()
        }
    }
    fn preempt(&mut self) -> Vec<InvocationIntent> {
        self.reducer.reduce(InvocationEvent::CancelLifecycle {
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
        if self.owned.is_some() && self.owned_primary == Some(event.vk) {
            return self.process_owned_primary(event);
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
        self.owned_primary_down_suppressed = primary_down_suppressed;
        AdapterOutcome {
            consume: true,
            recovery: false,
            intents: self.reducer.reduce(InvocationEvent::ChordPressed {
                id,
                primary_key: primary,
                at,
                threshold_ms: self.config.threshold_ms,
                generation: self.config.generation,
                context_token: self.config.context_token,
                menu_id: self.config.menu_id.clone(),
                interaction: self.config.interaction,
                repeat: false,
            }),
        }
    }

    fn process_owned_primary(&mut self, event: KeyEvent) -> AdapterOutcome {
        let id = self.owned.expect("owned primary has invocation");
        if event.transition == KeyTransition::Up {
            let consume = self.owned_primary_down_suppressed;
            self.owned = None;
            self.owned_primary = None;
            self.owned_primary_down_suppressed = false;
            self.candidate_primary_down = None;
            return AdapterOutcome {
                consume,
                recovery: false,
                intents: self
                    .reducer
                    .reduce(InvocationEvent::PrimaryReleased { id, at: event.at }),
            };
        }
        AdapterOutcome {
            consume: true,
            recovery: false,
            intents: self.reducer.reduce(InvocationEvent::ChordPressed {
                id,
                primary_key: event.vk,
                at: event.at,
                threshold_ms: self.config.threshold_ms,
                generation: self.config.generation,
                context_token: self.config.context_token,
                menu_id: self.config.menu_id.clone(),
                interaction: self.config.interaction,
                repeat: true,
            }),
        }
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

#[derive(Default)]
struct EscapeOwnership {
    active_session: Option<(InvocationId, crate::radial::model::SessionId)>,
    owned_provenance: Option<InputProvenance>,
}

impl EscapeOwnership {
    fn feedback(&mut self, event: &InvocationEvent) {
        match event {
            InvocationEvent::RadialSessionOpened { id, session_id } => {
                self.active_session = Some((*id, session_id.clone()))
            }
            InvocationEvent::RadialSessionClosed { id }
                if self
                    .active_session
                    .as_ref()
                    .is_some_and(|(active, _)| active == id) =>
            {
                self.active_session = None
            }
            _ => {}
        }
    }

    fn process(
        &mut self,
        event: KeyEvent,
        accept_external_injected: bool,
    ) -> Option<AdapterOutcome> {
        if event.vk != 0x1B {
            return None;
        }
        if let Some(owned) = self.owned_provenance {
            if event.provenance != owned {
                return Some(AdapterOutcome::pass());
            }
            if event.transition == KeyTransition::Up {
                self.owned_provenance = None;
            }
            return Some(AdapterOutcome {
                consume: true,
                recovery: false,
                intents: Vec::new(),
            });
        }
        if event.provenance == InputProvenance::SelfInjected
            || (event.provenance == InputProvenance::ExternalInjected && !accept_external_injected)
            || event.transition != KeyTransition::Down
        {
            return Some(AdapterOutcome::pass());
        }
        let (_, session_id) = self.active_session.clone()?;
        self.owned_provenance = Some(event.provenance);
        Some(AdapterOutcome {
            consume: true,
            recovery: false,
            intents: vec![InvocationIntent::CloseRadial {
                session_id: Some(session_id),
            }],
        })
    }
}

#[derive(Clone, Debug)]
pub struct ServiceNotice {
    pub recovery: bool,
    pub intents: Vec<InvocationIntent>,
    pub error: Option<String>,
    pub action: Option<RelatedAction>,
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
    stopped: std::sync::mpsc::Receiver<()>,
    #[cfg(all(windows, not(test)))]
    command_sequence: std::sync::atomic::AtomicU64,
    #[cfg(all(windows, not(test)))]
    invalid_commands: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

#[cfg(all(windows, not(test)))]
enum ServiceCommand {
    Feedback(InvocationEvent),
    Reload(
        InvocationConfig,
        Vec<RelatedBinding>,
        std::sync::mpsc::SyncSender<Result<(), String>>,
    ),
    Exclusive(bool),
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
    ) -> Result<RouteHandoff, String> {
        let (acknowledge, completion) = std::sync::mpsc::sync_channel(1);
        #[cfg(all(windows, not(test)))]
        {
            self.send_command(ServiceCommand::Reload(config, related, acknowledge))?;
            return Ok(RouteHandoff { completion });
        }
        #[cfg(any(not(windows), test))]
        {
            let _ = (config, related);
            let _ = acknowledge.send(Ok(()));
            Ok(RouteHandoff { completion })
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
    #[cfg(all(windows, not(test)))]
    fn send_command(&self, command: ServiceCommand) -> Result<(), String> {
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
            let _ = self.send_command(ServiceCommand::Shutdown);
            let _ = self
                .stopped
                .recv_timeout(std::time::Duration::from_millis(1500));
            if let Some(j) = self.join.take() {
                if j.is_finished() {
                    let _ = j.join();
                } else {
                    let _ = std::thread::Builder::new()
                        .name("launcher-invocation-deferred-join".into())
                        .spawn(move || {
                            let _ = j.join();
                        });
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
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock, mpsc};
    use std::time::Instant;
    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::*;
    pub(super) const WM_SERVICE_COMMAND: u32 = WM_APP + 0x53;
    struct State {
        adapter: LauncherInvocationAdapter,
        owner: Arc<dyn Fn() -> PriorityOwner + Send + Sync>,
        notices: mpsc::Sender<ServiceNotice>,
        wake: mpsc::Sender<()>,
        epoch: Instant,
        primary_down: bool,
        timers: HashMap<InvocationId, usize>,
        related: Vec<(RelatedBinding, bool)>,
        down: std::collections::HashSet<u32>,
        commands: mpsc::Receiver<(u64, ServiceCommand)>,
        invalid_commands: Arc<std::sync::atomic::AtomicU64>,
        shutdown_requested: bool,
        escape: EscapeOwnership,
        pending_handoffs: PendingRouteHandoffs,
    }
    static STATE: OnceLock<Mutex<Option<State>>> = OnceLock::new();
    fn publish(state: &State, out: AdapterOutcome) {
        if out.recovery || !out.intents.is_empty() {
            let _ = state.notices.send(ServiceNotice {
                recovery: out.recovery,
                intents: out.intents,
                error: None,
                action: None,
            });
            let _ = state.wake.send(());
        }
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
        let injected = data.flags.contains(LLKHF_INJECTED);
        let provenance = classify_provenance(injected, data.dwExtraInfo);
        let at = state.epoch.elapsed().as_millis() as u64;
        if let Some(out) = state.escape.process(
            KeyEvent {
                vk: data.vkCode,
                transition,
                at,
                provenance,
            },
            state.adapter.config().accept_external_injected,
        ) {
            let consume = out.consume;
            publish(state, out);
            if consume {
                return LRESULT(1);
            }
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
            if !state.adapter.has_owned_cycle() && state.escape.owned_provenance.is_none() {
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
            let mut selected: Option<(u8, RelatedAction)> = None;
            for (binding, latched) in &mut state.related {
                let eligible = related_binding_eligible(&state.down, &binding.hotkey);
                if eligible && !*latched {
                    let priority = related_priority(&binding.action);
                    if selected.as_ref().is_none_or(|(p, _)| priority < *p) {
                        selected = Some((priority, binding.action.clone()));
                    }
                }
                *latched = eligible;
            }
            if let Some((_, action)) =
                selected.filter(|(_, action)| related_allowed(current_owner, action))
            {
                let intents = state.adapter.preempt();
                let _ = update_timers(state, &intents);
                let _ = state.notices.send(ServiceNotice {
                    recovery: false,
                    intents,
                    error: None,
                    action: Some(action),
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
        let out = state.adapter.process(
            KeyEvent {
                vk: data.vkCode,
                transition,
                at,
                provenance,
            },
            owner,
        );
        let timer_error = update_timers(state, &out.intents);
        if let Some(error) = timer_error {
            let cancel = state
                .adapter
                .reducer
                .reduce(InvocationEvent::CancelLifecycle {
                    primary_still_down: state.adapter.owned.is_some(),
                });
            let _ = state.notices.send(ServiceNotice {
                recovery: false,
                intents: cancel,
                error: Some(error),
                action: None,
            });
            let _ = state.wake.send(());
        }
        let consume = out.consume;
        publish(state, out);
        acknowledge_handoffs_if_drained(state);
        if state.shutdown_requested && !state.adapter.has_owned_cycle() {
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
                        kill_timer(timer, true, "deadline replacement");
                    }
                    let delay = at
                        .saturating_sub(state.epoch.elapsed().as_millis() as u64)
                        .clamp(1, u32::MAX as u64) as u32;
                    let timer = unsafe { SetTimer(None, 0, delay, None) };
                    if timer == 0 {
                        return Some(format!("failed to schedule radial deadline for {}", id.0));
                    }
                    state.timers.insert(id, timer);
                }
                InvocationIntent::CancelDeadline { id } => {
                    if let Some(timer) = state.timers.remove(&id) {
                        kill_timer(timer, true, "deadline cancellation");
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
        if state.adapter.has_owned_cycle() {
            return;
        }
        if state
            .pending_handoffs
            .acknowledge_if_drained(state.adapter.has_owned_cycle())
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
        let startup = Arc::new(StartupCancellation::default());
        let worker_startup = Arc::clone(&startup);
        let join = std::thread::Builder::new()
            .name("launcher-invocation-hook".into())
            .spawn(move || {
                struct StoppedOnDrop(Option<mpsc::SyncSender<()>>);
                impl Drop for StoppedOnDrop {
                    fn drop(&mut self) {
                        if let Some(stopped) = self.0.take() {
                            let _ = stopped.send(());
                        }
                    }
                }
                let _stopped = StoppedOnDrop(Some(stopped_tx));
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
                {
                    let Ok(mut slot) = lock.lock() else {
                        let _ = setup_tx.send(Err("launcher hook state poisoned".into()));
                        return;
                    };
                    if slot.is_some() {
                        let _ = setup_tx.send(Err("launcher hook already active".into()));
                        return;
                    }
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
                    });
                }
                if worker_startup.is_cancelled() {
                    if let Ok(mut slot) = lock.lock() {
                        *slot = None;
                    }
                    return;
                }
                let hook = match cancellable_startup_install(
                    &worker_startup,
                    || unsafe {
                        SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), HINSTANCE::default(), 0)
                    },
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
                let _ = setup_tx.send(Ok(id));
                let mut msg = MSG::default();
                while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
                    if msg.message == WM_SERVICE_COMMAND {
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
                                        state.escape.feedback(&event);
                                        let intents = state.adapter.reducer.reduce(event);
                                        let _ = update_timers(state, &intents);
                                        let _ = state.notices.send(ServiceNotice {
                                            recovery: false,
                                            intents,
                                            error: None,
                                            action: None,
                                        });
                                        let _ = state.wake.send(());
                                    }
                                    ServiceCommand::Reload(config, related, acknowledgement) => {
                                        match state.adapter.reload(
                                            config,
                                            state.epoch.elapsed().as_millis() as u64,
                                        ) {
                                            Ok(intents) => {
                                                state.related = related
                                                    .into_iter()
                                                    .map(|binding| (binding, false))
                                                    .collect();
                                                state.down.clear();
                                                let _ = update_timers(state, &intents);
                                                let _ = state.notices.send(ServiceNotice {
                                                    recovery: false,
                                                    intents,
                                                    error: None,
                                                    action: None,
                                                });
                                                let _ = state.wake.send(());
                                                state.pending_handoffs.applied(
                                                    state.adapter.has_owned_cycle(),
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
                                                });
                                                let _ = state.wake.send(());
                                            }
                                        }
                                    }
                                    ServiceCommand::Exclusive(active) => {
                                        let intents = state.adapter.set_exclusive(active);
                                        let _ = update_timers(state, &intents);
                                        let _ = state.notices.send(ServiceNotice {
                                            recovery: false,
                                            intents,
                                            error: None,
                                            action: None,
                                        });
                                        let _ = state.wake.send(());
                                    }
                                    ServiceCommand::Shutdown => {
                                        state.shutdown_requested = true;
                                        let intents = state.adapter.reducer.reduce(
                                            InvocationEvent::CancelLifecycle {
                                                primary_still_down: state.adapter.owned.is_some(),
                                            },
                                        );
                                        let _ = update_timers(state, &intents);
                                        for (_, timer) in state.timers.drain() {
                                            kill_timer(timer, true, "service shutdown");
                                        }
                                        if !state.adapter.has_owned_cycle()
                                            && state.escape.owned_provenance.is_none()
                                        {
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
                                    .find_map(|(id, actual)| (*actual == timer).then_some(*id))
                                {
                                    state.timers.remove(&id);
                                    kill_timer(timer, false, "one-shot timer fire");
                                    let at = state.epoch.elapsed().as_millis() as u64;
                                    let generation = state.adapter.config().generation;
                                    let intents = state.adapter.deadline(id, at, generation);
                                    let _ = state.notices.send(ServiceNotice {
                                        recovery: false,
                                        intents,
                                        error: None,
                                        action: None,
                                    });
                                    let _ = state.wake.send(());
                                }
                            }
                        }
                    }
                }
                let _ = unsafe { UnhookWindowsHookEx(hook) };
                if let Ok(mut s) = lock.lock() {
                    if let Some(state) = s.as_mut() {
                        for (_, timer) in state.timers.drain() {
                            kill_timer(timer, true, "service teardown");
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
                stopped,
                command_sequence: std::sync::atomic::AtomicU64::new(0),
                invalid_commands,
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
                let _ = std::thread::Builder::new()
                    .name("launcher-hook-startup-deferred-join".into())
                    .spawn(move || {
                        let _ = join.join();
                    });
                if cleaned {
                    Err("launcher hook readiness timed out and was cancelled".into())
                } else {
                    Err("launcher hook readiness timed out; cancellation is still draining".into())
                }
            }
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
            a.reload(c, 2).unwrap().as_slice(),
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
        adapter.reload(disabled, 2).unwrap();
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
        adapter.reload(direct_only, 2).unwrap();
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

        adapter.reload(cfg(), 5).unwrap();
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
            [InvocationIntent::CloseRadial { .. }]
        ));
    }
    #[test]
    fn escape_provenance_and_down_repeat_up_are_balanced() {
        let mut escape = EscapeOwnership::default();
        escape.feedback(&InvocationEvent::RadialSessionOpened {
            id: InvocationId(7),
            session_id: crate::radial::model::SessionId::new("active"),
        });
        let mut own = e(0x1B, KeyTransition::Down, 1);
        own.provenance = InputProvenance::SelfInjected;
        assert!(!escape.process(own, true).unwrap().consume);

        let down = escape
            .process(e(0x1B, KeyTransition::Down, 2), true)
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
                .process(e(0x1B, KeyTransition::Repeat, 3), true)
                .unwrap()
                .consume
        );
        escape.feedback(&InvocationEvent::RadialSessionClosed {
            id: InvocationId(7),
        });
        assert!(
            escape
                .process(e(0x1B, KeyTransition::Up, 4), true)
                .unwrap()
                .consume
        );
        assert!(
            !escape
                .process(e(0x1B, KeyTransition::Up, 5), true)
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
}
