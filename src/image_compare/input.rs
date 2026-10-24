use super::budget::ImageBudgetLease;
use super::{ImageCompareBudget, ImageCompareError};
use image::io::Limits;
use image::io::Reader as ImageReader;
use image::{GenericImageView, ImageFormat};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub(crate) const MAX_IMAGE_PIXELS: usize = 16_777_216;
pub(crate) const MAX_ENCODED_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// Picker extensions admitted by the bounded decoder path. Content sniffing
/// remains authoritative when files are loaded.
pub(crate) const SUPPORTED_IMAGE_FILE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif"];
// Codec constructors may retain/copy metadata from the bounded encoded input.
// Reserve its full ceiling before construction; the pixel term covers decoded
// and converted surfaces while the codec is active. Decoder-side max_alloc is
// separately capped at this metadata reservation so PNG cannot use the pixel
// portion of the lease to grow ancillary/parser allocations.
pub(super) const DECODER_METADATA_RESERVATION_BYTES: usize = MAX_ENCODED_FILE_BYTES as usize;
const MAX_DECODER_AND_CONVERSION_BYTES_PER_PIXEL: usize = 20;
const FILE_READ_CHUNK_BYTES: usize = 64 * 1024;

static NEXT_IMAGE_INPUT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ImageSide {
    A,
    B,
}

impl ImageSide {
    pub(super) const fn index(self) -> usize {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }

    pub(super) const fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ImageInputSource {
    File { path: PathBuf, display_name: String },
    Clipboard { format: String },
    Test { display_name: String },
}

impl ImageInputSource {
    pub(crate) fn display_name(&self) -> &str {
        match self {
            Self::File { display_name, .. } | Self::Test { display_name } => display_name,
            Self::Clipboard { format } => format,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ImageInputId(u64);

impl ImageInputId {
    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}

struct ImageInputData {
    id: ImageInputId,
    revision: u64,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    source: ImageInputSource,
    _reservation: ImageBudgetLease,
}

/// Immutable decoded pixels shared by snapshots and comparison jobs.
#[derive(Clone)]
pub(crate) struct ImageInput(Arc<ImageInputData>);

impl ImageInput {
    pub(crate) fn id(&self) -> ImageInputId {
        self.0.id
    }

    pub(crate) fn revision(&self) -> u64 {
        self.0.revision
    }

    pub(crate) fn dimensions(&self) -> (u32, u32) {
        (self.0.width, self.0.height)
    }

    pub(crate) fn source(&self) -> &ImageInputSource {
        &self.0.source
    }

    pub(crate) fn pixels(&self) -> &[u8] {
        &self.0.pixels
    }

    pub(super) fn from_loaded(
        loaded: LoadedInput,
        revision: u64,
    ) -> Result<Self, ImageCompareError> {
        let id = next_input_id()?;
        Ok(Self(Arc::new(ImageInputData {
            id,
            revision,
            width: loaded.width,
            height: loaded.height,
            pixels: loaded.pixels,
            source: loaded.source,
            _reservation: loaded.reservation,
        })))
    }
}

pub(super) struct LoadedInput {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) pixels: Vec<u8>,
    pub(super) source: ImageInputSource,
    pub(super) reservation: ImageBudgetLease,
}

impl LoadedInput {
    pub(super) fn from_rgba(
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        source: ImageInputSource,
        reservation: ImageBudgetLease,
    ) -> Result<Self, ImageCompareError> {
        let expected = checked_rgba_len(width, height)?;
        if pixels.len() != expected {
            return Err(ImageCompareError::InvalidPixelLength {
                expected,
                actual: pixels.len(),
            });
        }
        if reservation.bytes() != expected {
            return Err(ImageCompareError::InvalidPixelLength {
                expected,
                actual: reservation.bytes(),
            });
        }
        Ok(Self {
            width,
            height,
            pixels,
            source,
            reservation,
        })
    }
}

pub(super) fn next_input_id() -> Result<ImageInputId, ImageCompareError> {
    NEXT_IMAGE_INPUT_ID
        .try_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            current.checked_add(1)
        })
        .map(ImageInputId)
        .map_err(|_| ImageCompareError::IdentityExhausted)
}

pub(super) fn checked_rgba_len(width: u32, height: u32) -> Result<usize, ImageCompareError> {
    if width == 0 || height == 0 {
        return Err(ImageCompareError::InvalidDimensions);
    }
    let pixels = usize::try_from(width)
        .ok()
        .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
        .ok_or(ImageCompareError::InvalidDimensions)?;
    if pixels > MAX_IMAGE_PIXELS {
        return Err(ImageCompareError::PixelLimit {
            pixels,
            limit: MAX_IMAGE_PIXELS,
        });
    }
    pixels
        .checked_mul(4)
        .ok_or(ImageCompareError::InvalidDimensions)
}

fn decoded_peak_reservation(pixels: usize) -> Result<usize, ImageCompareError> {
    pixels
        .checked_mul(MAX_DECODER_AND_CONVERSION_BYTES_PER_PIXEL)
        .and_then(|bytes| bytes.checked_add(DECODER_METADATA_RESERVATION_BYTES))
        .ok_or(ImageCompareError::InvalidDimensions)
}

pub(super) fn load_file(
    path: &Path,
    budget: &ImageCompareBudget,
    cancelled: &AtomicBool,
) -> Result<LoadedInput, ImageCompareError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(ImageCompareError::Cancelled);
    }
    let mut file = File::open(path).map_err(|error| {
        ImageCompareError::Read(format!("image file could not be opened: {error}"))
    })?;
    let before = file_stamp(&file, path)?;
    if !before.is_file {
        return Err(ImageCompareError::Read(
            "image source is not a regular file".into(),
        ));
    }
    if before.len > MAX_ENCODED_FILE_BYTES {
        return Err(ImageCompareError::EncodedInputTooLarge {
            bytes: before.len,
            limit: MAX_ENCODED_FILE_BYTES,
        });
    }
    let byte_len =
        usize::try_from(before.len).map_err(|_| ImageCompareError::EncodedInputTooLarge {
            bytes: before.len,
            limit: MAX_ENCODED_FILE_BYTES,
        })?;
    let encoded_lease = budget.reserve(byte_len)?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(byte_len).map_err(|error| {
        ImageCompareError::Read(format!("image file buffer could not be allocated: {error}"))
    })?;
    bytes.resize(byte_len, 0);
    let mut offset = 0;
    while offset < byte_len {
        if cancelled.load(Ordering::Acquire) {
            return Err(ImageCompareError::Cancelled);
        }
        let end = offset.saturating_add(FILE_READ_CHUNK_BYTES).min(byte_len);
        let read = file.read(&mut bytes[offset..end]).map_err(|error| {
            ImageCompareError::Read(format!("image file could not be read: {error}"))
        })?;
        if read == 0 {
            return Err(ImageCompareError::SourceChanged);
        }
        offset += read;
    }
    let mut extra = [0_u8; 1];
    if file.read(&mut extra).map_err(|error| {
        ImageCompareError::Read(format!(
            "image file could not be checked for growth: {error}"
        ))
    })? != 0
    {
        return Err(ImageCompareError::SourceChanged);
    }
    if file_stamp(&file, path)? != before {
        return Err(ImageCompareError::SourceChanged);
    }
    let display_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Image file".to_owned());
    let loaded = decode_encoded_reserved(
        &bytes,
        budget,
        cancelled,
        &encoded_lease,
        ImageInputSource::File {
            path: path.to_path_buf(),
            display_name,
        },
    )?;
    if file_stamp(&file, path)? != before {
        return Err(ImageCompareError::SourceChanged);
    }
    verify_file_contents_unchanged(&mut file, &bytes, cancelled)?;
    if file_stamp(&file, path)? != before {
        return Err(ImageCompareError::SourceChanged);
    }
    Ok(loaded)
}

