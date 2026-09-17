//! Main-owned native authoring preview lease.
//!
//! This coordinator deliberately has no executor, history sink, audio session,
//! trigger registration, or invocation service. Native input is reduced only
//! for navigation; every dispatch intent is counted and discarded.

use super::{AuthoringRequestId, AuthoringSessionId, DraftGeneration, NativePreviewLease};
use crate::radial::context::{InvocationContext, WindowIdentity};
use crate::radial::diagnostics::RadialDiagnostic;
use crate::radial::geometry::{
    FrozenSpatialContext, PhysicalPoint, PhysicalRect, ScaleFactor, cascade_layout, shape_center,
    translate_layout,
};
use crate::radial::model::{
    CellContent, CellId, InvocationId, MenuId, RadialDocument, SessionId, SubmenuPresentation,
};
use crate::radial::native::{CloseReason, NativeCommand, NativeEvent, NativeHost};
pub use crate::radial::preparation::PreparedFrameInput as PreviewFrameInput;
use crate::radial::preparation::{
    PreparedPlacement, PreviewFramePreparer, PreviewPlacement, PreviewProjection,
    ensure_preview_center_back, synthetic_preview_dynamic,
};
use crate::radial::render::{build_scene_prepared_selected, build_scene_prepared_selected_tooltip};
use crate::radial::session::{CellRole, FrameId, SessionEvent, SessionIntent, SessionReducer};
use crate::radial::tooltip::{
    TooltipDeadlineScheduler, TooltipHoverState, TooltipIdentity, TooltipPreferences, monotonic_ms,
};
use crate::radial::validation::validate;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};

pub fn build_preview_frame_input(
    preparer: &mut PreviewFramePreparer,
    document: &RadialDocument,
    menu_id: &MenuId,
    anchor: PhysicalPoint,
    work_area: PhysicalRect,
    scale: ScaleFactor,
    generation: u64,
    selected: Option<&CellId>,
) -> Result<PreviewFrameInput, String> {
    let menu = document
        .menus
        .iter()
        .find(|menu| &menu.id == menu_id)
        .ok_or_else(|| format!("preview menu {menu_id} no longer exists"))?;
    let projection = PreviewProjection {
        dynamic: synthetic_preview_dynamic(menu),
        ..PreviewProjection::default()
    };
    build_preview_frame_input_projected(
        preparer,
        document,
        menu_id,
        anchor,
        work_area,
        scale,
        generation,
        selected,
        &projection,
    )
}

pub fn build_preview_frame_input_projected(
    preparer: &mut PreviewFramePreparer,
    document: &RadialDocument,
    menu_id: &MenuId,
    anchor: PhysicalPoint,
    work_area: PhysicalRect,
    scale: ScaleFactor,
    generation: u64,
    selected: Option<&CellId>,
    projection: &PreviewProjection,
) -> Result<PreviewFrameInput, String> {
    preparer.prepare(
        document, menu_id, anchor, work_area, scale, generation, selected, projection,
    )
}

trait PreviewHostPort: Send {
    fn send(&self, command: NativeCommand) -> Result<(), String>;
    fn try_recv(&self) -> Option<NativeEvent>;
    fn shutdown(&mut self);
}

