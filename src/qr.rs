//! Local QR code generation for arbitrary UTF-8 text.
//!
//! This module owns the application-facing QR types and keeps the encoder's
//! dependency types behind the generation boundary.

use std::fmt;

use image::{Rgba, RgbaImage};
use qrcode::{Color, EcLevel, QrCode};

const QUIET_ZONE_MODULES: usize = 4;
const RASTER_MODULE_SCALE: u32 = 8;

/// Payload counts that can be displayed even when QR generation fails.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QrPayloadMetadata {
    /// Unicode scalar values in the source text.
    pub character_count: usize,
    /// Number of UTF-8 bytes in the source text.
    pub utf8_byte_count: usize,
}

impl QrPayloadMetadata {
    /// Measures a source without attempting to encode it.
    pub fn from_source(source: &str) -> Self {
        Self {
            character_count: source.chars().count(),
            utf8_byte_count: source.len(),
        }
    }
}

/// The QR error-correction level used during generation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum QrErrorCorrection {
    Low,
    #[default]
    Medium,
    Quartile,
    High,
}

impl QrErrorCorrection {
    /// Returns a compact label suitable for controls such as error-correction selectors.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Low => "Low (L)",
            Self::Medium => "Medium (M)",
            Self::Quartile => "Quartile (Q)",
            Self::High => "High (H)",
        }
    }

    fn encoder_level(self) -> EcLevel {
        match self {
            QrErrorCorrection::Low => EcLevel::L,
            QrErrorCorrection::Medium => EcLevel::M,
            QrErrorCorrection::Quartile => EcLevel::Q,
            QrErrorCorrection::High => EcLevel::H,
        }
    }
}

/// A generated square QR module matrix.
///
/// Modules are stored in row-major order. The source payload is deliberately
/// not retained in this value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedQr {
    size: usize,
    modules: Vec<bool>,
    error_correction: QrErrorCorrection,
}

impl GeneratedQr {
    /// Returns the matrix width and height in modules.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Returns the selected error-correction level.
    pub fn error_correction(&self) -> QrErrorCorrection {
        self.error_correction
    }

    /// Returns whether the module at `(x, y)` is dark, or `None` when outside
    /// the matrix.
    pub fn is_dark(&self, x: usize, y: usize) -> Option<bool> {
        if x >= self.size || y >= self.size {
            return None;
        }

        Some(self.modules[y * self.size + x])
    }

    /// Renders the module matrix as a deterministic black-and-white raster
    /// with a four-module white quiet zone and eight pixels per module.
    pub fn to_rgba_image(&self) -> RgbaImage {
        let side = ((self.size + QUIET_ZONE_MODULES * 2) as u32) * RASTER_MODULE_SCALE;
        let matrix_start = QUIET_ZONE_MODULES;
        let matrix_end = matrix_start + self.size;

        RgbaImage::from_fn(side, side, |x, y| {
            let module_x = (x / RASTER_MODULE_SCALE) as usize;
            let module_y = (y / RASTER_MODULE_SCALE) as usize;
            let is_dark = module_x >= matrix_start
                && module_x < matrix_end
                && module_y >= matrix_start
                && module_y < matrix_end
                && self.modules[(module_y - matrix_start) * self.size + (module_x - matrix_start)];

            let value = if is_dark { 0 } else { u8::MAX };
            Rgba([value, value, value, u8::MAX])
        })
    }
}

/// A failure to generate a QR code.
///
/// Variants carry no encoder error or payload data, so displaying or debugging
/// this value cannot expose the user's source text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QrGenerationError {
    EmptyPayload,
    CapacityExceeded,
    EncodingFailed,
}

impl fmt::Display for QrGenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyPayload => "QR content is empty",
            Self::CapacityExceeded => {
                "Text exceeds QR code capacity for the selected error-correction level"
            }
            Self::EncodingFailed => "Unable to encode QR code",
        };

        formatter.write_str(message)
    }
}

impl std::error::Error for QrGenerationError {}

/// Encodes the exact UTF-8 bytes of `source` into the smallest QR version that
/// can contain it at the selected error-correction level.
pub fn generate(
    source: &str,
    error_correction: QrErrorCorrection,
) -> Result<GeneratedQr, QrGenerationError> {
    if source.is_empty() {
        return Err(QrGenerationError::EmptyPayload);
    }

    let encoder_level = error_correction.encoder_level();
    let code = if source.is_ascii() {
        QrCode::with_error_correction_level(source.as_bytes(), encoder_level).map_err(|error| {
            match error {
                qrcode::types::QrError::DataTooLong => QrGenerationError::CapacityExceeded,
                _ => QrGenerationError::EncodingFailed,
            }
        })?
    } else {
        encode_with_utf8_eci(source, encoder_level)?
    };

    let size = code.width();
    let mut modules = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            modules.push(code[(x, y)] == Color::Dark);
        }
    }

    Ok(GeneratedQr {
        size,
        modules,
        error_correction,
    })
}

