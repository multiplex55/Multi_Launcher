//! Transactional staging of reference images used while authoring macros.
//!
//! Staging and document replacement are deliberately separate.  In particular,
//! callers must not remove the old asset until the edited macro has been saved;
//! the store's normal `cleanup_assets` pass is the appropriate place to collect
//! files which are no longer referenced by the saved document.

use super::MkMacroStore;
use anyhow::{Context, Result};
use image::{DynamicImage, ImageDecoder, ImageFormat, RgbaImage, codecs::png::PngDecoder};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

/// Shared import/preview decode budget. It bounds allocations made from untrusted PNG headers.
pub const MAX_IMAGE_DIMENSION: u32 = 16_384;
pub const MAX_DECODED_RGBA_BYTES: u64 = 64 * 1024 * 1024;

pub fn validated_rgba_len(width: u32, height: u32) -> Result<usize> {
    if width == 0 || height == 0 {
        anyhow::bail!("Reference image is too large to import")
    }
    let width_usize = usize::try_from(width)
        .map_err(|_| anyhow::anyhow!("Reference image is too large to import"))?;
    let height_usize = usize::try_from(height)
        .map_err(|_| anyhow::anyhow!("Reference image is too large to import"))?;
    let pixels = width_usize
        .checked_mul(height_usize)
        .ok_or_else(|| anyhow::anyhow!("Reference image is too large to import"))?;
    let bytes = pixels
        .checked_mul(4)
        .ok_or_else(|| anyhow::anyhow!("Reference image is too large to import"))?;
    if width > MAX_IMAGE_DIMENSION
        || height > MAX_IMAGE_DIMENSION
        || bytes as u64 > MAX_DECODED_RGBA_BYTES
    {
        anyhow::bail!("Reference image is too large to import")
    }
    Ok(bytes)
}

pub fn validate_image_dimensions(width: u32, height: u32) -> Result<()> {
    validated_rgba_len(width, height).map(|_| ())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedImageAsset {
    pub asset_id: u64,
    /// Portable managed reference (`mkmacro_assets/<macro>/<id>.png`).
    pub managed_reference: PathBuf,
}

pub struct ImageAssetAuthoringService<'a> {
    store: &'a MkMacroStore,
}

impl<'a> ImageAssetAuthoringService<'a> {
    pub fn new(store: &'a MkMacroStore) -> Self {
        Self { store }
    }

    pub fn import_png(&self, macro_id: u64, source: &Path) -> Result<StagedImageAsset> {
        // Decode before allocation and before any draft can be changed.  Supplying
        // the format explicitly prevents a renamed JPEG from being accepted.
        let bytes = std::fs::read(source)
            .with_context(|| format!("read reference image {}", source.display()))?;
        let decoder = PngDecoder::new(Cursor::new(&bytes))
            .with_context(|| format!("{} is not a valid PNG image", source.display()))?;
        let (width, height) = decoder.dimensions();
        validate_image_dimensions(width, height)?;
        let image = DynamicImage::from_decoder(decoder)
            .with_context(|| format!("decode PNG image {}", source.display()))?
            .to_rgba8();
        self.stage_rgba(macro_id, &image)
    }

