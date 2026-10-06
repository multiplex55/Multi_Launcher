use eframe::egui;
use std::sync::{Arc, atomic::Ordering};
use std::time::{Duration, Instant};

use super::LauncherApp;
use crate::color_pick::{
    ColorPickController, ColorPickEvent, ColorPickOutcome, ColorPickSessionId,
};
use crate::launcher_parking::LauncherParkingTransaction;
use crate::mkmacro::screen::ScreenRect;
use crate::visibility::{
    RootActivationDisposition, RootViewportCtx, VisiblePlacementPolicy,
    apply_visibility_with_focus_intent,
};

#[derive(Default)]
pub(super) struct ColorPickLifecycle {
    pub(super) controller: ColorPickController,
    session: Option<LauncherSession>,
    pub(super) parking: Option<LauncherParkingTransaction<ColorPickSessionId>>,
    pending: Option<ColorPickOutcome>,
    restore_error: Option<String>,
}
struct LauncherSession {
    id: ColorPickSessionId,
    query: String,
    selected: Option<usize>,
    owned_revision: u64,
    superseded: bool,
    published: bool,
}

impl LauncherApp {
    pub(super) fn color_pick_owns_root(&self) -> bool {
        self.color_pick.session.is_some() || self.color_pick.parking.is_some()
    }

    pub(super) fn ensure_color_pick_does_not_own_root(&self) -> Result<(), String> {
        if self.color_pick_owns_root() {
            Err("Finish or cancel the screen color picker before starting Screen Draw".into())
        } else if self.ocr_owns_root() {
            Err("Finish or cancel Screen Region OCR before starting Screen Draw".into())
        } else {
            Ok(())
        }
    }

    pub(super) fn begin_color_pick(&mut self) -> Result<bool, String> {
        if self.color_pick_owns_root() || self.color_pick.controller.is_active() {
            return Ok(false);
        }
        if self.ocr_owns_root() {
            return Err("Finish or cancel Screen Region OCR before picking a screen color".into());
        }
        if self.screen_draw_launcher_parking.is_some()
            || !matches!(
                self.screen_draw_controller.state(),
                crate::screen_draw::ScreenDrawState::NoSession
                    | crate::screen_draw::ScreenDrawState::Failed { .. }
            )
        {
            return Err(
                "Close the active Screen Draw session before picking a screen color".into(),
            );
        }
        if !self.visible_flag.load(Ordering::SeqCst) || !self.last_visible {
            return Err("Show the launcher before picking a screen color".into());
        }
        let revision = self.visibility_revision.current();
        let Some(id) = self.color_pick.controller.request(Instant::now())? else {
            return Ok(false);
        };
        self.color_pick.session = Some(LauncherSession {
            id,
            query: self.query.clone(),
            selected: self.selected,
            owned_revision: revision,
            superseded: false,
            published: false,
        });
        self.egui_ctx.request_repaint();
        Ok(true)
    }

    pub(super) fn poll_color_pick(&mut self, ctx: &egui::Context) {
        if !self.color_pick_owns_root() && !self.color_pick.controller.is_active() {
            return;
        }
        let mut lifecycle = std::mem::take(&mut self.color_pick);
        let repaint = ctx.clone();
        lifecycle.controller.set_repaint(Arc::new(move || {
            repaint.request_repaint_of(egui::ViewportId::ROOT)
        }));
        if let Some(session) = &mut lifecycle.session
            && self.visibility_revision.current() != session.owned_revision
        {
            session.superseded = true;
            lifecycle.controller.cancel();
        }
        let now = Instant::now();
        for event in lifecycle.controller.poll(now) {
            match event {
                ColorPickEvent::ParkLauncher { session, desktop } => {
                    let result = self.park_color_pick(&mut lifecycle, session, desktop);
                    lifecycle.controller.parking_applied(session, result);
                }
                ColorPickEvent::VerifyParking { session } => {
                    let result = lifecycle
                        .parking
                        .as_ref()
                        .filter(|parking| parking.generation() == session)
                        .ok_or_else(|| {
                            "Color picker has no matching launcher parking owner".to_owned()
                        })
                        .and_then(LauncherParkingTransaction::verify);
                    lifecycle.controller.parking_verified(session, result, now);
                }
                ColorPickEvent::Finished { session, outcome } => {
                    if let Some(saved) = &lifecycle.session
                        && saved.id == session
                    {
                        lifecycle.pending = Some(if saved.superseded {
                            ColorPickOutcome::Cancelled
                        } else {
                            outcome
                        });
                    }
                }
            }
        }
        if lifecycle.pending.is_some() {
            match self.restore_color_pick(&mut lifecycle) {
                Ok(true) => {
                    if let Some(saved) = lifecycle.session.take() {
                        let outcome = lifecycle
                            .pending
                            .take()
                            .unwrap_or(ColorPickOutcome::Cancelled);
                        match outcome {
                            ColorPickOutcome::Picked(color) if !saved.superseded => {
                                self.query = format!("color {}", color.hex());
                                self.last_results_valid = false;
                                self.search();
                                self.move_cursor_end = true;
                            }
                            outcome => {
                                if !saved.superseded {
                                    self.query = saved.query;
                                    self.selected = saved.selected;
                                }
                                if let ColorPickOutcome::Failed(error) = outcome {
                                    self.report_error_message("color_pick", error);
                                }
                            }
                        }
                        if self.visible_flag.load(Ordering::SeqCst) {
                            self.focus_input();
                        }
                    }
                    lifecycle.restore_error = None;
                }
                Ok(false) => {}
                Err(error) => {
                    if lifecycle.restore_error.as_ref() != Some(&error) {
                        self.report_error_message("color_pick.restore", &error);
                        lifecycle.restore_error = Some(error);
                    }
                }
            }
        }
        if lifecycle.session.is_some() || lifecycle.controller.is_active() {
            ctx.request_repaint_after(if lifecycle.restore_error.is_some() {
                Duration::from_millis(250)
            } else {
                Duration::from_millis(25)
            });
        }
        self.color_pick = lifecycle;
    }

