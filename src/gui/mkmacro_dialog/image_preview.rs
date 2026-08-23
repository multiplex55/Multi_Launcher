//! Asynchronous, bounded previews for macro PNG assets.
//!
//! Previewing deliberately has three phases: cache inspection, background file
//! validation/decoding, and UI-thread texture creation.  In particular, the
//! egui data lock is never held while doing I/O, image work, or loading a texture.
use crate::mkmacro::MkMacroStore;
use eframe::egui;
use image::ImageDecoder;
use std::{
    collections::HashMap,
    fs,
    io::Cursor,
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant, SystemTime},
};

pub const PREVIEW_BOUND: f32 = 220.0;
const VALIDATE_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct PreviewKey {
    macro_id: u64,
    asset_id: u64,
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
    assets: HashMap<(u64, u64), AssetState>,
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
    macro_id: u64,
    asset_id: u64,
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
    if width <= PREVIEW_BOUND as u32 && height <= PREVIEW_BOUND as u32 {
        return (width, height);
    }
    let scale = (PREVIEW_BOUND / width as f32).min(PREVIEW_BOUND / height as f32);
    (
        (width as f32 * scale).round().max(1.0) as u32,
        (height as f32 * scale).round().max(1.0) as u32,
    )
}

fn key_from_metadata(macro_id: u64, asset_id: u64, metadata: &fs::Metadata) -> PreviewKey {
    PreviewKey {
        macro_id,
        asset_id,
        len: metadata.len(),
        modified: metadata.modified().ok(),
    }
}

fn decode_job(job: &Job) -> DecodeResult {
    let missing_key = PreviewKey {
        macro_id: job.macro_id,
        asset_id: job.asset_id,
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
    let key = key_from_metadata(job.macro_id, job.asset_id, &metadata);
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
        .entry((key.macro_id, key.asset_id))
        .or_default();
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
    macro_id: u64,
    asset_id: u64,
    now: Instant,
) -> Inspection {
    let results: Vec<_> = cache.receiver.lock().unwrap().try_iter().collect();
    for result in results {
        apply_result(cache, result);
    }
    let sender = cache.sender.clone();
    let state = cache.assets.entry((macro_id, asset_id)).or_default();
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
            macro_id,
            asset_id,
            previous_key: state.key.clone(),
            sender,
        });
    }
    result
}

