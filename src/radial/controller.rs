use super::bindings::{
    PreparationGeneration, PreparedCell, RadialPrepareEnvelope, RadialPrepareReply,
    RadialPrepareRequest, project_menu_frame,
};
use super::context::{InvocationContext, WindowIdentity};
use super::dynamic::{FrozenAvailability, FrozenBinding};
use super::geometry::{
    CellLayout, HitShape, LayoutSnapshot, LogicalPoint, PhysicalPoint, PhysicalRect, ScaleFactor,
    layout_menu,
};
use super::handoff::{
    DispatchEvent, DispatchIntent, InteractionRequirement, PendingRadialDispatch,
    RadialDispatchIdentity, RadialDispatchRequest,
};
use super::invocation::InvocationIntent;
use super::model::{
    ActionBinding, AfterActionPolicy, CellContent, ClickGesture, Control, InteractionMode,
    InvocationId, MenuId, Override, RadialDocument, RingId, SessionId, SubmenuPresentation,
};
use super::native::{CloseReason, NativeCommand, NativeEvent, NativeHost};
use super::render::{InputOwner, build_scene};
use super::session::{
    CellRole, NavigationCommand, NavigationModifiers, PointerButton, SessionEvent, SessionIntent,
    SessionReducer,
};
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum DeadlineKey {
    ActionHandoff,
    Dwell,
}

enum DeadlineCommand {
    Arm(DeadlineKey, u64),
    Cancel(DeadlineKey),
    Stop,
}

struct HandoffDeadlineScheduler {
    tx: mpsc::Sender<DeadlineCommand>,
    join: Option<JoinHandle<()>>,
}

