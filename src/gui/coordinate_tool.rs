use std::collections::VecDeque;
use std::sync::Arc;

use crate::commands::CoordinateToolCommand;
use crate::coordinate_tool::{
    CaptureOutcome, CaptureRuntime, CaptureSessionId, CoordinateCaptureController,
    CoordinateRuntimeFactory, CoordinateSpace, CoordinateToolController, CoordinateToolPreferences,
    CoordinateUnavailable, NativeCoordinateCaptureRuntime, format_coordinate,
};
use crate::launcher_parking::LauncherParkingTransaction;
use crate::settings::Settings;

pub(crate) trait CoordinateClipboardWriter {
    fn set_text(&mut self, text: &str) -> anyhow::Result<()>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CoordinateCaptureFeedback {
    Copied(String),
    Cancelled,
    Failed(String),
}

struct SystemCoordinateClipboard;

impl CoordinateClipboardWriter for SystemCoordinateClipboard {
    fn set_text(&mut self, text: &str) -> anyhow::Result<()> {
        crate::actions::clipboard::set_text(text)
    }
}

pub(super) struct CoordinatePickParking {
    pub(super) transaction: Option<LauncherParkingTransaction<u64>>,
    pub(super) prior_visible: bool,
    pub(super) owned_revision: u64,
}

pub(crate) struct CoordinateToolGui {
    controller: CoordinateToolController,
    settings_path: String,
    preferences: CoordinateToolPreferences,
    clipboard: Box<dyn CoordinateClipboardWriter>,
    reported_error: Option<String>,
    capture: CoordinateCaptureController,
    active_pick: Option<(CaptureSessionId, CoordinateSpace, bool)>,
    capture_feedback: VecDeque<CoordinateCaptureFeedback>,
}

impl CoordinateToolGui {
    pub(crate) fn new(settings_path: String, preferences: CoordinateToolPreferences) -> Self {
        Self::with_backends(
            settings_path,
            preferences,
            Arc::new(crate::coordinate_tool::NativeCoordinateRuntimeFactory),
            Box::new(SystemCoordinateClipboard),
        )
    }

    fn with_backends(
        settings_path: String,
        preferences: CoordinateToolPreferences,
        factory: Arc<dyn CoordinateRuntimeFactory>,
        clipboard: Box<dyn CoordinateClipboardWriter>,
    ) -> Self {
        Self::with_backends_and_capture(
            settings_path,
            preferences,
            factory,
            clipboard,
            Arc::new(NativeCoordinateCaptureRuntime),
        )
    }

    pub(crate) fn with_backends_and_capture(
        settings_path: String,
        preferences: CoordinateToolPreferences,
        factory: Arc<dyn CoordinateRuntimeFactory>,
        clipboard: Box<dyn CoordinateClipboardWriter>,
        capture_runtime: Arc<dyn CaptureRuntime>,
    ) -> Self {
        let preferences = preferences.normalized();
        Self {
            controller: CoordinateToolController::new_with_preferences(
                factory,
                preferences.clone(),
            ),
            settings_path,
            preferences,
            clipboard,
            reported_error: None,
            capture: CoordinateCaptureController::new(capture_runtime),
            active_pick: None,
            capture_feedback: VecDeque::new(),
        }
    }

    pub(crate) fn apply_loaded_preferences(
        &mut self,
        preferences: CoordinateToolPreferences,
    ) -> Result<(), String> {
        let preferences = preferences.normalized();
        self.controller
            .set_preferences(preferences.clone())
            .map_err(|error| format!("Could not apply coordinate preferences: {error}"))?;
        self.preferences = preferences;
        Ok(())
    }

