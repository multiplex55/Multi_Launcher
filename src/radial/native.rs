use super::geometry::{LayoutSnapshot, LogicalPoint};
use super::model::{CellId, SessionId};
use super::render::{InputOwner, VectorScene, input_owner};
use super::session::{NavigationCommand, NavigationModifiers, PointerButton};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};

#[derive(Clone)]
struct HostEvents {
    tx: mpsc::Sender<NativeEvent>,
    wake: Option<mpsc::Sender<()>>,
}
impl HostEvents {
    fn send(&self, event: NativeEvent) {
        let _ = self.tx.send(event);
        if let Some(wake) = &self.wake {
            let _ = wake.send(());
        }
    }
}

#[cfg(windows)]
const WM_RADIAL_COMMAND: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 0x52;

#[derive(Clone, Debug)]
pub enum NativeCommand {
    Open {
        session_id: SessionId,
        scene: VectorScene,
        layout: LayoutSnapshot,
        always_on_top: bool,
        activate_on_show: bool,
    },
    /// Atomically replaces the scene for the same logical session. No Closed
    /// event is emitted, so navigation cannot be mistaken for teardown.
    Replace {
        session_id: SessionId,
        scene: VectorScene,
        layout: LayoutSnapshot,
        always_on_top: bool,
        activate_on_show: bool,
    },
    /// Presents a new frame/layout in the existing HWND. The session remains
    /// captured and no Closed event is emitted.
    Present {
        session_id: SessionId,
        scene: VectorScene,
        layout: LayoutSnapshot,
        always_on_top: bool,
        activate_on_show: bool,
    },
    Close {
        session_id: SessionId,
        reason: CloseReason,
    },
    Shutdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseReason {
    Dismissed,
    ActionHandoff,
    ExclusiveTool,
    SettingsReload,
    DisplayRelayout,
    HostFailure,
    Shutdown,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NativeEvent {
    Ready {
        session_id: SessionId,
        layout_generation: u64,
    },
    PointerDown {
        session_id: SessionId,
        owner: InputOwner,
        point: LogicalPoint,
        button: PointerButton,
    },
    PointerMoved {
        session_id: SessionId,
        owner: InputOwner,
        point: LogicalPoint,
    },
    PointerLeft {
        session_id: SessionId,
    },
    PointerUp {
        session_id: SessionId,
        owner: InputOwner,
        point: LogicalPoint,
        button: PointerButton,
    },
    CaptureLost {
        session_id: SessionId,
    },
    Escape {
        session_id: SessionId,
    },
    Navigate {
        session_id: SessionId,
        command: NavigationCommand,
        modifiers: NavigationModifiers,
    },
    DisplayChanged {
        session_id: SessionId,
    },
    Closed {
        session_id: SessionId,
        reason: CloseReason,
    },
    Failed {
        session_id: Option<SessionId>,
        message: String,
    },
    Stopped,
}

fn is_drag_owner(layout: &LayoutSnapshot, owner: &InputOwner, button: PointerButton) -> bool {
    let InputOwner::Actionable(id) = owner else {
        return false;
    };
    layout.cells.iter().any(|cell| {
        &cell.cell_id == id
            && match button {
                PointerButton::Primary => cell.control,
                PointerButton::Secondary => cell.secondary_control,
            } == Some(super::model::Control::Drag)
    })
}

struct Wake {
    thread_id: AtomicU32,
}
impl Wake {
    fn new() -> Self {
        Self {
            thread_id: AtomicU32::new(0),
        }
    }
    fn signal(&self) -> Result<(), String> {
        #[cfg(windows)]
        {
            let id = self.thread_id.load(Ordering::Acquire);
            if id != 0 {
                unsafe {
                    windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                        id,
                        WM_RADIAL_COMMAND,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    )
                }
                .map_err(|e| format!("failed to wake radial host: {e}"))?;
            }
        }
        Ok(())
    }
    fn quit(&self) {
        #[cfg(windows)]
        {
            let id = self.thread_id.load(Ordering::Acquire);
            if id != 0 {
                let _ = unsafe {
                    windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                        id,
                        windows::Win32::UI::WindowsAndMessaging::WM_QUIT,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    )
                };
            }
        }
    }
}

/// One retained, event-driven native host thread. A session is acknowledged
/// only after the surface and its explicit input region are usable.
pub struct NativeHost {
    commands: mpsc::Sender<(u32, NativeCommand)>,
    events: mpsc::Receiver<NativeEvent>,
    wake: Arc<Wake>,
    join: Option<JoinHandle<()>>,
    stopped: mpsc::Receiver<()>,
    command_sequence: AtomicU32,
    invalid_commands: Arc<AtomicU32>,
}

type SurfaceFactory = Arc<
    dyn Fn(
            SessionId,
            VectorScene,
            LayoutSnapshot,
            bool,
            bool,
            HostEvents,
        ) -> Result<PlatformSurface, String>
        + Send
        + Sync,
>;

fn command_is_valid(sequence: u32, invalid_through: u32) -> bool {
    sequence > invalid_through
}

