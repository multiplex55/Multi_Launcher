use image::{Rgba, RgbaImage};

use super::controller::CoordinateRenderFrame;
use super::model::{
    CoordinateSample, CoordinateSpace, CoordinateUnavailable, MonitorId, PhysicalPoint,
    PhysicalRect, PhysicalSize,
};
use super::settings::{
    CrosshairColor, CrosshairPreferences, HaloPreferences, HudDetail, ZoomMode, ZoomPreferences,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PixelRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl PixelRect {
    fn expanded(self, amount: u32) -> Self {
        let expansion = amount
            .checked_mul(2)
            .expect("bounded pixel rectangle expansion fits u32");
        Self {
            x: self
                .x
                .checked_sub(amount)
                .expect("crosshair outline remains inside its bitmap"),
            y: self
                .y
                .checked_sub(amount)
                .expect("crosshair outline remains inside its bitmap"),
            width: self
                .width
                .checked_add(expansion)
                .expect("bounded pixel rectangle width fits u32"),
            height: self
                .height
                .checked_add(expansion)
                .expect("bounded pixel rectangle height fits u32"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CrosshairGeometry {
    width: u32,
    height: u32,
    colored_strokes: [PixelRect; 4],
    outline_strokes: Option<[PixelRect; 4]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GuideOrientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GuideGeometry {
    pub origin: PhysicalPoint,
    pub width: u32,
    pub height: u32,
    colored_stroke: PixelRect,
    outline: Option<PixelRect>,
}

/// Physical desktop geometry for a circular halo centered on the sampled
/// cursor hotspot. Source and destination have identical extents at 1x.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HaloGeometry {
    pub origin: PhysicalPoint,
    pub source: PhysicalRect,
    pub diameter: i32,
}

/// How a source rectangle is known to intersect readable physical content.
/// The virtual desktop rectangle is deliberately not used as evidence here:
/// it can span gaps between monitors and does not prove those pixels exist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ZoomSourceCoverage {
    Unknown,
    SampledMonitor {
        id: MonitorId,
        bounds: PhysicalRect,
        visible: Option<PhysicalRect>,
        missing: PhysicalInsets,
    },
}

/// Which sampled bounds were available for destination placement/clipping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ZoomDestinationBounds {
    SampledMonitor,
    VirtualDesktopFallback,
}

/// Physical pixel margins omitted by an intersection, measured from the
/// corresponding edge of the requested rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PhysicalInsets {
    pub left: i64,
    pub top: i64,
    pub right: i64,
    pub bottom: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RasterPoint {
    pub x: u32,
    pub y: u32,
}

/// Fractional destination-pixel translation required after applying the exact
/// zoom factor to `source_hotspot` to land on `destination_hotspot`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RasterTranslation {
    pub x: f32,
    pub y: f32,
}

/// Per-axis outcome for offset placement. Centered mode remains cursor-anchored
/// and reports `Centered` even when the resulting destination clips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ZoomAxisPlacement {
    Centered,
    RequestedOffset,
    MirroredOffset,
    Clamped,
    LeadingEdgeAnchored,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ZoomRasterAlignment {
    /// Zero-based source pixel index corresponding to the cursor hotspot.
    pub source_hotspot: RasterPoint,
    /// Zero-based destination pixel index where that source hotspot is shown.
    pub destination_hotspot: RasterPoint,
    /// The configured scale is preserved exactly; rounded source dimensions
    /// never replace or modify it.
    pub zoom_factor: f32,
    /// Residual translation after scaling the source hotspot by `zoom_factor`.
    /// This captures fractional alignment caused by rounded source coverage.
    pub hotspot_translation: RasterTranslation,
}

/// Cursor-anchored source and monitor-aware destination for a circular lens.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ZoomLensGeometry {
    /// Source remains centered on `hotspot`, irrespective of destination
    /// mirroring or clamping.
    pub source: PhysicalRect,
    pub hotspot: PhysicalPoint,
    pub source_coverage: ZoomSourceCoverage,
    /// Full requested square destination; clipping is described separately.
    pub destination: PhysicalRect,
    /// Actual hotspot anchor after an offset flip/clamp. For odd raster sizes,
    /// this is the center pixel selected by the leading-edge convention.
    pub destination_hotspot: PhysicalPoint,
    pub destination_bounds: Option<ZoomDestinationBounds>,
    pub visible_destination: Option<PhysicalRect>,
    pub destination_missing: PhysicalInsets,
    pub placement: [ZoomAxisPlacement; 2],
    pub alignment: ZoomRasterAlignment,
}

/// Integer child placement and a conservative destination mask for presenting
/// one validated zoom source. The child origin is relative to the circular
/// host; `client_coverage` is the part of that host which can show pixels from
/// the sampled monitor after source clipping and native integer placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ZoomRasterRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ZoomPresentationGeometry {
    pub lens: ZoomLensGeometry,
    /// The exact source rectangle passed to MagSetWindowSource.
    pub source: PhysicalRect,
    /// Rounded child position in host-client pixels. The configured zoom
    /// factor is never changed to compensate for this integer placement.
    pub child_origin: PhysicalPoint,
    /// Child extent rounds upward to contain the exact transformed source.
    pub child_size: PhysicalSize,
    /// Error introduced by SetWindowPos' integer coordinate requirement; each
    /// axis is at most half a physical pixel from the desired fractional map.
    pub child_rounding_error: RasterTranslation,
    /// Source-backed pixel-center coverage clipped to the monitor and lens
    /// viewport before the native circular host region is applied.
    pub client_coverage: ZoomRasterRect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ZoomGeometryError {
    PlacementBoundsUnavailable,
    ArithmeticOverflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ZoomPresentationError {
    UnknownSourceCoverage,
    HotspotOutsideSampledMonitor,
    EmptySourceCoverage,
    EmptyDestinationCoverage,
    ArithmeticOverflow,
}

/// Resolve the geometry that the native magnifier can safely present.
///
/// A sampled monitor is the only proof that a desktop source rectangle names
/// readable pixels. Virtual-desktop placement fallback is deliberately not
/// accepted as source coverage. The source rectangle remains the intersection
/// of the cursor-anchored request and that monitor; it is never shifted or
/// stretched to fill the lens. The exact configured transform is preserved,
/// while child dimensions round up and the host mask includes only pixel
/// centers backed by the selected source rectangle. Win32 child placement is integral, so the
/// returned residual records its bounded subpixel alignment error.
pub(crate) fn zoom_presentation_geometry(
    lens: ZoomLensGeometry,
) -> Result<ZoomPresentationGeometry, ZoomPresentationError> {
    let (monitor_bounds, visible_source, missing) = match &lens.source_coverage {
        ZoomSourceCoverage::Unknown => return Err(ZoomPresentationError::UnknownSourceCoverage),
        ZoomSourceCoverage::SampledMonitor {
            bounds,
            visible,
            missing,
            ..
        } => (*bounds, *visible, *missing),
    };
    if lens.hotspot.x < monitor_bounds.left()
        || lens.hotspot.x >= monitor_bounds.right()
        || lens.hotspot.y < monitor_bounds.top()
        || lens.hotspot.y >= monitor_bounds.bottom()
    {
        return Err(ZoomPresentationError::HotspotOutsideSampledMonitor);
    }
    let source = visible_source.ok_or(ZoomPresentationError::EmptySourceCoverage)?;
    let factor = f64::from(lens.alignment.zoom_factor);
    if !factor.is_finite() || factor <= 0.0 {
        return Err(ZoomPresentationError::ArithmeticOverflow);
    }

    let ideal_child_x =
        f64::from(lens.alignment.hotspot_translation.x) + missing.left as f64 * factor;
    let ideal_child_y =
        f64::from(lens.alignment.hotspot_translation.y) + missing.top as f64 * factor;

    // Integer child placement can move a fractional transform by half a
    // pixel. Add only monitor-proven guard pixels on whichever edge needs
    // them. Moving the child with a leading guard preserves the original
    // cursor-to-lens mapping; if a monitor edge prevents a guard, the mask
    // clips that genuinely unavailable destination edge.
    let visible_destination = lens
        .visible_destination
        .ok_or(ZoomPresentationError::EmptyDestinationCoverage)?;
    let local_visible = ZoomRasterRect {
        left: i32::try_from(
            i64::from(visible_destination.left()) - i64::from(lens.destination.left()),
        )
        .map_err(|_| ZoomPresentationError::ArithmeticOverflow)?,
        top: i32::try_from(
            i64::from(visible_destination.top()) - i64::from(lens.destination.top()),
        )
        .map_err(|_| ZoomPresentationError::ArithmeticOverflow)?,
        right: i32::try_from(
            i64::from(visible_destination.right()) - i64::from(lens.destination.left()),
        )
        .map_err(|_| ZoomPresentationError::ArithmeticOverflow)?,
        bottom: i32::try_from(
            i64::from(visible_destination.bottom()) - i64::from(lens.destination.top()),
        )
        .map_err(|_| ZoomPresentationError::ArithmeticOverflow)?,
    };
    let (source_left, source_right, child_x) = guard_source_axis(
        lens.source.left(),
        lens.source.right(),
        source.left(),
        source.right(),
        monitor_bounds.left(),
        monitor_bounds.right(),
        ideal_child_x,
        local_visible.left,
        local_visible.right,
        factor,
    )?;
    let (source_top, source_bottom, child_y) = guard_source_axis(
        lens.source.top(),
        lens.source.bottom(),
        source.top(),
        source.bottom(),
        monitor_bounds.top(),
        monitor_bounds.bottom(),
        ideal_child_y,
        local_visible.top,
        local_visible.bottom,
        factor,
    )?;
    let original_source = source;
    let source = PhysicalRect::new(source_left, source_top, source_right, source_bottom)
        .ok_or(ZoomPresentationError::EmptySourceCoverage)?;

    let exact_width = source.width() as f64 * factor;
    let exact_height = source.height() as f64 * factor;
    let child_width = checked_ceil_extent(exact_width)?;
    let child_height = checked_ceil_extent(exact_height)?;
    let backed_right = checked_ceil_position(f64::from(child_x) + exact_width - 0.5)?;
    let backed_bottom = checked_ceil_position(f64::from(child_y) + exact_height - 0.5)?;

    let mut backed = ZoomRasterRect {
        left: child_x,
        top: child_y,
        right: backed_right,
        bottom: backed_bottom,
    };
    let diameter = lens.destination.width();
    let diameter =
        i32::try_from(diameter).map_err(|_| ZoomPresentationError::ArithmeticOverflow)?;
    backed = intersect_raster_rect(
        backed,
        ZoomRasterRect {
            left: 0,
            top: 0,
            right: diameter,
            bottom: diameter,
        },
    )
    .ok_or(ZoomPresentationError::EmptyDestinationCoverage)?;

    // Destination visibility is clipped to the sampled monitor by the pure
    // placement helper. Convert it to host-local coordinates and intersect it
    // with the mapped source coverage before creating the native window region.
    let destination_origin_x = i64::from(lens.destination.left());
    let destination_origin_y = i64::from(lens.destination.top());
    let monitor_clip = ZoomRasterRect {
        left: i32::try_from(i64::from(visible_destination.left()) - destination_origin_x)
            .map_err(|_| ZoomPresentationError::ArithmeticOverflow)?,
        top: i32::try_from(i64::from(visible_destination.top()) - destination_origin_y)
            .map_err(|_| ZoomPresentationError::ArithmeticOverflow)?,
        right: i32::try_from(i64::from(visible_destination.right()) - destination_origin_x)
            .map_err(|_| ZoomPresentationError::ArithmeticOverflow)?,
        bottom: i32::try_from(i64::from(visible_destination.bottom()) - destination_origin_y)
            .map_err(|_| ZoomPresentationError::ArithmeticOverflow)?,
    };
    backed = intersect_raster_rect(backed, monitor_clip)
        .ok_or(ZoomPresentationError::EmptyDestinationCoverage)?;

    Ok(ZoomPresentationGeometry {
        lens,
        source,
        child_origin: PhysicalPoint::new(child_x, child_y),
        child_size: PhysicalSize::new(i64::from(child_width), i64::from(child_height))
            .ok_or(ZoomPresentationError::ArithmeticOverflow)?,
        child_rounding_error: RasterTranslation {
            x: (f64::from(child_x)
                - (ideal_child_x
                    - (i64::from(original_source.left()) - i64::from(source.left())) as f64
                        * factor)) as f32,
            y: (f64::from(child_y)
                - (ideal_child_y
                    - (i64::from(original_source.top()) - i64::from(source.top())) as f64 * factor))
                as f32,
        },
        client_coverage: backed,
    })
}

fn rounded_i32(value: f64) -> Result<i32, ZoomPresentationError> {
    if !value.is_finite() || value < i32::MIN as f64 || value > i32::MAX as f64 {
        return Err(ZoomPresentationError::ArithmeticOverflow);
    }
    i32::try_from(value.round() as i64).map_err(|_| ZoomPresentationError::ArithmeticOverflow)
}

fn checked_ceil_extent(value: f64) -> Result<i32, ZoomPresentationError> {
    if !value.is_finite() || value <= 0.0 || value.ceil() > i32::MAX as f64 {
        return Err(ZoomPresentationError::ArithmeticOverflow);
    }
    Ok(value.ceil() as i32)
}

fn guard_source_axis(
    requested_start: i32,
    requested_end: i32,
    visible_start: i32,
    visible_end: i32,
    monitor_start: i32,
    monitor_end: i32,
    ideal_child_start: f64,
    visible_destination_start: i32,
    visible_destination_end: i32,
    factor: f64,
) -> Result<(i32, i32, i32), ZoomPresentationError> {
    let mut source_start = i64::from(visible_start);
    let mut source_end = i64::from(visible_end);
    let can_guard_leading = visible_start == requested_start && visible_start > monitor_start;
    if can_guard_leading {
        let guard_needed =
            ((ideal_child_start - f64::from(visible_destination_start) - 0.5) / factor).floor()
                + 1.0;
        let guard = guard_needed
            .max(0.0)
            .min((visible_start - monitor_start) as f64) as i64;
        source_start -= guard;
    }
    let child_origin =
        rounded_i32(ideal_child_start - (i64::from(visible_start) - source_start) as f64 * factor)?;

    let source_extent = source_end - source_start;
    let rightmost_pixel_center = f64::from(visible_destination_end) - 0.5 - f64::from(child_origin);
    let needed_extent = (rightmost_pixel_center / factor).floor() + 1.0;
    if !needed_extent.is_finite() || needed_extent > i64::MAX as f64 {
        return Err(ZoomPresentationError::ArithmeticOverflow);
    }
    if visible_end == requested_end && visible_end < monitor_end {
        let available = i64::from(monitor_end) - source_end;
        let trailing_guard = (needed_extent.max(0.0) as i64 - source_extent)
            .max(0)
            .min(available);
        source_end += trailing_guard;
    }
    Ok((
        i32::try_from(source_start).map_err(|_| ZoomPresentationError::ArithmeticOverflow)?,
        i32::try_from(source_end).map_err(|_| ZoomPresentationError::ArithmeticOverflow)?,
        child_origin,
    ))
}

fn checked_ceil_position(value: f64) -> Result<i32, ZoomPresentationError> {
    if !value.is_finite() || value < i32::MIN as f64 || value > i32::MAX as f64 {
        return Err(ZoomPresentationError::ArithmeticOverflow);
    }
    Ok(value.ceil() as i32)
}

fn intersect_raster_rect(first: ZoomRasterRect, second: ZoomRasterRect) -> Option<ZoomRasterRect> {
    let result = ZoomRasterRect {
        left: first.left.max(second.left),
        top: first.top.max(second.top),
        right: first.right.min(second.right),
        bottom: first.bottom.min(second.bottom),
    };
    (result.right > result.left && result.bottom > result.top).then_some(result)
}

/// Calculate source and destination geometry using only this live sample's
/// physical cursor point. Source dimensions use `ceil(diameter / factor)` so
/// every destination pixel has input coverage. The native transform remains
/// the exact normalized preference, not a ratio reconstructed from the rounded
/// source size.
///
/// Rectangles use exclusive right/bottom edges. Their leading edge is placed
/// `floor(extent / 2)` pixels before the hotspot; the hotspot therefore maps
/// to raster index `floor(extent / 2)`. Odd extents select their single middle
/// pixel and even extents select the lower/right of their two middle pixels.
/// Source clipping is reported only against the sampled monitor. A virtual
/// desktop rectangle is not proof of coverage across monitor gaps.
pub(crate) fn zoom_lens_geometry(
    sample: &CoordinateSample,
    preferences: ZoomPreferences,
) -> Result<ZoomLensGeometry, ZoomGeometryError> {
    let preferences = preferences.normalized();
    let hotspot = sample.desktop_point;
    let diameter = i64::from(preferences.diameter);
    // Keep the division explicit in f64 so non-integral preferences round
    // upward deterministically while the native scale remains an f32 value.
    let rounded_source_extent = (diameter as f64 / f64::from(preferences.zoom_factor)).ceil();
    if !rounded_source_extent.is_finite()
        || rounded_source_extent <= 0.0
        || rounded_source_extent > i64::MAX as f64
    {
        return Err(ZoomGeometryError::ArithmeticOverflow);
    }
    let source_extent = rounded_source_extent as i64;
    let source = rect_from_center(
        i64::from(hotspot.x),
        i64::from(hotspot.y),
        source_extent,
        source_extent,
    )
    .ok_or(ZoomGeometryError::ArithmeticOverflow)?;

    let source_coverage = if let Some(monitor) = sample.monitor.as_ref() {
        ZoomSourceCoverage::SampledMonitor {
            id: monitor.id.clone(),
            bounds: monitor.bounds,
            visible: intersect_rect(source, monitor.bounds),
            missing: rectangle_missing(source, monitor.bounds),
        }
    } else {
        ZoomSourceCoverage::Unknown
    };

    let (bounds, bounds_kind) = if let Some(monitor) = sample.monitor.as_ref() {
        (
            Some(monitor.bounds),
            Some(ZoomDestinationBounds::SampledMonitor),
        )
    } else if let Some(virtual_bounds) = sample.virtual_desktop_bounds {
        (
            Some(virtual_bounds),
            Some(ZoomDestinationBounds::VirtualDesktopFallback),
        )
    } else {
        (None, None)
    };

    let (destination, destination_hotspot, placement) = match preferences.mode {
        ZoomMode::Centered => {
            let rect = rect_from_center(
                i64::from(hotspot.x),
                i64::from(hotspot.y),
                diameter,
                diameter,
            )
            .ok_or(ZoomGeometryError::ArithmeticOverflow)?;
            (
                rect,
                hotspot,
                [ZoomAxisPlacement::Centered, ZoomAxisPlacement::Centered],
            )
        }
        ZoomMode::Offset => {
            let bounds = bounds.ok_or(ZoomGeometryError::PlacementBoundsUnavailable)?;
            let (left, x_anchor, x_placement) = place_offset_axis(
                i64::from(hotspot.x),
                i64::from(preferences.destination_offset.x),
                i64::from(bounds.left()),
                i64::from(bounds.right()),
                diameter,
            )?;
            let (top, y_anchor, y_placement) = place_offset_axis(
                i64::from(hotspot.y),
                i64::from(preferences.destination_offset.y),
                i64::from(bounds.top()),
                i64::from(bounds.bottom()),
                diameter,
            )?;
            let rect = rect_from_leading(left, top, diameter, diameter)
                .ok_or(ZoomGeometryError::ArithmeticOverflow)?;
            let anchor = PhysicalPoint::new(
                i32::try_from(x_anchor).map_err(|_| ZoomGeometryError::ArithmeticOverflow)?,
                i32::try_from(y_anchor).map_err(|_| ZoomGeometryError::ArithmeticOverflow)?,
            );
            (rect, anchor, [x_placement, y_placement])
        }
    };

    let source_anchor =
        u32::try_from(source_extent / 2).map_err(|_| ZoomGeometryError::ArithmeticOverflow)?;
    let destination_anchor =
        u32::try_from(diameter / 2).map_err(|_| ZoomGeometryError::ArithmeticOverflow)?;
    let zoom_factor = preferences.zoom_factor;
    Ok(ZoomLensGeometry {
        source,
        hotspot,
        source_coverage,
        destination,
        destination_hotspot,
        destination_bounds: bounds_kind,
        visible_destination: bounds.and_then(|bounds| intersect_rect(destination, bounds)),
        destination_missing: bounds
            .map(|bounds| rectangle_missing(destination, bounds))
            .unwrap_or_default(),
        placement,
        alignment: ZoomRasterAlignment {
            source_hotspot: RasterPoint {
                x: source_anchor,
                y: source_anchor,
            },
            destination_hotspot: RasterPoint {
                x: destination_anchor,
                y: destination_anchor,
            },
            zoom_factor,
            hotspot_translation: RasterTranslation {
                x: destination_anchor as f32 - source_anchor as f32 * zoom_factor,
                y: destination_anchor as f32 - source_anchor as f32 * zoom_factor,
            },
        },
    })
}

fn rect_from_center(center_x: i64, center_y: i64, width: i64, height: i64) -> Option<PhysicalRect> {
    let left = center_x.checked_sub(width / 2)?;
    let top = center_y.checked_sub(height / 2)?;
    rect_from_leading(left, top, width, height)
}

fn rect_from_leading(left: i64, top: i64, width: i64, height: i64) -> Option<PhysicalRect> {
    let right = left.checked_add(width)?;
    let bottom = top.checked_add(height)?;
    PhysicalRect::new(
        i32::try_from(left).ok()?,
        i32::try_from(top).ok()?,
        i32::try_from(right).ok()?,
        i32::try_from(bottom).ok()?,
    )
}

fn intersect_rect(first: PhysicalRect, second: PhysicalRect) -> Option<PhysicalRect> {
    PhysicalRect::new(
        first.left().max(second.left()),
        first.top().max(second.top()),
        first.right().min(second.right()),
        first.bottom().min(second.bottom()),
    )
}

fn rectangle_missing(rect: PhysicalRect, bounds: PhysicalRect) -> PhysicalInsets {
    let (left, right) = interval_missing(
        i64::from(rect.left()),
        i64::from(rect.right()),
        i64::from(bounds.left()),
        i64::from(bounds.right()),
    );
    let (top, bottom) = interval_missing(
        i64::from(rect.top()),
        i64::from(rect.bottom()),
        i64::from(bounds.top()),
        i64::from(bounds.bottom()),
    );
    PhysicalInsets {
        left,
        top,
        right,
        bottom,
    }
}

fn interval_missing(start: i64, end: i64, bound_start: i64, bound_end: i64) -> (i64, i64) {
    let extent = end - start;
    if end <= bound_start {
        (extent, 0)
    } else if start >= bound_end {
        (0, extent)
    } else {
        ((bound_start - start).max(0), (end - bound_end).max(0))
    }
}

fn place_offset_axis(
    cursor: i64,
    offset: i64,
    bound_start: i64,
    bound_end: i64,
    extent: i64,
) -> Result<(i64, i64, ZoomAxisPlacement), ZoomGeometryError> {
    let requested_center = cursor
        .checked_add(offset)
        .ok_or(ZoomGeometryError::ArithmeticOverflow)?;
    if let Some(leading) = axis_leading_if_fits(requested_center, bound_start, bound_end, extent) {
        return Ok((
            leading,
            requested_center,
            ZoomAxisPlacement::RequestedOffset,
        ));
    }

    let mirrored_center = cursor
        .checked_sub(offset)
        .ok_or(ZoomGeometryError::ArithmeticOverflow)?;
    if offset != 0 {
        if let Some(leading) = axis_leading_if_fits(mirrored_center, bound_start, bound_end, extent)
        {
            return Ok((leading, mirrored_center, ZoomAxisPlacement::MirroredOffset));
        }
    }

    let requested_leading = requested_center
        .checked_sub(extent / 2)
        .ok_or(ZoomGeometryError::ArithmeticOverflow)?;
    let available_extent = bound_end
        .checked_sub(bound_start)
        .ok_or(ZoomGeometryError::ArithmeticOverflow)?;
    let max_leading = bound_end
        .checked_sub(extent)
        .ok_or(ZoomGeometryError::ArithmeticOverflow)?
        .max(bound_start);
    let leading = requested_leading.clamp(bound_start, max_leading);
    let placement = if available_extent < extent {
        ZoomAxisPlacement::LeadingEdgeAnchored
    } else {
        ZoomAxisPlacement::Clamped
    };
    let anchor = leading
        .checked_add(extent / 2)
        .ok_or(ZoomGeometryError::ArithmeticOverflow)?;
    Ok((leading, anchor, placement))
}

fn axis_leading_if_fits(center: i64, bound_start: i64, bound_end: i64, extent: i64) -> Option<i64> {
    let leading = center.checked_sub(extent / 2)?;
    let trailing = leading.checked_add(extent)?;
    (leading >= bound_start && trailing <= bound_end).then_some(leading)
}

/// Identity color matrix used by the ordinary zoom lens. Magnification uses a
/// 5x5 row-major affine RGB transform; keeping this pure lets the native zoom
/// path share the same explicit no-recolor contract as the tests.
pub(crate) const fn zoom_identity_color_matrix() -> [f32; 25] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, // output red = input red
        0.0, 1.0, 0.0, 0.0, 0.0, // output green = input green
        0.0, 0.0, 1.0, 0.0, 0.0, // output blue = input blue
        0.0, 0.0, 0.0, 1.0, 0.0, // preserve alpha
        0.0, 0.0, 0.0, 0.0, 1.0, // no additive color
    ]
}