    pub(crate) fn execute(
        &mut self,
        command: &CoordinateToolCommand,
    ) -> Result<Option<String>, String> {
        use CoordinateToolCommand as Command;

        match command {
            Command::ToggleHud => {
                let enabled = !self.controller.runtime_state().hud_enabled();
                self.controller.set_hud_enabled(enabled)?;
            }
            Command::SetHudEnabled(enabled) => self.controller.set_hud_enabled(*enabled)?,
            Command::SetSpace(space) => {
                let space = *space;
                self.update_preferences(|preferences| preferences.space = space)?;
            }
            Command::SetHudDetail(detail) => {
                let detail = *detail;
                self.update_preferences(|preferences| preferences.hud_detail = detail)?;
            }
            Command::SetOffset(offset) => {
                let offset = *offset;
                self.update_preferences(|preferences| preferences.cursor_offset = offset)?;
            }
            Command::Freeze => self.controller.freeze(),
            Command::Unfreeze => self.controller.unfreeze(),
            Command::Copy => return self.copy_displayed_sample().map(Some),
            Command::Pick => {
                self.begin_pick()?;
            }
            Command::Cancel => {
                if self.cancel_pick() {
                    return Ok(Some(
                        "Coordinate pick is draining any consumed click before cancellation."
                            .into(),
                    ));
                }
            }
            Command::HudHelp | Command::CrosshairHelp => {
                return Err("help is handled by the coordinate command handler".into());
            }
            Command::ToggleCrosshair => {
                let enabled = !self.controller.runtime_state().crosshair_enabled();
                self.controller.set_crosshair_enabled(enabled)?;
            }
            Command::SetCrosshairEnabled(enabled) => {
                self.controller.set_crosshair_enabled(*enabled)?;
            }
            Command::SetCrosshairColor(color) => {
                let color = *color;
                self.update_preferences(|preferences| preferences.crosshair.color = color)?;
            }
            Command::SetCrosshairThickness(thickness) => {
                let thickness = *thickness;
                self.update_preferences(|preferences| preferences.crosshair.thickness = thickness)?;
            }
            Command::SetCrosshairLength(length) => {
                let length = *length;
                self.update_preferences(|preferences| preferences.crosshair.arm_length = length)?;
            }
            Command::SetCrosshairOpacity(opacity) => {
                let opacity = *opacity;
                self.update_preferences(|preferences| preferences.crosshair.opacity = opacity)?;
            }
            Command::SetGuides(enabled) => {
                let enabled = *enabled;
                self.update_preferences(|preferences| {
                    preferences.crosshair.virtual_desktop_guides = enabled
                })?;
            }
            Command::SetContrast(enabled) => {
                let enabled = *enabled;
                self.update_preferences(|preferences| {
                    preferences.crosshair.high_contrast_outline = enabled
                })?;
            }
            Command::Invalid { error, .. } => return Err(error.clone()),
        }
        Ok(None)
    }

    pub(crate) fn poll_error(&mut self) -> Option<String> {
        match self.controller.last_error() {
            Some(error) if self.reported_error.as_deref() != Some(error.as_str()) => {
                self.reported_error = Some(error.clone());
                Some(error)
            }
            Some(_) => None,
            None => {
                self.reported_error = None;
                None
            }
        }
    }

    pub(crate) fn shutdown(&mut self) -> Result<(), String> {
        if let Some((_, _, publish_allowed)) = &mut self.active_pick {
            *publish_allowed = false;
        }
        self.capture.request_shutdown();
        self.controller.shutdown()
    }
}

impl LauncherApp {
    pub(super) fn ensure_no_coordinate_pick(&self) -> Result<(), String> {
        if self.coordinate_tool.capture_pending() {
            Err("Finish or cancel coordinate picking before starting another screen capture".into())
        } else {
            Ok(())
        }
    }

    pub(super) fn ensure_coordinate_pick_admitted(&self) -> Result<(), String> {
        if self.coordinate_tool.capture_pending() {
            return Ok(());
        }
        if self.color_pick_capture_busy() {
            return Err(
                "Finish or cancel the screen color picker before picking coordinates".into(),
            );
        }
        if self.ocr_capture_busy() {
            return Err("Finish or cancel Screen Region OCR before picking coordinates".into());
        }
        if self.screen_draw_launcher_parking.is_some()
            || !matches!(
                self.screen_draw_controller.state(),
                crate::screen_draw::ScreenDrawState::NoSession
                    | crate::screen_draw::ScreenDrawState::Failed { .. }
            )
        {
            return Err("Close the active Screen Draw session before picking coordinates".into());
        }
        if self.crop_screenshot_operation.is_some() {
            return Err("Finish or cancel the screenshot crop before picking coordinates".into());
        }
        if self
            .mkmacro_dialog
            .visual_overlay_controller()
            .operation_id()
            .is_some()
        {
            return Err(
                "Finish or cancel the active screen selection before picking coordinates".into(),
            );
        }
        Ok(())
    }

    pub(super) fn begin_coordinate_pick(&mut self) -> Result<String, String> {
        // Finish any previous terminal result and release its parking owner
        // before a new pick can replace the lifecycle state.
        let ctx = self.egui_ctx.clone();
        self.poll_coordinate_pick(&ctx);
        if self.coordinate_tool.capture_pending() {
            return Ok(COORDINATE_PICK_PROMPT.into());
        }
        self.ensure_coordinate_pick_admitted()?;

        let (observed, (prior_visible, focus, invocation)) =
            self.visibility_revision.inspect(|| {
                (
                    self.visible_flag.load(Ordering::SeqCst),
                    self.visibility_revision.focus_intent(),
                    self.visibility_revision.invocation_id(),
                )
            });
        let generation = self
            .coordinate_capture_generation
            .checked_add(1)
            .ok_or_else(|| "Coordinate capture generation space was exhausted".to_owned())?;

        #[cfg(windows)]
        let mut transaction = if prior_visible {
            use crate::mkmacro::screen::ScreenCaptureBackend;
            let desktop = crate::mkmacro::screen::WindowsScreenCaptureBackend::system()
                .virtual_desktop()
                .map_err(|error| format!("Could not locate the virtual desktop: {error}"))?;
            let hwnd = self
                .launcher_hwnd
                .ok_or_else(|| "Launcher HWND is unavailable for coordinate picking".to_owned())?;
            Some(LauncherParkingTransaction::begin(
                generation, hwnd, desktop,
            )?)
        } else {
            None
        };
        #[cfg(not(windows))]
        let mut transaction: Option<LauncherParkingTransaction<u64>> = None;
        #[cfg(not(windows))]
        if prior_visible {
            return Err("Coordinate picking requires the Windows launcher window".into());
        }

        let owned_revision = if prior_visible {
            let Some((revision, ())) = self
                .visibility_revision
                .request_if_current_with_focus_intent_and_invocation(
                    observed,
                    focus,
                    invocation,
                    || {
                        self.visible_flag.store(false, Ordering::SeqCst);
                        self.restore_flag.store(false, Ordering::SeqCst);
                    },
                )
            else {
                if let Some(transaction) = &mut transaction {
                    transaction.restore()?;
                }
                return Err(
                    "A newer launcher visibility request interrupted coordinate picking".into(),
                );
            };
            self.last_visible = false;
            revision
        } else {
            observed
        };

        self.coordinate_capture_generation = generation;
        self.coordinate_capture_parking = Some(CoordinatePickParking {
            transaction,
            prior_visible,
            owned_revision,
        });
        if let Err(error) = self.coordinate_tool.begin_pick() {
            self.finish_coordinate_pick_parking()?;
            return Err(error);
        }
        self.egui_ctx.request_repaint();
        Ok(COORDINATE_PICK_PROMPT.into())
    }

