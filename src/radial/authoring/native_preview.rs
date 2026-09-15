//! Main-owned native authoring preview lease.
//!
//! This coordinator deliberately has no executor, history sink, audio session,
//! trigger registration, or invocation service. Native input is reduced only
//! for navigation; every dispatch intent is counted and discarded.

use super::{AuthoringRequestId, AuthoringSessionId, DraftGeneration, NativePreviewLease};
use crate::radial::context::{InvocationContext, WindowIdentity};
use crate::radial::geometry::{
    LayoutSnapshot, PhysicalPoint, PhysicalRect, ScaleFactor, layout_document_menu,
};
use crate::radial::model::{CellContent, CellId, InvocationId, MenuId, RadialDocument, SessionId};
use crate::radial::native::{CloseReason, NativeCommand, NativeEvent, NativeHost};
use crate::radial::render::{VectorScene, build_scene_selected};
use crate::radial::session::{CellRole, SessionEvent, SessionIntent, SessionReducer};
use crate::radial::validation::validate;
use std::collections::BTreeMap;
use std::sync::{Arc, mpsc};

#[derive(Clone, Debug, PartialEq)]
pub struct PreviewFrameInput {
    pub layout: LayoutSnapshot,
    pub scene: VectorScene,
}

pub fn build_preview_frame_input(
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
    let layout = layout_document_menu(document, menu, anchor, work_area, scale, 0.55)
        .map_err(|error| format!("native preview layout failed: {error:?}"))?;
    let scene = build_scene_selected(&layout, generation, selected);
    Ok(PreviewFrameInput { layout, scene })
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
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreviewLeaseResult {
    pub lease: NativePreviewLease,
    pub sampled_context: InvocationContext,
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
    active: Option<ActivePreview>,
    newest: BTreeMap<AuthoringSessionId, (DraftGeneration, AuthoringRequestId)>,
    last_external: Option<WindowIdentity>,
    pub intercepted_dispatches: usize,
}

impl NativePreviewCoordinator {
    pub fn new(wake: mpsc::Sender<()>) -> Self {
        let factory = Arc::new(move || {
            NativeHost::spawn_with_wake(Some(wake.clone()))
                .map(|host| Box::new(host) as Box<dyn PreviewHostPort>)
        });
        Self::with_factory(factory)
    }

    fn with_factory(factory: HostFactory) -> Self {
        Self {
            host: None,
            factory,
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
        let frame = build_preview_frame_input(
            &document,
            &menu_id,
            anchor,
            work,
            scale,
            generation.0,
            None,
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
        let reducer = SessionReducer::new(
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
        self.host
            .as_ref()
            .ok_or("native preview host unavailable")?
            .send(NativeCommand::Open {
                session_id: native_session.clone(),
                scene: frame.scene.clone(),
                layout: frame.layout.clone(),
                always_on_top: true,
                activate_on_show: false,
            })?;
        self.active = Some(ActivePreview {
            lease: lease.clone(),
            native_session,
            document,
            menu_id,
            frame,
            work_area: work,
            reducer,
        });
        Ok(PreviewLeaseResult {
            lease,
            sampled_context,
        })
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
        if !self.active.as_ref().is_some_and(|active| {
            active.lease == *previous && previous.editor_session == active.lease.editor_session
        }) {
            return Err("native preview lease is no longer active".into());
        }
        self.start(
            previous.editor_session,
            generation,
            request_id,
            document,
            menu_id,
            sample_external_context,
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
    }

    pub fn poll(&mut self) -> Vec<(NativePreviewLease, Option<String>)> {
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
                if ended || result.is_err() {
                    notices.push((lease, result.err()));
                }
            }
        }
        notices
    }

    fn handle_event(&mut self, event: NativeEvent) -> Result<(), String> {
        let Some(active) = self.active.as_mut() else {
            return Ok(());
        };
        let event_session = match &event {
            NativeEvent::Ready { session_id, .. }
            | NativeEvent::PointerDown { session_id, .. }
            | NativeEvent::PointerMoved { session_id, .. }
            | NativeEvent::PointerLeft { session_id }
            | NativeEvent::PointerUp { session_id, .. }
            | NativeEvent::CaptureLost { session_id }
            | NativeEvent::Escape { session_id }
            | NativeEvent::Navigate { session_id, .. }
            | NativeEvent::DisplayChanged { session_id }
            | NativeEvent::Closed { session_id, .. } => Some(session_id),
            NativeEvent::Failed { session_id, .. } => session_id.as_ref(),
            NativeEvent::Stopped => None,
        };
        if event_session.is_some_and(|session| session != &active.native_session) {
            return Ok(());
        }
        let generation = active.lease.generation.0;
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
                    cell_role(&active.document, &active.menu_id, cell)
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
                    cell_role(&active.document, &active.menu_id, cell)
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
                            cell_role(&active.document, &active.menu_id, &cell.cell_id),
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
                return Ok(());
            }
            NativeEvent::DisplayChanged { .. } => {
                self.stop_active(CloseReason::DisplayRelayout);
                return Ok(());
            }
            NativeEvent::Closed { .. } | NativeEvent::Stopped => {
                self.active = None;
                return Ok(());
            }
            NativeEvent::Failed { message, .. } => {
                self.active = None;
                return Err(message);
            }
            NativeEvent::Ready { .. } | NativeEvent::PointerLeft { .. } => Vec::new(),
        };
        self.apply_intents(intents)
    }

    fn apply_intents(&mut self, intents: Vec<SessionIntent>) -> Result<(), String> {
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
                    self.present_active()?;
                }
                SessionIntent::Back | SessionIntent::PageChanged { .. } => {
                    if let Some(active) = self.active.as_mut()
                        && let Some(frame) = active.reducer.state.stack.last()
                    {
                        active.menu_id = frame.menu_id.clone();
                    }
                    self.present_active()?;
                }
                SessionIntent::CloseTree => self.stop_active(CloseReason::Dismissed),
            }
        }
        self.present_active()
    }

    fn present_active(&mut self) -> Result<(), String> {
        let Some(active) = self.active.as_mut() else {
            return Ok(());
        };
        let selected = active.reducer.state.hovered.as_ref().or(active
            .reducer
            .state
            .selected
            .as_ref());
        active.frame = build_preview_frame_input(
            &active.document,
            &active.menu_id,
            active.frame.layout.origin,
            active.work_area,
            active.frame.layout.scale_factor,
            active.lease.generation.0,
            selected,
        )?;
        self.host
            .as_ref()
            .ok_or("native preview host unavailable")?
            .send(NativeCommand::Present {
                session_id: active.native_session.clone(),
                scene: active.frame.scene.clone(),
                layout: active.frame.layout.clone(),
                always_on_top: true,
                activate_on_show: false,
            })
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
}

impl Drop for NativePreviewCoordinator {
    fn drop(&mut self) {
        self.stop_active(CloseReason::Shutdown);
        if let Some(mut host) = self.host.take() {
            host.shutdown();
        }
    }
}

fn cell_role(document: &RadialDocument, menu_id: &MenuId, cell_id: &CellId) -> CellRole {
    let content = document
        .menus
        .iter()
        .find(|menu| &menu.id == menu_id)
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
        _ => CellRole::Spacer,
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

    struct FakeHost {
        commands: Arc<Mutex<Vec<NativeCommand>>>,
        events: Arc<Mutex<VecDeque<NativeEvent>>>,
    }
    impl PreviewHostPort for FakeHost {
        fn send(&self, command: NativeCommand) -> Result<(), String> {
            self.commands.lock().unwrap().push(command);
            Ok(())
        }
        fn try_recv(&self) -> Option<NativeEvent> {
            self.events.lock().unwrap().pop_front()
        }
        fn shutdown(&mut self) {}
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
        let native = build_preview_frame_input(
            &document,
            &document.default_menu_id,
            anchor,
            work,
            scale,
            9,
            None,
        )
        .unwrap();
        let embedded = build_preview_frame_input(
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
