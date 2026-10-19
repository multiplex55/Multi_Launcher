//! Window identity and crop resolution for opt-in live pin selection.
//!
//! The selection rectangle and DWM extended-frame bounds are interpreted in
//! physical virtual-desktop pixels. The binding is an observation made after
//! the selector closes; a live capture backend must retain its own native
//! capture-item lifetime before treating an HWND as a durable source.

use crate::mkmacro::ScreenRect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinCaptureIntent {
    Static,
    Live,
}

/// Process-local observation used to open a native capture item later.
///
/// HWND, PID, and thread ID reject ordinary stale/reused handles at selection
/// time; they do not replace the capture-item lifetime owned by the live
/// backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinSourceWindowIdentity {
    pub hwnd: usize,
    pub process_id: u32,
    pub thread_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinSourceCrop {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Initial crop metadata for a later window-bound capture session.
/// `captured_bounds` uses DWM's physical extended-frame rectangle; M2-B must
/// confirm the actual frame origin and dimensions before presenting pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinSourceBinding {
    pub window: PinSourceWindowIdentity,
    pub captured_bounds: ScreenRect,
    pub crop: PinSourceCrop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PinSourceWindowObservation {
    pub identity: PinSourceWindowIdentity,
    pub extended_frame_bounds: Option<ScreenRect>,
    pub visible: bool,
}

pub(crate) trait PinSourceResolver: Send + Sync {
    fn resolve(&self, selection: ScreenRect) -> Result<PinSourceBinding, String>;
}

pub(crate) trait PinSourceWindowBackend: Send + Sync {
    /// Returns visible top-level windows in native top-to-bottom z-order.
    fn visible_top_level_windows(&self) -> Result<Vec<PinSourceWindowObservation>, String>;

    /// Revalidates the selected identity and geometry after the z-order scan.
    fn revalidate(
        &self,
        identity: PinSourceWindowIdentity,
    ) -> Result<Option<PinSourceWindowObservation>, String>;
}

pub(crate) fn resolve_source_with_backend(
    selection: ScreenRect,
    current_process_id: u32,
    backend: &impl PinSourceWindowBackend,
) -> Result<PinSourceBinding, String> {
    selection
        .validate_capture()
        .map_err(|error| format!("invalid live pin selection: {error}"))?;

    let windows = backend.visible_top_level_windows()?;
    let binding = resolve_source_from_z_order(selection, current_process_id, &windows)?;
    let Some(current) = backend.revalidate(binding.window)? else {
        return Err("the selected source window closed during selection".into());
    };
    if !current.visible
        || current.identity != binding.window
        || current.extended_frame_bounds != Some(binding.captured_bounds)
    {
        return Err("the selected source window changed during selection; try again".into());
    }
    Ok(binding)
}

fn resolve_source_from_z_order(
    selection: ScreenRect,
    current_process_id: u32,
    windows: &[PinSourceWindowObservation],
) -> Result<PinSourceBinding, String> {
    selection
        .validate_capture()
        .map_err(|error| format!("invalid live pin selection: {error}"))?;

    for window in windows {
        if !window.visible {
            // Hidden launcher and pin HWNDs do not contribute stale bounds or
            // occlusion after their native hide acknowledgements.
            continue;
        }
        let Some(bounds) = window.extended_frame_bounds else {
            return Err(
                "could not verify a visible top-level window before the selected source".into(),
            );
        };
        if bounds.is_empty() {
            // Empty frames cannot render pixels or occlude a selection.
            continue;
        }
        if !rectangles_intersect(selection, bounds) {
            continue;
        }
        validate_frame_bounds(bounds)?;
        validate_window_identity(window.identity)?;
        if window.identity.process_id == current_process_id {
            return Err("select a region in an external window, not a launcher window".into());
        }
        if !bounds.contains(selection) {
            return Err(
                "the selected region is split across windows or covered by another window".into(),
            );
        }

        let x = u32::try_from(i64::from(selection.x) - i64::from(bounds.x))
            .map_err(|_| "selected crop X is outside its source window".to_owned())?;
        let y = u32::try_from(i64::from(selection.y) - i64::from(bounds.y))
            .map_err(|_| "selected crop Y is outside its source window".to_owned())?;
        return Ok(PinSourceBinding {
            window: window.identity,
            captured_bounds: bounds,
            crop: PinSourceCrop {
                x,
                y,
                width: selection.width,
                height: selection.height,
            },
        });
    }

    Err("select a region fully inside one visible external window".into())
}

fn validate_window_identity(identity: PinSourceWindowIdentity) -> Result<(), String> {
    if identity.hwnd == 0 || identity.process_id == 0 || identity.thread_id == 0 {
        return Err("the selected source window has an invalid native identity".into());
    }
    Ok(())
}

fn validate_frame_bounds(bounds: ScreenRect) -> Result<(), String> {
    if bounds.is_empty()
        || bounds.right() > i64::from(i32::MAX) + 1
        || bounds.bottom() > i64::from(i32::MAX) + 1
    {
        return Err("a visible window returned invalid physical frame bounds".into());
    }
    Ok(())
}

fn rectangles_intersect(left: ScreenRect, right: ScreenRect) -> bool {
    i64::from(left.x) < right.right()
        && i64::from(right.x) < left.right()
        && i64::from(left.y) < right.bottom()
        && i64::from(right.y) < left.bottom()
}

#[derive(Default)]
pub(crate) struct NativePinSourceResolver;

impl PinSourceResolver for NativePinSourceResolver {
    fn resolve(&self, selection: ScreenRect) -> Result<PinSourceBinding, String> {
        #[cfg(windows)]
        {
            resolve_source_with_backend(selection, std::process::id(), &WindowsPinSourceBackend)
        }
        #[cfg(not(windows))]
        {
            let _ = selection;
            Err("live window pin source selection is available only on Windows".into())
        }
    }
}

#[cfg(windows)]
struct WindowsPinSourceBackend;

#[cfg(windows)]
impl PinSourceWindowBackend for WindowsPinSourceBackend {
    fn visible_top_level_windows(&self) -> Result<Vec<PinSourceWindowObservation>, String> {
        use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, IsWindow, IsWindowVisible};

        struct Context(Vec<PinSourceWindowObservation>);

        unsafe extern "system" fn visit(hwnd: HWND, parameter: LPARAM) -> BOOL {
            let context = unsafe { &mut *(parameter.0 as *mut Context) };
            if hwnd.0.is_null()
                || !unsafe { IsWindow(hwnd) }.as_bool()
                || !unsafe { IsWindowVisible(hwnd) }.as_bool()
            {
                return BOOL(1);
            }
            if let Some(observation) = observe_window(hwnd) {
                context.0.push(observation);
            }
            BOOL(1)
        }

        let mut context = Context(Vec::new());
        let result =
            unsafe { EnumWindows(Some(visit), LPARAM((&mut context as *mut Context) as isize)) };
        result.map_err(|error| format!("could not enumerate visible source windows: {error}"))?;
        Ok(context.0)
    }

    fn revalidate(
        &self,
        identity: PinSourceWindowIdentity,
    ) -> Result<Option<PinSourceWindowObservation>, String> {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{IsWindow, IsWindowVisible};

        let hwnd = HWND(identity.hwnd as *mut core::ffi::c_void);
        if !unsafe { IsWindow(hwnd) }.as_bool() || !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return Ok(None);
        }
        let Some(observation) = observe_window(hwnd) else {
            return Ok(None);
        };
        Ok((observation.identity == identity).then_some(observation))
    }
}