fn verify_file_contents_unchanged(
    file: &mut File,
    expected: &[u8],
    cancelled: &AtomicBool,
) -> Result<(), ImageCompareError> {
    file.seek(SeekFrom::Start(0)).map_err(|error| {
        ImageCompareError::Read(format!("image source could not be revalidated: {error}"))
    })?;
    let mut chunk = [0_u8; FILE_READ_CHUNK_BYTES];
    let mut offset = 0;
    while offset < expected.len() {
        if cancelled.load(Ordering::Acquire) {
            return Err(ImageCompareError::Cancelled);
        }
        let amount = (expected.len() - offset).min(chunk.len());
        let mut read_total = 0;
        while read_total < amount {
            let read = file.read(&mut chunk[read_total..amount]).map_err(|error| {
                ImageCompareError::Read(format!("image source could not be revalidated: {error}"))
            })?;
            if read == 0 {
                return Err(ImageCompareError::SourceChanged);
            }
            read_total += read;
        }
        if chunk[..amount] != expected[offset..offset + amount] {
            return Err(ImageCompareError::SourceChanged);
        }
        offset += amount;
    }
    let mut extra = [0_u8; 1];
    if file.read(&mut extra).map_err(|error| {
        ImageCompareError::Read(format!("image source could not be revalidated: {error}"))
    })? != 0
    {
        return Err(ImageCompareError::SourceChanged);
    }
    Ok(())
}

pub(super) fn decode_encoded_reserved(
    bytes: &[u8],
    budget: &ImageCompareBudget,
    cancelled: &AtomicBool,
    _encoded_lease: &ImageBudgetLease,
    source: ImageInputSource,
) -> Result<LoadedInput, ImageCompareError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(ImageCompareError::Cancelled);
    }

    // Format sniffing reads only a fixed signature prefix. Do not construct a
    // decoder here: image 0.24's `into_dimensions` constructs codecs with
    // default limits, before their metadata and pixel allocations are charged.
    let header_reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| {
            ImageCompareError::Decode(format!("image format could not be read: {error}"))
        })?;
    let format = header_reader
        .format()
        .ok_or_else(|| ImageCompareError::Unsupported("image format is not recognized".into()))?;
    if !supported_format(format) {
        return Err(ImageCompareError::Unsupported(format!(
            "{} images are not supported by image comparison",
            format_name(format)
        )));
    }
    let (width, height) = preflight_dimensions(bytes, format)?;
    let rgba_len = checked_rgba_len(width, height)?;
    let pixels = rgba_len / 4;
    let peak = decoded_peak_reservation(pixels)?;
    let mut decode_lease = budget.reserve(peak)?;
    if cancelled.load(Ordering::Acquire) {
        return Err(ImageCompareError::Cancelled);
    }

    let mut limits = Limits::default();
    // Exact preflight dimensions reject codec/header disagreement before its
    // full pixel buffer is allocated.
    limits.max_image_width = Some(width);
    limits.max_image_height = Some(height);
    // Bound decoder-internal parser/metadata allocations independently from
    // the decoded/conversion surface reservation. Formats whose constructors
    // are not covered by this decoder limit also have bounded container
    // preflight and a 64 MiB encoded-input ceiling.
    limits.max_alloc = Some(u64::try_from(DECODER_METADATA_RESERVATION_BYTES).unwrap_or(u64::MAX));
    let mut reader = ImageReader::with_format(std::io::Cursor::new(bytes), format);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|error| {
        ImageCompareError::Decode(format!("image could not be decoded: {error}"))
    })?;
    if cancelled.load(Ordering::Acquire) {
        return Err(ImageCompareError::Cancelled);
    }
    if decoded.dimensions() != (width, height) {
        return Err(ImageCompareError::Decode(
            "decoded image dimensions did not match its bounded header".into(),
        ));
    }
    let pixels = decoded.into_rgba8().into_raw();
    if pixels.len() != rgba_len {
        return Err(ImageCompareError::InvalidPixelLength {
            expected: rgba_len,
            actual: pixels.len(),
        });
    }
    decode_lease.resize(rgba_len)?;
    LoadedInput::from_rgba(width, height, pixels, source, decode_lease)
}

fn supported_format(format: ImageFormat) -> bool {
    matches!(
        format,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Bmp | ImageFormat::Gif
    )
}

fn format_name(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "PNG",
        ImageFormat::Jpeg => "JPEG",
        ImageFormat::Bmp => "BMP",
        ImageFormat::Ico => "ICO",
        ImageFormat::Gif => "GIF",
        ImageFormat::Tiff => "TIFF",
        _ => "This format",
    }
}

