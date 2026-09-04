//! Transactional staging of reference images used while authoring macros.
//!
//! Staging and document replacement are deliberately separate.  In particular,
//! callers never own the files they reference. Library deletion is an explicit
//! store-level operation outside action editing.

use super::{ImageImportChoice, ImageImportResult, MkImageRef, MkMacroStore};
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
        anyhow::bail!(
            "Reference image is too large to import (dimension/pixel/decoded-size limit exceeded)"
        )
    }
    let width_usize = usize::try_from(width).map_err(|_| {
        anyhow::anyhow!(
            "Reference image is too large to import (dimension/pixel/decoded-size limit exceeded)"
        )
    })?;
    let height_usize = usize::try_from(height).map_err(|_| {
        anyhow::anyhow!(
            "Reference image is too large to import (dimension/pixel/decoded-size limit exceeded)"
        )
    })?;
    let pixels = width_usize.checked_mul(height_usize).ok_or_else(|| {
        anyhow::anyhow!(
            "Reference image is too large to import (dimension/pixel/decoded-size limit exceeded)"
        )
    })?;
    let bytes = pixels.checked_mul(4).ok_or_else(|| {
        anyhow::anyhow!(
            "Reference image is too large to import (dimension/pixel/decoded-size limit exceeded)"
        )
    })?;
    if width > MAX_IMAGE_DIMENSION
        || height > MAX_IMAGE_DIMENSION
        || bytes as u64 > MAX_DECODED_RGBA_BYTES
    {
        anyhow::bail!(
            "Reference image is too large to import (dimension/pixel/decoded-size limit exceeded)"
        )
    }
    Ok(bytes)
}

pub fn validate_image_dimensions(width: u32, height: u32) -> Result<()> {
    validated_rgba_len(width, height).map(|_| ())
}

pub struct ImageAssetAuthoringService<'a> {
    store: &'a MkMacroStore,
}

impl<'a> ImageAssetAuthoringService<'a> {
    pub fn new(store: &'a MkMacroStore) -> Self {
        Self { store }
    }

    pub fn import_png(&self, source: &Path) -> Result<ImageImportResult> {
        self.store.import_png(source)
    }

    pub fn import_png_with_choice(
        &self,
        source: &Path,
        choice: ImageImportChoice,
    ) -> Result<ImageImportResult> {
        self.store.import_png_with_choice(source, choice)
    }

    pub fn stage_rgba(
        &self,
        image: &RgbaImage,
        filename: MkImageRef,
        choice: ImageImportChoice,
    ) -> Result<ImageImportResult> {
        validate_image_dimensions(image.width(), image.height())?;
        self.store.write_captured_png(image, filename, choice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, GenericImageView, ImageFormat, Rgb, RgbImage, Rgba};
    use std::io::Cursor;

    fn fixture() -> (tempfile::TempDir, MkMacroStore) {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        (dir, store)
    }

    fn write_png(path: &Path, color: [u8; 4]) {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(3, 2, Rgba(color)))
            .save_with_format(path, ImageFormat::Png)
            .unwrap();
    }

    #[test]
    fn external_import_uses_source_filename_and_in_root_import_copies_nothing() {
        let (dir, store) = fixture();
        let source = dir.path().join("chosen.png");
        write_png(&source, [1, 2, 3, 255]);
        let image = match ImageAssetAuthoringService::new(&store)
            .import_png(&source)
            .unwrap()
        {
            ImageImportResult::Imported(image) => image,
            other => panic!("unexpected import result: {other:?}"),
        };
        assert_eq!(image.filename(), "chosen.png");
        assert_eq!(store.image_path(&image).unwrap().is_file(), true);
        std::fs::remove_file(source).unwrap();
        assert_eq!(
            store.validate_image_ref(&image).unwrap().dimensions(),
            (3, 2)
        );

        let in_root = store
            .image_path(&MkImageRef::from_filename("inside.png"))
            .unwrap();
        write_png(&in_root, [4, 5, 6, 255]);
        let before = std::fs::read(&in_root).unwrap();
        assert_eq!(
            ImageAssetAuthoringService::new(&store)
                .import_png(&in_root)
                .unwrap(),
            ImageImportResult::Imported(MkImageRef::from_filename("inside.png"))
        );
        assert_eq!(std::fs::read(in_root).unwrap(), before);
    }

    #[test]
    fn dimension_budget_rejects_checked_pixel_and_byte_limits() {
        assert!(validate_image_dimensions(0, 1).is_err());
        assert!(validate_image_dimensions(u32::MAX, u32::MAX).is_err());
        assert!(validate_image_dimensions(4097, 4096).is_err());
        assert_eq!(validated_rgba_len(4096, 4096).unwrap(), 64 * 1024 * 1024);
        assert!(validated_rgba_len(4096, 4097).is_err());
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
            assert!(service.import_png(&source).is_err());
        }
        assert!(store.image_refs().unwrap().is_empty());
    }

    #[test]
    fn collision_choices_replace_save_as_and_cancel_are_explicit() {
        let (dir, store) = fixture();
        let source = dir.path().join("shared.png");
        write_png(&source, [1, 2, 3, 255]);
        let shared = MkImageRef::from_filename("shared.png");
        store
            .write_captured_png(
                &RgbaImage::from_pixel(1, 1, Rgba([9, 9, 9, 255])),
                shared.clone(),
                ImageImportChoice::SaveAs(shared.clone()),
            )
            .unwrap();
        assert!(matches!(
            ImageAssetAuthoringService::new(&store)
                .import_png(&source)
                .unwrap(),
            ImageImportResult::Collision { .. }
        ));
        assert!(matches!(
            ImageAssetAuthoringService::new(&store)
                .import_png_with_choice(&source, ImageImportChoice::Cancel)
                .unwrap(),
            ImageImportResult::Cancelled
        ));
        assert!(matches!(
            ImageAssetAuthoringService::new(&store)
                .import_png_with_choice(&source, ImageImportChoice::SaveAs(MkImageRef::from_filename("other.png")))
                .unwrap(),
            ImageImportResult::Imported(ref image) if image.filename() == "other.png"
        ));
        ImageAssetAuthoringService::new(&store)
            .import_png_with_choice(&source, ImageImportChoice::ReplaceExisting)
            .unwrap();
        assert_eq!(
            store.validate_image_ref(&shared).unwrap().get_pixel(0, 0).0,
            [1, 2, 3, 255]
        );
    }

    #[test]
    fn captured_png_is_written_only_after_a_filename_and_round_trips() {
        let (_dir, store) = fixture();
        let service = ImageAssetAuthoringService::new(&store);
        let image = RgbaImage::from_fn(3, 2, |x, y| Rgba([x as u8, y as u8, 77, 128]));
        assert_eq!(
            service
                .stage_rgba(
                    &image,
                    MkImageRef::from_filename("capture.png"),
                    ImageImportChoice::Cancel
                )
                .unwrap(),
            ImageImportResult::Cancelled
        );
        let result = service
            .stage_rgba(
                &image,
                MkImageRef::from_filename("capture.png"),
                ImageImportChoice::SaveAs(MkImageRef::from_filename("capture.png")),
            )
            .unwrap();
        assert_eq!(
            result,
            ImageImportResult::Imported(MkImageRef::from_filename("capture.png"))
        );
        assert_eq!(
            store
                .validate_image_ref(&MkImageRef::from_filename("capture.png"))
                .unwrap()
                .dimensions(),
            (3, 2)
        );
    }
}