pub fn show(ui: &mut egui::Ui, store: &MkMacroStore, macro_id: u64, asset_id: u64) {
    let Ok(path) = store.asset_path(macro_id, asset_id) else {
        ui.colored_label(egui::Color32::RED, "No reference image selected");
        return;
    };
    let ctx = ui.ctx().clone();
    let cache_id = egui::Id::new("mkmacro-image-preview-cache-v2");
    // Phase one: a short cache-only egui data section.
    let inspection = ctx.data_mut(|data| {
        inspect_cache(
            data.get_temp_mut_or_default::<PreviewCache>(cache_id),
            path.clone(),
            macro_id,
            asset_id,
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
            format!(
                "mkmacro-preview-{}-{}-{}",
                macro_id, asset_id, decoded.key.len
            ),
            egui::ColorImage::from_rgba_unmultiplied(decoded.thumbnail_size, &decoded.rgba),
            Default::default(),
        );
        (decoded, texture)
    });
    let uploaded_ready = uploaded.and_then(|(decoded, texture)| ctx.data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<PreviewCache>(cache_id);
        let state = cache.assets.get_mut(&(macro_id, asset_id))?;
        if state.key.as_ref() != Some(&decoded.key) || !matches!(&state.outcome, Some(Outcome::Decoded(current)) if current.key == decoded.key) { return None; }
        let ready = CachedPreview { texture, width: decoded.width, height: decoded.height, thumbnail_size: decoded.thumbnail_size };
        state.outcome = Some(Outcome::Ready(ready.clone()));
        Some(ready)
    }));
    if let Some(ready) = uploaded_ready.or(inspection.ready) {
        ui.image((
            ready.texture.id(),
            egui::vec2(
                ready.thumbnail_size[0] as f32,
                ready.thumbnail_size[1] as f32,
            ),
        ));
        ui.label(format!(
            "{} — {} × {} px",
            path.file_name().unwrap_or_default().to_string_lossy(),
            ready.width,
            ready.height
        ));
        ui.small(format!("Asset ID {asset_id}"));
    } else if let Some(error) = inspection.failed {
        ui.colored_label(egui::Color32::RED, format!("{error} (asset {asset_id})"));
    } else {
        ui.small("Loading reference image preview...");
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
        for (w, h) in [(800, 200), (200, 800), (800, 800), (20, 10)] {
            let (tw, th) = thumbnail_dimensions(w, h);
            assert!(tw <= 220 && th <= 220);
            assert!((tw as f32 / th as f32 - w as f32 / h as f32).abs() < 0.01);
        }
        assert_eq!(thumbnail_dimensions(20, 10), (20, 10));
    }

    #[test]
    fn decode_allocates_only_thumbnail_rgba() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.png");
        fs::write(&path, png(1000, 500)).unwrap();
        let (tx, _) = mpsc::channel();
        let result = decode_job(&Job {
            path,
            macro_id: 1,
            asset_id: 2,
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
    fn corrupt_and_non_png_are_stable_cached_failures() {
        for bytes in [b"garbage".to_vec(), {
            let mut c = Cursor::new(Vec::new());
            DynamicImage::ImageRgba8(RgbaImage::new(1, 1))
                .write_to(&mut c, ImageFormat::Jpeg)
                .unwrap();
            c.into_inner()
        }] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("x.png");
            fs::write(&path, bytes).unwrap();
            let (tx, _) = mpsc::channel();
            let job = Job {
                path,
                macro_id: 1,
                asset_id: 1,
                previous_key: None,
                sender: tx,
            };
            let DecodeResult::Failed(key, _) = decode_job(&job) else {
                panic!()
            };
            let mut retry = job;
            retry.previous_key = Some(key.clone());
            assert!(matches!(decode_job(&retry),DecodeResult::Unchanged(k) if k==key));
        }
    }

    #[test]
    fn keys_change_with_length_or_modification_time() {
        let a = PreviewKey {
            macro_id: 1,
            asset_id: 2,
            len: 3,
            modified: None,
        };
        assert_ne!(
            a,
            PreviewKey {
                len: 4,
                ..a.clone()
            }
        );
        assert_ne!(
            a,
            PreviewKey {
                modified: Some(SystemTime::UNIX_EPOCH),
                ..a.clone()
            }
        );
    }

    #[test]
    fn repeated_polling_starts_one_job_and_texture_step_is_outside_access() {
        let mut cache = PreviewCache::default();
        let now = Instant::now();
        assert!(
            inspect_cache(&mut cache, "x".into(), 1, 2, now)
                .job
                .is_some()
        );
        assert!(
            inspect_cache(&mut cache, "x".into(), 1, 2, now)
                .job
                .is_none()
        );
        let access = std::cell::Cell::new(false);
        let loader = || assert!(!access.get());
        access.set(true);
        let _ = &mut cache;
        access.set(false);
        loader();
    }

    #[test]
    fn stale_completion_is_discarded_before_texture_insertion() {
        let mut cache = PreviewCache::default();
        let old = PreviewKey {
            macro_id: 1,
            asset_id: 2,
            len: 1,
            modified: None,
        };
        let new = PreviewKey {
            len: 2,
            ..old.clone()
        };
        let state = cache.assets.entry((1, 2)).or_default();
        state.key = Some(new.clone());
        state.outcome = Some(Outcome::Failed("new".into()));
        assert_ne!(state.key.as_ref(), Some(&old));
        assert!(matches!(state.outcome, Some(Outcome::Failed(_))));
    }

    #[test]
    fn state_machine_progresses_across_ticks_without_blocking_actions() {
        let mut cache = PreviewCache::default();
        let now = Instant::now();
        let first = inspect_cache(&mut cache, "unused".into(), 9, 8, now);
        assert!(first.pending && first.job.is_some());
        let key = PreviewKey {
            macro_id: 9,
            asset_id: 8,
            len: 100,
            modified: None,
        };
        cache
            .sender
            .send(DecodeResult::Decoded(DecodedPreview {
                key: key.clone(),
                width: 4000,
                height: 2000,
                thumbnail_size: [220, 110],
                rgba: Arc::from(vec![0; 220 * 110 * 4]),
            }))
            .unwrap();
        let mut other_editor_actions = 0;
        other_editor_actions += 1;
        let second = inspect_cache(&mut cache, "unused".into(), 9, 8, now);
        assert!(second.decoded.is_some());
        assert_eq!(other_editor_actions, 1); // the UI tick remained free to process unrelated work
        let state = cache.assets.get_mut(&(9, 8)).unwrap();
        state.outcome = Some(Outcome::Failed("fake texture-ready adapter".into()));
        assert_eq!(state.key.as_ref(), Some(&key));
    }
}
