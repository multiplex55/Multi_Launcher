use super::geometry::{LayoutSnapshot, LogicalPoint, PhysicalPoint};
use super::model::{CellId, SessionId};
use super::render::{InputOwner, VectorScene, input_owner};
use super::session::{NavigationCommand, NavigationModifiers, PointerButton};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{
    Arc,
    atomic::{AtomicU8, AtomicU32, Ordering},
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
    BeginSystemDrag {
        session_id: SessionId,
        layout_generation: u64,
    },
    Shutdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseReason {
    Dismissed,
    ActionHandoff,
    ExclusiveTool,
    SettingsReload,
    FeatureDisabled,
    DisplayRelayout,
    HostFailure,
    HookFailure,
    Suspend,
    SessionLock,
    DesktopUnavailable,
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
        layout_generation: u64,
        owner: InputOwner,
        point: LogicalPoint,
        button: PointerButton,
    },
    PointerMoved {
        session_id: SessionId,
        layout_generation: u64,
        owner: InputOwner,
        point: LogicalPoint,
    },
    PointerLeft {
        session_id: SessionId,
        layout_generation: u64,
    },
    PointerUp {
        session_id: SessionId,
        layout_generation: u64,
        owner: InputOwner,
        point: LogicalPoint,
        button: PointerButton,
    },
    /// Final surface relocation from one synchronous, user-approved system
    /// drag. Window-position messages update native state only; the controller
    /// accepts this correlated event and translates the logical session once.
    Relocated {
        session_id: SessionId,
        layout_generation: u64,
        from: PhysicalPoint,
        to: PhysicalPoint,
    },
    CaptureLost {
        session_id: SessionId,
        layout_generation: u64,
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

impl NativeEvent {
    pub(crate) fn session_id(&self) -> Option<&SessionId> {
        match self {
            Self::Ready { session_id, .. }
            | Self::Closed { session_id, .. }
            | Self::PointerDown { session_id, .. }
            | Self::PointerMoved { session_id, .. }
            | Self::PointerLeft { session_id, .. }
            | Self::PointerUp { session_id, .. }
            | Self::Relocated { session_id, .. }
            | Self::CaptureLost { session_id, .. }
            | Self::Escape { session_id }
            | Self::Navigate { session_id, .. }
            | Self::DisplayChanged { session_id } => Some(session_id),
            Self::Failed { session_id, .. } => session_id.as_ref(),
            Self::Stopped => None,
        }
    }
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
    reap_permit: Option<crate::thread_reaper::ReapPermit>,
    stopped: mpsc::Receiver<()>,
    command_sequence: AtomicU32,
    invalid_commands: Arc<AtomicU32>,
    lifecycle: Arc<AtomicU8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum HostLifecycle {
    Running = 0,
    Stopping = 1,
    Stopped = 2,
}

impl HostLifecycle {
    fn load(value: &AtomicU8) -> Self {
        match value.load(Ordering::Acquire) {
            0 => Self::Running,
            1 => Self::Stopping,
            _ => Self::Stopped,
        }
    }
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
type SuppressionFactory = Arc<dyn Fn() -> Box<dyn Send> + Send + Sync>;

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
        Self::spawn_with_factories(
            wake_sender,
            surface_factory,
            Arc::new(|| Box::new(crate::mouse_gestures::service::acquire_gesture_suppression())),
        )
    }

    fn spawn_with_factories(
        wake_sender: Option<mpsc::Sender<()>>,
        surface_factory: SurfaceFactory,
        suppression_factory: SuppressionFactory,
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
        let lifecycle = Arc::new(AtomicU8::new(HostLifecycle::Running as u8));
        let worker_lifecycle = Arc::clone(&lifecycle);
        let reap_permit = crate::thread_reaper::reserve()
            .map_err(|error| format!("failed to reserve radial host worker: {error}"))?;
        let completion_notifier = reap_permit.completion_notifier();
        let join = thread::Builder::new()
            .name("radial-native-host".into())
            .spawn(move || {
                let _completion_notifier = completion_notifier;
                let result = catch_unwind(AssertUnwindSafe(|| {
                    worker_loop(
                        rx,
                        event_tx.clone(),
                        &worker_wake,
                        start_tx,
                        worker_invalid_commands,
                        surface_factory,
                        suppression_factory,
                    )
                }));
                if result.is_err() {
                    event_tx.send(NativeEvent::Failed {
                        session_id: None,
                        message: "radial native host panicked".into(),
                    });
                }
                worker_lifecycle.store(HostLifecycle::Stopped as u8, Ordering::Release);
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
                // Queue shutdown even when the worker has not published its
                // thread id yet. The Windows worker drains commands before
                // entering GetMessage, so a late startup cannot become orphaned.
                let _ = commands.send((u32::MAX, NativeCommand::Shutdown));
                wake.quit();
                return match reap_permit.reap(join) {
                    Ok(()) => Err("radial host readiness timed out".into()),
                    Err(error) => Err(format!("radial host readiness timed out; {error}")),
                };
            }
        }
        Ok(Self {
            commands,
            events,
            wake,
            join: Some(join),
            reap_permit: Some(reap_permit),
            stopped,
            command_sequence: AtomicU32::new(0),
            invalid_commands,
            lifecycle,
        })
    }
    pub fn send(&self, command: NativeCommand) -> Result<(), String> {
        if HostLifecycle::load(&self.lifecycle) != HostLifecycle::Running {
            return Err("radial host is stopping".into());
        }
        self.enqueue(command)
    }
    fn enqueue(&self, command: NativeCommand) -> Result<(), String> {
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
        if self.join.is_none() {
            return;
        }
        if HostLifecycle::load(&self.lifecycle) == HostLifecycle::Running {
            self.lifecycle
                .store(HostLifecycle::Stopping as u8, Ordering::Release);
            let _ = self.enqueue(NativeCommand::Shutdown);
        }
        let mut stopped = self
            .stopped
            .recv_timeout(std::time::Duration::from_secs(2))
            .is_ok();
        if !stopped {
            self.wake.quit();
            stopped = self
                .stopped
                .recv_timeout(std::time::Duration::from_secs(2))
                .is_ok();
        }
        if let Some(join) = self.join.take() {
            if stopped || join.is_finished() {
                let _ = join.join();
                self.reap_permit.take();
            } else {
                if let Err(error) = self
                    .reap_permit
                    .take()
                    .expect("live radial host owns a reaper permit")
                    .reap(join)
                {
                    eprintln!("radial native host cleanup degraded: {error}");
                }
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
    layout_generation: u64,
    surface: PlatformSurface,
    _suppression: Box<dyn Send>,
}

fn handle(
    command: NativeCommand,
    active: &mut Option<Active>,
    events: &HostEvents,
    surface_factory: &SurfaceFactory,
    suppression_factory: &SuppressionFactory,
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
            // Suppression is part of startup ownership, not a post-ready side
            // effect. If any surface stage fails, this guard is dropped on the
            // same worker before failure is published.
            let suppression = suppression_factory();
            match surface_factory(
                session_id.clone(),
                scene.clone(),
                layout,
                always_on_top,
                activate_on_show,
                events.clone(),
            ) {
                Ok(surface) => {
                    *active = Some(Active {
                        id: session_id.clone(),
                        layout_generation: scene.generation,
                        surface,
                        _suppression: suppression,
                    });
                    let _ = events.send(NativeEvent::Ready {
                        session_id,
                        layout_generation: scene.generation,
                    });
                }
                Err(message) => {
                    drop(suppression);
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
            let current = active.as_mut().expect("validated active session");
            let result =
                current
                    .surface
                    .present(scene.clone(), layout, always_on_top, activate_on_show);
            match result {
                Ok(()) => {
                    current.layout_generation = scene.generation;
                    events.send(NativeEvent::Ready {
                        session_id,
                        layout_generation: scene.generation,
                    })
                }
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
        NativeCommand::BeginSystemDrag {
            session_id,
            layout_generation,
        } => {
            if let Some(current) = active.as_mut().filter(|current| {
                current.id == session_id && current.layout_generation == layout_generation
            }) {
                if let Err(message) = current.surface.begin_system_drag(layout_generation) {
                    events.send(NativeEvent::Failed {
                        session_id: Some(session_id),
                        message,
                    });
                }
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
    suppression_factory: &SuppressionFactory,
) -> bool {
    while let Ok((sequence, c)) = rx.try_recv() {
        if !command_is_valid(sequence, invalid_commands.load(Ordering::Acquire)) {
            continue;
        }
        if !handle(c, active, events, surface_factory, suppression_factory) {
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
    suppression_factory: SuppressionFactory,
) {
    let _ = start.send(Ok(()));
    let mut active = None;
    while let Ok((sequence, c)) = rx.recv() {
        if !command_is_valid(sequence, invalid_commands.load(Ordering::Acquire)) {
            continue;
        }
        if !handle(
            c,
            &mut active,
            &events,
            &surface_factory,
            &suppression_factory,
        ) {
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
    suppression_factory: SuppressionFactory,
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
        &suppression_factory,
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
                &suppression_factory,
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

    fn begin_system_drag(&mut self, _: u64) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(windows)]
struct SystemPlatformSurface {
    visual_hwnd: windows::Win32::Foundation::HWND,
    input_hwnd: windows::Win32::Foundation::HWND,
}
#[cfg(windows)]
unsafe impl Send for SystemPlatformSurface {}

#[cfg(windows)]
struct OwnedWindow(Option<windows::Win32::Foundation::HWND>);
#[cfg(windows)]
impl OwnedWindow {
    fn new(hwnd: windows::Win32::Foundation::HWND) -> Self {
        Self(Some(hwnd))
    }
    fn transfer(mut self) -> windows::Win32::Foundation::HWND {
        self.0
            .take()
            .expect("native window was already transferred")
    }
}
#[cfg(windows)]
impl Drop for OwnedWindow {
    fn drop(&mut self) {
        if let Some(hwnd) = self.0.take() {
            let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd) };
        }
    }
}
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
    capture: CaptureOwnership,
    destroyed: Arc<std::sync::atomic::AtomicBool>,
    presented: Arc<std::sync::atomic::AtomicBool>,
    activate_on_show: bool,
    compositor: super::compositor::CompositorCache,
    animation_epoch: std::time::Instant,
    animation_timer: Option<OwnedWindowTimer>,
    animation_serial: usize,
    visual_hwnd: windows::Win32::Foundation::HWND,
    input_hwnd: windows::Win32::Foundation::HWND,
    arrow_cursor: windows::Win32::UI::WindowsAndMessaging::HCURSOR,
    input_x: i32,
    input_y: i32,
    visual_x: i32,
    visual_y: i32,
    visual_offset_x: i32,
    visual_offset_y: i32,
}

#[cfg(windows)]
#[derive(Default)]
struct CaptureOwnership(bool);

#[cfg(windows)]
impl CaptureOwnership {
    fn acquire(&mut self, hwnd: windows::Win32::Foundation::HWND) -> bool {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetCapture, SetCapture};
        let _ = unsafe { SetCapture(hwnd) };
        self.0 = unsafe { GetCapture() } == hwnd;
        self.0
    }

    fn release(&mut self) {
        if self.0 {
            self.0 = false;
            let _ = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture() };
        }
    }

    fn is_active(&self) -> bool {
        self.0
    }
}

#[cfg(windows)]
impl Drop for CaptureOwnership {
    fn drop(&mut self) {
        let transition = radial_cursor_transition(CursorPolicyInput::Teardown {
            capture_active: self.is_active(),
        });
        if transition.release_capture {
            self.release();
        }
    }
}

#[cfg(windows)]
struct OwnedWindowTimer {
    hwnd: windows::Win32::Foundation::HWND,
    id: usize,
}

#[cfg(windows)]
impl Drop for OwnedWindowTimer {
    fn drop(&mut self) {
        let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::KillTimer(self.hwnd, self.id) };
    }
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
            if !ptr.is_null()
                && hwnd == unsafe { &*ptr }.input_hwnd
                && unsafe { &*ptr }.activate_on_show
            {
                MA_ACTIVATE as isize
            } else {
                MA_NOACTIVATE as isize
            },
        );
    }
    if msg == WM_SETCURSOR {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        let hit_test = (l.0 as u16 as i16) as i32;
        if !ptr.is_null()
            && radial_cursor_transition(CursorPolicyInput::SetCursor {
                is_input_window: hwnd == unsafe { &*ptr }.input_hwnd,
                hit_test,
            })
            .owns_arrow()
        {
            unsafe { SetCursor((*ptr).arrow_cursor) };
            return windows::Win32::Foundation::LRESULT(1);
        }
    }
    if msg == WM_NCHITTEST {
        use windows::Win32::Foundation::POINT;
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        if !ptr.is_null() {
            let state = unsafe { &*ptr };
            if hwnd == state.visual_hwnd {
                return windows::Win32::Foundation::LRESULT(HTTRANSPARENT as isize);
            }
            let (x, y) = signed_message_point(l);
            let mut point = POINT { x, y };
            if unsafe { windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut point) }.as_bool()
            {
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
        if !ptr.is_null() && hwnd == unsafe { &*ptr }.input_hwnd {
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
            if radial_cursor_transition(CursorPolicyInput::CapturedPointerMove {
                capture_active: state.capture.is_active(),
            })
            .owns_arrow()
            {
                unsafe { SetCursor(state.arrow_cursor) };
            }
            let point = client_physical_to_logical(state.origin, state.scale_factor, x, y);
            let owner = input_owner(&state.layout, point, false);
            let event = if msg == WM_LBUTTONDOWN || msg == WM_RBUTTONDOWN {
                if matches!(owner, InputOwner::Actionable(_)) {
                    if !unsafe { (*ptr).capture.acquire(hwnd) } {
                        state.events.send(NativeEvent::Failed {
                            session_id: Some(state.session_id.clone()),
                            message: "failed to acquire radial pointer capture".into(),
                        });
                        return windows::Win32::Foundation::LRESULT(0);
                    }
                }
                NativeEvent::PointerDown {
                    session_id: state.session_id.clone(),
                    layout_generation: state.scene.generation,
                    owner,
                    point,
                    button: if msg == WM_RBUTTONDOWN {
                        PointerButton::Secondary
                    } else {
                        PointerButton::Primary
                    },
                }
            } else if msg == WM_LBUTTONUP || msg == WM_RBUTTONUP {
                let transition = radial_cursor_transition(CursorPolicyInput::EndCapture {
                    capture_active: state.capture.is_active(),
                });
                if transition.release_capture {
                    unsafe { (*ptr).capture.release() };
                }
                NativeEvent::PointerUp {
                    session_id: state.session_id.clone(),
                    layout_generation: state.scene.generation,
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
                    layout_generation: state.scene.generation,
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
        if !ptr.is_null() && hwnd == unsafe { &*ptr }.input_hwnd {
            let state = unsafe { &*ptr };
            state.events.send(NativeEvent::PointerLeft {
                session_id: state.session_id.clone(),
                layout_generation: state.scene.generation,
            });
        }
        return windows::Win32::Foundation::LRESULT(0);
    }
    if msg == WM_CAPTURECHANGED || msg == WM_CANCELMODE {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        if !ptr.is_null()
            && hwnd == unsafe { &*ptr }.input_hwnd
            && unsafe { (*ptr).capture.is_active() }
        {
            unsafe { (*ptr).capture.0 = false };
            let state = unsafe { &*ptr };
            let _ = state.events.send(NativeEvent::CaptureLost {
                session_id: state.session_id.clone(),
                layout_generation: state.scene.generation,
            });
        }
    }
    if msg == WM_DISPLAYCHANGE || msg == WM_DPICHANGED {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        if !ptr.is_null() {
            let state = unsafe { &*ptr };
            if hwnd == state.input_hwnd || hwnd == state.visual_hwnd {
                let _ = state.events.send(NativeEvent::DisplayChanged {
                    session_id: state.session_id.clone(),
                });
            }
        }
    }
    if msg == WM_KEYDOWN && w.0 == 0x1B {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        if !ptr.is_null() && hwnd == unsafe { &*ptr }.input_hwnd {
            let state = unsafe { &*ptr };
            let _ = state.events.send(NativeEvent::Escape {
                session_id: state.session_id.clone(),
            });
        }
        return windows::Win32::Foundation::LRESULT(0);
    }
    if msg == WM_TIMER {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        if !ptr.is_null()
            && hwnd == unsafe { &*ptr }.visual_hwnd
            && unsafe { (*ptr).animation_timer.as_ref().map(|timer| timer.id) } == Some(w.0)
        {
            let state = unsafe { &mut *ptr };
            drop(state.animation_timer.take());
            let elapsed = state.animation_epoch.elapsed().as_millis() as u64;
            let scene = state.scene.clone();
            match state
                .compositor
                .compose(&scene, state.scale_factor, elapsed)
            {
                Ok(frame) => {
                    if let Err(message) =
                        present_layered(hwnd, state.visual_x, state.visual_y, &frame.image)
                    {
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
    if msg == WM_WINDOWPOSCHANGED {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        if !ptr.is_null() && hwnd == unsafe { &*ptr }.input_hwnd {
            let position = unsafe { &*(l.0 as *const WINDOWPOS) };
            if !position.flags.contains(SWP_NOMOVE) {
                let state = unsafe { &mut *ptr };
                state.input_x = position.x;
                state.input_y = position.y;
                state.visual_x = position.x.saturating_add(state.visual_offset_x);
                state.visual_y = position.y.saturating_add(state.visual_offset_y);
                let _ = unsafe {
                    SetWindowPos(
                        state.visual_hwnd,
                        windows::Win32::Foundation::HWND::default(),
                        state.visual_x,
                        state.visual_y,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
                    )
                };
            }
        }
    }
    if msg == WM_NCDESTROY {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
        if !ptr.is_null() && hwnd == unsafe { &*ptr }.visual_hwnd {
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
fn cancel_animation(_: windows::Win32::Foundation::HWND, state: &mut WindowState) {
    drop(state.animation_timer.take());
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
        state.animation_timer = Some(OwnedWindowTimer { hwnd, id: timer });
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct NativeSurfaceBounds {
    visual: (i32, i32, i32, i32),
    input: (i32, i32, i32, i32),
    input_origin: LogicalPoint,
}

fn native_surface_bounds(scene: &VectorScene, layout: &LayoutSnapshot) -> NativeSurfaceBounds {
    NativeSurfaceBounds {
        visual: physical_scene_bounds(scene.bounds, layout.scale_factor),
        input: physical_scene_bounds(layout.visual_extent, layout.scale_factor),
        input_origin: layout.visual_extent.min,
    }
}

fn radial_arrow_owned_client(is_input_window: bool, hit_test: i32) -> bool {
    const HTCLIENT_VALUE: i32 = 1;
    is_input_window && hit_test == HTCLIENT_VALUE
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CursorHookDecision {
    OwnArrow,
    Delegate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CursorPolicyInput {
    ClassRegistration,
    SetCursor {
        is_input_window: bool,
        hit_test: i32,
    },
    CapturedPointerMove {
        capture_active: bool,
    },
    EndCapture {
        capture_active: bool,
    },
    BeginSystemDrag {
        capture_active: bool,
    },
    Teardown {
        capture_active: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CursorTransition {
    decision: CursorHookDecision,
    release_capture: bool,
}

impl CursorTransition {
    fn owns_arrow(self) -> bool {
        matches!(self.decision, CursorHookDecision::OwnArrow)
    }
}

fn radial_cursor_transition(input: CursorPolicyInput) -> CursorTransition {
    match input {
        CursorPolicyInput::ClassRegistration => CursorTransition {
            decision: CursorHookDecision::OwnArrow,
            release_capture: false,
        },
        CursorPolicyInput::SetCursor {
            is_input_window,
            hit_test,
        } => CursorTransition {
            decision: if radial_arrow_owned_client(is_input_window, hit_test) {
                CursorHookDecision::OwnArrow
            } else {
                CursorHookDecision::Delegate
            },
            release_capture: false,
        },
        CursorPolicyInput::CapturedPointerMove { capture_active } => CursorTransition {
            decision: if capture_active {
                CursorHookDecision::OwnArrow
            } else {
                CursorHookDecision::Delegate
            },
            release_capture: false,
        },
        CursorPolicyInput::EndCapture { capture_active }
        | CursorPolicyInput::Teardown { capture_active } => CursorTransition {
            decision: CursorHookDecision::Delegate,
            release_capture: capture_active,
        },
        CursorPolicyInput::BeginSystemDrag { capture_active } => CursorTransition {
            decision: CursorHookDecision::Delegate,
            release_capture: capture_active,
        },
    }
}

fn radial_cursor_hook_decision(is_input_window: bool, hit_test: i32) -> CursorHookDecision {
    radial_cursor_transition(CursorPolicyInput::SetCursor {
        is_input_window,
        hit_test,
    })
    .decision
}

fn radial_capture_cursor_decision(
    capture_active: bool,
    system_drag_started: bool,
) -> CursorHookDecision {
    if system_drag_started {
        radial_cursor_transition(CursorPolicyInput::BeginSystemDrag { capture_active }).decision
    } else {
        radial_cursor_transition(CursorPolicyInput::CapturedPointerMove { capture_active }).decision
    }
}

#[derive(Clone, Debug, PartialEq)]
enum NativeRegionShape {
    Ellipse {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    },
    Polygon(Vec<(i32, i32)>),
}

#[derive(Clone, Debug, PartialEq)]
struct NativeInputRegionPlan {
    base: Vec<NativeRegionShape>,
    exclusions: Vec<NativeRegionShape>,
    overlays: Vec<NativeRegionShape>,
}

fn native_input_region_plan(
    layout: &LayoutSnapshot,
    window_origin: LogicalPoint,
) -> NativeInputRegionPlan {
    let convert = |shape: &super::geometry::HitShape| {
        native_region_shape(shape, window_origin, layout.scale_factor)
    };
    let base = if layout.style.fill_item_hit_zones {
        layout.input_regions.iter().map(convert).collect()
    } else {
        Vec::new()
    };
    let exclusions = if layout.style.fill_item_hit_zones && !layout.style.fill_center_hit_zone {
        vec![native_region_shape(
            &super::geometry::HitShape::Circle {
                center: layout.center,
                radius: layout.center_radius,
            },
            window_origin,
            layout.scale_factor,
        )]
    } else {
        Vec::new()
    };
    let mut overlays: Vec<_> = layout
        .cells
        .iter()
        .map(|cell| convert(&cell.shape))
        .collect();
    if layout.style.fill_center_hit_zone {
        overlays.push(native_region_shape(
            &super::geometry::HitShape::Circle {
                center: layout.center,
                radius: layout.center_radius,
            },
            window_origin,
            layout.scale_factor,
        ));
    }
    NativeInputRegionPlan {
        base,
        exclusions,
        overlays,
    }
}

fn native_region_shape(
    shape: &super::geometry::HitShape,
    origin: LogicalPoint,
    scale: super::geometry::ScaleFactor,
) -> NativeRegionShape {
    let physical = |point: LogicalPoint| {
        (
            ((point.x - origin.x) as f64 * scale.get()).round() as i32,
            ((point.y - origin.y) as f64 * scale.get()).round() as i32,
        )
    };
    match *shape {
        super::geometry::HitShape::Circle { center, radius } => {
            let left = ((center.x - radius - origin.x) as f64 * scale.get()).floor() as i32;
            let top = ((center.y - radius - origin.y) as f64 * scale.get()).floor() as i32;
            let right = ((center.x + radius - origin.x) as f64 * scale.get()).ceil() as i32;
            let bottom = ((center.y + radius - origin.y) as f64 * scale.get()).ceil() as i32;
            NativeRegionShape::Ellipse {
                left,
                top,
                right,
                bottom,
            }
        }
        super::geometry::HitShape::Wedge {
            center,
            inner_radius,
            outer_radius,
            start_angle,
            end_angle,
        } => {
            let raw_span = end_angle - start_angle;
            let span = if raw_span.abs() >= std::f32::consts::TAU - 0.00001 {
                std::f32::consts::TAU
            } else {
                raw_span.rem_euclid(std::f32::consts::TAU).max(0.0001)
            };
            let segments = (span / (std::f32::consts::PI / 36.0)).ceil() as usize;
            let segments = segments.clamp(2, 144);
            let radial_point = |radius: f32, angle: f32| LogicalPoint {
                x: center.x + radius * angle.cos(),
                y: center.y + radius * angle.sin(),
            };
            let mut points = Vec::with_capacity((segments + 1) * 2);
            for index in 0..=segments {
                let angle = start_angle + span * index as f32 / segments as f32;
                points.push(physical(radial_point(outer_radius, angle)));
            }
            for index in (0..=segments).rev() {
                let angle = start_angle + span * index as f32 / segments as f32;
                points.push(physical(radial_point(inner_radius, angle)));
            }
            NativeRegionShape::Polygon(points)
        }
    }
}

#[cfg(test)]
fn native_region_plan_contains(plan: &NativeInputRegionPlan, point: (i32, i32)) -> bool {
    fn contains(shape: &NativeRegionShape, point: (i32, i32)) -> bool {
        match shape {
            NativeRegionShape::Ellipse {
                left,
                top,
                right,
                bottom,
            } => {
                let rx = (*right - *left) as f64 * 0.5;
                let ry = (*bottom - *top) as f64 * 0.5;
                let cx = *left as f64 + rx;
                let cy = *top as f64 + ry;
                let dx = (point.0 as f64 + 0.5 - cx) / rx;
                let dy = (point.1 as f64 + 0.5 - cy) / ry;
                dx * dx + dy * dy <= 1.0
            }
            NativeRegionShape::Polygon(vertices) => {
                let (x, y) = (point.0 as f64 + 0.5, point.1 as f64 + 0.5);
                let mut inside = false;
                for index in 0..vertices.len() {
                    let (x1, y1) = vertices[index];
                    let (x2, y2) = vertices[(index + 1) % vertices.len()];
                    if ((y1 as f64 > y) != (y2 as f64 > y))
                        && x < (x2 - x1) as f64 * (y - y1 as f64) / (y2 - y1) as f64 + x1 as f64
                    {
                        inside = !inside;
                    }
                }
                inside
            }
        }
    }
    let base = plan.base.iter().any(|shape| contains(shape, point));
    let excluded = plan.exclusions.iter().any(|shape| contains(shape, point));
    (base && !excluded) || plan.overlays.iter().any(|shape| contains(shape, point))
}

#[cfg(windows)]
struct OwnedRegion(Option<windows::Win32::Graphics::Gdi::HRGN>);

#[cfg(windows)]
impl OwnedRegion {
    fn new(region: windows::Win32::Graphics::Gdi::HRGN) -> Result<Self, String> {
        if region.0.is_null() {
            Err("native region allocation failed".into())
        } else {
            Ok(Self(Some(region)))
        }
    }
    fn get(&self) -> windows::Win32::Graphics::Gdi::HRGN {
        self.0.expect("native region was already transferred")
    }
    fn transfer(mut self) {
        let _ = self.0.take();
    }
}

#[cfg(windows)]
impl Drop for OwnedRegion {
    fn drop(&mut self) {
        if let Some(region) = self.0.take() {
            let _ = unsafe { windows::Win32::Graphics::Gdi::DeleteObject(region) };
        }
    }
}

#[cfg(windows)]
fn create_native_input_region(plan: &NativeInputRegionPlan) -> Result<OwnedRegion, String> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::*;

    fn create_shape(shape: &NativeRegionShape) -> Result<OwnedRegion, String> {
        let region = match shape {
            NativeRegionShape::Ellipse {
                left,
                top,
                right,
                bottom,
            } => unsafe { CreateEllipticRgn(*left, *top, *right, *bottom) },
            NativeRegionShape::Polygon(points) => {
                let points: Vec<_> = points.iter().map(|&(x, y)| POINT { x, y }).collect();
                unsafe { CreatePolygonRgn(&points, WINDING) }
            }
        };
        OwnedRegion::new(region)
    }
    unsafe fn combine(
        destination: HRGN,
        shape: &NativeRegionShape,
        mode: RGN_COMBINE_MODE,
    ) -> bool {
        let Ok(part) = create_shape(shape) else {
            return false;
        };
        (unsafe { CombineRgn(destination, destination, part.get(), mode) }) != GDI_REGION_TYPE(0)
    }

    let region = OwnedRegion::new(unsafe { CreateRectRgn(0, 0, 0, 0) })
        .map_err(|_| "CreateRectRgn failed for radial input proxy".to_string())?;
    for shape in &plan.base {
        if !unsafe { combine(region.get(), shape, RGN_OR) } {
            return Err("failed to union radial input region".into());
        }
    }
    for shape in &plan.exclusions {
        if !unsafe { combine(region.get(), shape, RGN_DIFF) } {
            return Err("failed to subtract radial input exclusion".into());
        }
    }
    for shape in &plan.overlays {
        if !unsafe { combine(region.get(), shape, RGN_OR) } {
            return Err("failed to union radial input overlay".into());
        }
    }
    Ok(region)
}

#[cfg(windows)]
fn apply_native_input_region(
    hwnd: windows::Win32::Foundation::HWND,
    region: OwnedRegion,
) -> Result<(), String> {
    use windows::Win32::Graphics::Gdi::SetWindowRgn;
    if unsafe { SetWindowRgn(hwnd, region.get(), true) } == 0 {
        Err("SetWindowRgn failed for radial input proxy".into())
    } else {
        // SetWindowRgn transfers ownership to the system on success.
        region.transfer();
        Ok(())
    }
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
        use std::sync::OnceLock;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::*;
        use windows::core::PCWSTR;
        static REGISTER: OnceLock<Result<(), String>> = OnceLock::new();
        static ARROW: OnceLock<Result<usize, String>> = OnceLock::new();
        let class: Vec<u16> = "MultiLauncherRadialHost\0".encode_utf16().collect();
        let instance = unsafe { GetModuleHandleW(PCWSTR::null()) }.map_err(|e| e.to_string())?;
        let arrow_cursor = ARROW
            .get_or_init(|| {
                unsafe { LoadCursorW(None, IDC_ARROW) }
                    .map(|cursor| cursor.0 as usize)
                    .map_err(|error| format!("failed to load shared arrow cursor: {error}"))
            })
            .clone()?;
        let arrow_cursor = HCURSOR(arrow_cursor as *mut core::ffi::c_void);
        let class_cursor = radial_cursor_transition(CursorPolicyInput::ClassRegistration);
        REGISTER
            .get_or_init(|| {
                let wc = WNDCLASSW {
                    hInstance: instance.into(),
                    lpszClassName: PCWSTR(class.as_ptr()),
                    lpfnWndProc: Some(wndproc),
                    hCursor: if class_cursor.owns_arrow() {
                        arrow_cursor
                    } else {
                        HCURSOR::default()
                    },
                    ..Default::default()
                };
                if unsafe { RegisterClassW(&wc) } == 0 {
                    Err(format!(
                        "failed to register radial host window class: {}",
                        windows::core::Error::from_win32()
                    ))
                } else {
                    Ok(())
                }
            })
            .clone()?;
        let bounds = native_surface_bounds(&scene, &layout);
        let (visual_x, visual_y, visual_width, visual_height) = bounds.visual;
        let (input_x, input_y, input_width, input_height) = bounds.input;
        let input_origin = bounds.input_origin;
        let scale_factor = layout.scale_factor;
        let destroyed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let presented = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let state = Box::new(WindowState {
            session_id,
            layout,
            scene,
            events,
            origin: input_origin,
            scale_factor,
            capture: CaptureOwnership::default(),
            destroyed: Arc::clone(&destroyed),
            presented: Arc::clone(&presented),
            activate_on_show,
            compositor: super::compositor::CompositorCache::default(),
            animation_epoch: std::time::Instant::now(),
            animation_timer: None,
            animation_serial: 0,
            visual_hwnd: HWND::default(),
            input_hwnd: HWND::default(),
            arrow_cursor,
            input_x,
            input_y,
            visual_x,
            visual_y,
            visual_offset_x: visual_x.saturating_sub(input_x),
            visual_offset_y: visual_y.saturating_sub(input_y),
        });
        let ptr = Box::into_raw(state);
        let mut visual_ex = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE;
        if always_on_top {
            visual_ex |= WS_EX_TOPMOST;
        }
        let visual_hwnd = unsafe {
            CreateWindowExW(
                visual_ex,
                PCWSTR(class.as_ptr()),
                PCWSTR::null(),
                WS_POPUP,
                visual_x,
                visual_y,
                visual_width,
                visual_height,
                HWND::default(),
                None,
                instance,
                Some(ptr.cast()),
            )
        };
        let visual_hwnd = match visual_hwnd {
            Ok(h) => h,
            Err(e) => {
                if !destroyed.swap(true, Ordering::AcqRel) {
                    unsafe { drop(Box::from_raw(ptr)) };
                }
                return Err(format!("CreateWindowExW failed: {e}"));
            }
        };
        let visual_window = OwnedWindow::new(visual_hwnd);
        unsafe { (*ptr).visual_hwnd = visual_hwnd };
        let mut input_ex = WS_EX_LAYERED | WS_EX_TOOLWINDOW;
        if !activate_on_show {
            input_ex |= WS_EX_NOACTIVATE;
        }
        if always_on_top {
            input_ex |= WS_EX_TOPMOST;
        }
        let input_hwnd = match unsafe {
            CreateWindowExW(
                input_ex,
                PCWSTR(class.as_ptr()),
                PCWSTR::null(),
                WS_POPUP,
                input_x,
                input_y,
                input_width,
                input_height,
                visual_hwnd,
                None,
                instance,
                Some(ptr.cast()),
            )
        } {
            Ok(hwnd) => hwnd,
            Err(error) => {
                return Err(format!("input proxy CreateWindowExW failed: {error}"));
            }
        };
        let input_window = OwnedWindow::new(input_hwnd);
        unsafe { (*ptr).input_hwnd = input_hwnd };
        if let Err(error) = unsafe {
            SetLayeredWindowAttributes(
                input_hwnd,
                windows::Win32::Foundation::COLORREF(0),
                1,
                LWA_ALPHA,
            )
        } {
            return Err(format!("input proxy transparency failed: {error}"));
        }
        let region = match create_native_input_region(&native_input_region_plan(
            &unsafe { &*ptr }.layout,
            input_origin,
        )) {
            Ok(region) => region,
            Err(error) => {
                return Err(error);
            }
        };
        if let Err(error) = apply_native_input_region(input_hwnd, region) {
            return Err(error);
        }
        presented.store(false, Ordering::Release);
        let initial_scene = unsafe { (*ptr).scene.clone() };
        let frame = match unsafe { &mut *ptr }
            .compositor
            .compose(&initial_scene, scale_factor, 0)
        {
            Ok(frame) => frame,
            Err(error) => {
                return Err(format!("initial radial composition failed: {error:?}"));
            }
        };
        if let Err(error) = present_layered(visual_hwnd, visual_x, visual_y, &frame.image) {
            return Err(error);
        }
        presented.store(true, Ordering::Release);
        unsafe {
            let _ = ShowWindow(visual_hwnd, SW_SHOWNOACTIVATE);
            let _ = ShowWindow(
                input_hwnd,
                if activate_on_show {
                    SW_SHOW
                } else {
                    SW_SHOWNOACTIVATE
                },
            );
        }
        if let Err(error) = schedule_animation(
            visual_hwnd,
            unsafe { &mut *ptr },
            frame.animation_deadline_ms,
        ) {
            return Err(error);
        }
        if !presented.load(Ordering::Acquire) {
            return Err("initial radial presentation produced no frame".into());
        }
        let input_hwnd = input_window.transfer();
        let visual_hwnd = visual_window.transfer();
        Ok(Self {
            visual_hwnd,
            input_hwnd,
        })
    }

    fn present(
        &mut self,
        scene: VectorScene,
        layout: LayoutSnapshot,
        always_on_top: bool,
        activate_on_show: bool,
    ) -> Result<(), String> {
        use windows::Win32::UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetWindowLongPtrW, HWND_NOTOPMOST, HWND_TOPMOST, SW_HIDE, SW_SHOW,
            SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetWindowLongPtrW, SetWindowPos,
            ShowWindow, WS_EX_NOACTIVATE,
        };
        let bounds = native_surface_bounds(&scene, &layout);
        let (visual_x, visual_y, visual_width, visual_height) = bounds.visual;
        let (input_x, input_y, input_width, input_height) = bounds.input;
        let ptr = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                self.input_hwnd,
                windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            ) as *mut WindowState
        };
        if ptr.is_null() {
            return Err("radial window state is unavailable".into());
        }
        // The controller's layout is already in absolute desktop coordinates.
        // Carrying the HWND's previous displacement here would apply a drag a
        // second time after the controller has translated the session.
        let target_x = input_x;
        let target_y = input_y;
        cancel_animation(self.visual_hwnd, unsafe { &mut *ptr });
        unsafe { (*ptr).animation_epoch = std::time::Instant::now() };
        let frame = unsafe { &mut *ptr }
            .compositor
            .compose(&scene, layout.scale_factor, 0)
            .map_err(|error| format!("radial composition failed: {error:?}"))?;
        let origin = bounds.input_origin;
        let region = create_native_input_region(&native_input_region_plan(&layout, origin))?;
        let mut style = unsafe { GetWindowLongPtrW(self.input_hwnd, GWL_EXSTYLE) };
        if activate_on_show {
            style &= !(WS_EX_NOACTIVATE.0 as isize);
        } else {
            style |= WS_EX_NOACTIVATE.0 as isize;
        }
        unsafe { SetWindowLongPtrW(self.input_hwnd, GWL_EXSTYLE, style) };
        let _ = unsafe { ShowWindow(self.input_hwnd, SW_HIDE) };
        let input_positioned = unsafe {
            SetWindowPos(
                self.input_hwnd,
                if always_on_top {
                    HWND_TOPMOST
                } else {
                    HWND_NOTOPMOST
                },
                target_x,
                target_y,
                input_width,
                input_height,
                SWP_NOACTIVATE,
            )
        };
        if let Err(error) = input_positioned {
            return Err(format!("radial input proxy relayout failed: {error}"));
        }
        apply_native_input_region(self.input_hwnd, region)?;
        unsafe {
            SetWindowPos(
                self.visual_hwnd,
                if always_on_top {
                    HWND_TOPMOST
                } else {
                    HWND_NOTOPMOST
                },
                visual_x,
                visual_y,
                visual_width,
                visual_height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        }
        .map_err(|error| format!("radial relayout failed: {error}"))?;
        present_layered(self.visual_hwnd, visual_x, visual_y, &frame.image)?;
        unsafe {
            (*ptr).layout = layout;
            (*ptr).scene = scene;
            (*ptr).origin = origin;
            (*ptr).scale_factor = frame.scale_factor;
            (*ptr).activate_on_show = activate_on_show;
            (*ptr).input_x = target_x;
            (*ptr).input_y = target_y;
            (*ptr).visual_x = visual_x;
            (*ptr).visual_y = visual_y;
            (*ptr).visual_offset_x = visual_x.saturating_sub(target_x);
            (*ptr).visual_offset_y = visual_y.saturating_sub(target_y);
            (*ptr).presented.store(true, Ordering::Release);
        }
        let _ = unsafe {
            ShowWindow(
                self.input_hwnd,
                if activate_on_show {
                    SW_SHOW
                } else {
                    SW_SHOWNOACTIVATE
                },
            )
        };
        schedule_animation(
            self.visual_hwnd,
            unsafe { &mut *ptr },
            frame.animation_deadline_ms,
        )?;
        Ok(())
    }

    fn begin_system_drag(&mut self, layout_generation: u64) -> Result<(), String> {
        use windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            GWLP_USERDATA, GetCursorPos, GetWindowLongPtrW, HTCAPTION, SendMessageW,
            WM_NCLBUTTONDOWN,
        };
        let ptr = unsafe { GetWindowLongPtrW(self.input_hwnd, GWLP_USERDATA) as *mut WindowState };
        if ptr.is_null() || unsafe { &*ptr }.scene.generation != layout_generation {
            return Ok(());
        }
        let transition = radial_cursor_transition(CursorPolicyInput::BeginSystemDrag {
            capture_active: unsafe { &*ptr }.capture.is_active(),
        });
        if transition.release_capture {
            unsafe { (*ptr).capture.release() };
        }
        let mut cursor = POINT::default();
        unsafe { GetCursorPos(&mut cursor) }
            .map_err(|error| format!("failed to locate pointer for radial drag: {error}"))?;
        let packed = ((cursor.y as u32 & 0xffff) << 16) | (cursor.x as u32 & 0xffff);
        let from = {
            let state = unsafe { &*ptr };
            PhysicalPoint {
                x: f64::from(state.input_x),
                y: f64::from(state.input_y),
            }
        };
        unsafe {
            SendMessageW(
                self.input_hwnd,
                WM_NCLBUTTONDOWN,
                WPARAM(HTCAPTION as usize),
                LPARAM(packed as isize),
            )
        };
        let state = unsafe { &*ptr };
        let to = PhysicalPoint {
            x: f64::from(state.input_x),
            y: f64::from(state.input_y),
        };
        if to != from {
            state.events.send(NativeEvent::Relocated {
                session_id: state.session_id.clone(),
                layout_generation,
                from,
                to,
            });
        }
        state.events.send(NativeEvent::CaptureLost {
            session_id: state.session_id.clone(),
            layout_generation,
        });
        Ok(())
    }
}

#[cfg(windows)]
struct OwnedMemoryDc(windows::Win32::Graphics::Gdi::HDC);
#[cfg(windows)]
impl Drop for OwnedMemoryDc {
    fn drop(&mut self) {
        let _ = unsafe { windows::Win32::Graphics::Gdi::DeleteDC(self.0) };
    }
}

#[cfg(windows)]
struct OwnedBitmap(windows::Win32::Graphics::Gdi::HBITMAP);
#[cfg(windows)]
impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        let _ = unsafe { windows::Win32::Graphics::Gdi::DeleteObject(self.0) };
    }
}

#[cfg(windows)]
struct SelectedGdiObject {
    dc: windows::Win32::Graphics::Gdi::HDC,
    previous: windows::Win32::Graphics::Gdi::HGDIOBJ,
}
#[cfg(windows)]
impl Drop for SelectedGdiObject {
    fn drop(&mut self) {
        let _ = unsafe { windows::Win32::Graphics::Gdi::SelectObject(self.dc, self.previous) };
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
        CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, SelectObject,
    };
    use windows::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};
    let width = i32::try_from(image.width()).map_err(|_| "radial frame width is too large")?;
    let height = i32::try_from(image.height()).map_err(|_| "radial frame height is too large")?;
    let byte_len = image.as_raw().len();
    unsafe {
        let dc = OwnedMemoryDc(CreateCompatibleDC(None));
        if dc.0.0.is_null() {
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
        let bitmap = match CreateDIBSection(dc.0, &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(bitmap) if !bits.is_null() => bitmap,
            _ => {
                return Err("CreateDIBSection failed for radial frame".into());
            }
        };
        let bitmap = OwnedBitmap(bitmap);
        let old = SelectObject(dc.0, bitmap.0);
        if old.0.is_null() {
            return Err("SelectObject failed for radial frame".into());
        }
        let _selection = SelectedGdiObject {
            dc: dc.0,
            previous: old,
        };
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
                dc.0,
                Some(&source_point),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
            .map_err(|error| format!("UpdateLayeredWindow failed: {error}"))
        });
        updated
    }
}
#[cfg(windows)]
impl Drop for SystemPlatformSurface {
    fn drop(&mut self) {
        let ptr = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                self.input_hwnd,
                windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            ) as *mut WindowState
        };
        if !ptr.is_null() {
            cancel_animation(self.visual_hwnd, unsafe { &mut *ptr });
            let transition = radial_cursor_transition(CursorPolicyInput::Teardown {
                capture_active: unsafe { &*ptr }.capture.is_active(),
            });
            if transition.release_capture {
                unsafe { (*ptr).capture.release() };
            }
        }
        let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.input_hwnd) };
        let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.visual_hwnd) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::geometry::{PhysicalPoint, PhysicalRect, ScaleFactor, layout_menu};
    use crate::radial::model::RadialDocument;
    use std::sync::Mutex;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ConstructionStage {
        ClassRegistration,
        VisualWindow,
        InputWindow,
        RegionCreate,
        RegionCombine,
        RegionApply,
        MemoryDc,
        Dib,
        Select,
        Restore,
        LayeredUpdate,
        Capture,
        Timer,
        Hook,
        LifecycleWindow,
        SessionNotification,
        DesktopNotification,
        Suppression,
    }

    const CONSTRUCTION_STAGES: [ConstructionStage; 18] = [
        ConstructionStage::Suppression,
        ConstructionStage::LifecycleWindow,
        ConstructionStage::SessionNotification,
        ConstructionStage::DesktopNotification,
        ConstructionStage::Hook,
        ConstructionStage::ClassRegistration,
        ConstructionStage::VisualWindow,
        ConstructionStage::InputWindow,
        ConstructionStage::RegionCreate,
        ConstructionStage::RegionCombine,
        ConstructionStage::RegionApply,
        ConstructionStage::MemoryDc,
        ConstructionStage::Dib,
        ConstructionStage::Select,
        ConstructionStage::Restore,
        ConstructionStage::LayeredUpdate,
        ConstructionStage::Capture,
        ConstructionStage::Timer,
    ];

    #[derive(Default, Debug, PartialEq, Eq)]
    struct SyntheticResources {
        windows: usize,
        regions: usize,
        dcs: usize,
        bitmaps: usize,
        selections: usize,
        captures: usize,
        timers: usize,
        hooks: usize,
        notifications: usize,
        suppressions: usize,
    }

    #[derive(Clone, Copy)]
    enum SyntheticResource {
        Window,
        Region,
        Dc,
        Bitmap,
        Selection,
        Capture,
        Timer,
        Hook,
        Notification,
        Suppression,
    }

    struct SyntheticLease {
        resources: Arc<Mutex<SyntheticResources>>,
        resource: SyntheticResource,
    }
    impl SyntheticLease {
        fn acquire(
            resources: &Arc<Mutex<SyntheticResources>>,
            resource: SyntheticResource,
        ) -> Self {
            adjust_synthetic(&mut resources.lock().unwrap(), resource, 1);
            Self {
                resources: Arc::clone(resources),
                resource,
            }
        }
    }
    impl Drop for SyntheticLease {
        fn drop(&mut self) {
            adjust_synthetic(&mut self.resources.lock().unwrap(), self.resource, -1);
        }
    }

    fn adjust_synthetic(
        resources: &mut SyntheticResources,
        resource: SyntheticResource,
        delta: isize,
    ) {
        let slot = match resource {
            SyntheticResource::Window => &mut resources.windows,
            SyntheticResource::Region => &mut resources.regions,
            SyntheticResource::Dc => &mut resources.dcs,
            SyntheticResource::Bitmap => &mut resources.bitmaps,
            SyntheticResource::Selection => &mut resources.selections,
            SyntheticResource::Capture => &mut resources.captures,
            SyntheticResource::Timer => &mut resources.timers,
            SyntheticResource::Hook => &mut resources.hooks,
            SyntheticResource::Notification => &mut resources.notifications,
            SyntheticResource::Suppression => &mut resources.suppressions,
        };
        if delta >= 0 {
            *slot = slot
                .checked_add(delta as usize)
                .expect("synthetic resource overflow");
        } else {
            *slot = slot
                .checked_sub(delta.unsigned_abs())
                .expect("synthetic resource underflow");
        }
    }

    fn synthetic_construction(fail_at: Option<ConstructionStage>) -> (bool, SyntheticResources) {
        let resources = Arc::new(Mutex::new(SyntheticResources::default()));
        let mut leases = Vec::new();
        for stage in CONSTRUCTION_STAGES {
            if fail_at == Some(stage) {
                drop(leases);
                return (
                    false,
                    Arc::try_unwrap(resources).unwrap().into_inner().unwrap(),
                );
            }
            let resource = match stage {
                ConstructionStage::VisualWindow | ConstructionStage::InputWindow => {
                    Some(SyntheticResource::Window)
                }
                ConstructionStage::RegionCreate => Some(SyntheticResource::Region),
                ConstructionStage::MemoryDc => Some(SyntheticResource::Dc),
                ConstructionStage::Dib => Some(SyntheticResource::Bitmap),
                ConstructionStage::Select => Some(SyntheticResource::Selection),
                ConstructionStage::Capture => Some(SyntheticResource::Capture),
                ConstructionStage::Timer => Some(SyntheticResource::Timer),
                ConstructionStage::Hook => Some(SyntheticResource::Hook),
                ConstructionStage::SessionNotification | ConstructionStage::DesktopNotification => {
                    Some(SyntheticResource::Notification)
                }
                ConstructionStage::Suppression => Some(SyntheticResource::Suppression),
                _ => None,
            };
            if let Some(resource) = resource {
                leases.push(SyntheticLease::acquire(&resources, resource));
            }
        }
        drop(leases);
        (
            true,
            Arc::try_unwrap(resources).unwrap().into_inner().unwrap(),
        )
    }

    #[test]
    fn every_native_construction_stage_failure_withholds_ready_and_cleans_resources() {
        for stage in CONSTRUCTION_STAGES {
            let (ready, resources) = synthetic_construction(Some(stage));
            assert!(!ready, "{stage:?} unexpectedly reached Ready");
            assert_eq!(resources, SyntheticResources::default(), "{stage:?}");
        }
    }

    #[test]
    fn present_capture_and_animation_failures_leave_no_surface_or_input_proxy() {
        for stage in [
            ConstructionStage::LayeredUpdate,
            ConstructionStage::Capture,
            ConstructionStage::Timer,
        ] {
            let (ready, resources) = synthetic_construction(Some(stage));
            assert!(!ready, "{stage:?} unexpectedly published a surface");
            assert_eq!(resources.windows, 0, "{stage:?} retained an HWND");
            assert_eq!(resources.regions, 0, "{stage:?} retained an input region");
            assert_eq!(resources.captures, 0, "{stage:?} retained capture");
            assert_eq!(resources.timers, 0, "{stage:?} retained animation state");
        }
    }

    #[test]
    fn one_hundred_synthetic_construction_cycles_finish_with_zero_resources() {
        for cycle in 0..100 {
            let stage_index = cycle % (CONSTRUCTION_STAGES.len() + 1);
            let failure =
                (stage_index < CONSTRUCTION_STAGES.len()).then(|| CONSTRUCTION_STAGES[stage_index]);
            let (_, resources) = synthetic_construction(failure);
            assert_eq!(resources, SyntheticResources::default(), "cycle {cycle}");
        }
    }
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
    fn late_commands_after_close_and_shutdown_are_harmless() {
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
        let session_id = SessionId::new("retired");
        host.send(NativeCommand::Open {
            session_id: session_id.clone(),
            scene: super::super::render::build_scene(&layout, 41),
            layout,
            always_on_top: false,
            activate_on_show: false,
        })
        .unwrap();
        assert!(matches!(host.recv().unwrap(), NativeEvent::Ready { .. }));
        host.send(NativeCommand::Close {
            session_id: session_id.clone(),
            reason: CloseReason::Dismissed,
        })
        .unwrap();
        assert!(matches!(host.recv().unwrap(), NativeEvent::Closed { .. }));

        host.send(NativeCommand::BeginSystemDrag {
            session_id: session_id.clone(),
            layout_generation: 41,
        })
        .unwrap();
        host.send(NativeCommand::Close {
            session_id,
            reason: CloseReason::Dismissed,
        })
        .unwrap();

        host.shutdown();
        assert_eq!(host.try_recv(), Some(NativeEvent::Stopped));
        assert!(host.try_recv().is_none());
        assert!(host.send(NativeCommand::Shutdown).is_err());
        host.shutdown();
        assert_eq!(HostLifecycle::load(&host.lifecycle), HostLifecycle::Stopped);
    }

    #[test]
    fn one_hundred_host_cycles_stop_workers_and_release_suppression() {
        struct Suppression(Arc<AtomicU32>);
        impl Drop for Suppression {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::AcqRel);
            }
        }
        let live_suppressions = Arc::new(AtomicU32::new(0));
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
        for cycle in 0..100 {
            let count = Arc::clone(&live_suppressions);
            let mut host = NativeHost::spawn_with_factories(
                None,
                Arc::new(|_, _, _, _, _, _| Ok(PlatformSurface)),
                Arc::new(move || {
                    count.fetch_add(1, Ordering::AcqRel);
                    Box::new(Suppression(Arc::clone(&count)))
                }),
            )
            .unwrap();
            let session_id = SessionId::new(format!("cycle-{cycle}"));
            host.send(NativeCommand::Open {
                session_id: session_id.clone(),
                scene: super::super::render::build_scene(&layout, cycle + 1),
                layout: layout.clone(),
                always_on_top: false,
                activate_on_show: false,
            })
            .unwrap();
            assert!(matches!(host.recv().unwrap(), NativeEvent::Ready { .. }));
            host.send(NativeCommand::Close {
                session_id: session_id.clone(),
                reason: CloseReason::Dismissed,
            })
            .unwrap();
            assert!(matches!(host.recv().unwrap(), NativeEvent::Closed { .. }));
            host.send(NativeCommand::Close {
                session_id,
                reason: CloseReason::Dismissed,
            })
            .unwrap();
            host.shutdown();
            assert_eq!(host.try_recv(), Some(NativeEvent::Stopped));
            assert!(host.try_recv().is_none());
            host.shutdown();
            assert_eq!(HostLifecycle::load(&host.lifecycle), HostLifecycle::Stopped);
            assert_eq!(live_suppressions.load(Ordering::Acquire), 0);
        }
    }
    #[test]
    fn surface_failure_publishes_failed_without_ready() {
        struct CountedSuppression(Arc<AtomicU32>);
        impl Drop for CountedSuppression {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::AcqRel);
            }
        }
        let acquired = Arc::new(AtomicU32::new(0));
        let released = Arc::new(AtomicU32::new(0));
        let acquired_by_factory = Arc::clone(&acquired);
        let released_by_guard = Arc::clone(&released);
        let mut host = NativeHost::spawn_with_factories(
            None,
            Arc::new(|_, _, _, _, _, _| Err("surface-stage failure".into())),
            Arc::new(move || {
                acquired_by_factory.fetch_add(1, Ordering::AcqRel);
                Box::new(CountedSuppression(Arc::clone(&released_by_guard)))
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
        assert_eq!(acquired.load(Ordering::Acquire), 1);
        assert_eq!(released.load(Ordering::Acquire), 1);
        host.shutdown();
    }

    #[test]
    fn successful_session_acquires_and_releases_suppression_exactly_once() {
        struct CountedSuppression(Arc<AtomicU32>);
        impl Drop for CountedSuppression {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::AcqRel);
            }
        }
        let acquired = Arc::new(AtomicU32::new(0));
        let released = Arc::new(AtomicU32::new(0));
        let acquired_by_factory = Arc::clone(&acquired);
        let released_by_guard = Arc::clone(&released);
        let mut host = NativeHost::spawn_with_factories(
            None,
            Arc::new(|_, _, _, _, _, _| Ok(PlatformSurface)),
            Arc::new(move || {
                acquired_by_factory.fetch_add(1, Ordering::AcqRel);
                Box::new(CountedSuppression(Arc::clone(&released_by_guard)))
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
        let session_id = SessionId::new("suppressed");
        host.send(NativeCommand::Open {
            session_id: session_id.clone(),
            scene: super::super::render::build_scene(&layout, 5),
            layout,
            always_on_top: false,
            activate_on_show: false,
        })
        .unwrap();
        assert!(matches!(host.recv().unwrap(), NativeEvent::Ready { .. }));
        assert_eq!(acquired.load(Ordering::Acquire), 1);
        assert_eq!(released.load(Ordering::Acquire), 0);
        host.send(NativeCommand::Close {
            session_id,
            reason: CloseReason::Dismissed,
        })
        .unwrap();
        assert!(matches!(host.recv().unwrap(), NativeEvent::Closed { .. }));
        assert_eq!(released.load(Ordering::Acquire), 1);
        host.shutdown();
        assert_eq!(released.load(Ordering::Acquire), 1);
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
    fn visual_tooltip_overflow_does_not_resize_or_move_the_input_surface() {
        let document = RadialDocument::starter();
        let layout = super::super::geometry::layout_document_menu(
            &document,
            &document.menus[0],
            PhysicalPoint { x: 400.0, y: 400.0 },
            PhysicalRect {
                min: PhysicalPoint {
                    x: -600.0,
                    y: -300.0,
                },
                max: PhysicalPoint {
                    x: 1_200.0,
                    y: 900.0,
                },
            },
            ScaleFactor::new(1.5).unwrap(),
            0.5,
        )
        .unwrap();
        let scene = super::super::render::build_scene(&layout, 3);
        let original = native_surface_bounds(&scene, &layout);
        let mut expanded_scene = scene.clone();
        expanded_scene.bounds.min.x -= 120.0;
        expanded_scene.bounds.min.y -= 36.0;
        expanded_scene.bounds.max.x += 180.0;
        expanded_scene.bounds.max.y += 92.0;
        let expanded = native_surface_bounds(&expanded_scene, &layout);

        assert_eq!(original.input, expanded.input);
        assert_eq!(original.input_origin, expanded.input_origin);
        assert_ne!(original.visual, expanded.visual);
        assert_eq!(
            native_input_region_plan(&layout, expanded.input_origin),
            native_input_region_plan(&layout, original.input_origin)
        );
    }

    #[test]
    fn arrow_cursor_is_owned_only_for_the_input_clients_client_area() {
        assert!(radial_arrow_owned_client(true, 1));
        assert!(!radial_arrow_owned_client(false, 1));
        assert!(!radial_arrow_owned_client(true, 2));
        assert!(!radial_arrow_owned_client(true, -1));
    }

    #[test]
    fn cursor_policy_covers_client_ownership_delegation_and_capture_release() {
        let class = radial_cursor_transition(CursorPolicyInput::ClassRegistration);
        assert!(class.owns_arrow());
        assert!(!class.release_capture);
        let captured_move = radial_cursor_transition(CursorPolicyInput::CapturedPointerMove {
            capture_active: true,
        });
        assert!(captured_move.owns_arrow());
        assert!(!captured_move.release_capture);
        let end_capture = radial_cursor_transition(CursorPolicyInput::EndCapture {
            capture_active: true,
        });
        assert_eq!(end_capture.decision, CursorHookDecision::Delegate);
        assert!(end_capture.release_capture);
        let system_drag = radial_cursor_transition(CursorPolicyInput::BeginSystemDrag {
            capture_active: true,
        });
        assert_eq!(system_drag.decision, CursorHookDecision::Delegate);
        assert!(system_drag.release_capture);
        let idle_drag = radial_cursor_transition(CursorPolicyInput::BeginSystemDrag {
            capture_active: false,
        });
        assert_eq!(idle_drag.decision, CursorHookDecision::Delegate);
        assert!(!idle_drag.release_capture);
        let teardown = radial_cursor_transition(CursorPolicyInput::Teardown {
            capture_active: true,
        });
        assert_eq!(teardown.decision, CursorHookDecision::Delegate);
        assert!(teardown.release_capture);
        assert_eq!(
            radial_cursor_hook_decision(true, 1),
            CursorHookDecision::OwnArrow
        );
        assert_eq!(
            radial_cursor_hook_decision(false, 1),
            CursorHookDecision::Delegate,
            "the passive visual window delegates WM_SETCURSOR"
        );
        assert_eq!(
            radial_cursor_hook_decision(true, 2),
            CursorHookDecision::Delegate,
            "non-client/system-drag hit tests delegate"
        );
        assert_eq!(
            radial_capture_cursor_decision(true, false),
            CursorHookDecision::OwnArrow
        );
        assert_eq!(
            radial_capture_cursor_decision(false, true),
            CursorHookDecision::Delegate,
            "BeginSystemDrag releases capture before native non-client handling"
        );
    }

    #[test]
    fn native_cursor_paths_use_the_shared_transition_policy() {
        let production = include_str!("native.rs")
            .split("\n#[cfg(test)]\nmod tests")
            .next()
            .unwrap();
        for input in [
            "CursorPolicyInput::ClassRegistration",
            "CursorPolicyInput::SetCursor",
            "CursorPolicyInput::CapturedPointerMove",
            "CursorPolicyInput::EndCapture",
            "CursorPolicyInput::BeginSystemDrag",
            "CursorPolicyInput::Teardown",
        ] {
            assert!(
                production.contains(input),
                "production cursor path is not routed through {input}"
            );
        }
        assert!(production.contains("if transition.release_capture"));
        assert!(production.contains("hCursor: if class_cursor.owns_arrow()"));
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
    fn native_region_plan_uses_os_shapes_without_clipping_visual_overflow() {
        let mut document = RadialDocument::starter();
        document.menus[0].style.values.effects.glow_enabled =
            crate::radial::model::Override::Value(true);
        let layout = super::super::geometry::layout_document_menu(
            &document,
            &document.menus[0],
            PhysicalPoint {
                x: -900.25,
                y: 200.5,
            },
            PhysicalRect {
                min: PhysicalPoint {
                    x: -1920.0,
                    y: -200.0,
                },
                max: PhysicalPoint { x: 0.0, y: 900.0 },
            },
            ScaleFactor::new(1.5).unwrap(),
            0.5,
        )
        .unwrap();
        let plan = native_input_region_plan(&layout, layout.visual_extent.min);
        assert!(matches!(
            plan.base.as_slice(),
            [NativeRegionShape::Ellipse { .. }]
        ));
        assert!(layout.visual_extent.min.x < layout.input_extent.min.x);
        let local = |point: LogicalPoint| {
            (
                ((point.x - layout.visual_extent.min.x) as f64 * 1.5).round() as i32,
                ((point.y - layout.visual_extent.min.y) as f64 * 1.5).round() as i32,
            )
        };
        assert!(native_region_plan_contains(&plan, local(layout.center)));
        assert!(!native_region_plan_contains(
            &plan,
            local(layout.visual_extent.min)
        ));
    }

    #[test]
    fn native_region_plan_preserves_protected_gaps_and_optional_center_hole() {
        let mut layout = layout_menu(
            &RadialDocument::starter().menus[0],
            PhysicalPoint { x: 300.0, y: 300.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 700.0, y: 700.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        let origin = layout.visual_extent.min;
        let local = |point: LogicalPoint| {
            (
                (point.x - origin.x).round() as i32,
                (point.y - origin.y).round() as i32,
            )
        };
        let between_cells = LogicalPoint {
            x: layout.center.x,
            y: layout.center.y - layout.center_radius - 3.0,
        };
        let plan = native_input_region_plan(&layout, origin);
        assert!(native_region_plan_contains(&plan, local(between_cells)));
        assert!(!native_region_plan_contains(
            &plan,
            local(layout.input_extent.min)
        ));

        layout.style.fill_center_hit_zone = false;
        let plan = native_input_region_plan(&layout, origin);
        assert!(!native_region_plan_contains(&plan, local(layout.center)));
        let cell_center = match layout.cells[0].shape {
            super::super::geometry::HitShape::Circle { center, .. } => center,
            super::super::geometry::HitShape::Wedge {
                center,
                inner_radius,
                outer_radius,
                start_angle,
                end_angle,
            } => {
                let angle = (start_angle + end_angle) * 0.5;
                let radius = (inner_radius + outer_radius) * 0.5;
                LogicalPoint {
                    x: center.x + radius * angle.cos(),
                    y: center.y + radius * angle.sin(),
                }
            }
        };
        assert!(native_region_plan_contains(&plan, local(cell_center)));
    }

    #[test]
    fn native_event_routing_and_layout_hit_test_choose_the_same_topmost_overlap() {
        let mut layout = layout_menu(
            &RadialDocument::starter().menus[0],
            PhysicalPoint { x: 300.0, y: 300.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 600.0, y: 600.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        let point = match layout.cells[0].shape {
            super::super::geometry::HitShape::Circle { center, .. }
            | super::super::geometry::HitShape::Wedge { center, .. } => center,
        };
        let mut top = layout.cells[0].clone();
        top.cell_id = CellId::new("native-topmost");
        layout.cells.push(top.clone());
        assert_eq!(
            layout.hit_test(point).map(|cell| &cell.cell_id),
            Some(&top.cell_id)
        );
        assert_eq!(
            input_owner(&layout, point, false),
            InputOwner::Actionable(top.cell_id)
        );
    }

    #[test]
    fn wedge_input_regions_become_bounded_deterministic_polygons() {
        let shape = super::super::geometry::HitShape::Wedge {
            center: LogicalPoint { x: -10.0, y: 20.0 },
            inner_radius: 10.0,
            outer_radius: 30.0,
            start_angle: -0.25,
            end_angle: 1.25,
        };
        let converted = native_region_shape(
            &shape,
            LogicalPoint {
                x: -50.5,
                y: -10.25,
            },
            ScaleFactor::new(1.25).unwrap(),
        );
        let NativeRegionShape::Polygon(points) = converted else {
            panic!("wedge did not produce a polygon")
        };
        assert!(points.len() >= 8);
        assert_eq!(
            points,
            match native_region_shape(
                &shape,
                LogicalPoint {
                    x: -50.5,
                    y: -10.25
                },
                ScaleFactor::new(1.25).unwrap(),
            ) {
                NativeRegionShape::Polygon(points) => points,
                _ => unreachable!(),
            }
        );
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
            assert!(unsafe { IsWindowVisible(surface.visual_hwnd) }.as_bool());
            assert!(unsafe { IsWindowVisible(surface.input_hwnd) }.as_bool());
            assert_ne!(surface.visual_hwnd, surface.input_hwnd);
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
                surface.input_hwnd,
                "owned wheel center is not hit-testable"
            );
            let exterior = POINT {
                x: layout.input_extent.min.x as i32,
                y: layout.input_extent.min.y as i32,
            };
            assert_ne!(
                unsafe { WindowFromPoint(exterior) },
                surface.input_hwnd,
                "exterior corner did not pass through shaped host"
            );
            drop(surface);
        }
        #[cfg(not(windows))]
        panic!("Windows-only live probe");
    }
}
