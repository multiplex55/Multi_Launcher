//! Immediate, host-independent rectangle authoring orchestration.
//!
//! Selection starts synchronously when requested. Confirmation completes in the
//! purpose-specific way: search regions publish geometry directly, while
//! reference images are captured and persisted. The workflow neither observes
//! nor changes launcher or dialog visibility, and has no restoration phase.

use super::visual_overlay::{
    NativeVisualOverlayService, OperationId, OverlayErrorKind, RectanglePurpose,
    VisualOverlayCommand, VisualOverlayController, VisualOverlayError, VisualOverlayEvent,
};
use crate::mkmacro::ScreenRect;
use crate::mkmacro::{ImageAssetAuthoringService, MkMacroStore, ScreenCaptureBackend};
use image::RgbaImage;
use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

/// Cloneable command client for the thread which exclusively owns native overlays.
/// GUI calls only enqueue work or drain already-produced semantic events.
#[derive(Clone)]
pub struct SharedVisualOverlayController(Arc<SharedOverlayClient>);
struct SharedOverlayClient {
    service: Mutex<Option<NativeVisualOverlayService>>,
    next_id: AtomicU64,
    active_id: AtomicU64,
    shutdown: AtomicBool,
    editor_events: Mutex<VecDeque<VisualOverlayEvent>>,
}
impl Default for SharedVisualOverlayController {
    fn default() -> Self {
        match NativeVisualOverlayService::start() {
            Ok(service) => Self::from_service(service),
            Err(error) => Self(Arc::new(SharedOverlayClient {
                service: Mutex::new(None),
                next_id: AtomicU64::new(1),
                active_id: AtomicU64::new(0),
                shutdown: AtomicBool::new(true),
                editor_events: Mutex::new(VecDeque::from([VisualOverlayEvent::Error {
                    operation_id: 1,
                    error: VisualOverlayError {
                        kind: OverlayErrorKind::Platform,
                        message: format!("failed to start visual overlay worker: {error}"),
                    },
                }])),
            })),
        }
    }
}
impl SharedVisualOverlayController {
    /// Test-oriented constructor; production uses `default`, constructing native state on the worker.
    pub fn new(controller: VisualOverlayController) -> Self {
        match NativeVisualOverlayService::start_with(move || controller) {
            Ok(service) => Self::from_service(service),
            Err(_) => Self::default(),
        }
    }
    fn from_service(service: NativeVisualOverlayService) -> Self {
        Self(Arc::new(SharedOverlayClient {
            service: Mutex::new(Some(service)),
            next_id: AtomicU64::new(1),
            active_id: AtomicU64::new(0),
            shutdown: AtomicBool::new(false),
            editor_events: Mutex::new(VecDeque::new()),
        }))
    }
    fn allocate(&self) -> OperationId {
        loop {
            let id = self.0.next_id.fetch_add(1, Ordering::Relaxed);
            if id != 0 {
                return id;
            }
        }
    }
    fn send_start(&self, id: OperationId, command: VisualOverlayCommand) -> OperationId {
        self.0.active_id.store(id, Ordering::Release);
        let sent = self
            .0
            .service
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|s| s.commands.send(command).is_ok());
        if !sent {
            self.0
                .active_id
                .compare_exchange(id, 0, Ordering::AcqRel, Ordering::Acquire)
                .ok();
            self.0
                .editor_events
                .lock()
                .unwrap()
                .push_back(VisualOverlayEvent::Error {
                    operation_id: id,
                    error: VisualOverlayError {
                        kind: OverlayErrorKind::Platform,
                        message: "visual overlay service is shut down".into(),
                    },
                });
        }
        id
    }
    pub fn begin_rectangle_pick(
        &self,
        purpose: RectanglePurpose,
        virtual_desktop: ScreenRect,
    ) -> OperationId {
        let id = self.allocate();
        self.send_start(
            id,
            VisualOverlayCommand::BeginRectanglePick {
                operation_id: id,
                purpose,
                virtual_desktop,
            },
        )
    }
    pub fn preview_rectangle(&self, rect: ScreenRect) -> OperationId {
        let id = self.allocate();
        self.send_start(
            id,
            VisualOverlayCommand::PreviewRectangle {
                operation_id: id,
                rect,
            },
        )
    }
    pub fn highlight_monitor(&self, monitor: crate::mkmacro::MonitorDescriptor) -> OperationId {
        let id = self.allocate();
        self.send_start(
            id,
            VisualOverlayCommand::HighlightMonitor {
                operation_id: id,
                monitor,
            },
        )
    }
    pub fn identify_monitors(
        &self,
        monitors: Vec<crate::mkmacro::MonitorDescriptor>,
    ) -> OperationId {
        let id = self.allocate();
        self.send_start(
            id,
            VisualOverlayCommand::IdentifyMonitors {
                operation_id: id,
                monitors,
            },
        )
    }
    pub fn highlight_window(
        &self,
        rect: ScreenRect,
        area_kind: super::visual_overlay::WindowAreaKind,
    ) -> OperationId {
        let id = self.allocate();
        self.send_start(
            id,
            VisualOverlayCommand::HighlightWindow {
                operation_id: id,
                rect,
                area_kind,
            },
        )
    }
    pub fn operation_id(&self) -> Option<OperationId> {
        match self.0.active_id.load(Ordering::Acquire) {
            0 => None,
            id => Some(id),
        }
    }
    pub fn cancel_operation(&self, expected_operation_id: OperationId) {
        if self
            .0
            .active_id
            .compare_exchange(
                expected_operation_id,
                0,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            if let Some(service) = self.0.service.lock().unwrap().as_ref() {
                let _ = service.commands.send(VisualOverlayCommand::Cancel {
                    expected_operation_id: Some(expected_operation_id),
                });
            }
        }
    }
    /// Cancels the operation which is current at the instant this method is called.
    /// Prefer [`Self::cancel_operation`] when an owner has retained its operation id.
    pub fn cancel(&self) {
        if let Some(id) = self.operation_id() {
            self.cancel_operation(id);
        }
    }
    pub fn shutdown(&self) {
        if self.0.shutdown.swap(true, Ordering::AcqRel) {
            return;
        }
        self.0.active_id.store(0, Ordering::Release);
        if let Some(mut service) = self.0.service.lock().unwrap().take() {
            service.shutdown_and_join();
        }
    }
    fn receive_into_editor(&self) {
        let mut incoming = vec![];
        if let Some(service) = self.0.service.lock().unwrap().as_ref() {
            incoming.extend(service.events.try_iter());
        }
        for event in &incoming {
            let id = match event {
                VisualOverlayEvent::RectangleConfirmed { operation_id, .. }
                | VisualOverlayEvent::Cancelled { operation_id }
                | VisualOverlayEvent::Expired { operation_id }
                | VisualOverlayEvent::Error { operation_id, .. } => *operation_id,
            };
            let _ = self
                .0
                .active_id
                .compare_exchange(id, 0, Ordering::AcqRel, Ordering::Acquire);
        }
        self.0.editor_events.lock().unwrap().extend(incoming);
    }
    /// Drains events already produced by the native worker; it never advances native input.
    pub fn poll(&self) -> Vec<VisualOverlayEvent> {
        self.receive_into_editor();
        self.0.editor_events.lock().unwrap().drain(..).collect()
    }
    fn poll_rectangle(&self, expected: OperationId) -> Option<VisualOverlayEvent> {
        self.receive_into_editor();
        let mut queue = self.0.editor_events.lock().unwrap();
        let position = queue.iter().position(|event| {
            matches!(event,
            VisualOverlayEvent::RectangleConfirmed { operation_id, .. }
            | VisualOverlayEvent::Cancelled { operation_id }
            | VisualOverlayEvent::Expired { operation_id }
            | VisualOverlayEvent::Error { operation_id, .. } if *operation_id==expected)
        });
        position.and_then(|index| queue.remove(index))
    }
}
impl Drop for SharedOverlayClient {
    fn drop(&mut self) {
        if let Some(mut service) = self.service.get_mut().unwrap().take() {
            service.shutdown_and_join();
        }
    }
}

