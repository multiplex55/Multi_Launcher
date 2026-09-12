use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use super::ScreenDrawRecoveryBridge;
use super::capture::{
    DesktopCaptureBackend, LauncherVisibilityProbe, ScreenDrawCaptureBackend,
    ScreenDrawSessionSnapshot, SystemLauncherVisibilityProbe,
};
#[cfg(not(test))]
use super::native_runtime::SystemNativeSessionFactory;
use super::native_runtime::{
    ExportRenderRequest, NativeRuntimeState, NativeSessionCommand, NativeSessionConfig,
    NativeSessionEvent, NativeSessionFactory, NativeSessionHandle,
};
use super::{
    CanvasBackground, ExportBackground, ExportDestination, RgbaColor, ScreenDrawSettings,
    ScreenDrawTool,
};
use super::{ExportOutcome, ExportRequest, ExportScope};
use crate::mkmacro::screen::{CapturedRegion, ScreenRect};

const LAUNCHER_PARKING_TIMEOUT: Duration = Duration::from_secs(1);
const LAUNCHER_PARKING_REPOLL_INTERVAL: Duration = Duration::from_millis(16);
const DESKTOP_CAPTURE_TIMEOUT: Duration = Duration::from_secs(5);
const NATIVE_TEARDOWN_TIMEOUT: Duration = Duration::from_secs(3);

/// Identifies one capture/session attempt so late worker events cannot mutate a
/// replacement session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScreenDrawGeneration(u64);

impl ScreenDrawGeneration {
    pub(crate) const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenDrawState {
    NoSession,
    AwaitingLauncherParking {
        generation: ScreenDrawGeneration,
    },
    Capturing {
        generation: ScreenDrawGeneration,
    },
    AwaitingNativeTeardown {
        generation: ScreenDrawGeneration,
    },
    Drawing {
        generation: ScreenDrawGeneration,
    },
    Ghost {
        generation: ScreenDrawGeneration,
    },
    Finish {
        generation: ScreenDrawGeneration,
    },
    SelectingRegion {
        generation: ScreenDrawGeneration,
    },
    DisplayChanged {
        generation: ScreenDrawGeneration,
    },
    Failed {
        generation: ScreenDrawGeneration,
        message: String,
    },
}

impl ScreenDrawState {
    pub const fn generation(&self) -> Option<ScreenDrawGeneration> {
        match self {
            Self::NoSession => None,
            Self::AwaitingLauncherParking { generation }
            | Self::Capturing { generation }
            | Self::AwaitingNativeTeardown { generation }
            | Self::Drawing { generation }
            | Self::Ghost { generation }
            | Self::Finish { generation }
            | Self::SelectingRegion { generation }
            | Self::DisplayChanged { generation }
            | Self::Failed { generation, .. } => Some(*generation),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenDrawTransitionError {
    operation: &'static str,
    state: ScreenDrawState,
}

impl fmt::Display for ScreenDrawTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot {} while Screen Draw is in {:?}",
            self.operation, self.state
        )
    }
}

impl std::error::Error for ScreenDrawTransitionError {}

/// Lightweight main-thread orchestration state. Native resources and the
/// annotation document belong to the later session worker layer.
pub struct ScreenDrawController {
    state: ScreenDrawState,
    toolbar_open: bool,
    next_generation: u64,
    clear_request_count: u64,
    capture_backend: Arc<dyn DesktopCaptureBackend>,
    visibility_probe: Arc<dyn LauncherVisibilityProbe>,
    capture_tx: mpsc::Sender<CaptureCompletion>,
    capture_rx: mpsc::Receiver<CaptureCompletion>,
    pending_capture: Option<PendingCapture>,
    session_snapshot: Option<ScreenDrawSessionSnapshot>,
    native_factory: Arc<dyn NativeSessionFactory>,
    native_worker: Option<NativeSessionHandle>,
    runtime_state: Option<NativeRuntimeState>,
    latest_runtime_warning: Option<String>,
    latest_runtime_error: Option<String>,
    latest_export_outcome: Option<ExportOutcome>,
    export_in_flight: bool,
    pending_region: Option<PendingRegionSelection>,
    region_suppression: Option<crate::mouse_gestures::service::GestureSuppressionGuard>,
    pending_editor_handoff: Option<ScreenDrawEditorHandoff>,
    pending_new_capture: Option<PendingNewCapture>,
    settings: ScreenDrawSettings,
    clock: Arc<dyn MonotonicClock>,
    recovery_bridge: Arc<ScreenDrawRecoveryBridge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingRegionSelection {
    generation: ScreenDrawGeneration,
    background: ExportBackground,
    destination: ExportDestination,
    preview_dispatched: bool,
}

struct PendingCapture {
    generation: ScreenDrawGeneration,
    cancellation: Arc<AtomicBool>,
    virtual_desktop: Option<ScreenRect>,
    parking_requested: bool,
    parking_deadline: Option<Instant>,
    capture_deadline: Option<Instant>,
}

struct PendingNewCapture {
    generation: ScreenDrawGeneration,
    deadline: Instant,
    timed_out: bool,
}

struct CaptureCompletion {
    generation: ScreenDrawGeneration,
    result: Result<CapturedRegion, String>,
}

enum CaptureStartupExit {
    Cancelled,
    Failed(String),
}

trait MonotonicClock: Send + Sync {
    fn now(&self) -> Instant;
}

#[derive(Debug, Default)]
struct SystemMonotonicClock;

impl MonotonicClock for SystemMonotonicClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenDrawParkingRequest {
    pub generation: ScreenDrawGeneration,
    pub virtual_desktop: ScreenRect,
}

/// Feature-scoped effects produced by one non-blocking coordinator poll.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScreenDrawCapturePoll {
    pub park_launcher: Option<ScreenDrawParkingRequest>,
    pub restore_launcher: bool,
    pub capture_started: bool,
    pub capture_completed: bool,
    pub session_completed: bool,
    pub diagnostic: Option<String>,
    pub region_picker_ready: Option<ScreenDrawRegionPickerReady>,
    pub editor_handoff: Option<ScreenDrawEditorHandoff>,
    pub repoll_after: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenDrawRegionPickerReady {
    pub generation: ScreenDrawGeneration,
    pub bounds: ScreenRect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenDrawEditorHandoff {
    pub generation: ScreenDrawGeneration,
    pub image: image::RgbaImage,
}

impl fmt::Debug for ScreenDrawController {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScreenDrawController")
            .field("state", &self.state)
            .field("toolbar_open", &self.toolbar_open)
            .field("next_generation", &self.next_generation)
            .field("clear_request_count", &self.clear_request_count)
            .field("pending_capture", &self.pending_capture.is_some())
            .field("session_snapshot", &self.session_snapshot)
            .field("native_worker", &self.native_worker.is_some())
            .field("runtime_state", &self.runtime_state)
            .field("export_in_flight", &self.export_in_flight)
            .field(
                "pending_editor_handoff",
                &self.pending_editor_handoff.is_some(),
            )
            .finish_non_exhaustive()
    }
}

impl Default for ScreenDrawController {
    fn default() -> Self {
        #[cfg(windows)]
        let capture_backend: Arc<dyn DesktopCaptureBackend> =
            Arc::new(ScreenDrawCaptureBackend::system());
        #[cfg(not(windows))]
        let capture_backend: Arc<dyn DesktopCaptureBackend> = Arc::new(UnsupportedCaptureBackend);
        Self::with_capture_dependencies(capture_backend, Arc::new(SystemLauncherVisibilityProbe))
    }
}

#[cfg(not(windows))]
struct UnsupportedCaptureBackend;

#[cfg(not(windows))]
impl DesktopCaptureBackend for UnsupportedCaptureBackend {
    fn virtual_desktop(&self) -> Result<crate::mkmacro::screen::ScreenRect, String> {
        Err("Screen Draw capture is available only on Windows".into())
    }

    fn capture_desktop(&self, _cancelled: &dyn Fn() -> bool) -> Result<CapturedRegion, String> {
        Err("Screen Draw capture is available only on Windows".into())
    }
}

impl ScreenDrawController {
    fn with_capture_dependencies(
        capture_backend: Arc<dyn DesktopCaptureBackend>,
        visibility_probe: Arc<dyn LauncherVisibilityProbe>,
    ) -> Self {
        #[cfg(not(test))]
        let native_factory: Arc<dyn NativeSessionFactory> = Arc::new(SystemNativeSessionFactory);
        #[cfg(test)]
        let native_factory: Arc<dyn NativeSessionFactory> = Arc::new(TestNativeSessionFactory);
        Self::with_runtime_dependencies(capture_backend, visibility_probe, native_factory)
    }

    fn with_runtime_dependencies(
        capture_backend: Arc<dyn DesktopCaptureBackend>,
        visibility_probe: Arc<dyn LauncherVisibilityProbe>,
        native_factory: Arc<dyn NativeSessionFactory>,
    ) -> Self {
        Self::with_runtime_dependencies_and_clock(
            capture_backend,
            visibility_probe,
            native_factory,
            Arc::new(SystemMonotonicClock),
        )
    }

    #[cfg(test)]
    pub(crate) fn with_test_dependencies(
        capture_backend: Arc<dyn DesktopCaptureBackend>,
        visibility_probe: Arc<dyn LauncherVisibilityProbe>,
        native_factory: Arc<dyn NativeSessionFactory>,
    ) -> Self {
        Self::with_runtime_dependencies(capture_backend, visibility_probe, native_factory)
    }

    fn with_runtime_dependencies_and_clock(
        capture_backend: Arc<dyn DesktopCaptureBackend>,
        visibility_probe: Arc<dyn LauncherVisibilityProbe>,
        native_factory: Arc<dyn NativeSessionFactory>,
        clock: Arc<dyn MonotonicClock>,
    ) -> Self {
        let (capture_tx, capture_rx) = mpsc::channel();
        Self {
            state: ScreenDrawState::NoSession,
            toolbar_open: false,
            next_generation: 1,
            clear_request_count: 0,
            capture_backend,
            visibility_probe,
            capture_tx,
            capture_rx,
            pending_capture: None,
            session_snapshot: None,
            native_factory,
            native_worker: None,
            runtime_state: None,
            latest_runtime_warning: None,
            latest_runtime_error: None,
            latest_export_outcome: None,
            export_in_flight: false,
            pending_region: None,
            region_suppression: None,
            pending_editor_handoff: None,
            pending_new_capture: None,
            settings: ScreenDrawSettings::default(),
            clock,
            recovery_bridge: Arc::new(ScreenDrawRecoveryBridge::default()),
        }
    }

    pub(crate) fn set_recovery_bridge(&mut self, bridge: Arc<ScreenDrawRecoveryBridge>) {
        self.recovery_bridge.set_active(false);
        self.recovery_bridge = bridge;
        self.recovery_bridge
            .set_active(Self::state_is_active(&self.state));
        if let Some(worker) = self.native_worker.as_ref() {
            self.recovery_bridge
                .install_emergency_handle(worker.emergency_handle());
        }
    }

    fn state_is_active(state: &ScreenDrawState) -> bool {
        !matches!(
            state,
            ScreenDrawState::NoSession | ScreenDrawState::Failed { .. }
        )
    }

    fn set_state(&mut self, state: ScreenDrawState) {
        self.recovery_bridge
            .set_active(Self::state_is_active(&state));
        self.state = state;
    }

    /// Re-publishes controller-owned lifecycle state after an external trigger
    /// has staged recovery optimistically before its GUI event is reduced.
    pub(crate) fn reconcile_recovery_publication(&self) {
        self.recovery_bridge
            .set_active(Self::state_is_active(&self.state));
    }
    pub fn state(&self) -> &ScreenDrawState {
        &self.state
    }