impl PreviewHostPort for NativeHost {
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

type HostFactory = Arc<dyn Fn() -> Result<Box<dyn PreviewHostPort>, String> + Send + Sync>;

struct ActivePreview {
    lease: NativePreviewLease,
    native_session: SessionId,
    document: Arc<RadialDocument>,
    menu_id: MenuId,
    frame: PreviewFrameInput,
    spatial: FrozenSpatialContext,
    layout_generation: u64,
    current_frame_id: FrameId,
    effective_presentation: SubmenuPresentation,
    navigation_frames: BTreeMap<FrameId, PreviewFrameState>,
    reducer: SessionReducer,
    projection: PreviewProjection,
    tooltip_hover: TooltipHoverState,
}

#[derive(Clone)]
struct PreviewFrameState {
    menu_id: MenuId,
    frame: PreviewFrameInput,
    projection: PreviewProjection,
    effective_presentation: SubmenuPresentation,
    selected: Option<CellId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreviewLeaseResult {
    pub lease: NativePreviewLease,
    pub sampled_context: InvocationContext,
    pub diagnostics: Vec<RadialDiagnostic>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NativePreviewNotice {
    Diagnostics {
        lease: NativePreviewLease,
        diagnostics: Vec<RadialDiagnostic>,
    },
    Stopped(NativePreviewLease),
    Failed {
        lease: NativePreviewLease,
        message: String,
    },
}

pub fn sanitize_preview_context(
    mut context: InvocationContext,
    own_process_id: u32,
    excluded_hwnds: &[usize],
    last_external: Option<WindowIdentity>,
) -> InvocationContext {
    let excluded = |window: &WindowIdentity| {
        window.pid == own_process_id || excluded_hwnds.contains(&window.hwnd)
    };
    if context.foreground.as_ref().is_some_and(excluded) {
        context.foreground = None;
    }
    if context.under_pointer.as_ref().is_some_and(excluded) {
        context.under_pointer = None;
    }
    context.last_external = last_external.clone();
    if context.foreground.is_none() {
        context.foreground = last_external;
    }
    context
}

pub struct NativePreviewCoordinator {
    host: Option<Box<dyn PreviewHostPort>>,
    factory: HostFactory,
    preparer: PreviewFramePreparer,
    active: Option<ActivePreview>,
    newest: BTreeMap<AuthoringSessionId, (DraftGeneration, AuthoringRequestId)>,
    last_external: Option<WindowIdentity>,
    tooltip_wake: Option<mpsc::Sender<()>>,
    tooltip_scheduler: Option<TooltipDeadlineScheduler>,
    tooltip_preferences: TooltipPreferences,
    pub intercepted_dispatches: usize,
}

impl NativePreviewCoordinator {
    pub fn new(wake: mpsc::Sender<()>, application_data: PathBuf) -> Self {
        let host_wake = wake.clone();
        let factory = Arc::new(move || {
            NativeHost::spawn_with_wake(Some(host_wake.clone()))
                .map(|host| Box::new(host) as Box<dyn PreviewHostPort>)
        });
        let mut coordinator =
            Self::with_factory_and_preparer(factory, PreviewFramePreparer::new(application_data));
        coordinator.tooltip_wake = Some(wake);
        coordinator
    }

    fn with_factory(factory: HostFactory) -> Self {
        Self::with_factory_and_preparer(factory, PreviewFramePreparer::new(PathBuf::new()))
    }

    fn with_factory_and_preparer(factory: HostFactory, preparer: PreviewFramePreparer) -> Self {
        Self {
            host: None,
            factory,
            preparer,
            active: None,
            newest: BTreeMap::new(),
            last_external: None,
            tooltip_wake: None,
            tooltip_scheduler: None,
            tooltip_preferences: TooltipPreferences::default(),
            intercepted_dispatches: 0,
        }
    }

    pub fn active_lease(&self) -> Option<NativePreviewLease> {
        self.active.as_ref().map(|active| active.lease.clone())
    }

    pub fn start(
        &mut self,
        editor_session: AuthoringSessionId,
        generation: DraftGeneration,
        request_id: AuthoringRequestId,
        document: Arc<RadialDocument>,
        menu_id: MenuId,
        sample_external_context: bool,
    ) -> Result<PreviewLeaseResult, String> {
        let menu = document
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .ok_or_else(|| "preview menu no longer exists".to_string())?;
        let projection = PreviewProjection {
            dynamic: synthetic_preview_dynamic(menu),
            ..PreviewProjection::default()
        };
        self.start_with_projection(
            editor_session,
            generation,
            request_id,
            document,
            menu_id,
            sample_external_context,
            projection,
        )
    }

    pub fn start_with_projection(
        &mut self,
        editor_session: AuthoringSessionId,
        generation: DraftGeneration,
        request_id: AuthoringRequestId,
        document: Arc<RadialDocument>,
        menu_id: MenuId,
        sample_external_context: bool,
        projection: PreviewProjection,
    ) -> Result<PreviewLeaseResult, String> {
        self.start_with_projection_context(
            editor_session,
            generation,
            request_id,
            document,
            menu_id,
            sample_external_context,
            projection,
            None,
        )
    }

    fn start_with_projection_context(
        &mut self,
        editor_session: AuthoringSessionId,
        generation: DraftGeneration,
        request_id: AuthoringRequestId,
        document: Arc<RadialDocument>,
        menu_id: MenuId,
        sample_external_context: bool,
        mut projection: PreviewProjection,
        frozen_context: Option<FrozenSpatialContext>,
    ) -> Result<PreviewLeaseResult, String> {
        projection.tooltip_preferences = self.tooltip_preferences;
        let sequence = (generation, request_id);
        if self
            .newest
            .get(&editor_session)
            .is_some_and(|newest| sequence <= *newest)
        {
            return Err("stale native preview request".into());
        }
        validate(&document).map_err(|error| error.to_string())?;
        let (anchor, work, scale) = frozen_context.map_or_else(
            || super::super::controller::desktop_geometry(),
            |context| {
                projection.placement = PreviewPlacement::FixedCenter;
                (
                    context.visible_center,
                    context.work_area,
                    context.scale_factor,
                )
            },
        );
        let frame = build_preview_frame_input_projected(
            &mut self.preparer,
            &document,
            &menu_id,
            anchor,
            work,
            scale,
            generation.0,
            None,
            &projection,
        )?;
        self.stop_active(CloseReason::SettingsReload);
        self.newest.insert(editor_session, sequence);
        if self.host.is_none() {
            self.host = Some((self.factory)()?);
        }
        let lease = NativePreviewLease {
            editor_session,
            generation,
            request_id,
        };
        let native_session = SessionId::new(format!(
            "authoring-preview-{}-{}-{}",
            editor_session.0, generation.0, request_id.0
        ));
        let sampled_context = if sample_external_context {
            let captured = InvocationContext::capture_current(
                request_id.0,
                self.last_external.clone(),
                std::process::id(),
            );
            let context = sanitize_preview_context(
                captured,
                std::process::id(),
                &[],
                self.last_external.clone(),
            );
            if let Some(window) = context.foreground.clone() {
                self.last_external = Some(window);
            }
            context
        } else {
            InvocationContext::empty(request_id.0)
        };
        let mut reducer = SessionReducer::new(
            native_session.clone(),
            document.revision,
            menu_id.clone(),
            anchor,
            generation.0,
            document
                .menus
                .iter()
                .find(|menu| menu.id == menu_id)
                .map_or(crate::radial::model::InteractionMode::StickyClick, |menu| {
                    menu.interaction
                }),
            InvocationId(request_id.0),
            frame.layout.center,
        );
        if let Some(reducer_frame) = reducer.state.stack.last_mut() {
            reducer_frame.origin = frame.layout.origin;
            reducer_frame.page = frame.page;
            reducer_frame.page_count = frame.page_count;
            reducer_frame.scale_factor = scale.get();
        }
        let mut spatial = frozen_context.unwrap_or(FrozenSpatialContext {
            requested_root_anchor: anchor,
            visible_center: frame.layout.origin,
            work_area: work,
            scale_factor: scale,
            spatial_generation: 0,
            topology_generation: 1,
        });
        // Layout geometry is desktop-logical, so converting the supplied
        // physical center through its f32 representation can shift it by a
        // fraction of a physical pixel. Navigation follows the root's actual
        // post-fit physical center, just like runtime sessions.
        spatial.visible_center = frame.layout.origin;
        let mut root_projection = projection.clone();
        root_projection.placement = PreviewPlacement::FixedCenter;
        let root_frame = PreviewFrameState {
            menu_id: menu_id.clone(),
            frame: frame.clone(),
            projection: root_projection,
            effective_presentation: SubmenuPresentation::SameCenter,
            selected: None,
        };
        let open = NativeCommand::Open {
            session_id: native_session.clone(),
            scene: frame.scene.clone(),
            layout: frame.layout.clone(),
            always_on_top: true,
            activate_on_show: false,
        };
        let diagnostics = frame.diagnostics.clone();
        self.active = Some(ActivePreview {
            lease: lease.clone(),
            native_session,
            document,
            menu_id,
            frame,
            spatial,
            layout_generation: generation.0,
            current_frame_id: FrameId(1),
            effective_presentation: SubmenuPresentation::SameCenter,
            navigation_frames: BTreeMap::from([(FrameId(1), root_frame)]),
            reducer,
            projection,
            tooltip_hover: TooltipHoverState::default(),
        });
        self.send_checked(open)?;
        Ok(PreviewLeaseResult {
            lease,
            sampled_context,
            diagnostics,
        })
    }

    pub fn prepare_frame(
        &mut self,
        document: &RadialDocument,
        menu_id: &MenuId,
        anchor: PhysicalPoint,
        work_area: PhysicalRect,
        scale: ScaleFactor,
        generation: u64,
        selected: Option<&CellId>,
        projection: &PreviewProjection,
    ) -> Result<PreviewFrameInput, String> {
        build_preview_frame_input_projected(
            &mut self.preparer,
            document,
            menu_id,
            anchor,
            work_area,
            scale,
            generation,
            selected,
            projection,
        )
    }

    pub fn update(
        &mut self,
        previous: &NativePreviewLease,
        document: Arc<RadialDocument>,
        menu_id: MenuId,
        generation: DraftGeneration,
        request_id: AuthoringRequestId,
        sample_external_context: bool,
    ) -> Result<PreviewLeaseResult, String> {
        let menu = document
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .ok_or_else(|| "preview menu no longer exists".to_string())?;
        let projection = PreviewProjection {
            dynamic: synthetic_preview_dynamic(menu),
            ..PreviewProjection::default()
        };
        self.update_with_projection(
            previous,
            document,
            menu_id,
            generation,
            request_id,
            sample_external_context,
            projection,
        )
    }

    pub fn update_with_projection(
        &mut self,
        previous: &NativePreviewLease,
        document: Arc<RadialDocument>,
        menu_id: MenuId,
        generation: DraftGeneration,
        request_id: AuthoringRequestId,
        sample_external_context: bool,
        projection: PreviewProjection,
    ) -> Result<PreviewLeaseResult, String> {
        if !self.active.as_ref().is_some_and(|active| {
            active.lease == *previous && previous.editor_session == active.lease.editor_session
        }) {
            return Err("native preview lease is no longer active".into());
        }
        let frozen_context = self.active.as_ref().map(|active| active.spatial);
        self.start_with_projection_context(
            previous.editor_session,
            generation,
            request_id,
            document,
            menu_id,
            sample_external_context,
            projection,
            frozen_context,
        )
    }

    pub fn stop(
        &mut self,
        editor_session: AuthoringSessionId,
        generation: DraftGeneration,
        request_id: AuthoringRequestId,
    ) {
        let sequence = (generation, request_id);
        if self
            .newest
            .get(&editor_session)
            .is_some_and(|newest| sequence < *newest)
        {
            return;
        }
        self.newest.insert(editor_session, sequence);
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.lease.editor_session == editor_session)
        {
            self.stop_active(CloseReason::Dismissed);
        }
    }

    pub fn cancel_all(&mut self) {
        self.stop_active(CloseReason::SettingsReload);
        let _ = self.sync_tooltip_deadline();
        if let Some(mut host) = self.host.take() {
            host.shutdown();
        }
        self.preparer.release();
    }

    pub fn poll(&mut self) -> Vec<NativePreviewNotice> {
        let mut notices = Vec::new();
        loop {
            let Some(event) = self.host.as_ref().and_then(|host| host.try_recv()) else {
                break;
            };
            let lease = self.active.as_ref().map(|active| active.lease.clone());
            let result = self.handle_event(event);
            if let Some(lease) = lease {
                let ended = !self
                    .active
                    .as_ref()
                    .is_some_and(|active| active.lease == lease);
                match result {
                    Err(message) => notices.push(NativePreviewNotice::Failed { lease, message }),
                    Ok(_) if ended => notices.push(NativePreviewNotice::Stopped(lease)),
                    Ok(Some(diagnostics)) => {
                        notices.push(NativePreviewNotice::Diagnostics { lease, diagnostics })
                    }
                    Ok(None) => {}
                }
            }
        }
        let due = self
            .active
            .as_ref()
            .and_then(|active| active.tooltip_hover.candidate())
            .filter(|(_, deadline)| *deadline <= monotonic_ms())
            .map(|(identity, _)| identity.clone());
        if let Some(identity) = due {
            let lease = self.active.as_ref().map(|active| active.lease.clone());
            let revealed = self
                .active
                .as_mut()
                .is_some_and(|active| active.tooltip_hover.expire(&identity, monotonic_ms()));
            if revealed {
                let _ = self.sync_tooltip_deadline();
                if let Some(lease) = lease {
                    match self.present_active() {
                        Ok(Some(diagnostics)) => {
                            notices.push(NativePreviewNotice::Diagnostics { lease, diagnostics })
                        }
                        Ok(None) => {}
                        Err(message) => {
                            notices.push(NativePreviewNotice::Failed { lease, message })
                        }
                    }
                }
            }
        }
        notices
    }

    /// Applies global tooltip preferences to the active authoring preview.
    /// The prepared resources are refreshed, but the wheel's frozen layout
    /// remains byte-for-byte stable so settings changes cannot move input.
    pub fn set_tooltip_preferences(
        &mut self,
        preferences: TooltipPreferences,
    ) -> Result<Option<Vec<RadialDiagnostic>>, String> {
        if self.tooltip_preferences == preferences {
            return Ok(None);
        }
        self.tooltip_preferences = preferences;
        let Some((document, menu_id, mut projection, layout, selected, work_area, scale, frame_id)) =
            self.active.as_mut().map(|active| {
                active.tooltip_hover.cancel();
                (
                    Arc::clone(&active.document),
                    active.menu_id.clone(),
                    active.projection.clone(),
                    active.frame.layout.clone(),
                    active
                        .reducer
                        .state
                        .hovered
                        .clone()
                        .or_else(|| active.reducer.state.selected.clone()),
                    active.spatial.work_area,
                    active.spatial.scale_factor,
                    active.current_frame_id,
                )
            })
        else {
            self.sync_tooltip_deadline()?;
            return Ok(None);
        };
        let previous_diagnostics = self
            .active
            .as_ref()
            .map(|active| active.frame.diagnostics.clone());
        projection.tooltip_preferences = preferences;
        projection.page = self
            .active
            .as_ref()
            .map_or(projection.page, |active| active.frame.page);
        projection.placement = PreviewPlacement::FixedCenter;
        let mut frame = build_preview_frame_input_projected(
            &mut self.preparer,
            &document,
            &menu_id,
            layout.origin,
            work_area,
            scale,
            self.active
                .as_ref()
                .map_or(0, |active| active.layout_generation),
            selected.as_ref(),
            &projection,
        )?;
        frame.layout = layout;
        if let Some(parent_resources) = self
            .active
            .as_ref()
            .and_then(|active| active.reducer.state.stack.last())
            .and_then(|current| current.parent_frame_id)
            .and_then(|parent| {
                self.active
                    .as_ref()?
                    .navigation_frames
                    .get(&parent)
                    .map(|frame| frame.frame.resources.clone())
            })
        {
            frame.resources = merge_preview_resources(&parent_resources, &frame.resources);
        }
        let current_generation = self
            .active
            .as_ref()
            .map_or(0, |active| active.layout_generation);
        frame.scene = build_scene_prepared_selected(
            &frame.layout,
            current_generation,
            &frame.resources,
            selected.as_ref(),
        );
        let active = self.active.as_mut().ok_or("preview lease ended")?;
        active.frame = frame;
        active.projection = projection;
        if let Some(frame_state) = active.navigation_frames.get_mut(&frame_id) {
            frame_state.frame = active.frame.clone();
            frame_state.projection = active.projection.clone();
        }
        self.sync_tooltip_deadline()?;
        self.present_active()?;
        let current_diagnostics = self
            .active
            .as_ref()
            .map(|active| active.frame.diagnostics.clone());
        Ok(previous_diagnostics
            .zip(current_diagnostics)
            .and_then(|(previous, current)| (previous != current).then_some(current)))
    }

    fn sync_tooltip_deadline(&mut self) -> Result<(), String> {
        let deadline = self
            .active
            .as_ref()
            .and_then(|active| active.tooltip_hover.candidate())
            .map(|(_, deadline)| deadline);
        if let Some(deadline) = deadline {
            if self.tooltip_scheduler.is_none()
                && let Some(wake) = self.tooltip_wake.clone()
            {
                self.tooltip_scheduler = Some(TooltipDeadlineScheduler::spawn(wake)?);
            }
            if let Some(scheduler) = &self.tooltip_scheduler {
                scheduler.arm(deadline);
            }
        } else if let Some(scheduler) = &self.tooltip_scheduler {
            scheduler.cancel();
        }
        Ok(())
    }

    fn handle_event(
        &mut self,
        event: NativeEvent,
    ) -> Result<Option<Vec<RadialDiagnostic>>, String> {
        if matches!(event, NativeEvent::Stopped) {
            if let Some(active) = self.active.as_mut() {
                active.tooltip_hover.cancel();
            }
            self.active = None;
            if let Some(scheduler) = &self.tooltip_scheduler {
                scheduler.cancel();
            }
            self.host.take();
            self.preparer.release();
            return Ok(None);
        }
        if let NativeEvent::Failed {
            ref session_id,
            ref message,
        } = event
        {
            if session_id.as_ref().is_some_and(|session| {
                self.active
                    .as_ref()
                    .is_some_and(|active| session != &active.native_session)
            }) {
                return Ok(None);
            }
            let message = message.clone();
            self.stop_active(CloseReason::HostFailure);
            if let Some(mut host) = self.host.take() {
                host.shutdown();
            }
            self.preparer.release();
            return Err(message);
        }
        let Some(active) = self.active.as_ref() else {
            return Ok(None);
        };
        let event_session = match &event {
            NativeEvent::Ready { session_id, .. }
            | NativeEvent::PointerDown { session_id, .. }
            | NativeEvent::PointerMoved { session_id, .. }
            | NativeEvent::PointerLeft { session_id, .. }
            | NativeEvent::PointerUp { session_id, .. }
            | NativeEvent::CaptureLost { session_id, .. }
            | NativeEvent::Escape { session_id }
            | NativeEvent::Navigate { session_id, .. }
            | NativeEvent::Relocated { session_id, .. }
            | NativeEvent::DisplayChanged { session_id }
            | NativeEvent::Closed { session_id, .. } => Some(session_id),
            NativeEvent::Failed { session_id, .. } => session_id.as_ref(),
            NativeEvent::Stopped => None,
        };
        if event_session.is_some_and(|session| session != &active.native_session) {
            return Ok(None);
        }
        let generation = active.layout_generation;
        let event_generation = match &event {
            NativeEvent::Ready {
                layout_generation, ..
            }
            | NativeEvent::PointerDown {
                layout_generation, ..
            }
            | NativeEvent::PointerMoved {
                layout_generation, ..
            }
            | NativeEvent::PointerLeft {
                layout_generation, ..
            }
            | NativeEvent::PointerUp {
                layout_generation, ..
            }
            | NativeEvent::CaptureLost {
                layout_generation, ..
            }
            | NativeEvent::Relocated {
                layout_generation, ..
            } => Some(*layout_generation),
            _ => None,
        };
        if event_generation.is_some_and(|event_generation| event_generation != generation) {
            return Ok(None);
        }
        if let NativeEvent::Relocated { from, to, .. } = &event {
            return self.relocate_active(*from, *to);
        }
        if matches!(
            &event,
            NativeEvent::Escape { .. }
                | NativeEvent::CaptureLost { .. }
                | NativeEvent::DisplayChanged { .. }
                | NativeEvent::Closed { .. }
        ) {
            match event {
                NativeEvent::Escape { .. } | NativeEvent::CaptureLost { .. } => {
                    self.stop_active(CloseReason::Dismissed);
                }
                NativeEvent::DisplayChanged { .. } => {
                    self.stop_active(CloseReason::DisplayRelayout);
                }
                NativeEvent::Closed { .. } => {
                    self.active = None;
                }
                _ => unreachable!(),
            }
            self.sync_tooltip_deadline()?;
            return Ok(None);
        }
        let (native_session, reducer_before, intents, visual_changed) = {
            let Some(active) = self.active.as_mut() else {
                return Ok(None);
            };
            let reducer_before = active.reducer.clone();
            let previous_hover = active.reducer.state.hovered.clone();
            let previous_visible = active.tooltip_hover.visible().cloned();
            let (intents, visual_changed) = match event {
                NativeEvent::PointerMoved { point, owner, .. } => {
                    let cell = active
                        .frame
                        .layout
                        .hit_test(point)
                        .map(|cell| cell.cell_id.clone());
                    let tooltip_cell =
                        (!matches!(&owner, crate::radial::render::InputOwner::Exterior))
                            .then(|| active.frame.layout.geometric_hover_cell(point))
                            .flatten()
                            .map(|cell| cell.cell_id.clone());
                    let intents = active.reducer.reduce(SessionEvent::PointerMoved {
                        point,
                        hovered: cell.clone(),
                        geometry_generation: generation,
                    });
                    let tooltip_identity = tooltip_cell
                        .as_ref()
                        .filter(|cell| active.frame.resources.tooltips.contains_key(*cell))
                        .map(|cell| TooltipIdentity {
                            session_id: active.native_session.clone(),
                            frame_id: active.current_frame_id,
                            layout_generation: generation,
                            cell_id: cell.clone(),
                        });
                    active.tooltip_hover.observe(
                        tooltip_identity,
                        true,
                        monotonic_ms(),
                        active.projection.tooltip_preferences.delay_ms,
                    );
                    if intents
                        .iter()
                        .any(|intent| matches!(intent, SessionIntent::BeginNativeDrag { .. }))
                    {
                        active.tooltip_hover.cancel();
                    }
                    (
                        intents,
                        previous_hover != active.reducer.state.hovered
                            || previous_visible != active.tooltip_hover.visible().cloned(),
                    )
                }
                NativeEvent::PointerLeft { .. } => {
                    let intents = active.reducer.reduce(SessionEvent::OutsideInteraction);
                    active.tooltip_hover.cancel();
                    (
                        intents,
                        previous_hover != active.reducer.state.hovered
                            || previous_visible.is_some(),
                    )
                }
                NativeEvent::PointerDown { point, button, .. } => {
                    active.tooltip_hover.cancel();
                    let cell = active
                        .frame
                        .layout
                        .hit_test(point)
                        .map(|cell| cell.cell_id.clone());
                    let role = cell.as_ref().map_or(CellRole::Unavailable, |cell| {
                        active_cell_role(active, cell, Some(button))
                    });
                    let intents = active.reducer.reduce(SessionEvent::PointerDown {
                        point,
                        cell,
                        role,
                        button,
                        geometry_generation: generation,
                    });
                    (intents, previous_visible.is_some())
                }
                NativeEvent::PointerUp { point, button, .. } => {
                    active.tooltip_hover.cancel();
                    let cell = active
                        .frame
                        .layout
                        .hit_test(point)
                        .map(|cell| cell.cell_id.clone());
                    let role = cell.as_ref().map_or(CellRole::Unavailable, |cell| {
                        active_cell_role(active, cell, Some(button))
                    });
                    let intents = active.reducer.reduce(SessionEvent::PointerUp {
                        point,
                        cell,
                        role,
                        button,
                        geometry_generation: generation,
                    });
                    (intents, previous_visible.is_some())
                }
                NativeEvent::Navigate {
                    command, modifiers, ..
                } => {
                    active.tooltip_hover.cancel();
                    active
                        .reducer
                        .reduce(SessionEvent::ModifiersChanged(modifiers));
                    let cells = active
                        .frame
                        .layout
                        .cells
                        .iter()
                        .filter(|cell| cell.actionable)
                        .map(|cell| {
                            (
                                cell.cell_id.clone(),
                                active_cell_role(active, &cell.cell_id, None),
                            )
                        })
                        .collect();
                    let intents = active.reducer.reduce(SessionEvent::Navigate {
                        command,
                        cells,
                        pointer_baseline: active.frame.layout.center,
                        geometry_generation: generation,
                    });
                    (
                        intents,
                        previous_hover.is_some() || previous_visible.is_some(),
                    )
                }
                NativeEvent::Escape { .. }
                | NativeEvent::CaptureLost { .. }
                | NativeEvent::DisplayChanged { .. }
                | NativeEvent::Closed { .. } => unreachable!("handled before reducer borrow"),
                NativeEvent::Ready { .. }
                | NativeEvent::Relocated { .. }
                | NativeEvent::Failed { .. }
                | NativeEvent::Stopped => (Vec::new(), false),
            };
            (
                active.native_session.clone(),
                reducer_before,
                intents,
                visual_changed,
            )
        };
        self.sync_tooltip_deadline()?;
        let result = self.apply_intents(intents, visual_changed);
        if result.is_err()
            && let Some(active) = self
                .active
                .as_mut()
                .filter(|active| active.native_session == native_session)
        {
            active.reducer = reducer_before;
        }
        result
    }

    fn apply_intents(
        &mut self,
        intents: Vec<SessionIntent>,
        present_after: bool,
    ) -> Result<Option<Vec<RadialDiagnostic>>, String> {
        let navigation_changed = intents.iter().any(|intent| {
            matches!(
                intent,
                SessionIntent::OpenSubmenu { .. }
                    | SessionIntent::Back
                    | SessionIntent::PageChanged { .. }
                    | SessionIntent::CloseTree
            )
        });
        let previous_diagnostics = self
            .active
            .as_ref()
            .map(|active| active.frame.diagnostics.clone());
        for intent in intents {
            match intent {
                SessionIntent::Dispatch { .. } => self.intercepted_dispatches += 1,
                SessionIntent::OpenSubmenu { cell_id } => self.open_submenu(&cell_id)?,
                SessionIntent::BeginNativeDrag {
                    geometry_generation,
                    ..
                } => {
                    let command = {
                        let active = self.active.as_ref().ok_or("preview lease ended")?;
                        if active.layout_generation != geometry_generation {
                            continue;
                        }
                        NativeCommand::BeginSystemDrag {
                            session_id: active.native_session.clone(),
                            layout_generation: geometry_generation,
                        }
                    };
                    self.send_checked(command)?;
                }
                SessionIntent::Back => self.restore_back()?,
                SessionIntent::PageChanged { page } => self.reflow_page(page)?,
                SessionIntent::CloseTree => self.stop_active(CloseReason::Dismissed),
            }
        }
        self.sync_tooltip_deadline()?;
        if present_after || navigation_changed {
            self.present_active_with_previous(previous_diagnostics)
        } else {
            Ok(None)
        }
    }

    fn present_active(&mut self) -> Result<Option<Vec<RadialDiagnostic>>, String> {
        let previous_diagnostics = self
            .active
            .as_ref()
            .map(|active| active.frame.diagnostics.clone());
        self.present_active_with_previous(previous_diagnostics)
    }

    fn present_active_with_previous(
        &mut self,
        previous_diagnostics: Option<Vec<RadialDiagnostic>>,
    ) -> Result<Option<Vec<RadialDiagnostic>>, String> {
        let (command, diagnostics) = {
            let Some(active) = self.active.as_mut() else {
                return Ok(None);
            };
            let selected = active
                .reducer
                .state
                .hovered
                .as_ref()
                .or(active.reducer.state.selected.as_ref())
                .cloned();
            let visible_tooltip = active
                .tooltip_hover
                .visible()
                .filter(|identity| {
                    identity.session_id == active.native_session
                        && identity.frame_id == active.current_frame_id
                        && identity.layout_generation == active.layout_generation
                })
                .map(|identity| &identity.cell_id);
            active.frame.scene = build_scene_prepared_selected_tooltip(
                &active.frame.layout,
                active.layout_generation,
                &active.frame.resources,
                selected.as_ref(),
                visible_tooltip,
                active.frame.work_area,
            );
            if let Some(reducer_frame) = active.reducer.state.stack.last_mut() {
                reducer_frame.page = active.frame.page;
                reducer_frame.page_count = active.frame.page_count;
            }
            if let Some(frame_state) = active.navigation_frames.get_mut(&active.current_frame_id) {
                frame_state.frame = active.frame.clone();
                frame_state.projection = active.projection.clone();
                frame_state.selected = selected;
            }
            let command = NativeCommand::Present {
                session_id: active.native_session.clone(),
                scene: active.frame.scene.clone(),
                layout: active.frame.layout.clone(),
                always_on_top: true,
                activate_on_show: false,
            };
            (command, active.frame.diagnostics.clone())
        };
        self.send_checked(command)?;
        Ok(previous_diagnostics
            .is_some_and(|previous| diagnostics != previous)
            .then_some(diagnostics))
    }

    fn open_submenu(&mut self, cell_id: &CellId) -> Result<(), String> {
        if let Some(active) = self.active.as_mut() {
            active.tooltip_hover.cancel();
        }
        self.sync_tooltip_deadline()?;
        let (
            document,
            parent_menu_id,
            parent_definition,
            parent_layout,
            parent_frame_id,
            spatial,
            mut projection,
            next_generation,
            native_session,
        ) = {
            let active = self.active.as_ref().ok_or("preview lease ended")?;
            let parent_definition = active
                .document
                .menus
                .iter()
                .find(|menu| menu.id == active.menu_id)
                .cloned()
                .ok_or("preview parent menu is unavailable")?;
            (
                Arc::clone(&active.document),
                active.menu_id.clone(),
                parent_definition,
                active.frame.layout.clone(),
                active.current_frame_id,
                active.spatial,
                active.projection.clone(),
                active
                    .layout_generation
                    .checked_add(1)
                    .ok_or("preview geometry generation exhausted")?,
                active.native_session.clone(),
            )
        };
        let target = submenu_target(&document, &parent_menu_id, cell_id)
            .ok_or("preview submenu target is unavailable")?;
        let child_menu = document
            .menus
            .iter()
            .find(|menu| menu.id == target)
            .cloned()
            .ok_or("preview submenu definition is unavailable")?;

        let (anchor, mut effective_presentation) = match parent_definition.submenu_presentation {
            SubmenuPresentation::SameCenter => {
                projection.placement = PreviewPlacement::FixedCenter;
                (parent_layout.origin, SubmenuPresentation::SameCenter)
            }
            SubmenuPresentation::Cascade => {
                let parent_cell = parent_layout
                    .cells
                    .iter()
                    .find(|cell| &cell.cell_id == cell_id)
                    .ok_or("preview submenu cell geometry is unavailable")?;
                let anchor = shape_center(&parent_cell.shape, spatial.scale_factor);
                projection.placement = PreviewPlacement::Cascade {
                    fallback_center: parent_layout.origin,
                };
                (anchor, SubmenuPresentation::Cascade)
            }
        };
        projection.page = 0;
        projection.dynamic = synthetic_preview_dynamic(&child_menu);
        let mut frame = build_preview_frame_input_projected(
            &mut self.preparer,
            &document,
            &target,
            anchor,
            spatial.work_area,
            spatial.scale_factor,
            next_generation,
            None,
            &projection,
        )?;
        ensure_preview_center_back(&mut frame.layout, &child_menu);
        if effective_presentation == SubmenuPresentation::Cascade {
            if frame.placement == PreparedPlacement::Cascade {
                let parent = self
                    .active
                    .as_ref()
                    .and_then(|active| active.navigation_frames.get(&parent_frame_id))
                    .ok_or("preview parent frame was not retained")?;
                frame.resources =
                    merge_preview_resources(&parent.frame.resources, &frame.resources);
                frame.layout = cascade_layout(&parent_layout, frame.layout);
            } else {
                effective_presentation = SubmenuPresentation::SameCenter;
            }
        }
        projection.placement = PreviewPlacement::FixedCenter;
        frame.scene =
            build_scene_prepared_selected(&frame.layout, next_generation, &frame.resources, None);

        let active = self.active.as_mut().ok_or("preview lease ended")?;
        if active.native_session != native_session || active.current_frame_id != parent_frame_id {
            return Err("preview parent frame changed while opening submenu".into());
        }
        let intents = active.reducer.reduce(SessionEvent::OpenChild {
            menu_id: target.clone(),
            origin: frame.layout.origin,
            geometry_generation: next_generation,
            pointer_baseline: frame.layout.center,
        });
        if !intents.is_empty() {
            return Err("preview session rejected submenu navigation".into());
        }
        let Some(child_frame_id) = active
            .reducer
            .state
            .stack
            .last()
            .map(|frame| frame.frame_id)
        else {
            return Err("preview submenu frame was not created".into());
        };
        if let Some(parent_frame) = active
            .reducer
            .state
            .stack
            .iter()
            .find(|frame| frame.frame_id == parent_frame_id)
            && let Some(parent_state) = active.navigation_frames.get_mut(&parent_frame_id)
        {
            parent_state.selected = parent_frame.selected.clone();
        }
        if let Some(reducer_frame) = active.reducer.state.stack.last_mut() {
            reducer_frame.page = frame.page;
            reducer_frame.page_count = frame.page_count;
            reducer_frame.scale_factor = spatial.scale_factor.get();
            reducer_frame.spatial_generation = spatial.spatial_generation;
        }
        active.navigation_frames.insert(
            child_frame_id,
            PreviewFrameState {
                menu_id: target.clone(),
                frame: frame.clone(),
                projection: projection.clone(),
                effective_presentation,
                selected: None,
            },
        );
        active.current_frame_id = child_frame_id;
        active.menu_id = target;
        active.frame = frame;
        active.projection = projection;
        active.effective_presentation = effective_presentation;
        active.layout_generation = next_generation;
        Ok(())
    }

    fn restore_back(&mut self) -> Result<(), String> {
        if let Some(active) = self.active.as_mut() {
            active.tooltip_hover.cancel();
        }
        self.sync_tooltip_deadline()?;
        let (frame_id, mut restored, next_generation, native_session) = {
            let active = self.active.as_ref().ok_or("preview lease ended")?;
            let reducer_frame = active
                .reducer
                .state
                .stack
                .last()
                .ok_or("preview root frame is unavailable")?;
            let state = active
                .navigation_frames
                .get(&reducer_frame.frame_id)
                .cloned()
                .ok_or("preview frame restoration data is unavailable")?;
            (
                reducer_frame.frame_id,
                state,
                active
                    .layout_generation
                    .checked_add(1)
                    .ok_or("preview geometry generation exhausted")?,
                active.native_session.clone(),
            )
        };
        restored.selected = self
            .active
            .as_ref()
            .and_then(|active| active.reducer.state.stack.last())
            .and_then(|frame| frame.selected.clone())
            .or(restored.selected.clone());
        restored.frame.scene = build_scene_prepared_selected(
            &restored.frame.layout,
            next_generation,
            &restored.frame.resources,
            restored.selected.as_ref(),
        );
        let active = self.active.as_mut().ok_or("preview lease ended")?;
        if active.native_session != native_session {
            return Err("preview session changed during Back".into());
        }
        let mut reducer = active.reducer.clone();
        reducer.reduce(SessionEvent::DisplayRelayout {
            geometry_generation: next_generation,
            pointer_baseline: restored.frame.layout.center,
        });
        if reducer
            .state
            .stack
            .last()
            .is_none_or(|frame| frame.geometry_generation != next_generation)
        {
            return Err("preview reducer rejected restored frame geometry".into());
        }
        active.reducer = reducer;
        active.navigation_frames.insert(frame_id, restored.clone());
        active.current_frame_id = frame_id;
        active.menu_id = restored.menu_id.clone();
        active.frame = restored.frame.clone();
        active.projection = restored.projection.clone();
        active.effective_presentation = restored.effective_presentation;
        active.layout_generation = next_generation;
        Ok(())
    }

    fn reflow_page(&mut self, _page: usize) -> Result<(), String> {
        if let Some(active) = self.active.as_mut() {
            active.tooltip_hover.cancel();
        }
        self.sync_tooltip_deadline()?;
        let (
            document,
            frame_id,
            reducer_frame,
            frame_state,
            spatial,
            next_generation,
            native_session,
        ) = {
            let active = self.active.as_ref().ok_or("preview lease ended")?;
            let reducer_frame = active
                .reducer
                .state
                .stack
                .last()
                .cloned()
                .ok_or("preview frame is unavailable")?;
            let state = active
                .navigation_frames
                .get(&reducer_frame.frame_id)
                .cloned()
                .ok_or("preview frame cache is unavailable")?;
            (
                Arc::clone(&active.document),
                reducer_frame.frame_id,
                reducer_frame,
                state,
                active.spatial,
                active
                    .layout_generation
                    .checked_add(1)
                    .ok_or("preview geometry generation exhausted")?,
                active.native_session.clone(),
            )
        };
        let mut projection = frame_state.projection.clone();
        projection.page = reducer_frame.page;
        projection.placement = PreviewPlacement::FixedCenter;
        let selected = reducer_frame.selected.clone();
        let mut frame = build_preview_frame_input_projected(
            &mut self.preparer,
            &document,
            &frame_state.menu_id,
            frame_state.frame.layout.origin,
            spatial.work_area,
            spatial.scale_factor,
            next_generation,
            selected.as_ref(),
            &projection,
        )?;
        let current_menu = document
            .menus
            .iter()
            .find(|menu| menu.id == frame_state.menu_id)
            .ok_or("preview frame menu is unavailable")?;
        ensure_preview_center_back(&mut frame.layout, current_menu);
        if frame_state.effective_presentation == SubmenuPresentation::Cascade
            && let Some(parent_frame_id) = reducer_frame.parent_frame_id
        {
            let parent = self
                .active
                .as_ref()
                .and_then(|active| active.navigation_frames.get(&parent_frame_id))
                .ok_or("preview Cascade parent frame is unavailable")?;
            frame.resources = merge_preview_resources(&parent.frame.resources, &frame.resources);
            frame.layout = cascade_layout(&parent.frame.layout, frame.layout);
        }
        frame.scene = build_scene_prepared_selected(
            &frame.layout,
            next_generation,
            &frame.resources,
            selected.as_ref(),
        );
        let mut reducer = self
            .active
            .as_ref()
            .filter(|active| active.native_session == native_session)
            .ok_or("preview session changed during page reflow")?
            .reducer
            .clone();
        reducer.reduce(SessionEvent::DisplayRelayout {
            geometry_generation: next_generation,
            pointer_baseline: frame.layout.center,
        });
        if reducer
            .state
            .stack
            .last()
            .is_none_or(|current| current.geometry_generation != next_generation)
        {
            return Err("preview reducer rejected page geometry".into());
        }
        if let Some(current) = reducer.state.stack.last_mut() {
            current.page = frame.page;
            current.page_count = frame.page_count;
        }
        let active = self.active.as_mut().ok_or("preview lease ended")?;
        active.reducer = reducer;
        active.navigation_frames.insert(
            frame_id,
            PreviewFrameState {
                menu_id: frame_state.menu_id.clone(),
                frame: frame.clone(),
                projection: projection.clone(),
                effective_presentation: frame_state.effective_presentation,
                selected,
            },
        );
        active.current_frame_id = frame_id;
        active.menu_id = frame_state.menu_id;
        active.frame = frame;
        active.projection = projection;
        active.effective_presentation = frame_state.effective_presentation;
        active.layout_generation = next_generation;
        Ok(())
    }

    fn relocate_active(
        &mut self,
        from: PhysicalPoint,
        to: PhysicalPoint,
    ) -> Result<Option<Vec<RadialDiagnostic>>, String> {
        if let Some(active) = self.active.as_mut() {
            active.tooltip_hover.cancel();
        }
        self.sync_tooltip_deadline()?;
        let (
            mut frames,
            current_frame_id,
            old_spatial_generation,
            next_spatial_generation,
            next_generation,
            native_session,
            delta,
            mut reducer,
            pointer_baseline,
        ) = {
            let active = self.active.as_ref().ok_or("preview lease ended")?;
            let delta = PhysicalPoint {
                x: to.x - from.x,
                y: to.y - from.y,
            };
            if !delta.x.is_finite() || !delta.y.is_finite() {
                return Ok(None);
            }
            let old_spatial_generation = active.spatial.spatial_generation;
            let next_spatial_generation = old_spatial_generation
                .checked_add(1)
                .ok_or("preview spatial generation exhausted")?;
            if active.reducer.state.spatial_generation != old_spatial_generation {
                return Ok(None);
            }
            let logical_delta = crate::radial::geometry::LogicalPoint {
                x: (delta.x / active.spatial.scale_factor.get()) as f32,
                y: (delta.y / active.spatial.scale_factor.get()) as f32,
            };
            if !logical_delta.x.is_finite() || !logical_delta.y.is_finite() {
                return Ok(None);
            }
            (
                active.navigation_frames.clone(),
                active.current_frame_id,
                old_spatial_generation,
                next_spatial_generation,
                active
                    .layout_generation
                    .checked_add(1)
                    .ok_or("preview geometry generation exhausted")?,
                active.native_session.clone(),
                delta,
                active.reducer.clone(),
                crate::radial::geometry::LogicalPoint {
                    x: active.reducer.state.arming_baseline.point.x + logical_delta.x,
                    y: active.reducer.state.arming_baseline.point.y + logical_delta.y,
                },
            )
        };
        for state in frames.values_mut() {
            translate_layout(&mut state.frame.layout, delta)
                .map_err(|error| format!("preview relocation failed: {error:?}"))?;
            state.frame.scene = build_scene_prepared_selected(
                &state.frame.layout,
                next_generation,
                &state.frame.resources,
                state.selected.as_ref(),
            );
        }
        let mut reducer_candidate = reducer.clone();
        reducer_candidate.reduce(SessionEvent::Relocated {
            expected_spatial_generation: old_spatial_generation,
            spatial_generation: next_spatial_generation,
            geometry_generation: next_generation,
            physical_delta: delta,
            pointer_baseline,
        });
        if reducer_candidate.state.spatial_generation != next_spatial_generation
            || reducer_candidate
                .state
                .stack
                .last()
                .is_none_or(|frame| frame.geometry_generation != next_generation)
        {
            return Ok(None);
        }
        reducer = reducer_candidate;
        let Some(current) = frames.get(&current_frame_id).cloned() else {
            return Err("preview current frame is missing during relocation".into());
        };
        if !pointer_baseline.x.is_finite() || !pointer_baseline.y.is_finite() {
            return Ok(None);
        }
        let Some(mut spatial) = self
            .active
            .as_ref()
            .filter(|active| {
                active.native_session == native_session
                    && active.spatial.spatial_generation == old_spatial_generation
            })
            .map(|active| active.spatial)
        else {
            return Ok(None);
        };
        spatial.visible_center.x += delta.x;
        spatial.visible_center.y += delta.y;
        spatial.spatial_generation = next_spatial_generation;
        if !spatial.visible_center.x.is_finite() || !spatial.visible_center.y.is_finite() {
            return Ok(None);
        }
        let active = self.active.as_mut().ok_or("preview lease ended")?;
        if active.native_session != native_session
            || active.spatial.spatial_generation != old_spatial_generation
        {
            return Ok(None);
        }
        active.reducer = reducer;
        active.navigation_frames = frames;
        active.current_frame_id = current_frame_id;
        active.menu_id = current.menu_id.clone();
        active.frame = current.frame.clone();
        active.projection = current.projection.clone();
        active.effective_presentation = current.effective_presentation;
        active.spatial = spatial;
        active.layout_generation = next_generation;
        self.present_active()
    }

    fn stop_active(&mut self, reason: CloseReason) {
        if let Some(scheduler) = &self.tooltip_scheduler {
            scheduler.cancel();
        }
        if let Some(active) = self.active.take()
            && let Some(host) = self.host.as_ref()
        {
            let _ = host.send(NativeCommand::Close {
                session_id: active.native_session,
                reason,
            });
        }
    }

    fn send_checked(&mut self, command: NativeCommand) -> Result<(), String> {
        let result = self.host.as_ref().map_or_else(
            || Err("native preview host unavailable".to_string()),
            |host| host.send(command),
        );
        if let Err(message) = result {
            self.stop_active(CloseReason::HostFailure);
            if let Some(mut host) = self.host.take() {
                host.shutdown();
            }
            self.preparer.release();
            return Err(message);
        }
        Ok(())
    }
}

impl Drop for NativePreviewCoordinator {
    fn drop(&mut self) {
        self.stop_active(CloseReason::Shutdown);
        if let Some(mut host) = self.host.take() {
            host.shutdown();
        }
    }
}

pub(crate) fn merge_preview_resources(
    parent: &crate::radial::render::PreparedSceneResources,
    child: &crate::radial::render::PreparedSceneResources,
) -> crate::radial::render::PreparedSceneResources {
    let mut merged = parent.clone();
    merged.media.extend(child.media.clone());
    merged.text.extend(child.text.clone());
    merged.tooltips.extend(child.tooltips.clone());
    merged
}

fn active_cell_role(
    active: &ActivePreview,
    cell_id: &CellId,
    button: Option<crate::radial::session::PointerButton>,
) -> CellRole {
    if active.reducer.state.stack.len() > 1 && cell_id.as_str() == "__center" {
        CellRole::Back
    } else {
        cell_role(
            &active.document,
            &active.menu_id,
            cell_id,
            &active.frame.provenance,
            button,
        )
    }
}

fn cell_role(
    document: &RadialDocument,
    menu_id: &MenuId,
    cell_id: &CellId,
    projected_provenance: &BTreeMap<CellId, crate::radial::bindings::ProjectedDynamicProvenance>,
    button: Option<crate::radial::session::PointerButton>,
) -> CellRole {
    if cell_id.as_str().starts_with("__radial_page_next:") {
        return CellRole::NextPage;
    }
    if cell_id.as_str().starts_with("__radial_page_previous:") {
        return CellRole::PreviousPage;
    }
    let menu = document.menus.iter().find(|menu| &menu.id == menu_id);
    if let Some(menu) = menu {
        let binding = match (cell_id.as_str(), button) {
            ("__center", Some(crate::radial::session::PointerButton::Secondary)) => {
                menu.center_secondary_control.map(control_role).or_else(|| {
                    menu.center_secondary_action
                        .as_ref()
                        .map(|_| CellRole::Action)
                })
            }
            ("__center", _) => menu
                .center_control
                .map(control_role)
                .or_else(|| menu.center_action.as_ref().map(|_| CellRole::Action)),
            ("__background", Some(crate::radial::session::PointerButton::Secondary)) => menu
                .background_secondary_control
                .map(control_role)
                .or_else(|| {
                    menu.background_secondary_action
                        .as_ref()
                        .map(|_| CellRole::Action)
                }),
            ("__background", _) => menu
                .background_control
                .map(control_role)
                .or_else(|| menu.background_action.as_ref().map(|_| CellRole::Action)),
            _ => None,
        };
        if let Some(role) = binding {
            return role;
        }
    }
    // An authored stable ID wins over a generated projection entry if the
    // user intentionally chose a dyn:-looking ID.
    if projected_provenance.contains_key(cell_id)
        && !menu.is_some_and(|menu| {
            menu.rings
                .iter()
                .flat_map(|ring| &ring.cells)
                .any(|cell| &cell.id == cell_id)
        })
    {
        return CellRole::Action;
    }
    let content = menu
        .and_then(|menu| {
            menu.rings
                .iter()
                .flat_map(|ring| &ring.cells)
                .find(|cell| &cell.id == cell_id)
        })
        .map(|cell| &cell.content);
    match content {
        Some(CellContent::Action { .. } | CellContent::Dynamic { .. }) => CellRole::Action,
        Some(CellContent::Submenu { .. }) => CellRole::Submenu,
        Some(CellContent::Control {
            control: crate::radial::model::Control::Back,
        }) => CellRole::Back,
        Some(CellContent::Control {
            control: crate::radial::model::Control::Close,
        }) => CellRole::Close,
        Some(CellContent::Control {
            control: crate::radial::model::Control::NextPage,
        }) => CellRole::NextPage,
        Some(CellContent::Control {
            control: crate::radial::model::Control::PreviousPage,
        }) => CellRole::PreviousPage,
        Some(CellContent::Control {
            control: crate::radial::model::Control::Drag,
        }) => CellRole::Drag,
        _ => CellRole::Spacer,
    }
}

fn control_role(control: crate::radial::model::Control) -> CellRole {
    match control {
        crate::radial::model::Control::Back => CellRole::Back,
        crate::radial::model::Control::Close => CellRole::Close,
        crate::radial::model::Control::NextPage => CellRole::NextPage,
        crate::radial::model::Control::PreviousPage => CellRole::PreviousPage,
        crate::radial::model::Control::Drag => CellRole::Drag,
    }
}

fn submenu_target(document: &RadialDocument, menu_id: &MenuId, cell_id: &CellId) -> Option<MenuId> {
    document
        .menus
        .iter()
        .find(|menu| &menu.id == menu_id)?
        .rings
        .iter()
        .flat_map(|ring| &ring.cells)
        .find(|cell| &cell.id == cell_id)
        .and_then(|cell| match &cell.content {
            CellContent::Submenu { menu_id } => Some(menu_id.clone()),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::diagnostics::{RadialDiagnosticKind, RadialDiagnosticSource};
    use crate::radial::model::{ActionBinding, TargetSelector};
    use crate::radial::session::{NavigationCommand, NavigationModifiers};
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeHost {
        commands: Arc<Mutex<Vec<NativeCommand>>>,
        events: Arc<Mutex<VecDeque<NativeEvent>>>,
    }

    struct FailingHost {
        fail_on: Arc<Mutex<Option<&'static str>>>,
        shutdowns: Arc<AtomicUsize>,
    }
    impl PreviewHostPort for FailingHost {
        fn send(&self, command: NativeCommand) -> Result<(), String> {
            let kind = match command {
                NativeCommand::Open { .. } => "open",
                NativeCommand::Present { .. } => "present",
                NativeCommand::BeginSystemDrag { .. } => "drag",
                _ => "other",
            };
            let mut fail_on = self.fail_on.lock().unwrap();
            if fail_on.as_ref().is_some_and(|expected| *expected == kind) {
                fail_on.take();
                return Err(format!("synthetic {kind} send failure"));
            }
            Ok(())
        }
        fn try_recv(&self) -> Option<NativeEvent> {
            None
        }
        fn shutdown(&mut self) {
            self.shutdowns.fetch_add(1, Ordering::AcqRel);
        }
    }
    impl PreviewHostPort for FakeHost {
        fn send(&self, command: NativeCommand) -> Result<(), String> {
            self.commands.lock().unwrap().push(command);
            Ok(())
        }
        fn try_recv(&self) -> Option<NativeEvent> {
            self.events.lock().unwrap().pop_front()
        }
        fn shutdown(&mut self) {
            self.commands.lock().unwrap().push(NativeCommand::Shutdown);
        }
    }

    fn coordinator() -> (
        NativePreviewCoordinator,
        Arc<Mutex<Vec<NativeCommand>>>,
        Arc<Mutex<VecDeque<NativeEvent>>>,
    ) {
        let commands = Arc::new(Mutex::new(Vec::new()));
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let factory_commands = Arc::clone(&commands);
        let factory_events = Arc::clone(&events);
        let factory = Arc::new(move || {
            Ok(Box::new(FakeHost {
                commands: Arc::clone(&factory_commands),
                events: Arc::clone(&factory_events),
            }) as Box<dyn PreviewHostPort>)
        });
        (
            NativePreviewCoordinator::with_factory(factory),
            commands,
            events,
        )
    }

    fn frozen_test_context() -> FrozenSpatialContext {
        FrozenSpatialContext {
            requested_root_anchor: PhysicalPoint {
                x: 1_700.0,
                y: 900.0,
            },
            visible_center: PhysicalPoint { x: 32.0, y: -16.0 },
            work_area: PhysicalRect {
                min: PhysicalPoint {
                    x: -1_600.0,
                    y: -1_000.0,
                },
                max: PhysicalPoint {
                    x: 1_600.0,
                    y: 1_000.0,
                },
            },
            scale_factor: ScaleFactor::new(1.25).unwrap(),
            spatial_generation: 0,
            topology_generation: 4,
        }
    }

    fn assert_physical_point_close(actual: PhysicalPoint, expected: PhysicalPoint) {
        const SUBPIXEL_EPSILON: f64 = 1.0e-6;
        assert!(
            (actual.x - expected.x).abs() <= SUBPIXEL_EPSILON
                && (actual.y - expected.y).abs() <= SUBPIXEL_EPSILON,
            "physical points differ beyond subpixel tolerance: actual={actual:?}, expected={expected:?}"
        );
    }

    fn queue_open_first_submenu(
        events: &Arc<Mutex<VecDeque<NativeEvent>>>,
        session_id: &SessionId,
    ) {
        events.lock().unwrap().extend([
            NativeEvent::Navigate {
                session_id: session_id.clone(),
                command: NavigationCommand::Next,
                modifiers: NavigationModifiers::default(),
            },
            NativeEvent::Navigate {
                session_id: session_id.clone(),
                command: NavigationCommand::ActivatePrimary,
                modifiers: NavigationModifiers::default(),
            },
        ]);
    }

    fn start_frozen_preview(
        coordinator: &mut NativePreviewCoordinator,
        document: Arc<RadialDocument>,
        editor_session: u64,
        menu_id: MenuId,
    ) -> NativePreviewLease {
        let menu = document
            .menus
            .iter()
            .find(|menu| menu.id == menu_id)
            .unwrap();
        coordinator
            .start_with_projection_context(
                AuthoringSessionId(editor_session),
                DraftGeneration(7),
                AuthoringRequestId(1),
                Arc::clone(&document),
                menu_id,
                false,
                PreviewProjection {
                    dynamic: synthetic_preview_dynamic(menu),
                    ..PreviewProjection::default()
                },
                Some(frozen_test_context()),
            )
            .unwrap()
            .lease
    }

    #[test]
    fn native_hover_reveals_from_one_shot_deadline_and_same_cell_motion_does_not_present() {
        let (mut coordinator, commands, events) = coordinator();
        coordinator
            .set_tooltip_preferences(TooltipPreferences {
                delay_ms: 0,
                ..TooltipPreferences::default()
            })
            .unwrap();
        let document = Arc::new(RadialDocument::starter());
        let menu_id = document.default_menu_id.clone();
        let lease = start_frozen_preview(&mut coordinator, Arc::clone(&document), 99, menu_id);
        let active = coordinator.active.as_ref().unwrap();
        let native_session = active.native_session.clone();
        let generation = active.layout_generation;
        let cell = active
            .frame
            .layout
            .cells
            .iter()
            .find(|cell| {
                !cell.label.is_empty()
                    && active.frame.resources.tooltips.contains_key(&cell.cell_id)
            })
            .unwrap();
        let point = active
            .frame
            .layout
            .scale_factor
            .physical_to_logical(shape_center(&cell.shape, active.frame.layout.scale_factor));
        let owner = crate::radial::render::input_owner(&active.frame.layout, point, false);
        events.lock().unwrap().push_back(NativeEvent::PointerMoved {
            session_id: native_session.clone(),
            owner: owner.clone(),
            point,
            layout_generation: generation,
        });
        let _ = coordinator.poll();
        let active = coordinator.active.as_ref().unwrap();
        assert_eq!(active.lease, lease);
        assert!(active.tooltip_hover.visible().is_some());
        assert!(active.frame.scene.primitives.iter().any(|primitive| {
            matches!(
                primitive,
                crate::radial::render::VectorPrimitive::Tooltip { .. }
            )
        }));
        let presented = commands
            .lock()
            .unwrap()
            .iter()
            .filter(|command| matches!(command, NativeCommand::Present { .. }))
            .count();

        events.lock().unwrap().push_back(NativeEvent::PointerMoved {
            session_id: native_session,
            owner,
            point: crate::radial::geometry::LogicalPoint {
                x: point.x + 0.1,
                y: point.y + 0.1,
            },
            layout_generation: generation,
        });
        let _ = coordinator.poll();
        assert_eq!(
            commands
                .lock()
                .unwrap()
                .iter()
                .filter(|command| matches!(command, NativeCommand::Present { .. }))
                .count(),
            presented,
            "pointer motion inside the same visible cell must not repaint the host"
        );
    }

    #[test]
    fn native_protective_hover_uses_geometry_without_becoming_actionable() {
        let (mut coordinator, _, events) = coordinator();
        coordinator
            .set_tooltip_preferences(TooltipPreferences {
                delay_ms: 0,
                ..TooltipPreferences::default()
            })
            .unwrap();
        let mut document = RadialDocument::starter();
        let cell_id = document.menus[0].rings[0].cells[0].id.clone();
        document.menus[0].rings[0].cells[0].content = CellContent::Spacer;
        let document = Arc::new(document);
        let lease = start_frozen_preview(
            &mut coordinator,
            Arc::clone(&document),
            100,
            document.default_menu_id.clone(),
        );
        let active = coordinator.active.as_ref().unwrap();
        let native_session = active.native_session.clone();
        let generation = active.layout_generation;
        let cell = active
            .frame
            .layout
            .cells
            .iter()
            .find(|cell| cell.cell_id == cell_id)
            .unwrap();
        let point = active
            .frame
            .layout
            .scale_factor
            .physical_to_logical(shape_center(&cell.shape, active.frame.layout.scale_factor));
        assert!(active.frame.layout.hit_test(point).is_none());
        assert_eq!(
            active
                .frame
                .layout
                .geometric_hover_cell(point)
                .map(|cell| &cell.cell_id),
            Some(&cell_id)
        );
        let owner = crate::radial::render::input_owner(&active.frame.layout, point, false);
        assert_eq!(owner, crate::radial::render::InputOwner::Protective);
        events.lock().unwrap().push_back(NativeEvent::PointerMoved {
            session_id: native_session,
            owner,
            point,
            layout_generation: generation,
        });
        let _ = coordinator.poll();
        let active = coordinator.active.as_ref().unwrap();
        assert_eq!(active.lease, lease);
        assert!(active.reducer.state.hovered.is_none());
        assert_eq!(
            active
                .tooltip_hover
                .visible()
                .map(|identity| &identity.cell_id),
            Some(&cell_id)
        );
        assert!(active.frame.scene.primitives.iter().any(|primitive| {
            matches!(
                primitive,
                crate::radial::render::VectorPrimitive::Tooltip { .. }
            )
        }));
    }

    #[test]
    fn lease_is_exclusive_stale_safe_and_never_dispatches() {
        let (mut coordinator, commands, events) = coordinator();
        let mut document = RadialDocument::starter();
        document.menus[0].rings[0].cells[0].content = CellContent::Action {
            binding: ActionBinding::Contextual {
                selector: TargetSelector::LastExternal,
                action_id: crate::universal_actions::ActionId::new("window.activate"),
            },
        };
        let document = Arc::new(document);
        let menu = document.default_menu_id.clone();
        let lease = coordinator
            .start(
                AuthoringSessionId(1),
                DraftGeneration(1),
                AuthoringRequestId(1),
                Arc::clone(&document),
                menu.clone(),
                false,
            )
            .unwrap()
            .lease;
        assert!(
            coordinator
                .start(
                    AuthoringSessionId(1),
                    DraftGeneration(1),
                    AuthoringRequestId(1),
                    Arc::clone(&document),
                    menu.clone(),
                    false,
                )
                .is_err()
        );
        let native_session = match &commands.lock().unwrap()[0] {
            NativeCommand::Open {
                session_id,
                activate_on_show,
                ..
            } => {
                assert!(!activate_on_show);
                session_id.clone()
            }
            _ => panic!("preview must open through the native host"),
        };
        events.lock().unwrap().extend([
            NativeEvent::Navigate {
                session_id: native_session.clone(),
                command: NavigationCommand::Next,
                modifiers: NavigationModifiers::default(),
            },
            NativeEvent::Navigate {
                session_id: native_session.clone(),
                command: NavigationCommand::ActivatePrimary,
                modifiers: NavigationModifiers::default(),
            },
        ]);
        assert!(
            coordinator
                .poll()
                .iter()
                .all(|notice| matches!(notice, NativePreviewNotice::Diagnostics { .. }))
        );
        assert_eq!(coordinator.intercepted_dispatches, 1);

        let next = coordinator
            .start(
                AuthoringSessionId(2),
                DraftGeneration(1),
                AuthoringRequestId(1),
                document,
                menu,
                false,
            )
            .unwrap();
        assert_ne!(next.lease.editor_session, lease.editor_session);
        assert!(
            commands
                .lock()
                .unwrap()
                .iter()
                .any(|command| { matches!(command, NativeCommand::Close { .. }) })
        );
        coordinator.stop(
            next.lease.editor_session,
            next.lease.generation,
            AuthoringRequestId(2),
        );
        assert!(coordinator.active.is_none());
    }

    #[test]
    fn preview_pages_and_thresholded_drag_stay_inert_but_drive_native_intents() {
        let (mut coordinator, commands, events) = coordinator();
        let mut document = RadialDocument::starter();
        let menu_index = document
            .menus
            .iter()
            .position(|menu| menu.id.as_str() == "starter-applications")
            .unwrap();
        document.menus[menu_index].center_control = Some(crate::radial::model::Control::Drag);
        let menu = document.menus[menu_index].id.clone();
        let document = Arc::new(document);
        coordinator
            .start(
                AuthoringSessionId(22),
                DraftGeneration(7),
                AuthoringRequestId(1),
                document,
                menu,
                false,
            )
            .unwrap();
        let active = coordinator.active.as_mut().unwrap();
        assert!(active.frame.page_count > 1);
        let first_page_ids = active
            .frame
            .layout
            .cells
            .iter()
            .filter(|cell| cell.cell_id.as_str().starts_with("dyn:"))
            .map(|cell| cell.cell_id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let native_session = active.native_session.clone();
        let active_lease = active.lease.clone();
        let center = active.frame.layout.center;
        events.lock().unwrap().extend([
            NativeEvent::Navigate {
                session_id: native_session.clone(),
                command: NavigationCommand::NextPage,
                modifiers: NavigationModifiers::default(),
            },
            NativeEvent::PointerDown {
                session_id: native_session.clone(),
                layout_generation: 8,
                owner: crate::radial::render::InputOwner::Actionable(CellId::new("__center")),
                point: center,
                button: crate::radial::session::PointerButton::Primary,
            },
            NativeEvent::PointerMoved {
                session_id: native_session,
                layout_generation: 8,
                owner: crate::radial::render::InputOwner::Actionable(CellId::new("__center")),
                point: crate::radial::geometry::LogicalPoint {
                    x: center.x + 20.0,
                    y: center.y,
                },
            },
        ]);
        let notices = coordinator.poll();
        assert!(notices.iter().all(|notice| matches!(
            notice,
            NativePreviewNotice::Diagnostics { lease, .. } if lease == &active_lease
        )));
        assert_eq!(
            coordinator.active.as_ref().unwrap().reducer.state.stack[0].page,
            1
        );
        let second_page_ids = coordinator
            .active
            .as_ref()
            .unwrap()
            .frame
            .layout
            .cells
            .iter()
            .filter(|cell| cell.cell_id.as_str().starts_with("dyn:"))
            .map(|cell| cell.cell_id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        assert!(!first_page_ids.is_empty() && !second_page_ids.is_empty());
        assert!(first_page_ids.is_disjoint(&second_page_ids));
        let commands = commands.lock().unwrap();
        assert!(commands.iter().any(|command| matches!(
            command,
            NativeCommand::BeginSystemDrag {
                layout_generation: 8,
                ..
            }
        )));
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, NativeCommand::Present { .. }))
        );
        assert_eq!(coordinator.intercepted_dispatches, 0);
    }

    #[test]
    fn native_preview_same_center_survives_three_levels_drag_and_exact_back() {
        let (mut coordinator, _, events) = coordinator();
        let mut document = RadialDocument::starter();
        let root = document.default_menu_id.clone();
        let favorites = MenuId::new("starter-favorites");
        let applications = MenuId::new("starter-applications");
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == root)
            .unwrap()
            .submenu_presentation = SubmenuPresentation::SameCenter;
        let favorites_menu = document
            .menus
            .iter_mut()
            .find(|menu| menu.id == favorites)
            .unwrap();
        favorites_menu.submenu_presentation = SubmenuPresentation::SameCenter;
        favorites_menu.rings[0].cells[0].content = CellContent::Submenu {
            menu_id: applications.clone(),
        };
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == applications)
            .unwrap()
            .rings[0]
            .cell_radius = 42.0;
        let document = Arc::new(document);
        let lease = start_frozen_preview(&mut coordinator, Arc::clone(&document), 70, root);
        let native_session = coordinator.active.as_ref().unwrap().native_session.clone();
        let root_origin = coordinator.active.as_ref().unwrap().frame.layout.origin;
        assert_physical_point_close(root_origin, frozen_test_context().visible_center);
        let root_active = coordinator.active.as_ref().unwrap();
        assert_eq!(root_active.spatial.visible_center, root_origin);
        assert_eq!(root_active.reducer.state.stack[0].origin, root_origin);

        queue_open_first_submenu(&events, &native_session);
        assert!(
            coordinator
                .poll()
                .iter()
                .all(|notice| matches!(notice, NativePreviewNotice::Diagnostics { .. }))
        );
        let after_first = coordinator.active.as_ref().unwrap();
        assert_eq!(after_first.reducer.state.stack.len(), 2);
        assert_eq!(after_first.frame.layout.origin, root_origin);
        assert_eq!(after_first.spatial.visible_center, root_origin);

        queue_open_first_submenu(&events, &native_session);
        assert!(
            coordinator
                .poll()
                .iter()
                .all(|notice| matches!(notice, NativePreviewNotice::Diagnostics { .. }))
        );
        let after_second = coordinator.active.as_ref().unwrap();
        assert_eq!(after_second.reducer.state.stack.len(), 3);
        assert_eq!(after_second.frame.layout.origin, root_origin);

        let drag_generation = after_second.layout_generation;
        let delta = PhysicalPoint {
            x: -217.5,
            y: 83.75,
        };
        events.lock().unwrap().extend([
            NativeEvent::Relocated {
                session_id: native_session.clone(),
                layout_generation: drag_generation,
                from: PhysicalPoint { x: 100.0, y: 50.0 },
                to: PhysicalPoint {
                    x: 100.0 + delta.x,
                    y: 50.0 + delta.y,
                },
            },
            NativeEvent::CaptureLost {
                session_id: native_session.clone(),
                layout_generation: drag_generation,
            },
        ]);
        assert!(coordinator.poll().is_empty());
        let moved_center = PhysicalPoint {
            x: root_origin.x + delta.x,
            y: root_origin.y + delta.y,
        };
        let moved = coordinator.active.as_ref().unwrap();
        assert_eq!(moved.spatial.visible_center, moved_center);
        assert_eq!(moved.spatial.spatial_generation, 1);
        assert_eq!(moved.reducer.state.stack.len(), 3);
        assert!(
            moved
                .navigation_frames
                .values()
                .all(|frame| frame.frame.layout.origin == moved_center)
        );
        assert_eq!(moved.layout_generation, drag_generation + 1);

        events.lock().unwrap().push_back(NativeEvent::Navigate {
            session_id: native_session.clone(),
            command: NavigationCommand::Back,
            modifiers: NavigationModifiers::default(),
        });
        assert!(
            coordinator
                .poll()
                .iter()
                .all(|notice| matches!(notice, NativePreviewNotice::Diagnostics { .. }))
        );
        let back_one = coordinator.active.as_ref().unwrap();
        assert_eq!(back_one.menu_id, favorites);
        assert_eq!(back_one.frame.layout.origin, moved_center);
        assert_eq!(back_one.spatial.work_area, frozen_test_context().work_area);

        events.lock().unwrap().push_back(NativeEvent::Navigate {
            session_id: native_session,
            command: NavigationCommand::Back,
            modifiers: NavigationModifiers::default(),
        });
        assert!(
            coordinator
                .poll()
                .iter()
                .all(|notice| matches!(notice, NativePreviewNotice::Diagnostics { .. }))
        );
        let back_root = coordinator.active.as_ref().unwrap();
        assert_eq!(back_root.reducer.state.stack[0].frame_id, FrameId(1));
        assert_eq!(back_root.frame.layout.origin, moved_center);
        assert_eq!(back_root.current_frame_id, FrameId(1));
        assert_eq!(back_root.lease, lease);
    }

