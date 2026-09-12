use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
    mpsc,
};
use std::thread::JoinHandle;

use super::{
    CanvasBackground, ExportBackground, ExportOutcome, ExportRequest, ExportSource, RgbaColor,
    ScreenDrawGeneration, ScreenDrawMode, ScreenDrawSessionSnapshot, ScreenDrawSettings,
    ScreenDrawTool, ToolbarWindowInfo,
};

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
    SetToolbarWindow(Option<ToolbarWindowInfo>),
    Ghost,
    Resume,
    Finish,
    PrepareRegionSelection {
        generation: ScreenDrawGeneration,
        background: ExportBackground,
    },
    EndRegionSelection,
    DisplayChanged,
    EmergencyPause,
    RenderExport(ExportRenderRequest),
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportRenderRequest {
    pub generation: ScreenDrawGeneration,
    pub request: ExportRequest,
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
    ExportCompleted {
        generation: ScreenDrawGeneration,
        outcome: ExportOutcome,
    },
    ExportFailed {
        generation: ScreenDrawGeneration,
        message: String,
    },
    RegionPreviewReady {
        generation: ScreenDrawGeneration,
        bounds: crate::mkmacro::screen::ScreenRect,
    },
    RegionPreviewFailed {
        generation: ScreenDrawGeneration,
        message: String,
    },
    EditorImageReady {
        generation: ScreenDrawGeneration,
        image: image::RgbaImage,
    },
    EmergencyPaused,
    DisplayChanged,
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
    fn initial(
        tool: ScreenDrawTool,
        color: RgbaColor,
        thickness: f32,
        background: CanvasBackground,
    ) -> Self {
        Self {
            mode: ScreenDrawMode::Drawing,
            tool,
            color,
            thickness,
            annotations_visible: true,
            background,
        }
    }
}

pub(crate) struct NativeSessionConfig {
    pub snapshot: ScreenDrawSessionSnapshot,
    pub tool: ScreenDrawTool,
    pub color: RgbaColor,
    pub thickness: f32,
    pub settings: ScreenDrawSettings,
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

    fn signal(&self) -> Result<(), String> {
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::{LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW;

            let thread_id = self.thread_id.load(Ordering::Acquire);
            if thread_id != 0 {
                unsafe {
                    PostThreadMessageW(thread_id, WM_SCREEN_DRAW_COMMAND, WPARAM(0), LPARAM(0))
                }
                .map_err(|error| format!("failed to wake Screen Draw native worker: {error}"))?;
            }
        }
        Ok(())
    }

    fn force_quit(&self) {
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::{LPARAM, WPARAM};
            use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};

            let thread_id = self.thread_id.load(Ordering::Acquire);
            if thread_id != 0 {
                // Last-resort wake for Drop/shutdown. WM_QUIT still unwinds the
                // worker normally, so its surface and suppression guards run.
                let _ = unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
            }
        }
    }
}

fn fail_closed_on_wake_error(
    wake_result: Result<(), String>,
    force_quit: impl FnOnce(),
) -> Result<(), String> {
    if wake_result.is_err() {
        force_quit();
    }
    wake_result
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

#[derive(Clone)]
pub struct NativeEmergencyHandle {
    command_tx: mpsc::Sender<NativeSessionCommand>,
    wake: Arc<CommandWake>,
}

impl fmt::Debug for NativeEmergencyHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeEmergencyHandle")
            .finish_non_exhaustive()
    }
}

