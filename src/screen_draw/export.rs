use std::borrow::Cow;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use image::{ImageEncoder, RgbaImage};

use super::document::fading_strokes_at;
use super::raster::{RasterBackground, render_annotations_into};
use super::{
    AnnotationObject, DesktopPoint, DesktopRect, ExportBackground, ExportDestination,
    ExportRequest, ExportScope, ScreenDrawSessionSnapshot, TransientStroke,
};

#[derive(Debug, Clone)]
pub struct ExportSource {
    pub snapshot: ScreenDrawSessionSnapshot,
    pub objects: Vec<AnnotationObject>,
    pub transient: Vec<TransientStroke>,
    pub now: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportOutcome {
    Clipboard,
    File(PathBuf),
}

pub(crate) trait ExportDestinationBackend: Send + Sync {
    fn deliver(
        &self,
        destination: ExportDestination,
        image: RgbaImage,
    ) -> Result<ExportOutcome, String>;
}

#[derive(Debug, Default)]
pub(crate) struct SystemExportDestination;

impl ExportDestinationBackend for SystemExportDestination {
    fn deliver(
        &self,
        destination: ExportDestination,
        image: RgbaImage,
    ) -> Result<ExportOutcome, String> {
        match destination {
            ExportDestination::Clipboard => copy_image_to_clipboard(image),
            ExportDestination::File => save_image_to_default_directory(&image),
            ExportDestination::ScreenshotEditor => {
                Err("Screenshot Editor export is not available yet".into())
            }
        }
    }
}

pub fn compose_export(request: ExportRequest, source: &ExportSource) -> Result<RgbaImage, String> {
    let capture = source.snapshot.capture();
    let capture_bounds = DesktopRect::new(
        capture.origin.0,
        capture.origin.1,
        capture.image.width(),
        capture.image.height(),
    );
    let output_bounds = match request.scope {
        ExportScope::FullDesktop => capture_bounds,
        ExportScope::Region(rect) => DesktopRect::new(rect.x, rect.y, rect.width, rect.height)
            .intersection(capture_bounds)
            .ok_or_else(|| {
                "Screen Draw export region is outside the captured desktop".to_string()
            })?,
    };
    if output_bounds.is_empty() {
        return Err("cannot export an empty Screen Draw region".into());
    }
    let background = match request.background {
        ExportBackground::FrozenDesktop => RasterBackground::Frozen {
            image: &capture.image,
            bounds: capture_bounds,
        },
        ExportBackground::Transparent => RasterBackground::Transparent,
        ExportBackground::White => RasterBackground::Solid(super::RgbaColor::WHITE),
        ExportBackground::Black => RasterBackground::Solid(super::RgbaColor::BLACK),
        ExportBackground::Solid(color) => RasterBackground::Solid(color),
    };
    let transient = fading_strokes_at(&source.transient, source.now);
    let mut output = RgbaImage::new(output_bounds.width, output_bounds.height);
    render_annotations_into(
        &mut output,
        DesktopPoint::new(output_bounds.x, output_bounds.y),
        background,
        &source.objects,
        true,
        &transient,
    )
    .map_err(|error| format!("failed to compose Screen Draw export: {error:?}"))?;
    Ok(output)
}

pub(crate) fn execute_export(
    request: ExportRequest,
    source: ExportSource,
    destination: Arc<dyn ExportDestinationBackend>,
) -> Result<ExportOutcome, String> {
    let image = compose_export(request, &source)?;
    destination.deliver(request.destination, image)
}

fn copy_image_to_clipboard(image: RgbaImage) -> Result<ExportOutcome, String> {
    let (width, height) = image.dimensions();
    let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
    clipboard
        .set_image(arboard::ImageData {
            width: width as usize,
            height: height as usize,
            bytes: Cow::Owned(image.into_raw()),
        })
        .map_err(|error| error.to_string())?;
    Ok(ExportOutcome::Clipboard)
}

fn save_image_to_default_directory(image: &RgbaImage) -> Result<ExportOutcome, String> {
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    save_png_unique(
        image,
        &crate::plugins::screenshot::screenshot_dir(),
        &timestamp,
    )
    .map(ExportOutcome::File)
}

pub(crate) fn unique_png_path(directory: &Path, timestamp: &str) -> PathBuf {
    for suffix in 0_u64.. {
        let candidate = png_path_with_suffix(directory, timestamp, suffix);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

pub(crate) fn save_png_unique(
    image: &RgbaImage,
    directory: &Path,
    timestamp: &str,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    for suffix in 0_u64.. {
        let path = png_path_with_suffix(directory, timestamp, suffix);
        let file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        };
        let result = image::codecs::png::PngEncoder::new(file).write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ColorType::Rgba8,
        );
        return match result {
            Ok(()) => Ok(path),
            Err(error) => {
                let _ = std::fs::remove_file(&path);
                Err(error.to_string())
            }
        };
    }
    unreachable!()
}

fn png_path_with_suffix(directory: &Path, timestamp: &str, suffix: u64) -> PathBuf {
    let base = format!("multi_launcher_screen_draw_{timestamp}");
    if suffix == 0 {
        directory.join(format!("{base}.png"))
    } else {
        directory.join(format!("{base}_{suffix}.png"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use image::{Rgba, RgbaImage};

    use super::*;
    use crate::mkmacro::screen::{CapturedRegion, ScreenRect};
    use crate::screen_draw::{
        AnnotationDocument, AnnotationKind, ArrowAnnotation, LineAnnotation, RgbaColor,
        ScreenDrawGeneration, ShapeAnnotation, ShapeStyle, Stroke, StrokePoint, TextAnnotation,
        TransientInk,
    };

    fn source(image: RgbaImage, origin: (i32, i32)) -> ExportSource {
        ExportSource {
            snapshot: ScreenDrawSessionSnapshot::new(
                ScreenDrawGeneration::from_raw(7),
                CapturedRegion { image, origin },
            ),
            objects: Vec::new(),
            transient: Vec::new(),
            now: Duration::ZERO,
        }
    }

    fn request(background: ExportBackground) -> ExportRequest {
        ExportRequest {
            scope: ExportScope::FullDesktop,
            background,
            destination: ExportDestination::File,
        }
    }

    #[test]
    fn every_background_is_composed_from_the_immutable_capture() {
        let frozen = RgbaImage::from_fn(2, 1, |x, _| Rgba([10 + x as u8, 20, 30, 255]));
        let source = source(frozen.clone(), (-20, 5));
        assert_eq!(
            compose_export(request(ExportBackground::FrozenDesktop), &source).unwrap(),
            frozen
        );
        for (background, expected) in [
            (ExportBackground::Transparent, [0, 0, 0, 0]),
            (ExportBackground::White, [255, 255, 255, 255]),
            (ExportBackground::Black, [0, 0, 0, 255]),
            (
                ExportBackground::Solid(RgbaColor::rgba(4, 5, 6, 7)),
                [4, 5, 6, 7],
            ),
        ] {
            let output = compose_export(request(background), &source).unwrap();
            assert!(output.pixels().all(|pixel| pixel.0 == expected));
        }
    }

    #[test]
    fn signed_region_translation_clips_to_the_captured_desktop() {
        let frozen = RgbaImage::from_pixel(2, 2, Rgba([9, 8, 7, 255]));
        let source = source(frozen, (-2, -1));
        let output = compose_export(
            ExportRequest {
                scope: ExportScope::Region(ScreenRect::new(-3, -2, 4, 4)),
                background: ExportBackground::FrozenDesktop,
                destination: ExportDestination::File,
            },
            &source,
        )
        .unwrap();
        assert_eq!(output.dimensions(), (2, 2));
        assert!(output.pixels().all(|pixel| pixel.0 == [9, 8, 7, 255]));
    }

    #[test]
    fn empty_and_fully_outside_regions_are_rejected() {
        let source = source(RgbaImage::new(20, 10), (-10, -5));
        for rect in [
            ScreenRect::new(-10, -5, 0, 4),
            ScreenRect::new(100, 100, 8, 8),
        ] {
            let error = compose_export(
                ExportRequest {
                    scope: ExportScope::Region(rect),
                    background: ExportBackground::Transparent,
                    destination: ExportDestination::Clipboard,
                },
                &source,
            )
            .unwrap_err();
            assert!(error.contains("empty") || error.contains("outside"));
        }
    }

    #[test]
    fn shared_raster_draws_every_annotation_kind_in_full_desktop_coordinates() {
        let mut document = AnnotationDocument::default();
        let style = ShapeStyle {
            color: RgbaColor::RED,
            thickness: 2.0,
        };
        let p = |x, y| DesktopPoint::new(x, y);
        let stroke = |from, to| Stroke {
            points: vec![StrokePoint::mouse(from), StrokePoint::mouse(to)],
            color: RgbaColor::RED,
            thickness: 2.0,
        };
        for kind in [
            AnnotationKind::Pen(stroke(p(-9, -9), p(-6, -9))),
            AnnotationKind::Highlighter(stroke(p(-9, -7), p(-6, -7))),
            AnnotationKind::Line(LineAnnotation {
                from: p(-9, -5),
                to: p(-6, -5),
                style,
            }),
            AnnotationKind::Arrow(ArrowAnnotation {
                from: p(-9, -3),
                to: p(-6, -3),
                style,
            }),
            AnnotationKind::Rectangle(ShapeAnnotation {
                from: p(-4, -9),
                to: p(-1, -7),
                style,
            }),
            AnnotationKind::Ellipse(ShapeAnnotation {
                from: p(-4, -6),
                to: p(-1, -3),
                style,
            }),
            AnnotationKind::Text(TextAnnotation {
                text: "A".into(),
                bounds: DesktopRect::new(-9, -1, 5, 5),
                color: RgbaColor::RED,
                font_size: 4.0,
            }),
        ] {
            document.commit(kind).unwrap();
        }
        let mut source = source(RgbaImage::new(10, 14), (-10, -10));
        source.objects = document.objects().to_vec();
        let output = compose_export(request(ExportBackground::Transparent), &source).unwrap();
        assert!(output.pixels().filter(|pixel| pixel.0[3] > 0).count() >= 15);
    }

    #[test]
    fn fading_ink_uses_injected_time_and_excludes_expired_strokes_without_mutation() {
        let mut ink = TransientInk::default();
        ink.add(
            Stroke {
                points: vec![
                    StrokePoint::mouse(DesktopPoint::new(1, 1)),
                    StrokePoint::mouse(DesktopPoint::new(4, 1)),
                ],
                color: RgbaColor::rgba(100, 20, 30, 200),
                thickness: 2.0,
            },
            Duration::ZERO,
            Duration::from_secs(2),
        )
        .unwrap();
        let mut source = source(RgbaImage::new(6, 3), (0, 0));
        source.transient = ink.snapshot();
        source.now = Duration::from_secs(1);
        let alive = compose_export(request(ExportBackground::Transparent), &source).unwrap();
        assert!(
            alive
                .pixels()
                .any(|pixel| pixel.0[3] > 0 && pixel.0[3] <= 100)
        );
        source.now = Duration::from_secs(2);
        let expired = compose_export(request(ExportBackground::Transparent), &source).unwrap();
        assert!(expired.pixels().all(|pixel| pixel.0[3] == 0));
        assert_eq!(ink.strokes().len(), 1);
    }

    #[test]
    fn unique_png_names_never_overwrite_and_saved_pixels_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let image = RgbaImage::from_pixel(2, 1, Rgba([1, 2, 3, 4]));
        let first = save_png_unique(&image, directory.path(), "20260911_123456").unwrap();
        let replacement = RgbaImage::from_pixel(2, 1, Rgba([9, 8, 7, 6]));
        let second = save_png_unique(&replacement, directory.path(), "20260911_123456").unwrap();
        assert_eq!(
            first.file_name().unwrap(),
            "multi_launcher_screen_draw_20260911_123456.png"
        );
        assert_eq!(
            second.file_name().unwrap(),
            "multi_launcher_screen_draw_20260911_123456_1.png"
        );
        assert_eq!(image::open(first).unwrap().to_rgba8(), image);
        assert_eq!(image::open(second).unwrap().to_rgba8(), replacement);
    }

    struct RecordingDestination {
        fail: bool,
        seen: Mutex<Vec<(ExportDestination, (u32, u32))>>,
    }

    impl ExportDestinationBackend for RecordingDestination {
        fn deliver(
            &self,
            destination: ExportDestination,
            image: RgbaImage,
        ) -> Result<ExportOutcome, String> {
            self.seen
                .lock()
                .unwrap()
                .push((destination, image.dimensions()));
            if self.fail {
                Err("fixture destination failure".into())
            } else {
                Ok(ExportOutcome::Clipboard)
            }
        }
    }

    #[test]
    fn destination_seam_receives_moved_composite_and_propagates_failure() {
        for fail in [false, true] {
            let destination = Arc::new(RecordingDestination {
                fail,
                seen: Mutex::new(Vec::new()),
            });
            let result = execute_export(
                ExportRequest {
                    destination: ExportDestination::Clipboard,
                    ..request(ExportBackground::Black)
                },
                source(RgbaImage::new(3, 2), (-1, -1)),
                destination.clone(),
            );
            assert_eq!(result.is_err(), fail);
            assert_eq!(
                destination.seen.lock().unwrap().as_slice(),
                &[(ExportDestination::Clipboard, (3, 2))]
            );
        }
    }
}