    pub const fn toolbar_open(&self) -> bool {
        self.toolbar_open
    }

    pub const fn clear_request_count(&self) -> u64 {
        self.clear_request_count
    }

    pub fn session_snapshot(&self) -> Option<&ScreenDrawSessionSnapshot> {
        self.session_snapshot.as_ref()
    }

    pub fn runtime_state(&self) -> Option<NativeRuntimeState> {
        self.runtime_state
    }

    pub fn latest_runtime_warning(&self) -> Option<&str> {
        self.latest_runtime_warning.as_deref()
    }

    pub fn latest_runtime_error(&self) -> Option<&str> {
        self.latest_runtime_error.as_deref()
    }

    pub fn latest_export_outcome(&self) -> Option<&ExportOutcome> {
        self.latest_export_outcome.as_ref()
    }

    pub const fn export_in_flight(&self) -> bool {
        self.export_in_flight
    }

    pub fn settings(&self) -> &ScreenDrawSettings {
        &self.settings
    }

    pub fn update_settings(&mut self, mut settings: ScreenDrawSettings) {
        settings.normalize();
        self.settings = settings;
    }

    #[cfg(test)]
    pub(crate) fn install_test_native_worker(
        &mut self,
    ) -> (
        mpsc::Receiver<NativeSessionCommand>,
        mpsc::Sender<NativeSessionEvent>,
    ) {
        let (worker, commands, events) = NativeSessionHandle::test_stub_with_events();
        self.recovery_bridge
            .install_emergency_handle(worker.emergency_handle());
        self.native_worker = Some(worker);
        (commands, events)
    }

    #[cfg(test)]
    pub(crate) fn install_test_session_snapshot(
        &mut self,
        generation: ScreenDrawGeneration,
        capture: CapturedRegion,
    ) {
        self.session_snapshot = Some(ScreenDrawSessionSnapshot::new(generation, capture));
    }

    pub fn request_start(&mut self) -> Result<ScreenDrawGeneration, ScreenDrawTransitionError> {
        if !matches!(
            self.state,
            ScreenDrawState::NoSession | ScreenDrawState::Failed { .. }
        ) {
            return Err(self.invalid("start a capture"));
        }
        Ok(self.stage_capture())
    }

    pub fn request_new_capture(
        &mut self,
    ) -> Result<ScreenDrawGeneration, ScreenDrawTransitionError> {
        if self.native_worker.is_none() {
            return Ok(self.stage_capture());
        }
        self.cancel_pending_capture();
        let generation = self.allocate_generation();
        self.toolbar_open = false;
        self.export_in_flight = false;
        self.pending_region = None;
        self.region_suppression = None;
        self.pending_editor_handoff = None;
        self.pending_new_capture = Some(PendingNewCapture {
            generation,
            deadline: self.clock.now() + NATIVE_TEARDOWN_TIMEOUT,
            timed_out: false,
        });
        self.set_state(ScreenDrawState::AwaitingNativeTeardown { generation });
        if let Err(error) = self.send_native(NativeSessionCommand::Shutdown) {
            // A closed command channel is not proof that native cleanup has
            // completed. Keep waiting for SessionClosed (or the deadline)
            // instead of starting capture beneath a potentially live HWND.
            self.latest_runtime_warning = Some(error.to_string());
        }
        Ok(generation)
    }

    pub fn open_toolbar(&mut self) {
        self.toolbar_open = true;
    }

    pub fn launcher_parked(
        &mut self,
        generation: ScreenDrawGeneration,
    ) -> Result<(), ScreenDrawTransitionError> {
        self.transition_generation(
            generation,
            "begin capture",
            |state| matches!(state, ScreenDrawState::AwaitingLauncherParking { .. }),
            ScreenDrawState::Capturing { generation },
        )
    }

    pub fn capture_succeeded(
        &mut self,
        generation: ScreenDrawGeneration,
    ) -> Result<(), ScreenDrawTransitionError> {
        self.transition_generation(
            generation,
            "complete capture",
            |state| matches!(state, ScreenDrawState::Capturing { .. }),
            ScreenDrawState::Drawing { generation },
        )?;
        self.pending_capture = None;
        self.toolbar_open = true;
        Ok(())
    }

    pub fn capture_failed(
        &mut self,
        generation: ScreenDrawGeneration,
        message: impl Into<String>,
    ) -> Result<(), ScreenDrawTransitionError> {
        if self.state.generation() != Some(generation)
            || !matches!(self.state, ScreenDrawState::Capturing { .. })
        {
            return Err(self.invalid("fail capture"));
        }
        self.finish_capture_startup(generation, CaptureStartupExit::Failed(message.into()));
        Ok(())
    }

    pub fn cancel_capture_startup(
        &mut self,
        generation: ScreenDrawGeneration,
    ) -> Result<ScreenDrawCapturePoll, ScreenDrawTransitionError> {
        if self.state.generation() != Some(generation)
            || !matches!(
                self.state,
                ScreenDrawState::AwaitingLauncherParking { .. } | ScreenDrawState::Capturing { .. }
            )
        {
            return Err(self.invalid("cancel capture startup"));
        }
        Ok(self.finish_capture_startup(generation, CaptureStartupExit::Cancelled))
    }

    pub(crate) fn fail_capture_startup(
        &mut self,
        generation: ScreenDrawGeneration,
        message: impl Into<String>,
    ) -> Result<ScreenDrawCapturePoll, ScreenDrawTransitionError> {
        if self.state.generation() != Some(generation)
            || !matches!(
                self.state,
                ScreenDrawState::AwaitingLauncherParking { .. } | ScreenDrawState::Capturing { .. }
            )
        {
            return Err(self.invalid("fail capture startup"));
        }
        Ok(self.finish_capture_startup(generation, CaptureStartupExit::Failed(message.into())))
    }