/// Five-by-five Magnification color matrix and equivalent RGB pixel blend.
/// Windows applies input channel vectors across matrix rows, so the additive
/// offset belongs in the final row (indices 20..23). Alpha is passed through.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HaloColorTransform {
    slope: f32,
    additive: f32,
}

impl HaloColorTransform {
    pub(crate) fn from_strength(strength: f32) -> Self {
        let strength = if strength.is_finite() {
            strength.clamp(0.0, 1.0)
        } else {
            HaloPreferences::default().inversion_strength
        };
        Self {
            slope: 1.0 - 2.0 * strength,
            additive: strength,
        }
    }

    pub(crate) fn matrix(self) -> [f32; 25] {
        [
            self.slope,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            self.slope,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            self.slope,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            self.additive,
            self.additive,
            self.additive,
            0.0,
            1.0,
        ]
    }

    #[cfg(test)]
    pub(crate) fn apply_rgba(self, pixel: [u8; 4]) -> [u8; 4] {
        let mut result = pixel;
        for channel in &mut result[..3] {
            let normalized = f32::from(*channel) / 255.0;
            *channel = ((normalized * self.slope + self.additive) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
        result
    }
}

/// Return the exact square source and placement rectangle for a 1x circular
/// halo. The window spans `[hotspot-radius, hotspot+radius)` on each axis;
/// coordinates are never shifted inward to accommodate a monitor edge.
pub(crate) fn halo_geometry(cursor: PhysicalPoint, radius: i32) -> Option<HaloGeometry> {
    let radius = radius.clamp(8, 256);
    let diameter = radius.checked_mul(2)?;
    let left = cursor.x.checked_sub(radius)?;
    let top = cursor.y.checked_sub(radius)?;
    let right = left.checked_add(diameter)?;
    let bottom = top.checked_add(diameter)?;
    Some(HaloGeometry {
        origin: PhysicalPoint::new(left, top),
        source: PhysicalRect::new(left, top, right, bottom)?,
        diameter,
    })
}

/// Build the optional crisp outline as a separate transparent-ring bitmap.
/// Pixel centers are measured from the same physical hotspot as the native
/// magnifier. At the minimum radius, rendered thickness is capped at radius-1
/// so at least the center pixels remain transparent even when the preference
/// requests the maximum eight-pixel border.
pub(crate) fn halo_outline_bitmap(preferences: HaloPreferences) -> Option<RgbaImage> {
    let preferences = preferences.normalized();
    if !preferences.outline_enabled {
        return None;
    }

    annulus_bitmap(
        preferences.radius.checked_mul(2)? as u32,
        preferences.outline_thickness,
        Rgba([
            preferences.outline_color.red,
            preferences.outline_color.green,
            preferences.outline_color.blue,
            255,
        ]),
    )
}

/// Build the optional transparent zoom-lens border in physical pixels. Like
/// the halo border it is independent of the magnifier host, which lets native
/// geometry and overlay exclusion own the ring separately.
pub(crate) fn zoom_outline_bitmap(preferences: ZoomPreferences) -> Option<RgbaImage> {
    let preferences = preferences.normalized();
    if !preferences.outline_enabled {
        return None;
    }

    annulus_bitmap(
        preferences.diameter as u32,
        preferences.outline_thickness,
        Rgba([
            preferences.outline_color.red,
            preferences.outline_color.green,
            preferences.outline_color.blue,
            255,
        ]),
    )
}

/// Rasterize a physical-pixel circular outline with a transparent center.
/// Effective thickness is capped so the innermost pixel centers remain clear,
/// including the minimum configured diameter with maximum border thickness.
fn annulus_bitmap(diameter: u32, requested_thickness: i32, color: Rgba<u8>) -> Option<RgbaImage> {
    let radius = diameter as f32 / 2.0;
    let maximum_hollow_thickness = i32::try_from(diameter / 2).ok()?.checked_sub(1)?;
    if maximum_hollow_thickness < 1 {
        return None;
    }
    let thickness = requested_thickness.max(1).min(maximum_hollow_thickness) as f32;
    let outer_squared = radius * radius;
    let inner_radius = radius - thickness;
    let inner_squared = inner_radius * inner_radius;
    let mut image = RgbaImage::new(diameter, diameter);
    for y in 0..diameter {
        let dy = y as f32 + 0.5 - radius;
        for x in 0..diameter {
            let dx = x as f32 + 0.5 - radius;
            let distance_squared = dx * dx + dy * dy;
            if distance_squared <= outer_squared && distance_squared > inner_squared {
                image.put_pixel(x, y, color);
            }
        }
    }
    Some(image)
}

/// Build a contrasting ring for the degraded halo mode. Unlike the optional
/// user outline, this bitmap always has adjacent opaque black and white bands
/// so it remains visible on both light and dark desktop content. It never
/// fills the center: at the minimum radius, effective thickness is capped at
/// radius minus one even when the saved optional outline asks for eight pixels.
pub(crate) fn halo_fallback_bitmap(preferences: HaloPreferences) -> Option<RgbaImage> {
    let preferences = preferences.normalized();
    let radius = preferences.radius;
    let diameter = radius.checked_mul(2)? as u32;
    let requested_thickness = if preferences.outline_enabled {
        preferences.outline_thickness.max(4)
    } else {
        4
    };
    let thickness = requested_thickness.min(radius - 1);
    let outer_band = (thickness / 2).max(1);
    let outer_inner_radius = (radius - outer_band) as f32;
    let inner_radius = (radius - thickness) as f32;
    let outer_inner_squared = outer_inner_radius * outer_inner_radius;
    let inner_squared = inner_radius * inner_radius;
    let outer_radius = radius as f32;
    let black = Rgba([0, 0, 0, 255]);
    let white = Rgba([255, 255, 255, 255]);
    let mut image = RgbaImage::new(diameter, diameter);
    for y in 0..diameter {
        let dy = y as f32 + 0.5 - outer_radius;
        for x in 0..diameter {
            let dx = x as f32 + 0.5 - outer_radius;
            let distance_squared = dx * dx + dy * dy;
            if distance_squared > outer_inner_squared
                && distance_squared <= outer_radius * outer_radius
            {
                image.put_pixel(x, y, black);
            } else if distance_squared > inner_squared && distance_squared <= outer_inner_squared {
                image.put_pixel(x, y, white);
            }
        }
    }
    Some(image)
}

fn crosshair_geometry(preferences: &CrosshairPreferences) -> CrosshairGeometry {
    // Bound raw/deserialized and directly constructed preferences before any
    // sizing arithmetic so malformed values cannot request a huge bitmap.
    let arm = preferences.arm_length.clamp(2, 256) as u32;
    let thickness = preferences.thickness.clamp(1, 16) as u32;
    let gap = preferences.center_gap.clamp(0, 128) as u32;
    let outline = u32::from(preferences.high_contrast_outline);
    let outline_expansion = outline
        .checked_mul(2)
        .expect("bounded crosshair outline expansion fits u32");
    let arm_radius = gap
        .checked_add(arm)
        .and_then(|extent| extent.checked_sub(1))
        .and_then(|extent| extent.checked_add(outline_expansion))
        .expect("bounded crosshair arm extent fits u32");
    let thickness_radius = (thickness / 2)
        .checked_add(outline)
        .expect("bounded crosshair thickness extent fits u32");
    let radius = arm_radius.max(thickness_radius);
    let extent = radius
        .checked_mul(2)
        .and_then(|diameter| diameter.checked_add(1))
        .expect("bounded crosshair bitmap dimensions fit u32");
    let width = extent;
    let height = extent;
    // Odd dimensions keep this exact center pixel aligned with the hotspot
    // when native.rs places the image at hotspot - (width / 2, height / 2).
    let center_x = width / 2;
    let center_y = height / 2;
    let inset = gap
        .checked_add(outline)
        .expect("bounded crosshair center inset fits u32");
    let negative_end_x = center_x
        .checked_sub(inset)
        .expect("crosshair negative arm remains inside its bitmap");
    let negative_start_x = negative_end_x
        .checked_sub(arm - 1)
        .expect("crosshair negative arm start remains inside its bitmap");
    let positive_start_x = center_x
        .checked_add(inset)
        .expect("crosshair positive arm remains inside its bitmap");
    let negative_end_y = center_y
        .checked_sub(inset)
        .expect("crosshair negative arm remains inside its bitmap");
    let negative_start_y = negative_end_y
        .checked_sub(arm - 1)
        .expect("crosshair negative arm start remains inside its bitmap");
    let positive_start_y = center_y
        .checked_add(inset)
        .expect("crosshair positive arm remains inside its bitmap");
    let horizontal_y = center_y
        .checked_sub(thickness / 2)
        .expect("crosshair horizontal stroke remains inside its bitmap");
    let vertical_x = center_x
        .checked_sub(thickness / 2)
        .expect("crosshair vertical stroke remains inside its bitmap");
    let colored_strokes = [
        PixelRect {
            x: negative_start_x,
            y: horizontal_y,
            width: arm,
            height: thickness,
        },
        PixelRect {
            x: positive_start_x,
            y: horizontal_y,
            width: arm,
            height: thickness,
        },
        PixelRect {
            x: vertical_x,
            y: negative_start_y,
            width: thickness,
            height: arm,
        },
        PixelRect {
            x: vertical_x,
            y: positive_start_y,
            width: thickness,
            height: arm,
        },
    ];
    let outline_strokes = preferences
        .high_contrast_outline
        .then(|| colored_strokes.map(|stroke| stroke.expanded(outline)));
    CrosshairGeometry {
        width,
        height,
        colored_strokes,
        outline_strokes,
    }
}

pub(crate) fn crosshair_bitmap(preferences: &CrosshairPreferences) -> RgbaImage {
    let geometry = crosshair_geometry(preferences);
    let mut image = RgbaImage::new(geometry.width, geometry.height);
    let alpha = opacity_alpha(preferences.opacity);
    if let Some(outlines) = geometry.outline_strokes {
        let outline = contrasting_color(preferences.color);
        for stroke in outlines {
            fill_rect(&mut image, stroke, color(outline, alpha));
        }
    }
    for stroke in geometry.colored_strokes {
        fill_rect(&mut image, stroke, color(preferences.color, alpha));
    }
    image
}

pub(crate) fn guide_geometry(
    orientation: GuideOrientation,
    desktop: PhysicalRect,
    cursor: PhysicalPoint,
    preferences: &CrosshairPreferences,
) -> Option<GuideGeometry> {
    let thickness = preferences.thickness.clamp(1, 16) as u32;
    let outline_width = u32::from(preferences.high_contrast_outline);
    let total = thickness + outline_width * 2;
    match orientation {
        GuideOrientation::Horizontal => {
            let width = u32::try_from(desktop.width()).ok()?;
            let y_offset = i32::try_from(total / 2).ok()?;
            let y = cursor.y.checked_sub(y_offset)?;
            Some(GuideGeometry {
                origin: PhysicalPoint::new(desktop.left(), y),
                width,
                height: total,
                colored_stroke: PixelRect {
                    x: 0,
                    y: outline_width,
                    width,
                    height: thickness,
                },
                outline: preferences.high_contrast_outline.then_some(PixelRect {
                    x: 0,
                    y: 0,
                    width,
                    height: total,
                }),
            })
        }
        GuideOrientation::Vertical => {
            let height = u32::try_from(desktop.height()).ok()?;
            let x_offset = i32::try_from(total / 2).ok()?;
            let x = cursor.x.checked_sub(x_offset)?;
            Some(GuideGeometry {
                origin: PhysicalPoint::new(x, desktop.top()),
                width: total,
                height,
                colored_stroke: PixelRect {
                    x: outline_width,
                    y: 0,
                    width: thickness,
                    height,
                },
                outline: preferences.high_contrast_outline.then_some(PixelRect {
                    x: 0,
                    y: 0,
                    width: total,
                    height,
                }),
            })
        }
    }
}

pub(crate) fn guide_bitmap(
    geometry: GuideGeometry,
    preferences: &CrosshairPreferences,
) -> RgbaImage {
    let mut image = RgbaImage::new(geometry.width, geometry.height);
    let alpha = opacity_alpha(preferences.opacity);
    if let Some(outline) = geometry.outline {
        fill_rect(
            &mut image,
            outline,
            color(contrasting_color(preferences.color), alpha),
        );
    }
    fill_rect(
        &mut image,
        geometry.colored_stroke,
        color(preferences.color, alpha),
    );
    image
}

fn fill_rect(image: &mut RgbaImage, rect: PixelRect, value: Rgba<u8>) {
    let right = rect.x.saturating_add(rect.width).min(image.width());
    let bottom = rect.y.saturating_add(rect.height).min(image.height());
    for y in rect.y.min(bottom)..bottom {
        for x in rect.x.min(right)..right {
            image.put_pixel(x, y, value);
        }
    }
}

fn color(color: CrosshairColor, alpha: u8) -> Rgba<u8> {
    Rgba([color.red, color.green, color.blue, alpha])
}

fn opacity_alpha(opacity: f32) -> u8 {
    let opacity = if opacity.is_finite() {
        opacity.clamp(0.1, 1.0)
    } else {
        1.0
    };
    (opacity * 255.0).round() as u8
}

fn contrasting_color(color: CrosshairColor) -> CrosshairColor {
    let luminance =
        u32::from(color.red) * 299 + u32::from(color.green) * 587 + u32::from(color.blue) * 114;
    if luminance >= 128_000 {
        CrosshairColor::new(0, 0, 0)
    } else {
        CrosshairColor::new(255, 255, 255)
    }
}

pub(crate) fn hud_lines(frame: &CoordinateRenderFrame) -> Vec<String> {
    let Some(sample) = frame.displayed_sample.as_ref() else {
        let mut lines = vec!["Coordinates unavailable".to_string()];
        if let Some(error) = frame.sample_error.as_deref() {
            lines.push(format!("Sampling: {}", concise_error(error)));
        }
        if frame.preferences.hud_detail == HudDetail::Detailed {
            lines.push("Help: mouse help".into());
        }
        return lines;
    };

    let selected_space = frame.preferences.space;
    let selected = format_coordinate_value(sample, selected_space);
    if frame.preferences.hud_detail == HudDetail::Compact {
        let mut lines = vec![format!("{}: {selected}", space_name(selected_space))];
        if let Some(monitor) = sample.monitor.as_ref() {
            lines.push(format!("Monitor: {}", monitor.id.0));
        }
        if let Some(error) = frame.sample_error.as_deref() {
            lines.push(format!("Sampling: {}", concise_error(error)));
        }
        return lines;
    }

    let mut lines = vec!["Coordinate Inspector".into()];
    lines.push(format!(
        "Selected: {} {selected}",
        space_name(selected_space)
    ));
    lines.push(format!(
        "Desktop: {},{}",
        sample.desktop_point.x, sample.desktop_point.y
    ));
    if let Some(monitor) = sample.monitor.as_ref() {
        lines.push(format!("Monitor: {}", monitor.id.0));
        lines.push(format!(
            "Monitor origin: {},{}",
            monitor.bounds.left(),
            monitor.bounds.top()
        ));
        lines.push(format!(
            "Work area: {},{} .. {},{}",
            monitor.work_area.left(),
            monitor.work_area.top(),
            monitor.work_area.right(),
            monitor.work_area.bottom()
        ));
        if let Some((dpi_x, dpi_y)) = monitor.effective_dpi {
            lines.push(format!("Effective DPI: {dpi_x}x{dpi_y}"));
        } else {
            lines.push("Effective DPI: unavailable".into());
        }
    } else {
        lines.push("Monitor: unavailable".into());
    }
    if let Some(client) = sample.foreground_client {
        lines.push(format!(
            "Client origin: {},{}",
            client.origin.x, client.origin.y
        ));
        if let Some(bounds) = client.bounds {
            lines.push(format!(
                "Client bounds: {},{} .. {},{}",
                bounds.left(),
                bounds.top(),
                bounds.right(),
                bounds.bottom()
            ));
        }
    } else {
        lines.push("Foreground client: unavailable".into());
    }
    if let Some(bounds) = sample.virtual_desktop_bounds {
        lines.push(format!(
            "Virtual desktop: {},{} .. {},{}",
            bounds.left(),
            bounds.top(),
            bounds.right(),
            bounds.bottom()
        ));
    } else {
        lines.push("Virtual desktop: unavailable".into());
    }
    lines.push(format!(
        "Freeze: {}",
        if frame.runtime_state.is_frozen() {
            "on"
        } else {
            "off"
        }
    ));
    if let Some(copied) = frame.runtime_state.last_successful_copy() {
        lines.push(format!(
            "Last copy: {} ({})",
            copied.text,
            space_name(copied.space)
        ));
    } else {
        lines.push("Last copy: none".into());
    }
    if let Some(error) = frame.sample_error.as_deref() {
        lines.push(format!("Sampling: {}", concise_error(error)));
    }
    lines.push("Help: mouse help".into());
    lines
}

pub(crate) fn hud_font_size(effective_dpi_x: Option<u32>) -> u32 {
    let dpi = effective_dpi_x.filter(|dpi| *dpi > 0).unwrap_or(96);
    (14_u32.saturating_mul(dpi).saturating_add(48) / 96).clamp(12, 32)
}

pub(crate) fn hud_dimensions(lines: &[String], font_size: u32) -> (u32, u32) {
    let longest = lines
        .iter()
        .map(|line| line.chars().count().min(96) as u32)
        .max()
        .unwrap_or(1);
    let approximate_char_width = (font_size * 3 + 2) / 5;
    let width = (longest
        .saturating_mul(approximate_char_width)
        .saturating_add(24))
    .max(220);
    let line_height = font_size.saturating_add(5);
    let height = (lines.len() as u32)
        .saturating_mul(line_height)
        .saturating_add(18)
        .max(36);
    (width, height)
}

pub(crate) fn hud_layout(
    lines: &[String],
    preferred_font_size: u32,
    work_area: Option<PhysicalSize>,
) -> (u32, u32, u32) {
    let mut font_size = preferred_font_size.clamp(12, 32);
    let max_width = work_area.map(|area| u32::try_from(area.width()).unwrap_or(u32::MAX).max(1));
    let max_height = work_area.map(|area| u32::try_from(area.height()).unwrap_or(u32::MAX).max(1));

    loop {
        let (width, height) = hud_dimensions(lines, font_size);
        let fits_width = max_width.is_none_or(|limit| width <= limit);
        let fits_height = max_height.is_none_or(|limit| height <= limit);
        if (fits_width && fits_height) || font_size == 12 {
            return (
                font_size,
                max_width.map_or(width, |limit| width.min(limit)),
                max_height.map_or(height, |limit| height.min(limit)),
            );
        }
        font_size -= 1;
    }
}

fn format_coordinate_value(sample: &CoordinateSample, space: CoordinateSpace) -> String {
    match sample.point_in(space) {
        Ok(point) => format!("{},{}", point.x, point.y),
        Err(CoordinateUnavailable::MonitorUnavailable) => "unavailable".into(),
        Err(CoordinateUnavailable::ForegroundClientUnavailable) => "unavailable".into(),
        Err(CoordinateUnavailable::ArithmeticOverflow) => "unavailable (range overflow)".into(),
    }
}

fn space_name(space: CoordinateSpace) -> &'static str {
    match space {
        CoordinateSpace::Desktop => "Desktop",
        CoordinateSpace::Monitor => "Monitor",
        CoordinateSpace::ForegroundClient => "Foreground client",
    }
}

fn concise_error(error: &str) -> String {
    let mut concise: String = error.chars().take(72).collect();
    if error.chars().count() > 72 {
        concise.push_str("...");
    }
    concise
}

#[cfg(test)]
mod tests {
    use super::{
        CrosshairGeometry, GuideOrientation, HaloColorTransform, PhysicalInsets, ZoomAxisPlacement,
        ZoomDestinationBounds, ZoomGeometryError, ZoomPresentationError, ZoomSourceCoverage,
        crosshair_bitmap, crosshair_geometry, guide_geometry, halo_fallback_bitmap, halo_geometry,
        halo_outline_bitmap, zoom_identity_color_matrix, zoom_lens_geometry, zoom_outline_bitmap,
        zoom_presentation_geometry,
    };
    use crate::coordinate_tool::controller::CoordinateRenderFrame;
    use crate::coordinate_tool::model::{
        CoordinateSample, CoordinateSpace, CoordinateToolRuntimeState, ForegroundClientGeometry,
        MonitorGeometry, MonitorId,
    };
    use crate::coordinate_tool::model::{PhysicalPoint, PhysicalRect, PhysicalSize};
    use crate::coordinate_tool::settings::{
        CoordinateOffset, CrosshairColor, CrosshairPreferences, HaloPreferences, ZoomMode,
        ZoomPreferences,
    };

