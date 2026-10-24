//! Bounded, session-only exact image comparison.
//!
//! This module deliberately does not share image types with the screenshot or
//! text-diff features. Inputs are immutable RGBA snapshots; loading and
//! comparison happen on one lazily-created worker per session.

mod budget;
mod clipboard;
mod compare;
mod input;
mod session;

pub(crate) use budget::{
    BudgetExceeded, ImageBudgetLease, ImageBudgetSnapshot, ImageCompareBudget,
};
pub(crate) use compare::{DifferenceClass, ImageComparison};
pub(crate) use input::{
    ImageInput, ImageInputId, ImageInputSource, ImageSide, MAX_IMAGE_PIXELS,
    SUPPORTED_IMAGE_FILE_EXTENSIONS,
};
pub(crate) use session::{
    ImageCompareSession, ImageCompareSnapshot, ImageLoadStatus, ImageSessionError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ImageCompareError {
    InvalidDimensions,
    InvalidPixelLength { expected: usize, actual: usize },
    PixelLimit { pixels: usize, limit: usize },
    EncodedInputTooLarge { bytes: u64, limit: u64 },
    Budget(BudgetExceeded),
    Unsupported(String),
    Decode(String),
    SourceChanged,
    Read(String),
    Cancelled,
    IdentityExhausted,
    ComparisonCanvasTooLarge,
}

impl std::fmt::Display for ImageCompareError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDimensions => write!(formatter, "image dimensions must be nonzero"),
            Self::InvalidPixelLength { expected, actual } => write!(
                formatter,
                "image pixel buffer has {actual} bytes; exactly {expected} are required"
            ),
            Self::PixelLimit { pixels, limit } => write!(
                formatter,
                "image has {pixels} pixels; the per-image limit is {limit}"
            ),
            Self::EncodedInputTooLarge { bytes, limit } => write!(
                formatter,
                "encoded image is {bytes} bytes; the input limit is {limit} bytes"
            ),
            Self::Budget(error) => error.fmt(formatter),
            Self::Unsupported(message) | Self::Decode(message) | Self::Read(message) => {
                formatter.write_str(message)
            }
            Self::SourceChanged => {
                write!(formatter, "image source changed while it was being read")
            }
            Self::Cancelled => write!(formatter, "image loading was cancelled"),
            Self::IdentityExhausted => write!(formatter, "image input identity space is exhausted"),
            Self::ComparisonCanvasTooLarge => write!(
                formatter,
                "combined image canvas exceeds the comparison pixel limit"
            ),
        }
    }
}

impl std::error::Error for ImageCompareError {}

