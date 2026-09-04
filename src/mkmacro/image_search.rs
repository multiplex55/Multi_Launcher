//! Exact-size, non-scaling RGBA template and pixel search.
use crate::mkmacro::MkMacroStore;
use crate::mkmacro::{
    CapturedRegion, DiagnosticKind, ExecResult, ExecutionDiagnostic, cancelled_error,
};
use image::{Rgba, RgbaImage};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use super::{
    MkImageNotFoundPolicy, MkImageOutputs, MkImagePayload, MkImageRef, MkPoint,
    ScreenCaptureBackend, SearchRegion, VisualSearch,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFileIdentity {
    pub path: PathBuf,
    pub reference: MkImageRef,
    pub len: u64,
    pub modified: Option<std::time::SystemTime>,
}

#[derive(Clone)]
struct DecodedImage {
    image: Arc<RgbaImage>,
    identity: ImageFileIdentity,
}

/// Run-scoped decode cache. Construct one for each playback; repeated visual waits
/// and searches using the same stable reference share the decoded pixels.
#[derive(Default)]
pub struct ImageDecodeCache {
    images: HashMap<(PathBuf, MkImageRef), DecodedImage>,
}
impl ImageDecodeCache {
    pub fn get_or_decode(
        &mut self,
        store: &MkMacroStore,
        reference: &MkImageRef,
    ) -> ExecResult<Arc<RgbaImage>> {
        let path = store.image_path(reference).map_err(|e| {
            ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                "Reference image could not be resolved",
            )
            .context("image", reference.filename())
            .context("detail", e.to_string())
        })?;
        let root_key = store
            .asset_root()
            .canonicalize()
            .unwrap_or_else(|_| store.asset_root());
        let cache_key = (root_key, reference.clone());
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => {
                self.images.remove(&cache_key);
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::InvalidTarget,
                    "Reference image is not a regular file",
                )
                .context("image", reference.filename())
                .context("path", path.display().to_string()));
            }
            Err(e) => {
                self.images.remove(&cache_key);
                let kind = if e.kind() == std::io::ErrorKind::NotFound {
                    DiagnosticKind::TargetNotFound
                } else {
                    DiagnosticKind::Backend
                };
                let message = if kind == DiagnosticKind::TargetNotFound {
                    format!("Reference image '{}' is missing", reference.filename())
                } else {
                    format!(
                        "Reference image '{}' metadata could not be read",
                        reference.filename()
                    )
                };
                return Err(ExecutionDiagnostic::new(kind, message)
                    .context("image", reference.filename())
                    .context("path", path.display().to_string())
                    .context("detail", e.to_string()));
            }
        };
        let identity = ImageFileIdentity {
            path: path.clone(),
            reference: reference.clone(),
            len: metadata.len(),
            modified: metadata.modified().ok(),
        };
        if let Some(entry) = self.images.get(&cache_key) {
            if entry.identity == identity {
                return Ok(entry.image.clone());
            }
        }
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.images.remove(&cache_key);
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::Backend,
                    format!(
                        "Reference image '{}' could not be read",
                        reference.filename()
                    ),
                )
                .context("image", reference.filename())
                .context("path", path.display().to_string())
                .context("detail", e.to_string()));
            }
        };
        let image = match image::load_from_memory_with_format(&bytes, image::ImageFormat::Png) {
            Ok(image) => image.to_rgba8(),
            Err(e) => {
                self.images.remove(&cache_key);
                return Err(ExecutionDiagnostic::new(
                    DiagnosticKind::InvalidTarget,
                    format!(
                        "Reference image '{}' could not be decoded",
                        reference.filename()
                    ),
                )
                .context("image", reference.filename())
                .context("path", path.display().to_string())
                .context("detail", e.to_string()));
            }
        };
        if image.width() == 0 || image.height() == 0 {
            self.images.remove(&cache_key);
            return Err(ExecutionDiagnostic::new(
                DiagnosticKind::InvalidTarget,
                "Reference image has invalid dimensions",
            )
            .context("image", reference.filename())
            .context(
                "dimensions",
                format!("{}x{}", image.width(), image.height()),
            ));
        }
        let image = Arc::new(image);
        self.images.insert(
            cache_key,
            DecodedImage {
                image: image.clone(),
                identity,
            },
        );
        Ok(image)
    }
    pub fn len(&self) -> usize {
        self.images.len()
    }
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}

