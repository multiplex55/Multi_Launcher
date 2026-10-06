//! GUI parking and publication effects for the transient OCR workflow.
//! Captured pixels and recognition mechanics stay on the one-shot worker.
use eframe::egui;
use std::sync::{Arc, atomic::Ordering};
use std::time::{Duration, Instant};

use super::LauncherApp;
use super::mkmacro_dialog::visual_overlay::{RectanglePurpose, VisualOverlayEvent};
use crate::launcher_parking::LauncherParkingTransaction;
use crate::mkmacro::ocr::OcrBackend;
#[cfg(windows)]
use crate::mkmacro::screen::WindowsScreenCaptureBackend;
use crate::mkmacro::screen::{ScreenCaptureBackend, ScreenRect};
use crate::ocr::job::{OcrJob, OcrJobEvent, OcrJobEventKind};
use crate::ocr::selection::{
    OcrGeneration, OcrSelectionController, SelectionEffect, SelectionOutcome,
};
use crate::visibility::{
    RootActivationDisposition, RootViewportCtx, VisiblePlacementPolicy,
    apply_visibility_with_focus_intent,
};

pub(super) struct OcrLifecycle {
    pub(super) controller: OcrSelectionController,
    session: Option<LauncherSession>,
    pub(super) parking: Option<LauncherParkingTransaction<OcrGeneration>>,
    desktop: Arc<dyn ScreenCaptureBackend>,
    recognizer: Arc<dyn OcrBackend>,
    job: Option<OcrJob>,
    #[cfg(test)]
    profile_provider: Option<fn() -> crate::mkmacro::ExecResult<Vec<String>>>,
    restore_error: Option<String>,
}

impl Default for OcrLifecycle {
    fn default() -> Self {
        #[cfg(windows)]
        let desktop = Arc::new(WindowsScreenCaptureBackend::system());
        #[cfg(not(windows))]
        let desktop = crate::mkmacro::Backends::unsupported().screenshot_capture;
        #[cfg(windows)]
        let recognizer = Arc::new(crate::mkmacro::ocr::WindowsOcrBackend::new());
        #[cfg(not(windows))]
        let recognizer = crate::mkmacro::Backends::unsupported().ocr;
        Self {
            controller: OcrSelectionController::default(),
            session: None,
            parking: None,
            desktop,
            recognizer,
            job: None,
            #[cfg(test)]
            profile_provider: None,
            restore_error: None,
        }
    }
}
struct LauncherSession {
    id: OcrGeneration,
    query: String,
    selected: Option<usize>,
    prior_visible: bool,
    owned_revision: u64,
    superseded: bool,
    query_superseded: bool,
    published: bool,
    desktop: Option<ScreenRect>,
    root_restored: bool,
}

impl LauncherApp {
    /// Result/progress admission is separate from physical parking ownership.
    /// M5 can route input to this transient surface without a persisted Panel.
    pub(super) fn ocr_surface_visible(&self) -> bool {
        self.visible_flag.load(Ordering::SeqCst)
            && self.ocr.controller.presentation().is_some()
            && self.ocr.session.as_ref().is_some_and(|session| {
                session.root_restored
                    && !session.superseded
                    && !session.query_superseded
                    && self.visibility_revision.current() == session.owned_revision
                    && self.query == session.query
                    && self.selected == session.selected
            })
    }

    pub(super) fn ocr_owns_root(&self) -> bool {
        self.ocr
            .session
            .as_ref()
            .is_some_and(|session| !session.root_restored)
            || self.ocr.parking.is_some()
    }

    fn ensure_ocr_selection_admitted(&self) -> Result<(), String> {
        if self.color_pick_owns_root() || self.color_pick.controller.is_active() {
            return Err("Finish or cancel the screen color picker before starting OCR".into());
        }
        if self.screen_draw_launcher_parking.is_some()
            || !matches!(
                self.screen_draw_controller.state(),
                crate::screen_draw::ScreenDrawState::NoSession
                    | crate::screen_draw::ScreenDrawState::Failed { .. }
            )
        {
            return Err("Close the active Screen Draw session before starting OCR".into());
        }
        if self
            .mkmacro_dialog
            .visual_overlay_controller()
            .operation_id()
            .is_some()
        {
            return Err("Finish or cancel the active screen selection before starting OCR".into());
        }
        Ok(())
    }

    /// Both ordinary launcher and assigned actions stage the same workflow.
    /// A hidden ROOT can be parked directly; it is never shown before selection.
    pub(super) fn begin_ocr_selection(&mut self) -> Result<bool, String> {
        if self.ocr.controller.is_active() {
            return Ok(false);
        }
        self.ensure_ocr_selection_admitted()?;
        let (owned_revision, prior_visible) = self
            .visibility_revision
            .inspect(|| self.visible_flag.load(Ordering::SeqCst));
        let Some(id) = self.ocr.controller.request(Instant::now())? else {
            return Ok(false);
        };
        self.ocr.session = Some(LauncherSession {
            id,
            query: self.query.clone(),
            selected: self.selected,
            prior_visible,
            owned_revision,
            superseded: false,
            query_superseded: false,
            published: false,
            desktop: None,
            root_restored: false,
        });
        self.egui_ctx.request_repaint();
        Ok(true)
    }

    pub(super) fn cancel_ocr_selection(&mut self) {
        if let Some(job) = &self.ocr.job {
            job.cancel();
        }
        if let Some(id) = self.ocr.controller.cancel() {
            self.mkmacro_dialog
                .visual_overlay_controller()
                .cancel_general_ocr_operation(id);
        }
        self.egui_ctx.request_repaint();
    }