    pub(super) fn cancel_coordinate_pick(&mut self) -> Option<String> {
        self.coordinate_tool
            .cancel_pick()
            .then(|| "Coordinate pick is draining any consumed click before cancellation.".into())
    }

    pub(super) fn poll_coordinate_pick(&mut self, ctx: &egui::Context) {
        let superseded = self
            .coordinate_capture_parking
            .as_ref()
            .is_some_and(|state| self.visibility_revision.current() != state.owned_revision);
        if superseded && self.coordinate_tool.capture_pending() {
            // Mark publication disallowed before polling: the worker may
            // already have completed, while its joined outcome is still queued.
            self.coordinate_tool.supersede_pick();
        }
        self.coordinate_tool.poll_capture();
        if !self.coordinate_tool.capture_pending()
            && self.coordinate_capture_parking.is_some()
            && let Err(error) = self.finish_coordinate_pick_parking()
        {
            self.report_error_message("coordinate_tool.restore", error);
        }
        while let Some(feedback) = self.coordinate_tool.take_capture_feedback() {
            match feedback {
                CoordinateCaptureFeedback::Copied(text) => {
                    if self.enable_toasts {
                        self.add_toast(crate::gui::Toast {
                            text: format!("Copied {text}").into(),
                            kind: crate::gui::ToastKind::Success,
                            options: crate::gui::ToastOptions::default()
                                .duration_in_seconds(self.toast_duration as f64),
                        });
                    }
                }
                CoordinateCaptureFeedback::Cancelled => {
                    if self.enable_toasts {
                        self.add_toast(crate::gui::Toast {
                            text: "Coordinate pick canceled; clipboard unchanged".into(),
                            kind: crate::gui::ToastKind::Info,
                            options: crate::gui::ToastOptions::default()
                                .duration_in_seconds(self.toast_duration as f64),
                        });
                    }
                }
                CoordinateCaptureFeedback::Failed(error) => {
                    self.report_error_message("coordinate_tool.pick", error);
                }
            }
        }
        if self.coordinate_tool.capture_pending() {
            ctx.request_repaint_after(std::time::Duration::from_millis(25));
        }
    }

    pub(super) fn reconcile_coordinate_pick_parking(&mut self) {
        if self.visible_flag.load(Ordering::SeqCst) {
            return;
        }
        if let Some(transaction) = self
            .coordinate_capture_parking
            .as_mut()
            .and_then(|state| state.transaction.as_mut())
            && let Err(error) = transaction.repark_after_stale_restore(|| Ok(()))
        {
            self.report_error_message("coordinate_tool.parking", error);
        }
    }

    pub(super) fn shutdown_coordinate_pick(&mut self) {
        if let Some(state) = &mut self.coordinate_capture_parking
            && let Some(transaction) = &mut state.transaction
        {
            transaction.commit_hidden();
        }
        self.coordinate_capture_parking = None;
    }

    fn finish_coordinate_pick_parking(&mut self) -> Result<(), String> {
        let Some(mut state) = self.coordinate_capture_parking.take() else {
            return Ok(());
        };
        match self.finish_coordinate_pick_parking_state(&mut state) {
            Ok(()) => Ok(()),
            Err(error) => {
                // Keep the restore owner available for the next frame to retry.
                self.coordinate_capture_parking = Some(state);
                Err(error)
            }
        }
    }

