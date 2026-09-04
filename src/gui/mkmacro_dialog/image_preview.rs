//! Asynchronous, bounded previews for macro PNG assets.
//!
//! Previewing deliberately has three phases: cache inspection, background file
//! validation/decoding, and UI-thread texture creation.  In particular, the
//! egui data lock is never held while doing I/O, image work, or loading a texture.
use crate::mkmacro::{MkImageRef, MkMacroStore};
use eframe::egui;
use image::ImageDecoder;
use std::{
    collections::HashMap,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant, SystemTime},
};

pub const PREVIEW_BOUND: f32 = 220.0;
/// Compact bound used inside a coordinate target form.
pub const TARGET_THUMBNAIL_BOUND: f32 = 140.0;
const VALIDATE_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct PreviewLookupKey {
    pub root: PathBuf,
    pub image: MkImageRef,
}

impl PreviewLookupKey {
    pub fn new(image: MkImageRef) -> Self {
        Self {
            root: PathBuf::new(),
            image,
        }
    }

    pub fn new_for_root(root: &Path, image: MkImageRef) -> Self {
        Self {
            root: root.to_path_buf(),
            image,
        }
    }

    pub fn new_for_store(store: &MkMacroStore, image: MkImageRef) -> Self {
        let root = store
            .asset_root()
            .canonicalize()
            .unwrap_or_else(|_| store.asset_root());
        Self::new_for_root(&root, image)
    }
}

const PREVIEW_CACHE_ID: &str = "mkmacro-image-preview-cache-v2";

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct PreviewKey {
    image: MkImageRef,
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Clone)]
struct CachedPreview {
    texture: egui::TextureHandle,
    width: u32,
    height: u32,
    thumbnail_size: [usize; 2],
}

#[derive(Clone, Debug)]
struct DecodedPreview {
    key: PreviewKey,
    width: u32,
    height: u32,
    thumbnail_size: [usize; 2],
    rgba: Arc<[u8]>,
}

#[derive(Clone, Debug)]
enum DecodeResult {
    Decoded(DecodedPreview),
    Failed(PreviewKey, String),
    Unchanged(PreviewKey),
}

#[derive(Clone)]
enum Outcome {
    Decoded(DecodedPreview),
    Ready(CachedPreview),
    Failed(String),
}

#[derive(Clone, Default)]
struct AssetState {
    key: Option<PreviewKey>,
    outcome: Option<Outcome>,
    pending: bool,
    last_validation: Option<Instant>,
}

#[derive(Clone)]
struct PreviewCache {
    assets: HashMap<(PathBuf, MkImageRef), AssetState>,
    sender: mpsc::Sender<DecodeResult>,
    receiver: Arc<Mutex<mpsc::Receiver<DecodeResult>>>,
}

impl Default for PreviewCache {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            assets: HashMap::new(),
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }
}

#[derive(Clone)]
struct Job {
    path: PathBuf,
    image: MkImageRef,
    previous_key: Option<PreviewKey>,
    sender: mpsc::Sender<DecodeResult>,
}

#[derive(Default)]
struct Inspection {
    ready: Option<CachedPreview>,
    failed: Option<String>,
    decoded: Option<DecodedPreview>,
    job: Option<Job>,
    pending: bool,
}

pub fn fitted_size(width: u32, height: u32, bound: f32) -> egui::Vec2 {
    if width == 0 || height == 0 {
        return egui::Vec2::ZERO;
    }
    let scale = (bound / width as f32).min(bound / height as f32).min(1.0);
    egui::vec2(width as f32 * scale, height as f32 * scale)
}

fn thumbnail_dimensions(width: u32, height: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (0, 0);
    }
    if width <= PREVIEW_BOUND as u32 && height <= PREVIEW_BOUND as u32 {
        return (width, height);
    }
    let scale = (PREVIEW_BOUND / width as f32).min(PREVIEW_BOUND / height as f32);
    (
        (width as f32 * scale).round().max(1.0) as u32,
        (height as f32 * scale).round().max(1.0) as u32,
    )
}