    pub(super) fn poll_ocr_selection(&mut self, ctx: &egui::Context) {
        if self.ocr.session.is_none() {
            return;
        }
        let mut lifecycle = std::mem::take(&mut self.ocr);
        let overlay = self.mkmacro_dialog.visual_overlay_controller();
        if let Some(session) = &mut lifecycle.session {
            session.superseded |= self.visibility_revision.current() != session.owned_revision;
            session.query_superseded |=
                self.query != session.query || self.selected != session.selected;
            if session.superseded || session.query_superseded {
                if let Some(job) = &lifecycle.job {
                    job.cancel();
                }
                if let Some(id) = lifecycle.controller.cancel() {
                    overlay.cancel_general_ocr_operation(id);
                }
            }
        }
        if let Some((generation, id)) = lifecycle.controller.operation()
            && let Some(event) = overlay.poll_general_ocr_rectangle_event(id)
        {
            let result = match event {
                VisualOverlayEvent::RectangleConfirmed {
                    operation_id,
                    purpose: RectanglePurpose::GeneralOcrCapture,
                    rect,
                } if operation_id == id => Ok(Some(rect)),
                VisualOverlayEvent::Cancelled { operation_id }
                | VisualOverlayEvent::Expired { operation_id }
                    if operation_id == id =>
                {
                    Ok(None)
                }
                VisualOverlayEvent::Error {
                    operation_id,
                    error,
                } if operation_id == id => Err(error.to_string()),
                _ => Err("OCR selector returned an unexpected terminal event".into()),
            };
            lifecycle.controller.terminal(generation, id, result);
        }
        match lifecycle.controller.poll(
            Instant::now(),
            self.launcher_hwnd.is_some() || lifecycle.parking.is_some(),
        ) {
            Some(SelectionEffect::Park(generation)) => {
                let result = lifecycle
                    .desktop
                    .virtual_desktop()
                    .map_err(|error| error.to_string())
                    .and_then(|desktop| {
                        desktop
                            .validate_capture()
                            .map_err(|error| error.to_string())?;
                        self.park_ocr_selection(&mut lifecycle, generation, desktop)
                    });
                lifecycle.controller.parking_applied(generation, result);
            }
            Some(SelectionEffect::Verify(generation)) => {
                let result = lifecycle
                    .parking
                    .as_ref()
                    .filter(|parking| parking.generation() == generation)
                    .ok_or_else(|| "OCR has no matching launcher parking owner".to_owned())
                    .and_then(LauncherParkingTransaction::verify);
                if lifecycle.controller.parking_verified(generation, result) {
                    let result = self.ensure_ocr_selection_admitted().and_then(|()| {
                        lifecycle
                            .session
                            .as_ref()
                            .and_then(|session| session.desktop)
                            .ok_or_else(|| "OCR selector has no desktop bounds".to_owned())
                            .and_then(|desktop| {
                                overlay
                                    .begin_general_ocr_rectangle_pick(desktop)
                                    .map_err(|error| error.to_string())
                            })
                    });
                    lifecycle.controller.selector_started(generation, result);
                }
            }
            None => {}
        }
        if let Some((generation, rect)) = lifecycle.controller.take_capture() {
            let repaint_ctx = ctx.clone();
            let repaint: Arc<dyn Fn() + Send + Sync> =
                Arc::new(move || repaint_ctx.request_repaint());
            #[cfg(test)]
            let job = if let Some(provider) = lifecycle.profile_provider {
                OcrJob::start_with_profile_provider(
                    generation,
                    rect,
                    lifecycle.desktop.clone(),
                    lifecycle.recognizer.clone(),
                    repaint,
                    provider,
                )
            } else {
                OcrJob::start(
                    generation,
                    rect,
                    lifecycle.desktop.clone(),
                    lifecycle.recognizer.clone(),
                    repaint,
                )
            };
            #[cfg(not(test))]
            let job = OcrJob::start(
                generation,
                rect,
                lifecycle.desktop.clone(),
                lifecycle.recognizer.clone(),
                repaint,
            );
            match job {
                Ok(job) => lifecycle.job = Some(job),
                Err(error) => lifecycle.controller.worker_event(OcrJobEvent {
                    generation,
                    kind: OcrJobEventKind::Finished(Err(error)),
                }),
            }
        }
        if let Some(job) = &mut lifecycle.job {
            while let Some(event) = job.poll() {
                lifecycle.controller.worker_event(event);
            }
        }
        if lifecycle.controller.restore_outcome().is_some() {
            match self.restore_ocr_selection(&mut lifecycle) {
                Ok(true) => {
                    let continuation = matches!(
                        lifecycle.controller.restore_outcome(),
                        Some(SelectionOutcome::Continue)
                    );
                    if let Some(saved) = lifecycle.session.as_mut() {
                        if !saved.superseded && !saved.query_superseded {
                            self.query = saved.query.clone();
                            self.selected = saved.selected;
                        }
                        if let Some(SelectionOutcome::Failed(error)) =
                            lifecycle.controller.restore_outcome()
                            && !saved.superseded
                        {
                            self.report_error_message("ocr.selection", error);
                        }
                        if self.visible_flag.load(Ordering::SeqCst) {
                            self.focus_input();
                        }
                        saved.root_restored = true;
                    }
                    lifecycle.controller.restored();
                    if !continuation {
                        lifecycle.session = None;
                        lifecycle.job = None;
                    }
                    lifecycle.restore_error = None;
                }
                Ok(false) => {}
                Err(error) => {
                    if lifecycle.restore_error.as_ref() != Some(&error) {
                        self.report_error_message("ocr.restore", &error);
                        lifecycle.restore_error = Some(error);
                    }
                }
            }
        }
        if !lifecycle.controller.is_active() && lifecycle.parking.is_none() {
            lifecycle.session = None;
            lifecycle.job = None;
        }
        if lifecycle.session.is_some() && lifecycle.controller.presentation().is_none() {
            ctx.request_repaint_after(if lifecycle.restore_error.is_some() {
                Duration::from_millis(250)
            } else {
                Duration::from_millis(25)
            });
        }
        self.ocr = lifecycle;
    }

    fn park_ocr_selection(
        &mut self,
        lifecycle: &mut OcrLifecycle,
        id: OcrGeneration,
        desktop: ScreenRect,
    ) -> Result<(), String> {
        self.ensure_ocr_selection_admitted()?;
        let session = lifecycle
            .session
            .as_mut()
            .filter(|session| session.id == id)
            .ok_or("OCR parking request has no current generation")?;
        let (observed, (focus, invocation)) = self.visibility_revision.inspect(|| {
            (
                self.visibility_revision.focus_intent(),
                self.visibility_revision.invocation_id(),
            )
        });
        if observed != session.owned_revision {
            session.superseded = true;
            return Err("A newer launcher visibility request cancelled OCR selection".into());
        }
        if !lifecycle
            .parking
            .as_ref()
            .is_some_and(|parking| parking.generation() == id)
        {
            let hwnd = self
                .launcher_hwnd
                .ok_or("Launcher HWND is unavailable for OCR selection")?;
            lifecycle.parking = Some(LauncherParkingTransaction::begin(id, hwnd, desktop)?);
        }
        session.desktop = Some(desktop);
        let publication = self
            .visibility_revision
            .request_if_current_with_focus_intent_and_invocation(
                observed,
                focus,
                invocation,
                || {
                    self.visible_flag.store(false, Ordering::SeqCst);
                    self.restore_flag.store(false, Ordering::SeqCst);
                },
            );
        if let Some((revision, ())) = publication {
            session.owned_revision = revision;
            self.last_visible = false;
            self.egui_ctx.request_repaint();
            Ok(())
        } else {
            session.superseded = true;
            Err("A newer launcher visibility request cancelled OCR selection".into())
        }
    }