    #[test]
    fn native_preview_uses_current_parent_presentation_with_cascade_fallback_scope() {
        let (mut coordinator, _, events) = coordinator();
        let mut document = RadialDocument::starter();
        let root = document.default_menu_id.clone();
        let favorites = MenuId::new("starter-favorites");
        let applications = MenuId::new("starter-applications");
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == root)
            .unwrap()
            .submenu_presentation = SubmenuPresentation::Cascade;
        let favorites_menu = document
            .menus
            .iter_mut()
            .find(|menu| menu.id == favorites)
            .unwrap();
        favorites_menu.submenu_presentation = SubmenuPresentation::SameCenter;
        favorites_menu.rings[0].cells[0].content = CellContent::Submenu {
            menu_id: applications,
        };
        let document = Arc::new(document);
        start_frozen_preview(&mut coordinator, Arc::clone(&document), 71, root);
        let native_session = coordinator.active.as_ref().unwrap().native_session.clone();
        let root_center = coordinator.active.as_ref().unwrap().spatial.visible_center;

        queue_open_first_submenu(&events, &native_session);
        assert!(
            coordinator
                .poll()
                .iter()
                .all(|notice| matches!(notice, NativePreviewNotice::Diagnostics { .. }))
        );
        let cascaded = coordinator.active.as_ref().unwrap();
        assert_eq!(
            cascaded.effective_presentation,
            SubmenuPresentation::Cascade
        );
        assert_ne!(cascaded.frame.layout.origin, root_center);
        let cascaded_center = cascaded.frame.layout.origin;

