use serde::{Deserialize, Serialize};

/// A signed physical pixel location in virtual-desktop coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PhysicalPoint {
    pub x: i32,
    pub y: i32,
}

impl PhysicalPoint {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// A physical rectangle whose right and bottom edges are exclusive, matching
/// Win32 `RECT` semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl PhysicalRect {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Option<Self> {
        (right > left && bottom > top).then_some(Self {
            left,
            top,
            right,
            bottom,
        })
    }

    pub const fn left(self) -> i32 {
        self.left
    }

    pub const fn top(self) -> i32 {
        self.top
    }

    pub const fn right(self) -> i32 {
        self.right
    }

    pub const fn bottom(self) -> i32 {
        self.bottom
    }

    pub const fn width(self) -> i64 {
        self.right as i64 - self.left as i64
    }

    pub const fn height(self) -> i64 {
        self.bottom as i64 - self.top as i64
    }
}

/// A non-empty physical size used for overlay placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalSize {
    width: i64,
    height: i64,
}

impl PhysicalSize {
    pub const fn new(width: i64, height: i64) -> Option<Self> {
        let max_physical_extent = i32::MAX as i64 - i32::MIN as i64;
        if width > 0 && width <= max_physical_extent && height > 0 && height <= max_physical_extent
        {
            Some(Self { width, height })
        } else {
            None
        }
    }

    pub const fn width(self) -> i64 {
        self.width
    }

    pub const fn height(self) -> i64 {
        self.height
    }
}

/// A stable native monitor identity, normally the monitor device name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MonitorId(pub String);

impl MonitorId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// Physical monitor geometry captured alongside a cursor sample.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonitorGeometry {
    pub id: MonitorId,
    pub bounds: PhysicalRect,
    pub work_area: PhysicalRect,
    pub effective_dpi: Option<(u32, u32)>,
}

/// Client-area origin in physical virtual-desktop coordinates, with the
/// transformed client bounds when Windows can provide a non-empty rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForegroundClientGeometry {
    pub origin: PhysicalPoint,
    pub bounds: Option<PhysicalRect>,
}

impl ForegroundClientGeometry {
    pub const fn new(origin: PhysicalPoint, bounds: Option<PhysicalRect>) -> Self {
        Self { origin, bounds }
    }
}

/// A coordinate space selected for display or copying.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateSpace {
    #[default]
    Desktop,
    Monitor,
    ForegroundClient,
}

/// Why a coordinate cannot be expressed in the requested space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoordinateUnavailable {
    MonitorUnavailable,
    ForegroundClientUnavailable,
    ArithmeticOverflow,
}

/// All native information associated with one cursor observation.
///
/// Keeping the point, monitor geometry, and foreground-client origin together
/// prevents a HUD or copy action from combining values sampled at different
/// moments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoordinateSample {
    pub desktop_point: PhysicalPoint,
    pub virtual_desktop_bounds: Option<PhysicalRect>,
    pub monitor: Option<MonitorGeometry>,
    pub foreground_client: Option<ForegroundClientGeometry>,
}

impl CoordinateSample {
    pub fn new(
        desktop_point: PhysicalPoint,
        virtual_desktop_bounds: Option<PhysicalRect>,
        monitor: Option<MonitorGeometry>,
        foreground_client: Option<ForegroundClientGeometry>,
    ) -> Self {
        Self {
            desktop_point,
            virtual_desktop_bounds,
            monitor,
            foreground_client,
        }
    }

    /// Convert this sample to a selected space with checked signed arithmetic.
    pub fn point_in(&self, space: CoordinateSpace) -> Result<PhysicalPoint, CoordinateUnavailable> {
        match space {
            CoordinateSpace::Desktop => Ok(self.desktop_point),
            CoordinateSpace::Monitor => {
                let monitor = self
                    .monitor
                    .as_ref()
                    .ok_or(CoordinateUnavailable::MonitorUnavailable)?;
                checked_relative_to(self.desktop_point, monitor.bounds.left, monitor.bounds.top)
            }
            CoordinateSpace::ForegroundClient => {
                let origin = self
                    .foreground_client
                    .ok_or(CoordinateUnavailable::ForegroundClientUnavailable)?;
                checked_relative_to(self.desktop_point, origin.origin.x, origin.origin.y)
            }
        }
    }

