//! Shared local screen-region OCR facade.
//!
//! The OCR engine, capture implementation, tiling, and document reconstruction
//! remain owned by the existing MkMacro OCR pipeline. This boundary gives
//! launcher features plain text for a selected rectangle without retaining the
//! captured pixels or exposing recognizer-specific APIs to their UI.

use crate::mkmacro::{
    DiagnosticKind, ExecResult, ExecutionDiagnostic, MkOcrLanguage, ScreenCaptureBackend,
    ScreenRect, SearchRegion, cancelled_error,
    ocr::{OcrBackend, OcrLanguageInfo, recognize_region},
};

/// Captures and recognizes a selected virtual-desktop rectangle using an
/// installed English recognizer and the existing local OCR pipeline.
///
/// General OCR always selects an installed English recognizer. It prefers an
/// installed English language from the Windows user profile, then `en-US`,
/// `en-GB`, and finally the first installed English tag in normalized order.
/// Cancellation is passed through to capture and each OCR operation. The
/// captured image and OCR document are discarded before this function returns.
pub fn recognize_screen_region(
    capture_backend: &dyn ScreenCaptureBackend,
    ocr_backend: &dyn OcrBackend,
    rect: ScreenRect,
    cancelled: &dyn Fn() -> bool,
) -> ExecResult<String> {
    if cancelled() {
        return Err(cancelled_error());
    }
    // Profile languages only affect preference. If Windows cannot provide
    // them, deterministic installed-English fallback selection still works.
    let profile_languages = profile_language_tags().unwrap_or_default();
    recognize_screen_region_with_profile_languages(
        capture_backend,
        ocr_backend,
        rect,
        &profile_languages,
        cancelled,
    )
}

fn recognize_screen_region_with_profile_languages(
    capture_backend: &dyn ScreenCaptureBackend,
    ocr_backend: &dyn OcrBackend,
    rect: ScreenRect,
    profile_languages: &[String],
    cancelled: &dyn Fn() -> bool,
) -> ExecResult<String> {
    if cancelled() {
        return Err(cancelled_error());
    }
    let installed_languages = ocr_backend.available_languages().map_err(|error| {
        error.context("ocr_policy_operation", "enumerate installed OCR languages")
    })?;
    let language = select_english_language(&installed_languages, profile_languages)?;
    recognize_screen_region_with_language(capture_backend, ocr_backend, rect, &language, cancelled)
}

fn select_english_language(
    installed_languages: &[OcrLanguageInfo],
    profile_languages: &[String],
) -> ExecResult<MkOcrLanguage> {
    let mut installed_english = installed_languages
        .iter()
        .filter(|language| is_english_language_tag(&language.tag))
        .collect::<Vec<_>>();
    installed_english.sort_by(|left, right| {
        normalized_language_tag(&left.tag)
            .cmp(&normalized_language_tag(&right.tag))
            .then(left.tag.cmp(&right.tag))
    });

    for preferred in profile_languages
        .iter()
        .filter(|tag| is_english_language_tag(tag))
    {
        let preferred = normalized_language_tag(preferred);
        if let Some(installed) = installed_english
            .iter()
            .find(|language| normalized_language_tag(&language.tag) == preferred)
        {
            return Ok(MkOcrLanguage::LanguageTag(installed.tag.clone()));
        }
    }

    for preferred in ["en-us", "en-gb"] {
        if let Some(installed) = installed_english
            .iter()
            .find(|language| normalized_language_tag(&language.tag) == preferred)
        {
            return Ok(MkOcrLanguage::LanguageTag(installed.tag.clone()));
        }
    }

    installed_english
        .first()
        .map(|language| MkOcrLanguage::LanguageTag(language.tag.clone()))
        .ok_or_else(|| {
            ExecutionDiagnostic::new(
                DiagnosticKind::UnsupportedOperation,
                "No supported English OCR language is installed. Install an English language pack in Windows Settings > Time & language > Language & region, then try again.",
            )
            .context("backend", "ocr")
            .context("operation", "resolve English OCR language")
        })
}

fn is_english_language_tag(tag: &str) -> bool {
    tag.trim()
        .split('-')
        .next()
        .is_some_and(|primary| primary.eq_ignore_ascii_case("en"))
}

fn normalized_language_tag(tag: &str) -> String {
    tag.trim().to_ascii_lowercase()
}