/// Production orchestration for one image-search attempt. Polling and deadlines
/// deliberately remain in `Executor`, so this adapter never sleeps and is easy
/// to exercise with deterministic capture fixtures.
pub struct ProductionVisualSearch {
    store: Arc<MkMacroStore>,
    capture: Arc<dyn ScreenCaptureBackend>,
    cache: Mutex<ImageDecodeCache>,
}

impl ProductionVisualSearch {
    pub fn new(store: Arc<MkMacroStore>, capture: Arc<dyn ScreenCaptureBackend>) -> Self {
        Self {
            store,
            capture,
            cache: Mutex::new(ImageDecodeCache::default()),
        }
    }
}

impl VisualSearch for ProductionVisualSearch {
    fn find_image(&self, macro_id: u64, payload: &MkImagePayload) -> ExecResult<Option<MkPoint>> {
        let reference = &payload.image;
        let needle = self
            .cache
            .lock()
            .unwrap()
            .get_or_decode(&self.store, reference)
            .map_err(|e| {
                e.context("image", reference.filename())
                    .context("macro_id", macro_id.to_string())
            })?;
        let frame = self.capture.capture(&payload.region, &|| false)?;
        find_template(
            &frame,
            &needle,
            MatchOptions {
                tolerance: payload.tolerance,
                alpha: payload.alpha,
                return_point: payload.return_point,
                first_result: true,
            },
            &|| false,
        )
        .map(|point| point.map(|(x, y)| MkPoint { x, y }))
    }

    fn read_pixel(&self, point: MkPoint) -> ExecResult<[u8; 4]> {
        let rect = super::ScreenRect::new(point.x, point.y, 1, 1);
        let frame = self
            .capture
            .capture(&SearchRegion::Rectangle { rect }, &|| false)?;
        Ok(frame.image.get_pixel(0, 0).0)
    }

