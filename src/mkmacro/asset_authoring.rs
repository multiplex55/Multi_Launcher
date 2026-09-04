//! Transactional staging of reference images used while authoring macros.
//!
//! Staging and document replacement are deliberately separate.  In particular,
//! callers never own the files they reference. Library deletion is an explicit
//! store-level operation outside action editing.

use super::{ImageImportChoice, ImageImportResult, MkImageRef, MkMacroStore};
pub use crate::image_crop::{ImageCropRect, crop_rgba, validate_crop_rect};
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

    fn distinct_rgba_fixture() -> RgbaImage {
        RgbaImage::from_fn(4, 4, |x, y| {
            Rgba([
                (x + y * 4) as u8,
                (x * 17 + y) as u8,
                (200 - x * 4 - y) as u8,
                (31 + x + y * 4) as u8,
            ])
        })
    }

    fn fixture_with_original() -> (tempfile::TempDir, MkMacroStore, MkImageRef, RgbaImage) {
        let (dir, store) = fixture();
        let original_ref = MkImageRef::from_filename("original.png");
        let original = distinct_rgba_fixture();
        let path = store.image_path(&original_ref).unwrap();
        std::fs::create_dir_all(store.asset_root()).unwrap();
        DynamicImage::ImageRgba8(original.clone())
            .save_with_format(path, ImageFormat::Png)
            .unwrap();
        (dir, store, original_ref, original)
    }

    fn assert_pixels_equal(expected: &RgbaImage, actual: &RgbaImage) {
        assert_eq!(actual.dimensions(), expected.dimensions());
        for (x, y, pixel) in expected.enumerate_pixels() {
            assert_eq!(
                actual.get_pixel(x, y),
                pixel,
                "pixel mismatch at ({x}, {y})"
            );
        }
    }

    #[test]
    fn save_as_crop_persists_exact_pixels_and_rejects_collision_without_mutation() {
        let (_dir, store, original_ref, expected_original) = fixture_with_original();
        let original = store.validate_image_ref(&original_ref).unwrap();
        let cropped_ref = MkImageRef::from_filename("original_cropped.png");
        let crop = crop_rgba(
            &original,
            ImageCropRect {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            },
        )
        .unwrap();
        let service = ImageAssetAuthoringService::new(&store);

        assert_eq!(
            service
                .stage_rgba(
                    &crop,
                    cropped_ref.clone(),
                    ImageImportChoice::SaveAs(cropped_ref.clone()),
                )
                .unwrap(),
            ImageImportResult::Imported(cropped_ref.clone())
        );

        let original_path = store.image_path(&original_ref).unwrap();
        let cropped_path = store.image_path(&cropped_ref).unwrap();
        assert!(original_path.is_file());
        assert!(cropped_path.is_file());
        assert_eq!(original_path.parent(), Some(store.asset_root().as_path()));
        assert_eq!(cropped_path.parent(), Some(store.asset_root().as_path()));
        assert_eq!(
            store.image_refs().unwrap(),
            vec![original_ref.clone(), cropped_ref.clone()]
        );

        let decoded_original = store.validate_image_ref(&original_ref).unwrap();
        let decoded_crop = store.validate_image_ref(&cropped_ref).unwrap();
        assert_pixels_equal(&expected_original, &decoded_original);
        assert_pixels_equal(&crop, &decoded_crop);
        assert_eq!(decoded_crop.dimensions(), (2, 2));
        assert_eq!(decoded_crop.get_pixel(0, 0).0, [5, 18, 195, 36]);
        assert_eq!(decoded_crop.get_pixel(1, 0).0, [6, 35, 191, 37]);
        assert_eq!(decoded_crop.get_pixel(0, 1).0, [9, 19, 194, 40]);
        assert_eq!(decoded_crop.get_pixel(1, 1).0, [10, 36, 190, 41]);

        let different = RgbaImage::from_pixel(2, 2, Rgba([250, 1, 2, 3]));
        assert_eq!(
            service
                .stage_rgba(
                    &different,
                    cropped_ref.clone(),
                    ImageImportChoice::SaveAs(cropped_ref.clone()),
                )
                .unwrap(),
            ImageImportResult::Collision {
                image: cropped_ref.clone()
            }
        );
        assert_pixels_equal(&crop, &store.validate_image_ref(&cropped_ref).unwrap());
    }

    #[test]
    fn overwrite_crop_keeps_original_reference_and_replaces_valid_managed_png() {
        let (_dir, store, original_ref, expected_original) = fixture_with_original();
        let original = store.validate_image_ref(&original_ref).unwrap();
        assert_pixels_equal(&expected_original, &original);
        let crop = crop_rgba(
            &original,
            ImageCropRect {
                x: 0,
                y: 2,
                width: 3,
                height: 2,
            },
        )
        .unwrap();

        let result = ImageAssetAuthoringService::new(&store)
            .stage_rgba(
                &crop,
                original_ref.clone(),
                ImageImportChoice::ReplaceExisting,
            )
            .unwrap();
        assert_eq!(result, ImageImportResult::Imported(original_ref.clone()));

        let decoded = store.validate_image_ref(&original_ref).unwrap();
        assert_pixels_equal(&crop, &decoded);
        assert_eq!(decoded.dimensions(), (3, 2));
        assert!(store.image_path(&original_ref).unwrap().is_file());
        assert_eq!(store.image_refs().unwrap(), vec![original_ref]);
    }

    #[test]
    fn crop_preserves_exact_pixels_dimensions_alpha_and_source() {
        let source = distinct_rgba_fixture();
        let before = source.clone();
        let cropped = crop_rgba(
            &source,
            ImageCropRect {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            },
        )
        .unwrap();
        assert_eq!(cropped.dimensions(), (2, 2));
        assert_eq!(cropped.get_pixel(0, 0).0, [5, 18, 195, 36]);
        assert_eq!(cropped.get_pixel(1, 0).0, [6, 35, 191, 37]);
        assert_eq!(cropped.get_pixel(0, 1).0, [9, 19, 194, 40]);
        assert_eq!(cropped.get_pixel(1, 1).0, [10, 36, 190, 41]);
        assert_eq!(source, before);
    }

    #[test]
    fn crop_accepts_full_image_one_pixel_and_both_boundaries() {
        let source = distinct_rgba_fixture();
        assert_eq!(
            crop_rgba(
                &source,
                ImageCropRect {
                    x: 0,
                    y: 0,
                    width: 4,
                    height: 4,
                }
            )
            .unwrap(),
            source
        );
        assert_eq!(
            crop_rgba(
                &source,
                ImageCropRect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                }
            )
            .unwrap()
            .get_pixel(0, 0)
            .0,
            [0, 0, 200, 31]
        );
        assert_eq!(
            crop_rgba(
                &source,
                ImageCropRect {
                    x: 3,
                    y: 3,
                    width: 1,
                    height: 1,
                }
            )
            .unwrap()
            .get_pixel(0, 0)
            .0,
            [15, 54, 185, 46]
        );
    }

    #[test]
    fn crop_rejects_invalid_rectangles_and_checked_add_overflow() {
        let source = distinct_rgba_fixture();
        for rect in [
            ImageCropRect {
                x: 0,
                y: 0,
                width: 0,
                height: 1,
            },
            ImageCropRect {
                x: 0,
                y: 0,
                width: 1,
                height: 0,
            },
            ImageCropRect {
                x: 4,
                y: 0,
                width: 1,
                height: 1,
            },
            ImageCropRect {
                x: 0,
                y: 4,
                width: 1,
                height: 1,
            },
            ImageCropRect {
                x: 3,
                y: 0,
                width: 2,
                height: 1,
            },
            ImageCropRect {
                x: 0,
                y: 3,
                width: 1,
                height: 2,
            },
            ImageCropRect {
                x: u32::MAX,
                y: 0,
                width: 1,
                height: 1,
            },
            ImageCropRect {
                x: 0,
                y: u32::MAX,
                width: 1,
                height: 1,
            },
            ImageCropRect {
                x: 1,
                y: 1,
                width: u32::MAX,
                height: 1,
            },
            ImageCropRect {
                x: 1,
                y: 1,
                width: 1,
                height: u32::MAX,
            },
        ] {
            assert!(
                crop_rgba(&source, rect).is_err(),
                "unexpectedly accepted {rect:?}"
            );
        }
    }
}
