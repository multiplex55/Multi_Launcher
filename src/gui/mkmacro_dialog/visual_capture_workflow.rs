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
    atomic::{AtomicU64, Ordering},
};

type OverlayServiceFactory =
    Arc<dyn Fn() -> Result<NativeVisualOverlayService, std::io::Error> + Send + Sync>;

/// Cloneable command client for the thread which exclusively owns native overlays.
/// GUI calls only enqueue work or drain already-produced semantic events.
#[derive(Clone)]
pub struct SharedVisualOverlayController(Arc<SharedOverlayClient>);
struct SharedOverlayClient {
    /// This mutex is the serialization boundary for starts, retirement, and
    /// terminal shutdown.  In particular, shutdown cannot race a replacement
    /// worker into existence.
    service: Mutex<OverlayServiceState>,
    factory: OverlayServiceFactory,
    next_id: AtomicU64,
    active_id: AtomicU64,
    editor_events: Mutex<VecDeque<VisualOverlayEvent>>,
}
struct OverlayServiceState {
    service: Option<NativeVisualOverlayService>,
    terminal_shutdown: bool,
}
impl SharedVisualOverlayController {
    /// Creates the production owner.  This is deliberately visible only to the
    /// containing dialog module so action/condition editors cannot accidentally
    /// grow their own native worker.
    pub(super) fn new_dialog_owner() -> Self {
        Self::from_factory(Arc::new(NativeVisualOverlayService::start), true)
    }
    /// Legacy one-shot test constructor.  Because the controller is consumed by
    /// generation one, this constructor cannot validate worker restart.
    pub fn new(controller: VisualOverlayController) -> Self {
        let controller = Arc::new(Mutex::new(Some(controller)));
        Self::from_factory(
            Arc::new(move || {
                let controller = controller.lock().unwrap().take().ok_or_else(|| {
                    std::io::Error::other("one-shot overlay controller was already consumed")
                })?;
                NativeVisualOverlayService::start_with(move || controller)
            }),
            true,
        )
    }
    fn from_factory(factory: OverlayServiceFactory, eager_start: bool) -> Self {
        let service = eager_start.then(|| factory()).and_then(Result::ok);
        Self(Arc::new(SharedOverlayClient {
            service: Mutex::new(OverlayServiceState {
                service,
                terminal_shutdown: false,
            }),
            factory,
            next_id: AtomicU64::new(1),
            active_id: AtomicU64::new(0),
            editor_events: Mutex::new(VecDeque::new()),
        }))
    }

    /// Restart-capable constructor for deterministic unit tests.  The closure
    /// is invoked once per generation and may inject a startup failure.
    #[cfg(test)]
    pub(crate) fn new_with_controller_factory<F>(factory: F) -> Self
    where
        F: Fn() -> Result<VisualOverlayController, std::io::Error> + Send + Sync + 'static,
    {
        let factory = Arc::new(factory);
        Self::from_factory(
            Arc::new(move || {
                let controller = factory()?;
                NativeVisualOverlayService::start_with(move || controller)
            }),
            false,
        )
    }

    /// Explicitly and permanently shuts down the shared native owner.
    #[cfg(test)]
    pub(crate) fn terminal_shutdown_for_test(&self) {
        let mut state = self.0.service.lock().unwrap();
        state.terminal_shutdown = true;
        self.0.active_id.store(0, Ordering::Release);
        if let Some(mut service) = state.service.take() {
            service.shutdown_and_join();
        }
    }