    #[test]
    fn halo_blend_preserves_input_full_inversion_and_expected_partial_rgb_values() {
        let unchanged = HaloColorTransform::from_strength(0.0);
        assert_eq!(unchanged.apply_rgba([0, 255, 128, 77]), [0, 255, 128, 77]);

        let partial = HaloColorTransform::from_strength(0.4);
        assert_eq!(partial.apply_rgba([0, 0, 0, 255]), [102, 102, 102, 255]);
        assert_eq!(
            partial.apply_rgba([255, 255, 255, 255]),
            [153, 153, 153, 255]
        );
        assert_eq!(partial.apply_rgba([128, 128, 128, 64]), [128, 128, 128, 64]);
        assert_eq!(partial.apply_rgba([255, 0, 0, 19]), [153, 102, 102, 19]);
        assert_eq!(partial.apply_rgba([17, 93, 241, 201]), [105, 121, 150, 201]);

        let full = HaloColorTransform::from_strength(1.0);
        assert_eq!(full.apply_rgba([0, 127, 255, 33]), [255, 128, 0, 33]);
    }

    #[test]
    fn halo_native_matrix_uses_rgb_slopes_last_row_offsets_and_unchanged_alpha() {
        let matrix = HaloColorTransform::from_strength(0.4).matrix();
        assert!((matrix[0] - 0.2).abs() < f32::EPSILON);
        assert!((matrix[6] - 0.2).abs() < f32::EPSILON);
        assert!((matrix[12] - 0.2).abs() < f32::EPSILON);
        assert_eq!(matrix[18], 1.0);
        assert_eq!(&matrix[20..23], &[0.4, 0.4, 0.4]);
        assert_eq!(matrix[24], 1.0);
        for index in [1, 2, 3, 4, 5, 7, 8, 9, 10, 11, 13, 14, 15, 16, 17, 19, 23] {
            assert_eq!(matrix[index], 0.0);
        }
        assert_eq!(HaloColorTransform::from_strength(0.0).matrix()[0], 1.0);
        assert_eq!(HaloColorTransform::from_strength(1.0).matrix()[0], -1.0);
        assert_eq!(HaloColorTransform::from_strength(1.0).matrix()[20], 1.0);
    }

