use super::assets::{AssetService, PrepareVariant, PreparedMedia, reference_identity};
use super::audio::{PreparedRadialSounds, RadialAudioSession, RadialCue, SystemRadialAudioOutput};
use super::bindings::{
    PreparationGeneration, PreparedCell, RadialPrepareEnvelope, RadialPrepareReply,
    RadialPrepareRequest, project_menu_frame_with_style,
};
use super::compositor::CompositorCache;
use super::context::{InvocationContext, WindowIdentity};
use super::diagnostics::{
    MAX_EXPECTED_LAYOUT_DIAGNOSTICS, MAX_RADIAL_DIAGNOSTICS, RadialDiagnostic, bound_diagnostics,
};
use super::dynamic::{FrozenAvailability, FrozenBinding};
use super::font_cache::{FontLayoutService, MAX_LAYOUT_CACHE_ENTRIES, SystemFontCatalog};
use super::geometry::{
    CellLayout, FrozenSpatialContext, HitShape, LayoutSnapshot, LogicalPoint, PhysicalPoint,
    PhysicalRect, ScaleFactor, cascade_placement_with_direction, compose_layered_layout,
    layout_document_menu, layout_document_menu_fixed_center, layout_menu, shape_center,
    translate_layout,
};
use super::handoff::{
    DispatchEvent, DispatchIntent, InteractionRequirement, PendingRadialDispatch,
    RadialDispatchIdentity, RadialDispatchRequest,
};
use super::invocation::{InvocationIntent, LifecycleCancellation};
use super::model::{
    ActionBinding, AfterActionPolicy, CellContent, ClickGesture, Control, InteractionMode,
    InvocationId, MenuId, Override, RadialDocument, RingId, SessionId, SubmenuPresentation,
    TriggerScope,
};
use super::native::{CloseReason, NativeCommand, NativeEvent, NativeHost};
use super::preparation::{prepare_visual_resources, scene_resource_fingerprint};
use super::render::{
    InputOwner, PreparedSceneResources, SceneLayer, build_scene, build_scene_prepared,
    build_scene_prepared_selected, build_scene_prepared_selected_tooltip, compose_layered_scene,
};
use super::session::{
    CellRole, FrameId, NavigationCommand, NavigationModifiers, PointerButton, SessionEvent,
    SessionIntent, SessionReducer, visible_frame_ids,
};
use super::skin::compile_menu_tree;
use super::tooltip::{TooltipHoverState, TooltipIdentity, TooltipPreferences};
use std::collections::VecDeque;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;

const MAX_VISIBLE_RESOURCE_DIAGNOSTICS: usize = MAX_RADIAL_DIAGNOSTICS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum DeadlineKey {
    ActionHandoff,
    Dwell,
    Tooltip,
}

enum DeadlineCommand {
    Arm(DeadlineKey, u64),
    Cancel(DeadlineKey),
    Stop,
}

struct HandoffDeadlineScheduler {
    tx: mpsc::Sender<DeadlineCommand>,
    join: Option<JoinHandle<()>>,
    armed_deadlines: Mutex<BTreeMap<DeadlineKey, u64>>,
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
            armed_deadlines: Mutex::new(BTreeMap::new()),
        })
    }
    fn arm(&self, key: DeadlineKey, deadline: u64) {
        if let Ok(mut armed_deadlines) = self.armed_deadlines.lock() {
            if armed_deadlines.get(&key) == Some(&deadline) {
                return;
            }
            armed_deadlines.insert(key, deadline);
        }
        let _ = self.tx.send(DeadlineCommand::Arm(key, deadline));
    }
    fn cancel(&self, key: DeadlineKey) {
        if let Ok(mut armed_deadlines) = self.armed_deadlines.lock()
            && armed_deadlines.remove(&key).is_none()
        {
            return;
        }
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
    /// The controller accepted an externally initiated open. The main thread
    /// correlates the invocation id with its command metadata and admits that
    /// lifecycle to the shared input service before forwarding preparation.
    ExternalSessionAdmitted {
        invocation_id: InvocationId,
    },
    InvocationCompleted {
        invocation_id: InvocationId,
    },
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
    LayoutFailed {
        menu_id: MenuId,
        error: super::geometry::LayoutError,
    },
    SubmenuPlacementFailed {
        session_id: SessionId,
        parent_frame_id: FrameId,
        parent_menu_id: MenuId,
        child_menu_id: MenuId,
        parent_presentation: SubmenuPresentation,
        message: String,
    },
    DispatchRequested(RadialDispatchRequest),
    InvocationReleaseAcknowledged {
        invocation_id: InvocationId,
    },
    PrepareRequested(RadialPrepareEnvelope),
    Diagnostic(RadialDiagnostic),
    Error(String),
}

/// The owner of keyboard input while a radial session is present.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GridKeyboardOwner {
    RadialMenu,
    LegacyLauncher,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticRecord {
    pub session_id: Option<SessionId>,
    pub message: String,
}

