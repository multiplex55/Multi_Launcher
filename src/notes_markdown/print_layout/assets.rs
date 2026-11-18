//! Bounded in-memory raster preparation for print scenes. No filesystem or network access.
use super::MAX_ASSET_BYTES;
use crate::notes_markdown::html::{
    self, LocalRasterFormat, LocalRasterResolver, MAX_LOCAL_RASTER_AGGREGATE_BYTES,
    MAX_LOCAL_RASTER_BYTES, MAX_MARKDOWN_HTML_IMAGES,
};
use image_025::ImageDecoder as _;
use sha2::{Digest, Sha256};
use std::{borrow::Cow, collections::HashMap, io::Cursor, sync::Arc};

const MAX_MERMAID_RENDERS: usize = 16;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrintRaster {
    pub width: u32,
    pub height: u32,
    /// Straight-alpha row-major RGBA8.
    pub rgba: Arc<[u8]>,
}
#[derive(Clone, Debug)]
pub(super) struct PreparedRaster {
    pub index: usize,
    pub width_hint: Option<f32>,
}
#[derive(Clone, Debug)]
pub(super) enum AssetFailure {
    Cancelled,
    Unavailable(&'static str),
    Mermaid(String),
}

// PNG ICC profiles are not used for print RGBA. Remove only authenticated
// iCCP chunks before constructing the decoder, which otherwise inflates them
// before the image dimensions and decoder-output budget are available.
fn png_without_icc<'a>(
    bytes: &'a [u8],
    cancel: &mut impl FnMut() -> bool,
) -> Result<Cow<'a, [u8]>, AssetFailure> {
    let mut offset = 8usize;
    let mut filtered: Option<Vec<u8>> = None;
    let mut seen = false;
    while offset < bytes.len() {
        if cancel() {
            return Err(AssetFailure::Cancelled);
        }
        let head = bytes
            .get(offset..offset + 8)
            .ok_or(AssetFailure::Unavailable("Invalid PNG chunk header."))?;
        let length = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        let end = offset
            .checked_add(12)
            .and_then(|n| n.checked_add(length))
            .filter(|end| *end <= bytes.len())
            .ok_or(AssetFailure::Unavailable("Invalid PNG chunk size."))?;
        let kind = &head[4..8];
        if kind == b"iCCP" {
            if seen {
                return Err(AssetFailure::Unavailable(
                    "PNG has duplicate ICC profile chunks.",
                ));
            }
            seen = true;
            let data = &bytes[offset + 4..end - 4];
            let expected = u32::from_be_bytes(
                bytes[end - 4..end]
                    .try_into()
                    .map_err(|_| AssetFailure::Unavailable("Invalid PNG CRC."))?,
            );
            let mut crc = flate2::Crc::new();
            for piece in data.chunks(4096) {
                if cancel() {
                    return Err(AssetFailure::Cancelled);
                }
                crc.update(piece);
            }
            if crc.sum() != expected {
                return Err(AssetFailure::Unavailable(
                    "PNG ICC profile chunk has an invalid checksum.",
                ));
            }
            if filtered.is_none() {
                filtered = Some(bytes[..offset].to_vec());
            }
        } else if let Some(out) = filtered.as_mut() {
            out.extend_from_slice(&bytes[offset..end]);
        }
        offset = end;
        if kind == b"IEND" {
            if let Some(out) = filtered.as_mut() {
                out.extend_from_slice(&bytes[offset..]);
            }
            break;
        }
    }
    Ok(filtered.map_or(Cow::Borrowed(bytes), Cow::Owned))
}