impl NativeHost {
    pub fn spawn() -> Result<Self, String> {
        Self::spawn_with_wake(None)
    }
    pub fn spawn_with_wake(wake_sender: Option<mpsc::Sender<()>>) -> Result<Self, String> {
        Self::spawn_with_factory(
            wake_sender,
            Arc::new(
                |session_id, scene, layout, always_on_top, activate_on_show, events| {
                    PlatformSurface::create(
                        session_id,
                        scene,
                        layout,
                        always_on_top,
                        activate_on_show,
                        events,
                    )
                },
            ),
        )
    }
    fn spawn_with_factory(
        wake_sender: Option<mpsc::Sender<()>>,
        surface_factory: SurfaceFactory,
    ) -> Result<Self, String> {
        let (commands, rx) = mpsc::channel();
        let invalid_commands = Arc::new(AtomicU32::new(0));
        let worker_invalid_commands = Arc::clone(&invalid_commands);
        let (event_tx, events) = mpsc::channel();
        let event_tx = HostEvents {
            tx: event_tx,
            wake: wake_sender,
        };
        let wake = Arc::new(Wake::new());
        let worker_wake = Arc::clone(&wake);
        let (start_tx, start_rx) = mpsc::sync_channel(1);
        let (stopped_tx, stopped) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("radial-native-host".into())
            .spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    worker_loop(
                        rx,
                        event_tx.clone(),
                        &worker_wake,
                        start_tx,
                        worker_invalid_commands,
                        surface_factory,
                    )
                }));
                if result.is_err() {
                    event_tx.send(NativeEvent::Failed {
                        session_id: None,
                        message: "radial native host panicked".into(),
                    });
                }
                event_tx.send(NativeEvent::Stopped);
                let _ = stopped_tx.send(());
            })
            .map_err(|e| format!("failed to spawn radial host: {e}"))?;
        match start_rx.recv_timeout(std::time::Duration::from_secs(2)) {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                let _ = join.join();
                return Err(error);
            }
            Err(_) => {
                wake.quit();
                let _ = thread::Builder::new()
                    .name("radial-native-startup-deferred-join".into())
                    .spawn(move || {
                        let _ = join.join();
                    });
                return Err("radial host readiness timed out".into());
            }
        }
        Ok(Self {
            commands,
            events,
            wake,
            join: Some(join),
            stopped,
            command_sequence: AtomicU32::new(0),
            invalid_commands,
        })
    }
    pub fn send(&self, command: NativeCommand) -> Result<(), String> {
        let sequence = self.command_sequence.fetch_add(1, Ordering::AcqRel) + 1;
        self.commands
            .send((sequence, command))
            .map_err(|_| "radial host command channel closed".to_string())?;
        if let Err(error) = self.wake.signal() {
            self.invalid_commands.fetch_max(sequence, Ordering::AcqRel);
            self.wake.quit();
            return Err(error);
        }
        Ok(())
    }
    pub fn try_recv(&self) -> Option<NativeEvent> {
        self.events.try_recv().ok()
    }
    pub fn recv(&self) -> Result<NativeEvent, String> {
        self.events
            .recv()
            .map_err(|_| "radial host event channel closed".into())
    }
    pub fn shutdown(&mut self) {
        let _ = self.send(NativeCommand::Shutdown);
        if self
            .stopped
            .recv_timeout(std::time::Duration::from_secs(2))
            .is_err()
        {
            self.wake.quit();
            let _ = self.stopped.recv_timeout(std::time::Duration::from_secs(2));
        }
        if let Some(join) = self.join.take() {
            if join.is_finished() {
                let _ = join.join();
            } else {
                let _ = thread::Builder::new()
                    .name("radial-native-deferred-join".into())
                    .spawn(move || {
                        let _ = join.join();
                    });
            }
        }
    }
}
impl Drop for NativeHost {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Active {
    id: SessionId,
    surface: PlatformSurface,
    _suppression: crate::mouse_gestures::service::GestureSuppressionGuard,
}

fn handle(
    command: NativeCommand,
    active: &mut Option<Active>,
    events: &HostEvents,
    surface_factory: &SurfaceFactory,
) -> bool {
    match command {
        NativeCommand::Open {
            session_id,
            scene,
            layout,
            always_on_top,
            activate_on_show,
        } => {
            if let Some(old) = active.take() {
                let id = old.id.clone();
                drop(old);
                let _ = events.send(NativeEvent::Closed {
                    session_id: id,
                    reason: CloseReason::Dismissed,
                });
            }
            match surface_factory(
                session_id.clone(),
                scene.clone(),
                layout,
                always_on_top,
                activate_on_show,
                events.clone(),
            ) {
                Ok(surface) => {
                    let suppression = crate::mouse_gestures::service::acquire_gesture_suppression();
                    *active = Some(Active {
                        id: session_id.clone(),
                        surface,
                        _suppression: suppression,
                    });
                    let _ = events.send(NativeEvent::Ready {
                        session_id,
                        layout_generation: scene.generation,
                    });
                }
                Err(message) => {
                    let _ = events.send(NativeEvent::Failed {
                        session_id: Some(session_id),
                        message,
                    });
                }
            }
            true
        }
        NativeCommand::Replace {
            session_id,
            scene,
            layout,
            always_on_top,
            activate_on_show,
        }
        | NativeCommand::Present {
            session_id,
            scene,
            layout,
            always_on_top,
            activate_on_show,
        } => {
            if active
                .as_ref()
                .is_none_or(|current| current.id != session_id)
            {
                events.send(NativeEvent::Failed {
                    session_id: Some(session_id),
                    message: "cannot replace a stale radial session".into(),
                });
                return true;
            }
            let result = active
                .as_mut()
                .expect("validated active session")
                .surface
                .present(scene.clone(), layout, always_on_top, activate_on_show);
            match result {
                Ok(()) => events.send(NativeEvent::Ready {
                    session_id,
                    layout_generation: scene.generation,
                }),
                Err(message) => events.send(NativeEvent::Failed {
                    session_id: Some(session_id),
                    message,
                }),
            }
            true
        }
        NativeCommand::Close { session_id, reason } => {
            if active.as_ref().is_some_and(|a| a.id == session_id) {
                drop(active.take());
                let _ = events.send(NativeEvent::Closed { session_id, reason });
            }
            true
        }
        NativeCommand::Shutdown => {
            if let Some(old) = active.take() {
                let id = old.id.clone();
                drop(old);
                let _ = events.send(NativeEvent::Closed {
                    session_id: id,
                    reason: CloseReason::Shutdown,
                });
            }
            false
        }
    }
}

fn drain(
    rx: &mpsc::Receiver<(u32, NativeCommand)>,
    active: &mut Option<Active>,
    events: &HostEvents,
    invalid_commands: &AtomicU32,
    surface_factory: &SurfaceFactory,
) -> bool {
    while let Ok((sequence, c)) = rx.try_recv() {
        if !command_is_valid(sequence, invalid_commands.load(Ordering::Acquire)) {
            continue;
        }
        if !handle(c, active, events, surface_factory) {
            return false;
        }
    }
    true
}

#[cfg(any(not(windows), test))]
fn worker_loop(
    rx: mpsc::Receiver<(u32, NativeCommand)>,
    events: HostEvents,
    _: &Wake,
    start: mpsc::SyncSender<Result<(), String>>,
    invalid_commands: Arc<AtomicU32>,
    surface_factory: SurfaceFactory,
) {
    let _ = start.send(Ok(()));
    let mut active = None;
    while let Ok((sequence, c)) = rx.recv() {
        if !command_is_valid(sequence, invalid_commands.load(Ordering::Acquire)) {
            continue;
        }
        if !handle(c, &mut active, &events, &surface_factory) {
            break;
        }
    }
}

#[cfg(all(windows, not(test)))]
fn worker_loop(
    rx: mpsc::Receiver<(u32, NativeCommand)>,
    events: HostEvents,
    wake: &Wake,
    start: mpsc::SyncSender<Result<(), String>>,
    invalid_commands: Arc<AtomicU32>,
    surface_factory: SurfaceFactory,
) {
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, TranslateMessage,
    };
    let mut msg = MSG::default();
    let _ = unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE) };
    wake.thread_id
        .store(unsafe { GetCurrentThreadId() }, Ordering::Release);
    let _ = start.send(Ok(()));
    let mut active = None;
    if !drain(
        &rx,
        &mut active,
        &events,
        &invalid_commands,
        &surface_factory,
    ) {
        return;
    }
    loop {
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if r.0 <= 0 {
            break;
        }
        if msg.message == WM_RADIAL_COMMAND {
            if !drain(
                &rx,
                &mut active,
                &events,
                &invalid_commands,
                &surface_factory,
            ) {
                break;
            }
        } else {
            let _ = unsafe { TranslateMessage(&msg) };
            unsafe { DispatchMessageW(&msg) };
        }
    }
    drop(active.take());
    wake.thread_id.store(0, Ordering::Release);
}