fn key_from_metadata(image: MkImageRef, path: PathBuf, metadata: &fs::Metadata) -> PreviewKey {
    PreviewKey {
        image,
        path,
        len: metadata.len(),
        modified: metadata.modified().ok(),
    }
}

fn decode_job(job: &Job) -> DecodeResult {
    let missing_key = PreviewKey {
        image: job.image.clone(),
        path: job.path.clone(),
        len: 0,
        modified: None,
    };
    let metadata = match fs::metadata(&job.path) {
        Ok(value) => value,
        Err(error) => {
            if job.previous_key.as_ref() == Some(&missing_key) {
                return DecodeResult::Unchanged(missing_key);
            }
            return DecodeResult::Failed(missing_key, format!("Missing reference image: {error}"));
        }
    };
    let key = key_from_metadata(job.image.clone(), job.path.clone(), &metadata);
    if job.previous_key.as_ref() == Some(&key) {
        return DecodeResult::Unchanged(key);
    }
    let result = (|| -> anyhow::Result<DecodedPreview> {
        let bytes = fs::read(&job.path)?;
        let decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes))
            .map_err(|error| anyhow::anyhow!("Not a valid PNG reference image: {error}"))?;
        let (width, height) = decoder.dimensions();
        crate::mkmacro::asset_authoring::validated_rgba_len(width, height)?;
        let image = image::DynamicImage::from_decoder(decoder)?.to_rgba8();
        let (thumb_width, thumb_height) = thumbnail_dimensions(width, height);
        // Triangle is a fast, smooth filter suitable for small UI previews.
        let thumbnail = image::imageops::resize(
            &image,
            thumb_width,
            thumb_height,
            image::imageops::FilterType::Triangle,
        );
        let thumbnail_size = [
            usize::try_from(thumb_width)?,
            usize::try_from(thumb_height)?,
        ];
        Ok(DecodedPreview {
            key: key.clone(),
            width,
            height,
            thumbnail_size,
            rgba: Arc::from(thumbnail.into_raw()),
        })
    })();
    match result {
        Ok(decoded) => DecodeResult::Decoded(decoded),
        Err(error) => {
            DecodeResult::Failed(key, format!("Missing or corrupt reference image: {error}"))
        }
    }
}

fn apply_result(cache: &mut PreviewCache, result: DecodeResult) {
    let key = match &result {
        DecodeResult::Decoded(value) => &value.key,
        DecodeResult::Failed(key, _) | DecodeResult::Unchanged(key) => key,
    };
    let state = cache
        .assets
        .entry((key.path.clone(), key.image.clone()))
        .or_default();
    // Results are useful only for the validation currently in flight. A late
    // duplicate must not replace a newer identity/outcome.
    if !state.pending {
        return;
    }
    state.pending = false;
    if matches!(result, DecodeResult::Unchanged(_)) {
        return;
    }
    // A result is accepted only if it is the validation currently in flight.
    // This also prevents an old file identity from replacing a newer one.
    state.key = Some(key.clone());
    state.outcome = Some(match result {
        DecodeResult::Decoded(value) => Outcome::Decoded(value),
        DecodeResult::Failed(_, error) => Outcome::Failed(error),
        DecodeResult::Unchanged(_) => unreachable!(),
    });
}

fn inspect_cache(
    cache: &mut PreviewCache,
    path: PathBuf,
    image: MkImageRef,
    now: Instant,
) -> Inspection {
    let results: Vec<_> = cache.receiver.lock().unwrap().try_iter().collect();
    for result in results {
        apply_result(cache, result);
    }
    let sender = cache.sender.clone();
    let state = cache
        .assets
        .entry((path.clone(), image.clone()))
        .or_default();
    let mut result = Inspection {
        pending: state.pending,
        ..Default::default()
    };
    match state.outcome.as_ref() {
        Some(Outcome::Ready(value)) => result.ready = Some(value.clone()),
        Some(Outcome::Failed(error)) => result.failed = Some(error.clone()),
        Some(Outcome::Decoded(value)) => result.decoded = Some(value.clone()),
        None => {}
    }
    let due = state
        .last_validation
        .map_or(true, |last| now.duration_since(last) >= VALIDATE_INTERVAL);
    if !state.pending && due {
        state.pending = true;
        state.last_validation = Some(now);
        result.pending = true;
        result.job = Some(Job {
            path,
            image,
            previous_key: state.key.clone(),
            sender,
        });
    }
    result
}