        queue_open_first_submenu(&events, &native_session);
        assert!(
            coordinator
                .poll()
                .iter()
                .all(|notice| matches!(notice, NativePreviewNotice::Diagnostics { .. }))
        );
        let centered = coordinator.active.as_ref().unwrap();
        assert_eq!(
            centered.effective_presentation,
            SubmenuPresentation::SameCenter
        );
        assert_physical_point_close(centered.frame.layout.origin, cascaded_center);
    }

    #[test]
    fn native_preview_cascade_that_cannot_fit_locally_falls_back_to_frozen_center() {
        let (mut coordinator, _, events) = coordinator();
        let mut document = RadialDocument::starter();
        let root = document.default_menu_id.clone();
        let root_menu = document
            .menus
            .iter_mut()
            .find(|menu| menu.id == root)
            .unwrap();
        root_menu.submenu_presentation = SubmenuPresentation::Cascade;
        root_menu.rings[0].rotation_degrees = 180.0;
        let favorites = document
            .menus
            .iter_mut()
            .find(|menu| menu.id.as_str() == "starter-favorites")
            .unwrap();
        favorites.submenu_presentation = SubmenuPresentation::SameCenter;
        favorites.rings[0].radius = 500.0;
        let document = Arc::new(document);
        let mut context = frozen_test_context();
        context.visible_center = PhysicalPoint {
            x: -1_200.0,
            y: -20.0,
        };
        let root_menu = document.menus.iter().find(|menu| menu.id == root).unwrap();
        let lease = coordinator
            .start_with_projection_context(
                AuthoringSessionId(72),
                DraftGeneration(8),
                AuthoringRequestId(1),
                Arc::clone(&document),
                root,
                false,
                PreviewProjection {
                    dynamic: synthetic_preview_dynamic(root_menu),
                    ..PreviewProjection::default()
                },
                Some(context),
            )
            .unwrap()
            .lease;
        let native_session = coordinator.active.as_ref().unwrap().native_session.clone();

        queue_open_first_submenu(&events, &native_session);
        assert!(
            coordinator
                .poll()
                .iter()
                .all(|notice| matches!(notice, NativePreviewNotice::Diagnostics { .. }))
        );
        let child = coordinator.active.as_ref().unwrap();
        assert_eq!(child.lease, lease);
        assert_eq!(
            child.effective_presentation,
            SubmenuPresentation::SameCenter
        );
        assert_eq!(child.frame.placement, PreparedPlacement::SameCenterFallback);
        assert_eq!(child.frame.layout.origin, context.visible_center);
    }