    fn finish_coordinate_pick_parking_state(
        &mut self,
        state: &mut CoordinatePickParking,
    ) -> Result<(), String> {
        let (observed, (visible, focus, invocation)) = self.visibility_revision.inspect(|| {
            (
                self.visible_flag.load(Ordering::SeqCst),
                self.visibility_revision.focus_intent(),
                self.visibility_revision.invocation_id(),
            )
        });
        if let Some(transaction) = &mut state.transaction {
            transaction.restore()?;
        }

        if observed != state.owned_revision {
            if !visible && let Some(transaction) = &mut state.transaction {
                transaction.repark_after_stale_restore(|| Ok(()))?;
                transaction.commit_hidden();
            }
            self.last_visible = visible;
            return Ok(());
        }

        let Some((_revision, ())) = self
            .visibility_revision
            .request_if_current_with_focus_intent_and_invocation(
                observed,
                focus,
                invocation,
                || {
                    self.visible_flag
                        .store(state.prior_visible, Ordering::SeqCst);
                    self.restore_flag
                        .store(state.prior_visible, Ordering::SeqCst);
                },
            )
        else {
            let (current_visible, ()) = self
                .visibility_revision
                .inspect(|| self.visible_flag.load(Ordering::SeqCst));
            if !current_visible && let Some(transaction) = &mut state.transaction {
                transaction.repark_after_stale_restore(|| Ok(()))?;
                transaction.commit_hidden();
            }
            self.last_visible = current_visible;
            return Ok(());
        };
        self.last_visible = state.prior_visible;
        self.egui_ctx.request_repaint_of(egui::ViewportId::ROOT);
        Ok(())
    }
}

impl CoordinateToolGui {
    /// Start one capture with the current coordinate space frozen into the
    /// session. A repeated request leaves that choice and session unchanged.
    pub(crate) fn begin_pick(&mut self) -> Result<bool, String> {
        self.poll_capture();
        if self.capture.is_active() {
            return Ok(false);
        }

        let session_id = match self.capture.begin() {
            Ok(session_id) => session_id,
            Err(error) => {
                // A failed thread start publishes a terminal status itself.
                // Its command error is reported synchronously here.
                while self.capture.take_completed().is_some() {}
                return Err(error);
            }
        };
        self.active_pick = Some((session_id, self.preferences.space, true));
        Ok(true)
    }

    pub(crate) fn cancel_pick(&mut self) -> bool {
        self.poll_capture();
        self.capture.is_active() && self.capture.cancel()
    }

    /// A newer launcher visibility request supersedes this capture's UI owner.
    /// Drain native input as usual, but never publish the click to the clipboard.
    pub(crate) fn supersede_pick(&mut self) {
        if let Some((_, _, publish_allowed)) = &mut self.active_pick {
            *publish_allowed = false;
        }
        self.capture.cancel();
    }

    pub(crate) fn capture_pending(&self) -> bool {
        self.active_pick.is_some() || self.capture.is_active()
    }

    pub(crate) fn poll_capture(&mut self) -> bool {
        let changed = self.capture.poll();
        while let Some(status) = self.capture.take_completed() {
            let active_pick = match self.active_pick.take() {
                Some(active) if active.0 == status.session_id => Some(active),
                Some(active) => {
                    self.active_pick = Some(active);
                    None
                }
                None => None,
            };
            match status.outcome {
                Some(CaptureOutcome::Captured(sample)) => {
                    let Some((_, space, publish_allowed)) = active_pick else {
                        continue;
                    };
                    if !publish_allowed {
                        self.capture_feedback
                            .push_back(CoordinateCaptureFeedback::Cancelled);
                        continue;
                    }
                    match format_coordinate(&sample, space) {
                        Ok(formatted) => match self.clipboard.set_text(&formatted.text) {
                            Ok(()) => {
                                let text = formatted.text.clone();
                                self.controller.record_successful_copy(formatted);
                                self.capture_feedback
                                    .push_back(CoordinateCaptureFeedback::Copied(text));
                            }
                            Err(error) => {
                                self.capture_feedback
                                    .push_back(CoordinateCaptureFeedback::Failed(format!(
                                        "Could not copy coordinates: {error}"
                                    )))
                            }
                        },
                        Err(error) => {
                            self.capture_feedback
                                .push_back(CoordinateCaptureFeedback::Failed(
                                    coordinate_unavailable_message(error),
                                ))
                        }
                    }
                }
                Some(CaptureOutcome::Cancelled) => {
                    if active_pick.is_some() {
                        self.capture_feedback
                            .push_back(CoordinateCaptureFeedback::Cancelled);
                    }
                }
                Some(CaptureOutcome::Failed(error)) => {
                    if active_pick.is_some() {
                        self.capture_feedback
                            .push_back(CoordinateCaptureFeedback::Failed(error));
                    }
                }
                Some(CaptureOutcome::Shutdown) | None => {}
            }
        }
        changed
    }

    pub(crate) fn take_capture_feedback(&mut self) -> Option<CoordinateCaptureFeedback> {
        self.capture_feedback.pop_front()
    }

    #[cfg(test)]
    pub(crate) fn preferences(&self) -> &CoordinateToolPreferences {
        &self.preferences
    }

    #[cfg(test)]
    pub(crate) fn runtime_state(&self) -> crate::coordinate_tool::CoordinateToolRuntimeState {
        self.controller.runtime_state()
    }