/// Read dimensions without constructing a codec. Some image 0.24 codecs
/// allocate or copy metadata during construction, before `ImageReader::decode`
/// applies its configured limits. This parser operates on the already-capped
/// encoded slice; decoder and output allocations happen only after the
/// returned pixel count and peak reservation have been accepted.
pub(super) fn preflight_dimensions(
    bytes: &[u8],
    format: ImageFormat,
) -> Result<(u32, u32), ImageCompareError> {
    match format {
        ImageFormat::Png => png_dimensions(bytes),
        ImageFormat::Jpeg => jpeg_dimensions(bytes),
        ImageFormat::Bmp => bmp_dimensions(bytes),
        ImageFormat::Gif => gif_dimensions(bytes),
        ImageFormat::Ico | ImageFormat::Tiff => Err(ImageCompareError::Unsupported(format!(
            "{} images are not enabled because their decoder metadata cannot be bounded safely",
            format_name(format)
        ))),
        _ => Err(ImageCompareError::Unsupported(format!(
            "{} images are not supported by image comparison",
            format_name(format)
        ))),
    }
}

fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageCompareError> {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24
        || bytes.get(..8) != Some(PNG_SIGNATURE.as_slice())
        || read_u32_be(bytes, 8)? != 13
        || bytes.get(12..16) != Some(b"IHDR")
    {
        return Err(ImageCompareError::Decode(
            "PNG image has no complete leading IHDR header".into(),
        ));
    }
    let width = read_u32_be(bytes, 16)?;
    let height = read_u32_be(bytes, 20)?;
    checked_rgba_len(width, height)?;
    Ok((width, height))
}

fn bmp_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageCompareError> {
    if bytes.len() < 26 || bytes.get(..2) != Some(b"BM") {
        return Err(ImageCompareError::Decode(
            "BMP image has no complete file and bitmap header".into(),
        ));
    }
    let header_size = read_u32(bytes, 14)?;
    let (width, height) = match header_size {
        12 => (
            u32::from(read_u16(bytes, 18)?),
            u32::from(read_u16(bytes, 20)?),
        ),
        40 | 52 | 56 | 108 | 124 if bytes.len() >= 14 + header_size as usize => {
            let width = read_i32(bytes, 18)?;
            let height = read_i32(bytes, 22)?;
            if width <= 0 || height == 0 || height == i32::MIN {
                return Err(ImageCompareError::InvalidDimensions);
            }
            (width as u32, height.unsigned_abs())
        }
        _ => {
            return Err(ImageCompareError::Unsupported(
                "BMP header layout is not supported by the bounded reader".into(),
            ));
        }
    };
    checked_rgba_len(width, height)?;
    Ok((width, height))
}

fn gif_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageCompareError> {
    if bytes.len() < 10
        || !bytes
            .get(..6)
            .is_some_and(|signature| signature == b"GIF87a" || signature == b"GIF89a")
    {
        return Err(ImageCompareError::Decode(
            "GIF image has no complete logical-screen header".into(),
        ));
    }
    let width = u32::from(read_u16(bytes, 6)?);
    let height = u32::from(read_u16(bytes, 8)?);
    checked_rgba_len(width, height)?;
    Ok((width, height))
}

fn jpeg_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageCompareError> {
    if bytes.get(..2) != Some(&[0xff, 0xd8]) {
        return Err(ImageCompareError::Decode(
            "JPEG image has no start-of-image marker".into(),
        ));
    }
    let mut offset = 2usize;
    while offset < bytes.len() {
        if bytes[offset] != 0xff {
            offset += 1;
            continue;
        }
        while bytes.get(offset) == Some(&0xff) {
            offset += 1;
        }
        let marker = *bytes
            .get(offset)
            .ok_or_else(|| ImageCompareError::Decode("JPEG marker is truncated".into()))?;
        offset += 1;
        if marker == 0xd9 || marker == 0xda {
            break;
        }
        if marker == 0x00 || marker == 0x01 || (0xd0..=0xd8).contains(&marker) {
            continue;
        }
        let segment_length = usize::from(read_u16_be(bytes, offset)?);
        if segment_length < 2 {
            return Err(ImageCompareError::Decode(
                "JPEG segment has an invalid length".into(),
            ));
        }
        let segment_end = offset
            .checked_add(segment_length)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| ImageCompareError::Decode("JPEG segment is truncated".into()))?;
        if is_jpeg_start_of_frame(marker) {
            let payload = offset + 2;
            if segment_length < 8 {
                return Err(ImageCompareError::Decode(
                    "JPEG frame header is truncated".into(),
                ));
            }
            let height = u32::from(read_u16_be(bytes, payload + 1)?);
            let width = u32::from(read_u16_be(bytes, payload + 3)?);
            checked_rgba_len(width, height)?;
            return Ok((width, height));
        }
        offset = segment_end;
    }
    Err(ImageCompareError::Decode(
        "JPEG image has no bounded frame header before image data".into(),
    ))
}