    #[test]
    fn halo_geometry_centers_signed_source_and_destination_on_the_hotspot() {
        let geometry = halo_geometry(PhysicalPoint::new(-1920, 250), 60).unwrap();
        assert_eq!(geometry.origin, PhysicalPoint::new(-1980, 190));
        assert_eq!(
            geometry.source,
            PhysicalRect::new(-1980, 190, -1860, 310).unwrap()
        );
        assert_eq!(geometry.diameter, 120);

        assert!(halo_geometry(PhysicalPoint::new(i32::MIN, 0), 60).is_none());
        assert!(halo_geometry(PhysicalPoint::new(i32::MAX, 0), 60).is_none());
        assert_eq!(
            halo_geometry(PhysicalPoint::new(10, 20), i32::MIN)
                .unwrap()
                .diameter,
            16
        );
        assert_eq!(
            halo_geometry(PhysicalPoint::new(10, 20), i32::MAX)
                .unwrap()
                .diameter,
            512
        );
    }

    #[test]
    fn zoom_default_source_is_cursor_centered_and_destination_mode_does_not_change_it() {
        let sample = zoom_sample(
            PhysicalPoint::new(500, 400),
            Some(PhysicalRect::new(0, 0, 1920, 1080).unwrap()),
            Some(PhysicalRect::new(0, 0, 1920, 1080).unwrap()),
        );
        let offset = zoom_lens_geometry(&sample, ZoomPreferences::default()).unwrap();
        assert_eq!(
            offset.source,
            PhysicalRect::new(460, 360, 540, 440).unwrap()
        );
        assert_eq!(offset.hotspot, sample.desktop_point);
        assert_eq!(
            offset.destination,
            PhysicalRect::new(540, 400, 700, 560).unwrap()
        );
        assert_eq!(offset.destination_hotspot, PhysicalPoint::new(620, 480));
        assert_eq!(offset.alignment.source_hotspot.x, 40);
        assert_eq!(offset.alignment.destination_hotspot.x, 80);
        assert_eq!(offset.alignment.zoom_factor, 2.0);

        let centered = zoom_lens_geometry(
            &sample,
            ZoomPreferences {
                mode: ZoomMode::Centered,
                ..ZoomPreferences::default()
            },
        )
        .unwrap();
        assert_eq!(centered.source, offset.source);
        assert_eq!(
            centered.destination,
            PhysicalRect::new(420, 320, 580, 480).unwrap()
        );
        assert_eq!(centered.destination_hotspot, sample.desktop_point);
        assert_eq!(centered.placement, [ZoomAxisPlacement::Centered; 2]);
    }

