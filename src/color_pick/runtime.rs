use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::{
    ColorPickOutcome,
    native::{NativePickerFactory, NativePickerHandle, SystemNativePickerFactory},
};
use crate::mkmacro::screen::SearchRegion;
use crate::mkmacro::screen::{
    CapturedRegion, ScreenCaptureBackend, ScreenRect, WindowsScreenCaptureBackend,
};

const PARK_TIMEOUT: Duration = Duration::from_secs(3);
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(15);
const NATIVE_START_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorPickSessionId(u64);
impl ColorPickSessionId {
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorPickEvent {
    ParkLauncher {
        session: ColorPickSessionId,
        desktop: ScreenRect,
    },
    VerifyParking {
        session: ColorPickSessionId,
    },
    Finished {
        session: ColorPickSessionId,
        outcome: ColorPickOutcome,
    },
}

#[derive(Clone, Copy)]
enum ParkingStage {
    Request,
    Applied,
    Verify,
    Waiting,
}
struct CaptureWorker {
    receiver: mpsc::Receiver<Result<CapturedRegion, String>>,
    thread: JoinHandle<()>,
    cancel: Arc<AtomicBool>,
}
impl Drop for CaptureWorker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

enum Phase {
    Parking(ParkingStage),
    Capture(CaptureWorker),
    Native(NativePickerHandle),
}
struct Session {
    id: ColorPickSessionId,
    bounds: ScreenRect,
    started: Instant,
    phase: Phase,
    pending: Option<ColorPickOutcome>,
}

pub struct ColorPickController {
    next_id: u64,
    session: Option<Session>,
    completed: Option<ColorPickEvent>,
    capture: Arc<dyn ScreenCaptureBackend>,
    native: Arc<dyn NativePickerFactory>,
    repaint: Arc<dyn Fn() + Send + Sync>,
}

impl Default for ColorPickController {
    fn default() -> Self {
        Self::with_backends(
            Arc::new(WindowsScreenCaptureBackend::system()),
            Arc::new(SystemNativePickerFactory),
            Arc::new(|| {}),
        )
    }
}

impl ColorPickController {
    pub fn set_repaint(&mut self, repaint: Arc<dyn Fn() + Send + Sync>) {
        self.repaint = repaint;
    }

    pub(crate) fn with_backends(
        capture: Arc<dyn ScreenCaptureBackend>,
        native: Arc<dyn NativePickerFactory>,
        repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            next_id: 0,
            session: None,
            completed: None,
            capture,
            native,
            repaint,
        }
    }

    pub fn active_session(&self) -> Option<ColorPickSessionId> {
        self.session.as_ref().map(|session| session.id)
    }
    pub fn is_active(&self) -> bool {
        self.session.is_some() || self.completed.is_some()
    }

    /// Repeated activation does not create another capture or native owner.
    pub fn request(&mut self, now: Instant) -> Result<Option<ColorPickSessionId>, String> {
        if self.is_active() {
            return Ok(None);
        }
        let bounds = self
            .capture
            .virtual_desktop()
            .map_err(|error| error.to_string())?;
        bounds
            .validate_capture()
            .map_err(|error| error.to_string())?;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or("color picker session identity exhausted")?;
        let id = ColorPickSessionId(self.next_id);
        self.session = Some(Session {
            id,
            bounds,
            started: now,
            phase: Phase::Parking(ParkingStage::Request),
            pending: None,
        });
        Ok(Some(id))
    }

    pub fn parking_applied(&mut self, id: ColorPickSessionId, result: Result<(), String>) {
        if let Some(session) = &mut self.session
            && session.id == id
            && matches!(session.phase, Phase::Parking(ParkingStage::Applied))
        {
            match result {
                Ok(()) => session.phase = Phase::Parking(ParkingStage::Verify),
                Err(error) => self.finish(ColorPickOutcome::Failed(format!(
                    "Could not park launcher: {error}"
                ))),
            }
        }
    }

