use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
    mpsc,
};
use std::thread::JoinHandle;

use super::{
    CanvasBackground, HotkeyChord, RgbaColor, ScreenDrawMode, ScreenDrawSessionSnapshot,
    ScreenDrawTool,
};

const EMERGENCY_HOTKEY_ID: i32 = 0x5344;
#[cfg(windows)]
const WM_SCREEN_DRAW_COMMAND: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 0x34;

#[derive(Debug, Clone, PartialEq)]
pub enum NativeSessionCommand {
    SetTool(ScreenDrawTool),
    SetColor(RgbaColor),
    SetThickness(f32),
    Undo,
    Redo,
    Clear,
    SetAnnotationsVisible(bool),
    SetBackground(CanvasBackground),
    Ghost,
    Resume,
    Finish,
    RenderExport(ExportRenderRequest),
    Shutdown,
}

/// Placeholder carried by the M08 protocol. M13 will attach the concrete
/// compositor result/destination to this request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportRenderRequest {
    pub include_annotations: bool,
    pub background: CanvasBackground,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NativeSessionEvent {
    SessionStarted(NativeRuntimeState),
    ModeChanged(ScreenDrawMode),
    DocumentChanged,
    ToolChanged(ScreenDrawTool),
    ColorChanged(RgbaColor),
    ThicknessChanged(f32),
    AnnotationsVisibilityChanged(bool),
    BackgroundChanged(CanvasBackground),
    ExportRenderDeferred(ExportRenderRequest),
    EmergencyPaused,
    Warning(String),
    Error(String),
    SessionClosed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeRuntimeState {
    pub mode: ScreenDrawMode,
    pub tool: ScreenDrawTool,
    pub color: RgbaColor,
    pub thickness: f32,
    pub annotations_visible: bool,
    pub background: CanvasBackground,
}

impl NativeRuntimeState {
    fn initial(tool: ScreenDrawTool, color: RgbaColor, thickness: f32) -> Self {
        Self {
            mode: ScreenDrawMode::Drawing,
            tool,
            color,
            thickness,
            annotations_visible: true,
            background: CanvasBackground::FrozenDesktop,
        }
    }
}

pub(crate) struct NativeSessionConfig {
    pub snapshot: ScreenDrawSessionSnapshot,
    pub emergency_hotkey: HotkeyChord,
    pub tool: ScreenDrawTool,
    pub color: RgbaColor,
    pub thickness: f32,
    pub request_repaint: Arc<dyn Fn() + Send + Sync>,
}

pub(crate) trait NativeSessionFactory: Send + Sync {
    fn spawn(&self, config: NativeSessionConfig) -> Result<NativeSessionHandle, String>;
}

#[derive(Debug, Default)]
pub(crate) struct SystemNativeSessionFactory;

impl NativeSessionFactory for SystemNativeSessionFactory {
    fn spawn(&self, config: NativeSessionConfig) -> Result<NativeSessionHandle, String> {
        NativeSessionHandle::spawn(config)
    }
}

struct CommandWake {
    thread_id: AtomicU32,
}

impl CommandWake {
    fn new() -> Self {
        Self {
            thread_id: AtomicU32::new(0),
        }
    }

    fn signal(&self) {
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::{LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW;

            let thread_id = self.thread_id.load(Ordering::Acquire);
            if thread_id != 0 {
                let _ = unsafe {
                    PostThreadMessageW(thread_id, WM_SCREEN_DRAW_COMMAND, WPARAM(0), LPARAM(0))
                };
            }
        }
    }
}

/// Nonblocking owner of one native session thread. Command delivery signals a
/// native message queue; joining is deferred until `poll_finished` observes
/// that the worker has already exited.
pub struct NativeSessionHandle {
    command_tx: mpsc::Sender<NativeSessionCommand>,
    event_rx: mpsc::Receiver<NativeSessionEvent>,
    wake: Arc<CommandWake>,
    finished: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl fmt::Debug for NativeSessionHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeSessionHandle")
            .field("finished", &self.finished.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl NativeSessionHandle {
    fn spawn(config: NativeSessionConfig) -> Result<Self, String> {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let wake = Arc::new(CommandWake::new());
        let worker_wake = Arc::clone(&wake);
        let finished = Arc::new(AtomicBool::new(false));
        let worker_finished = Arc::clone(&finished);
        let join = std::thread::Builder::new()
            .name("screen-draw-native".into())
            .spawn(move || {
                let event_sink = EventSink::new(event_tx, Arc::clone(&config.request_repaint));
                let panic_events = event_sink.clone();
                let result = catch_unwind(AssertUnwindSafe(|| {
                    let mut core = match WorkerCore::production(&config, event_sink.clone()) {
                        Ok(core) => core,
                        Err(error) => {
                            event_sink.send(NativeSessionEvent::Error(error));
                            event_sink.send(NativeSessionEvent::SessionClosed);
                            return;
                        }
                    };
                    let result = run_platform_loop(
                        &mut core,
                        command_rx,
                        &worker_wake,
                        &config.emergency_hotkey,
                    );
                    finish_worker(&mut core, Ok(result));
                }));
                if result.is_err() {
                    panic_events.send(NativeSessionEvent::Error(
                        "Screen Draw native worker panicked".into(),
                    ));
                    panic_events.send(NativeSessionEvent::SessionClosed);
                }
                worker_finished.store(true, Ordering::Release);
            })
            .map_err(|error| format!("failed to start Screen Draw native worker: {error}"))?;
        Ok(Self {
            command_tx,
            event_rx,
            wake,
            finished,
            join: Some(join),
        })
    }

    pub fn send(&self, command: NativeSessionCommand) -> Result<(), String> {
        self.command_tx
            .send(command)
            .map_err(|_| "Screen Draw native worker is closed".to_string())?;
        self.wake.signal();
        Ok(())
    }

    pub fn try_recv(&self) -> Option<NativeSessionEvent> {
        self.event_rx.try_recv().ok()
    }

    pub fn poll_finished(&mut self) -> bool {
        if !self.finished.load(Ordering::Acquire) {
            return false;
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        true
    }

    pub fn request_shutdown(&self) {
        let _ = self.send(NativeSessionCommand::Shutdown);
    }

    #[cfg(test)]
    pub(crate) fn test_stub() -> (Self, mpsc::Receiver<NativeSessionCommand>) {
        let (handle, command_rx, _event_tx) = Self::test_stub_with_events();
        (handle, command_rx)
    }

    #[cfg(test)]
    pub(crate) fn test_stub_with_events() -> (
        Self,
        mpsc::Receiver<NativeSessionCommand>,
        mpsc::Sender<NativeSessionEvent>,
    ) {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        (
            Self {
                command_tx,
                event_rx,
                wake: Arc::new(CommandWake::new()),
                finished: Arc::new(AtomicBool::new(false)),
                join: None,
            },
            command_rx,
            event_tx,
        )
    }
}

impl Drop for NativeSessionHandle {
    fn drop(&mut self) {
        self.request_shutdown();
        // Never block an egui/drop path on an unhealthy native thread. A
        // completed worker is cheap to reap; otherwise detaching leaves its
        // own fail-safe shutdown command and panic guard responsible for disarm.
        let _ = self.poll_finished();
    }
}

#[derive(Clone)]
struct EventSink {
    tx: mpsc::Sender<NativeSessionEvent>,
    request_repaint: Arc<dyn Fn() + Send + Sync>,
}

impl EventSink {
    fn new(
        tx: mpsc::Sender<NativeSessionEvent>,
        request_repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            tx,
            request_repaint,
        }
    }

    fn send(&self, event: NativeSessionEvent) {
        let _ = self.tx.send(event);
        (self.request_repaint)();
    }
}

trait SuppressionLease: Send {}
impl SuppressionLease for crate::mouse_gestures::service::GestureSuppressionGuard {}

trait InteractiveSurface: Send {
    fn release_pointer_capture(&mut self);
    fn hide_interactive(&mut self);
    fn show_passive_annotations(&mut self);
    fn resume_interactive(&mut self);
    fn cancel_active(&mut self) -> bool {
        false
    }
    fn set_tool(&mut self, _tool: ScreenDrawTool) {}
    fn set_color(&mut self, _color: RgbaColor) {}
    fn set_thickness(&mut self, _thickness: f32) {}
    fn undo(&mut self) -> bool {
        false
    }
    fn redo(&mut self) -> bool {
        false
    }
    fn clear(&mut self) -> bool {
        false
    }
    fn set_annotations_visible(&mut self, _visible: bool) {}
    fn set_background(&mut self, _background: CanvasBackground) {}
}

#[derive(Default)]
struct PendingCanvasSurface;

impl InteractiveSurface for PendingCanvasSurface {
    fn release_pointer_capture(&mut self) {
        #[cfg(windows)]
        unsafe {
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
        }
    }

    fn hide_interactive(&mut self) {}
    fn show_passive_annotations(&mut self) {}
    fn resume_interactive(&mut self) {}
}

#[cfg(windows)]
impl InteractiveSurface for super::native_canvas::NativeCanvasSurface {
    fn release_pointer_capture(&mut self) {
        self.release_pointer_capture();
    }
    fn hide_interactive(&mut self) {
        self.hide();
    }
    fn show_passive_annotations(&mut self) {}
    fn resume_interactive(&mut self) {
        self.resume();
    }
    fn cancel_active(&mut self) -> bool {
        self.cancel_active()
    }
    fn set_color(&mut self, color: RgbaColor) {
        self.set_color(color);
    }
    fn set_tool(&mut self, tool: ScreenDrawTool) {
        self.set_tool(tool);
    }
    fn set_thickness(&mut self, thickness: f32) {
        self.set_thickness(thickness);
    }
    fn undo(&mut self) -> bool {
        self.undo()
    }
    fn redo(&mut self) -> bool {
        self.redo()
    }
    fn clear(&mut self) -> bool {
        self.clear()
    }
    fn set_annotations_visible(&mut self, visible: bool) {
        self.set_visible(visible);
    }
    fn set_background(&mut self, background: CanvasBackground) {
        self.set_background(background);
    }
}

struct WorkerCore {
    state: NativeRuntimeState,
    _snapshot: ScreenDrawSessionSnapshot,
    events: EventSink,
    surface: Box<dyn InteractiveSurface>,
    suppression: Option<Box<dyn SuppressionLease>>,
    suppression_factory: Arc<dyn Fn() -> Box<dyn SuppressionLease> + Send + Sync>,
    running: bool,
}

impl Drop for WorkerCore {
    fn drop(&mut self) {
        self.disarm_interactive();
    }
}

impl WorkerCore {
    fn production(config: &NativeSessionConfig, events: EventSink) -> Result<Self, String> {
        let suppression_factory: Arc<dyn Fn() -> Box<dyn SuppressionLease> + Send + Sync> =
            Arc::new(|| Box::new(crate::mouse_gestures::service::acquire_gesture_suppression()));
        let suppression = Some(suppression_factory());
        #[cfg(windows)]
        let surface: Box<dyn InteractiveSurface> = {
            use super::native_canvas::{CanvasEvent, NativeCanvasSurface};
            use windows::Win32::Foundation::{LPARAM, WPARAM};
            use windows::Win32::System::Threading::GetCurrentThreadId;
            use windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
            let canvas_events = events.clone();
            let thread_id = unsafe { GetCurrentThreadId() };
            Box::new(NativeCanvasSurface::create(
                config.snapshot.clone(),
                config.color,
                config.thickness,
                config.tool,
                Box::new(move |event| match event {
                    CanvasEvent::DocumentChanged => {
                        canvas_events.send(NativeSessionEvent::DocumentChanged)
                    }
                    CanvasEvent::Escape => {
                        let _ = unsafe {
                            PostThreadMessageW(
                                thread_id,
                                super::native_canvas::WM_CANVAS_ESCAPE,
                                WPARAM(0),
                                LPARAM(0),
                            )
                        };
                    }
                    CanvasEvent::DisplayChanged => {
                        let _ = unsafe {
                            PostThreadMessageW(
                                thread_id,
                                super::native_canvas::WM_CANVAS_DISPLAY_CHANGED,
                                WPARAM(0),
                                LPARAM(0),
                            )
                        };
                    }
                    CanvasEvent::Closed => {
                        let _ = unsafe {
                            PostThreadMessageW(
                                thread_id,
                                super::native_canvas::WM_CANVAS_CLOSED,
                                WPARAM(0),
                                LPARAM(0),
                            )
                        };
                    }
                }),
            )?)
        };
        #[cfg(not(windows))]
        let surface: Box<dyn InteractiveSurface> = Box::new(PendingCanvasSurface);
        Ok(Self {
            state: NativeRuntimeState::initial(config.tool, config.color, config.thickness),
            _snapshot: config.snapshot.clone(),
            events,
            surface,
            suppression,
            suppression_factory,
            running: true,
        })
    }

    fn handle(&mut self, command: NativeSessionCommand) {
        match command {
            NativeSessionCommand::SetTool(tool) => {
                self.state.tool = tool;
                self.surface.set_tool(tool);
                self.events.send(NativeSessionEvent::ToolChanged(tool));
            }
            NativeSessionCommand::SetColor(color) => {
                self.state.color = color;
                self.surface.set_color(color);
                self.events.send(NativeSessionEvent::ColorChanged(color));
            }
            NativeSessionCommand::SetThickness(thickness) => {
                if thickness.is_finite() && thickness > 0.0 {
                    self.state.thickness = thickness;
                    self.surface.set_thickness(thickness);
                    self.events
                        .send(NativeSessionEvent::ThicknessChanged(thickness));
                } else {
                    self.events.send(NativeSessionEvent::Warning(
                        "ignored invalid Screen Draw thickness".into(),
                    ));
                }
            }
            NativeSessionCommand::Undo => {
                if self.surface.undo() {
                    self.events.send(NativeSessionEvent::DocumentChanged);
                }
            }
            NativeSessionCommand::Redo => {
                if self.surface.redo() {
                    self.events.send(NativeSessionEvent::DocumentChanged);
                }
            }
            NativeSessionCommand::Clear => {
                if self.surface.clear() {
                    self.events.send(NativeSessionEvent::DocumentChanged);
                }
            }
            NativeSessionCommand::SetAnnotationsVisible(visible) => {
                self.state.annotations_visible = visible;
                self.surface.set_annotations_visible(visible);
                self.events
                    .send(NativeSessionEvent::AnnotationsVisibilityChanged(visible));
            }
            NativeSessionCommand::SetBackground(background) => {
                self.state.background = background;
                self.surface.set_background(background);
                self.events
                    .send(NativeSessionEvent::BackgroundChanged(background));
            }
            NativeSessionCommand::Ghost => self.safe_pause(false),
            NativeSessionCommand::Resume => {
                if self.suppression.is_none() {
                    self.suppression = Some((self.suppression_factory)());
                }
                self.surface.resume_interactive();
                self.state.mode = ScreenDrawMode::Drawing;
                self.events
                    .send(NativeSessionEvent::ModeChanged(ScreenDrawMode::Drawing));
            }
            NativeSessionCommand::Finish => {
                self.disarm_interactive();
                self.state.mode = ScreenDrawMode::Finish;
                self.events
                    .send(NativeSessionEvent::ModeChanged(ScreenDrawMode::Finish));
            }
            NativeSessionCommand::RenderExport(request) => self
                .events
                .send(NativeSessionEvent::ExportRenderDeferred(request)),
            NativeSessionCommand::Shutdown => {
                self.disarm_interactive();
                self.running = false;
            }
        }
    }

    fn safe_pause(&mut self, emergency: bool) {
        self.disarm_interactive();
        self.state.mode = ScreenDrawMode::Ghost;
        if emergency {
            self.events.send(NativeSessionEvent::EmergencyPaused);
        }
        self.events
            .send(NativeSessionEvent::ModeChanged(ScreenDrawMode::Ghost));
    }

    /// One idempotent safety path for Escape/emergency, mode changes, errors,
    /// panic cleanup, and shutdown. The pending surface implements the same
    /// contract before M09 creates concrete HWNDs.
    fn disarm_interactive(&mut self) {
        self.surface.cancel_active();
        self.surface.release_pointer_capture();
        self.surface.hide_interactive();
        self.surface.show_passive_annotations();
        self.suppression.take();
    }
}

fn drain_commands(core: &mut WorkerCore, command_rx: &mpsc::Receiver<NativeSessionCommand>) {
    while let Ok(command) = command_rx.try_recv() {
        core.handle(command);
        if !core.running {
            break;
        }
    }
}

fn finish_worker(core: &mut WorkerCore, result: std::thread::Result<Result<(), String>>) {
    core.disarm_interactive();
    match result {
        Ok(Ok(())) => {}
        Ok(Err(message)) => core.events.send(NativeSessionEvent::Error(message)),
        Err(_) => core.events.send(NativeSessionEvent::Error(
            "Screen Draw native worker panicked".into(),
        )),
    }
    core.events.send(NativeSessionEvent::SessionClosed);
}

fn report_hotkey_registration(
    events: &EventSink,
    emergency_hotkey: &HotkeyChord,
    result: Result<(), String>,
) -> bool {
    match result {
        Ok(()) => true,
        Err(error) => {
            events.send(NativeSessionEvent::Warning(format!(
                "emergency hotkey '{}' is unavailable: {error}",
                emergency_hotkey.as_str()
            )));
            false
        }
    }
}

#[cfg(not(windows))]
fn run_platform_loop(
    core: &mut WorkerCore,
    command_rx: mpsc::Receiver<NativeSessionCommand>,
    _wake: &CommandWake,
    _emergency_hotkey: &HotkeyChord,
) -> Result<(), String> {
    core.events
        .send(NativeSessionEvent::SessionStarted(core.state));
    while core.running {
        match command_rx.recv() {
            Ok(command) => core.handle(command),
            Err(_) => core.running = false,
        }
    }
    Ok(())
}

#[cfg(windows)]
fn run_platform_loop(
    core: &mut WorkerCore,
    command_rx: mpsc::Receiver<NativeSessionCommand>,
    wake: &CommandWake,
    emergency_hotkey: &HotkeyChord,
) -> Result<(), String> {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{HOT_KEY_MODIFIERS, RegisterHotKey};
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, TranslateMessage, WM_HOTKEY,
    };

    // Creating the queue before publishing the thread id prevents the classic
    // PostThreadMessage race where the first wake is lost.
    let mut message = MSG::default();
    let _ = unsafe { PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE) };
    wake.thread_id
        .store(unsafe { GetCurrentThreadId() }, Ordering::Release);

    struct RegisteredEmergencyHotkey(bool);
    impl Drop for RegisteredEmergencyHotkey {
        fn drop(&mut self) {
            if self.0 {
                let _ = unsafe {
                    windows::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey(
                        windows::Win32::Foundation::HWND::default(),
                        EMERGENCY_HOTKEY_ID,
                    )
                };
            }
        }
    }

    let mut registration = RegisteredEmergencyHotkey(false);
    match super::hotkeys::to_native_hotkey(emergency_hotkey) {
        Ok(hotkey) => {
            let result = unsafe {
                RegisterHotKey(
                    HWND::default(),
                    EMERGENCY_HOTKEY_ID,
                    HOT_KEY_MODIFIERS(hotkey.modifiers),
                    hotkey.virtual_key,
                )
            }
            .map_err(|error| error.to_string());
            registration.0 = report_hotkey_registration(&core.events, emergency_hotkey, result);
        }
        Err(error) => core.events.send(NativeSessionEvent::Warning(error)),
    }

    core.events
        .send(NativeSessionEvent::SessionStarted(core.state));
    drain_commands(core, &command_rx);
    while core.running {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 == -1 {
            wake.thread_id.store(0, Ordering::Release);
            return Err("GetMessageW failed in the Screen Draw native worker".into());
        }
        if result.0 == 0 {
            break;
        }
        if message.message == WM_SCREEN_DRAW_COMMAND {
            drain_commands(core, &command_rx);
        } else if message.message == super::native_canvas::WM_CANVAS_ESCAPE {
            core.safe_pause(false);
        } else if message.message == super::native_canvas::WM_CANVAS_DISPLAY_CHANGED {
            core.safe_pause(false);
            core.events.send(NativeSessionEvent::Warning(
                "display configuration changed; Screen Draw was safely paused".into(),
            ));
        } else if message.message == super::native_canvas::WM_CANVAS_CLOSED {
            core.disarm_interactive();
            core.running = false;
        } else if message.message == WM_HOTKEY
            && message.wParam == WPARAM(EMERGENCY_HOTKEY_ID as usize)
        {
            core.safe_pause(true);
        } else {
            let _ = unsafe { TranslateMessage(&message) };
            unsafe { DispatchMessageW(&message) };
        }
    }

    core.disarm_interactive();
    drop(registration);
    wake.thread_id.store(0, Ordering::Release);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::screen::CapturedRegion;
    use crate::screen_draw::ScreenDrawGeneration;
    use image::RgbaImage;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;

    struct CountingLease(Arc<AtomicUsize>);

    impl Drop for CountingLease {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl SuppressionLease for CountingLease {}

    #[derive(Default)]
    struct SurfaceCounts {
        cancel: usize,
        release: usize,
        hide: usize,
        passive: usize,
        resume: usize,
    }

    struct CountingSurface(Arc<Mutex<SurfaceCounts>>);

    impl InteractiveSurface for CountingSurface {
        fn cancel_active(&mut self) -> bool {
            self.0.lock().unwrap().cancel += 1;
            false
        }
        fn release_pointer_capture(&mut self) {
            self.0.lock().unwrap().release += 1;
        }
        fn hide_interactive(&mut self) {
            self.0.lock().unwrap().hide += 1;
        }
        fn show_passive_annotations(&mut self) {
            self.0.lock().unwrap().passive += 1;
        }
        fn resume_interactive(&mut self) {
            self.0.lock().unwrap().resume += 1;
        }
    }

    fn test_core() -> (
        WorkerCore,
        mpsc::Receiver<NativeSessionEvent>,
        Arc<Mutex<SurfaceCounts>>,
        Arc<AtomicUsize>,
    ) {
        let (event_tx, event_rx) = mpsc::channel();
        let surface = Arc::new(Mutex::new(SurfaceCounts::default()));
        let releases = Arc::new(AtomicUsize::new(0));
        let lease_factory_releases = Arc::clone(&releases);
        let suppression_factory: Arc<dyn Fn() -> Box<dyn SuppressionLease> + Send + Sync> =
            Arc::new(move || Box::new(CountingLease(Arc::clone(&lease_factory_releases))));
        let snapshot = ScreenDrawSessionSnapshot::new(
            ScreenDrawGeneration::from_raw(1),
            CapturedRegion {
                image: RgbaImage::new(1, 1),
                origin: (0, 0),
            },
        );
        let core = WorkerCore {
            state: NativeRuntimeState::initial(ScreenDrawTool::Pen, RgbaColor::RED, 3.0),
            _snapshot: snapshot,
            events: EventSink::new(event_tx, Arc::new(|| {})),
            surface: Box::new(CountingSurface(Arc::clone(&surface))),
            suppression: Some(suppression_factory()),
            suppression_factory,
            running: true,
        };
        (core, event_rx, surface, releases)
    }

    #[test]
    fn protocol_updates_mirrored_state_and_emits_low_frequency_events() {
        let (mut core, events, _, _) = test_core();
        core.handle(NativeSessionCommand::SetTool(ScreenDrawTool::Arrow));
        core.handle(NativeSessionCommand::SetColor(RgbaColor::WHITE));
        core.handle(NativeSessionCommand::SetThickness(8.0));
        core.handle(NativeSessionCommand::SetAnnotationsVisible(false));
        core.handle(NativeSessionCommand::SetBackground(CanvasBackground::Black));
        assert_eq!(core.state.tool, ScreenDrawTool::Arrow);
        assert_eq!(core.state.color, RgbaColor::WHITE);
        assert_eq!(core.state.thickness, 8.0);
        assert!(!core.state.annotations_visible);
        assert_eq!(core.state.background, CanvasBackground::Black);
        assert_eq!(events.try_iter().count(), 5);
    }

    #[test]
    fn central_disarm_is_idempotent_and_resume_reacquires_suppression() {
        let (mut core, _, surface, releases) = test_core();
        core.safe_pause(true);
        core.disarm_interactive();
        assert_eq!(releases.load(Ordering::SeqCst), 1);
        let counts = surface.lock().unwrap();
        assert_eq!(counts.cancel, 2);
        assert_eq!((counts.release, counts.hide, counts.passive), (2, 2, 2));
        drop(counts);

        core.handle(NativeSessionCommand::Resume);
        assert!(core.suppression.is_some());
        assert_eq!(surface.lock().unwrap().resume, 1);
        core.handle(NativeSessionCommand::Finish);
        assert_eq!(releases.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn shutdown_disarms_before_stopping_and_invalid_thickness_only_warns() {
        let (mut core, events, surface, releases) = test_core();
        core.handle(NativeSessionCommand::SetThickness(f32::NAN));
        assert!(matches!(
            events.recv().unwrap(),
            NativeSessionEvent::Warning(_)
        ));
        core.handle(NativeSessionCommand::Shutdown);
        assert!(!core.running);
        assert_eq!(releases.load(Ordering::SeqCst), 1);
        assert_eq!(surface.lock().unwrap().release, 1);
    }

    #[test]
    fn handle_shutdown_is_signal_only_until_worker_finishes() {
        let (handle, commands) = NativeSessionHandle::test_stub();
        handle.request_shutdown();
        assert_eq!(commands.recv().unwrap(), NativeSessionCommand::Shutdown);
    }

    #[test]
    fn emergency_hotkey_conflict_is_a_warning_and_does_not_stop_worker() {
        let (core, events, _, _) = test_core();
        let registered = report_hotkey_registration(
            &core.events,
            &HotkeyChord::from_unchecked("Ctrl+Shift+F12"),
            Err("already registered".into()),
        );
        assert!(!registered);
        assert!(core.running);
        assert!(matches!(
            events.recv().unwrap(),
            NativeSessionEvent::Warning(message) if message.contains("already registered")
        ));
    }

    #[test]
    fn panic_cleanup_disarms_and_emits_terminal_error_then_closed() {
        let (mut core, events, surface, releases) = test_core();
        let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
            panic!("fixture panic")
        }));
        finish_worker(&mut core, result);
        assert_eq!(releases.load(Ordering::SeqCst), 1);
        assert_eq!(surface.lock().unwrap().release, 1);
        assert!(matches!(
            events.recv().unwrap(),
            NativeSessionEvent::Error(message) if message.contains("panicked")
        ));
        assert_eq!(events.recv().unwrap(), NativeSessionEvent::SessionClosed);
    }

    #[test]
    fn every_worker_event_requests_repaint() {
        let count = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&count);
        let (tx, rx) = mpsc::channel();
        let sink = EventSink::new(
            tx,
            Arc::new(move || {
                observed.fetch_add(1, Ordering::SeqCst);
            }),
        );
        sink.send(NativeSessionEvent::DocumentChanged);
        sink.send(NativeSessionEvent::SessionClosed);
        assert_eq!(rx.try_iter().count(), 2);
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }
}
