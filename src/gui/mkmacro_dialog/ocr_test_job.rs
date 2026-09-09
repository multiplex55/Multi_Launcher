//! Explicit one-shot OCR authoring jobs. No work starts from construction or
//! polling; callers must request language discovery or recognition.
use crate::mkmacro::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OcrDraftIdentity {
    pub macro_id: u64,
    pub step_id: Option<u64>,
    pub draft_generation: u64,
    pub condition_path: Option<super::condition_editor::ConditionPath>,
}

#[derive(Debug, Clone)]
pub struct OcrAuthoringPreview {
    pub recognized: OcrRegionDocument,
    pub search: Option<OcrSearchResult>,
}

pub struct OcrTestRequest {
    pub identity: OcrDraftIdentity,
    pub region: SearchRegion,
    pub language: MkOcrLanguage,
    pub search: Option<MkOcrSearchSpec>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OcrTestConfiguration {
    pub region: SearchRegion,
    pub language: MkOcrLanguage,
    pub search: Option<MkOcrSearchSpec>,
}

impl OcrTestRequest {
    pub fn configuration(&self) -> OcrTestConfiguration {
        OcrTestConfiguration {
            region: self.region.clone(),
            language: self.language.clone(),
            search: self.search.clone(),
        }
    }
}

pub struct OcrTestCompletion {
    pub identity: OcrDraftIdentity,
    pub configuration: OcrTestConfiguration,
    pub result: ExecResult<OcrAuthoringPreview>,
}

#[derive(Default)]
pub struct OcrTestJob {
    receiver: Option<mpsc::Receiver<OcrTestCompletion>>,
    cancelled: Option<Arc<AtomicBool>>,
}

impl OcrTestJob {
    pub fn start(
        &mut self,
        capture: Arc<dyn ScreenCaptureBackend>,
        ocr: Arc<dyn OcrBackend>,
        request: OcrTestRequest,
    ) {
        self.cancel();
        let configuration = request.configuration();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = recognize_region(
                capture.as_ref(),
                ocr.as_ref(),
                &request.region,
                &request.language,
                &|| worker_cancelled.load(Ordering::Acquire),
            )
            .and_then(|recognized| {
                let search = request
                    .search
                    .as_ref()
                    .map(|spec| {
                        search_document(
                            &recognized.document,
                            &spec.text,
                            spec.match_mode,
                            spec.case_sensitive,
                            spec.occurrence,
                        )
                        .map_err(|error| {
                            ExecutionDiagnostic::new(
                                DiagnosticKind::InvalidRegex,
                                error.to_string(),
                            )
                        })
                    })
                    .transpose()?;
                Ok(OcrAuthoringPreview { recognized, search })
            });
            let _ = sender.send(OcrTestCompletion {
                identity: request.identity,
                configuration,
                result,
            });
        });
        self.cancelled = Some(cancelled);
        self.receiver = Some(receiver);
    }

    pub fn take_if_current(
        &mut self,
        expected: &OcrDraftIdentity,
    ) -> Option<ExecResult<OcrAuthoringPreview>> {
        let completion = self.receiver.as_ref()?.try_recv().ok()?;
        self.receiver = None;
        self.cancelled = None;
        (completion.identity == *expected).then_some(completion.result)
    }

    pub fn take(&mut self) -> Option<OcrTestCompletion> {
        let completion = self.receiver.as_ref()?.try_recv().ok()?;
        self.receiver = None;
        self.cancelled = None;
        Some(completion)
    }

    pub fn cancel(&mut self) {
        if let Some(cancelled) = self.cancelled.take() {
            cancelled.store(true, Ordering::Release);
        }
        self.receiver = None;
    }

    pub fn active(&self) -> bool {
        self.receiver.is_some()
    }
}