    #[test]
    fn submenu_and_page_diagnostic_changes_are_correlated_and_clear_when_resolved() {
        let (mut submenu_coordinator, _, events) = coordinator();
        let mut document = RadialDocument::starter();
        let child_index = document
            .menus
            .iter()
            .position(|menu| menu.id.as_str() == "starter-favorites")
            .unwrap();
        let missing_id = crate::radial::model::AssetId::new("child-missing");
        document.assets.push(crate::radial::model::AssetRecord {
            id: missing_id.clone(),
            kind: crate::radial::model::MediaKind::Image,
            relative_path: "images/child-missing.png".into(),
            content_sha256: "0".repeat(64),
            byte_len: 3,
        });
        document.menus[child_index].rings[0].cells[0].icon =
            crate::radial::model::Override::Value(crate::radial::model::MediaReference::Managed {
                asset_id: missing_id,
            });
        let document = Arc::new(document);
        let root = document.default_menu_id.clone();
        let started = submenu_coordinator
            .start(
                AuthoringSessionId(40),
                DraftGeneration(5),
                AuthoringRequestId(8),
                Arc::clone(&document),
                root,
                false,
            )
            .unwrap();
        let initial_diagnostics = started.diagnostics;
        let lease = started.lease;
        let native_session = submenu_coordinator
            .active
            .as_ref()
            .unwrap()
            .native_session
            .clone();
        events.lock().unwrap().extend([
            NativeEvent::Navigate {
                session_id: native_session.clone(),
                command: NavigationCommand::Next,
                modifiers: NavigationModifiers::default(),
            },
            NativeEvent::Navigate {
                session_id: native_session.clone(),
                command: NavigationCommand::ActivatePrimary,
                modifiers: NavigationModifiers::default(),
            },
        ]);
        assert!(submenu_coordinator.poll().iter().any(|notice| matches!(
            notice,
            NativePreviewNotice::Diagnostics { lease: actual, diagnostics }
                if actual == &lease && diagnostics.iter().any(|diagnostic| matches!(
                    &diagnostic.source,
                    RadialDiagnosticSource::Asset { identity, .. }
                        if identity.contains("child-missing")
                ))
        )));

        events.lock().unwrap().push_back(NativeEvent::Navigate {
            session_id: native_session,
            command: NavigationCommand::Back,
            modifiers: NavigationModifiers::default(),
        });
        assert!(submenu_coordinator.poll().iter().any(|notice| matches!(
            notice,
            NativePreviewNotice::Diagnostics { lease: actual, diagnostics }
                if actual == &lease && diagnostics == &initial_diagnostics
        )));

        let (mut page_coordinator, _, page_events) = coordinator();
        let menu = document
            .menus
            .iter()
            .find(|menu| menu.id.as_str() == "starter-applications")
            .unwrap()
            .id
            .clone();
        let page_started = page_coordinator
            .start(
                AuthoringSessionId(41),
                DraftGeneration(6),
                AuthoringRequestId(9),
                document,
                menu,
                false,
            )
            .unwrap();
        let page_capacity = page_coordinator
            .active
            .as_ref()
            .unwrap()
            .frame
            .layout
            .cells
            .iter()
            .filter(|cell| cell.cell_id.as_str().starts_with("dyn:"))
            .count();
        let page_active = page_coordinator.active.as_mut().unwrap();
        for frame in page_active.projection.dynamic.values_mut() {
            for entry in frame.entries.iter_mut().skip(page_capacity) {
                entry.label = "A deliberately overlong page-only preview label ".repeat(20);
            }
        }
        let page_session = page_active.native_session.clone();
        page_events
            .lock()
            .unwrap()
            .push_back(NativeEvent::Navigate {
                session_id: page_session,
                command: NavigationCommand::NextPage,
                modifiers: NavigationModifiers::default(),
            });
        assert!(page_coordinator.poll().iter().any(|notice| matches!(
            notice,
            NativePreviewNotice::Diagnostics { lease, diagnostics }
                if lease == &page_started.lease
                    && diagnostics.iter().any(|diagnostic| matches!(
                        &diagnostic.kind,
                        RadialDiagnosticKind::LabelTruncated
                    ))
        )));
    }