impl HandoffDeadlineScheduler {
    fn spawn(wake: mpsc::Sender<()>) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();
        let join = std::thread::Builder::new()
            .name("radial-handoff-deadline".into())
            .spawn(move || {
                let mut deadlines = BTreeMap::<DeadlineKey, u64>::new();
                loop {
                    let command = if let Some(at) = deadlines.values().copied().min() {
                        let remaining = at.saturating_sub(monotonic_ms());
                        match rx.recv_timeout(std::time::Duration::from_millis(remaining.max(1))) {
                            Ok(command) => Some(command),
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                let now = monotonic_ms();
                                deadlines.retain(|_, at| *at > now);
                                let _ = wake.send(());
                                None
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    } else {
                        match rx.recv() {
                            Ok(command) => Some(command),
                            Err(_) => break,
                        }
                    };
                    match command {
                        Some(DeadlineCommand::Arm(key, at)) => {
                            deadlines.insert(key, at);
                        }
                        Some(DeadlineCommand::Cancel(key)) => {
                            deadlines.remove(&key);
                        }
                        Some(DeadlineCommand::Stop) => break,
                        None => {}
                    }
                }
            })
            .map_err(|error| format!("failed to start radial handoff scheduler: {error}"))?;
        Ok(Self {
            tx,
            join: Some(join),
        })
    }
    fn arm(&self, key: DeadlineKey, deadline: u64) {
        let _ = self.tx.send(DeadlineCommand::Arm(key, deadline));
    }
    fn cancel(&self, key: DeadlineKey) {
        let _ = self.tx.send(DeadlineCommand::Cancel(key));
    }
}
impl Drop for HandoffDeadlineScheduler {
    fn drop(&mut self) {
        let _ = self.tx.send(DeadlineCommand::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[derive(Clone, Debug)]
pub enum ControllerEvent {
    ToggleLegacyLauncher,
    Opened {
        invocation_id: InvocationId,
        session_id: SessionId,
    },
    Closed {
        invocation_id: InvocationId,
        session_id: SessionId,
        reason: CloseReason,
    },
    InvocationFailed {
        invocation_id: InvocationId,
        message: String,
    },
    DispatchRequested(RadialDispatchRequest),
    InvocationReleaseAcknowledged {
        invocation_id: InvocationId,
    },
    PrepareRequested(RadialPrepareEnvelope),
    Error(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticRecord {
    pub session_id: Option<SessionId>,
    pub message: String,
}

trait HostPort: Send {
    fn send(&self, command: NativeCommand) -> Result<(), String>;
    fn try_recv(&self) -> Option<NativeEvent>;
    fn shutdown(&mut self);
}
impl HostPort for NativeHost {
    fn send(&self, command: NativeCommand) -> Result<(), String> {
        NativeHost::send(self, command)
    }
    fn try_recv(&self) -> Option<NativeEvent> {
        NativeHost::try_recv(self)
    }
    fn shutdown(&mut self) {
        NativeHost::shutdown(self)
    }
}
type HostFactory = Arc<dyn Fn() -> Result<Box<dyn HostPort>, String> + Send + Sync>;

struct PendingSession {
    invocation_id: InvocationId,
    session_id: SessionId,
    menu_id: MenuId,
    interaction: InteractionMode,
    layout: LayoutSnapshot,
    generation: u64,
    always_on_top: bool,
    context: InvocationContext,
    trigger_still_down: bool,
    prepared: Option<RadialPrepareReply>,
}
struct ActiveSession {
    invocation_id: InvocationId,
    session_id: SessionId,
    menu_id: MenuId,
    layout: LayoutSnapshot,
    reducer: SessionReducer,
    pointer: LogicalPoint,
    always_on_top: bool,
    context: InvocationContext,
    trigger_still_down: bool,
    prepared: Option<RadialPrepareReply>,
    navigation_layouts: BTreeMap<MenuId, LayoutSnapshot>,
    navigation_frames: BTreeMap<MenuId, super::bindings::PreparedMenuFrame>,
}

struct WaitingOpen {
    invocation_id: InvocationId,
    menu_id: MenuId,
    interaction: InteractionMode,
    always_on_top: bool,
    context: InvocationContext,
    trigger_still_down: bool,
    generation: PreparationGeneration,
    deadline_ms: u64,
    allow_context_rules: bool,
}

struct PreparationBridge {
    tx: mpsc::Sender<RadialPrepareReply>,
    rx: mpsc::Receiver<RadialPrepareReply>,
    wake: mpsc::Sender<()>,
    next_generation: u64,
    waiting: Option<WaitingOpen>,
}

/// Session-correlated coordinator. The native thread is lazy and exists only
/// after an accepted open request.
pub struct RadialController {
    host: Option<Box<dyn HostPort>>,
    factory: HostFactory,
    document: Arc<RadialDocument>,
    pending: Option<PendingSession>,
    active: Option<ActiveSession>,
    next_session: u64,
    layout_generation: u64,
    diagnostics: Option<VecDeque<DiagnosticRecord>>,
    handoff: Option<PendingRadialDispatch>,
    preparation: Option<PreparationBridge>,
    last_external: Option<WindowIdentity>,
    deadline_scheduler: Option<HandoffDeadlineScheduler>,
    deadline_wake: Option<mpsc::Sender<()>>,
}
impl RadialController {
    pub fn new(document: Arc<RadialDocument>, diagnostics: bool, wake: mpsc::Sender<()>) -> Self {
        let wake_for_host = wake.clone();
        let mut controller = Self::with_factory(
            document,
            diagnostics,
            Arc::new(move || {
                NativeHost::spawn_with_wake(Some(wake_for_host.clone()))
                    .map(|h| Box::new(h) as Box<dyn HostPort>)
            }),
        );
        let (tx, rx) = mpsc::channel();
        controller.preparation = Some(PreparationBridge {
            tx,
            rx,
            wake: wake.clone(),
            next_generation: 1,
            waiting: None,
        });
        controller.deadline_wake = Some(wake);
        controller
    }
    fn with_factory(
        document: Arc<RadialDocument>,
        diagnostics: bool,
        factory: HostFactory,
    ) -> Self {
        Self {
            host: None,
            factory,
            document,
            pending: None,
            active: None,
            next_session: 1,
            layout_generation: 1,
            diagnostics: diagnostics.then(VecDeque::new),
            handoff: None,
            preparation: None,
            last_external: None,
            deadline_scheduler: None,
            deadline_wake: None,
        }
    }
    pub fn replace_document(&mut self, document: Arc<RadialDocument>) {
        self.handoff = None;
        self.cancel_handoff_deadline();
        if let Some(bridge) = &mut self.preparation {
            bridge.waiting = None;
        }
        self.close(CloseReason::SettingsReload, None);
        self.document = document;
    }

    fn capture_context(&mut self, token: u64) -> InvocationContext {
        let context = InvocationContext::capture_current(
            token,
            self.last_external.clone(),
            std::process::id(),
        );
        if let Some(foreground) = context.foreground.clone() {
            self.last_external = Some(foreground);
        }
        context
    }
    pub fn disable(&mut self) {
        self.handoff = None;
        self.cancel_handoff_deadline();
        if let Some(bridge) = &mut self.preparation {
            bridge.waiting = None;
        }
        self.close(CloseReason::SettingsReload, None);
        if let Some(mut host) = self.host.take() {
            host.shutdown();
        }
        self.pending = None;
        self.active = None;
    }
    pub fn handle_intents(
        &mut self,
        intents: Vec<InvocationIntent>,
        always_on_top: bool,
    ) -> Vec<ControllerEvent> {
        let mut out = vec![];
        for intent in intents {
            match intent {
                InvocationIntent::ToggleLegacyLauncher { .. } => {
                    out.push(ControllerEvent::ToggleLegacyLauncher)
                }
                InvocationIntent::OpenRadial {
                    id,
                    menu_id,
                    context_token,
                    interaction,
                    trigger_still_down,
                    ..
                } => {
                    let context = self.capture_context(context_token);
                    self.request_open(
                        id,
                        menu_id,
                        interaction,
                        always_on_top,
                        context,
                        trigger_still_down,
                        true,
                        &mut out,
                    )
                }
                InvocationIntent::ToggleDirectMenu {
                    id,
                    menu_id,
                    trigger_still_down,
                    ..
                } => {
                    let current_menu = self
                        .pending
                        .as_ref()
                        .map(|session| &session.menu_id)
                        .or_else(|| self.active.as_ref().map(|session| &session.menu_id))
                        .or_else(|| {
                            self.preparation
                                .as_ref()
                                .and_then(|bridge| bridge.waiting.as_ref())
                                .map(|waiting| &waiting.menu_id)
                        });
                    if current_menu.is_some_and(|current| current == &menu_id) {
                        if let Some(bridge) = &mut self.preparation {
                            bridge.waiting = None;
                        }
                        self.close(CloseReason::Dismissed, None);
                        continue;
                    }
                    let interaction = self
                        .document
                        .menus
                        .iter()
                        .find(|m| m.id == menu_id)
                        .map_or(InteractionMode::StickyClick, |m| m.interaction);
                    let context = self.capture_context(id.0);
                    self.request_open(
                        id,
                        menu_id,
                        interaction,
                        always_on_top,
                        context,
                        trigger_still_down,
                        false,
                        &mut out,
                    )
                }
                InvocationIntent::CloseRadial { session_id } => {
                    self.close(CloseReason::Dismissed, session_id.as_ref())
                }
                InvocationIntent::TriggerReleased { id } => {
                    if let Some(active) = self
                        .active
                        .as_mut()
                        .filter(|active| active.invocation_id == id)
                    {
                        active.trigger_still_down = false;
                    }
                    if let Some(active) = self
                        .active
                        .as_ref()
                        .filter(|active| active.invocation_id == id)
                    {
                        let cell = active.reducer.state.hovered.clone();
                        let role = cell.as_ref().map_or(CellRole::Unavailable, |cell| {
                            self.cell_role_for_session(&active.session_id, cell)
                        });
                        let session_id = active.session_id.clone();
                        let point = active.pointer;
                        let generation = self.layout_generation_for(&session_id);
                        self.session_event(
                            &session_id,
                            SessionEvent::TriggerReleased {
                                point,
                                cell,
                                role,
                                geometry_generation: generation,
                            },
                            &mut out,
                        );
                    }
                    out.push(ControllerEvent::InvocationReleaseAcknowledged { invocation_id: id });
                    self.reduce_handoff(
                        DispatchEvent::InvocationReleased { invocation_id: id },
                        &mut out,
                    );
                }
                InvocationIntent::Navigate {
                    session_id,
                    command,
                    modifiers,
                } => {
                    self.navigate(&session_id, command, modifiers, &mut out);
                }
                InvocationIntent::ScheduleDeadline { .. }
                | InvocationIntent::CancelDeadline { .. }
                | InvocationIntent::HoldCancelledBeforePresentation { .. } => {}
            }
        }
        out
    }
    fn request_open(
        &mut self,
        invocation_id: InvocationId,
        menu_id: MenuId,
        interaction: InteractionMode,
        always_on_top: bool,
        context: InvocationContext,
        trigger_still_down: bool,
        allow_context_rules: bool,
        out: &mut Vec<ControllerEvent>,
    ) {
        self.handoff = None;
        self.cancel_handoff_deadline();
        let Some(bridge) = self.preparation.as_mut() else {
            self.open(
                invocation_id,
                menu_id,
                interaction,
                always_on_top,
                context,
                trigger_still_down,
                None,
                out,
            );
            return;
        };
        let generation = PreparationGeneration(bridge.next_generation);
        let Some(next) = bridge.next_generation.checked_add(1) else {
            out.push(ControllerEvent::InvocationFailed {
                invocation_id,
                message: "radial preparation generation overflow".into(),
            });
            return;
        };
        bridge.next_generation = next;
        bridge.waiting = Some(WaitingOpen {
            invocation_id,
            menu_id: menu_id.clone(),
            interaction,
            always_on_top,
            context: context.clone(),
            trigger_still_down,
            generation,
            deadline_ms: monotonic_ms().checked_add(5_000).unwrap_or(u64::MAX),
            allow_context_rules,
        });
        out.push(ControllerEvent::PrepareRequested(RadialPrepareEnvelope {
            request: RadialPrepareRequest {
                generation,
                invocation_id,
                requested_menu_id: menu_id,
                document: Arc::clone(&self.document),
                context,
                invocation_query: String::new(),
                allow_context_rules,
            },
            reply: bridge.tx.clone(),
            wake: bridge.wake.clone(),
        }));
    }
    fn open(
        &mut self,
        invocation_id: InvocationId,
        menu_id: MenuId,
        interaction: InteractionMode,
        always_on_top: bool,
        context: InvocationContext,
        trigger_still_down: bool,
        prepared: Option<RadialPrepareReply>,
        out: &mut Vec<ControllerEvent>,
    ) {
        let menu_id = prepared
            .as_ref()
            .map(|reply| reply.menu_id.clone())
            .unwrap_or(menu_id);
        let menu = prepared
            .as_ref()
            .map(|reply| reply.frame.menu.clone())
            .or_else(|| {
                self.document
                    .menus
                    .iter()
                    .find(|m| m.id == menu_id)
                    .cloned()
            });
        let Some(menu) = menu else {
            out.push(ControllerEvent::InvocationFailed {
                invocation_id,
                message: format!("radial menu {menu_id} no longer exists"),
            });
            return;
        };
        let (anchor, work, scale) = desktop_geometry();
        let mut layout = match layout_menu(&menu, anchor, work, scale, 0.55) {
            Ok(v) => v,
            Err(e) => {
                out.push(ControllerEvent::InvocationFailed {
                    invocation_id,
                    message: format!("radial layout failed: {e:?}"),
                });
                return;
            }
        };
        if let Some(reply) = prepared.as_ref() {
            augment_special_cells(&mut layout, &reply.frame.cells, &reply.frame.menu);
            apply_prepared_availability(&mut layout, &reply.frame);
        }
        if self.host.is_none() {
            match (self.factory)() {
                Ok(host) => self.host = Some(host),
                Err(message) => {
                    out.push(ControllerEvent::InvocationFailed {
                        invocation_id,
                        message,
                    });
                    return;
                }
            }
        }
        self.close(CloseReason::Dismissed, None);
        let session_id = SessionId::new(format!("native-{}", self.next_session));
        self.next_session = self.next_session.checked_add(1).unwrap_or(1);
        let generation = self.layout_generation;
        self.layout_generation = self.layout_generation.checked_add(1).unwrap_or(1);
        let scene = build_scene(&layout, generation);
        self.record(
            Some(session_id.clone()),
            format!(
                "open requested invocation={} layout_generation={generation}",
                invocation_id.0
            ),
        );
        let command = NativeCommand::Open {
            session_id: session_id.clone(),
            scene,
            layout: layout.clone(),
            always_on_top,
        };
        match self.host.as_ref().expect("lazy host").send(command) {
            Ok(()) => {
                self.pending = Some(PendingSession {
                    invocation_id,
                    session_id,
                    menu_id,
                    interaction: if prepared.is_some() {
                        menu.interaction
                    } else {
                        interaction
                    },
                    layout,
                    generation,
                    always_on_top,
                    context,
                    trigger_still_down,
                    prepared,
                })
            }
            Err(message) => {
                self.retire_host();
                out.push(ControllerEvent::InvocationFailed {
                    invocation_id,
                    message,
                })
            }
        }
    }
    pub fn poll(&mut self) -> Vec<ControllerEvent> {
        let mut out = vec![];
        self.poll_preparation(&mut out);
        loop {
            let event = self.host.as_ref().and_then(|h| h.try_recv());
            let Some(event) = event else { break };
            self.handle_native(event, &mut out)
        }
        self.reduce_handoff(
            DispatchEvent::Tick {
                now_ms: monotonic_ms(),
            },
            &mut out,
        );
        self.poll_dwell(&mut out);
        out
    }
    pub fn navigate_active(
        &mut self,
        command: NavigationCommand,
        modifiers: NavigationModifiers,
    ) -> Vec<ControllerEvent> {
        let mut out = Vec::new();
        if let Some(id) = self.active.as_ref().map(|active| active.session_id.clone()) {
            self.navigate(&id, command, modifiers, &mut out);
        }
        out
    }
    fn poll_preparation(&mut self, out: &mut Vec<ControllerEvent>) {
        let mut replies = Vec::new();
        if let Some(bridge) = &self.preparation {
            while let Ok(reply) = bridge.rx.try_recv() {
                replies.push(reply);
            }
        }
        for reply in replies {
            let waiting = self.preparation.as_mut().and_then(|bridge| {
                let matches = bridge.waiting.as_ref().is_some_and(|waiting| {
                    waiting.generation == reply.generation
                        && waiting.invocation_id == reply.invocation_id
                });
                matches.then(|| bridge.waiting.take()).flatten()
            });
            let Some(waiting) = waiting else { continue };
            self.open(
                waiting.invocation_id,
                reply.menu_id.clone(),
                waiting.interaction,
                waiting.always_on_top,
                waiting.context,
                waiting.trigger_still_down,
                Some(reply),
                out,
            );
        }
        let timed_out = self.preparation.as_mut().and_then(|bridge| {
            bridge
                .waiting
                .as_ref()
                .is_some_and(|waiting| monotonic_ms() >= waiting.deadline_ms)
                .then(|| bridge.waiting.take())
                .flatten()
        });
        if let Some(waiting) = timed_out {
            out.push(ControllerEvent::InvocationFailed {
                invocation_id: waiting.invocation_id,
                message: "radial GUI preparation timed out".into(),
            });
        }
    }
    fn poll_dwell(&mut self, out: &mut Vec<ControllerEvent>) {
        let ready = self.active.as_ref().and_then(|active| {
            let (cell, deadline) = active.reducer.state.dwell_candidate.clone()?;
            (monotonic_ms() >= deadline).then(|| {
                (
                    active.session_id.clone(),
                    cell.clone(),
                    self.cell_role_for_session(&active.session_id, &cell),
                    deadline,
                    active.pointer,
                    self.layout_generation_for(&active.session_id),
                )
            })
        });
        if let Some((session_id, cell, role, deadline, point, geometry_generation)) = ready {
            self.session_event(
                &session_id,
                SessionEvent::DwellExpired {
                    cell,
                    role,
                    at: deadline,
                    point,
                    geometry_generation,
                },
                out,
            );
        }
    }
    fn handle_native(&mut self, event: NativeEvent, out: &mut Vec<ControllerEvent>) {
        match event {
            NativeEvent::Ready {
                session_id,
                layout_generation,
            } => {
                if !self.pending.as_ref().is_some_and(|p| {
                    p.session_id == session_id && p.generation == layout_generation
                }) {
                    return;
                }
                let p = self.pending.take().expect("correlated pending session");
                let mut reducer = SessionReducer::new(
                    session_id.clone(),
                    self.document.revision,
                    p.menu_id.clone(),
                    p.layout.origin,
                    layout_generation,
                    p.interaction,
                    p.invocation_id,
                    p.layout.center,
                );
                if let Some(root) = reducer.state.stack.last_mut() {
                    root.scale_factor = p.layout.scale_factor.get();
                    root.page_count = p
                        .prepared
                        .as_ref()
                        .map_or(1, |reply| reply.frame.page_count);
                }
                if let Some(prepared) = &p.prepared {
                    for (cell_id, frame) in &prepared.dynamic {
                        reducer.reduce(SessionEvent::FreezeDynamic {
                            frame_id: super::session::FrameId(1),
                            source_cell: cell_id.clone(),
                            results: frame.entries.clone(),
                        });
                    }
                }
                let invocation_id = p.invocation_id;
                let pointer = p.layout.center;
                let mut navigation_layouts = BTreeMap::new();
                navigation_layouts.insert(p.menu_id.clone(), p.layout.clone());
                let navigation_frames = p
                    .prepared
                    .as_ref()
                    .map_or_else(BTreeMap::new, |reply| reply.frames.clone());
                self.active = Some(ActiveSession {
                    invocation_id,
                    session_id: session_id.clone(),
                    menu_id: p.menu_id,
                    layout: p.layout,
                    reducer,
                    pointer,
                    always_on_top: p.always_on_top,
                    context: p.context,
                    trigger_still_down: p.trigger_still_down,
                    prepared: p.prepared,
                    navigation_layouts,
                    navigation_frames,
                });
                self.record(Some(session_id.clone()), "native host ready".into());
                out.push(ControllerEvent::Opened {
                    invocation_id,
                    session_id,
                });
            }
            NativeEvent::Closed { session_id, reason } => {
                if let Some(p) = self.pending.take() {
                    if p.session_id != session_id {
                        self.pending = Some(p);
                        return;
                    }
                    self.record(
                        Some(session_id.clone()),
                        "pending native cleanup complete".into(),
                    );
                    out.push(ControllerEvent::Closed {
                        invocation_id: p.invocation_id,
                        session_id,
                        reason,
                    });
                    return;
                }
                if let Some(a) = self.active.take() {
                    if a.session_id != session_id {
                        self.active = Some(a);
                        return;
                    }
                    self.record(Some(session_id.clone()), "native cleanup complete".into());
                    out.push(ControllerEvent::Closed {
                        invocation_id: a.invocation_id,
                        session_id: session_id.clone(),
                        reason,
                    });
                    self.reduce_handoff(DispatchEvent::Closed { session_id, reason }, out);
                }
            }
            NativeEvent::Failed {
                session_id,
                message,
            } => {
                if session_id.as_ref().is_some_and(|id| {
                    self.pending.as_ref().is_none_or(|p| &p.session_id != id)
                        && self.active.as_ref().is_none_or(|a| &a.session_id != id)
                }) {
                    return;
                }
                let invocation_id = self
                    .pending
                    .as_ref()
                    .map(|p| p.invocation_id)
                    .or_else(|| self.active.as_ref().map(|a| a.invocation_id));
                self.pending = None;
                self.active = None;
                self.retire_host();
                self.record(session_id, message.clone());
                if let Some(invocation_id) = invocation_id {
                    out.push(ControllerEvent::InvocationFailed {
                        invocation_id,
                        message,
                    })
                } else {
                    out.push(ControllerEvent::Error(message))
                }
            }
            NativeEvent::PointerMoved {
                session_id,
                owner,
                point,
            } => {
                if let Some(active) = self
                    .active
                    .as_mut()
                    .filter(|active| active.session_id == session_id)
                {
                    active.pointer = point;
                }
                let generation = self.layout_generation_for(&session_id);
                let hovered = owner_cell(owner);
                self.session_event(
                    &session_id,
                    SessionEvent::PointerMoved {
                        point,
                        hovered: hovered.clone(),
                        geometry_generation: generation,
                    },
                    out,
                );
                if let Some(cell) = hovered
                    && let Some(dwell_ms) = self
                        .active
                        .as_ref()
                        .filter(|active| active.session_id == session_id)
                        .and_then(|active| {
                            self.document
                                .menus
                                .iter()
                                .find(|menu| menu.id == active.menu_id)
                                .and_then(|menu| menu.hover_dwell_ms)
                        })
                {
                    let role = self.cell_role_for_session(&session_id, &cell);
                    self.session_event(
                        &session_id,
                        SessionEvent::StartDwell {
                            cell,
                            role,
                            deadline: monotonic_ms().checked_add(dwell_ms).unwrap_or(u64::MAX),
                        },
                        out,
                    );
                }
            }
            NativeEvent::PointerLeft { session_id } => {
                self.session_event(&session_id, SessionEvent::OutsideInteraction, out)
            }
            NativeEvent::PointerDown {
                session_id,
                owner,
                point,
                button,
            } => {
                let generation = self.layout_generation_for(&session_id);
                let role = self.role_for_button(&session_id, &owner, button);
                self.session_event(
                    &session_id,
                    SessionEvent::PointerDown {
                        point,
                        cell: owner_cell(owner),
                        role,
                        button,
                        geometry_generation: generation,
                    },
                    out,
                )
            }
            NativeEvent::PointerUp {
                session_id,
                owner,
                point,
                button,
            } => {
                let generation = self.layout_generation_for(&session_id);
                let role = self.role_for_button(&session_id, &owner, button);
                self.session_event(
                    &session_id,
                    SessionEvent::PointerUp {
                        point,
                        cell: owner_cell(owner),
                        role,
                        button,
                        geometry_generation: generation,
                    },
                    out,
                )
            }
            NativeEvent::CaptureLost { session_id } => {
                self.session_event(&session_id, SessionEvent::OutsideInteraction, out)
            }
            NativeEvent::Escape { session_id } => {
                self.close(CloseReason::Dismissed, Some(&session_id))
            }
            NativeEvent::Navigate {
                session_id,
                command,
                modifiers,
            } => self.navigate(&session_id, command, modifiers, out),
            NativeEvent::DisplayChanged { session_id } => {
                self.close(CloseReason::DisplayRelayout, Some(&session_id))
            }
            NativeEvent::Stopped => self.retire_host(),
        }
    }
    fn session_event(
        &mut self,
        id: &SessionId,
        event: SessionEvent,
        out: &mut Vec<ControllerEvent>,
    ) {
        let Some(active) = self.active.as_mut().filter(|a| &a.session_id == id) else {
            return;
        };
        let intents = active.reducer.reduce(event);
        self.sync_dwell_deadline();
        for intent in intents {
            self.handle_session_intent(id, intent, out);
        }
    }
    fn handle_session_intent(
        &mut self,
        id: &SessionId,
        intent: SessionIntent,
        out: &mut Vec<ControllerEvent>,
    ) {
        match intent {
            SessionIntent::CloseTree => self.close(CloseReason::Dismissed, Some(id)),
            SessionIntent::Dispatch {
                cell_id,
                token,
                button,
                modifiers,
                source,
            } => {
                let Some((
                    prepared,
                    invocation_id,
                    preparation_generation,
                    context,
                    trigger_still_down,
                    stack_len,
                    pointer,
                )) = self
                    .active
                    .as_ref()
                    .filter(|active| &active.session_id == id)
                    .and_then(|active| {
                        self.prepared_action(active, &cell_id, button, modifiers)
                            .map(|prepared| {
                                (
                                    prepared,
                                    active.invocation_id,
                                    active
                                        .prepared
                                        .as_ref()
                                        .map_or(PreparationGeneration(0), |reply| reply.generation),
                                    active.context.clone(),
                                    active.trigger_still_down,
                                    active.reducer.state.stack.len(),
                                    active.pointer,
                                )
                            })
                    })
                else {
                    return;
                };
                if prepared.availability != FrozenAvailability::Available {
                    return;
                }
                let requirement = prepared.requirement;
                let after_action = prepared.after_action;
                if after_action == AfterActionPolicy::KeepOpen
                    && prepared.requirement != InteractionRequirement::None
                {
                    out.push(ControllerEvent::Error(format!(
                        "action requires {:?} and cannot keep the radial menu open",
                        prepared.requirement
                    )));
                    return;
                }
                let request = RadialDispatchRequest {
                    identity: RadialDispatchIdentity {
                        session_id: id.clone(),
                        invocation_id,
                        session_generation: token.session_generation,
                        token,
                        config_revision: self.document.revision,
                        preparation_generation,
                    },
                    requirement,
                    binding: prepared.binding,
                    history_query: prepared.history_query,
                    context,
                    after_action,
                    source,
                };
                let release_required = trigger_still_down;
                let close_required = after_action == AfterActionPolicy::CloseTree
                    || (after_action == AfterActionPolicy::CloseCurrentMenu && stack_len == 1)
                    || requirement != InteractionRequirement::None;
                if after_action == AfterActionPolicy::CloseCurrentMenu && stack_len > 1 {
                    let geometry_generation = self.next_layout_generation();
                    self.session_event(
                        id,
                        SessionEvent::Back {
                            geometry_generation,
                            pointer_baseline: pointer,
                        },
                        out,
                    );
                }
                match PendingRadialDispatch::new(
                    request,
                    close_required,
                    release_required,
                    monotonic_ms(),
                    5_000,
                ) {
                    Ok(mut pending) => {
                        let intents = pending.reduce(DispatchEvent::Begin);
                        self.handoff = Some(pending);
                        self.arm_handoff_deadline(monotonic_ms().saturating_add(5_000));
                        self.apply_handoff_intents(intents, out);
                    }
                    Err(message) => out.push(ControllerEvent::Error(message)),
                }
            }
            SessionIntent::OpenSubmenu { cell_id } => self.open_submenu(id, &cell_id, out),
            SessionIntent::Back | SessionIntent::PageChanged { .. } => {
                self.refresh_active_scene(id, out)
            }
        }
    }
    fn role(&self, id: &SessionId, owner: &InputOwner) -> CellRole {
        let InputOwner::Actionable(cell) = owner else {
            return CellRole::Spacer;
        };
        let Some(active) = self.active.as_ref().filter(|a| &a.session_id == id) else {
            return CellRole::Unavailable;
        };
        if let InputOwner::Actionable(cell) = owner
            && active.prepared.as_ref().is_some_and(|prepared| {
                prepared.menu_id == active.menu_id && prepared.unavailable.contains_key(cell)
            })
        {
            return CellRole::Unavailable;
        }
        self.cell_role_for_session(id, cell)
    }
    fn role_for_button(
        &self,
        id: &SessionId,
        owner: &InputOwner,
        button: PointerButton,
    ) -> CellRole {
        let InputOwner::Actionable(cell) = owner else {
            return self.role(id, owner);
        };
        let Some(active) = self
            .active
            .as_ref()
            .filter(|active| &active.session_id == id)
        else {
            return CellRole::Unavailable;
        };
        if let Some(prepared) =
            self.prepared_action(active, cell, button, active.reducer.state.modifiers)
        {
            return if prepared.availability == FrozenAvailability::Available {
                CellRole::Action
            } else {
                CellRole::Unavailable
            };
        }
        if active.prepared.as_ref().is_some_and(|reply| {
            reply.frame.cells.contains_key(cell)
                || reply.frame.alternates.keys().any(|(id, _)| id == cell)
        }) {
            return CellRole::Unavailable;
        }
        self.role(id, owner)
    }
    fn cell_role_for_session(&self, id: &SessionId, cell: &super::model::CellId) -> CellRole {
        let Some(active) = self
            .active
            .as_ref()
            .filter(|active| &active.session_id == id)
        else {
            return CellRole::Unavailable;
        };
        if let Some(prepared) = active
            .prepared
            .as_ref()
            .and_then(|reply| reply.frame.cells.get(cell))
        {
            return if prepared.availability == FrozenAvailability::Available {
                CellRole::Action
            } else {
                CellRole::Unavailable
            };
        }
        if let Some(menu) = active.prepared.as_ref().map(|reply| &reply.frame.menu) {
            if active.prepared.as_ref().is_some_and(|reply| {
                reply.frame.alternates.iter().any(|((id, _), prepared)| {
                    id == cell && prepared.availability == FrozenAvailability::Available
                })
            }) {
                return CellRole::Action;
            }
            return cell_role_in_menu(menu, cell);
        }
        self.cell_role(&active.menu_id, cell)
    }
    fn cell_role(&self, menu_id: &MenuId, cell: &super::model::CellId) -> CellRole {
        self.document
            .menus
            .iter()
            .find(|m| &m.id == menu_id)
            .map_or(CellRole::Unavailable, |menu| cell_role_in_menu(menu, cell))
    }
    fn prepared_action(
        &self,
        active: &ActiveSession,
        cell_id: &super::model::CellId,
        button: PointerButton,
        modifiers: NavigationModifiers,
    ) -> Option<PreparedCell> {
        let mut gesture = if button == PointerButton::Secondary {
            ClickGesture::Secondary
        } else if modifiers.alt_gr {
            ClickGesture::AltPrimary
        } else if modifiers.control {
            ClickGesture::CtrlPrimary
        } else if modifiers.shift {
            ClickGesture::ShiftPrimary
        } else if modifiers.alt || modifiers.alt_gr {
            ClickGesture::AltPrimary
        } else {
            ClickGesture::Primary
        };
        let menu = self
            .document
            .menus
            .iter()
            .find(|menu| menu.id == active.menu_id)?;
        if gesture == ClickGesture::Primary
            && menu.mirror_primary_to_secondary
            && menu
                .rings
                .iter()
                .flat_map(|ring| &ring.cells)
                .find(|cell| &cell.id == cell_id)
                .is_some_and(|cell| {
                    cell.alternate_clicks
                        .iter()
                        .any(|alternate| alternate.gesture == ClickGesture::Secondary)
                })
        {
            gesture = ClickGesture::Secondary;
        }
        if gesture != ClickGesture::Primary
            && let Some(prepared) = active
                .prepared
                .as_ref()
                .and_then(|reply| reply.frame.alternates.get(&(cell_id.clone(), gesture)))
        {
            return Some(prepared.clone());
        }
        if gesture == ClickGesture::Primary
            && let Some(prepared) = active
                .prepared
                .as_ref()
                .and_then(|reply| reply.frame.cells.get(cell_id))
                .cloned()
        {
            return Some(prepared);
        }
        let cell = menu
            .rings
            .iter()
            .flat_map(|ring| &ring.cells)
            .find(|cell| &cell.id == cell_id)?;
        let CellContent::Action { binding } = &cell.content else {
            return None;
        };
        let alternate = cell
            .alternate_clicks
            .iter()
            .find(|alternate| alternate.gesture == gesture)
            .map(|alternate| (alternate.action.clone(), alternate.after_action));
        let (binding, after_action) = alternate.or_else(|| {
            (gesture == ClickGesture::Primary).then(|| (binding.clone(), cell.after_action))
        })?;
        Some(PreparedCell {
            binding: FrozenBinding::Stable(binding),
            availability: FrozenAvailability::Available,
            requirement: InteractionRequirement::ExternalInput,
            after_action: self.effective_after_action(menu, after_action),
            history_query: String::new(),
        })
    }

    fn effective_after_action(
        &self,
        menu: &super::model::MenuDefinition,
        cell: AfterActionPolicy,
    ) -> AfterActionPolicy {
        super::model::effective_after_action(&self.document, menu, cell)
    }
    fn open_submenu(
        &mut self,
        id: &SessionId,
        cell_id: &super::model::CellId,
        out: &mut Vec<ControllerEvent>,
    ) {
        let Some(active) = self
            .active
            .as_ref()
            .filter(|active| &active.session_id == id)
        else {
            return;
        };
        let parent_menu = active.menu_id.clone();
        let Some(cell) = self
            .document
            .menus
            .iter()
            .find(|menu| menu.id == parent_menu)
            .and_then(|menu| {
                menu.rings
                    .iter()
                    .flat_map(|ring| &ring.cells)
                    .find(|cell| &cell.id == cell_id)
            })
        else {
            return;
        };
        let CellContent::Submenu { menu_id } = &cell.content else {
            return;
        };
        let menu_id = menu_id.clone();
        let prepared_child = active
            .prepared
            .as_ref()
            .and_then(|reply| reply.frames.get(&menu_id))
            .cloned();
        let child = prepared_child
            .as_ref()
            .map(|frame| frame.menu.clone())
            .or_else(|| {
                self.document
                    .menus
                    .iter()
                    .find(|menu| menu.id == menu_id)
                    .cloned()
            });
        let Some(child) = child else {
            return;
        };
        let (desktop_anchor, work, scale) = desktop_geometry();
        let anchor = match child.submenu_presentation {
            SubmenuPresentation::SameCenter => active.layout.requested_anchor,
            SubmenuPresentation::Cascade => active
                .layout
                .cells
                .iter()
                .find(|layout| &layout.cell_id == cell_id)
                .map(|layout| shape_center(&layout.shape, active.layout.origin, scale))
                .unwrap_or(desktop_anchor),
        };
        let Ok(mut layout) = layout_menu(&child, anchor, work, scale, 0.55) else {
            out.push(ControllerEvent::Error(
                "radial submenu layout failed".into(),
            ));
            return;
        };
        if let Some(frame) = prepared_child.as_ref() {
            augment_special_cells(&mut layout, &frame.cells, &frame.menu);
            apply_prepared_availability(&mut layout, frame);
        }
        if child.submenu_presentation == SubmenuPresentation::Cascade {
            layout = cascade_layout(&active.layout, layout);
        }
        let pointer = active.pointer;
        let generation = self.next_layout_generation();
        if let Some(active) = self
            .active
            .as_mut()
            .filter(|active| &active.session_id == id)
        {
            active.reducer.reduce(SessionEvent::OpenChild {
                menu_id: menu_id.clone(),
                origin: layout.origin,
                geometry_generation: generation,
                pointer_baseline: pointer,
            });
            if let Some(current) = active.reducer.state.stack.last_mut() {
                current.scale_factor = layout.scale_factor.get();
                current.page_count = prepared_child.as_ref().map_or(1, |frame| frame.page_count);
            }
            active.menu_id = menu_id.clone();
            active
                .navigation_layouts
                .insert(menu_id.clone(), layout.clone());
            active.layout = layout;
            if let (Some(reply), Some(frame)) = (active.prepared.as_mut(), prepared_child) {
                active
                    .navigation_frames
                    .insert(menu_id.clone(), frame.clone());
                reply.frame = frame;
            }
        }
        self.refresh_active_scene(id, out);
    }
    fn refresh_active_scene(&mut self, id: &SessionId, out: &mut Vec<ControllerEvent>) {
        let page = self
            .active
            .as_ref()
            .filter(|active| &active.session_id == id)
            .and_then(|active| active.reducer.state.stack.last())
            .map_or(0, |frame| frame.page);
        let cascade_parent = self
            .active
            .as_ref()
            .filter(|active| &active.session_id == id)
            .and_then(|active| {
                (active
                    .prepared
                    .as_ref()?
                    .frame
                    .base_menu
                    .submenu_presentation
                    == SubmenuPresentation::Cascade)
                    .then(|| {
                        active
                            .reducer
                            .state
                            .stack
                            .iter()
                            .rev()
                            .nth(1)
                            .and_then(|frame| active.navigation_layouts.get(&frame.menu_id))
                            .cloned()
                    })
                    .flatten()
            });
        if let Some(active) = self
            .active
            .as_mut()
            .filter(|active| &active.session_id == id)
            && let Some(prepared) = active.prepared.as_mut()
        {
            let base = prepared.frame.base_menu.clone();
            let alternates = prepared.frame.alternates.clone();
            prepared.frame = project_menu_frame(
                &base,
                prepared.frame.static_cells.clone(),
                &prepared.frame.dynamic,
                page,
                12,
            );
            if let Some(current) = active.reducer.state.stack.last_mut() {
                current.page = prepared.frame.page;
                current.page_count = prepared.frame.page_count;
            }
            prepared.frame.alternates = alternates;
            let (_, work, _) = desktop_geometry();
            if let Ok(layout) = layout_menu(
                &prepared.frame.menu,
                active.layout.requested_anchor,
                work,
                active.layout.scale_factor,
                0.55,
            ) {
                let mut layout = layout;
                augment_special_cells(&mut layout, &prepared.frame.cells, &prepared.frame.menu);
                apply_prepared_availability(&mut layout, &prepared.frame);
                if let Some(parent) = cascade_parent.as_ref() {
                    layout = cascade_layout(parent, layout);
                }
                active.layout = layout;
                active
                    .navigation_frames
                    .insert(active.menu_id.clone(), prepared.frame.clone());
                active
                    .navigation_layouts
                    .insert(active.menu_id.clone(), active.layout.clone());
            }
        }
        let Some(active) = self
            .active
            .as_ref()
            .filter(|active| &active.session_id == id)
        else {
            return;
        };
        let frame = active.reducer.state.stack.last().cloned();
        if let Some(frame) = frame
            && frame.menu_id != active.menu_id
        {
            if let Some(active) = self
                .active
                .as_mut()
                .filter(|active| &active.session_id == id)
            {
                if let Some(layout) = active.navigation_layouts.get(&frame.menu_id).cloned() {
                    active.menu_id = frame.menu_id.clone();
                    active.layout = layout;
                }
                if let Some(prepared) = active.prepared.as_mut()
                    && let Some(saved) = active.navigation_frames.get(&frame.menu_id).cloned()
                {
                    prepared.frame = saved;
                }
            }
        }
        let Some(active) = self
            .active
            .as_ref()
            .filter(|active| &active.session_id == id)
        else {
            return;
        };
        let command = NativeCommand::Replace {
            session_id: id.clone(),
            scene: build_scene(&active.layout, self.layout_generation_for(id)),
            layout: active.layout.clone(),
            always_on_top: active.always_on_top,
        };
        if self
            .host
            .as_ref()
            .is_none_or(|host| host.send(command).is_err())
        {
            out.push(ControllerEvent::Error(
                "failed to update radial navigation scene".into(),
            ));
            self.retire_host();
        }
    }
    fn navigate(
        &mut self,
        id: &SessionId,
        command: NavigationCommand,
        modifiers: NavigationModifiers,
        out: &mut Vec<ControllerEvent>,
    ) {
        let Some(active) = self
            .active
            .as_ref()
            .filter(|active| &active.session_id == id)
        else {
            return;
        };
        let cells = active
            .layout
            .cells
            .iter()
            .filter(|cell| cell.actionable)
            .map(|cell| {
                (
                    cell.cell_id.clone(),
                    self.cell_role_for_session(&active.session_id, &cell.cell_id),
                )
            })
            .collect();
        let point = active.pointer;
        let generation = self.layout_generation_for(id);
        self.session_event(id, SessionEvent::ModifiersChanged(modifiers), out);
        self.session_event(
            id,
            SessionEvent::Navigate {
                command,
                cells,
                pointer_baseline: point,
                geometry_generation: generation,
            },
            out,
        );
    }
    fn next_layout_generation(&mut self) -> u64 {
        let generation = self.layout_generation;
        self.layout_generation = self.layout_generation.checked_add(1).unwrap_or(1);
        generation
    }
    fn arm_handoff_deadline(&mut self, deadline: u64) {
        self.arm_deadline(DeadlineKey::ActionHandoff, deadline);
    }
    fn arm_deadline(&mut self, key: DeadlineKey, deadline: u64) {
        if self.deadline_scheduler.is_none()
            && let Some(wake) = self.deadline_wake.clone()
        {
            self.deadline_scheduler = HandoffDeadlineScheduler::spawn(wake).ok();
        }
        if let Some(scheduler) = &self.deadline_scheduler {
            scheduler.arm(key, deadline);
        }
    }
    fn cancel_handoff_deadline(&self) {
        if let Some(scheduler) = &self.deadline_scheduler {
            scheduler.cancel(DeadlineKey::ActionHandoff);
        }
    }
    fn sync_dwell_deadline(&mut self) {
        let deadline = self.active.as_ref().and_then(|active| {
            active
                .reducer
                .state
                .dwell_candidate
                .as_ref()
                .map(|(_, deadline)| *deadline)
        });
        if let Some(deadline) = deadline {
            self.arm_deadline(DeadlineKey::Dwell, deadline);
        } else if let Some(scheduler) = &self.deadline_scheduler {
            scheduler.cancel(DeadlineKey::Dwell);
        }
    }
    fn reduce_handoff(&mut self, event: DispatchEvent, out: &mut Vec<ControllerEvent>) {
        let Some(pending) = self.handoff.as_mut() else {
            return;
        };
        let intents = pending.reduce(event);
        self.apply_handoff_intents(intents, out);
    }
    fn apply_handoff_intents(
        &mut self,
        intents: Vec<DispatchIntent>,
        out: &mut Vec<ControllerEvent>,
    ) {
        for intent in intents {
            match intent {
                DispatchIntent::CloseRadial { session_id } => {
                    self.close(CloseReason::ActionHandoff, Some(&session_id));
                }
                DispatchIntent::AwaitInvocationRelease { .. } => {}
                DispatchIntent::Dispatch(request) => {
                    out.push(ControllerEvent::DispatchRequested(request));
                    self.handoff = None;
                    self.cancel_handoff_deadline();
                }
                DispatchIntent::Cancelled { reason } => {
                    out.push(ControllerEvent::Error(format!("radial action {reason}")));
                    self.handoff = None;
                    self.cancel_handoff_deadline();
                }
            }
        }
    }
    fn layout_generation_for(&self, id: &SessionId) -> u64 {
        self.active
            .as_ref()
            .filter(|a| &a.session_id == id)
            .map_or(0, |a| {
                a.reducer
                    .state
                    .stack
                    .last()
                    .map_or(0, |f| f.geometry_generation)
            })
    }
    pub fn close(&mut self, reason: CloseReason, requested: Option<&SessionId>) {
        if reason != CloseReason::ActionHandoff {
            self.handoff = None;
            self.cancel_handoff_deadline();
        }
        let target = self
            .pending
            .as_ref()
            .map(|p| p.session_id.clone())
            .or_else(|| self.active.as_ref().map(|a| a.session_id.clone()));
        let Some(id) = target else { return };
        if requested.is_some_and(|v| v != &id) {
            return;
        }
        if let Some(host) = self.host.as_ref() {
            if host
                .send(NativeCommand::Close {
                    session_id: id,
                    reason,
                })
                .is_err()
            {
                self.retire_host();
            }
        }
    }
    fn retire_host(&mut self) {
        if let Some(mut host) = self.host.take() {
            host.shutdown()
        }
        self.pending = None;
        self.active = None;
    }
    pub fn diagnostics(&self) -> impl Iterator<Item = &DiagnosticRecord> {
        self.diagnostics.iter().flat_map(|v| v.iter())
    }
    fn record(&mut self, session_id: Option<SessionId>, message: String) {
        if let Some(log) = &mut self.diagnostics {
            if log.len() == 64 {
                log.pop_front();
            }
            log.push_back(DiagnosticRecord {
                session_id,
                message,
            });
        }
    }
}
fn owner_cell(owner: InputOwner) -> Option<super::model::CellId> {
    match owner {
        InputOwner::Actionable(id) => Some(id),
        _ => None,
    }
}

fn monotonic_ms() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH
        .get_or_init(Instant::now)
        .elapsed()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn shape_center(
    shape: &super::geometry::HitShape,
    origin: PhysicalPoint,
    scale: ScaleFactor,
) -> PhysicalPoint {
    let logical = match shape {
        super::geometry::HitShape::Circle { center, .. }
        | super::geometry::HitShape::Wedge { center, .. } => *center,
    };
    let offset = scale.logical_to_physical(logical);
    PhysicalPoint {
        x: origin.x + offset.x,
        y: origin.y + offset.y,
    }
}

fn cascade_layout(parent: &LayoutSnapshot, mut child: LayoutSnapshot) -> LayoutSnapshot {
    let mut ancestors = parent.cells.clone();
    for cell in &mut ancestors {
        cell.actionable = false;
    }
    ancestors.extend(child.cells);
    child.cells = ancestors;
    let mut input_regions = parent.input_regions.clone();
    input_regions.extend(child.input_regions);
    child.input_regions = input_regions;
    child.input_extent.min.x = child.input_extent.min.x.min(parent.input_extent.min.x);
    child.input_extent.min.y = child.input_extent.min.y.min(parent.input_extent.min.y);
    child.input_extent.max.x = child.input_extent.max.x.max(parent.input_extent.max.x);
    child.input_extent.max.y = child.input_extent.max.y.max(parent.input_extent.max.y);
    child.visual_extent.min.x = child.visual_extent.min.x.min(parent.visual_extent.min.x);
    child.visual_extent.min.y = child.visual_extent.min.y.min(parent.visual_extent.min.y);
    child.visual_extent.max.x = child.visual_extent.max.x.max(parent.visual_extent.max.x);
    child.visual_extent.max.y = child.visual_extent.max.y.max(parent.visual_extent.max.y);
    child
}

fn augment_special_cells(
    layout: &mut LayoutSnapshot,
    prepared: &BTreeMap<super::model::CellId, PreparedCell>,
    menu: &super::model::MenuDefinition,
) {
    let center_id = super::model::CellId::new("__center");
    if prepared.contains_key(&center_id)
        || menu.center_secondary_action.is_some()
        || menu.center_control.is_some()
    {
        layout.cells.push(CellLayout {
            cell_id: center_id,
            ring_id: RingId::new("__special"),
            label: String::new(),
            icon: Override::Inherit,
            control: menu.center_control,
            shape: HitShape::Circle {
                center: layout.center,
                radius: layout.center_radius,
            },
            actionable: true,
        });
    }
    let background_id = super::model::CellId::new("__background");
    if prepared.contains_key(&background_id) || menu.background_secondary_action.is_some() {
        let radius = (layout.input_extent.max.x - layout.input_extent.min.x)
            .max(layout.input_extent.max.y - layout.input_extent.min.y)
            * 0.5;
        layout.cells.push(CellLayout {
            cell_id: background_id,
            ring_id: RingId::new("__special"),
            label: String::new(),
            icon: Override::Inherit,
            control: None,
            shape: HitShape::Circle {
                center: layout.center,
                radius,
            },
            actionable: true,
        });
    }
}

fn apply_prepared_availability(
    layout: &mut LayoutSnapshot,
    frame: &super::bindings::PreparedMenuFrame,
) {
    for cell in &mut layout.cells {
        let primary = frame.cells.get(&cell.cell_id);
        let alternates: Vec<_> = frame
            .alternates
            .iter()
            .filter(|((id, _), _)| id == &cell.cell_id)
            .map(|((_, gesture), prepared)| (*gesture, prepared))
            .collect();
        if primary.is_some() || !alternates.is_empty() {
            cell.actionable = primary
                .is_some_and(|prepared| prepared.availability == FrozenAvailability::Available)
                || alternates
                    .iter()
                    .any(|(_, prepared)| prepared.availability == FrozenAvailability::Available);
            let mut diagnostics = Vec::new();
            if let Some(PreparedCell {
                availability: FrozenAvailability::Unavailable { reason },
                ..
            }) = primary
            {
                diagnostics.push(format!("Left: {reason}"));
            }
            for (gesture, prepared) in alternates {
                if let FrozenAvailability::Unavailable { reason } = &prepared.availability {
                    diagnostics.push(format!("{}: {reason}", gesture_label(gesture)));
                }
            }
            if !diagnostics.is_empty() {
                let diagnostic = diagnostics.join(" | ");
                cell.label = if cell.label.is_empty() {
                    diagnostic
                } else {
                    format!("{} [{diagnostic}]", cell.label)
                };
            }
        }
    }
}

fn gesture_label(gesture: super::model::ClickGesture) -> &'static str {
    match gesture {
        super::model::ClickGesture::Primary => "Left",
        super::model::ClickGesture::Secondary => "Right",
        super::model::ClickGesture::CtrlPrimary => "Ctrl+Left",
        super::model::ClickGesture::ShiftPrimary => "Shift+Left",
        super::model::ClickGesture::AltPrimary => "Alt+Left",
    }
}

fn cell_role_in_menu(menu: &super::model::MenuDefinition, cell: &super::model::CellId) -> CellRole {
    menu.rings
        .iter()
        .flat_map(|ring| &ring.cells)
        .find(|candidate| &candidate.id == cell)
        .map_or(CellRole::Unavailable, |candidate| {
            match &candidate.content {
                CellContent::Action { .. } | CellContent::Dynamic { .. } => CellRole::Action,
                CellContent::Submenu { .. } => CellRole::Submenu,
                CellContent::Spacer => CellRole::Spacer,
                CellContent::Control { control } => match control {
                    Control::Back => CellRole::Back,
                    Control::Close => CellRole::Close,
                    Control::NextPage => CellRole::NextPage,
                    Control::PreviousPage => CellRole::PreviousPage,
                    Control::Drag => CellRole::Spacer,
                },
            }
        })
}

fn desktop_geometry() -> (PhysicalPoint, PhysicalRect, ScaleFactor) {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::{POINT, RECT};
        use windows::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
        };
        use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        let monitor = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let ok = GetMonitorInfoW(monitor, &mut info).as_bool();
        let r = if ok {
            info.rcWork
        } else {
            RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            }
        };
        let mut dpi_x = 96;
        let mut dpi_y = 96;
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_err() {
            dpi_x = 96;
        }
        let scale = ScaleFactor::new(dpi_x.max(96) as f64 / 96.0)
            .unwrap_or_else(|| ScaleFactor::new(1.0).unwrap());
        return (
            PhysicalPoint {
                x: p.x as f64,
                y: p.y as f64,
            },
            PhysicalRect {
                min: PhysicalPoint {
                    x: r.left as f64,
                    y: r.top as f64,
                },
                max: PhysicalPoint {
                    x: r.right as f64,
                    y: r.bottom as f64,
                },
            },
            scale,
        );
    }
    #[cfg(not(windows))]
    {
        (
            PhysicalPoint { x: 500.0, y: 500.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint {
                    x: 1920.0,
                    y: 1080.0,
                },
            },
            ScaleFactor::new(1.0).unwrap(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    struct Fake {
        sent: Arc<Mutex<Vec<NativeCommand>>>,
        events: Arc<Mutex<VecDeque<NativeEvent>>>,
    }
    impl HostPort for Fake {
        fn send(&self, c: NativeCommand) -> Result<(), String> {
            self.sent.lock().unwrap().push(c);
            Ok(())
        }
        fn try_recv(&self) -> Option<NativeEvent> {
            self.events.lock().unwrap().pop_front()
        }
        fn shutdown(&mut self) {}
    }
    fn controller(
        events: Arc<Mutex<VecDeque<NativeEvent>>>,
        created: Arc<Mutex<usize>>,
    ) -> RadialController {
        RadialController::with_factory(
            Arc::new(RadialDocument::starter()),
            true,
            Arc::new(move || {
                *created.lock().unwrap() += 1;
                Ok(Box::new(Fake {
                    sent: Arc::new(Mutex::new(vec![])),
                    events: events.clone(),
                }))
            }),
        )
    }
    fn open() -> InvocationIntent {
        InvocationIntent::OpenRadial {
            id: InvocationId(4),
            menu_id: MenuId::new("starter"),
            context_token: 0,
            interaction: InteractionMode::StickyClick,
            trigger_still_down: true,
        }
    }
    #[test]
    fn host_is_lazy_and_ready_is_correlated() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let made = Arc::new(Mutex::new(0));
        let mut c = controller(events.clone(), made.clone());
        assert_eq!(*made.lock().unwrap(), 0);
        c.handle_intents(vec![open()], false);
        assert_eq!(*made.lock().unwrap(), 1);
        let id = c.pending.as_ref().unwrap().session_id.clone();
        let generation = c.pending.as_ref().unwrap().generation;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: SessionId::new("stale"),
            layout_generation: generation,
        });
        assert!(c.poll().is_empty());
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: id,
            layout_generation: generation,
        });
        assert!(matches!(
            c.poll().as_slice(),
            [ControllerEvent::Opened {
                invocation_id: InvocationId(4),
                ..
            }]
        ));
    }
    #[test]
    fn asynchronous_preparation_rejects_stale_reply_before_host_open() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let made = Arc::new(Mutex::new(0));
        let mut c = controller(events, made.clone());
        let (tx, rx) = mpsc::channel();
        let (wake, _wake_rx) = mpsc::channel();
        c.preparation = Some(PreparationBridge {
            tx,
            rx,
            wake,
            next_generation: 1,
            waiting: None,
        });
        let requested = c.handle_intents(vec![open()], false);
        let ControllerEvent::PrepareRequested(envelope) = &requested[0] else {
            panic!()
        };
        let prepared_menu = RadialDocument::starter().menus.remove(0);
        let prepared_frame = crate::radial::bindings::PreparedMenuFrame {
            base_menu: prepared_menu.clone(),
            menu: prepared_menu,
            cells: Default::default(),
            static_cells: Default::default(),
            dynamic: Default::default(),
            alternates: Default::default(),
            page: 0,
            page_count: 1,
        };
        let mut stale = RadialPrepareReply {
            generation: PreparationGeneration(99),
            invocation_id: InvocationId(4),
            menu_id: MenuId::new("starter"),
            unavailable: Default::default(),
            dynamic: Default::default(),
            frame: prepared_frame.clone(),
            static_cells: Default::default(),
            frames: [(MenuId::new("starter"), prepared_frame)].into(),
        };
        envelope.reply.send(stale.clone()).unwrap();
        c.poll();
        assert_eq!(*made.lock().unwrap(), 0);
        stale.generation = envelope.request.generation;
        envelope.reply.send(stale).unwrap();
        c.poll();
        assert_eq!(*made.lock().unwrap(), 1);
        assert!(c.pending.is_some());
    }

    #[test]
    fn explicit_direct_menu_never_enables_context_routing() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let made = Arc::new(Mutex::new(0));
        let mut c = controller(events, made);
        let (tx, rx) = mpsc::channel();
        let (wake, _wake_rx) = mpsc::channel();
        c.preparation = Some(PreparationBridge {
            tx,
            rx,
            wake,
            next_generation: 1,
            waiting: None,
        });
        let events = c.handle_intents(
            vec![InvocationIntent::ToggleDirectMenu {
                id: InvocationId(77),
                menu_id: MenuId::new("starter"),
                primary_key: 0x54,
                provenance: crate::radial::invocation::InputProvenance::Physical,
                trigger_still_down: true,
            }],
            false,
        );
        let ControllerEvent::PrepareRequested(envelope) = &events[0] else {
            panic!("expected preparation")
        };
        assert!(!envelope.request.allow_context_rules);
        assert_eq!(envelope.request.requested_menu_id.as_str(), "starter");
    }
    #[test]
    fn backend_failure_does_not_publish_pending_or_active() {
        let mut c = RadialController::with_factory(
            Arc::new(RadialDocument::starter()),
            false,
            Arc::new(|| Err("backend failed".into())),
        );
        assert!(matches!(
            c.handle_intents(vec![open()], false).as_slice(),
            [ControllerEvent::InvocationFailed {
                invocation_id: InvocationId(4),
                ..
            }]
        ));
        assert!(c.host.is_none() && c.pending.is_none() && c.active.is_none());
    }
    #[test]
    fn named_close_rejects_stale_session() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let made = Arc::new(Mutex::new(0));
        let mut c = controller(events, made);
        c.handle_intents(vec![open()], false);
        let current = c.pending.as_ref().unwrap().session_id.clone();
        c.close(CloseReason::Dismissed, Some(&SessionId::new("older")));
        assert_eq!(c.pending.as_ref().unwrap().session_id, current);
    }

    #[test]
    fn direct_same_menu_closes_and_different_menu_replaces() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let sent = Arc::new(Mutex::new(Vec::new()));
        let sent_for_factory = sent.clone();
        let events_for_factory = events.clone();
        let mut document = RadialDocument::starter();
        let mut second = document.menus[0].clone();
        second.id = MenuId::new("second");
        second.name = "Second".into();
        document.menus.push(second);
        let mut controller = RadialController::with_factory(
            Arc::new(document),
            false,
            Arc::new(move || {
                Ok(Box::new(Fake {
                    sent: sent_for_factory.clone(),
                    events: events_for_factory.clone(),
                }))
            }),
        );
        controller.handle_intents(
            vec![InvocationIntent::ToggleDirectMenu {
                id: InvocationId(10),
                menu_id: MenuId::new("starter"),
                primary_key: 0x54,
                provenance: crate::radial::invocation::InputProvenance::Physical,
                trigger_still_down: true,
            }],
            false,
        );
        controller.handle_intents(
            vec![InvocationIntent::ToggleDirectMenu {
                id: InvocationId(11),
                menu_id: MenuId::new("starter"),
                primary_key: 0x54,
                provenance: crate::radial::invocation::InputProvenance::Physical,
                trigger_still_down: true,
            }],
            false,
        );
        assert!(matches!(
            sent.lock().unwrap().last(),
            Some(NativeCommand::Close { .. })
        ));

        controller.handle_intents(
            vec![InvocationIntent::ToggleDirectMenu {
                id: InvocationId(12),
                menu_id: MenuId::new("second"),
                primary_key: 0x54,
                provenance: crate::radial::invocation::InputProvenance::Physical,
                trigger_still_down: true,
            }],
            false,
        );
        assert!(matches!(
            sent.lock().unwrap().last(),
            Some(NativeCommand::Open { .. })
        ));
        assert_eq!(
            controller.pending.as_ref().unwrap().menu_id.as_str(),
            "second"
        );
    }

    #[test]
    fn retained_handoff_scheduler_wakes_without_incidental_native_events() {
        let (wake, rx) = mpsc::channel();
        let scheduler = HandoffDeadlineScheduler::spawn(wake).unwrap();
        scheduler.arm(DeadlineKey::ActionHandoff, monotonic_ms().saturating_add(5));
        assert!(rx.recv_timeout(std::time::Duration::from_secs(1)).is_ok());
        drop(scheduler);
    }

    #[test]
    fn retained_scheduler_owns_one_shot_dwell_and_cancellation() {
        let (wake, rx) = mpsc::channel();
        let scheduler = HandoffDeadlineScheduler::spawn(wake).unwrap();
        scheduler.arm(DeadlineKey::Dwell, monotonic_ms().saturating_add(5));
        assert!(rx.recv_timeout(std::time::Duration::from_secs(1)).is_ok());
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(25))
                .is_err()
        );
        scheduler.arm(DeadlineKey::Dwell, monotonic_ms().saturating_add(5));
        scheduler.cancel(DeadlineKey::Dwell);
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(25))
                .is_err()
        );
    }

    #[test]
    fn cascade_keeps_parent_protective_and_unions_host_extent() {
        let menu = RadialDocument::starter().menus.remove(0);
        let work = PhysicalRect {
            min: PhysicalPoint { x: 0.0, y: 0.0 },
            max: PhysicalPoint {
                x: 1200.0,
                y: 800.0,
            },
        };
        let scale = ScaleFactor::new(1.0).unwrap();
        let parent = layout_menu(
            &menu,
            PhysicalPoint { x: 300.0, y: 400.0 },
            work,
            scale,
            0.5,
        )
        .unwrap();
        let child = layout_menu(
            &menu,
            PhysicalPoint { x: 800.0, y: 400.0 },
            work,
            scale,
            0.5,
        )
        .unwrap();
        let parent_count = parent.cells.len();
        let composed = cascade_layout(&parent, child);
        assert!(
            composed.cells[..parent_count]
                .iter()
                .all(|cell| !cell.actionable)
        );
        assert!(composed.input_extent.min.x <= parent.input_extent.min.x);
        assert!(composed.input_extent.max.x >= parent.input_extent.max.x);
        let HitShape::Circle { center, .. } = parent.cells[0].shape else {
            panic!("starter layout must use circular cells")
        };
        assert_eq!(
            super::super::render::input_owner(&composed, center, false),
            InputOwner::Protective
        );
    }

    #[test]
    fn center_and_background_actions_fill_only_their_owned_regions() {
        let menu = RadialDocument::starter().menus.remove(0);
        let mut layout = layout_menu(
            &menu,
            PhysicalPoint { x: 300.0, y: 300.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 600.0, y: 600.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        let prepared = ["__center", "__background"]
            .into_iter()
            .map(|id| {
                (
                    crate::radial::model::CellId::new(id),
                    PreparedCell {
                        binding: FrozenBinding::Stable(ActionBinding::Contextual {
                            selector: crate::radial::model::TargetSelector::CapturedForeground,
                            action_id: crate::universal_actions::action_ids::WINDOW_ACTIVATE,
                        }),
                        availability: FrozenAvailability::Available,
                        requirement: InteractionRequirement::None,
                        after_action: AfterActionPolicy::KeepOpen,
                        history_query: String::new(),
                    },
                )
            })
            .collect();
        augment_special_cells(&mut layout, &prepared, &menu);
        assert_eq!(
            super::super::render::input_owner(&layout, layout.center, false),
            InputOwner::Actionable(crate::radial::model::CellId::new("__center"))
        );
        let ordinary = &layout.cells[0];
        let HitShape::Circle { center, .. } = ordinary.shape else {
            panic!("starter layout must use circular cells")
        };
        assert_eq!(
            super::super::render::input_owner(&layout, center, false),
            InputOwner::Actionable(ordinary.cell_id.clone())
        );
        let gap = LogicalPoint {
            x: layout.center.x,
            y: layout.center.y + layout.center_radius * 1.5,
        };
        assert_eq!(
            super::super::render::input_owner(&layout, gap, false),
            InputOwner::Actionable(crate::radial::model::CellId::new("__background"))
        );
    }

    #[test]
    fn button_specific_unavailability_is_visible_before_click() {
        let menu = RadialDocument::starter().menus.remove(0);
        let mut layout = layout_menu(
            &menu,
            PhysicalPoint { x: 300.0, y: 300.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 600.0, y: 600.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        let center = crate::radial::model::CellId::new("__center");
        let stable = FrozenBinding::Stable(ActionBinding::Contextual {
            selector: crate::radial::model::TargetSelector::CapturedForeground,
            action_id: crate::universal_actions::action_ids::WINDOW_ACTIVATE,
        });
        let primary = PreparedCell {
            binding: stable.clone(),
            availability: FrozenAvailability::Available,
            requirement: InteractionRequirement::None,
            after_action: AfterActionPolicy::KeepOpen,
            history_query: String::new(),
        };
        let secondary = PreparedCell {
            binding: stable,
            availability: FrozenAvailability::Unavailable {
                reason: "KeepOpen is incompatible with ExternalInput".into(),
            },
            requirement: InteractionRequirement::ExternalInput,
            after_action: AfterActionPolicy::KeepOpen,
            history_query: String::new(),
        };
        let mut cells = BTreeMap::new();
        cells.insert(center.clone(), primary);
        let mut alternates = BTreeMap::new();
        alternates.insert(
            (
                center.clone(),
                crate::radial::model::ClickGesture::Secondary,
            ),
            secondary,
        );
        let frame = crate::radial::bindings::PreparedMenuFrame {
            base_menu: menu.clone(),
            menu: menu.clone(),
            cells: cells.clone(),
            static_cells: cells,
            dynamic: BTreeMap::new(),
            alternates,
            page: 0,
            page_count: 1,
        };
        augment_special_cells(&mut layout, &frame.cells, &menu);
        apply_prepared_availability(&mut layout, &frame);
        let center_layout = layout
            .cells
            .iter()
            .find(|cell| cell.cell_id == center)
            .unwrap();
        assert!(
            center_layout.actionable,
            "valid left action remains available"
        );
        assert!(center_layout.label.contains("Right: KeepOpen"));

        let mut all_unavailable = frame;
        all_unavailable.cells.get_mut(&center).unwrap().availability =
            FrozenAvailability::Unavailable {
                reason: "Left unavailable".into(),
            };
        apply_prepared_availability(&mut layout, &all_unavailable);
        let center_layout = layout
            .cells
            .iter()
            .find(|cell| cell.cell_id == center)
            .unwrap();
        assert!(!center_layout.actionable);
        assert!(center_layout.label.contains("Left: Left unavailable"));
    }
}
