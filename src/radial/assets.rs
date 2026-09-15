//! Bounded, preparation-time media loading for radial menus.
//!
//! GIF is the only animated format and is bounded by frame count and total
//! duration. TIFF is deliberately decoded as one static image. The `image`
//! ICO decoder selects the best directory entry before decoding it. Render and
//! hit-test code receive `PreparedMedia` snapshots and never access this service.

use super::cache::{AssetCacheKey, CachedAsset, PreparedAssetCache};
use super::model::{
    AssetRecord, MediaKind, MediaReference, MediaSearchRoots, RADIAL_ASSETS_DIRECTORY,
    RenderingQuality, limits,
};
use image::{AnimationDecoder, ImageFormat};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

pub const MAX_SOURCE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_DECODED_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_ANIMATION_FRAMES: usize = 120;
pub const MAX_ANIMATION_DURATION_MS: u64 = 30_000;
pub const MAX_WAVE_DATA_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_WAVE_DURATION_MS: u64 = 30_000;
pub const MAX_WAVE_SAMPLE_RATE: u32 = 192_000;
pub const MAX_WAVE_CHANNELS: u16 = 8;
pub const DEFAULT_CACHE_ENTRIES: usize = 256;
pub const MAX_PREVIEW_OVERLAY_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManagedAssetOverlay {
    entries: Arc<[(AssetRecord, Arc<[u8]>)]>,
}

impl ManagedAssetOverlay {
    pub fn validated(
        entries: impl IntoIterator<Item = (AssetRecord, Arc<[u8]>)>,
    ) -> Result<Self, AssetDiagnostic> {
        let entries = entries.into_iter().collect::<Vec<_>>();
        let total = entries.iter().try_fold(0usize, |total, (record, bytes)| {
            if bytes.len() as u64 != record.byte_len
                || bytes.len() as u64 > MAX_SOURCE_BYTES
                || hex::encode(Sha256::digest(bytes)) != record.content_sha256
            {
                return Err(AssetDiagnostic::ManagedRecordMismatch);
            }
            total
                .checked_add(bytes.len())
                .filter(|total| *total <= MAX_PREVIEW_OVERLAY_BYTES)
                .ok_or(AssetDiagnostic::SourceBudgetExceeded)
        })?;
        let _ = total;
        let mut ids = std::collections::BTreeSet::new();
        if entries
            .iter()
            .any(|(record, _)| !ids.insert(record.id.clone()))
        {
            return Err(AssetDiagnostic::ManagedRecordMismatch);
        }
        Ok(Self {
            entries: entries.into(),
        })
    }

