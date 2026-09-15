use super::assets::{AssetService, PrepareVariant, PreparedMedia, reference_identity};
use super::audio::{PreparedRadialSounds, RadialAudioSession, RadialCue, SystemRadialAudioOutput};
use super::bindings::{
    PreparationGeneration, PreparedCell, RadialPrepareEnvelope, RadialPrepareReply,
    RadialPrepareRequest, project_menu_frame_with_style,
};
use super::context::{InvocationContext, WindowIdentity};
use super::dynamic::{FrozenAvailability, FrozenBinding};
use super::font_cache::{
    FontLayoutService, FontRequest, MAX_LAYOUT_CACHE_ENTRIES, SystemFontCatalog,
};
use super::geometry::{
    CellLayout, HitShape, LayoutSnapshot, LogicalPoint, PhysicalPoint, PhysicalRect, ScaleFactor,
    layout_document_menu, layout_menu,
};
use super::handoff::{
    DispatchEvent, DispatchIntent, InteractionRequirement, PendingRadialDispatch,
    RadialDispatchIdentity, RadialDispatchRequest,
};
use super::invocation::InvocationIntent;
use super::model::{
    ActionBinding, AfterActionPolicy, CellContent, ClickGesture, Control, InteractionMode,
    InvocationId, MenuId, Override, RadialDocument, RingId, SessionId, SubmenuPresentation,
    TriggerScope,
};
use super::native::{CloseReason, NativeCommand, NativeEvent, NativeHost};
use super::render::{
    InputOwner, PreparedSceneResources, build_scene, build_scene_prepared,
    build_scene_prepared_selected,
};
use super::session::{
    CellRole, NavigationCommand, NavigationModifiers, PointerButton, SessionEvent, SessionIntent,
    SessionReducer,
};
use super::skin::compile_menu_tree;
use std::collections::VecDeque;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