    pub fn parking_verified(
        &mut self,
        id: ColorPickSessionId,
        result: Result<bool, String>,
        now: Instant,
    ) {
        let Some(session) = &mut self.session else {
            return;
        };
        if session.id != id || !matches!(session.phase, Phase::Parking(ParkingStage::Waiting)) {
            return;
        }
        match result {
            Ok(false) => session.phase = Phase::Parking(ParkingStage::Verify),
            Err(error) => self.finish(ColorPickOutcome::Failed(format!(
                "Could not verify launcher parking: {error}"
            ))),
            Ok(true) => {
                let (sender, receiver) = mpsc::channel();
                let cancel = Arc::new(AtomicBool::new(false));
                let worker_cancel = Arc::clone(&cancel);
                let capture = Arc::clone(&self.capture);
                let repaint = Arc::clone(&self.repaint);
                let thread = thread::Builder::new()
                    .name("color-picker-capture".into())
                    .spawn(move || {
                        let result = catch_unwind(AssertUnwindSafe(|| {
                            capture.capture(&SearchRegion::Desktop, &|| {
                                worker_cancel.load(Ordering::Acquire)
                            })
                        }))
                        .map_err(|_| "desktop capture worker panicked".to_owned())
                        .and_then(|result| result.map_err(|error| error.to_string()));
                        let _ = sender.send(result);
                        repaint();
                    });
                match thread {
                    Ok(thread) => {
                        session.started = now;
                        session.phase = Phase::Capture(CaptureWorker {
                            receiver,
                            thread,
                            cancel,
                        });
                    }
                    Err(error) => self.finish(ColorPickOutcome::Failed(format!(
                        "Could not start desktop capture: {error}"
                    ))),
                }
            }
        }
    }

    fn finish(&mut self, outcome: ColorPickOutcome) {
        if let Some(session) = self.session.take() {
            self.completed = Some(ColorPickEvent::Finished {
                session: session.id,
                outcome,
            });
        }
    }

    pub fn cancel(&mut self) {
        let Some(session) = &mut self.session else {
            return;
        };
        session.pending.get_or_insert(ColorPickOutcome::Cancelled);
        match &session.phase {
            Phase::Parking(_) => self.finish(ColorPickOutcome::Cancelled),
            Phase::Capture(worker) => {
                worker.cancel.store(true, Ordering::Release);
                self.finish(ColorPickOutcome::Cancelled);
            }
            Phase::Native(handle) => handle.cancel(),
        }
    }

    /// Polling never waits for an unfinished native/capture thread.
    pub fn poll(&mut self, now: Instant) -> Vec<ColorPickEvent> {
        if let Some(event) = self.completed.take() {
            return vec![event];
        }
        let Some(session) = &mut self.session else {
            return Vec::new();
        };
        let elapsed = now.saturating_duration_since(session.started);
        let mut terminal = None;
        match &mut session.phase {
            Phase::Parking(stage) => {
                if elapsed >= PARK_TIMEOUT {
                    terminal = Some(ColorPickOutcome::Failed(
                        "Timed out waiting for capture-safe launcher parking".into(),
                    ));
                } else {
                    match stage {
                        ParkingStage::Request => {
                            *stage = ParkingStage::Applied;
                            return vec![ColorPickEvent::ParkLauncher {
                                session: session.id,
                                desktop: session.bounds,
                            }];
                        }
                        ParkingStage::Verify => {
                            *stage = ParkingStage::Waiting;
                            return vec![ColorPickEvent::VerifyParking {
                                session: session.id,
                            }];
                        }
                        _ => {}
                    }
                }
            }
            Phase::Capture(worker) => {
                if elapsed >= CAPTURE_TIMEOUT {
                    terminal = Some(ColorPickOutcome::Failed("Desktop capture timed out".into()));
                    worker.cancel.store(true, Ordering::Release);
                } else if worker.thread.is_finished() {
                    let result = worker.receiver.try_recv().unwrap_or_else(|_| {
                        Err("Desktop capture worker closed without a result".into())
                    });
                    if let Some(outcome) = session.pending.take() {
                        terminal = Some(outcome);
                    } else {
                        match result {
                            Err(error) => terminal = Some(ColorPickOutcome::Failed(error)),
                            Ok(snapshot) if snapshot.rect() != session.bounds => {
                                terminal = Some(ColorPickOutcome::Failed(
                                    "Display geometry changed during capture; reopen the picker"
                                        .into(),
                                ))
                            }
                            Ok(snapshot) => match self
                                .native
                                .spawn(Arc::new(snapshot), Arc::clone(&self.repaint))
                            {
                                Ok(handle) => {
                                    session.phase = Phase::Native(handle);
                                    session.started = now;
                                }
                                Err(error) => terminal = Some(ColorPickOutcome::Failed(error)),
                            },
                        }
                    }
                }
            }
            Phase::Native(handle) => {
                if elapsed >= NATIVE_START_TIMEOUT
                    && !handle.is_ready()
                    && session.pending.is_none()
                {
                    session.pending = Some(ColorPickOutcome::Failed(
                        "Native color picker startup timed out".into(),
                    ));
                    handle.cancel();
                }
                if let Some(outcome) = handle.poll() {
                    terminal = Some(session.pending.take().unwrap_or(outcome));
                }
            }
        }
        if let Some(outcome) = terminal {
            self.finish(outcome);
        }
        self.completed.take().into_iter().collect()
    }
}

