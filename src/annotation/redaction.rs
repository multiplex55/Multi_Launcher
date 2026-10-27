//! Deterministic, bounded raster redaction operations.
//!
//! Regions use normalized image-space coordinates. After clipping to the image, pixel coverage is
//! half-open: `floor(min)..ceil(max)`. Blur samples clamp to the selected region's edge, and both
//! blur and pixelation average RGBA channels with integer round-to-nearest. These details keep the
//! operation deterministic and prevent neighboring, unselected pixels from affecting an effect.

use std::fmt;

use image::{Rgba, RgbaImage};

const DEFAULT_SCRATCH_LIMIT_BYTES: usize = 32 * 1024 * 1024;

pub const MIN_BLUR_RADIUS: u8 = 1;
pub const MAX_BLUR_RADIUS: u8 = 32;
pub const DEFAULT_BLUR_RADIUS: u8 = 8;
pub const MIN_PIXEL_BLOCK_SIZE: u8 = 2;
pub const MAX_PIXEL_BLOCK_SIZE: u8 = 64;
pub const DEFAULT_PIXEL_BLOCK_SIZE: u8 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedactionMode {
    Solid,
    Blur,
    Pixelate,
}

/// Immutable effect settings captured by one history operation.
///
/// Blur and pixelation are visual obscuring effects, not guarantees that sensitive pixels cannot
/// be inferred. Solid replacement is the mode for removing covered pixels from the output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RedactionEffect {
    mode: RedactionMode,
    color: [u8; 3],
    strength: u8,
}

impl RedactionEffect {
    pub const fn solid_black() -> Self {
        Self::solid([0, 0, 0])
    }

    pub const fn solid(color: [u8; 3]) -> Self {
        Self {
            mode: RedactionMode::Solid,
            color,
            strength: 0,
        }
    }

    pub fn blur(radius: u8) -> Result<Self, RedactionError> {
        if !(MIN_BLUR_RADIUS..=MAX_BLUR_RADIUS).contains(&radius) {
            return Err(RedactionError::InvalidBlurRadius(radius));
        }
        Ok(Self {
            mode: RedactionMode::Blur,
            color: [0, 0, 0],
            strength: radius,
        })
    }

    pub fn pixelate(block_size: u8) -> Result<Self, RedactionError> {
        if !(MIN_PIXEL_BLOCK_SIZE..=MAX_PIXEL_BLOCK_SIZE).contains(&block_size) {
            return Err(RedactionError::InvalidPixelBlockSize(block_size));
        }
        Ok(Self {
            mode: RedactionMode::Pixelate,
            color: [0, 0, 0],
            strength: block_size,
        })
    }

    pub const fn mode(self) -> RedactionMode {
        self.mode
    }

    pub const fn color(self) -> [u8; 3] {
        self.color
    }

    /// Blur radius in image pixels or pixelation block size in image pixels.
    pub const fn strength(self) -> u8 {
        self.strength
    }
}

impl Default for RedactionEffect {
    fn default() -> Self {
        Self::solid_black()
    }
}

/// A finite, non-empty normalized rectangle in source-image pixel coordinates, before canvas zoom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RedactionBounds {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

impl RedactionBounds {
    pub fn from_corners(
        first_x: f32,
        first_y: f32,
        second_x: f32,
        second_y: f32,
    ) -> Result<Self, RedactionError> {
        if ![first_x, first_y, second_x, second_y]
            .into_iter()
            .all(|coordinate| coordinate.is_finite())
        {
            return Err(RedactionError::InvalidBounds);
        }
        let (min_x, max_x) = if first_x <= second_x {
            (first_x, second_x)
        } else {
            (second_x, first_x)
        };
        let (min_y, max_y) = if first_y <= second_y {
            (first_y, second_y)
        } else {
            (second_y, first_y)
        };
        if min_x == max_x || min_y == max_y {
            return Err(RedactionError::EmptyBounds);
        }
        Ok(Self {
            min_x,
            min_y,
            max_x,
            max_y,
        })
    }

    pub const fn min(self) -> [f32; 2] {
        [self.min_x, self.min_y]
    }

    pub const fn max(self) -> [f32; 2] {
        [self.max_x, self.max_y]
    }
}

/// Compact history value: a rectangle and captured effect parameters only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RedactionOperation {
    bounds: RedactionBounds,
    effect: RedactionEffect,
}

impl RedactionOperation {
    pub const fn new(bounds: RedactionBounds, effect: RedactionEffect) -> Self {
        Self { bounds, effect }
    }

    pub const fn bounds(self) -> RedactionBounds {
        self.bounds
    }