    #[test]
    fn zoom_color_transform_is_identity_for_rgb_and_alpha() {
        let matrix = zoom_identity_color_matrix();
        for row in 0..5 {
            for column in 0..5 {
                assert_eq!(
                    matrix[row * 5 + column],
                    if row == column { 1.0 } else { 0.0 }
                );
            }
        }
    }

    #[test]
    fn zoom_source_coverage_uses_ceil_and_keeps_exact_scale_for_odd_and_even_rasters() {
        let sample = zoom_sample(
            PhysicalPoint::new(1000, 900),
            Some(PhysicalRect::new(0, 0, 2000, 1800).unwrap()),
            None,
        );
        for (diameter, factor, expected_source_extent, expected_translation) in [
            (160, 1.25, 128_i64, 0.0_f32),
            (160, 2.0, 80, 0.0),
            (160, 4.0, 40, 0.0),
            (161, 1.25, 129, 0.0),
            (161, 2.0, 81, 0.0),
            (161, 4.0, 41, 0.0),
            (163, 1.7, 96, -0.6),
            (163, 4.0, 41, 1.0),
            (166, 4.0, 42, -1.0),
        ] {
            let geometry = zoom_lens_geometry(
                &sample,
                ZoomPreferences {
                    diameter,
                    zoom_factor: factor,
                    mode: ZoomMode::Centered,
                    ..ZoomPreferences::default()
                },
            )
            .unwrap();
            assert_eq!(geometry.alignment.zoom_factor, factor);
            assert_eq!(geometry.source.width(), expected_source_extent);
            assert_eq!(geometry.source.height(), expected_source_extent);
            let source_leading_extent =
                i64::from(sample.desktop_point.x) - expected_source_extent / 2;
            assert_eq!(i64::from(geometry.source.left()), source_leading_extent);
            assert_eq!(
                i64::from(geometry.source.right()),
                source_leading_extent + expected_source_extent
            );
            assert_eq!(
                geometry.alignment.source_hotspot.x,
                (expected_source_extent / 2) as u32
            );
            assert_eq!(
                geometry.alignment.destination_hotspot.x,
                (diameter / 2) as u32
            );
            assert!(
                (geometry.alignment.hotspot_translation.x - expected_translation).abs() < 0.0001,
                "diameter={diameter}, factor={factor}"
            );
            assert_eq!(
                geometry.alignment.hotspot_translation.y,
                geometry.alignment.hotspot_translation.x
            );
        }
    }

    #[test]
    fn zoom_destination_mirrors_then_clamps_without_moving_the_source() {
        let sample = zoom_sample(
            PhysicalPoint::new(930, 400),
            Some(PhysicalRect::new(0, 0, 1000, 800).unwrap()),
            None,
        );
        let preferences = ZoomPreferences {
            destination_offset: CoordinateOffset::new(100, 0),
            ..ZoomPreferences::default()
        };
        let mirrored = zoom_lens_geometry(&sample, preferences).unwrap();
        assert_eq!(mirrored.placement[0], ZoomAxisPlacement::MirroredOffset);
        assert_eq!(mirrored.destination_hotspot.x, 830);
        assert_eq!(
            mirrored.destination,
            PhysicalRect::new(750, 320, 910, 480).unwrap()
        );

        let clamp_sample = zoom_sample(
            PhysicalPoint::new(50, 200),
            Some(PhysicalRect::new(0, 0, 600, 400).unwrap()),
            None,
        );
        let clamped = zoom_lens_geometry(
            &clamp_sample,
            ZoomPreferences {
                destination_offset: CoordinateOffset::new(500, 0),
                ..ZoomPreferences::default()
            },
        )
        .unwrap();
        let centered = zoom_lens_geometry(
            &clamp_sample,
            ZoomPreferences {
                mode: ZoomMode::Centered,
                ..ZoomPreferences::default()
            },
        )
        .unwrap();
        assert_eq!(clamped.placement[0], ZoomAxisPlacement::Clamped);
        assert_eq!(clamped.destination_hotspot.x, 520);
        assert_eq!(
            clamped.destination,
            PhysicalRect::new(440, 120, 600, 280).unwrap()
        );
        assert_eq!(clamped.source, centered.source);
    }

