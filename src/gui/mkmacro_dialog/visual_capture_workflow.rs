//! Non-blocking orchestration for rectangle authoring.
//!
//! This module intentionally knows nothing about egui.  The owning GUI supplies
//! the window, overlay, clock, capture and store boundaries and calls [`tick`]
//! once per frame.  Consequently hiding a window never leads to a sleep on the
//! UI thread and every terminal path passes through the same restoration step.

use super::visual_overlay::{
    OperationId, RectanglePurpose, VisualOverlayController, VisualOverlayEvent,
};
use crate::mkmacro::ScreenRect;
use crate::mkmacro::{ImageAssetAuthoringService, MkMacroStore, ScreenCaptureBackend};
use image::RgbaImage;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Exact visibility to put back after an authoring operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavedVisibility {
    pub launcher: bool,
    pub mkmacro_dialog: bool,
}

pub trait VisibilityAdapter: Send {
    fn snapshot(&self) -> SavedVisibility;
    fn request_hidden(&mut self);
    fn hidden_observed(&self) -> bool;
    fn restore(&mut self, saved: SavedVisibility);
}
pub trait WorkflowClock: Send {
    fn now(&self) -> Duration;
}

/// Monotonic production clock.  The private epoch also makes its values small
/// and immune to wall-clock adjustments.
pub struct SystemWorkflowClock(Instant);
impl Default for SystemWorkflowClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}
impl WorkflowClock for SystemWorkflowClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}

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
    pub fn poll(&self) -> Vec<VisualOverlayEvent> {
        let mut shared = self.0.lock().unwrap();
        let mut events = std::mem::take(&mut shared.editor_events);
        events.extend(shared.controller.poll());
        events
    }
    pub fn preview_rectangle(&self, rect: ScreenRect) {
        self.0.lock().unwrap().controller.preview_rectangle(rect);
    }
    pub fn highlight_monitor(&self, monitor: crate::mkmacro::MonitorDescriptor) {
        self.0.lock().unwrap().controller.highlight_monitor(monitor);
    }
    pub fn identify_monitors(&self, monitors: Vec<crate::mkmacro::MonitorDescriptor>) {
        self.0
            .lock()
            .unwrap()
            .controller
            .identify_monitors(monitors);
    }
    pub fn highlight_window(&self, rect: ScreenRect, kind: super::visual_overlay::WindowAreaKind) {
        self.0
            .lock()
            .unwrap()
            .controller
            .highlight_window(rect, kind);
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
    HidingLauncher {
        saved_visibility: SavedVisibility,
        requested_at: Duration,
    },
    WaitingForDesktopRedraw {
        saved_visibility: SavedVisibility,
        ready_at: Duration,
    },
    Selecting {
        saved_visibility: SavedVisibility,
        operation_id: OperationId,
        purpose: RectanglePurpose,
    },
    Capturing {
        saved_visibility: SavedVisibility,
        rect: ScreenRect,
    },
    Restoring {
        saved_visibility: SavedVisibility,
        outcome: WorkflowOutcome,
    },
}

pub struct VisualCaptureWorkflow {
    state: WorkflowState,
    token: Option<DraftToken>,
    purpose: Option<RectanglePurpose>,
    visibility: Box<dyn VisibilityAdapter>,
    clock: Box<dyn WorkflowClock>,
    overlay: Box<dyn RectangleOverlay>,
    capture: Box<dyn CaptureAdapter>,
    assets: Box<dyn AssetStoreAdapter>,
    redraw_delay: Duration,
    restored: bool,
    completed: Option<WorkflowOutcome>,
}