fn encode_with_utf8_eci(
    source: &str,
    error_correction: EcLevel,
) -> Result<QrCode, QrGenerationError> {
    for version in 1_i16..=40 {
        let mut bits = qrcode::bits::Bits::new(qrcode::types::Version::Normal(version));
        let bits_result = bits
            .push_eci_designator(26)
            .and_then(|()| bits.push_byte_data(source.as_bytes()))
            .and_then(|()| bits.push_terminator(error_correction));

        match bits_result {
            Ok(()) => {}
            Err(qrcode::types::QrError::DataTooLong) => continue,
            Err(_) => return Err(QrGenerationError::EncodingFailed),
        }

        match QrCode::with_bits(bits, error_correction) {
            Ok(code) => return Ok(code),
            Err(qrcode::types::QrError::DataTooLong) => continue,
            Err(_) => return Err(QrGenerationError::EncodingFailed),
        }
    }

    Err(QrGenerationError::CapacityExceeded)
}

#[cfg(test)]
mod tests {
    use super::{
        GeneratedQr, QUIET_ZONE_MODULES, QrErrorCorrection, QrGenerationError, QrPayloadMetadata,
        RASTER_MODULE_SCALE, generate,
    };
    use image::Rgba;
    use qrcode::{Color, EcLevel, QrCode, bits::Bits, types::Version};

    fn library_code(source: &str, error_correction: QrErrorCorrection) -> QrCode {
        let level = match error_correction {
            QrErrorCorrection::Low => EcLevel::L,
            QrErrorCorrection::Medium => EcLevel::M,
            QrErrorCorrection::Quartile => EcLevel::Q,
            QrErrorCorrection::High => EcLevel::H,
        };

        if source.is_ascii() {
            return QrCode::with_error_correction_level(source.as_bytes(), level)
                .expect("test payload should fit");
        }

        for version in 1_i16..=40 {
            let mut bits = Bits::new(Version::Normal(version));
            match bits
                .push_eci_designator(26)
                .and_then(|()| bits.push_byte_data(source.as_bytes()))
                .and_then(|()| bits.push_terminator(level))
            {
                Ok(()) => {}
                Err(qrcode::types::QrError::DataTooLong) => continue,
                Err(error) => panic!("unexpected test encoder error: {error}"),
            }

            match QrCode::with_bits(bits, level) {
                Ok(code) => return code,
                Err(qrcode::types::QrError::DataTooLong) => continue,
                Err(error) => panic!("unexpected test encoder error: {error}"),
            }
        }

        panic!("test payload should fit");
    }

    fn assert_matches_library(
        generated: &GeneratedQr,
        source: &str,
        error_correction: QrErrorCorrection,
    ) {
        let expected = library_code(source, error_correction);

        assert_eq!(generated.size(), expected.width());
        assert_eq!(generated.error_correction(), error_correction);
        for y in 0..expected.width() {
            for x in 0..expected.width() {
                assert_eq!(
                    generated.is_dark(x, y),
                    Some(expected[(x, y)] == Color::Dark),
                    "module differs at ({x}, {y})"
                );
            }
        }
        assert_eq!(generated.is_dark(expected.width(), 0), None);
        assert_eq!(generated.is_dark(0, expected.width()), None);
    }

    #[test]
    fn generates_ascii_url_multiline_and_unicode_payloads() {
        for source in [
            "Hello, QR!",
            "https://example.com/path?q=hello%20world",
            "first line\nsecond line\nthird line",
            "こんにちは — café — 🦀",
        ] {
            let generated = generate(source, QrErrorCorrection::Medium).unwrap();
            assert!(generated.size() > 0);
            assert_matches_library(&generated, source, QrErrorCorrection::Medium);
        }
    }

    #[test]
    fn encodes_exact_whitespace_and_utf8_bytes() {
        let source = " \t hello\r\n世界  ";
        let original_bytes = source.as_bytes().to_vec();
        let generated = generate(source, QrErrorCorrection::Medium).unwrap();

        assert_eq!(source.as_bytes(), original_bytes);
        assert_matches_library(&generated, source, QrErrorCorrection::Medium);
        assert_ne!(
            generated,
            generate(source.trim(), QrErrorCorrection::Medium).unwrap()
        );
    }

    #[test]
    fn supports_every_error_correction_level() {
        for error_correction in [
            QrErrorCorrection::Low,
            QrErrorCorrection::Medium,
            QrErrorCorrection::Quartile,
            QrErrorCorrection::High,
        ] {
            let generated = generate("all levels", error_correction).unwrap();
            assert_matches_library(&generated, "all levels", error_correction);
        }
    }

