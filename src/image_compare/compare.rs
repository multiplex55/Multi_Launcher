use super::budget::ImageBudgetLease;
use super::input::{ImageInput, MAX_IMAGE_PIXELS};
use super::{ImageCompareBudget, ImageCompareError};
use std::sync::Arc;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DifferenceClass {
    Equal = 0,
    Changed = 1,
    AOnly = 2,
    BOnly = 3,
    Neither = 4,
}

struct ComparisonData {
    width: u32,
    height: u32,
    input_ids: (super::input::ImageInputId, super::input::ImageInputId),
    input_revisions: (u64, u64),
    classes: Vec<DifferenceClass>,
    changed_pixels: usize,
    _reservation: ImageBudgetLease,
}

/// Pair-bound exact comparison output. Cloning this handle does not copy its
/// mask or release its accounted storage.
#[derive(Clone)]
pub(crate) struct ImageComparison(Arc<ComparisonData>);

impl ImageComparison {
    pub(crate) fn dimensions(&self) -> (u32, u32) {
        (self.0.width, self.0.height)
    }

    pub(crate) fn input_ids(&self) -> (super::input::ImageInputId, super::input::ImageInputId) {
        self.0.input_ids
    }

    pub(crate) fn input_revisions(&self) -> (u64, u64) {
        self.0.input_revisions
    }

    pub(crate) fn classes(&self) -> &[DifferenceClass] {
        &self.0.classes
    }

    pub(crate) fn changed_pixels(&self) -> usize {
        self.0.changed_pixels
    }

    pub(crate) fn class_at(&self, x: u32, y: u32) -> Option<DifferenceClass> {
        if x >= self.0.width || y >= self.0.height {
            return None;
        }
        let index = (y as usize)
            .checked_mul(self.0.width as usize)?
            .checked_add(x as usize)?;
        self.0.classes.get(index).copied()
    }
}

pub(super) fn compare_inputs(
    a: &ImageInput,
    b: &ImageInput,
    budget: &ImageCompareBudget,
    cancelled: impl Fn() -> bool,
) -> Result<ImageComparison, ImageCompareError> {
    let (a_width, a_height) = a.dimensions();
    let (b_width, b_height) = b.dimensions();
    let width = a_width.max(b_width);
    let height = a_height.max(b_height);
    let canvas_pixels = usize::try_from(width)
        .ok()
        .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
        .filter(|pixels| *pixels > 0 && *pixels <= MAX_IMAGE_PIXELS)
        .ok_or(ImageCompareError::ComparisonCanvasTooLarge)?;
    let mut reservation = budget.reserve(canvas_pixels)?;
    let mut classes = Vec::new();
    classes.try_reserve_exact(canvas_pixels).map_err(|error| {
        ImageCompareError::Decode(format!("comparison mask could not be allocated: {error}"))
    })?;
    classes.resize(canvas_pixels, DifferenceClass::Neither);

    let a_pixels = a.pixels();
    let b_pixels = b.pixels();
    let mut changed_pixels = 0usize;
    for y in 0..height as usize {
        if cancelled() {
            return Err(ImageCompareError::Cancelled);
        }
        let canvas_row = y * width as usize;
        let a_inside_y = y < a_height as usize;
        let b_inside_y = y < b_height as usize;
        for x in 0..width as usize {
            let a_inside = a_inside_y && x < a_width as usize;
            let b_inside = b_inside_y && x < b_width as usize;
            let class = match (a_inside, b_inside) {
                (false, false) => DifferenceClass::Neither,
                (true, false) => DifferenceClass::AOnly,
                (false, true) => DifferenceClass::BOnly,
                (true, true) => {
                    let a_offset = (y * a_width as usize + x) * 4;
                    let b_offset = (y * b_width as usize + x) * 4;
                    let equal =
                        a_pixels[a_offset..a_offset + 4] == b_pixels[b_offset..b_offset + 4];
                    if equal {
                        DifferenceClass::Equal
                    } else {
                        changed_pixels += 1;
                        DifferenceClass::Changed
                    }
                }
            };
            classes[canvas_row + x] = class;
        }
    }
    reservation.resize(canvas_pixels)?;
    Ok(ImageComparison(Arc::new(ComparisonData {
        width,
        height,
        input_ids: (a.id(), b.id()),
        input_revisions: (a.revision(), b.revision()),
        classes,
        changed_pixels,
        _reservation: reservation,
    })))
}