    /// Place a HUD using this sample's monitor work area, so it cannot be
    /// clamped against a different monitor than the cursor sample identifies.
    pub fn hud_origin(
        &self,
        offset: PhysicalPoint,
        hud_size: PhysicalSize,
    ) -> Option<PhysicalPoint> {
        let work_area = self.monitor.as_ref()?.work_area;
        Some(clamp_hud_origin(
            self.desktop_point,
            offset,
            hud_size,
            work_area,
        ))
    }
}

fn checked_relative_to(
    point: PhysicalPoint,
    origin_x: i32,
    origin_y: i32,
) -> Result<PhysicalPoint, CoordinateUnavailable> {
    let x = point
        .x
        .checked_sub(origin_x)
        .ok_or(CoordinateUnavailable::ArithmeticOverflow)?;
    let y = point
        .y
        .checked_sub(origin_y)
        .ok_or(CoordinateUnavailable::ArithmeticOverflow)?;
    Ok(PhysicalPoint::new(x, y))
}

/// Clamp a cursor-offset HUD origin inside a monitor work area.
///
/// If the HUD is larger than the work area on an axis, its origin is placed at
/// that axis's leading edge. Intermediate arithmetic uses `i64` so a cursor
/// near an `i32` boundary cannot overflow before clamping.
pub fn clamp_hud_origin(
    cursor: PhysicalPoint,
    offset: PhysicalPoint,
    hud_size: PhysicalSize,
    work_area: PhysicalRect,
) -> PhysicalPoint {
    let x = clamp_axis(
        cursor.x,
        offset.x,
        work_area.left,
        work_area.right,
        hud_size.width,
    );
    let y = clamp_axis(
        cursor.y,
        offset.y,
        work_area.top,
        work_area.bottom,
        hud_size.height,
    );
    PhysicalPoint::new(x, y)
}

fn clamp_axis(cursor: i32, offset: i32, leading: i32, trailing: i32, size: i64) -> i32 {
    let min = i64::from(leading);
    let max = (i64::from(trailing) - size).max(min);
    (i64::from(cursor) + i64::from(offset)).clamp(min, max) as i32
}

/// A successfully formatted coordinate value. Clipboard operations are owned
/// by a later native integration layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormattedCoordinate {
    pub space: CoordinateSpace,
    pub point: PhysicalPoint,
    pub text: String,
}

pub fn format_coordinate(
    sample: &CoordinateSample,
    space: CoordinateSpace,
) -> Result<FormattedCoordinate, CoordinateUnavailable> {
    let point = sample.point_in(space)?;
    Ok(FormattedCoordinate {
        space,
        point,
        text: format!("{},{}", point.x, point.y),
    })
}

/// Runtime-only toggles and inspector state. None of these values are
/// persisted in the user's settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoordinateToolRuntimeState {
    hud_enabled: bool,
    crosshair_enabled: bool,
    halo_enabled: bool,
    zoom_enabled: bool,
    frozen_sample: Option<CoordinateSample>,
    last_successful_copy: Option<FormattedCoordinate>,
}

/// Native preparation and presentation state for one cursor effect. Requested
/// state remains in `CoordinateToolRuntimeState`; an unavailable resource does
/// not change that request or disable another effect.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CursorEffectStatus {
    #[default]
    Disabled,
    /// Native resources are ready and kept hidden until their renderer is active.
    Prepared,
    /// The passive effect surface is currently visible.
    Active,
    /// The halo is following the live cursor with a contrasting outline only;
    /// desktop inversion could not be initialized or presented.
    Fallback(String),
    /// The effect remains requested but has no live cursor sample to follow.
    Paused,
    /// Resource preparation or cleanup failed. The reason is retained for the
    /// effect status UI without turning into a shared HUD/crosshair error.
    Unavailable(String),
}

