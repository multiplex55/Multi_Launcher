//! Format-independent image crop primitives.
//!
//! Crop rectangles are always expressed in source-image pixel coordinates.
//! This module deliberately knows nothing about files, image encoders, or GUI
//! state so it can be shared by macro authoring and standalone tools.

use anyhow::Result;
use image::RgbaImage;

/// A non-empty rectangle in an image's source-pixel coordinate system.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImageCropRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl ImageCropRect {
    pub fn right(self) -> Result<u32> {
        self.x
            .checked_add(self.width)
            .ok_or_else(|| anyhow::anyhow!("image crop right edge overflows u32"))
    }

    pub fn bottom(self) -> Result<u32> {
        self.y
            .checked_add(self.height)
            .ok_or_else(|| anyhow::anyhow!("image crop bottom edge overflows u32"))
    }
}

/// Validate a source-pixel crop before any pixel operation is attempted.
pub fn validate_crop_rect(
    source_width: u32,
    source_height: u32,
    rect: ImageCropRect,
) -> Result<()> {
    if source_width == 0 || source_height == 0 {
        anyhow::bail!("cannot crop an empty image")
    }
    if rect.width == 0 || rect.height == 0 {
        anyhow::bail!("image crop width and height must be positive")
    }
    let right = rect.right()?;
    let bottom = rect.bottom()?;
    if rect.x >= source_width {
        anyhow::bail!("image crop x coordinate is outside the source image")
    }
    if rect.y >= source_height {
        anyhow::bail!("image crop y coordinate is outside the source image")
    }
    if right > source_width {
        anyhow::bail!("image crop extends beyond the source image width")
    }
    if bottom > source_height {
        anyhow::bail!("image crop extends beyond the source image height")
    }
    Ok(())
}

/// Return an owned crop whose dimensions exactly equal the validated rect.
pub fn crop_rgba(source: &RgbaImage, rect: ImageCropRect) -> Result<RgbaImage> {
    validate_crop_rect(source.width(), source.height(), rect)?;
    Ok(image::imageops::crop_imm(source, rect.x, rect.y, rect.width, rect.height).to_image())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn source() -> RgbaImage {
        RgbaImage::from_fn(4, 3, |x, y| {
            Rgba([x as u8, y as u8, (x + y * 4) as u8, 255])
        })
    }

    #[test]
    fn crop_has_exact_dimensions_and_preserves_source_pixels() {
        let image = source();
        let before = image.clone();
        let cropped = crop_rgba(
            &image,
            ImageCropRect {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            },
        )
        .unwrap();
        assert_eq!(cropped.dimensions(), (2, 2));
        assert_eq!(cropped.get_pixel(0, 0).0, [1, 1, 5, 255]);
        assert_eq!(cropped.get_pixel(1, 1).0, [2, 2, 10, 255]);
        assert_eq!(image, before);
    }

    #[test]
    fn full_and_boundary_selections_are_accepted() {
        let image = source();
        assert_eq!(
            crop_rgba(
                &image,
                ImageCropRect {
                    x: 0,
                    y: 0,
                    width: 4,
                    height: 3,
                }
            )
            .unwrap(),
            image
        );
        assert_eq!(
            crop_rgba(
                &image,
                ImageCropRect {
                    x: 3,
                    y: 2,
                    width: 1,
                    height: 1,
                }
            )
            .unwrap()
            .dimensions(),
            (1, 1)
        );
    }

    #[test]
    fn empty_overflow_and_out_of_bounds_selections_are_rejected() {
        let image = source();
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
                y: 3,
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
                y: 2,
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
                x: 1,
                y: 1,
                width: u32::MAX,
                height: 1,
            },
        ] {
            assert!(crop_rgba(&image, rect).is_err(), "accepted {rect:?}");
        }
        assert!(
            crop_rgba(
                &RgbaImage::new(0, 1),
                ImageCropRect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1
                }
            )
            .is_err()
        );
    }
}
