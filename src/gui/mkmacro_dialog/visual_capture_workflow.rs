//! Immediate, host-independent rectangle authoring orchestration.
//!
//! Selection starts synchronously when requested. Confirmation completes in the
//! purpose-specific way: search regions publish geometry directly, while
//! reference images are captured and persisted. The workflow neither observes
//! nor changes launcher or dialog visibility, and has no restoration phase.

use super::visual_overlay::{
    OperationId, RectanglePurpose, VisualOverlayController, VisualOverlayEvent,
};
use crate::mkmacro::ScreenRect;
use crate::mkmacro::{ImageAssetAuthoringService, MkMacroStore, ScreenCaptureBackend};
use image::RgbaImage;
use std::sync::{Arc, Mutex};

/// Cloneable, single-queue facade for the overlay controller.  Workflow events
/// are routed to its adapter while passive/editor events remain available to
/// the Action Editor, so two consumers never race the native queue.
#[derive(Clone)]
pub struct SharedVisualOverlayController(Arc<Mutex<SharedOverlay>>);
struct SharedOverlay {
    controller: VisualOverlayController,
    editor_events: Vec<VisualOverlayEvent>,
}
impl Default for SharedVisualOverlayController {
    fn default() -> Self {
        Self::new(VisualOverlayController::default())
    }
}
impl SharedVisualOverlayController {
    pub fn new(controller: VisualOverlayController) -> Self {
        Self(Arc::new(Mutex::new(SharedOverlay {
            controller,
            editor_events: vec![],
        })))
    }
    pub fn operation_id(&self) -> Option<OperationId> {
        self.0.lock().unwrap().controller.operation_id()
    }
    pub fn cancel(&self) {
        self.0.lock().unwrap().controller.cancel();
    }
    pub fn shutdown(&self) {
        let mut shared = self.0.lock().unwrap();
        shared.controller.shutdown();
        shared.editor_events.clear();
    }
    pub fn poll(&self) -> Vec<VisualOverlayEvent> {
        let mut shared = self.0.lock().unwrap();
        let mut events = std::mem::take(&mut shared.editor_events);
        events.extend(shared.controller.poll());
        events
    }
    pub fn preview_rectangle(&self, rect: ScreenRect) -> OperationId {
        self.0.lock().unwrap().controller.preview_rectangle(rect)
    }
    pub fn highlight_monitor(&self, monitor: crate::mkmacro::MonitorDescriptor) -> OperationId {
        self.0.lock().unwrap().controller.highlight_monitor(monitor)
    }
    pub fn identify_monitors(
        &self,
        monitors: Vec<crate::mkmacro::MonitorDescriptor>,
    ) -> OperationId {
        self.0
            .lock()
            .unwrap()
            .controller
            .identify_monitors(monitors)
    }
    pub fn highlight_window(
        &self,
        rect: ScreenRect,
        kind: super::visual_overlay::WindowAreaKind,
    ) -> OperationId {
        self.0
            .lock()
            .unwrap()
            .controller
            .highlight_window(rect, kind)
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
        let id = self
            .overlay
            .0
            .lock()
            .unwrap()
            .controller
            .begin_rectangle_pick(purpose, desktop);
        self.operation_id = Some(id);
        Ok(id)
    }
    fn poll(&mut self) -> SelectionEvent {
        let Some(expected) = self.operation_id else {
            return SelectionEvent::Pending;
        };
        let mut shared = self.overlay.0.lock().unwrap();
        for event in shared.controller.poll() {
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
                other => shared.editor_events.push(other),
            }
        }
        SelectionEvent::Pending
    }
    fn cancel(&mut self) {
        self.overlay.cancel();
        self.operation_id = None;
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
    fn poll(&mut self) -> SelectionEvent;
    fn cancel(&mut self);
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
                    self.complete_with_cleanup(WorkflowOutcome::Cancelled)
                }
                SelectionEvent::Failed {
                    operation_id: id,
                    message,
                } if id == operation_id => {
                    self.complete_with_cleanup(WorkflowOutcome::Failed(message))
                }
                SelectionEvent::Confirmed {
                    operation_id: id,
                    rect,
                } if id == operation_id => {
                    if rect.is_empty() {
                        self.complete_with_cleanup(WorkflowOutcome::Failed(
                            "selection must be nonempty".into(),
                        ));
                    } else if purpose == RectanglePurpose::SearchRegion {
                        self.complete_with_cleanup(WorkflowOutcome::Region {
                            token: self.token.unwrap(),
                            rect,
                        });
                    } else {
                        self.overlay.cancel();
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
        self.overlay.cancel();
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
        if self.active() {
            self.overlay.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Log {
        begins: Vec<RectanglePurpose>,
        events: Vec<SelectionEvent>,
        cancels: usize,
        captures: Vec<ScreenRect>,
        writes: usize,
        capture_error: bool,
        write_error: bool,
    }
    struct Overlay(Arc<Mutex<Log>>);
    impl RectangleOverlay for Overlay {
        fn begin(&mut self, p: RectanglePurpose) -> Result<OperationId, String> {
            self.0.lock().unwrap().begins.push(p);
            Ok(7)
        }
        fn poll(&mut self) -> SelectionEvent {
            let mut l = self.0.lock().unwrap();
            if l.events.is_empty() {
                SelectionEvent::Pending
            } else {
                l.events.remove(0)
            }
        }
        fn cancel(&mut self) {
            self.0.lock().unwrap().cancels += 1;
        }
    }
    struct Capture(Arc<Mutex<Log>>);
    impl CaptureAdapter for Capture {
        fn capture_rect(&mut self, r: ScreenRect) -> Result<RgbaImage, String> {
            let mut l = self.0.lock().unwrap();
            l.captures.push(r);
            if l.capture_error {
                Err("capture failed".into())
            } else {
                Ok(RgbaImage::from_pixel(
                    r.width,
                    r.height,
                    Rgba([1, 2, 3, 255]),
                ))
            }
        }
    }
    struct Store(Arc<Mutex<Log>>);
    impl AssetStoreAdapter for Store {
        fn write_png_asset(&mut self, _: u64, _: &RgbaImage) -> Result<u64, String> {
            let mut l = self.0.lock().unwrap();
            l.writes += 1;
            if l.write_error {
                Err("write failed".into())
            } else {
                Ok(42)
            }
        }
    }
    fn fixture() -> (VisualCaptureWorkflow, Arc<Mutex<Log>>) {
        let l = Arc::new(Mutex::new(Log::default()));
        (
            VisualCaptureWorkflow::new(
                Box::new(Overlay(l.clone())),
                Box::new(Capture(l.clone())),
                Box::new(Store(l.clone())),
            ),
            l,
        )
    }
    fn token() -> DraftToken {
        DraftToken {
            macro_id: 3,
            draft_generation: 9,
        }
    }
    fn confirm(l: &Arc<Mutex<Log>>, id: OperationId, rect: ScreenRect) {
        l.lock().unwrap().events.push(SelectionEvent::Confirmed {
            operation_id: id,
            rect,
        });
    }

    #[test]
    fn begin_starts_overlay_immediately() {
        let (mut w, l) = fixture();
        w.begin(token(), RectanglePurpose::SearchRegion).unwrap();
        assert_eq!(
            l.lock().unwrap().begins,
            vec![RectanglePurpose::SearchRegion]
        );
        assert!(matches!(
            w.state(),
            WorkflowState::Selecting {
                operation_id: 7,
                ..
            }
        ));
    }
    #[test]
    fn search_region_confirmation_completes_without_capture() {
        let (mut w, l) = fixture();
        let r = ScreenRect::new(-4, 8, 9, 3);
        w.begin(token(), RectanglePurpose::SearchRegion).unwrap();
        confirm(&l, 7, r);
        w.tick();
        assert_eq!(
            w.take_completed(),
            Some(WorkflowOutcome::Region {
                token: token(),
                rect: r
            })
        );
        let l = l.lock().unwrap();
        assert!(l.captures.is_empty());
        assert_eq!(l.writes, 0);
    }
    #[test]
    fn reference_confirmation_captures_and_stages_exact_rectangle() {
        let (mut w, l) = fixture();
        let r = ScreenRect::new(-13, 27, 6, 4);
        w.begin(token(), RectanglePurpose::ReferenceImageCapture)
            .unwrap();
        confirm(&l, 7, r);
        w.tick();
        assert_eq!(w.state(), &WorkflowState::Capturing { rect: r });
        w.tick();
        assert_eq!(
            w.take_completed(),
            Some(WorkflowOutcome::Asset {
                token: token(),
                asset_id: 42
            })
        );
        let l = l.lock().unwrap();
        assert_eq!(l.captures, vec![r]);
        assert_eq!(l.writes, 1);
    }
    #[test]
    fn cancel_before_or_during_drag_completes_once() {
        for via_event in [false, true] {
            let (mut w, l) = fixture();
            w.begin(token(), RectanglePurpose::ReferenceImageCapture)
                .unwrap();
            if via_event {
                l.lock()
                    .unwrap()
                    .events
                    .push(SelectionEvent::Cancelled { operation_id: 7 });
                w.tick()
            } else {
                w.cancel()
            }
            assert_eq!(w.take_completed(), Some(WorkflowOutcome::Cancelled));
            w.cancel();
            assert_eq!(w.take_completed(), None);
            let l = l.lock().unwrap();
            assert_eq!(l.cancels, 1);
            assert!(l.captures.is_empty());
            assert_eq!(l.writes, 0);
        }
    }
    #[test]
    fn capture_failure_completes_without_followup_restoration_stage() {
        let (mut w, l) = fixture();
        l.lock().unwrap().capture_error = true;
        w.begin(token(), RectanglePurpose::ReferenceImageCapture)
            .unwrap();
        confirm(&l, 7, ScreenRect::new(1, 2, 3, 4));
        w.tick();
        w.tick();
        assert!(matches!(
            w.take_completed(),
            Some(WorkflowOutcome::Failed(_))
        ));
        assert_eq!(w.state(), &WorkflowState::Idle);
        assert_eq!(l.lock().unwrap().writes, 0);
    }
    #[test]
    fn asset_failure_completes_once() {
        let (mut w, l) = fixture();
        l.lock().unwrap().write_error = true;
        w.begin(token(), RectanglePurpose::ReferenceImageCapture)
            .unwrap();
        confirm(&l, 7, ScreenRect::new(1, 2, 3, 4));
        w.tick();
        w.tick();
        assert!(matches!(
            w.take_completed(),
            Some(WorkflowOutcome::Failed(_))
        ));
        w.tick();
        assert_eq!(w.take_completed(), None);
        assert_eq!(l.lock().unwrap().writes, 1);
    }
    #[test]
    fn stale_operation_event_is_ignored() {
        let (mut w, l) = fixture();
        w.begin(token(), RectanglePurpose::SearchRegion).unwrap();
        confirm(&l, 6, ScreenRect::new(0, 0, 4, 4));
        w.tick();
        assert!(matches!(
            w.state(),
            WorkflowState::Selecting {
                operation_id: 7,
                ..
            }
        ));
        assert_eq!(w.take_completed(), None);
    }
}