#[cfg(windows)]
fn observe_window(hwnd: windows::Win32::Foundation::HWND) -> Option<PinSourceWindowObservation> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Dwm::{
        DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GA_ROOT, GetAncestor, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    };

    if !unsafe { IsWindowVisible(hwnd) }.as_bool()
        || unsafe { IsIconic(hwnd) }.as_bool()
        || unsafe { GetAncestor(hwnd, GA_ROOT) } != hwnd
    {
        return None;
    }

    let mut process_id = 0;
    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    let identity = PinSourceWindowIdentity {
        hwnd: hwnd.0 as usize,
        process_id,
        thread_id,
    };
    let unknown_bounds = || {
        Some(PinSourceWindowObservation {
            identity,
            extended_frame_bounds: None,
            visible: true,
        })
    };

    let mut cloaked = 0u32;
    if unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    }
    .is_err()
    {
        return unknown_bounds();
    }
    if cloaked != 0 {
        return None;
    }

    let mut rect = RECT::default();
    if unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut rect as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    }
    .is_err()
    {
        return unknown_bounds();
    }
    let Some(width) = rect.right.checked_sub(rect.left) else {
        return unknown_bounds();
    };
    let Some(height) = rect.bottom.checked_sub(rect.top) else {
        return unknown_bounds();
    };
    if width == 0 || height == 0 {
        return None;
    }
    if width < 0 || height < 0 {
        return unknown_bounds();
    }
    let (Ok(width), Ok(height)) = (u32::try_from(width), u32::try_from(height)) else {
        return unknown_bounds();
    };
    let extended_frame_bounds = ScreenRect::new(rect.left, rect.top, width, height);
    Some(PinSourceWindowObservation {
        identity,
        extended_frame_bounds: Some(extended_frame_bounds),
        visible: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(hwnd: usize, process_id: u32, bounds: ScreenRect) -> PinSourceWindowObservation {
        PinSourceWindowObservation {
            identity: PinSourceWindowIdentity {
                hwnd,
                process_id,
                thread_id: 100 + u32::try_from(hwnd).unwrap(),
            },
            extended_frame_bounds: Some(bounds),
            visible: true,
        }
    }

    struct FakeBackend {
        windows: Vec<PinSourceWindowObservation>,
        revalidated: Option<PinSourceWindowObservation>,
    }

    impl PinSourceWindowBackend for FakeBackend {
        fn visible_top_level_windows(&self) -> Result<Vec<PinSourceWindowObservation>, String> {
            Ok(self.windows.clone())
        }

        fn revalidate(
            &self,
            identity: PinSourceWindowIdentity,
        ) -> Result<Option<PinSourceWindowObservation>, String> {
            Ok(self
                .revalidated
                .filter(|window| window.identity == identity))
        }
    }

    #[test]
    fn live_source_binds_negative_physical_selection_to_relative_crop() {
        let frame = ScreenRect::new(-2_000, -300, 900, 700);
        let selection = ScreenRect::new(-1_850, -120, 240, 180);
        let source =
            resolve_source_from_z_order(selection, 1, &[observation(7, 2, frame)]).unwrap();

        assert_eq!(source.window.hwnd, 7);
        assert_eq!(source.captured_bounds, frame);
        assert_eq!(
            source.crop,
            PinSourceCrop {
                x: 150,
                y: 180,
                width: 240,
                height: 180,
            }
        );
    }

    #[test]
    fn live_source_rejects_partial_higher_occluder_but_ignores_lower_windows() {
        let selection = ScreenRect::new(100, 100, 200, 160);
        let source = observation(7, 2, ScreenRect::new(0, 0, 600, 500));
        let lower = observation(8, 3, ScreenRect::new(40, 40, 400, 400));
        assert!(resolve_source_from_z_order(selection, 1, &[source, lower]).is_ok());

        let mut unknown_lower = observation(10, 4, ScreenRect::new(0, 0, 0, 0));
        unknown_lower.extended_frame_bounds = None;
        assert!(resolve_source_from_z_order(selection, 1, &[source, unknown_lower]).is_ok());
        assert!(
            resolve_source_from_z_order(selection, 1, &[unknown_lower, source])
                .unwrap_err()
                .contains("could not verify a visible top-level window")
        );

        let occluder = observation(9, 3, ScreenRect::new(80, 80, 80, 300));
        let error = resolve_source_from_z_order(selection, 1, &[occluder, source]).unwrap_err();
        assert!(error.contains("split across windows or covered"));
    }

    #[test]
    fn live_source_ignores_hidden_self_bounds_and_rejects_visible_self_or_multi_window() {
        let selection = ScreenRect::new(-20, 10, 80, 90);
        let mut hidden_self = observation(1, 1, ScreenRect::new(-50, -20, 300, 300));
        hidden_self.visible = false;
        let source = observation(2, 2, ScreenRect::new(-100, -100, 250, 250));
        assert!(resolve_source_from_z_order(selection, 1, &[hidden_self, source]).is_ok());

        let visible_self = observation(1, 1, ScreenRect::new(-50, -20, 300, 300));
        assert!(
            resolve_source_from_z_order(selection, 1, &[visible_self, source])
                .unwrap_err()
                .contains("external window")
        );

        let left = observation(3, 3, ScreenRect::new(-100, -20, 100, 300));
        let right = observation(4, 4, ScreenRect::new(0, -20, 100, 300));
        assert!(
            resolve_source_from_z_order(selection, 1, &[left, right])
                .unwrap_err()
                .contains("split across windows")
        );
    }

    #[test]
    fn live_source_rejects_destroyed_or_changed_native_identity() {
        let selection = ScreenRect::new(10, 20, 30, 40);
        let source = observation(7, 2, ScreenRect::new(0, 0, 100, 100));
        let destroyed = FakeBackend {
            windows: vec![source],
            revalidated: None,
        };
        assert!(
            resolve_source_with_backend(selection, 1, &destroyed)
                .unwrap_err()
                .contains("closed during selection")
        );

        let changed = FakeBackend {
            windows: vec![source],
            revalidated: Some(observation(7, 3, source.extended_frame_bounds.unwrap())),
        };
        assert!(
            resolve_source_with_backend(selection, 1, &changed)
                .unwrap_err()
                .contains("closed during selection")
        );
    }

    #[test]
    fn live_source_rejects_invalid_geometry_and_identity() {
        let selection = ScreenRect::new(10, 20, 30, 40);
        let empty = observation(7, 2, ScreenRect::new(0, 0, 0, 100));
        let source = observation(8, 3, ScreenRect::new(0, 0, 100, 100));
        assert!(resolve_source_from_z_order(selection, 1, &[empty, source]).is_ok());

        let oversized = observation(7, 2, ScreenRect::new(0, 0, u32::MAX, 100));
        assert!(
            resolve_source_from_z_order(selection, 1, &[oversized])
                .unwrap_err()
                .contains("invalid physical frame bounds")
        );

        let mut invalid_identity = observation(0, 2, ScreenRect::new(0, 0, 100, 100));
        invalid_identity.identity.thread_id = 0;
        assert!(
            resolve_source_from_z_order(selection, 1, &[invalid_identity])
                .unwrap_err()
                .contains("invalid native identity")
        );
    }
}