impl VisualCaptureWorkflow {
    pub fn new(
        visibility: Box<dyn VisibilityAdapter>,
        clock: Box<dyn WorkflowClock>,
        overlay: Box<dyn RectangleOverlay>,
        capture: Box<dyn CaptureAdapter>,
        assets: Box<dyn AssetStoreAdapter>,
    ) -> Self {
        Self {
            state: WorkflowState::Idle,
            token: None,
            purpose: None,
            visibility,
            clock,
            overlay,
            capture,
            assets,
            redraw_delay: Duration::from_millis(34),
            restored: true,
            completed: None,
        }
    }
    pub fn state(&self) -> &WorkflowState {
        &self.state
    }
    pub fn active(&self) -> bool {
        !matches!(self.state, WorkflowState::Idle)
    }
    pub fn set_redraw_delay(&mut self, delay: Duration) {
        self.redraw_delay = delay;
    }
    pub fn begin(
        &mut self,
        token: DraftToken,
        purpose: RectanglePurpose,
    ) -> Result<(), &'static str> {
        if self.active() {
            return Err("a visual authoring operation is already active");
        }
        let saved_visibility = self.visibility.snapshot();
        let requested_at = self.clock.now();
        self.token = Some(token);
        self.purpose = Some(purpose);
        self.restored = false;
        self.completed = None;
        self.visibility.request_hidden();
        self.state = WorkflowState::HidingLauncher {
            saved_visibility,
            requested_at,
        };
        Ok(())
    }
    /// Advances at most one asynchronous stage. It never waits or sleeps.
    pub fn tick(&mut self) {
        let state = self.state.clone();
        match state {
            WorkflowState::Idle => {}
            WorkflowState::HidingLauncher {
                saved_visibility, ..
            } if self.visibility.hidden_observed() => {
                self.state = WorkflowState::WaitingForDesktopRedraw {
                    saved_visibility,
                    ready_at: self.clock.now().saturating_add(self.redraw_delay),
                };
            }
            WorkflowState::WaitingForDesktopRedraw {
                saved_visibility,
                ready_at,
            } if self.clock.now() >= ready_at => {
                let purpose = self.purpose.expect("active workflow has purpose");
                match self.overlay.begin(purpose) {
                    Ok(operation_id) => {
                        self.state = WorkflowState::Selecting {
                            saved_visibility,
                            operation_id,
                            purpose,
                        }
                    }
                    Err(error) => self.finish(saved_visibility, WorkflowOutcome::Failed(error)),
                }
            }
            WorkflowState::Selecting {
                saved_visibility,
                operation_id,
                purpose,
            } => match self.overlay.poll() {
                SelectionEvent::Pending => {}
                SelectionEvent::Cancelled { operation_id: id } if id == operation_id => {
                    self.finish(saved_visibility, WorkflowOutcome::Cancelled)
                }
                SelectionEvent::Failed {
                    operation_id: id,
                    message,
                } if id == operation_id => {
                    self.finish(saved_visibility, WorkflowOutcome::Failed(message))
                }
                SelectionEvent::Confirmed {
                    operation_id: id,
                    rect,
                } if id == operation_id => {
                    if rect.is_empty() {
                        self.finish(
                            saved_visibility,
                            WorkflowOutcome::Failed("selection must be nonempty".into()),
                        );
                    } else if purpose == RectanglePurpose::SearchRegion {
                        self.finish(
                            saved_visibility,
                            WorkflowOutcome::Region {
                                token: self.token.unwrap(),
                                rect,
                            },
                        );
                    } else {
                        self.state = WorkflowState::Capturing {
                            saved_visibility,
                            rect,
                        };
                    }
                }
                _ => {} // stale native events are deliberately ignored
            },
            WorkflowState::Capturing {
                saved_visibility,
                rect,
            } => {
                let token = self.token.unwrap();
                let outcome = self
                    .capture
                    .capture_rect(rect)
                    .and_then(|image| self.assets.write_png_asset(token.macro_id, &image))
                    .map(|asset_id| WorkflowOutcome::Asset { token, asset_id })
                    .unwrap_or_else(WorkflowOutcome::Failed);
                self.finish(saved_visibility, outcome);
            }
            WorkflowState::Restoring {
                saved_visibility,
                outcome,
            } => {
                self.restore_once(saved_visibility);
                self.completed = Some(outcome);
                self.state = WorkflowState::Idle;
                self.token = None;
                self.purpose = None;
            }
            _ => {}
        }
    }
    fn finish(&mut self, saved_visibility: SavedVisibility, outcome: WorkflowOutcome) {
        self.overlay.cancel();
        self.state = WorkflowState::Restoring {
            saved_visibility,
            outcome,
        };
    }
    fn restore_once(&mut self, saved: SavedVisibility) {
        if !self.restored {
            self.visibility.restore(saved);
            self.restored = true;
        }
    }
    pub fn cancel(&mut self) {
        let saved = match self.state {
            WorkflowState::HidingLauncher {
                saved_visibility, ..
            }
            | WorkflowState::WaitingForDesktopRedraw {
                saved_visibility, ..
            }
            | WorkflowState::Selecting {
                saved_visibility, ..
            }
            | WorkflowState::Capturing {
                saved_visibility, ..
            }
            | WorkflowState::Restoring {
                saved_visibility, ..
            } => Some(saved_visibility),
            WorkflowState::Idle => None,
        };
        if let Some(saved) = saved {
            self.finish(saved, WorkflowOutcome::Cancelled);
        }
    }
    /// Returns results only after the restoration stage has run.
    pub fn take_completed(&mut self) -> Option<WorkflowOutcome> {
        self.completed.take()
    }
}
impl Drop for VisualCaptureWorkflow {
    fn drop(&mut self) {
        self.overlay.cancel();
        let saved = match self.state {
            WorkflowState::HidingLauncher {
                saved_visibility, ..
            }
            | WorkflowState::WaitingForDesktopRedraw {
                saved_visibility, ..
            }
            | WorkflowState::Selecting {
                saved_visibility, ..
            }
            | WorkflowState::Capturing {
                saved_visibility, ..
            }
            | WorkflowState::Restoring {
                saved_visibility, ..
            } => Some(saved_visibility),
            WorkflowState::Idle => None,
        };
        if let Some(saved) = saved {
            self.restore_once(saved);
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
        entries: Vec<String>,
        restores: usize,
        saved: Option<SavedVisibility>,
        now_ms: u64,
        event: Option<SelectionEvent>,
        capture_error: bool,
        write_error: bool,
    }
    struct Vis(Arc<Mutex<Log>>, SavedVisibility);
    impl VisibilityAdapter for Vis {
        fn snapshot(&self) -> SavedVisibility {
            self.1
        }
        fn request_hidden(&mut self) {
            self.0.lock().unwrap().entries.push("hide".into());
        }
        fn hidden_observed(&self) -> bool {
            true
        }
        fn restore(&mut self, saved: SavedVisibility) {
            let mut l = self.0.lock().unwrap();
            l.restores += 1;
            l.saved = Some(saved);
            l.entries.push("restore".into());
        }
    }
    struct Clock(Arc<Mutex<Log>>);
    impl WorkflowClock for Clock {
        fn now(&self) -> Duration {
            Duration::from_millis(self.0.lock().unwrap().now_ms)
        }
    }
    struct Overlay(Arc<Mutex<Log>>);
    impl RectangleOverlay for Overlay {
        fn begin(&mut self, _: RectanglePurpose) -> Result<OperationId, String> {
            self.0.lock().unwrap().entries.push("overlay".into());
            Ok(7)
        }
        fn poll(&mut self) -> SelectionEvent {
            self.0
                .lock()
                .unwrap()
                .event
                .take()
                .unwrap_or(SelectionEvent::Pending)
        }
        fn cancel(&mut self) {
            self.0.lock().unwrap().entries.push("overlay-close".into());
        }
    }
    struct Capture(Arc<Mutex<Log>>);
    impl CaptureAdapter for Capture {
        fn capture_rect(&mut self, r: ScreenRect) -> Result<RgbaImage, String> {
            let mut l = self.0.lock().unwrap();
            l.entries.push(format!("capture:{r:?}"));
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
    struct Assets(Arc<Mutex<Log>>);
    impl AssetStoreAdapter for Assets {
        fn write_png_asset(&mut self, _: u64, _: &RgbaImage) -> Result<u64, String> {
            let mut l = self.0.lock().unwrap();
            l.entries.push("write".into());
            if l.write_error {
                Err("write failed".into())
            } else {
                Ok(42)
            }
        }
    }
    fn fixture(saved: SavedVisibility) -> (VisualCaptureWorkflow, Arc<Mutex<Log>>) {
        let l = Arc::new(Mutex::new(Log::default()));
        (
            VisualCaptureWorkflow::new(
                Box::new(Vis(l.clone(), saved)),
                Box::new(Clock(l.clone())),
                Box::new(Overlay(l.clone())),
                Box::new(Capture(l.clone())),
                Box::new(Assets(l.clone())),
            ),
            l,
        )
    }
    fn reach_selecting(
        w: &mut VisualCaptureWorkflow,
        l: &Arc<Mutex<Log>>,
        purpose: RectanglePurpose,
    ) {
        w.begin(
            DraftToken {
                macro_id: 3,
                draft_generation: 9,
            },
            purpose,
        )
        .unwrap();
        w.tick();
        assert!(matches!(
            w.state(),
            WorkflowState::WaitingForDesktopRedraw { .. }
        ));
        w.tick();
        assert!(
            matches!(w.state(), WorkflowState::WaitingForDesktopRedraw { .. }),
            "clock wait is nonblocking"
        );
        l.lock().unwrap().now_ms = 34;
        w.tick();
        assert!(matches!(w.state(), WorkflowState::Selecting { .. }));
    }
    fn complete_restoration(w: &mut VisualCaptureWorkflow) {
        while w.active() {
            w.tick();
        }
    }

    #[test]
    fn pick_region_hides_waits_selects_restores_then_publishes() {
        let (mut w, l) = fixture(SavedVisibility {
            launcher: true,
            mkmacro_dialog: true,
        });
        reach_selecting(&mut w, &l, RectanglePurpose::SearchRegion);
        let rect = ScreenRect::new(-20, 4, 12, 8);
        l.lock().unwrap().event = Some(SelectionEvent::Confirmed {
            operation_id: 7,
            rect,
        });
        w.tick();
        assert!(matches!(w.state(), WorkflowState::Restoring { .. }));
        assert!(w.take_completed().is_none());
        w.tick();
        assert_eq!(
            w.take_completed(),
            Some(WorkflowOutcome::Region {
                token: DraftToken {
                    macro_id: 3,
                    draft_generation: 9
                },
                rect
            })
        );
        let g = l.lock().unwrap();
        assert_eq!(g.restores, 1);
        assert_eq!(g.entries[0..3], ["hide", "overlay", "overlay-close"]);
    }
    #[test]
    fn reference_capture_occurs_only_after_confirmation_and_preserves_exact_rect() {
        let (mut w, l) = fixture(SavedVisibility {
            launcher: true,
            mkmacro_dialog: true,
        });
        reach_selecting(&mut w, &l, RectanglePurpose::ReferenceImageCapture);
        assert!(
            !l.lock()
                .unwrap()
                .entries
                .iter()
                .any(|x| x.starts_with("capture"))
        );
        let rect = ScreenRect::new(5, -8, 2, 3);
        l.lock().unwrap().event = Some(SelectionEvent::Confirmed {
            operation_id: 7,
            rect,
        });
        w.tick();
        assert!(matches!(w.state(),WorkflowState::Capturing{rect:r,..} if *r==rect));
        w.tick();
        w.tick();
        assert_eq!(
            w.take_completed(),
            Some(WorkflowOutcome::Asset {
                token: DraftToken {
                    macro_id: 3,
                    draft_generation: 9
                },
                asset_id: 42
            })
        );
        assert!(
            l.lock()
                .unwrap()
                .entries
                .contains(&format!("capture:{rect:?}"))
        );
    }
    #[test]
    fn escape_before_or_during_drag_restores_exactly_once() {
        for event in [
            SelectionEvent::Cancelled { operation_id: 7 },
            SelectionEvent::Cancelled { operation_id: 7 },
        ] {
            let saved = SavedVisibility {
                launcher: false,
                mkmacro_dialog: true,
            };
            let (mut w, l) = fixture(saved);
            reach_selecting(&mut w, &l, RectanglePurpose::SearchRegion);
            l.lock().unwrap().event = Some(event);
            complete_restoration(&mut w);
            let g = l.lock().unwrap();
            assert_eq!(g.restores, 1);
            assert_eq!(g.saved, Some(saved));
        }
    }
    #[test]
    fn failures_restore_and_never_publish_an_asset() {
        for capture_failure in [true, false] {
            let (mut w, l) = fixture(SavedVisibility {
                launcher: true,
                mkmacro_dialog: true,
            });
            {
                let mut g = l.lock().unwrap();
                g.capture_error = capture_failure;
                g.write_error = !capture_failure;
            }
            reach_selecting(&mut w, &l, RectanglePurpose::ReferenceImageCapture);
            l.lock().unwrap().event = Some(SelectionEvent::Confirmed {
                operation_id: 7,
                rect: ScreenRect::new(0, 0, 1, 1),
            });
            complete_restoration(&mut w);
            assert!(matches!(
                w.take_completed(),
                Some(WorkflowOutcome::Failed(_))
            ));
            assert_eq!(l.lock().unwrap().restores, 1);
        }
    }
    #[test]
    fn editor_close_and_drop_are_idempotent() {
        let (mut w, l) = fixture(SavedVisibility {
            launcher: false,
            mkmacro_dialog: false,
        });
        w.begin(
            DraftToken {
                macro_id: 1,
                draft_generation: 1,
            },
            RectanglePurpose::SearchRegion,
        )
        .unwrap();
        w.cancel();
        w.cancel();
        complete_restoration(&mut w);
        assert_eq!(l.lock().unwrap().restores, 1);
        drop(w);
        assert_eq!(l.lock().unwrap().restores, 1);
    }
    #[test]
    fn stale_overlay_result_is_ignored() {
        let (mut w, l) = fixture(SavedVisibility {
            launcher: true,
            mkmacro_dialog: true,
        });
        reach_selecting(&mut w, &l, RectanglePurpose::SearchRegion);
        l.lock().unwrap().event = Some(SelectionEvent::Confirmed {
            operation_id: 6,
            rect: ScreenRect::new(0, 0, 4, 4),
        });
        w.tick();
        assert!(matches!(w.state(), WorkflowState::Selecting { .. }));
        w.cancel();
        complete_restoration(&mut w);
    }
}