    #[test]
    fn zoom_uses_signed_physical_monitor_bounds_not_work_area_or_dpi() {
        let sample = CoordinateSample::new(
            PhysicalPoint::new(-600, -400),
            Some(PhysicalRect::new(-1920, -1080, 1920, 1080).unwrap()),
            Some(MonitorGeometry {
                id: MonitorId::new("DISPLAY_UPPER_LEFT"),
                bounds: PhysicalRect::new(-1920, -1080, 0, 0).unwrap(),
                work_area: PhysicalRect::new(-1900, -1000, -20, -350).unwrap(),
                effective_dpi: Some((288, 192)),
            }),
            None,
        );
        let geometry = zoom_lens_geometry(
            &sample,
            ZoomPreferences {
                destination_offset: CoordinateOffset::new(120, 80),
                ..ZoomPreferences::default()
            },
        )
        .unwrap();
        assert_eq!(geometry.destination_hotspot, PhysicalPoint::new(-480, -320));
        assert_eq!(
            geometry.destination,
            PhysicalRect::new(-560, -400, -400, -240).unwrap()
        );
        assert_eq!(
            geometry.destination_bounds,
            Some(ZoomDestinationBounds::SampledMonitor)
        );
        assert_eq!(geometry.placement[1], ZoomAxisPlacement::RequestedOffset);
    }

    #[test]
    fn zoom_reports_edge_clipping_and_preserves_lens_diameter_when_oversized() {
        let edge_sample = zoom_sample(
            PhysicalPoint::new(10, 20),
            Some(PhysicalRect::new(0, 0, 1000, 1000).unwrap()),
            None,
        );
        let centered = zoom_lens_geometry(
            &edge_sample,
            ZoomPreferences {
                mode: ZoomMode::Centered,
                ..ZoomPreferences::default()
            },
        )
        .unwrap();
        assert_eq!(
            centered.destination,
            PhysicalRect::new(-70, -60, 90, 100).unwrap()
        );
        assert_eq!(
            centered.visible_destination,
            Some(PhysicalRect::new(0, 0, 90, 100).unwrap())
        );
        assert_eq!(
            centered.destination_missing,
            PhysicalInsets {
                left: 70,
                top: 60,
                right: 0,
                bottom: 0,
            }
        );

        let small_monitor = zoom_sample(
            PhysicalPoint::new(110, 120),
            Some(PhysicalRect::new(10, 20, 210, 220).unwrap()),
            None,
        );
        let oversized = zoom_lens_geometry(
            &small_monitor,
            ZoomPreferences {
                diameter: 480,
                destination_offset: CoordinateOffset::new(0, 0),
                ..ZoomPreferences::default()
            },
        )
        .unwrap();
        assert_eq!(oversized.destination.width(), 480);
        assert_eq!(oversized.destination.height(), 480);
        assert_eq!(
            oversized.destination,
            PhysicalRect::new(10, 20, 490, 500).unwrap()
        );
        assert_eq!(
            oversized.placement,
            [ZoomAxisPlacement::LeadingEdgeAnchored; 2]
        );
        assert_eq!(
            oversized.destination_missing,
            PhysicalInsets {
                left: 0,
                top: 0,
                right: 280,
                bottom: 280,
            }
        );
    }

    #[test]
    fn zoom_source_edge_margins_use_only_the_sampled_monitor_not_virtual_desktop_gaps() {
        let monitor = PhysicalRect::new(-100, -100, 0, 0).unwrap();
        let sample = zoom_sample(
            PhysicalPoint::new(-5, -5),
            Some(monitor),
            Some(PhysicalRect::new(-100, -100, 100, 100).unwrap()),
        );
        let geometry = zoom_lens_geometry(
            &sample,
            ZoomPreferences {
                mode: ZoomMode::Centered,
                ..ZoomPreferences::default()
            },
        )
        .unwrap();
        assert_eq!(
            geometry.source,
            PhysicalRect::new(-45, -45, 35, 35).unwrap()
        );
        assert_eq!(
            geometry.source_coverage,
            ZoomSourceCoverage::SampledMonitor {
                id: MonitorId::new("DISPLAY_TEST"),
                bounds: monitor,
                visible: Some(PhysicalRect::new(-45, -45, 0, 0).unwrap()),
                missing: PhysicalInsets {
                    left: 0,
                    top: 0,
                    right: 35,
                    bottom: 35,
                },
            }
        );

        let no_monitor = zoom_sample(
            PhysicalPoint::new(-5, -5),
            None,
            Some(PhysicalRect::new(-100, -100, 100, 100).unwrap()),
        );
        let unknown = zoom_lens_geometry(
            &no_monitor,
            ZoomPreferences {
                mode: ZoomMode::Centered,
                ..ZoomPreferences::default()
            },
        )
        .unwrap();
        assert_eq!(unknown.source_coverage, ZoomSourceCoverage::Unknown);
    }

    #[test]
    fn zoom_reports_fully_uncovered_source_and_destination_on_the_correct_side() {
        let monitor = PhysicalRect::new(0, 0, 100, 100).unwrap();
        let before = zoom_lens_geometry(
            &zoom_sample(PhysicalPoint::new(-500, -500), Some(monitor), None),
            ZoomPreferences {
                mode: ZoomMode::Centered,
                ..ZoomPreferences::default()
            },
        )
        .unwrap();
        assert_eq!(
            before.source_coverage,
            ZoomSourceCoverage::SampledMonitor {
                id: MonitorId::new("DISPLAY_TEST"),
                bounds: monitor,
                visible: None,
                missing: PhysicalInsets {
                    left: 80,
                    top: 80,
                    right: 0,
                    bottom: 0,
                },
            }
        );
        assert_eq!(
            before.destination_missing,
            PhysicalInsets {
                left: 160,
                top: 160,
                right: 0,
                bottom: 0,
            }
        );

        let after = zoom_lens_geometry(
            &zoom_sample(PhysicalPoint::new(500, 500), Some(monitor), None),
            ZoomPreferences {
                mode: ZoomMode::Centered,
                ..ZoomPreferences::default()
            },
        )
        .unwrap();
        assert_eq!(
            after.source_coverage,
            ZoomSourceCoverage::SampledMonitor {
                id: MonitorId::new("DISPLAY_TEST"),
                bounds: monitor,
                visible: None,
                missing: PhysicalInsets {
                    left: 0,
                    top: 0,
                    right: 80,
                    bottom: 80,
                },
            }
        );
        assert_eq!(
            after.destination_missing,
            PhysicalInsets {
                left: 0,
                top: 0,
                right: 160,
                bottom: 160,
            }
        );
    }

    #[test]
    fn zoom_requires_bounds_for_unanchored_offset_but_labels_virtual_fallback() {
        let no_bounds = zoom_sample(PhysicalPoint::new(500, 500), None, None);
        assert_eq!(
            zoom_lens_geometry(&no_bounds, ZoomPreferences::default()),
            Err(ZoomGeometryError::PlacementBoundsUnavailable)
        );
        let centered = zoom_lens_geometry(
            &no_bounds,
            ZoomPreferences {
                mode: ZoomMode::Centered,
                ..ZoomPreferences::default()
            },
        )
        .unwrap();
        assert_eq!(centered.destination_bounds, None);
        assert_eq!(centered.visible_destination, None);
        assert_eq!(centered.source_coverage, ZoomSourceCoverage::Unknown);

        let virtual_fallback = zoom_sample(
            PhysicalPoint::new(0, 0),
            None,
            Some(PhysicalRect::new(-100, -100, 100, 100).unwrap()),
        );
        let geometry = zoom_lens_geometry(
            &virtual_fallback,
            ZoomPreferences {
                diameter: 64,
                destination_offset: CoordinateOffset::new(80, 0),
                ..ZoomPreferences::default()
            },
        )
        .unwrap();
        assert_eq!(
            geometry.destination_bounds,
            Some(ZoomDestinationBounds::VirtualDesktopFallback)
        );
        assert_eq!(geometry.placement[0], ZoomAxisPlacement::Clamped);
        assert_eq!(
            geometry.destination,
            PhysicalRect::new(36, -32, 100, 32).unwrap()
        );
        assert_eq!(geometry.source_coverage, ZoomSourceCoverage::Unknown);
    }