impl From<BudgetExceeded> for ImageCompareError {
    fn from(error: BudgetExceeded) -> Self {
        Self::Budget(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    fn decoded(bytes: &[u8], source: &str) -> ImageInput {
        input::decode_encoded_for_test(bytes, source).unwrap()
    }

    #[test]
    fn image_compare_domain_equal_rgba_is_independent_of_encoded_file_format() {
        let pixels = RgbaImage::from_fn(3, 2, |x, y| {
            Rgba([x as u8 * 37, y as u8 * 83, (x + y) as u8 * 19, 255])
        });
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(pixels.clone())
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let mut bmp = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(pixels)
            .write_to(&mut bmp, ImageFormat::Bmp)
            .unwrap();
        assert_ne!(png.get_ref(), bmp.get_ref());

        let a = decoded(png.get_ref(), "one.png");
        let b = decoded(bmp.get_ref(), "two.bmp");
        let comparison =
            compare::compare_inputs(&a, &b, &ImageCompareBudget::default(), || false).unwrap();
        assert_eq!(comparison.changed_pixels(), 0);
        assert!(
            comparison
                .classes()
                .iter()
                .all(|class| *class == DifferenceClass::Equal)
        );
        assert_eq!(comparison.input_ids(), (a.id(), b.id()));
    }

    #[test]
    fn image_compare_domain_decodes_each_advertised_container_with_preflight_dimensions() {
        let source = image::RgbImage::from_fn(3, 2, |x, y| {
            image::Rgb([x as u8 * 70, y as u8 * 110, (x + y) as u8 * 35])
        });
        for format in [ImageFormat::Jpeg, ImageFormat::Gif] {
            let mut encoded = Cursor::new(Vec::new());
            DynamicImage::ImageRgb8(source.clone())
                .write_to(&mut encoded, format)
                .unwrap();
            let image = decoded(encoded.get_ref(), "bounded container");
            assert_eq!(image.dimensions(), (3, 2), "{format:?}");
            assert_eq!(image.pixels().len(), 3 * 2 * 4, "{format:?}");
        }
    }

    #[test]
    fn image_compare_domain_compares_every_raw_rgba_channel_including_invisible_rgb() {
        let budget = ImageCompareBudget::default();
        let a = input::from_raw_for_test(
            5,
            1,
            vec![
                10, 20, 30, 40, // baseline
                0, 0, 0, 0, // invisible baseline
                5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
            ],
            "A",
        )
        .unwrap();
        let b = input::from_raw_for_test(
            5,
            1,
            vec![
                10, 21, 30, 40, // green only
                1, 0, 0, 0, // RGB differs while alpha remains zero
                5, 6, 7, 9, // alpha only
                9, 10, 12, 12, // blue only
                13, 14, 15, 16,
            ],
            "B",
        )
        .unwrap();
        let result = compare::compare_inputs(&a, &b, &budget, || false).unwrap();
        assert_eq!(result.changed_pixels(), 4);
        assert_eq!(result.classes()[0], DifferenceClass::Changed);
        assert_eq!(result.classes()[1], DifferenceClass::Changed);
        assert_eq!(result.classes()[2], DifferenceClass::Changed);
        assert_eq!(result.classes()[3], DifferenceClass::Changed);
        assert_eq!(result.classes()[4], DifferenceClass::Equal);
    }

    #[test]
    fn image_compare_domain_top_left_masks_crossed_extents_with_neither_regions() {
        let a = input::from_raw_for_test(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8], "A").unwrap();
        let b = input::from_raw_for_test(1, 2, vec![1, 2, 3, 4, 9, 9, 9, 9], "B").unwrap();
        let result =
            compare::compare_inputs(&a, &b, &ImageCompareBudget::default(), || false).unwrap();
        assert_eq!(result.dimensions(), (2, 2));
        assert_eq!(
            result.classes(),
            &[
                DifferenceClass::Equal,
                DifferenceClass::AOnly,
                DifferenceClass::BOnly,
                DifferenceClass::Neither,
            ]
        );
    }

    #[test]
    fn image_compare_domain_checked_dimensions_and_exact_byte_lengths_fail_closed() {
        assert!(input::checked_rgba_len(0, 1).is_err());
        assert!(input::checked_rgba_len(u32::MAX, u32::MAX).is_err());
        assert!(input::checked_rgba_len(MAX_IMAGE_PIXELS as u32 + 1, 1).is_err());
        assert_eq!(
            input::checked_rgba_len(4096, 4096).unwrap(),
            64 * 1024 * 1024
        );
        assert!(input::from_raw_for_test(2, 1, vec![0; 7], "bad length").is_err());
        assert!(input::decode_encoded_for_test(b"not an image", "bad").is_err());
    }

    #[test]
    fn image_compare_domain_bounded_decoder_preflights_dimensions_and_refuses_unbounded_formats() {
        assert_eq!(
            SUPPORTED_IMAGE_FILE_EXTENSIONS,
            &["png", "jpg", "jpeg", "bmp", "gif"]
        );
        let mut png_header = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png_header.extend_from_slice(&1_u32.to_be_bytes());
        png_header.extend_from_slice(&1_u32.to_be_bytes());
        assert_eq!(
            input::preflight_dimensions(&png_header, ImageFormat::Png).unwrap(),
            (1, 1)
        );
        let too_wide = (MAX_IMAGE_PIXELS as u32 + 1).to_be_bytes();
        png_header[16..20].copy_from_slice(&too_wide);
        assert!(matches!(
            input::preflight_dimensions(&png_header, ImageFormat::Png),
            Err(ImageCompareError::PixelLimit { .. })
        ));
        assert!(matches!(
            input::preflight_dimensions(b"", ImageFormat::Tiff),
            Err(ImageCompareError::Unsupported(_))
        ));
        assert!(matches!(
            input::preflight_dimensions(b"", ImageFormat::Ico),
            Err(ImageCompareError::Unsupported(_))
        ));

        // Insufficient budget is rejected after only bounded header parsing,
        // before an image codec is constructed from this deliberately
        // incomplete PNG payload.
        let mut valid_dimensions_only = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        valid_dimensions_only.extend_from_slice(&1_u32.to_be_bytes());
        valid_dimensions_only.extend_from_slice(&1_u32.to_be_bytes());
        let tiny_budget = ImageCompareBudget::with_limit(128);
        let encoded = tiny_budget.reserve(valid_dimensions_only.len()).unwrap();
        assert!(matches!(
            input::decode_encoded_reserved(
                &valid_dimensions_only,
                &tiny_budget,
                &std::sync::atomic::AtomicBool::new(false),
                &encoded,
                ImageInputSource::Test {
                    display_name: "preflight".into(),
                },
            ),
            Err(ImageCompareError::Budget(_))
        ));
    }

    #[test]
    fn image_compare_domain_clipboard_dib_dimensions_and_storage_are_checked_before_rgba() {
        let dib = test_dib_24(2, 1);
        let decoded = input::decode_dib_for_test(&dib, 8).unwrap();
        assert_eq!(decoded.dimensions(), (2, 1));
        assert_eq!(decoded.pixels(), &[1, 2, 3, 255, 4, 5, 6, 255]);
        assert!(input::decode_dib_for_test(&dib[..39], 8).is_err());
        let mut truncated = dib.clone();
        truncated.truncate(truncated.len() - 1);
        assert!(input::decode_dib_for_test(&truncated, 8).is_err());
        assert!(input::decode_dib_for_test(&dib, 13).is_err());
        let mut huge = dib;
        huge[4..8].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(input::decode_dib_for_test(&huge, 8).is_err());
    }

    fn test_dib_24(width: i32, height: i32) -> Vec<u8> {
        let stride = (width as usize * 3).div_ceil(4) * 4;
        let mut bytes = vec![0u8; 40 + stride * height.unsigned_abs() as usize];
        bytes[0..4].copy_from_slice(&40u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&width.to_le_bytes());
        bytes[8..12].copy_from_slice(&height.to_le_bytes());
        bytes[12..14].copy_from_slice(&1u16.to_le_bytes());
        bytes[14..16].copy_from_slice(&24u16.to_le_bytes());
        // Bottom-up BGR pixels; the six-byte row has two alignment bytes.
        bytes[40..46].copy_from_slice(&[3, 2, 1, 6, 5, 4]);
        bytes
    }
}
