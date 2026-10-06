//! Shared local screen-region OCR facade.
//!
//! The OCR engine, capture implementation, tiling, and document reconstruction
//! remain owned by the existing MkMacro OCR pipeline. This boundary gives
//! launcher features plain text for a selected rectangle without retaining the
//! captured pixels or exposing recognizer-specific APIs to their UI.

use crate::mkmacro::{
    ExecResult, MkOcrLanguage, ScreenCaptureBackend, ScreenRect, SearchRegion,
    ocr::{OcrBackend, recognize_region},
};

/// Captures and recognizes a selected virtual-desktop rectangle using the
/// existing local OCR pipeline, returning its reconstructed multiline text.
///
/// The caller supplies the language so workflows can apply their own language
/// policy while MkMacro keeps its existing configurable behavior. Cancellation
/// is passed through to capture and each OCR operation. The captured image and
/// OCR document are discarded before this function returns.
pub fn recognize_screen_region(
    capture_backend: &dyn ScreenCaptureBackend,
    ocr_backend: &dyn OcrBackend,
    rect: ScreenRect,
    language: &MkOcrLanguage,
    cancelled: &dyn Fn() -> bool,
) -> ExecResult<String> {
    let region = SearchRegion::Rectangle { rect };
    let recognized = recognize_region(capture_backend, ocr_backend, &region, language, cancelled)?;
    Ok(recognized.document.recognized_text())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{
        DiagnosticKind, ExecutionDiagnostic, OcrDocument, OcrLanguageInfo, OcrLine,
    };
    use image::RgbaImage;
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    struct FakeCapture {
        desktop: ScreenRect,
        regions: Mutex<Vec<SearchRegion>>,
        capture_rects: Mutex<Vec<ScreenRect>>,
        capture_error: Option<ExecutionDiagnostic>,
        cancel_on_capture: Option<std::sync::Arc<AtomicBool>>,
    }

    impl FakeCapture {
        fn new(desktop: ScreenRect) -> Self {
            Self {
                desktop,
                regions: Mutex::new(Vec::new()),
                capture_rects: Mutex::new(Vec::new()),
                capture_error: None,
                cancel_on_capture: None,
            }
        }
    }

    impl ScreenCaptureBackend for FakeCapture {
        fn virtual_desktop(&self) -> ExecResult<ScreenRect> {
            Ok(self.desktop)
        }

        fn region_bounds(&self, region: &SearchRegion) -> ExecResult<ScreenRect> {
            self.regions.lock().unwrap().push(region.clone());
            match region {
                SearchRegion::Rectangle { rect } => Ok(*rect),
                _ => Err(ExecutionDiagnostic::new(
                    DiagnosticKind::InvalidTarget,
                    "expected a rectangle capture",
                )),
            }
        }

        fn capture_rect(&self, rect: ScreenRect, _: &dyn Fn() -> bool) -> ExecResult<RgbaImage> {
            self.capture_rects.lock().unwrap().push(rect);
            if let Some(error) = &self.capture_error {
                return Err(error.clone());
            }
            if let Some(cancelled) = &self.cancel_on_capture {
                cancelled.store(true, Ordering::SeqCst);
            }
            Ok(RgbaImage::new(rect.width, rect.height))
        }
    }

    struct FakeOcr {
        document: OcrDocument,
        error: Option<ExecutionDiagnostic>,
        recognition_calls: AtomicUsize,
    }

    impl FakeOcr {
        fn new(document: OcrDocument) -> Self {
            Self {
                document,
                error: None,
                recognition_calls: AtomicUsize::new(0),
            }
        }
    }

    impl OcrBackend for FakeOcr {
        fn available_languages(&self) -> ExecResult<Vec<OcrLanguageInfo>> {
            Ok(vec![OcrLanguageInfo {
                tag: "en-US".into(),
                display_name: "English (United States)".into(),
            }])
        }

        fn max_image_dimension(&self) -> ExecResult<u32> {
            Ok(4096)
        }

        fn recognize(
            &self,
            _: &RgbaImage,
            _: &MkOcrLanguage,
            _: &dyn Fn() -> bool,
        ) -> ExecResult<OcrDocument> {
            self.recognition_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(error) = &self.error {
                return Err(error.clone());
            }
            Ok(self.document.clone())
        }
    }

    fn multiline_document() -> OcrDocument {
        OcrDocument {
            lines: vec![
                OcrLine {
                    text: "First recognized line".into(),
                    words: vec![],
                },
                OcrLine {
                    text: "Second recognized line".into(),
                    words: vec![],
                },
            ],
            ..OcrDocument::default()
        }
    }

    fn test_desktop() -> ScreenRect {
        ScreenRect::new(-1920, -1080, 3840, 2160)
    }

    #[test]
    fn recognizes_the_supplied_signed_rectangle_and_returns_reconstructed_text() {
        let capture = FakeCapture::new(test_desktop());
        let ocr = FakeOcr::new(multiline_document());
        let rect = ScreenRect::new(-1600, -300, 240, 120);

        let text = recognize_screen_region(
            &capture,
            &ocr,
            rect,
            &MkOcrLanguage::LanguageTag("en-US".into()),
            &|| false,
        )
        .unwrap();

        assert_eq!(text, "First recognized line\nSecond recognized line");
        assert_eq!(
            *capture.regions.lock().unwrap(),
            vec![SearchRegion::Rectangle { rect }]
        );
        assert_eq!(*capture.capture_rects.lock().unwrap(), vec![rect]);
        assert_eq!(ocr.recognition_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn propagates_capture_failure_with_its_diagnostic_context() {
        let mut capture = FakeCapture::new(test_desktop());
        capture.capture_error = Some(
            ExecutionDiagnostic::new(DiagnosticKind::Backend, "screen capture failed")
                .context("backend", "test capture"),
        );
        let ocr = FakeOcr::new(multiline_document());

        let error = recognize_screen_region(
            &capture,
            &ocr,
            ScreenRect::new(-20, -10, 10, 10),
            &MkOcrLanguage::Auto,
            &|| false,
        )
        .unwrap_err();

        assert_eq!(error.kind, DiagnosticKind::Backend);
        assert_eq!(error.message, "screen capture failed");
        assert_eq!(error.context.get("backend").unwrap(), "test capture");
        assert_eq!(
            error.context.get("ocr_pipeline_operation").unwrap(),
            "capture region"
        );
        assert_eq!(ocr.recognition_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn propagates_ocr_failure_with_its_diagnostic_context() {
        let capture = FakeCapture::new(test_desktop());
        let mut ocr = FakeOcr::new(multiline_document());
        ocr.error = Some(ExecutionDiagnostic::new(
            DiagnosticKind::Backend,
            "local recognizer failed",
        ));

        let error = recognize_screen_region(
            &capture,
            &ocr,
            ScreenRect::new(-20, -10, 10, 10),
            &MkOcrLanguage::Auto,
            &|| false,
        )
        .unwrap_err();

        assert_eq!(error.kind, DiagnosticKind::Backend);
        assert_eq!(error.message, "local recognizer failed");
        assert_eq!(
            error.context.get("ocr_pipeline_operation").unwrap(),
            "recognize tile"
        );
        assert_eq!(error.context.get("tile_index").unwrap(), "0");
        assert_eq!(ocr.recognition_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn propagates_cancellation_during_capture_without_starting_ocr() {
        let cancelled = std::sync::Arc::new(AtomicBool::new(false));
        let mut capture = FakeCapture::new(test_desktop());
        capture.cancel_on_capture = Some(cancelled.clone());
        let ocr = FakeOcr::new(multiline_document());

        let error = recognize_screen_region(
            &capture,
            &ocr,
            ScreenRect::new(-20, -10, 10, 10),
            &MkOcrLanguage::Auto,
            &|| cancelled.load(Ordering::SeqCst),
        )
        .unwrap_err();

        assert_eq!(error.kind, DiagnosticKind::Cancelled);
        assert_eq!(ocr.recognition_calls.load(Ordering::SeqCst), 0);
        assert_eq!(capture.capture_rects.lock().unwrap().len(), 1);
    }
}