    pub const fn effect(self) -> RedactionEffect {
        self.effect
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RedactionError {
    InvalidBounds,
    EmptyBounds,
    InvalidBlurRadius(u8),
    InvalidPixelBlockSize(u8),
    DimensionOverflow,
    ScratchLimitExceeded { required: usize, limit: usize },
    ScratchAllocationFailed,
}

impl fmt::Display for RedactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBounds => formatter.write_str("redaction bounds must be finite"),
            Self::EmptyBounds => formatter.write_str("redaction bounds must have positive area"),
            Self::InvalidBlurRadius(radius) => write!(
                formatter,
                "blur radius {radius} is outside {MIN_BLUR_RADIUS}..={MAX_BLUR_RADIUS} pixels"
            ),
            Self::InvalidPixelBlockSize(size) => write!(
                formatter,
                "pixelation block size {size} is outside {MIN_PIXEL_BLOCK_SIZE}..={MAX_PIXEL_BLOCK_SIZE} pixels"
            ),
            Self::DimensionOverflow => formatter.write_str("redaction dimensions overflow"),
            Self::ScratchLimitExceeded { required, limit } => write!(
                formatter,
                "redaction needs {required} bytes of scratch, exceeding the {limit}-byte limit"
            ),
            Self::ScratchAllocationFailed => {
                formatter.write_str("could not reserve redaction scratch memory")
            }
        }
    }
}

impl std::error::Error for RedactionError {}