/// Per-effect native status, separate from persisted preferences and requested
/// runtime flags.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoordinateEffectsStatus {
    halo: CursorEffectStatus,
    zoom: CursorEffectStatus,
}

impl CoordinateEffectsStatus {
    pub fn halo(&self) -> &CursorEffectStatus {
        &self.halo
    }

    pub fn zoom(&self) -> &CursorEffectStatus {
        &self.zoom
    }

    pub(crate) fn set_halo(&mut self, status: CursorEffectStatus) {
        self.halo = status;
    }

    pub(crate) fn set_zoom(&mut self, status: CursorEffectStatus) {
        self.zoom = status;
    }
}

impl CoordinateToolRuntimeState {
    pub const fn hud_enabled(&self) -> bool {
        self.hud_enabled
    }

    pub const fn crosshair_enabled(&self) -> bool {
        self.crosshair_enabled
    }

    pub const fn halo_enabled(&self) -> bool {
        self.halo_enabled
    }

    pub const fn zoom_enabled(&self) -> bool {
        self.zoom_enabled
    }

    /// Whether the shared coordinate worker is needed by any of the four
    /// independent session modes.
    pub const fn has_active_mode(&self) -> bool {
        self.hud_enabled || self.crosshair_enabled || self.halo_enabled || self.zoom_enabled
    }

    pub fn set_hud_enabled(&mut self, enabled: bool) {
        self.hud_enabled = enabled;
    }

    pub fn toggle_hud(&mut self) -> bool {
        self.hud_enabled = !self.hud_enabled;
        self.hud_enabled
    }

    pub fn set_crosshair_enabled(&mut self, enabled: bool) {
        self.crosshair_enabled = enabled;
    }

    pub fn toggle_crosshair(&mut self) -> bool {
        self.crosshair_enabled = !self.crosshair_enabled;
        self.crosshair_enabled
    }

    pub fn set_halo_enabled(&mut self, enabled: bool) {
        self.halo_enabled = enabled;
    }

    pub fn toggle_halo(&mut self) -> bool {
        self.halo_enabled = !self.halo_enabled;
        self.halo_enabled
    }

    pub fn set_zoom_enabled(&mut self, enabled: bool) {
        self.zoom_enabled = enabled;
    }

    pub fn toggle_zoom(&mut self) -> bool {
        self.zoom_enabled = !self.zoom_enabled;
        self.zoom_enabled
    }

    /// Turn off crosshair, halo, and zoom while preserving the coordinate HUD
    /// and all inspector state (freeze and last successful copy).
    pub fn disable_effects(&mut self) {
        self.crosshair_enabled = false;
        self.halo_enabled = false;
        self.zoom_enabled = false;
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen_sample.is_some()
    }

    pub fn frozen_sample(&self) -> Option<&CoordinateSample> {
        self.frozen_sample.as_ref()
    }

    pub fn freeze(&mut self, sample: &CoordinateSample) {
        if self.frozen_sample.is_none() {
            self.frozen_sample = Some(sample.clone());
        }
    }

    pub fn unfreeze(&mut self) {
        self.frozen_sample = None;
    }

    /// Return the exact sample the HUD should display and copy actions should
    /// use for the current frame.
    pub fn displayed_sample<'a>(&'a self, live: &'a CoordinateSample) -> &'a CoordinateSample {
        self.frozen_sample.as_ref().unwrap_or(live)
    }

    /// Format the value associated with the same sample the HUD displays.
    /// Callers performing a one-shot pick can use `format_coordinate` with
    /// their click-time sample directly, independent of the frozen HUD.
    pub fn copy_value(
        &self,
        live: &CoordinateSample,
        space: CoordinateSpace,
    ) -> Result<FormattedCoordinate, CoordinateUnavailable> {
        format_coordinate(self.displayed_sample(live), space)
    }

