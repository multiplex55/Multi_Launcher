use serde::{Deserialize, Serialize};

/// A point in signed physical coordinates on the Windows virtual desktop.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DesktopPoint {
    pub x: i32,
    pub y: i32,
}

impl DesktopPoint {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DesktopSize {
    pub width: u32,
    pub height: u32,
}

impl DesktopSize {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// A half-open rectangle in signed physical virtual-desktop coordinates.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DesktopRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl DesktopRect {
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn origin(self) -> DesktopPoint {
        DesktopPoint::new(self.x, self.y)
    }

    pub const fn size(self) -> DesktopSize {
        DesktopSize::new(self.width, self.height)
    }

    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn right(self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }

    pub fn bottom(self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }

    pub fn contains_point(self, point: DesktopPoint) -> bool {
        !self.is_empty()
            && i64::from(point.x) >= i64::from(self.x)
            && i64::from(point.x) < self.right()
            && i64::from(point.y) >= i64::from(self.y)
            && i64::from(point.y) < self.bottom()
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let left = i64::from(self.x).max(i64::from(other.x));
        let top = i64::from(self.y).max(i64::from(other.y));
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right <= left || bottom <= top {
            return None;
        }
        Some(Self::new(
            i32::try_from(left).ok()?,
            i32::try_from(top).ok()?,
            u32::try_from(right - left).ok()?,
            u32::try_from(bottom - top).ok()?,
        ))
    }

    pub fn desktop_to_local(self, point: DesktopPoint) -> Option<LocalPoint> {
        if !self.contains_point(point) {
            return None;
        }
        Some(LocalPoint::new(
            u32::try_from(i64::from(point.x) - i64::from(self.x)).ok()?,
            u32::try_from(i64::from(point.y) - i64::from(self.y)).ok()?,
        ))
    }

    pub fn local_to_desktop(self, point: LocalPoint) -> Option<DesktopPoint> {
        if point.x >= self.width || point.y >= self.height {
            return None;
        }
        Some(DesktopPoint::new(
            i32::try_from(i64::from(self.x) + i64::from(point.x)).ok()?,
            i32::try_from(i64::from(self.y) + i64::from(point.y)).ok()?,
        ))
    }
}

impl From<crate::mkmacro::screen::ScreenRect> for DesktopRect {
    fn from(value: crate::mkmacro::screen::ScreenRect) -> Self {
        Self::new(value.x, value.y, value.width, value.height)
    }
}

impl From<DesktopRect> for crate::mkmacro::screen::ScreenRect {
    fn from(value: DesktopRect) -> Self {
        Self::new(value.x, value.y, value.width, value.height)
    }
}

/// An image-buffer coordinate. Construction follows checked desktop translation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalPoint {
    pub x: u32,
    pub y: u32,
}

impl LocalPoint {
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}

/// A validated, buffer-local crop of an offline desktop image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CropPlan {
    pub desktop_rect: DesktopRect,
    pub source_x: u32,
    pub source_y: u32,
    pub width: u32,
    pub height: u32,
}

/// Intersects a requested desktop region with an offline source and translates
/// the result into source-buffer coordinates.
pub fn plan_crop(source: DesktopRect, requested: DesktopRect) -> Option<CropPlan> {
    let desktop_rect = source.intersection(requested)?;
    let local = source.desktop_to_local(desktop_rect.origin())?;
    Some(CropPlan {
        desktop_rect,
        source_x: local.x,
        source_y: local.y,
        width: desktop_rect.width,
        height: desktop_rect.height,
    })
}

/// Recovers a toolbar onto the nearest visible monitor and keeps it fully
/// inside that monitor whenever the monitor is large enough.
pub fn clamp_toolbar_position(
    requested: DesktopPoint,
    toolbar_size: DesktopSize,
    monitors: &[DesktopRect],
) -> Option<DesktopPoint> {
    let monitor = monitors
        .iter()
        .copied()
        .filter(|monitor| !monitor.is_empty())
        .min_by_key(|monitor| distance_to_rect_squared(requested, *monitor))?;

    let max_x = monitor
        .right()
        .saturating_sub(i64::from(toolbar_size.width))
        .max(i64::from(monitor.x));
    let max_y = monitor
        .bottom()
        .saturating_sub(i64::from(toolbar_size.height))
        .max(i64::from(monitor.y));
    Some(DesktopPoint::new(
        i32::try_from(i64::from(requested.x).clamp(i64::from(monitor.x), max_x)).ok()?,
        i32::try_from(i64::from(requested.y).clamp(i64::from(monitor.y), max_y)).ok()?,
    ))
}

fn distance_to_rect_squared(point: DesktopPoint, rect: DesktopRect) -> i128 {
    let nearest_x = i64::from(point.x).clamp(i64::from(rect.x), rect.right().saturating_sub(1));
    let nearest_y = i64::from(point.y).clamp(i64::from(rect.y), rect.bottom().saturating_sub(1));
    let dx = i128::from(i64::from(point.x) - nearest_x);
    let dy = i128::from(i64::from(point.y) - nearest_y);
    dx * dx + dy * dy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_desktop_coordinates_round_trip_through_local_space() {
        let desktop = DesktopRect::new(-1920, -240, 4480, 1680);
        let point = DesktopPoint::new(-17, 800);
        let local = desktop.desktop_to_local(point).unwrap();
        assert_eq!(local, LocalPoint::new(1903, 1040));
        assert_eq!(desktop.local_to_desktop(local), Some(point));
        assert_eq!(desktop.desktop_to_local(DesktopPoint::new(-1921, 0)), None);
    }

    #[test]
    fn crop_plan_clips_and_translates_a_negative_origin_selection() {
        let source = DesktopRect::new(-1920, -200, 4480, 1640);
        let requested = DesktopRect::new(-2000, -100, 500, 400);
        assert_eq!(
            plan_crop(source, requested),
            Some(CropPlan {
                desktop_rect: DesktopRect::new(-1920, -100, 420, 400),
                source_x: 0,
                source_y: 100,
                width: 420,
                height: 400,
            })
        );
        assert_eq!(plan_crop(source, DesktopRect::new(3000, 0, 20, 20)), None);
    }

    #[test]
    fn toolbar_clamping_handles_negative_and_disconnected_monitors() {
        let monitors = [
            DesktopRect::new(-1920, 0, 1920, 1080),
            DesktopRect::new(0, 0, 2560, 1440),
        ];
        assert_eq!(
            clamp_toolbar_position(
                DesktopPoint::new(-80, 1000),
                DesktopSize::new(200, 300),
                &monitors,
            ),
            Some(DesktopPoint::new(-200, 780))
        );
        assert_eq!(
            clamp_toolbar_position(
                DesktopPoint::new(-5000, -5000),
                DesktopSize::new(200, 300),
                &monitors,
            ),
            Some(DesktopPoint::new(-1920, 0))
        );
        assert_eq!(
            clamp_toolbar_position(DesktopPoint::new(0, 0), DesktopSize::new(20, 20), &[]),
            None
        );
    }

    #[test]
    fn oversized_toolbar_anchors_at_monitor_origin() {
        assert_eq!(
            clamp_toolbar_position(
                DesktopPoint::new(100, 100),
                DesktopSize::new(1000, 1000),
                &[DesktopRect::new(-100, -50, 300, 200)],
            ),
            Some(DesktopPoint::new(-100, -50))
        );
    }
}