    fn update_preferences(
        &mut self,
        update: impl FnOnce(&mut CoordinateToolPreferences),
    ) -> Result<(), String> {
        let settings_path = self.settings_path.clone();
        let committed = Settings::update(&settings_path, |settings| {
            update(&mut settings.coordinate_tool);
            settings.coordinate_tool = settings.coordinate_tool.clone().normalized();
            Ok(())
        })
        .map_err(|error| format!("Could not save coordinate preferences: {error}"))?;
        self.apply_loaded_preferences(committed.coordinate_tool)
    }

    fn copy_displayed_sample(&mut self) -> Result<String, String> {
        let sample = self.controller.sample_for_copy()?;
        let formatted = format_coordinate(&sample, self.preferences.space)
            .map_err(coordinate_unavailable_message)?;
        self.clipboard
            .set_text(&formatted.text)
            .map_err(|error| format!("Could not copy coordinates: {error}"))?;
        let text = formatted.text.clone();
        self.controller.record_successful_copy(formatted);
        Ok(text)
    }
}

const COORDINATE_PICK_PROMPT: &str = "Coordinate pick is active. Click once to copy the selected physical coordinates, or press Escape to cancel.";

fn coordinate_unavailable_message(reason: CoordinateUnavailable) -> String {
    match reason {
        CoordinateUnavailable::MonitorUnavailable => {
            "Monitor-relative coordinates are unavailable for the current sample.".into()
        }
        CoordinateUnavailable::ForegroundClientUnavailable => {
            "Foreground-client coordinates are unavailable for the current sample.".into()
        }
        CoordinateUnavailable::ArithmeticOverflow => {
            "Coordinate conversion exceeded the supported physical-pixel range.".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::coordinate_tool::{
        CaptureControl, CaptureOutcome, CapturePhase, CaptureRuntime, CaptureSessionId,
        CaptureUpdatePublisher, CoordinateOffset, CoordinateRenderFrame, CoordinateSample,
        CoordinateSampler, CoordinateSpace, CoordinateSurfaceBackend, ForegroundClientGeometry,
        MonitorGeometry, MonitorId, PhysicalPoint, PhysicalRect,
    };

    #[derive(Clone)]
    struct FakeFactory {
        sample: Arc<Mutex<CoordinateSample>>,
        rendered: mpsc::Sender<CoordinateRenderFrame>,
    }

    struct FakeSampler(Arc<Mutex<CoordinateSample>>);

    impl CoordinateSampler for FakeSampler {
        fn sample(&mut self) -> Result<CoordinateSample, String> {
            Ok(self.0.lock().unwrap().clone())
        }
    }

    struct FakeBackend(mpsc::Sender<CoordinateRenderFrame>);

    impl CoordinateSurfaceBackend for FakeBackend {
        fn render(&mut self, frame: &CoordinateRenderFrame) -> Result<(), String> {
            self.0
                .send(frame.clone())
                .map_err(|error| error.to_string())
        }

        fn shutdown(&mut self) -> Result<(), String> {
            Ok(())
        }
    }

    impl CoordinateRuntimeFactory for FakeFactory {
        fn create_sampler(&self) -> Result<Box<dyn CoordinateSampler>, String> {
            Ok(Box::new(FakeSampler(Arc::clone(&self.sample))))
        }

        fn create_backend(&self) -> Result<Box<dyn CoordinateSurfaceBackend>, String> {
            Ok(Box::new(FakeBackend(self.rendered.clone())))
        }
    }

    #[derive(Clone)]
    struct FakeClipboard {
        fail: Arc<AtomicBool>,
        writes: Arc<Mutex<Vec<String>>>,
        current: Arc<Mutex<Option<String>>>,
    }

    impl CoordinateClipboardWriter for FakeClipboard {
        fn set_text(&mut self, text: &str) -> anyhow::Result<()> {
            if self.fail.load(Ordering::Acquire) {
                anyhow::bail!("injected clipboard failure");
            }
            self.writes.lock().unwrap().push(text.to_owned());
            *self.current.lock().unwrap() = Some(text.to_owned());
            Ok(())
        }
    }

    struct FakeCaptureRuntime {
        started: mpsc::Sender<CaptureSessionId>,
        outcomes: Mutex<mpsc::Receiver<CaptureOutcome>>,
    }

    impl CaptureRuntime for FakeCaptureRuntime {
        fn run_session(
            &self,
            session_id: CaptureSessionId,
            _control: CaptureControl,
            updates: CaptureUpdatePublisher,
        ) -> CaptureOutcome {
            let _ = updates.publish_phase(CapturePhase::WaitingForFreshClick);
            let _ = self.started.send(session_id);
            self.outcomes
                .lock()
                .unwrap()
                .recv()
                .unwrap_or_else(|_| CaptureOutcome::Failed("test outcome sender closed".into()))
        }
    }

    fn sample(x: i32, y: i32) -> CoordinateSample {
        CoordinateSample::new(
            PhysicalPoint::new(x, y),
            Some(PhysicalRect::new(-1920, 0, 1920, 1080).unwrap()),
            Some(MonitorGeometry {
                id: MonitorId::new("DISPLAY1"),
                bounds: PhysicalRect::new(-1920, 0, 0, 1080).unwrap(),
                work_area: PhysicalRect::new(-1920, 0, 0, 1040).unwrap(),
                effective_dpi: Some((96, 96)),
            }),
            Some(ForegroundClientGeometry::new(
                PhysicalPoint::new(-1800, 40),
                Some(PhysicalRect::new(-1800, 40, -100, 800).unwrap()),
            )),
        )
    }

    fn adapter(
        settings_path: String,
    ) -> (
        CoordinateToolGui,
        mpsc::Receiver<CoordinateRenderFrame>,
        Arc<Mutex<CoordinateSample>>,
        FakeClipboard,
    ) {
        let (rendered, frames) = mpsc::channel();
        let current_sample = Arc::new(Mutex::new(sample(-1800, 200)));
        let factory = FakeFactory {
            sample: Arc::clone(&current_sample),
            rendered,
        };
        let clipboard = FakeClipboard {
            fail: Arc::new(AtomicBool::new(false)),
            writes: Arc::new(Mutex::new(Vec::new())),
            current: Arc::new(Mutex::new(None)),
        };
        let adapter = CoordinateToolGui::with_backends(
            settings_path,
            CoordinateToolPreferences::default(),
            Arc::new(factory),
            Box::new(clipboard.clone()),
        );
        (adapter, frames, current_sample, clipboard)
    }

    fn capture_adapter(
        settings_path: String,
    ) -> (
        CoordinateToolGui,
        mpsc::Receiver<CoordinateRenderFrame>,
        mpsc::Receiver<CaptureSessionId>,
        mpsc::Sender<CaptureOutcome>,
        FakeClipboard,
    ) {
        let (rendered, frames) = mpsc::channel();
        let current_sample = Arc::new(Mutex::new(sample(-1800, 200)));
        let factory = FakeFactory {
            sample: current_sample,
            rendered,
        };
        let (started_tx, started_rx) = mpsc::channel();
        let (outcome_tx, outcome_rx) = mpsc::channel();
        let runtime = Arc::new(FakeCaptureRuntime {
            started: started_tx,
            outcomes: Mutex::new(outcome_rx),
        });
        let clipboard = FakeClipboard {
            fail: Arc::new(AtomicBool::new(false)),
            writes: Arc::new(Mutex::new(Vec::new())),
            current: Arc::new(Mutex::new(None)),
        };
        let adapter = CoordinateToolGui::with_backends_and_capture(
            settings_path,
            CoordinateToolPreferences::default(),
            Arc::new(factory),
            Box::new(clipboard.clone()),
            runtime,
        );
        (adapter, frames, started_rx, outcome_tx, clipboard)
    }

    fn wait_for_capture_join(gui: &mut CoordinateToolGui) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while gui.capture.is_active() && Instant::now() < deadline {
            gui.poll_capture();
            thread::yield_now();
        }
        gui.poll_capture();
        assert!(!gui.capture.is_active(), "fake capture worker should join");
    }

    fn receive_frame(frames: &mpsc::Receiver<CoordinateRenderFrame>) -> CoordinateRenderFrame {
        frames
            .recv_timeout(Duration::from_secs(2))
            .expect("fake worker should publish its frame")
    }

    #[test]
    fn preference_updates_persist_transactionally_before_runtime_publication() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let path = path.to_string_lossy().to_string();
        let (mut gui, _, _, _) = adapter(path.clone());
        gui.execute(&CoordinateToolCommand::SetSpace(
            crate::coordinate_tool::CoordinateSpace::ForegroundClient,
        ))
        .unwrap();
        let committed = Settings::load(&path).unwrap();
        assert_eq!(
            committed.coordinate_tool.space,
            crate::coordinate_tool::CoordinateSpace::ForegroundClient
        );
        assert_eq!(gui.preferences().space, committed.coordinate_tool.space);

        let invalid_path = directory.path().to_string_lossy().to_string();
        let (mut failed_gui, _, _, _) = adapter(invalid_path);
        let before = failed_gui.preferences().clone();
        assert!(
            failed_gui
                .execute(&CoordinateToolCommand::SetOffset(CoordinateOffset::new(
                    80, -40
                )))
                .is_err()
        );
        assert_eq!(failed_gui.preferences(), &before);
    }

    #[test]
    fn copy_tracks_only_successful_clipboard_writes_and_uses_frozen_display_sample() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("settings.json")
            .to_string_lossy()
            .to_string();
        let (mut gui, frames, current_sample, clipboard) = adapter(path);
        gui.execute(&CoordinateToolCommand::SetHudEnabled(true))
            .unwrap();
        assert!(receive_frame(&frames).current_sample.is_some());

        clipboard.fail.store(true, Ordering::Release);
        assert!(gui.execute(&CoordinateToolCommand::Copy).is_err());
        assert!(gui.runtime_state().last_successful_copy().is_none());
        assert!(clipboard.writes.lock().unwrap().is_empty());

        gui.execute(&CoordinateToolCommand::Freeze).unwrap();
        *current_sample.lock().unwrap() = sample(-1600, 300);
        let mut saw_moved_cursor = false;
        for _ in 0..8 {
            let frame = receive_frame(&frames);
            if frame
                .current_sample
                .as_ref()
                .is_some_and(|sample| sample.desktop_point == PhysicalPoint::new(-1600, 300))
            {
                saw_moved_cursor = true;
                break;
            }
        }
        assert!(
            saw_moved_cursor,
            "the fake sampler should publish the moved point"
        );

        clipboard.fail.store(false, Ordering::Release);
        assert_eq!(
            gui.execute(&CoordinateToolCommand::Copy).unwrap(),
            Some("-1800,200".into())
        );
        assert_eq!(*clipboard.writes.lock().unwrap(), ["-1800,200"]);
        assert_eq!(
            gui.runtime_state().last_successful_copy().unwrap().text,
            "-1800,200"
        );
        gui.shutdown().unwrap();
    }