    /// Record a formatted value only after the caller confirms its clipboard
    /// write succeeded. Formatting failure cannot clear or replace this status.
    pub fn record_successful_copy(&mut self, copied: FormattedCoordinate) {
        self.last_successful_copy = Some(copied);
    }

    pub fn last_successful_copy(&self) -> Option<&FormattedCoordinate> {
        self.last_successful_copy.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CoordinateSample, CoordinateSpace, CoordinateToolRuntimeState, CoordinateUnavailable,
        ForegroundClientGeometry, MonitorGeometry, MonitorId, PhysicalPoint, PhysicalRect,
        PhysicalSize, clamp_hud_origin, format_coordinate,
    };

    fn monitor(bounds: PhysicalRect, work_area: PhysicalRect) -> MonitorGeometry {
        MonitorGeometry {
            id: MonitorId::new("DISPLAY1"),
            bounds,
            work_area,
            effective_dpi: Some((96, 96)),
        }
    }

    fn sample(point: PhysicalPoint) -> CoordinateSample {
        CoordinateSample::new(
            point,
            Some(PhysicalRect::new(-1920, 0, 1920, 1080).unwrap()),
            Some(monitor(
                PhysicalRect::new(-1920, 0, 0, 1080).unwrap(),
                PhysicalRect::new(-1920, 0, 0, 1040).unwrap(),
            )),
            Some(ForegroundClientGeometry::new(
                PhysicalPoint::new(-1800, 40),
                Some(PhysicalRect::new(-1800, 40, -100, 800).unwrap()),
            )),
        )
    }

    #[test]
    fn coordinate_spaces_preserve_signed_desktop_and_relative_coordinates() {
        let sample = sample(PhysicalPoint::new(-1732, 215));
        assert_eq!(
            sample.point_in(CoordinateSpace::Desktop),
            Ok(PhysicalPoint::new(-1732, 215))
        );
        assert_eq!(
            sample.point_in(CoordinateSpace::Monitor),
            Ok(PhysicalPoint::new(188, 215))
        );
        assert_eq!(
            sample.point_in(CoordinateSpace::ForegroundClient),
            Ok(PhysicalPoint::new(68, 175))
        );
        assert_eq!(
            format_coordinate(&sample, CoordinateSpace::Desktop)
                .unwrap()
                .text,
            "-1732,215"
        );
    }

    #[test]
    fn missing_monitor_and_client_origin_are_explicitly_unavailable() {
        let sample = CoordinateSample::new(PhysicalPoint::new(4, 5), None, None, None);
        assert_eq!(
            sample.point_in(CoordinateSpace::Monitor),
            Err(CoordinateUnavailable::MonitorUnavailable)
        );
        assert_eq!(
            sample.point_in(CoordinateSpace::ForegroundClient),
            Err(CoordinateUnavailable::ForegroundClientUnavailable)
        );
        assert_eq!(
            format_coordinate(&sample, CoordinateSpace::ForegroundClient),
            Err(CoordinateUnavailable::ForegroundClientUnavailable)
        );
    }

    #[test]
    fn relative_coordinate_overflow_is_reported_instead_of_wrapping() {
        let sample = CoordinateSample::new(
            PhysicalPoint::new(i32::MAX, i32::MIN),
            None,
            Some(monitor(
                PhysicalRect::new(i32::MIN, i32::MIN, 0, 1).unwrap(),
                PhysicalRect::new(i32::MIN, i32::MIN, 0, 1).unwrap(),
            )),
            Some(ForegroundClientGeometry::new(
                PhysicalPoint::new(i32::MIN, i32::MIN),
                None,
            )),
        );
        assert_eq!(
            sample.point_in(CoordinateSpace::Monitor),
            Err(CoordinateUnavailable::ArithmeticOverflow)
        );
        assert_eq!(
            sample.point_in(CoordinateSpace::ForegroundClient),
            Err(CoordinateUnavailable::ArithmeticOverflow)
        );
    }