#[derive(Clone, Copy, Debug)]
struct PixelBounds {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl PixelBounds {
    fn clipped(bounds: RedactionBounds, image: &RgbaImage) -> Option<Self> {
        let image_width = image.width();
        let image_height = image.height();
        if image_width == 0 || image_height == 0 {
            return None;
        }

        let clip = |value: f32, extent: u32| f64::from(value).clamp(0.0, f64::from(extent));
        let left = clip(bounds.min_x, image_width).floor() as u32;
        let top = clip(bounds.min_y, image_height).floor() as u32;
        let right = clip(bounds.max_x, image_width).ceil() as u32;
        let bottom = clip(bounds.max_y, image_height).ceil() as u32;
        if left >= right || top >= bottom {
            return None;
        }
        Some(Self {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }

    fn pixel_count(self) -> Result<usize, RedactionError> {
        usize::try_from(self.width)
            .ok()
            .and_then(|width| {
                usize::try_from(self.height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .ok_or(RedactionError::DimensionOverflow)
    }
}

struct ScratchBuffers {
    limit: usize,
    reserved: usize,
}

impl ScratchBuffers {
    fn new(limit: usize) -> Self {
        Self { limit, reserved: 0 }
    }

    fn pixels(&mut self, count: usize) -> Result<Vec<[u8; 4]>, RedactionError> {
        let requested = count
            .checked_mul(std::mem::size_of::<[u8; 4]>())
            .ok_or(RedactionError::DimensionOverflow)?;
        let projected = self
            .reserved
            .checked_add(requested)
            .ok_or(RedactionError::DimensionOverflow)?;
        if projected > self.limit {
            return Err(RedactionError::ScratchLimitExceeded {
                required: projected,
                limit: self.limit,
            });
        }

        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(count)
            .map_err(|_| RedactionError::ScratchAllocationFailed)?;
        let actual = pixels
            .capacity()
            .checked_mul(std::mem::size_of::<[u8; 4]>())
            .and_then(|bytes| self.reserved.checked_add(bytes))
            .ok_or(RedactionError::DimensionOverflow)?;
        if actual > self.limit {
            return Err(RedactionError::ScratchLimitExceeded {
                required: actual,
                limit: self.limit,
            });
        }
        self.reserved = actual;
        pixels.resize(count, [0; 4]);
        Ok(pixels)
    }
}

/// Apply one operation to the current composed image using the production scratch ceiling.
pub fn apply_redaction(
    image: &mut RgbaImage,
    operation: RedactionOperation,
) -> Result<(), RedactionError> {
    apply_redaction_with_scratch_limit(image, operation, DEFAULT_SCRATCH_LIMIT_BYTES)
}

fn apply_redaction_with_scratch_limit(
    image: &mut RgbaImage,
    operation: RedactionOperation,
    scratch_limit: usize,
) -> Result<(), RedactionError> {
    let Some(region) = PixelBounds::clipped(operation.bounds, image) else {
        return Ok(());
    };
    let effect = operation.effect;
    if effect.mode == RedactionMode::Solid {
        let color = Rgba([effect.color[0], effect.color[1], effect.color[2], 255]);
        for y in region.y..region.y + region.height {
            for x in region.x..region.x + region.width {
                image.put_pixel(x, y, color);
            }
        }
        return Ok(());
    }

    let pixel_count = region.pixel_count()?;
    let mut scratch = ScratchBuffers::new(scratch_limit);
    let mut source = scratch.pixels(pixel_count)?;
    copy_region_to_scratch(image, region, &mut source);
    match effect.mode {
        RedactionMode::Solid => unreachable!("solid redaction returned above"),
        RedactionMode::Pixelate => pixelate_region(image, region, &source, effect.strength),
        RedactionMode::Blur => {
            let mut horizontal = scratch.pixels(pixel_count)?;
            blur_horizontal(
                &source,
                &mut horizontal,
                region.width,
                region.height,
                effect.strength,
            );
            blur_vertical_into_image(image, &horizontal, region, effect.strength);
        }
    }
    Ok(())
}

fn copy_region_to_scratch(image: &RgbaImage, region: PixelBounds, output: &mut [[u8; 4]]) {
    let width = region.width as usize;
    for local_y in 0..region.height as usize {
        for local_x in 0..width {
            let source = image.get_pixel(region.x + local_x as u32, region.y + local_y as u32);
            output[local_y * width + local_x] = source.0;
        }
    }
}

fn pixelate_region(image: &mut RgbaImage, region: PixelBounds, source: &[[u8; 4]], block: u8) {
    let width = region.width as usize;
    let height = region.height as usize;
    let block = usize::from(block);
    for block_y in (0..height).step_by(block) {
        let end_y = block_y.saturating_add(block).min(height);
        for block_x in (0..width).step_by(block) {
            let end_x = block_x.saturating_add(block).min(width);
            let mut sum = [0u64; 4];
            let count = ((end_x - block_x) * (end_y - block_y)) as u64;
            for y in block_y..end_y {
                for x in block_x..end_x {
                    for (channel, value) in source[y * width + x].iter().enumerate() {
                        sum[channel] += u64::from(*value);
                    }
                }
            }
            let average = average(sum, count);
            for y in block_y..end_y {
                for x in block_x..end_x {
                    image.put_pixel(region.x + x as u32, region.y + y as u32, Rgba(average));
                }
            }
        }
    }
}

fn blur_horizontal(
    source: &[[u8; 4]],
    output: &mut [[u8; 4]],
    width: u32,
    height: u32,
    radius: u8,
) {
    let width = width as usize;
    let radius = i64::from(radius);
    let window = (radius as u64 * 2) + 1;
    for y in 0..height as usize {
        let row = y * width;
        let mut sum = [0u64; 4];
        for offset in -radius..=radius {
            let x = offset.clamp(0, width as i64 - 1) as usize;
            for (channel, value) in source[row + x].iter().enumerate() {
                sum[channel] += u64::from(*value);
            }
        }
        for x in 0..width {
            output[row + x] = average(sum, window);
            if x + 1 < width {
                let remove_x = (x as i64 - radius).clamp(0, width as i64 - 1) as usize;
                let add_x = (x as i64 + radius + 1).clamp(0, width as i64 - 1) as usize;
                for (channel, value) in source[row + remove_x].iter().enumerate() {
                    sum[channel] -= u64::from(*value);
                }
                for (channel, value) in source[row + add_x].iter().enumerate() {
                    sum[channel] += u64::from(*value);
                }
            }
        }
    }
}

fn blur_vertical_into_image(
    image: &mut RgbaImage,
    horizontal: &[[u8; 4]],
    region: PixelBounds,
    radius: u8,
) {
    let width = region.width as usize;
    let height = region.height as usize;
    let radius = i64::from(radius);
    let window = (radius as u64 * 2) + 1;
    for x in 0..width {
        let mut sum = [0u64; 4];
        for offset in -radius..=radius {
            let y = offset.clamp(0, height as i64 - 1) as usize;
            for (channel, value) in horizontal[y * width + x].iter().enumerate() {
                sum[channel] += u64::from(*value);
            }
        }
        for y in 0..height {
            image.put_pixel(
                region.x + x as u32,
                region.y + y as u32,
                Rgba(average(sum, window)),
            );
            if y + 1 < height {
                let remove_y = (y as i64 - radius).clamp(0, height as i64 - 1) as usize;
                let add_y = (y as i64 + radius + 1).clamp(0, height as i64 - 1) as usize;
                for (channel, value) in horizontal[remove_y * width + x].iter().enumerate() {
                    sum[channel] -= u64::from(*value);
                }
                for (channel, value) in horizontal[add_y * width + x].iter().enumerate() {
                    sum[channel] += u64::from(*value);
                }
            }
        }
    }
}

fn average(sum: [u64; 4], count: u64) -> [u8; 4] {
    sum.map(|channel| ((channel + count / 2) / count) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(x1: f32, y1: f32, x2: f32, y2: f32) -> RedactionBounds {
        RedactionBounds::from_corners(x1, y1, x2, y2).unwrap()
    }

    fn operation(bounds: RedactionBounds, effect: RedactionEffect) -> RedactionOperation {
        RedactionOperation::new(bounds, effect)
    }

    fn solid_pixels(width: u32, height: u32, color: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(width, height, Rgba(color))
    }

    #[test]
    fn solid_uses_opaque_black_and_clipped_half_open_bounds() {
        let mut image = solid_pixels(4, 3, [18, 37, 56, 91]);
        let op = operation(bounds(3.8, 2.2, -0.3, 0.2), RedactionEffect::solid_black());
        apply_redaction(&mut image, op).unwrap();
        for y in 0..3 {
            for x in 0..4 {
                assert_eq!(image.get_pixel(x, y).0, [0, 0, 0, 255], "pixel ({x},{y})");
            }
        }
    }

    #[test]
    fn solid_covers_fractional_one_pixel_border_and_ignores_outside_region() {
        let mut image = solid_pixels(3, 2, [11, 22, 33, 44]);
        apply_redaction(
            &mut image,
            operation(
                bounds(1.2, 0.1, 1.8, 0.9),
                RedactionEffect::solid([7, 8, 9]),
            ),
        )
        .unwrap();
        assert_eq!(image.get_pixel(1, 0).0, [7, 8, 9, 255]);
        assert_eq!(image.get_pixel(0, 0).0, [11, 22, 33, 44]);

        apply_redaction(
            &mut image,
            operation(
                bounds(2.0, 1.0, 3.0, 2.0),
                RedactionEffect::solid([1, 2, 3]),
            ),
        )
        .unwrap();
        assert_eq!(image.get_pixel(2, 1).0, [1, 2, 3, 255]);

        let before = image.clone();
        apply_redaction(
            &mut image,
            operation(
                bounds(-10.0, -5.0, -2.0, -1.0),
                RedactionEffect::solid([255, 0, 0]),
            ),
        )
        .unwrap();
        assert_eq!(image, before);
    }

    #[test]
    fn bounds_and_strengths_reject_invalid_values() {
        assert_eq!(
            RedactionBounds::from_corners(f32::NAN, 0.0, 1.0, 1.0),
            Err(RedactionError::InvalidBounds)
        );
        assert_eq!(
            RedactionBounds::from_corners(1.0, 1.0, 1.0, 3.0),
            Err(RedactionError::EmptyBounds)
        );
        assert!(RedactionEffect::blur(0).is_err());
        assert!(RedactionEffect::blur(MAX_BLUR_RADIUS + 1).is_err());
        assert!(RedactionEffect::pixelate(0).is_err());
        assert!(RedactionEffect::pixelate(MAX_PIXEL_BLOCK_SIZE + 1).is_err());
        assert_eq!(
            RedactionEffect::default(),
            RedactionEffect::solid([0, 0, 0])
        );
        assert!(RedactionEffect::blur(MIN_BLUR_RADIUS).is_ok());
        assert!(RedactionEffect::blur(MAX_BLUR_RADIUS).is_ok());
        assert!(RedactionEffect::pixelate(MIN_PIXEL_BLOCK_SIZE).is_ok());
        assert!(RedactionEffect::pixelate(MAX_PIXEL_BLOCK_SIZE).is_ok());
        assert_eq!(
            RedactionEffect::blur(DEFAULT_BLUR_RADIUS)
                .unwrap()
                .strength(),
            DEFAULT_BLUR_RADIUS
        );
        assert_eq!(
            RedactionEffect::pixelate(DEFAULT_PIXEL_BLOCK_SIZE)
                .unwrap()
                .strength(),
            DEFAULT_PIXEL_BLOCK_SIZE
        );
        assert!(matches!(
            ScratchBuffers::new(usize::MAX).pixels(usize::MAX),
            Err(RedactionError::DimensionOverflow)
        ));
    }

    #[test]
    fn blur_is_deterministic_clamped_to_region_and_preserves_constant_color() {
        let mut image = RgbaImage::from_fn(3, 1, |x, _| {
            Rgba([[0, 0, 0, 0], [0, 0, 0, 0], [90, 30, 12, 255]][x as usize])
        });
        apply_redaction(
            &mut image,
            operation(
                bounds(0.0, 0.0, 3.0, 1.0),
                RedactionEffect::blur(1).unwrap(),
            ),
        )
        .unwrap();
        assert_eq!(image.get_pixel(0, 0).0, [0, 0, 0, 0]);
        assert_eq!(image.get_pixel(1, 0).0, [30, 10, 4, 85]);
        assert_eq!(image.get_pixel(2, 0).0, [60, 20, 8, 170]);

        let color = [63, 91, 127, 201];
        let outside = [7, 12, 18, 255];
        let mut constant = RgbaImage::from_fn(4, 3, |x, y| {
            Rgba(if (1..3).contains(&x) && (1..3).contains(&y) {
                color
            } else {
                outside
            })
        });
        apply_redaction(
            &mut constant,
            operation(
                bounds(1.0, 1.0, 3.0, 3.0),
                RedactionEffect::blur(3).unwrap(),
            ),
        )
        .unwrap();
        for y in 0..3 {
            for x in 0..4 {
                assert_eq!(
                    constant.get_pixel(x, y).0,
                    if (1..3).contains(&x) && (1..3).contains(&y) {
                        color
                    } else {
                        outside
                    }
                );
            }
        }

        let mut two_dimensional = RgbaImage::from_fn(3, 3, |x, y| {
            let value = if (x, y) == (2, 2) { 90 } else { 0 };
            Rgba([value, value, value, 255])
        });
        apply_redaction(
            &mut two_dimensional,
            operation(
                bounds(0.0, 0.0, 3.0, 3.0),
                RedactionEffect::blur(1).unwrap(),
            ),
        )
        .unwrap();
        for (x, y, expected) in [
            (0, 0, 0),
            (1, 0, 0),
            (2, 0, 0),
            (0, 1, 0),
            (1, 1, 10),
            (2, 1, 20),
            (0, 2, 0),
            (1, 2, 20),
            (2, 2, 40),
        ] {
            assert_eq!(
                two_dimensional.get_pixel(x, y).0,
                [expected, expected, expected, 255]
            );
        }
    }

    #[test]
    fn pixelation_averages_full_and_partial_blocks_with_channel_rounding() {
        let mut image = RgbaImage::from_fn(3, 2, |x, y| {
            let value = [[0, 1, 3], [4, 5, 6]][y as usize][x as usize];
            Rgba([value, value, value, 100 + value])
        });
        apply_redaction(
            &mut image,
            operation(
                bounds(0.0, 0.0, 3.0, 2.0),
                RedactionEffect::pixelate(2).unwrap(),
            ),
        )
        .unwrap();
        for (x, y, expected) in [
            (0, 0, [3, 3, 3, 103]),
            (1, 0, [3, 3, 3, 103]),
            (0, 1, [3, 3, 3, 103]),
            (1, 1, [3, 3, 3, 103]),
            (2, 0, [5, 5, 5, 105]),
            (2, 1, [5, 5, 5, 105]),
        ] {
            assert_eq!(image.get_pixel(x, y).0, expected);
        }

        let mut oversized_block = RgbaImage::from_fn(2, 1, |x, _| {
            let value = [0, 10][x as usize];
            Rgba([value, value, value, 255])
        });
        apply_redaction(
            &mut oversized_block,
            operation(
                bounds(0.0, 0.0, 2.0, 1.0),
                RedactionEffect::pixelate(MAX_PIXEL_BLOCK_SIZE).unwrap(),
            ),
        )
        .unwrap();
        assert_eq!(oversized_block.get_pixel(0, 0).0, [5, 5, 5, 255]);
        assert_eq!(oversized_block.get_pixel(1, 0).0, [5, 5, 5, 255]);
    }

    #[test]
    fn scratch_is_region_bounded_and_limit_failure_precedes_pixel_changes() {
        let mut image = RgbaImage::from_fn(1024, 768, |x, y| {
            Rgba([(x % 251) as u8, (y % 239) as u8, 91, 255])
        });
        let small_regions = [
            bounds(20.0, 30.0, 52.0, 62.0),
            bounds(200.0, 300.0, 232.0, 332.0),
            bounds(700.0, 500.0, 732.0, 532.0),
        ];
        for region in small_regions {
            apply_redaction(
                &mut image,
                operation(region, RedactionEffect::blur(2).unwrap()),
            )
            .unwrap();
        }

        let before = image.clone();
        let error = apply_redaction_with_scratch_limit(
            &mut image,
            operation(
                bounds(10.0, 10.0, 74.0, 74.0),
                RedactionEffect::blur(2).unwrap(),
            ),
            20_000,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            RedactionError::ScratchLimitExceeded { required, limit: 20_000 }
                if required >= 32_768
        ));
        assert_eq!(
            image, before,
            "refusal must happen before mutating any pixels"
        );
    }
}
