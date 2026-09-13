use serde::{Deserialize, Serialize};

/// A point in signed physical coordinates on the Windows virtual desktop.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DesktopPoint {
    pub x: i32,
    pub y: i32,
}

/// A subpixel point used while clipping physical pointer movement. Keeping
/// intersections in floating point prevents premature unsigned conversion or
/// rounding on desktops whose origin is negative.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct DesktopPointF64 {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

impl From<DesktopPoint> for DesktopPointF64 {
    fn from(point: DesktopPoint) -> Self {
        Self {
            x: f64::from(point.x),
            y: f64::from(point.y),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct VisibleSegment {
    pub(crate) from: DesktopPointF64,
    pub(crate) to: DesktopPointF64,
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

    /// Expands this rectangle in signed desktop space without overflowing at
    /// either edge of the physical-coordinate domain.
    pub(crate) fn inflated(self, radius: u32) -> Self {
        if self.is_empty() || radius == 0 {
            return self;
        }
        let radius = i64::from(radius);
        let left = (i64::from(self.x) - radius).max(i64::from(i32::MIN));
        let top = (i64::from(self.y) - radius).max(i64::from(i32::MIN));
        let right = (self.right() + radius).min(i64::from(i32::MAX) + 1);
        let bottom = (self.bottom() + radius).min(i64::from(i32::MAX) + 1);
        Self::new(
            left as i32,
            top as i32,
            u32::try_from(right - left).unwrap_or(u32::MAX),
            u32::try_from(bottom - top).unwrap_or(u32::MAX),
        )
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

/// Subtracts an axis-aligned exclusion rectangle from one pointer segment.
/// Returned fragments retain input order. Rectangle edges are treated as
/// excluded; an exact corner tangent, which has no positive-length overlap,
/// leaves the segment intact.
pub(crate) fn visible_segment_fragments(
    from: DesktopPoint,
    to: DesktopPoint,
    exclusion: DesktopRect,
) -> Vec<VisibleSegment> {
    let from = DesktopPointF64::from(from);
    let to = DesktopPointF64::from(to);
    let full = VisibleSegment { from, to };
    if exclusion.is_empty() {
        return vec![full];
    }

    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let left = f64::from(exclusion.x);
    let top = f64::from(exclusion.y);
    let right = exclusion.right() as f64;
    let bottom = exclusion.bottom() as f64;

    let mut enter = 0.0_f64;
    let mut exit = 1.0_f64;
    for (p, q) in [
        (-dx, from.x - left),
        (dx, right - from.x),
        (-dy, from.y - top),
        (dy, bottom - from.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return vec![full];
            }
            continue;
        }
        let ratio = q / p;
        if p < 0.0 {
            enter = enter.max(ratio);
        } else {
            exit = exit.min(ratio);
        }
        if enter > exit {
            return vec![full];
        }
    }

    // A corner tangent removes only a mathematical point. Treating it as a
    // crossing would create an unnecessary persisted subpath discontinuity.
    if exit - enter <= f64::EPSILON * 16.0 {
        return vec![full];
    }

    let point_at = |t: f64| DesktopPointF64 {
        x: from.x + dx * t,
        y: from.y + dy * t,
    };
    let mut visible = Vec::with_capacity(2);
    if enter > 0.0 {
        visible.push(VisibleSegment {
            from,
            to: point_at(enter),
        });
    }
    if exit < 1.0 {
        visible.push(VisibleSegment {
            from: point_at(exit),
            to,
        });
    }
    visible
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

    #[test]
    fn rectangle_inflation_is_signed_and_overflow_safe() {
        assert_eq!(
            DesktopRect::new(-100, -50, 200, 100).inflated(34),
            DesktopRect::new(-134, -84, 268, 168)
        );
        assert_eq!(
            DesktopRect::new(i32::MIN + 2, i32::MAX - 20, 10, 10).inflated(100),
            DesktopRect::new(i32::MIN, i32::MAX - 120, 112, 121)
        );
    }

    fn assert_point(actual: DesktopPointF64, expected: (f64, f64)) {
        assert!((actual.x - expected.0).abs() < 1e-9, "{actual:?}");
        assert!((actual.y - expected.1).abs() < 1e-9, "{actual:?}");
    }

    #[test]
    fn segment_crossing_exclusion_returns_two_ordered_visible_fragments() {
        let fragments = visible_segment_fragments(
            DesktopPoint::new(50, 200),
            DesktopPoint::new(400, 200),
            DesktopRect::new(100, 100, 250, 700),
        );
        assert_eq!(fragments.len(), 2);
        assert_point(fragments[0].from, (50.0, 200.0));
        assert_point(fragments[0].to, (100.0, 200.0));
        assert_point(fragments[1].from, (350.0, 200.0));
        assert_point(fragments[1].to, (400.0, 200.0));
    }

    #[test]
    fn segment_exclusion_handles_inside_entry_exit_and_negative_coordinates() {
        let rect = DesktopRect::new(-300, -200, 100, 100);
        assert!(
            visible_segment_fragments(
                DesktopPoint::new(-250, -150),
                DesktopPoint::new(-220, -120),
                rect
            )
            .is_empty()
        );

        let entering = visible_segment_fragments(
            DesktopPoint::new(-400, -150),
            DesktopPoint::new(-250, -150),
            rect,
        );
        assert_eq!(entering.len(), 1);
        assert_point(entering[0].from, (-400.0, -150.0));
        assert_point(entering[0].to, (-300.0, -150.0));

        let leaving = visible_segment_fragments(
            DesktopPoint::new(-250, -150),
            DesktopPoint::new(-100, -150),
            rect,
        );
        assert_eq!(leaving.len(), 1);
        assert_point(leaving[0].from, (-200.0, -150.0));
        assert_point(leaving[0].to, (-100.0, -150.0));
    }

    #[test]
    fn segment_exclusion_has_deterministic_boundary_and_tangent_behavior() {
        let rect = DesktopRect::new(10, 10, 10, 10);
        let tangent =
            visible_segment_fragments(DesktopPoint::new(0, 0), DesktopPoint::new(10, 10), rect);
        assert_eq!(tangent.len(), 1);
        assert_point(tangent[0].from, (0.0, 0.0));
        assert_point(tangent[0].to, (10.0, 10.0));

        let along_edge =
            visible_segment_fragments(DesktopPoint::new(0, 10), DesktopPoint::new(30, 10), rect);
        assert_eq!(along_edge.len(), 2);
        assert_point(along_edge[0].to, (10.0, 10.0));
        assert_point(along_edge[1].from, (20.0, 10.0));

        let outside = visible_segment_fragments(
            DesktopPoint::new(i32::MIN, -1),
            DesktopPoint::new(i32::MAX, -1),
            rect,
        );
        assert_eq!(outside.len(), 1);
    }
}