fn is_jpeg_start_of_frame(marker: u8) -> bool {
    matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf)
}

fn read_u32_be(bytes: &[u8], offset: usize) -> Result<u32, ImageCompareError> {
    let end = offset
        .checked_add(4)
        .ok_or(ImageCompareError::InvalidDimensions)?;
    let raw = bytes
        .get(offset..end)
        .ok_or_else(|| ImageCompareError::Decode("image dimension header is truncated".into()))?;
    Ok(u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn read_u16_be(bytes: &[u8], offset: usize) -> Result<u16, ImageCompareError> {
    let end = offset
        .checked_add(2)
        .ok_or(ImageCompareError::InvalidDimensions)?;
    let raw = bytes
        .get(offset..end)
        .ok_or_else(|| ImageCompareError::Decode("image dimension header is truncated".into()))?;
    Ok(u16::from_be_bytes([raw[0], raw[1]]))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: Option<std::time::SystemTime>,
    is_file: bool,
    #[cfg(windows)]
    identity: crate::file_operations::NativeFileIdentity,
}

fn file_stamp(file: &File, path: &Path) -> Result<FileStamp, ImageCompareError> {
    let handle_metadata = file.metadata().map_err(|error| {
        ImageCompareError::Read(format!("image source metadata could not be read: {error}"))
    })?;
    let path_metadata = std::fs::metadata(path).map_err(|_| ImageCompareError::SourceChanged)?;
    let mut stamp = metadata_stamp(&handle_metadata);
    if stamp != metadata_stamp(&path_metadata) {
        return Err(ImageCompareError::SourceChanged);
    }
    // A file replacement can preserve size and timestamps. On Windows, bind
    // the open handle to the path with the volume serial and file index too.
    #[cfg(windows)]
    {
        let handle_identity =
            crate::file_operations::native_file_identity(file).map_err(|error| {
                ImageCompareError::Read(format!(
                    "image file identity could not be verified: {error}"
                ))
            })?;
        if handle_identity.volume_serial_number() == 0
            || handle_identity.file_id().iter().all(|byte| *byte == 0)
        {
            return Err(ImageCompareError::Read(
                "image file identity could not be verified".into(),
            ));
        }
        let path_file = File::open(path).map_err(|_| ImageCompareError::SourceChanged)?;
        let path_identity = crate::file_operations::native_file_identity(&path_file)
            .map_err(|_| ImageCompareError::SourceChanged)?;
        if handle_identity != path_identity {
            return Err(ImageCompareError::SourceChanged);
        }
        stamp.identity = handle_identity;
    }
    Ok(stamp)
}

fn metadata_stamp(metadata: &std::fs::Metadata) -> FileStamp {
    FileStamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
        is_file: metadata.is_file(),
        #[cfg(windows)]
        identity: crate::file_operations::NativeFileIdentity::default(),
    }
}

#[cfg(test)]
pub(super) fn decode_encoded_for_test(
    bytes: &[u8],
    label: &str,
) -> Result<ImageInput, ImageCompareError> {
    let budget = ImageCompareBudget::default();
    let encoded = budget.reserve(bytes.len())?;
    let loaded = decode_encoded_reserved(
        bytes,
        &budget,
        &AtomicBool::new(false),
        &encoded,
        ImageInputSource::Test {
            display_name: label.to_owned(),
        },
    )?;
    ImageInput::from_loaded(loaded, 1)
}

#[cfg(test)]
pub(super) fn from_raw_for_test(
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    label: &str,
) -> Result<ImageInput, ImageCompareError> {
    let rgba_len = checked_rgba_len(width, height)?;
    if pixels.len() != rgba_len {
        return Err(ImageCompareError::InvalidPixelLength {
            expected: rgba_len,
            actual: pixels.len(),
        });
    }
    let budget = ImageCompareBudget::default();
    let lease = budget.reserve(rgba_len)?;
    ImageInput::from_loaded(
        LoadedInput::from_rgba(
            width,
            height,
            pixels,
            ImageInputSource::Test {
                display_name: label.to_owned(),
            },
            lease,
        )?,
        1,
    )
}

#[cfg(test)]
pub(super) fn decode_dib_for_test(
    bytes: &[u8],
    format: u32,
) -> Result<TestDibImage, ImageCompareError> {
    let budget = ImageCompareBudget::default();
    let (width, height, pixels, lease) =
        decode_dib_bytes(bytes, format, &budget, &AtomicBool::new(false))?;
    Ok(TestDibImage {
        width,
        height,
        pixels,
        _lease: lease,
    })
}

#[cfg(test)]
pub(super) struct TestDibImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    _lease: ImageBudgetLease,
}