fn recognize_screen_region_with_language(
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

#[cfg(windows)]
fn profile_language_tags() -> ExecResult<Vec<String>> {
    use windows::{
        System::UserProfile::GlobalizationPreferences,
        Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    };

    struct WinRtGuard;
    impl Drop for WinRtGuard {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }

    // WinRT initialization is per-thread; balance every successful call
    // (including S_FALSE) with RoUninitialize. No guard is created on failure.
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
        .map_err(|error| profile_language_error("initialize WinRT", error))?;
    let _winrt = WinRtGuard;
    let languages = GlobalizationPreferences::Languages()
        .map_err(|error| profile_language_error("read profile languages", error))?;
    let count = languages
        .Size()
        .map_err(|error| profile_language_error("count profile languages", error))?;
    (0..count)
        .map(|index| {
            languages
                .GetAt(index)
                .map(|language| language.to_string())
                .map_err(|error| profile_language_error("read profile language", error))
        })
        .collect()
}

#[cfg(windows)]
fn profile_language_error(
    operation: &'static str,
    error: impl std::fmt::Display,
) -> ExecutionDiagnostic {
    ExecutionDiagnostic::new(
        DiagnosticKind::Backend,
        format!("Windows language preferences failed during {operation}: {error}"),
    )
    .context("backend", "windows.globalization")
    .context("operation", operation)
}

#[cfg(not(windows))]
fn profile_language_tags() -> ExecResult<Vec<String>> {
    Ok(Vec::new())
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
        installed_languages: Vec<OcrLanguageInfo>,
        language_query_error: Option<ExecutionDiagnostic>,
        recognition_calls: AtomicUsize,
        recognition_languages: Mutex<Vec<MkOcrLanguage>>,
    }

    impl FakeOcr {
        fn new(document: OcrDocument) -> Self {
            Self {
                document,
                error: None,
                installed_languages: vec![OcrLanguageInfo {
                    tag: "en-US".into(),
                    display_name: "English (United States)".into(),
                }],
                language_query_error: None,
                recognition_calls: AtomicUsize::new(0),
                recognition_languages: Mutex::new(Vec::new()),
            }
        }
    }

    impl OcrBackend for FakeOcr {
        fn available_languages(&self) -> ExecResult<Vec<OcrLanguageInfo>> {
            if let Some(error) = &self.language_query_error {
                return Err(error.clone());
            }
            Ok(self.installed_languages.clone())
        }

        fn max_image_dimension(&self) -> ExecResult<u32> {
            Ok(4096)
        }

        fn recognize(
            &self,
            _: &RgbaImage,
            language: &MkOcrLanguage,
            _: &dyn Fn() -> bool,
        ) -> ExecResult<OcrDocument> {
            self.recognition_calls.fetch_add(1, Ordering::SeqCst);
            self.recognition_languages
                .lock()
                .unwrap()
                .push(language.clone());
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

    fn language_info(tag: &str) -> OcrLanguageInfo {
        OcrLanguageInfo {
            tag: tag.into(),
            display_name: tag.into(),
        }
    }

    fn languages(tags: &[&str]) -> Vec<OcrLanguageInfo> {
        tags.iter().map(|tag| language_info(tag)).collect()
    }

    #[test]
    fn recognizes_the_supplied_signed_rectangle_and_returns_reconstructed_text() {
        let capture = FakeCapture::new(test_desktop());
        let ocr = FakeOcr::new(multiline_document());
        let rect = ScreenRect::new(-1600, -300, 240, 120);

        let text =
            recognize_screen_region_with_profile_languages(&capture, &ocr, rect, &[], &|| false)
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

        let error = recognize_screen_region_with_profile_languages(
            &capture,
            &ocr,
            ScreenRect::new(-20, -10, 10, 10),
            &[],
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

        let error = recognize_screen_region_with_profile_languages(
            &capture,
            &ocr,
            ScreenRect::new(-20, -10, 10, 10),
            &[],
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

        let error = recognize_screen_region_with_profile_languages(
            &capture,
            &ocr,
            ScreenRect::new(-20, -10, 10, 10),
            &[],
            &|| cancelled.load(Ordering::SeqCst),
        )
        .unwrap_err();

        assert_eq!(error.kind, DiagnosticKind::Cancelled);
        assert_eq!(ocr.recognition_calls.load(Ordering::SeqCst), 0);
        assert_eq!(capture.capture_rects.lock().unwrap().len(), 1);
    }

    #[test]
    fn general_ocr_english_selects_the_only_installed_language() {
        assert_eq!(
            select_english_language(&languages(&["en-CA"]), &[]).unwrap(),
            MkOcrLanguage::LanguageTag("en-CA".into())
        );
    }

    #[test]
    fn general_ocr_english_prefers_common_variants() {
        assert_eq!(
            select_english_language(&languages(&["en-GB", "en-AU", "en-US"]), &[]).unwrap(),
            MkOcrLanguage::LanguageTag("en-US".into())
        );
        assert_eq!(
            select_english_language(&languages(&["en-GB", "en-AU"]), &[]).unwrap(),
            MkOcrLanguage::LanguageTag("en-GB".into())
        );
    }

    #[test]
    fn general_ocr_english_rejects_non_english_tags_in_mixed_installation() {
        let mut installed = languages(&["fr-FR", "en-AU", "de-DE"]);
        installed[0].display_name = "English (United States)".into();

        assert_eq!(
            select_english_language(&installed, &[]).unwrap(),
            MkOcrLanguage::LanguageTag("en-AU".into())
        );
    }

    #[test]
    fn general_ocr_english_prefers_an_installed_profile_tag() {
        let mut ocr = FakeOcr::new(multiline_document());
        ocr.installed_languages = languages(&["fr-FR", "en-US", "en-AU"]);
        let capture = FakeCapture::new(test_desktop());
        let profile_languages = vec!["fr-FR".into(), "EN-au".into(), "en-US".into()];

        recognize_screen_region_with_profile_languages(
            &capture,
            &ocr,
            ScreenRect::new(-20, -10, 10, 10),
            &profile_languages,
            &|| false,
        )
        .unwrap();

        assert_eq!(
            *ocr.recognition_languages.lock().unwrap(),
            vec![MkOcrLanguage::LanguageTag("en-AU".into())]
        );
    }

    #[test]
    fn general_ocr_english_fallback_is_stable_across_backend_orderings() {
        let first_order = languages(&["en-ZA", "en-NZ", "en-AU"]);
        let reversed_order = languages(&["en-AU", "en-NZ", "en-ZA"]);

        assert_eq!(
            select_english_language(&first_order, &[]).unwrap(),
            MkOcrLanguage::LanguageTag("en-AU".into())
        );
        assert_eq!(
            select_english_language(&reversed_order, &[]).unwrap(),
            MkOcrLanguage::LanguageTag("en-AU".into())
        );
    }

    #[test]
    fn general_ocr_english_missing_language_error_is_actionable() {
        let no_english = languages(&["fr-FR", "eng-GB", "English-US"]);
        for installed in [&no_english[..], &[][..]] {
            let error = select_english_language(installed, &[]).unwrap_err();
            assert_eq!(error.kind, DiagnosticKind::UnsupportedOperation);
            assert!(error.message.contains("Install an English language pack"));
            assert_eq!(
                error.context.get("operation").unwrap(),
                "resolve English OCR language"
            );
        }
    }

    #[test]
    fn general_ocr_english_preserves_language_enumeration_diagnostics() {
        let mut ocr = FakeOcr::new(multiline_document());
        ocr.language_query_error = Some(
            ExecutionDiagnostic::new(DiagnosticKind::Backend, "language enumeration failed")
                .context("backend", "test OCR"),
        );
        let capture = FakeCapture::new(test_desktop());

        let error = recognize_screen_region_with_profile_languages(
            &capture,
            &ocr,
            ScreenRect::new(-20, -10, 10, 10),
            &[],
            &|| false,
        )
        .unwrap_err();

        assert_eq!(error.kind, DiagnosticKind::Backend);
        assert_eq!(error.message, "language enumeration failed");
        assert_eq!(error.context.get("backend").unwrap(), "test OCR");
        assert_eq!(
            error.context.get("ocr_policy_operation").unwrap(),
            "enumerate installed OCR languages"
        );
        assert!(capture.regions.lock().unwrap().is_empty());
        assert_eq!(ocr.recognition_calls.load(Ordering::SeqCst), 0);
    }
}