    fn get(&self, id: &super::model::AssetId) -> Option<&(AssetRecord, Arc<[u8]>)> {
        self.entries.iter().find(|(record, _)| &record.id == id)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Package admission uses the same bounded decoder as runtime preparation so
/// compressed files cannot defer a decoded image/frame bomb until first use.
pub(crate) fn validate_packaged_media(
    bytes: &[u8],
    kind: MediaKind,
) -> Result<(), AssetDiagnostic> {
    match kind {
        MediaKind::Image => decode_image(bytes).map(|_| ()),
        MediaKind::Sound => decode_wave(bytes).map(|_| ()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetDiagnostic {
    NotFound,
    ManagedPathOutsideRoot,
    ManagedRecordMismatch,
    InvalidSearchFileName,
    SearchRootsUnavailable,
    UnsupportedFormat,
    Corrupt(String),
    SourceBudgetExceeded,
    DimensionBudgetExceeded,
    DecodedBudgetExceeded,
    FrameBudgetExceeded,
    DurationBudgetExceeded,
    InvalidWave,
    IconIndexInvalid,
    IconResourceUnavailable(String),
}

impl std::fmt::Display for AssetDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedImageFrame {
    pub width: u32,
    pub height: u32,
    pub duration_ms: u32,
    /// Straight-alpha RGBA8. Premultiplication belongs to the future compositor.
    pub rgba: Arc<[u8]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedImage {
    pub frames: Vec<PreparedImageFrame>,
    pub animated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedSound {
    pub wav: Arc<[u8]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreparedMedia {
    Image(PreparedImage),
    Sound(PreparedSound),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedAssetSnapshot {
    pub media: Arc<PreparedMedia>,
    pub portability: AssetPortability,
    pub source: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetPortability {
    ManagedPortable,
    ExternalLocation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrepareVariant {
    pub effective_style: u64,
    pub dpi_milli: u32,
    pub logical_width_milli: u32,
    pub logical_height_milli: u32,
    pub quality: RenderingQuality,
}

pub trait IconResourceExtractor: Send + Sync {
    fn extract(
        &self,
        path: &Path,
        zero_based_index: u32,
        size: u32,
    ) -> Result<PreparedImage, AssetDiagnostic>;
}

pub struct AssetService<E = PlatformIconExtractor> {
    application_data: PathBuf,
    search_roots: MediaSearchRoots,
    cache: PreparedAssetCache<PreparedOutcome>,
    icons: E,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PreparedOutcome {
    Available(PreparedAssetSnapshot),
    Unavailable(AssetDiagnostic),
}

impl AssetService<PlatformIconExtractor> {
    pub fn new(application_data: PathBuf, search_roots: MediaSearchRoots) -> Self {
        Self::with_extractor(application_data, search_roots, PlatformIconExtractor)
    }
}

impl<E: IconResourceExtractor> AssetService<E> {
    pub fn with_extractor(
        application_data: PathBuf,
        search_roots: MediaSearchRoots,
        icons: E,
    ) -> Self {
        Self {
            application_data,
            search_roots,
            cache: PreparedAssetCache::new(DEFAULT_CACHE_ENTRIES, MAX_DECODED_BYTES),
            icons,
        }
    }

    pub fn prepare(
        &mut self,
        reference: &MediaReference,
        expected: MediaKind,
        records: &[AssetRecord],
        variant: PrepareVariant,
    ) -> Result<PreparedAssetSnapshot, AssetDiagnostic> {
        self.prepare_with_overlay(
            reference,
            expected,
            records,
            variant,
            &ManagedAssetOverlay::default(),
        )
    }

    pub fn prepare_with_overlay(
        &mut self,
        reference: &MediaReference,
        expected: MediaKind,
        records: &[AssetRecord],
        variant: PrepareVariant,
        overlay: &ManagedAssetOverlay,
    ) -> Result<PreparedAssetSnapshot, AssetDiagnostic> {
        let overlay_version = match reference {
            MediaReference::Managed { asset_id } => overlay
                .get(asset_id)
                .map(|(record, _)| record.content_sha256.as_str())
                .unwrap_or("persisted"),
            _ => "external",
        };
        let unresolved_key = cache_key(
            reference_identity(reference),
            format!("unresolved:{overlay_version}"),
            variant,
        );
        if let Some(CachedAsset::Ready(value)) = self.cache.get(&unresolved_key) {
            if let PreparedOutcome::Unavailable(diagnostic) = &*value {
                return Err(diagnostic.clone());
            }
        }
        let resolved = match self.resolve(reference, expected, records, overlay) {
            Ok(resolved) => resolved,
            Err(error) => {
                self.cache.insert_ready(
                    unresolved_key,
                    PreparedOutcome::Unavailable(error.clone()),
                    1,
                );
                return Err(error);
            }
        };
        let key = cache_key(resolved.identity, resolved.version, variant);
        if let Some(cached) = self.cache.get(&key) {
            return match cached {
                CachedAsset::Ready(value) => match &*value {
                    PreparedOutcome::Available(snapshot) => Ok(snapshot.clone()),
                    PreparedOutcome::Unavailable(diagnostic) => Err(diagnostic.clone()),
                },
                CachedAsset::Unavailable(reason) => {
                    Err(AssetDiagnostic::Corrupt(reason.as_ref().to_owned()))
                }
            };
        }
        let result = if let Some(index) = resolved.icon_index {
            let desired = ((variant.logical_width_milli as u64 * variant.dpi_milli as u64)
                / 1_000_000)
                .clamp(1, limits::MAX_TEXTURE_DIMENSION as u64) as u32;
            self.icons
                .extract(&resolved.path, index, desired)
                .map(PreparedMedia::Image)
        } else {
            resolved
                .verified_bytes
                .map(Ok)
                .unwrap_or_else(|| read_bounded(&resolved.path))
                .and_then(|bytes| match expected {
                    MediaKind::Image => decode_image(&bytes).map(PreparedMedia::Image),
                    MediaKind::Sound => decode_wave(&bytes).map(PreparedMedia::Sound),
                })
        };
        match result {
            Ok(media) => {
                let cost = media_cost(&media);
                let snapshot = PreparedAssetSnapshot {
                    media: Arc::new(media),
                    portability: resolved.portability,
                    source: resolved.path,
                };
                self.cache
                    .insert_ready(key, PreparedOutcome::Available(snapshot.clone()), cost);
                Ok(snapshot)
            }
            Err(error) => {
                self.cache
                    .insert_ready(key, PreparedOutcome::Unavailable(error.clone()), 1);
                Err(error)
            }
        }
    }

    pub fn invalidate_identity(&mut self, identity: &str) {
        self.cache.invalidate_identity(identity);
    }

    pub fn invalidate_reference(&mut self, reference: &MediaReference) {
        if matches!(reference, MediaReference::SearchPath { .. }) {
            // Search resolution identity includes the selected configured root.
            self.cache.clear();
        } else {
            self.cache
                .invalidate_identity(&reference_identity(reference));
        }
    }

    pub fn replace_search_roots(&mut self, search_roots: MediaSearchRoots) {
        self.search_roots = search_roots;
        self.cache.clear();
    }

    fn resolve(
        &self,
        reference: &MediaReference,
        expected: MediaKind,
        records: &[AssetRecord],
        overlay: &ManagedAssetOverlay,
    ) -> Result<ResolvedSource, AssetDiagnostic> {
        match reference {
            MediaReference::Managed { asset_id } => {
                let record = records
                    .iter()
                    .find(|record| &record.id == asset_id)
                    .ok_or(AssetDiagnostic::ManagedRecordMismatch)?;
                if record.kind != expected || !safe_relative(Path::new(&record.relative_path)) {
                    return Err(AssetDiagnostic::ManagedRecordMismatch);
                }
                if let Some((overlay_record, bytes)) = overlay.get(asset_id) {
                    if overlay_record != record
                        || bytes.len() as u64 != record.byte_len
                        || hex::encode(Sha256::digest(bytes)) != record.content_sha256
                    {
                        return Err(AssetDiagnostic::ManagedRecordMismatch);
                    }
                    return Ok(ResolvedSource {
                        identity: format!("managed:{asset_id}"),
                        version: format!("pending:{}:{}", record.content_sha256, record.byte_len),
                        path: PathBuf::from(format!("pending-managed:{asset_id}")),
                        portability: AssetPortability::ManagedPortable,
                        icon_index: None,
                        verified_bytes: Some(bytes.to_vec()),
                    });
                }
                let root = self.application_data.join(RADIAL_ASSETS_DIRECTORY);
                let path = root.join(&record.relative_path);
                ensure_beneath(&root, &path)?;
                if std::fs::metadata(&path)
                    .map_err(|_| AssetDiagnostic::NotFound)?
                    .len()
                    != record.byte_len
                {
                    return Err(AssetDiagnostic::ManagedRecordMismatch);
                }
                let bytes = read_bounded(&path)?;
                let digest = hex::encode(Sha256::digest(&bytes));
                if digest != record.content_sha256 {
                    return Err(AssetDiagnostic::ManagedRecordMismatch);
                }
                Ok(ResolvedSource {
                    identity: format!("managed:{asset_id}"),
                    version: format!("{}:{}", record.content_sha256, record.byte_len),
                    path,
                    portability: AssetPortability::ManagedPortable,
                    icon_index: None,
                    verified_bytes: Some(bytes),
                })
            }
            MediaReference::ExternalFile { path } => {
                let path = PathBuf::from(path);
                Ok(ResolvedSource::file("external", path, None))
            }
            MediaReference::SearchPath { file_name } => {
                if !safe_file_name(file_name) {
                    return Err(AssetDiagnostic::InvalidSearchFileName);
                }
                let configured = match expected {
                    MediaKind::Image => &self.search_roots.image_directories,
                    MediaKind::Sound => &self.search_roots.sound_directories,
                };
                let mut root_available = false;
                for root in configured {
                    root_available |= Path::new(root).is_dir();
                    let candidate = Path::new(root).join(file_name);
                    if candidate.is_file() {
                        return Ok(ResolvedSource::file("search", candidate, None));
                    }
                }
                if windows_media_eligible(
                    expected,
                    file_name,
                    self.search_roots.search_windows_media_for_sounds,
                ) {
                    if let Some(root) = windows_media_directory() {
                        root_available |= root.is_dir();
                        let candidate = root.join(file_name);
                        if candidate.is_file() {
                            return Ok(ResolvedSource::file("windows-media", candidate, None));
                        }
                    }
                }
                if root_available {
                    Err(AssetDiagnostic::NotFound)
                } else {
                    Err(AssetDiagnostic::SearchRootsUnavailable)
                }
            }
            MediaReference::IconResource { path, index } => {
                let extension = Path::new(path)
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default();
                if *index == 0 {
                    return Err(AssetDiagnostic::IconIndexInvalid);
                }
                if expected != MediaKind::Image
                    || !matches!(
                        extension.to_ascii_lowercase().as_str(),
                        "exe" | "dll" | "cpl"
                    )
                {
                    return Err(AssetDiagnostic::UnsupportedFormat);
                }
                let mut source =
                    ResolvedSource::file("icon-resource", PathBuf::from(path), Some(index - 1));
                source.identity.push(':');
                source.identity.push_str(&index.to_string());
                Ok(source)
            }
        }
    }
}

fn cache_key(identity: String, version: String, variant: PrepareVariant) -> AssetCacheKey {
    AssetCacheKey {
        identity,
        version,
        effective_style: variant.effective_style,
        dpi_milli: variant.dpi_milli,
        logical_width_milli: variant.logical_width_milli,
        logical_height_milli: variant.logical_height_milli,
        quality: variant.quality,
    }
}

pub fn reference_identity(reference: &MediaReference) -> String {
    match reference {
        MediaReference::Managed { asset_id } => format!("managed:{asset_id}"),
        MediaReference::ExternalFile { path } => format!("external:{path}"),
        MediaReference::SearchPath { file_name } => format!("search:{file_name}"),
        MediaReference::IconResource { path, index } => format!("icon-resource:{path}:{index}"),
    }
}

struct ResolvedSource {
    identity: String,
    version: String,
    path: PathBuf,
    portability: AssetPortability,
    icon_index: Option<u32>,
    verified_bytes: Option<Vec<u8>>,
}

impl ResolvedSource {
    fn file(prefix: &str, path: PathBuf, icon_index: Option<u32>) -> Self {
        let version = std::fs::metadata(&path)
            .map(|metadata| {
                let modified = metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |duration| duration.as_nanos());
                format!("{}:{modified}", metadata.len())
            })
            .unwrap_or_else(|_| "missing".into());
        Self {
            identity: format!("{prefix}:{}", path.to_string_lossy()),
            version,
            path,
            portability: AssetPortability::ExternalLocation,
            icon_index,
            verified_bytes: None,
        }
    }
}

fn safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn safe_file_name(value: &str) -> bool {
    let path = Path::new(value);
    !value.trim().is_empty()
        && value.len() <= 255
        && path
            .file_name()
            .is_some_and(|name| name == path.as_os_str())
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn ensure_beneath(root: &Path, candidate: &Path) -> Result<(), AssetDiagnostic> {
    let root = root.canonicalize().map_err(|_| AssetDiagnostic::NotFound)?;
    let candidate = candidate
        .canonicalize()
        .map_err(|_| AssetDiagnostic::NotFound)?;
    if candidate.starts_with(root) {
        Ok(())
    } else {
        Err(AssetDiagnostic::ManagedPathOutsideRoot)
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, AssetDiagnostic> {
    let file = File::open(path).map_err(|_| AssetDiagnostic::NotFound)?;
    if file
        .metadata()
        .map_err(|_| AssetDiagnostic::NotFound)?
        .len()
        > MAX_SOURCE_BYTES
    {
        return Err(AssetDiagnostic::SourceBudgetExceeded);
    }
    let mut bytes = Vec::new();
    file.take(MAX_SOURCE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| AssetDiagnostic::Corrupt(error.to_string()))?;
    if bytes.len() as u64 > MAX_SOURCE_BYTES {
        Err(AssetDiagnostic::SourceBudgetExceeded)
    } else {
        Ok(bytes)
    }
}

pub fn decode_image(bytes: &[u8]) -> Result<PreparedImage, AssetDiagnostic> {
    let format = image::guess_format(bytes).map_err(|_| AssetDiagnostic::UnsupportedFormat)?;
    if !matches!(
        format,
        ImageFormat::Png
            | ImageFormat::Jpeg
            | ImageFormat::Bmp
            | ImageFormat::Ico
            | ImageFormat::Gif
            | ImageFormat::Tiff
    ) {
        return Err(AssetDiagnostic::UnsupportedFormat);
    }
    if format == ImageFormat::Gif {
        return decode_gif(bytes);
    }
    let (width, height) = image::io::Reader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|error| AssetDiagnostic::Corrupt(error.to_string()))?;
    validate_dimensions(width, height, 1)?;
    let image = image::load_from_memory_with_format(bytes, format)
        .map_err(|error| AssetDiagnostic::Corrupt(error.to_string()))?;
    let rgba = image.into_rgba8().into_raw();
    Ok(PreparedImage {
        frames: vec![PreparedImageFrame {
            width,
            height,
            duration_ms: 0,
            rgba: rgba.into(),
        }],
        animated: false,
    })
}

fn decode_gif(bytes: &[u8]) -> Result<PreparedImage, AssetDiagnostic> {
    let decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
        .map_err(|error| AssetDiagnostic::Corrupt(error.to_string()))?;
    let mut frames = Vec::new();
    let mut decoded = 0usize;
    let mut duration = 0u64;
    for frame in decoder.into_frames() {
        if frames.len() == MAX_ANIMATION_FRAMES {
            return Err(AssetDiagnostic::FrameBudgetExceeded);
        }
        let frame = frame.map_err(|error| AssetDiagnostic::Corrupt(error.to_string()))?;
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let delay = if denominator == 0 {
            0
        } else {
            numerator.saturating_add(denominator - 1) / denominator
        };
        duration = duration.saturating_add(delay as u64);
        if duration > MAX_ANIMATION_DURATION_MS {
            return Err(AssetDiagnostic::DurationBudgetExceeded);
        }
        let buffer = frame.into_buffer();
        validate_dimensions(buffer.width(), buffer.height(), frames.len() + 1)?;
        decoded = decoded
            .checked_add(buffer.len())
            .ok_or(AssetDiagnostic::DecodedBudgetExceeded)?;
        if decoded > MAX_DECODED_BYTES {
            return Err(AssetDiagnostic::DecodedBudgetExceeded);
        }
        frames.push(PreparedImageFrame {
            width: buffer.width(),
            height: buffer.height(),
            duration_ms: delay,
            rgba: buffer.into_raw().into(),
        });
    }
    if frames.is_empty() {
        return Err(AssetDiagnostic::Corrupt("GIF has no frames".into()));
    }
    Ok(PreparedImage {
        animated: frames.len() > 1,
        frames,
    })
}

fn validate_dimensions(width: u32, height: u32, frames: usize) -> Result<(), AssetDiagnostic> {
    if width == 0
        || height == 0
        || width > limits::MAX_TEXTURE_DIMENSION
        || height > limits::MAX_TEXTURE_DIMENSION
    {
        return Err(AssetDiagnostic::DimensionBudgetExceeded);
    }
    let bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|per_frame| per_frame.checked_mul(frames))
        .ok_or(AssetDiagnostic::DecodedBudgetExceeded)?;
    if bytes > MAX_DECODED_BYTES {
        Err(AssetDiagnostic::DecodedBudgetExceeded)
    } else {
        Ok(())
    }
}

fn decode_wave(bytes: &[u8]) -> Result<PreparedSound, AssetDiagnostic> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(AssetDiagnostic::InvalidWave);
    }
    let declared = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    if declared.checked_add(8) != Some(bytes.len()) {
        return Err(AssetDiagnostic::InvalidWave);
    }
    let mut cursor = 12usize;
    let mut format = None;
    let mut data_len = None;
    while cursor < bytes.len() {
        let header_end = cursor.checked_add(8).ok_or(AssetDiagnostic::InvalidWave)?;
        if header_end > bytes.len() {
            return Err(AssetDiagnostic::InvalidWave);
        }
        let id = &bytes[cursor..cursor + 4];
        let length = u32::from_le_bytes(bytes[cursor + 4..header_end].try_into().unwrap()) as usize;
        let data_start = header_end;
        let data_end = data_start
            .checked_add(length)
            .ok_or(AssetDiagnostic::InvalidWave)?;
        if data_end > bytes.len() {
            return Err(AssetDiagnostic::InvalidWave);
        }
        if id == b"fmt " {
            if format.is_some() || length < 16 {
                return Err(AssetDiagnostic::InvalidWave);
            }
            let tag = u16::from_le_bytes(bytes[data_start..data_start + 2].try_into().unwrap());
            let channels =
                u16::from_le_bytes(bytes[data_start + 2..data_start + 4].try_into().unwrap());
            let sample_rate =
                u32::from_le_bytes(bytes[data_start + 4..data_start + 8].try_into().unwrap());
            let byte_rate =
                u32::from_le_bytes(bytes[data_start + 8..data_start + 12].try_into().unwrap());
            let block_align =
                u16::from_le_bytes(bytes[data_start + 12..data_start + 14].try_into().unwrap());
            let bits =
                u16::from_le_bytes(bytes[data_start + 14..data_start + 16].try_into().unwrap());
            if !matches!(tag, 1 | 3)
                || channels == 0
                || channels > MAX_WAVE_CHANNELS
                || sample_rate == 0
                || sample_rate > MAX_WAVE_SAMPLE_RATE
                || !matches!(bits, 8 | 16 | 24 | 32)
            {
                return Err(AssetDiagnostic::InvalidWave);
            }
            let expected_align = channels
                .checked_mul(bits / 8)
                .ok_or(AssetDiagnostic::InvalidWave)?;
            let expected_rate = sample_rate
                .checked_mul(u32::from(expected_align))
                .ok_or(AssetDiagnostic::InvalidWave)?;
            if block_align != expected_align || byte_rate != expected_rate {
                return Err(AssetDiagnostic::InvalidWave);
            }
            format = Some(byte_rate);
        } else if id == b"data" {
            if data_len.replace(length).is_some() || length > MAX_WAVE_DATA_BYTES {
                return Err(AssetDiagnostic::DecodedBudgetExceeded);
            }
        }
        cursor = data_end
            .checked_add(length & 1)
            .ok_or(AssetDiagnostic::InvalidWave)?;
    }
    if cursor != bytes.len() {
        return Err(AssetDiagnostic::InvalidWave);
    }
    let (byte_rate, data_len) = format.zip(data_len).ok_or(AssetDiagnostic::InvalidWave)?;
    let duration = (data_len as u64)
        .checked_mul(1_000)
        .ok_or(AssetDiagnostic::DurationBudgetExceeded)?
        / u64::from(byte_rate);
    if duration > MAX_WAVE_DURATION_MS {
        return Err(AssetDiagnostic::DurationBudgetExceeded);
    }
    Ok(PreparedSound { wav: bytes.into() })
}

fn media_cost(media: &PreparedMedia) -> usize {
    match media {
        PreparedMedia::Image(image) => image.frames.iter().map(|frame| frame.rgba.len()).sum(),
        PreparedMedia::Sound(sound) => sound.wav.len(),
    }
}

fn windows_media_directory() -> Option<PathBuf> {
    std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .map(|path| path.join("Media"))
}

fn windows_media_eligible(expected: MediaKind, file_name: &str, enabled: bool) -> bool {
    enabled
        && expected == MediaKind::Sound
        && Path::new(file_name)
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("wav"))
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PlatformIconExtractor;

#[cfg(not(windows))]
impl IconResourceExtractor for PlatformIconExtractor {
    fn extract(
        &self,
        _path: &Path,
        _index: u32,
        _size: u32,
    ) -> Result<PreparedImage, AssetDiagnostic> {
        Err(AssetDiagnostic::IconResourceUnavailable(
            "Windows icon resources are unavailable on this platform".into(),
        ))
    }
}

#[cfg(windows)]
impl IconResourceExtractor for PlatformIconExtractor {
    fn extract(
        &self,
        path: &Path,
        index: u32,
        size: u32,
    ) -> Result<PreparedImage, AssetDiagnostic> {
        extract_windows_icon(path, index, size)
    }
}

#[cfg(windows)]
fn extract_windows_icon(
    path: &Path,
    index: u32,
    size: u32,
) -> Result<PreparedImage, AssetDiagnostic> {
    use std::os::windows::ffi::OsStrExt;
    use std::{mem, ptr, slice};
    use windows::Win32::Foundation::{FreeLibrary, HANDLE};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
        DeleteDC, DeleteObject, SelectObject,
    };
    use windows::Win32::System::LibraryLoader::{
        LOAD_LIBRARY_AS_DATAFILE, LOAD_LIBRARY_AS_IMAGE_RESOURCE, LoadLibraryExW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DI_NORMAL, DestroyIcon, DrawIconEx, HICON, PrivateExtractIconsW,
    };
    use windows::core::PCWSTR;

    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    if wide.len() > 260 || index > i32::MAX as u32 {
        return Err(AssetDiagnostic::IconIndexInvalid);
    }
    let module = unsafe {
        LoadLibraryExW(
            PCWSTR(wide.as_ptr()),
            HANDLE::default(),
            LOAD_LIBRARY_AS_DATAFILE | LOAD_LIBRARY_AS_IMAGE_RESOURCE,
        )
    }
    .map_err(|error| AssetDiagnostic::IconResourceUnavailable(error.to_string()))?;
    struct Module(windows::Win32::Foundation::HMODULE);
    impl Drop for Module {
        fn drop(&mut self) {
            let _ = unsafe { FreeLibrary(self.0) };
        }
    }
    let _module = Module(module);
    let mut fixed = [0u16; 260];
    fixed[..wide.len()].copy_from_slice(&wide);
    let mut icons = [HICON::default()];
    let count = unsafe {
        PrivateExtractIconsW(
            &fixed,
            index as i32,
            size as i32,
            size as i32,
            Some(&mut icons),
            None,
            0,
        )
    };
    if count == 0 || icons[0].0.is_null() {
        return Err(AssetDiagnostic::IconResourceUnavailable(
            "resource index was not found".into(),
        ));
    }
    let icon = icons[0];
    let dimension = size.clamp(1, limits::MAX_TEXTURE_DIMENSION);
    if let Err(error) = validate_dimensions(dimension, dimension, 1) {
        let _ = unsafe { DestroyIcon(icon) };
        return Err(error);
    }
    let byte_len = dimension as usize * dimension as usize * 4;
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.0.is_null() {
            let _ = DestroyIcon(icon);
            return Err(AssetDiagnostic::IconResourceUnavailable(
                "GDI allocation failed".into(),
            ));
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: dimension as i32,
                biHeight: -(dimension as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            bmiColors: [Default::default()],
        };
        let mut bits = ptr::null_mut();
        let dib = match CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(dib) if !bits.is_null() => dib,
            result => {
                let _ = DeleteDC(dc);
                let _ = DestroyIcon(icon);
                return Err(AssetDiagnostic::IconResourceUnavailable(
                    result
                        .err()
                        .map_or_else(|| "empty DIB".into(), |error| error.to_string()),
                ));
            }
        };
        let old = SelectObject(dc, dib);
        if old.0.is_null() {
            let _ = DeleteObject(dib);
            let _ = DeleteDC(dc);
            let _ = DestroyIcon(icon);
            return Err(AssetDiagnostic::IconResourceUnavailable(
                "GDI bitmap selection failed".into(),
            ));
        }
        ptr::write_bytes(bits as *mut u8, 0, byte_len);
        let rendered = DrawIconEx(
            dc,
            0,
            0,
            icon,
            dimension as i32,
            dimension as i32,
            0,
            None,
            DI_NORMAL,
        );
        let mut rgba = slice::from_raw_parts(bits as *const u8, byte_len).to_vec();
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let _ = SelectObject(dc, old);
        let _ = DeleteObject(dib);
        let _ = DeleteDC(dc);
        let _ = DestroyIcon(icon);
        rendered.map_err(|error| AssetDiagnostic::IconResourceUnavailable(error.to_string()))?;
        Ok(PreparedImage {
            frames: vec![PreparedImageFrame {
                width: dimension,
                height: dimension,
                duration_ms: 0,
                rgba: rgba.into(),
            }],
            animated: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageOutputFormat, Rgba, RgbaImage};
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn encoded(format: ImageOutputFormat) -> Vec<u8> {
        let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 3, Rgba([1, 2, 3, 255])));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, format).unwrap();
        bytes.into_inner()
    }

    fn animated_gif(frame_count: usize, delay_ms: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            let frames = (0..frame_count).map(|_| {
                image::Frame::from_parts(
                    RgbaImage::from_pixel(1, 1, Rgba([1, 2, 3, 255])),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(delay_ms, 1),
                )
            });
            encoder.encode_frames(frames).unwrap();
        }
        bytes
    }

    #[test]
    fn supported_static_formats_decode_with_budgets() {
        for format in [
            ImageOutputFormat::Png,
            ImageOutputFormat::Jpeg(80),
            ImageOutputFormat::Bmp,
            ImageOutputFormat::Ico,
            ImageOutputFormat::Gif,
            ImageOutputFormat::Tiff,
        ] {
            let decoded = decode_image(&encoded(format)).unwrap();
            assert_eq!((decoded.frames[0].width, decoded.frames[0].height), (2, 3));
            assert!(!decoded.animated);
        }
        assert!(matches!(
            decode_image(b"not an image"),
            Err(AssetDiagnostic::UnsupportedFormat)
        ));
        assert!(matches!(
            decode_image(&[137, 80, 78, 71, 13, 10, 26, 10]),
            Err(AssetDiagnostic::Corrupt(_))
        ));
        assert!(matches!(
            decode_wave(b"not wave"),
            Err(AssetDiagnostic::InvalidWave)
        ));
    }

    #[test]
    fn gif_animation_enforces_frame_and_duration_budgets_during_decode() {
        let animation = decode_image(&animated_gif(2, 10)).unwrap();
        assert!(animation.animated);
        assert_eq!(animation.frames.len(), 2);
        assert!(matches!(
            decode_image(&animated_gif(MAX_ANIMATION_FRAMES + 1, 1)),
            Err(AssetDiagnostic::FrameBudgetExceeded)
        ));
        assert!(matches!(
            decode_image(&animated_gif(2, 20_000)),
            Err(AssetDiagnostic::DurationBudgetExceeded)
        ));
    }

    #[test]
    fn pending_overlay_precedes_disk_without_writes_and_validates_image_gif_and_sound() {
        let directory = tempfile::tempdir().unwrap();
        let png = encoded(ImageOutputFormat::Png);
        let gif = animated_gif(2, 10);
        let wav = pcm_wav(1, 8_000, 8, &[0; 16]);
        let entries = [
            ("pending-png", MediaKind::Image, "pending.png", png),
            ("pending-gif", MediaKind::Image, "pending.gif", gif),
            ("pending-wav", MediaKind::Sound, "pending.wav", wav),
        ]
        .into_iter()
        .map(|(id, kind, relative_path, bytes)| {
            let record = AssetRecord {
                id: super::super::model::AssetId::new(id),
                kind,
                relative_path: relative_path.into(),
                content_sha256: hex::encode(Sha256::digest(&bytes)),
                byte_len: bytes.len() as u64,
            };
            (record, Arc::<[u8]>::from(bytes))
        })
        .collect::<Vec<_>>();
        let records = entries
            .iter()
            .map(|(record, _)| record.clone())
            .collect::<Vec<_>>();
        let overlay = ManagedAssetOverlay::validated(entries).unwrap();
        let mut service = AssetService::new(directory.path().to_path_buf(), Default::default());
        let variant = PrepareVariant {
            effective_style: 1,
            dpi_milli: 1_000,
            logical_width_milli: 32_000,
            logical_height_milli: 32_000,
            quality: RenderingQuality::Balanced,
        };
        for record in &records {
            let snapshot = service
                .prepare_with_overlay(
                    &MediaReference::Managed {
                        asset_id: record.id.clone(),
                    },
                    record.kind,
                    &records,
                    variant,
                    &overlay,
                )
                .unwrap();
            assert!(
                snapshot
                    .source
                    .to_string_lossy()
                    .starts_with("pending-managed:")
            );
        }
        assert!(!directory.path().join(RADIAL_ASSETS_DIRECTORY).exists());

        let bad = AssetRecord {
            id: super::super::model::AssetId::new("corrupt"),
            kind: MediaKind::Image,
            relative_path: "corrupt.png".into(),
            content_sha256: "0".repeat(64),
            byte_len: 3,
        };
        assert_eq!(
            ManagedAssetOverlay::validated([(bad, Arc::<[u8]>::from([1, 2, 3]))]),
            Err(AssetDiagnostic::ManagedRecordMismatch)
        );
        assert!(!directory.path().join(RADIAL_ASSETS_DIRECTORY).exists());
    }

    #[derive(Clone)]
    struct MissingIcon;
    impl IconResourceExtractor for MissingIcon {
        fn extract(
            &self,
            _path: &Path,
            index: u32,
            _size: u32,
        ) -> Result<PreparedImage, AssetDiagnostic> {
            Err(AssetDiagnostic::IconResourceUnavailable(format!(
                "missing {index}"
            )))
        }
    }

    struct CountingIcon(AtomicUsize);
    impl IconResourceExtractor for CountingIcon {
        fn extract(
            &self,
            _path: &Path,
            _index: u32,
            size: u32,
        ) -> Result<PreparedImage, AssetDiagnostic> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(PreparedImage {
                frames: vec![PreparedImageFrame {
                    width: size,
                    height: size,
                    duration_ms: 0,
                    rgba: vec![0; size as usize * size as usize * 4].into(),
                }],
                animated: false,
            })
        }
    }

    struct CountingMissingIcon(AtomicUsize);
    impl IconResourceExtractor for CountingMissingIcon {
        fn extract(
            &self,
            _path: &Path,
            _index: u32,
            _size: u32,
        ) -> Result<PreparedImage, AssetDiagnostic> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Err(AssetDiagnostic::IconResourceUnavailable("missing".into()))
        }
    }

    fn variant(dpi: u32) -> PrepareVariant {
        PrepareVariant {
            effective_style: 7,
            dpi_milli: dpi,
            logical_width_milli: 32_000,
            logical_height_milli: 32_000,
            quality: RenderingQuality::Balanced,
        }
    }

    #[test]
    fn managed_paths_are_confined_and_search_roots_are_visible_and_filename_only() {
        let temp = tempfile::tempdir().unwrap();
        let assets = temp.path().join(RADIAL_ASSETS_DIRECTORY);
        std::fs::create_dir(&assets).unwrap();
        let managed_bytes = encoded(ImageOutputFormat::Png);
        std::fs::write(assets.join("image.png"), &managed_bytes).unwrap();
        let search = temp.path().join("visible-search-root");
        std::fs::create_dir(&search).unwrap();
        std::fs::write(search.join("searched.png"), encoded(ImageOutputFormat::Png)).unwrap();
        let record = AssetRecord {
            id: super::super::model::AssetId::new("image"),
            kind: MediaKind::Image,
            relative_path: "image.png".into(),
            content_sha256: hex::encode(Sha256::digest(&managed_bytes)),
            byte_len: managed_bytes.len() as u64,
        };
        let mut service = AssetService::with_extractor(
            temp.path().to_path_buf(),
            MediaSearchRoots {
                image_directories: vec![search.to_string_lossy().into_owned()],
                sound_directories: Vec::new(),
                search_windows_media_for_sounds: false,
            },
            MissingIcon,
        );
        let loaded = service
            .prepare(
                &MediaReference::Managed {
                    asset_id: record.id.clone(),
                },
                MediaKind::Image,
                &[record],
                variant(1000),
            )
            .unwrap();
        assert_eq!(loaded.portability, AssetPortability::ManagedPortable);
        let searched = service
            .prepare(
                &MediaReference::SearchPath {
                    file_name: "searched.png".into(),
                },
                MediaKind::Image,
                &[],
                variant(1000),
            )
            .unwrap();
        assert_eq!(searched.portability, AssetPortability::ExternalLocation);
        let escaped = AssetRecord {
            id: super::super::model::AssetId::new("escaped"),
            kind: MediaKind::Image,
            relative_path: "../image.png".into(),
            content_sha256: "v1".into(),
            byte_len: 1,
        };
        assert!(matches!(
            service.prepare(
                &MediaReference::Managed {
                    asset_id: escaped.id.clone()
                },
                MediaKind::Image,
                &[escaped],
                variant(1000),
            ),
            Err(AssetDiagnostic::ManagedRecordMismatch)
        ));
        assert!(matches!(
            service.prepare(
                &MediaReference::SearchPath {
                    file_name: "../image.png".into()
                },
                MediaKind::Image,
                &[],
                variant(1000),
            ),
            Err(AssetDiagnostic::InvalidSearchFileName)
        ));
    }

    fn pcm_wav(channels: u16, rate: u32, bits: u16, data: &[u8]) -> Vec<u8> {
        let block = channels.saturating_mul(bits / 8);
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36_u32 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0");
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&rate.saturating_mul(block as u32).to_le_bytes());
        out.extend_from_slice(&block.to_le_bytes());
        out.extend_from_slice(&bits.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn wav_parser_enforces_format_and_duration_budgets() {
        assert!(decode_wave(&pcm_wav(2, 48_000, 16, &[0; 192])).is_ok());
        assert!(matches!(
            decode_wave(&pcm_wav(MAX_WAVE_CHANNELS + 1, 48_000, 16, &[0; 32])),
            Err(AssetDiagnostic::InvalidWave)
        ));
        assert!(matches!(
            decode_wave(&pcm_wav(2, MAX_WAVE_SAMPLE_RATE + 1, 16, &[0; 32])),
            Err(AssetDiagnostic::InvalidWave)
        ));
        let too_long = vec![0; (MAX_WAVE_DURATION_MS as usize / 1_000 + 1) * 8_000];
        assert!(matches!(
            decode_wave(&pcm_wav(1, 8_000, 8, &too_long)),
            Err(AssetDiagnostic::DurationBudgetExceeded)
        ));
        let mut truncated = pcm_wav(1, 8_000, 8, &[0; 8]);
        truncated.pop();
        assert!(matches!(
            decode_wave(&truncated),
            Err(AssetDiagnostic::InvalidWave)
        ));
    }

    #[test]
    fn managed_content_hash_is_checked_before_cache_reuse() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(RADIAL_ASSETS_DIRECTORY);
        std::fs::create_dir(&root).unwrap();
        let original = encoded(ImageOutputFormat::Png);
        let path = root.join("managed.png");
        std::fs::write(&path, &original).unwrap();
        let record = AssetRecord {
            id: super::super::model::AssetId::new("tamper"),
            kind: MediaKind::Image,
            relative_path: "managed.png".into(),
            content_sha256: hex::encode(Sha256::digest(&original)),
            byte_len: original.len() as u64,
        };
        let mut service = AssetService::with_extractor(
            temp.path().to_path_buf(),
            MediaSearchRoots::default(),
            MissingIcon,
        );
        let reference = MediaReference::Managed {
            asset_id: record.id.clone(),
        };
        service
            .prepare(
                &reference,
                MediaKind::Image,
                &[record.clone()],
                variant(1000),
            )
            .unwrap();
        let mut tampered = original;
        let index = tampered.len() - 1;
        tampered[index] ^= 1;
        std::fs::write(path, tampered).unwrap();
        assert!(matches!(
            service.prepare(&reference, MediaKind::Image, &[record], variant(1000)),
            Err(AssetDiagnostic::ManagedRecordMismatch)
        ));
    }

    #[test]
    fn source_budget_and_icon_index_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("large.wav");
        let mut file = File::create(&path).unwrap();
        file.set_len(MAX_SOURCE_BYTES + 1).unwrap();
        file.flush().unwrap();
        assert!(matches!(
            read_bounded(&path),
            Err(AssetDiagnostic::SourceBudgetExceeded)
        ));
        let mut service = AssetService::with_extractor(
            temp.path().to_path_buf(),
            MediaSearchRoots::default(),
            MissingIcon,
        );
        assert!(matches!(
            service.prepare(
                &MediaReference::IconResource {
                    path: "missing.dll".into(),
                    index: 0
                },
                MediaKind::Image,
                &[],
                variant(1000),
            ),
            Err(AssetDiagnostic::IconIndexInvalid)
        ));
        assert!(matches!(
            service.prepare(
                &MediaReference::SearchPath {
                    file_name: "absent.png".into()
                },
                MediaKind::Image,
                &[],
                variant(1000),
            ),
            Err(AssetDiagnostic::SearchRootsUnavailable)
        ));
        assert_eq!(service.cache.len(), 2);
        assert!(matches!(
            service.prepare(
                &MediaReference::SearchPath {
                    file_name: "absent.png".into()
                },
                MediaKind::Image,
                &[],
                variant(1000),
            ),
            Err(AssetDiagnostic::SearchRootsUnavailable)
        ));
        assert_eq!(service.cache.len(), 2);
        assert!(windows_media_eligible(MediaKind::Sound, "notify.WAV", true));
        assert!(!windows_media_eligible(
            MediaKind::Image,
            "notify.wav",
            true
        ));
        assert!(!windows_media_eligible(
            MediaKind::Sound,
            "notify.mp3",
            true
        ));
        assert!(!windows_media_eligible(
            MediaKind::Sound,
            "notify.wav",
            false
        ));
    }

    #[test]
    fn prepared_cache_hits_and_dpi_variants_are_distinct_and_external_is_nonportable() {
        let temp = tempfile::tempdir().unwrap();
        let mut service = AssetService::with_extractor(
            temp.path().to_path_buf(),
            MediaSearchRoots::default(),
            CountingIcon(AtomicUsize::new(0)),
        );
        let reference = MediaReference::IconResource {
            path: "shell32.dll".into(),
            index: 1,
        };
        let first = service
            .prepare(&reference, MediaKind::Image, &[], variant(1000))
            .unwrap();
        let second = service
            .prepare(&reference, MediaKind::Image, &[], variant(1000))
            .unwrap();
        assert!(Arc::ptr_eq(&first.media, &second.media));
        assert_eq!(service.icons.0.load(Ordering::Relaxed), 1);
        let scaled = service
            .prepare(&reference, MediaKind::Image, &[], variant(1500))
            .unwrap();
        assert!(!Arc::ptr_eq(&first.media, &scaled.media));
        assert_eq!(service.icons.0.load(Ordering::Relaxed), 2);
        assert_eq!(first.portability, AssetPortability::ExternalLocation);

        let external = temp.path().join("external.png");
        std::fs::write(&external, encoded(ImageOutputFormat::Png)).unwrap();
        let snapshot = service
            .prepare(
                &MediaReference::ExternalFile {
                    path: external.to_string_lossy().into_owned(),
                },
                MediaKind::Image,
                &[],
                variant(1000),
            )
            .unwrap();
        assert_eq!(snapshot.portability, AssetPortability::ExternalLocation);
    }

    #[test]
    fn unavailable_preparation_is_negatively_cached() {
        let temp = tempfile::tempdir().unwrap();
        let mut service = AssetService::with_extractor(
            temp.path().to_path_buf(),
            MediaSearchRoots::default(),
            CountingMissingIcon(AtomicUsize::new(0)),
        );
        let reference = MediaReference::IconResource {
            path: "missing.dll".into(),
            index: 1,
        };
        assert!(
            service
                .prepare(&reference, MediaKind::Image, &[], variant(1000))
                .is_err()
        );
        assert!(
            service
                .prepare(&reference, MediaKind::Image, &[], variant(1000))
                .is_err()
        );
        assert_eq!(service.icons.0.load(Ordering::Relaxed), 1);
    }
}