#[cfg(test)]
impl TestDibImage {
    pub(super) fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub(super) fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

/// Build an immutable input from an already bounded, application-owned RGBA
/// vector. This is also the pure seam used by clipboard and decoder tests.
pub(super) fn input_from_rgba(
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    source: ImageInputSource,
    budget: &ImageCompareBudget,
    revision: u64,
) -> Result<ImageInput, ImageCompareError> {
    let rgba_len = checked_rgba_len(width, height)?;
    if pixels.len() != rgba_len {
        return Err(ImageCompareError::InvalidPixelLength {
            expected: rgba_len,
            actual: pixels.len(),
        });
    }
    let lease = budget.reserve(rgba_len)?;
    ImageInput::from_loaded(
        LoadedInput::from_rgba(width, height, pixels, source, lease)?,
        revision,
    )
}

#[cfg(windows)]
pub(super) fn load_clipboard(
    budget: &ImageCompareBudget,
    cancelled: &AtomicBool,
) -> Result<LoadedInput, ImageCompareError> {
    super::clipboard::load_windows_clipboard(budget, cancelled)
}

#[cfg(not(windows))]
pub(super) fn load_clipboard(
    _budget: &ImageCompareBudget,
    _cancelled: &AtomicBool,
) -> Result<LoadedInput, ImageCompareError> {
    Err(ImageCompareError::Unsupported(
        "bounded image clipboard input is only available on Windows".into(),
    ))
}

pub(super) fn decode_dib_bytes(
    bytes: &[u8],
    format: u32,
    budget: &ImageCompareBudget,
    cancelled: &AtomicBool,
) -> Result<(u32, u32, Vec<u8>, ImageBudgetLease), ImageCompareError> {
    const CF_DIB: u32 = 8;
    const CF_DIBV5: u32 = 17;
    const BI_RGB: u32 = 0;

    if format != CF_DIB && format != CF_DIBV5 {
        return Err(ImageCompareError::Unsupported(
            "clipboard image format is unsupported; use CF_DIB or CF_DIBV5".into(),
        ));
    }
    if bytes.len() < 40 {
        return Err(ImageCompareError::Decode(
            "clipboard DIB header is truncated".into(),
        ));
    }
    let header_size = read_u32(bytes, 0)? as usize;
    if header_size < 40 || header_size > bytes.len() || (format == CF_DIBV5 && header_size < 124) {
        return Err(ImageCompareError::Decode(
            "clipboard DIB header size is invalid".into(),
        ));
    }
    let width_raw = read_i32(bytes, 4)?;
    let height_raw = read_i32(bytes, 8)?;
    if width_raw <= 0 || height_raw == 0 || height_raw == i32::MIN {
        return Err(ImageCompareError::InvalidDimensions);
    }
    let width = width_raw as u32;
    let height = height_raw.unsigned_abs();
    if read_u16(bytes, 12)? != 1 {
        return Err(ImageCompareError::Unsupported(
            "clipboard DIB has an unsupported plane count".into(),
        ));
    }
    let bits_per_pixel = read_u16(bytes, 14)?;
    let compression = read_u32(bytes, 16)?;
    if compression != BI_RGB || !matches!(bits_per_pixel, 24 | 32) {
        return Err(ImageCompareError::Unsupported(
            "clipboard image must be an uncompressed 24-bit or 32-bit DIB".into(),
        ));
    }
    if read_u32(bytes, 32)? != 0 {
        return Err(ImageCompareError::Unsupported(
            "clipboard DIB color tables are not supported".into(),
        ));
    }
    if format == CF_DIBV5 {
        let profile_data = read_u32(bytes, 112)?;
        let profile_size = read_u32(bytes, 116)?;
        if profile_data != 0 || profile_size != 0 {
            return Err(ImageCompareError::Unsupported(
                "clipboard DIB color profiles are not supported".into(),
            ));
        }
    }
    let rgba_len = checked_rgba_len(width, height)?;
    let bits = u32::from(bits_per_pixel);
    let row_bits = usize::try_from(width)
        .ok()
        .and_then(|value| value.checked_mul(bits as usize))
        .ok_or(ImageCompareError::InvalidDimensions)?;
    let stride = row_bits
        .checked_add(31)
        .and_then(|value| value.checked_div(32))
        .and_then(|value| value.checked_mul(4))
        .ok_or(ImageCompareError::InvalidDimensions)?;
    let pixel_bytes = stride
        .checked_mul(height as usize)
        .ok_or(ImageCompareError::InvalidDimensions)?;
    let required = header_size
        .checked_add(pixel_bytes)
        .ok_or(ImageCompareError::InvalidDimensions)?;
    if required > bytes.len() {
        return Err(ImageCompareError::Decode(
            "clipboard DIB pixel storage is truncated".into(),
        ));
    }
    let alpha_mask = if format == CF_DIBV5 && bits_per_pixel == 32 {
        read_u32(bytes, 52)?
    } else {
        0
    };
    if alpha_mask != 0 && alpha_mask != 0xff00_0000 {
        return Err(ImageCompareError::Unsupported(
            "clipboard DIB alpha mask is unsupported".into(),
        ));
    }
    let reservation = budget.reserve(rgba_len)?;
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(rgba_len).map_err(|error| {
        ImageCompareError::Read(format!(
            "clipboard image buffer could not be allocated: {error}"
        ))
    })?;
    pixels.resize(rgba_len, 0);
    let top_down = height_raw < 0;
    let source_bytes_per_pixel = usize::from(bits_per_pixel / 8);
    for y in 0..height as usize {
        if cancelled.load(Ordering::Acquire) {
            return Err(ImageCompareError::Cancelled);
        }
        let source_y = if top_down { y } else { height as usize - 1 - y };
        let row_start = header_size + source_y * stride;
        for x in 0..width as usize {
            let source_index = row_start + x * source_bytes_per_pixel;
            let output_index = (y * width as usize + x) * 4;
            pixels[output_index] = bytes[source_index + 2];
            pixels[output_index + 1] = bytes[source_index + 1];
            pixels[output_index + 2] = bytes[source_index];
            pixels[output_index + 3] = if source_bytes_per_pixel == 4 && alpha_mask != 0 {
                bytes[source_index + 3]
            } else {
                255
            };
        }
    }
    Ok((width, height, pixels, reservation))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ImageCompareError> {
    let end = offset
        .checked_add(2)
        .ok_or(ImageCompareError::InvalidDimensions)?;
    let raw = bytes
        .get(offset..end)
        .ok_or_else(|| ImageCompareError::Decode("clipboard DIB header is truncated".into()))?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ImageCompareError> {
    let end = offset
        .checked_add(4)
        .ok_or(ImageCompareError::InvalidDimensions)?;
    let raw = bytes
        .get(offset..end)
        .ok_or_else(|| ImageCompareError::Decode("clipboard DIB header is truncated".into()))?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, ImageCompareError> {
    Ok(read_u32(bytes, offset)? as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn image_compare_domain_file_load_is_bounded_and_detects_source_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("small.png");
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(2, 1, image::Rgba([1, 2, 3, 4]))
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        std::fs::write(&path, bytes.into_inner()).unwrap();
        let budget = ImageCompareBudget::default();
        let loaded = load_file(&path, &budget, &AtomicBool::new(false)).unwrap();
        assert_eq!((loaded.width, loaded.height), (2, 1));
        assert_eq!(loaded.pixels, [1, 2, 3, 4, 1, 2, 3, 4]);

        let grow_path = temp.path().join("growing.bin");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&grow_path)
            .unwrap();
        file.write_all(b"old").unwrap();
        file.sync_all().unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(b"new").unwrap();
        file.sync_all().unwrap();
        assert!(matches!(
            verify_file_contents_unchanged(&mut file, b"old", &AtomicBool::new(false)),
            Err(ImageCompareError::SourceChanged)
        ));
    }
}