fn scene_resource_fingerprint(layout: &LayoutSnapshot) -> u64 {
    let mut hasher = DefaultHasher::new();
    // The layout/style snapshot is immutable and contains every resolved media
    // and presentation input. Geometry generation is deliberately excluded so
    // page, submenu, and reopen cycles reuse identical decoded resources.
    format!("{:?}", layout.style).hash(&mut hasher);
    for cell in &layout.cells {
        format!("{:?}{:?}", cell.icon, cell.visual).hash(&mut hasher);
    }
    hasher.finish()
}

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
    application_always_on_top: bool,
    always_on_top: bool,
    activate_on_show: bool,
    context: InvocationContext,
    trigger_still_down: bool,
    prepared: Option<RadialPrepareReply>,
    resources: PreparedSceneResources,
    sounds: PreparedRadialSounds,
}
struct ActiveSession {
    invocation_id: InvocationId,
    session_id: SessionId,
    menu_id: MenuId,
    layout: LayoutSnapshot,
    reducer: SessionReducer,
    pointer: LogicalPoint,
    application_always_on_top: bool,
    always_on_top: bool,
    activate_on_show: bool,
    context: InvocationContext,
    trigger_still_down: bool,
    owned_item_input: Option<InvocationId>,
    prepared: Option<RadialPrepareReply>,
    navigation_layouts: BTreeMap<MenuId, LayoutSnapshot>,
    navigation_frames: BTreeMap<MenuId, super::bindings::PreparedMenuFrame>,
    resources: PreparedSceneResources,
    audio: Option<RadialAudioSession<SystemRadialAudioOutput>>,
    audio_generation: u64,
    navigation_sounds: BTreeMap<MenuId, PreparedRadialSounds>,
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

#[derive(Clone, Debug)]
struct QueuedItemActivation {
    id: InvocationId,
    menu_id: MenuId,
    cell_id: super::model::CellId,
    gesture: ClickGesture,
    source: crate::commands::ActivationSource,
    trigger_still_down: bool,
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
    queued_item_activation: Option<QueuedItemActivation>,
    release_aliases: BTreeMap<InvocationId, InvocationId>,
    release_waits: BTreeMap<InvocationId, BTreeSet<InvocationId>>,
    asset_service: Option<AssetService>,
    font_service: Option<FontLayoutService>,
    font_catalog: Option<SystemFontCatalog>,
    visible_resource_diagnostics: BTreeSet<String>,
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
            queued_item_activation: None,
            release_aliases: BTreeMap::new(),
            asset_service: None,
            font_service: None,
            font_catalog: None,
            visible_resource_diagnostics: BTreeSet::new(),
            release_waits: BTreeMap::new(),
        }
    }
    pub fn replace_document(&mut self, document: Arc<RadialDocument>) {
        self.invalidate_resources();
        if let Some(service) = &mut self.asset_service {
            service.replace_search_roots(document.media_search_roots.clone());
        }
        self.document = document;
    }

    /// Installs the preparation-only resource services used by the production
    /// controller. Tests with synthetic hosts can intentionally omit this.
    pub fn configure_resources(&mut self, application_data: PathBuf) {
        self.asset_service = Some(AssetService::new(
            application_data,
            self.document.media_search_roots.clone(),
        ));
        let catalog = SystemFontCatalog::discover();
        self.font_service = Some(FontLayoutService::with_catalog(
            catalog.clone(),
            MAX_LAYOUT_CACHE_ENTRIES,
        ));
        self.font_catalog = Some(catalog);
    }

    pub fn font_families(&self) -> Vec<String> {
        self.font_catalog
            .as_ref()
            .map(SystemFontCatalog::family_names)
            .unwrap_or_default()
    }

    /// Invalidate every prepared/layout resource owned by the active radial
    /// generation. Asset/font/compositor caches are invocation-local today;
    /// closing the tree is therefore the fail-closed cache invalidation boundary.
    pub fn invalidate_resources(&mut self) {
        self.visible_resource_diagnostics.clear();
        self.handoff = None;
        self.cancel_handoff_deadline();
        if let Some(bridge) = &mut self.preparation {
            bridge.waiting = None;
        }
        self.close(CloseReason::SettingsReload, None);
        self.queued_item_activation = None;
        self.release_aliases.clear();
        self.release_waits.clear();
        self.layout_generation = self.layout_generation.checked_add(1).unwrap_or(1);
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
        self.queued_item_activation = None;
        self.release_aliases.clear();
        self.release_waits.clear();
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
                    let release_invocation = self.release_aliases.remove(&id).unwrap_or(id);
                    if let Some(waiting) = self
                        .preparation
                        .as_mut()
                        .and_then(|bridge| bridge.waiting.as_mut())
                        .filter(|waiting| waiting.invocation_id == id)
                    {
                        waiting.trigger_still_down = false;
                    }
                    if let Some(pending) = self
                        .pending
                        .as_mut()
                        .filter(|pending| pending.invocation_id == id)
                    {
                        pending.trigger_still_down = false;
                    }
                    if let Some(queued) = self
                        .queued_item_activation
                        .as_mut()
                        .filter(|queued| queued.id == id)
                    {
                        queued.trigger_still_down = false;
                    }
                    if let Some(active) = self.active.as_mut().filter(|active| {
                        active.invocation_id == id || active.owned_item_input == Some(id)
                    }) {
                        if active.invocation_id == id {
                            active.trigger_still_down = false;
                        }
                        if active.owned_item_input == Some(id) {
                            active.owned_item_input = None;
                        }
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
                    let fully_released = self
                        .release_waits
                        .get_mut(&release_invocation)
                        .map(|keys| {
                            keys.remove(&id);
                            keys.is_empty()
                        })
                        .unwrap_or(true);
                    if fully_released {
                        self.release_waits.remove(&release_invocation);
                        self.reduce_handoff(
                            DispatchEvent::InvocationReleased {
                                invocation_id: release_invocation,
                            },
                            &mut out,
                        );
                    }
                }
                InvocationIntent::Navigate {
                    session_id,
                    command,
                    modifiers,
                } => {
                    self.navigate(&session_id, command, modifiers, &mut out);
                }
                InvocationIntent::ActivateItem {
                    id,
                    menu_id,
                    cell_id,
                    gesture,
                    scope,
                    source,
                    trigger_still_down,
                } => {
                    let matches_active = self
                        .active
                        .as_ref()
                        .is_some_and(|active| active.menu_id == menu_id);
                    if matches_active {
                        if let Some(active) = self.active.as_mut() {
                            if trigger_still_down {
                                active.owned_item_input = Some(id);
                            }
                        }
                        self.activate_item(&menu_id, &cell_id, gesture, source, &mut out);
                    } else if scope == TriggerScope::Global {
                        let interaction = self
                            .document
                            .menus
                            .iter()
                            .find(|menu| menu.id == menu_id)
                            .map_or(InteractionMode::StickyClick, |menu| menu.interaction);
                        self.queued_item_activation = Some(QueuedItemActivation {
                            id,
                            menu_id: menu_id.clone(),
                            cell_id,
                            gesture,
                            source,
                            trigger_still_down,
                        });
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
                        );
                    }
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
        let application_always_on_top = always_on_top;
        let (always_on_top, activate_on_show) =
            resolve_window_options(&self.document, &menu, application_always_on_top);
        let mut layout =
            match layout_document_menu(&self.document, &menu, anchor, work, scale, 0.55) {
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
        let (resources, sounds, resource_diagnostics) =
            self.prepare_scene_resources(&menu, &layout, generation);
        out.extend(resource_diagnostics.into_iter().map(ControllerEvent::Error));
        let scene = if self.asset_service.is_some() || self.font_service.is_some() {
            build_scene_prepared(&layout, generation, &resources)
        } else {
            build_scene(&layout, generation)
        };
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
            activate_on_show,
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
                    application_always_on_top,
                    always_on_top,
                    activate_on_show,
                    context,
                    trigger_still_down,
                    prepared,
                    resources,
                    sounds,
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

    fn prepare_scene_resources(
        &mut self,
        menu: &super::model::MenuDefinition,
        layout: &LayoutSnapshot,
        _generation: u64,
    ) -> (PreparedSceneResources, PreparedRadialSounds, Vec<String>) {
        let mut resources = PreparedSceneResources::default();
        let variant = PrepareVariant {
            effective_style: scene_resource_fingerprint(layout),
            dpi_milli: (layout.scale_factor.get() * 1_000.0)
                .round()
                .clamp(1.0, u32::MAX as f64) as u32,
            logical_width_milli: (layout.style.item_size.max(1.0) * 1_000.0) as u32,
            logical_height_milli: (layout.style.item_size.max(1.0) * 1_000.0) as u32,
            quality: layout.style.image_quality,
        };
        let mut diagnostics = Vec::new();
        if let Some(service) = &mut self.asset_service {
            let mut refs = vec![
                layout.style.item_glow.clone(),
                layout.style.menu_outer_rim.clone(),
                layout.style.menu_background.clone(),
                layout.style.menu_foreground.clone(),
                layout.style.center_background.clone(),
                layout.style.center_image.clone(),
            ];
            for cell in &layout.cells {
                refs.extend([
                    cell.icon.clone(),
                    cell.visual.item_background.clone(),
                    cell.visual.item_foreground.clone(),
                    cell.visual.item_shadow.clone(),
                    cell.visual.submenu_indicator.clone(),
                ]);
            }
            for reference in refs {
                if let Override::Value(reference) = reference {
                    match service.prepare(
                        &reference,
                        super::model::MediaKind::Image,
                        &self.document.assets,
                        variant,
                    ) {
                        Ok(snapshot) => {
                            resources
                                .media
                                .insert(reference_identity(&reference), snapshot);
                        }
                        Err(error) => diagnostics.push(format!(
                            "radial image {} unavailable: {error}",
                            reference_identity(&reference)
                        )),
                    }
                }
            }
        }
        if let Some(service) = &mut self.font_service {
            for cell in &layout.cells {
                let request = FontRequest {
                    family: (!cell.visual.font_family.is_empty())
                        .then(|| cell.visual.font_family.clone()),
                    size_milli: (cell.visual.font_size.max(1.0) * 1_000.0) as u32,
                    bold: cell.visual.bold,
                    italic: cell.visual.italic,
                    dpi_milli: variant.dpi_milli,
                    max_width_milli: (layout.style.item_size.max(1.0)
                        * cell.visual.text_box_scale
                        * 1_000.0) as u32,
                };
                let prepared_label = service.prepare(&cell.label, request.clone());
                diagnostics.extend(
                    prepared_label.diagnostics.iter().map(|diagnostic| {
                        format!("radial font for {}: {diagnostic:?}", cell.cell_id)
                    }),
                );
                resources.text.insert(cell.cell_id.clone(), prepared_label);
                if let Some(definition) = menu
                    .rings
                    .iter()
                    .flat_map(|ring| &ring.cells)
                    .find(|candidate| candidate.id == cell.cell_id)
                {
                    let explicit = match &definition.tooltip {
                        Override::Value(value) if !value.is_empty() => Some(value.as_str()),
                        _ => None,
                    };
                    let tooltip = match cell.visual.tooltip_mode {
                        super::model::TooltipMode::Disabled => None,
                        super::model::TooltipMode::Explicit => explicit,
                        super::model::TooltipMode::Automatic => {
                            explicit.or(Some(cell.label.as_str()))
                        }
                    };
                    if let Some(tooltip) = tooltip {
                        resources
                            .tooltips
                            .insert(cell.cell_id.clone(), service.prepare(tooltip, request));
                    }
                }
            }
        }
        let mut sounds = PreparedRadialSounds::default();
        if let (Some(service), Ok(style)) = (
            &mut self.asset_service,
            compile_menu_tree(&self.document, menu),
        ) {
            let refs = [
                (&style.menu.values.sounds.on_show, &mut sounds.open),
                (&style.menu.values.sounds.on_close, &mut sounds.close),
                (&style.menu.values.sounds.on_select, &mut sounds.select),
                (
                    &style.menu.values.sounds.on_submenu_show,
                    &mut sounds.submenu_show,
                ),
                (
                    &style.menu.values.sounds.on_submenu_close,
                    &mut sounds.submenu_close,
                ),
            ];
            for (reference, destination) in refs {
                if let Override::Value(reference) = reference {
                    match service.prepare(
                        reference,
                        super::model::MediaKind::Sound,
                        &self.document.assets,
                        variant,
                    ) {
                        Ok(snapshot) => {
                            if let PreparedMedia::Sound(sound) = &*snapshot.media {
                                *destination = Some(Arc::clone(&sound.wav));
                            }
                        }
                        Err(error) => diagnostics.push(format!(
                            "radial sound {} unavailable: {error}",
                            reference_identity(reference)
                        )),
                    }
                }
            }
        }
        diagnostics
            .retain(|diagnostic| self.visible_resource_diagnostics.insert(diagnostic.clone()));
        for diagnostic in &diagnostics {
            self.record(None, diagnostic.clone());
        }
        (resources, sounds, diagnostics)
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
                let root_sounds = p.sounds.clone();
                let mut audio = RadialAudioSession::new(
                    session_id.clone(),
                    layout_generation,
                    p.sounds,
                    SystemRadialAudioOutput,
                );
                audio.cue(
                    &session_id,
                    layout_generation,
                    RadialCue::Open,
                    monotonic_ms(),
                );
                self.active = Some(ActiveSession {
                    invocation_id,
                    session_id: session_id.clone(),
                    menu_id: p.menu_id.clone(),
                    layout: p.layout,
                    reducer,
                    pointer,
                    application_always_on_top: p.application_always_on_top,
                    always_on_top: p.always_on_top,
                    activate_on_show: p.activate_on_show,
                    context: p.context,
                    trigger_still_down: p.trigger_still_down,
                    owned_item_input: self
                        .queued_item_activation
                        .as_ref()
                        .filter(|queued| queued.id == p.invocation_id && queued.trigger_still_down)
                        .map(|queued| queued.id),
                    prepared: p.prepared,
                    navigation_layouts,
                    navigation_frames,
                    resources: p.resources,
                    audio: Some(audio),
                    audio_generation: layout_generation,
                    navigation_sounds: BTreeMap::from([(p.menu_id.clone(), root_sounds)]),
                });
                self.record(Some(session_id.clone()), "native host ready".into());
                out.push(ControllerEvent::Opened {
                    invocation_id,
                    session_id,
                });
                if let Some(queued) = self.queued_item_activation.take() {
                    if queued.id == invocation_id {
                        self.activate_item(
                            &queued.menu_id,
                            &queued.cell_id,
                            queued.gesture,
                            queued.source,
                            out,
                        );
                    } else {
                        self.queued_item_activation = Some(queued);
                    }
                }
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
                    if self
                        .queued_item_activation
                        .as_ref()
                        .is_some_and(|queued| queued.id == p.invocation_id)
                    {
                        self.queued_item_activation = None;
                    }
                    return;
                }
                if let Some(mut a) = self.active.take() {
                    if a.session_id != session_id {
                        self.active = Some(a);
                        return;
                    }
                    if let Some(audio) = a.audio.take() {
                        if !audio.finish_close(&session_id, a.audio_generation, monotonic_ms()) {
                            out.push(ControllerEvent::Error(
                                "radial close audio retirement queue is saturated".into(),
                            ));
                        }
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
                if let Some(mut active) = self.active.take()
                    && let Some(audio) = active.audio.take()
                    && !audio.stop(&active.session_id, active.audio_generation)
                {
                    out.push(ControllerEvent::Error(
                        "radial audio stop retirement queue is saturated".into(),
                    ));
                }
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
                if owner != super::render::InputOwner::Exterior {
                    self.session_event(&session_id, SessionEvent::MenuInteraction, out);
                }
                let previous_selection = self
                    .active
                    .as_ref()
                    .filter(|active| active.session_id == session_id)
                    .and_then(|active| active.reducer.state.hovered.clone());
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
                let current_selection = self
                    .active
                    .as_ref()
                    .filter(|active| active.session_id == session_id)
                    .and_then(|active| active.reducer.state.hovered.clone());
                if current_selection != previous_selection {
                    if let Some(cell) = current_selection.clone()
                        && let Some(active) = self
                            .active
                            .as_mut()
                            .filter(|active| active.session_id == session_id)
                        && let Some(audio) = &mut active.audio
                    {
                        audio.cue(
                            &session_id,
                            active.audio_generation,
                            RadialCue::Select(cell),
                            monotonic_ms(),
                        );
                    }
                    self.refresh_active_scene(&session_id, out);
                }
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
                let had_hover = self
                    .active
                    .as_ref()
                    .filter(|active| active.session_id == session_id)
                    .is_some_and(|active| active.reducer.state.hovered.is_some());
                self.session_event(&session_id, SessionEvent::OutsideInteraction, out);
                if had_hover {
                    self.refresh_active_scene(&session_id, out);
                }
            }
            NativeEvent::PointerDown {
                session_id,
                owner,
                point,
                button,
            } => {
                if owner != super::render::InputOwner::Exterior {
                    self.session_event(&session_id, SessionEvent::MenuInteraction, out);
                }
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

    fn activate_item(
        &mut self,
        menu_id: &MenuId,
        cell_id: &super::model::CellId,
        gesture: ClickGesture,
        source: crate::commands::ActivationSource,
        out: &mut Vec<ControllerEvent>,
    ) {
        if self.handoff.is_some() {
            return;
        }
        let Some(session_id) = self
            .active
            .as_ref()
            .filter(|active| &active.menu_id == menu_id)
            .map(|active| active.session_id.clone())
        else {
            return;
        };
        let generation = self.layout_generation_for(&session_id);
        let role = self
            .active
            .as_ref()
            .and_then(|active| {
                let modifiers = NavigationModifiers {
                    control: gesture == ClickGesture::CtrlPrimary,
                    shift: gesture == ClickGesture::ShiftPrimary,
                    alt: gesture == ClickGesture::AltPrimary,
                    alt_gr: false,
                };
                let button = if gesture == ClickGesture::Secondary {
                    PointerButton::Secondary
                } else {
                    PointerButton::Primary
                };
                if let Some(control) = cell_gesture_control(
                    active.prepared.as_ref().map_or_else(
                        || {
                            self.document
                                .menus
                                .iter()
                                .find(|menu| menu.id == active.menu_id)
                        },
                        |reply| Some(&reply.frame.menu),
                    ),
                    cell_id,
                    button,
                    modifiers,
                ) {
                    return Some(Err(control_role(control)));
                }
                self.prepared_action(active, cell_id, button, modifiers)
                    .map(Ok)
            })
            .map_or(CellRole::Unavailable, |prepared| match prepared {
                Err(role) => role,
                Ok(prepared) => {
                    if prepared.availability == FrozenAvailability::Available {
                        CellRole::Action
                    } else {
                        CellRole::Unavailable
                    }
                }
            });
        self.session_event(
            &session_id,
            SessionEvent::ActivateItem {
                cell: cell_id.clone(),
                role,
                gesture,
                source,
                geometry_generation: generation,
            },
            out,
        );
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
                    item_trigger_still_down,
                    owned_item_input,
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
                                    active.owned_item_input.is_some(),
                                    active.owned_item_input,
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
                let release_required = trigger_still_down || item_trigger_still_down;
                if release_required {
                    let waits = self.release_waits.entry(invocation_id).or_default();
                    if trigger_still_down {
                        waits.insert(invocation_id);
                    }
                    if let Some(item_id) = owned_item_input {
                        waits.insert(item_id);
                        self.release_aliases.insert(item_id, invocation_id);
                    }
                }
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
        if let Some(control) = special_surface_control(
            active.prepared.as_ref().map_or_else(
                || {
                    self.document
                        .menus
                        .iter()
                        .find(|menu| menu.id == active.menu_id)
                },
                |reply| Some(&reply.frame.menu),
            ),
            cell,
            button,
        ) {
            return control_role(control);
        }
        if let Some(control) = cell_gesture_control(
            active.prepared.as_ref().map_or_else(
                || {
                    self.document
                        .menus
                        .iter()
                        .find(|menu| menu.id == active.menu_id)
                },
                |reply| Some(&reply.frame.menu),
            ),
            cell,
            button,
            active.reducer.state.modifiers,
        ) {
            return control_role(control);
        }
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
        let menu = active
            .prepared
            .as_ref()
            .map(|reply| &reply.frame.menu)
            .or_else(|| {
                self.document
                    .menus
                    .iter()
                    .find(|menu| menu.id == active.menu_id)
            });
        if let Some(control) = special_surface_control(menu, cell, PointerButton::Primary) {
            return control_role(control);
        }
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
            .map(|alternate| {
                let policy = if gesture == ClickGesture::Secondary
                    && alternate.after_action == AfterActionPolicy::Inherit
                {
                    cell.secondary_after_action
                } else {
                    alternate.after_action
                };
                (alternate.action.clone(), policy)
            });
        let (binding, after_action) = alternate.or_else(|| {
            (gesture == ClickGesture::Primary).then(|| (binding.clone(), cell.after_action))
        })?;
        if matches!(binding, ActionBinding::Contextual { .. }) {
            // Contextual actions are safe only when supplied by the correlated
            // preparation reply with its captured WindowTargetIdentity.
            return None;
        }
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
        let Ok(mut layout) =
            layout_document_menu(&self.document, &child, anchor, work, scale, 0.55)
        else {
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
        let application_always_on_top = active.application_always_on_top;
        let (always_on_top, activate_on_show) =
            resolve_window_options(&self.document, &child, application_always_on_top);
        let generation = self.next_layout_generation();
        let (resources, sounds, diagnostics) =
            self.prepare_scene_resources(&child, &layout, generation);
        out.extend(diagnostics.into_iter().map(ControllerEvent::Error));
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
            active.always_on_top = always_on_top;
            active.activate_on_show = activate_on_show;
            active
                .navigation_layouts
                .insert(menu_id.clone(), layout.clone());
            active.layout = layout;
            active.resources = resources;
            if let Some(audio) = &mut active.audio {
                audio.replace_sounds(sounds.clone());
                audio.cue(
                    id,
                    active.audio_generation,
                    RadialCue::SubmenuShow(menu_id.clone()),
                    monotonic_ms(),
                );
            }
            active.navigation_sounds.insert(menu_id.clone(), sounds);
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
        let mut resources_changed = false;
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
            && prepared.frame.page != page
        {
            let base = prepared.frame.base_menu.clone();
            let alternates = prepared.frame.alternates.clone();
            let effective_style = super::skin::compile_menu_tree(&self.document, &base).ok();
            prepared.frame = project_menu_frame_with_style(
                &base,
                prepared.frame.static_cells.clone(),
                &prepared.frame.dynamic,
                page,
                effective_style.as_ref(),
            );
            if let Some(current) = active.reducer.state.stack.last_mut() {
                current.page = prepared.frame.page;
                current.page_count = prepared.frame.page_count;
            }
            prepared.frame.alternates = alternates;
            let (_, work, _) = desktop_geometry();
            if let Ok(layout) = layout_document_menu(
                &self.document,
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
                resources_changed = true;
            }
        }
        let previous_menu = self
            .active
            .as_ref()
            .filter(|active| &active.session_id == id)
            .map(|active| active.menu_id.clone());
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
            let restored_window_options = self
                .document
                .menus
                .iter()
                .find(|menu| menu.id == frame.menu_id)
                .map(|menu| {
                    resolve_window_options(&self.document, menu, active.application_always_on_top)
                });
            if let Some(active) = self
                .active
                .as_mut()
                .filter(|active| &active.session_id == id)
            {
                if let Some(layout) = active.navigation_layouts.get(&frame.menu_id).cloned() {
                    active.menu_id = frame.menu_id.clone();
                    active.layout = layout;
                    resources_changed = true;
                }
                if let Some((always_on_top, activate_on_show)) = restored_window_options {
                    active.always_on_top = always_on_top;
                    active.activate_on_show = activate_on_show;
                }
                if let Some(prepared) = active.prepared.as_mut()
                    && let Some(saved) = active.navigation_frames.get(&frame.menu_id).cloned()
                {
                    prepared.frame = saved;
                }
                if let Some(audio) = &mut active.audio {
                    if let Some(previous) = previous_menu.as_ref() {
                        audio.cue(
                            id,
                            active.audio_generation,
                            RadialCue::SubmenuClose(previous.clone()),
                            monotonic_ms(),
                        );
                    }
                    if let Some(sounds) = active.navigation_sounds.get(&frame.menu_id).cloned() {
                        audio.replace_sounds(sounds);
                    }
                }
            }
        }
        let resource_input = resources_changed
            .then(|| {
                self.active
                    .as_ref()
                    .filter(|active| &active.session_id == id)
                    .and_then(|active| {
                        self.document
                            .menus
                            .iter()
                            .find(|menu| menu.id == active.menu_id)
                            .cloned()
                            .or_else(|| {
                                active
                                    .prepared
                                    .as_ref()
                                    .map(|reply| reply.frame.menu.clone())
                            })
                            .map(|menu| (menu, active.layout.clone()))
                    })
            })
            .flatten();
        if let Some((menu, layout)) = resource_input {
            let generation = self.layout_generation_for(id);
            let (resources, sounds, diagnostics) =
                self.prepare_scene_resources(&menu, &layout, generation);
            out.extend(diagnostics.into_iter().map(ControllerEvent::Error));
            if let Some(active) = self
                .active
                .as_mut()
                .filter(|active| &active.session_id == id)
            {
                active.resources = resources;
                active
                    .navigation_sounds
                    .insert(menu.id.clone(), sounds.clone());
                if let Some(audio) = &mut active.audio {
                    audio.replace_sounds(sounds);
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
        let command = NativeCommand::Present {
            session_id: id.clone(),
            scene: build_scene_prepared_selected(
                &active.layout,
                self.layout_generation_for(id),
                &active.resources,
                active
                    .reducer
                    .state
                    .hovered
                    .as_ref()
                    .or(active.reducer.state.selected.as_ref()),
            ),
            layout: active.layout.clone(),
            always_on_top: active.always_on_top,
            activate_on_show: active.activate_on_show,
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
                    self.release_aliases.clear();
                    self.release_waits.clear();
                }
                DispatchIntent::Cancelled { reason } => {
                    out.push(ControllerEvent::Error(format!("radial action {reason}")));
                    self.handoff = None;
                    self.cancel_handoff_deadline();
                    self.release_aliases.clear();
                    self.release_waits.clear();
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
            self.release_aliases.clear();
            self.release_waits.clear();
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
        let mut audio_retirement_failed = false;
        if let Some(active) = &mut self.active
            && let Some(audio) = active.audio.take()
        {
            audio_retirement_failed = !audio.stop(&active.session_id, active.audio_generation);
        }
        if audio_retirement_failed {
            self.record(
                None,
                "radial audio stop retirement queue is saturated".into(),
            );
        }
        if let Some(mut host) = self.host.take() {
            host.shutdown()
        }
        self.pending = None;
        self.active = None;
    }
    pub fn diagnostics(&self) -> impl Iterator<Item = &DiagnosticRecord> {
        self.diagnostics.iter().flat_map(|v| v.iter())
    }
    pub fn active_input_scope(&self) -> Option<(SessionId, MenuId)> {
        self.active.as_ref().and_then(|active| {
            (active.reducer.state.keyboard_ownership
                != super::session::KeyboardOwnership::ExternalApplication)
                .then(|| (active.session_id.clone(), active.menu_id.clone()))
        })
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

fn resolve_window_options(
    document: &RadialDocument,
    menu: &super::model::MenuDefinition,
    application_always_on_top: bool,
) -> (bool, bool) {
    let Some(style) = super::skin::compile_menu_tree(document, menu).ok() else {
        return (application_always_on_top, false);
    };
    let always_on_top = if style.menu.source(super::skin::StyleField::AlwaysOnTop)
        == Some(&super::skin::StyleSource::ApplicationFallback)
    {
        application_always_on_top
    } else {
        super::skin::resolved_bool(&style.menu.values.window.always_on_top)
    };
    let activate_on_show = super::skin::resolved_bool(&style.menu.values.window.activate_on_show);
    (always_on_top, activate_on_show)
}

fn augment_special_cells(
    layout: &mut LayoutSnapshot,
    prepared: &BTreeMap<super::model::CellId, PreparedCell>,
    menu: &super::model::MenuDefinition,
) {
    let special_visual = layout
        .cells
        .first()
        .map(|cell| cell.visual.clone())
        .unwrap_or_default();
    let center_id = super::model::CellId::new("__center");
    if prepared.contains_key(&center_id)
        || menu.center_secondary_action.is_some()
        || menu.center_control.is_some()
        || menu.center_secondary_control.is_some()
    {
        layout.cells.push(CellLayout {
            cell_id: center_id,
            ring_id: RingId::new("__special"),
            label: String::new(),
            icon: Override::Inherit,
            control: menu.center_control,
            secondary_control: menu.center_secondary_control,
            shape: HitShape::Circle {
                center: layout.center,
                radius: layout.center_radius,
            },
            actionable: true,
            visual: special_visual.clone(),
        });
    }
    let background_id = super::model::CellId::new("__background");
    if prepared.contains_key(&background_id)
        || menu.background_secondary_action.is_some()
        || menu.background_control.is_some()
        || menu.background_secondary_control.is_some()
    {
        let radius = (layout.input_extent.max.x - layout.input_extent.min.x)
            .max(layout.input_extent.max.y - layout.input_extent.min.y)
            * 0.5;
        layout.cells.push(CellLayout {
            cell_id: background_id,
            ring_id: RingId::new("__special"),
            label: String::new(),
            icon: Override::Inherit,
            control: menu.background_control,
            secondary_control: menu.background_secondary_control,
            shape: HitShape::Circle {
                center: layout.center,
                radius,
            },
            actionable: true,
            visual: special_visual,
        });
    }
}

fn special_surface_control(
    menu: Option<&super::model::MenuDefinition>,
    cell: &super::model::CellId,
    button: PointerButton,
) -> Option<Control> {
    let menu = menu?;
    match (cell.as_str(), button) {
        ("__center", PointerButton::Primary) => menu.center_control,
        ("__center", PointerButton::Secondary) => menu.center_secondary_control,
        ("__background", PointerButton::Primary) => menu.background_control,
        ("__background", PointerButton::Secondary) => menu.background_secondary_control,
        _ => None,
    }
}

fn control_role(control: Control) -> CellRole {
    match control {
        Control::Back => CellRole::Back,
        Control::Close => CellRole::Close,
        Control::NextPage => CellRole::NextPage,
        Control::PreviousPage => CellRole::PreviousPage,
        Control::Drag => CellRole::Spacer,
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
            if let Some(reason) = primary.and_then(|prepared| prepared.availability.reason()) {
                diagnostics.push(format!("Left: {reason}"));
            }
            for (gesture, prepared) in alternates {
                if let Some(reason) = prepared.availability.reason() {
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

fn cell_gesture_control(
    menu: Option<&super::model::MenuDefinition>,
    cell_id: &super::model::CellId,
    button: PointerButton,
    modifiers: NavigationModifiers,
) -> Option<Control> {
    let gesture = if button == PointerButton::Secondary {
        ClickGesture::Secondary
    } else if modifiers.control {
        ClickGesture::CtrlPrimary
    } else if modifiers.shift {
        ClickGesture::ShiftPrimary
    } else if modifiers.alt || modifiers.alt_gr {
        ClickGesture::AltPrimary
    } else {
        ClickGesture::Primary
    };
    menu?
        .rings
        .iter()
        .flat_map(|ring| &ring.cells)
        .find(|cell| &cell.id == cell_id)?
        .alternate_controls
        .iter()
        .find(|binding| binding.gesture == gesture)
        .map(|binding| binding.control)
}

pub(crate) fn desktop_geometry() -> (PhysicalPoint, PhysicalRect, ScaleFactor) {
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
    use image::{DynamicImage, ImageOutputFormat, Rgba, RgbaImage};
    use std::io::Cursor;
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
    fn outside_relinquish_removes_active_local_input_scope() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let made = Arc::new(Mutex::new(0));
        let mut controller = controller(events.clone(), made);
        controller.handle_intents(vec![open()], false);
        let pending = controller.pending.as_ref().unwrap();
        let session_id = pending.session_id.clone();
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: session_id.clone(),
            layout_generation: pending.generation,
        });
        controller.poll();
        assert!(controller.active_input_scope().is_some());
        let mut output = Vec::new();
        controller.session_event(&session_id, SessionEvent::OutsideInteraction, &mut output);
        assert!(controller.active_input_scope().is_none());
    }

    #[test]
    fn owned_pointer_reentry_restores_local_input_scope_but_exterior_does_not() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let made = Arc::new(Mutex::new(0));
        let mut controller = controller(events.clone(), made);
        controller.handle_intents(vec![open()], false);
        let pending = controller.pending.as_ref().unwrap();
        let session_id = pending.session_id.clone();
        let generation = pending.generation;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: session_id.clone(),
            layout_generation: generation,
        });
        controller.poll();
        events.lock().unwrap().push_back(NativeEvent::PointerLeft {
            session_id: session_id.clone(),
        });
        controller.poll();
        assert!(controller.active_input_scope().is_none());
        let point = controller.active.as_ref().unwrap().layout.center;
        events.lock().unwrap().push_back(NativeEvent::PointerMoved {
            session_id: session_id.clone(),
            owner: InputOwner::Exterior,
            point,
        });
        controller.poll();
        assert!(controller.active_input_scope().is_none());
        events.lock().unwrap().push_back(NativeEvent::PointerDown {
            session_id,
            owner: InputOwner::Protective,
            point,
            button: PointerButton::Primary,
        });
        controller.poll();
        assert!(controller.active_input_scope().is_some());
    }

    #[test]
    fn resource_failure_is_a_normal_controller_event_when_debug_logging_is_disabled() {
        let directory = tempfile::tempdir().unwrap();
        let mut document = RadialDocument::starter();
        document.skins[0].style.values.images.menu_background =
            Override::Value(super::super::model::MediaReference::ExternalFile {
                path: directory
                    .path()
                    .join("missing.png")
                    .to_string_lossy()
                    .into_owned(),
            });
        let mut controller = RadialController::with_factory(
            Arc::new(document),
            false,
            Arc::new(|| {
                Ok(Box::new(Fake {
                    sent: Arc::new(Mutex::new(Vec::new())),
                    events: Arc::new(Mutex::new(VecDeque::new())),
                }))
            }),
        );
        controller.configure_resources(directory.path().to_path_buf());
        let output = controller.handle_intents(vec![open()], false);
        assert!(output.iter().any(|event| matches!(event, ControllerEvent::Error(message) if message.contains("missing.png"))));
        assert!(controller.diagnostics().next().is_none());
    }

    #[test]
    fn resource_cache_fingerprint_ignores_generation_and_changes_with_effective_style() {
        let document = RadialDocument::starter();
        let menu = &document.menus[0];
        let mut layout = layout_menu(
            menu,
            PhysicalPoint { x: 300.0, y: 300.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 600.0, y: 600.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        let first = scene_resource_fingerprint(&layout);
        assert_eq!(first, scene_resource_fingerprint(&layout));
        layout.cells[0].visual.icon_opacity = 0.25;
        assert_ne!(first, scene_resource_fingerprint(&layout));
    }

    #[test]
    fn production_open_prepares_media_real_glyphs_tooltips_and_audio_before_native_open() {
        let directory = tempfile::tempdir().unwrap();
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba([4, 8, 12, 255])))
            .write_to(&mut png, ImageOutputFormat::Png)
            .unwrap();
        let image_path = directory.path().join("skin.png");
        std::fs::write(&image_path, png.into_inner()).unwrap();
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF\x28\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x40\x1f\0\0\x40\x1f\0\0\x01\0\x08\0data\x04\0\0\0\x80\x80\x80\x80");
        let sound_path = directory.path().join("open.wav");
        std::fs::write(&sound_path, wav).unwrap();
        let mut document = RadialDocument::starter();
        document.skins[0].style.values.images.menu_background =
            Override::Value(super::super::model::MediaReference::ExternalFile {
                path: image_path.to_string_lossy().into_owned(),
            });
        document.skins[0]
            .style
            .values
            .images
            .menu_background_opacity = Override::Value(0.25);
        document.menus[0].rings[0].cells[0].icon =
            Override::Value(super::super::model::MediaReference::ExternalFile {
                path: image_path.to_string_lossy().into_owned(),
            });
        document.menus[0].rings[0].cells[0]
            .style
            .images
            .icon_opacity = Override::Value(0.25);
        document.skins[0].style.values.sounds.on_show =
            Override::Value(super::super::model::MediaReference::ExternalFile {
                path: sound_path.to_string_lossy().into_owned(),
            });
        document.menus[0].rings[0].cells[0].tooltip = Override::Value("Prepared tooltip".into());
        let sent = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&sent);
        let mut controller = RadialController::with_factory(
            Arc::new(document),
            true,
            Arc::new(move || {
                Ok(Box::new(Fake {
                    sent: Arc::clone(&observed),
                    events: Arc::new(Mutex::new(VecDeque::new())),
                }))
            }),
        );
        controller.configure_resources(directory.path().to_path_buf());
        controller.handle_intents(vec![open()], false);
        let commands = sent.lock().unwrap();
        let NativeCommand::Open { scene, .. } = &commands[0] else {
            panic!("expected open")
        };
        assert_eq!(
            scene
                .primitives
                .iter()
                .filter(|primitive| matches!(
                    primitive,
                    super::super::render::VectorPrimitive::Image { opacity: 64, .. }
                ))
                .count(),
            2,
            "menu background and cell icon opacity are independent prepared primitives"
        );
        assert!(scene.primitives.iter().any(|primitive| matches!(primitive, super::super::render::VectorPrimitive::Text { prepared, .. } if !prepared.glyphs.is_empty())));
        assert!(
            !controller
                .pending
                .as_ref()
                .unwrap()
                .resources
                .tooltips
                .is_empty()
        );
        assert!(controller.pending.as_ref().unwrap().sounds.open.is_some());
    }
    #[test]
    fn resource_invalidation_closes_active_generation_and_clears_preparation() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let made = Arc::new(Mutex::new(0));
        let mut controller = controller(Arc::clone(&events), made);
        controller.handle_intents(vec![open()], false);
        let session_id = controller.pending.as_ref().unwrap().session_id.clone();
        let generation = controller.layout_generation;

        controller.invalidate_resources();

        assert!(
            controller.pending.is_some(),
            "close remains correlated until native ack"
        );
        assert!(controller.active.is_none());
        assert_ne!(controller.layout_generation, generation);
        events.lock().unwrap().push_back(NativeEvent::Closed {
            session_id,
            reason: CloseReason::SettingsReload,
        });
        controller.poll();
        assert!(controller.pending.is_none());
    }
    #[test]
    fn menu_window_options_override_application_fallback_without_context_recapture() {
        let mut document = RadialDocument::starter();
        document.menus[0].style.values.window.always_on_top = Override::Value(false);
        document.menus[0].style.values.window.activate_on_show = Override::Value(true);
        let sent = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&sent);
        let mut controller = RadialController::with_factory(
            Arc::new(document),
            false,
            Arc::new(move || {
                Ok(Box::new(Fake {
                    sent: Arc::clone(&observed),
                    events: Arc::new(Mutex::new(VecDeque::new())),
                }))
            }),
        );
        controller.handle_intents(vec![open()], true);
        assert!(matches!(
            sent.lock().unwrap().as_slice(),
            [NativeCommand::Open {
                always_on_top: false,
                activate_on_show: true,
                ..
            }]
        ));
        assert_eq!(controller.pending.as_ref().unwrap().context.token, 0);
    }

    #[test]
    fn item_shortcut_uses_the_normal_close_ack_handoff_exactly_once() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let made = Arc::new(Mutex::new(0));
        let mut document = RadialDocument::starter();
        document.menus[0].rings[0].cells[0].content = CellContent::Action {
            binding: ActionBinding::Persisted {
                action: crate::universal_actions::PersistedUniversalActionRef {
                    target: None,
                    action_id: crate::universal_actions::ActionId::new("test"),
                },
            },
        };
        document.menus[0].rings[0].cells[0].after_action = AfterActionPolicy::CloseTree;
        let factory_events = Arc::clone(&events);
        let mut controller = RadialController::with_factory(
            Arc::new(document),
            true,
            Arc::new(move || {
                *made.lock().unwrap() += 1;
                Ok(Box::new(Fake {
                    sent: Arc::new(Mutex::new(vec![])),
                    events: Arc::clone(&factory_events),
                }))
            }),
        );
        let mut open = open();
        if let InvocationIntent::OpenRadial {
            trigger_still_down, ..
        } = &mut open
        {
            *trigger_still_down = false;
        }
        controller.handle_intents(vec![open], false);
        let session_id = controller.pending.as_ref().unwrap().session_id.clone();
        let generation = controller.pending.as_ref().unwrap().generation;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: session_id.clone(),
            layout_generation: generation,
        });
        controller.poll();
        let cell_id = controller.document.menus[0].rings[0].cells[0].id.clone();
        let activate = InvocationIntent::ActivateItem {
            id: InvocationId(88),
            menu_id: MenuId::new("starter"),
            cell_id,
            gesture: ClickGesture::Primary,
            scope: TriggerScope::MenuLocal,
            source: crate::commands::ActivationSource::RadialShortcut,
            trigger_still_down: true,
        };
        let activation_events = controller.handle_intents(vec![activate.clone(), activate], false);
        assert!(
            activation_events
                .iter()
                .all(|event| !matches!(event, ControllerEvent::DispatchRequested(_)))
        );
        events.lock().unwrap().push_back(NativeEvent::Closed {
            session_id,
            reason: CloseReason::ActionHandoff,
        });
        let closed = controller.poll();
        assert!(
            closed
                .iter()
                .all(|event| !matches!(event, ControllerEvent::DispatchRequested(_)))
        );
        let dispatched = controller.handle_intents(
            vec![InvocationIntent::TriggerReleased {
                id: InvocationId(88),
            }],
            false,
        );
        assert_eq!(
            dispatched
                .iter()
                .filter(|event| matches!(event, ControllerEvent::DispatchRequested(_)))
                .count(),
            1
        );
        assert!(dispatched.iter().any(|event| matches!(
            event,
            ControllerEvent::DispatchRequested(request)
                if request.source == crate::commands::ActivationSource::RadialShortcut
        )));
        assert!(
            controller
                .handle_intents(
                    vec![InvocationIntent::TriggerReleased {
                        id: InvocationId(88),
                    }],
                    false,
                )
                .iter()
                .all(|event| !matches!(event, ControllerEvent::DispatchRequested(_)))
        );
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
    fn imported_surface_controls_preserve_independent_primary_secondary_semantics() {
        let mut menu = RadialDocument::starter().menus.remove(0);
        menu.center_control = Some(Control::Close);
        menu.center_secondary_control = Some(Control::Drag);
        menu.background_control = Some(Control::Back);
        menu.background_secondary_control = Some(Control::Close);
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
        augment_special_cells(&mut layout, &BTreeMap::new(), &menu);
        let center = layout
            .cells
            .iter()
            .find(|cell| cell.cell_id.as_str() == "__center")
            .unwrap();
        let background = layout
            .cells
            .iter()
            .find(|cell| cell.cell_id.as_str() == "__background")
            .unwrap();
        assert_eq!(
            (center.control, center.secondary_control),
            (Some(Control::Close), Some(Control::Drag))
        );
        assert_eq!(
            (background.control, background.secondary_control),
            (Some(Control::Back), Some(Control::Close))
        );
        assert_eq!(
            special_surface_control(Some(&menu), &center.cell_id, PointerButton::Primary),
            Some(Control::Close)
        );
        assert_eq!(
            special_surface_control(Some(&menu), &center.cell_id, PointerButton::Secondary),
            Some(Control::Drag)
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
