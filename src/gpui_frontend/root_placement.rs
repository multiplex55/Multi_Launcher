//! Saved launcher client size and optional static outer placement.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RootPlacement {
    pub initial_client: (i32, i32),
    startup_client: (i32, i32),
    follow_mouse: bool,
    position: Option<(i32, i32)>,
    client_size: Option<(i32, i32)>,
}

impl RootPlacement {
    pub(super) fn with_preferences(self, follow_mouse: bool, enabled: bool, position: Option<(i32, i32)>, size: Option<(i32, i32)>) -> Self {
        let (enabled, position, size) = multi_launcher::settings::normalize_static_window_config(follow_mouse, enabled, position, size);
        Self { follow_mouse, position: enabled.then_some(position).flatten(), client_size: enabled.then_some(size.filter(|(w, h)| *w > 0 && *h > 0).map(|(w, h)| (w.max(320), h.max(160)))).flatten(), ..self }
    }
    pub fn from_settings(settings: &multi_launcher::settings::Settings) -> Self {
        let valid = |size: (i32, i32)| size.0 > 0 && size.1 > 0;
        let clamp = |size: (i32, i32)| (size.0.max(320), size.1.max(160));
        Self {
            startup_client: settings.window_size.unwrap_or((400, 220)),
            follow_mouse: settings.follow_mouse,
            initial_client: clamp(
                settings
                    .window_size
                    .filter(|size| valid(*size))
                    .unwrap_or((400, 220)),
            ),
            position: None,
            client_size: None,
        }.with_preferences(settings.follow_mouse, settings.static_location_enabled, settings.static_pos, settings.static_size)
    }
    fn has_static(self) -> bool {
        self.position.is_some() || self.client_size.is_some()
    }
}

#[derive(Clone, Copy)]
pub(super) enum PlacementEdge {
    Startup,
    Reveal,
}

fn mouse_position(cursor: Option<(f32, f32)>, basis: (i32, i32), scale: f32) -> Option<(i32, i32)> {
    let (x, y) = multi_launcher::window_manager::cursor_centered_position(
        cursor?,
        (basis.0 as f32, basis.1 as f32),
    );
    let convert = |value: f32| {
        let physical = f64::from(value * scale).round();
        (physical.is_finite() && physical >= i32::MIN as f64 && physical <= i32::MAX as f64)
            .then_some(physical as i32)
    };
    Some((convert(x)?, convert(y)?))
}

fn client_basis(
    edge: PlacementEdge,
    startup: (i32, i32),
    metrics: NativeWindowMetrics,
) -> (i32, i32) {
    match edge {
        PlacementEdge::Startup => startup,
        PlacementEdge::Reveal => (
            (metrics.client_width as f32 / metrics.scale_factor) as i32,
            (metrics.client_height as f32 / metrics.scale_factor) as i32,
        ),
    }
}

pub(super) fn should_place_reveal(was_visible: bool, desired_visible: bool, current: bool) -> bool {
    !was_visible && desired_visible && current
}