    #[test]
    fn zoom_presentation_uses_full_source_with_exact_scale_and_circular_client_bounds() {
        let sample = zoom_sample(
            PhysicalPoint::new(500, 400),
            Some(PhysicalRect::new(-1000, -1000, 1000, 1000).unwrap()),
            None,
        );
        let geometry = zoom_presentation_geometry(
            zoom_lens_geometry(
                &sample,
                ZoomPreferences {
                    mode: ZoomMode::Centered,
                    ..ZoomPreferences::default()
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            geometry.source,
            PhysicalRect::new(460, 360, 540, 440).unwrap()
        );
        assert_eq!(geometry.child_origin, PhysicalPoint::new(0, 0));
        assert_eq!(geometry.child_size.width(), 160);
        assert_eq!(geometry.child_size.height(), 160);
        assert_eq!(geometry.client_coverage.left, 0);
        assert_eq!(geometry.client_coverage.top, 0);
        assert_eq!(geometry.client_coverage.right, 160);
        assert_eq!(geometry.client_coverage.bottom, 160);
        assert_eq!(geometry.lens.alignment.zoom_factor, 2.0);
        assert_eq!(
            geometry.child_rounding_error,
            super::RasterTranslation { x: 0.0, y: 0.0 }
        );
    }

    #[test]
    fn zoom_partial_source_clips_without_reanchoring_and_preserves_fractional_scale() {
        let sample = zoom_sample(
            PhysicalPoint::new(10, 10),
            Some(PhysicalRect::new(0, 0, 100, 100).unwrap()),
            Some(PhysicalRect::new(-100, -100, 100, 100).unwrap()),
        );
        let geometry = zoom_presentation_geometry(
            zoom_lens_geometry(
                &sample,
                ZoomPreferences {
                    mode: ZoomMode::Centered,
                    ..ZoomPreferences::default()
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(geometry.source, PhysicalRect::new(0, 0, 50, 50).unwrap());
        assert_eq!(geometry.child_origin, PhysicalPoint::new(60, 60));
        assert_eq!(geometry.child_size.width(), 100);
        assert_eq!(geometry.client_coverage.left, 70);
        assert_eq!(geometry.client_coverage.top, 70);
        assert_eq!(geometry.client_coverage.right, 160);
        assert_eq!(geometry.client_coverage.bottom, 160);
        // The cursor source pixel remains mapped to the center of the lens,
        // even though the unavailable leading source pixels were clipped.
        assert_eq!(geometry.child_origin.x + (10 * 2), 80);

        let fractional_sample = zoom_sample(
            PhysicalPoint::new(300, 300),
            Some(PhysicalRect::new(0, 0, 1000, 1000).unwrap()),
            None,
        );
        let fractional = zoom_presentation_geometry(
            zoom_lens_geometry(
                &fractional_sample,
                ZoomPreferences {
                    diameter: 163,
                    zoom_factor: 1.7,
                    mode: ZoomMode::Centered,
                    ..ZoomPreferences::default()
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(fractional.lens.alignment.zoom_factor, 1.7);
        assert_eq!(
            fractional.source,
            PhysicalRect::new(252, 252, 349, 349).unwrap()
        );
        assert_eq!(fractional.child_size.width(), 165);
        assert_eq!(fractional.client_coverage.right, 163);
        assert!((fractional.child_rounding_error.x.abs()) <= 0.5);
        assert!((fractional.child_rounding_error.y.abs()) <= 0.5);

        // At this exact half-pixel translation Rust rounds the child origin
        // outward. A leading source guard keeps the full destination covered
        // without changing the cursor-anchored mapping or configured factor.
        let leading_guard = zoom_presentation_geometry(
            zoom_lens_geometry(
                &fractional_sample,
                ZoomPreferences {
                    diameter: 166,
                    zoom_factor: 1.25,
                    mode: ZoomMode::Centered,
                    ..ZoomPreferences::default()
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(leading_guard.lens.alignment.zoom_factor, 1.25);
        assert_eq!(
            leading_guard.source,
            PhysicalRect::new(233, 233, 367, 367).unwrap()
        );
        assert_eq!(leading_guard.child_origin, PhysicalPoint::new(-1, -1));
        assert_eq!(leading_guard.client_coverage.left, 0);
        assert_eq!(leading_guard.client_coverage.top, 0);
        assert_eq!(leading_guard.client_coverage.right, 166);
        assert_eq!(leading_guard.client_coverage.bottom, 166);
        assert!(leading_guard.child_rounding_error.x.abs() <= 0.5);
        assert!(leading_guard.child_rounding_error.y.abs() <= 0.5);
    }

    #[test]
    fn zoom_presentation_requires_known_nonempty_monitor_and_destination_coverage() {
        let virtual_only = zoom_sample(
            PhysicalPoint::new(0, 0),
            None,
            Some(PhysicalRect::new(-100, -100, 100, 100).unwrap()),
        );
        let lens = zoom_lens_geometry(
            &virtual_only,
            ZoomPreferences {
                mode: ZoomMode::Centered,
                ..ZoomPreferences::default()
            },
        )
        .unwrap();
        assert_eq!(
            zoom_presentation_geometry(lens),
            Err(ZoomPresentationError::UnknownSourceCoverage)
        );

        let outside = zoom_sample(
            PhysicalPoint::new(200, 200),
            Some(PhysicalRect::new(0, 0, 100, 100).unwrap()),
            Some(PhysicalRect::new(-100, -100, 300, 300).unwrap()),
        );
        let lens = zoom_lens_geometry(
            &outside,
            ZoomPreferences {
                mode: ZoomMode::Centered,
                ..ZoomPreferences::default()
            },
        )
        .unwrap();
        assert_eq!(
            zoom_presentation_geometry(lens),
            Err(ZoomPresentationError::HotspotOutsideSampledMonitor)
        );
    }

    #[test]
    fn zoom_returns_arithmetic_overflow_instead_of_wrapping_extreme_source_rectangles() {
        for point in [
            PhysicalPoint::new(i32::MIN, 0),
            PhysicalPoint::new(i32::MAX, 0),
            PhysicalPoint::new(0, i32::MIN),
            PhysicalPoint::new(0, i32::MAX),
        ] {
            let sample = zoom_sample(point, None, None);
            assert_eq!(
                zoom_lens_geometry(
                    &sample,
                    ZoomPreferences {
                        mode: ZoomMode::Centered,
                        ..ZoomPreferences::default()
                    }
                ),
                Err(ZoomGeometryError::ArithmeticOverflow)
            );
        }
    }

    fn zoom_sample(
        point: PhysicalPoint,
        monitor_bounds: Option<PhysicalRect>,
        virtual_desktop_bounds: Option<PhysicalRect>,
    ) -> CoordinateSample {
        CoordinateSample::new(
            point,
            virtual_desktop_bounds,
            monitor_bounds.map(|bounds| MonitorGeometry {
                id: MonitorId::new("DISPLAY_TEST"),
                bounds,
                work_area: bounds,
                effective_dpi: None,
            }),
            None,
        )
    }

    #[test]
    fn halo_outline_is_separate_transparent_and_keeps_a_hole_at_minimum_radius() {
        let mut preferences = HaloPreferences {
            outline_enabled: true,
            outline_color: CrosshairColor::new(31, 121, 223),
            outline_thickness: 1,
            ..HaloPreferences::default()
        };
        let thin = halo_outline_bitmap(preferences).unwrap();
        assert_eq!(thin.dimensions(), (120, 120));
        assert_eq!(thin.get_pixel(60, 0).0, [31, 121, 223, 255]);
        assert_eq!(thin.get_pixel(0, 0).0[3], 0);
        for (x, y) in [(59, 59), (60, 59), (59, 60), (60, 60)] {
            assert_eq!(thin.get_pixel(x, y).0[3], 0);
        }
        let thin_pixels = thin.pixels().filter(|pixel| pixel.0[3] > 0).count();

        preferences.outline_thickness = 4;
        let thick = halo_outline_bitmap(preferences).unwrap();
        let thick_pixels = thick.pixels().filter(|pixel| pixel.0[3] > 0).count();
        assert!(thick_pixels > thin_pixels);
        assert_eq!(thick.get_pixel(60, 0).0, [31, 121, 223, 255]);
        assert!(halo_outline_bitmap(HaloPreferences::default()).is_none());

        preferences.radius = 8;
        preferences.outline_thickness = 8;
        let minimum = halo_outline_bitmap(preferences).unwrap();
        assert_eq!(minimum.dimensions(), (16, 16));
        assert!(minimum.pixels().any(|pixel| pixel.0[3] == 255));
        assert_eq!(minimum.get_pixel(7, 7).0[3], 0);
        assert_eq!(minimum.get_pixel(8, 7).0[3], 0);
        assert_eq!(minimum.get_pixel(7, 8).0[3], 0);
        assert_eq!(minimum.get_pixel(8, 8).0[3], 0);
    }

    #[test]
    fn zoom_outline_preserves_odd_size_color_thickness_and_transparent_center() {
        let mut preferences = ZoomPreferences {
            diameter: 65,
            outline_enabled: true,
            outline_color: CrosshairColor::new(17, 83, 201),
            outline_thickness: 4,
            ..ZoomPreferences::default()
        };
        let thin = zoom_outline_bitmap(preferences).unwrap();
        assert_eq!(thin.dimensions(), (65, 65));
        assert_eq!(thin.get_pixel(32, 0).0, [17, 83, 201, 255]);
        assert_eq!(thin.get_pixel(32, 4).0[3], 0);
        assert_eq!(thin.get_pixel(32, 32).0[3], 0);
        assert_eq!(thin.get_pixel(0, 0).0[3], 0);

        preferences.outline_thickness = 8;
        let thick = zoom_outline_bitmap(preferences).unwrap();
        assert_eq!(thick.get_pixel(32, 4).0, [17, 83, 201, 255]);
        assert_eq!(thick.get_pixel(32, 32).0[3], 0);

        preferences.outline_enabled = false;
        assert!(zoom_outline_bitmap(preferences).is_none());
    }

    #[test]
    fn halo_fallback_ring_has_contrasting_bands_and_transparent_center_at_minimum_radius() {
        let fallback = halo_fallback_bitmap(HaloPreferences::default()).unwrap();
        assert_eq!(fallback.dimensions(), (120, 120));
        assert_eq!(fallback.get_pixel(60, 0).0, [0, 0, 0, 255]);
        assert!(
            fallback
                .pixels()
                .any(|pixel| pixel.0 == [255, 255, 255, 255])
        );
        assert_eq!(fallback.get_pixel(60, 60).0[3], 0);

        let minimum = halo_fallback_bitmap(HaloPreferences {
            radius: 8,
            outline_enabled: true,
            outline_thickness: 8,
            ..HaloPreferences::default()
        })
        .unwrap();
        assert_eq!(minimum.dimensions(), (16, 16));
        assert!(minimum.pixels().any(|pixel| pixel.0 == [0, 0, 0, 255]));
        assert!(
            minimum
                .pixels()
                .any(|pixel| pixel.0 == [255, 255, 255, 255])
        );
        for (x, y) in [(7, 7), (8, 7), (7, 8), (8, 8)] {
            assert_eq!(minimum.get_pixel(x, y).0[3], 0);
        }
    }

    #[test]
    fn crosshair_geometry_keeps_four_arms_and_outline_around_each_arm() {
        let preferences = CrosshairPreferences::default();
        let CrosshairGeometry {
            width,
            height,
            colored_strokes,
            outline_strokes,
        } = crosshair_geometry(&preferences);

        assert_eq!((width, height), (59, 59));
        assert!(
            colored_strokes
                .iter()
                .all(|stroke| stroke.width > 0 && stroke.height > 0)
        );
        let outlines = outline_strokes.expect("default crosshair has contrast outline");
        for (stroke, outline) in colored_strokes.iter().zip(outlines) {
            assert!(outline.x <= stroke.x);
            assert!(outline.y <= stroke.y);
            assert!(outline.x + outline.width >= stroke.x + stroke.width);
            assert!(outline.y + outline.height >= stroke.y + stroke.height);
        }
    }

    #[test]
    fn guide_geometry_spans_signed_virtual_desktop_with_narrow_bands() {
        let desktop = PhysicalRect::new(-1920, -200, 2560, 1240).unwrap();
        let cursor = PhysicalPoint::new(-640, 333);
        let preferences = CrosshairPreferences::default();

        let horizontal =
            guide_geometry(GuideOrientation::Horizontal, desktop, cursor, &preferences).unwrap();
        assert_eq!(horizontal.origin, PhysicalPoint::new(-1920, 331));
        assert_eq!((horizontal.width, horizontal.height), (4480, 4));

        let vertical =
            guide_geometry(GuideOrientation::Vertical, desktop, cursor, &preferences).unwrap();
        assert_eq!(vertical.origin, PhysicalPoint::new(-642, -200));
        assert_eq!((vertical.width, vertical.height), (4, 1440));

        let changed_gap = CrosshairPreferences {
            center_gap: 128,
            ..preferences
        };
        assert_eq!(
            horizontal,
            guide_geometry(GuideOrientation::Horizontal, desktop, cursor, &changed_gap).unwrap()
        );
        assert_eq!(
            vertical,
            guide_geometry(GuideOrientation::Vertical, desktop, cursor, &changed_gap).unwrap()
        );
    }

    #[test]
    fn crosshair_pixel_clearance_is_symmetric_for_gaps_thicknesses_and_outlines() {
        let color = CrosshairColor::new(200, 30, 40);
        const ARM_LENGTH: u32 = 9;
        for gap in [0, 1, 16, 128] {
            for thickness in [3, 4] {
                for high_contrast_outline in [false, true] {
                    let preferences = CrosshairPreferences {
                        color,
                        thickness,
                        arm_length: ARM_LENGTH as i32,
                        center_gap: gap,
                        opacity: 1.0,
                        high_contrast_outline,
                        ..Default::default()
                    };
                    let geometry = crosshair_geometry(&preferences);
                    let image = crosshair_bitmap(&preferences);
                    let center = (image.width() / 2, image.height() / 2);
                    assert_eq!(image.width() % 2, 1);
                    assert_eq!(image.height() % 2, 1);

                    for (horizontal, positive) in
                        [(true, false), (true, true), (false, false), (false, true)]
                    {
                        let visible_distance =
                            first_visible_axis_pixel(&image, center, horizontal, positive);
                        assert_eq!(
                            visible_distance, gap as u32,
                            "gap={gap}, thickness={thickness}, outline={high_contrast_outline}, horizontal={horizontal}, positive={positive}"
                        );

                        let colored_length =
                            colored_axis_pixels(&image, center, horizontal, positive, color);
                        assert_eq!(
                            colored_length, ARM_LENGTH,
                            "colored arm length changed for gap={gap}, thickness={thickness}, outline={high_contrast_outline}, horizontal={horizontal}, positive={positive}"
                        );
                    }

                    let expected_rgba = [color.red, color.green, color.blue, 255];
                    for stroke in &geometry.colored_strokes[..2] {
                        let endpoint_x = if stroke.x < center.0 {
                            stroke.x
                        } else {
                            stroke.x + stroke.width - 1
                        };
                        let colored_pixels = (0..image.height())
                            .filter(|y| image.get_pixel(endpoint_x, *y).0 == expected_rgba)
                            .count();
                        assert_eq!(colored_pixels, thickness as usize);
                    }
                    for stroke in &geometry.colored_strokes[2..] {
                        let endpoint_y = if stroke.y < center.1 {
                            stroke.y
                        } else {
                            stroke.y + stroke.height - 1
                        };
                        let colored_pixels = (0..image.width())
                            .filter(|x| image.get_pixel(*x, endpoint_y).0 == expected_rgba)
                            .count();
                        assert_eq!(colored_pixels, thickness as usize);
                    }
                }
            }
        }
    }

    #[test]
    fn crosshair_geometry_bounds_raw_dimensions_before_sizing() {
        let minimum = CrosshairPreferences {
            arm_length: i32::MIN,
            thickness: i32::MIN,
            center_gap: i32::MIN,
            high_contrast_outline: false,
            ..Default::default()
        };
        let minimum_geometry = crosshair_geometry(&minimum);
        assert_eq!((minimum_geometry.width, minimum_geometry.height), (3, 3));
        assert_eq!(minimum_geometry.colored_strokes[0].width, 2);
        assert_eq!(minimum_geometry.colored_strokes[0].height, 1);

        let maximum = CrosshairPreferences {
            arm_length: i32::MAX,
            thickness: i32::MAX,
            center_gap: i32::MAX,
            high_contrast_outline: true,
            ..Default::default()
        };
        let maximum_geometry = crosshair_geometry(&maximum);
        assert_eq!(
            (maximum_geometry.width, maximum_geometry.height),
            (771, 771)
        );
        assert_eq!(maximum_geometry.colored_strokes[0].width, 256);
        assert_eq!(maximum_geometry.colored_strokes[0].height, 16);
    }

    fn first_visible_axis_pixel(
        image: &image::RgbaImage,
        center: (u32, u32),
        horizontal: bool,
        positive: bool,
    ) -> u32 {
        let axis_length = if horizontal {
            image.width()
        } else {
            image.height()
        };
        let center_position = if horizontal { center.0 } else { center.1 };
        let ray_length = if positive {
            axis_length - center_position
        } else {
            center_position + 1
        };
        (0..ray_length)
            .find(|distance| {
                let x = if horizontal {
                    if positive {
                        center.0 + *distance
                    } else {
                        center.0 - *distance
                    }
                } else {
                    center.0
                };
                let y = if horizontal {
                    center.1
                } else if positive {
                    center.1 + *distance
                } else {
                    center.1 - *distance
                };
                x < image.width() && y < image.height() && image.get_pixel(x, y).0[3] > 0
            })
            .expect("crosshair has visible pixels on all four axes")
    }

    fn colored_axis_pixels(
        image: &image::RgbaImage,
        center: (u32, u32),
        horizontal: bool,
        positive: bool,
        color: CrosshairColor,
    ) -> u32 {
        let axis_length = if horizontal {
            image.width()
        } else {
            image.height()
        };
        let center_position = if horizontal { center.0 } else { center.1 };
        let ray_length = if positive {
            axis_length - center_position
        } else {
            center_position + 1
        };
        let expected = [color.red, color.green, color.blue, 255];
        (0..ray_length)
            .filter(|distance| {
                let x = if horizontal {
                    if positive {
                        center.0 + *distance
                    } else {
                        center.0 - *distance
                    }
                } else {
                    center.0
                };
                let y = if horizontal {
                    center.1
                } else if positive {
                    center.1 + *distance
                } else {
                    center.1 - *distance
                };
                x < image.width() && y < image.height() && image.get_pixel(x, y).0 == expected
            })
            .count() as u32
    }

    #[test]
    fn contrast_outline_switches_to_the_opposite_luminance_family() {
        assert_eq!(
            super::contrasting_color(CrosshairColor::new(255, 0, 0)),
            CrosshairColor::new(255, 255, 255)
        );
        assert_eq!(
            super::contrasting_color(CrosshairColor::new(255, 255, 255)),
            CrosshairColor::new(0, 0, 0)
        );
    }

    #[test]
    fn detailed_hud_reports_origins_freeze_copy_and_help_context() {
        let monitor = PhysicalRect::new(-1920, 0, 0, 1080).unwrap();
        let sample = CoordinateSample::new(
            PhysicalPoint::new(-1732, 215),
            Some(PhysicalRect::new(-1920, 0, 3840, 2160).unwrap()),
            Some(MonitorGeometry {
                id: MonitorId::new("DISPLAY_LEFT"),
                bounds: monitor,
                work_area: PhysicalRect::new(-1920, 0, 0, 1040).unwrap(),
                effective_dpi: Some((144, 144)),
            }),
            Some(ForegroundClientGeometry::new(
                PhysicalPoint::new(-1800, 40),
                Some(PhysicalRect::new(-1800, 40, -100, 800).unwrap()),
            )),
        );
        let mut runtime_state = CoordinateToolRuntimeState::default();
        runtime_state.freeze(&sample);
        runtime_state.record_successful_copy(
            super::super::model::format_coordinate(&sample, CoordinateSpace::Desktop).unwrap(),
        );
        let frame = CoordinateRenderFrame {
            preferences: crate::coordinate_tool::CoordinateToolPreferences {
                space: CoordinateSpace::ForegroundClient,
                hud_detail: crate::coordinate_tool::HudDetail::Detailed,
                ..Default::default()
            },
            runtime_state,
            current_sample: Some(sample.clone()),
            displayed_sample: Some(sample.clone()),
            placement_sample: Some(sample.clone()),
            sample_error: None,
        };

        let lines = super::hud_lines(&frame).join("\n");
        assert!(lines.contains("Selected: Foreground client 68,175"));
        assert!(lines.contains("Monitor: DISPLAY_LEFT"));
        assert!(lines.contains("Monitor origin: -1920,0"));
        assert!(lines.contains("Client origin: -1800,40"));
        assert!(lines.contains("Freeze: on"));
        assert!(lines.contains("Last copy: -1732,215 (Desktop)"));
        assert!(lines.contains("Help: mouse help"));
        assert_eq!(super::hud_font_size(Some(144)), 21);
        assert_eq!(super::hud_font_size(None), 14);
    }

    #[test]
    fn high_dpi_frozen_hud_fits_sampling_error_and_final_help_row_in_work_area() {
        let sample = CoordinateSample::new(
            PhysicalPoint::new(-1732, 215),
            Some(PhysicalRect::new(-1920, 0, 1920, 2160).unwrap()),
            Some(MonitorGeometry {
                id: MonitorId::new("DISPLAY_LEFT"),
                bounds: PhysicalRect::new(-1920, 0, 0, 1080).unwrap(),
                work_area: PhysicalRect::new(-1920, 0, 0, 1040).unwrap(),
                effective_dpi: Some((288, 288)),
            }),
            Some(ForegroundClientGeometry::new(
                PhysicalPoint::new(-1800, 40),
                Some(PhysicalRect::new(-1800, 40, -100, 800).unwrap()),
            )),
        );
        let mut runtime_state = CoordinateToolRuntimeState::default();
        runtime_state.freeze(&sample);
        runtime_state.record_successful_copy(
            super::super::model::format_coordinate(&sample, CoordinateSpace::Desktop).unwrap(),
        );
        let frame = CoordinateRenderFrame {
            preferences: crate::coordinate_tool::CoordinateToolPreferences {
                hud_detail: crate::coordinate_tool::HudDetail::Detailed,
                ..Default::default()
            },
            runtime_state,
            current_sample: Some(sample.clone()),
            displayed_sample: Some(sample.clone()),
            placement_sample: Some(sample),
            sample_error: Some("sample backend is temporarily unavailable".into()),
        };
        let lines = super::hud_lines(&frame);
        assert!(lines.iter().any(|line| line.starts_with("Sampling: ")));
        assert_eq!(lines.last().map(String::as_str), Some("Help: mouse help"));

        let uncapped = super::hud_layout(&lines, 32, None);
        assert!(uncapped.2 > 480);

        let work_area = PhysicalSize::new(1920, 520).unwrap();
        let (font_size, width, height) = super::hud_layout(&lines, 32, Some(work_area));
        assert!(
            font_size < 32,
            "font should shrink to fit the work-area height"
        );
        assert!(width <= 1920);
        assert!(
            width > 640,
            "long signed coordinates and sampling details must not be clipped by a fixed width cap"
        );
        assert!(height <= 520);
        let last_row_bottom =
            9 + u32::try_from(lines.len() - 1).unwrap() * (font_size + 5) + font_size;
        assert!(
            last_row_bottom <= height,
            "final HUD row must be fully inside the allocated surface"
        );
    }
}
