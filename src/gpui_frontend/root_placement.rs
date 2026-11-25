//! Saved launcher client size and optional static outer placement.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RootPlacement {
    pub initial_client: (i32, i32),
    position: Option<(i32, i32)>,
    client_size: Option<(i32, i32)>,
}

impl RootPlacement {
    pub fn from_settings(settings: &multi_launcher::settings::Settings) -> Self {
        let (enabled, position, client_size) =
            multi_launcher::settings::normalize_static_window_config(
                settings.follow_mouse,
                settings.static_location_enabled,
                settings.static_pos,
                settings.static_size,
            );
        let valid = |size: (i32, i32)| size.0 > 0 && size.1 > 0;
        let clamp = |size: (i32, i32)| (size.0.max(320), size.1.max(160));
        Self {
            initial_client: clamp(
                settings
                    .window_size
                    .filter(|size| valid(*size))
                    .unwrap_or((400, 220)),
            ),
            position: enabled.then_some(position).flatten(),
            client_size: enabled
                .then_some(client_size.filter(|size| valid(*size)).map(clamp))
                .flatten(),
        }
    }
    fn has_static(self) -> bool {
        self.position.is_some() || self.client_size.is_some()
    }
}

pub(super) fn should_place_reveal(was_visible: bool, desired_visible: bool, current: bool) -> bool {
    !was_visible && desired_visible && current
}

#[cfg(windows)]
pub(super) fn apply_static(
    placement: RootPlacement,
    window: NativeWindow,
    current: impl Fn() -> bool,
) -> Result<(), String> {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
        },
    };
    if !placement.has_static() {
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
    use super::{RootPlacement, should_place_reveal};
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
        assert!(should_place_reveal(false, true, true));
        assert!(!should_place_reveal(true, true, true));
        assert!(!should_place_reveal(false, true, false));
    }
}