pub struct OcrLanguageJob {
    receiver: Option<mpsc::Receiver<ExecResult<Vec<OcrLanguageInfo>>>>,
}
impl Default for OcrLanguageJob {
    fn default() -> Self {
        Self { receiver: None }
    }
}
impl OcrLanguageJob {
    pub fn request(&mut self, backend: Arc<dyn OcrBackend>) {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(backend.available_languages());
        });
        self.receiver = Some(receiver);
    }
    pub fn take(&mut self) -> Option<ExecResult<Vec<OcrLanguageInfo>>> {
        let result = self.receiver.as_ref()?.try_recv().ok()?;
        self.receiver = None;
        Some(result)
    }
    pub fn active(&self) -> bool {
        self.receiver.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;
    use std::sync::{Mutex, atomic::AtomicUsize};

    struct FakeCapture;
    impl ScreenCaptureBackend for FakeCapture {
        fn virtual_desktop(&self) -> ExecResult<ScreenRect> {
            Ok(ScreenRect::new(0, 0, 20, 10))
        }
        fn region_bounds(&self, _: &SearchRegion) -> ExecResult<ScreenRect> {
            Ok(ScreenRect::new(0, 0, 20, 10))
        }
        fn capture_rect(&self, _: ScreenRect, _: &dyn Fn() -> bool) -> ExecResult<RgbaImage> {
            Ok(RgbaImage::new(20, 10))
        }
    }

    struct FakeOcr {
        language_calls: AtomicUsize,
        recognize_thread: Mutex<Option<std::thread::ThreadId>>,
    }
    impl FakeOcr {
        fn new() -> Self {
            Self {
                language_calls: AtomicUsize::new(0),
                recognize_thread: Mutex::new(None),
            }
        }
    }
    impl OcrBackend for FakeOcr {
        fn available_languages(&self) -> ExecResult<Vec<OcrLanguageInfo>> {
            self.language_calls.fetch_add(1, Ordering::Relaxed);
            Ok(vec![OcrLanguageInfo {
                tag: "en-US".into(),
                display_name: "English".into(),
            }])
        }
        fn max_image_dimension(&self) -> ExecResult<u32> {
            Ok(100)
        }
        fn recognize(
            &self,
            image: &RgbaImage,
            _: &MkOcrLanguage,
            _: &dyn Fn() -> bool,
        ) -> ExecResult<OcrDocument> {
            *self.recognize_thread.lock().unwrap() = Some(std::thread::current().id());
            Ok(OcrDocument {
                recognized_language: Some("en-US".into()),
                image_width: image.width(),
                image_height: image.height(),
                lines: vec![OcrLine {
                    text: "Hello world".into(),
                    words: vec![OcrWord {
                        text: "Hello".into(),
                        bounds: ScreenRect::new(1, 1, 5, 5),
                    }],
                }],
            })
        }
    }

    fn wait_for<T>(mut poll: impl FnMut() -> Option<T>) -> T {
        for _ in 0..100 {
            if let Some(value) = poll() {
                return value;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("background OCR job did not complete")
    }

    #[test]
    fn jobs_are_lazy_and_language_refresh_is_explicit() {
        let backend = Arc::new(FakeOcr::new());
        let mut job = OcrLanguageJob::default();
        assert_eq!(backend.language_calls.load(Ordering::Relaxed), 0);
        assert!(job.take().is_none());
        job.request(backend.clone());
        let languages = wait_for(|| job.take()).unwrap();
        assert_eq!(languages[0].tag, "en-US");
        assert_eq!(backend.language_calls.load(Ordering::Relaxed), 1);
        job.request(backend.clone());
        let _ = wait_for(|| job.take()).unwrap();
        assert_eq!(backend.language_calls.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn recognition_runs_off_thread_and_stale_identity_is_rejected() {
        let backend = Arc::new(FakeOcr::new());
        let caller = std::thread::current().id();
        let identity = OcrDraftIdentity {
            macro_id: 7,
            step_id: Some(9),
            draft_generation: 3,
            condition_path: None,
        };
        let mut job = OcrTestJob::default();
        job.start(
            Arc::new(FakeCapture),
            backend.clone(),
            OcrTestRequest {
                identity: identity.clone(),
                region: SearchRegion::Desktop,
                language: MkOcrLanguage::Auto,
                search: None,
            },
        );
        let mut stale = identity.clone();
        stale.draft_generation += 1;
        let completion = job
            .receiver
            .as_ref()
            .expect("started OCR job has a completion receiver")
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("background OCR job did not complete within the harness timeout");
        let (sender, receiver) = mpsc::channel();
        sender.send(completion).unwrap();
        job.receiver = Some(receiver);

        assert!(job.take_if_current(&stale).is_none());
        assert!(!job.active(), "stale completion was not consumed");
        assert_ne!(*backend.recognize_thread.lock().unwrap(), Some(caller));
    }
}