#[cfg(any(not(windows), test))]
struct PlatformSurface;
#[cfg(any(not(windows), test))]
impl PlatformSurface {
    fn create(
        _: SessionId,
        _: VectorScene,
        _: LayoutSnapshot,
        _: bool,
        _: bool,
        _: HostEvents,
    ) -> Result<Self, String> {
        Ok(Self)
    }

    fn present(
        &mut self,
        _: VectorScene,
        _: LayoutSnapshot,
        _: bool,
        _: bool,
    ) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(windows)]
struct SystemPlatformSurface {
    hwnd: windows::Win32::Foundation::HWND,
}
#[cfg(windows)]
unsafe impl Send for SystemPlatformSurface {}
#[cfg(all(windows, not(test)))]
type PlatformSurface = SystemPlatformSurface;

#[cfg(windows)]
struct WindowState {
    session_id: SessionId,
    layout: LayoutSnapshot,
    scene: VectorScene,
    events: HostEvents,
    origin: LogicalPoint,
    scale_factor: super::geometry::ScaleFactor,
    captured: bool,
    destroyed: Arc<std::sync::atomic::AtomicBool>,
    presented: Arc<std::sync::atomic::AtomicBool>,
    activate_on_show: bool,
    compositor: super::compositor::CompositorCache,
    animation_epoch: std::time::Instant,
    animation_timer: usize,
    animation_serial: usize,
}