    pub fn stage_rgba(&self, macro_id: u64, image: &RgbaImage) -> Result<StagedImageAsset> {
        validate_image_dimensions(image.width(), image.height())?;
        let (asset_id, managed_reference) = self.store.stage_new_png_asset(macro_id, image)?;
        Ok(StagedImageAsset {
            asset_id,
            managed_reference,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, GenericImageView, Rgb, RgbImage, Rgba};
    use std::io::Cursor;

    fn fixture() -> (tempfile::TempDir, MkMacroStore) {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn png_import_is_managed_and_independent_of_source() {
        let (dir, store) = fixture();
        let source = dir.path().join("chosen.png");
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(3, 2, Rgba([1, 2, 3, 255])))
            .save_with_format(&source, ImageFormat::Png)
            .unwrap();
        let staged = ImageAssetAuthoringService::new(&store)
            .import_png(7, &source)
            .unwrap();
        assert_eq!(staged.asset_id, 1);
        assert_eq!(
            staged.managed_reference,
            Path::new("mkmacro_assets/7/1.png")
        );
        std::fs::remove_file(source).unwrap();
        assert_eq!(
            image::open(store.asset_path(7, 1).unwrap())
                .unwrap()
                .dimensions(),
            (3, 2)
        );
    }

    #[test]
    fn rgb_png_is_converted_to_rgba() {
        let (dir, store) = fixture();
        let source = dir.path().join("rgb.png");
        DynamicImage::ImageRgb8(RgbImage::from_pixel(2, 1, Rgb([4, 5, 6])))
            .save_with_format(&source, ImageFormat::Png)
            .unwrap();
        let staged = ImageAssetAuthoringService::new(&store)
            .import_png(8, &source)
            .unwrap();
        assert_eq!(
            image::open(store.asset_path(8, staged.asset_id).unwrap())
                .unwrap()
                .to_rgba8()
                .get_pixel(0, 0)
                .0,
            [4, 5, 6, 255]
        );
    }

    #[test]
    fn dimension_budget_rejects_checked_pixel_and_byte_limits() {
        assert!(validate_image_dimensions(0, 1).is_err());
        assert!(
            validate_image_dimensions(u32::MAX, u32::MAX)
                .unwrap_err()
                .to_string()
                .contains("too large")
        );
        assert!(
            validate_image_dimensions(4097, 4096)
                .unwrap_err()
                .to_string()
                .contains("too large")
        );
    }

    #[test]
    fn oversized_png_has_friendly_error_and_allocates_nothing() {
        let (dir, store) = fixture();
        let source = dir.path().join("huge.png");
        DynamicImage::ImageRgb8(RgbImage::new(MAX_IMAGE_DIMENSION + 1, 1))
            .save_with_format(&source, ImageFormat::Png)
            .unwrap();
        let error = ImageAssetAuthoringService::new(&store)
            .import_png(9, &source)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Reference image is too large to import")
        );
        assert!(store.asset_ids(9).unwrap().is_empty());
    }

    #[test]
    fn strict_png_rejects_renamed_jpeg_and_corruption() {
        let (dir, store) = fixture();
        let service = ImageAssetAuthoringService::new(&store);
        let mut jpeg = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(RgbImage::from_pixel(1, 1, Rgb([1, 2, 3])))
            .write_to(&mut jpeg, ImageFormat::Jpeg)
            .unwrap();
        for (name, bytes) in [
            ("renamed.png", jpeg.into_inner()),
            ("bad.png", b"garbage".to_vec()),
        ] {
            let source = dir.path().join(name);
            std::fs::write(&source, bytes).unwrap();
            assert!(service.import_png(3, &source).is_err());
        }
        assert!(store.asset_ids(3).unwrap().is_empty());
    }

    #[test]
    fn allocation_does_not_replace_existing_and_empty_capture_is_rejected() {
        let (_dir, store) = fixture();
        let first = RgbaImage::from_pixel(1, 1, Rgba([9, 8, 7, 255]));
        store.write_png_asset(4, 1, &first).unwrap();
        let second = RgbaImage::from_pixel(1, 1, Rgba([1, 2, 3, 255]));
        let staged = ImageAssetAuthoringService::new(&store)
            .stage_rgba(4, &second)
            .unwrap();
        assert_eq!(staged.asset_id, 2);
        assert_eq!(
            image::open(store.asset_path(4, 1).unwrap())
                .unwrap()
                .to_rgba8(),
            first
        );
        assert!(
            ImageAssetAuthoringService::new(&store)
                .stage_rgba(4, &RgbaImage::new(0, 1))
                .is_err()
        );
        assert_eq!(store.asset_ids(4).unwrap(), vec![1, 2]);
        assert_eq!(
            image::open(store.asset_path(4, 1).unwrap())
                .unwrap()
                .to_rgba8(),
            first,
            "a rejected replacement must neither remove nor overwrite the old PNG"
        );
    }

    #[test]
    fn staged_capture_png_round_trips_dimensions_and_rgba_pixels() {
        let (_dir, store) = fixture();
        let image = RgbaImage::from_fn(3, 2, |x, y| Rgba([x as u8, y as u8, 77, 128 + x as u8]));
        let staged = ImageAssetAuthoringService::new(&store)
            .stage_rgba(12, &image)
            .unwrap();
        let decoded = image::open(store.asset_path(12, staged.asset_id).unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(decoded.dimensions(), (3, 2));
        assert_eq!(decoded.get_pixel(0, 0).0, [0, 0, 77, 128]);
        assert_eq!(decoded.get_pixel(2, 1).0, [2, 1, 77, 130]);
    }
}
