//! One-shot screen OCR workers. Events contain identity and text, never pixels.
use super::{
    capture_screen_region, recognize_captured_screen_region_with_profile_provider,
    selection::OcrGeneration,
};
use crate::mkmacro::{
    DiagnosticKind, ExecResult, ExecutionDiagnostic, ScreenCaptureBackend, ScreenRect,
    ocr::OcrBackend,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

pub(crate) struct OcrJobEvent {
    pub(crate) generation: OcrGeneration,
    pub(crate) kind: OcrJobEventKind,
}

pub(crate) enum OcrJobEventKind {
    /// Capture has stopped; the launcher may restore while recognition runs.
    Captured,
    /// Also terminates capture ownership if capture failed before Captured.
    Finished(ExecResult<String>),
}

pub(crate) struct OcrJob {
    generation: OcrGeneration,
    receiver: Option<mpsc::Receiver<OcrJobEvent>>,
    cancelled: Arc<AtomicBool>,
}

impl OcrJob {
    pub(crate) fn start(
        generation: OcrGeneration,
        rect: ScreenRect,
        capture: Arc<dyn ScreenCaptureBackend>,
        ocr: Arc<dyn OcrBackend>,
        repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> ExecResult<Self> {
        Self::start_with_profile_provider(
            generation,
            rect,
            capture,
            ocr,
            repaint,
            super::profile_language_tags,
        )
    }

    pub(crate) fn start_with_profile_provider(
        generation: OcrGeneration,
        rect: ScreenRect,
        capture: Arc<dyn ScreenCaptureBackend>,
        ocr: Arc<dyn OcrBackend>,
        repaint: Arc<dyn Fn() + Send + Sync>,
        profile_languages: fn() -> ExecResult<Vec<String>>,
    ) -> ExecResult<Self> {
        Self::start_with_spawn(
            generation,
            rect,
            capture,
            ocr,
            repaint,
            profile_languages,
            |worker| {
                std::thread::Builder::new()
                    .name("screen-region-ocr".into())
                    .spawn(worker)
                    .map(|_| ())
            },
        )
    }

    fn start_with_spawn(
        generation: OcrGeneration,
        rect: ScreenRect,
        capture: Arc<dyn ScreenCaptureBackend>,
        ocr: Arc<dyn OcrBackend>,
        repaint: Arc<dyn Fn() + Send + Sync>,
        profile_languages: fn() -> ExecResult<Vec<String>>,
        spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<()>,
    ) -> ExecResult<Self> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let (sender, receiver) = mpsc::channel();
        let worker = Box::new(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let frame = capture_screen_region(capture.as_ref(), rect, &|| {
                    worker_cancelled.load(Ordering::Acquire)
                })?;
                if sender
                    .send(OcrJobEvent {
                        generation,
                        kind: OcrJobEventKind::Captured,
                    })
                    .is_err()
                {
                    return Err(crate::mkmacro::cancelled_error());
                }
                repaint();
                recognize_captured_screen_region_with_profile_provider(
                    ocr.as_ref(),
                    frame,
                    &|| worker_cancelled.load(Ordering::Acquire),
                    profile_languages,
                )
            }))
            .unwrap_or_else(|_| Err(worker_error("OCR worker panicked")));
            let _ = sender.send(OcrJobEvent {
                generation,
                kind: OcrJobEventKind::Finished(result),
            });
            repaint();
        });
        spawn(worker)
            .map_err(|error| worker_error(format!("Could not start OCR worker: {error}")))?;
        Ok(Self {
            generation,
            receiver: Some(receiver),
            cancelled,
        })
    }

    /// Signals cancellation without dropping the capture terminal acknowledgement.
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub(crate) fn poll(&mut self) -> Option<OcrJobEvent> {
        let event = match self.receiver.as_ref()?.try_recv() {
            Ok(event) => event,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => OcrJobEvent {
                generation: self.generation,
                kind: OcrJobEventKind::Finished(Err(worker_error(
                    "OCR worker disconnected before completion",
                ))),
            },
        };
        if matches!(event.kind, OcrJobEventKind::Finished(_)) {
            self.receiver = None;
        }
        Some(event)
    }
}