#[cfg(windows)]
unsafe extern "system" fn wndproc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    w: windows::Win32::Foundation::WPARAM,
    l: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::Controls::WM_MOUSELEAVE;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
    };
    use windows::Win32::UI::WindowsAndMessaging::*;
    if msg == WM_NCCREATE {
        let cs = unsafe { &*(l.0 as *const CREATESTRUCTW) };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize) };
    }
    if msg == WM_MOUSEACTIVATE {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        return windows::Win32::Foundation::LRESULT(
            if !ptr.is_null() && unsafe { &*ptr }.activate_on_show {
                MA_ACTIVATE as isize
            } else {
                MA_NOACTIVATE as isize
            },
        );
    }
    if msg == WM_NCHITTEST {
        use windows::Win32::Foundation::POINT;
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        if !ptr.is_null() {
            let (x, y) = signed_message_point(l);
            let mut point = POINT { x, y };
            if unsafe { windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut point) }.as_bool()
            {
                let state = unsafe { &*ptr };
                let logical =
                    client_physical_to_logical(state.origin, state.scale_factor, point.x, point.y);
                return windows::Win32::Foundation::LRESULT(
                    if !native_point_owned(&state.layout, logical) {
                        HTTRANSPARENT as isize
                    } else {
                        HTCLIENT as isize
                    },
                );
            }
        }
    }
    if msg == WM_PAINT {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        if !ptr.is_null() {
            paint_scene(hwnd, unsafe { &*ptr });
        }
        return windows::Win32::Foundation::LRESULT(0);
    }
    if msg == WM_MOUSEMOVE
        || msg == WM_LBUTTONDOWN
        || msg == WM_LBUTTONUP
        || msg == WM_RBUTTONDOWN
        || msg == WM_RBUTTONUP
    {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        if !ptr.is_null() {
            if msg == WM_MOUSEMOVE {
                let mut tracking = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = unsafe { TrackMouseEvent(&mut tracking) };
            }
            let (x, y) = signed_message_point(l);
            let state = unsafe { &*ptr };
            let point = client_physical_to_logical(state.origin, state.scale_factor, x, y);
            let owner = input_owner(&state.layout, point, false);
            let drag_button = if msg == WM_LBUTTONDOWN {
                Some(PointerButton::Primary)
            } else if msg == WM_RBUTTONDOWN {
                Some(PointerButton::Secondary)
            } else {
                None
            };
            if drag_button.is_some_and(|button| is_drag_owner(&state.layout, &owner, button)) {
                let _ = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture() };
                unsafe {
                    SendMessageW(
                        hwnd,
                        WM_NCLBUTTONDOWN,
                        windows::Win32::Foundation::WPARAM(HTCAPTION as usize),
                        l,
                    )
                };
                return windows::Win32::Foundation::LRESULT(0);
            }
            let event = if msg == WM_LBUTTONDOWN || msg == WM_RBUTTONDOWN {
                if matches!(owner, InputOwner::Actionable(_)) {
                    let _ =
                        unsafe { windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd) };
                    unsafe { (*ptr).captured = true };
                }
                NativeEvent::PointerDown {
                    session_id: state.session_id.clone(),
                    owner,
                    point,
                    button: if msg == WM_RBUTTONDOWN {
                        PointerButton::Secondary
                    } else {
                        PointerButton::Primary
                    },
                }
            } else if msg == WM_LBUTTONUP || msg == WM_RBUTTONUP {
                if unsafe { (*ptr).captured } {
                    unsafe { (*ptr).captured = false };
                    let _ =
                        unsafe { windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture() };
                }
                NativeEvent::PointerUp {
                    session_id: state.session_id.clone(),
                    owner,
                    point,
                    button: if msg == WM_RBUTTONUP {
                        PointerButton::Secondary
                    } else {
                        PointerButton::Primary
                    },
                }
            } else {
                NativeEvent::PointerMoved {
                    session_id: state.session_id.clone(),
                    owner,
                    point,
                }
            };
            let _ = state.events.send(event);
        }
        return windows::Win32::Foundation::LRESULT(0);
    }
    if msg == WM_MOUSELEAVE {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        if !ptr.is_null() {
            let state = unsafe { &*ptr };
            state.events.send(NativeEvent::PointerLeft {
                session_id: state.session_id.clone(),
            });
        }
        return windows::Win32::Foundation::LRESULT(0);
    }
    if msg == WM_CAPTURECHANGED || msg == WM_CANCELMODE {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        if !ptr.is_null() && unsafe { (*ptr).captured } {
            unsafe { (*ptr).captured = false };
            let state = unsafe { &*ptr };
            let _ = state.events.send(NativeEvent::CaptureLost {
                session_id: state.session_id.clone(),
            });
        }
    }
    if msg == WM_DISPLAYCHANGE {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        if !ptr.is_null() {
            let state = unsafe { &*ptr };
            let _ = state.events.send(NativeEvent::DisplayChanged {
                session_id: state.session_id.clone(),
            });
        }
    }
    if msg == WM_KEYDOWN && w.0 == 0x1B {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        if !ptr.is_null() {
            let state = unsafe { &*ptr };
            let _ = state.events.send(NativeEvent::Escape {
                session_id: state.session_id.clone(),
            });
        }
        return windows::Win32::Foundation::LRESULT(0);
    }
    if msg == WM_TIMER {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        if !ptr.is_null() && unsafe { (*ptr).animation_timer } == w.0 {
            let state = unsafe { &mut *ptr };
            let _ = unsafe { KillTimer(hwnd, state.animation_timer) };
            state.animation_timer = 0;
            let elapsed = state.animation_epoch.elapsed().as_millis() as u64;
            let scene = state.scene.clone();
            match state
                .compositor
                .compose(&scene, state.scale_factor, elapsed)
            {
                Ok(frame) => {
                    let (x, y, _, _) = physical_scene_bounds(scene.bounds, state.scale_factor);
                    if let Err(message) = present_layered(hwnd, x, y, &frame.image) {
                        state.events.send(NativeEvent::Failed {
                            session_id: Some(state.session_id.clone()),
                            message,
                        });
                    } else {
                        if let Err(message) =
                            schedule_animation(hwnd, state, frame.animation_deadline_ms)
                        {
                            state.events.send(NativeEvent::Failed {
                                session_id: Some(state.session_id.clone()),
                                message,
                            });
                        }
                    }
                }
                Err(error) => state.events.send(NativeEvent::Failed {
                    session_id: Some(state.session_id.clone()),
                    message: format!("radial animation composition failed: {error:?}"),
                }),
            }
        }
        return windows::Win32::Foundation::LRESULT(0);
    }
    if msg == WM_NCDESTROY {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
        if !ptr.is_null() {
            let mut state = unsafe { Box::from_raw(ptr) };
            cancel_animation(hwnd, &mut state);
            state.destroyed.store(true, Ordering::Release);
            drop(state);
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

fn native_point_owned(layout: &LayoutSnapshot, point: LogicalPoint) -> bool {
    !matches!(input_owner(layout, point, false), InputOwner::Exterior)
}

#[cfg(windows)]
fn cancel_animation(hwnd: windows::Win32::Foundation::HWND, state: &mut WindowState) {
    if state.animation_timer != 0 {
        let _ = unsafe {
            windows::Win32::UI::WindowsAndMessaging::KillTimer(hwnd, state.animation_timer)
        };
        state.animation_timer = 0;
    }
}

#[cfg(windows)]
fn schedule_animation(
    hwnd: windows::Win32::Foundation::HWND,
    state: &mut WindowState,
    deadline_ms: Option<u64>,
) -> Result<(), String> {
    cancel_animation(hwnd, state);
    let Some(deadline) = deadline_ms else {
        return Ok(());
    };
    let elapsed = state.animation_epoch.elapsed().as_millis() as u64;
    let delay = deadline.saturating_sub(elapsed).clamp(1, u32::MAX as u64) as u32;
    state.animation_serial = state.animation_serial.wrapping_add(1).max(1);
    let timer = unsafe {
        windows::Win32::UI::WindowsAndMessaging::SetTimer(hwnd, state.animation_serial, delay, None)
    };
    if timer == 0 {
        Err("failed to schedule radial animation frame".into())
    } else {
        state.animation_timer = timer;
        Ok(())
    }
}

#[cfg(windows)]
fn paint_scene(hwnd: windows::Win32::Foundation::HWND, state: &WindowState) {
    use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
    let mut paint = PAINTSTRUCT::default();
    let dc = unsafe { BeginPaint(hwnd, &mut paint) };
    if !dc.0.is_null() {
        let _ = unsafe { EndPaint(hwnd, &paint) };
    }
    let _ = state;
}

/// Correctly sign-extends native client/screen coordinates on negative monitors.
pub fn signed_message_point(value: windows::Win32::Foundation::LPARAM) -> (i32, i32) {
    (
        (value.0 as u16 as i16) as i32,
        (((value.0 as usize >> 16) as u16 as i16) as i32),
    )
}

fn client_physical_to_logical(
    origin: LogicalPoint,
    scale: super::geometry::ScaleFactor,
    x: i32,
    y: i32,
) -> LogicalPoint {
    LogicalPoint {
        x: origin.x + (x as f64 / scale.get()) as f32,
        y: origin.y + (y as f64 / scale.get()) as f32,
    }
}

fn physical_scene_bounds(
    bounds: super::geometry::LogicalRect,
    scale: super::geometry::ScaleFactor,
) -> (i32, i32, i32, i32) {
    let min = scale.logical_to_physical(bounds.min);
    let max = scale.logical_to_physical(bounds.max);
    (
        min.x.floor() as i32,
        min.y.floor() as i32,
        (max.x - min.x).ceil().max(1.0) as i32,
        (max.y - min.y).ceil().max(1.0) as i32,
    )
}

#[cfg(windows)]
impl SystemPlatformSurface {
    fn create(
        session_id: SessionId,
        scene: VectorScene,
        layout: LayoutSnapshot,
        always_on_top: bool,
        activate_on_show: bool,
        events: HostEvents,
    ) -> Result<Self, String> {
        use std::sync::Once;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::*;
        use windows::core::PCWSTR;
        static REGISTER: Once = Once::new();
        let class: Vec<u16> = "MultiLauncherRadialHost\0".encode_utf16().collect();
        let instance = unsafe { GetModuleHandleW(PCWSTR::null()) }.map_err(|e| e.to_string())?;
        REGISTER.call_once(|| {
            let wc = WNDCLASSW {
                hInstance: instance.into(),
                lpszClassName: PCWSTR(class.as_ptr()),
                lpfnWndProc: Some(wndproc),
                ..Default::default()
            };
            let _ = unsafe { RegisterClassW(&wc) };
        });
        let (x, y, width, height) = physical_scene_bounds(scene.bounds, layout.scale_factor);
        let logical_origin = scene.bounds.min;
        let scale_factor = layout.scale_factor;
        let destroyed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let presented = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let state = Box::new(WindowState {
            session_id,
            layout,
            scene,
            events,
            origin: logical_origin,
            scale_factor,
            captured: false,
            destroyed: Arc::clone(&destroyed),
            presented: Arc::clone(&presented),
            activate_on_show,
            compositor: super::compositor::CompositorCache::default(),
            animation_epoch: std::time::Instant::now(),
            animation_timer: 0,
            animation_serial: 0,
        });
        let ptr = Box::into_raw(state);
        let mut ex = WS_EX_LAYERED | WS_EX_TOOLWINDOW;
        if !activate_on_show {
            ex |= WS_EX_NOACTIVATE;
        }
        if always_on_top {
            ex |= WS_EX_TOPMOST;
        }
        let hwnd = unsafe {
            CreateWindowExW(
                ex,
                PCWSTR(class.as_ptr()),
                PCWSTR::null(),
                WS_POPUP,
                x,
                y,
                width,
                height,
                HWND::default(),
                None,
                instance,
                Some(ptr.cast()),
            )
        };
        let hwnd = match hwnd {
            Ok(h) => h,
            Err(e) => {
                if !destroyed.swap(true, Ordering::AcqRel) {
                    unsafe { drop(Box::from_raw(ptr)) };
                }
                return Err(format!("CreateWindowExW failed: {e}"));
            }
        };
        presented.store(false, Ordering::Release);
        let initial_scene = unsafe { (*ptr).scene.clone() };
        let frame = match unsafe { &mut *ptr }
            .compositor
            .compose(&initial_scene, scale_factor, 0)
        {
            Ok(frame) => frame,
            Err(error) => {
                let _ = unsafe { DestroyWindow(hwnd) };
                return Err(format!("initial radial composition failed: {error:?}"));
            }
        };
        if let Err(error) = present_layered(hwnd, x, y, &frame.image) {
            let _ = unsafe { DestroyWindow(hwnd) };
            return Err(error);
        }
        presented.store(true, Ordering::Release);
        unsafe {
            let _ = ShowWindow(
                hwnd,
                if activate_on_show {
                    SW_SHOW
                } else {
                    SW_SHOWNOACTIVATE
                },
            );
        }
        if let Err(error) =
            schedule_animation(hwnd, unsafe { &mut *ptr }, frame.animation_deadline_ms)
        {
            let _ = unsafe { DestroyWindow(hwnd) };
            return Err(error);
        }
        if !presented.load(Ordering::Acquire) {
            let _ = unsafe { DestroyWindow(hwnd) };
            return Err("initial radial presentation produced no frame".into());
        }
        Ok(Self { hwnd })
    }

    fn present(
        &mut self,
        scene: VectorScene,
        layout: LayoutSnapshot,
        always_on_top: bool,
        activate_on_show: bool,
    ) -> Result<(), String> {
        use windows::Win32::UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetWindowLongPtrW, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE,
            SWP_SHOWWINDOW, SetWindowLongPtrW, SetWindowPos, WS_EX_NOACTIVATE,
        };
        let (x, y, width, height) = physical_scene_bounds(scene.bounds, layout.scale_factor);
        let ptr = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                self.hwnd,
                windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            ) as *mut WindowState
        };
        if ptr.is_null() {
            return Err("radial window state is unavailable".into());
        }
        cancel_animation(self.hwnd, unsafe { &mut *ptr });
        unsafe { (*ptr).animation_epoch = std::time::Instant::now() };
        let frame = unsafe { &mut *ptr }
            .compositor
            .compose(&scene, layout.scale_factor, 0)
            .map_err(|error| format!("radial composition failed: {error:?}"))?;
        let origin = scene.bounds.min;
        let mut style = unsafe { GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) };
        if activate_on_show {
            style &= !(WS_EX_NOACTIVATE.0 as isize);
        } else {
            style |= WS_EX_NOACTIVATE.0 as isize;
        }
        unsafe { SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, style) };
        unsafe {
            SetWindowPos(
                self.hwnd,
                if always_on_top {
                    HWND_TOPMOST
                } else {
                    HWND_NOTOPMOST
                },
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        }
        .map_err(|error| format!("radial relayout failed: {error}"))?;
        present_layered(self.hwnd, x, y, &frame.image)?;
        unsafe {
            (*ptr).layout = layout;
            (*ptr).scene = scene;
            (*ptr).origin = origin;
            (*ptr).scale_factor = frame.scale_factor;
            (*ptr).activate_on_show = activate_on_show;
            (*ptr).presented.store(true, Ordering::Release);
        }
        schedule_animation(self.hwnd, unsafe { &mut *ptr }, frame.animation_deadline_ms)?;
        Ok(())
    }
}