    #[cfg(test)]
    pub(crate) fn test_fixture() -> TestOverlayServiceFixture {
        let observer = Arc::new(super::visual_overlay::ServiceTestObserver::default());
        let factory: OverlayServiceFactory = Arc::new({
            let observer = observer.clone();
            move || {
                NativeVisualOverlayService::start_with_observer(
                    {
                        let observer = observer.clone();
                        move || {
                            VisualOverlayController::new(Box::new(
                                super::visual_overlay::ServiceTestRenderer(observer),
                            ))
                        }
                    },
                    observer.clone(),
                )
            }
        });
        TestOverlayServiceFixture {
            controller: Self::from_factory(factory, true),
            observer,
        }
    }
    fn allocate(&self) -> OperationId {
        loop {
            let id = self.0.next_id.fetch_add(1, Ordering::Relaxed);
            if id != 0 {
                return id;
            }
        }
    }
    fn send_with_recovery(&self, id: OperationId, command: VisualOverlayCommand) -> OperationId {
        let mut buffered = Vec::new();
        let failure = {
            let mut state = self.0.service.lock().unwrap();
            if state.terminal_shutdown {
                Some("Visual overlay owner has shut down".to_owned())
            } else {
                let dead = state
                    .service
                    .as_ref()
                    .is_some_and(|service| service.is_finished());
                if dead {
                    let mut retired = state.service.take().unwrap();
                    buffered.extend(retired.cleanup_finished());
                    self.0.active_id.store(0, Ordering::Release);
                }
                if state.service.is_none() {
                    match (self.0.factory)() {
                        Ok(service) => state.service = Some(service),
                        Err(error) => {
                            let prefix = if dead {
                                "Visual overlay worker terminated and could not be restarted"
                            } else {
                                "Could not start visual overlay worker"
                            };
                            return self.finish_dispatch(
                                id,
                                buffered,
                                Some(format!("{prefix}: {error}")),
                            );
                        }
                    }
                }
                let service = state.service.as_ref().unwrap();
                let previous = self.0.active_id.load(Ordering::Acquire);
                // Replacement is part of dispatch, rather than a convention imposed
                // on individual editors.  Consequently active and passive operations
                // have identical ordering and the old operation is cancelled once.
                let replacement_sent = previous == 0
                    || service
                        .commands
                        .send(VisualOverlayCommand::Cancel {
                            expected_operation_id: Some(previous),
                        })
                        .is_ok();
                if replacement_sent && service.commands.send(command.clone()).is_ok() {
                    self.0.active_id.store(id, Ordering::Release);
                    None
                } else {
                    // The receiver can close between is_finished and send. Retire
                    // that generation and retry this same request (and id) once.
                    let mut retired = state.service.take().unwrap();
                    retired.shutdown_and_join();
                    self.0.active_id.store(0, Ordering::Release);
                    match (self.0.factory)() {
                        Err(error) => Some(format!(
                            "Visual overlay worker terminated and could not be restarted: {error}"
                        )),
                        Ok(service) => {
                            state.service = Some(service);
                            if state
                                .service
                                .as_ref()
                                .unwrap()
                                .commands
                                .send(command)
                                .is_ok()
                            {
                                self.0.active_id.store(id, Ordering::Release);
                                None
                            } else {
                                let mut rejected = state.service.take().unwrap();
                                rejected.shutdown_and_join();
                                Some("Visual overlay replacement rejected the command".into())
                            }
                        }
                    }
                }
            }
        };
        self.finish_dispatch(id, buffered, failure)
    }
    fn finish_dispatch(
        &self,
        id: OperationId,
        buffered: Vec<VisualOverlayEvent>,
        failure: Option<String>,
    ) -> OperationId {
        let mut events = self.0.editor_events.lock().unwrap();
        events.extend(buffered);
        if let Some(message) = failure {
            let _ = self
                .0
                .active_id
                .compare_exchange(id, 0, Ordering::AcqRel, Ordering::Acquire);
            events.push_back(VisualOverlayEvent::Error {
                operation_id: id,
                error: VisualOverlayError {
                    kind: OverlayErrorKind::Platform,
                    message,
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
        self.send_with_recovery(
            id,
            VisualOverlayCommand::BeginRectanglePick {
                operation_id: id,
                purpose,
                virtual_desktop,
            },
        )
    }
    pub fn begin_point_pick(
        &self,
        request: super::visual_overlay::VisualPointRequest,
    ) -> OperationId {
        let id = self.allocate();
        self.send_with_recovery(
            id,
            VisualOverlayCommand::PickPoint {
                operation_id: id,
                request,
            },
        )
    }
    pub fn preview_rectangle(&self, rect: ScreenRect) -> OperationId {
        let id = self.allocate();
        self.send_with_recovery(
            id,
            VisualOverlayCommand::PreviewRectangle {
                operation_id: id,
                rect,
            },
        )
    }
    pub fn highlight_monitor(&self, monitor: crate::mkmacro::MonitorDescriptor) -> OperationId {
        let id = self.allocate();
        self.send_with_recovery(
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
        self.send_with_recovery(
            id,
            VisualOverlayCommand::IdentifyMonitors {
                operation_id: id,
                monitors,
            },
        )
    }
    pub fn preview_desktop(&self, monitors: Vec<crate::mkmacro::MonitorDescriptor>) -> OperationId {
        let id = self.allocate();
        self.send_with_recovery(
            id,
            VisualOverlayCommand::PreviewDesktop {
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
        self.send_with_recovery(
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
            if let Some(service) = self.0.service.lock().unwrap().service.as_ref() {
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
    fn receive_into_editor(&self) {
        let mut incoming = vec![];
        if let Some(service) = self.0.service.lock().unwrap().service.as_ref() {
            incoming.extend(service.events.try_iter());
        }
        for event in &incoming {
            let id = match event {
                VisualOverlayEvent::PointConfirmed { operation_id, .. }
                | VisualOverlayEvent::RectangleConfirmed { operation_id, .. }
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

#[cfg(test)]
pub(crate) struct TestOverlayServiceFixture {
    pub controller: SharedVisualOverlayController,
    pub observer: Arc<super::visual_overlay::ServiceTestObserver>,
}
impl Drop for SharedOverlayClient {
    fn drop(&mut self) {
        let state = self.service.get_mut().unwrap();
        state.terminal_shutdown = true;
        if let Some(mut service) = state.service.take() {
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
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};

    struct Desktop;
    impl ScreenCaptureBackend for Desktop {
        fn virtual_desktop(&self) -> crate::mkmacro::ExecResult<ScreenRect> {
            Ok(ScreenRect::new(-10, -10, 100, 100))
        }
        fn region_bounds(
            &self,
            _: &crate::mkmacro::SearchRegion,
        ) -> crate::mkmacro::ExecResult<ScreenRect> {
            self.virtual_desktop()
        }
        fn capture_rect(
            &self,
            rect: ScreenRect,
            _: &dyn Fn() -> bool,
        ) -> crate::mkmacro::ExecResult<RgbaImage> {
            Ok(RgbaImage::new(rect.width, rect.height))
        }
    }

    #[test]
    fn cloned_clients_cancel_operations_without_terminating_the_shared_service() {
        let fixture = SharedVisualOverlayController::test_fixture();
        let dialog = fixture.controller.clone();
        let editor = dialog.clone();
        let mut adapter = VisualOverlayRectangleAdapter::new(dialog.clone(), Arc::new(Desktop));

        let first = editor.preview_rectangle(ScreenRect::new(1, 2, 3, 4));
        editor.cancel_operation(first);
        fixture.observer.wait_for_commands(2);
        let second = adapter.begin(RectanglePurpose::SearchRegion).unwrap();
        fixture.observer.wait_for_commands(3);
        assert_ne!(first, second);
        assert_eq!(fixture.observer.starts.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.observer.worker_ids.lock().unwrap().len(), 1);
        assert!(
            matches!(fixture.observer.commands.lock().unwrap()[1], VisualOverlayCommand::Cancel { expected_operation_id: Some(id) } if id == first)
        );
        assert!(
            matches!(fixture.observer.commands.lock().unwrap()[2], VisualOverlayCommand::BeginRectanglePick { operation_id, .. } if operation_id == second)
        );
        assert!(!dialog.poll().iter().any(|event| matches!(event, VisualOverlayEvent::Error { error, .. } if error.message.contains("shut down"))));

        drop(editor);
        drop(adapter);
        let third = dialog.preview_rectangle(ScreenRect::new(5, 6, 7, 8));
        fixture.observer.wait_for_commands(5);
        assert!(
            matches!(fixture.observer.commands.lock().unwrap()[3], VisualOverlayCommand::Cancel { expected_operation_id: Some(id) } if id == second)
        );
        assert!(
            matches!(fixture.observer.commands.lock().unwrap()[4], VisualOverlayCommand::PreviewRectangle { operation_id, .. } if operation_id == third)
        );
        assert_eq!(fixture.observer.joins.load(Ordering::SeqCst), 0);
        drop(dialog);
        drop(fixture.controller);
        assert_eq!(fixture.observer.shutdowns.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.observer.joins.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn one_service_orders_every_operation_type_and_replaces_each_predecessor_once() {
        use super::super::visual_overlay::WindowAreaKind;
        use crate::mkmacro::MonitorDescriptor;

        let fixture = SharedVisualOverlayController::test_fixture();
        let owner = fixture.controller.clone();
        let signed = ScreenRect::new(-1_920, -240, 1_280, 1_024);
        let monitor = MonitorDescriptor {
            index: 7,
            bounds: signed,
            primary: false,
        };
        let ids = [
            owner.begin_rectangle_pick(RectanglePurpose::ReferenceImageCapture, signed),
            owner.begin_rectangle_pick(RectanglePurpose::SearchRegion, signed),
            owner.preview_rectangle(signed),
            owner.highlight_monitor(monitor.clone()),
            owner.identify_monitors(vec![monitor.clone()]),
            owner.preview_desktop(vec![monitor.clone()]),
            owner.highlight_window(signed, WindowAreaKind::ClientArea),
        ];
        fixture.observer.wait_for_commands(13);

        let commands = fixture.observer.commands.lock().unwrap();
        for (index, id) in ids.iter().copied().enumerate().skip(1) {
            assert!(
                matches!(commands[index * 2 - 1], VisualOverlayCommand::Cancel { expected_operation_id: Some(old) } if old == ids[index - 1])
            );
            assert_eq!(commands.iter().filter(|command| matches!(command, VisualOverlayCommand::Cancel { expected_operation_id: Some(old) } if *old == ids[index - 1])).count(), 1);
            let command_id = match &commands[index * 2] {
                VisualOverlayCommand::BeginRectanglePick { operation_id, .. }
                | VisualOverlayCommand::PreviewRectangle { operation_id, .. }
                | VisualOverlayCommand::HighlightMonitor { operation_id, .. }
                | VisualOverlayCommand::IdentifyMonitors { operation_id, .. }
                | VisualOverlayCommand::PreviewDesktop { operation_id, .. }
                | VisualOverlayCommand::HighlightWindow { operation_id, .. } => *operation_id,
                other => panic!("unexpected operation command: {other:?}"),
            };
            assert_eq!(command_id, id);
        }
        assert!(
            matches!(commands[0], VisualOverlayCommand::BeginRectanglePick { operation_id, purpose: RectanglePurpose::ReferenceImageCapture, virtual_desktop } if operation_id == ids[0] && virtual_desktop == signed)
        );
        assert!(
            matches!(commands[2], VisualOverlayCommand::BeginRectanglePick { operation_id, purpose: RectanglePurpose::SearchRegion, virtual_desktop } if operation_id == ids[1] && virtual_desktop == signed)
        );
        assert!(
            matches!(&commands[8], VisualOverlayCommand::IdentifyMonitors { monitors, .. } if monitors == &vec![monitor.clone()])
        );
        assert!(
            matches!(&commands[10], VisualOverlayCommand::PreviewDesktop { monitors, .. } if monitors == &vec![monitor.clone()])
        );
        assert!(
            matches!(commands[12], VisualOverlayCommand::HighlightWindow { rect, area_kind: WindowAreaKind::ClientArea, .. } if rect == signed)
        );
        assert_eq!(
            ids.iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            ids.len()
        );
        assert_eq!(fixture.observer.starts.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.observer.shutdowns.load(Ordering::SeqCst), 0);
        assert_eq!(fixture.observer.joins.load(Ordering::SeqCst), 0);
        drop(commands);
        drop(owner);
        assert_eq!(fixture.observer.shutdowns.load(Ordering::SeqCst), 0);
        drop(fixture.controller);
        assert_eq!(fixture.observer.shutdowns.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.observer.joins.load(Ordering::SeqCst), 1);
    }

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
