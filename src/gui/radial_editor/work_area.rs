//! Monitor work-area adapter used when restoring the independent Designer.
//!
//! Window positions are desktop coordinates, while an egui screen rectangle
//! may be local to the current viewport.  The Windows path therefore queries
//! the nearest monitor directly and converts its actual work rectangle to the
//! logical units used by the saved Designer geometry.  If that query is not
//! available, only the size is clamped and the saved position is omitted so
//! the OS can place the viewport accessibly; a local `(0, 0)` is never treated
//! as a desktop origin.

use super::canvas::{
    CanvasPoint, WindowGeometry, WindowWorkArea, clamp_window_geometry, clamp_window_size,
};
use eframe::egui;

pub(crate) fn restore_geometry(
    ctx: &egui::Context,
    geometry: WindowGeometry,
    saved_position_scale_factor: Option<f32>,
    current_zoom_factor: f32,
) -> WindowGeometry {
    if !valid_zoom_factor(current_zoom_factor) {
        return clamp_window_size(geometry, viewport_size(ctx));
    }
    current_work_area(
        ctx,
        geometry.position,
        saved_position_scale_factor,
        current_zoom_factor,
    )
    .and_then(|(work_area, current_scale)| {
        geometry_in_current_scale(geometry, saved_position_scale_factor, current_scale)
            .map(|geometry| clamp_window_geometry(geometry, work_area))
    })
    .unwrap_or_else(|| clamp_window_size(geometry, viewport_size(ctx)))
}

fn viewport_size(ctx: &egui::Context) -> CanvasPoint {
    let screen = ctx.screen_rect();
    ctx.input(|input| input.viewport().monitor_size)
        .map(|size| CanvasPoint::new(size.x, size.y))
        .unwrap_or_else(|| CanvasPoint::new(screen.width(), screen.height()))
}

