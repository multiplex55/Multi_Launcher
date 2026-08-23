//! One stateful boundary for all macro-authoring desktop overlays.
//!
//! The controller deliberately deals only in signed desktop coordinates.  A
//! native implementation is responsible for converting window-client input to
//! desktop coordinates before returning an [`OverlayInput`].
use crate::mkmacro::{MkPoint, MonitorDescriptor, ScreenRect};
use std::{
    collections::VecDeque,
    fmt,
    time::{Duration, Instant},
};

/// Passive overlays remain visible long enough to be recognized without
/// becoming persistent desktop furniture.
pub const PASSIVE_OVERLAY_DURATION: Duration = Duration::from_millis(2500);

pub type OperationId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RectanglePurpose {
    SearchRegion,
    ReferenceImageCapture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAreaKind {
    WholeWindow,
    ClientArea,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisualOverlayState {
    Idle,
    PickingRectangle {
        start: Option<MkPoint>,
        current: Option<MkPoint>,
        purpose: RectanglePurpose,
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
    RectanglePicker {
        virtual_desktop: ScreenRect,
        selection: Option<ScreenRect>,
    },
    RectanglePreview(ScreenRect),
    Monitor(MonitorDescriptor),
    Monitors(Vec<MonitorDescriptor>),
    Window {
        rect: ScreenRect,
        area_kind: WindowAreaKind,
    },
}
impl OverlayVisual {
    fn passive(&self) -> bool {
        !matches!(self, Self::RectanglePicker { .. })
    }
}

/// Isolates native windows, input, painting and resource ownership from the
/// deterministic state machine.
pub trait OverlayRenderer: Send {
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
    fn replace(&mut self) -> OperationId {
        if let Some(id) = self.operation_id.take() {
            self.renderer.close();
            self.events
                .push_back(VisualOverlayEvent::Cancelled { operation_id: id });
        }
        self.state = VisualOverlayState::Idle;
        self.virtual_desktop = None;
        self.allocate()
    }
    fn start(&mut self, state: VisualOverlayState, visual: OverlayVisual) -> OperationId {
        let id = self.replace();
        self.shut_down = false;
        match self.renderer.show(id, &visual, visual.passive()) {
            Ok(()) => {
                self.operation_id = Some(id);
                self.state = state;
            }
            Err(error) => self.events.push_back(VisualOverlayEvent::Error {
                operation_id: id,
                error,
            }),
        }
        id
    }
    pub fn begin_rectangle_pick(
        &mut self,
        purpose: RectanglePurpose,
        virtual_desktop: ScreenRect,
    ) -> OperationId {
        if virtual_desktop.is_empty()
            || virtual_desktop.right() > i64::from(i32::MAX) + 1
            || virtual_desktop.bottom() > i64::from(i32::MAX) + 1
        {
            let id = self.replace();
            self.events.push_back(VisualOverlayEvent::Error {
                operation_id: id,
                error: VisualOverlayError::geometry(
                    "virtual desktop is empty or exceeds signed coordinate bounds",
                ),
            });
            return id;
        }
        let id = self.start(
            VisualOverlayState::PickingRectangle {
                start: None,
                current: None,
                purpose,
            },
            OverlayVisual::RectanglePicker {
                virtual_desktop,
                selection: None,
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
    pub fn cancel(&mut self) {
        if let Some(id) = self.operation_id.take() {
            self.renderer.close();
            self.events
                .push_back(VisualOverlayEvent::Cancelled { operation_id: id });
        }
        self.state = VisualOverlayState::Idle;
        self.virtual_desktop = None;
    }
    pub fn shutdown(&mut self) {
        if self.shut_down {
            return;
        }
        self.cancel();
        self.renderer.close();
        self.shut_down = true;
    }
    pub fn poll(&mut self) -> Vec<VisualOverlayEvent> {
        let input_result = self.renderer.poll_input();
        if let Ok(inputs) = input_result.as_ref() {
            for input in inputs.iter().copied() {
                self.handle_input(input);
            }
        }
        if let Err(error) = input_result {
            if let Some(id) = self.operation_id.take() {
                self.renderer.close();
                self.state = VisualOverlayState::Idle;
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
            | VisualOverlayState::HighlightingWindow { expires_at, .. } => {
                self.clock.now() >= *expires_at
            }
            _ => false,
        };
        if expired {
            if let Some(id) = self.operation_id.take() {
                self.renderer.close();
                self.state = VisualOverlayState::Idle;
                self.events
                    .push_back(VisualOverlayEvent::Expired { operation_id: id });
            }
        }
        self.events.drain(..).collect()
    }
    fn handle_input(&mut self, input: OverlayInput) {
        if self.operation_id != Some(input.operation_id) {
            return;
        }
        match input.kind {
            OverlayInputKind::Escape => self.cancel(),
            OverlayInputKind::LeftPressed(point) => {
                let VisualOverlayState::PickingRectangle { start, current, .. } = &mut self.state
                else {
                    return;
                };
                *start = Some(point);
                *current = Some(point);
                self.repaint_picker();
            }
            OverlayInputKind::PointerMoved(point) => {
                let VisualOverlayState::PickingRectangle {
                    start: Some(_),
                    current,
                    ..
                } = &mut self.state
                else {
                    return;
                };
                *current = Some(point);
                self.repaint_picker();
            }
            OverlayInputKind::LeftReleased(point) => {
                let VisualOverlayState::PickingRectangle {
                    start: Some(_),
                    current,
                    ..
                } = &mut self.state
                else {
                    return;
                };
                *current = Some(point);
                self.confirm_picker();
            }
            OverlayInputKind::Enter => self.confirm_picker(),
        }
    }
    fn repaint_picker(&mut self) {
        let (start, current) = match &self.state {
            VisualOverlayState::PickingRectangle { start, current, .. } => (*start, *current),
            _ => return,
        };
        let selection = start
            .zip(current)
            .and_then(|(a, b)| normalized_rect(a, b).ok())
            .filter(|r| !r.is_empty());
        if let (Some(id), Some(virtual_desktop)) = (self.operation_id, self.virtual_desktop) {
            if let Err(error) = self.renderer.repaint(
                id,
                &OverlayVisual::RectanglePicker {
                    virtual_desktop,
                    selection,
                },
            ) {
                self.renderer.close();
                self.operation_id = None;
                self.state = VisualOverlayState::Idle;
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
                start: Some(a),
                current: Some(b),
                purpose,
            } => (a, b, purpose),
            _ => return,
        };
        match normalized_rect(start, current) {
            Ok(rect) if !rect.is_empty() => {
                let id = self.operation_id.take().unwrap();
                self.renderer.close();
                self.state = VisualOverlayState::Idle;
                self.virtual_desktop = None;
                self.events
                    .push_back(VisualOverlayEvent::RectangleConfirmed {
                        operation_id: id,
                        purpose,
                        rect,
                    });
            }
            _ => {} // Keep the picker visible: the user can retry or cancel.
        }
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

    #[derive(Default)]
    struct FakeData {
        inputs: Vec<OverlayInput>,
        closes: usize,
        transparent: Vec<bool>,
    }
    struct FakeRenderer(Arc<Mutex<FakeData>>);
    impl OverlayRenderer for FakeRenderer {
        fn show(
            &mut self,
            _: OperationId,
            _: &OverlayVisual,
            transparent: bool,
        ) -> Result<(), VisualOverlayError> {
            self.0.lock().unwrap().transparent.push(transparent);
            Ok(())
        }
        fn repaint(&mut self, _: OperationId, _: &OverlayVisual) -> Result<(), VisualOverlayError> {
            Ok(())
        }
        fn poll_input(&mut self) -> Result<Vec<OverlayInput>, VisualOverlayError> {
            Ok(std::mem::take(&mut self.0.lock().unwrap().inputs))
        }
        fn close(&mut self) {
            self.0.lock().unwrap().closes += 1;
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
            c.poll(),
            vec![VisualOverlayEvent::RectangleConfirmed {
                operation_id: id,
                purpose: RectanglePurpose::ReferenceImageCapture,
                rect: ScreenRect::new(-1920, 150, 1820, 750)
            }]
        );
        assert_eq!(c.state(), &VisualOverlayState::Idle);
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
        assert!(c.poll().is_empty()); // Enter cannot confirm an empty drag.
        let second = c.preview_rectangle(ScreenRect::new(-4, -5, 6, 7));
        assert_eq!(
            c.poll(),
            vec![VisualOverlayEvent::Cancelled {
                operation_id: first
            }]
        );
        assert_eq!(fake.lock().unwrap().transparent, vec![false, true]);
        *now.lock().unwrap() = PASSIVE_OVERLAY_DURATION;
        assert_eq!(
            c.poll(),
            vec![VisualOverlayEvent::Expired {
                operation_id: second
            }]
        );
        c.shutdown();
        let closes = fake.lock().unwrap().closes;
        c.shutdown();
        assert_eq!(fake.lock().unwrap().closes, closes);
    }
}
