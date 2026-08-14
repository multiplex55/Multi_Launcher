//! Exact-size, non-scaling RGBA template and pixel search.
use crate::mkmacro::MkMacroStore;
use crate::mkmacro::{
    CapturedRegion, DiagnosticKind, ExecResult, ExecutionDiagnostic, cancelled_error,
};
use image::{Rgba, RgbaImage};
use std::{collections::HashMap, path::Path, sync::Arc};

/// Run-scoped decode cache. Construct one for each playback; repeated visual waits
/// and searches using the same stable reference share the decoded pixels.
#[derive(Default)]
pub struct ImageDecodeCache {
    images: HashMap<String, Arc<RgbaImage>>,
}
impl ImageDecodeCache {
    pub fn get_or_decode(
        &mut self,
        store: &MkMacroStore,
        macro_id: u64,
        reference: &str,
    ) -> ExecResult<Arc<RgbaImage>> {
        if let Some(image) = self.images.get(reference) {
            return Ok(image.clone());
        }
        let path = store
            .resolve_asset_reference(macro_id, Path::new(reference))
            .map_err(|e| ExecutionDiagnostic::new(DiagnosticKind::InvalidTarget, e.to_string()))?;
        let image = image::open(&path)
            .map_err(|e| {
                ExecutionDiagnostic::new(
                    DiagnosticKind::InvalidTarget,
                    format!("decode reference image {}: {e}", path.display()),
                )
            })?
            .to_rgba8();
        let image = Arc::new(image);
        self.images.insert(reference.to_owned(), image.clone());
        Ok(image)
    }
    pub fn len(&self) -> usize {
        self.images.len()
    }
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnPoint {
    TopLeft,
    Center,
}
impl Default for ReturnPoint {
    fn default() -> Self {
        Self::Center
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlphaPolicy {
    Compare,
    Ignore,
}
impl Default for AlphaPolicy {
    fn default() -> Self {
        Self::Compare
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MatchOptions {
    pub tolerance: u8,
    pub alpha: AlphaPolicy,
    pub return_point: ReturnPoint,
    pub first_result: bool,
}
impl Default for MatchOptions {
    fn default() -> Self {
        Self {
            tolerance: 0,
            alpha: AlphaPolicy::Compare,
            return_point: ReturnPoint::Center,
            first_result: true,
        }
    }
}

/// Searches rows top-to-bottom and columns left-to-right. Anchors (corners and
/// center, de-duplicated) are checked first; survivors receive a complete comparison.
pub fn find_template(
    frame: &CapturedRegion,
    needle: &RgbaImage,
    options: MatchOptions,
    cancelled: &dyn Fn() -> bool,
) -> ExecResult<Option<(i32, i32)>> {
    if needle.width() == 0 || needle.height() == 0 {
        return Err(invalid("reference image is empty"));
    }
    if needle.width() > frame.image.width() || needle.height() > frame.image.height() {
        return Err(invalid(format!(
            "reference image {}x{} is larger than captured region {}x{}",
            needle.width(),
            needle.height(),
            frame.image.width(),
            frame.image.height()
        )));
    }
    let mut anchors = vec![
        (0, 0),
        (needle.width() - 1, 0),
        (0, needle.height() - 1),
        (needle.width() - 1, needle.height() - 1),
        (needle.width() / 2, needle.height() / 2),
    ];
    anchors.sort_unstable();
    anchors.dedup();
    for y in 0..=frame.image.height() - needle.height() {
        for x in 0..=frame.image.width() - needle.width() {
            if cancelled() {
                return Err(cancelled_error());
            }
            if !anchors.iter().all(|&(ax, ay)| {
                pixel_eq(
                    frame.image.get_pixel(x + ax, y + ay),
                    needle.get_pixel(ax, ay),
                    options.tolerance,
                    options.alpha,
                )
            }) {
                continue;
            }
            let mut ok = true;
            'pixels: for ny in 0..needle.height() {
                for nx in 0..needle.width() {
                    if cancelled() {
                        return Err(cancelled_error());
                    }
                    if !pixel_eq(
                        frame.image.get_pixel(x + nx, y + ny),
                        needle.get_pixel(nx, ny),
                        options.tolerance,
                        options.alpha,
                    ) {
                        ok = false;
                        break 'pixels;
                    }
                }
            }
            if ok {
                let local = match options.return_point {
                    ReturnPoint::TopLeft => (x, y),
                    ReturnPoint::Center => (x + needle.width() / 2, y + needle.height() / 2),
                };
                return Ok(frame.desktop_point(local));
            }
        }
    }
    Ok(None)
}
fn pixel_eq(a: &Rgba<u8>, b: &Rgba<u8>, tolerance: u8, alpha: AlphaPolicy) -> bool {
    let n = if alpha == AlphaPolicy::Compare { 4 } else { 3 };
    (0..n).all(|i| a.0[i].abs_diff(b.0[i]) <= tolerance)
}
fn invalid(s: impl Into<String>) -> ExecutionDiagnostic {
    ExecutionDiagnostic::new(DiagnosticKind::InvalidTarget, s)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelScanOrder {
    TopLeftRows,
    TopRightRows,
    BottomLeftRows,
    BottomRightRows,
    LeftTopColumns,
    RightTopColumns,
}
impl Default for PixelScanOrder {
    fn default() -> Self {
        Self::TopLeftRows
    }
}
pub fn find_pixel(
    frame: &CapturedRegion,
    color: Rgba<u8>,
    tolerance: u8,
    alpha: AlphaPolicy,
    order: PixelScanOrder,
    cancelled: &dyn Fn() -> bool,
) -> ExecResult<Option<(i32, i32)>> {
    let (w, h) = frame.image.dimensions();
    let mut points: Vec<(u32, u32)> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).collect();
    match order {
        PixelScanOrder::TopLeftRows => {}
        PixelScanOrder::TopRightRows => points.sort_by_key(|&(x, y)| (y, std::cmp::Reverse(x))),
        PixelScanOrder::BottomLeftRows => points.sort_by_key(|&(x, y)| (std::cmp::Reverse(y), x)),
        PixelScanOrder::BottomRightRows => {
            points.sort_by_key(|&(x, y)| (std::cmp::Reverse(y), std::cmp::Reverse(x)))
        }
        PixelScanOrder::LeftTopColumns => points.sort_by_key(|&(x, y)| (x, y)),
        PixelScanOrder::RightTopColumns => points.sort_by_key(|&(x, y)| (std::cmp::Reverse(x), y)),
    }
    for p in points {
        if cancelled() {
            return Err(cancelled_error());
        }
        if pixel_eq(frame.image.get_pixel(p.0, p.1), &color, tolerance, alpha) {
            return Ok(frame.desktop_point(p));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cap(w: u32, h: u32) -> CapturedRegion {
        CapturedRegion {
            image: RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 255])),
            origin: (-5, -7),
        }
    }
    #[test]
    fn exact_edges_center_and_no_match() {
        for (x, y) in [(0, 0), (2, 0), (0, 2), (2, 2)] {
            let mut f = cap(4, 4);
            for dy in 0..2 {
                for dx in 0..2 {
                    f.image.put_pixel(x + dx, y + dy, Rgba([9, 8, 7, 255]))
                }
            }
            let n = RgbaImage::from_pixel(2, 2, Rgba([9, 8, 7, 255]));
            assert_eq!(
                find_template(&f, &n, MatchOptions::default(), &|| false).unwrap(),
                Some((-5 + x as i32 + 1, -7 + y as i32 + 1))
            );
        }
        assert_eq!(
            find_template(
                &cap(3, 3),
                &RgbaImage::from_pixel(1, 1, Rgba([1, 1, 1, 255])),
                MatchOptions::default(),
                &|| false
            )
            .unwrap(),
            None
        )
    }
    #[test]
    fn tolerance_and_alpha() {
        let mut f = cap(1, 1);
        f.image.put_pixel(0, 0, Rgba([10, 20, 30, 1]));
        let n = RgbaImage::from_pixel(1, 1, Rgba([12, 20, 30, 255]));
        let mut o = MatchOptions {
            tolerance: 2,
            ..Default::default()
        };
        assert_eq!(find_template(&f, &n, o, &|| false).unwrap(), None);
        o.alpha = AlphaPolicy::Ignore;
        assert!(find_template(&f, &n, o, &|| false).unwrap().is_some());
        o.tolerance = 1;
        assert_eq!(find_template(&f, &n, o, &|| false).unwrap(), None)
    }
    #[test]
    fn too_large_and_deterministic() {
        let mut f = cap(3, 1);
        f.image.put_pixel(0, 0, Rgba([1, 1, 1, 255]));
        f.image.put_pixel(2, 0, Rgba([1, 1, 1, 255]));
        let n = RgbaImage::from_pixel(1, 1, Rgba([1, 1, 1, 255]));
        assert_eq!(
            find_template(
                &f,
                &n,
                MatchOptions {
                    return_point: ReturnPoint::TopLeft,
                    ..Default::default()
                },
                &|| false
            )
            .unwrap(),
            Some((-5, -7))
        );
        assert!(find_template(&f, &RgbaImage::new(4, 1), Default::default(), &|| false).is_err())
    }
    #[test]
    fn pixel_orders() {
        let mut f = cap(2, 2);
        for p in [(1, 0), (0, 1)] {
            f.image.put_pixel(p.0, p.1, Rgba([5, 5, 5, 255]))
        }
        assert_eq!(
            find_pixel(
                &f,
                Rgba([5, 5, 5, 255]),
                0,
                AlphaPolicy::Compare,
                PixelScanOrder::TopLeftRows,
                &|| false
            )
            .unwrap(),
            Some((-4, -7))
        );
        assert_eq!(
            find_pixel(
                &f,
                Rgba([6, 5, 5, 255]),
                1,
                AlphaPolicy::Compare,
                PixelScanOrder::LeftTopColumns,
                &|| false
            )
            .unwrap(),
            Some((-5, -6))
        )
    }
}
