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
            // Nothing produced by the replaced operation may be observed after
            // its replacement.  Keep one useful cancellation notification,
            // but quarantine an earlier error/expiry (and duplicate cancel).
            self.events.retain(|event| event_operation_id(event) != id);
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
            Err(error) => {
                // show() is permitted to fail after creating some native
                // resources, so close is mandatory even though the operation
                // was never published as active.
                self.renderer.close();
                self.state = VisualOverlayState::Idle;
                self.operation_id = None;
                self.virtual_desktop = None;
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
        self.renderer.close();
        self.operation_id = None;
        self.state = VisualOverlayState::Idle;
        self.virtual_desktop = None;
        self.events.clear();
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
                // Publish the final pointer position through the same repaint
                // path as press/move before confirmation tears the windows down.
                self.repaint_picker();
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

fn event_operation_id(event: &VisualOverlayEvent) -> OperationId {
    match event {
        VisualOverlayEvent::RectangleConfirmed { operation_id, .. }
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
        repaints: Vec<OverlayVisual>,
        show_error: Option<VisualOverlayError>,
        poll_error: Option<VisualOverlayError>,
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
            match self.0.lock().unwrap().show_error.take() {
                Some(error) => Err(error),
                None => Ok(()),
            }
        }
        fn repaint(
            &mut self,
            _: OperationId,
            visual: &OverlayVisual,
        ) -> Result<(), VisualOverlayError> {
            self.0.lock().unwrap().repaints.push(visual.clone());
            Ok(())
        }
        fn poll_input(&mut self) -> Result<Vec<OverlayInput>, VisualOverlayError> {
            let mut data = self.0.lock().unwrap();
            if let Some(error) = data.poll_error.take() {
                Err(error)
            } else {
                Ok(std::mem::take(&mut data.inputs))
            }
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

    fn monitor(index: usize, bounds: ScreenRect) -> MonitorDescriptor {
        MonitorDescriptor {
            index,
            bounds,
            primary: index == 1,
        }
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

    fn platform_error(message: &str) -> VisualOverlayError {
        VisualOverlayError {
            kind: OverlayErrorKind::Platform,
            message: message.into(),
        }
    }

    #[test]
    fn replacement_quarantines_old_errors_and_keeps_only_new_operation_active() {
        let (mut c, fake, _) = controller();
        let first = c.identify_monitors(vec![monitor(1, ScreenRect::new(0, 0, 10, 10))]);
        c.events.push_back(VisualOverlayEvent::Error {
            operation_id: first,
            error: platform_error("stale"),
        });
        let second = c.highlight_window(ScreenRect::new(1, 2, 3, 4), WindowAreaKind::WholeWindow);
        assert_eq!(c.operation_id(), Some(second));
        assert_eq!(
            c.poll(),
            vec![VisualOverlayEvent::Cancelled {
                operation_id: first
            }]
        );
        assert_eq!(fake.lock().unwrap().closes, 1);
    }

    #[test]
    fn startup_and_poll_failures_close_and_return_to_idle_once() {
        let (mut c, fake, _) = controller();
        fake.lock().unwrap().show_error = Some(platform_error("partial startup"));
        let failed = c.preview_rectangle(ScreenRect::new(0, 0, 2, 2));
        assert_eq!(c.state(), &VisualOverlayState::Idle);
        assert_eq!(c.operation_id(), None);
        assert!(
            matches!(c.poll().as_slice(), [VisualOverlayEvent::Error { operation_id, .. }] if *operation_id == failed)
        );

        let active = c.preview_rectangle(ScreenRect::new(0, 0, 2, 2));
        fake.lock().unwrap().poll_error = Some(platform_error("poll failed"));
        assert!(
            matches!(c.poll().as_slice(), [VisualOverlayEvent::Error { operation_id, .. }] if *operation_id == active)
        );
        assert!(c.poll().is_empty());
        assert_eq!(c.state(), &VisualOverlayState::Idle);
    }

    #[test]
    fn cancel_shutdown_and_drop_close_without_late_events() {
        let (mut c, fake, _) = controller();
        c.preview_rectangle(ScreenRect::new(0, 0, 2, 2));
        c.cancel();
        c.cancel();
        c.shutdown();
        c.shutdown();
        assert!(c.poll().is_empty());
        let closes = fake.lock().unwrap().closes;
        drop(c);
        assert_eq!(fake.lock().unwrap().closes, closes);

        let (mut c, fake, _) = controller();
        c.preview_rectangle(ScreenRect::new(0, 0, 2, 2));
        let before = fake.lock().unwrap().closes;
        drop(c);
        assert!(fake.lock().unwrap().closes > before);
    }
}
