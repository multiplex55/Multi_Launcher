use super::geometry::{LayoutSnapshot, LogicalPoint};
use super::model::{CellId, SessionId};
use super::render::{InputOwner, VectorScene, input_owner};
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
    },
    PointerMoved {
        session_id: SessionId,
        owner: InputOwner,
        point: LogicalPoint,
    },
    PointerUp {
        session_id: SessionId,
        owner: InputOwner,
        point: LogicalPoint,
    },
    CaptureLost {
        session_id: SessionId,
    },
    Escape {
        session_id: SessionId,
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
            Arc::new(|session_id, scene, layout, always_on_top, events| {
                PlatformSurface::create(session_id, scene, layout, always_on_top, events)
            }),
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
        _: HostEvents,
    ) -> Result<Self, String> {
        Ok(Self)
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
}

#[cfg(windows)]
unsafe extern "system" fn wndproc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    w: windows::Win32::Foundation::WPARAM,
    l: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::*;
    if msg == WM_NCCREATE {
        let cs = unsafe { &*(l.0 as *const CREATESTRUCTW) };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize) };
    }
    if msg == WM_MOUSEACTIVATE {
        return windows::Win32::Foundation::LRESULT(MA_NOACTIVATE as isize);
    }
    if msg == WM_PAINT {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        if !ptr.is_null() {
            paint_scene(hwnd, unsafe { &*ptr });
        }
        return windows::Win32::Foundation::LRESULT(0);
    }
    if msg == WM_MOUSEMOVE || msg == WM_LBUTTONDOWN || msg == WM_LBUTTONUP {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        if !ptr.is_null() {
            let (x, y) = signed_message_point(l);
            let state = unsafe { &*ptr };
            let point = client_physical_to_logical(state.origin, state.scale_factor, x, y);
            let owner = input_owner(&state.layout, point, false);
            let event = if msg == WM_LBUTTONDOWN {
                if matches!(owner, InputOwner::Actionable(_)) {
                    let _ =
                        unsafe { windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd) };
                    unsafe { (*ptr).captured = true };
                }
                NativeEvent::PointerDown {
                    session_id: state.session_id.clone(),
                    owner,
                    point,
                }
            } else if msg == WM_LBUTTONUP {
                if unsafe { (*ptr).captured } {
                    unsafe { (*ptr).captured = false };
                    let _ =
                        unsafe { windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture() };
                }
                NativeEvent::PointerUp {
                    session_id: state.session_id.clone(),
                    owner,
                    point,
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
    if msg == WM_NCDESTROY {
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
        if !ptr.is_null() {
            let state = unsafe { Box::from_raw(ptr) };
            state.destroyed.store(true, Ordering::Release);
            drop(state);
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

#[cfg(windows)]
fn paint_scene(hwnd: windows::Win32::Foundation::HWND, state: &WindowState) {
    use super::render::VectorPrimitive;
    use windows::Win32::Foundation::{COLORREF, RECT};
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, CreateSolidBrush, DeleteObject, Ellipse, EndPaint, FillRect, PAINTSTRUCT,
        Polygon, SelectObject,
    };
    let mut paint = PAINTSTRUCT::default();
    let dc = unsafe { BeginPaint(hwnd, &mut paint) };
    if dc.0.is_null() {
        return;
    }
    let background = unsafe { CreateSolidBrush(COLORREF(0x0021_211e)) };
    let mut client = RECT::default();
    let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client) };
    let _ = unsafe { FillRect(dc, &client, background) };
    let _ = unsafe { DeleteObject(background) };
    for primitive in &state.scene.primitives {
        let (color, draw): (_, Box<dyn FnOnce()>) = match primitive {
            VectorPrimitive::FilledCircle {
                center,
                radius,
                color,
            } => {
                let (cx, cy) = (
                    ((center.x - state.origin.x) as f64 * state.scale_factor.get()).round() as i32,
                    ((center.y - state.origin.y) as f64 * state.scale_factor.get()).round() as i32,
                );
                let r = (*radius as f64 * state.scale_factor.get()).round() as i32;
                let brush = unsafe { CreateSolidBrush(colorref(*color)) };
                let old = unsafe { SelectObject(dc, brush) };
                let draw = Box::new(move || {
                    let _ = unsafe { Ellipse(dc, cx - r, cy - r, cx + r, cy + r) };
                    let _ = unsafe { SelectObject(dc, old) };
                    let _ = unsafe { DeleteObject(brush) };
                });
                (*color, draw)
            }
            VectorPrimitive::FilledWedge {
                center,
                inner_radius,
                outer_radius,
                start_angle,
                end_angle,
                color,
                ..
            } => {
                let (cx, cy) = (
                    ((center.x - state.origin.x) as f64 * state.scale_factor.get()).round() as i32,
                    ((center.y - state.origin.y) as f64 * state.scale_factor.get()).round() as i32,
                );
                let mut span = (*end_angle - *start_angle).rem_euclid(std::f32::consts::TAU);
                if span == 0.0 {
                    span = std::f32::consts::TAU;
                }
                let steps = ((span / std::f32::consts::TAU) * 48.0).ceil().max(2.0) as usize;
                let mut points = Vec::with_capacity((steps + 1) * 2);
                for i in 0..=steps {
                    let a = *start_angle + span * i as f32 / steps as f32;
                    points.push(windows::Win32::Foundation::POINT {
                        x: cx
                            + (a.cos() as f64 * *outer_radius as f64 * state.scale_factor.get())
                                .round() as i32,
                        y: cy
                            + (a.sin() as f64 * *outer_radius as f64 * state.scale_factor.get())
                                .round() as i32,
                    });
                }
                for i in (0..=steps).rev() {
                    let a = *start_angle + span * i as f32 / steps as f32;
                    points.push(windows::Win32::Foundation::POINT {
                        x: cx
                            + (a.cos() as f64 * *inner_radius as f64 * state.scale_factor.get())
                                .round() as i32,
                        y: cy
                            + (a.sin() as f64 * *inner_radius as f64 * state.scale_factor.get())
                                .round() as i32,
                    });
                }
                let brush = unsafe { CreateSolidBrush(colorref(*color)) };
                let old = unsafe { SelectObject(dc, brush) };
                let draw = Box::new(move || {
                    let _ = unsafe { Polygon(dc, &points) };
                    let _ = unsafe { SelectObject(dc, old) };
                    let _ = unsafe { DeleteObject(brush) };
                });
                (*color, draw)
            }
            VectorPrimitive::Text { .. } => continue,
        };
        let _ = color;
        draw();
    }
    let _ = unsafe { EndPaint(hwnd, &paint) };
    state.presented.store(true, Ordering::Release);
}

#[cfg(windows)]
fn colorref(color: super::render::Rgba) -> windows::Win32::Foundation::COLORREF {
    windows::Win32::Foundation::COLORREF(
        color.0 as u32 | ((color.1 as u32) << 8) | ((color.2 as u32) << 16),
    )
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

fn physical_surface_bounds(layout: &LayoutSnapshot) -> (i32, i32, i32, i32) {
    let scale = layout.scale_factor;
    let min = scale.logical_to_physical(layout.input_extent.min);
    let max = scale.logical_to_physical(layout.input_extent.max);
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
        events: HostEvents,
    ) -> Result<Self, String> {
        use std::sync::Once;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Gdi::{
            CreateEllipticRgn, InvalidateRect, SetWindowRgn, UpdateWindow,
        };
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
        let (x, y, width, height) = physical_surface_bounds(&layout);
        let logical_origin = layout.input_extent.min;
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
        });
        let ptr = Box::into_raw(state);
        let mut ex = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
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
        let region = unsafe { CreateEllipticRgn(0, 0, width, height) };
        if region.0.is_null() {
            unsafe {
                let _ = DestroyWindow(hwnd);
            };
            return Err("CreateEllipticRgn failed".into());
        }
        if unsafe { SetWindowRgn(hwnd, region, true) } == 0 {
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::DeleteObject(region);
                let _ = DestroyWindow(hwnd);
            };
            return Err("SetWindowRgn failed".into());
        }
        if let Err(error) = unsafe {
            SetLayeredWindowAttributes(
                hwnd,
                windows::Win32::Foundation::COLORREF(0),
                245,
                LWA_ALPHA,
            )
        } {
            let _ = unsafe { DestroyWindow(hwnd) };
            return Err(format!("SetLayeredWindowAttributes failed: {error}"));
        }
        presented.store(false, Ordering::Release);
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        if unsafe { InvalidateRect(hwnd, None, true) }.0 == 0 {
            let _ = unsafe { DestroyWindow(hwnd) };
            return Err(format!(
                "initial radial invalidation failed: {}",
                windows::core::Error::from_win32()
            ));
        }
        let _ = unsafe { UpdateWindow(hwnd) };
        if !presented.load(Ordering::Acquire) {
            let _ = unsafe { DestroyWindow(hwnd) };
            return Err("initial radial presentation produced no frame".into());
        }
        Ok(Self { hwnd })
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
            Arc::new(|_, _, _, _, _| Err("surface-stage failure".into())),
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
            Arc::new(move |_, _, _, _, _| {
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
            let (x, y, width, height) = physical_surface_bounds(&layout);
            let expected_min = scale.logical_to_physical(layout.input_extent.min);
            let expected_max = scale.logical_to_physical(layout.input_extent.max);
            assert_eq!(
                (x, y),
                (expected_min.x.floor() as i32, expected_min.y.floor() as i32)
            );
            assert_eq!(width, (expected_max.x - expected_min.x).ceil() as i32);
            assert_eq!(height, (expected_max.y - expected_min.y).ceil() as i32);
        }
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