impl Drop for ColorPickController {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::RgbColor;
    use crate::mkmacro::{DiagnosticKind, ExecResult, ExecutionDiagnostic};
    use std::sync::{Mutex, atomic::AtomicUsize};

    struct Capture {
        count: AtomicUsize,
        fail: bool,
        wait_for_cancel: bool,
        changed_bounds: bool,
        release: Option<Mutex<mpsc::Receiver<()>>>,
        trace: Arc<Mutex<Vec<&'static str>>>,
    }
    impl ScreenCaptureBackend for Capture {
        fn virtual_desktop(&self) -> ExecResult<ScreenRect> {
            Ok(super::super::tests::snapshot().rect())
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
        fn capture(
            &self,
            region: &SearchRegion,
            cancelled: &dyn Fn() -> bool,
        ) -> ExecResult<CapturedRegion> {
            assert_eq!(*region, SearchRegion::Desktop);
            self.count.fetch_add(1, Ordering::AcqRel);
            self.trace.lock().unwrap().push("capture");
            if let Some(release) = &self.release {
                release.lock().unwrap().recv().unwrap();
                self.trace.lock().unwrap().push("capture.released");
            }
            if self.wait_for_cancel {
                while !cancelled() {
                    thread::yield_now();
                }
            }
            if self.fail {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    "capture fixture failed",
                ));
            }
            let mut snapshot = (*super::super::tests::snapshot()).clone();
            if self.changed_bounds {
                snapshot.origin.0 += 1;
            }
            Ok(snapshot)
        }
    }
    #[derive(Clone, Copy)]
    enum NativeMode {
        Accept,
        Cancel,
        WaitCancel,
        StartupStall,
        FailSpawn,
        Panic,
        CloseChannel,
    }
    struct Factory {
        mode: NativeMode,
        count: AtomicUsize,
        trace: Arc<Mutex<Vec<&'static str>>>,
    }
    struct Cleanup(Arc<Mutex<Vec<&'static str>>>);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            self.0.lock().unwrap().push("native.cleanup");
        }
    }
    impl NativePickerFactory for Factory {
        fn spawn(
            &self,
            snapshot: Arc<CapturedRegion>,
            repaint: Arc<dyn Fn() + Send + Sync>,
        ) -> Result<NativePickerHandle, String> {
            self.count.fetch_add(1, Ordering::AcqRel);
            if matches!(self.mode, NativeMode::FailSpawn) {
                return Err("native fixture failed".into());
            }
            if matches!(self.mode, NativeMode::CloseChannel) {
                return Ok(NativePickerHandle::closed_fixture());
            }
            let trace = Arc::clone(&self.trace);
            let mode = self.mode;
            NativePickerHandle::spawn_worker(repaint, move |cancel, ready| {
                let _cleanup = Cleanup(Arc::clone(&trace));
                trace.lock().unwrap().push("native.start");
                if !matches!(mode, NativeMode::StartupStall) {
                    ready.store(true, Ordering::Release);
                }
                match mode {
                    NativeMode::Accept => {
                        let mut picker = super::super::FrozenPicker::new(snapshot).unwrap();
                        for point in [(-3, -2), (0, 0), (2, 1)] {
                            picker.hover(point);
                            picker.magnifier();
                        }
                        picker.accept((2, 1)).unwrap()
                    }
                    NativeMode::Cancel => ColorPickOutcome::Cancelled,
                    NativeMode::WaitCancel | NativeMode::StartupStall => {
                        while !cancel.load(Ordering::Acquire) {
                            thread::yield_now();
                        }
                        ColorPickOutcome::Cancelled
                    }
                    NativeMode::Panic => panic!("native fixture panic"),
                    _ => unreachable!(),
                }
            })
        }
    }
    fn controller(
        mode: NativeMode,
    ) -> (
        ColorPickController,
        Arc<Capture>,
        Arc<Factory>,
        Arc<Mutex<Vec<&'static str>>>,
    ) {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let capture = Arc::new(Capture {
            count: AtomicUsize::new(0),
            fail: false,
            wait_for_cancel: false,
            changed_bounds: false,
            release: None,
            trace: Arc::clone(&trace),
        });
        let native = Arc::new(Factory {
            mode,
            count: AtomicUsize::new(0),
            trace: Arc::clone(&trace),
        });
        let controller =
            ColorPickController::with_backends(capture.clone(), native.clone(), Arc::new(|| {}));
        (controller, capture, native, trace)
    }
    fn verify(controller: &mut ColorPickController, now: Instant) -> ColorPickSessionId {
        let id = controller.request(now).unwrap().unwrap();
        assert!(
            matches!(controller.poll(now).as_slice(), [ColorPickEvent::ParkLauncher { session, .. }] if *session == id)
        );
        controller.parking_applied(id, Ok(()));
        assert_eq!(
            controller.poll(now),
            vec![ColorPickEvent::VerifyParking { session: id }]
        );
        controller.parking_verified(id, Ok(true), now);
        id
    }
    fn terminal(controller: &mut ColorPickController) -> ColorPickOutcome {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(ColorPickEvent::Finished { outcome, .. }) =
                controller.poll(Instant::now()).pop()
            {
                return outcome;
            }
            assert!(Instant::now() < deadline, "fixture worker did not finish");
            thread::yield_now();
        }
    }
    fn await_native(controller: &mut ColorPickController) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !matches!(
            controller.session.as_ref().map(|session| &session.phase),
            Some(Phase::Native(_))
        ) {
            assert!(controller.poll(Instant::now()).is_empty());
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
    }

    #[test]
    fn park_then_later_verify_precedes_single_frozen_capture_and_cleanup() {
        let (mut controller, capture, native, trace) = controller(NativeMode::Accept);
        let now = Instant::now();
        let id = controller.request(now).unwrap().unwrap();
        assert!(controller.request(now).unwrap().is_none());
        controller.parking_verified(id, Ok(true), now); // Premature admission ignored.
        assert_eq!(capture.count.load(Ordering::Acquire), 0);
        assert!(matches!(
            controller.poll(now)[0],
            ColorPickEvent::ParkLauncher { .. }
        ));
        trace.lock().unwrap().push("park");
        controller.parking_applied(id, Ok(()));
        assert_eq!(capture.count.load(Ordering::Acquire), 0);
        assert_eq!(
            controller.poll(now),
            vec![ColorPickEvent::VerifyParking { session: id }]
        );
        controller.parking_verified(id, Ok(false), now);
        assert_eq!(capture.count.load(Ordering::Acquire), 0);
        controller.poll(now);
        trace.lock().unwrap().push("verify");
        controller.parking_verified(id, Ok(true), now);
        assert_eq!(
            terminal(&mut controller),
            ColorPickOutcome::Picked(RgbColor::new(5, 3, 123))
        );
        assert_eq!(capture.count.load(Ordering::Acquire), 1);
        assert_eq!(native.count.load(Ordering::Acquire), 1);
        assert_eq!(
            *trace.lock().unwrap(),
            vec![
                "park",
                "verify",
                "capture",
                "native.start",
                "native.cleanup"
            ]
        );
        assert!(!controller.is_active());
        assert!(controller.poll(now).is_empty());
    }

    #[test]
    fn cancellation_waits_for_native_cleanup_and_allows_reopening() {
        let (mut controller, _, _, trace) = controller(NativeMode::WaitCancel);
        verify(&mut controller, Instant::now());
        await_native(&mut controller);
        controller.cancel();
        assert_eq!(terminal(&mut controller), ColorPickOutcome::Cancelled);
        assert_eq!(trace.lock().unwrap().last(), Some(&"native.cleanup"));
        assert!(controller.request(Instant::now()).unwrap().is_some());
        controller.cancel();
        assert_eq!(terminal(&mut controller), ColorPickOutcome::Cancelled);
    }

    #[test]
    fn native_cancel_failure_panic_and_channel_loss_release_ownership() {
        for mode in [
            NativeMode::Cancel,
            NativeMode::FailSpawn,
            NativeMode::Panic,
            NativeMode::CloseChannel,
        ] {
            let (mut controller, _, _, trace) = controller(mode);
            verify(&mut controller, Instant::now());
            let outcome = terminal(&mut controller);
            if matches!(mode, NativeMode::Cancel) {
                assert_eq!(outcome, ColorPickOutcome::Cancelled);
            } else {
                assert!(matches!(outcome, ColorPickOutcome::Failed(_)));
            }
            if matches!(mode, NativeMode::Panic) {
                assert_eq!(trace.lock().unwrap().last(), Some(&"native.cleanup"));
            }
            assert!(!controller.is_active());
        }
    }

    #[test]
    fn stale_parking_admission_and_parking_timeout_do_not_capture() {
        let (mut controller, capture, _, _) = controller(NativeMode::Accept);
        let now = Instant::now();
        let old = controller.request(now).unwrap().unwrap();
        controller.cancel();
        assert_eq!(terminal(&mut controller), ColorPickOutcome::Cancelled);
        let fresh = controller.request(now).unwrap().unwrap();
        controller.poll(now);
        controller.parking_applied(old, Ok(()));
        controller.parking_verified(old, Ok(true), now);
        assert_eq!(controller.active_session(), Some(fresh));
        assert_eq!(capture.count.load(Ordering::Acquire), 0);
        assert!(matches!(
            controller.poll(now + PARK_TIMEOUT).as_slice(),
            [ColorPickEvent::Finished {
                outcome: ColorPickOutcome::Failed(_),
                ..
            }]
        ));
        assert!(!controller.is_active());
    }

    #[test]
    fn capture_failure_or_geometry_change_never_starts_native_surface() {
        for changed in [false, true] {
            let (mut controller, capture, native, _) = controller(NativeMode::Accept);
            // Controller owns another Arc: replace with an explicitly failing capture seam.
            let replacement = Arc::new(Capture {
                count: AtomicUsize::new(0),
                fail: !changed,
                wait_for_cancel: false,
                changed_bounds: changed,
                release: None,
                trace: Arc::clone(&capture.trace),
            });
            controller.capture = replacement;
            verify(&mut controller, Instant::now());
            assert!(matches!(
                terminal(&mut controller),
                ColorPickOutcome::Failed(_)
            ));
            assert_eq!(native.count.load(Ordering::Acquire), 0);
        }
    }

    #[test]
    fn capture_and_native_start_timeouts_cancel_workers_before_terminal() {
        let (mut controller, capture, _, _) = controller(NativeMode::Accept);
        controller.capture = Arc::new(Capture {
            count: AtomicUsize::new(0),
            fail: false,
            wait_for_cancel: true,
            changed_bounds: false,
            release: None,
            trace: Arc::clone(&capture.trace),
        });
        let now = Instant::now();
        verify(&mut controller, now);
        assert!(matches!(
            controller.poll(now + CAPTURE_TIMEOUT).as_slice(),
            [ColorPickEvent::Finished {
                outcome: ColorPickOutcome::Failed(_),
                ..
            }]
        ));

        let (mut controller, _, _, trace) = self::controller(NativeMode::StartupStall);
        verify(&mut controller, now);
        await_native(&mut controller);
        let native_started = controller.session.as_ref().unwrap().started;
        let events = controller.poll(native_started + NATIVE_START_TIMEOUT);
        let outcome = match events.into_iter().next() {
            Some(ColorPickEvent::Finished { outcome, .. }) => outcome,
            _ => terminal(&mut controller),
        };
        assert!(matches!(outcome, ColorPickOutcome::Failed(_)));
        assert_eq!(trace.lock().unwrap().last(), Some(&"native.cleanup"));
    }

    #[test]
    fn blocked_capture_timeout_or_cancel_retires_without_waiting_and_stale_result_is_ignored() {
        for cancel in [false, true] {
            let (mut controller, _, native, trace) = self::controller(NativeMode::Accept);
            let (release, receive) = mpsc::channel();
            let blocked = Arc::new(Capture {
                count: AtomicUsize::new(0),
                fail: false,
                wait_for_cancel: false,
                changed_bounds: false,
                release: Some(Mutex::new(receive)),
                trace: Arc::clone(&trace),
            });
            controller.capture = blocked.clone();
            let now = Instant::now();
            verify(&mut controller, now);
            let deadline = Instant::now() + Duration::from_secs(3);
            while blocked.count.load(Ordering::Acquire) == 0 {
                assert!(Instant::now() < deadline);
                thread::yield_now();
            }
            if cancel {
                controller.cancel();
                assert_eq!(terminal(&mut controller), ColorPickOutcome::Cancelled);
            } else {
                assert!(matches!(
                    controller.poll(now + CAPTURE_TIMEOUT).as_slice(),
                    [ColorPickEvent::Finished {
                        outcome: ColorPickOutcome::Failed(_),
                        ..
                    }]
                ));
            }
            assert!(!controller.is_active());
            assert_eq!(native.count.load(Ordering::Acquire), 0);
            controller.capture = Arc::new(Capture {
                count: AtomicUsize::new(0),
                fail: false,
                wait_for_cancel: false,
                changed_bounds: false,
                release: None,
                trace: Arc::clone(&trace),
            });
            verify(&mut controller, Instant::now());
            assert!(matches!(
                terminal(&mut controller),
                ColorPickOutcome::Picked(_)
            ));
            release.send(()).unwrap();
            while !trace.lock().unwrap().contains(&"capture.released") {
                assert!(Instant::now() < deadline);
                thread::yield_now();
            }
            assert!(controller.poll(Instant::now()).is_empty());
            assert_eq!(native.count.load(Ordering::Acquire), 1);
        }
    }
}
