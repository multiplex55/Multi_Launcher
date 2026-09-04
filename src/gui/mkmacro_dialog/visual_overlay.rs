//! One stateful boundary for all macro-authoring desktop overlays.
//!
//! The controller deliberately deals only in signed desktop coordinates.  A
//! native implementation is responsible for converting window-client input to
//! desktop coordinates before returning an [`OverlayInput`].
use crate::mkmacro::{MkPoint, MonitorDescriptor, ScreenRect};
#[cfg(test)]
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::{
    collections::VecDeque,
    fmt,
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Passive overlays remain visible long enough to be recognized without
/// becoming persistent desktop furniture.
pub const PASSIVE_OVERLAY_DURATION: Duration = Duration::from_millis(2500);
pub const RECTANGLE_OUTLINE_WIDTH: i32 = 3;
pub const RECTANGLE_TOOLTIP_OFFSET: (i32, i32) = (16, 16);
pub const RECTANGLE_INSTRUCTION: &str = "Draw a rectangle around the region — Esc to cancel";
pub const POINT_INSTRUCTION: &str = "Click a screen position — Esc to cancel";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RectangleTooltip {
    pub text: String,
    pub pointer: MkPoint,
}

pub fn rectangle_tooltip_text(selection: Option<ScreenRect>) -> String {
    match selection {
        None => RECTANGLE_INSTRUCTION.to_owned(),
        Some(rect) => format!(
            "X: {}  Y: {}\nW: {}  H: {}\nRelease to select — Esc to cancel",
            rect.x, rect.y, rect.width, rect.height
        ),
    }
}

/// Places a tooltip near a pointer, flipping before it at crowded right/bottom edges.
pub fn place_rectangle_tooltip(
    pointer: MkPoint,
    size: (u32, u32),
    monitor: ScreenRect,
    offset: (i32, i32),
) -> MkPoint {
    let width = i64::from(size.0);
    let height = i64::from(size.1);
    let left = i64::from(monitor.x);
    let top = i64::from(monitor.y);
    let max_x = (monitor.right() - width).max(left);
    let max_y = (monitor.bottom() - height).max(top);
    let preferred_x = i64::from(pointer.x) + i64::from(offset.0);
    let preferred_y = i64::from(pointer.y) + i64::from(offset.1);
    let x = if preferred_x + width <= monitor.right() {
        preferred_x
    } else {
        i64::from(pointer.x) - i64::from(offset.0) - width
    };
    let y = if preferred_y + height <= monitor.bottom() {
        preferred_y
    } else {
        i64::from(pointer.y) - i64::from(offset.1) - height
    };
    MkPoint {
        x: x.clamp(left, max_x) as i32,
        y: y.clamp(top, max_y) as i32,
    }
}

pub fn monitor_nearest_pointer(monitors: &[ScreenRect], pointer: MkPoint) -> Option<ScreenRect> {
    monitors.iter().copied().min_by_key(|m| {
        let x = i64::from(pointer.x).clamp(i64::from(m.x), m.right().saturating_sub(1));
        let y = i64::from(pointer.y).clamp(i64::from(m.y), m.bottom().saturating_sub(1));
        (i64::from(pointer.x) - x).pow(2) + (i64::from(pointer.y) - y).pow(2)
    })
}

pub type OperationId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualPointDestination {
    SetVariablePoint,
}

/// Complete editor identity travels to the native worker and back unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualPointRequest {
    pub macro_id: u64,
    pub draft_generation: u64,
    pub step_id: Option<u64>,
    pub destination: VisualPointDestination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointInteractionPhase {
    AwaitingInitialRelease,
    Armed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RectanglePurpose {
    CropScreenshot,
    SearchRegion,
    ReferenceImageCapture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAreaKind {
    WholeWindow,
    ClientArea,
}

/// Explicit phases of a rectangle interaction. An initially-held button is
/// deliberately consumed by `AwaitingInitialRelease`; that release only arms
/// the picker and can never confirm the GUI-launch click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RectangleInteractionPhase {
    Starting,
    AwaitingInitialRelease,
    Armed,
    Dragging { start: MkPoint, current: MkPoint },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisualOverlayState {
    Idle,
    PickingRectangle {
        operation_id: OperationId,
        purpose: RectanglePurpose,
        virtual_desktop: ScreenRect,
        pointer: MkPoint,
        phase: RectangleInteractionPhase,
    },
    PickingPoint {
        operation_id: OperationId,
        request: VisualPointRequest,
        pointer: MkPoint,
        phase: PointInteractionPhase,
    },
    PreviewingRectangle {
        rect: ScreenRect,
        expires_at: Duration,
    },
    HighlightingMonitor {
        descriptor: MonitorDescriptor,
        expires_at: Duration,
    },
    IdentifyingMonitors {
        descriptors: Vec<MonitorDescriptor>,
        expires_at: Duration,
    },
    PreviewingDesktop {
        descriptors: Vec<MonitorDescriptor>,
        expires_at: Duration,
    },
    HighlightingWindow {
        rect: ScreenRect,
        area_kind: WindowAreaKind,
        expires_at: Duration,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayErrorKind {
    UnsupportedPlatform,
    InvalidGeometry,
    Platform,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualOverlayError {
    pub kind: OverlayErrorKind,
    pub message: String,
}
impl VisualOverlayError {
    pub fn unsupported() -> Self {
        Self {
            kind: OverlayErrorKind::UnsupportedPlatform,
            message: "visual overlays are only supported on Windows".into(),
        }
    }
    fn geometry(message: impl Into<String>) -> Self {
        Self {
            kind: OverlayErrorKind::InvalidGeometry,
            message: message.into(),
        }
    }
}
impl fmt::Display for VisualOverlayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for VisualOverlayError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisualOverlayEvent {
    PointConfirmed {
        operation_id: OperationId,
        request: VisualPointRequest,
        point: MkPoint,
    },
    RectangleConfirmed {
        operation_id: OperationId,
        purpose: RectanglePurpose,
        rect: ScreenRect,
    },
    Cancelled {
        operation_id: OperationId,
    },
    Expired {
        operation_id: OperationId,
    },
    Error {
        operation_id: OperationId,
        error: VisualOverlayError,
    },
}

/// Messages are the only way native overlay work crosses onto its owning thread.
#[derive(Debug, Clone)]
pub(crate) enum VisualOverlayCommand {
    PickPoint {
        operation_id: OperationId,
        request: VisualPointRequest,
    },
    BeginRectanglePick {
        operation_id: OperationId,
        purpose: RectanglePurpose,
        virtual_desktop: ScreenRect,
    },
    PreviewRectangle {
        operation_id: OperationId,
        rect: ScreenRect,
    },
    HighlightMonitor {
        operation_id: OperationId,
        monitor: MonitorDescriptor,
    },
    IdentifyMonitors {
        operation_id: OperationId,
        monitors: Vec<MonitorDescriptor>,
    },
    PreviewDesktop {
        operation_id: OperationId,
        monitors: Vec<MonitorDescriptor>,
    },
    HighlightWindow {
        operation_id: OperationId,
        rect: ScreenRect,
        area_kind: WindowAreaKind,
    },
    Cancel {
        expected_operation_id: Option<OperationId>,
    },
    Shutdown,
}

/// The channel endpoints of the long-lived native owner.  Dropping the command
/// sender is also a shutdown request; events remain buffered by the receiver.
pub(crate) struct NativeVisualOverlayService {
    pub commands: Sender<VisualOverlayCommand>,
    pub events: Receiver<VisualOverlayEvent>,
    worker: Option<JoinHandle<()>>,
    #[cfg(test)]
    observer: Option<Arc<ServiceTestObserver>>,
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct ServiceTestObserver {
    pub commands: Mutex<Vec<VisualOverlayCommand>>,
    pub generation_commands: Mutex<Vec<(usize, VisualOverlayCommand)>>,
    pub starts: AtomicUsize,
    pub shutdowns: AtomicUsize,
    pub joins: AtomicUsize,
    pub worker_ids: Mutex<Vec<thread::ThreadId>>,
    inputs: Mutex<Vec<OverlayInput>>,
    left_down: AtomicBool,
    cursor: Mutex<MkPoint>,
    changed: Condvar,
}

#[cfg(test)]
impl ServiceTestObserver {
    pub fn set_left_button_down(&self, down: bool) {
        self.left_down.store(down, Ordering::SeqCst);
    }

    pub fn set_cursor(&self, point: MkPoint) {
        *self.cursor.lock().unwrap() = point;
    }

    pub fn emit(&self, operation_id: OperationId, kind: OverlayInputKind) {
        self.inputs
            .lock()
            .unwrap()
            .push(OverlayInput { operation_id, kind });
    }

    pub fn wait_for_commands(&self, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut commands = self.commands.lock().unwrap();
        while commands.len() < count {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "overlay worker received only {} of {count} expected commands: {commands:?}",
                commands.len()
            );
            let (next, result) = self.changed.wait_timeout(commands, remaining).unwrap();
            commands = next;
            assert!(
                !result.timed_out() || commands.len() >= count,
                "timed out waiting for overlay command {count}; received: {commands:?}"
            );
        }
    }

    pub fn confirm_rectangle(&self, operation_id: OperationId, from: MkPoint, to: MkPoint) {
        self.inputs.lock().unwrap().extend([
            OverlayInput {
                operation_id,
                kind: OverlayInputKind::LeftPressed(from),
            },
            OverlayInput {
                operation_id,
                kind: OverlayInputKind::LeftReleased(to),
            },
        ]);
    }

    pub fn cancel_rectangle(&self, operation_id: OperationId) {
        self.inputs.lock().unwrap().push(OverlayInput {
            operation_id,
            kind: OverlayInputKind::Escape,
        });
    }
}

#[cfg(test)]
pub(crate) struct ServiceTestRenderer(pub Arc<ServiceTestObserver>);

#[cfg(test)]
impl OverlayRenderer for ServiceTestRenderer {
    fn left_button_down(&mut self) -> Result<bool, VisualOverlayError> {
        Ok(self.0.left_down.load(Ordering::SeqCst))
    }
    fn cursor_position(&mut self) -> Result<MkPoint, VisualOverlayError> {
        Ok(*self.0.cursor.lock().unwrap())
    }
    fn show(
        &mut self,
        _: OperationId,
        _: &OverlayVisual,
        _: bool,
    ) -> Result<(), VisualOverlayError> {
        Ok(())
    }
    fn repaint(&mut self, _: OperationId, _: &OverlayVisual) -> Result<(), VisualOverlayError> {
        Ok(())
    }
    fn poll_input(&mut self) -> Result<Vec<OverlayInput>, VisualOverlayError> {
        Ok(std::mem::take(&mut *self.0.inputs.lock().unwrap()))
    }
    fn close(&mut self) {}
}

impl NativeVisualOverlayService {
    pub fn start() -> Result<Self, std::io::Error> {
        Self::start_with(|| VisualOverlayController::default())
    }

    pub(crate) fn start_with<F>(factory: F) -> Result<Self, std::io::Error>
    where
        F: FnOnce() -> VisualOverlayController + Send + 'static,
    {
        Self::start_with_wait_interval(factory, Duration::from_millis(12))
    }

    #[cfg(test)]
    pub(crate) fn start_with_observer<F>(
        factory: F,
        observer: Arc<ServiceTestObserver>,
    ) -> Result<Self, std::io::Error>
    where
        F: FnOnce() -> VisualOverlayController + Send + 'static,
    {
        Self::start_with_wait_interval_and_observer(
            factory,
            Duration::from_millis(12),
            Some(observer),
        )
    }

    /// Test hook for running the real service loop with a deterministic controller and
    /// a nonblocking wait. Production deliberately uses a modest blocking interval.
    pub(crate) fn start_with_wait_interval<F>(
        factory: F,
        active_wait: Duration,
    ) -> Result<Self, std::io::Error>
    where
        F: FnOnce() -> VisualOverlayController + Send + 'static,
    {
        #[cfg(test)]
        {
            Self::start_with_wait_interval_and_observer(factory, active_wait, None)
        }
        #[cfg(not(test))]
        {
            Self::start_with_wait_interval_and_observer(factory, active_wait)
        }
    }

    fn start_with_wait_interval_and_observer<F>(
        factory: F,
        active_wait: Duration,
        #[cfg(test)] observer: Option<Arc<ServiceTestObserver>>,
    ) -> Result<Self, std::io::Error>
    where
        F: FnOnce() -> VisualOverlayController + Send + 'static,
    {
        let (command_tx, command_rx) = mpsc::channel::<VisualOverlayCommand>();
        let (event_tx, event_rx) = mpsc::channel();
        #[cfg(test)]
        let worker_observer = observer.clone();
        let worker = thread::Builder::new()
            .name("native-visual-overlay".into())
            .spawn(move || {
                #[cfg(test)]
                let generation = if let Some(observer) = &worker_observer {
                    let generation = observer.starts.fetch_add(1, Ordering::SeqCst) + 1;
                    observer
                        .worker_ids
                        .lock()
                        .unwrap()
                        .push(thread::current().id());
                    generation
                } else {
                    0
                };
                let mut controller = factory();
                let mut running = true;
                while running {
                    let active = controller.operation_id().is_some();
                    let first = if active {
                        match command_rx.recv_timeout(active_wait) {
                            Ok(command) => Some(command),
                            Err(RecvTimeoutError::Timeout) => None,
                            Err(RecvTimeoutError::Disconnected) => {
                                running = false;
                                None
                            }
                        }
                    } else {
                        match command_rx.recv() {
                            Ok(command) => Some(command),
                            Err(_) => {
                                running = false;
                                None
                            }
                        }
                    };
                    if let Some(command) = first {
                        #[cfg(test)]
                        if let Some(observer) = &worker_observer {
                            observer.commands.lock().unwrap().push(command.clone());
                            observer
                                .generation_commands
                                .lock()
                                .unwrap()
                                .push((generation, command.clone()));
                            observer.changed.notify_all();
                        }
                        running = apply_command(&mut controller, command);
                        while running {
                            match command_rx.try_recv() {
                                Ok(command) => {
                                    #[cfg(test)]
                                    if let Some(observer) = &worker_observer {
                                        observer.commands.lock().unwrap().push(command.clone());
                                        observer
                                            .generation_commands
                                            .lock()
                                            .unwrap()
                                            .push((generation, command.clone()));
                                        observer.changed.notify_all();
                                    }
                                    running = apply_command(&mut controller, command)
                                }
                                Err(mpsc::TryRecvError::Empty) => break,
                                Err(mpsc::TryRecvError::Disconnected) => {
                                    running = false;
                                    break;
                                }
                            }
                        }
                    }
                    if running && controller.operation_id().is_some() {
                        controller.advance();
                    }
                    for event in controller.drain_events() {
                        let _ = event_tx.send(event);
                    }
                }
                controller.shutdown();
                #[cfg(test)]
                if let Some(observer) = &worker_observer {
                    observer.shutdowns.fetch_add(1, Ordering::SeqCst);
                    observer.changed.notify_all();
                }
            })?;
        Ok(Self {
            commands: command_tx,
            events: event_rx,
            worker: Some(worker),
            #[cfg(test)]
            observer,
        })
    }

    pub fn shutdown_and_join(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = self.commands.send(VisualOverlayCommand::Shutdown);
            let _ = worker.join();
            #[cfg(test)]
            if let Some(observer) = &self.observer {
                observer.joins.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    /// Reports whether the owner thread has stopped without waiting for it.
    pub fn is_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Retires a worker which is already known to have stopped.  Events are
    /// drained before the handle is joined so callers can preserve diagnostics
    /// produced immediately before termination.  Calling this repeatedly is
    /// harmless; a handle is recorded as joined exactly once.
    pub fn cleanup_finished(&mut self) -> Vec<VisualOverlayEvent> {
        let events: Vec<_> = self.events.try_iter().collect();
        if self.is_finished() {
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
                #[cfg(test)]
                if let Some(observer) = &self.observer {
                    observer.joins.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
        events
    }
}

fn apply_command(controller: &mut VisualOverlayController, command: VisualOverlayCommand) -> bool {
    match command {
        VisualOverlayCommand::PickPoint {
            operation_id,
            request,
        } => {
            controller.begin_point_pick_with_id(operation_id, request);
        }
        VisualOverlayCommand::BeginRectanglePick {
            operation_id,
            purpose,
            virtual_desktop,
        } => {
            controller.begin_rectangle_pick_with_id(operation_id, purpose, virtual_desktop);
        }
        VisualOverlayCommand::PreviewRectangle { operation_id, rect } => {
            controller.preview_rectangle_with_id(operation_id, rect)
        }
        VisualOverlayCommand::HighlightMonitor {
            operation_id,
            monitor,
        } => {
            controller.highlight_monitor_with_id(operation_id, monitor);
        }
        VisualOverlayCommand::IdentifyMonitors {
            operation_id,
            monitors,
        } => {
            controller.identify_monitors_with_id(operation_id, monitors);
        }
        VisualOverlayCommand::PreviewDesktop {
            operation_id,
            monitors,
        } => {
            controller.preview_desktop_with_id(operation_id, monitors);
        }
        VisualOverlayCommand::HighlightWindow {
            operation_id,
            rect,
            area_kind,
        } => {
            controller.highlight_window_with_id(operation_id, rect, area_kind);
        }
        VisualOverlayCommand::Cancel {
            expected_operation_id,
        } => {
            if expected_operation_id.is_none() || expected_operation_id == controller.operation_id()
            {
                controller.cancel()
            }
        }
        VisualOverlayCommand::Shutdown => return false,
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayInputKind {
    LeftPressed(MkPoint),
    PointerMoved(MkPoint),
    LeftReleased(MkPoint),
    Escape,
    Enter,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayInput {
    pub operation_id: OperationId,
    pub kind: OverlayInputKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayVisual {
    PointPicker {
        pointer: MkPoint,
    },
    RectanglePicker {
        virtual_desktop: ScreenRect,
        selection: Option<ScreenRect>,
        tooltip: RectangleTooltip,
    },
    RectanglePreview(ScreenRect),
    Monitor(MonitorDescriptor),
    Monitors(Vec<MonitorDescriptor>),
    /// Ordinary desktop coverage preview: outlines only, never identification labels.
    Desktop(Vec<MonitorDescriptor>),
    Window {
        rect: ScreenRect,
        area_kind: WindowAreaKind,
    },
}
impl OverlayVisual {
    pub(crate) fn passive(&self) -> bool {
        !matches!(
            self,
            Self::RectanglePicker { .. } | Self::PointPicker { .. }
        )
    }
}

/// The four independently-owned windows which make up a passive outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum OutlineEdge {
    Top,
    Right,
    Bottom,
    Left,
}

/// Platform-neutral styling contract for passive outline windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PassiveWindowStyle {
    BrightYellow,
}

impl fmt::Display for OutlineEdge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Right => "right",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PassiveWindowSpec {
    Edge {
        edge: OutlineEdge,
        target: ScreenRect,
        rect: ScreenRect,
        style: PassiveWindowStyle,
    },
    Badge {
        monitor: ScreenRect,
        index: usize,
    },
}

/// Decomposes an outline without doing arithmetic in the signed coordinate
/// type. Overlap at the corners is intentional and prevents holes for tiny
/// (even one-pixel) targets.
pub(crate) fn outline_edge_rects(rect: ScreenRect) -> [(OutlineEdge, ScreenRect); 4] {
    let horizontal = rect.height.min(RECTANGLE_OUTLINE_WIDTH as u32).max(1);
    let vertical = rect.width.min(RECTANGLE_OUTLINE_WIDTH as u32).max(1);
    let bottom = (rect.bottom() - i64::from(horizontal))
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    let right =
        (rect.right() - i64::from(vertical)).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    [
        (
            OutlineEdge::Top,
            ScreenRect::new(rect.x, rect.y, rect.width.max(1), horizontal),
        ),
        (
            OutlineEdge::Right,
            ScreenRect::new(right, rect.y, vertical, rect.height.max(1)),
        ),
        (
            OutlineEdge::Bottom,
            ScreenRect::new(rect.x, bottom, rect.width.max(1), horizontal),
        ),
        (
            OutlineEdge::Left,
            ScreenRect::new(rect.x, rect.y, vertical, rect.height.max(1)),
        ),
    ]
}

fn rect_intersection(a: ScreenRect, b: ScreenRect) -> Option<ScreenRect> {
    let left = i64::from(a.x).max(i64::from(b.x));
    let top = i64::from(a.y).max(i64::from(b.y));
    let right = a.right().min(b.right());
    let bottom = a.bottom().min(b.bottom());
    (left < right && top < bottom).then(|| {
        ScreenRect::new(
            left as i32,
            top as i32,
            (right - left) as u32,
            (bottom - top) as u32,
        )
    })
}

/// Produces small, platform-neutral passive windows. Edge pixels are clipped
/// independently to real displays, so no popup bridges a virtual-desktop gap.
pub(crate) fn passive_overlay_plan(
    visual: &OverlayVisual,
    displays: &[ScreenRect],
) -> Option<Vec<PassiveWindowSpec>> {
    if !visual.passive() {
        return None;
    }
    let (targets, badges): (Vec<_>, Vec<_>) = match visual {
        OverlayVisual::RectanglePreview(r) | OverlayVisual::Window { rect: r, .. } => {
            (vec![*r], vec![])
        }
        OverlayVisual::Monitor(d) => (vec![d.bounds], vec![]),
        OverlayVisual::Desktop(ds) => (ds.iter().map(|d| d.bounds).collect(), vec![]),
        OverlayVisual::Monitors(ds) => (
            ds.iter().map(|d| d.bounds).collect(),
            ds.iter().map(|d| (d.bounds, d.index)).collect(),
        ),
        OverlayVisual::RectanglePicker { .. } | OverlayVisual::PointPicker { .. } => unreachable!(),
    };
    let mut plan = Vec::new();
    for target in targets {
        for (edge, edge_rect) in outline_edge_rects(target) {
            for display in displays {
                if let Some(rect) = rect_intersection(edge_rect, *display) {
                    plan.push(PassiveWindowSpec::Edge {
                        edge,
                        target,
                        rect,
                        style: PassiveWindowStyle::BrightYellow,
                    });
                }
            }
        }
    }
    plan.extend(
        badges
            .into_iter()
            .map(|(monitor, index)| PassiveWindowSpec::Badge { monitor, index }),
    );
    plan.sort_by_key(|spec| match spec {
        PassiveWindowSpec::Edge {
            edge, target, rect, ..
        } => (0, target.x, target.y, *edge as i32, rect.x, rect.y),
        PassiveWindowSpec::Badge { monitor, index } => {
            (1, monitor.x, monitor.y, 0, *index as i32, 0)
        }
    });
    Some(plan)
}

/// Runs the production Win32 renderer directly for manual smoke testing.
#[cfg(windows)]
pub fn run_passive_overlay_smoke_test() -> Result<(), VisualOverlayError> {
    native::run_passive_overlay_smoke_test()
}

/// Keeps the manual harness buildable while making its platform requirement explicit.
#[cfg(not(windows))]
pub fn run_passive_overlay_smoke_test() -> Result<(), VisualOverlayError> {
    Err(VisualOverlayError::unsupported())
}

/// Platform-neutral description of the pixels a native overlay must produce.
/// The clear is deliberately first so repainting cannot leave an old drag
/// rectangle behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OverlayFramePrimitive {
    Clear,
    Outline(ScreenRect),
    MonitorLabel { bounds: ScreenRect, index: usize },
}

pub(crate) fn overlay_frame(visual: &OverlayVisual) -> Vec<OverlayFramePrimitive> {
    let mut frame = vec![OverlayFramePrimitive::Clear];
    match visual {
        OverlayVisual::PointPicker { .. } => {}
        OverlayVisual::RectanglePicker { selection, .. } => {
            if let Some(rect) = selection {
                frame.push(OverlayFramePrimitive::Outline(*rect));
            }
        }
        OverlayVisual::RectanglePreview(rect) | OverlayVisual::Window { rect, .. } => {
            frame.push(OverlayFramePrimitive::Outline(*rect))
        }
        OverlayVisual::Monitor(monitor) => {
            frame.push(OverlayFramePrimitive::Outline(monitor.bounds));
            frame.push(OverlayFramePrimitive::MonitorLabel {
                bounds: monitor.bounds,
                index: monitor.index,
            });
        }
        OverlayVisual::Monitors(monitors) => {
            for monitor in monitors {
                frame.push(OverlayFramePrimitive::Outline(monitor.bounds));
                frame.push(OverlayFramePrimitive::MonitorLabel {
                    bounds: monitor.bounds,
                    index: monitor.index,
                });
            }
        }
        OverlayVisual::Desktop(monitors) => {
            for monitor in monitors {
                frame.push(OverlayFramePrimitive::Outline(monitor.bounds));
            }
        }
    }
    frame
}

pub(crate) fn intersecting_monitor_bounds(
    monitors: &[ScreenRect],
    target: ScreenRect,
) -> Vec<ScreenRect> {
    monitors
        .iter()
        .copied()
        .filter(|m| {
            i64::from(m.x) < target.right()
                && i64::from(target.x) < m.right()
                && i64::from(m.y) < target.bottom()
                && i64::from(target.y) < m.bottom()
        })
        .collect()
}

pub(crate) fn desktop_to_overlay(rect: ScreenRect, origin: ScreenRect) -> (i64, i64, i64, i64) {
    let left = i64::from(rect.x) - i64::from(origin.x);
    let top = i64::from(rect.y) - i64::from(origin.y);
    (
        left,
        top,
        left + i64::from(rect.width),
        top + i64::from(rect.height),
    )
}

pub(crate) fn monitor_union(monitors: &[MonitorDescriptor]) -> Option<ScreenRect> {
    monitors.iter().map(|m| m.bounds).reduce(|a, b| {
        let left = a.x.min(b.x);
        let top = a.y.min(b.y);
        ScreenRect::new(
            left,
            top,
            (a.right().max(b.right()) - i64::from(left)) as u32,
            (a.bottom().max(b.bottom()) - i64::from(top)) as u32,
        )
    })
}

pub(crate) fn overlay_is_mouse_transparent(visual: &OverlayVisual) -> bool {
    visual.passive()
}

/// Describes the native input policy for an overlay visual. Rectangle picking
/// is the only mode which must consume mouse messages before they reach the
/// application underneath it; all other visuals continue to rely on polling
/// (or have no input contract at all).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayInputMode {
    PollingOnly,
    PerMonitorShields,
}

    if matches!(visual, OverlayVisual::RectanglePicker { .. }) {
        OverlayInputMode::PerMonitorShields
    } else {
        OverlayInputMode::PollingOnly
    }
}

pub(crate) fn overlay_requires_input_shields(visual: &OverlayVisual) -> bool {
    matches!(
        overlay_input_mode(visual),
        OverlayInputMode::PerMonitorShields
    )
}

/// Returns one physical-monitor shield specification for every display that
/// intersects the rectangle picker's virtual desktop. Signed monitor bounds
/// are returned unchanged, including negative origins and irregular layouts.
pub(crate) fn input_shield_plan(
    visual: &OverlayVisual,
    displays: &[ScreenRect],
) -> Vec<ScreenRect> {
    let OverlayVisual::RectanglePicker {
        virtual_desktop, ..
    } = visual
    else {
        return vec![];
    };
    intersecting_monitor_bounds(displays, *virtual_desktop)
}

/// Isolates native windows, input, painting and resource ownership from the
/// deterministic state machine.
pub trait OverlayRenderer: Send {
    /// Returns the physical button state without inventing an edge.
    fn left_button_down(&mut self) -> Result<bool, VisualOverlayError>;
    fn cursor_position(&mut self) -> Result<MkPoint, VisualOverlayError>;
    fn show(
        &mut self,
        operation_id: OperationId,
        visual: &OverlayVisual,
        mouse_transparent: bool,
    ) -> Result<(), VisualOverlayError>;
    fn repaint(
        &mut self,
        operation_id: OperationId,
        visual: &OverlayVisual,
    ) -> Result<(), VisualOverlayError>;
    fn poll_input(&mut self) -> Result<Vec<OverlayInput>, VisualOverlayError>;
    fn close(&mut self);
}

pub trait OverlayClock: Send {
    fn now(&self) -> Duration;
}
struct SystemClock {
    epoch: Instant,
}
impl Default for SystemClock {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
        }
    }
}
impl OverlayClock for SystemClock {
    fn now(&self) -> Duration {
        self.epoch.elapsed()
    }
}

pub struct VisualOverlayController {
    state: VisualOverlayState,
    operation_id: Option<OperationId>,
    next_id: OperationId,
    renderer: Box<dyn OverlayRenderer>,
    clock: Box<dyn OverlayClock>,
    events: VecDeque<VisualOverlayEvent>,
    virtual_desktop: Option<ScreenRect>,
    picker_pointer: Option<MkPoint>,
    shut_down: bool,
}

impl Default for VisualOverlayController {
    fn default() -> Self {
        Self::new(Box::new(NativeOverlayRenderer::default()))
    }
}
impl VisualOverlayController {
    pub fn new(renderer: Box<dyn OverlayRenderer>) -> Self {
        Self::with_clock(renderer, Box::new(SystemClock::default()))
    }
    pub fn with_clock(renderer: Box<dyn OverlayRenderer>, clock: Box<dyn OverlayClock>) -> Self {
        Self {
            state: VisualOverlayState::Idle,
            operation_id: None,
            next_id: 1,
            renderer,
            clock,
            events: VecDeque::new(),
            virtual_desktop: None,
            picker_pointer: None,
            shut_down: false,
        }
    }
    pub fn state(&self) -> &VisualOverlayState {
        &self.state
    }
    pub fn operation_id(&self) -> Option<OperationId> {
        self.operation_id
    }
    fn allocate(&mut self) -> OperationId {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }
    fn replace_with_id(&mut self, new_id: OperationId) -> OperationId {
        if let Some(id) = self.operation_id.take() {
            tracing::debug!(outgoing_operation_id = id, incoming_operation_id = new_id,
                state = ?self.state, "replacing visual overlay operation");
            self.renderer.close();
            // Nothing produced by the replaced operation may be observed after
            // its replacement.  Keep one useful cancellation notification,
            // but quarantine an earlier error/expiry (and duplicate cancel).
            self.events.retain(|event| event_operation_id(event) != id);
            self.events
                .push_back(VisualOverlayEvent::Cancelled { operation_id: id });
        }
        self.state = VisualOverlayState::Idle;
        self.virtual_desktop = None;
        self.picker_pointer = None;
        new_id
    }
    fn replace(&mut self) -> OperationId {
        let id = self.allocate();
        self.replace_with_id(id)
    }
    fn start(&mut self, state: VisualOverlayState, visual: OverlayVisual) -> OperationId {
        let id = self.replace();
        self.start_replaced(id, state, visual)
    }
    fn start_with_id(
        &mut self,
        id: OperationId,
        state: VisualOverlayState,
        visual: OverlayVisual,
    ) -> OperationId {
        self.replace_with_id(id);
        self.start_replaced(id, state, visual)
    }
    fn start_replaced(
        &mut self,
        id: OperationId,
        state: VisualOverlayState,
        visual: OverlayVisual,
    ) -> OperationId {
        self.shut_down = false;
        let started_at = self.clock.now();
        let expires_at = passive_expiry(&state);
        tracing::debug!(operation_id = id, visual = ?visual, ?started_at, ?expires_at,
            state = ?self.state, "starting visual overlay operation");
        match self.renderer.show(id, &visual, visual.passive()) {
            Ok(()) => {
                self.operation_id = Some(id);
                self.state = state;
            }
            Err(error) => {
                // show() is permitted to fail after creating some native
                // resources, so close is mandatory even though the operation
                // was never published as active.
                self.renderer.close();
                self.state = VisualOverlayState::Idle;
                self.operation_id = None;
                self.virtual_desktop = None;
                self.picker_pointer = None;
                self.events.push_back(VisualOverlayEvent::Error {
                    operation_id: id,
                    error,
                });
            }
        }
        id
    }
    pub fn begin_rectangle_pick(
        &mut self,
        purpose: RectanglePurpose,
        virtual_desktop: ScreenRect,
    ) -> OperationId {
        let id = self.allocate();
        self.begin_rectangle_pick_with_id(id, purpose, virtual_desktop)
    }
    pub fn begin_point_pick(&mut self, request: VisualPointRequest) -> OperationId {
        let id = self.allocate();
        self.begin_point_pick_with_id(id, request)
    }
    pub(crate) fn begin_point_pick_with_id(
        &mut self,
        id: OperationId,
        request: VisualPointRequest,
    ) -> OperationId {
        let pointer = match self.renderer.cursor_position() {
            Ok(point) => point,
            Err(error) => {
                self.replace_with_id(id);
                self.renderer.close();
                self.events.push_back(VisualOverlayEvent::Error {
                    operation_id: id,
                    error,
                });
                return id;
            }
        };
        self.start_with_id(
            id,
            VisualOverlayState::PickingPoint {
                operation_id: id,
                request,
                pointer,
                phase: PointInteractionPhase::AwaitingInitialRelease,
            },
            OverlayVisual::PointPicker { pointer },
        )
    }
    pub(crate) fn begin_rectangle_pick_with_id(
        &mut self,
        requested_id: OperationId,
        purpose: RectanglePurpose,
        virtual_desktop: ScreenRect,
    ) -> OperationId {
        if virtual_desktop.is_empty()
            || virtual_desktop.right() > i64::from(i32::MAX) + 1
            || virtual_desktop.bottom() > i64::from(i32::MAX) + 1
        {
            let id = self.replace_with_id(requested_id);
            self.events.push_back(VisualOverlayEvent::Error {
                operation_id: id,
                error: VisualOverlayError::geometry(
                    "virtual desktop is empty or exceeds signed coordinate bounds",
                ),
            });
            return id;
        }
        let left_down = match self.renderer.left_button_down() {
            Ok(left_down) => left_down,
            Err(error) => {
                let id = self.replace_with_id(requested_id);
                self.events.push_back(VisualOverlayEvent::Error {
                    operation_id: id,
                    error,
                });
                return id;
            }
        };
        let pointer = match self.renderer.cursor_position() {
            Ok(pointer) => pointer,
            Err(error) => {
                let id = self.replace_with_id(requested_id);
                self.events.push_back(VisualOverlayEvent::Error {
                    operation_id: id,
                    error,
                });
                return id;
            }
        };
        self.picker_pointer = Some(pointer);
        let id = self.start_with_id(
            requested_id,
            VisualOverlayState::PickingRectangle {
                operation_id: requested_id,
                purpose,
                virtual_desktop,
                pointer,
                phase: if left_down {
                    RectangleInteractionPhase::AwaitingInitialRelease
                } else {
                    RectangleInteractionPhase::Armed
                },
            },
            OverlayVisual::RectanglePicker {
                virtual_desktop,
                selection: None,
                tooltip: RectangleTooltip {
                    text: rectangle_tooltip_text(None),
                    pointer,
                },
            },
        );
        if self.operation_id == Some(id) {
            self.virtual_desktop = Some(virtual_desktop);
        }
        id
    }
    pub fn preview_rectangle(&mut self, rect: ScreenRect) -> OperationId {
        self.start_passive(
            VisualOverlayState::PreviewingRectangle {
                rect,
                expires_at: self.deadline(),
            },
            OverlayVisual::RectanglePreview(rect),
        )
    }
    pub(crate) fn preview_rectangle_with_id(&mut self, id: OperationId, rect: ScreenRect) {
        self.start_passive_with_id(
            id,
            VisualOverlayState::PreviewingRectangle {
                rect,
                expires_at: self.deadline(),
            },
            OverlayVisual::RectanglePreview(rect),
        );
    }
    pub(crate) fn highlight_monitor_with_id(
        &mut self,
        id: OperationId,
        descriptor: MonitorDescriptor,
    ) {
        self.start_passive_with_id(
            id,
            VisualOverlayState::HighlightingMonitor {
                descriptor: descriptor.clone(),
                expires_at: self.deadline(),
            },
            OverlayVisual::Monitor(descriptor),
        );
    }
    pub(crate) fn identify_monitors_with_id(
        &mut self,
        id: OperationId,
        descriptors: Vec<MonitorDescriptor>,
    ) {
        self.start_passive_with_id(
            id,
            VisualOverlayState::IdentifyingMonitors {
                descriptors: descriptors.clone(),
                expires_at: self.deadline(),
            },
            OverlayVisual::Monitors(descriptors),
        );
    }
    pub(crate) fn preview_desktop_with_id(
        &mut self,
        id: OperationId,
        descriptors: Vec<MonitorDescriptor>,
    ) {
        self.start_passive_with_id(
            id,
            VisualOverlayState::PreviewingDesktop {
                descriptors: descriptors.clone(),
                expires_at: self.deadline(),
            },
            OverlayVisual::Desktop(descriptors),
        );
    }
    pub(crate) fn highlight_window_with_id(
        &mut self,
        id: OperationId,
        rect: ScreenRect,
        area_kind: WindowAreaKind,
    ) {
        self.start_passive_with_id(
            id,
            VisualOverlayState::HighlightingWindow {
                rect,
                area_kind,
                expires_at: self.deadline(),
            },
            OverlayVisual::Window { rect, area_kind },
        );
    }
    pub fn highlight_monitor(&mut self, descriptor: MonitorDescriptor) -> OperationId {
        let state = VisualOverlayState::HighlightingMonitor {
            descriptor: descriptor.clone(),
            expires_at: self.deadline(),
        };
        self.start_passive(state, OverlayVisual::Monitor(descriptor))
    }
    pub fn identify_monitors(&mut self, descriptors: Vec<MonitorDescriptor>) -> OperationId {
        let state = VisualOverlayState::IdentifyingMonitors {
            descriptors: descriptors.clone(),
            expires_at: self.deadline(),
        };
        self.start_passive(state, OverlayVisual::Monitors(descriptors))
    }
    pub fn highlight_window(&mut self, rect: ScreenRect, area_kind: WindowAreaKind) -> OperationId {
        self.start_passive(
            VisualOverlayState::HighlightingWindow {
                rect,
                area_kind,
                expires_at: self.deadline(),
            },
            OverlayVisual::Window { rect, area_kind },
        )
    }
    fn deadline(&self) -> Duration {
        self.clock.now().saturating_add(PASSIVE_OVERLAY_DURATION)
    }
    fn start_passive(&mut self, state: VisualOverlayState, visual: OverlayVisual) -> OperationId {
        if geometry_of(&visual).is_some_and(ScreenRect::is_empty) {
            let id = self.replace();
            self.events.push_back(VisualOverlayEvent::Error {
                operation_id: id,
                error: VisualOverlayError::geometry("overlay rectangle must be nonempty"),
            });
            id
        } else {
            self.start(state, visual)
        }
    }
    fn start_passive_with_id(
        &mut self,
        id: OperationId,
        state: VisualOverlayState,
        visual: OverlayVisual,
    ) {
        if geometry_of(&visual).is_some_and(ScreenRect::is_empty) {
            self.replace_with_id(id);
            self.events.push_back(VisualOverlayEvent::Error {
                operation_id: id,
                error: VisualOverlayError::geometry("overlay rectangle must be nonempty"),
            });
        } else {
            self.start_with_id(id, state, visual);
        }
    }
    pub fn cancel(&mut self) {
        // Keep cleanup unconditional: a native show can fail after allocating
        // resources, and a disconnected/replaced owner must never strand one.
        self.renderer.close();
        if let Some(id) = self.operation_id.take() {
            self.events
                .push_back(VisualOverlayEvent::Cancelled { operation_id: id });
        }
        self.state = VisualOverlayState::Idle;
        self.virtual_desktop = None;
        self.picker_pointer = None;
    }
    pub fn shutdown(&mut self) {
        if self.shut_down {
            return;
        }
        self.renderer.close();
        self.operation_id = None;
        self.state = VisualOverlayState::Idle;
        self.virtual_desktop = None;
        self.picker_pointer = None;
        self.events.clear();
        self.shut_down = true;
    }
    /// Advances native input/message state. Only the service worker calls this in production.
    pub fn advance(&mut self) {
        let input_result = self.renderer.poll_input();
        if let Ok(inputs) = input_result.as_ref() {
            for input in inputs.iter().copied() {
                self.handle_input(input);
            }
        }
        if let Err(error) = input_result {
            self.renderer.close();
            if let Some(id) = self.operation_id.take() {
                self.state = VisualOverlayState::Idle;
                self.virtual_desktop = None;
                self.events.push_back(VisualOverlayEvent::Error {
                    operation_id: id,
                    error,
                });
            }
        }
        let expired = match &self.state {
            VisualOverlayState::PreviewingRectangle { expires_at, .. }
            | VisualOverlayState::HighlightingMonitor { expires_at, .. }
            | VisualOverlayState::IdentifyingMonitors { expires_at, .. }
            | VisualOverlayState::PreviewingDesktop { expires_at, .. }
            | VisualOverlayState::HighlightingWindow { expires_at, .. } => {
                self.clock.now() >= *expires_at
            }
            _ => false,
        };
        if expired {
            if let Some(id) = self.operation_id.take() {
                tracing::debug!(operation_id = id, state = ?self.state, now = ?self.clock.now(),
                    "passive visual overlay expired; closing operation");
                self.renderer.close();
                self.state = VisualOverlayState::Idle;
                self.events
                    .push_back(VisualOverlayEvent::Expired { operation_id: id });
            }
        }
    }
    /// Drains semantic events without polling native resources.
    pub fn drain_events(&mut self) -> Vec<VisualOverlayEvent> {
        self.events.drain(..).collect()
    }
    #[deprecated(note = "use advance followed by drain_events")]
    pub fn poll(&mut self) -> Vec<VisualOverlayEvent> {
        self.advance();
        self.drain_events()
    }
    fn handle_input(&mut self, input: OverlayInput) {
        if self.operation_id != Some(input.operation_id) {
            return;
        }
        if matches!(input.kind, OverlayInputKind::Escape) {
            self.cancel();
            return;
        }
        if let VisualOverlayState::PickingPoint { pointer, phase, .. } = &mut self.state {
            match input.kind {
                OverlayInputKind::PointerMoved(point) => {
                    *pointer = point;
                    self.repaint_point_picker();
                }
                OverlayInputKind::LeftReleased(point)
                    if matches!(phase, PointInteractionPhase::AwaitingInitialRelease) =>
                {
                    *pointer = point;
                    *phase = PointInteractionPhase::Armed;
                    self.repaint_point_picker();
                }
                OverlayInputKind::LeftPressed(_)
                    if matches!(phase, PointInteractionPhase::Armed) =>
                {
                    self.confirm_point();
                }
                _ => {}
            }
            return;
        }
        let VisualOverlayState::PickingRectangle { pointer, phase, .. } = &mut self.state else {
            return;
        };
        match input.kind {
            OverlayInputKind::PointerMoved(point) => {
                *pointer = point;
                if let RectangleInteractionPhase::Dragging { current, .. } = phase {
                    *current = point;
                }
                self.repaint_picker();
            }
            OverlayInputKind::LeftPressed(point)
                if matches!(phase, RectangleInteractionPhase::Armed) =>
            {
                *pointer = point;
                *phase = RectangleInteractionPhase::Dragging {
                    start: point,
                    current: point,
                };
                self.repaint_picker();
            }
            OverlayInputKind::LeftReleased(point) => match *phase {
                RectangleInteractionPhase::AwaitingInitialRelease => {
                    *pointer = point;
                    *phase = RectangleInteractionPhase::Armed;
                    self.repaint_picker();
                }
                RectangleInteractionPhase::Dragging { start, .. } => {
                    *pointer = point;
                    *phase = RectangleInteractionPhase::Dragging {
                        start,
                        current: point,
                    };
                    // Paint the actual release coordinate before native resources are destroyed.
                    self.repaint_picker();
                    self.confirm_picker();
                }
                _ => {}
            },
            // Mouse release is authoritative for interactive selection.
            OverlayInputKind::Enter
            | OverlayInputKind::LeftPressed(_)
            | OverlayInputKind::Escape => {}
        }
    }
    fn repaint_point_picker(&mut self) {
        let VisualOverlayState::PickingPoint { pointer, .. } = self.state else {
            return;
        };
        let Some(id) = self.operation_id else { return };
        if let Err(error) = self
            .renderer
            .repaint(id, &OverlayVisual::PointPicker { pointer })
        {
            self.renderer.close();
            self.operation_id = None;
            self.state = VisualOverlayState::Idle;
            self.events.push_back(VisualOverlayEvent::Error {
                operation_id: id,
                error,
            });
        }
    }
    fn confirm_point(&mut self) {
        let VisualOverlayState::PickingPoint {
            request: ref request_value,
            ..
        } = self.state
        else {
            return;
        };
        let request = request_value.clone();
        let id = self
            .operation_id
            .take()
            .expect("point picker must be active");
        match self.renderer.cursor_position() {
            Ok(point) => {
                self.renderer.close();
                self.state = VisualOverlayState::Idle;
                self.events.push_back(VisualOverlayEvent::PointConfirmed {
                    operation_id: id,
                    request,
                    point,
                });
            }
            Err(error) => {
                self.renderer.close();
                self.state = VisualOverlayState::Idle;
                self.events.push_back(VisualOverlayEvent::Error {
                    operation_id: id,
                    error,
                });
            }
        }
    }
    fn repaint_picker(&mut self) {
        let (virtual_desktop, pointer, selection) = match &self.state {
            VisualOverlayState::PickingRectangle {
                virtual_desktop,
                pointer,
                phase,
                ..
            } => {
                let selection = match phase {
                    RectangleInteractionPhase::Dragging { start, current } => {
                        normalized_rect(*start, *current).ok()
                    }
                    _ => None,
                };
                (*virtual_desktop, *pointer, selection)
            }
            _ => return,
        };
        if let Some(id) = self.operation_id {
            if let Err(error) = self.renderer.repaint(
                id,
                &OverlayVisual::RectanglePicker {
                    virtual_desktop,
                    selection,
                    tooltip: RectangleTooltip {
                        text: rectangle_tooltip_text(selection),
                        pointer,
                    },
                },
            ) {
                self.renderer.close();
                self.operation_id = None;
                self.state = VisualOverlayState::Idle;
                self.virtual_desktop = None;
                self.picker_pointer = None;
                self.events.push_back(VisualOverlayEvent::Error {
                    operation_id: id,
                    error,
                });
            }
        }
    }
    fn confirm_picker(&mut self) {
        let (start, current, purpose) = match self.state {
            VisualOverlayState::PickingRectangle {
                phase: RectangleInteractionPhase::Dragging { start, current },
                purpose,
                ..
            } => (start, current, purpose),
            _ => return,
        };
        match normalized_rect(start, current) {
            Ok(rect) if !rect.is_empty() => {
                let id = self.operation_id.take().unwrap();
                self.renderer.close();
                self.state = VisualOverlayState::Idle;
                self.virtual_desktop = None;
                self.picker_pointer = None;
                self.events
                    .push_back(VisualOverlayEvent::RectangleConfirmed {
                        operation_id: id,
                        purpose,
                        rect,
                    });
            }
            // Empty drags are retries: discard the drag and re-arm without emitting.
            _ => {
                if let VisualOverlayState::PickingRectangle { phase, .. } = &mut self.state {
                    *phase = RectangleInteractionPhase::Armed;
                }
                self.repaint_picker();
            }
        }
    }
}

fn passive_expiry(state: &VisualOverlayState) -> Option<Duration> {
    match state {
        VisualOverlayState::PreviewingRectangle { expires_at, .. }
        | VisualOverlayState::HighlightingMonitor { expires_at, .. }
        | VisualOverlayState::IdentifyingMonitors { expires_at, .. }
        | VisualOverlayState::PreviewingDesktop { expires_at, .. }
        | VisualOverlayState::HighlightingWindow { expires_at, .. } => Some(*expires_at),
        _ => None,
    }
}

fn event_operation_id(event: &VisualOverlayEvent) -> OperationId {
    match event {
        VisualOverlayEvent::PointConfirmed { operation_id, .. }
        | VisualOverlayEvent::RectangleConfirmed { operation_id, .. }
        | VisualOverlayEvent::Cancelled { operation_id }
        | VisualOverlayEvent::Expired { operation_id }
        | VisualOverlayEvent::Error { operation_id, .. } => *operation_id,
    }
}
impl Drop for VisualOverlayController {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn geometry_of(visual: &OverlayVisual) -> Option<ScreenRect> {
    match visual {
        OverlayVisual::RectanglePreview(r) | OverlayVisual::Window { rect: r, .. } => Some(*r),
        OverlayVisual::Monitor(d) => Some(d.bounds),
        _ => None,
    }
}

/// Normalizes either drag direction without unsigned casts or overflowing i32 subtraction.
pub fn normalized_rect(a: MkPoint, b: MkPoint) -> Result<ScreenRect, VisualOverlayError> {
    let left = a.x.min(b.x);
    let top = a.y.min(b.y);
    let width = i64::from(a.x).abs_diff(i64::from(b.x));
    let height = i64::from(a.y).abs_diff(i64::from(b.y));
    Ok(ScreenRect::new(
        left,
        top,
        u32::try_from(width)
            .map_err(|_| VisualOverlayError::geometry("rectangle width overflow"))?,
        u32::try_from(height)
            .map_err(|_| VisualOverlayError::geometry("rectangle height overflow"))?,
    ))
}

#[cfg(windows)]
#[path = "visual_overlay_windows.rs"]
mod native;
#[cfg(windows)]
use native::NativeOverlayRenderer;

#[cfg(not(windows))]
#[derive(Default)]
struct NativeOverlayRenderer;
#[cfg(not(windows))]
impl OverlayRenderer for NativeOverlayRenderer {
    fn left_button_down(&mut self) -> Result<bool, VisualOverlayError> {
        Ok(false)
    }
    fn cursor_position(&mut self) -> Result<MkPoint, VisualOverlayError> {
        Err(VisualOverlayError::unsupported())
    }
    fn show(
        &mut self,
        _: OperationId,
        _: &OverlayVisual,
        _: bool,
    ) -> Result<(), VisualOverlayError> {
        Err(VisualOverlayError::unsupported())
    }
    fn repaint(&mut self, _: OperationId, _: &OverlayVisual) -> Result<(), VisualOverlayError> {
        Err(VisualOverlayError::unsupported())
    }
    fn poll_input(&mut self) -> Result<Vec<OverlayInput>, VisualOverlayError> {
        Ok(vec![])
    }
    fn close(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn advance_and_drain(controller: &mut VisualOverlayController) -> Vec<VisualOverlayEvent> {
        controller.advance();
        controller.drain_events()
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum RecordedCall {
        Show {
            operation_id: OperationId,
            visual: OverlayVisual,
            mouse_transparent: bool,
        },
        Repaint {
            operation_id: OperationId,
            visual: OverlayVisual,
        },
        Poll,
        Close,
    }

    struct FakeData {
        left_down: bool,
        cursor: MkPoint,
        inputs: Vec<OverlayInput>,
        calls: Vec<RecordedCall>,
        show_error: Option<VisualOverlayError>,
        poll_error: Option<VisualOverlayError>,
    }
    impl Default for FakeData {
        fn default() -> Self {
            Self {
                left_down: false,
                cursor: point(40, 50),
                inputs: vec![],
                calls: vec![],
                show_error: None,
                poll_error: None,
            }
        }
    }
    struct FakeRenderer(Arc<Mutex<FakeData>>);
    impl OverlayRenderer for FakeRenderer {
        fn left_button_down(&mut self) -> Result<bool, VisualOverlayError> {
            Ok(self.0.lock().unwrap().left_down)
        }
        fn cursor_position(&mut self) -> Result<MkPoint, VisualOverlayError> {
            Ok(self.0.lock().unwrap().cursor)
        }
        fn show(
            &mut self,
            operation_id: OperationId,
            visual: &OverlayVisual,
            transparent: bool,
        ) -> Result<(), VisualOverlayError> {
            let mut data = self.0.lock().unwrap();
            data.calls.push(RecordedCall::Show {
                operation_id,
                visual: visual.clone(),
                mouse_transparent: transparent,
            });
            match data.show_error.take() {
                Some(error) => Err(error),
                None => Ok(()),
            }
        }
        fn repaint(
            &mut self,
            operation_id: OperationId,
            visual: &OverlayVisual,
        ) -> Result<(), VisualOverlayError> {
            self.0.lock().unwrap().calls.push(RecordedCall::Repaint {
                operation_id,
                visual: visual.clone(),
            });
            Ok(())
        }
        fn poll_input(&mut self) -> Result<Vec<OverlayInput>, VisualOverlayError> {
            let mut data = self.0.lock().unwrap();
            data.calls.push(RecordedCall::Poll);
            if let Some(error) = data.poll_error.take() {
                Err(error)
            } else {
                Ok(std::mem::take(&mut data.inputs))
            }
        }
        fn close(&mut self) {
            self.0.lock().unwrap().calls.push(RecordedCall::Close);
        }
    }
    struct FakeClock(Arc<Mutex<Duration>>);
    impl OverlayClock for FakeClock {
        fn now(&self) -> Duration {
            *self.0.lock().unwrap()
        }
    }
    fn controller() -> (
        VisualOverlayController,
        Arc<Mutex<FakeData>>,
        Arc<Mutex<Duration>>,
    ) {
        let data = Arc::new(Mutex::new(FakeData::default()));
        let now = Arc::new(Mutex::new(Duration::ZERO));
        (
            VisualOverlayController::with_clock(
                Box::new(FakeRenderer(data.clone())),
                Box::new(FakeClock(now.clone())),
            ),
            data,
            now,
        )
    }
    fn point(x: i32, y: i32) -> MkPoint {
        MkPoint { x, y }
    }

    fn point_request() -> VisualPointRequest {
        VisualPointRequest {
            macro_id: 9,
            draft_generation: 4,
            step_id: Some(77),
            destination: VisualPointDestination::SetVariablePoint,
        }
    }

    #[test]
    fn point_picker_arms_then_confirms_once_and_preserves_negative_coordinates() {
        let (mut c, data, _) = controller();
        let id = c.begin_point_pick(point_request());
        data.lock().unwrap().inputs.push(OverlayInput {
            operation_id: id,
            kind: OverlayInputKind::LeftPressed(point(1, 2)),
        });
        assert!(
            advance_and_drain(&mut c).is_empty(),
            "initiating press is ignored"
        );
        assert!(matches!(
            c.state(),
            VisualOverlayState::PickingPoint {
                phase: PointInteractionPhase::AwaitingInitialRelease,
                ..
            }
        ));
        data.lock().unwrap().inputs.push(OverlayInput {
            operation_id: id,
            kind: OverlayInputKind::LeftReleased(point(1, 2)),
        });
        assert!(advance_and_drain(&mut c).is_empty());
        assert!(matches!(
            c.state(),
            VisualOverlayState::PickingPoint {
                phase: PointInteractionPhase::Armed,
                ..
            }
        ));
        data.lock().unwrap().cursor = point(-321, -45);
        data.lock().unwrap().inputs.extend([
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::LeftPressed(point(0, 0)),
            },
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::LeftPressed(point(0, 0)),
            },
        ]);
        assert_eq!(
            advance_and_drain(&mut c),
            vec![VisualOverlayEvent::PointConfirmed {
                operation_id: id,
                request: point_request(),
                point: point(-321, -45)
            }]
        );
        assert_eq!(c.state(), &VisualOverlayState::Idle);
        assert!(advance_and_drain(&mut c).is_empty());
    }

    #[test]
    fn point_picker_held_launch_button_must_release_before_next_click() {
        let (mut c, data, _) = controller();
        data.lock().unwrap().left_down = true;
        let id = c.begin_point_pick(point_request());

        for _ in 0..3 {
            assert!(advance_and_drain(&mut c).is_empty());
            assert!(matches!(
                c.state(),
                VisualOverlayState::PickingPoint {
                    phase: PointInteractionPhase::AwaitingInitialRelease,
                    ..
                }
            ));
        }
        data.lock().unwrap().inputs.push(OverlayInput {
            operation_id: id,
            kind: OverlayInputKind::LeftReleased(point(12, 34)),
        });
        assert!(advance_and_drain(&mut c).is_empty());
        assert!(matches!(
            c.state(),
            VisualOverlayState::PickingPoint {
                phase: PointInteractionPhase::Armed,
                ..
            }
        ));

        data.lock().unwrap().cursor = point(-320, 650);
        data.lock().unwrap().inputs.push(OverlayInput {
            operation_id: id,
            kind: OverlayInputKind::LeftPressed(point(-320, 650)),
        });
        assert_eq!(
            advance_and_drain(&mut c),
            vec![VisualOverlayEvent::PointConfirmed {
                operation_id: id,
                request: point_request(),
                point: point(-320, 650),
            }]
        );
    }

    #[test]
    fn point_picker_escape_cancels_before_and_after_arming() {
        for arm in [false, true] {
            let (mut c, data, _) = controller();
            let id = c.begin_point_pick(point_request());
            if arm {
                data.lock().unwrap().inputs.push(OverlayInput {
                    operation_id: id,
                    kind: OverlayInputKind::LeftReleased(point(0, 0)),
                });
                assert!(advance_and_drain(&mut c).is_empty());
            }
            data.lock().unwrap().inputs.push(OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::Escape,
            });
            assert_eq!(
                advance_and_drain(&mut c),
                vec![VisualOverlayEvent::Cancelled { operation_id: id }]
            );
            assert_eq!(c.state(), &VisualOverlayState::Idle);
        }
    }

    fn close_count(fake: &Arc<Mutex<FakeData>>) -> usize {
        fake.lock()
            .unwrap()
            .calls
            .iter()
            .filter(|call| matches!(call, RecordedCall::Close))
            .count()
    }

    #[test]
    fn geometry_normalizes_signed_coordinates_and_rejects_overflow() {
        assert_eq!(
            normalized_rect(point(10, 20), point(-10, -30)).unwrap(),
            ScreenRect::new(-10, -30, 20, 50)
        );
        assert_eq!(
            normalized_rect(point(-1920, 150), point(-100, 900)).unwrap(),
            ScreenRect::new(-1920, 150, 1820, 750)
        );
        assert_eq!(
            normalized_rect(point(50, -1080), point(900, -20)).unwrap(),
            ScreenRect::new(50, -1080, 850, 1060)
        );
        assert_eq!(normalized_rect(point(1, 1), point(1, 5)).unwrap().width, 0);
        assert_eq!(
            normalized_rect(point(i32::MIN, 0), point(i32::MAX, 1))
                .unwrap()
                .width,
            u32::MAX
        );
    }

    #[test]
    fn tooltip_text_is_deterministic_for_every_direction_and_negative_geometry() {
        let expected = "X: -20  Y: -30\nW: 30  H: 50\nRelease to select — Esc to cancel";
        for (a, b) in [
            (point(-20, -30), point(10, 20)),
            (point(10, -30), point(-20, 20)),
            (point(-20, 20), point(10, -30)),
            (point(10, 20), point(-20, -30)),
        ] {
            assert_eq!(
                rectangle_tooltip_text(Some(normalized_rect(a, b).unwrap())),
                expected
            );
        }
        assert_eq!(rectangle_tooltip_text(None), RECTANGLE_INSTRUCTION);
    }

    #[test]
    fn tooltip_placement_flips_clamps_and_supports_signed_offset_monitors() {
        let monitor = ScreenRect::new(-1000, 200, 800, 600);
        assert_eq!(
            place_rectangle_tooltip(point(-990, 210), (100, 50), monitor, (16, 16)),
            point(-974, 226)
        );
        assert_eq!(
            place_rectangle_tooltip(point(-210, 790), (100, 50), monitor, (16, 16)),
            point(-326, 724)
        );
        assert_eq!(
            place_rectangle_tooltip(point(-500, 500), (900, 700), monitor, (16, 16)),
            point(-1000, 200)
        );
        assert_eq!(
            monitor_nearest_pointer(
                &[monitor, ScreenRect::new(0, -500, 500, 500)],
                point(-20, -20)
            ),
            Some(ScreenRect::new(0, -500, 500, 500))
        );
    }

    #[test]
    fn picker_shows_initial_hint_and_moves_replace_geometry_and_hint_together() {
        let (mut c, fake, _) = controller();
        let id = c.begin_rectangle_pick(
            RectanglePurpose::SearchRegion,
            ScreenRect::new(-100, -100, 400, 400),
        );
        let calls = fake.lock().unwrap().calls.clone();
        assert!(
            matches!(&calls[0], RecordedCall::Show { visual: OverlayVisual::RectanglePicker { selection: None, tooltip, .. }, .. } if tooltip.text == RECTANGLE_INSTRUCTION && tooltip.pointer == point(40, 50))
        );
        fake.lock().unwrap().inputs = vec![
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::LeftPressed(point(10, 20)),
            },
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::PointerMoved(point(-10, -20)),
            },
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::PointerMoved(point(30, 40)),
            },
        ];
        assert!(advance_and_drain(&mut c).is_empty());
        let repaints: Vec<_> = fake
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter_map(|call| match call {
                RecordedCall::Repaint { visual, .. } => Some(visual.clone()),
                _ => None,
            })
            .collect();
        for visual in &repaints {
            let OverlayVisual::RectanglePicker {
                selection, tooltip, ..
            } = visual
            else {
                panic!()
            };
            let frame = overlay_frame(visual);
            assert_eq!(frame.first(), Some(&OverlayFramePrimitive::Clear));
            assert_eq!(
                frame.get(1),
                selection.map(OverlayFramePrimitive::Outline).as_ref()
            );
            assert_eq!(tooltip.text, rectangle_tooltip_text(*selection));
        }
        assert!(
            matches!(repaints.last(), Some(OverlayVisual::RectanglePicker { selection: Some(r), .. }) if *r == ScreenRect::new(10, 20, 20, 20))
        );
    }

    #[test]
    fn half_open_drag_geometry_is_identical_in_all_four_directions() {
        let expected = ScreenRect::new(500, 300, 700, 600);
        for (a, b) in [
            (point(500, 300), point(1200, 900)),
            (point(1200, 300), point(500, 900)),
            (point(500, 900), point(1200, 300)),
            (point(1200, 900), point(500, 300)),
        ] {
            assert_eq!(normalized_rect(a, b).unwrap(), expected);
        }
    }

    fn monitor(index: usize, bounds: ScreenRect) -> MonitorDescriptor {
        MonitorDescriptor {
            index,
            bounds,
            primary: index == 1,
        }
    }

    #[test]
    fn passive_outline_geometry_is_exact_for_normal_negative_and_tiny_rectangles() {
        assert_eq!(
            outline_edge_rects(ScreenRect::new(10, 20, 100, 50)),
            [
                (OutlineEdge::Top, ScreenRect::new(10, 20, 100, 3)),
                (OutlineEdge::Right, ScreenRect::new(107, 20, 3, 50)),
                (OutlineEdge::Bottom, ScreenRect::new(10, 67, 100, 3)),
                (OutlineEdge::Left, ScreenRect::new(10, 20, 3, 50)),
            ]
        );
        let negative = outline_edge_rects(ScreenRect::new(-20, -10, 8, 7));
        assert_eq!(negative[2].1, ScreenRect::new(-20, -6, 8, 3));
        let tiny = outline_edge_rects(ScreenRect::new(-2, -3, 2, 1));
        assert!(tiny.iter().all(|(_, r)| r.width > 0 && r.height > 0));
        assert_eq!(tiny[3].1, ScreenRect::new(-2, -3, 2, 1));
    }

    fn rectangle_picker_visual(virtual_desktop: ScreenRect) -> OverlayVisual {
        OverlayVisual::RectanglePicker {
            virtual_desktop,
            selection: None,
            tooltip: RectangleTooltip {
                text: rectangle_tooltip_text(None),
                pointer: point(0, 0),
            },
        }
    }

    #[test]
    fn only_rectangle_picker_uses_per_monitor_input_shields() {
        let displays = vec![
            ScreenRect::new(-1920, -200, 1920, 1080),
            ScreenRect::new(0, 0, 2560, 1440),
            ScreenRect::new(3000, -900, 1200, 800),
        ];
        let picker = rectangle_picker_visual(ScreenRect::new(-2000, -300, 6400, 2200));
        assert_eq!(
            overlay_input_mode(&picker),
            OverlayInputMode::PerMonitorShields
        );
        assert!(overlay_requires_input_shields(&picker));
        assert_eq!(input_shield_plan(&picker, &displays), displays);
        assert_eq!(
            input_shield_plan(&picker, &displays).len(),
            3,
            "one shield specification is produced per intersecting monitor"
        );

        let passive_visuals = vec![
            OverlayVisual::RectanglePreview(ScreenRect::new(-10, -20, 30, 40)),
            OverlayVisual::Monitor(monitor(1, displays[0])),
            OverlayVisual::Monitors(vec![monitor(1, displays[0]), monitor(2, displays[2])]),
            OverlayVisual::Desktop(vec![monitor(1, displays[0]), monitor(2, displays[2])]),
            OverlayVisual::Window {
                rect: ScreenRect::new(10, 20, 30, 40),
                area_kind: WindowAreaKind::ClientArea,
            },
            OverlayVisual::PointPicker {
                pointer: point(-20, -30),
            },
        ];
        for visual in passive_visuals {
            assert_eq!(overlay_input_mode(&visual), OverlayInputMode::PollingOnly);
            assert!(!overlay_requires_input_shields(&visual));
            assert!(input_shield_plan(&visual, &displays).is_empty());
        }
    }

    #[test]
    fn shield_plan_preserves_negative_origins_mixed_sizes_and_offsets() {
        let displays = [
            ScreenRect::new(-2560, -1440, 2560, 1440),
            ScreenRect::new(0, -300, 1920, 1080),
            ScreenRect::new(2100, 200, 1280, 720),
            ScreenRect::new(4000, 3000, 800, 600),
        ];
        let picker = rectangle_picker_visual(ScreenRect::new(-2600, -400, 6200, 1600));
        assert_eq!(
            input_shield_plan(&picker, &displays),
            displays[..3].to_vec(),
            "shield bounds retain each physical monitor's signed ScreenRect"
        );
    }
    fn assert_four_bright_yellow_edges(
        visual: OverlayVisual,
        target: ScreenRect,
        displays: &[ScreenRect],
    ) {
        let plan = passive_overlay_plan(&visual, displays).unwrap();
        let expected: Vec<_> = outline_edge_rects(target)
            .into_iter()
            .map(|(edge, rect)| PassiveWindowSpec::Edge {
                edge,
                target,
                rect,
                style: PassiveWindowStyle::BrightYellow,
            })
            .collect();
        assert_eq!(plan, expected);
    }

    #[test]
    fn passive_single_target_variants_have_exact_order_bounds_clipping_and_style() {
        let display = ScreenRect::new(-500, -300, 1600, 1000);
        let rectangle = ScreenRect::new(100, 100, 500, 300);
        assert_four_bright_yellow_edges(
            OverlayVisual::RectanglePreview(rectangle),
            rectangle,
            &[display],
        );

        let monitor_descriptor = monitor(4, ScreenRect::new(-400, -200, 400, 250));
        assert_four_bright_yellow_edges(
            OverlayVisual::Monitor(monitor_descriptor.clone()),
            monitor_descriptor.bounds,
            &[display],
        );

        for area_kind in [WindowAreaKind::WholeWindow, WindowAreaKind::ClientArea] {
            let supplied_bounds = ScreenRect::new(-125, 40, 225, 175);
            assert_four_bright_yellow_edges(
                OverlayVisual::Window {
                    rect: supplied_bounds,
                    area_kind,
                },
                supplied_bounds,
                &[display],
            );
        }
    }

    #[test]
    fn desktop_preview_preserves_negative_monitor_topology_and_edge_order() {
        let left = monitor(7, ScreenRect::new(-600, -100, 300, 200));
        let right = monitor(2, ScreenRect::new(100, 50, 400, 250));
        let plan = passive_overlay_plan(
            &OverlayVisual::Desktop(vec![right.clone(), left.clone()]),
            &[left.bounds, right.bounds],
        )
        .unwrap();
        let mut expected = Vec::new();
        for target in [left.bounds, right.bounds] {
            expected.extend(outline_edge_rects(target).map(|(edge, rect)| {
                PassiveWindowSpec::Edge {
                    edge,
                    target,
                    rect,
                    style: PassiveWindowStyle::BrightYellow,
                }
            }));
        }
        assert_eq!(plan, expected);
        assert!(plan.iter().all(|spec| match spec {
            PassiveWindowSpec::Edge { rect, .. } =>
                rect.right() <= left.bounds.right()
                    || i64::from(rect.x) >= i64::from(right.bounds.x),
            PassiveWindowSpec::Badge { .. } => false,
        }));
    }

    #[test]
    fn passive_plan_clips_cross_monitor_edges_and_never_bridges_a_gap() {
        let displays = [
            ScreenRect::new(-200, -50, 100, 100),
            ScreenRect::new(0, -50, 100, 100),
        ];
        let target = ScreenRect::new(-150, -20, 200, 60);
        let plan =
            passive_overlay_plan(&OverlayVisual::RectanglePreview(target), &displays).unwrap();
        let edges: Vec<_> = plan
            .iter()
            .filter_map(|s| match s {
                PassiveWindowSpec::Edge { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect();
        assert!(edges.iter().all(|r| r.right() <= -100 || r.x >= 0));
        assert_eq!(
            plan.iter()
                .filter(|spec| matches!(spec, PassiveWindowSpec::Edge {
                edge: OutlineEdge::Top,
                target: actual_target,
                style: PassiveWindowStyle::BrightYellow,
                ..
            } if *actual_target == target))
                .count(),
            2
        );
        assert!(
            edges
                .iter()
                .any(|r| *r == ScreenRect::new(-150, -20, 50, 3))
        );
        assert!(edges.iter().any(|r| *r == ScreenRect::new(0, -20, 50, 3)));
    }

    #[test]
    fn passive_variants_have_the_required_outline_and_badge_semantics() {
        let a = monitor(2, ScreenRect::new(-100, 0, 100, 80));
        let b = monitor(1, ScreenRect::new(0, 0, 120, 80));
        let displays = [a.bounds, b.bounds];
        let desktop = passive_overlay_plan(
            &OverlayVisual::Desktop(vec![b.clone(), a.clone()]),
            &displays,
        )
        .unwrap();
        assert_eq!(
            desktop
                .iter()
                .filter(|s| matches!(s, PassiveWindowSpec::Edge { .. }))
                .count(),
            8
        );
        assert!(
            !desktop
                .iter()
                .any(|s| matches!(s, PassiveWindowSpec::Badge { .. }))
        );
        let selected = passive_overlay_plan(&OverlayVisual::Monitor(a.clone()), &displays).unwrap();
        assert_eq!(selected.len(), 4);
        let identified =
            passive_overlay_plan(&OverlayVisual::Monitors(vec![a, b]), &displays).unwrap();
        assert_eq!(
            identified
                .iter()
                .filter(|s| matches!(s, PassiveWindowSpec::Badge { .. }))
                .count(),
            2
        );
        for kind in [WindowAreaKind::WholeWindow, WindowAreaKind::ClientArea] {
            let rect = ScreenRect::new(-50, 5, 30, 20);
            let plan = passive_overlay_plan(
                &OverlayVisual::Window {
                    rect,
                    area_kind: kind,
                },
                &displays,
            )
            .unwrap();
            assert!(plan.iter().any(|s| matches!(s, PassiveWindowSpec::Edge { edge: OutlineEdge::Top, rect: r, .. } if *r == ScreenRect::new(-50, 5, 30, 3))));
        }
        assert!(
            passive_overlay_plan(
                &OverlayVisual::RectanglePicker {
                    virtual_desktop: displays[0],
                    selection: None,
                    tooltip: RectangleTooltip {
                        text: String::new(),
                        pointer: point(0, 0)
                    },
                },
                &displays
            )
            .is_none()
        );
    }

    #[test]
    fn monitor_selection_handles_negative_origins_spans_and_irregular_gaps() {
        let monitors = [
            ScreenRect::new(-1920, -200, 1920, 1080),
            ScreenRect::new(0, 0, 1920, 1080),
            ScreenRect::new(2500, -1200, 1200, 900),
        ];
        assert_eq!(
            intersecting_monitor_bounds(&monitors, ScreenRect::new(-100, -100, 300, 300)),
            vec![monitors[0], monitors[1]]
        );
        assert!(
            intersecting_monitor_bounds(&monitors, ScreenRect::new(2100, 100, 200, 200)).is_empty()
        );
        assert_eq!(
            desktop_to_overlay(ScreenRect::new(-1800, -100, 20, 30), monitors[0]),
            (120, 100, 140, 130)
        );
    }

    #[test]
    fn monitor_identification_union_preserves_signed_extents_and_gaps() {
        let monitors = vec![
            monitor(1, ScreenRect::new(-1600, 100, 1600, 900)),
            monitor(2, ScreenRect::new(400, -800, 1000, 700)),
        ];
        assert_eq!(
            monitor_union(&monitors),
            Some(ScreenRect::new(-1600, -800, 3000, 1800))
        );
        assert_eq!(monitor_union(&[]), None);
    }

    #[test]
    fn picker_style_is_interactive_and_passive_styles_are_mouse_transparent() {
        let picker = OverlayVisual::RectanglePicker {
            virtual_desktop: ScreenRect::new(0, 0, 1, 1),
            selection: None,
            tooltip: RectangleTooltip {
                text: rectangle_tooltip_text(None),
                pointer: point(0, 0),
            },
        };
        assert!(!overlay_is_mouse_transparent(&picker));
        assert!(overlay_is_mouse_transparent(
            &OverlayVisual::RectanglePreview(ScreenRect::new(0, 0, 1, 1))
        ));
        assert!(overlay_is_mouse_transparent(&OverlayVisual::Monitor(
            monitor(1, ScreenRect::new(0, 0, 1, 1))
        )));
    }

    #[test]
    fn every_repaint_frame_clears_before_drawing_the_new_selection() {
        let old = ScreenRect::new(-20, -20, 10, 10);
        let new = ScreenRect::new(20, 20, 30, 30);
        for selection in [Some(old), Some(new), None] {
            let frame = overlay_frame(&OverlayVisual::RectanglePicker {
                virtual_desktop: ScreenRect::new(-100, -100, 200, 200),
                selection,
                tooltip: RectangleTooltip {
                    text: rectangle_tooltip_text(selection),
                    pointer: point(0, 0),
                },
            });
            assert_eq!(frame.first(), Some(&OverlayFramePrimitive::Clear));
            assert_eq!(
                frame.get(1),
                selection.map(OverlayFramePrimitive::Outline).as_ref()
            );
        }
    }

    #[test]
    fn pick_confirms_purpose_and_ignores_stale_input() {
        let (mut c, fake, _) = controller();
        let id = c.begin_rectangle_pick(
            RectanglePurpose::ReferenceImageCapture,
            ScreenRect::new(-1920, -1080, 3840, 2160),
        );
        fake.lock().unwrap().inputs = vec![
            OverlayInput {
                operation_id: id.wrapping_add(1),
                kind: OverlayInputKind::Escape,
            },
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::LeftPressed(point(-100, 900)),
            },
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::PointerMoved(point(-1920, 150)),
            },
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::LeftReleased(point(-1920, 150)),
            },
        ];
        assert_eq!(
            advance_and_drain(&mut c),
            vec![VisualOverlayEvent::RectangleConfirmed {
                operation_id: id,
                purpose: RectanglePurpose::ReferenceImageCapture,
                rect: ScreenRect::new(-1920, 150, 1820, 750)
            }]
        );
        assert_eq!(c.state(), &VisualOverlayState::Idle);
    }

    fn queue_drag(fake: &Arc<Mutex<FakeData>>, id: OperationId, start: MkPoint, end: MkPoint) {
        fake.lock().unwrap().inputs = vec![
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::LeftPressed(start),
            },
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::LeftReleased(end),
            },
        ];
    }

    #[test]
    fn dragging_from_bottom_right_to_top_left_emits_a_normalized_screen_rect() {
        let (mut c, fake, _) = controller();
        let id = c.begin_rectangle_pick(
            RectanglePurpose::SearchRegion,
            ScreenRect::new(0, 0, 1920, 1080),
        );
        queue_drag(&fake, id, point(900, 700), point(100, 200));
        assert_eq!(
            advance_and_drain(&mut c),
            vec![VisualOverlayEvent::RectangleConfirmed {
                operation_id: id,
                purpose: RectanglePurpose::SearchRegion,
                rect: ScreenRect::new(100, 200, 800, 500),
            }]
        );
    }

    #[test]
    fn dragging_across_negative_virtual_desktop_coordinates_normalizes_correctly() {
        let (mut c, fake, _) = controller();
        let id = c.begin_rectangle_pick(
            RectanglePurpose::ReferenceImageCapture,
            ScreenRect::new(-2560, -1200, 4480, 2280),
        );
        queue_drag(&fake, id, point(400, 300), point(-2100, -900));
        assert_eq!(
            advance_and_drain(&mut c),
            vec![VisualOverlayEvent::RectangleConfirmed {
                operation_id: id,
                purpose: RectanglePurpose::ReferenceImageCapture,
                rect: ScreenRect::new(-2100, -900, 2500, 1200),
            }]
        );
    }

    #[test]
    fn zero_width_or_zero_height_drags_cannot_be_confirmed() {
        for (start, end) in [
            (point(10, 20), point(10, 80)),
            (point(10, 20), point(80, 20)),
        ] {
            let (mut c, fake, _) = controller();
            let id = c.begin_rectangle_pick(
                RectanglePurpose::SearchRegion,
                ScreenRect::new(0, 0, 100, 100),
            );
            queue_drag(&fake, id, start, end);
            assert!(
                advance_and_drain(&mut c).is_empty(),
                "degenerate drag {start:?} -> {end:?} emitted an event"
            );
            assert_eq!(c.operation_id(), Some(id));
            assert!(matches!(
                c.state(),
                VisualOverlayState::PickingRectangle { .. }
            ));
            assert_eq!(close_count(&fake), 0);
        }
    }

    #[test]
    fn highlight_window_forwards_signed_bounds_area_kind_and_active_operation_id() {
        let (mut c, fake, _) = controller();
        let whole = ScreenRect::new(-1800, 25, 700, 500);
        let whole_id = c.highlight_window(whole, WindowAreaKind::WholeWindow);
        let client = ScreenRect::new(-1750, 70, 620, 410);
        let client_id = c.highlight_window(client, WindowAreaKind::ClientArea);
        let calls = fake.lock().unwrap().calls.clone();
        assert!(calls.contains(&RecordedCall::Show {
            operation_id: whole_id,
            visual: OverlayVisual::Window {
                rect: whole,
                area_kind: WindowAreaKind::WholeWindow
            },
            mouse_transparent: true,
        }));
        assert!(calls.contains(&RecordedCall::Show {
            operation_id: client_id,
            visual: OverlayVisual::Window {
                rect: client,
                area_kind: WindowAreaKind::ClientArea
            },
            mouse_transparent: true,
        }));
        assert_eq!(c.operation_id(), Some(client_id));
    }

    #[test]
    fn identify_monitors_forwards_every_descriptor_once_in_defined_order() {
        let descriptors = vec![
            MonitorDescriptor {
                index: 0,
                bounds: ScreenRect::new(0, 0, 1920, 1080),
                primary: true,
            },
            monitor(7, ScreenRect::new(-1600, 120, 1600, 900)),
            monitor(12, ScreenRect::new(200, -1200, 1200, 1200)),
        ];
        let (mut c, fake, _) = controller();
        let id = c.identify_monitors(descriptors.clone());
        assert_eq!(
            fake.lock().unwrap().calls.first(),
            Some(&RecordedCall::Show {
                operation_id: id,
                visual: OverlayVisual::Monitors(descriptors),
                mouse_transparent: true,
            })
        );
    }

    #[test]
    fn highlight_monitor_forwards_index_bounds_and_applies_passive_timeout() {
        let descriptor = monitor(42, ScreenRect::new(-2560, -200, 2560, 1440));
        let (mut c, fake, now) = controller();
        *now.lock().unwrap() = Duration::from_secs(3);
        let id = c.highlight_monitor(descriptor.clone());
        assert_eq!(
            fake.lock().unwrap().calls.first(),
            Some(&RecordedCall::Show {
                operation_id: id,
                visual: OverlayVisual::Monitor(descriptor.clone()),
                mouse_transparent: true,
            })
        );
        assert_eq!(
            c.state(),
            &VisualOverlayState::HighlightingMonitor {
                descriptor,
                expires_at: Duration::from_secs(3) + PASSIVE_OVERLAY_DURATION,
            }
        );
    }

    #[test]
    fn passive_overlay_remains_active_before_duration_and_expires_at_deadline_once() {
        let (mut c, fake, now) = controller();
        let id = c.preview_rectangle(ScreenRect::new(-10, -20, 30, 40));
        *now.lock().unwrap() = PASSIVE_OVERLAY_DURATION - Duration::from_nanos(1);
        assert!(advance_and_drain(&mut c).is_empty());
        assert_eq!(c.operation_id(), Some(id));
        *now.lock().unwrap() = PASSIVE_OVERLAY_DURATION;
        assert_eq!(
            advance_and_drain(&mut c),
            vec![VisualOverlayEvent::Expired { operation_id: id }]
        );
        assert_eq!(close_count(&fake), 1);
        assert!(advance_and_drain(&mut c).is_empty());
        assert_eq!(close_count(&fake), 1);
    }

    #[test]
    fn replacing_passive_or_interactive_operation_cleans_up_before_showing_replacement() {
        let (mut c, fake, _) = controller();
        let first = c.preview_rectangle(ScreenRect::new(0, 0, 10, 10));
        let second = c.begin_rectangle_pick(
            RectanglePurpose::SearchRegion,
            ScreenRect::new(0, 0, 20, 20),
        );
        let third = c.highlight_monitor(monitor(3, ScreenRect::new(-20, 0, 20, 20)));
        let calls = fake.lock().unwrap().calls.clone();
        let significant: Vec<_> = calls
            .iter()
            .filter(|call| !matches!(call, RecordedCall::Poll))
            .collect();
        assert!(matches!(significant.as_slice(), [
            RecordedCall::Show { operation_id: a, .. }, RecordedCall::Close,
            RecordedCall::Show { operation_id: b, .. }, RecordedCall::Close,
            RecordedCall::Show { operation_id: d, .. }
        ] if *a == first && *b == second && *d == third));
    }

    #[test]
    fn escape_emits_one_cancellation_for_active_rectangle_and_returns_to_idle() {
        let (mut c, fake, _) = controller();
        let id = c.begin_rectangle_pick(
            RectanglePurpose::SearchRegion,
            ScreenRect::new(0, 0, 20, 20),
        );
        fake.lock().unwrap().inputs = vec![OverlayInput {
            operation_id: id,
            kind: OverlayInputKind::Escape,
        }];
        assert_eq!(
            advance_and_drain(&mut c),
            vec![VisualOverlayEvent::Cancelled { operation_id: id }]
        );
        assert_eq!(c.state(), &VisualOverlayState::Idle);
        assert_eq!(c.operation_id(), None);
        assert_eq!(close_count(&fake), 1);
    }

    #[test]
    fn escape_empty_retry_replacement_expiration_and_shutdown_are_deterministic() {
        let (mut c, fake, now) = controller();
        let first = c.begin_rectangle_pick(
            RectanglePurpose::SearchRegion,
            ScreenRect::new(0, 0, 100, 100),
        );
        fake.lock().unwrap().inputs = vec![OverlayInput {
            operation_id: first,
            kind: OverlayInputKind::Enter,
        }];
        assert!(advance_and_drain(&mut c).is_empty()); // Enter cannot confirm an empty drag.
        let second = c.preview_rectangle(ScreenRect::new(-4, -5, 6, 7));
        assert_eq!(
            advance_and_drain(&mut c),
            vec![VisualOverlayEvent::Cancelled {
                operation_id: first
            }]
        );
        assert_eq!(
            fake.lock()
                .unwrap()
                .calls
                .iter()
                .filter_map(|call| match call {
                    RecordedCall::Show {
                        mouse_transparent, ..
                    } => Some(*mouse_transparent),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![false, true]
        );
        *now.lock().unwrap() = PASSIVE_OVERLAY_DURATION;
        assert_eq!(
            advance_and_drain(&mut c),
            vec![VisualOverlayEvent::Expired {
                operation_id: second
            }]
        );
        c.shutdown();
        let closes = close_count(&fake);
        c.shutdown();
        assert_eq!(close_count(&fake), closes);
    }

    fn platform_error(message: &str) -> VisualOverlayError {
        VisualOverlayError {
            kind: OverlayErrorKind::Platform,
            message: message.into(),
        }
    }

    #[test]
    fn stale_input_and_queued_error_are_ignored_after_operation_replacement() {
        let (mut c, fake, _) = controller();
        let first = c.identify_monitors(vec![monitor(1, ScreenRect::new(0, 0, 10, 10))]);
        c.events.push_back(VisualOverlayEvent::Error {
            operation_id: first,
            error: platform_error("stale"),
        });
        let second = c.highlight_window(ScreenRect::new(1, 2, 3, 4), WindowAreaKind::WholeWindow);
        fake.lock().unwrap().inputs = vec![OverlayInput {
            operation_id: first,
            kind: OverlayInputKind::Escape,
        }];
        assert_eq!(c.operation_id(), Some(second));
        assert_eq!(
            advance_and_drain(&mut c),
            vec![VisualOverlayEvent::Cancelled {
                operation_id: first
            }]
        );
        assert_eq!(close_count(&fake), 1);
    }

    #[test]
    fn partial_startup_and_poll_failures_close_and_return_to_idle_once() {
        let (mut c, fake, _) = controller();
        fake.lock().unwrap().show_error = Some(platform_error("partial startup"));
        let failed = c.preview_rectangle(ScreenRect::new(0, 0, 2, 2));
        assert_eq!(c.state(), &VisualOverlayState::Idle);
        assert_eq!(c.operation_id(), None);
        assert_eq!(close_count(&fake), 1, "partial startup is cleaned up");
        assert!(
            matches!(advance_and_drain(&mut c).as_slice(), [VisualOverlayEvent::Error { operation_id, .. }] if *operation_id == failed)
        );

        let active = c.preview_rectangle(ScreenRect::new(0, 0, 2, 2));
        fake.lock().unwrap().poll_error = Some(platform_error("poll failed"));
        assert!(
            matches!(advance_and_drain(&mut c).as_slice(), [VisualOverlayEvent::Error { operation_id, .. }] if *operation_id == active)
        );
        assert_eq!(close_count(&fake), 2, "poll failure is cleaned up");
        assert!(advance_and_drain(&mut c).is_empty());
        assert_eq!(c.state(), &VisualOverlayState::Idle);
    }

    #[test]
    fn repeated_escape_cancel_shutdown_poll_and_drop_do_not_duplicate_cleanup_or_late_events() {
        let (mut c, fake, _) = controller();
        let id =
            c.begin_rectangle_pick(RectanglePurpose::SearchRegion, ScreenRect::new(0, 0, 2, 2));
        fake.lock().unwrap().inputs = vec![
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::Escape,
            },
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::Escape,
            },
        ];
        assert_eq!(
            advance_and_drain(&mut c),
            vec![VisualOverlayEvent::Cancelled { operation_id: id }]
        );
        c.cancel();
        c.cancel();
        c.shutdown();
        c.shutdown();
        assert!(advance_and_drain(&mut c).is_empty());
        let closes = close_count(&fake);
        drop(c);
        assert_eq!(close_count(&fake), closes);

        let (mut c, fake, _) = controller();
        c.preview_rectangle(ScreenRect::new(0, 0, 2, 2));
        let before = close_count(&fake);
        drop(c);
        assert!(close_count(&fake) > before);
    }

    #[test]
    fn rectangle_picker_full_lifecycle_records_visuals_frames_tooltips_and_cleanup() {
        for purpose in [
            RectanglePurpose::SearchRegion,
            RectanglePurpose::CropScreenshot,
            RectanglePurpose::ReferenceImageCapture,
        ] {
            let (mut controller, fake, _) = controller();
            let desktop = ScreenRect::new(-100, -80, 400, 300);
            let id = controller.begin_rectangle_pick(purpose, desktop);
            fake.lock().unwrap().inputs = vec![OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::LeftPressed(point(20, 30)),
            }];
            assert!(advance_and_drain(&mut controller).is_empty());
            fake.lock().unwrap().inputs = vec![OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::PointerMoved(point(-40, -10)),
            }];
            assert!(advance_and_drain(&mut controller).is_empty());
            fake.lock().unwrap().inputs = vec![OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::LeftReleased(point(-40, -10)),
            }];
            assert_eq!(
                advance_and_drain(&mut controller),
                vec![VisualOverlayEvent::RectangleConfirmed {
                    operation_id: id,
                    purpose,
                    rect: ScreenRect::new(-40, -10, 60, 40),
                }]
            );
            assert!(matches!(
                fake.lock().unwrap().calls.last(),
                Some(RecordedCall::Close)
            ));
            assert_eq!(controller.state(), &VisualOverlayState::Idle);

            let calls = fake.lock().unwrap().calls.clone();
            let visuals: Vec<_> = calls
                .iter()
                .filter_map(|call| match call {
                    RecordedCall::Show {
                        operation_id,
                        visual,
                        mouse_transparent,
                    } => Some((*operation_id, visual.clone(), *mouse_transparent)),
                    RecordedCall::Repaint {
                        operation_id,
                        visual,
                    } => Some((*operation_id, visual.clone(), false)),
                    _ => None,
                })
                .collect();
            assert!(
                visuals.len() >= 3,
                "initial hint, press, and move/release visuals must be recorded"
            );
            assert!(
                visuals
                    .iter()
                    .all(|(operation_id, _, transparent)| *operation_id == id && !transparent)
            );
            assert!(
                matches!(&visuals[0].1, OverlayVisual::RectanglePicker { selection: None, tooltip, .. } if tooltip.text == RECTANGLE_INSTRUCTION)
            );
            assert!(visuals.iter().any(|(_, visual, _)| matches!(visual, OverlayVisual::RectanglePicker { selection: Some(rect), tooltip, .. }
                if *rect == ScreenRect::new(-40, -10, 60, 40) && tooltip.text == rectangle_tooltip_text(Some(*rect)))));
            for (_, visual, _) in &visuals {
                let frame = overlay_frame(visual);
                assert_eq!(frame.first(), Some(&OverlayFramePrimitive::Clear));
            }
            assert_eq!(close_count(&fake), 1);
        }
    }

    #[test]
    fn all_rectangle_purposes_have_identical_picker_presentation() {
        let mut presentations = Vec::new();
        for purpose in [
            RectanglePurpose::SearchRegion,
            RectanglePurpose::CropScreenshot,
            RectanglePurpose::ReferenceImageCapture,
        ] {
            let (mut controller, fake, _) = controller();
            let id = controller.begin_rectangle_pick(purpose, ScreenRect::new(-10, -20, 100, 80));
            fake.lock().unwrap().inputs = vec![
                OverlayInput {
                    operation_id: id,
                    kind: OverlayInputKind::LeftPressed(point(8, 9)),
                },
                OverlayInput {
                    operation_id: id,
                    kind: OverlayInputKind::PointerMoved(point(18, 29)),
                },
            ];
            assert!(advance_and_drain(&mut controller).is_empty());
            presentations.push(
                fake.lock()
                    .unwrap()
                    .calls
                    .iter()
                    .filter_map(|call| match call {
                        RecordedCall::Show {
                            visual,
                            mouse_transparent,
                            ..
                        } => Some((visual.clone(), *mouse_transparent, overlay_frame(visual))),
                        RecordedCall::Repaint { visual, .. } => {
                            Some((visual.clone(), false, overlay_frame(visual)))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            );
        }
        assert_eq!(presentations.len(), 3);
        assert!(presentations.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn escape_before_and_during_drag_closes_once_and_emits_one_cancellation() {
        for start_drag in [false, true] {
            let (mut controller, fake, _) = controller();
            let id = controller.begin_rectangle_pick(
                RectanglePurpose::SearchRegion,
                ScreenRect::new(0, 0, 100, 100),
            );
            let mut inputs = Vec::new();
            if start_drag {
                inputs.push(OverlayInput {
                    operation_id: id,
                    kind: OverlayInputKind::LeftPressed(point(10, 20)),
                });
                inputs.push(OverlayInput {
                    operation_id: id,
                    kind: OverlayInputKind::PointerMoved(point(30, 40)),
                });
            }
            inputs.push(OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::Escape,
            });
            fake.lock().unwrap().inputs = inputs;
            assert_eq!(
                advance_and_drain(&mut controller),
                vec![VisualOverlayEvent::Cancelled { operation_id: id }]
            );
            assert!(advance_and_drain(&mut controller).is_empty());
            assert_eq!(close_count(&fake), 1);
        }
    }

    #[test]
    fn held_launch_click_is_consumed_before_picker_arms() {
        let (mut c, fake, _) = controller();
        fake.lock().unwrap().left_down = true;
        let id = c.begin_rectangle_pick(
            RectanglePurpose::SearchRegion,
            ScreenRect::new(-100, -100, 200, 200),
        );
        assert!(matches!(
            c.state(),
            VisualOverlayState::PickingRectangle {
                phase: RectangleInteractionPhase::AwaitingInitialRelease,
                ..
            }
        ));
        fake.lock().unwrap().inputs = vec![OverlayInput {
            operation_id: id,
            kind: OverlayInputKind::LeftReleased(point(40, 50)),
        }];
        assert!(advance_and_drain(&mut c).is_empty());
        assert!(matches!(
            c.state(),
            VisualOverlayState::PickingRectangle {
                phase: RectangleInteractionPhase::Armed,
                ..
            }
        ));
        queue_drag(&fake, id, point(-80, -60), point(70, 90));
        assert_eq!(
            advance_and_drain(&mut c),
            vec![VisualOverlayEvent::RectangleConfirmed {
                operation_id: id,
                purpose: RectanglePurpose::SearchRegion,
                rect: ScreenRect::new(-80, -60, 150, 150),
            }]
        );
    }

    #[test]
    fn picker_without_launch_click_is_armed_and_never_times_out() {
        let (mut c, fake, now) = controller();
        let id = c.begin_rectangle_pick(
            RectanglePurpose::SearchRegion,
            ScreenRect::new(0, 0, 100, 100),
        );
        assert!(matches!(
            c.state(),
            VisualOverlayState::PickingRectangle {
                phase: RectangleInteractionPhase::Armed,
                ..
            }
        ));
        *now.lock().unwrap() = PASSIVE_OVERLAY_DURATION * 100;
        fake.lock().unwrap().inputs = vec![
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::LeftReleased(point(10, 10)),
            },
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::Enter,
            },
        ];
        assert!(advance_and_drain(&mut c).is_empty());
        assert_eq!(c.operation_id(), Some(id));
    }

    #[test]
    fn empty_release_rearms_picker_for_a_retry() {
        let (mut c, fake, _) = controller();
        let id = c.begin_rectangle_pick(
            RectanglePurpose::SearchRegion,
            ScreenRect::new(0, 0, 100, 100),
        );
        queue_drag(&fake, id, point(12, 12), point(12, 12));
        assert!(advance_and_drain(&mut c).is_empty());
        assert!(matches!(
            c.state(),
            VisualOverlayState::PickingRectangle {
                phase: RectangleInteractionPhase::Armed,
                ..
            }
        ));
        queue_drag(&fake, id, point(10, 10), point(30, 40));
        assert!(matches!(advance_and_drain(&mut c).as_slice(),
            [VisualOverlayEvent::RectangleConfirmed { rect, .. }] if *rect == ScreenRect::new(10, 10, 20, 30)));
    }

    fn wait_until(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !predicate() {
            assert!(
                Instant::now() < deadline,
                "service worker did not make progress"
            );
            thread::yield_now();
        }
    }

    fn test_service(
        data: Arc<Mutex<FakeData>>,
        now: Arc<Mutex<Duration>>,
    ) -> NativeVisualOverlayService {
        NativeVisualOverlayService::start_with_wait_interval(
            move || {
                VisualOverlayController::with_clock(
                    Box::new(FakeRenderer(data)),
                    Box::new(FakeClock(now)),
                )
            },
            Duration::ZERO,
        )
        .unwrap()
    }

    #[test]
    fn service_worker_finishes_held_click_drag_and_repaints_without_gui_drains() {
        let data = Arc::new(Mutex::new(FakeData {
            left_down: true,
            ..FakeData::default()
        }));
        let now = Arc::new(Mutex::new(Duration::ZERO));
        let mut service = test_service(data.clone(), now);
        let id = 701;
        service
            .commands
            .send(VisualOverlayCommand::BeginRectanglePick {
                operation_id: id,
                purpose: RectanglePurpose::ReferenceImageCapture,
                virtual_desktop: ScreenRect::new(-100, -100, 400, 300),
            })
            .unwrap();
        wait_until(|| {
            data.lock().unwrap().calls.iter().any(|call| matches!(call, RecordedCall::Show { operation_id, .. } if *operation_id == id))
        });

        // The launch release only arms the worker-owned picker. No event receiver is touched.
        data.lock().unwrap().inputs.push(OverlayInput {
            operation_id: id,
            kind: OverlayInputKind::LeftReleased(point(40, 50)),
        });
        wait_until(|| {
            data.lock().unwrap().calls.iter().filter(|call| matches!(call, RecordedCall::Repaint { operation_id, .. } if *operation_id == id)).count() >= 1
        });
        data.lock().unwrap().inputs.extend([
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::LeftPressed(point(10, 20)),
            },
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::PointerMoved(point(30, 40)),
            },
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::PointerMoved(point(50, 60)),
            },
            // Release is authoritative and intentionally differs from the final move.
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::LeftReleased(point(70, 80)),
            },
        ]);

        let event = service.events.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            event,
            VisualOverlayEvent::RectangleConfirmed {
                operation_id: id,
                purpose: RectanglePurpose::ReferenceImageCapture,
                rect: ScreenRect::new(10, 20, 60, 60),
            }
        );
        assert!(service.events.try_recv().is_err());
        let repaints = data.lock().unwrap().calls.iter().filter(|call| matches!(call, RecordedCall::Repaint { operation_id, .. } if *operation_id == id)).count();
        assert!(
            repaints >= 5,
            "arming, press, moves, and final release repaint on the worker"
        );
        assert_eq!(close_count(&data), 1);
        service.shutdown_and_join();
    }

    #[test]
    fn service_worker_cancels_on_escape_and_never_times_out_interactive_picker() {
        let data = Arc::new(Mutex::new(FakeData::default()));
        let now = Arc::new(Mutex::new(Duration::ZERO));
        let mut service = test_service(data.clone(), now.clone());
        let id = 702;
        service
            .commands
            .send(VisualOverlayCommand::BeginRectanglePick {
                operation_id: id,
                purpose: RectanglePurpose::SearchRegion,
                virtual_desktop: ScreenRect::new(0, 0, 100, 100),
            })
            .unwrap();
        wait_until(|| {
            data.lock().unwrap().calls.iter().any(|call| matches!(call, RecordedCall::Show { operation_id, .. } if *operation_id == id))
        });
        *now.lock().unwrap() = PASSIVE_OVERLAY_DURATION * 100;
        data.lock().unwrap().inputs.extend([
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::LeftPressed(point(1, 2)),
            },
            OverlayInput {
                operation_id: id,
                kind: OverlayInputKind::PointerMoved(point(8, 9)),
            },
        ]);
        wait_until(|| {
            data.lock().unwrap().calls.iter().any(|call| matches!(call, RecordedCall::Repaint { operation_id, visual: OverlayVisual::RectanglePicker { selection: Some(_), .. } } if *operation_id == id))
        });
        assert!(
            service.events.try_recv().is_err(),
            "interactive operations have no deadline"
        );
        data.lock().unwrap().inputs.push(OverlayInput {
            operation_id: id,
            kind: OverlayInputKind::Escape,
        });
        assert_eq!(
            service.events.recv_timeout(Duration::from_secs(2)).unwrap(),
            VisualOverlayEvent::Cancelled { operation_id: id }
        );
        assert!(service.events.try_recv().is_err());
        assert_eq!(close_count(&data), 1);
        service.shutdown_and_join();
    }

    #[test]
    fn every_passive_category_expires_and_closes_on_service_thread_without_gui_polling() {
        let data = Arc::new(Mutex::new(FakeData::default()));
        let now = Arc::new(Mutex::new(Duration::ZERO));
        let mut service = test_service(data.clone(), now.clone());
        let descriptor = monitor(2, ScreenRect::new(-100, 0, 100, 80));
        let commands = vec![
            VisualOverlayCommand::PreviewRectangle {
                operation_id: 801,
                rect: ScreenRect::new(1, 2, 30, 40),
            },
            VisualOverlayCommand::HighlightMonitor {
                operation_id: 802,
                monitor: descriptor.clone(),
            },
            VisualOverlayCommand::IdentifyMonitors {
                operation_id: 803,
                monitors: vec![descriptor],
            },
            VisualOverlayCommand::HighlightWindow {
                operation_id: 804,
                rect: ScreenRect::new(5, 6, 70, 80),
                area_kind: WindowAreaKind::ClientArea,
            },
        ];
        for (index, command) in commands.into_iter().enumerate() {
            let id = 801 + index as u64;
            let shows_before = data
                .lock()
                .unwrap()
                .calls
                .iter()
                .filter(|call| matches!(call, RecordedCall::Show { .. }))
                .count();
            service.commands.send(command).unwrap();
            wait_until(|| {
                data.lock()
                    .unwrap()
                    .calls
                    .iter()
                    .filter(|call| matches!(call, RecordedCall::Show { .. }))
                    .count()
                    > shows_before
            });
            // Advancing the clock is independent of receiving/draining semantic events.
            *now.lock().unwrap() += PASSIVE_OVERLAY_DURATION;
            assert_eq!(
                service.events.recv_timeout(Duration::from_secs(2)).unwrap(),
                VisualOverlayEvent::Expired { operation_id: id }
            );
            assert_eq!(close_count(&data), index + 1);
        }
        service.shutdown_and_join();
    }
}