/// On-demand resource census for the existing debug-logging path. Keeping the
/// snapshot controller-owned avoids global counters and performs no work until
/// a caller explicitly asks for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadialResourceSnapshot {
    pub layout_generation: u64,
    pub native_hosts: usize,
    pub pending_sessions: usize,
    pub active_sessions: usize,
    pub preparation_bridges: usize,
    pub deadline_schedulers: usize,
    pub asset_services: usize,
    pub font_services: usize,
    pub font_catalogs: usize,
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
    spatial: FrozenSpatialContext,
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
    closing: bool,
    menu_id: MenuId,
    current_frame_id: FrameId,
    layout: LayoutSnapshot,
    spatial: FrozenSpatialContext,
    reducer: SessionReducer,
    tooltip_hover: TooltipHoverState,
    pointer: LogicalPoint,
    application_always_on_top: bool,
    always_on_top: bool,
    activate_on_show: bool,
    context: InvocationContext,
    trigger_still_down: bool,
    owned_item_input: Option<InvocationId>,
    prepared: Option<RadialPrepareReply>,
    navigation_layouts: BTreeMap<FrameId, LayoutSnapshot>,
    navigation_frames: BTreeMap<FrameId, super::bindings::PreparedMenuFrame>,
    navigation_presentations: BTreeMap<FrameId, SubmenuPresentation>,
    navigation_resources: BTreeMap<FrameId, PreparedSceneResources>,
    navigation_window_options: BTreeMap<FrameId, (bool, bool)>,
    resources: PreparedSceneResources,
    audio: Option<RadialAudioSession<SystemRadialAudioOutput>>,
    audio_generation: u64,
    navigation_sounds: BTreeMap<FrameId, PreparedRadialSounds>,
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
    grid_keyboard_owner: GridKeyboardOwner,
    asset_service: Option<AssetService>,
    font_service: Option<FontLayoutService>,
    font_catalog: Option<SystemFontCatalog>,
    visible_resource_diagnostics: VecDeque<u64>,
    tooltip_preferences: TooltipPreferences,
    terminal_events: VecDeque<ControllerEvent>,
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
            visible_resource_diagnostics: VecDeque::new(),
            tooltip_preferences: TooltipPreferences::default(),
            terminal_events: VecDeque::new(),
            release_waits: BTreeMap::new(),
            grid_keyboard_owner: GridKeyboardOwner::RadialMenu,
        }
    }
    pub fn replace_document(&mut self, document: Arc<RadialDocument>) {
        self.invalidate_resources();
        if let Some(service) = &mut self.asset_service {
            service.replace_search_roots(document.media_search_roots.clone());
        }
        self.document = document;
    }

    pub fn set_tooltip_preferences(
        &mut self,
        preferences: TooltipPreferences,
    ) -> Vec<ControllerEvent> {
        if self.tooltip_preferences == preferences {
            return Vec::new();
        }
        self.tooltip_preferences = preferences;
        let Some((session_id, menu_id, layout, work_area)) = self.active.as_mut().map(|active| {
            active.tooltip_hover.cancel();
            (
                active.session_id.clone(),
                active.menu_id.clone(),
                active.layout.clone(),
                active.spatial.work_area,
            )
        }) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let menu = self
            .document
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .cloned();
        let Some(menu) = menu else {
            return out;
        };
        let generation = self.layout_generation_for(&session_id);
        let (resources, sounds, diagnostics) =
            self.prepare_scene_resources(&menu, &layout, generation, work_area);
        if let Some(active) = self
            .active
            .as_mut()
            .filter(|active| active.session_id == session_id)
        {
            active.resources = resources;
            active
                .navigation_resources
                .insert(active.current_frame_id, active.resources.clone());
            active
                .navigation_sounds
                .insert(active.current_frame_id, sounds);
        }
        out.extend(diagnostics.into_iter().map(ControllerEvent::Diagnostic));
        self.sync_tooltip_deadline();
        self.refresh_active_scene(&session_id, &mut out);
        out
    }

    /// Installs the preparation-only resource services used by the production
    /// controller. Tests with synthetic hosts can intentionally omit this.
    pub fn configure_resources(&mut self, application_data: PathBuf) {
        if self.asset_service.is_some() {
            return;
        }
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

    /// Retires demand-driven caches and scheduler ownership. The caller first
    /// closes active runtime/preview sessions, so no frame can retain these
    /// services after the final runtime/editor lease is released.
    pub fn release_resources(&mut self) {
        self.cancel_handoff_deadline();
        if let Some(scheduler) = &self.deadline_scheduler {
            scheduler.cancel(DeadlineKey::Tooltip);
        }
        self.deadline_scheduler = None;
        self.asset_service = None;
        self.font_service = None;
        self.font_catalog = None;
        self.visible_resource_diagnostics.clear();
    }

    pub fn resources_configured(&self) -> bool {
        self.asset_service.is_some()
    }

    pub fn resource_snapshot(&self) -> Option<RadialResourceSnapshot> {
        self.diagnostics.as_ref().map(|_| RadialResourceSnapshot {
            layout_generation: self.layout_generation,
            native_hosts: usize::from(self.host.is_some()),
            pending_sessions: usize::from(self.pending.is_some()),
            active_sessions: usize::from(self.active.is_some()),
            preparation_bridges: usize::from(self.preparation.is_some()),
            deadline_schedulers: usize::from(self.deadline_scheduler.is_some()),
            asset_services: usize::from(self.asset_service.is_some()),
            font_services: usize::from(self.font_service.is_some()),
            font_catalogs: usize::from(self.font_catalog.is_some()),
        })
    }

    pub fn prepare_authoring_asset(
        &mut self,
        reference: &super::model::MediaReference,
        expected: super::model::MediaKind,
        records: &[super::model::AssetRecord],
        variant: PrepareVariant,
    ) -> Result<super::assets::PreparedAssetSnapshot, super::assets::AssetDiagnostic> {
        self.asset_service
            .as_mut()
            .ok_or(super::assets::AssetDiagnostic::SearchRootsUnavailable)?
            .prepare(reference, expected, records, variant)
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
        self.stop_for_lifecycle(CloseReason::FeatureDisabled);
    }
    pub fn shutdown(&mut self) {
        self.stop_for_lifecycle(CloseReason::Shutdown);
    }
    fn stop_for_lifecycle(&mut self, reason: CloseReason) {
        self.handoff = None;
        self.cancel_handoff_deadline();
        if let Some(bridge) = &mut self.preparation {
            bridge.waiting = None;
        }
        self.close(reason, None);
        if let Some(mut host) = self.host.take() {
            host.shutdown();
        }
        self.pending = None;
        self.active = None;
        self.queued_item_activation = None;
        self.release_aliases.clear();
        self.release_waits.clear();
        self.deadline_scheduler = None;
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
                InvocationIntent::OpenExternalRadial {
                    id,
                    menu_id,
                    context_token,
                    interaction,
                    trigger_still_down,
                } => {
                    out.push(ControllerEvent::ExternalSessionAdmitted { invocation_id: id });
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
                        out.push(ControllerEvent::InvocationCompleted { invocation_id: id });
                        continue;
                    }
                    let interaction = self
                        .document
                        .menus
                        .iter()
                        .find(|m| m.id == menu_id)
                        .map_or(InteractionMode::StickyClick, |m| m.interaction);
                    let context = self.capture_context(id.0);
                    out.push(ControllerEvent::ExternalSessionAdmitted { invocation_id: id });
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
                InvocationIntent::CancelRadialLifecycle { id, reason } => {
                    let waiting_matches = self
                        .preparation
                        .as_ref()
                        .and_then(|bridge| bridge.waiting.as_ref())
                        .is_some_and(|waiting| waiting.invocation_id == id);
                    let matches = waiting_matches
                        || self
                            .pending
                            .as_ref()
                            .is_some_and(|pending| pending.invocation_id == id)
                        || self.active.as_ref().is_some_and(|active| {
                            active.invocation_id == id || active.owned_item_input == Some(id)
                        });
                    if matches {
                        if waiting_matches && let Some(bridge) = &mut self.preparation {
                            bridge.waiting = None;
                        }
                        if self
                            .queued_item_activation
                            .as_ref()
                            .is_some_and(|queued| queued.id == id)
                        {
                            self.queued_item_activation = None;
                        }
                        self.close(close_reason_for_lifecycle(reason), None);
                    }
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
                        if scope == TriggerScope::Global {
                            out.push(ControllerEvent::InvocationCompleted { invocation_id: id });
                        }
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
                        out.push(ControllerEvent::ExternalSessionAdmitted { invocation_id: id });
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
                InvocationIntent::HoldCancelledBeforePresentation { id } => {
                    if self
                        .preparation
                        .as_ref()
                        .and_then(|bridge| bridge.waiting.as_ref())
                        .is_some_and(|waiting| waiting.invocation_id == id)
                    {
                        if let Some(bridge) = &mut self.preparation {
                            bridge.waiting = None;
                        }
                    }
                    if self
                        .pending
                        .as_ref()
                        .is_some_and(|pending| pending.invocation_id == id)
                    {
                        self.close(CloseReason::Dismissed, None);
                    }
                    if self
                        .queued_item_activation
                        .as_ref()
                        .is_some_and(|queued| queued.id == id)
                    {
                        self.queued_item_activation = None;
                    }
                }
                InvocationIntent::ScheduleDeadline { .. }
                | InvocationIntent::CancelDeadline { .. } => {}
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
        let spatial = FrozenSpatialContext {
            requested_root_anchor: anchor,
            visible_center: layout.origin,
            work_area: work,
            scale_factor: scale,
            spatial_generation: 0,
            topology_generation: 1,
        };
        if let Some(reply) = prepared.as_ref() {
            augment_special_cells(&mut layout, &reply.frame.cells, &reply.frame.menu, false);
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
            self.prepare_scene_resources(&menu, &layout, generation, work);
        out.extend(
            resource_diagnostics
                .into_iter()
                .map(ControllerEvent::Diagnostic),
        );
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
                    spatial,
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
        work_area: PhysicalRect,
    ) -> (
        PreparedSceneResources,
        PreparedRadialSounds,
        Vec<RadialDiagnostic>,
    ) {
        let (resources, mut diagnostics) = prepare_visual_resources(
            &self.document,
            menu,
            layout,
            work_area,
            self.tooltip_preferences,
            self.asset_service.as_mut(),
            self.font_service.as_mut(),
            &super::assets::ManagedAssetOverlay::default(),
        );
        let variant = PrepareVariant {
            effective_style: scene_resource_fingerprint(layout),
            dpi_milli: (layout.scale_factor.get() * 1_000.0)
                .round()
                .clamp(1.0, u32::MAX as f64) as u32,
            logical_width_milli: (layout.style.item_size.max(1.0) * 1_000.0) as u32,
            logical_height_milli: (layout.style.item_size.max(1.0) * 1_000.0) as u32,
            quality: layout.style.image_quality,
        };
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
                        Err(error) => {
                            let identity = reference_identity(reference);
                            diagnostics.push(RadialDiagnostic::new(
                                super::diagnostics::RadialDiagnosticSeverity::Error,
                                super::diagnostics::RadialDiagnosticKind::SoundUnavailable(
                                    error.clone(),
                                ),
                                super::diagnostics::RadialDiagnosticSource::Asset {
                                    menu_id: menu.id.clone(),
                                    identity: identity.clone(),
                                },
                                (variant.effective_style, variant.dpi_milli, error.clone()),
                                format!("radial sound {identity} unavailable: {error}"),
                            ));
                        }
                    }
                }
            }
        }
        diagnostics = bound_diagnostics(
            diagnostics,
            MAX_EXPECTED_LAYOUT_DIAGNOSTICS,
            MAX_RADIAL_DIAGNOSTICS,
        );
        diagnostics.retain(|diagnostic| {
            if self
                .visible_resource_diagnostics
                .contains(&diagnostic.fingerprint)
            {
                return false;
            }
            self.visible_resource_diagnostics
                .push_back(diagnostic.fingerprint);
            if self.visible_resource_diagnostics.len() > MAX_VISIBLE_RESOURCE_DIAGNOSTICS {
                self.visible_resource_diagnostics.pop_front();
            }
            true
        });
        for diagnostic in &diagnostics {
            self.record(None, diagnostic.message.clone());
        }
        (resources, sounds, diagnostics)
    }
    pub fn poll(&mut self) -> Vec<ControllerEvent> {
        let mut out = self.terminal_events.drain(..).collect::<Vec<_>>();
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
        self.poll_tooltip(&mut out);
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
    fn poll_tooltip(&mut self, out: &mut Vec<ControllerEvent>) {
        let now_ms = monotonic_ms();
        let ready = self.active.as_ref().and_then(|active| {
            let (identity, deadline) = active.tooltip_hover.candidate()?;
            (deadline <= now_ms).then(|| (active.session_id.clone(), identity.clone()))
        });
        if let Some((session_id, identity)) = ready
            && self
                .active
                .as_mut()
                .filter(|active| active.session_id == session_id)
                .is_some_and(|active| active.tooltip_hover.expire(&identity, now_ms))
        {
            self.sync_tooltip_deadline();
            self.refresh_active_scene(&session_id, out);
        }
    }
    fn handle_native(&mut self, event: NativeEvent, out: &mut Vec<ControllerEvent>) {
        if self.active.as_ref().is_some_and(|active| {
            active.closing
                && event.session_id() == Some(&active.session_id)
                && !matches!(
                    &event,
                    NativeEvent::Closed { .. } | NativeEvent::Failed { .. }
                )
        }) {
            return;
        }
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
                let keyboard_event = match self.grid_keyboard_owner {
                    GridKeyboardOwner::RadialMenu => SessionEvent::ResumeKeyboard,
                    GridKeyboardOwner::LegacyLauncher => SessionEvent::SuspendKeyboard,
                };
                reducer.reduce(keyboard_event);
                if let Some(root) = reducer.state.stack.last_mut() {
                    root.scale_factor = p.layout.scale_factor.get();
                    root.spatial_generation = p.spatial.spatial_generation;
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
                navigation_layouts.insert(FrameId(1), p.layout.clone());
                let navigation_frames = p.prepared.as_ref().map_or_else(BTreeMap::new, |reply| {
                    BTreeMap::from([(FrameId(1), reply.frame.clone())])
                });
                let navigation_resources = BTreeMap::from([(FrameId(1), p.resources.clone())]);
                let navigation_window_options =
                    BTreeMap::from([(FrameId(1), (p.always_on_top, p.activate_on_show))]);
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
                    closing: false,
                    menu_id: p.menu_id.clone(),
                    current_frame_id: FrameId(1),
                    layout: p.layout,
                    spatial: p.spatial,
                    reducer,
                    tooltip_hover: TooltipHoverState::default(),
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
                    navigation_presentations: BTreeMap::new(),
                    navigation_resources,
                    navigation_window_options,
                    resources: p.resources,
                    audio: Some(audio),
                    audio_generation: layout_generation,
                    navigation_sounds: BTreeMap::from([(FrameId(1), root_sounds)]),
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
                layout_generation,
                owner,
                point,
            } => {
                if self.layout_generation_for(&session_id) != layout_generation {
                    return;
                }
                if owner != super::render::InputOwner::Exterior {
                    self.session_event(&session_id, SessionEvent::MenuInteraction, out);
                }
                let tooltip_region_owned = !matches!(&owner, super::render::InputOwner::Exterior);
                let previous_selection = self
                    .active
                    .as_ref()
                    .filter(|active| active.session_id == session_id)
                    .and_then(|active| active.reducer.state.hovered.clone());
                let previous_visible_tooltip = self
                    .active
                    .as_ref()
                    .filter(|active| active.session_id == session_id)
                    .and_then(|active| active.tooltip_hover.visible().cloned());
                if let Some(active) = self
                    .active
                    .as_mut()
                    .filter(|active| active.session_id == session_id)
                {
                    active.pointer = point;
                }
                let hovered = owner_cell(owner);
                self.session_event(
                    &session_id,
                    SessionEvent::PointerMoved {
                        point,
                        hovered: hovered.clone(),
                        geometry_generation: layout_generation,
                    },
                    out,
                );
                let tooltip_cell = self
                    .active
                    .as_ref()
                    .filter(|active| active.session_id == session_id)
                    .and_then(|active| {
                        tooltip_region_owned
                            .then(|| active.layout.geometric_hover_cell(point))
                            .flatten()
                            .map(|cell| cell.cell_id.clone())
                    });
                let current_selection = self
                    .active
                    .as_ref()
                    .filter(|active| active.session_id == session_id)
                    .and_then(|active| active.reducer.state.hovered.clone());
                let tooltip_identity = self
                    .active
                    .as_ref()
                    .filter(|active| active.session_id == session_id)
                    .and_then(|active| {
                        tooltip_cell
                            .as_ref()
                            .filter(|cell| active.resources.tooltips.contains_key(*cell))
                            .map(|cell| TooltipIdentity {
                                session_id: session_id.clone(),
                                frame_id: active.current_frame_id,
                                layout_generation,
                                cell_id: cell.clone(),
                            })
                    });
                if let Some(active) = self
                    .active
                    .as_mut()
                    .filter(|active| active.session_id == session_id)
                {
                    active.tooltip_hover.observe(
                        tooltip_identity,
                        true,
                        monotonic_ms(),
                        self.tooltip_preferences.delay_ms,
                    );
                }
                self.sync_tooltip_deadline();
                let current_visible_tooltip = self
                    .active
                    .as_ref()
                    .filter(|active| active.session_id == session_id)
                    .and_then(|active| active.tooltip_hover.visible().cloned());
                let tooltip_scene_changed = current_visible_tooltip != previous_visible_tooltip;
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
                }
                if current_selection != previous_selection || tooltip_scene_changed {
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
            NativeEvent::PointerLeft {
                session_id,
                layout_generation,
            } => {
                if self.layout_generation_for(&session_id) != layout_generation {
                    return;
                }
                let had_hover = self
                    .active
                    .as_ref()
                    .filter(|active| active.session_id == session_id)
                    .is_some_and(|active| active.reducer.state.hovered.is_some());
                let had_visible_tooltip = self
                    .active
                    .as_mut()
                    .filter(|active| active.session_id == session_id)
                    .is_some_and(|active| active.tooltip_hover.cancel());
                self.session_event(&session_id, SessionEvent::OutsideInteraction, out);
                self.sync_tooltip_deadline();
                if had_hover || had_visible_tooltip {
                    self.refresh_active_scene(&session_id, out);
                }
            }
            NativeEvent::PointerDown {
                session_id,
                layout_generation,
                owner,
                point,
                button,
            } => {
                if self.layout_generation_for(&session_id) != layout_generation {
                    return;
                }
                if owner != super::render::InputOwner::Exterior {
                    self.session_event(&session_id, SessionEvent::MenuInteraction, out);
                }
                let had_tooltip = self
                    .active
                    .as_mut()
                    .filter(|active| active.session_id == session_id)
                    .is_some_and(|active| active.tooltip_hover.cancel());
                if let InputOwner::NavigateToFrame(frame_id) = owner {
                    self.session_event(
                        &session_id,
                        SessionEvent::BeginAncestorNavigationStamped {
                            frame_id,
                            point,
                            button,
                            geometry_generation: layout_generation,
                            event_time_ms: monotonic_ms(),
                        },
                        out,
                    );
                    self.sync_tooltip_deadline();
                    if had_tooltip {
                        self.refresh_active_scene(&session_id, out);
                    }
                    return;
                }
                let role = self.role_for_button(&session_id, &owner, button);
                self.session_event(
                    &session_id,
                    SessionEvent::PointerDownStamped {
                        point,
                        cell: owner_cell(owner),
                        role,
                        button,
                        geometry_generation: layout_generation,
                        event_time_ms: monotonic_ms(),
                    },
                    out,
                );
                self.sync_tooltip_deadline();
                if had_tooltip {
                    self.refresh_active_scene(&session_id, out);
                }
            }
            NativeEvent::PointerUp {
                session_id,
                layout_generation,
                owner,
                point,
                button,
            } => {
                if self.layout_generation_for(&session_id) != layout_generation {
                    return;
                }
                if matches!(owner, InputOwner::NavigateToFrame(_)) {
                    // The corresponding pointer-down already popped the exact
                    // frame and armed SessionReducer's release-consumption
                    // latch.  Do not let this release tail enter the restored
                    // child's actionable geometry.
                    self.session_event(
                        &session_id,
                        SessionEvent::PointerUp {
                            point,
                            cell: None,
                            role: CellRole::Spacer,
                            button,
                            geometry_generation: layout_generation,
                        },
                        out,
                    );
                    return;
                }
                let role = self.role_for_button(&session_id, &owner, button);
                self.session_event(
                    &session_id,
                    SessionEvent::PointerUp {
                        point,
                        cell: owner_cell(owner),
                        role,
                        button,
                        geometry_generation: layout_generation,
                    },
                    out,
                )
            }
            NativeEvent::CaptureLost {
                session_id,
                layout_generation,
            } => {
                if self.layout_generation_for(&session_id) != layout_generation {
                    return;
                }
                let had_tooltip = self
                    .active
                    .as_mut()
                    .filter(|active| active.session_id == session_id)
                    .is_some_and(|active| active.tooltip_hover.cancel());
                self.session_event(&session_id, SessionEvent::OutsideInteraction, out);
                self.sync_tooltip_deadline();
                if had_tooltip {
                    self.refresh_active_scene(&session_id, out);
                }
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
            NativeEvent::Relocated {
                session_id,
                layout_generation,
                from,
                to,
            } => {
                if let Some(active) = self
                    .active
                    .as_mut()
                    .filter(|active| active.session_id == session_id)
                {
                    active.tooltip_hover.cancel();
                }
                self.sync_tooltip_deadline();
                let Some(active) = self
                    .active
                    .as_ref()
                    .filter(|active| active.session_id == session_id)
                else {
                    return;
                };
                let Some(current_frame) = active.reducer.state.stack.last() else {
                    return;
                };
                let Some(next_frame_generation) = current_frame.geometry_generation.checked_add(1)
                else {
                    return;
                };
                let Some(current_layout) = active
                    .navigation_layouts
                    .get(&current_frame.frame_id)
                    .cloned()
                else {
                    return;
                };
                if current_frame.geometry_generation != layout_generation
                    || active.reducer.state.spatial_generation != active.spatial.spatial_generation
                    || active.current_frame_id != current_frame.frame_id
                    || current_layout.origin != current_frame.origin
                {
                    return;
                }
                let delta = PhysicalPoint {
                    x: to.x - from.x,
                    y: to.y - from.y,
                };
                if !delta.x.is_finite() || !delta.y.is_finite() {
                    return;
                }
                let Some(spatial_generation) = active.spatial.spatial_generation.checked_add(1)
                else {
                    return;
                };
                let generation = self.layout_generation.max(next_frame_generation);
                let Some(next_global_generation) = generation.checked_add(1) else {
                    return;
                };
                let delta_scale = active.spatial.scale_factor.get();
                let logical_delta = LogicalPoint {
                    x: (delta.x / delta_scale) as f32,
                    y: (delta.y / delta_scale) as f32,
                };
                let pointer_baseline = LogicalPoint {
                    x: active.pointer.x + logical_delta.x,
                    y: active.pointer.y + logical_delta.y,
                };
                let mut spatial = active.spatial;
                spatial.visible_center.x += delta.x;
                spatial.visible_center.y += delta.y;
                spatial.spatial_generation = spatial_generation;
                if !logical_delta.x.is_finite()
                    || !logical_delta.y.is_finite()
                    || !pointer_baseline.x.is_finite()
                    || !pointer_baseline.y.is_finite()
                    || !spatial.visible_center.x.is_finite()
                    || !spatial.visible_center.y.is_finite()
                {
                    return;
                }

                let live_frames: BTreeSet<_> = active
                    .reducer
                    .state
                    .stack
                    .iter()
                    .map(|frame| frame.frame_id)
                    .collect();
                let mut translated_layouts = active.navigation_layouts.clone();
                translated_layouts.retain(|frame_id, _| live_frames.contains(frame_id));
                for layout in translated_layouts.values_mut() {
                    if let Err(error) = translate_layout(layout, delta) {
                        out.push(ControllerEvent::Error(format!(
                            "radial relocation failed: {error:?}"
                        )));
                        return;
                    }
                }
                let mut reducer = active.reducer.clone();
                reducer.reduce(SessionEvent::Relocated {
                    expected_spatial_generation: active.spatial.spatial_generation,
                    spatial_generation,
                    geometry_generation: generation,
                    physical_delta: delta,
                    pointer_baseline,
                });
                if reducer.state.spatial_generation != spatial_generation
                    || reducer.state.stack.iter().any(|frame| {
                        frame.geometry_generation != generation
                            || frame.spatial_generation != spatial_generation
                            || translated_layouts
                                .get(&frame.frame_id)
                                .is_none_or(|layout| {
                                    layout.origin.x != frame.origin.x
                                        || layout.origin.y != frame.origin.y
                                })
                    })
                {
                    return;
                }
                let Some(translated_current) =
                    translated_layouts.get(&current_frame.frame_id).cloned()
                else {
                    return;
                };
                let Some(active) = self
                    .active
                    .as_mut()
                    .filter(|active| active.session_id == session_id)
                else {
                    return;
                };
                active.navigation_layouts = translated_layouts;
                active.layout = translated_current;
                active.pointer = pointer_baseline;
                active.spatial = spatial;
                active.reducer = reducer;
                prune_runtime_navigation(active);
                let selected = active.reducer.state.hovered.as_ref().or(active
                    .reducer
                    .state
                    .selected
                    .as_ref());
                let visible_tooltip = active
                    .tooltip_hover
                    .visible()
                    .filter(|visible| {
                        visible.session_id == session_id
                            && visible.frame_id == active.current_frame_id
                            && visible.layout_generation == generation
                    })
                    .map(|visible| visible.cell_id.clone());
                let (scene, layout) =
                    runtime_present_scene(active, generation, selected, visible_tooltip);
                active.layout = layout.clone();
                let command = NativeCommand::Present {
                    session_id: session_id.clone(),
                    scene,
                    layout,
                    always_on_top: active.always_on_top,
                    activate_on_show: active.activate_on_show,
                };
                self.layout_generation = next_global_generation;
                if self
                    .host
                    .as_ref()
                    .is_none_or(|host| host.send(command).is_err())
                {
                    out.push(ControllerEvent::Error(
                        "failed to synchronize radial drag relocation".into(),
                    ));
                    self.retire_host();
                    return;
                }
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
        if matches!(
            &event,
            SessionEvent::PointerDown { .. }
                | SessionEvent::PointerDownStamped { .. }
                | SessionEvent::PointerUp { .. }
                | SessionEvent::BeginAncestorNavigation { .. }
                | SessionEvent::BeginAncestorNavigationStamped { .. }
                | SessionEvent::TriggerReleased { .. }
                | SessionEvent::OpenChild { .. }
                | SessionEvent::Back { .. }
                | SessionEvent::NavigateToFrame { .. }
                | SessionEvent::NavigateToFrameStamped { .. }
                | SessionEvent::PageChanged { .. }
                | SessionEvent::DisplayRelayout { .. }
                | SessionEvent::Relocated { .. }
                | SessionEvent::OutsideInteraction
                | SessionEvent::SelectKeyboard { .. }
                | SessionEvent::Navigate { .. }
                | SessionEvent::DwellExpired { .. }
                | SessionEvent::ActivateItem { .. }
                | SessionEvent::Close
        ) && let Some(active) = self
            .active
            .as_mut()
            .filter(|active| &active.session_id == id)
        {
            active.tooltip_hover.cancel();
        }
        let page_change = self
            .active
            .as_ref()
            .filter(|active| &active.session_id == id)
            .and_then(|active| {
                active
                    .reducer
                    .clone()
                    .reduce(event.clone())
                    .into_iter()
                    .find_map(|intent| match intent {
                        SessionIntent::PageChanged { page } => Some(page),
                        _ => None,
                    })
            });
        if let Some(page) = page_change
            && let Err((menu_id, error)) = self.preflight_page_change(id, page)
        {
            out.push(ControllerEvent::LayoutFailed { menu_id, error });
            return;
        }
        let Some(active) = self.active.as_mut().filter(|a| &a.session_id == id) else {
            return;
        };
        let intents = active.reducer.reduce(event);
        self.sync_dwell_deadline();
        self.sync_tooltip_deadline();
        for intent in intents {
            self.handle_session_intent(id, intent, out);
        }
    }

    fn preflight_page_change(
        &self,
        id: &SessionId,
        page: usize,
    ) -> Result<(), (MenuId, super::geometry::LayoutError)> {
        let Some(active) = self
            .active
            .as_ref()
            .filter(|active| &active.session_id == id)
        else {
            return Ok(());
        };
        let Some(frame) = active.reducer.state.stack.last().cloned() else {
            return Ok(());
        };
        let previous = active
            .navigation_frames
            .get(&frame.frame_id)
            .filter(|prepared| prepared.page != page)
            .cloned()
            .or_else(|| {
                active
                    .prepared
                    .as_ref()
                    .filter(|prepared| {
                        prepared.menu_id == frame.menu_id && prepared.frame.page != page
                    })
                    .map(|prepared| prepared.frame.clone())
            });
        let Some(previous) = previous else {
            return Ok(());
        };
        let base = previous.base_menu.clone();
        let style = compile_menu_tree(&self.document, &base).ok();
        let mut projected = project_menu_frame_with_style(
            &base,
            previous.static_cells.clone(),
            &previous.dynamic,
            page,
            style.as_ref(),
        );
        projected.alternates = previous.alternates;
        layout_document_menu_fixed_center(
            &self.document,
            &projected.menu,
            frame.origin,
            active.spatial.work_area,
            active.spatial.scale_factor,
            0.55,
        )
        .map(|_| ())
        .map_err(|error| (frame.menu_id, error))
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
            SessionIntent::BeginNativeDrag {
                geometry_generation,
            } => {
                if self.layout_generation_for(id) == geometry_generation
                    && let Some(host) = self.host.as_ref()
                    && let Err(message) = host.send(NativeCommand::BeginSystemDrag {
                        session_id: id.clone(),
                        layout_generation: geometry_generation,
                    })
                {
                    out.push(ControllerEvent::Error(message));
                }
            }
            SessionIntent::Back => {
                if let Some(active) = self
                    .active
                    .as_mut()
                    .filter(|active| &active.session_id == id)
                {
                    prune_runtime_navigation(active);
                }
                self.refresh_active_scene(id, out)
            }
            SessionIntent::PageChanged { .. } => {
                let pointer = self
                    .active
                    .as_ref()
                    .filter(|active| &active.session_id == id)
                    .map(|active| active.pointer);
                if let Some(pointer) = pointer {
                    let generation = self.next_layout_generation();
                    self.session_event(
                        id,
                        SessionEvent::DisplayRelayout {
                            geometry_generation: generation,
                            pointer_baseline: pointer,
                        },
                        out,
                    );
                }
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
        if child_center_is_back(active, cell) {
            return CellRole::Back;
        }
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
        if child_center_is_back(active, cell) {
            return CellRole::Back;
        }
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
        if child_center_is_back(active, cell) {
            return CellRole::Back;
        }
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
        let parent_menu_id = active.menu_id.clone();
        let parent_frame_id = active.current_frame_id;
        let session_id = active.session_id.clone();
        // `active.layout` may be the composite host snapshot.  Placement and
        // anchor semantics must use the current frame's own extents so an
        // ancestor union cannot spread later menus across the work area.
        let parent_layout = active
            .navigation_layouts
            .get(&active.current_frame_id)
            .cloned()
            .unwrap_or_else(|| active.layout.clone());
        let parent_direction = active
            .reducer
            .state
            .stack
            .iter()
            .find(|frame| frame.frame_id == active.current_frame_id)
            .and_then(|frame| frame.cascade_direction);
        let spatial = active.spatial;
        let pointer = active.pointer;
        let application_always_on_top = active.application_always_on_top;
        let Some(parent_definition) = self
            .document
            .menus
            .iter()
            .find(|menu| menu.id == parent_menu_id)
            .cloned()
        else {
            return;
        };
        let Some(menu_id) = parent_definition
            .rings
            .iter()
            .flat_map(|ring| &ring.cells)
            .find(|cell| &cell.id == cell_id)
            .and_then(|cell| match &cell.content {
                CellContent::Submenu { menu_id } => Some(menu_id.clone()),
                _ => None,
            })
        else {
            return;
        };
        let prepared_child = self
            .active
            .as_ref()
            .filter(|active| &active.session_id == id)
            .and_then(|active| active.prepared.as_ref())
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
        let parent_presentation = parent_definition.submenu_presentation;
        let (effective_presentation, mut layout, cascade_direction) = match parent_presentation {
            SubmenuPresentation::SameCenter => match layout_document_menu_fixed_center(
                &self.document,
                &child,
                parent_layout.origin,
                spatial.work_area,
                spatial.scale_factor,
                0.55,
            ) {
                Ok(layout) => (SubmenuPresentation::SameCenter, layout, None),
                Err(error) => {
                    out.push(ControllerEvent::SubmenuPlacementFailed {
                        session_id,
                        parent_frame_id,
                        parent_menu_id,
                        child_menu_id: menu_id,
                        parent_presentation,
                        message: format!("SameCenter placement failed: {error:?}"),
                    });
                    return;
                }
            },
            SubmenuPresentation::Cascade => {
                let selection = match cascade_placement_with_direction(
                    &self.document,
                    &parent_layout,
                    &child,
                    spatial.work_area,
                    spatial.scale_factor,
                    0.55,
                    parent_direction,
                ) {
                    Ok(selection) => selection,
                    Err(error) => {
                        out.push(ControllerEvent::SubmenuPlacementFailed {
                            session_id,
                            parent_frame_id,
                            parent_menu_id,
                            child_menu_id: menu_id,
                            parent_presentation,
                            message: format!("Cascade placement failed: {error:?}"),
                        });
                        return;
                    }
                };
                match layout_document_menu_fixed_center(
                    &self.document,
                    &child,
                    selection.anchor,
                    spatial.work_area,
                    spatial.scale_factor,
                    0.55,
                ) {
                    Ok(layout) => (selection.presentation, layout, selection.direction),
                    Err(error) => {
                        out.push(ControllerEvent::SubmenuPlacementFailed {
                            session_id,
                            parent_frame_id,
                            parent_menu_id,
                            child_menu_id: menu_id,
                            parent_presentation,
                            message: format!("Cascade placement failed: {error:?}"),
                        });
                        return;
                    }
                }
            }
        };
        let empty_prepared = BTreeMap::new();
        let prepared_cells = prepared_child
            .as_ref()
            .map_or(&empty_prepared, |frame| &frame.cells);
        augment_special_cells(&mut layout, prepared_cells, &child, true);
        if let Some(frame) = prepared_child.as_ref() {
            apply_prepared_availability(&mut layout, frame);
        }
        if let Some(center) = layout
            .cells
            .iter_mut()
            .find(|cell| cell.cell_id.as_str() == "__center")
        {
            center.actionable = true;
        }
        let (always_on_top, activate_on_show) =
            resolve_window_options(&self.document, &child, application_always_on_top);
        let generation = self.next_layout_generation();
        let (resources, sounds, diagnostics) =
            self.prepare_scene_resources(&child, &layout, generation, spatial.work_area);
        out.extend(diagnostics.into_iter().map(ControllerEvent::Diagnostic));
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
                current.cascade_direction = cascade_direction;
                current.scale_factor = layout.scale_factor.get();
                current.spatial_generation = spatial.spatial_generation;
                current.page_count = prepared_child.as_ref().map_or(1, |frame| frame.page_count);
                current.page = prepared_child.as_ref().map_or(0, |frame| frame.page);
            }
            let Some(frame_id) = active
                .reducer
                .state
                .stack
                .last()
                .map(|frame| frame.frame_id)
            else {
                return;
            };
            active.menu_id = menu_id.clone();
            active.current_frame_id = frame_id;
            active.always_on_top = always_on_top;
            active.activate_on_show = activate_on_show;
            active.navigation_layouts.insert(frame_id, layout.clone());
            active
                .navigation_presentations
                .insert(frame_id, effective_presentation);
            active
                .navigation_window_options
                .insert(frame_id, (always_on_top, activate_on_show));
            active
                .navigation_resources
                .insert(frame_id, resources.clone());
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
            active.navigation_sounds.insert(frame_id, sounds);
            if let (Some(reply), Some(frame)) = (active.prepared.as_mut(), prepared_child) {
                active.navigation_frames.insert(frame_id, frame.clone());
                reply.frame = frame;
            }
        }
        self.refresh_active_scene(id, out);
    }
    fn refresh_active_scene(&mut self, id: &SessionId, out: &mut Vec<ControllerEvent>) {
        let Some((frame, current_frame_id, previous_menu, page_input)) = self
            .active
            .as_ref()
            .filter(|active| &active.session_id == id)
            .and_then(|active| {
                active.reducer.state.stack.last().cloned().map(|frame| {
                    let previous_menu = active.menu_id.clone();
                    let page_input = active
                        .navigation_frames
                        .get(&frame.frame_id)
                        .filter(|prepared| prepared.page != frame.page)
                        .cloned()
                        .or_else(|| {
                            active
                                .prepared
                                .as_ref()
                                .filter(|prepared| {
                                    prepared.menu_id == frame.menu_id
                                        && prepared.frame.page != frame.page
                                })
                                .map(|prepared| prepared.frame.clone())
                        });
                    (frame, active.current_frame_id, previous_menu, page_input)
                })
            })
        else {
            return;
        };

        let mut page_projection = None;
        if let Some(previous) = page_input {
            let base = previous.base_menu.clone();
            let style = super::skin::compile_menu_tree(&self.document, &base).ok();
            let mut projected = project_menu_frame_with_style(
                &base,
                previous.static_cells.clone(),
                &previous.dynamic,
                frame.page,
                style.as_ref(),
            );
            projected.alternates = previous.alternates.clone();
            let mut layout = match layout_document_menu_fixed_center(
                &self.document,
                &projected.menu,
                frame.origin,
                self.active
                    .as_ref()
                    .filter(|active| &active.session_id == id)
                    .expect("active session was checked above")
                    .spatial
                    .work_area,
                self.active
                    .as_ref()
                    .filter(|active| &active.session_id == id)
                    .expect("active session was checked above")
                    .spatial
                    .scale_factor,
                0.55,
            ) {
                Ok(layout) => layout,
                Err(error) => {
                    out.push(ControllerEvent::LayoutFailed {
                        menu_id: frame.menu_id,
                        error,
                    });
                    return;
                }
            };
            let child_frame = frame.parent_frame_id.is_some();
            augment_special_cells(&mut layout, &projected.cells, &projected.menu, child_frame);
            apply_prepared_availability(&mut layout, &projected);
            if child_frame
                && let Some(center) = layout
                    .cells
                    .iter_mut()
                    .find(|cell| cell.cell_id.as_str() == "__center")
            {
                center.actionable = true;
            }
            let generation = self.layout_generation_for(id);
            let work_area = self
                .active
                .as_ref()
                .filter(|active| &active.session_id == id)
                .expect("page projection requires its active session")
                .spatial
                .work_area;
            let (resources, sounds, diagnostics) =
                self.prepare_scene_resources(&projected.menu, &layout, generation, work_area);
            out.extend(diagnostics.into_iter().map(ControllerEvent::Diagnostic));
            page_projection = Some((projected, layout, resources, sounds));
        }

        let restore_frame = current_frame_id != frame.frame_id;
        if restore_frame {
            let restored = self
                .active
                .as_ref()
                .filter(|active| &active.session_id == id)
                .and_then(|active| {
                    Some((
                        active.navigation_layouts.get(&frame.frame_id)?.clone(),
                        active.navigation_resources.get(&frame.frame_id)?.clone(),
                        active
                            .navigation_window_options
                            .get(&frame.frame_id)
                            .copied()?,
                    ))
                });
            let Some((layout, resources, (always_on_top, activate_on_show))) = restored else {
                out.push(ControllerEvent::Error(format!(
                    "radial frame {} could not be restored",
                    frame.frame_id.0
                )));
                return;
            };
            if let Some(active) = self
                .active
                .as_mut()
                .filter(|active| &active.session_id == id)
            {
                active.current_frame_id = frame.frame_id;
                active.menu_id = frame.menu_id.clone();
                active.layout = layout;
                active.resources = resources;
                active.always_on_top = always_on_top;
                active.activate_on_show = activate_on_show;
                if let Some(saved) = active.navigation_frames.get(&frame.frame_id).cloned()
                    && let Some(prepared) = active.prepared.as_mut()
                {
                    prepared.frame = saved;
                }
                if let Some(sounds) = active.navigation_sounds.get(&frame.frame_id).cloned() {
                    if let Some(audio) = &mut active.audio {
                        audio.replace_sounds(sounds);
                    }
                }
                if let Some(audio) = &mut active.audio {
                    audio.cue(
                        id,
                        active.audio_generation,
                        RadialCue::SubmenuClose(previous_menu),
                        monotonic_ms(),
                    );
                }
            }
        } else if let Some((projected, layout, resources, sounds)) = page_projection {
            if let Some(current) = self
                .active
                .as_mut()
                .filter(|active| &active.session_id == id)
            {
                if let Some(frame_state) = current.reducer.state.stack.last_mut() {
                    frame_state.page = projected.page;
                    frame_state.page_count = projected.page_count;
                }
                current.current_frame_id = frame.frame_id;
                current.menu_id = frame.menu_id.clone();
                current.layout = layout.clone();
                current.resources = resources.clone();
                current
                    .navigation_frames
                    .insert(frame.frame_id, projected.clone());
                current.navigation_layouts.insert(frame.frame_id, layout);
                current
                    .navigation_resources
                    .insert(frame.frame_id, resources);
                current
                    .navigation_sounds
                    .insert(frame.frame_id, sounds.clone());
                if let Some(prepared) = current.prepared.as_mut() {
                    prepared.frame = projected;
                }
                if let Some(audio) = &mut current.audio {
                    audio.replace_sounds(sounds);
                }
            }
        } else if let Some(active) = self
            .active
            .as_mut()
            .filter(|active| &active.session_id == id)
        {
            active.current_frame_id = frame.frame_id;
            active.menu_id = frame.menu_id.clone();
        }
        let generation = self.layout_generation_for(id);
        let Some(active) = self
            .active
            .as_mut()
            .filter(|active| &active.session_id == id)
        else {
            return;
        };
        let selected = active.reducer.state.hovered.as_ref().or(active
            .reducer
            .state
            .selected
            .as_ref());
        let visible_tooltip = active
            .tooltip_hover
            .visible()
            .filter(|visible| {
                visible.session_id == *id
                    && visible.frame_id == active.current_frame_id
                    && visible.layout_generation == generation
            })
            .map(|visible| visible.cell_id.clone());
        let (scene, layout) = runtime_present_scene(active, generation, selected, visible_tooltip);
        active.layout = layout.clone();
        let command = NativeCommand::Present {
            session_id: id.clone(),
            scene,
            layout,
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
        let active_next = self
            .active
            .as_ref()
            .and_then(|active| active.reducer.state.stack.last())
            .map(|frame| frame.geometry_generation.saturating_add(1))
            .unwrap_or(0);
        let generation = self.layout_generation.max(active_next);
        self.layout_generation = generation.saturating_add(1);
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
    fn sync_tooltip_deadline(&mut self) {
        let deadline = self.active.as_ref().and_then(|active| {
            active
                .tooltip_hover
                .candidate()
                .map(|(_, deadline)| deadline)
        });
        if let Some(deadline) = deadline {
            self.arm_deadline(DeadlineKey::Tooltip, deadline);
        } else if let Some(scheduler) = &self.deadline_scheduler {
            scheduler.cancel(DeadlineKey::Tooltip);
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
        if let Some(active) = self
            .active
            .as_mut()
            .filter(|active| active.session_id == id)
        {
            active.tooltip_hover.cancel();
        }
        if let Some(scheduler) = &self.deadline_scheduler {
            scheduler.cancel(DeadlineKey::Tooltip);
        }
        if let Some(active) = self
            .active
            .as_mut()
            .filter(|active| active.session_id == id)
        {
            active.closing = true;
        }
        let close_failed = self.host.as_ref().is_some_and(|host| {
            host.send(NativeCommand::Close {
                session_id: id.clone(),
                reason,
            })
            .is_err()
        });
        if close_failed {
            let invocation_id = self
                .pending
                .as_ref()
                .filter(|pending| pending.session_id == id)
                .map(|pending| pending.invocation_id)
                .or_else(|| {
                    self.active
                        .as_ref()
                        .filter(|active| active.session_id == id)
                        .map(|active| active.invocation_id)
                });
            self.retire_host();
            // A failed transport send still retires the native host.  Feed
            // the same correlated terminal event into the handoff reducer so
            // an ActionHandoff cannot remain stuck waiting for a Closed event
            // that the retired host can no longer deliver.
            let mut handoff_events = Vec::new();
            self.reduce_handoff(
                DispatchEvent::Closed {
                    session_id: id.clone(),
                    reason,
                },
                &mut handoff_events,
            );
            self.terminal_events.extend(handoff_events);
            if let Some(invocation_id) = invocation_id {
                self.terminal_events.push_back(ControllerEvent::Closed {
                    invocation_id,
                    session_id: id,
                    reason,
                });
            }
        }
    }
    fn retire_host(&mut self) {
        if let Some(scheduler) = &self.deadline_scheduler {
            scheduler.cancel(DeadlineKey::Tooltip);
        }
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
    pub fn active_frame_matches(
        &self,
        session_id: &SessionId,
        frame_id: FrameId,
        menu_id: &MenuId,
    ) -> bool {
        self.active.as_ref().is_some_and(|active| {
            &active.session_id == session_id
                && active.current_frame_id == frame_id
                && &active.menu_id == menu_id
                && active
                    .reducer
                    .state
                    .stack
                    .last()
                    .is_some_and(|frame| frame.frame_id == frame_id && &frame.menu_id == menu_id)
        })
    }
    /// Keep the radial tree visible while giving keyboard focus to the legacy
    /// launcher grid. Pointer interaction can resume menu ownership through
    /// the existing `MenuInteraction` event.
    pub fn suspend_active_keyboard(&mut self) {
        self.set_grid_keyboard_owner(GridKeyboardOwner::LegacyLauncher);
    }
    /// Return keyboard ownership to the still-visible radial tree after the
    /// legacy launcher grid is hidden again.  The native session and pointer
    /// state remain untouched, so a no-mouse grid toggle cannot lose context.
    pub fn resume_active_keyboard(&mut self) {
        self.set_grid_keyboard_owner(GridKeyboardOwner::RadialMenu);
    }
    pub fn set_grid_keyboard_owner(&mut self, owner: GridKeyboardOwner) {
        self.grid_keyboard_owner = owner;
        let Some(session_id) = self.active.as_ref().map(|active| active.session_id.clone()) else {
            return;
        };
        let event = match owner {
            GridKeyboardOwner::RadialMenu => SessionEvent::ResumeKeyboard,
            GridKeyboardOwner::LegacyLauncher => SessionEvent::SuspendKeyboard,
        };
        let mut ignored = Vec::new();
        self.session_event(&session_id, event, &mut ignored);
    }
    pub fn grid_keyboard_owner(&self) -> GridKeyboardOwner {
        self.grid_keyboard_owner
    }
    /// Transfer keyboard ownership for one legacy-grid toggle. The caller
    /// supplies the grid state immediately before that edge so several queued
    /// toggles remain ordered without synthesizing pointer movement.
    pub fn handle_legacy_grid_toggle(&mut self, grid_was_visible: bool) {
        if grid_was_visible {
            self.set_grid_keyboard_owner(GridKeyboardOwner::RadialMenu);
        } else {
            self.set_grid_keyboard_owner(GridKeyboardOwner::LegacyLauncher);
        }
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

fn close_reason_for_lifecycle(reason: LifecycleCancellation) -> CloseReason {
    match reason {
        LifecycleCancellation::SettingsReload | LifecycleCancellation::SessionReplaced => {
            CloseReason::SettingsReload
        }
        LifecycleCancellation::FeatureDisabled => CloseReason::FeatureDisabled,
        LifecycleCancellation::HostFailure => CloseReason::HostFailure,
        LifecycleCancellation::HookFailure => CloseReason::HookFailure,
        LifecycleCancellation::Shutdown => CloseReason::Shutdown,
        LifecycleCancellation::Suspend => CloseReason::Suspend,
        LifecycleCancellation::SessionLock => CloseReason::SessionLock,
        LifecycleCancellation::DesktopUnavailable => CloseReason::DesktopUnavailable,
        LifecycleCancellation::PriorityPreempted => CloseReason::ExclusiveTool,
    }
}
fn owner_cell(owner: InputOwner) -> Option<super::model::CellId> {
    match owner {
        InputOwner::Actionable(id) => Some(id),
        _ => None,
    }
}

fn prune_runtime_navigation(active: &mut ActiveSession) {
    let live: BTreeSet<_> = active
        .reducer
        .state
        .stack
        .iter()
        .map(|frame| frame.frame_id)
        .collect();
    active
        .navigation_layouts
        .retain(|frame_id, _| live.contains(frame_id));
    active
        .navigation_frames
        .retain(|frame_id, _| live.contains(frame_id));
    active
        .navigation_presentations
        .retain(|frame_id, _| live.contains(frame_id));
    active
        .navigation_resources
        .retain(|frame_id, _| live.contains(frame_id));
    active
        .navigation_window_options
        .retain(|frame_id, _| live.contains(frame_id));
    active
        .navigation_sounds
        .retain(|frame_id, _| live.contains(frame_id));
}

fn runtime_present_scene(
    active: &ActiveSession,
    generation: u64,
    selected: Option<&super::model::CellId>,
    visible_tooltip: Option<super::model::CellId>,
) -> (super::render::VectorScene, LayoutSnapshot) {
    let current_frame = active.reducer.state.stack.last();
    let current_id = current_frame
        .map(|frame| frame.frame_id)
        .unwrap_or(active.current_frame_id);
    let visible_ids = visible_frame_ids(&active.reducer.state.stack, |frame_id| {
        active
            .navigation_presentations
            .get(&frame_id)
            .copied()
            .unwrap_or(SubmenuPresentation::SameCenter)
    });
    let own_layout = active
        .navigation_layouts
        .get(&current_id)
        .cloned()
        .unwrap_or_else(|| active.layout.clone());
    if visible_ids.len() <= 1 {
        return (
            build_scene_prepared_selected_tooltip(
                &own_layout,
                generation,
                &active.resources,
                selected,
                visible_tooltip.as_ref(),
                active.spatial.work_area,
            ),
            own_layout,
        );
    }
    let layers: Vec<_> = visible_ids
        .into_iter()
        .filter_map(|frame_id| {
            let frame = active
                .reducer
                .state
                .stack
                .iter()
                .find(|frame| frame.frame_id == frame_id)?;
            Some(SceneLayer {
                frame_id,
                layout: active.navigation_layouts.get(&frame_id)?.clone(),
                resources: active.navigation_resources.get(&frame_id)?.clone(),
                selected: if frame_id == current_id {
                    selected.cloned()
                } else {
                    frame.selected.clone()
                },
                visible_tooltip: (frame_id == current_id)
                    .then_some(visible_tooltip.clone())
                    .flatten(),
            })
        })
        .collect();
    compose_layered_scene(&layers, generation, active.spatial.work_area)
        .map(|layered| (layered.scene, layered.layout))
        .unwrap_or_else(|| {
            (
                build_scene_prepared_selected_tooltip(
                    &own_layout,
                    generation,
                    &active.resources,
                    selected,
                    visible_tooltip.as_ref(),
                    active.spatial.work_area,
                ),
                own_layout,
            )
        })
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
    force_child_back: bool,
) {
    let special_visual = layout
        .cells
        .first()
        .map(|cell| cell.visual.clone())
        .unwrap_or_default();
    let center_id = super::model::CellId::new("__center");
    if !layout.cells.iter().any(|cell| cell.cell_id == center_id)
        && (force_child_back
            || prepared.contains_key(&center_id)
            || menu.center_secondary_action.is_some()
            || menu.center_control.is_some()
            || menu.center_secondary_control.is_some())
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
        layout.cells.insert(
            0,
            CellLayout {
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
            },
        );
    }
}

fn child_center_is_back(active: &ActiveSession, cell: &super::model::CellId) -> bool {
    active.reducer.state.stack.len() > 1 && cell.as_str() == "__center"
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
        Control::Drag => CellRole::Drag,
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
                    Control::Drag => CellRole::Drag,
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
    use crate::radial::model::CellId;

    #[test]
    fn lifecycle_cancellation_reasons_retain_native_close_identity() {
        for (cancellation, close) in [
            (
                LifecycleCancellation::SettingsReload,
                CloseReason::SettingsReload,
            ),
            (
                LifecycleCancellation::FeatureDisabled,
                CloseReason::FeatureDisabled,
            ),
            (LifecycleCancellation::HostFailure, CloseReason::HostFailure),
            (LifecycleCancellation::HookFailure, CloseReason::HookFailure),
            (LifecycleCancellation::Shutdown, CloseReason::Shutdown),
            (LifecycleCancellation::Suspend, CloseReason::Suspend),
            (LifecycleCancellation::SessionLock, CloseReason::SessionLock),
            (
                LifecycleCancellation::DesktopUnavailable,
                CloseReason::DesktopUnavailable,
            ),
            (
                LifecycleCancellation::SessionReplaced,
                CloseReason::SettingsReload,
            ),
            (
                LifecycleCancellation::PriorityPreempted,
                CloseReason::ExclusiveTool,
            ),
        ] {
            assert_eq!(close_reason_for_lifecycle(cancellation), close);
        }
    }

    #[test]
    fn disable_retires_demand_driven_deadline_worker() {
        let mut controller = RadialController::with_factory(
            Arc::new(RadialDocument::starter()),
            false,
            Arc::new(|| Err("host must remain lazy".into())),
        );
        let (wake, _wake_rx) = mpsc::channel();
        controller.deadline_scheduler = Some(HandoffDeadlineScheduler::spawn(wake).unwrap());
        controller.disable();
        assert!(controller.deadline_scheduler.is_none());
        assert!(controller.host.is_none());
    }
    use image::{DynamicImage, ImageOutputFormat, Rgba, RgbaImage};
    use std::io::Cursor;
    use std::sync::Mutex;
    #[derive(Default)]
    struct NoopViewport;
    impl crate::visibility::ViewportCtx for NoopViewport {
        fn send_viewport_cmd(&self, _cmd: eframe::egui::ViewportCommand) {}

        fn request_repaint(&self) {}
    }
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

    struct CloseFailingFake {
        events: Arc<Mutex<VecDeque<NativeEvent>>>,
    }

    impl HostPort for CloseFailingFake {
        fn send(&self, command: NativeCommand) -> Result<(), String> {
            if matches!(command, NativeCommand::Close { .. }) {
                Err("test close transport failure".into())
            } else {
                Ok(())
            }
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

    #[test]
    fn close_send_failure_emits_correlated_terminal_feedback() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let factory_events = Arc::clone(&events);
        let mut controller = RadialController::with_factory(
            Arc::new(RadialDocument::starter()),
            false,
            Arc::new(move || {
                Ok(Box::new(CloseFailingFake {
                    events: Arc::clone(&factory_events),
                }))
            }),
        );
        controller.handle_intents(vec![open()], false);
        let pending = controller.pending.as_ref().expect("pending session");
        let invocation_id = pending.invocation_id;
        let session_id = pending.session_id.clone();
        let generation = pending.generation;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: session_id.clone(),
            layout_generation: generation,
        });
        controller.poll();

        controller.close(CloseReason::Dismissed, Some(&session_id));
        let events = controller.poll();
        assert!(events.iter().any(|event| matches!(
            event,
            ControllerEvent::Closed {
                invocation_id: id,
                session_id: closed,
                reason: CloseReason::Dismissed,
            } if *id == invocation_id && closed == &session_id
        )));
        assert!(controller.active.is_none());
        assert!(controller.pending.is_none());
    }

    #[test]
    fn close_send_failure_completes_action_handoff_once() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let factory_events = Arc::clone(&events);
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
        let mut controller = RadialController::with_factory(
            Arc::new(document),
            true,
            Arc::new(move || {
                Ok(Box::new(CloseFailingFake {
                    events: Arc::clone(&factory_events),
                }))
            }),
        );
        let mut open_intent = open();
        if let InvocationIntent::OpenRadial {
            trigger_still_down, ..
        } = &mut open_intent
        {
            *trigger_still_down = false;
        }
        controller.handle_intents(vec![open_intent], false);
        let session_id = controller.pending.as_ref().unwrap().session_id.clone();
        let generation = controller.pending.as_ref().unwrap().generation;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: session_id.clone(),
            layout_generation: generation,
        });
        controller.poll();

        let cell_id = controller.document.menus[0].rings[0].cells[0].id.clone();
        let activation = InvocationIntent::ActivateItem {
            id: InvocationId(88),
            menu_id: MenuId::new("starter"),
            cell_id,
            gesture: ClickGesture::Primary,
            scope: TriggerScope::MenuLocal,
            source: crate::commands::ActivationSource::RadialShortcut,
            trigger_still_down: true,
        };
        let activation_events = controller.handle_intents(vec![activation], false);
        assert!(
            activation_events
                .iter()
                .all(|event| !matches!(event, ControllerEvent::DispatchRequested(_)))
        );
        assert!(controller.active.is_none());
        assert!(controller.pending.is_none());
        assert!(controller.poll().iter().any(|event| matches!(
            event,
            ControllerEvent::Closed {
                reason: CloseReason::ActionHandoff,
                ..
            }
        )));

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
    fn correlated_drag_relocates_three_level_same_center_tree_and_back_restores_each_frame() {
        let mut document = RadialDocument::starter();
        let favorites_id = MenuId::new("starter-favorites");
        let applications_id = MenuId::new("starter-applications");
        for menu in &mut document.menus {
            if menu.id == document.default_menu_id
                || menu.id == favorites_id
                || menu.id == applications_id
            {
                menu.submenu_presentation = SubmenuPresentation::SameCenter;
            }
        }
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == favorites_id)
            .unwrap()
            .rings[0]
            .cells[0]
            .content = CellContent::Submenu {
            menu_id: applications_id.clone(),
        };
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == favorites_id)
            .unwrap()
            .rings[0]
            .radius = 70.0;
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == applications_id)
            .unwrap()
            .rings[0]
            .radius = 130.0;

        let sent = Arc::new(Mutex::new(Vec::new()));
        let sent_by_factory = Arc::clone(&sent);
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let events_by_factory = Arc::clone(&events);
        let mut controller = RadialController::with_factory(
            Arc::new(document),
            true,
            Arc::new(move || {
                Ok(Box::new(Fake {
                    sent: Arc::clone(&sent_by_factory),
                    events: Arc::clone(&events_by_factory),
                }))
            }),
        );
        controller.handle_intents(vec![open()], false);
        let pending = controller.pending.as_ref().unwrap();
        let session_id = pending.session_id.clone();
        let root_generation = pending.generation;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: session_id.clone(),
            layout_generation: root_generation,
        });
        controller.poll();

        let root_origin = controller.active.as_ref().unwrap().layout.origin;
        let root_center = controller.active.as_ref().unwrap().spatial.visible_center;
        assert_eq!(root_origin, root_center);
        let first_delta = PhysicalPoint { x: 77.0, y: -41.0 };
        events.lock().unwrap().push_back(NativeEvent::Relocated {
            session_id: session_id.clone(),
            layout_generation: root_generation,
            from: PhysicalPoint::default(),
            to: first_delta,
        });
        controller.poll();

        let moved_center = PhysicalPoint {
            x: root_center.x + first_delta.x,
            y: root_center.y + first_delta.y,
        };
        let after_drag = controller.active.as_ref().unwrap();
        assert_eq!(after_drag.spatial.visible_center, moved_center);
        assert_eq!(after_drag.layout.origin, moved_center);
        assert_eq!(after_drag.reducer.state.spatial_generation, 1);
        assert_eq!(
            after_drag.reducer.state.stack[0].geometry_generation,
            controller.layout_generation_for(&session_id)
        );
        let first_generation = controller.layout_generation_for(&session_id);
        let before_stale = (
            after_drag.layout.clone(),
            after_drag.spatial,
            after_drag.reducer.state.clone(),
            controller.layout_generation,
            sent.lock().unwrap().len(),
        );
        events.lock().unwrap().push_back(NativeEvent::Relocated {
            session_id: session_id.clone(),
            layout_generation: root_generation,
            from: PhysicalPoint::default(),
            to: PhysicalPoint {
                x: 1000.0,
                y: 1000.0,
            },
        });
        controller.poll();
        let after_stale = controller.active.as_ref().unwrap();
        assert_eq!(after_stale.layout, before_stale.0);
        assert_eq!(after_stale.spatial, before_stale.1);
        assert_eq!(after_stale.reducer.state, before_stale.2);
        assert_eq!(controller.layout_generation, before_stale.3);
        assert_eq!(sent.lock().unwrap().len(), before_stale.4);

        let mut output = Vec::new();
        controller.open_submenu(
            &session_id,
            &CellId::new("starter-root-favorites"),
            &mut output,
        );
        let favorites_frame = controller.active.as_ref().unwrap().current_frame_id;
        assert_eq!(
            controller.active.as_ref().unwrap().layout.origin,
            moved_center
        );
        controller.open_submenu(
            &session_id,
            &CellId::new("starter-favorites-source"),
            &mut output,
        );
        let applications_frame = controller.active.as_ref().unwrap().current_frame_id;
        assert!(
            controller
                .active
                .as_ref()
                .unwrap()
                .reducer
                .state
                .stack
                .iter()
                .all(|frame| frame.origin == moved_center)
        );

        let back_generation = controller.next_layout_generation();
        let pointer = controller.active.as_ref().unwrap().pointer;
        controller.session_event(
            &session_id,
            SessionEvent::Back {
                geometry_generation: back_generation,
                pointer_baseline: pointer,
            },
            &mut output,
        );
        let back_to_favorites = controller.active.as_ref().unwrap();
        assert_eq!(back_to_favorites.current_frame_id, favorites_frame);
        assert_eq!(back_to_favorites.layout.origin, moved_center);
        assert_eq!(
            back_to_favorites.navigation_layouts.get(&favorites_frame),
            Some(&back_to_favorites.layout)
        );

        let second_delta = PhysicalPoint { x: -23.0, y: 58.0 };
        let second_generation = controller.layout_generation_for(&session_id);
        events.lock().unwrap().push_back(NativeEvent::Relocated {
            session_id: session_id.clone(),
            layout_generation: second_generation,
            from: PhysicalPoint::default(),
            to: second_delta,
        });
        controller.poll();
        let moved_again = PhysicalPoint {
            x: moved_center.x + second_delta.x,
            y: moved_center.y + second_delta.y,
        };
        let after_second_drag = controller.active.as_ref().unwrap();
        assert_eq!(after_second_drag.spatial.visible_center, moved_again);
        assert_eq!(after_second_drag.layout.origin, moved_again);
        assert_eq!(
            after_second_drag.navigation_layouts[&favorites_frame].origin, moved_again,
            "live retained layouts remain translated after Back"
        );
        assert!(
            !after_second_drag
                .navigation_layouts
                .contains_key(&applications_frame),
            "popped frame layouts are pruned"
        );

        let root_back_generation = controller.next_layout_generation();
        let pointer = controller.active.as_ref().unwrap().pointer;
        controller.session_event(
            &session_id,
            SessionEvent::Back {
                geometry_generation: root_back_generation,
                pointer_baseline: pointer,
            },
            &mut output,
        );
        let back_to_root = controller.active.as_ref().unwrap();
        assert_eq!(back_to_root.reducer.state.stack.len(), 1);
        assert_eq!(back_to_root.layout.origin, moved_again);
        assert_eq!(back_to_root.spatial.visible_center, moved_again);
        assert!(matches!(
            sent.lock().unwrap().last(),
            Some(NativeCommand::Present {
                session_id: presented_session,
                scene,
                layout,
                ..
            }) if presented_session == &session_id
                && layout == &back_to_root.layout
                && scene.generation == root_back_generation
        ));
        assert!(output.is_empty());
        assert!(first_generation > root_generation);
    }

    #[test]
    fn mixed_cascade_then_same_center_anchors_grandchild_to_current_parent_frame() {
        let mut document = RadialDocument::starter();
        let root_id = document.default_menu_id.clone();
        let favorites_id = MenuId::new("starter-favorites");
        let applications_id = MenuId::new("starter-applications");
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == root_id)
            .unwrap()
            .submenu_presentation = SubmenuPresentation::Cascade;
        let favorites = document
            .menus
            .iter_mut()
            .find(|menu| menu.id == favorites_id)
            .unwrap();
        favorites.submenu_presentation = SubmenuPresentation::SameCenter;
        favorites.rings[0].cells[0].content = CellContent::Submenu {
            menu_id: applications_id.clone(),
        };

        let events = Arc::new(Mutex::new(VecDeque::new()));
        let factory_events = Arc::clone(&events);
        let mut controller = RadialController::with_factory(
            Arc::new(document),
            false,
            Arc::new(move || {
                Ok(Box::new(Fake {
                    sent: Arc::new(Mutex::new(Vec::new())),
                    events: Arc::clone(&factory_events),
                }))
            }),
        );
        controller.handle_intents(vec![open()], false);
        let pending = controller.pending.as_ref().unwrap();
        let session_id = pending.session_id.clone();
        let generation = pending.generation;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: session_id.clone(),
            layout_generation: generation,
        });
        controller.poll();
        let root_center = controller.active.as_ref().unwrap().spatial.visible_center;
        let ample_room = 10_000.0;
        controller.active.as_mut().unwrap().spatial.work_area = PhysicalRect {
            min: PhysicalPoint {
                x: root_center.x - ample_room,
                y: root_center.y - ample_room,
            },
            max: PhysicalPoint {
                x: root_center.x + ample_room,
                y: root_center.y + ample_room,
            },
        };

        let mut output = Vec::new();
        controller.open_submenu(
            &session_id,
            &CellId::new("starter-root-favorites"),
            &mut output,
        );
        let favorites_frame = controller.active.as_ref().unwrap().current_frame_id;
        let favorites_layout = controller.active.as_ref().unwrap().layout.clone();
        let favorites_center = favorites_layout.origin;
        assert_ne!(
            favorites_center, root_center,
            "Cascade must offset the child"
        );
        assert_eq!(
            controller.active.as_ref().unwrap().navigation_presentations[&favorites_frame],
            SubmenuPresentation::Cascade,
            "the roomy frozen work area must permit a local Cascade rather than fallback"
        );

        controller.open_submenu(
            &session_id,
            &CellId::new("starter-favorites-source"),
            &mut output,
        );
        let applications_frame = controller.active.as_ref().unwrap().current_frame_id;
        let applications_layout = controller.active.as_ref().unwrap().layout.clone();
        assert_eq!(applications_layout.origin, favorites_center);
        assert_eq!(
            controller.active.as_ref().unwrap().reducer.state.stack[2].origin,
            favorites_center
        );

        let back_generation = controller.next_layout_generation();
        let pointer = controller.active.as_ref().unwrap().pointer;
        controller.session_event(
            &session_id,
            SessionEvent::Back {
                geometry_generation: back_generation,
                pointer_baseline: pointer,
            },
            &mut output,
        );
        let restored = controller.active.as_ref().unwrap();
        assert_eq!(restored.current_frame_id, favorites_frame);
        assert_eq!(restored.layout, favorites_layout);
        assert_eq!(restored.layout.origin, favorites_center);
        assert_ne!(applications_frame, favorites_frame);
        assert!(output.is_empty());
    }

    #[test]
    fn cascade_relocation_preserves_all_live_layers_and_back_prunes_exact_frame() {
        let mut document = RadialDocument::starter();
        let root_id = document.default_menu_id.clone();
        let favorites_id = MenuId::new("starter-favorites");
        let applications_id = MenuId::new("starter-applications");
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == root_id)
            .unwrap()
            .submenu_presentation = SubmenuPresentation::Cascade;
        let favorites = document
            .menus
            .iter_mut()
            .find(|menu| menu.id == favorites_id)
            .unwrap();
        favorites.submenu_presentation = SubmenuPresentation::Cascade;
        favorites.rings[0].cells[0].content = CellContent::Submenu {
            menu_id: applications_id.clone(),
        };

        let events = Arc::new(Mutex::new(VecDeque::new()));
        let sent = Arc::new(Mutex::new(Vec::new()));
        let factory_events = Arc::clone(&events);
        let factory_sent = Arc::clone(&sent);
        let mut controller = RadialController::with_factory(
            Arc::new(document),
            false,
            Arc::new(move || {
                Ok(Box::new(Fake {
                    sent: Arc::clone(&factory_sent),
                    events: Arc::clone(&factory_events),
                }))
            }),
        );
        controller.handle_intents(vec![open()], false);
        let pending = controller.pending.as_ref().unwrap();
        let session_id = pending.session_id.clone();
        let generation = pending.generation;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: session_id.clone(),
            layout_generation: generation,
        });
        controller.poll();
        let root_center = controller.active.as_ref().unwrap().spatial.visible_center;
        let ample = 10_000.0;
        controller.active.as_mut().unwrap().spatial.work_area = PhysicalRect {
            min: PhysicalPoint {
                x: root_center.x - ample,
                y: root_center.y - ample,
            },
            max: PhysicalPoint {
                x: root_center.x + ample,
                y: root_center.y + ample,
            },
        };
        let mut output = Vec::new();
        controller.open_submenu(
            &session_id,
            &CellId::new("starter-root-favorites"),
            &mut output,
        );
        controller.open_submenu(
            &session_id,
            &CellId::new("starter-favorites-source"),
            &mut output,
        );
        assert!(output.is_empty());
        let before = controller.active.as_ref().unwrap();
        let live_ids: Vec<_> = before
            .reducer
            .state
            .stack
            .iter()
            .map(|frame| frame.frame_id)
            .collect();
        let scene_ids = before
            .layout
            .layered_input
            .as_ref()
            .expect("Cascade uses a composite input scene")
            .layers
            .iter()
            .map(|layer| layer.frame_id)
            .collect::<Vec<_>>();
        assert_eq!(live_ids, vec![FrameId(1), FrameId(2), FrameId(3)]);
        assert_eq!(scene_ids, live_ids);
        let origins = before
            .navigation_layouts
            .iter()
            .map(|(id, layout)| (*id, layout.origin))
            .collect::<BTreeMap<_, _>>();
        let drag = PhysicalPoint { x: 47.0, y: -31.0 };
        let relocation_generation = controller.layout_generation_for(&session_id);
        events.lock().unwrap().push_back(NativeEvent::Relocated {
            session_id: session_id.clone(),
            layout_generation: relocation_generation,
            from: PhysicalPoint::default(),
            to: drag,
        });
        controller.poll();
        let pointer = {
            let moved = controller.active.as_ref().unwrap();
            assert_eq!(
                moved
                    .layout
                    .layered_input
                    .as_ref()
                    .unwrap()
                    .layers
                    .iter()
                    .map(|layer| layer.frame_id)
                    .collect::<Vec<_>>(),
                live_ids
            );
            for (frame_id, origin) in origins {
                let translated = moved.navigation_layouts[&frame_id].origin;
                assert_eq!(translated.x, origin.x + drag.x);
                assert_eq!(translated.y, origin.y + drag.y);
            }
            moved.pointer
        };
        let back_generation = controller.next_layout_generation();
        controller.session_event(
            &session_id,
            SessionEvent::Back {
                geometry_generation: back_generation,
                pointer_baseline: pointer,
            },
            &mut output,
        );
        let backed = controller.active.as_ref().unwrap();
        assert_eq!(backed.reducer.state.stack.len(), 2);
        assert_eq!(
            backed
                .layout
                .layered_input
                .as_ref()
                .unwrap()
                .layers
                .iter()
                .map(|layer| layer.frame_id)
                .collect::<Vec<_>>(),
            vec![FrameId(1), FrameId(2)]
        );
        assert!(!backed.navigation_layouts.contains_key(&FrameId(3)));
    }

    #[test]
    fn child_center_always_backs_even_when_authored_control_conflicts_or_is_absent() {
        let mut document = RadialDocument::starter();
        let root_id = document.default_menu_id.clone();
        let favorites_id = MenuId::new("starter-favorites");
        let applications_id = MenuId::new("starter-applications");
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == root_id)
            .unwrap()
            .center_control = Some(Control::Close);
        let favorites = document
            .menus
            .iter_mut()
            .find(|menu| menu.id == favorites_id)
            .unwrap();
        favorites.submenu_presentation = SubmenuPresentation::SameCenter;
        favorites.center_control = Some(Control::Close);
        favorites.rings[0].cells[0].content = CellContent::Submenu {
            menu_id: applications_id.clone(),
        };
        let applications = document
            .menus
            .iter_mut()
            .find(|menu| menu.id == applications_id)
            .unwrap();
        applications.center_control = None;

        let events = Arc::new(Mutex::new(VecDeque::new()));
        let factory_events = Arc::clone(&events);
        let mut controller = RadialController::with_factory(
            Arc::new(document),
            false,
            Arc::new(move || {
                Ok(Box::new(Fake {
                    sent: Arc::new(Mutex::new(Vec::new())),
                    events: Arc::clone(&factory_events),
                }))
            }),
        );
        controller.handle_intents(vec![open()], false);
        let pending = controller.pending.as_ref().unwrap();
        let session_id = pending.session_id.clone();
        let generation = pending.generation;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: session_id.clone(),
            layout_generation: generation,
        });
        controller.poll();

        let root_center = InputOwner::Actionable(CellId::new("__center"));
        assert_eq!(
            controller.role_for_button(&session_id, &root_center, PointerButton::Primary),
            CellRole::Close,
            "root keeps its authored center control"
        );

        let mut output = Vec::new();
        controller.open_submenu(
            &session_id,
            &CellId::new("starter-root-favorites"),
            &mut output,
        );
        let favorites_frame = controller.active.as_ref().unwrap().current_frame_id;
        let favorites_layout = controller.active.as_ref().unwrap().layout.clone();
        let center_id = CellId::new("__center");
        let child_center = InputOwner::Actionable(center_id.clone());
        assert_eq!(
            super::super::render::input_owner(&favorites_layout, favorites_layout.center, false),
            child_center
        );
        assert_eq!(
            controller.role_for_button(&session_id, &child_center, PointerButton::Primary),
            CellRole::Back,
            "child Back must override the menu's authored Close control"
        );

        controller.open_submenu(
            &session_id,
            &CellId::new("starter-favorites-source"),
            &mut output,
        );
        let applications_frame = controller.active.as_ref().unwrap().current_frame_id;
        let applications_layout = controller.active.as_ref().unwrap().layout.clone();
        assert!(
            applications_layout
                .cells
                .iter()
                .any(|cell| cell.cell_id == center_id)
        );
        assert_eq!(
            controller.role_for_button(&session_id, &child_center, PointerButton::Primary),
            CellRole::Back,
            "a child with no authored center surface still gets Back"
        );

        let click_center = |controller: &mut RadialController| {
            let generation = controller.layout_generation_for(&session_id);
            let active = controller.active.as_ref().unwrap();
            let point = active.layout.center;
            events.lock().unwrap().extend([
                NativeEvent::PointerDown {
                    session_id: session_id.clone(),
                    layout_generation: generation,
                    owner: child_center.clone(),
                    point,
                    button: PointerButton::Primary,
                },
                NativeEvent::PointerUp {
                    session_id: session_id.clone(),
                    layout_generation: generation,
                    owner: child_center.clone(),
                    point,
                    button: PointerButton::Primary,
                },
            ]);
            controller.poll();
        };

        click_center(&mut controller);
        let after_apps_back = controller.active.as_ref().unwrap();
        assert_eq!(after_apps_back.current_frame_id, favorites_frame);
        assert_eq!(after_apps_back.layout, favorites_layout);
        click_center(&mut controller);
        let after_favorites_back = controller.active.as_ref().unwrap();
        assert_eq!(after_favorites_back.current_frame_id, FrameId(1));
        assert_eq!(after_favorites_back.reducer.state.stack.len(), 1);
        assert_ne!(applications_frame, favorites_frame);
        assert!(output.is_empty());
    }

    #[test]
    fn impossible_fixed_center_child_fit_leaves_session_and_native_unchanged() {
        let mut document = RadialDocument::starter();
        let root_id = document.default_menu_id.clone();
        let child_id = MenuId::new("starter-favorites");
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == root_id)
            .unwrap()
            .submenu_presentation = SubmenuPresentation::SameCenter;
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == child_id)
            .unwrap()
            .rings[0]
            .radius = 100_000.0;

        let events = Arc::new(Mutex::new(VecDeque::new()));
        let sent = Arc::new(Mutex::new(Vec::new()));
        let factory_events = Arc::clone(&events);
        let factory_sent = Arc::clone(&sent);
        let mut controller = RadialController::with_factory(
            Arc::new(document),
            true,
            Arc::new(move || {
                Ok(Box::new(Fake {
                    sent: Arc::clone(&factory_sent),
                    events: Arc::clone(&factory_events),
                }))
            }),
        );
        controller.handle_intents(vec![open()], false);
        let pending = controller.pending.as_ref().unwrap();
        let session_id = pending.session_id.clone();
        let generation = pending.generation;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: session_id.clone(),
            layout_generation: generation,
        });
        controller.poll();

        let active = controller.active.as_ref().unwrap();
        let before = (
            active.layout.clone(),
            active.spatial,
            active.reducer.state.clone(),
            active.navigation_layouts.clone(),
            active.current_frame_id,
            active.resources.clone(),
            active.navigation_resources.clone(),
            controller.layout_generation,
            sent.lock().unwrap().len(),
        );
        let mut output = Vec::new();
        controller.open_submenu(
            &session_id,
            &CellId::new("starter-root-favorites"),
            &mut output,
        );

        assert!(output.iter().any(|event| matches!(
            event,
            ControllerEvent::SubmenuPlacementFailed {
                child_menu_id,
                parent_presentation: SubmenuPresentation::SameCenter,
                message,
                ..
            } if child_menu_id == &child_id && message.contains("FixedCenterDoesNotFit")
        )));
        let active = controller.active.as_ref().unwrap();
        assert_eq!(active.layout, before.0);
        assert_eq!(active.spatial, before.1);
        assert_eq!(active.reducer.state, before.2);
        assert_eq!(active.navigation_layouts, before.3);
        assert_eq!(active.current_frame_id, before.4);
        assert_eq!(active.resources, before.5);
        assert_eq!(active.navigation_resources, before.6);
        assert_eq!(controller.layout_generation, before.7);
        assert_eq!(sent.lock().unwrap().len(), before.8);
    }

    #[test]
    fn debug_resource_census_is_opt_in_and_tracks_lazy_session_ownership() {
        let silent = RadialController::with_factory(
            Arc::new(RadialDocument::starter()),
            false,
            Arc::new(|| Err("unused".into())),
        );
        assert_eq!(silent.resource_snapshot(), None);

        let events = Arc::new(Mutex::new(VecDeque::new()));
        let made = Arc::new(Mutex::new(0));
        let mut observed = controller(events, made);
        let idle = observed.resource_snapshot().unwrap();
        assert_eq!(idle.native_hosts, 0);
        assert_eq!(idle.pending_sessions, 0);
        assert_eq!(idle.active_sessions, 0);

        observed.handle_intents(vec![open()], false);
        let opening = observed.resource_snapshot().unwrap();
        assert_eq!(opening.native_hosts, 1);
        assert_eq!(opening.pending_sessions, 1);
        assert_eq!(opening.active_sessions, 0);
    }

    #[test]
    fn typed_lifecycle_intent_closes_only_the_correlated_invocation() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let sent_by_factory = Arc::clone(&sent);
        let mut controller = RadialController::with_factory(
            Arc::new(RadialDocument::starter()),
            false,
            Arc::new(move || {
                Ok(Box::new(Fake {
                    sent: Arc::clone(&sent_by_factory),
                    events: Arc::new(Mutex::new(VecDeque::new())),
                }))
            }),
        );
        controller.handle_intents(vec![open()], false);
        controller.handle_intents(
            vec![InvocationIntent::CancelRadialLifecycle {
                id: InvocationId(99),
                reason: LifecycleCancellation::Suspend,
            }],
            false,
        );
        assert_eq!(sent.lock().unwrap().len(), 1, "stale id must be ignored");
        controller.handle_intents(
            vec![InvocationIntent::CancelRadialLifecycle {
                id: InvocationId(4),
                reason: LifecycleCancellation::Suspend,
            }],
            false,
        );
        assert!(matches!(
            sent.lock().unwrap().as_slice(),
            [
                NativeCommand::Open { .. },
                NativeCommand::Close {
                    reason: CloseReason::Suspend,
                    ..
                }
            ]
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
    fn drag_intent_is_forwarded_once_with_current_layout_generation() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let sent_by_factory = Arc::clone(&sent);
        let events_by_factory = Arc::clone(&events);
        let mut controller = RadialController::with_factory(
            Arc::new(RadialDocument::starter()),
            false,
            Arc::new(move || {
                Ok(Box::new(Fake {
                    sent: Arc::clone(&sent_by_factory),
                    events: Arc::clone(&events_by_factory),
                }))
            }),
        );
        controller.handle_intents(vec![open()], false);
        let pending = controller.pending.as_ref().unwrap();
        let session_id = pending.session_id.clone();
        let generation = pending.generation;
        let center = pending.layout.center;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: session_id.clone(),
            layout_generation: generation,
        });
        controller.poll();
        for event in [
            NativeEvent::PointerDown {
                session_id: session_id.clone(),
                layout_generation: generation,
                owner: InputOwner::Actionable(CellId::new("__center")),
                point: center,
                button: PointerButton::Primary,
            },
            NativeEvent::PointerMoved {
                session_id: session_id.clone(),
                layout_generation: generation.saturating_sub(1),
                owner: InputOwner::Actionable(CellId::new("__center")),
                point: LogicalPoint {
                    x: center.x + 20.0,
                    y: center.y,
                },
            },
            NativeEvent::PointerMoved {
                session_id: session_id.clone(),
                layout_generation: generation,
                owner: InputOwner::Actionable(CellId::new("__center")),
                point: LogicalPoint {
                    x: center.x + 8.0,
                    y: center.y,
                },
            },
            NativeEvent::PointerMoved {
                session_id: session_id.clone(),
                layout_generation: generation,
                owner: InputOwner::Actionable(CellId::new("__center")),
                point: LogicalPoint {
                    x: center.x + 20.0,
                    y: center.y,
                },
            },
        ] {
            events.lock().unwrap().push_back(event);
            controller.poll();
        }
        let commands = sent.lock().unwrap();
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(command, NativeCommand::BeginSystemDrag { .. }))
                .count(),
            1
        );
        assert!(commands.iter().any(|command| matches!(
            command,
            NativeCommand::BeginSystemDrag {
                session_id: id,
                layout_generation,
            } if id == &session_id && *layout_generation == generation
        )));
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
            layout_generation: generation,
        });
        controller.poll();
        assert!(controller.active_input_scope().is_none());
        let point = controller.active.as_ref().unwrap().layout.center;
        events.lock().unwrap().push_back(NativeEvent::PointerMoved {
            session_id: session_id.clone(),
            layout_generation: generation,
            owner: InputOwner::Exterior,
            point,
        });
        controller.poll();
        assert!(controller.active_input_scope().is_none());
        events.lock().unwrap().push_back(NativeEvent::PointerDown {
            session_id,
            layout_generation: generation,
            owner: InputOwner::Protective,
            point,
            button: PointerButton::Primary,
        });
        controller.poll();
        assert!(controller.active_input_scope().is_some());
    }

    #[test]
    fn protective_runtime_hover_uses_geometry_without_changing_action_owner() {
        let mut document = RadialDocument::starter();
        let cell_id = document.menus[0].rings[0].cells[0].id.clone();
        document.menus[0].rings[0].cells[0].content = CellContent::Spacer;
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let factory_events = Arc::clone(&events);
        let mut controller = RadialController::with_factory(
            Arc::new(document),
            true,
            Arc::new(move || {
                Ok(Box::new(Fake {
                    sent: Arc::new(Mutex::new(Vec::new())),
                    events: Arc::clone(&factory_events),
                }))
            }),
        );
        controller.set_tooltip_preferences(TooltipPreferences {
            delay_ms: 0,
            ..TooltipPreferences::default()
        });
        controller.configure_resources(std::path::PathBuf::new());
        controller.handle_intents(vec![open()], false);
        let pending = controller.pending.as_ref().unwrap();
        let session_id = pending.session_id.clone();
        let generation = pending.generation;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: session_id.clone(),
            layout_generation: generation,
        });
        controller.poll();

        let layout = controller.active.as_ref().unwrap().layout.clone();
        let cell = layout
            .cells
            .iter()
            .find(|cell| cell.cell_id == cell_id)
            .unwrap();
        let point = layout
            .scale_factor
            .physical_to_logical(shape_center(&cell.shape, layout.scale_factor));
        assert!(layout.hit_test(point).is_none());
        assert_eq!(
            super::super::render::input_owner(&layout, point, false),
            InputOwner::Protective
        );
        events.lock().unwrap().push_back(NativeEvent::PointerMoved {
            session_id: session_id.clone(),
            layout_generation: generation,
            owner: InputOwner::Protective,
            point,
        });
        controller.poll();
        let active = controller.active.as_ref().unwrap();
        assert!(active.reducer.state.hovered.is_none());
        assert_eq!(
            active
                .tooltip_hover
                .visible()
                .map(|identity| &identity.cell_id),
            Some(&cell_id)
        );
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
        assert!(output.iter().any(|event| matches!(
            event,
            ControllerEvent::Diagnostic(diagnostic)
                if matches!(
                    &diagnostic.kind,
                    super::super::diagnostics::RadialDiagnosticKind::AssetUnavailable(
                        super::super::assets::AssetDiagnostic::NotFound
                    )
                )
                    && matches!(
                        &diagnostic.source,
                        super::super::diagnostics::RadialDiagnosticSource::Asset { identity, .. }
                            if identity.contains("missing.png")
                    )
                    && diagnostic.message.contains("missing.png")
        )));
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
    fn runtime_and_authoring_preview_prepare_identical_resources_and_scene() {
        let directory = tempfile::tempdir().unwrap();
        let image_path = directory.path().join("preview.png");
        std::fs::write(
            &image_path,
            base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
            )
            .unwrap(),
        )
        .unwrap();
        let mut document = RadialDocument::starter();
        document.menus[0].center_control = Some(Control::Close);
        document.menus[0].background_control = Some(Control::Back);
        document.menus[0].rings[0].cells[0].icon =
            Override::Value(super::super::model::MediaReference::ExternalFile {
                path: image_path.to_string_lossy().into_owned(),
            });
        document.menus[0].rings[0].cells[0].tooltip = Override::Value("Prepared tooltip".into());
        let document = Arc::new(document);
        let menu = document.menus[0].clone();
        let anchor = PhysicalPoint { x: 300.0, y: 300.0 };
        let work = PhysicalRect {
            min: PhysicalPoint { x: 0.0, y: 0.0 },
            max: PhysicalPoint { x: 600.0, y: 600.0 },
        };
        let scale = ScaleFactor::new(1.0).unwrap();
        let mut layout = layout_document_menu(&document, &menu, anchor, work, scale, 0.55).unwrap();
        augment_special_cells(&mut layout, &BTreeMap::new(), &menu, false);
        let mut controller = RadialController::with_factory(
            Arc::clone(&document),
            false,
            Arc::new(|| Err("unused host".into())),
        );
        controller.configure_resources(directory.path().to_path_buf());
        let (runtime_resources, _, runtime_diagnostics) =
            controller.prepare_scene_resources(&menu, &layout, 44, work);
        assert!(runtime_diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.kind,
            super::super::diagnostics::RadialDiagnosticKind::LabelTruncated
        )));
        let runtime_scene = build_scene_prepared_selected(&layout, 44, &runtime_resources, None);

        let mut preview =
            super::super::preparation::PreviewFramePreparer::new(directory.path().to_path_buf());
        let prepared = preview
            .prepare(
                &document,
                &menu.id,
                anchor,
                work,
                scale,
                44,
                None,
                &super::super::preparation::PreviewProjection::default(),
            )
            .unwrap();
        assert_eq!(prepared.diagnostics, runtime_diagnostics);
        assert_eq!(prepared.layout, layout);
        assert_eq!(prepared.resources, runtime_resources);
        assert_eq!(prepared.scene, runtime_scene);
        assert!(prepared.resources.media.len() == 1);
        let expected_tooltip_ids: BTreeSet<_> = prepared
            .layout
            .cells
            .iter()
            .filter(|cell| !cell.label.trim().is_empty())
            .map(|cell| cell.cell_id.clone())
            .collect();
        assert_eq!(
            prepared
                .resources
                .tooltips
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>(),
            expected_tooltip_ids,
            "AllCells prepares every non-empty user-facing label, even without custom descriptions"
        );
        for cell in prepared
            .layout
            .cells
            .iter()
            .filter(|cell| !cell.label.trim().is_empty())
        {
            let tooltip = prepared
                .resources
                .tooltips
                .get(&cell.cell_id)
                .expect("every non-empty label has an AllCells tooltip");
            assert_eq!(tooltip.full_label.as_ref(), cell.label.as_str());
        }
        for special_id in [CellId::new("__center"), CellId::new("__background")] {
            let special_text = prepared
                .resources
                .text
                .get(&special_id)
                .expect("special cells retain prepared text entries");
            assert!(special_text.source_text.is_empty());
            assert!(!prepared.resources.tooltips.contains_key(&special_id));
        }
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
    fn runtime_cascade_raster_keeps_parent_and_child_skin_pixels_distinct() {
        let directory = tempfile::tempdir().unwrap();
        let write_png = |name: &str, color: [u8; 4]| {
            let path = directory.path().join(name);
            let mut bytes = Cursor::new(Vec::new());
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(4, 4, Rgba(color)))
                .write_to(&mut bytes, ImageOutputFormat::Png)
                .unwrap();
            std::fs::write(&path, bytes.into_inner()).unwrap();
            path
        };
        let parent_color = [211, 17, 29, 255];
        let child_color = [19, 207, 43, 255];
        let parent_path = write_png("parent.png", parent_color);
        let child_path = write_png("child.png", child_color);
        let mut document = RadialDocument::starter();
        let root_id = document.default_menu_id.clone();
        let child_id = MenuId::new("starter-favorites");
        let grandchild_id = MenuId::new("starter-applications");
        for menu in &mut document.menus {
            if menu.id == root_id || menu.id == child_id {
                menu.submenu_presentation = SubmenuPresentation::Cascade;
            }
        }
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == child_id)
            .unwrap()
            .rings[0]
            .cells[0]
            .content = CellContent::Submenu {
            menu_id: grandchild_id.clone(),
        };
        document
            .skins
            .first_mut()
            .unwrap()
            .style
            .values
            .images
            .menu_background = Override::Value(super::super::model::MediaReference::ExternalFile {
            path: parent_path.to_string_lossy().into_owned(),
        });
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == child_id)
            .unwrap()
            .style
            .values
            .images
            .menu_background = Override::Value(super::super::model::MediaReference::ExternalFile {
            path: child_path.to_string_lossy().into_owned(),
        });
        let root_cell = document
            .menus
            .iter()
            .find(|menu| menu.id == root_id)
            .and_then(|menu| {
                menu.rings
                    .iter()
                    .flat_map(|ring| &ring.cells)
                    .find(|cell| {
                        matches!(&cell.content, CellContent::Submenu { menu_id } if menu_id == &child_id)
                    })
            })
            .map(|cell| cell.id.clone())
            .unwrap();
        let child_cell = document
            .menus
            .iter()
            .find(|menu| menu.id == child_id)
            .and_then(|menu| {
                menu.rings
                    .iter()
                    .flat_map(|ring| &ring.cells)
                    .find(|cell| {
                        matches!(&cell.content, CellContent::Submenu { menu_id } if menu_id == &grandchild_id)
                    })
            })
            .map(|cell| cell.id.clone())
            .unwrap();
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let factory_events = Arc::clone(&events);
        let mut controller = RadialController::with_factory(
            Arc::new(document),
            false,
            Arc::new(move || {
                Ok(Box::new(Fake {
                    sent: Arc::new(Mutex::new(Vec::new())),
                    events: Arc::clone(&factory_events),
                }))
            }),
        );
        controller.configure_resources(directory.path().to_path_buf());
        controller.handle_intents(vec![open()], false);
        let session_id = controller.pending.as_ref().unwrap().session_id.clone();
        let generation = controller.pending.as_ref().unwrap().generation;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: session_id.clone(),
            layout_generation: generation,
        });
        controller.poll();
        let mut out = Vec::new();
        controller.open_submenu(&session_id, &root_cell, &mut out);
        controller.open_submenu(&session_id, &child_cell, &mut out);
        let active = controller.active.as_ref().unwrap();
        let parent_frame_id = active.reducer.state.stack[0].frame_id;
        let child_frame_id = active.reducer.state.stack[1].frame_id;
        let (scene, _) =
            runtime_present_scene(active, active.reducer.state.session_generation, None, None);
        let scale = active.spatial.scale_factor;
        let raster = CompositorCache::default()
            .compose(&scene, scale, 0)
            .unwrap();
        let dpi = scale.get();
        let background_bounds = scene
            .primitives
            .iter()
            .filter_map(|primitive| match primitive {
                super::super::render::VectorPrimitive::Image { bounds, image, .. }
                    if image.frames.first().is_some_and(|frame| {
                        frame.rgba.get(0..4) == Some(parent_color.as_slice())
                            || frame.rgba.get(0..4) == Some(child_color.as_slice())
                    }) =>
                {
                    Some((*bounds, image.frames[0].rgba[0..4].to_vec()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(background_bounds.len() >= 2);
        let own_pixels = |expected: &[u8], own_bounds: super::super::geometry::LogicalRect| {
            let others = background_bounds
                .iter()
                .filter(|(bounds, color)| *bounds != own_bounds && color.as_slice() != expected)
                .map(|(bounds, _)| *bounds)
                .collect::<Vec<_>>();
            let min_x = ((f64::from(own_bounds.min.x) - f64::from(raster.logical_bounds.min.x))
                * dpi)
                .floor()
                .max(0.0) as u32;
            let max_x = ((f64::from(own_bounds.max.x) - f64::from(raster.logical_bounds.min.x))
                * dpi)
                .ceil()
                .min(f64::from(raster.image.width())) as u32;
            let min_y = ((f64::from(own_bounds.min.y) - f64::from(raster.logical_bounds.min.y))
                * dpi)
                .floor()
                .max(0.0) as u32;
            let max_y = ((f64::from(own_bounds.max.y) - f64::from(raster.logical_bounds.min.y))
                * dpi)
                .ceil()
                .min(f64::from(raster.image.height())) as u32;
            (min_y..max_y)
                .flat_map(|y| (min_x..max_x).map(move |x| (x, y)))
                .filter(|(x, y)| {
                    let point = LogicalPoint {
                        x: raster.logical_bounds.min.x + (*x as f64 + 0.5) as f32 / dpi as f32,
                        y: raster.logical_bounds.min.y + (*y as f64 + 0.5) as f32 / dpi as f32,
                    };
                    !others.iter().any(|bounds| {
                        point.x >= bounds.min.x
                            && point.x <= bounds.max.x
                            && point.y >= bounds.min.y
                            && point.y <= bounds.max.y
                    })
                })
                .filter(|(x, y)| raster.image.get_pixel(*x, *y).0.as_slice() == expected)
                .count()
        };
        let parent_bounds = background_bounds
            .iter()
            .find(|(_, color)| color.as_slice() == parent_color.as_slice())
            .map(|(bounds, _)| *bounds)
            .expect("parent background image should be emitted");
        let child_bounds = background_bounds
            .iter()
            .find(|(_, color)| color.as_slice() == child_color.as_slice())
            .map(|(bounds, _)| *bounds)
            .expect("child background image should be emitted");
        assert!(own_pixels(&parent_color, parent_bounds) > 0);
        assert!(own_pixels(&child_color, child_bounds) > 0);
        assert!(active.navigation_resources.contains_key(&parent_frame_id));
        assert!(active.navigation_resources.contains_key(&child_frame_id));
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
            dynamic_provenance: Default::default(),
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
    fn prepared_definition_frames_survive_back_and_reopen_with_new_frame_identity() {
        let document = RadialDocument::starter();
        let root_id = document.default_menu_id.clone();
        let child_id = document
            .menus
            .iter()
            .find(|menu| menu.id != root_id)
            .map(|menu| menu.id.clone())
            .expect("starter document should contain a child menu");
        let submenu_cell = document
            .menus
            .iter()
            .find(|menu| menu.id == root_id)
            .and_then(|menu| {
                menu.rings
                    .iter()
                    .flat_map(|ring| &ring.cells)
                    .find(|cell| {
                        matches!(&cell.content, CellContent::Submenu { menu_id } if menu_id == &child_id)
                    })
            })
            .map(|cell| cell.id.clone())
            .expect("starter root should point at a prepared child");
        let root_menu = document
            .menus
            .iter()
            .find(|menu| menu.id == root_id)
            .cloned()
            .unwrap();
        let child_menu = document
            .menus
            .iter()
            .find(|menu| menu.id == child_id)
            .cloned()
            .unwrap();
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let factory_events = Arc::clone(&events);
        let mut controller = RadialController::with_factory(
            Arc::new(document.clone()),
            false,
            Arc::new(move || {
                Ok(Box::new(Fake {
                    sent: Arc::new(Mutex::new(Vec::new())),
                    events: Arc::clone(&factory_events),
                }))
            }),
        );
        let (tx, rx) = mpsc::channel();
        let (wake, _wake_rx) = mpsc::channel();
        controller.preparation = Some(PreparationBridge {
            tx,
            rx,
            wake,
            next_generation: 1,
            waiting: None,
        });
        let requested = controller.handle_intents(vec![open()], false);
        let ControllerEvent::PrepareRequested(envelope) = requested
            .iter()
            .find(|event| matches!(event, ControllerEvent::PrepareRequested(_)))
            .expect("open should request preparation")
        else {
            unreachable!()
        };
        let root_frame = crate::radial::bindings::project_menu_frame(
            &root_menu,
            BTreeMap::new(),
            &BTreeMap::new(),
            0,
            64,
        );
        let child_frame = crate::radial::bindings::project_menu_frame(
            &child_menu,
            BTreeMap::new(),
            &BTreeMap::new(),
            0,
            64,
        );
        envelope
            .reply
            .send(RadialPrepareReply {
                generation: envelope.request.generation,
                invocation_id: envelope.request.invocation_id,
                menu_id: root_id.clone(),
                unavailable: BTreeMap::new(),
                dynamic: BTreeMap::new(),
                frame: root_frame.clone(),
                static_cells: BTreeMap::new(),
                frames: [
                    (root_id.clone(), root_frame),
                    (child_id.clone(), child_frame),
                ]
                .into(),
            })
            .unwrap();
        controller.poll();
        let session_id = controller.pending.as_ref().unwrap().session_id.clone();
        let generation = controller.pending.as_ref().unwrap().generation;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: session_id.clone(),
            layout_generation: generation,
        });
        controller.poll();
        let mut out = Vec::new();
        controller.open_submenu(&session_id, &submenu_cell, &mut out);
        let first_frame = controller.active.as_ref().unwrap().current_frame_id;
        assert!(
            controller
                .active
                .as_ref()
                .unwrap()
                .navigation_frames
                .get(&first_frame)
                .is_some_and(|frame| frame.menu.id == child_id)
        );
        let back_generation = controller.layout_generation_for(&session_id);
        controller.session_event(
            &session_id,
            SessionEvent::Back {
                geometry_generation: back_generation,
                pointer_baseline: LogicalPoint::default(),
            },
            &mut out,
        );
        let active = controller.active.as_ref().unwrap();
        assert!(
            active
                .prepared
                .as_ref()
                .is_some_and(|reply| reply.frames.contains_key(&child_id))
        );
        assert!(!active.navigation_frames.contains_key(&first_frame));

        controller.open_submenu(&session_id, &submenu_cell, &mut out);
        let reopened_frame = controller.active.as_ref().unwrap().current_frame_id;
        assert_ne!(reopened_frame, first_frame);
        let active = controller.active.as_ref().unwrap();
        assert!(
            active
                .navigation_frames
                .get(&reopened_frame)
                .is_some_and(|frame| frame.menu.id == child_id)
        );
        assert_eq!(active.prepared.as_ref().unwrap().frame.menu.id, child_id);
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
        assert!(events.iter().any(|event| matches!(
            event,
            ControllerEvent::ExternalSessionAdmitted {
                invocation_id: InvocationId(77),
                ..
            }
        )));
        let Some(ControllerEvent::PrepareRequested(envelope)) = events
            .iter()
            .find(|event| matches!(event, ControllerEvent::PrepareRequested(_)))
        else {
            panic!("expected preparation")
        };
        assert!(!envelope.request.allow_context_rules);
        assert_eq!(envelope.request.requested_menu_id.as_str(), "starter");
    }
    #[test]
    fn external_open_admission_precedes_native_preparation() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let made = Arc::new(Mutex::new(0));
        let mut controller = controller(events, made);
        let (tx, rx) = mpsc::channel();
        let (wake, _wake_rx) = mpsc::channel();
        controller.preparation = Some(PreparationBridge {
            tx,
            rx,
            wake,
            next_generation: 1,
            waiting: None,
        });
        let events = controller.handle_intents(
            vec![InvocationIntent::OpenExternalRadial {
                id: InvocationId(78),
                menu_id: MenuId::new("starter"),
                context_token: 780,
                interaction: InteractionMode::StickyClick,
                trigger_still_down: false,
            }],
            false,
        );
        assert!(matches!(
            events.as_slice(),
            [
                ControllerEvent::ExternalSessionAdmitted {
                    invocation_id: InvocationId(78)
                },
                ControllerEvent::PrepareRequested(_)
            ]
        ));
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
    fn display_change_close_rejects_queued_relocation_until_native_cleanup() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let sent = Arc::new(Mutex::new(Vec::new()));
        let factory_events = Arc::clone(&events);
        let factory_sent = Arc::clone(&sent);
        let mut controller = RadialController::with_factory(
            Arc::new(RadialDocument::starter()),
            true,
            Arc::new(move || {
                Ok(Box::new(Fake {
                    sent: Arc::clone(&factory_sent),
                    events: Arc::clone(&factory_events),
                }))
            }),
        );
        controller.handle_intents(vec![open()], false);
        let pending = controller.pending.as_ref().unwrap();
        let session_id = pending.session_id.clone();
        let generation = pending.generation;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: session_id.clone(),
            layout_generation: generation,
        });
        controller.poll();

        let active = controller.active.as_ref().unwrap();
        let before = (
            active.layout.clone(),
            active.spatial,
            active.reducer.state.clone(),
        );
        events.lock().unwrap().extend([
            NativeEvent::DisplayChanged {
                session_id: session_id.clone(),
            },
            NativeEvent::Relocated {
                session_id: session_id.clone(),
                layout_generation: generation,
                from: PhysicalPoint::default(),
                to: PhysicalPoint { x: 250.0, y: -90.0 },
            },
        ]);
        controller.poll();

        let active = controller.active.as_ref().unwrap();
        assert!(active.closing);
        assert_eq!(active.layout, before.0);
        assert_eq!(active.spatial, before.1);
        assert_eq!(active.reducer.state, before.2);
        assert!(sent.lock().unwrap().iter().any(|command| matches!(
            command,
            NativeCommand::Close {
                session_id: closed,
                reason: CloseReason::DisplayRelayout,
            } if closed == &session_id
        )));

        events.lock().unwrap().push_back(NativeEvent::Closed {
            session_id,
            reason: CloseReason::DisplayRelayout,
        });
        controller.poll();
        assert!(controller.active.is_none());
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
        let completed = controller.handle_intents(
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
            completed.as_slice(),
            [ControllerEvent::InvocationCompleted {
                invocation_id: InvocationId(11)
            }]
        ));
        assert!(matches!(
            sent.lock().unwrap().last(),
            Some(NativeCommand::Close { .. })
        ));

        let replaced = controller.handle_intents(
            vec![InvocationIntent::ToggleDirectMenu {
                id: InvocationId(12),
                menu_id: MenuId::new("second"),
                primary_key: 0x54,
                provenance: crate::radial::invocation::InputProvenance::Physical,
                trigger_still_down: true,
            }],
            false,
        );
        assert!(replaced.iter().any(|event| matches!(
            event,
            ControllerEvent::ExternalSessionAdmitted {
                invocation_id: InvocationId(12),
                ..
            }
        )));
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
    fn legacy_grid_toggle_round_trip_restores_radial_keyboard_without_mouse() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let events = Arc::new(Mutex::new(VecDeque::new()));
        let made = Arc::new(Mutex::new(0));
        let mut controller = controller(events.clone(), made);
        controller.handle_intents(
            vec![InvocationIntent::ToggleDirectMenu {
                id: InvocationId(1_001),
                menu_id: MenuId::new("starter"),
                primary_key: 0x54,
                provenance: crate::radial::invocation::InputProvenance::Physical,
                trigger_still_down: false,
            }],
            false,
        );
        let pending = controller.pending.as_ref().unwrap();
        let session_id = pending.session_id.clone();
        let generation = pending.generation;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id: session_id.clone(),
            layout_generation: generation,
        });
        controller.poll();
        let original_scope = controller.active_input_scope();
        assert!(original_scope.is_some());

        // Two queued hidden -> visible -> hidden edges must be preserved even
        // though viewport commands are applied only after this event batch.
        let visibility = AtomicBool::new(false);
        let mut toggles = crate::visibility::VisibilityToggleBatch::default();
        let first_was_visible = toggles.record_toggle(&visibility);
        assert!(!first_was_visible);
        controller.handle_legacy_grid_toggle(first_was_visible);
        assert!(controller.active_input_scope().is_none());

        let second_was_visible = toggles.record_toggle(&visibility);
        assert!(second_was_visible);
        controller.handle_legacy_grid_toggle(second_was_visible);
        assert!(!visibility.load(Ordering::SeqCst));
        assert_eq!(toggles.final_visible(), Some(false));
        assert_eq!(controller.active_input_scope(), original_scope);
    }

    #[test]
    fn legacy_hotkey_trigger_transfers_keyboard_for_direct_radial_session() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let events = Arc::new(Mutex::new(VecDeque::new()));
        let made = Arc::new(Mutex::new(0));
        let mut controller = controller(events.clone(), made);
        controller.handle_intents(
            vec![InvocationIntent::ToggleDirectMenu {
                id: InvocationId(1_003),
                menu_id: MenuId::new("starter"),
                primary_key: 0x54,
                provenance: crate::radial::invocation::InputProvenance::Physical,
                trigger_still_down: false,
            }],
            false,
        );
        let pending = controller.pending.as_ref().unwrap();
        let session_id = pending.session_id.clone();
        let generation = pending.generation;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id,
            layout_generation: generation,
        });
        controller.poll();
        let original_scope = controller.active_input_scope();
        assert!(original_scope.is_some());

        let trigger = crate::hotkey::HotkeyTrigger::new(
            crate::hotkey::parse_hotkey("End").expect("test hotkey parses"),
        );
        let visibility = Arc::new(AtomicBool::new(false));
        let restore_flag = Arc::new(AtomicBool::new(false));
        let ctx = Arc::new(Mutex::new(Some(NoopViewport)));
        let mut queued_visibility = None;

        *trigger.open.lock().unwrap() = true;
        assert!(crate::visibility::handle_visibility_trigger_with_owner(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
            (-10_000.0, -10_000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
            |was_visible| controller.handle_legacy_grid_toggle(was_visible),
        ));
        assert!(visibility.load(Ordering::SeqCst));
        assert!(controller.active_input_scope().is_none());

        *trigger.open.lock().unwrap() = true;
        assert!(crate::visibility::handle_visibility_trigger_with_owner(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
            (-10_000.0, -10_000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
            |was_visible| controller.handle_legacy_grid_toggle(was_visible),
        ));
        assert!(!visibility.load(Ordering::SeqCst));
        assert_eq!(controller.active_input_scope(), original_scope);
    }

    #[test]
    fn grid_keyboard_owner_persists_from_opening_through_native_ready() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let events = Arc::new(Mutex::new(VecDeque::new()));
        let made = Arc::new(Mutex::new(0));
        let mut controller = controller(events.clone(), made);
        controller.handle_intents(
            vec![InvocationIntent::ToggleDirectMenu {
                id: InvocationId(1_002),
                menu_id: MenuId::new("starter"),
                primary_key: 0x54,
                provenance: crate::radial::invocation::InputProvenance::Physical,
                trigger_still_down: false,
            }],
            false,
        );
        let pending = controller.pending.as_ref().unwrap();
        let session_id = pending.session_id.clone();
        let generation = pending.generation;

        // The grid is shown while native preparation is still pending. There
        // is no active reducer to suspend yet, so the controller must retain
        // this owner and apply it when the session becomes Ready.
        let trigger = crate::hotkey::HotkeyTrigger::new(
            crate::hotkey::parse_hotkey("End").expect("test hotkey parses"),
        );
        let visibility = Arc::new(AtomicBool::new(false));
        let restore_flag = Arc::new(AtomicBool::new(false));
        let ctx = Arc::new(Mutex::new(Some(NoopViewport)));
        let mut queued_visibility = None;
        *trigger.open.lock().unwrap() = true;
        assert!(crate::visibility::handle_visibility_trigger_with_owner(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
            (-10_000.0, -10_000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
            |was_visible| controller.handle_legacy_grid_toggle(was_visible),
        ));
        assert!(visibility.load(Ordering::SeqCst));
        assert_eq!(
            controller.grid_keyboard_owner(),
            GridKeyboardOwner::LegacyLauncher
        );
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id,
            layout_generation: generation,
        });
        controller.poll();
        assert!(controller.active_input_scope().is_none());

        // Hiding the grid returns ownership without mouse movement.
        *trigger.open.lock().unwrap() = true;
        assert!(crate::visibility::handle_visibility_trigger_with_owner(
            &trigger,
            &visibility,
            &restore_flag,
            &ctx,
            &mut queued_visibility,
            (-10_000.0, -10_000.0),
            false,
            false,
            None,
            None,
            (400.0, 220.0),
            |was_visible| controller.handle_legacy_grid_toggle(was_visible),
        ));
        assert!(!visibility.load(Ordering::SeqCst));
        assert_eq!(
            controller.grid_keyboard_owner(),
            GridKeyboardOwner::RadialMenu
        );
        assert!(controller.active_input_scope().is_some());
    }

    #[test]
    fn active_global_item_input_publishes_explicit_terminal_completion() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let made = Arc::new(Mutex::new(0));
        let mut controller = controller(events.clone(), made);
        controller.handle_intents(vec![open()], false);
        let pending = controller.pending.as_ref().unwrap();
        let session_id = pending.session_id.clone();
        let generation = pending.generation;
        events.lock().unwrap().push_back(NativeEvent::Ready {
            session_id,
            layout_generation: generation,
        });
        controller.poll();

        let events = controller.handle_intents(
            vec![InvocationIntent::ActivateItem {
                id: InvocationId(88),
                menu_id: MenuId::new("starter"),
                cell_id: CellId::new("starter-root-close"),
                gesture: ClickGesture::Primary,
                scope: TriggerScope::Global,
                source: crate::commands::ActivationSource::RadialShortcut,
                trigger_still_down: false,
            }],
            false,
        );
        assert!(events.iter().any(|event| matches!(
            event,
            ControllerEvent::InvocationCompleted {
                invocation_id: InvocationId(88)
            }
        )));
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
    fn cascade_keeps_parent_frame_and_unions_host_extent() {
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
        let composed =
            compose_layered_layout(&[(FrameId(1), &parent), (FrameId(2), &child)]).unwrap();
        assert!(composed.input_extent.min.x <= parent.input_extent.min.x);
        assert!(composed.input_extent.max.x >= parent.input_extent.max.x);
        let HitShape::Circle { center, .. } = parent.cells[0].shape else {
            panic!("starter layout must use circular cells")
        };
        assert_eq!(
            super::super::render::input_owner(&composed, center, false),
            InputOwner::NavigateToFrame(FrameId(1))
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
        augment_special_cells(&mut layout, &prepared, &menu, false);
        assert_eq!(
            super::super::render::input_owner(&layout, layout.center, false),
            InputOwner::Actionable(crate::radial::model::CellId::new("__center"))
        );
        let ordinary = layout
            .cells
            .iter()
            .find(|cell| !cell.cell_id.as_str().starts_with("__"))
            .expect("starter menu has an ordinary cell");
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
        augment_special_cells(&mut layout, &BTreeMap::new(), &menu, false);
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
            dynamic_provenance: BTreeMap::new(),
            page: 0,
            page_count: 1,
        };
        augment_special_cells(&mut layout, &frame.cells, &menu, false);
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