    fn park_color_pick(
        &mut self,
        lifecycle: &mut ColorPickLifecycle,
        id: ColorPickSessionId,
        desktop: ScreenRect,
    ) -> Result<(), String> {
        let session = lifecycle
            .session
            .as_mut()
            .filter(|session| session.id == id)
            .ok_or("Color picker parking request has no current session")?;
        if self.ocr_owns_root() {
            return Err("Screen Region OCR owns the launcher capture lifecycle".into());
        }
        if self.screen_draw_launcher_parking.is_some()
            || !matches!(
                self.screen_draw_controller.state(),
                crate::screen_draw::ScreenDrawState::NoSession
                    | crate::screen_draw::ScreenDrawState::Failed { .. }
            )
        {
            return Err("Screen Draw owns the launcher capture lifecycle".into());
        }
        let (observed, (focus, invocation)) = self.visibility_revision.inspect(|| {
            (
                self.visibility_revision.focus_intent(),
                self.visibility_revision.invocation_id(),
            )
        });
        if observed != session.owned_revision {
            session.superseded = true;
            return Err("A newer launcher visibility request cancelled screen picking".into());
        }
        if !lifecycle
            .parking
            .as_ref()
            .is_some_and(|parking| parking.generation() == id)
        {
            let hwnd = self
                .launcher_hwnd
                .ok_or("Launcher HWND is unavailable for screen color picking")?;
            lifecycle.parking = Some(LauncherParkingTransaction::begin(id, hwnd, desktop)?);
        }
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
            Err("A newer launcher visibility request cancelled screen picking".into())
        }
    }

    /// Exact native calls stay outside the ordering gate. Publication below
    /// revalidates newer requests before any presentation/activation effect.
    fn restore_color_pick(&mut self, lifecycle: &mut ColorPickLifecycle) -> Result<bool, String> {
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
        if let Some(parking) = &mut lifecycle.parking {
            parking.restore()?;
        }
        if !session.superseded && !session.published {
            if let Some((revision, ())) = self
                .visibility_revision
                .request_if_current_with_focus_intent_and_invocation(
                    observed,
                    focus,
                    invocation,
                    || {
                        self.visible_flag.store(true, Ordering::SeqCst);
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
                    VisiblePlacementPolicy::PreserveCurrentGeometry,
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
                return Err("ROOT color picker restoration could not be admitted or queued".into());
            }
        }
        self.last_visible = visible;
        if !visible && let Some(parking) = &mut lifecycle.parking {
            parking.commit_hidden();
        }
        lifecycle.parking = None;
        Ok(true)
    }

    pub(super) fn reconcile_color_pick_parking(&mut self) {
        if let Some(parking) = &mut self.color_pick.parking
            && let Err(error) = parking.repark_after_stale_restore(|| Ok(()))
        {
            self.color_pick.controller.cancel();
            self.report_error_message("color_pick.parking", error);
        }
    }

    pub(super) fn shutdown_color_pick(&mut self) {
        self.color_pick.controller.cancel();
        if let Some(parking) = &mut self.color_pick.parking {
            parking.commit_hidden();
        }
        self.color_pick.parking = None;
        self.color_pick.session = None;
        self.color_pick.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::RgbColor;
    use crate::color_pick::native::{NativePickerFactory, NativePickerHandle};
    use crate::launcher_parking::{
        LauncherParkingTestObserver, LauncherWindowRect, launcher_parking_test_fixture,
    };
    use crate::mkmacro::{
        ExecResult,
        screen::{CapturedRegion, ScreenCaptureBackend, SearchRegion},
    };
    use crate::plugin::PluginManager;
    use crate::plugins::color_picker::ColorPickerPlugin;
    use crate::settings::Settings;
    use crate::visibility::RootFocusIntent;
    use std::sync::{Mutex, atomic::AtomicBool, mpsc};

    struct Capture;
    impl ScreenCaptureBackend for Capture {
        fn virtual_desktop(&self) -> ExecResult<ScreenRect> {
            Ok(ScreenRect::new(-100, -50, 640, 480))
        }
        fn region_bounds(&self, _: &SearchRegion) -> ExecResult<ScreenRect> {
            self.virtual_desktop()
        }
        fn capture_rect(
            &self,
            _: ScreenRect,
            _: &dyn Fn() -> bool,
        ) -> ExecResult<image::RgbaImage> {
            unreachable!()
        }
        fn capture(&self, _: &SearchRegion, _: &dyn Fn() -> bool) -> ExecResult<CapturedRegion> {
            Ok(CapturedRegion {
                image: image::RgbaImage::from_pixel(640, 480, image::Rgba([12, 34, 56, 255])),
                origin: (-100, -50),
            })
        }
    }
    struct Native {
        result: Mutex<mpsc::Receiver<ColorPickOutcome>>,
        started: Arc<AtomicBool>,
        cleaned: Arc<AtomicBool>,
        release_cleanup: Arc<AtomicBool>,
    }
    impl NativePickerFactory for Native {
        fn spawn(
            &self,
            _: Arc<CapturedRegion>,
            repaint: Arc<dyn Fn() + Send + Sync>,
        ) -> Result<NativePickerHandle, String> {
            // One receiver belongs to one worker; tests create a fresh factory on reopening.
            let (_, empty) = mpsc::channel();
            let result = std::mem::replace(&mut *self.result.lock().unwrap(), empty);
            let started = self.started.clone();
            let cleaned = self.cleaned.clone();
            let release_cleanup = self.release_cleanup.clone();
            NativePickerHandle::spawn_worker(repaint, move |cancel, ready| {
                ready.store(true, Ordering::Release);
                started.store(true, Ordering::Release);
                let outcome = loop {
                    if cancel.load(Ordering::Acquire) {
                        break ColorPickOutcome::Cancelled;
                    }
                    match result.try_recv() {
                        Ok(value) => break value,
                        Err(mpsc::TryRecvError::Empty) => std::thread::yield_now(),
                        Err(_) => break ColorPickOutcome::Cancelled,
                    }
                };
                while !release_cleanup.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
                cleaned.store(true, Ordering::Release);
                outcome
            })
        }
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
        let ctx = egui::Context::default();
        let mut plugins = PluginManager::new();
        plugins.register(Box::new(ColorPickerPlugin::default()));
        let mut app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            plugins,
            "actions.json".into(),
            "settings.json".into(),
            Settings::default(),
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(true)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        app.last_visible = true;
        app.visibility_revision
            .request_with_focus_intent(RootFocusIntent::PreserveForeground, || {});
        app.query = "keep original query".into();
        app.selected = Some(2);
        app
    }
    fn start_with_cleanup_gate(
        app: &mut LauncherApp,
        release_cleanup: Arc<AtomicBool>,
    ) -> (
        mpsc::Sender<ColorPickOutcome>,
        Arc<AtomicBool>,
        LauncherParkingTestObserver,
    ) {
        let (tx, rx) = mpsc::channel();
        let started = Arc::new(AtomicBool::new(false));
        let cleaned = Arc::new(AtomicBool::new(false));
        app.color_pick.controller = ColorPickController::with_backends(
            Arc::new(Capture),
            Arc::new(Native {
                result: Mutex::new(rx),
                started: started.clone(),
                cleaned: cleaned.clone(),
                release_cleanup,
            }),
            Arc::new(|| {}),
        );
        assert!(app.begin_color_pick().unwrap());
        assert!(!app.begin_color_pick().unwrap());
        let id = app.color_pick.session.as_ref().unwrap().id;
        let (parking, observer) =
            launcher_parking_test_fixture(id, original(), ScreenRect::new(-100, -50, 640, 480));
        app.color_pick.parking = Some(parking);
        poll(app);
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert!(!app.last_visible);
        wait(app, |app| {
            app.color_pick.controller.is_active() && started.load(Ordering::Acquire)
        });
        (tx, cleaned, observer)
    }
    fn start(
        app: &mut LauncherApp,
    ) -> (
        mpsc::Sender<ColorPickOutcome>,
        Arc<AtomicBool>,
        LauncherParkingTestObserver,
    ) {
        start_with_cleanup_gate(app, Arc::new(AtomicBool::new(true)))
    }
    fn poll(app: &mut LauncherApp) {
        let ctx = app.egui_ctx.clone();
        app.poll_color_pick(&ctx);
    }
    fn wait(app: &mut LauncherApp, done: impl Fn(&LauncherApp) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !done(app) {
            poll(app);
            assert!(Instant::now() < deadline, "picker fixture stalled");
            std::thread::yield_now();
        }
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
    fn color_pick_picked_uses_existing_results_after_cleanup_and_exact_restore() {
        let mut app = app();
        let (tx, cleaned, observer) = start(&mut app);
        tx.send(ColorPickOutcome::Picked(RgbColor::new(12, 34, 56)))
            .unwrap();
        wait(&mut app, |app| !app.color_pick_owns_root());
        assert!(cleaned.load(Ordering::Acquire));
        assert_eq!(observer.current_rect(), original());
        assert_eq!(app.query, "color #0c2238");
        let expected = crate::plugins::color_picker::color_actions(RgbColor::new(12, 34, 56));
        for expected in expected {
            assert!(
                app.results
                    .iter()
                    .any(|result| result.action == expected.action)
            );
        }
        assert!(app.visible_flag.load(Ordering::SeqCst) && app.last_visible);
        assert!(!app.restore_flag.load(Ordering::SeqCst));
    }
    #[test]
    fn color_pick_cancel_failure_preserve_query_selection_geometry_and_allow_reopen() {
        for outcome in [
            ColorPickOutcome::Cancelled,
            ColorPickOutcome::Failed("fixture failure".into()),
        ] {
            let mut app = app();
            let (tx, cleaned, observer) = start(&mut app);
            tx.send(outcome).unwrap();
            wait(&mut app, |app| !app.color_pick_owns_root());
            assert!(cleaned.load(Ordering::Acquire));
            assert_eq!(observer.current_rect(), original());
            assert_eq!(app.query, "keep original query");
            assert_eq!(app.selected, Some(2));
            let (_tx, _, _observer) = start(&mut app);
            app.color_pick.controller.cancel();
            wait(&mut app, |app| !app.color_pick_owns_root());
        }
    }
    #[test]
    fn color_pick_restore_failure_retains_transaction_and_outcome_until_retry() {
        let mut app = app();
        let (tx, cleaned, observer) = start(&mut app);
        observer.fail_next_restore();
        tx.send(ColorPickOutcome::Picked(RgbColor::new(1, 2, 3)))
            .unwrap();
        wait(&mut app, |app| app.color_pick.restore_error.is_some());
        assert!(cleaned.load(Ordering::Acquire));
        assert!(app.color_pick.pending.is_some());
        assert!(app.color_pick.parking.is_some());
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert_eq!(app.query, "keep original query");
        poll(&mut app);
        assert!(!app.color_pick_owns_root());
        assert_eq!(app.query, "color #010203");
        assert_eq!(observer.current_rect(), original());
    }
    #[test]
    fn color_pick_new_visibility_cancels_preserves_new_query_and_native_placement() {
        for visible in [false, true] {
            let mut app = app();
            let (_tx, cleaned, observer) = start(&mut app);
            let parked = observer.current_rect();
            visibility(&app, visible);
            app.query = "newer query".into();
            app.selected = Some(1);
            let revision = app.visibility_revision.current();
            wait(&mut app, |app| !app.color_pick_owns_root());
            assert!(cleaned.load(Ordering::Acquire));
            assert_eq!(app.query, "newer query");
            assert_eq!(app.selected, Some(1));
            assert_eq!(app.visibility_revision.current(), revision);
            assert_eq!(app.visible_flag.load(Ordering::SeqCst), visible);
            assert_eq!(app.last_visible, visible);
            if visible {
                assert_eq!(observer.current_rect(), original());
            } else {
                assert_eq!(observer.current_rect(), parked);
            }
        }
    }
    #[test]
    fn color_pick_new_visibility_during_native_restore_keeps_latest_intent() {
        for show in [false, true] {
            let mut app = app();
            let (tx, _, observer) = start(&mut app);
            let parked = observer.current_rect();
            let revision = app.visibility_revision.clone();
            let visible = app.visible_flag.clone();
            let restore = app.restore_flag.clone();
            observer.before_next_restore(move || {
                revision.request_with_focus_intent(RootFocusIntent::PreserveForeground, || {
                    visible.store(show, Ordering::SeqCst);
                    restore.store(true, Ordering::SeqCst);
                });
            });
            tx.send(ColorPickOutcome::Picked(RgbColor::new(1, 2, 3)))
                .unwrap();
            wait(&mut app, |app| !app.color_pick_owns_root());
            assert_eq!(app.visible_flag.load(Ordering::SeqCst), show);
            assert_eq!(app.last_visible, show);
            assert_eq!(
                observer.current_rect(),
                if show { original() } else { parked }
            );
            assert_eq!(app.query, "keep original query");
        }
    }
    #[test]
    fn color_pick_hidden_and_screen_draw_conflicts_reject_before_mutation() {
        let mut app = app();
        app.visible_flag.store(false, Ordering::SeqCst);
        assert!(
            app.begin_color_pick()
                .unwrap_err()
                .contains("Show the launcher")
        );
        assert!(!app.color_pick.controller.is_active());
        assert!(app.color_pick.session.is_none());
        app.visible_flag.store(true, Ordering::SeqCst);
        app.screen_draw_controller.request_start().unwrap();
        assert!(app.begin_color_pick().unwrap_err().contains("Screen Draw"));
        assert!(app.color_pick.session.is_none());
        app.screen_draw_controller.close();
        let (_tx, _, _observer) = start(&mut app);
        assert!(app.start_or_focus_screen_draw().is_err());
        assert!(app.request_new_screen_draw_capture().is_err());
        assert!(app.resume_screen_draw().is_err());
        app.color_pick.controller.cancel();
        wait(&mut app, |app| !app.color_pick_owns_root());
    }
    #[test]
    fn color_pick_root_suppresses_geometry_until_native_teardown() {
        let mut app = app();
        let release = Arc::new(AtomicBool::new(false));
        let (tx, cleaned, observer) = start_with_cleanup_gate(&mut app, release.clone());
        let parked = observer.current_rect();
        tx.send(ColorPickOutcome::Picked(RgbColor::new(1, 2, 3)))
            .unwrap();
        visibility(&app, true);
        app.static_location_enabled = true;
        app.static_pos = Some((240, 180));
        app.static_size = Some((900, 650));
        observer.set_current_rect(original()); // Simulates stale external ROOT restore.
        app.root_window_bridge.request_presentation_reconcile();
        let ctx = app.egui_ctx.clone();
        let output = ctx.run(egui::RawInput::default(), |root| {
            app.render_root_frame(root, None)
        });
        assert!(app.color_pick_owns_root());
        assert!(!cleaned.load(Ordering::Acquire));
        assert_eq!(observer.current_rect(), parked);
        let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(!commands.iter().any(|command| matches!(
            command,
            egui::ViewportCommand::OuterPosition(_) | egui::ViewportCommand::InnerSize(_)
        )));
        assert_eq!(app.query, "keep original query");
        release.store(true, Ordering::Release);
        wait(&mut app, |app| !app.color_pick_owns_root());
        assert_eq!(observer.current_rect(), original());
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert_eq!(app.query, "keep original query");
    }

    #[cfg(windows)]
    #[test]
    fn color_pick_exact_restore_queues_ordered_activation_without_configured_placement() {
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
        let (tx, _, observer) = start(&mut app);
        tx.send(ColorPickOutcome::Picked(RgbColor::new(1, 2, 3)))
            .unwrap();
        let ctx = app.egui_ctx.clone();
        ctx.begin_frame(egui::RawInput::default());
        let (_, queued) = crate::window_manager::with_launcher_restore_queue_for_test(|| {
            wait(&mut app, |app| !app.color_pick_owns_root());
        });
        let output = ctx.end_frame();
        assert_eq!(queued, [(app.visibility_revision.current(), 42)]);
        assert_eq!(observer.current_rect(), original());
        assert!(
            !output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .iter()
                .any(|command| matches!(
                    command,
                    egui::ViewportCommand::OuterPosition(_) | egui::ViewportCommand::InnerSize(_)
                ))
        );
    }
}