pub struct VisualOverlayRectangleAdapter {
    overlay: SharedVisualOverlayController,
    backend: Arc<dyn ScreenCaptureBackend>,
    operation_id: Option<OperationId>,
}
impl VisualOverlayRectangleAdapter {
    pub fn new(
        overlay: SharedVisualOverlayController,
        backend: Arc<dyn ScreenCaptureBackend>,
    ) -> Self {
        Self {
            overlay,
            backend,
            operation_id: None,
        }
    }
}
impl RectangleOverlay for VisualOverlayRectangleAdapter {
    fn begin(&mut self, purpose: RectanglePurpose) -> Result<OperationId, String> {
        let desktop = self.backend.virtual_desktop().map_err(|e| e.to_string())?;
        let id = self.overlay.begin_rectangle_pick(purpose, desktop);
        self.operation_id = Some(id);
        Ok(id)
    }
    fn poll(&mut self) -> SelectionEvent {
        let Some(expected) = self.operation_id else {
            return SelectionEvent::Pending;
        };
        if let Some(event) = self.overlay.poll_rectangle(expected) {
            match event {
                VisualOverlayEvent::RectangleConfirmed {
                    operation_id, rect, ..
                } if operation_id == expected => {
                    return SelectionEvent::Confirmed { operation_id, rect };
                }
                VisualOverlayEvent::Cancelled { operation_id } if operation_id == expected => {
                    return SelectionEvent::Cancelled { operation_id };
                }
                VisualOverlayEvent::Error {
                    operation_id,
                    error,
                } if operation_id == expected => {
                    return SelectionEvent::Failed {
                        operation_id,
                        message: error.to_string(),
                    };
                }
                VisualOverlayEvent::Expired { operation_id } if operation_id == expected => {
                    return SelectionEvent::Failed {
                        operation_id,
                        message: "interactive rectangle picker expired unexpectedly".into(),
                    };
                }
                _ => SelectionEvent::Pending,
            }
        } else {
            SelectionEvent::Pending
        }
    }
    fn cancel(&mut self, expected_operation_id: OperationId) {
        if self.operation_id == Some(expected_operation_id) {
            self.operation_id = None;
            self.overlay.cancel_operation(expected_operation_id);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectionEvent {
    Pending,
    Confirmed {
        operation_id: OperationId,
        rect: ScreenRect,
    },
    Cancelled {
        operation_id: OperationId,
    },
    Failed {
        operation_id: OperationId,
        message: String,
    },
}
pub trait RectangleOverlay: Send {
    fn begin(&mut self, purpose: RectanglePurpose) -> Result<OperationId, String>;
    /// Performs a nonblocking drain of semantic events already produced by the worker.
    /// It must not poll or otherwise advance native input.
    fn poll(&mut self) -> SelectionEvent;
    fn cancel(&mut self, expected_operation_id: OperationId);
}
pub trait CaptureAdapter: Send {
    fn capture_rect(&mut self, rect: ScreenRect) -> Result<RgbaImage, String>;
}
/// Combines asset allocation and PNG persistence into a transactional boundary.
/// It must return an id only after the PNG has been completely written.
pub trait AssetStoreAdapter: Send {
    fn write_png_asset(&mut self, macro_id: u64, image: &RgbaImage) -> Result<u64, String>;
}

/// Production capture adapter; importantly this calls the backend's rectangle
/// primitive rather than taking a full-desktop screenshot and cropping it.
pub struct ScreenCaptureAdapter(pub Arc<dyn ScreenCaptureBackend>);
impl CaptureAdapter for ScreenCaptureAdapter {
    fn capture_rect(&mut self, rect: ScreenRect) -> Result<RgbaImage, String> {
        self.0
            .capture_rect(rect, &|| false)
            .map_err(|e| e.to_string())
    }
}
pub struct MkMacroAssetStoreAdapter(pub Arc<MkMacroStore>);
impl AssetStoreAdapter for MkMacroAssetStoreAdapter {
    fn write_png_asset(&mut self, macro_id: u64, image: &RgbaImage) -> Result<u64, String> {
        ImageAssetAuthoringService::new(&self.0)
            .stage_rgba(macro_id, image)
            .map(|staged| staged.asset_id)
            .map_err(|e| e.to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DraftToken {
    pub macro_id: u64,
    pub draft_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkflowOutcome {
    Region { token: DraftToken, rect: ScreenRect },
    Asset { token: DraftToken, asset_id: u64 },
    Cancelled,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkflowState {
    Idle,
    Selecting {
        operation_id: OperationId,
        purpose: RectanglePurpose,
    },
    Capturing {
        rect: ScreenRect,
    },
}

pub struct VisualCaptureWorkflow {
    state: WorkflowState,
    token: Option<DraftToken>,
    overlay: Box<dyn RectangleOverlay>,
    capture: Box<dyn CaptureAdapter>,
    assets: Box<dyn AssetStoreAdapter>,
    completed: Option<WorkflowOutcome>,
}

impl VisualCaptureWorkflow {
    pub fn new(
        overlay: Box<dyn RectangleOverlay>,
        capture: Box<dyn CaptureAdapter>,
        assets: Box<dyn AssetStoreAdapter>,
    ) -> Self {
        Self {
            state: WorkflowState::Idle,
            token: None,
            overlay,
            capture,
            assets,
            completed: None,
        }
    }
    pub fn state(&self) -> &WorkflowState {
        &self.state
    }
    pub fn active(&self) -> bool {
        !matches!(self.state, WorkflowState::Idle)
    }
    pub fn begin(
        &mut self,
        token: DraftToken,
        purpose: RectanglePurpose,
    ) -> Result<(), &'static str> {
        if self.active() {
            return Err("a visual authoring operation is already active");
        }
        self.completed = None;
        match self.overlay.begin(purpose) {
            Ok(operation_id) => {
                self.token = Some(token);
                self.state = WorkflowState::Selecting {
                    operation_id,
                    purpose,
                };
                Ok(())
            }
            Err(error) => {
                self.complete(WorkflowOutcome::Failed(error));
                Ok(())
            }
        }
    }
    pub fn tick(&mut self) {
        match self.state.clone() {
            WorkflowState::Idle => {}
            WorkflowState::Selecting {
                operation_id,
                purpose,
            } => match self.overlay.poll() {
                SelectionEvent::Pending => {}
                SelectionEvent::Cancelled { operation_id: id } if id == operation_id => {
                    self.complete(WorkflowOutcome::Cancelled)
                }
                SelectionEvent::Failed {
                    operation_id: id,
                    message,
                } if id == operation_id => self.complete(WorkflowOutcome::Failed(message)),
                SelectionEvent::Confirmed {
                    operation_id: id,
                    rect,
                } if id == operation_id => {
                    if rect.is_empty() {
                        self.complete(WorkflowOutcome::Failed("selection must be nonempty".into()));
                    } else if purpose == RectanglePurpose::SearchRegion {
                        self.complete(WorkflowOutcome::Region {
                            token: self.token.unwrap(),
                            rect,
                        });
                    } else {
                        self.state = WorkflowState::Capturing { rect };
                    }
                }
                _ => {}
            },
            WorkflowState::Capturing { rect } => {
                let token = self.token.unwrap();
                let outcome = self
                    .capture
                    .capture_rect(rect)
                    .and_then(|image| self.assets.write_png_asset(token.macro_id, &image))
                    .map(|asset_id| WorkflowOutcome::Asset { token, asset_id })
                    .unwrap_or_else(WorkflowOutcome::Failed);
                self.complete(outcome);
            }
        }
    }
    fn complete_with_cleanup(&mut self, outcome: WorkflowOutcome) {
        if let WorkflowState::Selecting { operation_id, .. } = self.state {
            self.overlay.cancel(operation_id);
        }
        self.complete(outcome);
    }
    fn complete(&mut self, outcome: WorkflowOutcome) {
        self.state = WorkflowState::Idle;
        self.token = None;
        self.completed = Some(outcome);
    }
    pub fn cancel(&mut self) {
        if self.active() {
            self.complete_with_cleanup(WorkflowOutcome::Cancelled);
        }
    }
    pub fn take_completed(&mut self) -> Option<WorkflowOutcome> {
        self.completed.take()
    }
}
impl Drop for VisualCaptureWorkflow {
    fn drop(&mut self) {
        if let WorkflowState::Selecting { operation_id, .. } = self.state {
            self.overlay.cancel(operation_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Call {
        Begin(RectanglePurpose),
        Poll,
        Cancel,
        Capture(ScreenRect),
        Write {
            macro_id: u64,
            dimensions: (u32, u32),
        },
    }
    #[derive(Default)]
    struct FakeState {
        calls: Vec<Call>,
        events: Vec<SelectionEvent>,
        begin_error: Option<String>,
        capture_error: Option<String>,
        write_error: Option<String>,
    }
    struct Overlay(Arc<Mutex<FakeState>>);
    impl RectangleOverlay for Overlay {
        fn begin(&mut self, purpose: RectanglePurpose) -> Result<OperationId, String> {
            let mut fake = self.0.lock().unwrap();
            fake.calls.push(Call::Begin(purpose));
            fake.begin_error.take().map_or(Ok(7), Err)
        }
        fn poll(&mut self) -> SelectionEvent {
            let mut fake = self.0.lock().unwrap();
            fake.calls.push(Call::Poll);
            if fake.events.is_empty() {
                SelectionEvent::Pending
            } else {
                fake.events.remove(0)
            }
        }
        fn cancel(&mut self, _expected_operation_id: OperationId) {
            self.0.lock().unwrap().calls.push(Call::Cancel);
        }
    }
    struct Capture(Arc<Mutex<FakeState>>);
    impl CaptureAdapter for Capture {
        fn capture_rect(&mut self, rect: ScreenRect) -> Result<RgbaImage, String> {
            let mut fake = self.0.lock().unwrap();
            fake.calls.push(Call::Capture(rect));
            if let Some(error) = fake.capture_error.take() {
                Err(error)
            } else {
                Ok(RgbaImage::from_pixel(
                    rect.width,
                    rect.height,
                    Rgba([1, 2, 3, 255]),
                ))
            }
        }
    }
    struct Store(Arc<Mutex<FakeState>>);
    impl AssetStoreAdapter for Store {
        fn write_png_asset(&mut self, macro_id: u64, image: &RgbaImage) -> Result<u64, String> {
            let mut fake = self.0.lock().unwrap();
            fake.calls.push(Call::Write {
                macro_id,
                dimensions: image.dimensions(),
            });
            fake.write_error.take().map_or(Ok(42), Err)
        }
    }
    fn fixture() -> (VisualCaptureWorkflow, Arc<Mutex<FakeState>>) {
        let fake = Arc::new(Mutex::new(FakeState::default()));
        (
            VisualCaptureWorkflow::new(
                Box::new(Overlay(fake.clone())),
                Box::new(Capture(fake.clone())),
                Box::new(Store(fake.clone())),
            ),
            fake,
        )
    }
    fn token() -> DraftToken {
        DraftToken {
            macro_id: 3,
            draft_generation: 9,
        }
    }
    fn queue(fake: &Arc<Mutex<FakeState>>, event: SelectionEvent) {
        fake.lock().unwrap().events.push(event);
    }
    fn confirm(fake: &Arc<Mutex<FakeState>>, operation_id: OperationId, rect: ScreenRect) {
        queue(fake, SelectionEvent::Confirmed { operation_id, rect });
    }
    fn data_calls(fake: &Arc<Mutex<FakeState>>) -> Vec<Call> {
        fake.lock()
            .unwrap()
            .calls
            .iter()
            .filter(|call| !matches!(call, Call::Poll | Call::Cancel))
            .cloned()
            .collect()
    }

    #[test]
    fn begin_is_immediate_and_pending_selection_has_no_downstream_effects() {
        let (mut workflow, fake) = fixture();
        workflow
            .begin(token(), RectanglePurpose::SearchRegion)
            .unwrap();
        assert_eq!(
            fake.lock().unwrap().calls,
            [Call::Begin(RectanglePurpose::SearchRegion)]
        );
        workflow.tick();
        assert!(matches!(
            workflow.state(),
            WorkflowState::Selecting {
                operation_id: 7,
                purpose: RectanglePurpose::SearchRegion
            }
        ));
        assert_eq!(workflow.take_completed(), None);
        assert_eq!(
            data_calls(&fake),
            [Call::Begin(RectanglePurpose::SearchRegion)]
        );
    }

    #[test]
    fn only_the_exact_operation_id_can_complete_selection() {
        let (mut workflow, fake) = fixture();
        workflow
            .begin(token(), RectanglePurpose::SearchRegion)
            .unwrap();
        confirm(&fake, 6, ScreenRect::new(1, 2, 3, 4));
        workflow.tick();
        assert!(workflow.active());
        assert_eq!(workflow.take_completed(), None);
        confirm(&fake, 7, ScreenRect::new(-20, -30, 30, 50));
        workflow.tick();
        assert_eq!(
            workflow.take_completed(),
            Some(WorkflowOutcome::Region {
                token: token(),
                rect: ScreenRect::new(-20, -30, 30, 50)
            })
        );
    }

    #[test]
    fn signed_normalized_search_region_completes_without_capture_or_write() {
        let (mut workflow, fake) = fixture();
        let normalized = ScreenRect::new(-1920, -900, 1820, 1050);
        workflow
            .begin(token(), RectanglePurpose::SearchRegion)
            .unwrap();
        confirm(&fake, 7, normalized);
        workflow.tick();
        assert_eq!(
            workflow.take_completed(),
            Some(WorkflowOutcome::Region {
                token: token(),
                rect: normalized
            })
        );
        assert_eq!(
            data_calls(&fake),
            [Call::Begin(RectanglePurpose::SearchRegion)]
        );
    }

    #[test]
    fn reference_capture_orders_confirmation_before_capture_and_write() {
        let (mut workflow, fake) = fixture();
        let rect = ScreenRect::new(-13, 27, 6, 4);
        workflow
            .begin(token(), RectanglePurpose::ReferenceImageCapture)
            .unwrap();
        confirm(&fake, 7, rect);
        workflow.tick();
        assert_eq!(workflow.state(), &WorkflowState::Capturing { rect });
        assert!(
            !fake
                .lock()
                .unwrap()
                .calls
                .iter()
                .any(|call| matches!(call, Call::Cancel))
        );
        assert_eq!(
            data_calls(&fake),
            [Call::Begin(RectanglePurpose::ReferenceImageCapture)]
        );
        workflow.tick();
        assert_eq!(
            workflow.take_completed(),
            Some(WorkflowOutcome::Asset {
                token: token(),
                asset_id: 42
            })
        );
        assert_eq!(
            data_calls(&fake),
            [
                Call::Begin(RectanglePurpose::ReferenceImageCapture),
                Call::Capture(rect),
                Call::Write {
                    macro_id: 3,
                    dimensions: (6, 4)
                }
            ]
        );
    }

    #[test]
    fn cancellation_event_and_explicit_cancel_never_capture_or_write() {
        for via_event in [false, true] {
            let (mut workflow, fake) = fixture();
            workflow
                .begin(token(), RectanglePurpose::ReferenceImageCapture)
                .unwrap();
            if via_event {
                queue(&fake, SelectionEvent::Cancelled { operation_id: 7 });
                workflow.tick();
            } else {
                workflow.cancel();
            }
            assert_eq!(workflow.take_completed(), Some(WorkflowOutcome::Cancelled));
            workflow.tick();
            workflow.cancel();
            assert_eq!(workflow.take_completed(), None);
            assert_eq!(
                data_calls(&fake),
                [Call::Begin(RectanglePurpose::ReferenceImageCapture)]
            );
        }
    }

    #[test]
    fn overlay_failure_and_empty_geometry_are_terminal_without_capture_or_write() {
        for event in [
            SelectionEvent::Failed {
                operation_id: 7,
                message: "overlay failed".into(),
            },
            SelectionEvent::Confirmed {
                operation_id: 7,
                rect: ScreenRect::new(-5, 8, 0, 4),
            },
        ] {
            let (mut workflow, fake) = fixture();
            workflow
                .begin(token(), RectanglePurpose::ReferenceImageCapture)
                .unwrap();
            queue(&fake, event);
            workflow.tick();
            assert!(matches!(
                workflow.take_completed(),
                Some(WorkflowOutcome::Failed(_))
            ));
            assert_eq!(
                data_calls(&fake),
                [Call::Begin(RectanglePurpose::ReferenceImageCapture)]
            );
        }
    }

    #[test]
    fn overlay_begin_failure_is_reported_synchronously() {
        let (mut workflow, fake) = fixture();
        fake.lock().unwrap().begin_error = Some("overlay unavailable".into());
        workflow
            .begin(token(), RectanglePurpose::SearchRegion)
            .unwrap();
        assert_eq!(
            workflow.take_completed(),
            Some(WorkflowOutcome::Failed("overlay unavailable".into()))
        );
        assert!(!workflow.active());
    }

    #[test]
    fn capture_and_store_failures_stop_at_the_failed_boundary() {
        for store_failure in [false, true] {
            let (mut workflow, fake) = fixture();
            if store_failure {
                fake.lock().unwrap().write_error = Some("write failed".into());
            } else {
                fake.lock().unwrap().capture_error = Some("capture failed".into());
            }
            let rect = ScreenRect::new(1, 2, 3, 4);
            workflow
                .begin(token(), RectanglePurpose::ReferenceImageCapture)
                .unwrap();
            confirm(&fake, 7, rect);
            workflow.tick();
            workflow.tick();
            assert!(matches!(
                workflow.take_completed(),
                Some(WorkflowOutcome::Failed(_))
            ));
            let calls = data_calls(&fake);
            assert_eq!(
                calls
                    .iter()
                    .filter(|c| matches!(c, Call::Capture(_)))
                    .count(),
                1
            );
            assert_eq!(
                calls
                    .iter()
                    .filter(|c| matches!(c, Call::Write { .. }))
                    .count(),
                usize::from(store_failure)
            );
        }
    }

    #[test]
    fn drop_cleans_up_an_active_overlay_once() {
        let (mut workflow, fake) = fixture();
        workflow
            .begin(token(), RectanglePurpose::SearchRegion)
            .unwrap();
        drop(workflow);
        assert_eq!(
            fake.lock()
                .unwrap()
                .calls
                .iter()
                .filter(|c| matches!(c, Call::Cancel))
                .count(),
            1
        );
    }

    #[test]
    fn terminal_polling_is_idempotent() {
        let (mut workflow, fake) = fixture();
        workflow
            .begin(token(), RectanglePurpose::SearchRegion)
            .unwrap();
        queue(&fake, SelectionEvent::Cancelled { operation_id: 7 });
        workflow.tick();
        let calls = fake.lock().unwrap().calls.len();
        assert_eq!(workflow.take_completed(), Some(WorkflowOutcome::Cancelled));
        for _ in 0..3 {
            workflow.tick();
            assert_eq!(workflow.take_completed(), None);
        }
        assert_eq!(fake.lock().unwrap().calls.len(), calls);
    }
}