    #[test]
    fn hud_and_crosshair_enablement_remain_independent_through_the_adapter() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("settings.json")
            .to_string_lossy()
            .to_string();
        let (mut gui, frames, _, _) = adapter(path);
        gui.execute(&CoordinateToolCommand::SetHudEnabled(true))
            .unwrap();
        receive_frame(&frames);
        gui.execute(&CoordinateToolCommand::SetCrosshairEnabled(true))
            .unwrap();
        gui.execute(&CoordinateToolCommand::SetHudEnabled(false))
            .unwrap();
        let state = gui.runtime_state();
        assert!(!state.hud_enabled());
        assert!(state.crosshair_enabled());
        gui.shutdown().unwrap();
    }

    #[test]
    fn pick_copies_click_time_sample_in_the_start_space_even_when_hud_is_frozen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("settings.json")
            .to_string_lossy()
            .to_string();
        let (mut gui, frames, started, outcomes, clipboard) = capture_adapter(path);
        gui.execute(&CoordinateToolCommand::SetHudEnabled(true))
            .unwrap();
        receive_frame(&frames);
        gui.execute(&CoordinateToolCommand::SetSpace(CoordinateSpace::Monitor))
            .unwrap();
        gui.execute(&CoordinateToolCommand::Freeze).unwrap();
        let frozen_copy = gui
            .controller
            .sample_for_copy()
            .and_then(|sample| format_coordinate(&sample, CoordinateSpace::Monitor));
        assert_eq!(frozen_copy.unwrap().text, "120,200");

        assert!(gui.begin_pick().unwrap());
        let session_id = started.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            outcomes
                .send(CaptureOutcome::Captured(sample(-1500, 250)))
                .is_ok()
        );
        wait_for_capture_join(&mut gui);

        assert_eq!(*clipboard.writes.lock().unwrap(), ["420,250"]);
        assert_eq!(*clipboard.current.lock().unwrap(), Some("420,250".into()));
        assert_eq!(
            gui.controller
                .runtime_state()
                .last_successful_copy()
                .unwrap()
                .text,
            "420,250"
        );
        assert_eq!(gui.active_pick, None);
        assert!(session_id.get() > 0);
        gui.shutdown().unwrap();
    }

    #[test]
    fn cancelled_pick_preserves_clipboard_and_success_status() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("settings.json")
            .to_string_lossy()
            .to_string();
        let (mut gui, _, started, outcomes, clipboard) = capture_adapter(path);
        *clipboard.current.lock().unwrap() = Some("sentinel".into());
        assert!(gui.begin_pick().unwrap());
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(gui.cancel_pick());
        outcomes.send(CaptureOutcome::Cancelled).unwrap();
        wait_for_capture_join(&mut gui);

        assert_eq!(*clipboard.current.lock().unwrap(), Some("sentinel".into()));
        assert!(clipboard.writes.lock().unwrap().is_empty());
        assert!(
            gui.controller
                .runtime_state()
                .last_successful_copy()
                .is_none()
        );
        assert_eq!(
            gui.take_capture_feedback(),
            Some(CoordinateCaptureFeedback::Cancelled)
        );
        gui.shutdown().unwrap();
    }

    #[test]
    fn pick_geometry_or_clipboard_failure_never_records_success() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("settings.json")
            .to_string_lossy()
            .to_string();
        let (mut gui, _, started, outcomes, clipboard) = capture_adapter(path.clone());
        gui.execute(&CoordinateToolCommand::SetSpace(CoordinateSpace::Monitor))
            .unwrap();
        let no_monitor = CoordinateSample::new(PhysicalPoint::new(7, 8), None, None, None);
        assert!(gui.begin_pick().unwrap());
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        outcomes.send(CaptureOutcome::Captured(no_monitor)).unwrap();
        wait_for_capture_join(&mut gui);
        assert!(clipboard.writes.lock().unwrap().is_empty());
        assert!(
            gui.controller
                .runtime_state()
                .last_successful_copy()
                .is_none()
        );
        assert!(matches!(
            gui.take_capture_feedback(),
            Some(CoordinateCaptureFeedback::Failed(message)) if message.contains("Monitor-relative")
        ));
        gui.shutdown().unwrap();

        let (mut gui, _, started, outcomes, clipboard) = capture_adapter(path);
        *clipboard.current.lock().unwrap() = Some("sentinel".into());
        clipboard.fail.store(true, Ordering::Release);
        assert!(gui.begin_pick().unwrap());
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        outcomes
            .send(CaptureOutcome::Captured(sample(-100, 40)))
            .unwrap();
        wait_for_capture_join(&mut gui);
        assert_eq!(*clipboard.current.lock().unwrap(), Some("sentinel".into()));
        assert!(clipboard.writes.lock().unwrap().is_empty());
        assert!(
            gui.controller
                .runtime_state()
                .last_successful_copy()
                .is_none()
        );
        assert!(matches!(
            gui.take_capture_feedback(),
            Some(CoordinateCaptureFeedback::Failed(message)) if message.contains("injected clipboard failure")
        ));
        gui.shutdown().unwrap();
    }

    #[test]
    fn repeated_pick_is_idempotent_and_previous_result_is_published_before_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("settings.json")
            .to_string_lossy()
            .to_string();
        let (mut gui, _, started, outcomes, clipboard) = capture_adapter(path);
        assert!(gui.begin_pick().unwrap());
        let first = started.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(!gui.begin_pick().unwrap());
        assert_eq!(gui.active_pick.map(|pick| pick.0), Some(first));

        outcomes
            .send(CaptureOutcome::Captured(sample(30, 40)))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while gui.capture.is_active() && Instant::now() < deadline {
            // Join the worker without consuming its terminal result through the
            // adapter. The next begin must publish it before opening session 2.
            gui.capture.poll();
            thread::yield_now();
        }
        assert!(!gui.capture.is_active());
        assert!(gui.capture.status().unwrap().outcome.is_some());
        assert!(gui.begin_pick().unwrap());
        assert_eq!(*clipboard.writes.lock().unwrap(), ["30,40"]);
        let second = started.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_ne!(first, second);

        outcomes.send(CaptureOutcome::Cancelled).unwrap();
        wait_for_capture_join(&mut gui);
        assert_eq!(
            gui.take_capture_feedback(),
            Some(CoordinateCaptureFeedback::Copied("30,40".into()))
        );
        gui.shutdown().unwrap();
    }

    #[test]
    fn superseded_joined_capture_does_not_publish_a_stale_click() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("settings.json")
            .to_string_lossy()
            .to_string();
        let (mut gui, _, started, outcomes, clipboard) = capture_adapter(path);
        *clipboard.current.lock().unwrap() = Some("sentinel".into());
        assert!(gui.begin_pick().unwrap());
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        outcomes
            .send(CaptureOutcome::Captured(sample(91, -27)))
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(2);
        while gui.capture.is_active() && Instant::now() < deadline {
            gui.capture.poll();
            thread::yield_now();
        }
        assert!(!gui.capture.is_active(), "capture cleanup should be joined");

        // A visibility change can race with worker completion before the GUI
        // consumes its terminal result. Supersession must still block publish.
        gui.supersede_pick();
        gui.poll_capture();
        assert_eq!(*clipboard.current.lock().unwrap(), Some("sentinel".into()));
        assert!(clipboard.writes.lock().unwrap().is_empty());
        assert!(
            gui.controller
                .runtime_state()
                .last_successful_copy()
                .is_none()
        );
        assert_eq!(
            gui.take_capture_feedback(),
            Some(CoordinateCaptureFeedback::Cancelled)
        );
        gui.shutdown().unwrap();
    }

    #[test]
    fn shutdown_after_capture_join_preserves_clipboard_and_copy_status() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("settings.json")
            .to_string_lossy()
            .to_string();
        let (mut gui, _, started, outcomes, clipboard) = capture_adapter(path);
        *clipboard.current.lock().unwrap() = Some("sentinel".into());
        assert!(gui.begin_pick().unwrap());
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        outcomes
            .send(CaptureOutcome::Captured(sample(10, 20)))
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(2);
        while gui.capture.is_active() && Instant::now() < deadline {
            gui.capture.poll();
            thread::yield_now();
        }
        assert!(!gui.capture.is_active(), "capture cleanup should be joined");

        gui.shutdown().unwrap();
        gui.poll_capture();
        assert_eq!(*clipboard.current.lock().unwrap(), Some("sentinel".into()));
        assert!(clipboard.writes.lock().unwrap().is_empty());
        assert!(
            gui.controller
                .runtime_state()
                .last_successful_copy()
                .is_none()
        );
        assert_eq!(
            gui.take_capture_feedback(),
            Some(CoordinateCaptureFeedback::Cancelled)
        );
    }
}