pub fn show(ui: &mut egui::Ui, store: &MkMacroStore, image: &MkImageRef) {
    show_sized(ui, store, image, PREVIEW_BOUND, true);
}

/// A compact view backed by the exact same asynchronous decode and texture
/// cache as [`show`]. The decoded pixels are never recreated merely because a
/// caller requests a different logical display size.
pub fn show_thumbnail(ui: &mut egui::Ui, store: &MkMacroStore, image: &MkImageRef, max_size: f32) {
    show_sized(ui, store, image, max_size.max(1.0), false);
}

/// Invalidates a managed image preview unconditionally. Metadata is not
/// consulted: an overwrite must evict the old texture even when the
/// replacement has the same length and timestamp.
pub(crate) fn invalidate(ctx: &egui::Context, store: &MkMacroStore, image: &MkImageRef) {
    let Ok(path) = store.image_path(image) else {
        return;
    };
    ctx.data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<PreviewCache>(egui::Id::new(PREVIEW_CACHE_ID));
        cache.assets.remove(&(path, image.clone()));
    });
    ctx.request_repaint();
}

fn show_sized(
    ui: &mut egui::Ui,
    store: &MkMacroStore,
    image: &MkImageRef,
    bound: f32,
    details: bool,
) {
    let lookup_key = PreviewLookupKey::new_for_store(store, image.clone());
    let Ok(path) = store.image_path(image) else {
        ui.colored_label(egui::Color32::RED, "No reference image selected");
        return;
    };
    let ctx = ui.ctx().clone();
    let cache_id = egui::Id::new(PREVIEW_CACHE_ID);
    // Phase one: a short cache-only egui data section.
    let inspection = ctx.data_mut(|data| {
        inspect_cache(
            data.get_temp_mut_or_default::<PreviewCache>(cache_id),
            path.clone(),
            lookup_key.image.clone(),
            Instant::now(),
        )
    });
    if let Some(job) = inspection.job {
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = decode_job(&job);
            let _ = job.sender.send(result);
            repaint.request_repaint();
        });
    }
    // Phase three: upload decoded pixels with no egui data lock held.
    let uploaded = inspection.decoded.map(|decoded| {
        let texture = ctx.load_texture(
            format!("mkmacro-preview-{}-{}", image.filename(), decoded.key.len),
            egui::ColorImage::from_rgba_unmultiplied(decoded.thumbnail_size, &decoded.rgba),
            Default::default(),
        );
        (decoded, texture)
    });
    let uploaded_ready = uploaded.and_then(|(decoded, texture)| ctx.data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<PreviewCache>(cache_id);
        let state = cache.assets.get_mut(&(path.clone(), image.clone()))?;
        if state.key.as_ref() != Some(&decoded.key) || !matches!(&state.outcome, Some(Outcome::Decoded(current)) if current.key == decoded.key) { return None; }
        let ready = CachedPreview { texture, width: decoded.width, height: decoded.height, thumbnail_size: decoded.thumbnail_size };
        state.outcome = Some(Outcome::Ready(ready.clone()));
        Some(ready)
    }));
    if let Some(ready) = uploaded_ready.or(inspection.ready) {
        ui.image((
            ready.texture.id(),
            fitted_size(ready.width, ready.height, bound),
        ));
        if details {
            ui.label(format!(
                "{} — {} × {} px",
                path.file_name().unwrap_or_default().to_string_lossy(),
                ready.width,
                ready.height
            ));
        }
    } else if let Some(error) = inspection.failed {
        ui.colored_label(
            egui::Color32::RED,
            if details {
                format!("{error} ({})", image.filename())
            } else {
                "Unavailable".into()
            },
        );
    } else {
        ui.small(if details {
            "Loading reference image preview..."
        } else {
            "Loading…"
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(w, h, Rgba([1, 2, 3, 255])))
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn thumbnail_shapes_are_bounded_aspect_preserving_and_never_upscaled() {
        for (w, h) in [
            (800, 200),
            (200, 800),
            (800, 800),
            (20, 10),
            (1_000_000, 1),
            (1, 1_000_000),
        ] {
            let (tw, th) = thumbnail_dimensions(w, h);
            assert!(tw <= 220 && th <= 220);
            assert!(tw > 0 && th > 0);
            // Each rounded side is within half a pixel of the ideal scale
            // (except the deliberately clamped one-pixel minimum).
            let scale = (PREVIEW_BOUND / w as f32)
                .min(PREVIEW_BOUND / h as f32)
                .min(1.0);
            assert!((tw as f32 - w as f32 * scale).abs() <= 1.0);
            assert!((th as f32 - h as f32 * scale).abs() <= 1.0);
        }
        assert_eq!(thumbnail_dimensions(20, 10), (20, 10));
        assert_eq!(thumbnail_dimensions(1, 1), (1, 1));
        assert_eq!(thumbnail_dimensions(0, 10), (0, 0));
        assert_eq!(thumbnail_dimensions(10, 0), (0, 0));
        assert_eq!(thumbnail_dimensions(0, 0), (0, 0));
        assert_eq!(fitted_size(0, u32::MAX, PREVIEW_BOUND), egui::Vec2::ZERO);
    }

    #[test]
    fn target_thumbnail_is_bounded_for_wide_tall_and_square_images() {
        for (width, height) in [(1200, 200), (200, 1200), (1200, 1200)] {
            let size = fitted_size(width, height, TARGET_THUMBNAIL_BOUND);
            assert!(size.x <= TARGET_THUMBNAIL_BOUND);
            assert!(size.y <= TARGET_THUMBNAIL_BOUND);
            assert!((size.x / size.y - width as f32 / height as f32).abs() < 0.001);
        }
    }

    #[test]
    fn decode_allocates_only_thumbnail_rgba() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.png");
        fs::write(&path, png(1000, 500)).unwrap();
        let (tx, _) = mpsc::channel();
        let result = decode_job(&Job {
            path,
            image: MkImageRef::from_filename("big.png"),
            previous_key: None,
            sender: tx,
        });
        let DecodeResult::Decoded(value) = result else {
            panic!()
        };
        assert_eq!(value.thumbnail_size, [220, 110]);
        assert_eq!(value.rgba.len(), 220 * 110 * 4);
    }

    #[test]
    fn overwrite_of_same_filename_redecodes_and_corruption_is_not_cached_as_old_pixels() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shared.png");
        let image = MkImageRef::from_filename("shared.png");
        fs::write(&path, png(2, 2)).unwrap();
        let (tx, _) = mpsc::channel();
        let first = decode_job(&Job {
            path: path.clone(),
            image: image.clone(),
            previous_key: None,
            sender: tx.clone(),
        });
        let DecodeResult::Decoded(first) = first else {
            panic!()
        };
        fs::write(&path, png(3, 2)).unwrap();
        let second = decode_job(&Job {
            path: path.clone(),
            image: image.clone(),
            previous_key: Some(first.key),
            sender: tx.clone(),
        });
        let DecodeResult::Decoded(second) = second else {
            panic!("replacement was served from cache")
        };
        assert_eq!(second.width, 3);
        fs::write(&path, b"corrupt").unwrap();
        assert!(matches!(
            decode_job(&Job { path, image, previous_key: Some(second.key), sender: tx }),
            DecodeResult::Failed(_, message) if message.contains("corrupt")
        ));
    }

    #[test]
    fn browser_and_coordinate_target_share_the_same_filename_lookup_identity() {
        let image = MkImageRef::from_filename("shared.png");
        assert_eq!(
            PreviewLookupKey::new(image.clone()),
            PreviewLookupKey::new(image)
        );
        assert_eq!(PREVIEW_CACHE_ID, "mkmacro-image-preview-cache-v2");
    }
}