    #[test]
    fn frozen_sample_is_shared_by_display_and_copy_until_unfrozen() {
        let first = sample(PhysicalPoint::new(-1700, 200));
        let second = sample(PhysicalPoint::new(-1650, 250));
        let mut state = CoordinateToolRuntimeState::default();
        state.freeze(&first);
        state.freeze(&second);

        let displayed = state.displayed_sample(&second);
        assert_eq!(displayed, &first);
        assert_eq!(
            state
                .copy_value(&second, CoordinateSpace::Monitor)
                .unwrap()
                .text,
            "220,200"
        );

        state.unfreeze();
        assert_eq!(state.displayed_sample(&second), &second);
        assert_eq!(
            format_coordinate(state.displayed_sample(&second), CoordinateSpace::Monitor)
                .unwrap()
                .text,
            "270,250"
        );
    }

    #[test]
    fn hud_and_crosshair_runtime_toggles_are_independent_and_copy_status_is_success_only() {
        let sample = sample(PhysicalPoint::new(30, 40));
        let mut state = CoordinateToolRuntimeState::default();
        assert!(!state.hud_enabled());
        assert!(!state.crosshair_enabled());
        assert!(state.toggle_hud());
        assert!(!state.crosshair_enabled());
        assert!(state.toggle_crosshair());
        assert!(state.hud_enabled());
        assert!(state.crosshair_enabled());

        let copied = format_coordinate(&sample, CoordinateSpace::Desktop).unwrap();
        state.record_successful_copy(copied.clone());
        assert_eq!(state.last_successful_copy(), Some(&copied));
        assert_eq!(
            state.copy_value(
                &CoordinateSample::new(PhysicalPoint::new(1, 2), None, None, None),
                CoordinateSpace::ForegroundClient,
            ),
            Err(CoordinateUnavailable::ForegroundClientUnavailable)
        );
        assert_eq!(state.last_successful_copy(), Some(&copied));
    }

    #[test]
    fn four_runtime_modes_are_independent_and_effects_off_preserves_hud_state() {
        let live = sample(PhysicalPoint::new(-1700, 210));
        let frozen = sample(PhysicalPoint::new(-1800, 200));
        let copied = format_coordinate(&frozen, CoordinateSpace::Desktop).unwrap();
        let mut state = CoordinateToolRuntimeState::default();

        assert!(!state.has_active_mode());
        assert!(state.toggle_hud());
        assert!(state.has_active_mode());
        assert!(state.toggle_crosshair());
        assert!(state.toggle_halo());
        assert!(state.toggle_zoom());
        assert!(state.hud_enabled());
        assert!(state.crosshair_enabled());
        assert!(state.halo_enabled());
        assert!(state.zoom_enabled());

        state.freeze(&frozen);
        state.record_successful_copy(copied.clone());
        state.disable_effects();

        assert!(state.hud_enabled());
        assert!(!state.crosshair_enabled());
        assert!(!state.halo_enabled());
        assert!(!state.zoom_enabled());
        assert!(state.has_active_mode());
        assert_eq!(state.frozen_sample(), Some(&frozen));
        assert_eq!(state.last_successful_copy(), Some(&copied));
        assert_eq!(state.displayed_sample(&live), &frozen);

        state.set_hud_enabled(false);
        assert!(!state.has_active_mode());
    }

    #[test]
    fn hud_origin_is_clamped_to_sampled_monitor_work_area_with_negative_coordinates() {
        let sample = sample(PhysicalPoint::new(-1910, 1030));
        let size = PhysicalSize::new(200, 80).unwrap();
        assert_eq!(
            sample.hud_origin(PhysicalPoint::new(-100, 20), size),
            Some(PhysicalPoint::new(-1920, 960))
        );
        assert_eq!(
            clamp_hud_origin(
                PhysicalPoint::new(i32::MAX, i32::MAX),
                PhysicalPoint::new(512, 512),
                size,
                PhysicalRect::new(-1920, 0, 0, 1040).unwrap(),
            ),
            PhysicalPoint::new(-200, 960)
        );
    }
}