fn current_work_area(
    ctx: &egui::Context,
    preferred_position: Option<CanvasPoint>,
    saved_position_scale_factor: Option<f32>,
    current_zoom_factor: f32,
) -> Option<(WindowWorkArea, CanvasPoint)> {
    #[cfg(windows)]
    {
        windows_work_area(
            ctx,
            preferred_position,
            saved_position_scale_factor,
            current_zoom_factor,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (
            ctx,
            preferred_position,
            saved_position_scale_factor,
            current_zoom_factor,
        );
        None
    }
}

const MIN_ZOOM_FACTOR: f32 = 0.25;
const MAX_ZOOM_FACTOR: f32 = 4.0;

fn valid_zoom_factor(zoom_factor: f32) -> bool {
    zoom_factor.is_finite() && (MIN_ZOOM_FACTOR..=MAX_ZOOM_FACTOR).contains(&zoom_factor)
}

#[cfg(any(windows, test))]
fn work_area_from_edges(left: i32, top: i32, right: i32, bottom: i32) -> Option<WindowWorkArea> {
    let width = right.saturating_sub(left) as f32;
    let height = bottom.saturating_sub(top) as f32;
    (width > 0.0 && height > 0.0).then(|| WindowWorkArea {
        origin: CanvasPoint::new(left as f32, top as f32),
        size: CanvasPoint::new(width, height),
    })
}

/// Convert a physical Windows work rectangle to the logical coordinates used
/// by an egui viewport on the monitor that owns it.  This intentionally takes
/// the selected monitor's effective DPI instead of a root viewport scale:
/// the two can differ when the Designer is restored onto a mixed-DPI display.
#[cfg(any(windows, test))]
fn logical_work_area_from_edges(
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    effective_dpi_x: u32,
    effective_dpi_y: u32,
    current_zoom_factor: f32,
) -> Option<WindowWorkArea> {
    let physical = work_area_from_edges(left, top, right, bottom)?;
    if effective_dpi_x == 0 || effective_dpi_y == 0 {
        return None;
    }
    if !valid_zoom_factor(current_zoom_factor) {
        return None;
    }
    let scale_x = effective_dpi_x as f32 / 96.0 * current_zoom_factor;
    let scale_y = effective_dpi_y as f32 / 96.0 * current_zoom_factor;
    if !scale_x.is_finite() || !scale_y.is_finite() || scale_x <= 0.0 || scale_y <= 0.0 {
        return None;
    }
    Some(WindowWorkArea {
        origin: CanvasPoint::new(physical.origin.x / scale_x, physical.origin.y / scale_y),
        size: CanvasPoint::new(physical.size.x / scale_x, physical.size.y / scale_y),
    })
}

#[cfg(any(windows, test))]
fn current_ui_scale_from_dpi(
    effective_dpi_x: u32,
    effective_dpi_y: u32,
    current_zoom_factor: f32,
) -> Option<CanvasPoint> {
    if effective_dpi_x == 0 || effective_dpi_y == 0 || !valid_zoom_factor(current_zoom_factor) {
        return None;
    }
    let scale = CanvasPoint::new(
        effective_dpi_x as f32 / 96.0 * current_zoom_factor,
        effective_dpi_y as f32 / 96.0 * current_zoom_factor,
    );
    (scale.x.is_finite() && scale.x > 0.0 && scale.y.is_finite() && scale.y > 0.0).then_some(scale)
}

#[cfg(any(windows, test))]
fn physical_position_from_saved_position(
    position: CanvasPoint,
    saved_scale_factor: Option<f32>,
) -> Option<CanvasPoint> {
    let scale = saved_scale_factor.filter(|scale| {
        scale.is_finite()
            && (crate::settings::RadialDesignerPreferences::MIN_WINDOW_SCALE_FACTOR
                ..=crate::settings::RadialDesignerPreferences::MAX_WINDOW_SCALE_FACTOR)
                .contains(scale)
    })?;
    if !position.x.is_finite() || !position.y.is_finite() {
        return None;
    }
    let physical = CanvasPoint::new(position.x * scale, position.y * scale);
    (physical.x.is_finite() && physical.y.is_finite()).then_some(physical)
}

#[cfg(any(windows, test))]
fn geometry_in_current_scale(
    geometry: WindowGeometry,
    saved_scale_factor: Option<f32>,
    current_scale: CanvasPoint,
) -> Option<WindowGeometry> {
    let position = geometry.position.map(|position| {
        physical_position_from_saved_position(position, saved_scale_factor).map(|physical| {
            CanvasPoint::new(physical.x / current_scale.x, physical.y / current_scale.y)
        })
    });
    let position = match position {
        Some(Some(position)) => Some(position),
        Some(None) => return None,
        None => None,
    };
    let size = if let Some(saved_scale_factor) = saved_scale_factor.filter(|scale| {
        scale.is_finite()
            && (crate::settings::RadialDesignerPreferences::MIN_WINDOW_SCALE_FACTOR
                ..=crate::settings::RadialDesignerPreferences::MAX_WINDOW_SCALE_FACTOR)
                .contains(scale)
    }) {
        CanvasPoint::new(
            geometry.size.x * saved_scale_factor / current_scale.x,
            geometry.size.y * saved_scale_factor / current_scale.y,
        )
    } else {
        geometry.size
    };
    Some(WindowGeometry { position, size })
}

/// Convert a persisted child-viewport position, which is in that viewport's
/// logical points, to the physical virtual-desktop point required by
/// `MonitorFromPoint`.  A missing or invalid scale is intentionally not
/// guessed: the caller must use the accessible OS-placement fallback.
#[cfg(any(windows, test))]
fn physical_point_from_saved_position(
    position: CanvasPoint,
    saved_scale_factor: Option<f32>,
) -> Option<(i32, i32)> {
    let physical = physical_position_from_saved_position(position, saved_scale_factor)?;
    Some((
        physical.x.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32,
        physical.y.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32,
    ))
}

#[cfg(windows)]
fn windows_work_area(
    _ctx: &egui::Context,
    preferred_position: Option<CanvasPoint>,
    saved_position_scale_factor: Option<f32>,
    current_zoom_factor: f32,
) -> Option<(WindowWorkArea, CanvasPoint)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let point = if let Some(position) = preferred_position {
        let (x, y) = physical_point_from_saved_position(position, saved_position_scale_factor)?;
        POINT { x, y }
    } else {
        let mut point = POINT::default();
        if unsafe { GetCursorPos(&mut point) }.is_err() {
            return None;
        }
        point
    };
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    if monitor.0.is_null() {
        return None;
    }
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return None;
    }
    let mut dpi_x = 0;
    let mut dpi_y = 0;
    if unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }.is_err()
        || dpi_x == 0
        || dpi_y == 0
    {
        return None;
    }
    let current_scale = current_ui_scale_from_dpi(dpi_x, dpi_y, current_zoom_factor)?;
    let work_area = logical_work_area_from_edges(
        info.rcWork.left,
        info.rcWork.top,
        info.rcWork.right,
        info.rcWork.bottom,
        dpi_x,
        dpi_y,
        current_zoom_factor,
    )?;
    Some((work_area, current_scale))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_area_adapter_preserves_negative_origin_and_taskbar_extent() {
        let work_area = work_area_from_edges(-1920, -40, -320, 1040).unwrap();
        assert_eq!(work_area.origin, CanvasPoint::new(-1920.0, -40.0));
        assert_eq!(work_area.size, CanvasPoint::new(1600.0, 1080.0));
    }

    #[test]
    fn target_monitor_dpi_drives_work_area_conversion_and_clamp() {
        let root_scale = 1.0_f32;
        let target_dpi = 144;
        assert_ne!(root_scale, target_dpi as f32 / 96.0);

        let work_area =
            logical_work_area_from_edges(-2560, -100, 0, 1040, target_dpi, target_dpi, 1.0)
                .unwrap();
        let expected_origin = CanvasPoint::new(-2560.0 / 1.5, -100.0 / 1.5);
        let expected_size = CanvasPoint::new(2560.0 / 1.5, 1140.0 / 1.5);
        assert!((work_area.origin.x - expected_origin.x).abs() < 0.001);
        assert!((work_area.origin.y - expected_origin.y).abs() < 0.001);
        assert!((work_area.size.x - expected_size.x).abs() < 0.001);
        assert!((work_area.size.y - expected_size.y).abs() < 0.001);

        let restored = clamp_window_geometry(
            WindowGeometry {
                position: Some(CanvasPoint::new(0.0, 1_000.0)),
                size: CanvasPoint::new(1_800.0, 900.0),
            },
            work_area,
        );
        assert!((restored.size.x - expected_size.x).abs() < 0.001);
        assert!((restored.size.y - expected_size.y).abs() < 0.001);
        let restored_position = restored.position.unwrap();
        // The clamped width fills the target work area, so the only visible
        // x-position is its negative-origin left edge.  The same applies to
        // y because the clamped height fills the target work-area extent.
        assert!((restored_position.x - expected_origin.x).abs() < 0.001);
        assert!((restored_position.y - expected_origin.y).abs() < 0.001);
    }

    #[test]
    fn saved_child_scale_maps_logical_position_to_the_target_monitor() {
        let root_scale = 1.0_f32;
        let saved_child_scale = 1.5_f32;
        let saved_position = CanvasPoint::new(1_280.0, 100.0);
        let (physical_x, physical_y) =
            physical_point_from_saved_position(saved_position, Some(saved_child_scale)).unwrap();

        assert_eq!(physical_x, 1_920);
        assert_eq!(physical_y, 150);
        assert_ne!(physical_x, (saved_position.x * root_scale).round() as i32);
        // The target monitor starts at physical x=1920.  Using the root
        // scale would incorrectly leave this saved logical point on the
        // preceding 100%-scale monitor.
        assert!((1_920..3_840).contains(&physical_x));
    }

    #[test]
    fn unchanged_zoom_reconstructs_saved_position_before_target_clamp() {
        let saved_scale = 1.5_f32;
        let target_dpi = 144;
        let current_zoom = 1.0_f32;
        let saved_position = CanvasPoint::new(-1_600.0, 100.0);
        let (physical_x, physical_y) =
            physical_point_from_saved_position(saved_position, Some(saved_scale)).unwrap();
        assert_eq!((physical_x, physical_y), (-2_400, 150));
        assert!((-2_560..0).contains(&physical_x));

        let work_area = logical_work_area_from_edges(
            -2_560,
            -100,
            0,
            1_040,
            target_dpi,
            target_dpi,
            current_zoom,
        )
        .unwrap();
        let current_scale =
            current_ui_scale_from_dpi(target_dpi, target_dpi, current_zoom).unwrap();
        let geometry = geometry_in_current_scale(
            WindowGeometry {
                position: Some(saved_position),
                size: CanvasPoint::new(1_000.0, 600.0),
            },
            Some(saved_scale),
            current_scale,
        )
        .unwrap();
        assert_eq!(geometry.position, Some(saved_position));
        assert_eq!(geometry.size, CanvasPoint::new(1_000.0, 600.0));
        let restored = clamp_window_geometry(geometry, work_area);
        assert!((restored.position.unwrap().x + 1_600.0).abs() < 0.001);
        assert!((restored.position.unwrap().y - 93.3333).abs() < 0.001);
    }

    #[test]
    fn changed_zoom_uses_target_full_scale_for_area_position_and_size() {
        let saved_scale = 1.5_f32;
        let target_dpi = 144;
        let current_zoom = 0.8_f32;
        let saved_position = CanvasPoint::new(-1_600.0, 100.0);
        let (physical_x, physical_y) =
            physical_point_from_saved_position(saved_position, Some(saved_scale)).unwrap();
        assert_eq!((physical_x, physical_y), (-2_400, 150));
        assert!((-2_560..0).contains(&physical_x));

        let work_area = logical_work_area_from_edges(
            -2_560,
            -100,
            0,
            1_040,
            target_dpi,
            target_dpi,
            current_zoom,
        )
        .unwrap();
        let expected_origin = CanvasPoint::new(-2_560.0 / 1.2, -100.0 / 1.2);
        let expected_size = CanvasPoint::new(2_560.0 / 1.2, 1_140.0 / 1.2);
        assert!((work_area.origin.x - expected_origin.x).abs() < 0.001);
        assert!((work_area.origin.y - expected_origin.y).abs() < 0.001);
        assert!((work_area.size.x - expected_size.x).abs() < 0.001);
        assert!((work_area.size.y - expected_size.y).abs() < 0.001);

        let current_scale =
            current_ui_scale_from_dpi(target_dpi, target_dpi, current_zoom).unwrap();
        assert!((current_scale.x - 1.2).abs() < 0.001);
        assert!((current_scale.y - 1.2).abs() < 0.001);
        let geometry = geometry_in_current_scale(
            WindowGeometry {
                position: Some(saved_position),
                size: CanvasPoint::new(1_000.0, 600.0),
            },
            Some(saved_scale),
            current_scale,
        )
        .unwrap();
        let geometry_position = geometry.position.unwrap();
        assert!((geometry_position.x + 2_000.0).abs() < 0.001);
        assert!((geometry_position.y - 125.0).abs() < 0.001);
        assert!((geometry.size.x - 1_250.0).abs() < 0.001);
        assert!((geometry.size.y - 750.0).abs() < 0.001);
        let restored = clamp_window_geometry(geometry, work_area);
        assert!((restored.position.unwrap().x + 2_000.0).abs() < 0.001);
        assert!((restored.position.unwrap().y - 116.6667).abs() < 0.001);
    }

    #[test]
    fn legacy_saved_position_without_child_scale_is_rejected() {
        let position = CanvasPoint::new(1_280.0, 100.0);
        assert!(physical_point_from_saved_position(position, None).is_none());
        assert!(physical_point_from_saved_position(position, Some(f32::NAN)).is_none());
        assert!(physical_point_from_saved_position(position, Some(0.25)).is_none());
    }

    #[test]
    fn zoom_factor_validation_keeps_bounds_explicit() {
        assert!(valid_zoom_factor(0.25));
        assert!(valid_zoom_factor(4.0));
        assert!(!valid_zoom_factor(0.24));
        // `4.0 + f32::EPSILON` rounds back to 4.0 in f32; use a representable
        // value above the upper bound instead.
        assert!(!valid_zoom_factor(4.01));
        assert!(!valid_zoom_factor(f32::NAN));
    }

    #[test]
    fn unavailable_work_area_fallback_keeps_size_but_clears_position() {
        let restored = clamp_window_size(
            WindowGeometry {
                position: Some(CanvasPoint::new(-50_000.0, 50_000.0)),
                size: CanvasPoint::new(2_000.0, 1_200.0),
            },
            CanvasPoint::new(1_200.0, 800.0),
        );
        assert_eq!(restored.position, None);
        assert_eq!(restored.size, CanvasPoint::new(1_200.0, 800.0));
    }

    #[test]
    fn invalid_current_zoom_uses_positionless_os_placement_fallback() {
        let geometry = WindowGeometry {
            position: Some(CanvasPoint::new(-1_600.0, 100.0)),
            size: CanvasPoint::new(1_000.0, 600.0),
        };
        assert!(!valid_zoom_factor(f32::NAN));
        let restored = restore_geometry(&egui::Context::default(), geometry, Some(1.5), f32::NAN);
        assert_eq!(restored.position, None);
    }

    #[test]
    fn invalid_target_dpi_does_not_create_a_zero_origin_work_area() {
        assert!(logical_work_area_from_edges(-1920, -40, 0, 1040, 0, 144, 1.0).is_none());
    }
}