    /// Exact native calls stay outside the ordering gate. Publication below
    /// revalidates newer requests before any presentation/activation effect.
    fn restore_ocr_selection(&mut self, lifecycle: &mut OcrLifecycle) -> Result<bool, String> {
        let continuing = matches!(
            lifecycle.controller.restore_outcome(),
            Some(SelectionOutcome::Continue)
        );
        let Some(session) = lifecycle.session.as_mut() else {
            return Ok(true);
        };
        let (observed, (focus, invocation)) = self.visibility_revision.inspect(|| {
            (
                self.visibility_revision.focus_intent(),
                self.visibility_revision.invocation_id(),
            )
        });
        session.superseded |= observed != session.owned_revision;
        // A hidden intent can still have an on-screen native snapshot while its
        // original hide is in flight. Keep the capture-safe parking on ordinary
        // cancellation rather than briefly restoring that rectangle.
        let retain_hidden_parking = !continuing && !session.prior_visible && !session.superseded;
        if !retain_hidden_parking && let Some(parking) = &mut lifecycle.parking {
            parking.restore()?;
        }
        let desired_visible = continuing || session.prior_visible;
        // A retry may change disposition after our own earlier publication
        // (for example, cancellation after activation failed). Revalidate the
        // same revision before replacing that publication with prior intent.
        if !session.superseded
            && (!session.published || self.visible_flag.load(Ordering::SeqCst) != desired_visible)
        {
            if let Some((revision, ())) = self
                .visibility_revision
                .request_if_current_with_focus_intent_and_invocation(
                    observed,
                    focus,
                    invocation,
                    || {
                        self.visible_flag.store(desired_visible, Ordering::SeqCst);
                        self.restore_flag.store(false, Ordering::SeqCst);
                    },
                )
            {
                session.owned_revision = revision;
                session.published = true;
            } else {
                session.superseded = true;
            }
        }
        let (revision, (visible, focus)) = self.visibility_revision.inspect(|| {
            (
                self.visible_flag.load(Ordering::SeqCst),
                self.visibility_revision.focus_intent(),
            )
        });
        session.superseded |= revision != session.owned_revision;
        if session.superseded || session.query_superseded {
            if let Some(job) = &lifecycle.job {
                job.cancel();
            }
            lifecycle.controller.cancel();
        }
        if !visible && let Some(parking) = &mut lifecycle.parking {
            parking.repark_after_stale_restore(|| Ok(()))?;
        }
        let root =
            RootViewportCtx::with_window_bridge(&self.egui_ctx, self.root_window_bridge.clone());
        let published = self.visibility_revision.with_current(
            revision,
            || self.visible_flag.load(Ordering::SeqCst) == visible,
            || {
                let disposition = apply_visibility_with_focus_intent(
                    visible,
                    focus,
                    if visible && !session.prior_visible {
                        VisiblePlacementPolicy::ApplyConfiguredPlacement
                    } else {
                        VisiblePlacementPolicy::PreserveCurrentGeometry
                    },
                    &root,
                    self.offscreen_pos,
                    self.follow_mouse,
                    self.static_location_enabled,
                    self.static_pos.map(|(x, y)| (x as f32, y as f32)),
                    self.static_size.map(|(w, h)| (w as f32, h as f32)),
                    (self.window_size.0 as f32, self.window_size.1 as f32),
                );
                self.restore_flag.store(false, Ordering::SeqCst);
                disposition
            },
        );
        let Some(disposition) = published else {
            self.root_window_bridge.request_presentation_reconcile();
            return Ok(false);
        };
        if disposition == RootActivationDisposition::OrderedNative {
            let target = self
                .root_window_bridge
                .activation_target()
                .ok_or("ROOT native activation target is not yet available")?;
            if !crate::window_manager::restore_launcher_to_current_desktop_ordered(
                target,
                self.visibility_revision.clone(),
                revision,
                self.visible_flag.clone(),
                self.root_window_bridge.clone(),
            ) {
                return Err("ROOT OCR restoration could not be admitted or queued".into());
            }
        }
        self.last_visible = visible;
        if (!visible || retain_hidden_parking)
            && let Some(parking) = &mut lifecycle.parking
        {
            parking.commit_hidden();
        }
        lifecycle.parking = None;
        Ok(true)
    }

    pub(super) fn reconcile_ocr_parking(&mut self) {
        if let Some(parking) = &mut self.ocr.parking
            && let Err(error) = parking.repark_after_stale_restore(|| Ok(()))
        {
            self.cancel_ocr_selection();
            self.report_error_message("ocr.parking", error);
        }
    }