impl NativeEmergencyHandle {
    pub fn emergency_pause(&self) -> Result<(), String> {
        self.command_tx
            .send(NativeSessionCommand::EmergencyPause)
            .map_err(|_| "Screen Draw native worker is closed".to_string())?;
        fail_closed_on_wake_error(self.wake.signal(), || self.wake.force_quit())
    }
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
                    let result = run_platform_loop(&mut core, command_rx, &worker_wake);
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
        fail_closed_on_wake_error(self.wake.signal(), || self.wake.force_quit())
    }

    pub fn emergency_handle(&self) -> NativeEmergencyHandle {
        NativeEmergencyHandle {
            command_tx: self.command_tx.clone(),
            wake: Arc::clone(&self.wake),
        }
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
    fn show_passive_annotations(&mut self, visible: bool);
    fn hide_passive_annotations(&mut self);
    fn show_export_preview(&mut self, _image: &image::RgbaImage) -> Result<(), String> {
        Err("Screen Draw export preview surface is not ready".into())
    }
    fn destroy_surfaces(&mut self);
    fn resume_interactive(&mut self);
    fn set_toolbar_window(&mut self, _info: Option<ToolbarWindowInfo>) {}
    fn annotations_visible(&self) -> bool {
        true
    }
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
    fn export_source(&self) -> Result<ExportSource, String> {
        Err("Screen Draw export surface is not ready".into())
    }
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
    fn show_passive_annotations(&mut self, _visible: bool) {}
    fn hide_passive_annotations(&mut self) {}
    fn destroy_surfaces(&mut self) {}
    fn resume_interactive(&mut self) {}
    fn set_toolbar_window(&mut self, _info: Option<ToolbarWindowInfo>) {}
}