pub(super) struct Assets<'a> {
    resolver: Option<&'a dyn LocalRasterResolver>,
    pub rasters: Vec<PrintRaster>,
    destinations: HashMap<String, Result<PreparedRaster, AssetFailure>>,
    contents: Vec<(
        [u8; 32],
        LocalRasterFormat,
        Arc<[u8]>,
        Result<usize, AssetFailure>,
    )>,
    borrowed: HashMap<(usize, usize, u8), Result<usize, AssetFailure>>,
    diagrams: HashMap<String, Result<usize, AssetFailure>>,
    compressed: usize,
    decoded: usize,
    image_count: usize,
    diagram_count: usize,
}
impl<'a> Assets<'a> {
    pub fn new(resolver: Option<&'a dyn LocalRasterResolver>) -> Self {
        Self {
            resolver,
            rasters: Vec::new(),
            destinations: HashMap::new(),
            contents: Vec::new(),
            borrowed: HashMap::new(),
            diagrams: HashMap::new(),
            compressed: 0,
            decoded: 0,
            image_count: 0,
            diagram_count: 0,
        }
    }
    pub fn local(
        &mut self,
        destination: &str,
        cancel: &mut impl FnMut() -> bool,
    ) -> Result<PreparedRaster, AssetFailure> {
        if cancel() {
            return Err(AssetFailure::Cancelled);
        }
        self.image_count += 1;
        if self.image_count > MAX_MARKDOWN_HTML_IMAGES {
            return Err(AssetFailure::Unavailable("Image count limit exceeded."));
        }
        if let Some(cached) = self.destinations.get(destination) {
            return cached.clone();
        }
        let result = self.load(destination, cancel).map(|index| {
            let width_hint = self
                .resolver
                .and_then(|r| r.width_hint_px(destination))
                .filter(|w| w.is_finite() && (1.0..=4096.0).contains(w));
            PreparedRaster { index, width_hint }
        });
        if !matches!(result, Err(AssetFailure::Cancelled)) {
            self.destinations
                .insert(destination.to_owned(), result.clone());
        }
        result
    }
    fn load(
        &mut self,
        destination: &str,
        cancel: &mut impl FnMut() -> bool,
    ) -> Result<usize, AssetFailure> {
        if !html::is_local_relative_destination(destination) {
            return Err(AssetFailure::Unavailable(
                "Image destination is not an allowed local relative path.",
            ));
        }
        let asset =
            self.resolver
                .and_then(|r| r.resolve(destination))
                .ok_or(AssetFailure::Unavailable(
                    "Local image is unavailable or blocked by the resolver.",
                ))?;
        let identity = (
            asset.bytes.as_ptr() as usize,
            asset.bytes.len(),
            asset.format as u8,
        );
        if let Some(cached) = self.borrowed.get(&identity) {
            return cached.clone();
        }
        if asset.bytes.is_empty() || asset.bytes.len() > MAX_LOCAL_RASTER_BYTES {
            return Err(AssetFailure::Unavailable(
                "Local image exceeds the compressed size limit or is empty.",
            ));
        }
        if self.compressed.saturating_add(asset.bytes.len()) > MAX_LOCAL_RASTER_AGGREGATE_BYTES {
            return Err(AssetFailure::Unavailable(
                "Aggregate compressed image work limit exceeded.",
            ));
        }
        self.compressed += asset.bytes.len();
        if cancel() {
            return Err(AssetFailure::Cancelled);
        }
        let digest: [u8; 32] = Sha256::digest(asset.bytes).into();
        if let Some((_, _, _, result)) = self.contents.iter().find(|(hash, format, bytes, _)| {
            *hash == digest && *format == asset.format && bytes.as_ref() == asset.bytes
        }) {
            let result = result.clone();
            self.borrowed.insert(identity, result.clone());
            return result;
        }
        let result = self.decode(asset.format, asset.bytes, cancel);
        if !matches!(result, Err(AssetFailure::Cancelled)) {
            self.contents
                .push((digest, asset.format, Arc::from(asset.bytes), result.clone()));
            self.borrowed.insert(identity, result.clone());
        }
        result
    }
    fn decode(
        &mut self,
        format: LocalRasterFormat,
        source: &[u8],
        cancel: &mut impl FnMut() -> bool,
    ) -> Result<usize, AssetFailure> {
        if !format.signature_matches(source) {
            return Err(AssetFailure::Unavailable(
                "Local image signature does not match its format.",
            ));
        }
        let decoded_source = if format == LocalRasterFormat::Png {
            png_without_icc(source, cancel)?
        } else {
            Cow::Borrowed(source)
        };
        let format = match format {
            LocalRasterFormat::Png => image_025::ImageFormat::Png,
            LocalRasterFormat::Jpeg => image_025::ImageFormat::Jpeg,
            LocalRasterFormat::Gif => image_025::ImageFormat::Gif,
            LocalRasterFormat::WebP => image_025::ImageFormat::WebP,
        };
        let mut limits = image_025::Limits::default();
        limits.max_image_width = Some(html::MAX_LOCAL_RASTER_DIMENSION);
        limits.max_image_height = Some(html::MAX_LOCAL_RASTER_DIMENSION);
        limits.max_alloc = Some(MAX_ASSET_BYTES.saturating_sub(self.decoded) as u64);
        let mut reader =
            image_025::ImageReader::with_format(Cursor::new(decoded_source.as_ref()), format);
        reader.limits(limits.clone());
        let mut decoder = reader.into_decoder().map_err(|_| {
            AssetFailure::Unavailable("Local image metadata could not be decoded within limits.")
        })?;
        let (width, height) = decoder.dimensions();
        if width == 0
            || height == 0
            || u64::from(width) * u64::from(height) > html::MAX_LOCAL_RASTER_PIXELS
        {
            return Err(AssetFailure::Unavailable(
                "Local image dimensions are invalid or exceed limits.",
            ));
        }
        let decoded = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or(AssetFailure::Unavailable(
                "Image dimensions exceed the decoded limit.",
            ))?;
        let attempted = decoded.max(usize::try_from(decoder.total_bytes()).map_err(|_| {
            AssetFailure::Unavailable("Local image decoder output exceeds limits.")
        })?);
        if self.decoded.saturating_add(attempted) > MAX_ASSET_BYTES {
            return Err(AssetFailure::Unavailable(
                "Aggregate decoded image limit exceeded.",
            ));
        }
        limits
            .reserve(decoder.total_bytes())
            .map_err(|_| AssetFailure::Unavailable("Local image decoder output exceeds limits."))?;
        decoder.set_limits(limits).map_err(|_| {
            AssetFailure::Unavailable("Local image decoder limits could not be enforced.")
        })?;
        self.decoded += attempted;
        let image = image_025::DynamicImage::from_decoder(decoder)
            .map_err(|_| {
                AssetFailure::Unavailable("Local image could not be decoded within limits.")
            })?
            .into_rgba8();
        if cancel() {
            return Err(AssetFailure::Cancelled);
        }
        let index = self.rasters.len();
        self.rasters.push(PrintRaster {
            width,
            height,
            rgba: Arc::from(image.into_raw()),
        });
        Ok(index)
    }
    pub fn mermaid(
        &mut self,
        source: &str,
        cancel: &mut impl FnMut() -> bool,
    ) -> Result<usize, AssetFailure> {
        if cancel() {
            return Err(AssetFailure::Cancelled);
        }
        if let Some(cached) = self.diagrams.get(source) {
            return cached.clone();
        }
        self.diagram_count += 1;
        if self.diagram_count > MAX_MERMAID_RENDERS {
            return Err(AssetFailure::Unavailable(
                "Mermaid render count limit exceeded.",
            ));
        }
        let size = crate::diagram::RasterSize::new(640, 480)
            .map_err(|e| AssetFailure::Mermaid(e.to_string()))?;
        let output = crate::diagram::render_mermaid_preview(source, size)
            .map_err(|e| AssetFailure::Mermaid(e.to_string()));
        if cancel() {
            return Err(AssetFailure::Cancelled);
        }
        let result = output.and_then(|output| {
            let scale = (640.0 / output.natural_width).min(480.0 / output.natural_height);
            let width = (output.natural_width * scale).ceil().clamp(1.0, 640.0) as u32;
            let height = (output.natural_height * scale).ceil().clamp(1.0, 480.0) as u32;
            let bytes = width as usize * height as usize * 4;
            if self.decoded.saturating_add(bytes) > MAX_ASSET_BYTES {
                return Err(AssetFailure::Unavailable(
                    "Aggregate decoded image limit exceeded.",
                ));
            }
            let mut rgba = Vec::with_capacity(bytes);
            for row in 0..height as usize {
                let start = row * 640 * 4;
                rgba.extend_from_slice(&output.raster.rgba[start..start + width as usize * 4]);
            }
            let index = self.rasters.len();
            self.rasters.push(PrintRaster {
                width,
                height,
                rgba: Arc::from(rgba),
            });
            self.decoded += bytes;
            Ok(index)
        });
        self.diagrams.insert(source.to_owned(), result.clone());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    struct Resolver {
        blobs: Vec<Vec<u8>>,
        hints: Cell<usize>,
    }
    impl LocalRasterResolver for Resolver {
        fn resolve<'a>(&'a self, name: &str) -> Option<html::LocalRasterAsset<'a>> {
            let index = name.parse::<usize>().ok()?;
            Some(html::LocalRasterAsset {
                format: LocalRasterFormat::Png,
                bytes: self.blobs.get(index)?,
            })
        }
        fn width_hint_px(&self, _: &str) -> Option<f32> {
            self.hints.set(self.hints.get() + 1);
            Some(40.0)
        }
    }
    #[test]
    fn png_icc_chunk_is_checked_and_removed_before_decoding() {
        let image = image_025::DynamicImage::ImageRgba8(image_025::ImageBuffer::from_pixel(
            2,
            2,
            image_025::Rgba([1, 2, 3, 255]),
        ));
        let mut output = Cursor::new(Vec::new());
        image
            .write_to(&mut output, image_025::ImageFormat::Png)
            .unwrap();
        let original = output.into_inner();
        let data = b"profile\0\0not-a-zlib-stream";
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
        chunk.extend_from_slice(b"iCCP");
        chunk.extend_from_slice(data);
        let mut crc = flate2::Crc::new();
        crc.update(&chunk[4..]);
        chunk.extend_from_slice(&crc.sum().to_be_bytes());
        let mut png = original[..33].to_vec();
        png.extend_from_slice(&chunk);
        png.extend_from_slice(&original[33..]);
        assert_eq!(
            png_without_icc(&png, &mut || false).unwrap().as_ref(),
            original.as_slice()
        );
        let resolver = Resolver {
            blobs: vec![png.clone()],
            hints: Cell::new(0),
        };
        let mut assets = Assets::new(Some(&resolver));
        assert!(assets.local("0", &mut || false).is_ok());
        png[33 + 8] ^= 1;
        assert!(png_without_icc(&png, &mut || false).is_err());
    }

    #[test]
    fn failed_inputs_charge_attempted_budget_and_unsafe_paths_skip_resolver() {
        let resolver = Resolver {
            blobs: (0..5).map(|i| vec![i; 1024 * 1024]).collect(),
            hints: Cell::new(0),
        };
        let mut assets = Assets::new(Some(&resolver));
        assert!(
            assets
                .local("https://example.test/x", &mut || false)
                .is_err()
        );
        assert_eq!(resolver.hints.get(), 0);
        for i in 0..4 {
            assert!(assets.local(&i.to_string(), &mut || false).is_err());
        }
        assert_eq!(assets.compressed, 4 * 1024 * 1024);
        assert!(
            matches!(assets.local("4",&mut ||false),Err(AssetFailure::Unavailable(message)) if message.contains("work limit"))
        );
        assert_eq!(resolver.hints.get(), 0);
        assert!(assets.local("0", &mut || false).is_err());
        assert_eq!(assets.compressed, 4 * 1024 * 1024);
    }
}