    pub(super) fn shutdown_ocr_selection(&mut self) {
        self.cancel_ocr_selection();
        if let Some(parking) = &mut self.ocr.parking {
            parking.commit_hidden();
        }
        self.ocr.parking = None;
        self.ocr.job = None;
        self.ocr.session = None;
        self.ocr.controller.release();
        self.ocr.restore_error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::super::mkmacro_dialog::visual_capture_workflow::{
        SharedVisualOverlayController, TestOverlayServiceFixture,
    };
    use super::*;
    use crate::launcher_parking::{
        LauncherParkingTestObserver, LauncherWindowRect, launcher_parking_test_fixture,
    };
    use crate::mkmacro::{
        ExecResult, MkPoint,
        screen::{CapturedRegion, SearchRegion},
    };
    use crate::ocr::selection::OcrPresentation;
    use crate::plugin::PluginManager;
    use crate::settings::Settings;
    use crate::visibility::RootFocusIntent;
    use std::sync::atomic::{AtomicBool, AtomicUsize};
    use std::sync::{Condvar, Mutex};

    struct Gate {
        open: Mutex<bool>,
        changed: Condvar,
    }
    impl Gate {
        fn new(open: bool) -> Self {
            Self {
                open: Mutex::new(open),
                changed: Condvar::new(),
            }
        }
        fn wait(&self) {
            let (_guard, timeout) = self
                .changed
                .wait_timeout_while(self.open.lock().unwrap(), Duration::from_secs(5), |open| {
                    !*open
                })
                .unwrap();
            assert!(!timeout.timed_out(), "OCR integration gate stalled");
        }
        fn release(&self) {
            *self.open.lock().unwrap() = true;
            self.changed.notify_all();
        }
    }
    struct AsyncBackend {
        capture_gate: Gate,
        recognition_gate: Gate,
        rectangles: Mutex<Vec<ScreenRect>>,
        recognition_started: AtomicBool,
        fail_capture: bool,
        text: String,
    }
    impl ScreenCaptureBackend for AsyncBackend {
        fn virtual_desktop(&self) -> ExecResult<ScreenRect> {
            Ok(desktop())
        }
        fn region_bounds(&self, _: &SearchRegion) -> ExecResult<ScreenRect> {
            panic!("exact capture only")
        }
        fn capture_rect(
            &self,
            _: ScreenRect,
            _: &dyn Fn() -> bool,
        ) -> ExecResult<image::RgbaImage> {
            panic!("exact capture only")
        }
        fn capture(
            &self,
            region: &SearchRegion,
            cancelled: &dyn Fn() -> bool,
        ) -> ExecResult<CapturedRegion> {
            let SearchRegion::Rectangle { rect } = region else {
                panic!("rectangle required")
            };
            self.rectangles.lock().unwrap().push(*rect);
            self.capture_gate.wait();
            if cancelled() {
                return Err(crate::mkmacro::cancelled_error());
            }
            if self.fail_capture {
                return Err(crate::mkmacro::ExecutionDiagnostic::new(
                    crate::mkmacro::DiagnosticKind::Backend,
                    "fixture capture failed",
                ));
            }
            Ok(CapturedRegion {
                image: image::RgbaImage::new(rect.width, rect.height),
                origin: (rect.x, rect.y),
            })
        }
    }
    impl OcrBackend for AsyncBackend {
        fn available_languages(&self) -> ExecResult<Vec<crate::mkmacro::ocr::OcrLanguageInfo>> {
            Ok(vec![crate::mkmacro::ocr::OcrLanguageInfo {
                tag: "en-US".into(),
                display_name: "English".into(),
            }])
        }
        fn max_image_dimension(&self) -> ExecResult<u32> {
            Ok(1000)
        }
        fn recognize(
            &self,
            _: &image::RgbaImage,
            language: &crate::mkmacro::MkOcrLanguage,
            _: &dyn Fn() -> bool,
        ) -> ExecResult<crate::mkmacro::ocr::OcrDocument> {
            assert_eq!(
                language,
                &crate::mkmacro::MkOcrLanguage::LanguageTag("en-US".into())
            );
            self.recognition_started.store(true, Ordering::SeqCst);
            self.recognition_gate.wait();
            Ok(crate::mkmacro::ocr::OcrDocument {
                lines: vec![crate::mkmacro::ocr::OcrLine {
                    text: self.text.clone(),
                    words: vec![],
                }],
                ..Default::default()
            })
        }
    }
    fn async_backend(
        app: &mut LauncherApp,
        capture_open: bool,
        recognition_open: bool,
        text: &str,
        fail_capture: bool,
    ) -> Arc<AsyncBackend> {
        let backend = Arc::new(AsyncBackend {
            capture_gate: Gate::new(capture_open),
            recognition_gate: Gate::new(recognition_open),
            rectangles: Mutex::new(vec![]),
            recognition_started: AtomicBool::new(false),
            text: text.into(),
            fail_capture,
        });
        app.ocr.desktop = backend.clone();
        app.ocr.recognizer = backend.clone();
        app.ocr.profile_provider = Some(|| Ok(vec![]));
        backend
    }
    fn confirm(fixture: &TestOverlayServiceFixture, id: u64) {
        fixture.observer.confirm_rectangle(
            id,
            MkPoint { x: -90, y: -40 },
            MkPoint { x: 100, y: 80 },
        );
    }

    struct Desktop(Arc<AtomicUsize>);
    impl ScreenCaptureBackend for Desktop {
        fn virtual_desktop(&self) -> ExecResult<ScreenRect> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(desktop())
        }
        fn region_bounds(&self, _: &SearchRegion) -> ExecResult<ScreenRect> {
            panic!("selection must not resolve capture regions");
        }
        fn capture_rect(
            &self,
            _: ScreenRect,
            _: &dyn Fn() -> bool,
        ) -> ExecResult<image::RgbaImage> {
            panic!("selection must not capture pixels");
        }
        fn capture(&self, _: &SearchRegion, _: &dyn Fn() -> bool) -> ExecResult<CapturedRegion> {
            panic!("selection must not capture pixels");
        }
    }
    fn desktop() -> ScreenRect {
        ScreenRect::new(-100, -50, 640, 480)
    }
    fn original() -> LauncherWindowRect {
        LauncherWindowRect {
            left: -90,
            top: -40,
            right: 310,
            bottom: 160,
        }
    }
    fn app() -> LauncherApp {
        app_with_paths("actions.json".into(), "settings.json".into())
    }
    fn app_with_paths(actions_path: String, settings_path: String) -> LauncherApp {
        let ctx = egui::Context::default();
        let mut app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            actions_path,
            settings_path,
            Settings::default(),
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(true)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        // Settle constructor placement before observing this workflow's effects.
        // The native mouse position can otherwise queue startup geometry into
        // the first frame measured by a direct test executable.
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        app.last_visible = true;
        app.visibility_revision
            .request_with_focus_intent(RootFocusIntent::PreserveForeground, || {});
        app.query = "original search".into();
        app.selected = Some(2);
        app.ocr.desktop = Arc::new(Desktop(Arc::new(AtomicUsize::new(0))));
        app
    }
    fn activation_app(hidden: bool) -> (tempfile::TempDir, LauncherApp) {
        let root = tempfile::tempdir().unwrap();
        let mut app = app_with_paths(
            root.path()
                .join("actions.json")
                .to_string_lossy()
                .into_owned(),
            root.path()
                .join("settings.json")
                .to_string_lossy()
                .into_owned(),
        );
        // Retain normal activation metadata/usage while avoiding the global
        // history.json path; this is the existing explicit history test seam.
        app.test_skip_history_persistence = true;
        app.clear_query_after_run = true;
        app.hide_after_run = true;
        app.visible_flag.store(!hidden, Ordering::SeqCst);
        app.last_visible = !hidden;
        (root, app)
    }
    fn ocr_action() -> crate::actions::Action {
        use crate::plugin::Plugin;
        crate::plugins::ocr::OcrPlugin.commands().remove(0)
    }
    fn install_activation_parking(
        app: &mut LauncherApp,
        original: LauncherWindowRect,
    ) -> LauncherParkingTestObserver {
        let generation = app.ocr.session.as_ref().unwrap().id;
        let (parking, observer) = launcher_parking_test_fixture(generation, original, desktop());
        app.ocr.parking = Some(parking);
        poll(app); // publish hidden; selection may only begin on a later poll
        assert!(app.ocr.controller.operation().is_none());
        poll(app);
        observer
    }
    fn poll(app: &mut LauncherApp) {
        let ctx = app.egui_ctx.clone();
        app.poll_ocr_selection(&ctx);
    }
    fn wait(app: &mut LauncherApp, done: impl Fn(&LauncherApp) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !done(app) {
            poll(app);
            assert!(Instant::now() < deadline, "OCR fixture stalled");
            std::thread::yield_now();
        }
    }
    fn staged(app: &mut LauncherApp) -> LauncherParkingTestObserver {
        assert!(app.begin_ocr_selection().unwrap());
        let generation = app.ocr.session.as_ref().unwrap().id;
        let (parking, observer) = launcher_parking_test_fixture(generation, original(), desktop());
        app.ocr.parking = Some(parking);
        observer
    }
    fn start(
        app: &mut LauncherApp,
    ) -> (TestOverlayServiceFixture, LauncherParkingTestObserver, u64) {
        let fixture = SharedVisualOverlayController::test_fixture();
        app.mkmacro_dialog.visual_overlay = fixture.controller.clone();
        let observer = staged(app);
        poll(app);
        assert!(fixture.controller.operation_id().is_none());
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        poll(app);
        let (_, id) = app.ocr.controller.operation().unwrap();
        fixture.observer.wait_for_commands(1);
        (fixture, observer, id)
    }
    fn visibility(app: &LauncherApp, visible: bool) {
        app.visibility_revision.request_with_focus_intent(
            RootFocusIntent::PreserveForeground,
            || {
                app.visible_flag.store(visible, Ordering::SeqCst);
                app.restore_flag.store(true, Ordering::SeqCst);
            },
        );
    }
    #[test]
    fn ocr_begin_stages_without_desktop_or_capture_and_verifies_on_later_poll() {
        let mut app = app();
        let calls = Arc::new(AtomicUsize::new(0));
        app.ocr.desktop = Arc::new(Desktop(calls.clone()));
        let fixture = SharedVisualOverlayController::test_fixture();
        app.mkmacro_dialog.visual_overlay = fixture.controller.clone();
        assert!(app.begin_ocr_selection().unwrap());
        assert!(!app.begin_ocr_selection().unwrap());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        poll(&mut app); // no HWND yet
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let generation = app.ocr.session.as_ref().unwrap().id;
        let (parking, _) = launcher_parking_test_fixture(generation, original(), desktop());
        app.ocr.parking = Some(parking);
        poll(&mut app);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(fixture.controller.operation_id().is_none());
        assert!(!app.last_visible);
        poll(&mut app);
        assert!(fixture.controller.operation_id().is_some());
        app.cancel_ocr_selection();
        wait(&mut app, |app| !app.ocr_owns_root());
    }
    #[test]
    fn ocr_real_activation_from_primary_and_assigned_surfaces_preserves_prior_visibility() {
        use crate::commands::ActivationSource;
        for (source, hidden, root_policy) in [
            (
                ActivationSource::Enter,
                false,
                crate::universal_actions::RootLauncherPolicy::Legacy,
            ),
            (
                ActivationSource::Click,
                false,
                crate::universal_actions::RootLauncherPolicy::Legacy,
            ),
            (
                ActivationSource::Dashboard,
                true,
                crate::universal_actions::RootLauncherPolicy::Legacy,
            ),
            (
                ActivationSource::RadialRelease,
                true,
                crate::universal_actions::RootLauncherPolicy::PreserveOrdinaryState,
            ),
            (
                ActivationSource::RadialShortcut,
                true,
                crate::universal_actions::RootLauncherPolicy::PreserveOrdinaryState,
            ),
        ] {
            let (_root, mut app) = activation_app(hidden);
            app.command_root_policy = root_policy;
            let fixture = SharedVisualOverlayController::test_fixture();
            app.mkmacro_dialog.visual_overlay = fixture.controller.clone();
            let revision = app.visibility_revision.current();
            let action = ocr_action();
            app.activate_action(
                action.clone(),
                Some("ignored saved action query".into()),
                source,
            );
            assert!(app.ocr_owns_root(), "{source:?}");
            assert_eq!(
                app.visibility_revision.current(),
                revision,
                "no pre-show for {source:?}"
            );
            assert_eq!(app.visible_flag.load(Ordering::SeqCst), !hidden);
            assert!(app.ocr.controller.operation().is_none());
            assert!(!app.any_panel_open());
            assert_eq!(app.query, "original search");
            assert_eq!(app.selected, Some(2));
            assert_eq!(app.test_activation_trace, [(action.clone(), source)]);
            assert_eq!(app.test_recorded_history_queries, ["original search"]);
            assert_eq!(app.usage.get("ocr:start"), Some(&1));
            let saved = if hidden {
                LauncherWindowRect {
                    left: -10000,
                    top: -10000,
                    right: -9600,
                    bottom: -9800,
                }
            } else {
                original()
            };
            let observer = install_activation_parking(&mut app, saved);
            let (_, id) = app.ocr.controller.operation().unwrap();
            fixture.observer.wait_for_commands(1);
            assert!(!app.visible_flag.load(Ordering::SeqCst));
            assert!(app.ocr.controller.confirmed().is_none());
            fixture.observer.cancel_rectangle(id);
            wait(&mut app, |app| !app.ocr_owns_root());
            assert_eq!(app.visible_flag.load(Ordering::SeqCst), !hidden);
            assert_eq!(app.last_visible, !hidden);
            assert_eq!(app.query, "original search");
            assert_eq!(app.selected, Some(2));
            let restored = observer.current_rect();
            assert_eq!(restored.right - restored.left, 400);
            assert_eq!(restored.bottom - restored.top, 200);
            if hidden {
                assert!(
                    observer.restored_rects().is_empty(),
                    "hidden ROOT never restores during cancel"
                );
                assert_ne!(restored, original());
            } else {
                assert_eq!(restored, original());
            }
            assert!(app.error.is_none());
        }
    }
    #[test]
    fn ocr_hidden_pending_hide_activation_never_restores_onscreen_snapshot_or_focuses() {
        let (_root, mut app) = activation_app(true);
        app.last_visible = true; // hidden flag has arrived before native presentation
        let fixture = SharedVisualOverlayController::test_fixture();
        app.mkmacro_dialog.visual_overlay = fixture.controller.clone();
        app.activate_action(
            ocr_action(),
            None,
            crate::commands::ActivationSource::RadialShortcut,
        );
        let observer = install_activation_parking(&mut app, original());
        let parked = observer.current_rect();
        let (_, id) = app.ocr.controller.operation().unwrap();
        fixture.observer.wait_for_commands(1);
        fixture.observer.cancel_rectangle(id);
        let ctx = app.egui_ctx.clone();
        ctx.begin_frame(egui::RawInput::default());
        wait(&mut app, |app| !app.ocr_owns_root());
        let output = ctx.end_frame();
        assert!(observer.restored_rects().is_empty());
        assert_eq!(observer.current_rect(), parked);
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert!(!app.last_visible);
        assert!(
            !output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .iter()
                .any(|command| matches!(
                    command,
                    egui::ViewportCommand::Focus
                        | egui::ViewportCommand::Minimized(false)
                        | egui::ViewportCommand::Visible(true)
                ))
        );
    }
    #[test]
    fn ocr_real_duplicate_activation_during_selecting_and_restore_retains_owner_and_history() {
        let (_root, mut app) = activation_app(false);
        let fixture = SharedVisualOverlayController::test_fixture();
        app.mkmacro_dialog.visual_overlay = fixture.controller.clone();
        app.activate_action(ocr_action(), None, crate::commands::ActivationSource::Enter);
        let observer = install_activation_parking(&mut app, original());
        let current = app.ocr.controller.operation().unwrap();
        fixture.observer.wait_for_commands(1);
        app.activate_action(
            ocr_action(),
            Some("ignored duplicate".into()),
            crate::commands::ActivationSource::RadialRelease,
        );
        assert_eq!(app.ocr.controller.operation(), Some(current));
        assert_eq!(fixture.controller.operation_id(), Some(current.1));
        assert_eq!(app.test_recorded_history_queries, ["original search"]);
        assert_eq!(app.usage.get("ocr:start"), Some(&1));
        observer.fail_next_restore();
        fixture.observer.cancel_rectangle(current.1);
        wait(&mut app, |app| app.ocr.restore_error.is_some());
        let generation = app.ocr.session.as_ref().unwrap().id;
        app.activate_action(ocr_action(), None, crate::commands::ActivationSource::Click);
        assert_eq!(app.ocr.session.as_ref().unwrap().id, generation);
        assert!(app.ocr.parking.is_some());
        assert!(app.ocr.controller.restore_outcome().is_some());
        assert_eq!(app.test_recorded_history_queries, ["original search"]);
        assert_eq!(fixture.observer.commands.lock().unwrap().iter().filter(|command|matches!(command,super::super::mkmacro_dialog::visual_overlay::VisualOverlayCommand::BeginRectanglePick { .. })).count(),1);
        poll(&mut app);
        assert!(!app.ocr_owns_root());
        assert_eq!(observer.current_rect(), original());
    }
    #[test]
    fn ocr_hidden_origin_new_show_uses_configured_placement() {
        let (_root, mut app) = activation_app(true);
        let fixture = SharedVisualOverlayController::test_fixture();
        app.mkmacro_dialog.visual_overlay = fixture.controller.clone();
        app.activate_action(
            ocr_action(),
            None,
            crate::commands::ActivationSource::Dashboard,
        );
        let hidden = LauncherWindowRect {
            left: -10000,
            top: -10000,
            right: -9600,
            bottom: -9800,
        };
        let observer = install_activation_parking(&mut app, hidden);
        app.static_location_enabled = true;
        app.static_pos = Some((240, 180));
        app.static_size = Some((900, 650));
        visibility(&app, true);
        let ctx = app.egui_ctx.clone();
        ctx.begin_frame(egui::RawInput::default());
        wait(&mut app, |app| !app.ocr_owns_root());
        let output = ctx.end_frame();
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert_eq!(observer.current_rect(), hidden); // exact native restore precedes egui placement
        let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(
            commands.contains(&egui::ViewportCommand::OuterPosition(egui::pos2(
                240.0, 180.0
            )))
        );
        assert!(commands.contains(&egui::ViewportCommand::InnerSize(egui::vec2(900.0, 650.0))));
    }
    #[test]
    fn ocr_confirm_holds_signed_rectangle_and_parking_until_capture_stage_or_close() {
        let mut app = app();
        let backend = async_backend(&mut app, false, false, "text", false);
        let (fixture, observer, id) = start(&mut app);
        let parked = observer.current_rect();
        fixture.observer.confirm_rectangle(
            id,
            MkPoint { x: -90, y: -40 },
            MkPoint { x: 100, y: 80 },
        );
        wait(&mut app, |_| !backend.rectangles.lock().unwrap().is_empty());
        assert_eq!(
            *backend.rectangles.lock().unwrap(),
            vec![ScreenRect::new(-90, -40, 190, 120)]
        );
        assert!(app.ocr_owns_root());
        assert_eq!(observer.current_rect(), parked);
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert!(fixture.controller.operation_id().is_none());
        assert!(!app.begin_ocr_selection().unwrap());
        app.cancel_ocr_selection();
        poll(&mut app);
        assert!(app.ocr_owns_root()); // cancellation waits for capture to stop
        assert_eq!(observer.current_rect(), parked);
        backend.capture_gate.release();
        wait(&mut app, |app| !app.ocr_owns_root());
        assert_eq!(observer.current_rect(), original());
        assert_eq!(app.query, "original search");
    }
    #[test]
    fn ocr_escape_and_explicit_cancel_restore_silently_after_terminal_ack() {
        for explicit in [false, true] {
            let mut app = app();
            let (fixture, observer, id) = start(&mut app);
            if explicit {
                app.cancel_ocr_selection();
                assert!(fixture.controller.operation_id().is_none());
                assert!(app.ocr.controller.restore_outcome().is_none());
                assert!(app.ocr_owns_root());
                assert!(!app.visible_flag.load(Ordering::SeqCst));
            } else {
                fixture.observer.cancel_rectangle(id);
            }
            wait(&mut app, |app| !app.ocr_owns_root());
            assert_eq!(observer.current_rect(), original());
            assert_eq!(app.query, "original search");
            assert_eq!(app.selected, Some(2));
            assert!(app.error.is_none());
            assert!(app.visible_flag.load(Ordering::SeqCst));
        }
    }
    #[test]
    fn ocr_selector_start_error_restores_and_releases_ownership() {
        let mut app = app();
        app.mkmacro_dialog.visual_overlay =
            SharedVisualOverlayController::new_with_controller_factory(|| {
                Err(std::io::Error::other("fixture startup error"))
            });
        let observer = staged(&mut app);
        poll(&mut app);
        poll(&mut app);
        wait(&mut app, |app| !app.ocr_owns_root());
        assert_eq!(observer.current_rect(), original());
        assert!(
            app.error
                .as_ref()
                .unwrap()
                .contains("fixture startup error")
        );
    }
    #[test]
    fn ocr_restore_failure_retains_transaction_and_retries() {
        let mut app = app();
        let (fixture, observer, id) = start(&mut app);
        observer.fail_next_restore();
        fixture.observer.cancel_rectangle(id);
        wait(&mut app, |app| app.ocr.restore_error.is_some());
        assert!(app.ocr.parking.is_some());
        assert!(app.ocr_owns_root());
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        poll(&mut app);
        assert!(!app.ocr_owns_root());
        assert_eq!(observer.current_rect(), original());
    }
    #[test]
    fn ocr_new_visibility_preserves_latest_query_selection_and_geometry() {
        for show in [false, true] {
            let mut app = app();
            let (_fixture, observer, _id) = start(&mut app);
            let parked = observer.current_rect();
            visibility(&app, show);
            app.query = "newer query".into();
            app.selected = Some(1);
            let revision = app.visibility_revision.current();
            wait(&mut app, |app| !app.ocr_owns_root());
            assert_eq!(app.query, "newer query");
            assert_eq!(app.selected, Some(1));
            assert_eq!(app.visibility_revision.current(), revision);
            assert_eq!(app.visible_flag.load(Ordering::SeqCst), show);
            assert_eq!(app.last_visible, show);
            assert_eq!(
                observer.current_rect(),
                if show { original() } else { parked }
            );
            assert!(app.error.is_none());
        }
    }
    #[test]
    fn ocr_visibility_race_during_native_restore_obeys_latest_intent() {
        for show in [false, true] {
            let mut app = app();
            let (fixture, observer, id) = start(&mut app);
            let parked = observer.current_rect();
            let revision = app.visibility_revision.clone();
            let visible = app.visible_flag.clone();
            observer.before_next_restore(move || {
                revision.request_with_focus_intent(RootFocusIntent::PreserveForeground, || {
                    visible.store(show, Ordering::SeqCst)
                });
            });
            fixture.observer.cancel_rectangle(id);
            wait(&mut app, |app| !app.ocr_owns_root());
            assert_eq!(app.visible_flag.load(Ordering::SeqCst), show);
            assert_eq!(
                observer.current_rect(),
                if show { original() } else { parked }
            );
        }
    }
    #[test]
    fn ocr_new_query_intent_cancels_without_restoring_stale_query() {
        let mut app = app();
        let (_fixture, observer, _id) = start(&mut app);
        app.query = "newer query without visibility request".into();
        app.selected = Some(1);
        wait(&mut app, |app| !app.ocr_owns_root());
        assert_eq!(app.query, "newer query without visibility request");
        assert_eq!(app.selected, Some(1));
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert_eq!(observer.current_rect(), original());
    }
    #[test]
    fn ocr_conflicts_reject_before_mutation_and_block_other_root_owners() {
        let mut app = app();
        app.screen_draw_controller.request_start().unwrap();
        let revision = app.visibility_revision.current();
        assert!(
            app.begin_ocr_selection()
                .unwrap_err()
                .contains("Screen Draw")
        );
        assert!(!app.ocr_owns_root());
        assert_eq!(app.visibility_revision.current(), revision);
        app.screen_draw_controller.close();
        assert!(app.begin_color_pick().unwrap());
        assert!(
            app.begin_ocr_selection()
                .unwrap_err()
                .contains("color picker")
        );
        assert!(!app.ocr_owns_root());
        app.shutdown_color_pick();
        app.color_pick.controller = crate::color_pick::ColorPickController::default();
        let fixture = SharedVisualOverlayController::test_fixture();
        app.mkmacro_dialog.visual_overlay = fixture.controller.clone();
        let other = fixture
            .controller
            .begin_rectangle_pick(RectanglePurpose::SearchRegion, desktop());
        assert!(
            app.begin_ocr_selection()
                .unwrap_err()
                .contains("active screen selection")
        );
        assert_eq!(fixture.controller.operation_id(), Some(other));
        fixture.controller.cancel_operation(other);
        assert!(app.begin_ocr_selection().unwrap());
        assert!(app.begin_color_pick().is_err());
        assert!(app.start_or_focus_screen_draw().is_err());
        assert!(app.request_new_screen_draw_capture().is_err());
        assert!(app.resume_screen_draw().is_err());
        app.shutdown_ocr_selection();
    }
    #[test]
    fn ocr_shutdown_keeps_launcher_parked_and_leaves_unrelated_overlay_alive() {
        let mut app = app();
        let (fixture, observer, _id) = start(&mut app);
        let parked = observer.current_rect();
        let unrelated = fixture
            .controller
            .begin_rectangle_pick(RectanglePurpose::SearchRegion, desktop());
        app.shutdown_ocr_selection();
        assert!(!app.ocr_owns_root());
        assert_eq!(fixture.controller.operation_id(), Some(unrelated));
        assert_eq!(observer.current_rect(), parked);
        assert!(!app.visible_flag.load(Ordering::SeqCst));
    }
    #[test]
    fn ocr_confirmed_root_suppresses_generic_placement_and_reconciles_native_parking() {
        let mut app = app();
        let backend = async_backend(&mut app, false, false, "text", false);
        let (fixture, observer, id) = start(&mut app);
        fixture.observer.confirm_rectangle(
            id,
            MkPoint { x: -90, y: -40 },
            MkPoint { x: 100, y: 80 },
        );
        wait(&mut app, |_| !backend.rectangles.lock().unwrap().is_empty());
        let parked = observer.current_rect();
        app.static_location_enabled = true;
        app.static_pos = Some((240, 180));
        app.static_size = Some((900, 650));
        observer.set_current_rect(original());
        app.root_window_bridge.request_presentation_reconcile();
        let ctx = app.egui_ctx.clone();
        let output = ctx.run(egui::RawInput::default(), |root| {
            app.render_root_frame(root, None)
        });
        assert!(app.ocr_owns_root());
        assert_eq!(observer.current_rect(), parked);
        let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(
            !commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::OuterPosition(_) | egui::ViewportCommand::InnerSize(_)
            )),
            "OCR parking emitted geometry commands: {commands:?}"
        );
        app.cancel_ocr_selection();
        backend.capture_gate.release();
        wait(&mut app, |app| !app.ocr_owns_root());
    }
    #[cfg(windows)]
    #[test]
    fn ocr_restore_uses_ordered_root_activation_with_exact_geometry() {
        let mut app = app();
        app.visibility_revision
            .request_with_focus_intent(RootFocusIntent::ActivateRoot, || {});
        app.root_window_bridge.set_identity_for_test(42);
        app.root_window_bridge
            .qualify_simulated_wake_for_test()
            .unwrap();
        app.static_location_enabled = true;
        app.static_pos = Some((240, 180));
        app.static_size = Some((900, 650));
        let (fixture, observer, id) = start(&mut app);
        fixture.observer.cancel_rectangle(id);
        let ctx = app.egui_ctx.clone();
        ctx.begin_frame(egui::RawInput::default());
        let (_, queued) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            wait(&mut app, |app| !app.ocr_owns_root())
        });
        let output = ctx.end_frame();
        assert_eq!(queued, [(app.visibility_revision.current(), 42)]);
        assert_eq!(observer.current_rect(), original());
        let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(
            !commands.iter().any(|command| matches!(
                command,
                egui::ViewportCommand::OuterPosition(_) | egui::ViewportCommand::InnerSize(_)
            )),
            "OCR exact restore emitted geometry commands: {commands:?}"
        );
    }

    #[test]
    fn ocr_capture_restores_before_recognition_and_close_detaches_old_generation() {
        let mut app = app();
        let backend = async_backend(&mut app, false, false, "late old text", false);
        let (fixture, observer, id) = start(&mut app);
        let generation = app.ocr.session.as_ref().unwrap().id;
        confirm(&fixture, id);
        wait(&mut app, |_| !backend.rectangles.lock().unwrap().is_empty());
        for _ in 0..3 {
            poll(&mut app);
        }
        assert!(app.ocr_owns_root());
        assert!(!backend.recognition_started.load(Ordering::SeqCst));
        assert_eq!(
            *backend.rectangles.lock().unwrap(),
            vec![ScreenRect::new(-90, -40, 190, 120)]
        );
        backend.capture_gate.release();
        wait(&mut app, |app| {
            backend.recognition_started.load(Ordering::SeqCst)
                && matches!(
                    app.ocr.controller.presentation(),
                    Some(OcrPresentation::Recognizing)
                )
        });
        assert!(!app.ocr_owns_root());
        assert!(app.ocr_surface_visible());
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert_eq!(observer.current_rect(), original());
        assert!(!app.begin_ocr_selection().unwrap());
        let closed = Instant::now();
        app.cancel_ocr_selection();
        poll(&mut app);
        assert!(closed.elapsed() < Duration::from_secs(1));
        assert!(app.ocr.session.is_none());
        let (_next_fixture, _next_observer, _next_id) = start(&mut app);
        let next_generation = app.ocr.session.as_ref().unwrap().id;
        assert_ne!(generation, next_generation);
        backend.recognition_gate.release();
        for _ in 0..5 {
            poll(&mut app);
        }
        assert_eq!(app.ocr.session.as_ref().unwrap().id, next_generation);
        assert!(app.ocr.controller.presentation().is_none());
        app.shutdown_ocr_selection();
    }

    #[test]
    fn ocr_fast_text_empty_and_capture_error_survive_restoration_retry() {
        for (text, fail_capture) in [("fast text", false), (" \n ", false), ("", true)] {
            let mut app = app();
            let backend = async_backend(&mut app, true, true, text, fail_capture);
            let (fixture, observer, id) = start(&mut app);
            observer.fail_next_restore();
            confirm(&fixture, id);
            wait(&mut app, |app| app.ocr.restore_error.is_some());
            assert!(app.ocr_owns_root());
            assert!(app.ocr.parking.is_some());
            assert!(app.ocr.controller.presentation().is_none());
            wait(&mut app, |app| {
                matches!(
                    app.ocr.controller.presentation(),
                    Some(
                        OcrPresentation::Result(_)
                            | OcrPresentation::NoText
                            | OcrPresentation::Error(_)
                    )
                )
            });
            match app.ocr.controller.presentation().unwrap() {
                OcrPresentation::Result(actual) => assert_eq!(actual, text),
                OcrPresentation::NoText => assert!(text.trim().is_empty() && !fail_capture),
                OcrPresentation::Error(error) => {
                    assert!(fail_capture && error.to_string().contains("fixture capture failed"))
                }
                _ => panic!("terminal expected"),
            }
            assert_eq!(backend.rectangles.lock().unwrap().len(), 1);
            assert_eq!(observer.current_rect(), original());
            assert!(app.visible_flag.load(Ordering::SeqCst));
            assert!(!app.ocr_owns_root());
        }
    }

    #[test]
    fn ocr_hidden_capture_success_publishes_configured_visible_placement() {
        let (_root, mut app) = activation_app(true);
        let backend = async_backend(&mut app, true, false, "text", false);
        let fixture = SharedVisualOverlayController::test_fixture();
        app.mkmacro_dialog.visual_overlay = fixture.controller.clone();
        app.begin_ocr_selection().unwrap();
        let observer = install_activation_parking(&mut app, original());
        let (_, id) = app.ocr.controller.operation().unwrap();
        fixture.observer.wait_for_commands(1);
        app.static_location_enabled = true;
        app.static_pos = Some((240, 180));
        app.static_size = Some((900, 650));
        confirm(&fixture, id);
        let ctx = app.egui_ctx.clone();
        ctx.begin_frame(egui::RawInput::default());
        wait(&mut app, |app| {
            matches!(
                app.ocr.controller.presentation(),
                Some(OcrPresentation::Recognizing)
            )
        });
        let output = ctx.end_frame();
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(!app.ocr_owns_root());
        assert_eq!(observer.current_rect(), original());
        let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(
            commands.contains(&egui::ViewportCommand::OuterPosition(egui::pos2(
                240., 180.
            )))
        );
        assert!(commands.contains(&egui::ViewportCommand::InnerSize(egui::vec2(900., 650.))));
        app.cancel_ocr_selection();
        backend.recognition_gate.release();
        poll(&mut app);
    }

    #[test]
    fn ocr_new_query_or_visibility_during_capture_and_recognition_invalidates_completion() {
        for during_capture in [true, false] {
            for new_visibility in [true, false] {
                let mut app = app();
                let backend = async_backend(&mut app, false, false, "stale", false);
                let (fixture, observer, id) = start(&mut app);
                confirm(&fixture, id);
                wait(&mut app, |_| !backend.rectangles.lock().unwrap().is_empty());
                if !during_capture {
                    backend.capture_gate.release();
                    wait(&mut app, |app| {
                        matches!(
                            app.ocr.controller.presentation(),
                            Some(OcrPresentation::Recognizing)
                        )
                    });
                }
                app.query = "new query".into();
                app.selected = Some(1);
                if new_visibility {
                    visibility(&app, false);
                }
                let revision = app.visibility_revision.current();
                poll(&mut app);
                if during_capture {
                    assert!(app.ocr_owns_root());
                }
                backend.capture_gate.release();
                backend.recognition_gate.release();
                wait(&mut app, |app| app.ocr.session.is_none());
                assert_eq!(app.query, "new query");
                assert_eq!(app.selected, Some(1));
                if new_visibility {
                    assert_eq!(app.visibility_revision.current(), revision);
                    assert!(!app.visible_flag.load(Ordering::SeqCst));
                } else {
                    assert!(app.visible_flag.load(Ordering::SeqCst));
                    assert_eq!(observer.current_rect(), original());
                }
                assert!(app.ocr.controller.presentation().is_none());
                assert!(app.error.is_none());
            }
        }
    }

    #[test]
    fn ocr_success_restore_racing_new_hidden_intent_never_publishes_result() {
        let mut app = app();
        let backend = async_backend(&mut app, true, true, "obsolete", false);
        let (fixture, observer, id) = start(&mut app);
        let parked = observer.current_rect();
        let revision = app.visibility_revision.clone();
        let visible = app.visible_flag.clone();
        observer.before_next_restore(move || {
            revision.request_with_focus_intent(RootFocusIntent::PreserveForeground, || {
                visible.store(false, Ordering::SeqCst)
            });
        });
        confirm(&fixture, id);
        wait(&mut app, |app| app.ocr.session.is_none());
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert_eq!(observer.current_rect(), parked);
        assert!(app.ocr.controller.presentation().is_none());
        assert_eq!(backend.rectangles.lock().unwrap().len(), 1);
    }

    #[test]
    fn ocr_superseded_session_leaves_escape_for_newer_ui() {
        for newer_visibility in [true, false] {
            let mut app = app();
            let (_fixture, _observer, _id) = start(&mut app);
            if newer_visibility {
                visibility(&app, true);
            } else {
                app.query = "newer UI".into();
            }
            let ctx = app.egui_ctx.clone();
            ctx.begin_frame(egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            });
            poll(&mut app);
            assert!(ctx.input(|input| input.key_pressed(egui::Key::Escape)));
            let _ = ctx.end_frame();
            wait(&mut app, |app| !app.ocr_owns_root());
        }
    }

    #[cfg(windows)]
    #[test]
    fn ocr_hidden_origin_cancel_after_published_restore_retry_reinstates_hidden_intent() {
        let (_root, mut app) = activation_app(true);
        // Without a ROOT identity, ordered activation fails only after the
        // native snapshot and visible publication have been restored.
        app.visibility_revision
            .request_with_focus_intent(RootFocusIntent::ActivateRoot, || {});
        let backend = async_backend(&mut app, true, false, "obsolete", false);
        let fixture = SharedVisualOverlayController::test_fixture();
        app.mkmacro_dialog.visual_overlay = fixture.controller.clone();
        assert!(app.begin_ocr_selection().unwrap());
        let observer = install_activation_parking(&mut app, original());
        let parked = observer.current_rect();
        let (_, id) = app.ocr.controller.operation().unwrap();
        fixture.observer.wait_for_commands(1);
        confirm(&fixture, id);
        wait(&mut app, |app| app.ocr.restore_error.is_some());
        assert!(app.ocr.session.as_ref().unwrap().published);
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(app.ocr.parking.is_some());
        assert_eq!(observer.current_rect(), original());
        app.cancel_ocr_selection();
        wait(&mut app, |app| !app.ocr_owns_root());
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert!(!app.last_visible);
        assert_eq!(observer.current_rect(), parked);
        assert_eq!(app.query, "original search");
        assert!(app.ocr.session.is_none());
        assert!(app.ocr.controller.presentation().is_none());
        backend.recognition_gate.release();
    }
}