#[cfg(windows)]
impl InteractiveSurface for super::native_canvas::NativeCanvasSurface {
    fn release_pointer_capture(&mut self) {
        self.release_pointer_capture();
    }
    fn hide_interactive(&mut self) {
        self.hide();
    }
    fn show_passive_annotations(&mut self, visible: bool) {
        self.show_passive(visible);
    }
    fn hide_passive_annotations(&mut self) {
        self.hide_passive();
    }
    fn show_export_preview(&mut self, image: &image::RgbaImage) -> Result<(), String> {
        self.show_export_preview(image)
    }
    fn destroy_surfaces(&mut self) {
        self.destroy_surfaces();
    }
    fn resume_interactive(&mut self) {
        self.resume();
    }
    fn set_toolbar_window(&mut self, info: Option<ToolbarWindowInfo>) {
        self.set_toolbar_window(info);
    }
    fn annotations_visible(&self) -> bool {
        self.annotations_visible()
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
    fn export_source(&self) -> Result<ExportSource, String> {
        Ok(self.export_source())
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
    export_destination: Arc<dyn super::export::ExportDestinationBackend>,
}

impl Drop for WorkerCore {
    fn drop(&mut self) {
        self.shutdown_surfaces();
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
                config.settings.clone(),
                Box::new(move |event| match event {
                    CanvasEvent::DocumentChanged => {
                        canvas_events.send(NativeSessionEvent::DocumentChanged)
                    }
                    CanvasEvent::ToolChanged(tool) => {
                        canvas_events.send(NativeSessionEvent::ToolChanged(tool))
                    }
                    CanvasEvent::ColorChanged(color) => {
                        canvas_events.send(NativeSessionEvent::ColorChanged(color))
                    }
                    CanvasEvent::ThicknessChanged(thickness) => {
                        canvas_events.send(NativeSessionEvent::ThicknessChanged(thickness))
                    }
                    CanvasEvent::AnnotationsVisibilityChanged(visible) => canvas_events
                        .send(NativeSessionEvent::AnnotationsVisibilityChanged(visible)),
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
            state: NativeRuntimeState::initial(
                config.tool,
                config.color,
                config.thickness,
                config.settings.default_background,
            ),
            _snapshot: config.snapshot.clone(),
            events,
            surface,
            suppression,
            suppression_factory,
            running: true,
            export_destination: Arc::new(super::export::SystemExportDestination),
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
            NativeSessionCommand::SetToolbarWindow(info) => {
                self.surface.set_toolbar_window(info);
            }
            NativeSessionCommand::Ghost => {
                if self.state.mode == ScreenDrawMode::Drawing {
                    self.safe_pause(false);
                }
            }
            NativeSessionCommand::Resume => {
                if !matches!(
                    self.state.mode,
                    ScreenDrawMode::Ghost | ScreenDrawMode::Finish
                ) {
                    self.events.send(NativeSessionEvent::Warning(
                        "ignored resume because Screen Draw surfaces are not resumable".into(),
                    ));
                    return;
                }
                if self.suppression.is_none() {
                    self.suppression = Some((self.suppression_factory)());
                }
                self.surface.resume_interactive();
                self.state.mode = ScreenDrawMode::Drawing;
                self.events
                    .send(NativeSessionEvent::ModeChanged(ScreenDrawMode::Drawing));
            }
            NativeSessionCommand::Finish => {
                if !matches!(
                    self.state.mode,
                    ScreenDrawMode::Drawing | ScreenDrawMode::Ghost
                ) {
                    return;
                }
                self.pause_to_passive();
                self.state.mode = ScreenDrawMode::Finish;
                self.events
                    .send(NativeSessionEvent::ModeChanged(ScreenDrawMode::Finish));
            }
            NativeSessionCommand::PrepareRegionSelection {
                generation,
                background,
            } => self.prepare_region_selection(generation, background),
            NativeSessionCommand::EndRegionSelection => self.end_region_selection(),
            NativeSessionCommand::DisplayChanged => self.display_changed(),
            NativeSessionCommand::EmergencyPause => self.safe_pause(true),
            NativeSessionCommand::RenderExport(request) => self.start_export(request),
            NativeSessionCommand::Shutdown => {
                self.shutdown_surfaces();
                self.running = false;
            }
        }
    }

    fn start_export(&mut self, request: ExportRenderRequest) {
        if !matches!(
            self.state.mode,
            ScreenDrawMode::Finish | ScreenDrawMode::DisplayChanged
        ) || request.generation != self._snapshot.generation()
        {
            self.events.send(NativeSessionEvent::ExportFailed {
                generation: request.generation,
                message: "ignored stale or invalid Screen Draw export request".into(),
            });
            return;
        }
        let source = match self.surface.export_source() {
            Ok(source) => source,
            Err(message) => {
                self.events.send(NativeSessionEvent::ExportFailed {
                    generation: request.generation,
                    message,
                });
                return;
            }
        };
        let events = self.events.clone();
        let destination = Arc::clone(&self.export_destination);
        let generation = request.generation;
        let spawn = std::thread::Builder::new()
            .name(format!("screen-draw-export-{}", generation.get()))
            .spawn(move || {
                if request.request.destination == super::ExportDestination::ScreenshotEditor {
                    match super::export::compose_export(request.request, &source) {
                        Ok(image) => {
                            events.send(NativeSessionEvent::EditorImageReady { generation, image })
                        }
                        Err(message) => events.send(NativeSessionEvent::ExportFailed {
                            generation,
                            message,
                        }),
                    }
                    return;
                }
                match super::export::execute_export(request.request, source, destination) {
                    Ok(outcome) => events.send(NativeSessionEvent::ExportCompleted {
                        generation,
                        outcome,
                    }),
                    Err(message) => events.send(NativeSessionEvent::ExportFailed {
                        generation,
                        message,
                    }),
                }
            });
        if let Err(error) = spawn {
            self.events.send(NativeSessionEvent::ExportFailed {
                generation,
                message: format!("failed to start Screen Draw export worker: {error}"),
            });
        }
    }

    fn prepare_region_selection(
        &mut self,
        generation: ScreenDrawGeneration,
        background: ExportBackground,
    ) {
        if self.state.mode != ScreenDrawMode::Finish || generation != self._snapshot.generation() {
            self.events.send(NativeSessionEvent::RegionPreviewFailed {
                generation,
                message: "ignored stale or invalid Screen Draw region request".into(),
            });
            return;
        }
        let source = match self.surface.export_source() {
            Ok(source) => source,
            Err(message) => {
                self.events.send(NativeSessionEvent::RegionPreviewFailed {
                    generation,
                    message,
                });
                return;
            }
        };
        let capture = source.snapshot.capture();
        let bounds = crate::mkmacro::screen::ScreenRect::new(
            capture.origin.0,
            capture.origin.1,
            capture.image.width(),
            capture.image.height(),
        );
        let request = ExportRequest {
            scope: super::ExportScope::FullDesktop,
            background,
            destination: super::ExportDestination::ScreenshotEditor,
        };
        let image = match super::export::compose_export(request, &source) {
            Ok(image) => image,
            Err(message) => {
                self.events.send(NativeSessionEvent::RegionPreviewFailed {
                    generation,
                    message,
                });
                return;
            }
        };
        self.surface.hide_passive_annotations();
        match self.surface.show_export_preview(&image) {
            Ok(()) => {
                self.state.mode = ScreenDrawMode::SelectingRegion;
                self.events
                    .send(NativeSessionEvent::RegionPreviewReady { generation, bounds });
            }
            Err(message) => {
                self.surface
                    .show_passive_annotations(self.state.annotations_visible);
                self.events.send(NativeSessionEvent::RegionPreviewFailed {
                    generation,
                    message,
                });
            }
        }
    }

    fn end_region_selection(&mut self) {
        if self.state.mode != ScreenDrawMode::SelectingRegion {
            return;
        }
        self.surface.hide_passive_annotations();
        self.surface
            .show_passive_annotations(self.state.annotations_visible);
        self.state.mode = ScreenDrawMode::Finish;
    }

    fn safe_pause(&mut self, emergency: bool) {
        if self.state.mode == ScreenDrawMode::DisplayChanged {
            self.shutdown_surfaces();
            if emergency {
                self.events.send(NativeSessionEvent::EmergencyPaused);
            }
            return;
        }
        self.pause_to_passive();
        self.state.mode = ScreenDrawMode::Ghost;
        if emergency {
            self.events.send(NativeSessionEvent::EmergencyPaused);
        }
        self.events
            .send(NativeSessionEvent::ModeChanged(ScreenDrawMode::Ghost));
    }

    fn release_interactive(&mut self) {
        self.surface.cancel_active();
        self.surface.release_pointer_capture();
        self.surface.hide_interactive();
        self.suppression.take();
    }

    fn pause_to_passive(&mut self) {
        self.release_interactive();
        self.state.annotations_visible = self.surface.annotations_visible();
        self.surface
            .show_passive_annotations(self.state.annotations_visible);
    }

    fn shutdown_surfaces(&mut self) {
        self.release_interactive();
        self.surface.hide_passive_annotations();
    }

    fn display_changed(&mut self) {
        if self.state.mode == ScreenDrawMode::DisplayChanged {
            return;
        }
        self.release_interactive();
        self.surface.destroy_surfaces();
        self.state.mode = ScreenDrawMode::DisplayChanged;
        self.events.send(NativeSessionEvent::DisplayChanged);
        self.events.send(NativeSessionEvent::Warning(
            "display configuration changed; Screen Draw surfaces were safely destroyed".into(),
        ));
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
    core.shutdown_surfaces();
    match result {
        Ok(Ok(())) => {}
        Ok(Err(message)) => core.events.send(NativeSessionEvent::Error(message)),
        Err(_) => core.events.send(NativeSessionEvent::Error(
            "Screen Draw native worker panicked".into(),
        )),
    }
    core.events.send(NativeSessionEvent::SessionClosed);
}

#[cfg(not(windows))]
fn run_platform_loop(
    core: &mut WorkerCore,
    command_rx: mpsc::Receiver<NativeSessionCommand>,
    _wake: &CommandWake,
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
) -> Result<(), String> {
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, TranslateMessage,
    };

    // Creating the queue before publishing the thread id prevents the classic
    // PostThreadMessage race where the first wake is lost.
    let mut message = MSG::default();
    let _ = unsafe { PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE) };
    wake.thread_id
        .store(unsafe { GetCurrentThreadId() }, Ordering::Release);

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
            core.display_changed();
        } else if message.message == super::native_canvas::WM_CANVAS_CLOSED {
            core.shutdown_surfaces();
            core.running = false;
        } else {
            let _ = unsafe { TranslateMessage(&message) };
            unsafe { DispatchMessageW(&message) };
        }
    }

