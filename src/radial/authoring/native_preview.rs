//! Main-owned native authoring preview lease.
//!
//! This coordinator deliberately has no executor, history sink, audio session,
//! trigger registration, or invocation service. Native input is reduced only
//! for navigation; every dispatch intent is counted and discarded.

use super::{AuthoringRequestId, AuthoringSessionId, DraftGeneration, NativePreviewLease};
use crate::radial::context::{InvocationContext, WindowIdentity};
use crate::radial::geometry::{PhysicalPoint, PhysicalRect, ScaleFactor};
use crate::radial::model::{CellContent, CellId, InvocationId, MenuId, RadialDocument, SessionId};
use crate::radial::native::{CloseReason, NativeCommand, NativeEvent, NativeHost};
pub use crate::radial::preparation::PreparedFrameInput as PreviewFrameInput;
use crate::radial::preparation::{
    PreviewFramePreparer, PreviewProjection, synthetic_preview_dynamic,
};
use crate::radial::session::{CellRole, SessionEvent, SessionIntent, SessionReducer};
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
    work_area: PhysicalRect,
    reducer: SessionReducer,
    projection: PreviewProjection,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreviewLeaseResult {
    pub lease: NativePreviewLease,
    pub sampled_context: InvocationContext,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NativePreviewNotice {
    Diagnostics {
        lease: NativePreviewLease,
        diagnostics: Vec<String>,
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
    pub intercepted_dispatches: usize,
}

impl NativePreviewCoordinator {
    pub fn new(wake: mpsc::Sender<()>, application_data: PathBuf) -> Self {
        let factory = Arc::new(move || {
            NativeHost::spawn_with_wake(Some(wake.clone()))
                .map(|host| Box::new(host) as Box<dyn PreviewHostPort>)
        });
        Self::with_factory_and_preparer(factory, PreviewFramePreparer::new(application_data))
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
            intercepted_dispatches: 0,
        }
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
        let sequence = (generation, request_id);
        if self
            .newest
            .get(&editor_session)
            .is_some_and(|newest| sequence <= *newest)
        {
            return Err("stale native preview request".into());
        }
        validate(&document).map_err(|error| error.to_string())?;
        self.stop_active(CloseReason::SettingsReload);
        self.newest.insert(editor_session, sequence);
        let (anchor, work, scale) = super::super::controller::desktop_geometry();
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
            reducer_frame.page = frame.page;
            reducer_frame.page_count = frame.page_count;
        }
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
            work_area: work,
            reducer,
            projection,
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
        self.start_with_projection(
            previous.editor_session,
            generation,
            request_id,
            document,
            menu_id,
            sample_external_context,
            projection,
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
        notices
    }

    fn handle_event(&mut self, event: NativeEvent) -> Result<Option<Vec<String>>, String> {
        if matches!(event, NativeEvent::Stopped) {
            self.active = None;
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
        let Some(active) = self.active.as_mut() else {
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
            | NativeEvent::DisplayChanged { session_id }
            | NativeEvent::Closed { session_id, .. } => Some(session_id),
            NativeEvent::Failed { session_id, .. } => session_id.as_ref(),
            NativeEvent::Stopped => None,
        };
        if event_session.is_some_and(|session| session != &active.native_session) {
            return Ok(None);
        }
        let generation = active.lease.generation.0;
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
            } => Some(*layout_generation),
            _ => None,
        };
        if event_generation.is_some_and(|event_generation| event_generation != generation) {
            return Ok(None);
        }
        let intents = match event {
            NativeEvent::PointerMoved { point, .. } => {
                let cell = active
                    .frame
                    .layout
                    .hit_test(point)
                    .map(|cell| cell.cell_id.clone());
                active.reducer.reduce(SessionEvent::PointerMoved {
                    point,
                    hovered: cell,
                    geometry_generation: generation,
                })
            }
            NativeEvent::PointerDown { point, button, .. } => {
                let cell = active
                    .frame
                    .layout
                    .hit_test(point)
                    .map(|cell| cell.cell_id.clone());
                let role = cell.as_ref().map_or(CellRole::Unavailable, |cell| {
                    cell_role(&active.document, &active.menu_id, cell, Some(button))
                });
                active.reducer.reduce(SessionEvent::PointerDown {
                    point,
                    cell,
                    role,
                    button,
                    geometry_generation: generation,
                })
            }
            NativeEvent::PointerUp { point, button, .. } => {
                let cell = active
                    .frame
                    .layout
                    .hit_test(point)
                    .map(|cell| cell.cell_id.clone());
                let role = cell.as_ref().map_or(CellRole::Unavailable, |cell| {
                    cell_role(&active.document, &active.menu_id, cell, Some(button))
                });
                active.reducer.reduce(SessionEvent::PointerUp {
                    point,
                    cell,
                    role,
                    button,
                    geometry_generation: generation,
                })
            }
            NativeEvent::Navigate {
                command, modifiers, ..
            } => {
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
                            cell_role(&active.document, &active.menu_id, &cell.cell_id, None),
                        )
                    })
                    .collect();
                active.reducer.reduce(SessionEvent::Navigate {
                    command,
                    cells,
                    pointer_baseline: active.frame.layout.center,
                    geometry_generation: generation,
                })
            }
            NativeEvent::Escape { .. } | NativeEvent::CaptureLost { .. } => {
                self.stop_active(CloseReason::Dismissed);
                return Ok(None);
            }
            NativeEvent::DisplayChanged { .. } => {
                self.stop_active(CloseReason::DisplayRelayout);
                return Ok(None);
            }
            NativeEvent::Closed { .. } => {
                self.active = None;
                return Ok(None);
            }
            NativeEvent::Ready { .. }
            | NativeEvent::PointerLeft { .. }
            | NativeEvent::Failed { .. }
            | NativeEvent::Stopped => Vec::new(),
        };
        self.apply_intents(intents)
    }

    fn apply_intents(
        &mut self,
        intents: Vec<SessionIntent>,
    ) -> Result<Option<Vec<String>>, String> {
        for intent in intents {
            match intent {
                SessionIntent::Dispatch { .. } => self.intercepted_dispatches += 1,
                SessionIntent::OpenSubmenu { cell_id } => {
                    let active = self.active.as_mut().ok_or("preview lease ended")?;
                    let target = submenu_target(&active.document, &active.menu_id, &cell_id)
                        .ok_or("preview submenu target is unavailable")?;
                    active.reducer.reduce(SessionEvent::OpenChild {
                        menu_id: target.clone(),
                        origin: active.frame.layout.origin,
                        geometry_generation: active.lease.generation.0,
                        pointer_baseline: active.frame.layout.center,
                    });
                    active.menu_id = target;
                    active.projection.page = 0;
                    if let Some(menu) = active
                        .document
                        .menus
                        .iter()
                        .find(|menu| menu.id == active.menu_id)
                    {
                        active.projection.dynamic = synthetic_preview_dynamic(menu);
                    }
                }
                SessionIntent::BeginNativeDrag {
                    geometry_generation,
                    ..
                } => {
                    let command = {
                        let active = self.active.as_ref().ok_or("preview lease ended")?;
                        NativeCommand::BeginSystemDrag {
                            session_id: active.native_session.clone(),
                            layout_generation: geometry_generation,
                        }
                    };
                    self.send_checked(command)?;
                }
                SessionIntent::Back => {
                    if let Some(active) = self.active.as_mut()
                        && let Some(frame) = active.reducer.state.stack.last()
                    {
                        active.menu_id = frame.menu_id.clone();
                        active.projection.page = frame.page;
                        if let Some(menu) = active
                            .document
                            .menus
                            .iter()
                            .find(|menu| menu.id == active.menu_id)
                        {
                            active.projection.dynamic = synthetic_preview_dynamic(menu);
                        }
                    }
                }
                SessionIntent::PageChanged { page } => {
                    if let Some(active) = self.active.as_mut() {
                        active.projection.page = page;
                    }
                }
                SessionIntent::CloseTree => self.stop_active(CloseReason::Dismissed),
            }
        }
        self.present_active()
    }

    fn present_active(&mut self) -> Result<Option<Vec<String>>, String> {
        let Some(active) = self.active.as_mut() else {
            return Ok(None);
        };
        let previous_diagnostics = active.frame.diagnostics.clone();
        let selected = active.reducer.state.hovered.as_ref().or(active
            .reducer
            .state
            .selected
            .as_ref());
        active.frame = build_preview_frame_input_projected(
            &mut self.preparer,
            &active.document,
            &active.menu_id,
            active.frame.layout.origin,
            active.work_area,
            active.frame.layout.scale_factor,
            active.lease.generation.0,
            selected,
            &active.projection,
        )?;
        if let Some(reducer_frame) = active.reducer.state.stack.last_mut() {
            reducer_frame.page = active.frame.page;
            reducer_frame.page_count = active.frame.page_count;
        }
        let command = NativeCommand::Present {
            session_id: active.native_session.clone(),
            scene: active.frame.scene.clone(),
            layout: active.frame.layout.clone(),
            always_on_top: true,
            activate_on_show: false,
        };
        let diagnostics = active.frame.diagnostics.clone();
        self.send_checked(command)?;
        Ok((diagnostics != previous_diagnostics).then_some(diagnostics))
    }

    fn stop_active(&mut self, reason: CloseReason) {
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

fn cell_role(
    document: &RadialDocument,
    menu_id: &MenuId,
    cell_id: &CellId,
    button: Option<crate::radial::session::PointerButton>,
) -> CellRole {
    if cell_id.as_str().starts_with("__radial_page_next:") {
        return CellRole::NextPage;
    }
    if cell_id.as_str().starts_with("__radial_page_previous:") {
        return CellRole::PreviousPage;
    }
    if cell_id.as_str().starts_with("dyn:") {
        return CellRole::Action;
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
        assert!(coordinator.poll().is_empty());
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
                layout_generation: 7,
                owner: crate::radial::render::InputOwner::Actionable(CellId::new("__center")),
                point: center,
                button: crate::radial::session::PointerButton::Primary,
            },
            NativeEvent::PointerMoved {
                session_id: native_session,
                layout_generation: 7,
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
                layout_generation: 7,
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
                if actual == &lease && diagnostics.iter().any(|message| message.contains("child-missing"))
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
                    && diagnostics.iter().any(|message| message.contains("LabelTruncated"))
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
                            .apply_intents(vec![SessionIntent::BeginNativeDrag {
                                geometry_generation: 1,
                            }])
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