    /// Advances parking verification and consumes capture completions without
    /// blocking the GUI thread. The callback is invoked by the short-lived
    /// worker so egui wakes promptly when capture finishes.
    pub fn poll_capture(
        &mut self,
        launcher_hwnd: Option<usize>,
        request_repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> ScreenDrawCapturePoll {
        if let Some(poll) = self.poll_native_worker(Arc::clone(&request_repaint)) {
            return poll;
        }
        if let Some(poll) = self.poll_capture_timeout() {
            return poll;
        }
        if let Some(poll) = self.poll_capture_completion(Arc::clone(&request_repaint)) {
            return poll;
        }
        if let Some(poll) = self.dispatch_pending_region_preview() {
            return poll;
        }

        let Some(pending) = self.pending_capture.as_mut() else {
            return ScreenDrawCapturePoll::default();
        };
        if self.state.generation() != Some(pending.generation) {
            pending.cancellation.store(true, Ordering::Release);
            self.pending_capture = None;
            return ScreenDrawCapturePoll::default();
        }
        if matches!(self.state, ScreenDrawState::Capturing { .. }) {
            let repoll_after = pending
                .capture_deadline
                .map(|deadline| deadline.saturating_duration_since(self.clock.now()));
            return ScreenDrawCapturePoll {
                repoll_after,
                ..ScreenDrawCapturePoll::default()
            };
        }
        if !matches!(self.state, ScreenDrawState::AwaitingLauncherParking { .. }) {
            return ScreenDrawCapturePoll::default();
        }

        let now = self.clock.now();
        if pending.virtual_desktop.is_none() {
            let generation = pending.generation;
            let virtual_desktop = match self.capture_backend.virtual_desktop() {
                Ok(desktop) => desktop,
                Err(error) => return self.fail_pending_capture(generation, error),
            };
            pending.virtual_desktop = Some(virtual_desktop);
            pending.parking_deadline = Some(now + LAUNCHER_PARKING_TIMEOUT);
        }
        let virtual_desktop = pending
            .virtual_desktop
            .expect("capture preparation stores virtual desktop before parking");
        if !pending.parking_requested {
            pending.parking_requested = true;
            request_repaint();
            return ScreenDrawCapturePoll {
                park_launcher: Some(ScreenDrawParkingRequest {
                    generation: pending.generation,
                    virtual_desktop,
                }),
                ..ScreenDrawCapturePoll::default()
            };
        }

        let generation = pending.generation;
        let deadline = pending
            .parking_deadline
            .expect("parking request always has an elapsed-time deadline");
        if now >= deadline {
            return self.fail_pending_capture(
                generation,
                "launcher did not leave the virtual desktop before the parking timeout".into(),
            );
        }
        match self
            .visibility_probe
            .launcher_is_capture_parked(launcher_hwnd, virtual_desktop)
        {
            Ok(true) => self.start_capture_worker(generation, request_repaint, now),
            Ok(false) => {
                let remaining = deadline.saturating_duration_since(now);
                ScreenDrawCapturePoll {
                    repoll_after: Some(remaining.min(LAUNCHER_PARKING_REPOLL_INTERVAL)),
                    ..ScreenDrawCapturePoll::default()
                }
            }
            Err(error) => self.fail_pending_capture(generation, error),
        }
    }

    pub fn enter_ghost(&mut self) -> Result<(), ScreenDrawTransitionError> {
        let generation = self.require_generation("enter Ghost mode", |state| {
            matches!(state, ScreenDrawState::Drawing { .. })
        })?;
        self.send_native(NativeSessionCommand::Ghost)?;
        self.set_state(ScreenDrawState::Ghost { generation });
        Ok(())
    }

    /// Reconciles the GUI model after the process-wide emergency bridge has
    /// already delivered a native pause. Repeated native/UI notifications are
    /// intentionally harmless.
    pub(crate) fn reconcile_emergency_pause(&mut self) {
        let Some(generation) = self.state.generation() else {
            return;
        };
        match self.state {
            ScreenDrawState::Drawing { .. }
            | ScreenDrawState::Ghost { .. }
            | ScreenDrawState::Finish { .. }
            | ScreenDrawState::SelectingRegion { .. } => {
                self.pending_region = None;
                self.region_suppression = None;
                self.export_in_flight = false;
                self.toolbar_open = true;
                self.set_state(ScreenDrawState::Ghost { generation });
            }
            // Native safe-pause deliberately preserves DisplayChanged so a
            // stale desktop can never be resumed.
            ScreenDrawState::DisplayChanged { .. }
            | ScreenDrawState::AwaitingLauncherParking { .. }
            | ScreenDrawState::Capturing { .. }
            | ScreenDrawState::AwaitingNativeTeardown { .. }
            | ScreenDrawState::Failed { .. }
            | ScreenDrawState::NoSession => {}
        }
    }

    pub fn resume_drawing(&mut self) -> Result<(), ScreenDrawTransitionError> {
        let generation = self.require_generation("resume drawing", |state| {
            matches!(
                state,
                ScreenDrawState::Ghost { .. } | ScreenDrawState::Finish { .. }
            )
        })?;
        self.send_native(NativeSessionCommand::Resume)?;
        self.set_state(ScreenDrawState::Drawing { generation });
        Ok(())
    }

    pub fn finish(&mut self) -> Result<(), ScreenDrawTransitionError> {
        let generation = self.require_generation("finish", |state| {
            matches!(
                state,
                ScreenDrawState::Drawing { .. } | ScreenDrawState::Ghost { .. }
            )
        })?;
        self.send_native(NativeSessionCommand::Finish)?;
        self.set_state(ScreenDrawState::Finish { generation });
        Ok(())
    }

    pub fn request_export(
        &mut self,
        request: ExportRequest,
    ) -> Result<(), ScreenDrawTransitionError> {
        let generation = self.require_generation("export", |state| {
            matches!(
                state,
                ScreenDrawState::Finish { .. } | ScreenDrawState::DisplayChanged { .. }
            )
        })?;
        if self.export_in_flight {
            return Err(self.invalid("start another export while one is already running"));
        }
        self.latest_runtime_error = None;
        self.send_native(NativeSessionCommand::RenderExport(ExportRenderRequest {
            generation,
            request,
        }))?;
        self.export_in_flight = true;
        Ok(())
    }

    pub fn begin_region_selection(
        &mut self,
        background: ExportBackground,
        destination: ExportDestination,
    ) -> Result<(), ScreenDrawTransitionError> {
        let generation = self.require_generation("select an export region", |state| {
            matches!(state, ScreenDrawState::Finish { .. })
        })?;
        if self.export_in_flight {
            return Err(self.invalid("select an export region while an export is running"));
        }
        let suppression = crate::mouse_gestures::service::acquire_gesture_suppression();
        self.pending_region = Some(PendingRegionSelection {
            generation,
            background,
            destination,
            preview_dispatched: false,
        });
        self.region_suppression = Some(suppression);
        self.latest_runtime_error = None;
        self.export_in_flight = true;
        self.toolbar_open = false;
        self.set_state(ScreenDrawState::SelectingRegion { generation });
        Ok(())
    }

    pub fn complete_region_selection(
        &mut self,
        generation: ScreenDrawGeneration,
        rect: ScreenRect,
    ) -> Result<(), ScreenDrawTransitionError> {
        if self.state.generation() != Some(generation)
            || !matches!(self.state, ScreenDrawState::SelectingRegion { .. })
        {
            return Err(self.invalid("finish region selection"));
        }
        let Some(pending) = self
            .pending_region
            .filter(|pending| pending.generation == generation)
        else {
            return Err(self.invalid("finish region selection without a pending request"));
        };
        let request = ExportRequest {
            scope: ExportScope::Region(rect),
            background: pending.background,
            destination: pending.destination,
        };
        self.send_native(NativeSessionCommand::EndRegionSelection)?;
        if let Err(error) =
            self.send_native(NativeSessionCommand::RenderExport(ExportRenderRequest {
                generation,
                request,
            }))
        {
            self.restore_region_finish(generation, Some(error.to_string()));
            return Err(error);
        }
        self.pending_region = None;
        self.region_suppression = None;
        self.pending_editor_handoff = None;
        self.toolbar_open = true;
        self.set_state(ScreenDrawState::Finish { generation });
        Ok(())
    }

    pub fn cancel_region_selection(
        &mut self,
        generation: ScreenDrawGeneration,
        diagnostic: Option<String>,
    ) -> Result<(), ScreenDrawTransitionError> {
        if self.state.generation() != Some(generation)
            || !matches!(self.state, ScreenDrawState::SelectingRegion { .. })
        {
            return Err(self.invalid("cancel region selection"));
        }
        let _ = self.send_native(NativeSessionCommand::EndRegionSelection);
        self.restore_region_finish(generation, diagnostic);
        Ok(())
    }

    pub fn note_display_changed(
        &mut self,
        generation: ScreenDrawGeneration,
    ) -> Result<(), ScreenDrawTransitionError> {
        if self.state.generation() != Some(generation)
            || !matches!(
                self.state,
                ScreenDrawState::Drawing { .. }
                    | ScreenDrawState::Ghost { .. }
                    | ScreenDrawState::Finish { .. }
                    | ScreenDrawState::SelectingRegion { .. }
            )
        {
            return Err(self.invalid("handle a display change"));
        }
        self.send_native(NativeSessionCommand::DisplayChanged)?;
        self.pending_region = None;
        self.region_suppression = None;
        self.pending_editor_handoff = None;
        self.pending_new_capture = None;
        self.export_in_flight = false;
        self.toolbar_open = true;
        self.set_state(ScreenDrawState::DisplayChanged { generation });
        Ok(())
    }

    pub fn request_clear(&mut self) -> Result<(), ScreenDrawTransitionError> {
        self.require_generation("clear annotations", |state| {
            matches!(
                state,
                ScreenDrawState::Drawing { .. }
                    | ScreenDrawState::Ghost { .. }
                    | ScreenDrawState::Finish { .. }
            )
        })?;
        self.send_native(NativeSessionCommand::Clear)?;
        self.clear_request_count = self.clear_request_count.saturating_add(1);
        Ok(())
    }

    pub fn set_tool(&mut self, tool: ScreenDrawTool) -> Result<(), ScreenDrawTransitionError> {
        self.require_active_session("change tools")?;
        self.send_native(NativeSessionCommand::SetTool(tool))
    }

    pub fn set_color(&mut self, color: RgbaColor) -> Result<(), ScreenDrawTransitionError> {
        self.require_active_session("change color")?;
        self.send_native(NativeSessionCommand::SetColor(color))
    }

    pub fn set_thickness(&mut self, thickness: f32) -> Result<(), ScreenDrawTransitionError> {
        self.require_active_session("change thickness")?;
        self.send_native(NativeSessionCommand::SetThickness(thickness))
    }

    pub fn undo(&mut self) -> Result<(), ScreenDrawTransitionError> {
        self.require_active_session("undo")?;
        self.send_native(NativeSessionCommand::Undo)
    }

    pub fn redo(&mut self) -> Result<(), ScreenDrawTransitionError> {
        self.require_active_session("redo")?;
        self.send_native(NativeSessionCommand::Redo)
    }

    pub fn set_annotations_visible(
        &mut self,
        visible: bool,
    ) -> Result<(), ScreenDrawTransitionError> {
        self.require_active_session("change annotation visibility")?;
        self.send_native(NativeSessionCommand::SetAnnotationsVisible(visible))
    }

    pub fn set_background(
        &mut self,
        background: CanvasBackground,
    ) -> Result<(), ScreenDrawTransitionError> {
        self.require_active_session("change background")?;
        self.send_native(NativeSessionCommand::SetBackground(background))
    }

    pub fn close(&mut self) {
        self.cancel_pending_capture();
        self.teardown_native_worker();
        self.session_snapshot = None;
        self.set_state(ScreenDrawState::NoSession);
        self.toolbar_open = false;
        self.runtime_state = None;
        self.export_in_flight = false;
        self.pending_region = None;
        self.region_suppression = None;
        self.pending_editor_handoff = None;
        self.pending_new_capture = None;
    }

    pub fn complete_session(
        &mut self,
        generation: ScreenDrawGeneration,
    ) -> Result<(), ScreenDrawTransitionError> {
        if self.state.generation() != Some(generation)
            || !matches!(
                self.state,
                ScreenDrawState::Finish { .. } | ScreenDrawState::DisplayChanged { .. }
            )
        {
            return Err(self.invalid("complete session"));
        }
        self.set_state(ScreenDrawState::NoSession);
        self.teardown_native_worker();
        self.session_snapshot = None;
        self.runtime_state = None;
        self.export_in_flight = false;
        self.pending_region = None;
        self.region_suppression = None;
        self.pending_editor_handoff = None;
        self.pending_new_capture = None;
        Ok(())
    }

    fn stage_capture(&mut self) -> ScreenDrawGeneration {
        self.cancel_pending_capture();
        self.teardown_native_worker();
        let generation = self.allocate_generation();
        self.activate_capture(generation);
        generation
    }

    fn allocate_generation(&mut self) -> ScreenDrawGeneration {
        let generation = ScreenDrawGeneration(self.next_generation);
        self.next_generation = self.next_generation.saturating_add(1);
        generation
    }

    fn activate_capture(&mut self, generation: ScreenDrawGeneration) {
        self.toolbar_open = false;
        self.session_snapshot = None;
        self.runtime_state = None;
        self.latest_runtime_warning = None;
        self.latest_runtime_error = None;
        self.latest_export_outcome = None;
        self.export_in_flight = false;
        self.pending_region = None;
        self.region_suppression = None;
        self.pending_editor_handoff = None;
        self.pending_new_capture = None;
        self.pending_capture = Some(PendingCapture {
            generation,
            cancellation: Arc::new(AtomicBool::new(false)),
            virtual_desktop: None,
            parking_requested: false,
            parking_deadline: None,
            capture_deadline: None,
        });
        self.set_state(ScreenDrawState::AwaitingLauncherParking { generation });
    }

    fn start_capture_worker(
        &mut self,
        generation: ScreenDrawGeneration,
        request_repaint: Arc<dyn Fn() + Send + Sync>,
        now: Instant,
    ) -> ScreenDrawCapturePoll {
        let Some(pending) = self.pending_capture.as_mut() else {
            return ScreenDrawCapturePoll::default();
        };
        pending.capture_deadline = Some(now + DESKTOP_CAPTURE_TIMEOUT);
        let cancellation = Arc::clone(&pending.cancellation);
        let capture_backend = Arc::clone(&self.capture_backend);
        let capture_tx = self.capture_tx.clone();
        self.set_state(ScreenDrawState::Capturing { generation });
        let spawn = std::thread::Builder::new()
            .name(format!("screen-draw-capture-{}", generation.get()))
            .spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    capture_backend.capture_desktop(&|| cancellation.load(Ordering::Acquire))
                }))
                .unwrap_or_else(|_| Err("Screen Draw capture worker panicked".into()));
                let _ = capture_tx.send(CaptureCompletion { generation, result });
                request_repaint();
            });
        if let Err(error) = spawn {
            return self.fail_pending_capture(
                generation,
                format!("failed to start Screen Draw capture worker: {error}"),
            );
        }
        ScreenDrawCapturePoll {
            capture_started: true,
            repoll_after: Some(DESKTOP_CAPTURE_TIMEOUT),
            ..ScreenDrawCapturePoll::default()
        }
    }

    fn poll_capture_timeout(&mut self) -> Option<ScreenDrawCapturePoll> {
        if !matches!(self.state, ScreenDrawState::Capturing { .. }) {
            return None;
        }
        let pending = self.pending_capture.as_ref()?;
        let deadline = pending.capture_deadline?;
        let now = self.clock.now();
        if now >= deadline {
            return Some(self.fail_pending_capture(
                pending.generation,
                "Screen Draw desktop capture did not complete before the capture timeout".into(),
            ));
        }
        None
    }

    fn poll_capture_completion(
        &mut self,
        request_repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> Option<ScreenDrawCapturePoll> {
        while let Ok(completion) = self.capture_rx.try_recv() {
            if self.state.generation() != Some(completion.generation)
                || !matches!(self.state, ScreenDrawState::Capturing { .. })
            {
                continue;
            }
            return Some(match completion.result {
                Ok(capture) => {
                    self.pending_capture = None;
                    self.session_snapshot = Some(ScreenDrawSessionSnapshot::new(
                        completion.generation,
                        capture,
                    ));
                    if let Err(error) = self
                        .start_native_worker(completion.generation, Arc::clone(&request_repaint))
                    {
                        self.fail_pending_capture(completion.generation, error)
                    } else if let Err(error) = self.capture_succeeded(completion.generation) {
                        self.fail_pending_capture(completion.generation, error.to_string())
                    } else {
                        ScreenDrawCapturePoll {
                            capture_completed: true,
                            ..ScreenDrawCapturePoll::default()
                        }
                    }
                }
                Err(error) => self.fail_pending_capture(completion.generation, error),
            });
        }
        None
    }

    fn fail_pending_capture(
        &mut self,
        generation: ScreenDrawGeneration,
        message: String,
    ) -> ScreenDrawCapturePoll {
        self.finish_capture_startup(generation, CaptureStartupExit::Failed(message))
    }

    fn finish_capture_startup(
        &mut self,
        generation: ScreenDrawGeneration,
        exit: CaptureStartupExit,
    ) -> ScreenDrawCapturePoll {
        self.cancel_pending_capture();
        self.teardown_native_worker();
        self.session_snapshot = None;
        self.runtime_state = None;
        self.toolbar_open = false;
        self.export_in_flight = false;
        self.pending_region = None;
        self.region_suppression = None;
        self.pending_editor_handoff = None;
        self.pending_new_capture = None;
        let diagnostic = match exit {
            CaptureStartupExit::Cancelled => {
                self.set_state(ScreenDrawState::NoSession);
                None
            }
            CaptureStartupExit::Failed(message) => {
                self.latest_runtime_error = Some(message.clone());
                self.set_state(ScreenDrawState::Failed {
                    generation,
                    message: message.clone(),
                });
                Some(message)
            }
        };
        ScreenDrawCapturePoll {
            restore_launcher: true,
            diagnostic,
            ..ScreenDrawCapturePoll::default()
        }
    }

    fn cancel_pending_capture(&mut self) {
        if let Some(pending) = self.pending_capture.take() {
            pending.cancellation.store(true, Ordering::Release);
        }
    }

    fn start_native_worker(
        &mut self,
        generation: ScreenDrawGeneration,
        request_repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<(), String> {
        let snapshot = self
            .session_snapshot
            .as_ref()
            .filter(|snapshot| snapshot.generation() == generation)
            .cloned()
            .ok_or_else(|| {
                "Screen Draw capture completed without a session snapshot".to_string()
            })?;
        let settings = self.settings.clone();
        let worker = self.native_factory.spawn(NativeSessionConfig {
            snapshot,
            tool: settings.default_tool,
            color: settings.default_color,
            thickness: settings.default_thickness,
            settings: settings.clone(),
            request_repaint,
        })?;
        self.recovery_bridge
            .install_emergency_handle(worker.emergency_handle());
        self.native_worker = Some(worker);
        Ok(())
    }

    fn poll_native_worker(
        &mut self,
        request_repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> Option<ScreenDrawCapturePoll> {
        let mut terminal_error = None;
        let mut export_completed = None;
        let mut export_failure = None;
        let mut region_picker_ready = None;
        let mut region_failure = None;
        let mut editor_teardown_complete = false;
        let mut new_capture_teardown_complete = false;
        let mut emergency_paused = false;
        if let Some(worker) = self.native_worker.as_mut() {
            while let Some(event) = worker.try_recv() {
                match event {
                    NativeSessionEvent::SessionStarted(state) => self.runtime_state = Some(state),
                    NativeSessionEvent::ModeChanged(mode) => {
                        if let Some(state) = self.runtime_state.as_mut() {
                            state.mode = mode;
                        }
                        if let Some(generation) = self.state.generation() {
                            let next = match mode {
                                super::ScreenDrawMode::Drawing => {
                                    ScreenDrawState::Drawing { generation }
                                }
                                super::ScreenDrawMode::Ghost => {
                                    ScreenDrawState::Ghost { generation }
                                }
                                super::ScreenDrawMode::Finish => {
                                    ScreenDrawState::Finish { generation }
                                }
                                _ => self.state.clone(),
                            };
                            // Every native mode remains an active bridge state.
                            self.state = next;
                        }
                    }
                    NativeSessionEvent::ToolChanged(tool) => {
                        if let Some(state) = self.runtime_state.as_mut() {
                            state.tool = tool;
                        }
                    }
                    NativeSessionEvent::ColorChanged(color) => {
                        if let Some(state) = self.runtime_state.as_mut() {
                            state.color = color;
                        }
                    }
                    NativeSessionEvent::ThicknessChanged(thickness) => {
                        if let Some(state) = self.runtime_state.as_mut() {
                            state.thickness = thickness;
                        }
                    }
                    NativeSessionEvent::AnnotationsVisibilityChanged(visible) => {
                        if let Some(state) = self.runtime_state.as_mut() {
                            state.annotations_visible = visible;
                        }
                    }
                    NativeSessionEvent::BackgroundChanged(background) => {
                        if let Some(state) = self.runtime_state.as_mut() {
                            state.background = background;
                        }
                    }
                    NativeSessionEvent::Warning(message) => {
                        tracing::warn!(warning = %message, "Screen Draw native worker warning");
                        self.latest_runtime_warning = Some(message)
                    }
                    NativeSessionEvent::Error(message) => terminal_error = Some(message),
                    NativeSessionEvent::EmergencyPaused => emergency_paused = true,
                    NativeSessionEvent::DisplayChanged => {
                        self.pending_region = None;
                        self.region_suppression = None;
                        self.export_in_flight = false;
                        self.toolbar_open = true;
                        if let Some(generation) = self.state.generation() {
                            // Display recovery remains inside the active session.
                            self.state = ScreenDrawState::DisplayChanged { generation };
                        }
                        if let Some(state) = self.runtime_state.as_mut() {
                            state.mode = super::ScreenDrawMode::DisplayChanged;
                        }
                    }
                    NativeSessionEvent::SessionClosed => {
                        if self.pending_editor_handoff.is_some() {
                            editor_teardown_complete = true;
                        } else if self.pending_new_capture.is_some() {
                            new_capture_teardown_complete = true;
                        } else if terminal_error.is_none() {
                            terminal_error =
                                Some("Screen Draw native worker closed unexpectedly".to_string());
                        }
                    }
                    NativeSessionEvent::ExportCompleted {
                        generation,
                        outcome,
                    } => {
                        if self.state.generation() == Some(generation)
                            && matches!(
                                self.state,
                                ScreenDrawState::Finish { .. }
                                    | ScreenDrawState::DisplayChanged { .. }
                            )
                        {
                            export_completed = Some((generation, outcome));
                        }
                    }
                    NativeSessionEvent::ExportFailed {
                        generation,
                        message,
                    } => {
                        if self.state.generation() == Some(generation)
                            && matches!(
                                self.state,
                                ScreenDrawState::Finish { .. }
                                    | ScreenDrawState::DisplayChanged { .. }
                            )
                        {
                            export_failure = Some(message);
                        }
                    }
                    NativeSessionEvent::RegionPreviewReady { generation, bounds } => {
                        if self.state.generation() == Some(generation)
                            && matches!(self.state, ScreenDrawState::SelectingRegion { .. })
                            && self
                                .pending_region
                                .is_some_and(|pending| pending.generation == generation)
                        {
                            region_picker_ready =
                                Some(ScreenDrawRegionPickerReady { generation, bounds });
                        }
                    }
                    NativeSessionEvent::RegionPreviewFailed {
                        generation,
                        message,
                    } => {
                        if self.state.generation() == Some(generation)
                            && matches!(self.state, ScreenDrawState::SelectingRegion { .. })
                        {
                            region_failure = Some((generation, message));
                        }
                    }
                    NativeSessionEvent::EditorImageReady { generation, image } => {
                        if self.state.generation() == Some(generation)
                            && matches!(
                                self.state,
                                ScreenDrawState::Finish { .. }
                                    | ScreenDrawState::DisplayChanged { .. }
                            )
                        {
                            if self.pending_editor_handoff.is_none() {
                                self.pending_editor_handoff =
                                    Some(ScreenDrawEditorHandoff { generation, image });
                                worker.request_shutdown();
                            }
                        }
                    }
                    NativeSessionEvent::DocumentChanged => {}
                }
            }
            let _ = worker.poll_finished();
        }
        if emergency_paused {
            self.reconcile_emergency_pause();
        }
        if new_capture_teardown_complete && terminal_error.is_none() {
            let pending = self
                .pending_new_capture
                .take()
                .expect("teardown completion requires a pending new capture");
            self.native_worker.take();
            self.recovery_bridge.clear_emergency_handle();
            self.session_snapshot = None;
            self.runtime_state = None;
            if pending.timed_out {
                self.toolbar_open = true;
                self.set_state(ScreenDrawState::Failed {
                    generation: pending.generation,
                    message: "Screen Draw native session closed after the new capture timeout"
                        .to_string(),
                });
            } else {
                self.activate_capture(pending.generation);
            }
            request_repaint();
            return Some(ScreenDrawCapturePoll::default());
        }
        if editor_teardown_complete {
            let handoff = self
                .pending_editor_handoff
                .take()
                .expect("teardown completion requires a pending editor handoff");
            self.finalize_editor_handoff();
            return Some(ScreenDrawCapturePoll {
                restore_launcher: true,
                editor_handoff: Some(handoff),
                ..ScreenDrawCapturePoll::default()
            });
        }
        if let Some((generation, message)) = region_failure {
            self.restore_region_finish(generation, Some(message.clone()));
            return Some(ScreenDrawCapturePoll {
                diagnostic: Some(message),
                ..ScreenDrawCapturePoll::default()
            });
        }
        if let Some(ready) = region_picker_ready {
            return Some(ScreenDrawCapturePoll {
                region_picker_ready: Some(ready),
                ..ScreenDrawCapturePoll::default()
            });
        }
        if let Some((generation, outcome)) = export_completed {
            self.latest_export_outcome = Some(outcome);
            let _ = self.complete_session(generation);
            return Some(ScreenDrawCapturePoll {
                session_completed: true,
                ..ScreenDrawCapturePoll::default()
            });
        }
        if let Some(message) = export_failure {
            self.export_in_flight = false;
            self.toolbar_open = true;
            tracing::error!(error = %message, "Screen Draw export failed");
            self.latest_runtime_error = Some(message.clone());
            return Some(ScreenDrawCapturePoll {
                diagnostic: Some(message),
                ..ScreenDrawCapturePoll::default()
            });
        }
        if terminal_error.is_none()
            && let Some(pending) = self.pending_new_capture.as_mut()
        {
            if pending.timed_out {
                return Some(ScreenDrawCapturePoll::default());
            }
            let remaining = pending.deadline.saturating_duration_since(self.clock.now());
            if remaining.is_zero() {
                let message =
                    "Screen Draw native session did not close before the new capture timeout"
                        .to_string();
                pending.timed_out = true;
                self.latest_runtime_error = Some(message.clone());
                self.toolbar_open = true;
                return Some(ScreenDrawCapturePoll {
                    restore_launcher: true,
                    diagnostic: Some(message),
                    ..ScreenDrawCapturePoll::default()
                });
            }
            return Some(ScreenDrawCapturePoll {
                repoll_after: Some(remaining),
                ..ScreenDrawCapturePoll::default()
            });
        }
        terminal_error.map(|message| {
            let generation = self
                .state
                .generation()
                .unwrap_or(ScreenDrawGeneration(self.next_generation));
            self.latest_runtime_error = Some(message.clone());
            self.teardown_native_worker();
            self.session_snapshot = None;
            self.runtime_state = None;
            self.export_in_flight = false;
            self.pending_region = None;
            self.region_suppression = None;
            self.pending_editor_handoff = None;
            self.pending_new_capture = None;
            self.toolbar_open = false;
            self.set_state(ScreenDrawState::Failed {
                generation,
                message: message.clone(),
            });
            ScreenDrawCapturePoll {
                restore_launcher: true,
                diagnostic: Some(message),
                ..ScreenDrawCapturePoll::default()
            }
        })
    }

    fn dispatch_pending_region_preview(&mut self) -> Option<ScreenDrawCapturePoll> {
        let pending = self.pending_region.as_ref().copied()?;
        if pending.preview_dispatched
            || self.state.generation() != Some(pending.generation)
            || !matches!(self.state, ScreenDrawState::SelectingRegion { .. })
        {
            return None;
        }
        if let Err(error) = self.send_native(NativeSessionCommand::PrepareRegionSelection {
            generation: pending.generation,
            background: pending.background,
        }) {
            let message = error.to_string();
            self.restore_region_finish(pending.generation, Some(message.clone()));
            return Some(ScreenDrawCapturePoll {
                diagnostic: Some(message),
                ..ScreenDrawCapturePoll::default()
            });
        }
        if let Some(pending) = self.pending_region.as_mut() {
            pending.preview_dispatched = true;
        }
        Some(ScreenDrawCapturePoll::default())
    }

    fn teardown_native_worker(&mut self) {
        if let Some(worker) = self.native_worker.take() {
            self.recovery_bridge.clear_emergency_handle();
            worker.request_shutdown();
        }
    }

    fn restore_region_finish(
        &mut self,
        generation: ScreenDrawGeneration,
        diagnostic: Option<String>,
    ) {
        self.pending_region = None;
        self.region_suppression = None;
        self.export_in_flight = false;
        self.toolbar_open = true;
        if let Some(message) = diagnostic {
            self.latest_runtime_error = Some(message);
        }
        if self.state.generation() == Some(generation) {
            self.set_state(ScreenDrawState::Finish { generation });
        }
    }

    fn finalize_editor_handoff(&mut self) {
        self.cancel_pending_capture();
        self.native_worker.take();
        self.recovery_bridge.clear_emergency_handle();
        self.session_snapshot = None;
        self.runtime_state = None;
        self.pending_region = None;
        self.region_suppression = None;
        self.pending_editor_handoff = None;
        self.pending_new_capture = None;
        self.export_in_flight = false;
        self.toolbar_open = false;
        self.set_state(ScreenDrawState::NoSession);
    }

    fn send_native(&self, command: NativeSessionCommand) -> Result<(), ScreenDrawTransitionError> {
        if let Some(worker) = self.native_worker.as_ref() {
            worker
                .send(command)
                .map_err(|_| self.invalid("communicate with the native session"))?;
        }
        Ok(())
    }

    fn require_active_session(
        &self,
        operation: &'static str,
    ) -> Result<ScreenDrawGeneration, ScreenDrawTransitionError> {
        self.require_generation(operation, |state| {
            matches!(
                state,
                ScreenDrawState::Drawing { .. }
                    | ScreenDrawState::Ghost { .. }
                    | ScreenDrawState::Finish { .. }
            )
        })
    }

    fn transition_generation(
        &mut self,
        generation: ScreenDrawGeneration,
        operation: &'static str,
        valid_state: impl FnOnce(&ScreenDrawState) -> bool,
        next: ScreenDrawState,
    ) -> Result<(), ScreenDrawTransitionError> {
        if self.state.generation() != Some(generation) || !valid_state(&self.state) {
            return Err(self.invalid(operation));
        }
        self.set_state(next);
        Ok(())
    }

    fn require_generation(
        &self,
        operation: &'static str,
        valid_state: impl FnOnce(&ScreenDrawState) -> bool,
    ) -> Result<ScreenDrawGeneration, ScreenDrawTransitionError> {
        if valid_state(&self.state) {
            self.state
                .generation()
                .ok_or_else(|| self.invalid(operation))
        } else {
            Err(self.invalid(operation))
        }
    }

    fn invalid(&self, operation: &'static str) -> ScreenDrawTransitionError {
        ScreenDrawTransitionError {
            operation,
            state: self.state.clone(),
        }
    }
}

impl Drop for ScreenDrawController {
    fn drop(&mut self) {
        self.cancel_pending_capture();
        self.teardown_native_worker();
    }
}

#[cfg(test)]
struct TestNativeSessionFactory;

#[cfg(test)]
impl NativeSessionFactory for TestNativeSessionFactory {
    fn spawn(&self, _config: NativeSessionConfig) -> Result<NativeSessionHandle, String> {
        Ok(NativeSessionHandle::test_stub().0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::screen::ScreenRect;
    use image::{Rgba, RgbaImage};
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;

    struct FakeClock {
        now: Mutex<Instant>,
    }

    impl FakeClock {
        fn new(now: Instant) -> Self {
            Self {
                now: Mutex::new(now),
            }
        }

        fn advance(&self, duration: Duration) {
            let mut now = self.now.lock().unwrap();
            *now += duration;
        }
    }

    impl MonotonicClock for FakeClock {
        fn now(&self) -> Instant {
            *self.now.lock().unwrap()
        }
    }

    #[derive(Default)]
    struct CountingNativeFactory {
        spawns: AtomicUsize,
        commands: Mutex<Vec<mpsc::Receiver<NativeSessionCommand>>>,
        events: Mutex<Vec<mpsc::Sender<NativeSessionEvent>>>,
    }

    impl NativeSessionFactory for CountingNativeFactory {
        fn spawn(&self, config: NativeSessionConfig) -> Result<NativeSessionHandle, String> {
            assert_eq!(config.snapshot.capture().origin, (-2, -1));
            self.spawns.fetch_add(1, Ordering::SeqCst);
            let (handle, commands, events) = NativeSessionHandle::test_stub_with_events();
            self.commands.lock().unwrap().push(commands);
            self.events.lock().unwrap().push(events);
            Ok(handle)
        }
    }

    struct FailingNativeFactory;

    impl NativeSessionFactory for FailingNativeFactory {
        fn spawn(&self, _config: NativeSessionConfig) -> Result<NativeSessionHandle, String> {
            Err("fixture native startup failed".into())
        }
    }

    struct FakeCaptureBackend {
        desktop_calls: AtomicUsize,
        calls: AtomicUsize,
        fail: bool,
        block_first_until_cancelled: bool,
    }

    impl FakeCaptureBackend {
        fn successful() -> Self {
            Self {
                desktop_calls: AtomicUsize::new(0),
                calls: AtomicUsize::new(0),
                fail: false,
                block_first_until_cancelled: false,
            }
        }
    }

    impl DesktopCaptureBackend for FakeCaptureBackend {
        fn virtual_desktop(&self) -> Result<ScreenRect, String> {
            self.desktop_calls.fetch_add(1, Ordering::SeqCst);
            Ok(ScreenRect::new(-2, -1, 4, 3))
        }

        fn capture_desktop(&self, cancelled: &dyn Fn() -> bool) -> Result<CapturedRegion, String> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if self.block_first_until_cancelled && call == 0 {
                while !cancelled() {
                    std::thread::yield_now();
                }
                return Err("cancelled replacement capture".into());
            }
            if self.fail {
                return Err("fixture capture failed".into());
            }
            let color = if call == 0 { 10 } else { 20 };
            Ok(CapturedRegion {
                image: RgbaImage::from_pixel(4, 3, Rgba([color, 2, 3, 255])),
                origin: (-2, -1),
            })
        }
    }

    struct FakeVisibilityProbe {
        results: Mutex<VecDeque<Result<bool, String>>>,
    }

    impl FakeVisibilityProbe {
        fn new(results: impl IntoIterator<Item = Result<bool, String>>) -> Self {
            Self {
                results: Mutex::new(results.into_iter().collect()),
            }
        }
    }

    impl LauncherVisibilityProbe for FakeVisibilityProbe {
        fn launcher_is_capture_parked(
            &self,
            _launcher_hwnd: Option<usize>,
            _virtual_desktop: ScreenRect,
        ) -> Result<bool, String> {
            self.results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(false))
        }
    }

    fn capture_controller(
        backend: Arc<FakeCaptureBackend>,
        visibility: impl IntoIterator<Item = Result<bool, String>>,
    ) -> ScreenDrawController {
        ScreenDrawController::with_capture_dependencies(
            backend,
            Arc::new(FakeVisibilityProbe::new(visibility)),
        )
    }

    fn capture_controller_with_native(
        backend: Arc<FakeCaptureBackend>,
        visibility: impl IntoIterator<Item = Result<bool, String>>,
        native_factory: Arc<dyn NativeSessionFactory>,
    ) -> ScreenDrawController {
        ScreenDrawController::with_runtime_dependencies(
            backend,
            Arc::new(FakeVisibilityProbe::new(visibility)),
            native_factory,
        )
    }

    fn capture_controller_with_clock(
        backend: Arc<FakeCaptureBackend>,
        visibility: impl IntoIterator<Item = Result<bool, String>>,
        native_factory: Arc<dyn NativeSessionFactory>,
        clock: Arc<dyn MonotonicClock>,
    ) -> ScreenDrawController {
        ScreenDrawController::with_runtime_dependencies_and_clock(
            backend,
            Arc::new(FakeVisibilityProbe::new(visibility)),
            native_factory,
            clock,
        )
    }

    fn repaint_counter() -> (Arc<AtomicUsize>, Arc<dyn Fn() + Send + Sync>) {
        let count = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&count);
        let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            observed.fetch_add(1, Ordering::SeqCst);
        });
        (count, callback)
    }

    fn poll_until(
        controller: &mut ScreenDrawController,
        repaint: &Arc<dyn Fn() + Send + Sync>,
        predicate: impl Fn(&ScreenDrawCapturePoll) -> bool,
    ) -> ScreenDrawCapturePoll {
        for _ in 0..10_000 {
            let poll = controller.poll_capture(Some(1), Arc::clone(repaint));
            if predicate(&poll) {
                return poll;
            }
            std::thread::yield_now();
        }
        panic!("capture coordinator did not reach expected result")
    }

    fn drawing_controller() -> (ScreenDrawController, ScreenDrawGeneration) {
        let mut controller = ScreenDrawController::default();
        let generation = controller.request_start().unwrap();
        controller.launcher_parked(generation).unwrap();
        controller.capture_succeeded(generation).unwrap();
        (controller, generation)
    }

    #[test]
    fn start_capture_and_toolbar_order_are_explicit() {
        let mut controller = ScreenDrawController::default();
        let generation = controller.request_start().unwrap();
        assert_eq!(
            controller.state(),
            &ScreenDrawState::AwaitingLauncherParking { generation }
        );
        assert!(!controller.toolbar_open());
        controller.launcher_parked(generation).unwrap();
        assert_eq!(
            controller.state(),
            &ScreenDrawState::Capturing { generation }
        );
        controller.capture_succeeded(generation).unwrap();
        assert_eq!(controller.state(), &ScreenDrawState::Drawing { generation });
        assert!(controller.toolbar_open());
    }

    #[test]
    fn drawing_ghost_resume_finish_and_region_transitions_preserve_generation() {
        let (mut controller, generation) = drawing_controller();
        controller.enter_ghost().unwrap();
        assert_eq!(controller.state(), &ScreenDrawState::Ghost { generation });
        controller.resume_drawing().unwrap();
        controller.finish().unwrap();
        controller
            .begin_region_selection(ExportBackground::Transparent, ExportDestination::Clipboard)
            .unwrap();
        assert_eq!(
            controller.state(),
            &ScreenDrawState::SelectingRegion { generation }
        );
        controller
            .cancel_region_selection(generation, None)
            .unwrap();
        assert_eq!(controller.state(), &ScreenDrawState::Finish { generation });
        controller.complete_session(generation).unwrap();
        assert_eq!(controller.state(), &ScreenDrawState::NoSession);
        assert!(controller.toolbar_open());
    }

    #[test]
    fn stale_and_invalid_transitions_do_not_mutate_state() {
        let (mut controller, generation) = drawing_controller();
        let before = controller.state().clone();
        assert!(
            controller
                .capture_failed(ScreenDrawGeneration(generation.get() + 1), "late")
                .is_err()
        );
        assert!(controller.request_start().is_err());
        assert!(controller.complete_session(generation).is_err());
        assert_eq!(controller.state(), &before);
    }

    #[test]
    fn capture_failure_is_recoverable_and_new_start_gets_new_generation() {
        let mut controller = ScreenDrawController::default();
        let first = controller.request_start().unwrap();
        controller.launcher_parked(first).unwrap();
        controller.capture_failed(first, "capture failed").unwrap();
        assert!(
            matches!(controller.state(), ScreenDrawState::Failed { message, .. } if message == "capture failed")
        );
        let second = controller.request_start().unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn display_change_disarms_logical_session_and_close_is_idempotent() {
        let (mut controller, generation) = drawing_controller();
        controller.note_display_changed(generation).unwrap();
        assert_eq!(
            controller.state(),
            &ScreenDrawState::DisplayChanged { generation }
        );
        assert!(controller.resume_drawing().is_err());
        controller.close();
        controller.close();
        assert_eq!(controller.state(), &ScreenDrawState::NoSession);
        assert!(!controller.toolbar_open());
    }

    #[test]
    fn clear_requires_an_active_safe_session() {
        let mut controller = ScreenDrawController::default();
        assert!(controller.request_clear().is_err());
        let (mut controller, _) = drawing_controller();
        controller.request_clear().unwrap();
        assert_eq!(controller.clear_request_count(), 1);
    }

    #[test]
    fn toolbar_visibility_is_orthogonal_to_session_state() {
        let mut controller = ScreenDrawController::default();
        controller.open_toolbar();
        assert_eq!(controller.state(), &ScreenDrawState::NoSession);
        assert!(controller.toolbar_open());
    }

    #[test]
    fn coordinator_requests_typed_parking_then_verifies_before_capture() {
        let backend = Arc::new(FakeCaptureBackend::successful());
        let mut controller = capture_controller(Arc::clone(&backend), [Ok(false), Ok(true)]);
        let generation = controller.request_start().unwrap();
        let (repaints, repaint) = repaint_counter();

        let parking = controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert_eq!(
            parking.park_launcher,
            Some(ScreenDrawParkingRequest {
                generation,
                virtual_desktop: ScreenRect::new(-2, -1, 4, 3),
            })
        );
        assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
        let waiting = controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert_eq!(waiting.repoll_after, Some(LAUNCHER_PARKING_REPOLL_INTERVAL));
        assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
        let started = controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert!(started.capture_started);
        assert_eq!(backend.desktop_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            controller.state(),
            &ScreenDrawState::Capturing { generation }
        );

        let completed = poll_until(&mut controller, &repaint, |poll| poll.capture_completed);
        assert!(completed.capture_completed);
        assert_eq!(controller.state(), &ScreenDrawState::Drawing { generation });
        assert!(controller.toolbar_open());
        let snapshot = controller.session_snapshot().unwrap();
        assert_eq!(snapshot.generation(), generation);
        assert_eq!(snapshot.capture().origin, (-2, -1));
        assert!(repaints.load(Ordering::SeqCst) >= 2);
    }

    #[test]
    fn native_worker_starts_only_after_capture_and_close_signals_shutdown() {
        let backend = Arc::new(FakeCaptureBackend::successful());
        let native = Arc::new(CountingNativeFactory::default());
        let native_dependency: Arc<dyn NativeSessionFactory> = native.clone();
        let mut controller = capture_controller_with_native(backend, [Ok(true)], native_dependency);
        let recovery = Arc::new(ScreenDrawRecoveryBridge::default());
        controller.set_recovery_bridge(Arc::clone(&recovery));
        let (_, repaint) = repaint_counter();
        controller.request_start().unwrap();
        assert!(recovery.is_active());
        assert!(!recovery.has_emergency_handle());
        assert_eq!(native.spawns.load(Ordering::SeqCst), 0);
        let parking = controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert!(parking.park_launcher.is_some());
        assert!(
            controller
                .poll_capture(Some(1), Arc::clone(&repaint))
                .capture_started
        );
        assert_eq!(native.spawns.load(Ordering::SeqCst), 0);
        let completed = poll_until(&mut controller, &repaint, |poll| poll.capture_completed);
        assert!(completed.capture_completed);
        assert_eq!(native.spawns.load(Ordering::SeqCst), 1);
        assert!(recovery.has_emergency_handle());

        controller.close();
        assert!(!recovery.is_active());
        assert!(!recovery.has_emergency_handle());
        let commands = native.commands.lock().unwrap();
        assert!(matches!(
            commands[0].try_recv(),
            Ok(NativeSessionCommand::Shutdown)
        ));
    }

    #[test]
    fn terminal_native_error_fails_session_and_restores_launcher() {
        let backend = Arc::new(FakeCaptureBackend::successful());
        let native = Arc::new(CountingNativeFactory::default());
        let native_dependency: Arc<dyn NativeSessionFactory> = native.clone();
        let mut controller = capture_controller_with_native(backend, [Ok(true)], native_dependency);
        let (_, repaint) = repaint_counter();
        controller.request_start().unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        poll_until(&mut controller, &repaint, |poll| poll.capture_completed);

        native.events.lock().unwrap()[0]
            .send(NativeSessionEvent::Error("fixture native failure".into()))
            .unwrap();
        let failure = controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert!(failure.restore_launcher);
        assert_eq!(
            failure.diagnostic.as_deref(),
            Some("fixture native failure")
        );
        assert_eq!(
            controller.latest_runtime_error(),
            Some("fixture native failure")
        );
        assert!(matches!(controller.state(), ScreenDrawState::Failed { .. }));
        assert!(controller.session_snapshot().is_none());
    }

    #[test]
    fn native_safe_pause_and_display_change_update_controller_without_losing_session() {
        let backend = Arc::new(FakeCaptureBackend::successful());
        let native = Arc::new(CountingNativeFactory::default());
        let native_dependency: Arc<dyn NativeSessionFactory> = native.clone();
        let mut controller = capture_controller_with_native(backend, [Ok(true)], native_dependency);
        let (_, repaint) = repaint_counter();
        let generation = controller.request_start().unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        poll_until(&mut controller, &repaint, |poll| poll.capture_completed);

        let event = native.events.lock().unwrap()[0].clone();
        event
            .send(NativeSessionEvent::ModeChanged(
                super::super::ScreenDrawMode::Ghost,
            ))
            .unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert_eq!(controller.state(), &ScreenDrawState::Ghost { generation });

        event.send(NativeSessionEvent::DisplayChanged).unwrap();
        event
            .send(NativeSessionEvent::Warning(
                "display changed fixture".into(),
            ))
            .unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert_eq!(
            controller.state(),
            &ScreenDrawState::DisplayChanged { generation }
        );
        assert!(controller.session_snapshot().is_some());
        assert!(controller.toolbar_open());
        assert_eq!(
            controller.latest_runtime_warning(),
            Some("display changed fixture")
        );
        assert!(controller.resume_drawing().is_err());
    }

    #[test]
    fn export_failure_preserves_finish_retry_and_stale_completion_is_ignored() {
        let backend = Arc::new(FakeCaptureBackend::successful());
        let observed_backend = Arc::clone(&backend);
        let native = Arc::new(CountingNativeFactory::default());
        let native_dependency: Arc<dyn NativeSessionFactory> = native.clone();
        let mut controller = capture_controller_with_native(backend, [Ok(true)], native_dependency);
        let (_, repaint) = repaint_counter();
        let generation = controller.request_start().unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        poll_until(&mut controller, &repaint, |poll| poll.capture_completed);
        controller.finish().unwrap();
        let request = ExportRequest {
            scope: super::super::ExportScope::FullDesktop,
            background: super::super::ExportBackground::Transparent,
            destination: super::super::ExportDestination::Clipboard,
        };
        controller.request_export(request).unwrap();
        assert!(controller.export_in_flight());
        assert!(controller.request_export(request).is_err());
        let queued: Vec<_> = native.commands.lock().unwrap()[0].try_iter().collect();
        assert_eq!(
            queued
                .iter()
                .filter(|command| matches!(command, NativeSessionCommand::RenderExport(_)))
                .count(),
            1
        );
        assert!(matches!(
            queued.last(),
            Some(NativeSessionCommand::RenderExport(ExportRenderRequest { generation: sent, request: actual }))
                if *sent == generation && *actual == request
        ));

        let event = native.events.lock().unwrap()[0].clone();
        event
            .send(NativeSessionEvent::ExportCompleted {
                generation: ScreenDrawGeneration::from_raw(generation.get() + 1),
                outcome: ExportOutcome::Clipboard,
            })
            .unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert_eq!(controller.state(), &ScreenDrawState::Finish { generation });
        assert!(controller.export_in_flight());

        event
            .send(NativeSessionEvent::ExportFailed {
                generation,
                message: "fixture clipboard failure".into(),
            })
            .unwrap();
        let failure = controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert_eq!(
            failure.diagnostic.as_deref(),
            Some("fixture clipboard failure")
        );
        assert_eq!(controller.state(), &ScreenDrawState::Finish { generation });
        assert!(controller.session_snapshot().is_some());
        assert!(controller.toolbar_open());
        assert!(!controller.export_in_flight());

        controller.request_export(request).unwrap();
        assert!(controller.export_in_flight());
        event
            .send(NativeSessionEvent::ExportCompleted {
                generation,
                outcome: ExportOutcome::Clipboard,
            })
            .unwrap();
        let completion = controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert!(completion.session_completed);
        assert_eq!(controller.state(), &ScreenDrawState::NoSession);
        assert!(controller.session_snapshot().is_none());
        assert!(controller.runtime_state().is_none());
        assert!(controller.toolbar_open());
        assert!(!controller.export_in_flight());
        assert_eq!(
            controller.latest_export_outcome(),
            Some(&ExportOutcome::Clipboard)
        );
        assert_eq!(observed_backend.calls.load(Ordering::SeqCst), 1);
        assert!(matches!(
            native.commands.lock().unwrap()[0].try_iter().last(),
            Some(NativeSessionCommand::Shutdown)
        ));
    }

    #[test]
    fn region_selection_orders_suppression_toolbar_preview_and_picker_readiness() {
        let backend = Arc::new(FakeCaptureBackend::successful());
        let native = Arc::new(CountingNativeFactory::default());
        let mut controller = capture_controller_with_native(backend, [Ok(true)], native.clone());
        let (_, repaint) = repaint_counter();
        let generation = controller.request_start().unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        poll_until(&mut controller, &repaint, |poll| poll.capture_completed);
        controller.finish().unwrap();
        let _ = native.commands.lock().unwrap()[0]
            .try_iter()
            .collect::<Vec<_>>();

        controller
            .begin_region_selection(ExportBackground::Black, ExportDestination::File)
            .unwrap();
        assert!(controller.region_suppression.is_some());
        assert!(!controller.toolbar_open());
        assert!(controller.export_in_flight());
        assert!(native.commands.lock().unwrap()[0].try_recv().is_err());
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert!(matches!(
            native.commands.lock().unwrap()[0].try_recv().unwrap(),
            NativeSessionCommand::PrepareRegionSelection {
                generation: sent,
                background: ExportBackground::Black,
            } if sent == generation
        ));

        native.events.lock().unwrap()[0]
            .send(NativeSessionEvent::RegionPreviewReady {
                generation,
                bounds: ScreenRect::new(-1920, -200, 3840, 1200),
            })
            .unwrap();
        let poll = controller.poll_capture(Some(1), repaint);
        assert_eq!(
            poll.region_picker_ready,
            Some(ScreenDrawRegionPickerReady {
                generation,
                bounds: ScreenRect::new(-1920, -200, 3840, 1200),
            })
        );
        controller
            .cancel_region_selection(generation, None)
            .unwrap();
        assert!(controller.region_suppression.is_none());
        assert!(controller.toolbar_open());
        assert!(!controller.export_in_flight());
        assert_eq!(controller.state(), &ScreenDrawState::Finish { generation });
        assert!(controller.session_snapshot().is_some());
        assert!(matches!(
            native.commands.lock().unwrap()[0].try_recv().unwrap(),
            NativeSessionCommand::EndRegionSelection
        ));
    }

    #[test]
    fn region_confirmation_queues_preview_teardown_before_signed_export_and_failure_retries() {
        let backend = Arc::new(FakeCaptureBackend::successful());
        let native = Arc::new(CountingNativeFactory::default());
        let mut controller = capture_controller_with_native(backend, [Ok(true)], native.clone());
        let (_, repaint) = repaint_counter();
        let generation = controller.request_start().unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        poll_until(&mut controller, &repaint, |poll| poll.capture_completed);
        controller.finish().unwrap();
        controller
            .begin_region_selection(
                ExportBackground::FrozenDesktop,
                ExportDestination::Clipboard,
            )
            .unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        let _ = native.commands.lock().unwrap()[0]
            .try_iter()
            .collect::<Vec<_>>();
        let rect = ScreenRect::new(-2, -1, 3, 2);
        controller
            .complete_region_selection(generation, rect)
            .unwrap();
        let queued = native.commands.lock().unwrap()[0]
            .try_iter()
            .collect::<Vec<_>>();
        assert!(matches!(queued.as_slice(), [
            NativeSessionCommand::EndRegionSelection,
            NativeSessionCommand::RenderExport(ExportRenderRequest { generation: sent, request })
        ] if *sent == generation && request.scope == ExportScope::Region(rect)));
        assert!(controller.region_suppression.is_none());
        assert!(controller.toolbar_open());

        native.events.lock().unwrap()[0]
            .send(NativeSessionEvent::ExportFailed {
                generation,
                message: "fixture region crop failure".into(),
            })
            .unwrap();
        let failure = controller.poll_capture(Some(1), repaint);
        assert_eq!(
            failure.diagnostic.as_deref(),
            Some("fixture region crop failure")
        );
        assert_eq!(controller.state(), &ScreenDrawState::Finish { generation });
        assert!(controller.toolbar_open());
        assert!(controller.session_snapshot().is_some());
    }

    #[test]
    fn successful_region_copy_and_save_complete_with_idle_toolbar_open() {
        for (destination, outcome) in [
            (ExportDestination::Clipboard, ExportOutcome::Clipboard),
            (
                ExportDestination::File,
                ExportOutcome::File(std::path::PathBuf::from("fixture-region.png")),
            ),
        ] {
            let backend = Arc::new(FakeCaptureBackend::successful());
            let native = Arc::new(CountingNativeFactory::default());
            let mut controller =
                capture_controller_with_native(backend, [Ok(true)], native.clone());
            let (_, repaint) = repaint_counter();
            let generation = controller.request_start().unwrap();
            controller.poll_capture(Some(1), Arc::clone(&repaint));
            controller.poll_capture(Some(1), Arc::clone(&repaint));
            poll_until(&mut controller, &repaint, |poll| poll.capture_completed);
            controller.finish().unwrap();
            controller
                .begin_region_selection(ExportBackground::White, destination)
                .unwrap();
            controller.poll_capture(Some(1), Arc::clone(&repaint));
            controller
                .complete_region_selection(generation, ScreenRect::new(-2, -1, 2, 2))
                .unwrap();
            assert_eq!(controller.state(), &ScreenDrawState::Finish { generation });
            assert!(controller.export_in_flight());
            assert!(controller.toolbar_open());

            native.events.lock().unwrap()[0]
                .send(NativeSessionEvent::ExportCompleted {
                    generation,
                    outcome: outcome.clone(),
                })
                .unwrap();
            let completion = controller.poll_capture(Some(1), Arc::clone(&repaint));
            assert!(completion.session_completed);
            assert_eq!(controller.state(), &ScreenDrawState::NoSession);
            assert!(!controller.export_in_flight());
            assert!(controller.toolbar_open());
            assert_eq!(controller.latest_export_outcome(), Some(&outcome));
        }
    }

    #[test]
    fn replacing_a_region_selection_releases_its_guard_and_ignores_late_ready_events() {
        let backend = Arc::new(FakeCaptureBackend::successful());
        let observed_backend = Arc::clone(&backend);
        let native = Arc::new(CountingNativeFactory::default());
        let mut controller = capture_controller_with_native(backend, [Ok(true)], native.clone());
        let (_, repaint) = repaint_counter();
        let first = controller.request_start().unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        poll_until(&mut controller, &repaint, |poll| poll.capture_completed);
        controller.finish().unwrap();
        controller
            .begin_region_selection(ExportBackground::White, ExportDestination::File)
            .unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert!(controller.region_suppression.is_some());

        let second = controller.request_new_capture().unwrap();
        assert_ne!(first, second);
        assert!(controller.region_suppression.is_none());
        assert!(controller.pending_region.is_none());
        assert!(!controller.export_in_flight());
        assert_eq!(
            controller.state(),
            &ScreenDrawState::AwaitingNativeTeardown { generation: second }
        );
        assert_eq!(observed_backend.calls.load(Ordering::SeqCst), 1);
        assert!(matches!(
            native.commands.lock().unwrap()[0].try_iter().last(),
            Some(NativeSessionCommand::Shutdown)
        ));
        native.events.lock().unwrap()[0]
            .send(NativeSessionEvent::SessionClosed)
            .unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert_eq!(
            controller.state(),
            &ScreenDrawState::AwaitingLauncherParking { generation: second }
        );
        assert!(
            native.events.lock().unwrap()[0]
                .send(NativeSessionEvent::RegionPreviewReady {
                    generation: first,
                    bounds: ScreenRect::new(-2, -1, 4, 3),
                })
                .is_err()
        );
    }

    #[test]
    fn new_capture_timeout_fails_safe_without_capturing_under_the_old_surface() {
        let backend = Arc::new(FakeCaptureBackend::successful());
        let observed_backend = Arc::clone(&backend);
        let native = Arc::new(CountingNativeFactory::default());
        let clock = Arc::new(FakeClock::new(Instant::now()));
        let mut controller =
            capture_controller_with_clock(backend, [Ok(true)], native.clone(), clock.clone());
        let (_, repaint) = repaint_counter();
        controller.request_start().unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        poll_until(&mut controller, &repaint, |poll| poll.capture_completed);

        let generation = controller.request_new_capture().unwrap();
        let waiting = controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert!(waiting.repoll_after.is_some_and(|delay| !delay.is_zero()));
        assert_eq!(observed_backend.calls.load(Ordering::SeqCst), 1);
        clock.advance(NATIVE_TEARDOWN_TIMEOUT);
        let terminal = controller.poll_capture(Some(1), Arc::clone(&repaint));

        assert!(terminal.restore_launcher);
        assert!(
            terminal
                .diagnostic
                .as_deref()
                .is_some_and(|message| message.contains("did not close"))
        );
        assert_eq!(
            controller.state(),
            &ScreenDrawState::AwaitingNativeTeardown { generation }
        );
        assert_eq!(observed_backend.calls.load(Ordering::SeqCst), 1);
        assert!(controller.session_snapshot().is_some());
        assert!(controller.native_worker.is_some());
        assert!(controller.toolbar_open());
        native.events.lock().unwrap()[0]
            .send(NativeSessionEvent::SessionClosed)
            .unwrap();
        controller.poll_capture(Some(1), repaint);
        assert!(matches!(
            controller.state(),
            ScreenDrawState::Failed { generation: failed, .. } if *failed == generation
        ));
        assert!(controller.native_worker.is_none());
    }

    #[test]
    fn recovery_during_native_teardown_quarantines_late_session_closed() {
        let backend = Arc::new(FakeCaptureBackend::successful());
        let observed_backend = Arc::clone(&backend);
        let native = Arc::new(CountingNativeFactory::default());
        let mut controller = capture_controller_with_native(backend, [Ok(true)], native.clone());
        let recovery = Arc::new(ScreenDrawRecoveryBridge::default());
        controller.set_recovery_bridge(Arc::clone(&recovery));
        let (_, repaint) = repaint_counter();
        controller.request_start().unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        poll_until(&mut controller, &repaint, |poll| poll.capture_completed);
        let late_events = native.events.lock().unwrap()[0].clone();

        let replacement = controller.request_new_capture().unwrap();
        assert_eq!(
            controller.state(),
            &ScreenDrawState::AwaitingNativeTeardown {
                generation: replacement
            }
        );
        controller.close();
        assert!(!recovery.is_active());
        assert!(!recovery.has_emergency_handle());

        let _ = late_events.send(NativeSessionEvent::SessionClosed);
        controller.poll_capture(Some(1), repaint);
        assert_eq!(controller.state(), &ScreenDrawState::NoSession);
        assert_eq!(observed_backend.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn emergency_during_region_selection_aborts_to_ghost_and_quarantines_picker_cancel() {
        use crate::gui::mkmacro_dialog::visual_capture_workflow::SharedVisualOverlayController;
        use crate::gui::mkmacro_dialog::visual_overlay::RectanglePurpose;

        let backend = Arc::new(FakeCaptureBackend::successful());
        let native = Arc::new(CountingNativeFactory::default());
        let mut controller = capture_controller_with_native(backend, [Ok(true)], native.clone());
        let (_, repaint) = repaint_counter();
        let generation = controller.request_start().unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        poll_until(&mut controller, &repaint, |poll| poll.capture_completed);
        controller.finish().unwrap();
        controller
            .begin_region_selection(ExportBackground::White, ExportDestination::Clipboard)
            .unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));

        let fixture = SharedVisualOverlayController::test_fixture();
        let operation_id = fixture.controller.begin_rectangle_pick(
            RectanglePurpose::ScreenDrawExport,
            ScreenRect::new(-2, -1, 4, 3),
        );
        fixture.observer.wait_for_commands(1);
        native.events.lock().unwrap()[0]
            .send(NativeSessionEvent::EmergencyPaused)
            .unwrap();
        native.events.lock().unwrap()[0]
            .send(NativeSessionEvent::EmergencyPaused)
            .unwrap();
        controller.poll_capture(Some(1), repaint);

        assert_eq!(controller.state(), &ScreenDrawState::Ghost { generation });
        assert!(controller.pending_region.is_none());
        assert!(controller.region_suppression.is_none());
        assert!(!controller.export_in_flight());
        assert!(controller.toolbar_open());
        assert!(controller.session_snapshot().is_some());
        assert!(controller.native_worker.is_some());

        fixture
            .controller
            .cancel_screen_draw_operation(operation_id);
        fixture.observer.wait_for_commands(2);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while fixture
            .controller
            .screen_draw_discard_pending_for_test(operation_id)
        {
            assert!(
                fixture.controller.poll().is_empty(),
                "picker cancellation must not leak into macro-editor events"
            );
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(fixture.controller.poll().is_empty());
        assert!(
            fixture
                .controller
                .poll_rectangle_event(operation_id)
                .is_none()
        );
    }

    #[test]
    fn editor_payload_tears_down_session_and_restores_launcher_without_recapture() {
        let backend = Arc::new(FakeCaptureBackend::successful());
        let observed_backend = backend.clone();
        let native = Arc::new(CountingNativeFactory::default());
        let mut controller = capture_controller_with_native(backend, [Ok(true)], native.clone());
        let (_, repaint) = repaint_counter();
        let generation = controller.request_start().unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        poll_until(&mut controller, &repaint, |poll| poll.capture_completed);
        controller.finish().unwrap();
        controller
            .request_export(ExportRequest {
                scope: ExportScope::FullDesktop,
                background: ExportBackground::White,
                destination: ExportDestination::ScreenshotEditor,
            })
            .unwrap();
        let image = RgbaImage::from_pixel(3, 2, Rgba([7, 8, 9, 255]));
        native.events.lock().unwrap()[0]
            .send(NativeSessionEvent::EditorImageReady {
                generation,
                image: image.clone(),
            })
            .unwrap();
        let rendered = controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert!(!rendered.restore_launcher);
        assert!(rendered.editor_handoff.is_none());
        assert_eq!(controller.state(), &ScreenDrawState::Finish { generation });
        assert!(controller.session_snapshot().is_some());
        assert!(controller.native_worker.is_some());
        assert!(controller.pending_editor_handoff.is_some());
        assert!(controller.toolbar_open());
        assert!(matches!(
            native.commands.lock().unwrap()[0].try_iter().last(),
            Some(NativeSessionCommand::Shutdown)
        ));

        native.events.lock().unwrap()[0]
            .send(NativeSessionEvent::SessionClosed)
            .unwrap();
        let poll = controller.poll_capture(Some(1), repaint);
        assert!(poll.restore_launcher);
        assert_eq!(poll.editor_handoff.unwrap().image, image);
        assert_eq!(controller.state(), &ScreenDrawState::NoSession);
        assert!(controller.session_snapshot().is_none());
        assert!(controller.native_worker.is_none());
        assert!(!controller.toolbar_open());
        assert_eq!(observed_backend.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn display_changed_session_exports_retained_original_and_never_resumes() {
        let backend = Arc::new(FakeCaptureBackend::successful());
        let observed_backend = Arc::clone(&backend);
        let native = Arc::new(CountingNativeFactory::default());
        let native_dependency: Arc<dyn NativeSessionFactory> = native.clone();
        let mut controller = capture_controller_with_native(backend, [Ok(true)], native_dependency);
        let (_, repaint) = repaint_counter();
        let generation = controller.request_start().unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        poll_until(&mut controller, &repaint, |poll| poll.capture_completed);
        let event = native.events.lock().unwrap()[0].clone();
        event.send(NativeSessionEvent::DisplayChanged).unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert!(controller.resume_drawing().is_err());

        let request = ExportRequest {
            scope: super::super::ExportScope::FullDesktop,
            background: super::super::ExportBackground::FrozenDesktop,
            destination: super::super::ExportDestination::File,
        };
        controller.request_export(request).unwrap();
        assert!(controller.export_in_flight());
        assert!(matches!(
            native.commands.lock().unwrap()[0].try_iter().last(),
            Some(NativeSessionCommand::RenderExport(ExportRenderRequest { generation: sent, request: actual }))
                if sent == generation && actual == request
        ));
        event
            .send(NativeSessionEvent::ExportFailed {
                generation,
                message: "fixture save failure".into(),
            })
            .unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert_eq!(
            controller.state(),
            &ScreenDrawState::DisplayChanged { generation }
        );
        assert!(controller.session_snapshot().is_some());
        assert!(!controller.export_in_flight());

        controller.request_export(request).unwrap();
        event
            .send(NativeSessionEvent::ExportCompleted {
                generation,
                outcome: ExportOutcome::File(std::path::PathBuf::from("fixture.png")),
            })
            .unwrap();
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert_eq!(controller.state(), &ScreenDrawState::NoSession);
        assert!(controller.toolbar_open());
        assert_eq!(observed_backend.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn launcher_parking_timeout_uses_elapsed_time_and_restores_on_failure() {
        let backend = Arc::new(FakeCaptureBackend::successful());
        let clock = Arc::new(FakeClock::new(Instant::now()));
        let mut controller = capture_controller_with_clock(
            Arc::clone(&backend),
            [Ok(false), Ok(false)],
            Arc::new(TestNativeSessionFactory),
            clock.clone(),
        );
        let generation = controller.request_start().unwrap();
        let (_, repaint) = repaint_counter();
        assert!(
            controller
                .poll_capture(Some(1), Arc::clone(&repaint))
                .park_launcher
                .is_some()
        );
        let waiting = controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert_eq!(waiting.repoll_after, Some(LAUNCHER_PARKING_REPOLL_INTERVAL));
        clock.advance(LAUNCHER_PARKING_TIMEOUT);
        let failure = controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert!(failure.restore_launcher);
        assert!(failure.diagnostic.unwrap().contains("parking timeout"));
        assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
        assert_eq!(backend.desktop_calls.load(Ordering::SeqCst), 1);
        assert!(matches!(
            controller.state(),
            ScreenDrawState::Failed { generation: failed, .. } if *failed == generation
        ));
        assert!(controller.session_snapshot().is_none());
        assert!(!controller.toolbar_open());
    }

    #[test]
    fn capture_timeout_cancels_without_joining_and_allows_retry() {
        let backend = Arc::new(FakeCaptureBackend {
            desktop_calls: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
            fail: false,
            block_first_until_cancelled: true,
        });
        let clock = Arc::new(FakeClock::new(Instant::now()));
        let mut controller = capture_controller_with_clock(
            Arc::clone(&backend),
            [Ok(true), Ok(true)],
            Arc::new(TestNativeSessionFactory),
            clock.clone(),
        );
        let (_, repaint) = repaint_counter();
        let first = controller.request_start().unwrap();
        assert!(
            controller
                .poll_capture(Some(1), Arc::clone(&repaint))
                .park_launcher
                .is_some()
        );
        assert!(
            controller
                .poll_capture(Some(1), Arc::clone(&repaint))
                .capture_started
        );
        while backend.calls.load(Ordering::SeqCst) == 0 {
            std::thread::yield_now();
        }

        clock.advance(DESKTOP_CAPTURE_TIMEOUT);
        let timeout = controller.poll_capture(Some(1), Arc::clone(&repaint));
        assert!(timeout.restore_launcher);
        assert!(timeout.diagnostic.unwrap().contains("capture timeout"));
        assert!(matches!(
            controller.state(),
            ScreenDrawState::Failed { generation, .. } if *generation == first
        ));

        let second = controller.request_start().unwrap();
        assert_ne!(first, second);
        assert!(
            controller
                .poll_capture(Some(1), Arc::clone(&repaint))
                .park_launcher
                .is_some()
        );
        let completed = poll_until(&mut controller, &repaint, |poll| poll.capture_completed);
        assert!(completed.capture_completed);
        assert_eq!(
            controller.state(),
            &ScreenDrawState::Drawing { generation: second }
        );
    }

    #[test]
    fn cancelling_capture_preparation_uses_shared_cleanup_and_invalidates_generation() {
        let backend = Arc::new(FakeCaptureBackend::successful());
        let mut controller = capture_controller(backend, [Ok(false)]);
        let generation = controller.request_start().unwrap();
        let (_, repaint) = repaint_counter();
        let request = controller.poll_capture(Some(1), repaint);
        assert_eq!(request.park_launcher.unwrap().generation, generation);

        let cancelled = controller.cancel_capture_startup(generation).unwrap();
        assert!(cancelled.restore_launcher);
        assert!(cancelled.diagnostic.is_none());
        assert_eq!(controller.state(), &ScreenDrawState::NoSession);
        assert!(controller.pending_capture.is_none());
        assert!(controller.session_snapshot().is_none());
    }

    #[test]
    fn native_start_failure_restores_and_a_new_generation_can_retry() {
        let backend = Arc::new(FakeCaptureBackend::successful());
        let mut controller = capture_controller_with_native(
            backend,
            [Ok(true), Ok(true)],
            Arc::new(FailingNativeFactory),
        );
        let recovery = Arc::new(ScreenDrawRecoveryBridge::default());
        controller.set_recovery_bridge(Arc::clone(&recovery));
        let (_, repaint) = repaint_counter();
        let first = controller.request_start().unwrap();
        assert!(recovery.is_active());
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        controller.poll_capture(Some(1), Arc::clone(&repaint));
        let failure = poll_until(&mut controller, &repaint, |poll| poll.restore_launcher);
        assert_eq!(
            failure.diagnostic.as_deref(),
            Some("fixture native startup failed")
        );
        assert!(controller.session_snapshot().is_none());
        assert!(controller.native_worker.is_none());
        assert!(!recovery.is_active());
        assert!(!recovery.has_emergency_handle());

        let second = controller.request_start().unwrap();
        assert!(recovery.is_active());
        assert_ne!(first, second);
        assert!(
            controller
                .poll_capture(Some(1), repaint)
                .park_launcher
                .is_some()
        );
    }

    #[test]
    fn new_capture_cancels_worker_and_ignores_stale_completion() {
        let backend = Arc::new(FakeCaptureBackend {
            desktop_calls: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
            fail: false,
            block_first_until_cancelled: true,
        });
        let mut controller = capture_controller(Arc::clone(&backend), [Ok(true), Ok(true)]);
        let (_, repaint) = repaint_counter();
        let first = controller.request_start().unwrap();
        assert!(
            controller
                .poll_capture(Some(1), Arc::clone(&repaint))
                .park_launcher
                .is_some()
        );
        assert!(
            controller
                .poll_capture(Some(1), Arc::clone(&repaint))
                .capture_started
        );
        while backend.calls.load(Ordering::SeqCst) == 0 {
            std::thread::yield_now();
        }

        let second = controller.request_new_capture().unwrap();
        assert_ne!(first, second);
        assert!(
            controller
                .poll_capture(Some(1), Arc::clone(&repaint))
                .park_launcher
                .is_some()
        );
        let started = poll_until(&mut controller, &repaint, |poll| poll.capture_started);
        assert!(started.capture_started);
        let completed = poll_until(&mut controller, &repaint, |poll| poll.capture_completed);
        assert!(completed.capture_completed);
        assert_eq!(
            controller.state(),
            &ScreenDrawState::Drawing { generation: second }
        );
        assert_eq!(
            controller
                .session_snapshot()
                .unwrap()
                .capture()
                .image
                .get_pixel(0, 0)
                .0[0],
            20
        );
    }

    #[test]
    fn capture_failure_clears_pending_state_and_requests_launcher_restore() {
        let backend = Arc::new(FakeCaptureBackend {
            desktop_calls: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
            fail: true,
            block_first_until_cancelled: false,
        });
        let mut controller = capture_controller(backend, [Ok(true)]);
        let (_, repaint) = repaint_counter();
        let generation = controller.request_start().unwrap();
        assert!(
            controller
                .poll_capture(Some(1), Arc::clone(&repaint))
                .park_launcher
                .is_some()
        );
        assert!(
            controller
                .poll_capture(Some(1), Arc::clone(&repaint))
                .capture_started
        );
        let failure = poll_until(&mut controller, &repaint, |poll| poll.restore_launcher);
        assert_eq!(
            failure.diagnostic.as_deref(),
            Some("fixture capture failed")
        );
        assert!(matches!(
            controller.state(),
            ScreenDrawState::Failed { generation: failed, .. } if *failed == generation
        ));
        assert!(controller.pending_capture.is_none());
        assert!(controller.session_snapshot().is_none());
        assert!(!controller.toolbar_open());
    }
}
