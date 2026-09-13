use super::geometry::{
    LayoutSnapshot, LogicalPoint, PhysicalPoint, PhysicalRect, ScaleFactor, layout_menu,
};
use super::invocation::InvocationIntent;
use super::model::{CellContent, InteractionMode, InvocationId, MenuId, RadialDocument, SessionId};
use super::native::{CloseReason, NativeCommand, NativeEvent, NativeHost};
use super::render::{InputOwner, build_scene};
use super::session::{CellRole, PointerButton, SessionEvent, SessionIntent, SessionReducer};
use std::collections::VecDeque;
use std::sync::{Arc, mpsc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControllerEvent {
    ToggleLegacyLauncher,
    Opened {
        invocation_id: InvocationId,
        session_id: SessionId,
    },
    Closed {
        invocation_id: InvocationId,
        session_id: SessionId,
    },
    InvocationFailed {
        invocation_id: InvocationId,
        message: String,
    },
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
}
struct ActiveSession {
    invocation_id: InvocationId,
    session_id: SessionId,
    menu_id: MenuId,
    layout: LayoutSnapshot,
    reducer: SessionReducer,
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
}
impl RadialController {
    pub fn new(document: Arc<RadialDocument>, diagnostics: bool, wake: mpsc::Sender<()>) -> Self {
        Self::with_factory(
            document,
            diagnostics,
            Arc::new(move || {
                NativeHost::spawn_with_wake(Some(wake.clone()))
                    .map(|h| Box::new(h) as Box<dyn HostPort>)
            }),
        )
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
        }
    }
    pub fn replace_document(&mut self, document: Arc<RadialDocument>) {
        self.close(CloseReason::SettingsReload, None);
        self.document = document;
    }
    pub fn disable(&mut self) {
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
                    interaction,
                    ..
                } => self.open(id, menu_id, interaction, always_on_top, &mut out),
                InvocationIntent::ToggleDirectMenu { id, menu_id } => {
                    let current_menu = self
                        .pending
                        .as_ref()
                        .map(|session| &session.menu_id)
                        .or_else(|| self.active.as_ref().map(|session| &session.menu_id));
                    if current_menu.is_some_and(|current| current == &menu_id) {
                        self.close(CloseReason::Dismissed, None);
                        continue;
                    }
                    let interaction = self
                        .document
                        .menus
                        .iter()
                        .find(|m| m.id == menu_id)
                        .map_or(InteractionMode::StickyClick, |m| m.interaction);
                    self.open(id, menu_id, interaction, always_on_top, &mut out)
                }
                InvocationIntent::CloseRadial { session_id } => {
                    self.close(CloseReason::Dismissed, session_id.as_ref())
                }
                InvocationIntent::ScheduleDeadline { .. }
                | InvocationIntent::CancelDeadline { .. }
                | InvocationIntent::HoldCancelledBeforePresentation { .. } => {}
            }
        }
        out
    }
    fn open(
        &mut self,
        invocation_id: InvocationId,
        menu_id: MenuId,
        interaction: InteractionMode,
        always_on_top: bool,
        out: &mut Vec<ControllerEvent>,
    ) {
        let Some(menu) = self.document.menus.iter().find(|m| m.id == menu_id) else {
            out.push(ControllerEvent::InvocationFailed {
                invocation_id,
                message: format!("radial menu {menu_id} no longer exists"),
            });
            return;
        };
        let (anchor, work, scale) = desktop_geometry();
        let layout = match layout_menu(menu, anchor, work, scale, 0.55) {
            Ok(v) => v,
            Err(e) => {
                out.push(ControllerEvent::InvocationFailed {
                    invocation_id,
                    message: format!("radial layout failed: {e:?}"),
                });
                return;
            }
        };
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
                    interaction,
                    layout,
                    generation,
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
        loop {
            let event = self.host.as_ref().and_then(|h| h.try_recv());
            let Some(event) = event else { break };
            self.handle_native(event, &mut out)
        }
        out
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
                let reducer = SessionReducer::new(
                    session_id.clone(),
                    self.document.revision,
                    p.menu_id.clone(),
                    p.layout.origin,
                    layout_generation,
                    p.interaction,
                    p.invocation_id,
                    p.layout.center,
                );
                let invocation_id = p.invocation_id;
                self.active = Some(ActiveSession {
                    invocation_id,
                    session_id: session_id.clone(),
                    menu_id: p.menu_id,
                    layout: p.layout,
                    reducer,
                });
                self.record(Some(session_id.clone()), "native host ready".into());
                out.push(ControllerEvent::Opened {
                    invocation_id,
                    session_id,
                });
            }
            NativeEvent::Closed { session_id, .. } => {
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
                        session_id,
                    });
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
                let generation = self.layout_generation_for(&session_id);
                self.session_event(
                    &session_id,
                    SessionEvent::PointerMoved {
                        point,
                        hovered: owner_cell(owner),
                        geometry_generation: generation,
                    },
                    out,
                )
            }
            NativeEvent::PointerDown {
                session_id,
                owner,
                point,
            } => {
                let generation = self.layout_generation_for(&session_id);
                let role = self.role(&session_id, &owner);
                self.session_event(
                    &session_id,
                    SessionEvent::PointerDown {
                        point,
                        cell: owner_cell(owner),
                        role,
                        button: PointerButton::Primary,
                        geometry_generation: generation,
                    },
                    out,
                )
            }
            NativeEvent::PointerUp {
                session_id,
                owner,
                point,
            } => {
                let generation = self.layout_generation_for(&session_id);
                let role = self.role(&session_id, &owner);
                self.session_event(
                    &session_id,
                    SessionEvent::PointerUp {
                        point,
                        cell: owner_cell(owner),
                        role,
                        button: PointerButton::Primary,
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
        for intent in intents {
            if intent == SessionIntent::CloseTree {
                self.close(CloseReason::Dismissed, Some(id))
            } else if matches!(intent, SessionIntent::Dispatch { .. }) {
                self.close(CloseReason::Dismissed, Some(id));
                out.push(ControllerEvent::Error(
                    "radial action dispatch is unavailable until M3".into(),
                ))
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
        self.document
            .menus
            .iter()
            .find(|m| m.id == active.menu_id)
            .and_then(|m| {
                m.rings
                    .iter()
                    .flat_map(|r| &r.cells)
                    .find(|c| &c.id == cell)
            })
            .map_or(CellRole::Unavailable, |c| match &c.content {
                CellContent::Action { .. } => CellRole::Action,
                CellContent::Submenu { .. } => CellRole::Submenu,
                _ => CellRole::Spacer,
            })
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
            }],
            false,
        );
        controller.handle_intents(
            vec![InvocationIntent::ToggleDirectMenu {
                id: InvocationId(11),
                menu_id: MenuId::new("starter"),
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
}