    fn find_pixel(
        &self,
        payload: &crate::mkmacro::MkPixelSearchPayload,
    ) -> ExecResult<Option<MkPoint>> {
        let rgb = crate::mkmacro::screen::parse_rgb(&payload.color)?;
        let frame = self.capture.capture(&payload.region, &|| false)?;
        find_pixel(
            &frame,
            Rgba([rgb[0], rgb[1], rgb[2], 255]),
            payload.tolerance,
            AlphaPolicy::Ignore,
            PixelScanOrder::TopLeftRows,
            &|| false,
        )
        .map(|p| p.map(|(x, y)| MkPoint { x, y }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ReturnPoint {
    TopLeft,
    #[default]
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum AlphaPolicy {
    #[default]
    Compare,
    Ignore,
}

#[derive(Debug, Clone, Copy)]
pub struct MatchOptions {
    pub tolerance: u8,
    pub alpha: AlphaPolicy,
    pub return_point: ReturnPoint,
    pub first_result: bool,
}
impl Default for MatchOptions {
    fn default() -> Self {
        Self {
            tolerance: 0,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
            first_result: true,
        }
    }
}

/// Searches rows top-to-bottom and columns left-to-right. Anchors (corners and
/// center, de-duplicated) are checked first; survivors receive a complete comparison.
pub fn find_template(
    frame: &CapturedRegion,
    needle: &RgbaImage,
    options: MatchOptions,
    cancelled: &dyn Fn() -> bool,
) -> ExecResult<Option<(i32, i32)>> {
    if needle.width() == 0 || needle.height() == 0 {
        return Err(invalid("reference image is empty"));
    }
    if needle.width() > frame.image.width() || needle.height() > frame.image.height() {
        return Err(invalid(format!(
            "reference image {}x{} is larger than captured region {}x{}",
            needle.width(),
            needle.height(),
            frame.image.width(),
            frame.image.height()
        )));
    }
    let mut anchors = vec![
        (0, 0),
        (needle.width() - 1, 0),
        (0, needle.height() - 1),
        (needle.width() - 1, needle.height() - 1),
        (needle.width() / 2, needle.height() / 2),
    ];
    anchors.sort_unstable();
    anchors.dedup();
    for y in 0..=frame.image.height() - needle.height() {
        for x in 0..=frame.image.width() - needle.width() {
            if cancelled() {
                return Err(cancelled_error());
            }
            if !anchors.iter().all(|&(ax, ay)| {
                pixel_eq(
                    frame.image.get_pixel(x + ax, y + ay),
                    needle.get_pixel(ax, ay),
                    options.tolerance,
                    options.alpha,
                )
            }) {
                continue;
            }
            let mut ok = true;
            'pixels: for ny in 0..needle.height() {
                for nx in 0..needle.width() {
                    if cancelled() {
                        return Err(cancelled_error());
                    }
                    if !pixel_eq(
                        frame.image.get_pixel(x + nx, y + ny),
                        needle.get_pixel(nx, ny),
                        options.tolerance,
                        options.alpha,
                    ) {
                        ok = false;
                        break 'pixels;
                    }
                }
            }
            if ok {
                let local = match options.return_point {
                    ReturnPoint::TopLeft => (x, y),
                    ReturnPoint::Center => (x + needle.width() / 2, y + needle.height() / 2),
                };
                return Ok(frame.desktop_point(local));
            }
        }
    }
    Ok(None)
}
fn pixel_eq(a: &Rgba<u8>, b: &Rgba<u8>, tolerance: u8, alpha: AlphaPolicy) -> bool {
    let n = if alpha == AlphaPolicy::Compare { 4 } else { 3 };
    (0..n).all(|i| a.0[i].abs_diff(b.0[i]) <= tolerance)
}
fn invalid(s: impl Into<String>) -> ExecutionDiagnostic {
    ExecutionDiagnostic::new(DiagnosticKind::InvalidTarget, s)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum PixelScanOrder {
    #[default]
    TopLeftRows,
    TopRightRows,
    BottomLeftRows,
    BottomRightRows,
    LeftTopColumns,
    RightTopColumns,
}
pub fn find_pixel(
    frame: &CapturedRegion,
    color: Rgba<u8>,
    tolerance: u8,
    alpha: AlphaPolicy,
    order: PixelScanOrder,
    cancelled: &dyn Fn() -> bool,
) -> ExecResult<Option<(i32, i32)>> {
    let (w, h) = frame.image.dimensions();
    let mut points: Vec<(u32, u32)> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).collect();
    match order {
        PixelScanOrder::TopLeftRows => {}
        PixelScanOrder::TopRightRows => points.sort_by_key(|&(x, y)| (y, std::cmp::Reverse(x))),
        PixelScanOrder::BottomLeftRows => points.sort_by_key(|&(x, y)| (std::cmp::Reverse(y), x)),
        PixelScanOrder::BottomRightRows => {
            points.sort_by_key(|&(x, y)| (std::cmp::Reverse(y), std::cmp::Reverse(x)))
        }
        PixelScanOrder::LeftTopColumns => points.sort_by_key(|&(x, y)| (x, y)),
        PixelScanOrder::RightTopColumns => points.sort_by_key(|&(x, y)| (std::cmp::Reverse(x), y)),
    }
    for p in points {
        if cancelled() {
            return Err(cancelled_error());
        }
        if pixel_eq(frame.image.get_pixel(p.0, p.1), &color, tolerance, alpha) {
            return Ok(frame.desktop_point(p));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    fn cap(w: u32, h: u32) -> CapturedRegion {
        CapturedRegion {
            image: RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 255])),
            origin: (-5, -7),
        }
    }
    #[test]
    fn exact_edges_center_and_no_match() {
        for (x, y) in [(0, 0), (2, 0), (0, 2), (2, 2)] {
            let mut f = cap(4, 4);
            for dy in 0..2 {
                for dx in 0..2 {
                    f.image.put_pixel(x + dx, y + dy, Rgba([9, 8, 7, 255]))
                }
            }
            let n = RgbaImage::from_pixel(2, 2, Rgba([9, 8, 7, 255]));
            assert_eq!(
                find_template(&f, &n, MatchOptions::default(), &|| false).unwrap(),
                Some((-5 + x as i32 + 1, -7 + y as i32 + 1))
            );
        }
        assert_eq!(
            find_template(
                &cap(3, 3),
                &RgbaImage::from_pixel(1, 1, Rgba([1, 1, 1, 255])),
                MatchOptions::default(),
                &|| false
            )
            .unwrap(),
            None
        )
    }
    #[test]
    fn tolerance_and_alpha() {
        let mut f = cap(1, 1);
        f.image.put_pixel(0, 0, Rgba([10, 20, 30, 1]));
        let n = RgbaImage::from_pixel(1, 1, Rgba([12, 20, 30, 255]));
        let mut o = MatchOptions {
            tolerance: 2,
            ..Default::default()
        };
        assert_eq!(find_template(&f, &n, o, &|| false).unwrap(), None);
        o.alpha = AlphaPolicy::Ignore;
        assert!(find_template(&f, &n, o, &|| false).unwrap().is_some());
        o.tolerance = 1;
        assert_eq!(find_template(&f, &n, o, &|| false).unwrap(), None)
    }
    #[test]
    fn cancellation_is_reported_without_finishing_the_scan() {
        let calls = AtomicUsize::new(0);
        let error = find_template(
            &cap(8, 8),
            &RgbaImage::from_pixel(2, 2, Rgba([1, 1, 1, 255])),
            MatchOptions::default(),
            &|| calls.fetch_add(1, Ordering::SeqCst) >= 2,
        )
        .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Cancelled);
    }

    struct FakeCapture {
        frame: CapturedRegion,
        captures: AtomicUsize,
    }
    impl ScreenCaptureBackend for FakeCapture {
        fn virtual_desktop(&self) -> ExecResult<super::super::ScreenRect> {
            Ok(self.frame.rect())
        }
        fn region_bounds(&self, _: &SearchRegion) -> ExecResult<super::super::ScreenRect> {
            Ok(self.frame.rect())
        }
        fn capture_rect(
            &self,
            _: super::super::ScreenRect,
            _: &dyn Fn() -> bool,
        ) -> ExecResult<RgbaImage> {
            self.captures.fetch_add(1, Ordering::SeqCst);
            Ok(self.frame.image.clone())
        }
    }
    fn payload(image: &str, tolerance: u8) -> MkImagePayload {
        MkImagePayload {
            image: MkImageRef::from_filename(image),
            wait: super::super::MkWaitOptions {
                timeout_ms: 0,
                poll_interval_ms: 0,
            },
            region: SearchRegion::Desktop,
            tolerance,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
            not_found_policy: MkImageNotFoundPolicy::Fail,
            outputs: MkImageOutputs::default(),
        }
    }
    fn adapter_fixture(
        frame: CapturedRegion,
        needle: &RgbaImage,
    ) -> (tempfile::TempDir, Arc<MkMacroStore>, ProductionVisualSearch) {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        store
            .write_captured_png(
                needle,
                MkImageRef::from_filename("needle.png"),
                crate::mkmacro::ImageImportChoice::SaveAs(MkImageRef::from_filename("needle.png")),
            )
            .unwrap();
        let store = Arc::new(store);
        let capture = Arc::new(FakeCapture {
            frame,
            captures: AtomicUsize::new(0),
        });
        let adapter = ProductionVisualSearch::new(store.clone(), capture);
        (dir, store, adapter)
    }
    #[test]
    fn production_adapter_loads_store_asset_forwards_tolerance_and_translates_center() {
        let mut frame = CapturedRegion {
            image: RgbaImage::from_pixel(5, 4, Rgba([0, 0, 0, 255])),
            origin: (-120, 35),
        };
        let needle = RgbaImage::from_pixel(2, 2, Rgba([10, 20, 30, 255]));
        for y in 1..3 {
            for x in 2..4 {
                frame.image.put_pixel(x, y, Rgba([12, 20, 30, 255]));
            }
        }
        let (_dir, _store, adapter) = adapter_fixture(frame, &needle);
        assert_eq!(
            adapter.find_image(7, &payload("needle.png", 1)).unwrap(),
            None
        );
        assert_eq!(
            adapter.find_image(7, &payload("needle.png", 2)).unwrap(),
            Some(MkPoint { x: -117, y: 37 })
        );
    }
    #[test]
    fn production_adapter_distinguishes_asset_failures() {
        let frame = cap(2, 2);
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let store = Arc::new(store);
        let make = || {
            ProductionVisualSearch::new(
                store.clone(),
                Arc::new(FakeCapture {
                    frame: frame.clone(),
                    captures: AtomicUsize::new(0),
                }),
            )
        };
        let image = MkImageRef::from_filename("missing.png");
        let missing = make()
            .find_image(7, &payload("missing.png", 0))
            .unwrap_err();
        let expected_path = store.image_path(&image).unwrap().display().to_string();
        assert_eq!(missing.kind, DiagnosticKind::TargetNotFound);
        assert_eq!(missing.message, "Reference image 'missing.png' is missing");
        assert_eq!(
            missing.context.get("image").map(String::as_str),
            Some("missing.png")
        );
        assert_eq!(
            missing.context.get("macro_id").map(String::as_str),
            Some("7")
        );
        assert_eq!(
            missing.context.get("path").map(String::as_str),
            Some(expected_path.as_str())
        );

        let path = store.image_path(&image).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"not a png").unwrap();
        let undecodable = make()
            .find_image(7, &payload("missing.png", 0))
            .unwrap_err();
        assert_eq!(undecodable.kind, DiagnosticKind::InvalidTarget);
        assert_eq!(
            undecodable.message,
            "Reference image 'missing.png' could not be decoded"
        );
        assert_eq!(
            undecodable.context.get("path").map(String::as_str),
            Some(expected_path.as_str())
        );

        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let unreadable = make()
            .find_image(7, &payload("missing.png", 0))
            .unwrap_err();
        assert_eq!(unreadable.kind, DiagnosticKind::InvalidTarget);
        assert_eq!(unreadable.message, "Reference image is not a regular file");
        assert_eq!(
            unreadable.context.get("path").map(String::as_str),
            Some(expected_path.as_str())
        );
    }

    #[test]
    fn decode_cache_reloads_after_same_filename_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let image = MkImageRef::from_filename("shared.png");
        let first = RgbaImage::from_pixel(2, 2, Rgba([10, 20, 30, 255]));
        store
            .write_captured_png(
                &first,
                image.clone(),
                crate::mkmacro::ImageImportChoice::SaveAs(image.clone()),
            )
            .unwrap();
        let mut cache = ImageDecodeCache::default();
        let decoded_first = cache.get_or_decode(&store, &image).unwrap();
        let second = RgbaImage::from_pixel(2, 2, Rgba([230, 220, 210, 255]));
        store
            .write_captured_png(
                &second,
                image.clone(),
                crate::mkmacro::ImageImportChoice::ReplaceExisting,
            )
            .unwrap();
        let decoded_second = cache.get_or_decode(&store, &image).unwrap();
        assert_ne!(decoded_first.as_raw(), decoded_second.as_raw());
        assert_eq!(decoded_second.get_pixel(0, 0), &Rgba([230, 220, 210, 255]));
        assert_eq!(cache.len(), 1);
    }
    #[test]
    fn too_large_and_deterministic() {
        let mut f = cap(3, 1);
        f.image.put_pixel(0, 0, Rgba([1, 1, 1, 255]));
        f.image.put_pixel(2, 0, Rgba([1, 1, 1, 255]));
        let n = RgbaImage::from_pixel(1, 1, Rgba([1, 1, 1, 255]));
        assert_eq!(
            find_template(
                &f,
                &n,
                MatchOptions {
                    return_point: ReturnPoint::TopLeft,
                    ..Default::default()
                },
                &|| false
            )
            .unwrap(),
            Some((-5, -7))
        );
        assert!(find_template(&f, &RgbaImage::new(4, 1), Default::default(), &|| false).is_err())
    }
    #[test]
    fn pixel_orders() {
        let mut f = cap(2, 2);
        for p in [(1, 0), (0, 1)] {
            f.image.put_pixel(p.0, p.1, Rgba([5, 5, 5, 255]))
        }
        assert_eq!(
            find_pixel(
                &f,
                Rgba([5, 5, 5, 255]),
                0,
                AlphaPolicy::Compare,
                PixelScanOrder::TopLeftRows,
                &|| false
            )
            .unwrap(),
            Some((-4, -7))
        );
        assert_eq!(
            find_pixel(
                &f,
                Rgba([6, 5, 5, 255]),
                1,
                AlphaPolicy::Compare,
                PixelScanOrder::LeftTopColumns,
                &|| false
            )
            .unwrap(),
            Some((-5, -6))
        )
    }
}