    core.shutdown_surfaces();
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
        passive_hide: usize,
        destroy: usize,
        resume: usize,
        last_passive_visible: Option<bool>,
        annotations_visible: bool,
        retained_document_objects: usize,
        export_previews: Vec<(u32, u32)>,
        toolbar_windows: Vec<Option<ToolbarWindowInfo>>,
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
        fn show_passive_annotations(&mut self, visible: bool) {
            let mut counts = self.0.lock().unwrap();
            counts.passive += 1;
            counts.last_passive_visible = Some(visible);
        }
        fn hide_passive_annotations(&mut self) {
            self.0.lock().unwrap().passive_hide += 1;
        }
        fn show_export_preview(&mut self, image: &RgbaImage) -> Result<(), String> {
            self.0
                .lock()
                .unwrap()
                .export_previews
                .push(image.dimensions());
            Ok(())
        }
        fn destroy_surfaces(&mut self) {
            self.0.lock().unwrap().destroy += 1;
        }
        fn resume_interactive(&mut self) {
            self.0.lock().unwrap().resume += 1;
        }
        fn set_toolbar_window(&mut self, info: Option<ToolbarWindowInfo>) {
            self.0.lock().unwrap().toolbar_windows.push(info);
        }
        fn annotations_visible(&self) -> bool {
            self.0.lock().unwrap().annotations_visible
        }
        fn set_annotations_visible(&mut self, visible: bool) {
            self.0.lock().unwrap().annotations_visible = visible;
        }
        fn export_source(&self) -> Result<ExportSource, String> {
            Ok(ExportSource {
                snapshot: ScreenDrawSessionSnapshot::new(
                    ScreenDrawGeneration::from_raw(1),
                    CapturedRegion {
                        image: RgbaImage::new(1, 1),
                        origin: (0, 0),
                    },
                ),
                objects: Vec::new(),
                transient: Vec::new(),
                now: std::time::Duration::ZERO,
            })
        }
    }

    struct FixtureDestination(Result<ExportOutcome, String>);

    impl super::super::export::ExportDestinationBackend for FixtureDestination {
        fn deliver(
            &self,
            _destination: super::super::ExportDestination,
            _image: RgbaImage,
        ) -> Result<ExportOutcome, String> {
            self.0.clone()
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
        surface.lock().unwrap().annotations_visible = true;
        surface.lock().unwrap().retained_document_objects = 2;
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
            state: NativeRuntimeState::initial(
                ScreenDrawTool::Pen,
                RgbaColor::RED,
                3.0,
                CanvasBackground::FrozenDesktop,
            ),
            _snapshot: snapshot,
            events: EventSink::new(event_tx, Arc::new(|| {})),
            surface: Box::new(CountingSurface(Arc::clone(&surface))),
            suppression: Some(suppression_factory()),
            suppression_factory,
            running: true,
            export_destination: Arc::new(super::super::export::SystemExportDestination),
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
        core.handle(NativeSessionCommand::EmergencyPause);
        core.pause_to_passive();
        assert_eq!(releases.load(Ordering::SeqCst), 1);
        let counts = surface.lock().unwrap();
        assert_eq!(counts.cancel, 2);
        assert_eq!((counts.release, counts.hide, counts.passive), (2, 2, 2));
        assert_eq!(counts.last_passive_visible, Some(true));
        assert_eq!(counts.retained_document_objects, 2);
        drop(counts);

        core.handle(NativeSessionCommand::Resume);
        assert!(core.suppression.is_some());
        assert_eq!(surface.lock().unwrap().resume, 1);
        core.handle(NativeSessionCommand::Finish);
        assert_eq!(releases.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn ghost_command_cancels_transient_input_and_preserves_document() {
        let (mut core, _, surface, releases) = test_core();
        core.handle(NativeSessionCommand::Ghost);

        assert_eq!(core.state.mode, ScreenDrawMode::Ghost);
        assert!(core.suppression.is_none());
        assert_eq!(releases.load(Ordering::SeqCst), 1);
        let counts = surface.lock().unwrap();
        assert_eq!((counts.cancel, counts.release), (1, 1));
        assert_eq!(counts.retained_document_objects, 2);
    }

    #[test]
    fn hidden_annotations_keep_ghost_surface_hidden_without_changing_history() {
        let (mut core, _, surface, _) = test_core();
        core.handle(NativeSessionCommand::SetAnnotationsVisible(false));
        core.safe_pause(false);
        let counts = surface.lock().unwrap();
        assert_eq!(counts.last_passive_visible, Some(false));
        assert_eq!(core.state.mode, ScreenDrawMode::Ghost);
        assert!(!core.state.annotations_visible);
    }

    #[test]
    fn safe_pause_uses_document_visibility_after_native_drawing_auto_reveals() {
        let (mut core, _, surface, _) = test_core();
        core.state.annotations_visible = false;
        surface.lock().unwrap().annotations_visible = true;
        core.safe_pause(false);
        assert!(core.state.annotations_visible);
        assert_eq!(surface.lock().unwrap().last_passive_visible, Some(true));
    }

    #[test]
    fn display_change_releases_input_hides_both_surfaces_and_preserves_worker() {
        let (mut core, events, surface, releases) = test_core();
        core.display_changed();
        assert!(core.running);
        assert!(core.suppression.is_none());
        assert_eq!(releases.load(Ordering::SeqCst), 1);
        assert_eq!(core.state.mode, ScreenDrawMode::DisplayChanged);
        assert_eq!(core.state.tool, ScreenDrawTool::Pen);
        assert_eq!(core._snapshot.capture().image.dimensions(), (1, 1));
        assert_eq!(surface.lock().unwrap().destroy, 1);
        assert_eq!(surface.lock().unwrap().retained_document_objects, 2);
        assert_eq!(events.recv().unwrap(), NativeSessionEvent::DisplayChanged);
        assert!(matches!(
            events.recv().unwrap(),
            NativeSessionEvent::Warning(_)
        ));

        core.display_changed();
        assert_eq!(surface.lock().unwrap().destroy, 1);

        let export_request = super::super::ExportRequest {
            scope: super::super::ExportScope::FullDesktop,
            background: super::super::ExportBackground::FrozenDesktop,
            destination: super::super::ExportDestination::Clipboard,
        };
        core.export_destination = Arc::new(FixtureDestination(Err("display export failed".into())));
        core.handle(NativeSessionCommand::RenderExport(ExportRenderRequest {
            generation: ScreenDrawGeneration::from_raw(1),
            request: export_request,
        }));
        assert!(matches!(
            events.recv_timeout(std::time::Duration::from_secs(2)).unwrap(),
            NativeSessionEvent::ExportFailed { message, .. } if message == "display export failed"
        ));
        assert_eq!(surface.lock().unwrap().destroy, 1);
        assert_eq!(core.state.mode, ScreenDrawMode::DisplayChanged);

        core.export_destination = Arc::new(FixtureDestination(Ok(ExportOutcome::Clipboard)));
        core.handle(NativeSessionCommand::RenderExport(ExportRenderRequest {
            generation: ScreenDrawGeneration::from_raw(1),
            request: export_request,
        }));
        assert!(matches!(
            events
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap(),
            NativeSessionEvent::ExportCompleted {
                outcome: ExportOutcome::Clipboard,
                ..
            }
        ));
        assert_eq!(surface.lock().unwrap().destroy, 1);

        core.handle(NativeSessionCommand::Resume);
        assert_eq!(core.state.mode, ScreenDrawMode::DisplayChanged);
        assert_eq!(surface.lock().unwrap().resume, 0);
        assert!(matches!(
            events.recv().unwrap(),
            NativeSessionEvent::Warning(message) if message.contains("not resumable")
        ));
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
    fn initial_runtime_state_uses_the_persisted_background() {
        let state = NativeRuntimeState::initial(
            ScreenDrawTool::Pen,
            RgbaColor::RED,
            3.0,
            CanvasBackground::Black,
        );
        assert_eq!(state.background, CanvasBackground::Black);
    }

    #[test]
    fn toolbar_window_is_forwarded_without_mutating_runtime_state_or_emitting_an_event() {
        use crate::screen_draw::{DesktopRect, NativeWindowHandle};

        let (mut core, events, surface, _) = test_core();
        let before = core.state;
        let info = ToolbarWindowInfo {
            handle: NativeWindowHandle::from_raw(41),
            bounds: DesktopRect::new(-900, -120, 420, 860),
        };

        core.handle(NativeSessionCommand::SetToolbarWindow(Some(info)));
        core.handle(NativeSessionCommand::SetToolbarWindow(None));

        assert_eq!(core.state, before);
        assert_eq!(
            surface.lock().unwrap().toolbar_windows,
            vec![Some(info), None]
        );
        assert!(matches!(events.try_recv(), Err(mpsc::TryRecvError::Empty)));
    }

    #[test]
    fn export_worker_is_generation_and_safe_mode_gated_and_reports_destination_results() {
        let (mut core, events, _, _) = test_core();
        let request = super::super::ExportRequest {
            scope: super::super::ExportScope::FullDesktop,
            background: super::super::ExportBackground::Transparent,
            destination: super::super::ExportDestination::Clipboard,
        };
        core.handle(NativeSessionCommand::RenderExport(ExportRenderRequest {
            generation: ScreenDrawGeneration::from_raw(2),
            request,
        }));
        assert!(matches!(
            events.recv().unwrap(),
            NativeSessionEvent::ExportFailed { message, .. } if message.contains("stale or invalid")
        ));

        core.state.mode = ScreenDrawMode::Finish;
        core.export_destination =
            Arc::new(FixtureDestination(Err("fixture export failure".into())));
        core.handle(NativeSessionCommand::RenderExport(ExportRenderRequest {
            generation: ScreenDrawGeneration::from_raw(1),
            request,
        }));
        assert!(matches!(
            events.recv_timeout(std::time::Duration::from_secs(2)).unwrap(),
            NativeSessionEvent::ExportFailed { message, .. } if message == "fixture export failure"
        ));
        assert!(core.running);
        assert_eq!(core.state.mode, ScreenDrawMode::Finish);

        core.export_destination = Arc::new(FixtureDestination(Ok(ExportOutcome::Clipboard)));
        core.handle(NativeSessionCommand::RenderExport(ExportRenderRequest {
            generation: ScreenDrawGeneration::from_raw(1),
            request,
        }));
        assert_eq!(
            events
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap(),
            NativeSessionEvent::ExportCompleted {
                generation: ScreenDrawGeneration::from_raw(1),
                outcome: ExportOutcome::Clipboard,
            }
        );
    }

    #[test]
    fn region_preview_is_composed_before_ready_and_editor_payload_bypasses_destinations() {
        let (mut core, events, surface, releases) = test_core();
        core.handle(NativeSessionCommand::Finish);
        assert_eq!(releases.load(Ordering::SeqCst), 1);
        assert_eq!(
            events.recv().unwrap(),
            NativeSessionEvent::ModeChanged(ScreenDrawMode::Finish)
        );

        core.handle(NativeSessionCommand::PrepareRegionSelection {
            generation: ScreenDrawGeneration::from_raw(1),
            background: super::super::ExportBackground::White,
        });
        assert_eq!(core.state.mode, ScreenDrawMode::SelectingRegion);
        assert_eq!(surface.lock().unwrap().export_previews, [(1, 1)]);
        assert_eq!(
            events.recv().unwrap(),
            NativeSessionEvent::RegionPreviewReady {
                generation: ScreenDrawGeneration::from_raw(1),
                bounds: crate::mkmacro::screen::ScreenRect::new(0, 0, 1, 1),
            }
        );

        core.handle(NativeSessionCommand::EndRegionSelection);
        assert_eq!(core.state.mode, ScreenDrawMode::Finish);
        let request = super::super::ExportRequest {
            scope: super::super::ExportScope::Region(crate::mkmacro::screen::ScreenRect::new(
                0, 0, 1, 1,
            )),
            background: super::super::ExportBackground::White,
            destination: super::super::ExportDestination::ScreenshotEditor,
        };
        core.export_destination = Arc::new(FixtureDestination(Err(
            "editor must not use the system destination".into(),
        )));
        core.handle(NativeSessionCommand::RenderExport(ExportRenderRequest {
            generation: ScreenDrawGeneration::from_raw(1),
            request,
        }));
        assert!(matches!(
            events.recv_timeout(std::time::Duration::from_secs(2)).unwrap(),
            NativeSessionEvent::EditorImageReady { generation, image }
                if generation == ScreenDrawGeneration::from_raw(1)
                    && image.dimensions() == (1, 1)
                    && image.get_pixel(0, 0).0 == [255, 255, 255, 255]
        ));
    }

    #[test]
    fn handle_shutdown_is_signal_only_until_worker_finishes() {
        let (handle, commands) = NativeSessionHandle::test_stub();
        handle.request_shutdown();
        assert_eq!(commands.recv().unwrap(), NativeSessionCommand::Shutdown);
    }

    #[test]
    fn every_command_wake_failure_forces_worker_quit_and_surfaces_error() {
        let forced = AtomicBool::new(false);
        let result = fail_closed_on_wake_error(Err("injected wake failure".into()), || {
            forced.store(true, Ordering::SeqCst);
        });
        assert_eq!(result.unwrap_err(), "injected wake failure");
        assert!(forced.load(Ordering::SeqCst));

        forced.store(false, Ordering::SeqCst);
        assert!(
            fail_closed_on_wake_error(Ok(()), || {
                forced.store(true, Ordering::SeqCst);
            })
            .is_ok()
        );
        assert!(!forced.load(Ordering::SeqCst));
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