    #[test]
    fn editor_resource_release_retires_preview_host_exactly_once() {
        let (mut coordinator, commands, _) = coordinator();
        let document = Arc::new(RadialDocument::starter());
        coordinator
            .start(
                AuthoringSessionId(9),
                DraftGeneration(1),
                AuthoringRequestId(1),
                Arc::clone(&document),
                document.default_menu_id.clone(),
                false,
            )
            .unwrap();
        coordinator.cancel_all();
        coordinator.cancel_all();
        assert!(coordinator.host.is_none());
        let commands = commands.lock().unwrap();
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(command, NativeCommand::Shutdown))
                .count(),
            1
        );
    }

    #[test]
    fn failed_or_stopped_host_is_retired_and_the_next_start_is_fresh() {
        let commands = Arc::new(Mutex::new(Vec::new()));
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let creations = Arc::new(AtomicUsize::new(0));
        let factory_commands = Arc::clone(&commands);
        let factory_events = Arc::clone(&events);
        let factory_creations = Arc::clone(&creations);
        let factory = Arc::new(move || {
            factory_creations.fetch_add(1, Ordering::AcqRel);
            Ok(Box::new(FakeHost {
                commands: Arc::clone(&factory_commands),
                events: Arc::clone(&factory_events),
            }) as Box<dyn PreviewHostPort>)
        });
        let mut coordinator = NativePreviewCoordinator::with_factory(factory);
        let document = Arc::new(RadialDocument::starter());
        let menu = document.default_menu_id.clone();
        let first = coordinator
            .start(
                AuthoringSessionId(31),
                DraftGeneration(1),
                AuthoringRequestId(1),
                Arc::clone(&document),
                menu.clone(),
                false,
            )
            .unwrap()
            .lease;
        let native_session = coordinator.active.as_ref().unwrap().native_session.clone();
        events.lock().unwrap().push_back(NativeEvent::Failed {
            session_id: Some(native_session),
            message: "present failure".into(),
        });
        assert_eq!(
            coordinator.poll(),
            vec![NativePreviewNotice::Failed {
                lease: first,
                message: "present failure".into(),
            }]
        );
        assert!(coordinator.active.is_none() && coordinator.host.is_none());
        let commands_after_failure = commands.lock().unwrap();
        assert!(commands_after_failure.iter().any(|command| matches!(
            command,
            NativeCommand::Close {
                reason: CloseReason::HostFailure,
                ..
            }
        )));
        assert!(
            commands_after_failure
                .iter()
                .any(|command| matches!(command, NativeCommand::Shutdown))
        );
        drop(commands_after_failure);

        coordinator
            .start(
                AuthoringSessionId(31),
                DraftGeneration(2),
                AuthoringRequestId(2),
                Arc::clone(&document),
                menu.clone(),
                false,
            )
            .unwrap();
        assert_eq!(creations.load(Ordering::Acquire), 2);
        events.lock().unwrap().push_back(NativeEvent::Stopped);
        assert_eq!(coordinator.poll().len(), 1);
        assert!(coordinator.active.is_none() && coordinator.host.is_none());
        coordinator
            .start(
                AuthoringSessionId(31),
                DraftGeneration(3),
                AuthoringRequestId(3),
                document,
                menu,
                false,
            )
            .unwrap();
        assert_eq!(creations.load(Ordering::Acquire), 3);
    }

    #[test]
    fn synchronous_open_present_and_drag_send_failures_retire_host_and_recover_fresh() {
        for failed_kind in ["open", "present", "drag"] {
            let fail_on = Arc::new(Mutex::new(Some(failed_kind)));
            let shutdowns = Arc::new(AtomicUsize::new(0));
            let creations = Arc::new(AtomicUsize::new(0));
            let factory_fail_on = Arc::clone(&fail_on);
            let factory_shutdowns = Arc::clone(&shutdowns);
            let factory_creations = Arc::clone(&creations);
            let factory = Arc::new(move || {
                factory_creations.fetch_add(1, Ordering::AcqRel);
                Ok(Box::new(FailingHost {
                    fail_on: Arc::clone(&factory_fail_on),
                    shutdowns: Arc::clone(&factory_shutdowns),
                }) as Box<dyn PreviewHostPort>)
            });
            let mut coordinator = NativePreviewCoordinator::with_factory(factory);
            let mut document = RadialDocument::starter();
            document.menus[0].center_control = Some(crate::radial::model::Control::Drag);
            let document = Arc::new(document);
            let menu = document.default_menu_id.clone();
            let first = coordinator.start(
                AuthoringSessionId(70),
                DraftGeneration(1),
                AuthoringRequestId(1),
                Arc::clone(&document),
                menu.clone(),
                false,
            );
            match failed_kind {
                "open" => assert!(first.is_err()),
                "present" => {
                    first.unwrap();
                    assert!(coordinator.present_active().is_err());
                }
                "drag" => {
                    first.unwrap();
                    assert!(
                        coordinator
                            .apply_intents(
                                vec![SessionIntent::BeginNativeDrag {
                                    geometry_generation: 1,
                                }],
                                false
                            )
                            .is_err()
                    );
                }
                _ => unreachable!(),
            }
            assert!(coordinator.active.is_none());
            assert!(coordinator.host.is_none());
            assert_eq!(shutdowns.load(Ordering::Acquire), 1);
            assert!(
                coordinator
                    .start(
                        AuthoringSessionId(70),
                        DraftGeneration(2),
                        AuthoringRequestId(2),
                        Arc::clone(&document),
                        menu,
                        false,
                    )
                    .is_ok()
            );
            assert_eq!(creations.load(Ordering::Acquire), 2);
        }
    }

    #[test]
    fn sampled_context_excludes_owned_and_preview_windows_and_preserves_external() {
        let external = WindowIdentity {
            hwnd: 7,
            pid: 70,
            process_name: Some("external.exe".into()),
            process_path: Some("C:\\Apps\\external.exe".into()),
            class_name: Some("ExternalWindow".into()),
            title: "External".into(),
        };
        let context = InvocationContext {
            token: 1,
            foreground: Some(WindowIdentity {
                hwnd: 10,
                pid: 99,
                process_name: Some("multi-launcher.exe".into()),
                process_path: None,
                class_name: None,
                title: "Radial editor".into(),
            }),
            under_pointer: Some(WindowIdentity {
                hwnd: 11,
                pid: 100,
                process_name: None,
                process_path: None,
                class_name: None,
                title: "Native preview".into(),
            }),
            last_external: None,
            monitor_id: "monitor:1".into(),
            pointer_physical: (1, 2),
        };
        let sanitized = sanitize_preview_context(context, 99, &[11], Some(external.clone()));
        assert_eq!(sanitized.foreground, Some(external.clone()));
        assert!(sanitized.under_pointer.is_none());
        assert_eq!(sanitized.last_external, Some(external));
    }

    #[test]
    fn shared_frame_builder_produces_identical_native_and_embedded_inputs() {
        let document = RadialDocument::starter();
        let anchor = PhysicalPoint { x: 300.0, y: 300.0 };
        let work = PhysicalRect {
            min: PhysicalPoint { x: 0.0, y: 0.0 },
            max: PhysicalPoint { x: 600.0, y: 600.0 },
        };
        let scale = ScaleFactor::new(1.0).unwrap();
        let mut native_preparer = PreviewFramePreparer::new(PathBuf::new());
        let native = build_preview_frame_input(
            &mut native_preparer,
            &document,
            &document.default_menu_id,
            anchor,
            work,
            scale,
            9,
            None,
        )
        .unwrap();
        let mut embedded_preparer = PreviewFramePreparer::new(PathBuf::new());
        let embedded = build_preview_frame_input(
            &mut embedded_preparer,
            &document,
            &document.default_menu_id,
            anchor,
            work,
            scale,
            9,
            None,
        )
        .unwrap();
        assert_eq!(native, embedded);
        assert_eq!(native.resources.text.len(), native.layout.cells.len());
        let mut native_compositor = crate::radial::compositor::CompositorCache::default();
        let mut embedded_compositor = crate::radial::compositor::CompositorCache::default();
        let native_image = native_compositor.compose(&native.scene, scale, 0).unwrap();
        let embedded_image = embedded_compositor
            .compose(&embedded.scene, scale, 0)
            .unwrap();
        assert_eq!(native_image.image.as_raw(), embedded_image.image.as_raw());
    }

    #[test]
    fn preview_boundary_has_no_audio_history_executor_or_reservation_owner() {
        let source = include_str!("native_preview.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(!production.contains("RadialAudio"));
        assert!(!production.contains("record_history"));
        assert!(!production.contains("hotkey"));
        assert!(!production.contains("DispatchRequested"));
        assert!(production.contains("SessionIntent::Dispatch"));
    }
}