#[cfg(windows)]
fn present_layered(
    hwnd: windows::Win32::Foundation::HWND,
    x: i32,
    y: i32,
    image: &image::RgbaImage,
) -> Result<(), String> {
    use std::{mem, ptr};
    use windows::Win32::Foundation::{COLORREF, POINT, SIZE};
    use windows::Win32::Graphics::Gdi::{
        AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
        CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, SelectObject,
    };
    use windows::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};
    let width = i32::try_from(image.width()).map_err(|_| "radial frame width is too large")?;
    let height = i32::try_from(image.height()).map_err(|_| "radial frame height is too large")?;
    let byte_len = image.as_raw().len();
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.0.is_null() {
            return Err("CreateCompatibleDC failed for radial frame".into());
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            bmiColors: [Default::default()],
        };
        let mut bits = ptr::null_mut();
        let bitmap = match CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(bitmap) if !bits.is_null() => bitmap,
            _ => {
                let _ = DeleteDC(dc);
                return Err("CreateDIBSection failed for radial frame".into());
            }
        };
        let old = SelectObject(dc, bitmap);
        if old.0.is_null() {
            let _ = DeleteObject(bitmap);
            let _ = DeleteDC(dc);
            return Err("SelectObject failed for radial frame".into());
        }
        let destination = std::slice::from_raw_parts_mut(bits.cast::<u8>(), byte_len);
        let converted = crate::platform::pixels::premultiplied_bgra(image, destination);
        let destination_point = POINT { x, y };
        let size = SIZE {
            cx: width,
            cy: height,
        };
        let source_point = POINT::default();
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let updated = converted.map_err(str::to_string).and_then(|_| {
            UpdateLayeredWindow(
                hwnd,
                None,
                Some(&destination_point),
                Some(&size),
                dc,
                Some(&source_point),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
            .map_err(|error| format!("UpdateLayeredWindow failed: {error}"))
        });
        let _ = SelectObject(dc, old);
        let _ = DeleteObject(bitmap);
        let _ = DeleteDC(dc);
        updated
    }
}
#[cfg(windows)]
impl Drop for SystemPlatformSurface {
    fn drop(&mut self) {
        let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.hwnd) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::geometry::{PhysicalPoint, PhysicalRect, ScaleFactor, layout_menu};
    use crate::radial::model::RadialDocument;
    #[test]
    fn lifecycle_is_ready_before_open_and_cleanup_is_idempotent() {
        let mut host = NativeHost::spawn().unwrap();
        let layout = layout_menu(
            &RadialDocument::starter().menus[0],
            PhysicalPoint { x: 200.0, y: 200.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 500.0, y: 500.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        let id = SessionId::new("test");
        host.send(NativeCommand::Open {
            session_id: id.clone(),
            scene: super::super::render::build_scene(&layout, 7),
            layout,
            always_on_top: false,
            activate_on_show: false,
        })
        .unwrap();
        assert!(matches!(
            host.recv().unwrap(),
            NativeEvent::Ready {
                layout_generation: 7,
                ..
            }
        ));
        host.send(NativeCommand::Close {
            session_id: id.clone(),
            reason: CloseReason::Dismissed,
        })
        .unwrap();
        assert!(matches!(host.recv().unwrap(), NativeEvent::Closed { .. }));
        host.send(NativeCommand::Close {
            session_id: id,
            reason: CloseReason::Dismissed,
        })
        .unwrap();
        host.shutdown();
    }
    #[test]
    fn surface_failure_publishes_failed_without_ready() {
        let mut host = NativeHost::spawn_with_factory(
            None,
            Arc::new(|_, _, _, _, _, _| Err("surface-stage failure".into())),
        )
        .unwrap();
        let layout = layout_menu(
            &RadialDocument::starter().menus[0],
            PhysicalPoint { x: 200.0, y: 200.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 500.0, y: 500.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        host.send(NativeCommand::Open {
            session_id: SessionId::new("fail"),
            scene: super::super::render::build_scene(&layout, 3),
            layout,
            always_on_top: false,
            activate_on_show: false,
        })
        .unwrap();
        assert!(matches!(
            host.recv().unwrap(),
            NativeEvent::Failed { ref message, .. } if message.contains("surface-stage failure")
        ));
        assert!(host.try_recv().is_none());
        host.shutdown();
    }
    #[test]
    fn ready_is_emitted_only_after_surface_factory_reports_presented() {
        let presented = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = presented.clone();
        let mut host = NativeHost::spawn_with_factory(
            None,
            Arc::new(move |_, _, _, _, _, _| {
                observed.store(true, Ordering::Release);
                Ok(PlatformSurface)
            }),
        )
        .unwrap();
        let layout = layout_menu(
            &RadialDocument::starter().menus[0],
            PhysicalPoint { x: 200.0, y: 200.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 500.0, y: 500.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        host.send(NativeCommand::Open {
            session_id: SessionId::new("presented"),
            scene: super::super::render::build_scene(&layout, 4),
            layout,
            always_on_top: false,
            activate_on_show: false,
        })
        .unwrap();
        assert!(matches!(host.recv().unwrap(), NativeEvent::Ready { .. }));
        assert!(presented.load(Ordering::Acquire));
        host.shutdown();
    }
    #[test]
    fn wake_failure_generation_rejects_queued_command_without_rejecting_successors() {
        assert!(!command_is_valid(4, 4));
        assert!(!command_is_valid(3, 4));
        assert!(command_is_valid(5, 4));
    }
    #[cfg(windows)]
    #[test]
    fn signed_coordinates_support_negative_monitors() {
        let packed = windows::Win32::Foundation::LPARAM(
            (((-40i16 as u16 as u32) << 16) | (-12i16 as u16 as u32)) as isize,
        );
        assert_eq!(signed_message_point(packed), (-12, -40));
    }
    #[test]
    fn native_surface_conversions_are_scale_consistent() {
        for factor in [1.0, 1.5, 2.0] {
            let scale = ScaleFactor::new(factor).unwrap();
            let origin = LogicalPoint {
                x: -800.0,
                y: -120.0,
            };
            let logical = client_physical_to_logical(origin, scale, 150, 90);
            assert!((logical.x - (-800.0 + 150.0 / factor as f32)).abs() < 0.001);
            assert!((logical.y - (-120.0 + 90.0 / factor as f32)).abs() < 0.001);

            let layout = layout_menu(
                &RadialDocument::starter().menus[0],
                PhysicalPoint {
                    x: -1200.0,
                    y: 300.0,
                },
                PhysicalRect {
                    min: PhysicalPoint {
                        x: -1920.0,
                        y: -200.0,
                    },
                    max: PhysicalPoint { x: 0.0, y: 1080.0 },
                },
                scale,
                0.5,
            )
            .unwrap();
            let (x, y, width, height) = physical_scene_bounds(layout.visual_extent, scale);
            let expected_min = scale.logical_to_physical(layout.visual_extent.min);
            let expected_max = scale.logical_to_physical(layout.visual_extent.max);
            assert_eq!(
                (x, y),
                (expected_min.x.floor() as i32, expected_min.y.floor() as i32)
            );
            assert_eq!(width, (expected_max.x - expected_min.x).ceil() as i32);
            assert_eq!(height, (expected_max.y - expected_min.y).ceil() as i32);
        }
    }

    #[test]
    fn visual_overflow_does_not_claim_native_input_or_get_clipped_to_input_extent() {
        let mut document = RadialDocument::starter();
        document.menus[0].style.values.effects.glow_enabled =
            super::super::model::Override::Value(true);
        document.menus[0].style.values.text.font_size = super::super::model::Override::Value(28.0);
        let layout = super::super::geometry::layout_document_menu(
            &document,
            &document.menus[0],
            PhysicalPoint { x: 400.0, y: 400.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 900.0, y: 900.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        assert!(layout.visual_extent.min.x < layout.input_extent.min.x);
        let visual_only = LogicalPoint {
            x: (layout.visual_extent.min.x + layout.input_extent.min.x) * 0.5,
            y: layout.center.y,
        };
        assert!(!native_point_owned(&layout, visual_only));
        assert!(native_point_owned(&layout, layout.center));
    }

    #[test]
    fn present_updates_same_surface_without_factory_recreation_or_closed_event() {
        let creates = Arc::new(AtomicU32::new(0));
        let observed = Arc::clone(&creates);
        let mut host = NativeHost::spawn_with_factory(
            None,
            Arc::new(move |_, _, _, _, _, _| {
                observed.fetch_add(1, Ordering::Relaxed);
                Ok(PlatformSurface)
            }),
        )
        .unwrap();
        let layout = layout_menu(
            &RadialDocument::starter().menus[0],
            PhysicalPoint { x: 200.0, y: 200.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 500.0, y: 500.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        let id = SessionId::new("in-place");
        host.send(NativeCommand::Open {
            session_id: id.clone(),
            scene: super::super::render::build_scene(&layout, 1),
            layout: layout.clone(),
            always_on_top: false,
            activate_on_show: false,
        })
        .unwrap();
        assert!(matches!(
            host.recv().unwrap(),
            NativeEvent::Ready {
                layout_generation: 1,
                ..
            }
        ));
        host.send(NativeCommand::Present {
            session_id: id,
            scene: super::super::render::build_scene(&layout, 2),
            layout,
            always_on_top: false,
            activate_on_show: false,
        })
        .unwrap();
        assert!(matches!(
            host.recv().unwrap(),
            NativeEvent::Ready {
                layout_generation: 2,
                ..
            }
        ));
        assert_eq!(creates.load(Ordering::Relaxed), 1);
        assert!(host.try_recv().is_none());
        host.shutdown();
    }

    #[test]
    fn drag_control_is_the_only_actionable_owner_that_moves_the_host() {
        let mut document = RadialDocument::starter();
        document.menus[0].rings[0].cells[0].content = crate::radial::model::CellContent::Control {
            control: crate::radial::model::Control::Drag,
        };
        let mut layout = layout_menu(
            &document.menus[0],
            PhysicalPoint { x: 200.0, y: 200.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 500.0, y: 500.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        let drag = InputOwner::Actionable(layout.cells[0].cell_id.clone());
        let ordinary = InputOwner::Actionable(layout.cells[1].cell_id.clone());
        assert!(is_drag_owner(&layout, &drag, PointerButton::Primary));
        assert!(!is_drag_owner(&layout, &drag, PointerButton::Secondary));
        assert!(!is_drag_owner(&layout, &ordinary, PointerButton::Primary));
        assert!(!is_drag_owner(
            &layout,
            &InputOwner::Protective,
            PointerButton::Primary
        ));
        layout.cells[0].control = None;
        layout.cells[0].secondary_control = Some(crate::radial::model::Control::Drag);
        assert!(!is_drag_owner(&layout, &drag, PointerButton::Primary));
        assert!(is_drag_owner(&layout, &drag, PointerButton::Secondary));
    }
    #[test]
    #[ignore = "requires an interactive Windows desktop; verifies nonactivation and cross-process click-through"]
    fn live_radial_host_probe() {
        #[cfg(windows)]
        {
            assert_eq!(
                std::env::var("MULTI_LAUNCHER_RADIAL_LIVE_PROBE").as_deref(),
                Ok("1"),
                "set MULTI_LAUNCHER_RADIAL_LIVE_PROBE=1 explicitly"
            );
            use windows::Win32::Foundation::POINT;
            use windows::Win32::UI::WindowsAndMessaging::{
                GetForegroundWindow, IsWindowVisible, WindowFromPoint,
            };
            let layout = layout_menu(
                &RadialDocument::starter().menus[0],
                PhysicalPoint { x: 500.0, y: 500.0 },
                PhysicalRect {
                    min: PhysicalPoint { x: 0.0, y: 0.0 },
                    max: PhysicalPoint {
                        x: 1200.0,
                        y: 900.0,
                    },
                },
                ScaleFactor::new(1.0).unwrap(),
                0.5,
            )
            .unwrap();
            let foreground = unsafe { GetForegroundWindow() };
            let (events, _) = mpsc::channel();
            let surface = SystemPlatformSurface::create(
                SessionId::new("live-probe"),
                super::super::render::build_scene(&layout, 1),
                layout.clone(),
                true,
                false,
                HostEvents {
                    tx: events,
                    wake: None,
                },
            )
            .unwrap();
            assert!(unsafe { IsWindowVisible(surface.hwnd) }.as_bool());
            assert_eq!(
                unsafe { GetForegroundWindow() },
                foreground,
                "nonactivating open changed foreground"
            );
            let center = POINT {
                x: layout.center.x as i32,
                y: layout.center.y as i32,
            };
            assert_eq!(
                unsafe { WindowFromPoint(center) },
                surface.hwnd,
                "owned wheel center is not hit-testable"
            );
            let exterior = POINT {
                x: layout.input_extent.min.x as i32,
                y: layout.input_extent.min.y as i32,
            };
            assert_ne!(
                unsafe { WindowFromPoint(exterior) },
                surface.hwnd,
                "exterior corner did not pass through shaped host"
            );
            drop(surface);
        }
        #[cfg(not(windows))]
        panic!("Windows-only live probe");
    }
}