    #[test]
    fn default_error_correction_is_medium() {
        assert_eq!(QrErrorCorrection::default(), QrErrorCorrection::Medium);
    }

    #[test]
    fn rejects_empty_payload_without_encoding() {
        assert_eq!(
            generate("", QrErrorCorrection::Medium),
            Err(QrGenerationError::EmptyPayload)
        );
    }

    #[test]
    fn oversized_payload_returns_capacity_error() {
        let source = "x".repeat(10_000);
        assert_eq!(
            generate(&source, QrErrorCorrection::Low),
            Err(QrGenerationError::CapacityExceeded)
        );
    }

    #[test]
    fn errors_do_not_include_payload_contents() {
        let source = "private payload ".repeat(1_000);
        let error = generate(&source, QrErrorCorrection::Low).unwrap_err();
        let rendered = format!("{error:?} {error}");

        assert!(matches!(error, QrGenerationError::CapacityExceeded));
        assert!(!rendered.contains("private payload"));
    }

    #[test]
    fn raster_is_square_with_quiet_zone_and_integer_module_scale() {
        let generated = generate("raster dimensions", QrErrorCorrection::Medium).unwrap();
        let raster = generated.to_rgba_image();
        let expected_side =
            ((generated.size() + QUIET_ZONE_MODULES * 2) as u32) * RASTER_MODULE_SCALE;

        assert_eq!(raster.width(), raster.height());
        assert_eq!(raster.width(), expected_side);

        let quiet_zone_pixels = QUIET_ZONE_MODULES as u32 * RASTER_MODULE_SCALE;
        for y in 0..raster.height() {
            for x in 0..raster.width() {
                if x < quiet_zone_pixels
                    || y < quiet_zone_pixels
                    || x >= raster.width() - quiet_zone_pixels
                    || y >= raster.height() - quiet_zone_pixels
                {
                    assert_eq!(*raster.get_pixel(x, y), Rgba([255, 255, 255, 255]));
                }
            }
        }
    }

    #[test]
    fn raster_contains_only_opaque_black_and_white_pixels() {
        let raster = generate("black and white", QrErrorCorrection::Medium)
            .unwrap()
            .to_rgba_image();

        for pixel in raster.pixels() {
            assert!(
                *pixel == Rgba([0, 0, 0, 255]) || *pixel == Rgba([255, 255, 255, 255]),
                "unexpected QR raster pixel: {pixel:?}"
            );
        }
    }

    #[test]
    fn raster_draws_every_module_as_a_uniform_integer_block() {
        let generated = generate("crisp modules", QrErrorCorrection::Medium).unwrap();
        let raster = generated.to_rgba_image();
        let quiet_zone_pixels = QUIET_ZONE_MODULES as u32 * RASTER_MODULE_SCALE;

        for module_y in 0..generated.size() {
            for module_x in 0..generated.size() {
                let expected_pixel = if generated.is_dark(module_x, module_y) == Some(true) {
                    Rgba([0, 0, 0, 255])
                } else {
                    Rgba([255, 255, 255, 255])
                };
                let pixel_x = quiet_zone_pixels + module_x as u32 * RASTER_MODULE_SCALE;
                let pixel_y = quiet_zone_pixels + module_y as u32 * RASTER_MODULE_SCALE;

                for y in pixel_y..pixel_y + RASTER_MODULE_SCALE {
                    for x in pixel_x..pixel_x + RASTER_MODULE_SCALE {
                        assert_eq!(*raster.get_pixel(x, y), expected_pixel);
                    }
                }
            }
        }
    }

    #[test]
    fn raster_is_deterministic_for_the_same_payload_and_correction() {
        let first = generate("same output", QrErrorCorrection::Quartile)
            .unwrap()
            .to_rgba_image();
        let second = generate("same output", QrErrorCorrection::Quartile)
            .unwrap()
            .to_rgba_image();

        assert_eq!(first, second);
    }

    #[test]
    fn error_correction_can_change_matrix_dimensions() {
        let source = "12345678901234567890";
        let medium = generate(source, QrErrorCorrection::Medium).unwrap();
        let high = generate(source, QrErrorCorrection::High).unwrap();

        assert_ne!(medium.size(), high.size());
    }

    #[test]
    fn payload_metadata_is_available_when_encoding_exceeds_capacity() {
        let source = "🦀".repeat(3_000);
        let metadata = QrPayloadMetadata::from_source(&source);

        assert_eq!(metadata.character_count, source.chars().count());
        assert_eq!(metadata.character_count, 3_000);
        assert_eq!(metadata.utf8_byte_count, source.len());
        assert_eq!(metadata.utf8_byte_count, 12_000);
        assert_eq!(
            generate(&source, QrErrorCorrection::Low),
            Err(QrGenerationError::CapacityExceeded)
        );
    }
}
