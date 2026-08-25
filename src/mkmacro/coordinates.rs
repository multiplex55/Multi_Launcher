//! Pure coordinate resolution and Win32 absolute-coordinate normalization.
use super::{DiagnosticKind, ExecResult, ExecutionDiagnostic, MkPoint};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualDesktop {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Converts a desktop pixel into SendInput's inclusive 0..=65535 coordinate space.
/// Points outside the virtual desktop are clamped to its nearest pixel.
pub fn normalize_absolute(point: MkPoint, desktop: VirtualDesktop) -> ExecResult<(i32, i32)> {
    if desktop.width <= 0 || desktop.height <= 0 {
        return Err(ExecutionDiagnostic::new(
            DiagnosticKind::InvalidTarget,
            "virtual desktop has zero or negative dimensions",
        ));
    }
    let max_x = desktop.x.saturating_add(desktop.width - 1);
    let max_y = desktop.y.saturating_add(desktop.height - 1);
    let x = point.x.clamp(desktop.x, max_x) as i64 - desktop.x as i64;
    let y = point.y.clamp(desktop.y, max_y) as i64 - desktop.y as i64;
    let nx = if desktop.width == 1 {
        0
    } else {
        x * 65_535 / (desktop.width - 1) as i64
    };
    let ny = if desktop.height == 1 {
        0
    } else {
        y * 65_535 / (desktop.height - 1) as i64
    };
    Ok((nx as i32, ny as i32))
}

pub trait CoordinateData {
    fn virtual_desktop(&self) -> ExecResult<VirtualDesktop>;
    fn active_window_rect(&self) -> ExecResult<Rect>;
    fn active_client_origin(&self) -> ExecResult<MkPoint>;
    fn mouse_position(&self) -> ExecResult<MkPoint>;
    fn image_result(&self, id: u64) -> ExecResult<MkPoint>;
    fn pixel_result(&self, id: u64) -> ExecResult<MkPoint> {
        Err(ExecutionDiagnostic::new(
            DiagnosticKind::TargetNotFound,
            format!("pixel search {id} has no coordinate result"),
        ))
    }
    fn uia_result(&self, id: &str) -> ExecResult<MkPoint>;
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoordinateBase {
    Screen(MkPoint),
    Window(MkPoint),
    Client(MkPoint),
    RelativeMouse(MkPoint),
    ImageResult { id: u64, offset: MkPoint },
    PixelResult { id: u64, offset: MkPoint },
    UiAutomationElement { id: String, offset: MkPoint },
}
pub trait OffsetRng {
    fn inclusive(&mut self, low: i32, high: i32) -> i32;
}
/// Resolves the base before applying an inclusive random offset. The final point is
/// clamped to the virtual desktop, which is the documented out-of-bounds policy.
pub fn resolve_coordinate(
    data: &dyn CoordinateData,
    target: &CoordinateBase,
    radius: u32,
    rng: &mut dyn OffsetRng,
) -> ExecResult<MkPoint> {
    let add = |a: MkPoint, b: MkPoint| MkPoint {
        x: a.x.saturating_add(b.x),
        y: a.y.saturating_add(b.y),
    };
    let mut p = match target {
        CoordinateBase::Screen(p) => *p,
        CoordinateBase::Window(p) => {
            let r = data.active_window_rect()?;
            add(MkPoint { x: r.x, y: r.y }, *p)
        }
        CoordinateBase::Client(p) => add(data.active_client_origin()?, *p),
        CoordinateBase::RelativeMouse(p) => add(data.mouse_position()?, *p),
        CoordinateBase::ImageResult { id, offset } => add(data.image_result(*id)?, *offset),
        CoordinateBase::PixelResult { id, offset } => add(data.pixel_result(*id)?, *offset),
        CoordinateBase::UiAutomationElement { id, offset } => add(data.uia_result(id)?, *offset),
    };
    let r = radius.min(i32::MAX as u32) as i32;
    if r > 0 {
        p.x = p.x.saturating_add(rng.inclusive(-r, r));
        p.y = p.y.saturating_add(rng.inclusive(-r, r));
    }
    let d = data.virtual_desktop()?;
    if d.width <= 0 || d.height <= 0 {
        return Err(ExecutionDiagnostic::new(
            DiagnosticKind::InvalidTarget,
            "virtual desktop has zero or negative dimensions",
        ));
    }
    p.x = p.x.clamp(d.x, d.x.saturating_add(d.width - 1));
    p.y = p.y.clamp(d.y, d.y.saturating_add(d.height - 1));
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalization_handles_negative_origins() {
        assert_eq!(
            normalize_absolute(
                MkPoint { x: -1920, y: 0 },
                VirtualDesktop {
                    x: -1920,
                    y: 0,
                    width: 3840,
                    height: 1080
                }
            )
            .unwrap(),
            (0, 0)
        );
        assert_eq!(
            normalize_absolute(
                MkPoint { x: 1919, y: 1079 },
                VirtualDesktop {
                    x: -1920,
                    y: 0,
                    width: 3840,
                    height: 1080
                }
            )
            .unwrap(),
            (65535, 65535)
        );
    }
    #[test]
    fn above_primary() {
        assert_eq!(
            normalize_absolute(
                MkPoint { x: 0, y: -1080 },
                VirtualDesktop {
                    x: 0,
                    y: -1080,
                    width: 1920,
                    height: 2160
                }
            )
            .unwrap(),
            (0, 0)
        );
    }
    #[test]
    fn invalid_desktop() {
        assert!(
            normalize_absolute(
                MkPoint { x: 0, y: 0 },
                VirtualDesktop {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 1
                }
            )
            .is_err()
        );
    }
    struct PixelData;
    impl CoordinateData for PixelData {
        fn virtual_desktop(&self) -> ExecResult<VirtualDesktop> {
            Ok(VirtualDesktop {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            })
        }
        fn active_window_rect(&self) -> ExecResult<Rect> {
            unreachable!()
        }
        fn active_client_origin(&self) -> ExecResult<MkPoint> {
            unreachable!()
        }
        fn mouse_position(&self) -> ExecResult<MkPoint> {
            unreachable!()
        }
        fn image_result(&self, _: u64) -> ExecResult<MkPoint> {
            unreachable!()
        }
        fn pixel_result(&self, id: u64) -> ExecResult<MkPoint> {
            assert_eq!(id, 42);
            Ok(MkPoint { x: 20, y: 30 })
        }
        fn uia_result(&self, _: &str) -> ExecResult<MkPoint> {
            unreachable!()
        }
    }
    struct NoOffset;
    impl OffsetRng for NoOffset {
        fn inclusive(&mut self, _: i32, _: i32) -> i32 {
            0
        }
    }
    #[test]
    fn pixel_result_supports_offsets() {
        assert_eq!(
            resolve_coordinate(
                &PixelData,
                &CoordinateBase::PixelResult {
                    id: 42,
                    offset: MkPoint { x: -3, y: 5 }
                },
                0,
                &mut NoOffset
            )
            .unwrap(),
            MkPoint { x: 17, y: 35 }
        );
    }
}