#[cfg(windows)]
pub(super) fn apply_placement(
    placement: RootPlacement,
    window: NativeWindow,
    edge: PlacementEdge,
    current: impl Fn() -> bool,
) -> Result<(), String> {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
        },
    };
    if !placement.follow_mouse && !placement.has_static() {
        return Ok(());
    }
    if !current() || !native_window_is_valid(window) {
        return Err("launcher placement request is stale".into());
    }
    let metrics = native_window_metrics(
        checked_geometry_target(window).ok_or("missing launcher HWND")?,
        1.0,
    )?
    .ok_or("launcher window metrics are unavailable")?;
    if placement.follow_mouse {
        let basis = client_basis(edge, placement.startup_client, metrics);
        let Some((x, y)) = mouse_position(
            multi_launcher::window_manager::current_mouse_position(),
            basis,
            metrics.scale_factor,
        ) else {
            return Ok(());
        };
        if !current() || !native_window_is_valid(window) {
            return Err("launcher placement request is stale".into());
        }
        return unsafe {
            SetWindowPos(
                HWND(window.identity.hwnd as *mut _),
                HWND::default(),
                x,
                y,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOSIZE,
            )
        }
        .map_err(|error| format!("Windows rejected launcher placement: {error}"));
    }
    let scaled = |value: i32| -> Result<i32, String> {
        let n = (value as f64 * metrics.scale_factor as f64).round();
        if !n.is_finite() || n < i32::MIN as f64 || n > i32::MAX as f64 {
            Err("saved launcher geometry is outside the native range".into())
        } else {
            Ok(n as i32)
        }
    };
    let (x, y) = placement
        .position
        .map(|(x, y)| Ok::<_, String>((scaled(x)?, scaled(y)?)))
        .transpose()?
        .unwrap_or((0, 0));
    let (w, h) = placement
        .client_size
        .map(|(w, h)| {
            let chrome_w = metrics
                .outer
                .width()
                .saturating_sub(metrics.client_width)
                .max(0);
            let chrome_h = metrics
                .outer
                .height()
                .saturating_sub(metrics.client_height)
                .max(0);
            Ok::<_, String>((
                scaled(w)?
                    .checked_add(chrome_w)
                    .ok_or("launcher width overflow")?,
                scaled(h)?
                    .checked_add(chrome_h)
                    .ok_or("launcher height overflow")?,
            ))
        })
        .transpose()?
        .unwrap_or((0, 0));
    let mut flags = SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER;
    if placement.position.is_none() {
        flags |= SWP_NOMOVE;
    }
    if placement.client_size.is_none() {
        flags |= SWP_NOSIZE;
    }
    if !current() || !native_window_is_valid(window) {
        return Err("launcher placement request is stale".into());
    }
    unsafe {
        SetWindowPos(
            HWND(window.identity.hwnd as *mut _),
            HWND::default(),
            x,
            y,
            w,
            h,
            flags,
        )
    }
    .map_err(|error| format!("Windows rejected launcher placement: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{PlacementEdge, RootPlacement, client_basis, mouse_position, should_place_reveal};
    use crate::gpui_frontend::{NativeWindowMetrics, NativeWindowRect};
    use multi_launcher::settings::Settings;

    #[test]
    fn saved_placement_normalizes_partial_values_and_follow_mouse() {
        let mut settings = Settings::default();
        settings.window_size = None;
        assert_eq!(
            RootPlacement::from_settings(&settings).initial_client,
            (400, 220)
        );
        settings.window_size = Some((120, 240));
        settings.follow_mouse = false;
        settings.static_location_enabled = true;
        settings.static_pos = Some((-50, 10));
        settings.static_size = Some((0, 300));
        let placement = RootPlacement::from_settings(&settings);
        assert_eq!(placement.initial_client, (320, 240));
        assert_eq!(placement.position, Some((-50, 10)));
        assert_eq!(placement.client_size, None);
        settings.static_size = Some((500, 300));
        assert_eq!(
            RootPlacement::from_settings(&settings).client_size,
            Some((500, 300))
        );
        settings.static_pos = None;
        assert_eq!(RootPlacement::from_settings(&settings).position, None);
        assert_eq!(
            RootPlacement::from_settings(&settings).client_size,
            Some((500, 300))
        );
        settings.static_location_enabled = false;
        assert!(!RootPlacement::from_settings(&settings).has_static());
        settings.static_location_enabled = true;
        settings.follow_mouse = true;
        assert!(!RootPlacement::from_settings(&settings).has_static());
        assert!(RootPlacement::from_settings(&settings).follow_mouse);
        assert!(should_place_reveal(false, true, true));
        assert!(!should_place_reveal(true, true, true));
        assert!(!should_place_reveal(false, true, false));
    }

    #[test]
    fn acknowledged_preferences_change_only_future_reveal_policy() {
        let initial = RootPlacement::from_settings(&Settings::default());
        let changed = initial.with_preferences(false, true, Some((-120, 45)), Some((640, 480)));
        assert_eq!(changed.initial_client, initial.initial_client);
        assert_eq!(changed.startup_client, initial.startup_client);
        assert_eq!(changed.position, Some((-120, 45)));
        assert_eq!(changed.client_size, Some((640, 480)));
        assert!(!should_place_reveal(true, true, true));
        assert!(should_place_reveal(false, true, true));
        assert!(!should_place_reveal(false, true, false));
    }

    #[test]
    fn mouse_placement_uses_stock_signed_formula_and_edge_basis() {
        let metrics = NativeWindowMetrics {
            outer: NativeWindowRect {
                left: -500,
                top: 0,
                right: 500,
                bottom: 800,
            },
            client_width: 1201,
            client_height: 601,
            scale_factor: 1.5,
        };
        let startup = (401, 221);
        assert_eq!(
            client_basis(PlacementEdge::Startup, startup, metrics),
            startup
        );
        assert_eq!(
            client_basis(PlacementEdge::Reveal, startup, metrics),
            (800, 400)
        );
        assert_eq!(
            mouse_position(Some((-100.0, 50.0)), startup, 1.5),
            Some((-451, -91))
        );
        assert_eq!(
            mouse_position(Some((-100.0, 50.0)), (800, 400), 1.5),
            Some((-750, -225))
        );
        assert_eq!(mouse_position(None, startup, 1.5), None);
        assert_eq!(
            mouse_position(Some((-299.0, 0.0)), (401, 0), 160.0_f32 / 96.0),
            Some((-833, 0))
        );
    }
}