impl Drop for OcrJob {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn worker_error(message: impl Into<String>) -> ExecutionDiagnostic {
    ExecutionDiagnostic::new(DiagnosticKind::Backend, message)
        .context("backend", "ocr")
        .context("ocr_job_operation", "worker")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{
        CapturedRegion, MkOcrLanguage, SearchRegion,
        ocr::{OcrDocument, OcrLanguageInfo, OcrLine},
    };
    use crate::ocr::selection::OcrSelectionController;
    use std::{
        sync::{Condvar, Mutex, atomic::AtomicUsize},
        time::{Duration, Instant},
    };

    struct Gate {
        open: Mutex<bool>,
        changed: Condvar,
        entered: mpsc::Sender<std::thread::ThreadId>,
    }
    impl Gate {
        fn new(open: bool) -> (Arc<Self>, mpsc::Receiver<std::thread::ThreadId>) {
            let (entered, receiver) = mpsc::channel();
            (
                Arc::new(Self {
                    open: Mutex::new(open),
                    changed: Condvar::new(),
                    entered,
                }),
                receiver,
            )
        }
        fn wait(&self) {
            self.entered.send(std::thread::current().id()).unwrap();
            let open = self.open.lock().unwrap();
            let (_open, timed_out) = self
                .changed
                .wait_timeout_while(open, Duration::from_secs(3), |open| !*open)
                .unwrap();
            assert!(!timed_out.timed_out(), "worker test gate stalled");
        }
        fn release(&self) {
            *self.open.lock().unwrap() = true;
            self.changed.notify_all();
        }
    }
    struct Capture {
        gate: Arc<Gate>,
        calls: AtomicUsize,
        rectangles: Mutex<Vec<ScreenRect>>,
        panic: bool,
        fail: bool,
    }
    impl ScreenCaptureBackend for Capture {
        fn virtual_desktop(&self) -> ExecResult<ScreenRect> {
            panic!("worker must use selected capture only");
        }
        fn region_bounds(&self, _: &SearchRegion) -> ExecResult<ScreenRect> {
            panic!("capture overridden");
        }
        fn capture_rect(
            &self,
            _: ScreenRect,
            _: &dyn Fn() -> bool,
        ) -> ExecResult<image::RgbaImage> {
            panic!("capture overridden");
        }
        fn capture(
            &self,
            region: &SearchRegion,
            cancelled: &dyn Fn() -> bool,
        ) -> ExecResult<CapturedRegion> {
            let SearchRegion::Rectangle { rect } = region else {
                panic!("expected selected rectangle");
            };
            self.rectangles.lock().unwrap().push(*rect);
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.gate.wait();
            assert!(!self.panic, "capture panic");
            if self.fail {
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    "capture failure",
                ));
            }
            if cancelled() {
                return Err(crate::mkmacro::cancelled_error());
            }
            Ok(CapturedRegion {
                image: image::RgbaImage::new(rect.width, rect.height),
                origin: (rect.x, rect.y),
            })
        }
    }
    struct Ocr {
        gate: Arc<Gate>,
        languages: AtomicUsize,
        language_thread: Mutex<Option<std::thread::ThreadId>>,
        requested: Mutex<Vec<MkOcrLanguage>>,
        panic: bool,
    }
    impl OcrBackend for Ocr {
        fn available_languages(&self) -> ExecResult<Vec<OcrLanguageInfo>> {
            self.languages.fetch_add(1, Ordering::SeqCst);
            *self.language_thread.lock().unwrap() = Some(std::thread::current().id());
            Ok(vec![OcrLanguageInfo {
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
            language: &MkOcrLanguage,
            _: &dyn Fn() -> bool,
        ) -> ExecResult<OcrDocument> {
            self.requested.lock().unwrap().push(language.clone());
            self.gate.wait();
            assert!(!self.panic, "recognition panic");
            Ok(OcrDocument {
                lines: vec![OcrLine {
                    text: "worker text".into(),
                    words: vec![],
                }],
                ..Default::default()
            })
        }
    }
    fn generation() -> OcrGeneration {
        OcrSelectionController::default()
            .request(Instant::now())
            .unwrap()
            .unwrap()
    }
    fn rect() -> ScreenRect {
        ScreenRect::new(-90, -30, 220, 80)
    }
    fn capture(gate: Arc<Gate>, panic: bool, fail: bool) -> Arc<Capture> {
        Arc::new(Capture {
            gate,
            calls: AtomicUsize::new(0),
            rectangles: Mutex::new(vec![]),
            panic,
            fail,
        })
    }
    fn ocr(gate: Arc<Gate>, panic: bool) -> Arc<Ocr> {
        Arc::new(Ocr {
            gate,
            languages: AtomicUsize::new(0),
            language_thread: Mutex::new(None),
            requested: Mutex::new(vec![]),
            panic,
        })
    }
    fn next(job: &mut OcrJob) -> OcrJobEvent {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(event) = job.poll() {
                return event;
            }
            assert!(Instant::now() < deadline, "job stalled");
            std::thread::yield_now();
        }
    }
    fn repaint() -> Arc<dyn Fn() + Send + Sync> {
        Arc::new(|| {})
    }

    fn start(
        generation: OcrGeneration,
        rect: ScreenRect,
        capture: Arc<dyn ScreenCaptureBackend>,
        ocr: Arc<dyn OcrBackend>,
        repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> ExecResult<OcrJob> {
        OcrJob::start_with_profile_provider(generation, rect, capture, ocr, repaint, || Ok(vec![]))
    }

    #[test]
    fn worker_captures_once_then_recognizes_off_thread_with_ordered_text_events() {
        let (capture_gate, capture_entered) = Gate::new(false);
        let (ocr_gate, ocr_entered) = Gate::new(false);
        let capture = capture(capture_gate.clone(), false, false);
        let ocr = ocr(ocr_gate.clone(), false);
        let repaints = Arc::new(AtomicUsize::new(0));
        let repaint_count = repaints.clone();
        let generation = generation();
        let mut job = start(
            generation,
            rect(),
            capture.clone(),
            ocr.clone(),
            Arc::new(move || {
                repaint_count.fetch_add(1, Ordering::SeqCst);
            }),
        )
        .unwrap();
        let capture_thread = capture_entered
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert_ne!(capture_thread, std::thread::current().id());
        assert!(job.poll().is_none());
        assert_eq!(ocr.languages.load(Ordering::SeqCst), 0);
        capture_gate.release();
        let event = next(&mut job);
        assert_eq!(event.generation, generation);
        assert!(matches!(event.kind, OcrJobEventKind::Captured));
        assert_eq!(
            ocr_entered.recv_timeout(Duration::from_secs(2)).unwrap(),
            capture_thread
        );
        assert_eq!(*ocr.language_thread.lock().unwrap(), Some(capture_thread));
        assert!(job.poll().is_none());
        ocr_gate.release();
        let event = next(&mut job);
        assert_eq!(event.generation, generation);
        let OcrJobEventKind::Finished(result) = event.kind else {
            panic!("expected terminal");
        };
        assert_eq!(result.unwrap(), "worker text");
        assert!(job.poll().is_none());
        assert_eq!(capture.calls.load(Ordering::SeqCst), 1);
        assert_eq!(*capture.rectangles.lock().unwrap(), vec![rect()]);
        assert_eq!(
            *ocr.requested.lock().unwrap(),
            vec![MkOcrLanguage::LanguageTag("en-US".into())]
        );
        // Captured's repaint is ordered before recognition; Finished's callback
        // may still run immediately after its channel publication.
        assert!(repaints.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn cancellation_during_capture_retains_terminal_and_skips_language_discovery() {
        let (gate, entered) = Gate::new(false);
        let (ocr_gate, _ocr_entered) = Gate::new(true);
        let ocr = ocr(ocr_gate, false);
        let mut job = start(
            generation(),
            rect(),
            capture(gate.clone(), false, false),
            ocr.clone(),
            repaint(),
        )
        .unwrap();
        entered.recv_timeout(Duration::from_secs(2)).unwrap();
        job.cancel();
        assert!(job.poll().is_none());
        gate.release();
        let OcrJobEventKind::Finished(result) = next(&mut job).kind else {
            panic!("cancelled capture must not publish Captured");
        };
        assert_eq!(result.unwrap_err().kind, DiagnosticKind::Cancelled);
        assert_eq!(ocr.languages.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn cancellation_during_recognition_discards_text_and_drop_does_not_join() {
        let (capture_gate, _capture_entered) = Gate::new(true);
        let (ocr_gate, entered) = Gate::new(false);
        let mut job = start(
            generation(),
            rect(),
            capture(capture_gate, false, false),
            ocr(ocr_gate.clone(), false),
            repaint(),
        )
        .unwrap();
        entered.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(next(&mut job).kind, OcrJobEventKind::Captured));
        job.cancel();
        ocr_gate.release();
        let OcrJobEventKind::Finished(result) = next(&mut job).kind else {
            panic!("expected terminal");
        };
        assert_eq!(result.unwrap_err().kind, DiagnosticKind::Cancelled);

        let (capture_gate, second_capture_entered) = Gate::new(true);
        let (ocr_gate, entered) = Gate::new(false);
        let second_ocr = ocr(ocr_gate.clone(), false);
        let mut job = start(
            generation(),
            rect(),
            capture(capture_gate, false, false),
            second_ocr.clone(),
            repaint(),
        )
        .unwrap();
        second_capture_entered
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert!(matches!(next(&mut job).kind, OcrJobEventKind::Captured));
        entered.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(!*ocr_gate.open.lock().unwrap());
        let drop_started = Instant::now();
        drop(job); // The blocked recognizer remains on the detached worker.
        let drop_elapsed = drop_started.elapsed();
        ocr_gate.release();
        assert!(
            drop_elapsed < Duration::from_secs(1),
            "dropping the job waited for the blocked recognizer: {drop_elapsed:?}"
        );
    }

    #[test]
    fn capture_failure_and_worker_panics_always_publish_terminal() {
        for (capture_panic, capture_fail, ocr_panic) in [
            (false, true, false),
            (true, false, false),
            (false, false, true),
        ] {
            let (capture_gate, _capture_entered) = Gate::new(true);
            let (ocr_gate, _ocr_entered) = Gate::new(true);
            let mut job = start(
                generation(),
                rect(),
                capture(capture_gate, capture_panic, capture_fail),
                ocr(ocr_gate, ocr_panic),
                repaint(),
            )
            .unwrap();
            if ocr_panic {
                assert!(matches!(next(&mut job).kind, OcrJobEventKind::Captured));
            }
            let OcrJobEventKind::Finished(result) = next(&mut job).kind else {
                panic!("expected terminal failure");
            };
            let error = result.unwrap_err();
            assert_eq!(error.kind, DiagnosticKind::Backend);
            assert!(job.poll().is_none());
            if capture_fail {
                assert_eq!(
                    error.context.get("ocr_pipeline_operation").unwrap(),
                    "capture region"
                );
            } else {
                assert_eq!(error.context.get("ocr_job_operation").unwrap(), "worker");
            }
        }
    }

    #[test]
    fn spawn_failure_and_disconnected_worker_are_explicit_terminal_failures() {
        let (capture_gate, _capture_entered) = Gate::new(true);
        let (ocr_gate, _ocr_entered) = Gate::new(true);
        let capture = capture(capture_gate, false, false);
        let ocr = ocr(ocr_gate, false);
        let error = OcrJob::start_with_spawn(
            generation(),
            rect(),
            capture.clone(),
            ocr.clone(),
            repaint(),
            || Ok(vec![]),
            |_| Err(std::io::Error::other("spawn rejected")),
        )
        .err()
        .unwrap();
        assert!(error.message.contains("spawn rejected"));
        let mut job = OcrJob::start_with_spawn(
            generation(),
            rect(),
            capture.clone(),
            ocr,
            repaint(),
            || Ok(vec![]),
            |_| Ok(()),
        )
        .unwrap();
        let OcrJobEventKind::Finished(result) = next(&mut job).kind else {
            panic!("expected disconnected terminal");
        };
        assert!(result.unwrap_err().message.contains("disconnected"));
        assert!(job.poll().is_none());
        assert_eq!(capture.calls.load(Ordering::SeqCst), 0);
    }
}
