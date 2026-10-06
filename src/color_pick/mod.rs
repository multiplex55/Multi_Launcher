mod native;
mod runtime;

pub use runtime::{ColorPickController, ColorPickEvent, ColorPickSessionId};

use crate::color::RgbColor;
use crate::mkmacro::screen::CapturedRegion;
use std::sync::Arc;

pub const MAGNIFIER_SIDE: usize = 9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorPickOutcome {
    Picked(RgbColor),
    Cancelled,
    Failed(String),
}

/// All pointer previews and selection read the same immutable desktop capture.
#[derive(Debug)]
pub struct FrozenPicker {
    snapshot: Arc<CapturedRegion>,
    hovered: Option<(u32, u32)>,
}

impl FrozenPicker {
    pub fn new(snapshot: Arc<CapturedRegion>) -> Result<Self, String> {
        snapshot
            .rect()
            .validate_capture()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            snapshot,
            hovered: None,
        })
    }

    pub fn snapshot(&self) -> &CapturedRegion {
        &self.snapshot
    }

    pub fn hover(&mut self, desktop: (i32, i32)) -> Option<RgbColor> {
        self.hovered = self.snapshot.local_point(desktop);
        self.hovered.map(|point| self.pixel(point))
    }

    pub fn hovered(&self) -> Option<(u32, u32)> {
        self.hovered
    }

    pub fn accept(&mut self, desktop: (i32, i32)) -> Option<ColorPickOutcome> {
        self.hover(desktop).map(ColorPickOutcome::Picked)
    }

    fn pixel(&self, point: (u32, u32)) -> RgbColor {
        let rgba = self.snapshot.image.get_pixel(point.0, point.1).0;
        RgbColor::new(rgba[0], rgba[1], rgba[2])
    }

    pub fn magnifier(&self) -> Option<[[RgbColor; MAGNIFIER_SIDE]; MAGNIFIER_SIDE]> {
        let (x, y) = self.hovered?;
        let radius = (MAGNIFIER_SIDE / 2) as i64;
        Some(std::array::from_fn(|row| {
            std::array::from_fn(|column| {
                let sample_x = (i64::from(x) + column as i64 - radius)
                    .clamp(0, i64::from(self.snapshot.image.width()) - 1)
                    as u32;
                let sample_y = (i64::from(y) + row as i64 - radius)
                    .clamp(0, i64::from(self.snapshot.image.height()) - 1)
                    as u32;
                self.pixel((sample_x, sample_y))
            })
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    pub(super) fn snapshot() -> Arc<CapturedRegion> {
        Arc::new(CapturedRegion {
            origin: (-3, -2),
            image: RgbaImage::from_fn(6, 4, |x, y| Rgba([x as u8, y as u8, 123, 17])),
        })
    }

    #[test]
    fn signed_mixed_layout_bounds_and_exclusive_edges_select_exact_rgb() {
        let mut picker = FrozenPicker::new(snapshot()).unwrap();
        assert_eq!(
            picker.accept((-3, -2)),
            Some(ColorPickOutcome::Picked(RgbColor::new(0, 0, 123)))
        );
        assert_eq!(
            picker.accept((2, 1)),
            Some(ColorPickOutcome::Picked(RgbColor::new(5, 3, 123)))
        );
        for point in [(-4, -2), (-3, -3), (3, 0), (0, 2), (i32::MAX, i32::MIN)] {
            assert_eq!(picker.accept(point), None);
            assert_eq!(picker.magnifier(), None);
        }
    }

    #[test]
    fn frozen_snapshot_and_centered_magnifier_keep_hover_pixel_at_edges() {
        let snapshot = snapshot();
        let mut picker = FrozenPicker::new(Arc::clone(&snapshot)).unwrap();
        for point in [(-3, -2), (2, 1), (0, 0)] {
            let color = picker.hover(point).unwrap();
            let grid = picker.magnifier().unwrap();
            assert_eq!(grid[4][4], color);
            assert!(Arc::ptr_eq(&snapshot, &picker.snapshot));
        }
        picker.hover((2, 1));
        let grid = picker.magnifier().unwrap();
        assert_eq!(grid[0][0], RgbColor::new(1, 0, 123));
        assert_eq!(grid[8][8], RgbColor::new(5, 3, 123));
        picker.hover((-3, -2));
        let grid = picker.magnifier().unwrap();
        assert_eq!(grid[0][0], RgbColor::new(0, 0, 123));
        assert_eq!(grid[4][4], RgbColor::new(0, 0, 123));
        assert_eq!(grid[8][8], RgbColor::new(4, 3, 123));
    }
}
